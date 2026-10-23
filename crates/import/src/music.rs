//! A level's song as music: the song played step by step
//! ([`Player`], `spec/formats/songs.md`), each step's voice settings
//! turned into audio ([`Mixer`], `spec/formats/voice-output.md` and
//! `spec/formats/echo.md`), with the samples in place while it plays
//! (`spec/formats/sound.md`, "Which samples a song has").
//!
//! The spec doesn't say on which output a value handed between two steps
//! first counts; here it counts from the first output after the step.
//! Nor does it cover the noise register's value when a song starts; here
//! each song starts it as the emulation measured did.

use clamber_audio::encode::Vorbis;

use crate::songs::{CHANNELS, GAIN, Player, Step};
use crate::sound::{Loop, Read, Sample, read_samples, read_song, song_samples};
use crate::voice_output::Mixer;
use crate::{Location, Song};

/// Audio outputs a second.
pub const RATE: u32 = 32_000;

/// Steps played to find where a song loops: about 40 minutes at 80
/// steps a second.
const MAX_STEPS: usize = 200_000;

/// Outputs after a song ends that are kept while voices still sound:
/// 10 seconds.
const MAX_TAIL: usize = 10 * RATE as usize;

/// `title-1`'s echo delay (`echo.md`, "In `title-1`").
const ECHO_DELAY: u8 = 4;

/// Outputs after the last voice falls silent, for the level's 2-output
/// delay (`voice-output.md`, "The voice's sound").
const AFTER_SILENT: usize = 2;

/// The logic frames from the cut of the song that was playing to the
/// start of song `song`, when the game changes song (`music.md`, "A
/// change of song"). Song 12, the map's, depends on whether the level
/// was finished before, so it has none here.
pub fn silent_frames(song: u16) -> Option<u32> {
    match song {
        0 => Some(70),
        16 => Some(48),
        17 => Some(30),
        23 => Some(31),
        25 => Some(32),
        26 => Some(44),
        _ => None,
    }
}

/// Where a level's song and its samples are in an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelSong {
    pub sample_table: Location,
    pub shared_samples: Location,
    pub sample_sets: Location,
    pub song_table: Location,
    /// The song's number, which is also its sample set's.
    pub song: u16,
}

impl LevelSong {
    /// The song as music, with only the samples in place while it plays.
    pub fn music<'a>(&self, read: impl Read<'a> + Copy) -> Result<Music, String> {
        let numbers = song_samples(read, self.shared_samples, self.sample_sets, self.song)?;
        let count = numbers.iter().max().map_or(0, |&n| n + 1);
        let mut samples = read_samples(read, self.sample_table, count)?;
        // A sample that isn't in place plays nothing.
        for (n, sample) in samples.iter_mut().enumerate() {
            if !numbers.iter().any(|&k| usize::from(k) == n) {
                sample.data.clear();
            }
        }
        let song = read_song(read, self.song_table, self.song)?;
        render(&song, &samples)
    }

    /// [`diagnose`] for the song.
    pub fn diagnose<'a>(&self, read: impl Read<'a> + Copy) -> Vec<String> {
        match read_song(read, self.song_table, self.song) {
            Ok(song) => diagnose(&song),
            Err(e) => vec![format!("can't read the song: {e}")],
        }
    }
}

/// Stereo audio at [`RATE`] outputs a second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Music {
    /// Left and right of each output.
    pub frames: Vec<[i16; 2]>,
    /// Where it loops, in outputs; `None` for a song that ends.
    pub looping: Option<Loop>,
    /// How closely it comes back to where its loop starts.
    pub fit: Fit,
}

impl Music {
    /// A stereo Ogg Vorbis file at `quality` (-1 to 10). Loop points
    /// aren't stored in it; they go in content metadata, and are frames
    /// of the file as they are of [`frames`](Music::frames).
    pub fn ogg(&self, quality: f32) -> Result<Vec<u8>, String> {
        let samples: Vec<i16> = self.frames.iter().flatten().copied().collect();
        let vorbis = Vorbis {
            quality,
            ..Vorbis::default()
        };
        vorbis.encode(&samples, 2, RATE).map_err(|e| e.to_string())
    }
}

/// How a song goes on, in steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// Every track has ended after this many steps.
    Ends(usize),
    /// After `lead_in` steps, the next `length` steps repeat forever, as
    /// closely as `fit` says.
    Loops {
        lead_in: usize,
        length: usize,
        fit: Fit,
    },
}

/// How closely a song's music comes back to where its loop starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Exactly where it was; also for a song that ends.
    Exact,
    /// Where it was in the score ([`Player::same_score_place`]), give or
    /// take a step and the effects' changes.
    Score,
    /// The track with the longest loop comes back to where it was in the
    /// score; tracks whose loops are shorter and don't fit into it jump
    /// at the seam ([`Player::same_track_place`]).
    Tracks,
}

impl Fit {
    /// How the song loops, for the import report.
    pub fn describe(self) -> &'static str {
        match self {
            Fit::Exact => "loops exactly",
            Fit::Score => "loops where its score comes back, give or take a step",
            Fit::Tracks => {
                "its tracks loop apart; loops on the longest track's loop, \
                 and the other tracks jump at the seam"
            }
        }
    }
}

/// Finds the shape of `song` by playing it until it ends or comes back
/// to a place it was at before. A song can take far longer to come back
/// to exactly where it was than to where it was in the score: with a
/// tempo that doesn't divide 256, the tempo sum only comes back after
/// several times round, and an effect's counters may never line up with
/// the notes again. Such a song loops where its score comes back. And
/// when its tracks loop with lengths that only fit together after very
/// many beats, it loops on its longest track's loop.
fn shape(song: &Song) -> Result<Shape, String> {
    if let Some(shape) = exact_shape(song)? {
        return Ok(shape);
    }
    if let Some(shape) = score_shape(song)? {
        return Ok(shape);
    }
    if let Some(shape) = track_shape(song)? {
        return Ok(shape);
    }
    Err(format!(
        "it neither ends nor loops within {MAX_STEPS} steps"
    ))
}

/// The shape of `song` where it comes back to exactly where it was
/// within [`MAX_STEPS`] (Brent's cycle finding, so that only two
/// players are kept).
fn exact_shape(song: &Song) -> Result<Option<Shape>, String> {
    let start = Player::new(song)?;
    let mut tortoise = start.clone();
    let mut hare = start.clone();
    let (mut power, mut length) = (1, 0);
    for steps in 1..=MAX_STEPS {
        hare.step()?;
        length += 1;
        if hare.ended() {
            return Ok(Some(Shape::Ends(steps)));
        }
        if hare.same_place(&tortoise) {
            // The loop is `length` steps long; it starts at the first
            // step whose place comes back `length` steps later.
            let mut tortoise = start.clone();
            let mut hare = start;
            for _ in 0..length {
                hare.step()?;
            }
            let mut lead_in = 0;
            while !hare.same_place(&tortoise) {
                tortoise.step()?;
                hare.step()?;
                lead_in += 1;
            }
            return Ok(Some(Shape::Loops {
                lead_in,
                length,
                fit: Fit::Exact,
            }));
        }
        if length == power {
            tortoise = hare.clone();
            power *= 2;
            length = 0;
        }
    }
    Ok(None)
}

/// Plays `player` up to and including its next beat, giving the steps
/// that took; `None` if there's no beat within 256 steps (tempo 0).
fn to_beat(player: &mut Player) -> Result<Option<usize>, String> {
    for steps in 1..=256 {
        if player.step()?.beat {
            return Ok(Some(steps));
        }
    }
    Ok(None)
}

/// Where, a beat at a time, `song` first comes back to a place `same`
/// says it was at before within [`MAX_STEPS`], as [`exact_shape`] does
/// a step at a time: the beat after which the loop starts (from 1), and
/// the loop's length in beats. `None` if it doesn't, or the song ends.
fn beat_loop(
    song: &Song,
    same: impl Fn(&Player, &Player) -> bool,
) -> Result<Option<(usize, usize)>, String> {
    let mut first = Player::new(song)?;
    let Some(mut steps) = to_beat(&mut first)? else {
        return Ok(None);
    };
    let mut tortoise = first.clone();
    let mut hare = first.clone();
    let (mut power, mut length) = (1, 0);
    loop {
        let Some(n) = to_beat(&mut hare)? else {
            return Ok(None);
        };
        steps += n;
        length += 1;
        if steps > MAX_STEPS || hare.ended() {
            return Ok(None);
        }
        if same(&hare, &tortoise) {
            break;
        }
        if length == power {
            tortoise = hare.clone();
            power *= 2;
            length = 0;
        }
    }
    // The loop starts at the first beat whose place comes back `length`
    // beats later.
    let mut tortoise = first.clone();
    let mut hare = first;
    for _ in 0..length {
        beat(&mut hare)?;
    }
    let mut start = 1;
    while !same(&hare, &tortoise) {
        beat(&mut tortoise)?;
        beat(&mut hare)?;
        start += 1;
    }
    Ok(Some((start, length)))
}

/// [`to_beat`] for a song known to have beats.
fn beat(player: &mut Player) -> Result<usize, String> {
    to_beat(player)?.ok_or_else(|| "a beat went missing".to_string())
}

/// The loop that starts after beat `start` and is `length` beats long,
/// in steps: as long as it is that first time round.
fn beats_to_steps(song: &Song, start: usize, length: usize, fit: Fit) -> Result<Shape, String> {
    let mut player = Player::new(song)?;
    let mut lead_in = 0;
    for _ in 0..start {
        lead_in += beat(&mut player)?;
    }
    let mut steps = 0;
    for _ in 0..length {
        steps += beat(&mut player)?;
    }
    Ok(Shape::Loops {
        lead_in,
        length: steps,
        fit,
    })
}

/// The shape of `song` where, on a beat, it comes back to where it was
/// in the score within [`MAX_STEPS`].
fn score_shape(song: &Song) -> Result<Option<Shape>, String> {
    match beat_loop(song, |a, b| a.same_score_place(b))? {
        Some((start, length)) => beats_to_steps(song, start, length, Fit::Score).map(Some),
        None => Ok(None),
    }
}

/// The shape of `song` from where its last track to start looping
/// starts, as long as its longest track's loop, each track's loop found
/// on its own as in [`score_shape`].
fn track_shape(song: &Song) -> Result<Option<Shape>, String> {
    let (mut start, mut length) = (1, 1);
    for i in 0..CHANNELS {
        let Some((s, l)) = beat_loop(song, |a, b| a.same_track_place(i, b))? else {
            return Ok(None);
        };
        start = start.max(s);
        length = length.max(l);
    }
    beats_to_steps(song, start, length, Fit::Tracks).map(Some)
}

/// Counts, step numbers and states for finding out why `song` doesn't
/// end or loop, one line each: whether and where it comes back to
/// where it was, exactly and in the score, and how often each part of
/// the player's place repeats over the last half of [`MAX_STEPS`]
/// steps. Nothing in it is from the song's data.
pub fn diagnose(song: &Song) -> Vec<String> {
    diagnose_for(song, MAX_STEPS)
}

fn diagnose_for(song: &Song, max_steps: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut shape_line = |what: &str, shape: Result<Option<Shape>, String>| {
        lines.push(match shape {
            Ok(Some(Shape::Ends(steps))) => format!("{what}: ends after {steps} steps"),
            Ok(Some(Shape::Loops {
                lead_in, length, ..
            })) => format!("{what}: loops every {length} steps from step {lead_in}"),
            Ok(None) => format!("{what}: doesn't come back within {MAX_STEPS} steps"),
            Err(e) => format!("{what}: stops: {e}"),
        })
    };
    shape_line("exactly", exact_shape(song));
    shape_line("in the score", score_shape(song));
    shape_line("by track", track_shape(song));

    let mut player = match Player::new(song) {
        Ok(player) => player,
        Err(e) => {
            lines.push(format!("can't start the song: {e}"));
            return lines;
        }
    };
    let names: Vec<String> = player.place_parts().into_iter().map(|(n, _)| n).collect();
    let mut prints = vec![Vec::with_capacity(max_steps); names.len()];
    let (mut beats, mut depth) = (0, 0);
    let mut ended = [None; CHANNELS];
    for steps in 1..=max_steps {
        match player.step() {
            Ok(step) => beats += usize::from(step.beat),
            Err(e) => {
                lines.push(format!("stops at step {steps}: {e}"));
                break;
            }
        }
        depth = depth.max(player.call_depth());
        for (i, &done) in player.ended_channels().iter().enumerate() {
            if done && ended[i].is_none() {
                ended[i] = Some(steps);
            }
        }
        for (part, (_, print)) in prints.iter_mut().zip(player.place_parts()) {
            // Fingerprints are only compared; half of one is plenty.
            part.push(print as u32);
        }
    }
    let played = prints[0].len();
    lines.push(format!(
        "played {played} steps, {beats} beats; at most {depth} calls deep"
    ));
    let ended: Vec<String> = (ended.iter().enumerate())
        .filter_map(|(i, at)| at.map(|at| format!("channel {i} at step {at}")))
        .collect();
    lines.push(match ended.is_empty() {
        true => "no channel ended".into(),
        false => format!("ended: {}", ended.join(", ")),
    });
    for (name, part) in names.iter().zip(&prints) {
        lines.push(match repeats(part) {
            Some((1, 1)) => format!("{name}: stays the same"),
            Some((from, 1)) => format!("{name}: stays the same from step {from}"),
            Some((from, period)) => {
                format!("{name}: repeats every {period} steps from step {from}")
            }
            None => format!(
                "{name}: doesn't repeat over the last {} steps",
                played - played / 2
            ),
        });
    }
    lines
}

/// The shortest period with which `prints` (of the place after steps 1,
/// 2, …) repeats over its second half, and the step from which it
/// does.
fn repeats(prints: &[u32]) -> Option<(usize, usize)> {
    let n = prints.len();
    let half = n / 2;
    let last = *prints.last()?;
    let candidates = (half..n - 1).rev().filter(|&j| prints[j] == last);
    for j in candidates.take(64) {
        let period = n - 1 - j;
        if (half + period..n).all(|i| prints[i] == prints[i - period]) {
            let mut from = n - 1 - period;
            while from > 0 && prints[from - 1] == prints[from - 1 + period] {
                from -= 1;
            }
            return Some((from + 1, period));
        }
    }
    None
}

/// Hands the voices, the main volume and the echo what `step` gives
/// them.
fn hand(mixer: &mut Mixer, step: &Step) {
    let settings = &step.settings;
    mixer.main_volume = settings.main_volume;
    mixer.echo.volume = settings.echo_volume;
    mixer.echo.feedback = settings.echo_feedback;
    mixer.echo.filter = settings.echo_filter;
    mixer.echo.writing = settings.echo_writing;
    mixer.noise_rate = settings.noise_rate;
    for (voice, s) in mixer.voices.iter_mut().zip(&step.voices) {
        voice.echo = s.echo;
        voice.sample = s.voice.sample;
        voice.pitch = s.voice.pitch;
        voice.noise = s.noise;
        voice.volume = s.voice.volume;
        voice.envelope.set(s.voice.envelope, GAIN);
        if s.released {
            voice.envelope.release();
        }
        if s.started {
            voice.envelope.start();
        }
    }
}

/// Plays `song` with `samples` (by sample number) as music. A song that
/// loops is played through its loop twice and loops over the second
/// time, so that what leads into the loop's start is what the loop's
/// end leaves sounding. A song that ends is played until its voices and
/// its echo fall silent.
pub fn render(song: &Song, samples: &[Sample]) -> Result<Music, String> {
    let shape = shape(song)?;
    let (steps, loop_from, fit) = match shape {
        Shape::Ends(steps) => (steps, None, Fit::Exact),
        Shape::Loops {
            lead_in,
            length,
            fit,
        } => (lead_in + 2 * length, Some(lead_in + length), fit),
    };
    let mut player = Player::new(song)?;
    // The memory starts all 0 when the sound starts; whether a song
    // starting later finds it so isn't covered, so each starts with it so.
    let mut mixer = Mixer::new(0, ECHO_DELAY);
    let mut frames = Vec::new();
    let mut loop_start = 0;
    for n in 0..steps {
        if Some(n) == loop_from {
            loop_start = frames.len();
        }
        let step = player.step()?;
        hand(&mut mixer, &step);
        for _ in 0..step.outputs() {
            frames.push(mixer.output(samples));
        }
    }
    let looping = match loop_from {
        Some(_) => Some(Loop {
            start: loop_start,
            length: frames.len() - loop_start,
        }),
        None => {
            let silent = |mixer: &Mixer| {
                mixer.voices.iter().all(|v| v.envelope.level() == 0) && mixer.echo.is_silent()
            };
            let mut quiet = 0;
            for _ in 0..MAX_TAIL {
                if quiet > AFTER_SILENT {
                    break;
                }
                frames.push(mixer.output(samples));
                quiet = if silent(&mixer) { quiet + 1 } else { 0 };
            }
            None
        }
    };
    Ok(Music {
        frames,
        looping,
        fit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HEADER_SIZE;
    use clamber_audio::encode::DEFAULT_QUALITY;

    /// A looping sample whose points are all 4 096 (as in
    /// `voice_output.rs`'s worked example).
    fn level_sample() -> Sample {
        Sample {
            loop_offset: 0,
            data: vec![0xC3, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11],
        }
    }

    /// A song whose channel 0 plays `track` (from byte 19) and whose
    /// other tracks end at once (byte 18).
    fn song(tempo: u8, track: &[u8]) -> Song {
        let mut tracks = [18u16; CHANNELS];
        tracks[0] = 19;
        let mut data: Vec<u8> = tracks.iter().flat_map(|t| t.to_le_bytes()).collect();
        data.extend([tempo, 0, 0]);
        data.extend(track);
        Song { base: 0, data }
    }

    /// Period 100, sample 0, volume and envelope (attack 15, sustain at
    /// full level), as a track's first commands.
    const SETUP: [u8; 10] = [42, 100, 1, 0, 2, 127, 127, 16, 143, 224];

    /// A track that plays a note of 4 beats, then goes round a note of 2
    /// beats and a rest of 2 beats forever.
    fn looping_song() -> Song {
        let mut track = SETUP.to_vec();
        track.extend([165, 4]);
        let back = 19 + track.len() as u16;
        track.extend([165, 2, 128, 2, 3]);
        track.extend(back.to_le_bytes());
        song(128, &track)
    }

    #[test]
    fn a_looping_song_loops_over_its_second_time_round() {
        let song = looping_song();
        // Beats on every other step: the loop is 4 beats, 8 steps.
        let Shape::Loops { length, .. } = shape(&song).unwrap() else {
            panic!("the song doesn't loop");
        };
        assert_eq!(length, 8);
        let music = render(&song, &[level_sample()]).unwrap();
        let looping = music.looping.unwrap();
        // Period 100: 400 outputs a step.
        assert_eq!(looping.length, 8 * 400);
        assert_eq!(looping.start + looping.length, music.frames.len());
        // The second time round sounds as the first did.
        let second = &music.frames[looping.start..];
        let first = &music.frames[looping.start - looping.length..looping.start];
        assert_eq!(first, second);
        assert!(second.iter().any(|f| f[0] != 0));
    }

    /// A song whose channels play `tracks`, one after another from byte
    /// 19, and whose other channels end at once (byte 18). Each track is
    /// [`SETUP`], then a note of `a` beats and one of `b` beats round and
    /// round.
    fn round_song(tempo: u8, tracks: &[(u8, u8)]) -> Song {
        let mut data = vec![0u8; HEADER_SIZE];
        data[16] = tempo;
        data.push(0);
        for (i, &(a, b)) in tracks.iter().enumerate() {
            let at = (data.len() as u16).to_le_bytes();
            data[2 * i..2 * i + 2].copy_from_slice(&at);
            data.extend(SETUP);
            let back = data.len() as u16;
            data.extend([165, a, 165, b, 3]);
            data.extend(back.to_le_bytes());
        }
        for i in tracks.len()..CHANNELS {
            data[2 * i..2 * i + 2].copy_from_slice(&18u16.to_le_bytes());
        }
        Song { base: 0, data }
    }

    /// Tempo 3: three beats every 256 steps, so a loop of 401 beats only
    /// comes back with the tempo sum where it was after 401 × 256 steps,
    /// too many to find. The score comes back after about 401 × 256 / 3.
    #[test]
    fn a_song_whose_tempo_sum_comes_back_late_loops_where_its_score_does() {
        let song = round_song(3, &[(200, 201)]);
        assert_eq!(exact_shape(&song).unwrap(), None);
        let Shape::Loops { length, fit, .. } = shape(&song).unwrap() else {
            panic!("the song doesn't loop");
        };
        assert_eq!(fit, Fit::Score);
        assert!(length.abs_diff(401 * 256 / 3) <= 1, "{length}");
    }

    /// Tracks of 401 and 400 beats only come back together after 160 400
    /// beats, too many to find: the song loops on the longer track's
    /// loop, and the diagnostics say which parts repeat how often.
    #[test]
    fn a_song_whose_tracks_loop_apart_loops_on_its_longest_track() {
        let song = round_song(128, &[(200, 201), (200, 200)]);
        // Beats on every other step; both tracks loop from their first
        // note, read on step 2.
        let want = Shape::Loops {
            lead_in: 2,
            length: 802,
            fit: Fit::Tracks,
        };
        assert_eq!(shape(&song).unwrap(), want);
        let music = render(&song, &[level_sample()]).unwrap();
        assert_eq!(music.fit, Fit::Tracks);
        assert_eq!(music.looping.unwrap().length, 802 * 400);

        let lines = diagnose_for(&song, 4000);
        for line in [
            format!("exactly: doesn't come back within {MAX_STEPS} steps"),
            format!("in the score: doesn't come back within {MAX_STEPS} steps"),
            "by track: loops every 802 steps from step 2".into(),
            "played 4000 steps, 2000 beats; at most 0 calls deep".into(),
            "ended: channel 2 at step 2, channel 3 at step 2, channel 4 at step 2, \
             channel 5 at step 2, channel 6 at step 2, channel 7 at step 2"
                .into(),
            "channel 0's position: repeats every 802 steps from step 2".into(),
            "channel 1's position: repeats every 800 steps from step 2".into(),
            "channel 2's position: stays the same from step 2".into(),
            "the tempo sum: repeats every 2 steps from step 1".into(),
            "the effect tempo sum: stays the same".into(),
            "the place in the score: doesn't repeat over the last 2000 steps".into(),
        ] {
            assert!(lines.contains(&line), "no `{line}` in {lines:#?}");
        }
    }

    /// A track that raises its transposition by 64 each time round comes
    /// back to where it was after two times round: notes only use it mod
    /// 128.
    #[test]
    fn a_song_raising_its_transposition_comes_back_when_its_notes_do() {
        let mut track = SETUP.to_vec();
        let back = 19 + track.len() as u16;
        track.extend([20, 64, 165, 2, 3]);
        track.extend(back.to_le_bytes());
        let Some(Shape::Loops { length, fit, .. }) = exact_shape(&song(128, &track)).unwrap()
        else {
            panic!("the song doesn't loop");
        };
        assert_eq!((length, fit), (8, Fit::Exact));
    }

    #[test]
    fn a_song_that_ends_plays_until_its_voices_fall_silent() {
        let mut track = SETUP.to_vec();
        track.extend([165, 4, 0]);
        let song = song(128, &track);
        assert!(matches!(shape(&song).unwrap(), Shape::Ends(_)));
        let music = render(&song, &[level_sample()]).unwrap();
        assert_eq!(music.looping, None);
        assert!(music.frames.iter().any(|f| f[0] != 0));
        let end = &music.frames[music.frames.len() - AFTER_SILENT..];
        assert!(end.iter().all(|f| *f == [0, 0]));
        assert!(music.frames.len() < 2 * RATE as usize);
    }

    #[test]
    fn a_song_with_echo_sounds_on_until_the_echo_falls_silent() {
        let ends = |echo: &[u8]| {
            let mut track = SETUP.to_vec();
            track.extend(echo);
            track.extend([165, 4, 0]);
            render(&song(128, &track), &[level_sample()]).unwrap()
        };
        let dry = ends(&[]);
        // Feedback 0 and echo volume 64, 64; echo on for the voice; a
        // filter that gives back what was written, 7 outputs late.
        let wet = ends(&[21, 0, 64, 64, 22, 24, 127, 0, 0, 0, 0, 0, 0, 0]);
        // Nothing comes back before the memory has gone round once.
        let round = 512 * usize::from(ECHO_DELAY);
        assert_eq!(dry.frames[..round], wet.frames[..round]);
        assert_ne!(dry.frames, wet.frames[..dry.frames.len()]);
        // It comes back a round after the last sound.
        let last = |music: &Music| music.frames.iter().rposition(|f| *f != [0, 0]).unwrap();
        assert!(last(&wet) >= last(&dry) + round);
        let end = &wet.frames[wet.frames.len() - AFTER_SILENT..];
        assert!(end.iter().all(|f| *f == [0, 0]));
    }

    #[test]
    fn samples_not_in_place_are_silent() {
        let song = looping_song();
        let music = render(&song, &[]).unwrap();
        assert!(music.frames.iter().all(|f| *f == [0, 0]));
    }

    #[test]
    fn a_voice_with_noise_on_sounds_the_noise_register() {
        let notes = |commands: &[u8]| {
            let mut track = SETUP.to_vec();
            track.extend(commands);
            track.extend([165, 4, 0]);
            render(&self::song(128, &track), &[level_sample()]).unwrap()
        };
        let sample = notes(&[]);
        // Rate 0: the register keeps its value, so the sound is steady,
        // and not the sample's.
        let steady = notes(&[26]);
        let heard = |music: &Music| -> Vec<[i16; 2]> {
            music
                .frames
                .iter()
                .copied()
                .filter(|f| *f != [0, 0])
                .collect()
        };
        let steady = heard(&steady);
        assert!(!steady.is_empty());
        assert_ne!(steady, heard(&sample));
        let full = steady.iter().max_by_key(|f| f[0].unsigned_abs()).unwrap();
        assert!(full[0] < -30_000, "{full:?}");
        // Rate 31: a step on every output.
        let stepping = heard(&notes(&[25, 31, 26]));
        assert!(stepping.iter().any(|f| f[0] > 0));
        assert!(stepping.iter().any(|f| f[0] < 0));
        // Noise off again plays the sample.
        assert_eq!(heard(&notes(&[25, 31, 26, 27])), heard(&sample));
    }

    #[test]
    fn the_ogg_is_stereo_at_the_output_rate_with_every_frame() {
        let music = Music {
            frames: (0..1000).map(|i| [i, -i]).collect(),
            looping: None,
            fit: Fit::Exact,
        };
        let ogg = music.ogg(DEFAULT_QUALITY).unwrap();
        let input = clamber_audio::Input::bytes(ogg, Some("ogg"));
        let (sound, rate) = clamber_audio::Sound::decode(&input).unwrap();
        assert_eq!((sound.channels, rate, sound.frames()), (2, RATE, 1000));
    }

    #[test]
    fn a_quality_out_of_range_is_an_error() {
        let music = Music {
            frames: vec![[0, 0]; 10],
            looping: None,
            fit: Fit::Exact,
        };
        assert!(music.ogg(12.0).unwrap_err().contains("quality"));
    }
}
