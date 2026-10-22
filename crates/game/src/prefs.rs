//! Choices the player makes in the game's menus, other than controls,
//! kept from one run to the next in `prefs.toml` ([`prefs_path`]).
//!
//! [`prefs_path`]: crate::prefs_path

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clamber_import::Widescreen;
use serde::{Deserialize, Serialize};

use crate::display::Rendering;
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
    /// The widescreen mode picked on the options screen for each game,
    /// by its pack's id, or [`BUNDLED`] for the bundled game. Games
    /// without one play in their own default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub widescreen: BTreeMap<String, Widescreen>,
    /// The rendering mode picked on the options screen for each imported
    /// pixel-art game, by its pack's id. Games without one are drawn in
    /// [`Rendering::Original`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rendering: BTreeMap<String, Rendering>,
}

/// What [`Prefs::widescreen`] calls the bundled game.
pub const BUNDLED: &str = "clamber";

/// The window's settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, from = "DisplayFile")]
pub struct Display {
    pub fullscreen: bool,
    /// Each frame waits for the display's next refresh, so it doesn't
    /// tear. Without it, frames are capped at [`FRAME_CAP`] a second.
    ///
    /// [`FRAME_CAP`]: crate::display::FRAME_CAP
    pub vsync: bool,
    /// Frames are shown as soon as they're drawn, without tearing, on a
    /// display that varies its refresh to match. Off unless the player
    /// turns it on, as it judders on a display that can't, and the game
    /// can't tell which kind it's on (#307). While it's on, [`vsync`]
    /// isn't used.
    ///
    /// [`vsync`]: Display::vsync
    pub vrr: bool,
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
            vrr: false,
            window: None,
        }
    }
}

/// [`Display`] as read from a file, which may be from when vsync and
/// VRR were one setting, `sync`.
#[derive(Default, Deserialize)]
#[serde(default)]
struct DisplayFile {
    fullscreen: bool,
    vsync: Option<bool>,
    vrr: Option<bool>,
    sync: Option<OldSync>,
    window: Option<[u32; 2]>,
}

/// The one setting there was for vsync and VRR.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OldSync {
    /// Vsync, as the game couldn't tell whether a display had VRR.
    Auto,
    Vsync,
    Vrr,
    Off,
}

impl From<DisplayFile> for Display {
    fn from(file: DisplayFile) -> Display {
        let sync = file.sync;
        Display {
            fullscreen: file.fullscreen,
            vsync: file.vsync.unwrap_or(!matches!(sync, Some(OldSync::Off))),
            vrr: file.vrr.unwrap_or(matches!(sync, Some(OldSync::Vrr))),
            window: file.window,
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
            music: Volume::DEFAULT,
            effects: Volume::DEFAULT,
        }
    }
}

impl Volume {
    pub const STEP: u8 = 10;
    /// What both start at, until a saved setting says otherwise.
    pub const DEFAULT: u8 = 80;

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
                    vrr: true,
                    window: Some([1600, 900]),
                },
                volume: Volume {
                    music: 30,
                    effects: 0,
                },
                import_folder: Some(PathBuf::from("games")),
                widescreen: BTreeMap::from([
                    (BUNDLED.to_string(), Widescreen::OriginalFraming),
                    ("title-1".to_string(), Widescreen::WideActivation),
                ]),
                rendering: BTreeMap::from([("title-1".to_string(), Rendering::Enhanced)]),
            },
        ] {
            assert_eq!(Prefs::from_toml_str(&prefs.to_toml_string()), Ok(prefs));
        }
        assert!(Prefs::from_toml_str("preset = 3").is_err());
    }

    #[test]
    fn vsync_is_on_and_vrr_off_unless_changed() {
        let display = Prefs::default().display;
        assert!(display.vsync && !display.vrr);
        let prefs = Prefs::from_toml_str("preset = \"snappy\"").unwrap();
        assert_eq!(prefs.display, display, "prefs from before options");
        let prefs = Prefs::from_toml_str("[display]\nfullscreen = true").unwrap();
        assert!(prefs.display.fullscreen);
        assert!(prefs.display.vsync && !prefs.display.vrr);
        let prefs = Prefs::from_toml_str("[display]\nvrr = true").unwrap();
        assert!(prefs.display.vsync && prefs.display.vrr);
        let toml = Prefs::default().to_toml_string();
        assert!(
            toml.contains("vsync = true")
                && toml.contains("vrr = false")
                && !toml.contains("\nsync"),
            "{toml}"
        );
    }

    #[test]
    fn the_old_sync_setting_becomes_vsync_and_vrr() {
        for (old, vsync, vrr) in [
            ("sync = \"auto\"", true, false),
            ("sync = \"vsync\"", true, false),
            ("sync = \"vrr\"", true, true),
            ("sync = \"off\"", false, false),
            ("vsync = false", false, false),
            ("vsync = true", true, false),
        ] {
            let prefs = Prefs::from_toml_str(&format!("[display]\n{old}")).unwrap();
            assert_eq!(
                (prefs.display.vsync, prefs.display.vrr),
                (vsync, vrr),
                "{old}"
            );
        }
    }

    #[test]
    fn volumes_start_at_80_unless_set() {
        assert_eq!(Prefs::default().volume, Volume::default());
        let prefs = Prefs::from_toml_str("[volume]\nmusic = 40").unwrap();
        assert_eq!((prefs.volume.music, prefs.volume.effects), (40, 80));
        assert_eq!(
            Volume::default(),
            Volume {
                music: 80,
                effects: 80
            }
        );
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
