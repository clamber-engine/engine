//! Clamber's placeholder sound, generated from sources.
//!
//! Everything comes from `art/art.toml` in a content folder (normally
//! `content/base`), which lists:
//!
//! - **sounds**: a sequence of synthesized tones ([`synth`]).
//! - **music**: a looping track of note patterns.
//!
//! Each is written as Ogg Vorbis or WAV, by its file's extension.
//!
//! Generated files are checked in next to their sources, so the game
//! never needs this crate to run; `cargo run -p clamber-art` rewrites them
//! after a source changes, and a test checks they are up to date. Ogg
//! files carry a hash of the samples they were encoded from in a
//! [`SOURCE_TAG`] comment, and are up to date when it matches: the
//! encoder's output can differ by a bit from one build to another. Any
//! output can be replaced by hand-made sound in the same format. The
//! bundled art is pre-rendered from the Blender scripts in `art/` at the
//! repository root instead (`tools/render-art.sh`).

pub mod synth;

use std::path::{Path, PathBuf};

use clamber_audio::encode::Vorbis;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The manifest, relative to the content folder.
pub const MANIFEST: &str = "art/art.toml";

/// The Vorbis comment holding the SHA-256 of the samples an Ogg file
/// was encoded from, as hex.
pub const SOURCE_TAG: &str = "CLAMBER_SOURCE";

/// A generated file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// Relative to the content folder.
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    /// For an Ogg file, the [`SOURCE_TAG`] comment in it, as
    /// `CLAMBER_SOURCE=<hash>`.
    pub source: Option<String>,
}

impl Output {
    /// The file for `samples` at `path`: Ogg Vorbis or WAV by its
    /// extension.
    pub fn encode(path: &str, samples: &[i16]) -> Result<Output, String> {
        let (bytes, source) = match Path::new(path).extension().and_then(|e| e.to_str()) {
            Some("wav") => (synth::wav(samples), None),
            Some("ogg") => {
                let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
                let hash: String = Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                let vorbis = Vorbis {
                    tags: vec![(SOURCE_TAG.into(), hash.clone())],
                    ..Vorbis::default()
                };
                let ogg = vorbis
                    .encode(samples, 1, synth::RATE)
                    .map_err(|e| e.to_string())?;
                (ogg, Some(format!("{SOURCE_TAG}={hash}")))
            }
            _ => return Err("the file must end in .ogg or .wav".into()),
        };
        Ok(Output {
            path: PathBuf::from(path),
            bytes,
            source,
        })
    }

    /// Whether `existing`, the file at [`path`](Output::path), is up to
    /// date: for Ogg, encoded from the same samples; otherwise the same
    /// bytes.
    pub fn is_current(&self, existing: &[u8]) -> bool {
        match &self.source {
            Some(tag) => existing.windows(tag.len()).any(|w| w == tag.as_bytes()),
            None => existing == self.bytes,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default)]
    pub sounds: Vec<synth::SoundSpec>,
    #[serde(default)]
    pub music: Vec<synth::MusicSpec>,
}

/// Generates every output the manifest in `content` lists.
pub fn generate(content: &Path) -> Result<Vec<Output>, String> {
    let manifest_path = content.join(MANIFEST);
    let src = read_string(&manifest_path)?;
    let manifest: Manifest =
        toml::from_str(&src).map_err(|e| format!("{}: {e}", manifest_path.display()))?;

    let mut out = Vec::new();
    for spec in &manifest.sounds {
        let samples = synth::sound(spec).map_err(|e| format!("sound {}: {e}", spec.out))?;
        out.push(
            Output::encode(&spec.out, &samples).map_err(|e| format!("sound {}: {e}", spec.out))?,
        );
    }
    for spec in &manifest.music {
        let samples = synth::music(spec).map_err(|e| format!("music {}: {e}", spec.out))?;
        out.push(
            Output::encode(&spec.out, &samples).map_err(|e| format!("music {}: {e}", spec.out))?,
        );
    }
    Ok(out)
}

fn read_string(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ogg_is_current_while_its_samples_are_the_same() {
        let out = Output::encode("s.ogg", &[0, 100, 200, 100]).unwrap();
        assert!(out.is_current(&out.bytes));
        let changed = Output::encode("s.ogg", &[0, 100, 201, 100]).unwrap();
        assert!(!changed.is_current(&out.bytes));
        assert!(!out.is_current(b"not an ogg"));
    }

    #[test]
    fn a_wav_is_current_while_its_bytes_are_the_same() {
        let out = Output::encode("s.wav", &[1, 2]).unwrap();
        assert_eq!(out.source, None);
        assert!(out.is_current(&synth::wav(&[1, 2])));
        assert!(!out.is_current(&synth::wav(&[1, 3])));
    }

    #[test]
    fn other_extensions_are_refused() {
        assert!(Output::encode("s.mp3", &[0]).unwrap_err().contains(".ogg"));
    }
}
