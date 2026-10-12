//! Clamber's placeholder art and sound, generated from sources.
//!
//! Everything comes from `art/art.toml` in a content folder (normally
//! `content/base`), which lists:
//!
//! - **sheets**: an SVG drawn as a grid of equal frames, left to right
//!   and top to bottom, rendered to a PNG sprite sheet with Aseprite JSON
//!   metadata in the `clamber-sprite` format ([`sheet`]).
//! - **images**: an SVG rendered to a PNG as it is, for backgrounds and
//!   tilesets.
//! - **sounds**: a sequence of synthesized tones, written as WAV
//!   ([`synth`]).
//! - **music**: a looping track of note patterns, written as WAV.
//!
//! Generated files are checked in next to their sources, so the game
//! never needs this crate to run; `cargo run -p clamber-art` rewrites them
//! after a source changes, and a test checks they are up to date. Any
//! output can be replaced by hand-made art in the same format.

pub mod sheet;
pub mod svg;
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
    pub sheets: Vec<sheet::SheetSpec>,
    #[serde(default)]
    pub images: Vec<ImageSpec>,
    #[serde(default)]
    pub sounds: Vec<synth::SoundSpec>,
    #[serde(default)]
    pub music: Vec<synth::MusicSpec>,
}

/// An SVG rendered to a PNG as it is.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageSpec {
    /// The SVG, relative to the manifest.
    pub source: String,
    /// The PNG, relative to the content folder.
    pub out: String,
}

/// Generates every output the manifest in `content` lists.
pub fn generate(content: &Path) -> Result<Vec<Output>, String> {
    let manifest_path = content.join(MANIFEST);
    let src = read_string(&manifest_path)?;
    let manifest: Manifest =
        toml::from_str(&src).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let art = manifest_path.parent().unwrap_or(Path::new(""));
    let svg = |source: &str| -> Result<(String, String), String> {
        let path = art.join(source);
        Ok((read_string(&path)?, path.display().to_string()))
    };

    let mut out = Vec::new();
    for spec in &manifest.sheets {
        let (src, origin) = svg(&spec.source)?;
        let image = svg::render(&src).map_err(|e| format!("{origin}: {e}"))?;
        out.extend(sheet::build(spec, &image).map_err(|e| format!("{origin}: {e}"))?);
    }
    for spec in &manifest.images {
        let (src, origin) = svg(&spec.source)?;
        let image = svg::render(&src).map_err(|e| format!("{origin}: {e}"))?;
        out.push(Output {
            path: PathBuf::from(&spec.out),
            bytes: clamber_sprite::encode_png(&image),
        });
    }
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
