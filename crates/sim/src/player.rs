//! The player's body: position, velocity and contact with the level.
//!
//! Movement here is the minimum the core needs to be exercised: walking
//! under the physics in effect, gravity, landing, following slopes, and
//! stopping at walls and ceilings. The movement state machine (jumps,
//! running, rolls) builds on it.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::{Dir, Hit, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::world::Event;

/// Half the body's width, px. The body is a box standing on its
/// position, which is the middle of its feet.
pub const HALF_WIDTH: Fixed = Fixed::from_int(8);

/// The body's height, px.
pub const HEIGHT: Fixed = Fixed::from_int(32);

/// How far a grounded body follows the ground up or down in one step, px.
const STEP: Fixed = Fixed::from_int(8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Facing {
    Left,
    #[default]
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Player {
    /// Roster slot of the character being played.
    pub character: usize,
    /// The middle of the feet.
    pub pos: Vec2,
    /// px per frame, +y down.
    pub vel: Vec2,
    /// The collider being stood on, if any.
    pub ground: Option<usize>,
    pub facing: Facing,
}

impl Player {
    /// A player standing still at `pos`, on the ground if there is ground
    /// right under it.
    pub fn new(level: &Level, pos: Vec2) -> Player {
        let mut player = Player {
            character: 0,
            pos,
            vel: Vec2::default(),
            ground: None,
            facing: Facing::Right,
        };
        player.ground = level
            .collision
            .cast(pos, Dir::Down, Fixed::ZERO)
            .map(|hit| hit.collider);
        player
    }

    pub fn grounded(&self) -> bool {
        self.ground.is_some()
    }

    /// Advances the body one step under `physics`.
    pub fn update(
        &mut self,
        level: &Level,
        physics: &Physics,
        input: &Frame,
        events: &mut Vec<Event>,
    ) {
        let held = |a| input.held.contains(a);
        let dir = match (held(Action::Left), held(Action::Right)) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };

        let facing = match dir {
            -1 => Facing::Left,
            1 => Facing::Right,
            _ => self.facing,
        };
        if facing != self.facing {
            self.facing = facing;
            events.push(Event::Turn);
        }

        self.vel.x = horizontal_speed(self.vel.x, dir, self.grounded(), physics);
        self.move_horizontally(level);

        if self.grounded() {
            self.follow_ground(level, events);
        } else {
            self.vel.y = (self.vel.y + physics[Param::Gravity]).min(physics[Param::MaxFallSpeed]);
            self.move_vertically(level, events);
        }
    }

    fn move_horizontally(&mut self, level: &Level) {
        if self.vel.x == Fixed::ZERO {
            return;
        }
        let (cast_dir, distance) = if self.vel.x.is_negative() {
            (Dir::Left, -self.vel.x)
        } else {
            (Dir::Right, self.vel.x)
        };
        // Probe just above step height and just below the head, so low
        // bumps are stepped over and slopes are walked up.
        let wall = [STEP + Fixed::ONE, HEIGHT - Fixed::ONE]
            .into_iter()
            .filter_map(|h| {
                let from = Vec2::new(self.pos.x, self.pos.y - h);
                level.collision.cast(from, cast_dir, distance + HALF_WIDTH)
            })
            .filter(is_wall)
            .map(|hit| hit.distance - HALF_WIDTH)
            .min();
        let moved = match wall {
            Some(room) if room < distance => {
                self.vel.x = Fixed::ZERO;
                room.max(Fixed::ZERO)
            }
            _ => distance,
        };
        match cast_dir {
            Dir::Left => self.pos.x -= moved,
            _ => self.pos.x += moved,
        }
    }

    /// Keeps a grounded body on the ground as it moves over slopes and
    /// bumps, or lets it leave the ground when there's none within reach.
    fn follow_ground(&mut self, level: &Level, events: &mut Vec<Event>) {
        let from = Vec2::new(self.pos.x, self.pos.y - STEP);
        match level.collision.cast(from, Dir::Down, STEP + STEP) {
            Some(hit) => {
                self.pos.y = hit.point.y;
                self.vel.y = Fixed::ZERO;
                self.ground = Some(hit.collider);
            }
            None => {
                self.ground = None;
                events.push(Event::LeaveGround);
            }
        }
    }

    fn move_vertically(&mut self, level: &Level, events: &mut Vec<Event>) {
        if self.vel.y.is_negative() {
            let head = Vec2::new(self.pos.x, self.pos.y - HEIGHT);
            match level.collision.cast(head, Dir::Up, -self.vel.y) {
                Some(hit) => {
                    self.pos.y = hit.point.y + HEIGHT;
                    self.vel.y = Fixed::ZERO;
                }
                None => self.pos.y += self.vel.y,
            }
            return;
        }
        match level.collision.cast(self.pos, Dir::Down, self.vel.y) {
            Some(hit) => {
                self.pos.y = hit.point.y;
                self.vel.y = Fixed::ZERO;
                self.ground = Some(hit.collider);
                events.push(Event::Land);
            }
            None => self.pos.y += self.vel.y,
        }
    }
}

/// Edges steeper than 45 degrees block; shallower ones are walked on.
fn is_wall(hit: &Hit) -> bool {
    let (a, b) = hit.edge;
    (b.y - a.y).abs() > (b.x - a.x).abs()
}

/// The horizontal speed after one step of pressing `dir` (-1, 0 or 1).
fn horizontal_speed(vx: Fixed, dir: i32, grounded: bool, physics: &Physics) -> Fixed {
    let top = physics[Param::TopSpeed];
    if dir == 0 {
        if !grounded {
            return vx;
        }
        // Friction never reverses the motion.
        let friction = physics[Param::GroundFriction];
        return if vx.abs() <= friction {
            Fixed::ZERO
        } else if vx.is_negative() {
            vx + friction
        } else {
            vx - friction
        };
    }
    // Work as if pressing right, then mirror.
    let (v, flip) = if dir < 0 { (-vx, true) } else { (vx, false) };
    let v = if !grounded {
        v + physics[Param::AirAcceleration]
    } else if v.is_negative() {
        v + physics[Param::GroundDeceleration]
    } else {
        v + physics[Param::GroundAcceleration]
    };
    // Acceleration alone doesn't pass top speed, but doesn't take away
    // speed gained some other way either.
    let limit = top.max(if flip { -vx } else { vx });
    let v = v.min(limit);
    if flip { -v } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(x: f64) -> Fixed {
        Fixed::from_f64(x).unwrap()
    }

    fn physics() -> Physics {
        clamber_pack::Presets::builtin().world("default").unwrap()
    }

    #[test]
    fn accelerates_to_top_speed_and_stops_with_friction() {
        let p = physics();
        let top = p[Param::TopSpeed];
        let mut v = Fixed::ZERO;
        for _ in 0..1000 {
            v = horizontal_speed(v, 1, true, &p);
        }
        assert_eq!(v, top);
        assert_eq!(horizontal_speed(-v, -1, true, &p), -top);
        for _ in 0..1000 {
            v = horizontal_speed(v, 0, true, &p);
        }
        assert_eq!(v, Fixed::ZERO);
    }

    #[test]
    fn turning_uses_deceleration() {
        let p = physics();
        let v = horizontal_speed(fx(2.0), -1, true, &p);
        assert_eq!(v, fx(2.0) - p[Param::GroundDeceleration]);
        // in the air, momentum is kept with nothing pressed
        assert_eq!(horizontal_speed(fx(2.0), 0, false, &p), fx(2.0));
    }

    #[test]
    fn speed_from_elsewhere_is_kept() {
        let p = physics();
        let fast = p[Param::TopSpeed] + fx(3.0);
        assert_eq!(horizontal_speed(fast, 1, true, &p), fast);
    }
}
