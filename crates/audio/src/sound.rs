//! Sound effects, decoded in full at load time, and resampling.

use std::sync::Arc;

use crate::AudioError;
use crate::decode::{Decoding, Input};

/// A fully decoded sound at the mixer's rate, ready to start instantly.
/// Cheap to clone.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// 1 (mono) or 2 (stereo).
    pub channels: usize,
    /// Interleaved samples at the mixer's rate.
    pub samples: Arc<[f32]>,
}

impl Sound {
    /// Decodes a whole file and converts it to `rate`.
    pub fn load(input: &Input, rate: u32) -> Result<Sound, AudioError> {
        let mut d = Decoding::open(input)?;
        let mut raw = Vec::new();
        while d.read(&mut raw, 1 << 16)? > 0 {}
        let mut samples = Vec::new();
        Resampler::new(d.rate, rate, d.channels).process(&raw, &mut samples);
        Ok(Sound {
            channels: d.channels,
            samples: samples.into(),
        })
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
}

/// Linear-interpolation sample-rate conversion over a stream of chunks.
#[derive(Clone, Debug)]
pub(crate) struct Resampler {
    channels: usize,
    /// Input frames per output frame, 32.32 fixed point.
    step: u64,
    /// Position of the next output frame, 32.32, counted from the last
    /// frame of the previous chunk (so 1.0 is the first frame of the next).
    pos: u64,
    last: Vec<f32>,
}

impl Resampler {
    pub fn new(from: u32, to: u32, channels: usize) -> Resampler {
        Resampler {
            channels,
            step: (u64::from(from) << 32) / u64::from(to),
            pos: 1 << 32,
            last: vec![0.0; channels],
        }
    }

    /// Converts the next chunk of interleaved input, appending to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let ch = self.channels;
        let n = input.len() / ch;
        if self.step == 1 << 32 {
            out.extend_from_slice(input);
            return;
        }
        // frame i of the virtual sequence [last, input...]
        let frame = |i: usize, c: usize| {
            if i == 0 {
                self.last[c]
            } else {
                input[(i - 1) * ch + c]
            }
        };
        while ((self.pos >> 32) as usize) < n {
            let i = (self.pos >> 32) as usize;
            let t = (self.pos & 0xffff_ffff) as f32 / 4_294_967_296.0;
            for c in 0..ch {
                let (a, b) = (frame(i, c), frame(i + 1, c));
                out.push(a + (b - a) * t);
            }
            self.pos += self.step;
        }
        if n > 0 {
            self.last.copy_from_slice(&input[(n - 1) * ch..n * ch]);
            self.pos -= (n as u64) << 32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_passes_through() {
        let mut out = Vec::new();
        Resampler::new(48000, 48000, 2).process(&[0.1, 0.2, 0.3, 0.4], &mut out);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn upsampling_interpolates_across_chunks() {
        let mut r = Resampler::new(1, 2, 1);
        let mut out = Vec::new();
        r.process(&[0.0, 1.0], &mut out);
        r.process(&[2.0, 3.0], &mut out);
        assert_eq!(out, [0.0, 0.5, 1.0, 1.5, 2.0, 2.5]);
    }

    #[test]
    fn downsampling_keeps_length_proportional() {
        let input: Vec<f32> = (0..4410).map(|i| i as f32).collect();
        let mut out = Vec::new();
        Resampler::new(44100, 48000, 1).process(&input, &mut out);
        assert!((out.len() as i64 - 4800).abs() <= 1, "{}", out.len());
        // and down
        let mut down = Vec::new();
        Resampler::new(48000, 24000, 1).process(&input, &mut down);
        assert_eq!(down.len(), 2205);
        assert_eq!(down[1], 2.0);
    }
}
