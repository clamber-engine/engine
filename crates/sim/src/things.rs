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
//! - With a `roll_attack_width` above 0, a roll defeats an enemy its
//!   attack box touches on an attack frame, and nothing else does (see
//!   [`roll_attacks`]). The player and the enemy then hold still for
//!   `roll_defeat_freeze` frames.
//! - While the player is knocked out, things don't act on it, and
//!   enemies other than the one that hurt it hold still for the frames
//!   it holds still after the hit.
//! - While a character is being lost, things don't act on the player,
//!   and while the one that took over is protected, nothing hurts it
//!   (see [`loss`](crate::loss)).
//! - A launcher sends the player off at its velocity, then rests for
//!   [`LAUNCH_REST`] frames before it can fire again.
//!
//! Objects that can be carried don't act on the player; the player picks
//! them up (see [`carry`](crate::carry)).
//!
//! A thrown object defeats the walkers it hits (see [`throw_hits`]),
//! which are knocked back the way it came (see [`knocked`]).
//!
//! A defeated walker jumps up and falls out of the level (see [`fall`]);
//! other defeated enemies show as defeated for [`DEFEATED_FRAMES`], then
//! are gone. They stay gone after the player loses a life.

use clamber_fixed::Fixed;
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetype, Behavior, Fall};
use crate::carry;
use crate::flow::{FlowEvent, overlap};
use crate::player::{Facing, HEIGHT, STEP, State, TINY, ease, frames, is_wall};
use crate::view;
use crate::world::{Event, Phase, Thing, World};

/// How long a defeated enemy shows before it's gone.
pub const DEFEATED_FRAMES: u8 = 20;

/// How long a launcher rests after firing.
pub const LAUNCH_REST: u8 = 20;

/// Moves every thing one step, then applies what the player touched.
/// `previous_feet` is the player's `pos.y` before this step; `frozen`
/// says the player was held still on it.
pub(crate) fn update(world: &mut World, previous_feet: Fixed, frozen: bool) {
    let physics = world.player_physics();
    let boxes = physics[Param::BounceWidth] > Fixed::ZERO;
    let hurt_boxes = physics[Param::HurtWidth] > Fixed::ZERO;
    let attack_boxes = physics[Param::RollAttackWidth] > Fixed::ZERO;
    let knocked = |world: &World| {
        world.player.knockout.is_some() || world.events.contains(&Event::Hurt) || world.losing()
    };
    let bounced = if boxes && !knocked(world) {
        bounce_on_boxes(world, &physics)
    } else {
        Vec::new()
    };
    let attacked = if attack_boxes && !frozen && !knocked(world) {
        roll_attacks(world, &physics)
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
        advance(&world.level, thing, &physics);
    }
    throw_hits(world);
    if let Some(view) = world.view {
        // Thrown objects are removed far from the view as walkers are.
        let left = view.x();
        for thing in world.things.iter_mut().filter(|t| t.active) {
            if matches!(thing.phase, Phase::Thrown(f) if f.thrown)
                && view::far(thing.pos.x.floor(), left, &physics)
            {
                thing.active = false;
            }
        }
    }
    // Enemies bounced on still moved this frame, as they were.
    let freeze = frames(physics[Param::BounceFreeze]);
    for index in bounced {
        defeat(&mut world.things[index], freeze);
    }
    let roll_freeze = frames(physics[Param::RollDefeatFreeze]);
    for &index in &attacked {
        defeat(&mut world.things[index], roll_freeze);
    }
    if !attacked.is_empty() {
        world.player.defeat_rolling(roll_freeze);
    }
    if knocked(world) {
        return;
    }
    interact(
        world,
        previous_feet,
        &physics,
        boxes,
        hurt_boxes,
        attack_boxes,
    );
    if world.protected() {
        world.events.retain(|&e| e != Event::Hurt);
    }
    if hurt_boxes && !knocked(world) {
        world.player.hurt_ahead = hurt_ahead(world, &physics);
    }
}

fn advance(level: &Level, thing: &mut Thing, physics: &Physics) {
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
        Phase::Held => return,
        Phase::Knocked { vy } => {
            if let Behavior::Walker {
                response,
                range,
                defeat,
                ..
            } = archetype.behavior
            {
                knocked(level, thing, &archetype, response, range, defeat, vy);
            }
            return;
        }
        Phase::Thrown(flight) => {
            let slope = physics[Param::FaceSlope];
            carry::fly(level, thing, archetype.size, flight, slope);
            return;
        }
        Phase::Placed => {
            carry::settle(level, thing, physics);
            return;
        }
        Phase::Ready => {}
    }
    if thing.wait > 0 {
        thing.wait -= 1;
        return;
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
        Behavior::Launcher { .. } | Behavior::Carry { .. } | Behavior::Collect { .. } => {}
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

/// Moves a walker knocked back by a thrown object, heading the way it's
/// knocked: its speed eases toward `knock_glide` that way by `response`.
/// On the step after the hit (`vy` is `None`) it can still turn at the
/// end of its range, and it moves along the floor; then it jumps up at
/// `defeat.jump` and falls as a defeated walker does, without holding
/// still and easing as it goes. It's gone once its top is below the
/// level.
fn knocked(
    level: &Level,
    thing: &mut Thing,
    archetype: &Archetype,
    response: Fixed,
    range: Option<(Fixed, Fixed)>,
    defeat: Fall,
    vy: Option<Fixed>,
) {
    if vy.is_none()
        && let Some((left, right)) = range
    {
        let start = level.entities[thing.index].at.x;
        let x = Fixed::from_int(thing.pos.x.floor() as i32);
        let past = match thing.heading {
            Facing::Left => x < start + left,
            Facing::Right => x > start + right,
        };
        if past {
            thing.heading = match thing.heading {
                Facing::Left => Facing::Right,
                Facing::Right => Facing::Left,
            };
        }
    }
    let target = match thing.heading {
        Facing::Left => -defeat.knock_glide,
        Facing::Right => defeat.knock_glide,
    };
    thing.vx = if response > Fixed::ZERO {
        ease(thing.vx, target, response)
    } else {
        target
    };
    let x = thing.pos.x + thing.vx;
    match vy {
        None => {
            let from = Vec2::new(x, thing.pos.y - STEP);
            let y = level
                .collision
                .cast(from, Dir::Down, STEP + STEP)
                .map_or(thing.pos.y, |hit| hit.point.y);
            thing.pos = Vec2::new(x, y);
            thing.phase = Phase::Knocked {
                vy: Some(-defeat.jump),
            };
        }
        Some(vy) => {
            let vy = (vy + defeat.gravity).min(defeat.max_fall);
            thing.pos = Vec2::new(x, thing.pos.y + vy);
            thing.phase = Phase::Knocked { vy: Some(vy) };
        }
    }
    if thing.pos.y - archetype.size.y > level.size.y {
        thing.active = false;
    }
}

/// Defeats the walkers thrown objects hit, with everything where it is
/// after moving this step: a walker that's walking is hit when its box
/// overlaps an object's hit box (see [`Behavior::Carry`]). An object
/// hits at most one walker a step; which of two in reach at once isn't
/// in the spec, so it's the first placed. The object goes on as it was.
/// A hit walker is knocked back the way the object came, at `knock`.
fn throw_hits(world: &mut World) {
    for i in 0..world.things.len() {
        let object = &world.things[i];
        let (Phase::Thrown(flight), Some(archetype)) = (object.phase, object.archetype) else {
            continue;
        };
        let Behavior::Carry {
            hit,
            hit_early,
            hit_early_time,
            ..
        } = archetype.behavior
        else {
            continue;
        };
        if !object.active || !flight.thrown {
            continue;
        }
        let right = flight.right();
        let reach = if flight.age <= hit_early_time {
            hit_early
        } else {
            hit
        }
        .bounds(object.pos, right);
        let walker = world.things.iter_mut().find(|t| {
            t.active
                && t.phase == Phase::Ready
                && t.archetype.is_some_and(|a| {
                    matches!(a.behavior, Behavior::Walker { .. })
                        && in_reach(reach, &a, t.pos, t.facing)
                })
        });
        let Some(walker) = walker else {
            continue;
        };
        let Some(Behavior::Walker { defeat, .. }) = walker.archetype.map(|a| a.behavior) else {
            continue;
        };
        (walker.heading, walker.vx) = if right {
            (Facing::Left, -defeat.knock)
        } else {
            (Facing::Right, defeat.knock)
        };
        walker.phase = Phase::Knocked { vy: None };
        world.flow.events.push(FlowEvent::Defeat(walker.index));
    }
}

/// Whether a walker of `archetype` at `pos` facing `facing` is in a
/// thrown object's `reach`, with its `x` rounded down.
fn in_reach(
    reach: (Fixed, Fixed, Fixed, Fixed),
    archetype: &Archetype,
    pos: Vec2,
    facing: Facing,
) -> bool {
    let at = Vec2::new(Fixed::from_int(pos.x.floor() as i32), pos.y);
    overlap(reach, archetype.bounds(at, facing))
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

/// Defeats every enemy the rolling player's attack box touches on an
/// attack frame, from the player's position after this step's move and
/// the enemies' before theirs, on whole pixels: every frame of a timed
/// roll before its `roll_end` frame, and the first `roll_attack_stop`
/// frames of the stop after one. The box is `roll_attack_width` wide,
/// centred `roll_attack_offset` in front of the player, as tall as the
/// body. Returns the enemies defeated, which are put out of action once
/// they've moved this frame.
fn roll_attacks(world: &mut World, physics: &Physics) -> Vec<usize> {
    let player = &world.player;
    let stop = player.roll_stop > 0 && player.roll_stop <= frames(physics[Param::RollAttackStop]);
    if player.roll.is_none() && !stop {
        return Vec::new();
    }
    let at = whole_pixels(player.pos);
    let offset = physics[Param::RollAttackOffset];
    let middle = match player.facing {
        Facing::Left => at.x - offset,
        Facing::Right => at.x + offset,
    };
    let half_width = Fixed::from_bits(physics[Param::RollAttackWidth].to_bits() / 2);
    let reach = (
        middle - half_width,
        at.y - HEIGHT,
        middle + half_width,
        at.y,
    );
    let mut attacked = Vec::new();
    for thing in world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
    {
        let Some(archetype) = thing.archetype else {
            continue;
        };
        if archetype.behavior.is_enemy()
            && overlap(
                reach,
                archetype.bounds(whole_pixels(thing.pos), going(thing)),
            )
        {
            attacked.push(thing.index);
            world.flow.events.push(FlowEvent::Defeat(thing.index));
        }
    }
    attacked
}

/// The enemy that hurts the player on the next step, if any: one its
/// hurt box touches now, on whole pixels. The box is `hurt_width` wide,
/// centred `hurt_offset` in front of the player, and reaches from the
/// top of the body to the feet, or to `hurt_jump_lift` above them from
/// `hurt_jump_time` frames into a jump. A rolling player isn't hurt, nor
/// one in the first `roll_stop_guard` frames of the stop after a roll.
fn hurt_ahead(world: &World, physics: &Physics) -> Option<usize> {
    let player = &world.player;
    let guarded = player.roll_stop > 0 && player.roll_stop <= frames(physics[Param::RollStopGuard]);
    if player.state == State::Roll || guarded {
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
/// instead. With `attack_boxes`, so were defeats by rolling.
fn interact(
    world: &mut World,
    previous_feet: Fixed,
    physics: &Physics,
    boxes: bool,
    hurt_boxes: bool,
    attack_boxes: bool,
) {
    let bounce = physics[Param::BounceSpeed];
    let freeze = frames(physics[Param::BounceFreeze]);
    let player = &mut world.player;
    let rolling = player.state == State::Roll && !attack_boxes;
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
            Behavior::Carry { .. } | Behavior::Collect { .. } => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archetype::Archetypes;
    use clamber_level::Entity;

    fn resolve(kind: &str, params: &str) -> Archetype {
        let entity = Entity {
            kind: kind.to_string(),
            id: None,
            at: Vec2::ints(0, 0),
            params: toml::from_str(params).unwrap(),
        };
        Archetypes::builtin()
            .resolve(&entity, "test", "e")
            .unwrap()
            .unwrap()
    }

    /// thrown-objects.md, "When a walker is hit": the ranges of I and the
    /// height limits of U, for the measured rows.
    #[test]
    fn thrown_objects_hit_title_1_walkers_in_the_measured_ranges() {
        let object = resolve("crate", "");
        let Behavior::Carry {
            hit,
            hit_early,
            hit_early_time,
            ..
        } = object.behavior
        else {
            panic!("{object:?}");
        };
        assert_eq!(hit_early_time, 16);
        let small = resolve("crawler", "size = [20, 12]\noffset = 0.5");
        let large = resolve("crawler", "size = [26, 38]\noffset = 0.5");
        use Facing::{Left, Right};
        // Walker, its facing, the object moving right, then I's range and
        // U's limit early and late.
        let rows = [
            (small, Left, true, (-48, 28), (-23, 27), 63, 30),
            (small, Left, false, (-29, 47), (-28, 22), 63, 30),
            (small, Right, false, (-28, 48), (-27, 23), 63, 30),
            (large, Left, true, (-51, 31), (-26, 30), 89, 56),
            (large, Right, true, (-50, 32), (-25, 31), 89, 56),
        ];
        let walker = Vec2::new(Fixed::from_ratio_pow2(1281, 2), Fixed::from_int(400));
        for (archetype, facing, right, early, late, early_u, late_u) in rows {
            for (hit, (from, to), most_u) in [(hit_early, early, early_u), (hit, late, late_u)] {
                // I from the object's x rounded down, with a fraction.
                let at = |i: i32, u: Fixed| {
                    let x = Fixed::from_int(320 + i) + Fixed::from_ratio_pow2(3, 2);
                    let reach = hit.bounds(Vec2::new(x, walker.y - u), right);
                    in_reach(reach, &archetype, walker, facing)
                };
                let row = format!("{facing:?} {right} {from}..={to}");
                let level = Fixed::ZERO;
                assert!(!at(from - 1, level), "{row}");
                assert!(at(from, level), "{row}");
                assert!(at(to, level), "{row}");
                assert!(!at(to + 1, level), "{row}");
                let just_under = Fixed::from_int(most_u) - Fixed::from_ratio_pow2(1, 4);
                assert!(at(from, just_under), "{row}");
                assert!(!at(from, Fixed::from_int(most_u)), "{row}");
                assert!(at(to, Fixed::from_int(-12)), "{row}");
            }
        }
    }
}
