//! Clamber's placeholder sound, generated from sources.
//!
//! Everything comes from `art/art.toml` in a content folder (normally
//! `content/base`), which lists:
//!
//! - **sounds**: a sequence of synthesized tones, written as WAV
//!   ([`synth`]).
//! - **music**: a looping track of note patterns, written as WAV.
//!
//! Generated files are checked in next to their sources, so the game
//! never needs this crate to run; `cargo run -p clamber-art` rewrites them
//! after a source changes, and a test checks they are up to date. Any
//! output can be replaced by hand-made sound in the same format. The
//! bundled art is pre-rendered from the Blender scripts in `art/` at the
//! repository root instead (`tools/render-art.sh`).

pub mod synth;

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The manifest, relative to the content folder.
pub const MANIFEST: &str = "art/art.toml";

/// A generated file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// Relative to the content folder.
    pub path: PathBuf,
    pub bytes: Vec<u8>,
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
        out.push(Output {
            path: PathBuf::from(&spec.out),
            bytes: synth::sound(spec).map_err(|e| format!("sound {}: {e}", spec.out))?,
        });
    }
    for spec in &manifest.music {
        out.push(Output {
            path: PathBuf::from(&spec.out),
            bytes: synth::music(spec).map_err(|e| format!("music {}: {e}", spec.out))?,
        });
    }
    Ok(out)
}

fn read_string(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}
