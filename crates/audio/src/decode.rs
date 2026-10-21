//! Decoding through Symphonia, frame-exact.

use std::io::Cursor;
use std::path::Path;

use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Timestamp;

use crate::AudioError;

/// Frames decoded and dropped before the one sought to. Codecs that
/// carry state from packet to packet decode the first packets after a
/// seek wrongly: MP3 frames take bits from the frames before them and
/// overlap with the frame before, and Vorbis packets overlap too. Four
/// MP3 frames covers both.
const PREROLL: u64 = 4 * 1152;

/// Where to read encoded audio from.
#[derive(Clone, Debug)]
pub enum Input {
    /// A file, read whole when it's opened rather than kept open.
    File(std::path::PathBuf),
    /// Encoded bytes, with the file extension as a format hint.
    Bytes(std::sync::Arc<[u8]>, Option<String>),
}

impl Input {
    pub fn file(path: impl AsRef<Path>) -> Input {
        Input::File(path.as_ref().to_path_buf())
    }

    pub fn bytes(bytes: impl Into<std::sync::Arc<[u8]>>, extension: Option<&str>) -> Input {
        Input::Bytes(bytes.into(), extension.map(str::to_string))
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Input::File(p) => p.display().to_string(),
            Input::Bytes(_, Some(ext)) => format!("<{ext} data>"),
            Input::Bytes(_, None) => "<audio data>".into(),
        }
    }
}

/// An open audio track, read as interleaved `f32` frames of one or two
/// channels (extra channels are dropped).
pub(crate) struct Decoding {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    pub rate: u32,
    pub channels: usize,
    /// Tags as (key, value) text, from the container and the track.
    pub tags: Vec<(String, String)>,
    /// Decoded frames not read yet, and the frame number of the first.
    pending: Vec<f32>,
    pending_at: i64,
    pending_read: usize,
    /// Frames before this are dropped (after a seek).
    skip_to: i64,
    scratch: Vec<f32>,
    origin: String,
}

impl Decoding {
    pub fn open(input: &Input) -> Result<Decoding, AudioError> {
        let origin = input.describe();
        let err = |m: String| AudioError(format!("{origin}: {m}"));
        let mut hint = Hint::new();
        let source: Box<dyn MediaSource> = match input {
            Input::File(path) => {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    hint.with_extension(ext);
                }
                // Read whole rather than streamed, so the file isn't held
                // open while it plays: an open file can't be replaced or
                // removed on Windows, as when a pack is imported again.
                let bytes = std::fs::read(path).map_err(|e| err(e.to_string()))?;
                Box::new(Cursor::new(bytes))
            }
            Input::Bytes(bytes, ext) => {
                if let Some(ext) = ext {
                    hint.with_extension(ext);
                }
                Box::new(Cursor::new(bytes.clone()))
            }
        };
        let stream = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
        let mut format = symphonia::default::get_probe()
            .probe(
                &hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| err(e.to_string()))?;

        let track = format
            .first_track_known_codec(TrackType::Audio)
            .ok_or_else(|| err("no audio track".into()))?;
        let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
            return Err(err("no audio track".into()));
        };
        let track_id = track.id;
        let rate = params
            .sample_rate
            .ok_or_else(|| err("unknown sample rate".into()))?;
        // positions are counted in frames, so timestamps must be too
        if let Some(tb) = track.time_base
            && (tb.numer.get() != 1 || tb.denom.get() != rate)
        {
            return Err(err(format!(
                "timestamps in units of {}/{} s, not one frame; use OGG, FLAC, WAV or MP3",
                tb.numer, tb.denom
            )));
        }
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| err(e.to_string()))?;
        let channels = params
            .channels
            .as_ref()
            .map_or(2, |c| c.count())
            .clamp(1, 2);

        let mut tags = Vec::new();
        if let Some(rev) = format.metadata().skip_to_latest() {
            let track_tags = rev
                .per_track
                .iter()
                .filter(|t| t.track_id == u64::from(track_id))
                .flat_map(|t| &t.metadata.tags);
            for tag in rev.media.tags.iter().chain(track_tags) {
                tags.push((tag.raw.key.clone(), tag.raw.value.to_string()));
            }
        }

        Ok(Decoding {
            format,
            decoder,
            track: track_id,
            rate,
            channels,
            tags,
            pending: Vec::new(),
            pending_at: 0,
            pending_read: 0,
            skip_to: 0,
            scratch: Vec::new(),
            origin,
        })
    }

    fn err(&self, m: impl std::fmt::Display) -> AudioError {
        AudioError(format!("{}: {m}", self.origin))
    }

    /// Decodes the next packet into `pending`. False at the end.
    fn fill(&mut self) -> Result<bool, AudioError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(false);
                }
                Err(e) => return Err(self.err(e)),
            };
            if packet.track_id != self.track {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                // a damaged packet is skipped, not fatal
                Err(SymError::DecodeError(_)) => continue,
                Err(e) => return Err(self.err(e)),
            };
            let source_channels = decoded.spec().channels().count().max(1);
            decoded.copy_to_vec_interleaved(&mut self.scratch);
            // first output frame, after any trimmed delay
            let mut at = packet.pts.get() + packet.trim_start.get() as i64;
            self.pending.clear();
            for frame in self.scratch.chunks_exact(source_channels) {
                if at >= self.skip_to {
                    self.pending.extend_from_slice(&frame[..self.channels]);
                }
                at += 1;
            }
            let kept = self.pending.len() / self.channels;
            self.pending_at = at - kept as i64;
            self.pending_read = 0;
            if kept > 0 {
                return Ok(true);
            }
        }
    }

    /// Appends up to `max` frames to `out`; returns how many. Fewer than
    /// `max` only at the end of the track.
    pub fn read(&mut self, out: &mut Vec<f32>, max: usize) -> Result<usize, AudioError> {
        let mut done = 0;
        while done < max {
            let available = self.pending.len() / self.channels - self.pending_read;
            if available == 0 {
                if !self.fill()? {
                    break;
                }
                continue;
            }
            let n = available.min(max - done);
            let start = self.pending_read * self.channels;
            out.extend_from_slice(&self.pending[start..start + n * self.channels]);
            self.pending_read += n;
            done += n;
        }
        Ok(done)
    }

    /// The frame number the next [`read`](Self::read) starts at.
    pub fn position(&self) -> i64 {
        if self.pending_read * self.channels < self.pending.len() {
            self.pending_at + self.pending_read as i64
        } else {
            self.skip_to.max(self.pending_at + self.pending_read as i64)
        }
    }

    /// Moves to an exact frame. Decoding starts [`PREROLL`] frames
    /// earlier, and what's decoded before `frame` is dropped.
    pub fn seek(&mut self, frame: u64) -> Result<(), AudioError> {
        let ts = Timestamp::new(frame.saturating_sub(PREROLL) as i64);
        self.format
            .seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts,
                    track_id: self.track,
                },
            )
            .map_err(|e| self.err(format!("seek to frame {frame}: {e}")))?;
        self.decoder.reset();
        self.pending.clear();
        self.pending_read = 0;
        self.pending_at = frame as i64;
        self.skip_to = frame as i64;
        Ok(())
    }
}
