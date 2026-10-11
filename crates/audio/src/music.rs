//! Streamed music with sample-accurate loops.

use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

use crate::AudioError;
use crate::decode::{Decoding, Input};
use crate::sound::Resampler;

/// Frames per chunk sent from the decoding thread.
const CHUNK: usize = 2048;
/// Chunks decoded ahead of playback.
const AHEAD: usize = 8;

/// How a track loops. Positions are frames at the file's own rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Looping {
    /// Play once and stop.
    Once,
    /// Play to the end, then continue from `start`, forever. With a
    /// `length`, the loop ends at `start + length` instead of the end of
    /// the file.
    From { start: u64, length: Option<u64> },
}

impl Looping {
    /// Loop points from `LOOPSTART` and `LOOPLENGTH` tags (any case), if
    /// there's a `LOOPSTART`. Values are frame counts.
    pub fn from_tags<'a>(
        tags: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Option<Looping>, String> {
        let (mut start, mut length) = (None, None);
        for (key, value) in tags {
            let slot = match key.to_ascii_uppercase().as_str() {
                "LOOPSTART" => &mut start,
                "LOOPLENGTH" => &mut length,
                _ => continue,
            };
            let n = value
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("{key} `{value}` isn't a frame count"))?;
            *slot = Some(n);
        }
        match (start, length) {
            (Some(start), length) => Ok(Some(Looping::From { start, length })),
            (None, Some(_)) => Err("LOOPLENGTH without LOOPSTART".into()),
            (None, None) => Ok(None),
        }
    }
}

/// Loop points as written in content metadata, for formats without loop
/// tags: `loop_start` and an optional `loop_length`, in frames at the
/// file's rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoopPoints {
    pub loop_start: u64,
    pub loop_length: Option<u64>,
}

impl From<LoopPoints> for Looping {
    fn from(p: LoopPoints) -> Looping {
        Looping::From {
            start: p.loop_start,
            length: p.loop_length,
        }
    }
}

/// A music track decoding on its own thread, a little ahead of playback.
/// Output is stereo at the mixer's rate.
#[derive(Debug)]
pub struct Music {
    chunks: Receiver<Vec<f32>>,
    chunk: Vec<f32>,
    read: usize,
    finished: bool,
    /// Times the mixer asked for audio that wasn't decoded yet.
    pub underruns: u64,
}

impl Music {
    /// Opens a track and starts decoding it. Looping comes from `looping`
    /// when given (for formats without tags), otherwise from the file's
    /// `LOOPSTART`/`LOOPLENGTH` tags, otherwise the whole track loops.
    pub fn open(input: Input, looping: Option<Looping>, rate: u32) -> Result<Music, AudioError> {
        let (opened_tx, opened_rx) = sync_channel(1);
        let (tx, rx) = sync_channel(AHEAD);
        std::thread::Builder::new()
            .name("music decode".into())
            .spawn(move || decode_thread(input, looping, rate, opened_tx, tx))
            .map_err(|e| AudioError(format!("can't start music thread: {e}")))?;
        opened_rx
            .recv()
            .map_err(|_| AudioError("music thread stopped".into()))??;
        Ok(Music {
            chunks: rx,
            chunk: Vec::new(),
            read: 0,
            finished: false,
            underruns: 0,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Fills `out` (stereo) with the next samples, without waiting; what
    /// isn't decoded yet is left silent. Returns false once the track ended.
    pub(crate) fn fill(&mut self, out: &mut [f32]) -> bool {
        self.fill_with(out, |rx| match rx.try_recv() {
            Ok(c) => Some(Some(c)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(None),
        })
    }

    /// Fills `out` (stereo), waiting for the decoder as needed. For
    /// offline rendering and tests.
    pub fn fill_blocking(&mut self, out: &mut [f32]) -> bool {
        self.fill_with(out, |rx| Some(rx.recv().ok()))
    }

    /// `next` returns `None` when nothing is ready, `Some(None)` at the end.
    fn fill_with(
        &mut self,
        out: &mut [f32],
        mut next: impl FnMut(&Receiver<Vec<f32>>) -> Option<Option<Vec<f32>>>,
    ) -> bool {
        let mut done = 0;
        while done < out.len() && !self.finished {
            if self.read == self.chunk.len() {
                match next(&self.chunks) {
                    Some(Some(c)) => {
                        self.chunk = c;
                        self.read = 0;
                    }
                    Some(None) => self.finished = true,
                    None => {
                        self.underruns += 1;
                        break;
                    }
                }
                continue;
            }
            let n = (self.chunk.len() - self.read).min(out.len() - done);
            out[done..done + n].copy_from_slice(&self.chunk[self.read..self.read + n]);
            self.read += n;
            done += n;
        }
        out[done..].fill(0.0);
        !self.finished
    }
}

fn decode_thread(
    input: Input,
    looping: Option<Looping>,
    rate: u32,
    opened: SyncSender<Result<(), AudioError>>,
    chunks: SyncSender<Vec<f32>>,
) {
    let setup = Decoding::open(&input).and_then(|d| {
        let tags = d.tags.iter().map(|(k, v)| (k.as_str(), v.as_str()));
        let from_tags = Looping::from_tags(tags)
            .map_err(|m| AudioError(format!("{}: {m}", input.describe())))?;
        let looping = looping.or(from_tags).unwrap_or(Looping::From {
            start: 0,
            length: None,
        });
        Ok((d, looping))
    });
    let (mut d, looping) = match setup {
        Ok(ok) => {
            let _ = opened.send(Ok(()));
            ok
        }
        Err(e) => {
            let _ = opened.send(Err(e));
            return;
        }
    };
    let mut resampler = Resampler::new(d.rate, rate, 2);
    let mut raw = Vec::with_capacity(CHUNK * d.channels);
    let mut stereo = Vec::with_capacity(CHUNK * 2);
    let mut empty_passes = 0;
    loop {
        let end = match looping {
            Looping::From {
                start,
                length: Some(len),
            } => Some(start + len),
            _ => None,
        };
        let want = end.map_or(CHUNK, |e| {
            (e.saturating_sub(d.position().max(0) as u64) as usize).min(CHUNK)
        });
        raw.clear();
        let got = match d.read(&mut raw, want) {
            Ok(n) => n,
            Err(_) => return,
        };
        if got > 0 {
            empty_passes = 0;
            stereo.clear();
            if d.channels == 1 {
                stereo.extend(raw.iter().flat_map(|&s| [s, s]));
            } else {
                stereo.extend_from_slice(&raw);
            }
            let mut out = Vec::with_capacity(stereo.len() * 2);
            resampler.process(&stereo, &mut out);
            if chunks.send(out).is_err() {
                return; // playback stopped
            }
        }
        if got < want || (got == 0 && want == 0) {
            // end of the file, or of the loop
            let Looping::From { start, .. } = looping else {
                return;
            };
            empty_passes += 1;
            if empty_passes > 1 || d.seek(start).is_err() {
                return; // nothing to play between the loop points
            }
        }
    }
}
