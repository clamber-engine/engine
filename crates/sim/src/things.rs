//! Enemies and interactive objects: things whose kind is an
//! [`Archetype`](crate::Archetype).
//!
//! Each step, things move as their behavior says, then act on the player
//! if its body overlaps theirs:
//!
//! - An enemy is defeated if the player rolls into it, or lands on it,
//!   which bounces the player off at the physics' `bounce_speed`. Landing
//!   means the player's feet were no lower than the enemy's middle at
//!   the start of the step and haven't risen since. Any other touch hurts
//!   the player, which costs a life (see [`flow`](crate::flow)).
//! - A launcher sends the player off at its velocity, then rests for
//!   [`LAUNCH_REST`] frames before it can fire again.
//!
//! Defeated enemies show as defeated for [`DEFEATED_FRAMES`], then are
//! gone. They stay gone after the player loses a life.

use clamber_fixed::Fixed;
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::Param;

use crate::archetype::Behavior;
use crate::flow::{FlowEvent, overlap};
use crate::player::{Facing, STEP, State, TINY, is_wall};
use crate::world::{Event, Phase, Thing, World};

/// How long a defeated enemy shows before it's gone.
pub const DEFEATED_FRAMES: u8 = 20;

/// How long a launcher rests after firing.
pub const LAUNCH_REST: u8 = 20;

/// Moves every thing one step, then applies what the player touched.
/// `previous_feet` is the player's `pos.y` before this step.
pub(crate) fn update(world: &mut World, previous_feet: Fixed) {
    for thing in world.things.iter_mut().filter(|t| t.active) {
        advance(&world.level, thing);
    }
    interact(world, previous_feet);
}

fn advance(level: &Level, thing: &mut Thing) {
    let Some(archetype) = thing.archetype else {
        return;
    };
    match thing.phase {
        Phase::Defeated(left) => {
            if left > 1 {
                thing.phase = Phase::Defeated(left - 1);
            } else {
                thing.active = false;
            }
            return;
        }
        Phase::Fired(since) => {
            thing.phase = if since + 1 >= LAUNCH_REST {
                Phase::Ready
            } else {
                Phase::Fired(since + 1)
            };
        }
        Phase::Ready => {}
    }
    thing.clock = thing.clock.wrapping_add(1);
    match archetype.behavior {
        Behavior::Walker { speed, .. } => walk(level, thing, speed, archetype.size),
        Behavior::Patrol { path, period } => {
            let origin = level.entities[thing.index].at;
            fly(thing, origin, path, period);
        }
        Behavior::Launcher { .. } => {}
    }
}

/// Moves a walker `speed` along the ground, or turns it around if a wall
/// or a ledge is in the way.
fn walk(level: &Level, thing: &mut Thing, speed: Fixed, size: Vec2) {
    let half_width = Fixed::from_bits(size.x.to_bits() / 2);
    let (cast_dir, sign) = match thing.facing {
        Facing::Left => (Dir::Left, -Fixed::ONE),
        Facing::Right => (Dir::Right, Fixed::ONE),
    };
    let pos = thing.pos;
    // Sweep from just above step height, so walkers climb what the
    // player can walk up, to just below the top of the walker.
    let probe = Vec2::new(pos.x, pos.y - STEP - TINY);
    let reach = (
        (size.y - Fixed::ONE - STEP - TINY).max(Fixed::ZERO),
        Fixed::ZERO,
    );
    let wall = level
        .collision
        .sweep(probe, cast_dir, speed + half_width, reach, is_wall);
    let x = pos.x + sign * speed;
    let ground_at = |x: Fixed| {
        let from = Vec2::new(x, pos.y - STEP);
        level.collision.cast(from, Dir::Down, STEP + STEP)
    };
    let ground = ground_at(x + sign * half_width).and_then(|_| ground_at(x));
    match (wall, ground) {
        (None, Some(hit)) => thing.pos = Vec2::new(x, hit.point.y),
        _ => {
            thing.facing = match thing.facing {
                Facing::Left => Facing::Right,
                Facing::Right => Facing::Left,
            }
        }
    }
}

/// Places a patrol along its path for its clock: out from `origin` in
/// the first half of each `period`, back in the second.
fn fly(thing: &mut Thing, origin: Vec2, path: Vec2, period: u32) {
    let half = i128::from(period / 2);
    let t = i128::from(thing.clock % period);
    let (along, outward) = if t < half {
        (t, true)
    } else {
        (i128::from(period) - t, false)
    };
    let part = |v: Fixed| Fixed::from_bits((i128::from(v.to_bits()) * along / half) as i64);
    thing.pos = Vec2::new(origin.x + part(path.x), origin.y + part(path.y));
    if path.x != Fixed::ZERO {
        thing.facing = if path.x.is_negative() == outward {
            Facing::Left
        } else {
            Facing::Right
        };
    }
}

fn interact(world: &mut World, previous_feet: Fixed) {
    let bounce = world.player_physics()[Param::BounceSpeed];
    let player = &mut world.player;
    let rolling = player.state == State::Roll;
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let Some(archetype) = thing.archetype else {
            continue;
        };
        if thing.phase != Phase::Ready {
            continue;
        }
        let area = archetype.bounds(thing.pos);
        if !overlap(player.body(), area) {
            continue;
        }
        match archetype.behavior {
            Behavior::Launcher { velocity } => {
                player.launch(velocity, &mut world.events);
                thing.phase = Phase::Fired(0);
                world.flow.events.push(FlowEvent::Launch(thing.index));
            }
            _ => {
                let middle = Fixed::from_bits((area.1.to_bits() + area.3.to_bits()) / 2);
                let landed = previous_feet <= middle && player.pos.y >= previous_feet;
                if !rolling && !landed {
                    world.events.push(Event::Hurt);
                    return;
                }
                if !rolling {
                    player.bounce(bounce, &mut world.events);
                }
                thing.phase = Phase::Defeated(DEFEATED_FRAMES);
                world.flow.events.push(FlowEvent::Defeat(thing.index));
            }
        }
    }
}
