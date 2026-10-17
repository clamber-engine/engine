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
//! counts to the nearer of the ground's top and its underside, which also
//! says whether it's a floor or a ceiling to the swimmer. Moving across
//! into ground no more than `swim_step` deep is a step up onto it, or a
//! push down to `swim_ceiling_gap` below a ceiling; deeper is a wall.
//! Rising into ground stops the rise, and sinking onto a floor rests on
//! it.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::Level;
use clamber_pack::{Param, Physics};

use crate::columns::{self, column};
use crate::player::{Facing, Player, State, ceil, ease};

/// How far a point is in the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Depth {
    /// Whole px to move up or down to be out of it.
    px: i64,
    /// It's nearer the ground's underside, so out of it is down.
    ceiling: bool,
    collider: usize,
}

/// The whole number `n` as a [`Fixed`].
fn px(n: i64) -> Fixed {
    columns::left_edge(n)
}

/// The solid ground in `column`, from the top down, with overlapping and
/// touching pieces joined: `(top, bottom, collider of the top piece)`.
fn solids(level: &Level, column: i64) -> Vec<(Fixed, Fixed, usize)> {
    let mut spans: Vec<_> = columns::spans(level, column)
        .into_iter()
        .filter(|s| s.top < s.bottom)
        .collect();
    spans.sort_by_key(|s| (s.top, s.bottom));
    let mut joined: Vec<(Fixed, Fixed, usize)> = Vec::new();
    for s in spans {
        match joined.last_mut() {
            Some(last) if s.top <= last.1 => last.1 = last.1.max(s.bottom),
            _ => joined.push((s.top, s.bottom, s.collider)),
        }
    }
    joined
}

/// How far the point in `column` with its feet on line `feet` is in the
/// ground, if it is.
fn depth(level: &Level, column: i64, feet: i64) -> Option<Depth> {
    let (row_top, row_bottom) = (px(feet - 1), px(feet));
    let (top, bottom, collider) = solids(level, column)
        .into_iter()
        .find(|&(top, bottom, _)| top < row_bottom && bottom > row_top)?;
    let up = ceil(row_bottom - top);
    let down = ceil(bottom - row_top);
    Some(Depth {
        px: up.min(down),
        ceiling: down < up,
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
    pub(crate) fn swim(&mut self, level: &Level, physics: &Physics, input: &Frame, dir: i32) {
        let held = |a| input.held.contains(a);
        if self.state != State::Swim {
            self.start_swimming();
        }

        // Speed across.
        let target = physics[Param::SwimSpeed] * Fixed::from_int(dir);
        self.vel.x = ease(self.vel.x, target, physics[Param::SwimResponse]);
        self.target = target;

        // A stroke, then sinking.
        if input.pressed.contains(Action::Jump) {
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

        if !stopped && dir != 0 {
            self.facing = if dir < 0 { Facing::Left } else { Facing::Right };
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
