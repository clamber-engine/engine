//! The echo (`spec/formats/echo.md`): a memory that gives back what the
//! voices with echo on played a fixed time before, filtered, fed back
//! into the memory and added to the [`Mixer`](crate::voice_output::Mixer)'s
//! output at the echo volume.

/// Entries in the memory for each step of the echo delay.
const ENTRIES_PER_DELAY: usize = 512;

/// Values the filter works over.
pub const FILTER_SIZE: usize = 8;

fn limit(n: i32) -> i32 {
    n.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}

/// Brought into −32 768 to 32 767 by adding or taking off 65 536.
fn wrap(n: i32) -> i32 {
    i32::from(n as i16)
}

/// Rounds to even: takes 1 off an odd number.
fn even(n: i32) -> i32 {
    n & !1
}

/// A byte read as signed.
fn signed(byte: u8) -> i32 {
    i32::from(byte as i8)
}

/// The echo memory and what it has been handed.
#[derive(Clone, Debug)]
pub struct Echo {
    /// Left and right, as bytes.
    pub volume: [u8; 2],
    /// `b`, as a byte.
    pub feedback: u8,
    /// `c0` to `c7`, as bytes.
    pub filter: [u8; FILTER_SIZE],
    pub writing: bool,
    /// Left and right values of each entry.
    memory: Vec<[i16; 2]>,
    position: usize,
    /// `h0` to `h7` for left and for right.
    history: [[i32; FILTER_SIZE]; 2],
}

impl Echo {
    /// An echo with delay `delay` (0 to 15) and its memory all 0, with
    /// volume, feedback and filter values 0 and writing off.
    pub fn new(delay: u8) -> Echo {
        let entries = (ENTRIES_PER_DELAY * usize::from(delay)).max(1);
        Echo {
            volume: [0, 0],
            feedback: 0,
            filter: [0; FILTER_SIZE],
            writing: false,
            memory: vec![[0, 0]; entries],
            position: 0,
            history: [[0; FILTER_SIZE]; 2],
        }
    }

    /// Whether the echo adds nothing now or later: its volume is 0 or
    /// there's nothing in the memory or the history to give back.
    pub fn is_silent(&self) -> bool {
        self.volume == [0, 0]
            || (self.memory.iter().all(|&e| e == [0, 0])
                && self.history.iter().flatten().all(|&h| h == 0))
    }

    /// Takes the entry at the position into the history, writes `input`
    /// (`E`, left and right) and the feedback there, and moves on. Gives
    /// the echo parts to add to the main output.
    pub fn output(&mut self, input: [i32; 2]) -> [i32; 2] {
        let entry = &mut self.memory[self.position];
        let parts = std::array::from_fn(|side| {
            let history = &mut self.history[side];
            history.rotate_left(1);
            history[FILTER_SIZE - 1] = i32::from(entry[side]) >> 1;
            let p: [i32; FILTER_SIZE] =
                std::array::from_fn(|j| (history[j] * signed(self.filter[j])) >> 6);
            let first: i32 = p[..FILTER_SIZE - 1].iter().sum();
            let f = even(limit(wrap(first) + wrap(p[FILTER_SIZE - 1])));
            if self.writing {
                let fed = wrap((f * signed(self.feedback)) >> 7);
                entry[side] = even(limit(input[side] + fed)) as i16;
            }
            wrap((f * signed(self.volume[side])) >> 7)
        });
        self.position = (self.position + 1) % self.memory.len();
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::{BLOCK_SIZE, Sample};
    use crate::voice_output::Mixer;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/echo/{name}",
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
        let [name, first, delay, outputs, samples, events] = fields[..] else {
            panic!("{case}");
        };
        let samples: Vec<Sample> = samples
            .split(';')
            .filter(|s| !s.is_empty())
            .map(sample)
            .collect();
        let events: Vec<(u64, Vec<&str>)> = events
            .split(';')
            .map(|e| {
                let mut words = e.split_whitespace();
                let k = words.next().unwrap().parse().unwrap();
                (k, words.collect())
            })
            .collect();
        let mut mixer = Mixer::new(first.parse().unwrap(), delay.parse().unwrap());
        let mut rows = Vec::new();
        for k in 0..outputs.parse().unwrap() {
            for (_, event) in events.iter().filter(|(at, _)| *at == k) {
                let number = |i: usize| event[i].parse::<u16>().unwrap();
                let byte = |i: usize| event[i].parse::<u8>().unwrap();
                let echo = &mut mixer.echo;
                match event[..] {
                    ["main", ..] => mixer.main_volume = [byte(1), byte(2)],
                    ["echo", "vol", ..] => echo.volume = [byte(2), byte(3)],
                    ["echo", "feedback", _] => echo.feedback = byte(2),
                    ["echo", "filter", ..] => echo.filter = std::array::from_fn(|j| byte(2 + j)),
                    ["echo", "writing", on] => echo.writing = on == "on",
                    ["echo", "entry", i, l, r] => {
                        echo.memory[i.parse::<usize>().unwrap()] =
                            [l.parse().unwrap(), r.parse().unwrap()]
                    }
                    [voice, ..] => {
                        let n: usize = voice.strip_prefix('v').unwrap().parse().unwrap();
                        let voice = &mut mixer.voices[n];
                        match event[1] {
                            "pitch" => voice.pitch = number(2),
                            "vol" => voice.volume = [byte(2), byte(3)],
                            "sample" => voice.sample = byte(2),
                            "env" => voice.envelope.set([byte(2), byte(3)], byte(4)),
                            "start" => voice.envelope.start(),
                            "release" => voice.envelope.release(),
                            "echo" => voice.echo = event[2] == "on",
                            other => panic!("unknown event {other}"),
                        }
                    }
                    [] => panic!("empty event"),
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
    fn the_worked_example() {
        // voice_output.rs's worked example with echo on, delay 1.
        let block = [0xC3, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11];
        let samples = [Sample {
            loop_offset: 0,
            data: block.to_vec(),
        }];
        let mut mixer = Mixer::new(3, 1);
        mixer.main_volume = [127, 127];
        mixer.echo.filter = [127, 0, 0, 0, 0, 0, 0, 0];
        mixer.echo.volume = [64, 128];
        mixer.echo.writing = true;
        let voice = &mut mixer.voices[0];
        voice.pitch = 4096;
        voice.volume = [127, 127];
        voice.envelope.set([143, 224], 0);
        voice.echo = true;
        let mut outputs = Vec::new();
        for k in 0..600 {
            if k == 4 {
                mixer.voices[0].envelope.start();
            }
            outputs.push(mixer.output(&samples));
        }
        assert_eq!(outputs[12], [2014, 2014]);
        assert!(outputs[13..=530].iter().all(|&o| o == [4028, 4028]));
        assert_eq!(outputs[531], [5035, 2014]);
        assert!(outputs[532..].iter().all(|&o| o == [6042, 0]));
    }

    #[test]
    fn delay_zero_has_one_entry() {
        assert_eq!(Echo::new(0).memory.len(), 1);
        assert_eq!(Echo::new(4).memory.len(), 2048);
    }
}
