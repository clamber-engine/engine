//! Audio for the Clamber engine.
//!
//! Files are decoded with Symphonia: OGG Vorbis, FLAC, WAV and MP3.
//!
//! - **Sound effects** ([`Sound`]) are decoded in full at load time, at the
//!   output's rate, so they start the moment they're played.
//! - **Music** ([`Music`]) streams from a decoding thread that runs a
//!   little ahead. Loops are sample-accurate, from `LOOPSTART` and
//!   `LOOPLENGTH` tags, or for formats without tags from [`LoopPoints`] in
//!   content metadata.
//! - The [`Mixer`] plays both, with per-[`Category`] volumes, a voice
//!   limit, and crossfades between tracks. The game controls it through an
//!   [`AudioHandle`]; a track that's already playing carries on when asked
//!   for again, so music continues across level sections.
//! - With the `output` feature, [`Output`] plays the mixer on the default
//!   sound device and reopens it when the device goes away.

mod decode;
mod mixer;
mod music;
#[cfg(feature = "output")]
mod output;
mod sound;

use std::fmt;

pub use decode::Input;
pub use mixer::{AudioHandle, Category, Mixer, Play, VoiceId, mixer};
pub use music::{LoopPoints, Looping, Music};
#[cfg(feature = "output")]
pub use output::Output;
pub use sound::Sound;

/// Audio that couldn't be loaded or played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioError(pub String);

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AudioError {}
