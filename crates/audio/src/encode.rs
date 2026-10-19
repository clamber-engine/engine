//! Encoding to Ogg Vorbis, for the importer and the generated content.
//!
//! The encoded file decodes to exactly as many frames as went in, with
//! the first frame first: the Ogg granule positions trim the encoder's
//! padding, so loop points counted in frames of the source are frames of
//! the file too.

use std::num::{NonZeroU8, NonZeroU32};

use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

use crate::AudioError;

/// The quality the importer and the generated content use unless told
/// otherwise, on Vorbis's -1 to 10 scale: transparent for game music
/// and effects.
pub const DEFAULT_QUALITY: f32 = 6.0;

/// Frames handed to the encoder at a time.
const BLOCK: usize = 1024;

/// How to encode.
#[derive(Clone, Debug, PartialEq)]
pub struct Vorbis {
    /// Vorbis quality, -1 (smallest) to 10 (best).
    pub quality: f32,
    /// Comment tags, as (key, value).
    pub tags: Vec<(String, String)>,
}

impl Default for Vorbis {
    fn default() -> Vorbis {
        Vorbis {
            quality: DEFAULT_QUALITY,
            tags: Vec::new(),
        }
    }
}

impl Vorbis {
    /// Encodes interleaved 16-bit samples of `channels` channels at
    /// `rate` frames a second as an Ogg Vorbis file.
    pub fn encode(&self, samples: &[i16], channels: u8, rate: u32) -> Result<Vec<u8>, AudioError> {
        let err = |e: vorbis_rs::VorbisError| AudioError(format!("Vorbis encoding: {e}"));
        if !(-1.0..=10.0).contains(&self.quality) {
            return Err(AudioError(format!(
                "Vorbis quality {} isn't between -1 and 10",
                self.quality
            )));
        }
        let (Some(ch), Some(r)) = (NonZeroU8::new(channels), NonZeroU32::new(rate)) else {
            return Err(AudioError("no channels or no rate to encode".into()));
        };
        let mut out = Vec::new();
        // A fixed serial, so the same audio encodes to the same file.
        let mut builder = VorbisEncoderBuilder::new_with_serial(r, ch, &mut out, 0);
        builder.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: self.quality / 10.0,
        });
        for (key, value) in &self.tags {
            builder
                .comment_tag(key.as_str(), value.as_str())
                .map_err(err)?;
        }
        let mut encoder = builder.build().map_err(err)?;
        let ch = usize::from(channels);
        let mut planar = vec![Vec::with_capacity(BLOCK); ch];
        for block in samples.chunks(BLOCK * ch) {
            for p in &mut planar {
                p.clear();
            }
            for frame in block.chunks_exact(ch) {
                for (p, &s) in planar.iter_mut().zip(frame) {
                    p.push(f32::from(s) / 32768.0);
                }
            }
            encoder.encode_audio_block(&planar).map_err(err)?;
        }
        encoder.finish().map_err(err)?;
        Ok(out)
    }
}
