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
//! - With a `bounce_width` above 0, the player instead bounces off an
//!   enemy its bounce box touches, checked on whole-pixel positions after
//!   the player moves but before enemies do (see [`bounce_on_boxes`]).
//! - With a `hurt_width` above 0, touching an enemy doesn't hurt. An
//!   enemy that the player's hurt box touches at the end of a step hurts
//!   the player on the next one instead (see [`hurt_ahead`]).
//! - While the player is knocked out, things don't act on it, and
//!   enemies other than the one that hurt it hold still for the frames
//!   it holds still after the hit.
//! - A launcher sends the player off at its velocity, then rests for
//!   [`LAUNCH_REST`] frames before it can fire again.
//!
//! A defeated walker jumps up and falls out of the level (see [`fall`]);
//! other defeated enemies show as defeated for [`DEFEATED_FRAMES`], then
//! are gone. They stay gone after the player loses a life.

use clamber_fixed::Fixed;
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetype, Behavior, Fall};
use crate::flow::{FlowEvent, overlap};
use crate::player::{Facing, HEIGHT, STEP, State, TINY, ease, frames, is_wall};
use crate::world::{Event, Phase, Thing, World};

/// How long a defeated enemy shows before it's gone.
pub const DEFEATED_FRAMES: u8 = 20;

/// How long a launcher rests after firing.
pub const LAUNCH_REST: u8 = 20;

/// Moves every thing one step, then applies what the player touched.
/// `previous_feet` is the player's `pos.y` before this step.
pub(crate) fn update(world: &mut World, previous_feet: Fixed) {
    let physics = world.player_physics();
    let boxes = physics[Param::BounceWidth] > Fixed::ZERO;
    let hurt_boxes = physics[Param::HurtWidth] > Fixed::ZERO;
    let knocked =
        |world: &World| world.player.knockout.is_some() || world.events.contains(&Event::Hurt);
    let bounced = if boxes && !knocked(world) {
        bounce_on_boxes(world, &physics)
    } else {
        Vec::new()
    };
    // Enemies hold still while a knocked-out player does, after the hit
    // frame, except the one that hurt it.
    let hold = frames(physics[Param::KnockoutHold]);
    let held = world
        .player
        .knockout
        .filter(|k| k.frame >= 1 && k.frame < hold)
        .map(|k| k.by);
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let enemy = thing.archetype.is_some_and(|a| a.behavior.is_enemy());
        if held.is_some_and(|by| enemy && by != Some(thing.index)) {
            continue;
        }
        advance(&world.level, thing);
    }
    // Enemies bounced on still moved this frame, as they were.
    let freeze = frames(physics[Param::BounceFreeze]);
    for index in bounced {
        defeat(&mut world.things[index], freeze);
    }
    if knocked(world) {
        return;
    }
    interact(world, previous_feet, &physics, boxes, hurt_boxes);
    if hurt_boxes && !knocked(world) {
        world.player.hurt_ahead = hurt_ahead(world, &physics);
    }
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
        Phase::Falling { vy, hold } => {
            if let Behavior::Walker { defeat, .. } = archetype.behavior {
                fall(level, thing, &archetype, defeat, vy, hold);
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
        Behavior::Walker {
            speed,
            response,
            turn_delay,
            range,
            ..
        } => {
            turn_toward_heading(thing);
            if let Some((left, right)) = range {
                let start = level.entities[thing.index].at.x;
                let x = Fixed::from_int(thing.pos.x.floor() as i32);
                let past = match thing.heading {
                    Facing::Left => x < start + left,
                    Facing::Right => x > start + right,
                };
                if past {
                    turn_around(thing, turn_delay);
                }
            }
            walk(level, thing, speed, response, turn_delay, archetype.size);
        }
        Behavior::Patrol { path, period } => {
            let origin = level.entities[thing.index].at;
            fly(thing, origin, path, period);
        }
        Behavior::Launcher { .. } => {}
    }
}

/// Counts down a walker's turn, turning its facing to its heading when
/// the count runs out.
fn turn_toward_heading(thing: &mut Thing) {
    if thing.turn > 0 {
        thing.turn -= 1;
        if thing.turn == 0 {
            thing.facing = thing.heading;
        }
    }
}

/// Heads a walker the other way. Its facing follows `delay` frames
/// later, or at once for 0.
fn turn_around(thing: &mut Thing, delay: u8) {
    thing.heading = match thing.heading {
        Facing::Left => Facing::Right,
        Facing::Right => Facing::Left,
    };
    thing.turn = delay;
    if delay == 0 {
        thing.facing = thing.heading;
    }
}

/// Defeats an enemy. A walker falls out of the level after holding still
/// for `hold` frames; anything else shows as defeated for a while.
fn defeat(thing: &mut Thing, hold: u8) {
    thing.phase = match thing.archetype.map(|a| a.behavior) {
        Some(Behavior::Walker { defeat, .. }) => Phase::Falling {
            vy: -defeat.jump,
            hold,
        },
        _ => Phase::Defeated(DEFEATED_FRAMES),
    };
}

/// Moves a defeated walker: still while `hold` lasts, then down under
/// the fall's gravity, through anything, at the speed it had across. It's
/// gone once its top is below the level.
fn fall(
    level: &Level,
    thing: &mut Thing,
    archetype: &Archetype,
    defeat: Fall,
    vy: Fixed,
    hold: u8,
) {
    if hold > 0 {
        thing.phase = Phase::Falling { vy, hold: hold - 1 };
        return;
    }
    let vy = (vy + defeat.gravity).min(defeat.max_fall);
    thing.pos = Vec2::new(thing.pos.x + thing.vx, thing.pos.y + vy);
    thing.phase = Phase::Falling { vy, hold: 0 };
    if thing.pos.y - archetype.size.y > level.size.y {
        thing.active = false;
    }
}

/// Moves a walker along the ground, its speed easing toward `speed` the
/// way it's heading by `response`. If a wall is in the way, it's put
/// against it; if a ledge is, it stays put. Either way it stops and turns
/// around, its facing following `turn_delay` frames after the next.
fn walk(
    level: &Level,
    thing: &mut Thing,
    speed: Fixed,
    response: Fixed,
    turn_delay: u8,
    size: Vec2,
) {
    let target = match thing.heading {
        Facing::Left => -speed,
        Facing::Right => speed,
    };
    let vx = if response > Fixed::ZERO {
        ease(thing.vx, target, response)
    } else {
        target
    };
    if vx == Fixed::ZERO {
        thing.vx = vx;
        return;
    }
    let half_width = Fixed::from_bits(size.x.to_bits() / 2);
    let (cast_dir, sign) = if vx.is_negative() {
        (Dir::Left, -Fixed::ONE)
    } else {
        (Dir::Right, Fixed::ONE)
    };
    let speed = vx.abs();
    let pos = thing.pos;
    // Sweep from just above step height, so walkers climb what the
    // player can walk up, to just below the top of the walker.
    let probe = Vec2::new(pos.x, pos.y - STEP - TINY);
    let reach = (
        (size.y - Fixed::ONE - STEP - TINY).max(Fixed::ZERO),
        Fixed::ZERO,
    );
    // A wall stops it only if this move would take it past touching.
    let wall = level
        .collision
        .sweep(probe, cast_dir, speed + half_width, reach, is_wall)
        .filter(|hit| hit.distance < speed + half_width);
    let x = pos.x + vx;
    let ground_at = |x: Fixed| {
        let from = Vec2::new(x, pos.y - STEP);
        level.collision.cast(from, Dir::Down, STEP + STEP)
    };
    let ground = ground_at(x + sign * half_width).and_then(|_| ground_at(x));
    match (wall, ground) {
        (None, Some(hit)) => {
            thing.pos = Vec2::new(x, hit.point.y);
            thing.vx = vx;
        }
        _ => {
            if let Some(hit) = wall {
                let gap = (hit.distance - half_width).max(Fixed::ZERO);
                thing.pos.x = pos.x + sign * gap;
            }
            thing.vx = Fixed::ZERO;
            turn_around(thing, turn_delay.saturating_add(1));
        }
    }
}

/// The way a thing is going: the way it moves, or faces when still.
fn going(thing: &Thing) -> Facing {
    if thing.vx.is_negative() {
        Facing::Left
    } else if thing.vx > Fixed::ZERO {
        Facing::Right
    } else {
        thing.facing
    }
}

/// `pos` rounded to whole pixels: left, and down.
fn whole_pixels(pos: Vec2) -> Vec2 {
    let down = |v: Fixed| Fixed::from_int(v.floor() as i32);
    Vec2::new(down(pos.x), -down(-pos.y))
}

/// Bounces the player off every enemy its bounce box touches, from the
/// player's position after this step's move and the enemies' before
/// theirs, on whole pixels. The box is `bounce_width` wide, centred
/// `bounce_offset` in front of the player, and reaches from the top of
/// the body to `bounce_below` under the feet. A player on the ground,
/// rolling, or rising at `bounce_max_rise` or faster doesn't bounce.
/// Returns the enemies bounced on, which are defeated once they've
/// moved this frame.
fn bounce_on_boxes(world: &mut World, physics: &Physics) -> Vec<usize> {
    let mut bounced = Vec::new();
    let player = &mut world.player;
    let max_rise = physics[Param::BounceMaxRise];
    let rising_fast = if max_rise > Fixed::ZERO {
        player.vel.y <= -max_rise
    } else {
        player.vel.y.is_negative()
    };
    if player.grounded() || player.state == State::Roll || rising_fast {
        return bounced;
    }
    let at = whole_pixels(player.pos);
    let offset = physics[Param::BounceOffset];
    let middle = match player.facing {
        Facing::Left => at.x - offset,
        Facing::Right => at.x + offset,
    };
    let half_width = Fixed::from_bits(physics[Param::BounceWidth].to_bits() / 2);
    let reach = (
        middle - half_width,
        at.y - HEIGHT,
        middle + half_width,
        at.y + physics[Param::BounceBelow],
    );
    let speed = physics[Param::BounceSpeed];
    let freeze = frames(physics[Param::BounceFreeze]);
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let Some(archetype) = thing.archetype else {
            continue;
        };
        if thing.phase != Phase::Ready || !archetype.behavior.is_enemy() {
            continue;
        }
        if overlap(
            reach,
            archetype.bounds(whole_pixels(thing.pos), going(thing)),
        ) {
            player.bounce(speed, freeze, &mut world.events);
            bounced.push(thing.index);
            world.flow.events.push(FlowEvent::Defeat(thing.index));
        }
    }
    bounced
}

/// The enemy that hurts the player on the next step, if any: one its
/// hurt box touches now, on whole pixels. The box is `hurt_width` wide,
/// centred `hurt_offset` in front of the player, and reaches from the
/// top of the body to the feet, or to `hurt_jump_lift` above them from
/// `hurt_jump_time` frames into a jump. A rolling player isn't hurt.
fn hurt_ahead(world: &World, physics: &Physics) -> Option<usize> {
    let player = &world.player;
    if player.state == State::Roll {
        return None;
    }
    let at = whole_pixels(player.pos);
    let offset = physics[Param::HurtOffset];
    let middle = match player.facing {
        Facing::Left => at.x - offset,
        Facing::Right => at.x + offset,
    };
    let half_width = Fixed::from_bits(physics[Param::HurtWidth].to_bits() / 2);
    let late = player
        .jump_age
        .is_some_and(|age| age >= frames(physics[Param::HurtJumpTime]));
    let lift = if late {
        physics[Param::HurtJumpLift]
    } else {
        Fixed::ZERO
    };
    let reach = (
        middle - half_width,
        at.y - HEIGHT,
        middle + half_width,
        at.y - lift,
    );
    world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
        .find(|t| {
            t.archetype.is_some_and(|a| {
                a.behavior.is_enemy() && overlap(reach, a.bounds(whole_pixels(t.pos), going(t)))
            })
        })
        .map(|t| t.index)
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

/// Acts on what the player's body touches after everything has moved.
/// With `boxes`, bounces were already decided by [`bounce_on_boxes`], so
/// any other touch of an enemy hurts, unless `hurt_boxes` decide that
/// instead.
fn interact(
    world: &mut World,
    previous_feet: Fixed,
    physics: &Physics,
    boxes: bool,
    hurt_boxes: bool,
) {
    let bounce = physics[Param::BounceSpeed];
    let freeze = frames(physics[Param::BounceFreeze]);
    let player = &mut world.player;
    let rolling = player.state == State::Roll;
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let Some(archetype) = thing.archetype else {
            continue;
        };
        if thing.phase != Phase::Ready {
            continue;
        }
        let area = archetype.bounds(thing.pos, going(thing));
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
                let landed = !boxes && previous_feet <= middle && player.pos.y >= previous_feet;
                if !rolling && !landed {
                    if hurt_boxes {
                        continue;
                    }
                    world.events.push(Event::Hurt);
                    return;
                }
                if !rolling {
                    player.bounce(bounce, freeze, &mut world.events);
                }
                defeat(thing, if rolling { 0 } else { freeze });
                world.flow.events.push(FlowEvent::Defeat(thing.index));
            }
        }
    }
}
