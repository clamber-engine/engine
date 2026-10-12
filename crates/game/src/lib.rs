//! The Clamber game: plays a level in a window.
//!
//! The simulation runs at exactly 60 Hz ([`Clock`]) and the screen is
//! drawn between its last two states, so motion stays smooth at any
//! refresh rate. Drawing is software rendering into a [`Canvas`] at a
//! fixed internal resolution ([`VIEW`]), scaled by whole pixels onto the
//! window with no filtering ([`present`]). Level layers are drawn with
//! parallax from a [`LayerArt`], and sprites from atlas pages by a
//! [`SpriteBank`], indexed sheets through a palette table. A [`Cast`]
//! draws the player and entities from their sheets, animated by what
//! they're doing.
//!
//! A [`Session`] puts the title, pause, level complete and game over
//! screens around the level, with [`Menu`]s driven by neutral actions,
//! and a [`Controls`] screen for remapping each player's bindings and
//! saving them to their settings file ([`settings_path`]).
//!
//! What happens in play becomes [`Cue`]s, which [`Audio`] plays from a
//! [`SoundList`], along with the level's music while it's being played.
//!
//! Everything here except the window itself works headlessly, so a
//! [`Session`] or [`Game`] can be stepped and drawn in tests, and
//! [`capture`]d to a GIF and screenshots without a display.

mod camera;
mod canvas;
mod capture;
mod cast;
mod clock;
mod controls;
mod game;
mod hud;
pub mod keys;
mod layers;
mod menu;
mod session;
mod sound;
mod sprites;
mod text;

use std::path::{Path, PathBuf};

pub use camera::{Camera, VIEW};
pub use canvas::{Canvas, Color, Fit, present, rgb};
pub use capture::{Capture, GIF_DELAY, SHOTS, capture, encode_gif, encode_png};
pub use cast::{Cast, PLAYER, Pose, Poses};
pub use clock::Clock;
pub use controls::{Controls, Request, Row, TIMEOUT};
pub use game::Game;
pub use hud::draw_hud;
pub use layers::LayerArt;
pub use menu::{Choice, Menu};
pub use session::{Item, Screen, Session, clock_time};
pub use sound::{Audio, Cue, DEFAULT_SOUNDS, SoundList, cues};
pub use sprites::{PaletteId, SheetId, Sprite, SpriteBank, SpriteBankBuilder};
pub use text::{ADVANCE, GLYPH, LINE, SHADOW, draw_text, draw_text_centred, text_size};

/// The level played when none is given, relative to the content folder.
pub const DEFAULT_LEVEL: &str = "levels/sandbox.level";

/// The player's and entities' sprite sheets, relative to the content
/// folder.
pub const SPRITES: &str = "base/sprites";

/// The content folder: next to the executable for a packaged build,
/// otherwise the working directory's, otherwise the source tree's.
pub fn content_dir() -> PathBuf {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("content")));
    let candidates = [
        beside_exe,
        Some(PathBuf::from("content")),
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|dir| dir.is_dir())
        .unwrap_or_else(|| PathBuf::from("content"))
}

/// The input settings file: `Clamber/input.toml` in the platform's
/// per-user settings folder. `None` when the environment doesn't say
/// where that is.
pub fn settings_path() -> Option<PathBuf> {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = if cfg!(windows) {
        var("APPDATA")?.join("Clamber")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/Clamber")
    } else {
        var("XDG_CONFIG_HOME")
            .or_else(|| Some(var("HOME")?.join(".config")))?
            .join("clamber")
    };
    Some(dir.join("input.toml"))
}
