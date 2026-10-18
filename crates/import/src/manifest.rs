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
    /// Whether the pack is pixel art, to be drawn at its view's size and
    /// scaled up by whole numbers with nearest-neighbour filtering.
    #[serde(default)]
    pub pixel_art: bool,
    /// The view's size in pixels, `[width, height]`: how much of a level
    /// is seen at once. Packs without one get the engine's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<[u32; 2]>,
    /// How the pack plays in a window wider than its view, unless the
    /// player picks otherwise. Packs without one get
    /// [`Widescreen::OriginalActivation`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widescreen: Option<Widescreen>,
}

/// How a game whose movement has a view of its own (a preset with
/// `view_width` above 0) plays in a window wider than that view. Nothing
/// may pop into sight in any of them: whatever is inside the window is
/// already there, or comes in from outside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Widescreen {
    /// Things are placed and drawn as soon as they're inside the wide
    /// window, but only move, act and collide once they're inside the
    /// original view, so play goes as it would there.
    #[default]
    OriginalActivation,
    /// Things appear and act by the whole wide window, so they come in
    /// from outside it. Play goes differently from the original view.
    WideActivation,
    /// Only the original view's width is shown, with borders at the
    /// sides. Play goes as it would there.
    OriginalFraming,
}

impl Widescreen {
    pub const ALL: [Widescreen; 3] = [
        Widescreen::OriginalActivation,
        Widescreen::WideActivation,
        Widescreen::OriginalFraming,
    ];

    /// Its name, as the options screen shows it.
    pub fn label(self) -> &'static str {
        match self {
            Widescreen::OriginalActivation => "Original activation",
            Widescreen::WideActivation => "Wide activation",
            Widescreen::OriginalFraming => "Original framing",
        }
    }

    /// The mode after this one in [`ALL`](Widescreen::ALL), wrapping
    /// around, or before it with `back`.
    pub fn next(self, back: bool) -> Widescreen {
        let all = Widescreen::ALL;
        let i = all.iter().position(|&m| m == self).expect("listed");
        let step = if back { all.len() - 1 } else { 1 };
        all[(i + step) % all.len()]
    }
}

/// The largest view, in pixels each way, a manifest can ask for.
pub const MAX_VIEW: u32 = 4096;

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
        let sizes = manifest.view.unwrap_or([1, 1]);
        if !sizes.iter().all(|n| (1..=MAX_VIEW).contains(n)) {
            return Err(format!(
                "the view must be from 1 to {MAX_VIEW} pixels each way"
            ));
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
            pixel_art: true,
            view: Some([320, 200]),
            widescreen: Some(Widescreen::OriginalFraming),
        }
    }

    #[test]
    fn round_trips() {
        let m = manifest();
        let text = m.to_toml();
        assert!(text.contains("redistribute = false"), "{text}");
        assert!(text.contains("pixel_art = true"), "{text}");
        assert!(text.contains("view = [320, 200]"), "{text}");
        assert!(text.contains("widescreen = \"original-framing\""), "{text}");
        assert_eq!(Manifest::from_toml_str(&text), Ok(m));
    }

    #[test]
    fn how_to_draw_it_is_optional() {
        let text = manifest().to_toml();
        let plain = text
            .replace("pixel_art = true", "")
            .replace("view = [320, 200]", "")
            .replace("widescreen = \"original-framing\"", "");
        let m = Manifest::from_toml_str(&plain).unwrap();
        assert!(!m.pixel_art);
        assert_eq!(m.view, None);
        assert_eq!(m.widescreen, None);
    }

    #[test]
    fn the_view_has_a_size() {
        let text = manifest().to_toml();
        for view in ["[0, 200]", "[320, 0]", "[320, 5000]"] {
            let bad = text.replace("[320, 200]", view);
            assert!(Manifest::from_toml_str(&bad).is_err(), "{view}");
        }
    }

    #[test]
    fn widescreen_modes_go_round() {
        for mode in Widescreen::ALL {
            assert_eq!(mode.next(false).next(true), mode);
        }
        assert_eq!(
            Widescreen::OriginalFraming.next(false),
            Widescreen::OriginalActivation
        );
        let bad = manifest()
            .to_toml()
            .replace("original-framing", "stretched");
        assert!(Manifest::from_toml_str(&bad).is_err());
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
