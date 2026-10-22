//! The Clamber game: plays a level in a window.
//!
//! The simulation runs at exactly 60 Hz ([`Clock`]) and the screen is
//! drawn between its last two states, so motion stays smooth at any
//! refresh rate. With vsync, the steps follow the display's refresh
//! instead ([`Pace`]), so each frame is drawn a whole number of
//! refreshes on from the last. Everything is drawn on a [`Surface`] in view units: the
//! camera always sees [`VIEW`] units of the world, at whatever size it's
//! shown. The GPU draws a frame's [`DrawList`] at the window's own
//! resolution, at its shape ([`Gpu`]), in step with the display's
//! refresh unless vsync is turned off. Without a GPU, a [`Canvas`] draws
//! in software at a whole number of pixels per unit ([`present`]); tests
//! and captures use one too. The window's size and how often it's drawn
//! come from [`display`]. Images are [`Texture`]s, filtered smoothly from
//! mipmaps unless they're pixel art. Level layers are drawn with parallax
//! from a [`LayerArt`], and sprites by a [`SpriteBank`], each sheet at
//! its own texels per world unit. A [`Cast`] draws the player and
//! entities from their sheets, animated by what they're doing.
//!
//! A [`Session`] puts the title, pause, level complete and game over
//! screens around the level, with [`Menu`]s driven by neutral actions,
//! and a [`Controls`] screen for remapping each player's bindings and
//! saving them to their settings file ([`settings_path`]), and a
//! movement screen for choosing among the [`Feels`]' presets, which is
//! remembered in the [`Prefs`] ([`prefs_path`]) along with the options
//! screen's choices. A credits screen shows the content's [`Credits`].
//! The title screen also imports a game the player owns, picked in the
//! system's file dialog, dropped on the window or picked with a
//! [`Picker`], as a personal pack, and lists the packs to play
//! ([`Games`]).
//! A pack with a level map begins a new game on it and goes back to it
//! after each level ([`MapPlay`]).
//!
//! What happens in play becomes [`Cue`]s, which [`Audio`] plays from a
//! [`SoundList`], along with music: the title's on the title screen and
//! the level's in play, crossfading between them, with loops and volumes
//! from a [`MusicList`].
//!
//! Everything here except the window itself works headlessly, so a
//! [`Session`] or [`Game`] can be stepped and drawn in tests, and
//! [`capture`]d to a GIF and screenshots without a display. An imported
//! pack's level can be [`replay`]ed from an input recording, with its
//! trace and the picture of each frame, part by part.

mod beyond;
mod bunch_pictures;
mod camera;
mod canvas;
mod capture;
mod cast;
mod clock;
mod controls;
mod credits;
mod dim;
pub mod display;
mod feels;
mod game;
mod gpu;
mod hud;
mod icons;
mod import;
pub mod keys;
mod layers;
mod letter_pictures;
mod map;
mod menu;
mod music;
mod pacing;
mod panel;
mod pictures;
mod prefs;
mod prompts;
pub mod replay;
pub mod report;
mod session;
mod sound;
mod sparkles;
mod spin_pictures;
mod sprites;
mod surface;
mod text;
mod thrower_pictures;
mod thrown_pictures;
mod title;
mod walker_pictures;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_import::{Replacements, replace_sheet};
use clamber_level::Level;
use clamber_pack::Feel;
use clamber_sim::World;

pub use camera::{Camera, VIEW};
pub use canvas::{Canvas, Color, Fit, present, rgb};
pub use capture::{Capture, GIF_DELAY, SHOTS, capture, encode_gif, encode_png, redraw};
pub use cast::{Cast, PLAYER, Pose, Poses};
pub use clock::{Clock, Pace};
pub use controls::{Controls, Request, Row, TIMEOUT};
pub use credits::{Credits, DEFAULT_CREDITS};
pub use display::{Look, Presenting, Rendering};
pub use feels::Feels;
pub use game::{Game, Part};
pub use gpu::Gpu;
pub use hud::draw_hud;
pub use import::{Edit, Games, Importing, Picker};
pub use layers::LayerArt;
pub use map::{LEVEL_MAP, MapPhase, MapPlay, MapStep};
pub use menu::{Choice, Menu};
pub use music::{DEFAULT_MUSIC, MusicList, MusicTrack};
pub use pacing::FrameTimes;
pub use prefs::{Display, Prefs, Volume};
pub use session::{Item, Screen, Session, Setting, Tune, clock_time};
pub use sound::{Audio, Cue, DEFAULT_SOUNDS, SoundList, cues};
pub use sprites::{PaletteId, SheetId, Sprite, SpriteBank, SpriteBankBuilder};
pub use surface::{DrawList, Drawn, Flip, Quad, Surface, Texture, triangulate};
pub use text::{ADVANCE, GLYPH, LINE, SHADOW, draw_text, draw_text_centred, text_size};

/// The level played when none is given, relative to the content folder.
pub const DEFAULT_LEVEL: &str = "base/levels/first.level";

/// The player's and entities' sprite sheets, relative to the content
/// folder.
pub const SPRITES: &str = "base/sprites";

/// A game of the level at `path`, with its layers' art, its music's
/// folder, and the player and entities drawn from the sheets in the
/// folders of `sprites` (a later folder's sheet replacing an earlier
/// one's of the same name, [`Cast::load_layered`]), played with `feel`.
pub fn load_game(path: &Path, sprites: &[&Path], feel: Arc<Feel>) -> Result<Game, String> {
    load_game_with(
        path,
        sprites,
        feel,
        &Replacements::default(),
        &mut Vec::new(),
    )
}

/// [`load_game`], with the layers' images and the sprite sheets that
/// `replacements` has pictures for drawn with them when the game isn't
/// drawn on the original's pixel grid ([`Game::with_replaced`]).
/// Replacements that don't fit are added to `problems`.
pub fn load_game_with(
    path: &Path,
    sprites: &[&Path],
    feel: Arc<Feel>,
    replacements: &Replacements,
    problems: &mut Vec<String>,
) -> Result<Game, String> {
    let level = Level::load(path).map_err(|e| e.to_string())?;
    let art = LayerArt::load(&level, path.parent().unwrap_or(Path::new("")))?;
    let sheets = Cast::load_sheets(sprites)?;
    let (mut replaced_art, mut replaced_cast) = (None, None);
    if !replacements.is_empty() {
        replaced_art = art.replaced(&level, replacements, problems);
        let mut any = false;
        let replaced: Vec<_> = sheets
            .iter()
            .map(|(name, sheet)| {
                let made = replace_sheet(sheet, replacements, problems);
                any |= made.is_some();
                (name.clone(), made.unwrap_or_else(|| sheet.clone()))
            })
            .collect();
        replaced_cast = any.then(|| Cast::new(replaced));
    }
    let cast = Cast::new(sheets.into_iter().collect());
    let world = World::try_new(Arc::new(level), feel).map_err(|e| e.to_string())?;
    Ok(Game::new(world)
        .with_art(art)
        .with_cast(cast)
        .with_replaced(replaced_art, replaced_cast))
}

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

/// The input settings file: `input.toml` in the [`user_dir`].
pub fn settings_path() -> Option<PathBuf> {
    Some(user_dir()?.join("input.toml"))
}

/// The menu choices file: `prefs.toml` in the [`user_dir`].
pub fn prefs_path() -> Option<PathBuf> {
    Some(user_dir()?.join("prefs.toml"))
}

/// The log of the last run's warnings and errors: `clamber.log` in the
/// [`user_dir`].
pub fn log_path() -> Option<PathBuf> {
    Some(user_dir()?.join("clamber.log"))
}

/// The game's folder in the platform's per-user settings folder:
/// `%APPDATA%\Clamber` on Windows. `None` when the environment doesn't
/// say where that is.
pub fn user_dir() -> Option<PathBuf> {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    Some(if cfg!(windows) {
        var("APPDATA")?.join("Clamber")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/Clamber")
    } else {
        var("XDG_CONFIG_HOME")
            .or_else(|| Some(var("HOME")?.join(".config")))?
            .join("clamber")
    })
}
