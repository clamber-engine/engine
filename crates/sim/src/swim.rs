//! Swimming, inside a water region.
//!
//! A swimmer has no walking, running, jumping or rolling. Its speed
//! across eases toward `swim_speed` or 0, a Jump press is a stroke that
//! sets its upward speed, and otherwise it sinks under `sink_gravity`.
//! Facing follows Left or Right at once.
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
        events: &mut Vec<Event>,
    ) {
        let held = |a| input.held.contains(a);
        if self.state != State::Swim {
            self.start_swimming();
        }

        // Speed across.
        let target = physics[Param::SwimSpeed] * Fixed::from_int(dir);
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
        let limit = if held(Action::Down) {
            physics[Param::SinkDownSpeed]
        } else {
            physics[Param::SinkSpeed]
        };
        if self.vel.y > Fixed::ZERO {
            self.vel.y = self.vel.y.min(limit);
        }

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

        if self.swim_floor && !was_on_floor {
            events.push(Event::Land);
        } else if was_on_floor && !self.swim_floor && !stroke {
            events.push(Event::LeaveGround);
        }

        if !stopped && dir != 0 {
            let facing = if dir < 0 { Facing::Left } else { Facing::Right };
            if facing != self.facing {
                self.facing = facing;
                events.push(Event::Turn);
            }
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
    }
}
