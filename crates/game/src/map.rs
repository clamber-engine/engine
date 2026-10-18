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

use clamber_input::{Action, Actions};
use clamber_worldmap::{
    Buttons, FinishTimes, LegWalker, Legs, LegsScreen, Mode, NodeKind, Progress, WorldMap,
};

use crate::canvas::{Color, rgb};
use crate::surface::Surface;

/// Where a pack keeps the map a new game begins on.
pub const LEVEL_MAP: &str = "maps/level-map.map";

const PATH: Color = rgb(0xc8, 0xa0, 0x60);
const LEVEL: Color = rgb(0xe8, 0x50, 0x40);
const STOP: Color = rgb(0xf0, 0xe0, 0xb0);
const LEAD: Color = rgb(0x40, 0xb0, 0xf0);

/// Where the game is with respect to the map.
#[derive(Clone, Debug, PartialEq)]
pub enum MapPhase {
    /// The map is showing.
    Shown(LegsScreen),
    /// A level entered from this node is being played.
    Playing(usize),
    /// The level at `node` was just finished: after `frames` more black
    /// frames the map appears with the lead there, as `times` says.
    Black {
        frames: u32,
        node: usize,
        times: FinishTimes,
    },
}

/// What a frame on the map did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapStep {
    /// Nothing that leaves the map.
    Stay,
    /// The map has faded out after a level node was entered: its level,
    /// as a path, begins now.
    Begin(PathBuf),
}

/// A legs map and the player's progress on it.
#[derive(Clone, Debug, PartialEq)]
pub struct MapPlay {
    pub map: WorldMap,
    /// The folder the map's level paths are relative to.
    pub root: PathBuf,
    pub progress: Progress,
    pub phase: MapPhase,
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
        Some(MapPlay {
            map,
            root: root.to_path_buf(),
            progress: Progress::default(),
            phase: MapPhase::Shown(screen),
        })
    }

    fn legs(&self) -> &Legs {
        match &self.map.mode {
            Mode::Legs(legs) => legs,
            _ => unreachable!("only legs maps are played"),
        }
    }

    /// Begins a new game: no progress, and the map with the lead at its
    /// start.
    pub fn new_game(&mut self) {
        let screen = LegsScreen::new_game(&self.map, self.legs()).expect("checked in new");
        self.progress = Progress::default();
        self.phase = MapPhase::Shown(screen);
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
        if let MapPhase::Black {
            frames,
            node,
            times,
        } = &mut self.phase
        {
            *frames = frames.saturating_sub(1);
            if *frames > 0 {
                return MapStep::Stay;
            }
            let (node, times) = (*node, times.clone());
            let given = self.map.nodes[node].onward;
            let lead = LegWalker::after_finish(&self.map, self.legs(), node, &times, given);
            self.phase = MapPhase::Shown(LegsScreen::new(lead));
        }
        let MapPhase::Shown(screen) = &self.phase else {
            return MapStep::Stay;
        };
        // where the lead is or will be, so a button kept held across a
        // walk doesn't turn into a new press of another
        let node = screen.lead.heading().unwrap_or(screen.lead.node);
        let buttons = buttons(held, self.enterable(node));
        let mut screen = screen.clone();
        let out = screen.step(&self.map, self.legs(), &self.progress, buttons);
        self.phase = MapPhase::Shown(screen);
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
    pub fn finished(&mut self) {
        let MapPhase::Playing(node) = self.phase else {
            return;
        };
        let place = self.map.place(node);
        let again = self.progress.is_cleared(&place);
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
        };
    }

    /// Draws the map as it is after the frame last run.
    pub fn draw(&self, canvas: &mut dyn Surface) {
        canvas.clear(0);
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
        let left = (screen.lead.page(legs) * i64::from(legs.page_width)) as f64;
        let height = self.map.size.y.to_f64();
        let at = |p: clamber_level::Vec2| {
            let y = p.y.to_f64();
            let y = if legs.y_up { height - y } else { y };
            (
                (p.x.to_f64() - left) * scale,
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
        let (x, y) = at(screen.lead.position());
        let (bw, bh) = ((6.0 * scale) as i32, (10.0 * scale) as i32);
        let x = x.round() as i32 - bw / 2;
        canvas.fill_rect(x, y.round() as i32 - bh, bw, bh, shade(LEAD));
    }
}

/// The map's buttons for the actions `held`. Jump, Action and Pause are
/// enter buttons where they `enter`, and other buttons elsewhere; Swap
/// is always another button.
fn buttons(held: Actions, enter: bool) -> Buttons {
    let pairs = [
        (Action::Right, Buttons::RIGHT),
        (Action::Left, Buttons::LEFT),
        (Action::Up, Buttons::UP),
        (Action::Down, Buttons::DOWN),
        (Action::Swap, Buttons::other(0)),
    ];
    let presses = [Action::Jump, Action::Action, Action::Pause];
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
fn dim(c: Color, light: u32) -> Color {
    let full = clamber_worldmap::FULL;
    let channel = |shift: u32| ((c >> shift & 0xff) * light / full) << shift;
    channel(16) | channel(8) | channel(0)
}
