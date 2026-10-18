//! Walking a legs map ([`Mode::Legs`](crate::Mode::Legs)) frame by frame.
//!
//! The lead stands at a node. On a frame it may start a walk, the
//! direction coming from the buttons held and the latest new press,
//! which is remembered for a while. A walk follows one path in straight
//! legs, corner to corner, each leg taking about the same number of
//! frames whatever its length. The map is shown a page at a time; a walk
//! onto another page changes the page, costs a lag frame and holds the
//! next walk back for a while, as appearing on the map does. A new press
//! of an enter button at a node ends the walker's part: the map fades out
//! and the caller does what the node is for.
//!
//! After a level is finished the map appears through
//! [`LegWalker::after_finish`]: it may hold still for a while, no walk
//! starts before a gate, and on the gate the direction toward the next
//! level is given as if just pressed. A new game begins with
//! [`LegWalker::new_game`]: after a while the lead walks off on its own.
//!
//! A [`LegsScreen`] runs the walker as the map is shown: entering a warp
//! to another node of the same map fades out and shows the map again
//! with the lead there; entering anything else leaves the map.

use clamber_level::{Fixed, Vec2};

use crate::map::{NodeKind, WorldMap};
use crate::progress::Progress;

/// A direction a path can be taken in, in the order they're chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    Right,
    Left,
    Up,
    Down,
}

impl Dir {
    /// When several are held or remembered, the first of these counts.
    pub const ORDER: [Dir; 4] = [Dir::Right, Dir::Left, Dir::Up, Dir::Down];

    pub fn name(self) -> &'static str {
        match self {
            Dir::Right => "right",
            Dir::Left => "left",
            Dir::Up => "up",
            Dir::Down => "down",
        }
    }

    pub fn parse(name: &str) -> Option<Dir> {
        Dir::ORDER.into_iter().find(|d| d.name() == name)
    }

    pub fn button(self) -> Buttons {
        match self {
            Dir::Right => Buttons::RIGHT,
            Dir::Left => Buttons::LEFT,
            Dir::Up => Buttons::UP,
            Dir::Down => Buttons::DOWN,
        }
    }
}

/// The buttons held on a frame: the four directions, up to eight enter
/// buttons, and up to eight others. Only directions choose a path; a new
/// press of any button replaces a remembered press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u32);

impl Buttons {
    pub const NONE: Buttons = Buttons(0);
    pub const RIGHT: Buttons = Buttons(1);
    pub const LEFT: Buttons = Buttons(2);
    pub const UP: Buttons = Buttons(4);
    pub const DOWN: Buttons = Buttons(8);

    /// Another button, `n` from 0 to 7.
    pub const fn other(n: u8) -> Buttons {
        assert!(n < 8);
        Buttons(16 << n)
    }

    /// An enter button, `n` from 0 to 7: a new press of one at a node
    /// enters it. Several are kept apart so that pressing one while
    /// another is held is still a new press.
    pub const fn enter(n: u8) -> Buttons {
        assert!(n < 8);
        Buttons(0x1000 << n)
    }

    /// All the enter buttons.
    pub const ENTER: Buttons = Buttons(0xf_f000);

    /// Whether any of `b` is among these.
    pub fn any(self, b: Buttons) -> bool {
        self.0 & b.0 != 0
    }

    pub fn contains(self, b: Buttons) -> bool {
        self.0 & b.0 == b.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The buttons held now that weren't held on the frame `before`.
    pub fn newly(self, before: Buttons) -> Buttons {
        Buttons(self.0 & !before.0)
    }

    /// The first direction among these, in [`Dir::ORDER`].
    pub fn direction(self) -> Option<Dir> {
        Dir::ORDER.into_iter().find(|d| self.contains(d.button()))
    }
}

impl std::ops::BitOr for Buttons {
    type Output = Buttons;
    fn bitor(self, rhs: Buttons) -> Buttons {
        Buttons(self.0 | rhs.0)
    }
}

/// How a legs map is walked.
#[derive(Clone, Debug, PartialEq)]
pub struct Legs {
    /// Each leg's speed on each axis is its length divided by this, so a
    /// leg takes about this many frames. A power of two.
    pub leg_frames: u32,
    /// Frames a new press is remembered, counting the frame it's made on.
    pub remember: u32,
    /// The map is shown in pages this wide, side by side.
    pub page_width: u32,
    /// After a walk changes the page, no walk starts for this many frames
    /// from that frame.
    pub page_wait: u32,
    /// Where the lead stands, from its node, when the map appears.
    pub appear_offset: Vec2,
    /// After the map appears, no walk starts for this many frames.
    pub appear_wait: u32,
    /// The map's y grows upward; for drawing. Walking uses the values as
    /// they are.
    pub y_up: bool,
    /// The map fades in from black as it appears, and out after an enter
    /// press, one step of brightness (0 to [`FULL`]) every this many
    /// frames.
    pub fade_step: u32,
    /// How the map appears after a level is finished, if it differs from
    /// appearing otherwise.
    pub finish: Option<Finish>,
    /// How a new game begins, if the lead walks off on its own.
    pub new_game: Option<NewGame>,
}

/// A new game: the map appears with the lead at the map's start node, and
/// on frame `walk` it walks off `toward` with no input. Nothing held or
/// pressed before then, or on that frame, changes that.
#[derive(Clone, Debug, PartialEq)]
pub struct NewGame {
    pub walk: u32,
    pub toward: Dir,
}

/// Full brightness; 0 is black.
pub const FULL: u32 = 15;

/// The map after a level is finished, the first time and again.
#[derive(Clone, Debug, PartialEq)]
pub struct Finish {
    pub first: FinishTimes,
    pub again: FinishTimes,
}

/// When things happen on the map after a level is finished, in frames
/// counted from the map's first frame.
#[derive(Clone, Debug, PartialEq)]
pub struct FinishTimes {
    /// Black frames between the level's last frame and the map's first.
    pub black: u32,
    /// Frames on which the map holds still, nothing on it happening:
    /// the first one and how many.
    pub still: Option<(u32, u32)>,
    /// No walk starts before this frame. On it, the direction toward the
    /// next level counts as newly pressed.
    pub gate: u32,
}

/// What happened on a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// A walk started along this path.
    pub started: Option<usize>,
    /// A walk ended at this node: this was its last frame.
    pub arrived: Option<usize>,
    /// The page shown changed to this one. The next frame is a lag frame.
    pub page: Option<i64>,
    /// A lag frame: nothing happened, the buttons weren't read, and the
    /// frame isn't counted.
    pub lag: bool,
    /// A still frame: nothing happened and the buttons weren't read.
    pub still: bool,
    /// An enter button was newly pressed with the lead standing at this
    /// node. The map fades out and the walker does nothing more.
    pub entered: Option<usize>,
}

/// A walk under way.
#[derive(Clone, Debug, PartialEq)]
struct Walk {
    path: usize,
    to: usize,
    points: Vec<Vec2>,
    /// The leg being walked: towards `points[leg]`.
    leg: usize,
    speed: Vec2,
    arrived: [bool; 2],
}

/// The lead on a legs map.
#[derive(Clone, Debug, PartialEq)]
pub struct LegWalker {
    /// The node the lead stands at, or last stood at while walking.
    pub node: usize,
    /// Whole pixels and fractions; the fractions carry from walk to walk.
    position: Vec2,
    walk: Option<Walk>,
    remembered: Buttons,
    /// The remembered press is forgotten from this frame.
    remembered_until: u64,
    held_before: Buttons,
    /// Frames counted since the map appeared; lag frames aren't.
    frame: u64,
    no_walk_before: u64,
    lag: bool,
    /// Still frames: from, up to but not including.
    still: (u64, u64),
    /// On this frame this direction counts as newly pressed.
    given: Option<(u64, Dir)>,
    /// The frame of the enter press.
    entered: Option<u64>,
    /// On this frame a walk this way starts, whatever is held or pressed.
    forced: Option<(u64, Dir)>,
}

fn axis(v: Vec2, i: usize) -> Fixed {
    if i == 0 { v.x } else { v.y }
}

fn set_axis(v: &mut Vec2, i: usize, n: Fixed) {
    if i == 0 {
        v.x = n;
    } else {
        v.y = n;
    }
}

fn whole(n: Fixed) -> Fixed {
    Fixed::from_bits(n.to_bits() & !(Fixed::ONE.to_bits() - 1))
}

impl LegWalker {
    /// The lead as the map appears, standing at `node`'s appearance point
    /// with no fractions.
    pub fn appear(map: &WorldMap, legs: &Legs, node: usize) -> LegWalker {
        let at = map.nodes[node].at;
        let position = Vec2::new(
            whole(at.x + legs.appear_offset.x),
            whole(at.y + legs.appear_offset.y),
        );
        LegWalker {
            node,
            position,
            walk: None,
            remembered: Buttons::NONE,
            remembered_until: 0,
            held_before: Buttons::NONE,
            frame: 0,
            no_walk_before: u64::from(legs.appear_wait),
            lag: false,
            still: (0, 0),
            given: None,
            entered: None,
            forced: None,
        }
    }

    /// The lead as the map appears for a new game, at `node` (the map's
    /// start), walking off as `n` says.
    pub fn new_game(map: &WorldMap, legs: &Legs, node: usize, n: &NewGame) -> LegWalker {
        let mut lead = LegWalker::appear(map, legs, node);
        lead.no_walk_before = u64::from(n.walk);
        lead.forced = Some((u64::from(n.walk), n.toward));
        lead
    }

    /// The lead as the map appears after the level at `node` is finished,
    /// with the times `t` (from [`Legs::finish`]). `given` is the direction
    /// toward the next level, if there is one.
    pub fn after_finish(
        map: &WorldMap,
        legs: &Legs,
        node: usize,
        t: &FinishTimes,
        given: Option<Dir>,
    ) -> LegWalker {
        let mut lead = LegWalker::appear(map, legs, node);
        lead.no_walk_before = u64::from(t.gate);
        if let Some((from, n)) = t.still {
            lead.still = (u64::from(from), u64::from(from) + u64::from(n));
        }
        lead.given = given.map(|d| (u64::from(t.gate), d));
        lead
    }

    pub fn position(&self) -> Vec2 {
        self.position
    }

    /// Puts the lead at `position`, fractions included, as when restoring
    /// a saved game.
    pub fn set_position(&mut self, position: Vec2) {
        self.position = position;
    }

    pub fn walking(&self) -> bool {
        self.walk.is_some()
    }

    /// The node the lead is walking to, if it's walking.
    pub fn heading(&self) -> Option<usize> {
        self.walk.as_ref().map(|w| w.to)
    }

    /// Frames counted since the map appeared.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Whether an enter button has been pressed at a node.
    pub fn entered(&self) -> bool {
        self.entered.is_some()
    }

    /// How bright the map is on the frame last run, from 0 (black) to
    /// [`FULL`]: fading in from the map's first frame, and out from the
    /// frame of the enter press.
    pub fn brightness(&self, legs: &Legs) -> u32 {
        let step = u64::from(legs.fade_step.max(1));
        let frame = self.frame.saturating_sub(1);
        let level = match self.entered {
            None => frame / step,
            Some(at) => u64::from(FULL).saturating_sub((frame - at) / step),
        };
        level.min(u64::from(FULL)) as u32
    }

    /// The page holding the lead's whole-pixel x.
    pub fn page(&self, legs: &Legs) -> i64 {
        self.position
            .x
            .floor()
            .div_euclid(i64::from(legs.page_width))
    }

    /// Runs a frame with `held` held.
    pub fn step(&mut self, map: &WorldMap, legs: &Legs, p: &Progress, held: Buttons) -> Step {
        if self.lag {
            self.lag = false;
            return Step {
                lag: true,
                ..Step::default()
            };
        }
        let mut step = Step::default();
        let frame = self.frame;
        if self.entered.is_some() {
            self.frame += 1;
            return step;
        }
        if (self.still.0..self.still.1).contains(&frame) {
            self.frame += 1;
            step.still = true;
            return step;
        }
        let new = held.newly(self.held_before);
        self.held_before = held;
        if !new.is_empty() {
            self.remembered = new;
            self.remembered_until = frame + u64::from(legs.remember);
        }
        // the given direction replaces whatever was pressed on its frame
        let given = self.given.filter(|&(at, _)| at == frame);
        if let Some((_, dir)) = given {
            self.remembered = dir.button();
            self.remembered_until = frame + u64::from(legs.remember);
        }
        let page = self.page(legs);
        let forced = self.forced.filter(|&(at, _)| at == frame).map(|(_, d)| d);

        if self.walk.is_none()
            && frame >= self.no_walk_before
            && given.is_none()
            && forced.is_none()
            && new.any(Buttons::ENTER)
        {
            // entering beats any direction
            self.entered = Some(frame);
            self.frame += 1;
            step.entered = Some(self.node);
            return step;
        }

        if self.walk.is_none() && frame >= self.no_walk_before {
            let mut buttons = held;
            if frame < self.remembered_until {
                buttons = buttons | self.remembered;
            }
            // a direction without a path blocks those after it
            if let Some((path, to, points)) = forced
                .or_else(|| buttons.direction())
                .and_then(|dir| map.leg_route(self.node, dir, p))
            {
                let mut walk = Walk {
                    path,
                    to,
                    points,
                    leg: 0,
                    speed: Vec2::ints(0, 0),
                    arrived: [false; 2],
                };
                let start = Vec2::new(whole(self.position.x), whole(self.position.y));
                Self::begin_leg(&mut walk, start, legs);
                self.walk = Some(walk);
                step.started = Some(path);
            }
        }

        if let Some(walk) = &mut self.walk {
            let end = walk.points[walk.leg];
            for i in 0..2 {
                if walk.arrived[i] {
                    continue;
                }
                let speed = axis(walk.speed, i);
                let n = axis(self.position, i) + speed;
                let target = axis(end, i).floor();
                let n = if speed > Fixed::ZERO {
                    walk.arrived[i] = n.floor() >= target;
                    n
                } else if n.floor() < target {
                    walk.arrived[i] = true;
                    n - whole(n) + whole(axis(end, i))
                } else {
                    n
                };
                set_axis(&mut self.position, i, n);
            }
            if walk.arrived == [true; 2] {
                if walk.leg + 1 == walk.points.len() {
                    self.node = walk.to;
                    step.arrived = Some(walk.to);
                    self.walk = None;
                } else {
                    walk.leg += 1;
                    Self::begin_leg(walk, end, legs);
                }
            }
        }

        let now = self.page(legs);
        if now != page {
            step.page = Some(now);
            self.no_walk_before = self.no_walk_before.max(frame + u64::from(legs.page_wait));
            self.lag = true;
        }
        self.frame += 1;
        step
    }

    /// Sets the speeds of the walk's leg `walk.leg`, which starts at
    /// `start`.
    fn begin_leg(walk: &mut Walk, start: Vec2, legs: &Legs) {
        let end = walk.points[walk.leg];
        let frames = i64::from(legs.leg_frames);
        walk.speed = Vec2::new(
            Fixed::from_bits((end.x - start.x).to_bits() / frames),
            Fixed::from_bits((end.y - start.y).to_bits() / frames),
        );
        walk.arrived = [walk.speed.x == Fixed::ZERO, walk.speed.y == Fixed::ZERO];
    }
}

/// What a [`LegsScreen`] frame did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenStep {
    /// The walker's frame, if it ran one.
    pub step: Step,
    /// The map appeared again, the lead at this node, after a warp to it.
    pub appeared: Option<usize>,
    /// The map has faded out after this node was entered, and isn't shown
    /// any more: the caller does what the node is for.
    pub left: Option<usize>,
}

/// A legs map as the game shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct LegsScreen {
    pub lead: LegWalker,
    /// Frames still to run after an enter press before the map is gone,
    /// and the node entered.
    leaving: Option<(u32, usize)>,
    left: bool,
}

impl LegsScreen {
    /// The screen with `lead` on it.
    pub fn new(lead: LegWalker) -> LegsScreen {
        LegsScreen {
            lead,
            leaving: None,
            left: false,
        }
    }

    /// A new game on `map`: the lead at the map's start node, walking off
    /// on its own if the map says so. `None` if the map has no start.
    pub fn new_game(map: &WorldMap, legs: &Legs) -> Option<LegsScreen> {
        let node = map.node(map.start.as_deref()?)?;
        let lead = match &legs.new_game {
            Some(n) => LegWalker::new_game(map, legs, node, n),
            None => LegWalker::appear(map, legs, node),
        };
        Some(LegsScreen::new(lead))
    }

    /// Whether the map has faded out after an enter press and is gone.
    pub fn left(&self) -> bool {
        self.left
    }

    /// How bright the map is on the frame last run (see
    /// [`LegWalker::brightness`]).
    pub fn brightness(&self, legs: &Legs) -> u32 {
        if self.left {
            0
        } else {
            self.lead.brightness(legs)
        }
    }

    /// Runs a frame with `held` held. After an enter press the map fades
    /// out; on the frame after its black frames, a warp to a node of this
    /// map shows the map again with the lead there, as it appears, and
    /// anything else leaves the map.
    pub fn step(&mut self, map: &WorldMap, legs: &Legs, p: &Progress, held: Buttons) -> ScreenStep {
        let mut out = ScreenStep::default();
        if self.left {
            return out;
        }
        if let Some((0, node)) = self.leaving {
            self.leaving = None;
            let here = match &map.nodes[node].kind {
                NodeKind::Warp { map: m, node: to } if *m == map.id => map.node(to),
                _ => None,
            };
            let Some(to) = here else {
                self.left = true;
                out.left = Some(node);
                return out;
            };
            self.lead = LegWalker::appear(map, legs, to);
            out.appeared = Some(to);
        }
        out.step = self.lead.step(map, legs, p, held);
        if let Some((n, _)) = &mut self.leaving {
            if !out.step.lag {
                *n -= 1;
            }
        } else if let Some(node) = out.step.entered {
            // a step of brightness at a time down to black, and one step
            // black
            self.leaving = Some((legs.fade_step.max(1) * (FULL + 1) - 1, node));
        }
        out
    }
}
