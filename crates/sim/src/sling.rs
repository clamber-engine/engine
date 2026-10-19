//! Launch barrels (spec/behavior/launch-barrel.md): things whose
//! archetype has a [`Sling`] behavior.
//!
//! A sling never moves. It catches the player on a frame on which it's
//! ready, after the player's move, when the player's offsets from it are
//! within its reach for what the player is doing then ([`catch`]): the
//! player stops where it moved to and is off the ground. On the next
//! frame it's moved by whole px onto the sling's position, keeping the
//! fractions, and held there, facing as it was, until the sling's `hold`
//! frames after the catch are over. On the frame after, it faces the way
//! of the shot and is given the shot's speed, without moving, and the
//! sling breaks if it `breaks`. It then moves at that speed for `glide`
//! frames, and flies from there until it lands ([`fly`]). Nothing held
//! makes a difference until the flight, and in the flight only Left and
//! Right do. From the frame after the landing it plays as usual. After a
//! catch, the sling catches no one for `rest` frames.
//!
//! Engine choices where the spec says nothing:
//!
//! - Only the character under control is caught, and only while it moves
//!   by the usual rules: not while riding, holding an object or a rope,
//!   swapping, knocked out, hurt or lost. Whatever it's doing then, its
//!   reach is the one for the nearest pose in the spec's table, as for a
//!   collectible.
//! - The other character follows as usual.
//! - Hurt or bounced before landing, the player is no longer shot, and
//!   goes on by the usual rules. Springs don't catch it before it lands.
//! - Walls and ceilings stop the player as in the air; on the frames
//!   without speed down it doesn't move up or down at all.
//! - A Jump press before the landing frame is forgotten. One on the
//!   landing frame is noted as usual.
//! - A sling that appears stays: the view doesn't remove it. A broken one
//!   is gone for the rest of the visit and leaves nothing behind.
//! - A sling shooting straight down faces the player right, as the
//!   spec's does; one shooting left faces it left, and right otherwise.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::Vec2;
use clamber_pack::Param;

use crate::archetype::{Behavior, Sling};
use crate::collect;
use crate::player::{Facing, State, ceil, frames};
use crate::world::{Phase, World};

/// The kind of entity a launch barrel is.
pub const SLING: &str = "sling";

/// The player caught by a sling, from the catch until it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slung {
    /// The sling's thing.
    pub thing: usize,
    pub values: Sling,
    /// The sling's position, whole px: its `x` and its `y`, rounded down.
    pub at: (i64, i64),
    /// Frames since the catch, from 0 on its frame.
    pub since: u16,
}

impl Slung {
    /// The way the player faces from the shot on.
    fn facing(&self) -> Facing {
        if self.values.shot.x.is_negative() {
            Facing::Left
        } else {
            Facing::Right
        }
    }
}

/// After the player's move on a step it moved by the usual rules: a
/// ready sling that has it within reach catches it.
pub(crate) fn catch(world: &mut World) {
    let player = &world.player;
    let free = player.sling.is_none()
        && player.rope.is_none()
        && player.riding.is_none()
        && player.hands.is_none()
        && player.knockout.is_none()
        && player.state != State::Hurt
        && player.swap == 0
        && world.swap.is_none()
        && !world.losing();
    if !free {
        return;
    }
    let crouch_start = frames(world.player_physics()[Param::CrouchStart]);
    let set_down = frames(world.player_physics()[Param::PutDownTime]).saturating_sub(1);
    let x = player.pos.x.floor();
    let feet = ceil(player.pos.y);
    let caught = world.things.iter().enumerate().find_map(|(i, t)| {
        let Behavior::Sling(values) = t.archetype?.behavior else {
            return None;
        };
        if !t.active || t.phase != Phase::Ready {
            return None;
        }
        let at = (t.pos.x.floor(), t.pos.y.floor());
        let past = match player.facing {
            Facing::Right => x - at.0,
            Facing::Left => at.0 + i64::from(values.left_shift) - x,
        };
        let below = feet - at.1;
        let (past, below) = (i32::try_from(past).ok()?, i32::try_from(below).ok()?);
        let span = collect::span(&values.reach, player, crouch_start, set_down);
        span.contains(past, below).then_some(Slung {
            thing: i,
            values,
            at,
            since: 0,
        })
    });
    if let Some(slung) = caught {
        world.things[slung.thing].phase = Phase::Caught(0);
        world.player.catch_in_sling(slung, &mut world.events);
    }
}

/// Each step, a sling that caught the player counts the frames since,
/// and is ready again once its `rest` is over.
pub(crate) fn update(world: &mut World) {
    for thing in &mut world.things {
        let (Some(Behavior::Sling(values)), Phase::Caught(n)) =
            (thing.archetype.map(|a| a.behavior), thing.phase)
        else {
            continue;
        };
        thing.phase = if n >= values.rest {
            Phase::Ready
        } else {
            Phase::Caught(n + 1)
        };
    }
}

/// One step of a player held or shot by a sling, if it is. Returns
/// whether it was, so the step's moves are done.
pub(crate) fn fly(world: &mut World, input: &Frame) -> bool {
    let Some(mut slung) = world.player.sling else {
        return false;
    };
    if world.player.state == State::Hurt {
        world.player.sling = None;
        return false;
    }
    slung.since = slung.since.saturating_add(1);
    let v = slung.values;
    let shot = u16::from(v.hold) + 1;
    if slung.since == shot && v.breaks {
        let thing = &mut world.things[slung.thing];
        thing.active = false;
        thing.phase = Phase::Broken;
    }
    let physics = world.player_physics();
    let p = &mut world.player;
    p.touched.clear();
    p.update_jump_held(input);
    p.jump_buffer.take();
    p.sling = Some(slung);
    if slung.since == 1 {
        let whole = |n: i64| Fixed::from_int(n as i32);
        p.pos.x += whole(slung.at.0 - p.pos.x.floor());
        p.pos.y += whole(slung.at.1 - ceil(p.pos.y));
    }
    if slung.since < shot {
        p.vel = Vec2::default();
        return true;
    }
    p.facing = slung.facing();
    if slung.since == shot {
        p.vel = v.shot;
        return true;
    }
    let held = |a| input.held.contains(a);
    let dir = match (held(Action::Left), held(Action::Right)) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    };
    let gliding = slung.since <= shot + u16::from(v.glide);
    if !gliding {
        p.target = v.steer * Fixed::from_int(dir);
        p.vel.x = steer(p.vel.x, p.target, v.steer_response);
        p.vel.y = (p.vel.y + v.gravity).min(v.max_fall);
    }
    let down = !gliding || p.vel.y != Fixed::ZERO;
    p.move_through_air(&world.level, &physics, down, &mut world.events);
    p.facing = slung.facing();
    if p.grounded() {
        p.sling = None;
        p.land_from_throw(input, &physics);
    }
    p.state = p.next_state(held(Action::Action), &world.events);
    true
}

/// `v` moved toward `target` by `response` of the gap, rounded toward
/// zero to a 1/256 px per frame, and not at all once that's nothing.
fn steer(v: Fixed, target: Fixed, response: Fixed) -> Fixed {
    let step = (target - v).checked_mul(response).unwrap_or(target - v);
    let grain = Fixed::from_bits(Fixed::ONE.to_bits() / 256).to_bits();
    let whole = step.to_bits().abs() / grain * grain;
    v + Fixed::from_bits(if step.is_negative() { -whole } else { whole })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px256(n: i64) -> Fixed {
        Fixed::from_bits((n << 32) / 256)
    }

    #[test]
    fn steering_closes_a_32nd_of_the_gap_rounded_toward_zero() {
        // launch-barrel.md, "The flight": the examples.
        let response = px256(8);
        let three = Fixed::from_int(3);
        let run = |target: Fixed, from: i64| {
            let mut v = px256(from);
            (0..3)
                .map(|_| {
                    v = steer(v, target, response);
                    v
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(Fixed::ZERO, -2167), [-2100, -2035, -1972].map(px256));
        assert_eq!(run(-three, -2167), [-2124, -2082, -2041].map(px256));
        assert_eq!(run(three, -2167), [-2076, -1988, -1902].map(px256));
        assert_eq!(run(three, 0), [24, 47, 69].map(px256));
        // A gap under 32 / 256 doesn't close.
        assert_eq!(steer(px256(-31), Fixed::ZERO, response), px256(-31));
    }
}
