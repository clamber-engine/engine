//! The Clamber game: plays a level in a window.
//!
//! The simulation runs at exactly 60 Hz ([`Clock`]) and the screen is
//! drawn between its last two states, so motion stays smooth at any
//! refresh rate. Drawing is software rendering into a [`Canvas`] at a
//! fixed internal resolution ([`VIEW`]), scaled by whole pixels onto the
//! window with no filtering ([`present`]).
//!
//! Everything here except the window itself works headlessly, so a
//! [`Game`] can be stepped and drawn in tests and captures.

mod camera;
mod canvas;
mod clock;
mod game;
pub mod keys;

use std::path::{Path, PathBuf};

pub use camera::{Camera, VIEW};
pub use canvas::{Canvas, Color, Fit, present, rgb};
pub use clock::Clock;
pub use game::Game;

/// The level played when none is given, relative to the content folder.
pub const DEFAULT_LEVEL: &str = "levels/sandbox.level";

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
