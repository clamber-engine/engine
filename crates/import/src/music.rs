//! A level's song as music: the song played step by step
//! ([`Player`], `spec/formats/songs.md`), each step's voice settings
//! turned into audio ([`Mixer`], `spec/formats/voice-output.md`), with
//! the samples in place while it plays (`spec/formats/sound.md`, "Which
//! samples a song has").
//!
//! The spec doesn't cover echo or noise yet: the music is what plays
//! with the echo volume at 0, and a voice with noise on is silent. Nor
//! does it say on which output a value handed between two steps first
//! counts; here it counts from the first output after the step.

use crate::songs::{GAIN, Player, Step};
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

/// Outputs after the last voice falls silent, for the level's 2-output
/// delay (`voice-output.md`, "The voice's sound").
const AFTER_SILENT: usize = 2;

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
        render(&song, &samples).map_err(|e| format!("song {}: {e}", self.song))
    }
}

/// Stereo audio at [`RATE`] outputs a second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Music {
    /// Left and right of each output.
    pub frames: Vec<[i16; 2]>,
    /// Where it loops, in outputs; `None` for a song that ends.
    pub looping: Option<Loop>,
}

impl Music {
    /// A stereo 16-bit WAV file. Loop points aren't stored in it; they go
    /// in content metadata.
    pub fn wav(&self) -> Vec<u8> {
        let data_len = 4 * self.frames.len() as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&2u16.to_le_bytes()); // two channels
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(4 * RATE).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for frame in &self.frames {
            for value in frame {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }
}

/// How a song goes on, in steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// Every track has ended after this many steps.
    Ends(usize),
    /// After `lead_in` steps, the next `length` steps repeat forever.
    Loops { lead_in: usize, length: usize },
}

/// Finds the shape of `song` by playing it until it ends or comes back
/// to a place it was at before (Brent's cycle finding, so that only two
/// players are kept).
fn shape(song: &Song) -> Result<Shape, String> {
    let start = Player::new(song)?;
    let mut tortoise = start.clone();
    let mut hare = start.clone();
    let (mut power, mut length) = (1, 0);
    for steps in 1..=MAX_STEPS {
        hare.step()?;
        length += 1;
        if hare.ended() {
            return Ok(Shape::Ends(steps));
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
            return Ok(Shape::Loops { lead_in, length });
        }
        if length == power {
            tortoise = hare.clone();
            power *= 2;
            length = 0;
        }
    }
    Err(format!(
        "it neither ends nor loops within {MAX_STEPS} steps"
    ))
}

/// Hands the voices and the main volume what `step` gives them.
fn hand(mixer: &mut Mixer, step: &Step) {
    mixer.main_volume = step.settings.main_volume;
    for (voice, s) in mixer.voices.iter_mut().zip(&step.voices) {
        voice.sample = s.voice.sample;
        voice.pitch = s.voice.pitch;
        voice.volume = if s.noise { [0, 0] } else { s.voice.volume };
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
/// end leaves sounding. A song that ends is played until its voices fall
/// silent.
pub fn render(song: &Song, samples: &[Sample]) -> Result<Music, String> {
    let shape = shape(song)?;
    let (steps, loop_from) = match shape {
        Shape::Ends(steps) => (steps, None),
        Shape::Loops { lead_in, length } => (lead_in + 2 * length, Some(lead_in + length)),
    };
    let mut player = Player::new(song)?;
    let mut mixer = Mixer::new(0);
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
            let silent = |mixer: &Mixer| mixer.voices.iter().all(|v| v.envelope.level() == 0);
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
    Ok(Music { frames, looping })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHANNELS;

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
    fn samples_not_in_place_and_noise_are_silent() {
        let song = looping_song();
        let music = render(&song, &[]).unwrap();
        assert!(music.frames.iter().all(|f| *f == [0, 0]));

        let mut track = SETUP.to_vec();
        track.extend([26, 165, 4, 0]);
        let music = render(&self::song(128, &track), &[level_sample()]).unwrap();
        assert!(music.frames.iter().all(|f| *f == [0, 0]));
    }

    #[test]
    fn the_wav_is_stereo_at_the_output_rate() {
        let music = Music {
            frames: vec![[1, -1], [2, -2]],
            looping: None,
        };
        let wav = music.wav();
        assert_eq!(&wav[22..24], &2u16.to_le_bytes());
        assert_eq!(&wav[24..28], &RATE.to_le_bytes());
        assert_eq!(&wav[40..44], &8u32.to_le_bytes());
        assert_eq!(&wav[44..], &[1, 0, 255, 255, 2, 0, 254, 255]);
    }
}
