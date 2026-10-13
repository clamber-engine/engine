//! Checking that a music loop is seamless.
//!
//! [`check`] plays a loop through its seam with [`Music`], as the game
//! does, and measures what's heard there against the straight decode:
//! a click is a jump bigger than any the music makes nearby, and a gap
//! is silence at the seam.

use crate::{AudioError, Input, Looping, Music, Sound};

/// Frames each side of the seam that are compared, about 46 ms at
/// 44.1 kHz.
const AROUND: usize = 2048;
/// Frames each side of the seam whose steps a click is measured against.
const CLICK: usize = 64;
/// Frames after the seam compared with the straight decode, at most.
const AFTER: usize = 1 << 16;
/// Quieter than this, about -66 dBFS, is silence.
const SILENT: f32 = 5e-4;

/// How a loop's seam sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seam {
    /// The sample-to-sample step at the seam relative to the largest
    /// the music makes within [`CLICK`] frames of it either side, in dB.
    /// A click is a step far bigger than anything around it.
    pub click: f32,
    /// Silence heard at the seam, in seconds.
    pub gap: f32,
    /// The difference between the frames before the loop's end and
    /// those before its start, relative to the music, in dB: how well
    /// the end leads into the start. Only meaningful for a loop that
    /// starts after an intro; at the start of a file nothing comes
    /// before it.
    pub mismatch: f32,
    /// The difference between playback after the seam and the straight
    /// decode from the loop start, relative to the music, in dB:
    /// whether seeking back decodes exactly.
    pub after: f32,
}

/// Plays `input` through the loop's seam once and measures it. `start`
/// and `end` are frames at the file's rate.
pub fn check(input: &Input, start: u64, end: u64) -> Result<Seam, AudioError> {
    let (natural, rate) = Sound::decode(input)?;
    let (start, end) = (start as usize, end as usize);
    if end <= start + CLICK || end > natural.frames() {
        return Err(AudioError(format!(
            "{}: loop {start}..{end} isn't within its {} frames",
            input.describe(),
            natural.frames()
        )));
    }
    let looping = Looping::From {
        start: start as u64,
        length: Some((end - start) as u64),
    };
    let mut music = Music::open(input.clone(), Some(looping), rate)?;
    // frame `end + k` of `played` is frame `start + k` of the loop
    let frames = end + AFTER;
    let mut played = vec![0.0; frames * 2];
    music.fill_blocking(&mut played);
    let ch = natural.channels;
    let theirs = |f: isize| -> [f32; 2] {
        if f < 0 || f as usize >= natural.frames() {
            return [0.0; 2];
        }
        let f = f as usize * ch;
        [natural.samples[f], natural.samples[f + ch - 1]]
    };
    let ours = |f: usize| [played[2 * f], played[2 * f + 1]];

    let (s, e) = (start as isize, end as isize);
    let compare = |range: std::ops::Range<isize>, before: bool| {
        let (mut diff, mut level) = (0.0, 0.0);
        for k in range {
            let (a, b) = if before {
                (theirs(e + k), theirs(s + k))
            } else {
                (ours((e + k) as usize), theirs(s + k))
            };
            for c in 0..2 {
                diff += f64::from(a[c] - b[c]).powi(2);
                level += f64::from(a[c]).powi(2) + f64::from(b[c]).powi(2);
            }
        }
        db(diff, level / 2.0)
    };
    let mismatch = compare(-(AROUND.min(start) as isize)..0, true);
    let after = compare(0..AFTER.min(end - start) as isize, false);

    let step = |f: usize| {
        let (a, b) = (ours(f - 1), ours(f));
        (a[0] - b[0]).abs().max((a[1] - b[1]).abs())
    };
    let largest = (end - CLICK..end + CLICK)
        .filter(|&f| f != end)
        .map(step)
        .fold(0.0, f32::max);
    let click = db(f64::from(step(end)).powi(2), f64::from(largest).powi(2));

    let silent = |f: &usize| ours(*f).iter().all(|s| s.abs() < SILENT);
    let quiet_before = (end.saturating_sub(AROUND)..end)
        .rev()
        .take_while(silent)
        .count();
    let quiet_after = (end..end + AROUND).take_while(silent).count();
    let gap = (quiet_before + quiet_after) as f32 / rate as f32;
    Ok(Seam {
        click,
        gap,
        mismatch,
        after,
    })
}

fn db(power: f64, reference: f64) -> f32 {
    (10.0 * ((power + 1e-12) / (reference + 1e-12)).log10()) as f32
}
