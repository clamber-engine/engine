//! Finds loop points for music without loop tags, and checks the seams.
//!
//! `cargo run --release -p clamber-audio --example find-loop -- FILE...`
//!
//! A track loops in one of two ways:
//!
//! - **It repeats itself**: after an intro, the music plays a stretch
//!   and then plays it again to the end of the file. The loop is one
//!   pass of that stretch, found at the lag where the music repeats most
//!   closely, with the seam where the two passes match best. The intro
//!   plays once.
//! - **The file is one pass**: it starts and stops abruptly, and wraps
//!   end to start. The loop runs from where the music starts, after the
//!   MP3 encoder's delay, to where it stops, before the encoder's
//!   padding, and those are fine-tuned so the join is smooth.
//!
//! Prints the loop points as frames at the file's rate, for a music
//! list, and the seam as [`clamber_audio::seam::check`] measures it.

use clamber_audio::{Input, Sound, seam};

/// Frames per step of the search for a repeat.
const HOP: usize = 512;
/// The shortest repeat considered, in seconds.
const SHORTEST: f64 = 8.0;
/// A step where the two passes differ by less than this, relative to
/// the music, counts as repeated.
const SAME: f64 = 0.05;
/// The repeat must run to within this many seconds of the music's end,
/// so that nothing after the loop goes unheard.
const TO_THE_END: f64 = 1.0;
/// Frames each side of a seam compared when placing it in a repeat.
const SEAM: isize = 256;
/// Where the music starts or stops: the first or last frame at least
/// this loud, relative to the loudest frame within [`NEAR`] of it.
const EDGE: f32 = 0.1;
const NEAR: usize = 1024;
/// How far the start and end may move from where they were found, to
/// smooth the join: (earlier, later), in frames.
const START_SHIFT: (usize, usize) = (48, 8);
const END_SHIFT: (usize, usize) = (8, 24);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        return Err("usage: find-loop FILE...".into());
    }
    for file in files {
        let input = Input::file(&file);
        let (sound, rate) = Sound::decode(&input)?;
        let track = Track::new(&sound);
        let Some((first, last)) = track.edges() else {
            println!("{file}: silent");
            continue;
        };
        let (kind, (start, end)) = match track.repeat(rate, last) {
            Some(points) => ("repeats itself", points),
            None => ("one pass", track.join(first, last)),
        };
        let s = seam::check(&input, start as u64, end as u64)?;
        let secs = |f: usize| f as f64 / rate as f64;
        println!(
            "{file}: {kind}; {rate} Hz, {:.2} s: intro {:.2} s, loop {:.2} s, unplayed {:.2} s",
            secs(track.len()),
            secs(start),
            secs(end - start),
            secs(track.len() - end),
        );
        println!(
            "  seam: click {:+.1} dB, gap {:.1} ms, mismatch {:.1} dB, after {:.1} dB",
            s.click,
            s.gap * 1000.0,
            s.mismatch,
            s.after
        );
        println!("  loop_start = {start}\n  loop_end = {end}");
    }
    Ok(())
}

/// A decoded track as stereo frames.
struct Track {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Track {
    fn new(sound: &Sound) -> Track {
        let ch = sound.channels;
        let frames = sound.samples.chunks_exact(ch);
        Track {
            left: frames.clone().map(|f| f[0]).collect(),
            right: frames.map(|f| f[ch - 1]).collect(),
        }
    }

    fn len(&self) -> usize {
        self.left.len()
    }

    fn peak(&self, f: usize) -> f32 {
        self.left[f].abs().max(self.right[f].abs())
    }

    /// Squared difference between frames `a` and `b`, and their power.
    fn compare(&self, a: usize, b: usize) -> (f64, f64) {
        let (l, r) = (&self.left, &self.right);
        let (al, ar, bl, br) = (l[a] as f64, r[a] as f64, l[b] as f64, r[b] as f64);
        (
            (al - bl).powi(2) + (ar - br).powi(2),
            al * al + ar * ar + bl * bl + br * br,
        )
    }

    /// How differently frames `a..a + n` and `b..b + n` sound, relative
    /// to their level.
    fn difference(&self, a: usize, b: usize, n: usize) -> f64 {
        let (mut diff, mut level) = (0.0, 0.0);
        for k in 0..n {
            let (d, p) = self.compare(a + k, b + k);
            diff += d;
            level += p;
        }
        if level < 1e-9 { 1.0 } else { diff / level }
    }

    /// The first frame of music and the frame after the last: where the
    /// level first rises to, and last falls from, a good part of what's
    /// near it. The MP3 encoder's delay and padding are outside them.
    fn edges(&self) -> Option<(usize, usize)> {
        let n = self.len();
        let loudest = |r: std::ops::Range<usize>| r.map(|f| self.peak(f)).fold(0.0, f32::max);
        let first = (0..n).find(|&f| {
            let p = self.peak(f);
            p > 1e-3 && p >= EDGE * loudest(f..(f + NEAR).min(n))
        })?;
        let last = (0..n).rev().find(|&f| {
            let p = self.peak(f);
            p > 1e-3 && p >= EDGE * loudest(f.saturating_sub(NEAR)..f + 1)
        })?;
        Some((first, last + 1))
    }

    /// Loop points for a track that repeats itself to `music_end`.
    fn repeat(&self, rate: u32, music_end: usize) -> Option<(usize, usize)> {
        let lag = self.lag(rate, music_end)?;
        // steps that repeat `lag` frames later
        let steps = (music_end - lag) / HOP;
        let same: Vec<bool> = (0..steps)
            .map(|i| self.difference(i * HOP, i * HOP + lag, HOP) < SAME)
            .collect();
        let (first, last) = longest_run(&same)?;
        let reach = (last + 1) * HOP + lag;
        if music_end - reach.min(music_end) > (TO_THE_END * rate as f64) as usize {
            return None;
        }
        // keep away from the run's ends, where the passes diverge
        let margin = (rate as usize / 2).div_ceil(HOP);
        let (from, to) = (first + margin, last.checked_sub(margin)?);
        if from >= to {
            return None;
        }
        // the best-matching step, then the best frame in it
        let around = |at: usize| {
            let a = at - SEAM as usize;
            self.difference(a, a + lag, 2 * SEAM as usize)
        };
        let best_step = (from..to).min_by(|&a, &b| around(a * HOP).total_cmp(&around(b * HOP)))?;
        let start = (best_step * HOP..(best_step + 1) * HOP)
            .min_by(|&a, &b| around(a).total_cmp(&around(b)))?;
        Some((start, start + lag))
    }

    /// The lag, in frames, at which the music before `music_end` repeats
    /// most closely: first in steps, from each step's loudness, then
    /// exactly, from the frames.
    fn lag(&self, rate: u32, music_end: usize) -> Option<usize> {
        let loudness: Vec<f64> = (0..music_end / HOP)
            .map(|i| {
                let p: f64 = (i * HOP..(i + 1) * HOP)
                    .map(|f| self.compare(f, f).1 / 2.0)
                    .sum();
                (p / HOP as f64 + 1e-9).ln()
            })
            .collect();
        let n = loudness.len();
        let shortest = (SHORTEST * rate as f64) as usize / HOP;
        // the passes must overlap for at least as long as the shortest
        let (_, coarse) = (shortest..n.saturating_sub(shortest))
            .map(|lag| {
                let pairs = n - lag;
                let error = (0..pairs)
                    .map(|i| (loudness[i] - loudness[i + lag]).abs())
                    .sum::<f64>()
                    / pairs as f64;
                (error, lag)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        // refine against the frames, over a couple of seconds in the
        // middle of the overlap
        let overlap = (n - coarse) * HOP;
        let window = (2 * rate as usize).min(overlap / 2);
        let at = (overlap - window) / 2;
        let lo = (coarse * HOP).saturating_sub(2 * HOP);
        let hi = (coarse * HOP + 2 * HOP).min(music_end - at - window);
        (lo..=hi).min_by(|&a, &b| {
            self.difference(at, at + a, window)
                .total_cmp(&self.difference(at, at + b, window))
        })
    }

    /// Loop points for a track that is one pass, from `first` to `last`
    /// (exclusive): moved a little where the end leads into the start
    /// most smoothly, continuing the end's slope.
    fn join(&self, first: usize, last: usize) -> (usize, usize) {
        let roughness = |start: usize, end: usize| {
            let mut r = 0.0;
            for ch in [&self.left, &self.right] {
                let (a, b, c, d) = (ch[end - 2], ch[end - 1], ch[start], ch[start + 1]);
                // each frame against the straight line through the two before
                r += (c - (2.0 * b - a)).powi(2) + (d - (2.0 * c - b)).powi(2);
            }
            r
        };
        let starts = first.saturating_sub(START_SHIFT.0)..first + START_SHIFT.1;
        let ends = last - END_SHIFT.0..(last + END_SHIFT.1).min(self.len());
        let mut best = (f32::INFINITY, (first, last));
        for start in starts {
            for end in ends.clone() {
                let r = roughness(start, end);
                if r < best.0 {
                    best = (r, (start, end));
                }
            }
        }
        best.1
    }
}

/// The first and last index of the longest run of `true`.
fn longest_run(flags: &[bool]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut from = None;
    for (i, &f) in flags.iter().chain([&false]).enumerate() {
        match (f, from) {
            (true, None) => from = Some(i),
            (false, Some(s)) => {
                if best.is_none_or(|(a, b)| i - 1 - s > b - a) {
                    best = Some((s, i - 1));
                }
                from = None;
            }
            _ => {}
        }
    }
    best
}
