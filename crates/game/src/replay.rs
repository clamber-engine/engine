//! Headless replays for differential testing: an input recording played
//! on a level of an imported pack from its start, as a [`Session`]
//! plays it, with a trace row and the picture of every frame at the
//! pack's own resolution, whole or part by part ([`Part`]). Nothing
//! here needs a window or a GPU, and the same inputs always give the
//! same rows and pictures.
//!
//! Inputs come from a scenario (`.scn`), the `input` column of a trace
//! (`.trace`), or a file of held actions, one line per frame
//! ([`parse_inputs`]).

use std::path::Path;
use std::sync::Arc;

use clamber_import::Installed;
use clamber_level::{Level, Vec2};
use clamber_pack::Presets;
use clamber_sim::{Script, World, generic_feel};
use clamber_sprite::{Rgba, RgbaImage};
use clamber_trace::{Buttons, Row, Scenario, Trace};

use crate::canvas::{Canvas, Color, rgb};
use crate::feels::STARTING_PRESET;
use crate::game::{Game, Part};
use crate::session::Session;
use crate::surface::Surface;

/// The first line of a file of held actions ([`parse_inputs`]).
pub const INPUTS_HEADER: &str = "# clamber-inputs v1";

/// The held buttons of each frame of an input recording: a scenario's
/// inputs for each of its frames, a trace's `input` column, or else a
/// file of held actions ([`parse_inputs`]), by `path`'s extension.
pub fn load_inputs(path: &Path) -> Result<Vec<Buttons>, String> {
    let origin = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{origin}: {e}"))?;
    match path.extension().and_then(|e| e.to_str()) {
        Some("scn") => {
            let scenario = Scenario::parse(&text, &origin).map_err(|e| e.to_string())?;
            Ok((0..scenario.frames).map(|f| scenario.held(f)).collect())
        }
        Some("trace") => {
            let trace = Trace::parse(&text, &origin).map_err(|e| e.to_string())?;
            Ok(trace.rows.iter().map(|r| r.input).collect())
        }
        _ => parse_inputs(&text, &origin),
    }
}

/// A file of held actions: [`INPUTS_HEADER`], then a line for each
/// frame from frame 0 with the actions held on it, as a trace's `input`
/// column has them (`.` for none). Blank lines and other lines starting
/// with `#` are skipped.
pub fn parse_inputs(text: &str, origin: &str) -> Result<Vec<Buttons>, String> {
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, first)) if first.trim_end() == INPUTS_HEADER => {}
        _ => return Err(format!("{origin}: the first line isn't `{INPUTS_HEADER}`")),
    }
    lines
        .map(|(i, line)| (i, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(i, line)| Buttons::parse_trace(line).map_err(|e| format!("{origin}:{}: {e}", i + 1)))
        .collect()
}

/// The preset an imported pack of `title` plays on: the first built-in
/// one holding that title's values, or else the starting preset.
pub fn preset_for(title: &str) -> &'static str {
    Presets::builtin()
        .iter()
        .find(|p| p.title.as_deref() == Some(title))
        .map_or(STARTING_PRESET, |p| p.id.as_str())
}

/// An imported pack's level being replayed.
pub struct Replay {
    pub session: Session,
    /// Where the player started, for the trace's positions.
    start: Vec2,
    /// Frames played.
    frame: u32,
    /// What was held on the last of them.
    held: Buttons,
}

impl Replay {
    /// `pack`'s level `level` (a file name in its levels folder, without
    /// `.level`; its first level if `None`), on `preset` (the pack's
    /// title's if `None`, [`preset_for`]), from its start or its arrival
    /// called `arrival`, drawn as the pack's own look has it, in
    /// original rendering.
    pub fn begin(
        pack: Installed,
        level: Option<&str>,
        preset: Option<&str>,
        arrival: Option<&str>,
    ) -> Result<Replay, String> {
        let levels = pack.levels();
        let path = match level {
            Some(id) => levels
                .into_iter()
                .find(|p| p.file_stem().is_some_and(|s| s == id))
                .ok_or_else(|| format!("{} has no level `{id}`", pack.dir.display()))?,
            None => levels
                .into_iter()
                .next()
                .ok_or_else(|| format!("{} has no levels", pack.dir.display()))?,
        };
        let preset = preset.unwrap_or_else(|| preset_for(&pack.manifest.title));
        let feel = generic_feel(preset).ok_or_else(|| format!("no preset named `{preset}`"))?;
        let loaded = Level::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let world = World::try_new(Arc::new(loaded), Arc::new(feel)).map_err(|e| e.to_string())?;
        let mut session = Session::new(Game::new(world));
        session.play_installed(&path, pack, arrival)?;
        let start = session.game.world.player.pos;
        Ok(Replay {
            session,
            start,
            frame: 0,
            held: Buttons::NONE,
        })
    }

    /// Frames played so far.
    pub fn frame(&self) -> u32 {
        self.frame
    }

    /// Plays a frame with `held` held, and gives its trace row.
    pub fn step(&mut self, held: Buttons) -> Row {
        let before = self.session.game.world.frame;
        let path = self.session.level_path.clone();
        let script = Script::from_buttons([self.held, held]);
        let input = script.frames().nth(1).expect("two frames");
        self.session.step(&input);
        let world = &self.session.game.world;
        let stepped = world.frame != before || self.session.level_path != path;
        let row = clamber_sim::row(world, self.frame, held, self.start, stepped);
        self.held = held;
        self.frame += 1;
        row
    }

    /// The size of the pictures: the pack's view.
    pub fn size(&self) -> (u32, u32) {
        self.session.look().view
    }

    /// The whole picture, as it's shown, at a pixel per unit.
    pub fn picture(&self) -> Canvas {
        let mut canvas = Canvas::scaled(self.size(), 1);
        self.session.draw(&mut canvas, 1.0);
        canvas
    }

    /// Each part of the game's picture on its own, named for files: the
    /// backdrop, each of the level's layers
    /// (`layer-<index>-<name>`), the sprites and the HUD. Where a part
    /// draws nothing, it's transparent. Menus over the game aren't in
    /// any part.
    pub fn parts(&self) -> Vec<(String, RgbaImage)> {
        let game = &self.session.game;
        let layers = game.world.level.layers.iter().enumerate();
        let layers =
            layers.map(|(i, l)| (format!("layer-{i}-{}", file_name(&l.name)), Part::Layer(i)));
        let parts = [("backdrop".to_string(), Part::Backdrop)]
            .into_iter()
            .chain(layers)
            .chain([
                ("sprites".to_string(), Part::Sprites),
                ("hud".to_string(), Part::Hud),
            ]);
        parts.map(|(name, part)| (name, self.part(part))).collect()
    }

    /// `part` alone, drawn once over black and once over white: what
    /// changes between them shows how see-through it is.
    fn part(&self, part: Part) -> RgbaImage {
        let over = |under: Color| {
            let mut canvas = Canvas::scaled(self.size(), 1);
            canvas.clear(under);
            self.session.game.draw_part(&mut canvas, 1.0, part);
            canvas
        };
        let (black, white) = (over(rgb(0, 0, 0)), over(rgb(255, 255, 255)));
        let mut image = RgbaImage::new(black.width, black.height);
        for (i, (&b, &w)) in black.pixels.iter().zip(&white.pixels).enumerate() {
            let (x, y) = (i as u32 % black.width, i as u32 / black.width);
            image.set(x, y, unblend(b, w));
        }
        image
    }
}

/// The colour drawn over black as `black` and over white as `white`,
/// with its opacity.
fn unblend(black: Color, white: Color) -> Rgba {
    let channel = |c: Color, shift: u32| ((c >> shift) & 0xff) as i32;
    let shifts = [16, 8, 0];
    let shown = shifts
        .map(|s| channel(white, s) - channel(black, s))
        .into_iter()
        .max();
    let alpha = (255 - shown.unwrap_or(0)).clamp(0, 255);
    if alpha == 0 {
        return [0; 4];
    }
    let [r, g, b] = shifts.map(|s| ((channel(black, s) * 255 + alpha / 2) / alpha).min(255) as u8);
    [r, g, b, alpha as u8]
}

/// `name` with anything but letters, digits, `-` and `_` as `_`.
fn file_name(name: &str) -> String {
    name.chars()
        .map(
            |c| match c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                true => c,
                false => '_',
            },
        )
        .collect()
}
