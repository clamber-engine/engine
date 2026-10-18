//! Swimming, inside a water region.
//!
//! A swimmer has no walking, running, jumping or rolling. Its speed
//! across eases toward `swim_speed` or 0, a Jump press is a stroke that
//! sets its upward speed, and otherwise it sinks under `sink_gravity`.
//! Facing follows Left or Right at once, or with `swim_turn_face` set,
//! changes partway through a turn of `swim_turn_time` frames that the
//! turn's first stroke can shorten. With `swim_fast_speed` set, Action and
//! a direction held on the frame before raise the target, and with
//! `sink_down_kept_on_up` Up straight after Down keeps the deeper sinking
//! limit.
//!
//! The ground acts on one point: the column holding `x` and the feet
//! line, `y` rounded up. The point is in the ground when the pixel
//! row just above the feet line is inside solid ground. How far in it is
//! counts to the ground's underside if it hangs (a ceiling to the
//! swimmer), and otherwise to its top (a floor). In a level where no
//! ground is marked as hanging, it counts to the nearer of the two, and
//! that says which it is. Moving across
//! into ground no more than `swim_step` deep is a step up onto it, or a
//! push down to `swim_ceiling_gap` below a ceiling; deeper is a wall.
//! Rising into ground stops the rise, and sinking onto a floor rests on
//! it.
//!
//! With `swim_ground_below` the ground acts instead by the ground below
//! the whole px of the position, as in Classic 2: up to `swim_step` above
//! it is a step, deeper a wall that drops the fraction of `x`; landing
//! puts the feet on the surface, from `slope_landing_reach` above it on a
//! sloped piece; rising into ground that isn't one-way is a bump; and
//! nothing pushes the swimmer down. `swim_screen_margin` holds it inside
//! the picture.
//!
//! In a trace (`swimming.md`, "In a trace") every swimming frame is
//! `swim`, a stroke is `stroke`, and coming onto and leaving the floor
//! without a stroke are `land` and `leave_ground`.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::Level;
use clamber_pack::{Param, Physics};

use crate::columns::{self, column};
use crate::player::{Facing, Player, State, ceil, ease};
use crate::world::Event;

/// How far a point is in the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Depth {
    /// Whole px to move up or down to be out of it.
    px: i64,
    /// It's nearer the ground's underside, so out of it is down.
    ceiling: bool,
    collider: usize,
}

/// A swimmer's turn under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SwimTurn {
    /// Frames since the frame it started on.
    pub age: u8,
    /// The frame of the turn its first stroke came on, if one has.
    pub stroke: Option<u8>,
    /// Facing has changed.
    pub turned: bool,
}

/// After a stroke on frame `n` of a turn, where a new turn can start
/// from, by `n` - 1.
const STROKE_FREE: [Param; 11] = [
    Param::SwimTurnStrokeFree1,
    Param::SwimTurnStrokeFree2,
    Param::SwimTurnStrokeFree3,
    Param::SwimTurnStrokeFree4,
    Param::SwimTurnStrokeFree5,
    Param::SwimTurnStrokeFree6,
    Param::SwimTurnStrokeFree7,
    Param::SwimTurnStrokeFree8,
    Param::SwimTurnStrokeFree9,
    Param::SwimTurnStrokeFree10,
    Param::SwimTurnStrokeFree11,
];

/// How far a swimmer's point in `column` with its feet on line `feet` is
/// in `level`'s ground, if it is: whole px, and whether that's to a
/// ceiling (out of it is down) rather than a floor.
pub fn swim_depth(level: &Level, column: i64, feet: i64) -> Option<(i64, bool)> {
    depth(level, column, feet).map(|d| (d.px, d.ceiling))
}

/// The whole number `n` as a [`Fixed`].
fn px(n: i64) -> Fixed {
    columns::left_edge(n)
}

/// How far the point in `column` with its feet on line `feet` is in the
/// ground, if it is. The piece of ground at the point reaches up through
/// the floors overlapping or touching it, and down through all the
/// ground: hanging ground resting on a floor has no way out below until
/// the floor's underside.
fn depth(level: &Level, column: i64, feet: i64) -> Option<Depth> {
    let (row_top, row_bottom) = (px(feet - 1), px(feet));
    let mut spans: Vec<_> = columns::spans(level, column)
        .into_iter()
        .filter(|s| s.top < s.bottom)
        .collect();
    spans.sort_by_key(|s| (s.top, s.bottom));
    let at = *spans
        .iter()
        .find(|s| s.top < row_bottom && s.bottom > row_top)?;
    let (mut top, mut bottom) = (at.top, at.bottom);
    let mut collider = at.collider;
    while let Some(s) = spans
        .iter()
        .find(|s| !s.hangs && !at.hangs && s.top < top && s.bottom >= top)
    {
        top = s.top;
        collider = s.collider;
    }
    while let Some(s) = spans.iter().find(|s| s.top <= bottom && s.bottom > bottom) {
        bottom = s.bottom;
    }
    let up = ceil(row_bottom - top);
    let down = ceil(bottom - row_top);
    let ceiling = if level.collision.hanging() {
        at.hangs
    } else {
        down < up
    };
    Some(Depth {
        px: if ceiling { down } else { up },
        ceiling,
        collider,
    })
}

/// The highest floor top in `column` from line `from` down to just above
/// line `to` + 1, solid or a segment, if any.
fn floor_between(level: &Level, column: i64, from: i64, to: i64) -> Option<(Fixed, usize)> {
    columns::spans(level, column)
        .into_iter()
        .filter(|s| s.top >= px(from) && s.top < px(to + 1))
        .min_by_key(|s| s.top)
        .map(|s| (s.top, s.collider))
}

/// The ground below a swimmer's point (swimming.md, "Walls, floors and
/// ceilings (`title-2`)").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Below {
    /// The surface's line, whole px: above the feet line if the point is
    /// in the ground.
    line: i64,
    collider: usize,
    /// Passable from below.
    one_way: bool,
}

/// The ground below the point in `column` with its feet on line `feet`:
/// if the pixel row just above the feet line is in the ground, its top,
/// reaching up through ground on it; otherwise the nearest top at the
/// feet line or below.
fn below(level: &Level, column: i64, feet: i64) -> Option<Below> {
    let (row_top, row_bottom) = (px(feet - 1), px(feet));
    let mut spans = columns::spans(level, column);
    spans.sort_by_key(|s| (s.top, s.bottom));
    let at = spans
        .iter()
        .find(|s| s.top < s.bottom && s.top < row_bottom && s.bottom > row_top);
    let found = match at {
        Some(&at) => {
            let mut top = at;
            while let Some(&s) = spans
                .iter()
                .find(|s| s.top < top.top && s.bottom >= top.top)
            {
                top = s;
            }
            top
        }
        None => *spans.iter().find(|s| s.top >= row_bottom)?,
    };
    Some(Below {
        line: found.top.floor(),
        collider: found.collider,
        one_way: found.one_way,
    })
}

/// `v` with its whole px part `n`, its fraction kept.
fn with_whole(v: Fixed, n: i64) -> Fixed {
    px(n) + (v - px(v.floor()))
}

impl Player {
    /// Whether the player at `pos` in `level` is in water.
    pub(crate) fn in_water(level: &Level, pos: clamber_level::Vec2) -> bool {
        level.regions_at(pos).any(|r| r.water)
    }

    /// One frame of swimming, with `dir` the direction held.
    pub(crate) fn swim(
        &mut self,
        level: &Level,
        physics: &Physics,
        input: &Frame,
        dir: i32,
        picture: Option<i64>,
        events: &mut Vec<Event>,
    ) {
        let held = |a| input.held.contains(a);
        if self.state != State::Swim {
            self.start_swimming();
        }

        // Speed across: faster after a frame with Action and a direction,
        // if the feel has a faster speed.
        let fast = std::mem::replace(&mut self.swim_fast, held(Action::Action) && dir != 0);
        let speed = match physics[Param::SwimFastSpeed] {
            s if fast && s > Fixed::ZERO => s,
            _ => physics[Param::SwimSpeed],
        };
        let target = speed * Fixed::from_int(dir);
        self.vel.x = ease(self.vel.x, target, physics[Param::SwimResponse]);
        self.target = target;

        // A stroke, then sinking.
        let stroke = input.pressed.contains(Action::Jump);
        if stroke {
            events.push(Event::Stroke);
            let speed = if held(Action::Up) {
                physics[Param::StrokeUpSpeed]
            } else if held(Action::Down) {
                physics[Param::StrokeDownSpeed]
            } else {
                physics[Param::StrokeSpeed]
            };
            self.vel.y = -speed;
        }
        self.vel.y += physics[Param::SinkGravity];
        let kept = physics[Param::SinkDownKeptOnUp] > Fixed::ZERO;
        let deep = held(Action::Down) || kept && held(Action::Up) && self.swim_deep;
        self.swim_deep = deep;
        let limit = if deep {
            physics[Param::SinkDownSpeed]
        } else {
            physics[Param::SinkSpeed]
        };
        if self.vel.y > Fixed::ZERO {
            self.vel.y = self.vel.y.min(limit);
        }

        let was_on_floor = self.swim_floor;
        let stopped = if physics[Param::SwimGroundBelow] > Fixed::ZERO {
            self.swim_by_ground_below(level, physics, picture)
        } else {
            self.swim_by_depth(level, physics)
        };

        if self.swim_floor && !was_on_floor {
            events.push(Event::Land);
        } else if was_on_floor && !self.swim_floor && !stroke {
            events.push(Event::LeaveGround);
        }

        if physics[Param::SwimTurnFace] > Fixed::ZERO {
            self.swim_turn(physics, dir, stroke, events);
        } else if !stopped && dir != 0 {
            let facing = if dir < 0 { Facing::Left } else { Facing::Right };
            if facing != self.facing {
                self.facing = facing;
                events.push(Event::Turn);
            }
        }
    }

    /// Moves a swimmer by its speeds against the ground below its point,
    /// then holds it inside the picture whose left edge is at `picture`.
    /// Returns whether it was stopped across, by a wall.
    fn swim_by_ground_below(
        &mut self,
        level: &Level,
        physics: &Physics,
        picture: Option<i64>,
    ) -> bool {
        // Across, by the ground at the point the move would reach.
        let mut stopped = false;
        let feet = self.pos.y.floor();
        let to = self.pos.x + self.vel.x;
        match below(level, column(to), feet) {
            Some(g) if feet - g.line > physics[Param::SwimStep].floor() => {
                self.pos.x = px(self.pos.x.floor());
                self.vel.x = Fixed::ZERO;
                stopped = true;
                self.touched.push(g.collider);
            }
            Some(g) if feet > g.line => {
                self.pos.x = to;
                self.pos.y = with_whole(self.pos.y, g.line);
                self.touched.push(g.collider);
            }
            _ => self.pos.x = to,
        }

        // Up and down.
        let was_on_floor = std::mem::take(&mut self.swim_floor);
        let before = self.pos.y.floor();
        self.pos.y += self.vel.y;
        let (x, feet) = (column(self.pos.x), self.pos.y.floor());
        let g = below(level, x, feet);
        if !self.vel.y.is_negative() {
            let reach = match physics[Param::SlopePiece].floor() {
                piece if piece > 0 => g.map_or(0, |g| {
                    let slope = physics[Param::FaceSlope];
                    match columns::sloped(level, x, px(g.line), slope, piece) {
                        true => physics[Param::SlopeLandingReach].floor(),
                        false => 0,
                    }
                }),
                _ => 0,
            };
            match g {
                Some(g) if feet >= g.line - reach => {
                    self.pos.y = px(g.line);
                    self.vel.y = Fixed::ZERO;
                    self.swim_floor = true;
                    self.touched.push(g.collider);
                }
                _ if was_on_floor => {
                    let leave = physics[Param::SwimLeaveSpeed];
                    if leave > Fixed::ZERO {
                        self.vel.y = leave;
                    }
                }
                _ => {}
            }
        } else if let Some(g) = g.filter(|g| feet > g.line && !g.one_way) {
            // A bump: back to the whole px it rose from, the fraction of
            // the move kept.
            self.pos.y = with_whole(self.pos.y, before);
            self.vel.y = Fixed::ZERO;
            self.touched.push(g.collider);
        }

        // Held inside the picture.
        let margin = physics[Param::SwimScreenMargin].floor();
        if let Some(left) = picture.filter(|_| margin > 0) {
            let right = left + physics[Param::ViewWidth].floor() - margin;
            let x = self.pos.x.floor();
            if x < left + margin {
                self.pos.x = with_whole(self.pos.x, left + margin);
            } else if x > right {
                self.pos.x = with_whole(self.pos.x, right);
            }
        }
        stopped
    }

    /// Moves a swimmer by its speeds against the ground by how deep its
    /// point is in it. Returns whether it was stopped across.
    fn swim_by_depth(&mut self, level: &Level, physics: &Physics) -> bool {
        // The left edge.
        let mut stopped = false;
        if self.vel.x.is_negative() && column(self.pos.x) < physics[Param::SwimLeftEdge].floor() {
            self.vel.x = Fixed::ZERO;
            stopped = true;
        }

        // Across.
        let step = physics[Param::SwimStep].floor();
        let feet = ceil(self.pos.y);
        match depth(level, column(self.pos.x), feet) {
            None => {
                let to = self.pos.x + self.vel.x;
                match depth(level, column(to), feet) {
                    None => self.pos.x = to,
                    Some(d) if d.px <= step => {
                        self.pos.x = to;
                        self.pos.y += if d.ceiling {
                            px(d.px + physics[Param::SwimCeilingGap].floor())
                        } else {
                            -px(d.px)
                        };
                        self.touched.push(d.collider);
                    }
                    Some(d) => {
                        // A wall: `x` stays in its column, with the
                        // fraction it would have had.
                        let fraction = to - px(column(to));
                        self.pos.x = px(column(self.pos.x)) + fraction;
                        self.vel.x = Fixed::ZERO;
                        stopped = true;
                        self.touched.push(d.collider);
                    }
                }
            }
            Some(d) if d.px <= step => self.pos.y -= px(d.px),
            Some(_) => {}
        }

        // Up and down.
        let was_on_floor = std::mem::take(&mut self.swim_floor);
        let feet = ceil(self.pos.y);
        let x = column(self.pos.x);
        if depth(level, x, feet).is_some() {
            self.pos.y += self.vel.y;
        } else if self.vel.y > Fixed::ZERO {
            self.pos.y += self.vel.y;
            let now = ceil(self.pos.y);
            if let Some((top, collider)) = floor_between(level, x, feet, now) {
                // Resting on the floor: the feet line on its top line,
                // the fraction kept.
                self.pos.y += px(top.floor() - now);
                self.vel.y = Fixed::ZERO;
                self.swim_floor = true;
                self.touched.push(collider);
            } else if was_on_floor {
                let leave = physics[Param::SwimLeaveSpeed];
                if leave > Fixed::ZERO {
                    self.vel.y = leave;
                }
            }
        } else {
            self.pos.y += self.vel.y;
            let now = ceil(self.pos.y);
            if let Some(d) = depth(level, x, now) {
                // A bump: back down to the feet line it rose from, the
                // fraction kept.
                self.pos.y += px(feet - now);
                self.vel.y = Fixed::ZERO;
                self.touched.push(d.collider);
            }
        }
        stopped
    }

    /// One frame of a swimmer's facing by turns, with `dir` the direction
    /// held and `stroke` whether this frame has a stroke.
    fn swim_turn(&mut self, physics: &Physics, dir: i32, stroke: bool, events: &mut Vec<Event>) {
        let face = physics[Param::SwimTurnFace].floor();
        if let Some(mut turn) = self.swim_turn {
            turn.age = turn.age.saturating_add(1);
            if stroke && turn.stroke.is_none() {
                turn.stroke = Some(turn.age);
            }
            let (face_on, free_from) = match turn.stroke {
                Some(n) => {
                    let n = i64::from(n);
                    let soonest = physics[Param::SwimTurnStrokeFace].floor();
                    let face_on = if soonest > 0 && n < face {
                        (n + 1).min(soonest)
                    } else {
                        face
                    };
                    let free = STROKE_FREE
                        .get(n as usize - 1)
                        .map_or(0, |&p| physics[p].floor());
                    let free = if free > 0 {
                        free
                    } else {
                        physics[Param::SwimTurnTime].floor()
                    };
                    (face_on, free)
                }
                None => (face, physics[Param::SwimTurnTime].floor()),
            };
            let age = i64::from(turn.age);
            if !turn.turned && age >= face_on {
                self.facing = match self.facing {
                    Facing::Left => Facing::Right,
                    Facing::Right => Facing::Left,
                };
                turn.turned = true;
                events.push(Event::Turn);
            }
            self.swim_turn = (age < free_from).then_some(turn);
        }
        let opposite = match self.facing {
            Facing::Left => dir > 0,
            Facing::Right => dir < 0,
        };
        if self.swim_turn.is_none() && opposite {
            self.swim_turn = Some(SwimTurn {
                age: 0,
                stroke: None,
                turned: false,
            });
        }
    }

    /// Leaves whatever the player was doing on land.
    fn start_swimming(&mut self) {
        self.state = State::Swim;
        self.ground = None;
        self.coyote = None;
        self.takeoff = false;
        self.takeoff_wait = None;
        self.turn = None;
        self.roll = None;
        self.roll_stop = 0;
        self.stopped = None;
        self.ground_action = None;
        self.roll_press = None;
        self.jump_age = None;
        self.since_land = None;
        self.swim_floor = false;
        self.swim_fast = false;
        self.swim_deep = false;
        self.swim_turn = None;
    }
}
