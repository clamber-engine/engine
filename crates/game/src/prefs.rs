//! Choices the player makes in the game's menus, other than controls,
//! kept from one run to the next in `prefs.toml` ([`prefs_path`]).
//!
//! [`prefs_path`]: crate::prefs_path

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clamber_import::Widescreen;
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
    /// The widescreen mode picked on the options screen for each game,
    /// by its pack's id, or [`BUNDLED`] for the bundled game. Games
    /// without one play in their own default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub widescreen: BTreeMap<String, Widescreen>,
}

/// What [`Prefs::widescreen`] calls the bundled game.
pub const BUNDLED: &str = "clamber";

/// The window's settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, from = "DisplayFile")]
pub struct Display {
    pub fullscreen: bool,
    /// How frames wait for the display.
    pub sync: FrameSync,
    /// The window's size when it isn't fullscreen, in physical pixels,
    /// as the player last left it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<[u32; 2]>,
}

impl Default for Display {
    fn default() -> Display {
        Display {
            fullscreen: false,
            sync: FrameSync::Auto,
            window: None,
        }
    }
}

/// [`Display`] as read from a file, which may be from before there was
/// a [`FrameSync`] setting, when there was only vsync on or off.
#[derive(Default, Deserialize)]
#[serde(default)]
struct DisplayFile {
    fullscreen: bool,
    sync: Option<FrameSync>,
    vsync: Option<bool>,
    window: Option<[u32; 2]>,
}

impl From<DisplayFile> for Display {
    fn from(file: DisplayFile) -> Display {
        let sync = match (file.sync, file.vsync) {
            (Some(sync), _) => sync,
            (None, Some(false)) => FrameSync::Off,
            (None, _) => FrameSync::Auto,
        };
        Display {
            fullscreen: file.fullscreen,
            sync,
            window: file.window,
        }
    }
}

/// How frames wait for the display, from the options screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameSync {
    /// VRR on a display that can vary its refresh, and vsync on others.
    #[default]
    Auto,
    /// Each frame waits for the display's next refresh, so it doesn't
    /// tear.
    Vsync,
    /// Frames are shown as soon as they're drawn, without tearing, on a
    /// display that varies its refresh to match.
    Vrr,
    /// Frames don't wait for the display, and may tear. They're capped
    /// at [`FRAME_CAP`] a second.
    ///
    /// [`FRAME_CAP`]: crate::display::FRAME_CAP
    Off,
}

impl FrameSync {
    pub const ALL: [FrameSync; 4] = [
        FrameSync::Auto,
        FrameSync::Vsync,
        FrameSync::Vrr,
        FrameSync::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FrameSync::Auto => "Auto",
            FrameSync::Vsync => "VSync",
            FrameSync::Vrr => "VRR",
            FrameSync::Off => "Off",
        }
    }

    /// The setting after this one, or before it if `back`, going round.
    pub fn next(self, back: bool) -> FrameSync {
        let i = FrameSync::ALL.iter().position(|&s| s == self).unwrap_or(0);
        let n = FrameSync::ALL.len();
        FrameSync::ALL[if back { (i + n - 1) % n } else { (i + 1) % n }]
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
                    sync: FrameSync::Vrr,
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
            },
        ] {
            assert_eq!(Prefs::from_toml_str(&prefs.to_toml_string()), Ok(prefs));
        }
        assert!(Prefs::from_toml_str("preset = 3").is_err());
    }

    #[test]
    fn sync_is_auto_unless_changed() {
        assert_eq!(Prefs::default().display.sync, FrameSync::Auto);
        let prefs = Prefs::from_toml_str("preset = \"snappy\"").unwrap();
        assert_eq!(
            prefs.display.sync,
            FrameSync::Auto,
            "prefs from before options"
        );
        let prefs = Prefs::from_toml_str("[display]\nfullscreen = true").unwrap();
        assert!(prefs.display.fullscreen);
        assert_eq!(prefs.display.sync, FrameSync::Auto);
        let prefs = Prefs::from_toml_str("[display]\nsync = \"vrr\"").unwrap();
        assert_eq!(prefs.display.sync, FrameSync::Vrr);
    }

    #[test]
    fn vsync_turned_off_before_there_was_sync_is_off() {
        let prefs = Prefs::from_toml_str("[display]\nvsync = false").unwrap();
        assert_eq!(prefs.display.sync, FrameSync::Off);
        let prefs = Prefs::from_toml_str("[display]\nvsync = true").unwrap();
        assert_eq!(prefs.display.sync, FrameSync::Auto);
        let toml = Prefs::default().to_toml_string();
        assert!(
            toml.contains("sync = \"auto\"") && !toml.contains("vsync"),
            "{toml}"
        );
    }

    #[test]
    fn sync_goes_round() {
        assert_eq!(FrameSync::Auto.next(false), FrameSync::Vsync);
        assert_eq!(FrameSync::Off.next(false), FrameSync::Auto);
        assert_eq!(FrameSync::Auto.next(true), FrameSync::Off);
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
