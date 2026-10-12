//! Choices the player makes in the game's menus, other than controls,
//! kept from one run to the next in `prefs.toml` ([`prefs_path`]).
//!
//! [`prefs_path`]: crate::prefs_path

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::report;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    /// The movement preset picked on the movement screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// How the game is shown, from the options screen and the window.
    #[serde(default)]
    pub display: Display,
}

/// The window's settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub fullscreen: bool,
    /// Wait for the display's refresh to show each frame, so it doesn't
    /// tear. Without it, frames are capped at [`FRAME_CAP`] a second.
    ///
    /// [`FRAME_CAP`]: crate::FRAME_CAP
    pub vsync: bool,
    /// The window's size when it isn't fullscreen, in physical pixels,
    /// as the player last left it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<[u32; 2]>,
}

impl Default for Display {
    fn default() -> Display {
        Display {
            fullscreen: false,
            vsync: true,
            window: None,
        }
    }
}

impl Prefs {
    /// The prefs in `path`: none when there's no such file, and none,
    /// with a warning, when it can't be read.
    pub fn load(path: &Path) -> Prefs {
        let src = match std::fs::read_to_string(path) {
            Ok(src) => src,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Prefs::default(),
            Err(e) => {
                report::warn(format!("{}: {e}; ignoring it", path.display()));
                return Prefs::default();
            }
        };
        Prefs::from_toml_str(&src).unwrap_or_else(|e| {
            report::warn(format!("{}: {e}; ignoring it", path.display()));
            Prefs::default()
        })
    }

    pub fn from_toml_str(src: &str) -> Result<Prefs, String> {
        toml::from_str(src).map_err(|e| e.message().to_string())
    }

    pub fn to_toml_string(&self) -> String {
        let mut out = String::from("# Clamber's menu choices, written by the game.\n");
        out += &toml::to_string(self).expect("prefs are plain values");
        out
    }

    /// Writes the prefs to `path`, creating its folder.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let shown = path.display();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{shown}: {e}"))?;
        }
        std::fs::write(path, self.to_toml_string()).map_err(|e| format!("{shown}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for prefs in [
            Prefs::default(),
            Prefs {
                preset: Some("snappy".into()),
                display: Display::default(),
            },
            Prefs {
                preset: None,
                display: Display {
                    fullscreen: true,
                    vsync: false,
                    window: Some([1600, 900]),
                },
            },
        ] {
            assert_eq!(Prefs::from_toml_str(&prefs.to_toml_string()), Ok(prefs));
        }
        assert!(Prefs::from_toml_str("preset = 3").is_err());
    }

    #[test]
    fn vsync_is_on_unless_turned_off() {
        assert!(Prefs::default().display.vsync);
        let prefs = Prefs::from_toml_str("preset = \"snappy\"").unwrap();
        assert!(prefs.display.vsync, "prefs from before there were options");
        let prefs = Prefs::from_toml_str("[display]\nfullscreen = true").unwrap();
        assert!(prefs.display.fullscreen && prefs.display.vsync);
    }

    #[test]
    fn saves_and_loads_a_file() {
        let dir = std::env::temp_dir().join(format!("clamber-prefs-{}", std::process::id()));
        let path = dir.join("sub").join("prefs.toml");
        assert_eq!(Prefs::load(&path), Prefs::default(), "no file yet");
        let prefs = Prefs {
            preset: Some("floaty".into()),
            ..Prefs::default()
        };
        prefs.save(&path).unwrap();
        assert_eq!(Prefs::load(&path), prefs);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
