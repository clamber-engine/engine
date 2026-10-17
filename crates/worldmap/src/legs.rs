//! Walking a legs map ([`Mode::Legs`](crate::Mode::Legs)) frame by frame.
//!
//! The lead stands at a node. On a frame it may start a walk, the
//! direction coming from the buttons held and the latest new press,
//! which is remembered for a while. A walk follows one path in straight
//! legs, corner to corner, each leg taking about the same number of
//! frames whatever its length. The map is shown a page at a time; a walk
//! onto another page changes the page, costs a lag frame and holds the
//! next walk back for a while, as appearing on the map does.

use clamber_level::{Fixed, Vec2};

use crate::map::WorldMap;
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

/// The buttons held on a frame: the four directions and up to four
/// others, which never choose a path but do replace a remembered press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u8);

impl Buttons {
    pub const NONE: Buttons = Buttons(0);
    pub const RIGHT: Buttons = Buttons(1);
    pub const LEFT: Buttons = Buttons(2);
    pub const UP: Buttons = Buttons(4);
    pub const DOWN: Buttons = Buttons(8);

    /// Another button, `n` from 0 to 3.
    pub const fn other(n: u8) -> Buttons {
        assert!(n < 4);
        Buttons(16 << n)
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
        }
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

    /// Frames counted since the map appeared.
    pub fn frame(&self) -> u64 {
        self.frame
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
        let new = held.newly(self.held_before);
        self.held_before = held;
        if !new.is_empty() {
            self.remembered = new;
            self.remembered_until = frame + u64::from(legs.remember);
        }
        let page = self.page(legs);

        if self.walk.is_none() && frame >= self.no_walk_before {
            let mut buttons = held;
            if frame < self.remembered_until {
                buttons = buttons | self.remembered;
            }
            // a direction without a path blocks those after it
            if let Some((path, to, points)) = buttons
                .direction()
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
