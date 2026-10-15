//! Choices the player makes in the game's menus, other than controls,
//! kept from one run to the next in `prefs.toml` ([`prefs_path`]).
//!
//! [`prefs_path`]: crate::prefs_path

use std::path::{Path, PathBuf};

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
    /// How loud the game plays, from the options screen.
    #[serde(default)]
    pub volume: Volume,
    /// The folder a game was last imported from, to start choosing the
    /// next one in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_folder: Option<PathBuf>,
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

/// Volumes, as percentages of full, in steps of [`Volume::STEP`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Volume {
    pub music: u8,
    pub effects: u8,
}

impl Default for Volume {
    fn default() -> Volume {
        Volume {
            music: 100,
            effects: 100,
        }
    }
}

impl Volume {
    pub const STEP: u8 = 10;

    /// The linear gain for a percentage. Gain goes with its square, so
    /// that each step sounds about as big as the last.
    pub fn gain(percent: u8) -> f32 {
        (f32::from(percent.min(100)) / 100.0).powi(2)
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
                ..Prefs::default()
            },
            Prefs {
                preset: None,
                display: Display {
                    fullscreen: true,
                    vsync: false,
                    window: Some([1600, 900]),
                },
                volume: Volume {
                    music: 30,
                    effects: 0,
                },
                import_folder: Some(PathBuf::from("games")),
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
    fn volumes_are_full_unless_turned_down() {
        assert_eq!(Prefs::default().volume, Volume::default());
        let prefs = Prefs::from_toml_str("[volume]\nmusic = 40").unwrap();
        assert_eq!((prefs.volume.music, prefs.volume.effects), (40, 100));
        assert_eq!(Volume::gain(100), 1.0);
        assert_eq!(Volume::gain(50), 0.25);
        assert_eq!(Volume::gain(0), 0.0);
        assert_eq!(Volume::gain(200), 1.0);
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
