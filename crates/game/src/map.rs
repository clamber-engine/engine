//! The level map between levels, for a pack that has one
//! (`spec/behavior/level-map.md`).
//!
//! A new game begins on the map ([`MapPlay::new_game`]). Entering a level
//! node begins its level on the frame after the map's black frames.
//! Finishing that level records it in the [`Progress`], and the map
//! appears again `black` frames after the level's last frame
//! ([`FinishTimes`]), with the lead at the level's node and the node's
//! `onward` direction given ([`MapPlay::finished`]).
//!
//! Engine choices where the spec says nothing: only level nodes and warps
//! within the map can be entered; a press at any other node (level stops,
//! places and the start point) does nothing, though like any press it
//! replaces the remembered one. The map is drawn as a stand-in picture:
//! its open paths as lines, its nodes as dots and the lead as a block,
//! at the map's brightness, a page at a time. No music plays on it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_input::{Action, Actions};
use clamber_worldmap::{
    Buttons, FinishTimes, LegWalker, Legs, LegsScreen, MapMusic, Mode, NodeKind, Progress, WorldMap,
};

use crate::canvas::{Color, rgb};
use crate::cast::Cast;
use crate::pictures;
use crate::surface::{Flip, Quad, Surface, Texture};

/// Where a pack keeps the map a new game begins on.
pub const LEVEL_MAP: &str = "maps/level-map.map";

const PATH: Color = rgb(0xc8, 0xa0, 0x60);
const LEVEL: Color = rgb(0xe8, 0x50, 0x40);
const STOP: Color = rgb(0xf0, 0xe0, 0xb0);
const LEAD: Color = rgb(0x40, 0xb0, 0xf0);
/// How far left of a page's edge the other character's origin may be and still
/// show (cut off) there: it is in view from position 248 on the previous page.
const OTHER_IN_VIEW: f64 = 8.0;
const OTHER: Color = rgb(0xf0, 0xa0, 0x40);

/// Where the game is with respect to the map.
#[derive(Clone, Debug, PartialEq)]
pub enum MapPhase {
    /// The map is showing.
    Shown(Box<LegsScreen>),
    /// A level entered from this node is being played.
    Playing(usize),
    /// The level at `node` was just finished: after `frames` more black
    /// frames the map appears with the lead there, as `times` says.
    Black {
        frames: u32,
        node: usize,
        times: FinishTimes,
        /// The lead was knocked out there instead: no way onward is
        /// given and the level isn't counted as finished.
        lost: bool,
    },
    /// The lead was knocked out with no tries left: the game-over picture
    /// ([`GameOver`]).
    GameOver(GameOver),
}

/// The game-over picture and what happens to it (`level-map.md`, "With no
/// tries left").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameOver {
    /// The picture's frame on the frame last run: negative while the
    /// screen is still black.
    pub frame: i32,
    /// Whether a button that presses was held on the frame before.
    held: bool,
    /// The frame of the press that started the fade-out, and the brightness
    /// the picture had then.
    press: Option<(i32, u32)>,
}

impl GameOver {
    /// The frames from the knock-out's step to the picture's frame 0.
    const WAIT: i32 = 40;
    /// The frames the fade-in and the fade-out take to change by one
    /// brightness step.
    const STEP: i32 = 2;
    /// Black frames after the fade-out, and the frame after them on which
    /// the next screen begins.
    const BLACK: i32 = 65;

    fn new() -> GameOver {
        GameOver {
            frame: -Self::WAIT,
            held: false,
            press: None,
        }
    }

    /// The frame on which the picture is black after the press.
    fn black_from(&self) -> Option<i32> {
        let (press, level) = self.press?;
        // a press on frame 1 is black on frame 3, not 1
        Some(if press == 1 {
            3
        } else {
            press + Self::STEP * level as i32
        })
    }

    /// The picture's brightness, 0 (black) to [`clamber_worldmap::FULL`].
    pub fn brightness(&self) -> u32 {
        let full = clamber_worldmap::FULL as i32;
        let level = match self.press {
            None => self.frame / Self::STEP,
            Some((press, level)) => level as i32 - (self.frame - press) / Self::STEP,
        };
        level.clamp(0, full) as u32
    }

    /// Runs a frame with `pressed` whether a button that presses is held:
    /// true on the frame the next screen begins.
    fn step(&mut self, pressed: bool) -> bool {
        self.frame += 1;
        let new = pressed && !self.held;
        self.held = pressed;
        if new && self.press.is_none() && self.frame >= 1 {
            let level = self.brightness();
            self.press = Some((self.frame, level));
        }
        self.black_from()
            .is_some_and(|black| self.frame > black + Self::BLACK)
    }
}

/// Which pictures the characters begin with when the map appears
/// (`level-map.md`, "When the map appears").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Opening {
    /// The ordinary cycle, from frame 1.
    #[default]
    Cycle,
    /// A level finished for the first time: the finish pictures up to
    /// frame 161, then the cycle with the still frames (the first and how
    /// many) left out of the count.
    Finish(u32, u32),
    /// A new game's first map: the lead's own start pictures.
    Start,
}

/// A character's picture on the map: its number in the set it is from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapLook {
    /// The ordinary cycle: `map-lead-1` to `map-lead-10`, or `map-other-1`
    /// to `map-other-8`.
    Cycle(usize),
    /// A finish picture, `map-lead-finish-1` to `map-lead-finish-11` (or
    /// the other character's).
    Finish(usize),
    /// `map-start-1` to `map-start-9`.
    Start(usize),
}

/// The last frame of the finish pictures.
const FINISH_END: u64 = 161;

/// What a frame on the map did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapStep {
    /// Nothing that leaves the map.
    Stay,
    /// The map has faded out after a level node was entered: its level,
    /// as a path, begins now.
    Begin(PathBuf),
    /// The game-over picture has faded out and its black frames are over.
    Over,
}

/// A legs map and the player's progress on it.
#[derive(Clone, Debug, PartialEq)]
pub struct MapPlay {
    pub map: WorldMap,
    /// The folder the map's level paths are relative to.
    pub root: PathBuf,
    pub progress: Progress,
    pub phase: MapPhase,
    lead: LeadLook,
    /// The same for the other character.
    other: LeadLook,
    /// Whether there are two characters on the map.
    two: bool,
    /// Whether the second character is the lead: the pictures follow the
    /// characters, not the places (`level-map.md`, "Pictures").
    pub(crate) second_leads: bool,
    pub(crate) opening: Opening,
    /// Whether the level last finished had been finished before.
    pub(crate) again: bool,
    /// The game-over picture, if the pack has it.
    game_over: Option<Picture>,
}

/// A picture ready to draw; equal to itself only.
#[derive(Clone, Debug)]
struct Picture(Arc<Texture>);

impl PartialEq for Picture {
    fn eq(&self, other: &Picture) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Picture {
    /// The pack's game-over picture, if it has one and it can be read.
    fn load(root: &Path) -> Option<Picture> {
        let bytes = std::fs::read(root.join(clamber_import::game_over_picture_path())).ok()?;
        match clamber_sprite::decode_png(&bytes) {
            Ok(clamber_sprite::Image::Rgba(image)) => Some(Picture(Texture::new(image, true))),
            _ => None,
        }
    }
}

/// What the lead's picture on the map depends on besides where it is
/// (`level-map.md`, "Pictures").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LeadLook {
    /// Facing left: the page's `x` last went down.
    left: bool,
    /// The lead's `x` on its page on the frame before.
    x: Option<i64>,
    /// The counted frame of the last page change.
    changed: Option<u64>,
    /// The page changes on the next frame.
    about: bool,
}

impl MapPlay {
    /// The map at [`LEVEL_MAP`] in the pack at `root`, if it has one a
    /// new game can begin on: a legs map with a start node.
    pub fn load(root: &Path) -> Result<Option<MapPlay>, String> {
        let path = root.join(LEVEL_MAP);
        if !path.is_file() {
            return Ok(None);
        }
        let map = WorldMap::load(&path).map_err(|e| e.to_string())?;
        Ok(MapPlay::new(map, root))
    }

    /// `map`, whose level paths are relative to `root`, before a new
    /// game. `None` unless it's a legs map with a start node.
    pub fn new(map: WorldMap, root: &Path) -> Option<MapPlay> {
        let Mode::Legs(legs) = &map.mode else {
            return None;
        };
        let screen = LegsScreen::new_game(&map, legs)?;
        let opening = Self::new_game_opening(legs);
        Some(MapPlay {
            map,
            root: root.to_path_buf(),
            progress: Progress::default(),
            phase: MapPhase::Shown(Box::new(screen)),
            lead: LeadLook::default(),
            other: LeadLook::default(),
            two: false,
            second_leads: false,
            opening,
            again: false,
            game_over: Picture::load(root),
        })
    }

    /// A new game's map shows the start pictures if the lead walks off on
    /// its own, as the map says.
    fn new_game_opening(legs: &Legs) -> Opening {
        if legs.new_game.is_some() {
            Opening::Start
        } else {
            Opening::Cycle
        }
    }

    /// The map's music: its file, in the folder of the map file, and the
    /// map's description of it.
    pub fn music(&self) -> Option<(PathBuf, &MapMusic)> {
        let music = self.map.music.as_ref()?;
        let dir = Path::new(LEVEL_MAP).parent().unwrap_or(Path::new(""));
        Some((self.root.join(dir).join(&music.file), music))
    }

    fn legs(&self) -> &Legs {
        match &self.map.mode {
            Mode::Legs(legs) => legs,
            _ => unreachable!("only legs maps are played"),
        }
    }

    /// Begins a new game: no progress, and the map with the lead at its
    /// start, and the other character beside it if `two`.
    pub fn new_game(&mut self, two: bool) {
        let mut screen = LegsScreen::new_game(&self.map, self.legs()).expect("checked in new");
        if two {
            screen.lead = screen.lead.with_other();
        }
        self.progress = Progress::default();
        self.two = two;
        self.second_leads = false;
        self.opening = Self::new_game_opening(self.legs());
        self.phase = MapPhase::Shown(Box::new(screen));
        self.lead = LeadLook::default();
        self.other = LeadLook::default();
    }

    /// Whether a press at `node` enters it: a level, or a warp to another
    /// node of this map.
    fn enterable(&self, node: usize) -> bool {
        match &self.map.nodes[node].kind {
            NodeKind::Level { .. } => true,
            NodeKind::Warp { map, node } => *map == self.map.id && self.map.node(node).is_some(),
            _ => false,
        }
    }

    /// Runs a frame of the map with `held` held.
    pub fn step(&mut self, held: Actions) -> MapStep {
        if let MapPhase::GameOver(over) = &mut self.phase {
            let pressed = [Action::Jump, Action::Pause, Action::Tandem]
                .into_iter()
                .any(|a| held.contains(a));
            return if over.step(pressed) {
                MapStep::Over
            } else {
                MapStep::Stay
            };
        }
        if let MapPhase::Black {
            frames,
            node,
            times,
            lost,
        } = &mut self.phase
        {
            *frames = frames.saturating_sub(1);
            if *frames > 0 {
                return MapStep::Stay;
            }
            let (node, times, lost) = (*node, times.clone(), *lost);
            let given = self.map.nodes[node].onward.filter(|_| !lost);
            let mut lead = LegWalker::after_finish(&self.map, self.legs(), node, &times, given);
            if self.two {
                lead = lead.with_other();
            }
            self.phase = MapPhase::Shown(Box::new(LegsScreen::new(lead)));
            self.opening = match times.still {
                Some((from, n)) => Opening::Finish(from, n),
                None => Opening::Cycle,
            };
            self.lead = LeadLook::default();
            self.other = LeadLook::default();
        }
        let MapPhase::Shown(screen) = &self.phase else {
            return MapStep::Stay;
        };
        // where the lead is or will be, so a button kept held across a
        // walk doesn't turn into a new press of another
        let node = screen.lead.heading().unwrap_or(screen.lead.node);
        let buttons = buttons(held, self.enterable(node));
        let mut screen = (**screen).clone();
        let out = screen.step(&self.map, self.legs(), &self.progress, buttons);
        if out.appeared.is_some() {
            // the map again after a press, the cycle from its first frame
            self.opening = Opening::Cycle;
        }
        let page_width = i64::from(self.legs().page_width.max(1));
        let x = screen.lead.position().x.floor().rem_euclid(page_width);
        // the page changes on the next frame if a frame run now says so
        let about = screen
            .clone()
            .step(&self.map, self.legs(), &self.progress, buttons)
            .step
            .page
            .is_some();
        let look = &mut self.lead;
        if out.step.page.is_some() {
            look.changed = Some(screen.lead.frame());
        } else if let Some(before) = look.x
            && x != before
        {
            look.left = x < before;
        }
        look.x = Some(x);
        look.about = about;
        if let Some(other) = screen.lead.other_position() {
            let x = other.x.floor().rem_euclid(page_width);
            let look = &mut self.other;
            if let Some(before) = look.x
                && x != before
                && (x - before).abs() < page_width / 2
            {
                look.left = x < before;
            }
            look.x = Some(x);
        }
        self.phase = MapPhase::Shown(Box::new(screen));
        let Some(node) = out.left else {
            return MapStep::Stay;
        };
        match &self.map.nodes[node].kind {
            NodeKind::Level { level, .. } => {
                self.phase = MapPhase::Playing(node);
                MapStep::Begin(self.root.join(level))
            }
            // warps to other maps aren't entered (see enterable)
            _ => MapStep::Stay,
        }
    }

    /// Whether a level entered from the map is being played.
    pub fn playing(&self) -> bool {
        matches!(self.phase, MapPhase::Playing(_))
    }

    /// The level entered from the map was finished: records it, and
    /// shows the map after the black frames. Does nothing unless a level
    /// entered from the map is being played.
    pub fn finished(&mut self, two: bool, second_leads: bool) {
        let MapPhase::Playing(node) = self.phase else {
            return;
        };
        self.two = two;
        self.second_leads = second_leads;
        let place = self.map.place(node);
        let again = self.progress.is_cleared(&place);
        self.again = again;
        let exit = match &self.map.nodes[node].kind {
            NodeKind::Level { exits, .. } => exits.first().cloned(),
            _ => None,
        };
        self.progress
            .clear(&place, exit.as_deref().unwrap_or("normal"));
        let legs = self.legs();
        let times = match &legs.finish {
            Some(f) if again => f.again.clone(),
            Some(f) => f.first.clone(),
            None => FinishTimes {
                black: 0,
                still: None,
                gate: legs.appear_wait,
            },
        };
        self.phase = MapPhase::Black {
            // the step after the last black one is the map's first
            frames: times.black.max(1),
            node,
            times,
            lost: false,
        };
    }

    /// The lead, alone, was knocked out in the level entered from the
    /// map: shows the map after the black frames, as after a level
    /// finished before, with the lead at the level's stop and nothing
    /// recorded (`level-map.md`, "After a knock-out as the lead
    /// alone"). Does nothing unless a level entered from the map is
    /// being played.
    pub fn knocked_out(&mut self) {
        let MapPhase::Playing(node) = self.phase else {
            return;
        };
        self.two = false;
        self.second_leads = false;
        self.again = true;
        let legs = self.legs();
        let times = match &legs.finish {
            Some(f) => FinishTimes {
                still: None,
                ..f.again.clone()
            },
            None => FinishTimes {
                black: 0,
                still: None,
                gate: legs.appear_wait,
            },
        };
        self.phase = MapPhase::Black {
            // the knock-out's step comes after the level's last, so the
            // black frames are one fewer
            frames: times.black.saturating_sub(1).max(1),
            node,
            times,
            lost: true,
        };
    }

    /// The lead, alone, was knocked out in the level entered from the
    /// map with no tries left: the game-over picture appears instead of
    /// the map (`level-map.md`, "With no tries left"). Does nothing
    /// unless a level entered from the map is being played.
    pub fn game_over(&mut self) {
        if !self.playing() {
            return;
        }
        self.two = false;
        self.second_leads = false;
        self.phase = MapPhase::GameOver(GameOver::new());
    }

    /// Whether the game-over picture is showing or about to.
    pub fn is_game_over(&self) -> bool {
        matches!(self.phase, MapPhase::GameOver(_))
    }

    /// The lead's map picture on the frame last run, with the set it is
    /// from (the second character's pictures when it leads), and whether
    /// it is mirrored; None when the map isn't shown or the lead isn't
    /// drawn, on the frames around a page change (`level-map.md`,
    /// "Pictures").
    pub fn lead_picture(&self) -> Option<(MapLook, bool)> {
        let MapPhase::Shown(screen) = &self.phase else {
            return None;
        };
        let frame = screen.lead.frame();
        let covered = self.lead.about || self.lead.changed.is_some_and(|p| frame <= p + 1);
        let cycle = if self.second_leads { 8 } else { 10 };
        let look = self.look(frame, cycle, true);
        let mirrored = self.lead.left && matches!(look, MapLook::Cycle(_));
        (!covered).then_some((look, mirrored))
    }

    /// What a character with `cycle` pictures shows after `frame` frames
    /// were run ("When the map appears"); `lead` if it is the one the
    /// start pictures are for.
    fn look(&self, frame: u64, cycle: u64, lead: bool) -> MapLook {
        // the map's frames number from 0, and the cycle's pictures begin
        // on frame 1
        let number = frame.saturating_sub(1);
        let counted = |c: u64| MapLook::Cycle((c.saturating_sub(1) / 4 % cycle) as usize);
        match self.opening {
            Opening::Cycle => counted(number),
            Opening::Start if lead => MapLook::Start((number.saturating_sub(1) / 4 % 9) as usize),
            Opening::Start => counted(number),
            Opening::Finish(..) if number <= FINISH_END => MapLook::Finish(finish_picture(number)),
            Opening::Finish(from, n) => {
                let from = u64::from(from);
                let still = (number + 1).saturating_sub(from).min(u64::from(n));
                counted(number - FINISH_END - still)
            }
        }
    }

    /// Draws `look` of the lead (`lead`) or the other character with its
    /// origin at `origin`, mirrored if `flip`; false unless the cast has
    /// the pictures.
    fn draw_look(
        &self,
        canvas: &mut dyn Surface,
        cast: &Cast,
        look: MapLook,
        lead: bool,
        flip: bool,
        origin: (i32, i32),
    ) -> bool {
        // the pictures follow the character, not the place
        let first = lead != self.second_leads;
        match look {
            MapLook::Cycle(n) if first => cast.draw_map_lead(canvas, n, flip, origin),
            MapLook::Cycle(n) => cast.draw_map_other(canvas, n, flip, origin),
            MapLook::Finish(n) => {
                let set = if first {
                    &pictures::MAP_LEAD_FINISH
                } else {
                    &pictures::MAP_OTHER_FINISH
                };
                cast.draw_map_set(canvas, set, n, origin)
            }
            MapLook::Start(n) => cast.draw_map_set(canvas, &pictures::MAP_START, n, origin),
        }
    }

    /// Draws the other character at `at` if it is on the lead's page;
    /// `view` is the page's left edge, the scale and the map's height.
    fn draw_other(
        &self,
        canvas: &mut dyn Surface,
        cast: &Cast,
        screen: &LegsScreen,
        at: clamber_level::Vec2,
        view: (f64, f64, f64),
    ) {
        let legs = self.legs();
        let (left_edge, scale, height) = view;
        let (_, h) = canvas.view();
        let (px, py) = (at.x.floor() as f64 - left_edge, at.y.floor() as f64);
        // after a walk over a page edge the other is still on the previous
        // page: out of view until 8 px short of the edge, then cut off
        // (`level-map.md`, "The other character on the previous page")
        if !other_in_view(px, legs.page_width) {
            return;
        }
        let py = if legs.y_up { height - py } else { py };
        let origin = (
            (px * scale - 1.0).floor() as i32,
            (py * scale + (f64::from(h) - height * scale) / 2.0).floor() as i32,
        );
        let cycle = if self.second_leads { 10 } else { 8 };
        let look = self.look(screen.lead.frame(), cycle, false);
        let flip = self.other.left && matches!(look, MapLook::Cycle(_));
        if self.draw_look(canvas, cast, look, false, flip, origin) {
            return;
        }
        let (bw, bh) = ((6.0 * scale) as i32, (10.0 * scale) as i32);
        canvas.fill_rect(
            origin.0 + 1 - bw / 2,
            origin.1 - bh,
            bw,
            bh,
            dim(OTHER, screen.brightness(legs)),
        );
    }

    /// Draws the game-over picture at its brightness, its pixels at the
    /// screen's, as the map's picture is placed.
    fn draw_game_over(&self, canvas: &mut dyn Surface, over: &GameOver) {
        let light = over.brightness();
        if over.frame < 0 || light == 0 {
            return;
        }
        let (w, h) = canvas.view();
        let Some(Picture(texture)) = &self.game_over else {
            let centre = w as i32 / 2;
            crate::text::draw_text_centred(
                canvas,
                centre,
                h as i32 / 2,
                "GAME OVER",
                dim(STOP, light),
            );
            return;
        };
        let scale = w as f32 / texture.width() as f32;
        let height = texture.height() as f32 * scale;
        let top = (h as f32 - height) / 2.0;
        canvas.quad(&Quad {
            texture: texture.clone(),
            src: clamber_sprite::Rect {
                x: 0,
                y: 0,
                w: texture.width(),
                h: texture.height(),
            },
            dst: [0.0, top, w as f32, top + height],
            flip: Flip::default(),
            flash: None,
            opacity: 255,
            lean: 0.0,
            normals: None,
        });
        let full = clamber_worldmap::FULL;
        if light < full {
            canvas.shade(0, (255 * (full - light) / full) as u8);
        }
    }

    /// Draws the map as it is after the frame last run.
    pub fn draw(&self, canvas: &mut dyn Surface, cast: &Cast) {
        canvas.clear(0);
        if let MapPhase::GameOver(over) = &self.phase {
            self.draw_game_over(canvas, over);
            return;
        }
        let MapPhase::Shown(screen) = &self.phase else {
            return;
        };
        let legs = self.legs();
        let light = screen.brightness(legs);
        if light == 0 {
            return;
        }
        let (w, h) = canvas.view();
        let scale = f64::from(w) / f64::from(legs.page_width.max(1));
        let left_edge = (screen.lead.page(legs) * i64::from(legs.page_width)) as f64;
        let height = self.map.size.y.to_f64();
        let at = |p: clamber_level::Vec2| {
            let y = p.y.to_f64();
            let y = if legs.y_up { height - y } else { y };
            (
                (p.x.to_f64() - left_edge) * scale,
                y * scale + (f64::from(h) - height * scale) / 2.0,
            )
        };
        let shade = |c: Color| dim(c, light);
        for (i, path) in self.map.paths.iter().enumerate() {
            if !self.map.path_visible(i, &self.progress) {
                continue;
            }
            let open =
                self.map.path_open(i, &self.progress) || self.map.path_open_back(i, &self.progress);
            if !open {
                continue;
            }
            let mut points = vec![self.map.nodes[path.from].at];
            points.extend(path.via.iter().copied());
            points.push(self.map.nodes[path.to].at);
            for pair in points.windows(2) {
                canvas.line(at(pair[0]), at(pair[1]), shade(PATH));
            }
        }
        for (i, node) in self.map.nodes.iter().enumerate() {
            if !self.map.node_visible(i, &self.progress) {
                continue;
            }
            let (x, y) = at(node.at);
            let color = match node.kind {
                NodeKind::Level { .. } => LEVEL,
                _ => STOP,
            };
            let r = (2.0 * scale).max(1.0);
            let (x, y, r) = (x.round() as i32, y.round() as i32, r.round() as i32);
            canvas.fill_rect(x - r, y - r, 2 * r, 2 * r, shade(color));
        }
        self.draw_lead(canvas, cast, screen, (left_edge, scale, height));
        // the other is in front of the lead where they overlap
        // (`level-map.md`, "Where the two characters overlap"), whichever
        // character leads
        if let Some(at) = screen.lead.other_position() {
            self.draw_other(canvas, cast, screen, at, (left_edge, scale, height));
        }
    }

    /// Draws the lead; `view` is the page's left edge, the scale and the
    /// map's height.
    fn draw_lead(
        &self,
        canvas: &mut dyn Surface,
        cast: &Cast,
        screen: &LegsScreen,
        view: (f64, f64, f64),
    ) {
        let legs = self.legs();
        let (left_edge, scale, height) = view;
        let (_, h) = canvas.view();
        let position = screen.lead.position();
        let Some((look, left)) = self.lead_picture() else {
            return;
        };
        let (px, py) = (position.x.floor() as f64, position.y.floor() as f64);
        let (px, py) = (px - left_edge, if legs.y_up { height - py } else { py });
        let origin = (
            px * scale - 1.0,
            py * scale + (f64::from(h) - height * scale) / 2.0,
        );
        let origin = (origin.0.floor() as i32, origin.1.floor() as i32);
        if self.draw_look(canvas, cast, look, true, left, origin) {
            return;
        }
        let y = position.y.to_f64();
        let y = if legs.y_up { height - y } else { y };
        let x = (position.x.to_f64() - left_edge) * scale;
        let y = y * scale + (f64::from(h) - height * scale) / 2.0;
        let (bw, bh) = ((6.0 * scale) as i32, (10.0 * scale) as i32);
        let x = x.round() as i32 - bw / 2;
        canvas.fill_rect(
            x,
            y.round() as i32 - bh,
            bw,
            bh,
            dim(LEAD, screen.brightness(legs)),
        );
    }
}

/// The finish picture, 0 to 10, on frame `number` of 1 to 161: 1 up to 8,
/// down to 2 and up again, each on 2 frames, then 9, 10 and 11
/// (`level-map.md`, "When the map appears").
fn finish_picture(number: u64) -> usize {
    match number {
        156..=157 => 8,
        158..=159 => 9,
        160.. => 10,
        _ => {
            let k = (number.saturating_sub(1) / 2 % 14) as usize;
            if k < 8 { k } else { 14 - k }
        }
    }
}

/// The map's buttons for the actions `held`. Jump, Action, Pause and
/// Tandem (which does what Jump does, `controls.md`) are enter buttons
/// where they `enter`, and other buttons elsewhere; Swap is always
/// another button.
fn buttons(held: Actions, enter: bool) -> Buttons {
    let pairs = [
        (Action::Right, Buttons::RIGHT),
        (Action::Left, Buttons::LEFT),
        (Action::Up, Buttons::UP),
        (Action::Down, Buttons::DOWN),
        (Action::Swap, Buttons::other(0)),
    ];
    let presses = [Action::Jump, Action::Action, Action::Pause, Action::Tandem];
    let mut b = Buttons::NONE;
    for (action, button) in pairs {
        if held.contains(action) {
            b = b | button;
        }
    }
    for (n, action) in presses.into_iter().enumerate() {
        if held.contains(action) {
            let n = n as u8;
            b = b | if enter {
                Buttons::enter(n)
            } else {
                Buttons::other(n + 1)
            };
        }
    }
    b
}

/// `c` at brightness `light`, 0 (black) to [`clamber_worldmap::FULL`].
/// Whether the other character, `px` across the shown page, is in view.
fn other_in_view(px: f64, page_width: u32) -> bool {
    (-OTHER_IN_VIEW..f64::from(page_width)).contains(&px)
}

fn dim(c: Color, light: u32) -> Color {
    let full = clamber_worldmap::FULL;
    let channel = |shift: u32| ((c >> shift & 0xff) * light / full) << shift;
    channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_other_shows_cut_off_from_8_px_before_the_page_edge() {
        assert!(!other_in_view(-9.0, 256));
        assert!(other_in_view(-8.0, 256));
        assert!(other_in_view(0.0, 256));
        assert!(!other_in_view(256.0, 256));
    }

    #[test]
    fn tandem_enters_as_jump_does() {
        let jump = buttons(Action::Jump.into(), true);
        let tandem = buttons(Action::Tandem.into(), true);
        assert!(jump.any(Buttons::ENTER) && tandem.any(Buttons::ENTER));
        // kept apart, so pressing one while the other is held is new
        let both = buttons(Actions::from(Action::Jump) | Action::Tandem.into(), true);
        assert!(!both.newly(jump).is_empty());
        // and only another button where nothing can be entered
        assert!(!buttons(Action::Tandem.into(), false).any(Buttons::ENTER));
        assert!(!buttons(Action::Tandem.into(), false).is_empty());
    }
}
