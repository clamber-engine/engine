//! An imported pack's `manifest.toml`.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The manifest's file name, in the pack's folder.
pub const MANIFEST: &str = "manifest.toml";

/// The manifest format this build writes and reads.
pub const FORMAT: u32 = 1;

/// What an imported pack is and where it came from.
///
/// Imported packs are made from the player's own copy of a game, so they
/// are theirs alone: `redistribute` is always `false`, and a manifest
/// saying anything else (or nothing) isn't loaded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    /// The pack's id, which is also its folder's name.
    pub id: String,
    pub name: String,
    /// The title id it was imported from.
    pub title: String,
    /// The release of the title, in words.
    pub release: String,
    /// The Clamber version that imported it.
    pub importer: String,
    pub redistribute: bool,
}

impl Manifest {
    pub fn to_toml(&self) -> String {
        let mut text = String::from(
            "# A personal import, made from the player's own copy of a game.\n\
             # Don't share it.\n",
        );
        text.push_str(&toml::to_string(self).expect("a manifest serializes"));
        text
    }

    pub fn from_toml_str(src: &str) -> Result<Manifest, String> {
        let manifest: Manifest = toml::from_str(src).map_err(|e| e.to_string())?;
        if manifest.format != FORMAT {
            return Err(format!(
                "manifest format {} isn't supported (this build reads {FORMAT})",
                manifest.format
            ));
        }
        if manifest.redistribute {
            return Err("an imported pack must have `redistribute = false`".into());
        }
        Ok(manifest)
    }

    /// Reads a pack's manifest from its folder.
    pub fn load(pack_dir: &Path) -> Result<Manifest, String> {
        let path = pack_dir.join(MANIFEST);
        let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Manifest::from_toml_str(&src).map_err(|e| format!("{}: {}", path.display(), e.trim_end()))
    }

    /// Whether the pack may be exported or shared. Anything that would
    /// copy a pack somewhere else calls this first.
    pub fn ensure_shareable(&self) -> Result<(), NotShareable> {
        if self.redistribute {
            Ok(())
        } else {
            Err(NotShareable(self.id.clone()))
        }
    }
}

/// A personal import can't be exported or shared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotShareable(pub String);

impl fmt::Display for NotShareable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is a personal import made from your own copy of a game; \
             it can't be exported or shared",
            self.0
        )
    }
}

impl std::error::Error for NotShareable {}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            format: FORMAT,
            id: "title-9".into(),
            name: "Imported title-9".into(),
            title: "title-9".into(),
            release: "test release".into(),
            importer: "0.1.0".into(),
            redistribute: false,
        }
    }

    #[test]
    fn round_trips() {
        let m = manifest();
        let text = m.to_toml();
        assert!(text.contains("redistribute = false"), "{text}");
        assert_eq!(Manifest::from_toml_str(&text), Ok(m));
    }

    #[test]
    fn must_not_be_redistributable() {
        let text = manifest().to_toml();
        let shared = text.replace("redistribute = false", "redistribute = true");
        assert!(Manifest::from_toml_str(&shared).is_err());
        let missing = text.replace("redistribute = false", "");
        assert!(Manifest::from_toml_str(&missing).is_err());
    }

    #[test]
    fn refuses_to_share() {
        let err = manifest().ensure_shareable().unwrap_err();
        assert!(err.to_string().contains("can't be exported or shared"));
    }
}
