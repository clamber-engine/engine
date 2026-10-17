//! Voice output (`spec/formats/voice-output.md`): how the eight voices
//! become the audio the console plays. Each [`VoiceOutput`] plays its notes'
//! sample points at its pitch, blends them, and scales the blend by its
//! level ([`Envelope`]); a [`Mixer`] adds the voices up with their
//! volumes and the main volume.
//!
//! Echo and noise aren't in the spec, so the output is the one with the
//! echo volume at 0.

use std::sync::OnceLock;

use crate::songs::CHANNELS;
use crate::sound::{BLOCK_SIZE, BlockHeader, Earlier, STEPS, Sample};
use crate::voices::Envelope;

/// Outputs from the check that takes a start to the new note's first.
const NOTE_DELAY: u8 = 7;

/// Outputs around a start on which the note before goes on.
const OVERLAP: u8 = 2;

/// Pitches past this play as the pitch less it.
const PITCH_WRAP: u16 = 16_384;

/// Fractions of a point a blend's weights are worked out for.
const FRACTIONS: usize = 256;

/// The weights for points `i` to `i` + 3 at each fraction `f`, from the
/// bell in "Blending".
fn weights() -> &'static [[i32; 4]; FRACTIONS] {
    static WEIGHTS: OnceLock<[[i32; 4]; FRACTIONS]> = OnceLock::new();
    WEIGHTS.get_or_init(|| {
        let bell = |d: f64| {
            let width = 0.945_f64;
            (-d * d / (2.0 * width * width)).exp() * (1.0 + (std::f64::consts::PI * d / 2.0).cos())
                / 2.0
        };
        let mut table = [[0; 4]; FRACTIONS];
        for (f, row) in table.iter_mut().enumerate() {
            let h: [f64; 4] =
                std::array::from_fn(|k| bell(k as f64 - 1.0 - (f as f64 + 0.5) / 256.0));
            let total: f64 = h.iter().sum();
            *row = h.map(|h| (2048.0 * h / total).round() as i32);
        }
        table
    })
}

/// Rounds to even: takes 1 off an odd number.
fn even(n: i32) -> i32 {
    n & !1
}

fn limit(n: i32) -> i32 {
    n.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

/// A volume byte read as signed.
fn signed(volume: u8) -> i32 {
    i32::from(volume as i8)
}

/// What a voice plays from a start to the next start.
#[derive(Clone, Debug)]
struct Note {
    /// `P`, in 1/4096ths of a point.
    position: u64,
    /// The points taken and not yet passed; `points[0]` is point `first`.
    points: Vec<i16>,
    first: usize,
    /// Blocks taken after block 0.
    taken: usize,
    /// The sample the next block comes from, and its number there.
    sample: u8,
    block: usize,
    /// The last block taken had its end flag set.
    after_end: bool,
    earlier: Earlier,
    ended: bool,
}

impl Note {
    /// A note taking block 0 of `sample`.
    fn new(samples: &[Sample], sample: u8) -> Note {
        let mut note = Note {
            position: 0,
            points: Vec::new(),
            first: 0,
            taken: 0,
            sample,
            block: 0,
            after_end: false,
            earlier: Earlier::default(),
            ended: false,
        };
        note.take(samples, sample);
        note
    }

    /// `i`, the whole part of the position.
    fn point(&self) -> usize {
        (self.position / 4096) as usize
    }

    /// `f`, the fraction from 0 to 255.
    fn fraction(&self) -> usize {
        ((self.position % 4096) / 16) as usize
    }

    /// Takes the next block, with the voice set to `current`. Returns
    /// whether the note ended on it.
    fn take(&mut self, samples: &[Sample], current: u8) -> bool {
        if self.after_end {
            // After the last block: the loop block of the sample the
            // voice is set to now.
            self.sample = current;
            self.block = samples
                .get(usize::from(current))
                .map_or(0, |s| usize::from(s.loop_offset) / BLOCK_SIZE);
        }
        let block = samples
            .get(usize::from(self.sample))
            .and_then(|s| s.blocks().nth(self.block));
        // A sample whose data runs out before a block with the end flag
        // ends the note there, as a one-shot sample does.
        let Some(block) = block else {
            self.ended = true;
            return true;
        };
        let header = BlockHeader::new(block[0]);
        self.earlier.decode([block], &mut self.points);
        self.block += 1;
        self.after_end = header.end;
        if header.end && !header.loops {
            self.ended = true;
        }
        self.ended
    }

    /// Moves on by `pitch`, then takes the blocks due. Returns whether
    /// the note ended on this output.
    fn advance(&mut self, samples: &[Sample], current: u8, pitch: u16) -> bool {
        self.position += u64::from(pitch % PITCH_WRAP);
        self.take_due(samples, current)
    }

    /// Takes block `m` once `i` reaches 16`m` − 12.
    fn take_due(&mut self, samples: &[Sample], current: u8) -> bool {
        let mut ended = false;
        while !self.ended && self.point() + 12 >= STEPS * (self.taken + 1) {
            self.taken += 1;
            ended = self.take(samples, current);
        }
        // Points before `i` are never blended again.
        let passed = self
            .point()
            .saturating_sub(self.first)
            .min(self.points.len());
        self.points.drain(..passed);
        self.first += passed;
        ended
    }

    /// The blend `B` at the note's position.
    fn blend(&self) -> i32 {
        let w = &weights()[self.fraction()];
        let i = self.point();
        let a: [i32; 4] = std::array::from_fn(|k| {
            let x = (i + k)
                .checked_sub(self.first)
                .and_then(|at| self.points.get(at))
                .map_or(0, |&x| i32::from(x));
            (w[k] * x) >> 11
        });
        let wrapped = (a[0] + a[1] + a[2]) as i16;
        even(limit(i32::from(wrapped) + a[3]))
    }
}

/// One voice: what it has been handed, its level, and the note it plays.
#[derive(Clone, Debug, Default)]
pub struct VoiceOutput {
    pub envelope: Envelope,
    pub pitch: u16,
    /// Left and right, as bytes.
    pub volume: [u8; 2],
    pub sample: u8,
    note: Option<Note>,
    /// The note a start took, and outputs since the check that took it.
    next: Option<(Note, u8)>,
    /// The levels of the last two outputs, the earlier first.
    levels: [u16; 2],
}

impl VoiceOutput {
    pub fn new() -> VoiceOutput {
        VoiceOutput::default()
    }

    /// The voice's sound `y` on the output where the shared count is
    /// `count`, playing `samples`.
    pub fn output(&mut self, samples: &[Sample], count: u64) -> i32 {
        let mut level = self.envelope.output(count);
        if self.envelope.took_start() {
            self.next = Some((Note::new(samples, self.sample), 0));
        } else if let Some((_, since)) = &mut self.next {
            *since += 1;
        }
        let since = self.next.as_ref().map(|(_, since)| *since);
        if since == Some(OVERLAP) {
            self.note = None;
        }
        if since == Some(NOTE_DELAY) {
            let (note, _) = self.next.take().unwrap();
            self.note = Some(note);
        } else if let Some(note) = &mut self.note
            && note.advance(samples, self.sample, self.pitch)
            // The note ends. (With a new note's start already taken, the
            // spec doesn't say; this keeps the new note.)
            && self.next.is_none()
        {
            self.envelope.end();
            level = self.envelope.level();
        }
        let heard = self.levels[0];
        self.levels = [self.levels[1], level];
        match &self.note {
            Some(note) => even((note.blend() * i32::from(heard)) >> 11),
            None => 0,
        }
    }
}

/// The eight voices and the main volume, giving the audio output one
/// output at a time.
#[derive(Clone, Debug)]
pub struct Mixer {
    pub voices: [VoiceOutput; CHANNELS],
    /// Left and right, as bytes.
    pub main_volume: [u8; 2],
    /// The shared count on the next output.
    pub count: u64,
}

impl Mixer {
    /// Voices that have never started, main volume 0, and the shared
    /// count `count` on the first output.
    pub fn new(count: u64) -> Mixer {
        Mixer {
            voices: Default::default(),
            main_volume: [0, 0],
            count,
        }
    }

    /// The next output's left and right values.
    pub fn output(&mut self, samples: &[Sample]) -> [i16; 2] {
        let mut sums = [0; 2];
        for voice in &mut self.voices {
            let y = voice.output(samples, self.count);
            for (sum, volume) in sums.iter_mut().zip(voice.volume) {
                *sum = limit(*sum + ((y * signed(volume)) >> 7));
            }
        }
        self.count += 1;
        // 32 768, from −32 768 under main volume −128, wraps to −32 768.
        std::array::from_fn(|side| ((sums[side] * signed(self.main_volume[side])) >> 7) as i16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/voice-output/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// A sample given as `blocks/loop`.
    fn sample(s: &str) -> Sample {
        let (blocks, loop_block) = s.split_once('/').unwrap();
        let loop_block: u16 = loop_block.parse().unwrap();
        Sample {
            loop_offset: loop_block * BLOCK_SIZE as u16,
            data: hex(blocks),
        }
    }

    /// Runs a row of `cases.tsv`, giving its rows as `outputs.tsv` has
    /// them.
    fn run(case: &str) -> Vec<String> {
        let fields: Vec<&str> = case.split('\t').collect();
        let [name, first, outputs, samples, events] = fields[..] else {
            panic!("{case}");
        };
        let samples: Vec<Sample> = samples.split(';').map(sample).collect();
        let events: Vec<(u64, Vec<&str>)> = events
            .split(';')
            .map(|e| {
                let mut words = e.split_whitespace();
                let k = words.next().unwrap().parse().unwrap();
                (k, words.collect())
            })
            .collect();
        let mut mixer = Mixer::new(first.parse().unwrap());
        let mut rows = Vec::new();
        for k in 0..outputs.parse().unwrap() {
            for (_, event) in events.iter().filter(|(at, _)| *at == k) {
                let number = |i: usize| event[i].parse::<u16>().unwrap();
                let byte = |i: usize| event[i].parse::<u8>().unwrap();
                if event[0] == "main" {
                    mixer.main_volume = [byte(1), byte(2)];
                    continue;
                }
                let n: usize = event[0].strip_prefix('v').unwrap().parse().unwrap();
                let voice = &mut mixer.voices[n];
                match event[1] {
                    "pitch" => voice.pitch = number(2),
                    "vol" => voice.volume = [byte(2), byte(3)],
                    "sample" => voice.sample = byte(2),
                    "env" => voice.envelope.set([byte(2), byte(3)], byte(4)),
                    "start" => voice.envelope.start(),
                    "release" => voice.envelope.release(),
                    other => panic!("unknown event {other}"),
                }
            }
            let [left, right] = mixer.output(&samples);
            rows.push(format!("{name}\t{k}\t{left}\t{right}"));
        }
        rows
    }

    #[test]
    fn outputs_match_the_spec_vectors() {
        let expected: Vec<String> = vector("outputs.tsv")
            .lines()
            .skip(1)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        let mut rows = Vec::new();
        for case in vector("cases.tsv")
            .lines()
            .skip(1)
            .filter(|l| !l.is_empty())
        {
            rows.extend(run(case));
        }
        assert_eq!(rows.len(), expected.len());
        for (row, expected) in rows.iter().zip(&expected) {
            assert_eq!(row, expected);
        }
    }

    #[test]
    fn weights_are_the_spec_examples() {
        assert_eq!(weights()[0], [370, 1303, 374, 0]);
        assert_eq!(weights()[128], [54, 967, 972, 55]);
        for f in 0..FRACTIONS {
            let w = weights()[f];
            let mut turned = weights()[255 - f];
            turned.reverse();
            assert_eq!(w, turned, "{f}");
            assert!((2047..=2049).contains(&w.iter().sum::<i32>()), "{f}");
        }
    }

    #[test]
    fn the_worked_example() {
        // Points all 4 096: a filter-0 block of shift 12 and steps 1
        // gives (1 << 12) >> 1, doubled.
        let block = [0xC3, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11];
        let samples = [Sample {
            loop_offset: 0,
            data: block.to_vec(),
        }];
        let mut mixer = Mixer::new(1);
        mixer.main_volume = [127, 127];
        let voice = &mut mixer.voices[0];
        voice.pitch = 4096;
        voice.volume = [127, 127];
        voice.envelope.set([143, 224], 0);
        voice.envelope.start();
        let last = (0..40).map(|_| mixer.output(&samples)).last().unwrap();
        assert_eq!(last, [4028, 4028]);
    }
}
