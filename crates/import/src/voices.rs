//! Voices (`spec/formats/voices.md`): a voice's level on each audio
//! output, from the envelope settings and gain it is handed
//! (`songs.md`), its starts and its releases.
//!
//! How the level scales the sample is in [`crate::voice_output`].

/// The highest level.
pub const MAX_LEVEL: u16 = 2047;

/// Outputs of level 0 after a start is taken.
const START_ZEROS: u8 = 4;

/// How much a release takes off each output.
const RELEASE_STEP: u16 = 8;

/// The period of each rate from 1 to 31, in outputs.
const PERIODS: [u32; 31] = [
    2048, 1536, 1280, 1024, 768, 640, 512, //
    384, 320, 256, 192, 160, 128, 96, 80, //
    64, 48, 40, 32, 24, 20, 16, 12, //
    10, 8, 6, 5, 4, 3, 2, 1,
];

/// The period of `rate`, or `None` for rate 0 (which never steps) and
/// rates over 31.
pub fn period(rate: u8) -> Option<u32> {
    PERIODS.get(usize::from(rate).checked_sub(1)?).copied()
}

/// Whether a step at `rate` is due on the output where the shared count
/// is `count`.
pub fn due(rate: u8, count: u64) -> bool {
    let Some(period) = period(rate) else {
        return false;
    };
    let offset = match period >> period.trailing_zeros() {
        1 => 0,
        3 => 496,
        _ => 744,
    };
    (count + offset).is_multiple_of(u64::from(period))
}

/// Where a voice's level is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Attack,
    Decay,
    Sustain,
    Release,
}

/// A voice's level and what it has been handed. Hand it settings, starts
/// and releases before an output, then call [`Envelope::output`] with
/// the shared count for that output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    level: u16,
    part: Part,
    /// `e1`, `e2`.
    settings: [u8; 2],
    gain: u8,
    start_handed: bool,
    release_handed: bool,
    /// The last check took a start.
    started_at_last_check: bool,
    /// The check on the last output took a start.
    took_start: bool,
    /// Outputs of 0 still to come after a start.
    zeros: u8,
}

impl Default for Envelope {
    fn default() -> Envelope {
        Envelope::new()
    }
}

impl Envelope {
    /// A voice that has never started: level 0, in release, settings 0.
    pub fn new() -> Envelope {
        Envelope {
            level: 0,
            part: Part::Release,
            settings: [0, 0],
            gain: 0,
            start_handed: false,
            release_handed: false,
            started_at_last_check: false,
            took_start: false,
            zeros: 0,
        }
    }

    /// The level on the last output.
    pub fn level(&self) -> u16 {
        self.level
    }

    /// Whether the check on the last output took a start.
    pub fn took_start(&self) -> bool {
        self.took_start
    }

    pub fn part(&self) -> Part {
        self.part
    }

    /// Hands the envelope settings `e1`, `e2` and the gain.
    pub fn set(&mut self, settings: [u8; 2], gain: u8) {
        self.settings = settings;
        self.gain = gain;
    }

    /// Hands a start, taking back a release still handed.
    pub fn start(&mut self) {
        self.start_handed = true;
        self.release_handed = false;
    }

    /// Hands a release; it stays handed until the next start.
    pub fn release(&mut self) {
        self.release_handed = true;
    }

    /// A sample without a loop has played to its end.
    pub fn end(&mut self) {
        self.level = 0;
        self.part = Part::Release;
        self.zeros = 0;
    }

    /// Works out the level for the output where the shared count is
    /// `count`, and returns it.
    pub fn output(&mut self, count: u64) -> u16 {
        let mut started = false;
        if count.is_multiple_of(2) {
            if self.start_handed {
                self.start_handed = false;
                // A start at the check right after one that took a start
                // is lost.
                started = !self.started_at_last_check;
            }
            self.started_at_last_check = started;
            if started {
                self.part = Part::Attack;
                self.zeros = START_ZEROS;
            }
            if self.release_handed {
                self.part = Part::Release;
            }
        }
        self.took_start = started;
        if started {
            // The level stays as it was on the check that takes a start.
        } else if self.zeros > 0 {
            self.zeros -= 1;
            self.level = 0;
        } else if self.part == Part::Release {
            self.level = self.level.saturating_sub(RELEASE_STEP);
        } else {
            self.envelope(count);
        }
        self.level
    }

    fn envelope(&mut self, count: u64) {
        let [e1, e2] = self.settings;
        if e1 & 0x80 == 0 {
            self.level = 16 * u16::from(self.gain % 128);
            return;
        }
        let attack = e1 % 16;
        let decay = (e1 / 16) % 8;
        let line = i32::from(e2 / 32);
        let sustain_rate = e2 % 32;
        let level = i32::from(self.level);
        let down = level - 1 - (level - 1).div_euclid(256);
        let (mut next, rate) = match self.part {
            Part::Attack if attack == 15 => (level + 1024, 31),
            Part::Attack => (level + 32, 2 * attack + 1),
            Part::Decay => (down, 2 * decay + 16),
            Part::Sustain => (down, sustain_rate),
            Part::Release => unreachable!("a released voice has no envelope"),
        };
        if self.part == Part::Decay && next.div_euclid(256) == line {
            self.part = Part::Sustain;
        }
        if next > i32::from(MAX_LEVEL) {
            next = i32::from(MAX_LEVEL);
            if self.part == Part::Attack {
                self.part = Part::Decay;
            }
        }
        if due(rate, count) {
            self.level = next as u16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/voices/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// Runs a row of `cases.tsv` and gives its rows as `levels.tsv` lists
    /// them: the first output's level, then every change.
    fn run(case: &str) -> Vec<String> {
        let fields: Vec<&str> = case.split('\t').collect();
        let [name, first, outputs, events] = fields[..] else {
            panic!("{case}");
        };
        let first: u64 = first.parse().unwrap();
        let outputs: u64 = outputs.parse().unwrap();
        let events: Vec<(u64, Vec<&str>)> = events
            .split(';')
            .map(|e| {
                let mut words = e.split_whitespace();
                let k = words.next().unwrap().parse().unwrap();
                (k, words.collect())
            })
            .collect();
        let mut envelope = Envelope::new();
        let mut rows = Vec::new();
        let mut last = None;
        for k in 0..outputs {
            for (_, event) in events.iter().filter(|(at, _)| *at == k) {
                let number = |i: usize| event[i].parse::<u8>().unwrap();
                match event[0] {
                    "set" => envelope.set([number(1), number(2)], number(3)),
                    "start" => envelope.start(),
                    "release" => envelope.release(),
                    other => panic!("unknown event {other}"),
                }
            }
            let level = envelope.output(first + k);
            if last != Some(level) {
                rows.push(format!("{name}\t{k}\t{level}"));
                last = Some(level);
            }
        }
        rows
    }

    #[test]
    fn levels_match_the_spec_vectors() {
        let cases = vector("cases.tsv");
        let mut rows = Vec::new();
        for case in cases.lines().skip(1).filter(|l| !l.is_empty()) {
            rows.extend(run(case));
        }
        let expected: Vec<String> = vector("levels.tsv")
            .lines()
            .skip(1)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        assert_eq!(rows.len(), expected.len());
        for (row, expected) in rows.iter().zip(&expected) {
            assert_eq!(row, expected);
        }
    }

    #[test]
    fn steps_come_on_the_same_outputs_for_every_rate_kind() {
        assert!(due(1, 0) && due(1, 2048) && !due(1, 1024));
        assert!(due(2, 1536 - 496) && !due(2, 0));
        assert!(due(3, 1280 - 744) && !due(3, 0));
        assert!(!due(0, 0));
        assert_eq!(period(31), Some(1));
        assert_eq!(period(32), None);
    }

    #[test]
    fn a_fast_held_note_and_its_release() {
        let mut envelope = Envelope::new();
        // a = 15, d = 0; s = 7, sustain rate 0.
        envelope.set([0x8F, 0xE0], 127);
        envelope.start();
        let levels: Vec<u16> = (0..8).map(|c| envelope.output(c)).collect();
        assert_eq!(levels, [0, 0, 0, 0, 0, 1024, 2047, 2047]);
        envelope.release();
        assert_eq!(envelope.output(8), 2039);
        assert_eq!(envelope.output(9), 2031);
        for c in 10..300 {
            envelope.output(c);
        }
        assert_eq!(envelope.level(), 0);
        assert_eq!(envelope.part(), Part::Release);
    }

    #[test]
    fn a_decay_stops_one_step_above_the_line() {
        let mut envelope = Envelope::new();
        // a = 15, d = 7 (rate 30); s = 0, sustain rate 0.
        envelope.set([0xFF, 0x00], 127);
        envelope.start();
        for c in 0..20_000 {
            envelope.output(c);
        }
        assert_eq!(envelope.part(), Part::Sustain);
        assert_eq!(envelope.level(), 256);
    }
}
