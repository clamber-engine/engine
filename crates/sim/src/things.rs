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
//! - A walker with a contact table decides its touches itself (see
//!   [`walker_contact`]): it hurts the player, is bounced on, or, with a
//!   `roll` range, is rolled into. With `roll_defeat_holds_walkers`, it
//!   then doesn't move on that frame, and every walker holds still on
//!   the frozen frames after it.
//! - With a `ground_attack_width` above 0, the ground action knocks out
//!   walkers and throwers its attack box touches on its attack frames
//!   (see [`ground_attacks`]). For `ground_attack_pause` frames after,
//!   the player, every walker and every thrower hold still, and nothing
//!   hurts the player.
//! - While the player is knocked out, things don't act on it, and
//!   enemies other than the one that hurt it hold still for the frames
//!   it holds still after the hit.
//! - While a character is being lost, things don't act on the player,
//!   and while the one that took over is protected, nothing hurts it
//!   (see [`loss`](crate::loss)).
//! - A launcher sends the player off at its velocity, then rests for
//!   [`LAUNCH_REST`] frames before it can fire again.
//!
//! A stinger sways around its start and hurts the player on any contact
//! (see [`stinger`]).
//!
//! A crate holding an animal breaks when the player lands on it, which
//! bounces off it, and the player can mount the animal that comes out
//! (see [`hutch`]).
//!
//! A player riding the animal meets walkers only by their rider tables:
//! it charges at and throws them, or is knocked off (see
//! [`ride`](crate::ride)). While a player knocked off holds still,
//! nothing moves or touches it.
//!
//! Objects that can be carried don't act on the player; the player picks
//! them up (see [`carry`](crate::carry)).
//!
//! A thrown object defeats the walkers it hits (see [`throw_hits`]),
//! which are knocked back the way it came (see [`knocked`]).
//!
//! An object holding the missing character breaks on the walker it hits,
//! and on throwers, which are knocked out, and balls, which are gone. A
//! walker can break it in the hands during a throw too (see
//! [`hands_hits`]). Breaking, it frees the character (see
//! [`join`](crate::join)).
//!
//! Throwers throw balls, which hurt the player as walkers' hurt boxes do,
//! on the next step, by the player's pose (see
//! [`throwers`](crate::throwers)). The same things stop them.
//!
//! A defeated walker jumps up and falls out of the level (see [`fall`]);
//! other defeated enemies show as defeated for [`DEFEATED_FRAMES`], then
//! are gone. They stay gone after the player loses a life.

use clamber_fixed::Fixed;
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetype, Behavior, Fall, Response};
use crate::carry;
use crate::columns;
use crate::flow::{FlowEvent, overlap};
use crate::hutch;
use crate::join;
use crate::player::{Act, Facing, HEIGHT, SPEED_GRAIN, STEP, State, TINY, frames, is_wall};
use crate::pose::{self, Stance};
use crate::ride;
use crate::stinger;
use crate::throwers::{self, Picture};
use crate::view;
use crate::world::{Event, Phase, Thing, World};

/// How long a defeated enemy shows before it's gone.
pub const DEFEATED_FRAMES: u8 = 20;

/// How long a launcher rests after firing.
pub const LAUNCH_REST: u8 = 20;

/// Moves every thing one step, then applies what the player touched.
/// `previous_feet` is the player's `pos.y` before this step; `frozen`
/// says the player was held still on it. Returns the walker with a
/// contact table that hurt the player on this step, if one did.
pub(crate) fn update(world: &mut World, previous_feet: Fixed, frozen: bool) -> Option<usize> {
    let physics = world.player_physics();
    pose::note(&mut world.player, &world.events, &physics);
    let boxes = physics[Param::BounceWidth] > Fixed::ZERO;
    let hurt_boxes = physics[Param::HurtWidth] > Fixed::ZERO;
    let attack_boxes = physics[Param::RollAttackWidth] > Fixed::ZERO;
    let knocked = |world: &World| {
        // Nothing touches the characters during the walk-out either.
        world.player.knockout.is_some()
            || world.events.contains(&Event::Hurt)
            || world.losing()
            || world.ending.is_some()
            // Nor while a player knocked off an animal holds still.
            || (world.player.unseated && (frozen || world.player.frozen > 0))
    };
    // A rider meets walkers only by their rider tables.
    let riding = world.player.riding.is_some();
    if riding && !frozen && !knocked(world) {
        ride::attack(world);
    }
    let mut bounced = if boxes && !riding && !knocked(world) {
        bounce_on_boxes(world, &physics)
    } else {
        Vec::new()
    };
    // Walkers with a contact table decide for themselves, on the frames
    // the player moves.
    let holds_walkers = physics[Param::BounceHoldsWalkers] > Fixed::ZERO;
    let mut hurt_by = None;
    let roll_freeze = frames(physics[Param::RollDefeatFreeze]);
    let roll_holds = physics[Param::RollDefeatHoldsWalkers] > Fixed::ZERO;
    let mut rolled_into = Vec::new();
    if !frozen && !riding && !knocked(world) {
        match walker_contact(world, &physics) {
            Some((index, Touch::Bounce)) => {
                world.player.bounce(
                    physics[Param::BounceSpeed],
                    frames(physics[Param::BounceFreeze]),
                    &mut world.events,
                );
                world.flow.events.push(FlowEvent::Defeat(index));
                if holds_walkers {
                    // It doesn't move on this frame.
                    let facing = world.player.facing;
                    bounce_off(&mut world.things[index], facing, 1);
                } else {
                    bounced.push(index);
                }
            }
            Some((index, Touch::Hurt)) => {
                world.events.push(Event::Hurt);
                hurt_by = Some(index);
            }
            Some((index, Touch::Defeat)) => {
                world.flow.events.push(FlowEvent::Defeat(index));
                world.player.defeat_rolling(roll_freeze, &physics);
                if roll_holds {
                    // It doesn't move on this frame.
                    let facing = world.player.facing;
                    bounce_off(&mut world.things[index], facing, 1);
                } else {
                    rolled_into.push(index);
                }
            }
            None => {}
        }
    }
    let attacked = if attack_boxes && !frozen && !knocked(world) {
        roll_attacks(world, &physics)
    } else {
        Vec::new()
    };
    let struck = if !frozen && !knocked(world) {
        ground_attacks(world, &physics)
    } else {
        Vec::new()
    };
    // Walkers and throwers, knocked out or not, hold still on the ground
    // action's pause frames.
    let paused = frozen && world.player.ground_action.is_some();
    // With `bounce_holds_walkers`, so do walkers on a bounce's frozen
    // frames.
    let held_by_bounce = holds_walkers && frozen && world.player.state == State::Bounce;
    // With `roll_defeat_holds_walkers`, on a roll's frozen frames too.
    let held_by_roll = roll_holds && frozen && world.player.state == State::Roll;
    // Enemies hold still while a knocked-out player does, after the hit
    // frame, except the one that hurt it.
    let hold = u16::from(frames(physics[Param::KnockoutHold]));
    let held = world
        .player
        .knockout
        .filter(|k| k.frame >= 1 && k.frame < hold)
        .map(|k| k.by);
    // With `knockout_enemy_wait`, every enemy holds still for that many
    // frames after the hit frame, and makes up for them on the next.
    let wait = u16::from(frames(physics[Param::KnockoutEnemyWait]));
    let knockout_frame = world.player.knockout.map(|k| k.frame);
    let waiting = wait > 0 && knockout_frame.is_some_and(|f| f >= 1 && f <= wait);
    let catch_up = wait > 0 && knockout_frame == Some(wait + 1);
    // Nothing moves while a player knocked off an animal holds still.
    if frozen && world.player.unseated {
        hutch::update(world, &physics, frozen, true);
        return None;
    }
    throwers::throw(world);
    // Balls leave by where the view was at the start of the step.
    let picture = world.view.map(|v| Picture::of(v, &physics));
    let mut freed = Vec::new();
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let enemy = thing.archetype.is_some_and(|a| a.behavior.is_enemy());
        if held.is_some_and(|by| enemy && by != Some(thing.index)) || (waiting && enemy) {
            continue;
        }
        // Stingers hold still with them, the one that hurt it too.
        let stinger = thing
            .archetype
            .is_some_and(|a| matches!(a.behavior, Behavior::Stinger(_)));
        if stinger && held.is_some() {
            continue;
        }
        if paused && thing.archetype.is_some_and(|a| a.behavior.fall().is_some()) {
            continue;
        }
        if (held_by_bounce || held_by_roll)
            && thing.archetype.is_some_and(|a| a.behavior.is_walker())
        {
            continue;
        }
        let moves = if catch_up && enemy { wait + 1 } else { 1 };
        for _ in 0..moves {
            if advance(&world.level, thing, &physics, picture) {
                freed.push(thing.pos);
            }
        }
    }
    freed.extend(throw_hits(world));
    freed.extend(hands_hits(world, &physics));
    if let Some(&at) = freed.first() {
        join::start(world, at);
    }
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
    let untouchable = knocked(world);
    hutch::update(world, &physics, frozen, untouchable);
    // Enemies bounced on still moved this frame, as they were.
    let freeze = frames(physics[Param::BounceFreeze]);
    let facing = world.player.facing;
    for index in bounced {
        bounce_off(&mut world.things[index], facing, freeze);
    }
    for &index in &attacked {
        defeat(&mut world.things[index], roll_freeze);
    }
    for index in rolled_into {
        bounce_off(&mut world.things[index], facing, roll_freeze);
    }
    if !attacked.is_empty() {
        world.player.defeat_rolling(roll_freeze, &physics);
    }
    // They hold still on the pause frames instead of holding here.
    for &index in &struck {
        defeat(&mut world.things[index], 0);
    }
    let pause = frames(physics[Param::GroundAttackPause]);
    if !struck.is_empty() {
        world.player.frozen = pause;
    }
    if knocked(world) {
        return hurt_by;
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
    // Nothing hurts the player on the frame after a ground action's
    // pause either.
    let pausing = world.player.ground_action.is_some() && (frozen || world.player.frozen > 0);
    if !knocked(world) && !pausing {
        // Walkers don't hurt a player knocked off an animal before it
        // lands (riding-walkers.md's example flies on over one), but it
        // can bounce off them.
        let walker = if hurt_boxes && !riding && !world.player.unseated {
            hurt_ahead(world, &physics)
        } else {
            None
        };
        world.player.hurt_ahead = walker
            .or_else(|| throwers::hurt_ahead(world))
            .or_else(|| stinger::hurt_ahead(world));
    }
    hurt_by
}

/// Moves a thing one step. Returns whether it was an object holding the
/// missing character that broke.
fn advance(level: &Level, thing: &mut Thing, physics: &Physics, picture: Option<Picture>) -> bool {
    let Some(archetype) = thing.archetype else {
        return false;
    };
    match thing.phase {
        Phase::Defeated(left) => {
            if left > 1 {
                thing.phase = Phase::Defeated(left - 1);
            } else {
                thing.active = false;
            }
            return false;
        }
        Phase::Falling { vy, hold } => {
            if let Some(defeat) = archetype.behavior.fall() {
                fall(level, thing, &archetype, defeat, vy, hold);
            }
            return false;
        }
        Phase::Fired(since) => {
            thing.phase = if since + 1 >= LAUNCH_REST {
                Phase::Ready
            } else {
                Phase::Fired(since + 1)
            };
        }
        Phase::Held => return false,
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
            return false;
        }
        Phase::Thrown(flight) => {
            let slope = physics[Param::FaceSlope];
            return carry::fly(level, thing, &archetype, flight, slope, picture);
        }
        Phase::Placed => {
            carry::settle(level, thing, physics);
            return false;
        }
        Phase::Flying { .. } => {
            if let Behavior::Ball(ball) = archetype.behavior {
                throwers::fly(level, thing, &ball, archetype.size, picture);
            }
            return false;
        }
        // Crates holding an animal move in [`hutch::update`].
        Phase::Opened(_) | Phase::Mounted(_) | Phase::Ridden | Phase::Bolting { .. } => {
            return false;
        }
        Phase::Swaying { .. } => {
            if let Behavior::Stinger(stinger) = archetype.behavior {
                stinger::sway(thing, level.entities[thing.index].at, &stinger);
            }
            return false;
        }
        Phase::Ready => {
            if let Behavior::Stinger(stinger) = archetype.behavior {
                stinger::start(thing, level.entities[thing.index].at, &stinger);
                return false;
            }
        }
    }
    if thing.wait > 0 {
        thing.wait -= 1;
        return false;
    }
    thing.clock = thing.clock.wrapping_add(1);
    match archetype.behavior {
        Behavior::Walker {
            speed,
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
            walk(level, thing, speed, &archetype, turn_delay);
        }
        Behavior::Patrol { path, period } => {
            let origin = level.entities[thing.index].at;
            fly(thing, origin, path, period);
        }
        Behavior::Ball(ball) => throwers::fly(level, thing, &ball, archetype.size, picture),
        Behavior::Launcher { .. }
        | Behavior::Carry { .. }
        | Behavior::Collect { .. }
        | Behavior::Thrower { .. }
        | Behavior::Hutch(_)
        | Behavior::Stinger(_) => {}
    }
    false
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

/// Defeats an enemy. A walker or a thrower falls out of the level after
/// holding still for `hold` frames; anything else shows as defeated for a
/// while.
fn defeat(thing: &mut Thing, hold: u8) {
    thing.phase = match thing.archetype.and_then(|a| a.behavior.fall()) {
        Some(defeat) => Phase::Falling {
            vy: -defeat.jump,
            hold,
        },
        None => Phase::Defeated(DEFEATED_FRAMES),
    };
}

/// Defeats an enemy bounced on by a player facing `facing`, as
/// [`defeat`] does. A walker with a `bounced_speed` moves at it away from
/// the way the player faces, and faces that way.
fn bounce_off(thing: &mut Thing, facing: Facing, hold: u8) {
    if let Some(Behavior::Walker {
        bounced_speed: Some(speed),
        ..
    }) = thing.archetype.map(|a| a.behavior)
    {
        let away = match facing {
            Facing::Left => Facing::Right,
            Facing::Right => Facing::Left,
        };
        thing.vx = match away {
            Facing::Left => -speed,
            Facing::Right => speed,
        };
        thing.heading = away;
        thing.facing = away;
        thing.turn = 0;
    }
    defeat(thing, hold);
}

/// Moves a defeated walker or thrower: still while `hold` lasts, then down under
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
    let next = (vy + defeat.gravity).min(defeat.max_fall);
    let by = if defeat.move_first { vy } else { next };
    thing.pos = Vec2::new(thing.pos.x + thing.vx, thing.pos.y + by);
    thing.phase = Phase::Falling { vy: next, hold: 0 };
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
    response: Response,
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
    thing.vx = ease(thing.vx, target, response);
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
/// in the spec, so it's the first placed. The object goes on as it was,
/// unless it holds the missing character: then it breaks.
///
/// One holding the character that hits no walker breaks on the first
/// thrower or ball, in that order, whose position its thrower or ball box
/// holds (see [`Holder`](crate::Holder)). A thrower it breaks on is
/// knocked out: it holds still on the next step, the last of the join's
/// (see [`join`]), and leaves from the one after as a defeated walker
/// does. A ball it breaks on is gone.
///
/// Returns where each object holding the character broke.
fn throw_hits(world: &mut World) -> Vec<Vec2> {
    let mut freed = Vec::new();
    for i in 0..world.things.len() {
        let object = &world.things[i];
        let (Phase::Thrown(flight), Some(archetype)) = (object.phase, object.archetype) else {
            continue;
        };
        let Behavior::Carry {
            hit,
            hit_early,
            hit_early_time,
            holder,
            ..
        } = archetype.behavior
        else {
            continue;
        };
        if !object.active || !flight.thrown {
            continue;
        }
        let right = flight.right();
        let early = flight.age <= hit_early_time;
        let pos = object.pos;
        let reach = if early { hit_early } else { hit }.bounds(pos, right);
        if hit_walker(world, reach, right) {
            if holder.is_some() {
                world.things[i].active = false;
                freed.push(pos);
            }
            continue;
        }
        let Some(holder) = holder else {
            continue;
        };
        let (thrower, ball) = if early {
            (holder.thrower_early, holder.ball_early)
        } else {
            (holder.thrower, holder.ball)
        };
        let target = world.things.iter_mut().find(|t| {
            t.active
                && match t.archetype.map(|a| a.behavior) {
                    Some(Behavior::Thrower { .. }) => {
                        t.phase == Phase::Ready && thrower.holds(pos, right, t.pos)
                    }
                    Some(Behavior::Ball(_)) => {
                        matches!(t.phase, Phase::Ready | Phase::Flying { .. })
                            && ball.holds(pos, right, t.pos)
                    }
                    _ => false,
                }
        });
        let Some(target) = target else {
            continue;
        };
        match target.archetype.and_then(|a| a.behavior.fall()) {
            Some(defeat) => {
                target.phase = Phase::Falling {
                    vy: -defeat.jump,
                    hold: 1,
                };
                world.flow.events.push(FlowEvent::Defeat(target.index));
            }
            None => target.active = false,
        }
        world.things[i].active = false;
        freed.push(pos);
    }
    freed
}

/// Knocks back the first walker that's walking whose box overlaps
/// `reach`, the way an object moving right if `right` came, at `knock`.
/// Returns whether there was one.
fn hit_walker(world: &mut World, reach: (Fixed, Fixed, Fixed, Fixed), right: bool) -> bool {
    let walker = world.things.iter_mut().find(|t| {
        t.active
            && t.phase == Phase::Ready
            && t.archetype.is_some_and(|a| {
                matches!(a.behavior, Behavior::Walker { .. })
                    && in_reach(reach, &a, t.pos, t.facing)
            })
    });
    let Some(walker) = walker else {
        return false;
    };
    let Some(Behavior::Walker { defeat, .. }) = walker.archetype.map(|a| a.behavior) else {
        return false;
    };
    (walker.heading, walker.vx) = if right {
        (Facing::Left, -defeat.knock)
    } else {
        (Facing::Right, defeat.knock)
    };
    walker.phase = Phase::Knocked { vy: None };
    world.flow.events.push(FlowEvent::Defeat(walker.index));
    true
}

/// During a throw, after its first frame and before the object leaves the
/// hands, a walker its `hit_early` box touches in the hands knocks back
/// and breaks an object holding the missing character, as one thrown the
/// way the player faces. Returns where it broke, if it did.
fn hands_hits(world: &mut World, physics: &Physics) -> Option<Vec2> {
    let hands = world.player.hands?;
    let release = frames(physics[Param::ThrowRelease]);
    if hands.act != Act::Throw || hands.frame == 0 || hands.frame >= release {
        return None;
    }
    let object = &world.things[hands.thing];
    if !object.active || object.phase != Phase::Held {
        return None;
    }
    let Some(Behavior::Carry {
        hit_early,
        holder: Some(_),
        ..
    }) = object.archetype.map(|a| a.behavior)
    else {
        return None;
    };
    let pos = carry::in_hands(&world.player);
    let right = world.player.facing == Facing::Right;
    if !hit_walker(world, hit_early.bounds(pos, right), right) {
        return None;
    }
    world.things[hands.thing].active = false;
    Some(pos)
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
/// way it's heading by its `response`, before or after it moves by its
/// `move_first`. If a wall is in the way, it's put against it; if a
/// ledge is, it stays put. Either way it stops and turns around, its
/// facing following `turn_delay` frames after the next.
fn walk(level: &Level, thing: &mut Thing, speed: Fixed, archetype: &Archetype, turn_delay: u8) {
    let Behavior::Walker {
        response,
        move_first,
        columns,
        ..
    } = archetype.behavior
    else {
        return;
    };
    let size = archetype.size;
    let target = match thing.heading {
        Facing::Left => -speed,
        Facing::Right => speed,
    };
    let eased = ease(thing.vx, target, response);
    let vx = if move_first { thing.vx } else { eased };
    if vx == Fixed::ZERO {
        thing.vx = eased;
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
    let foot = if columns {
        columns::middle(columns::column(x))
    } else {
        x
    };
    let ground = ground_at(x + sign * half_width).and_then(|_| ground_at(foot));
    match (wall, ground) {
        (None, Some(hit)) => {
            thing.pos = Vec2::new(x, hit.point.y);
            thing.vx = eased;
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

/// `v` eased toward `target` by `response` (see [`Response`]).
fn ease(v: Fixed, target: Fixed, response: Response) -> Fixed {
    if response.at_once() {
        return target;
    }
    let gap = target - v;
    let grain = SPEED_GRAIN.to_bits();
    let step: i64 = response
        .parts
        .iter()
        .map(|&part| gap.checked_mul(part).unwrap_or(gap).to_bits().abs() / grain * grain)
        .sum();
    if step == 0 {
        target
    } else if gap.is_negative() {
        v - Fixed::from_bits(step)
    } else {
        v + Fixed::from_bits(step)
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

/// What a walker with a contact table does to the player it touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Touch {
    Hurt,
    Bounce,
    /// Rolled into: the walker is defeated.
    Defeat,
}

/// The first walker with a contact table that touches the player, and
/// what it does (see [`WalkerContact`](crate::WalkerContact)): from the
/// player's position after this step's move and the walkers' before
/// theirs. A rolling player defeats it. Otherwise a player on the ground
/// is hurt. In the air, it bounces if it rises at `bounce_rise` or slower
/// and is no more than `bounce_above` below the walker; otherwise it's
/// hurt.
///
/// Which of two walkers touching on the same frame counts isn't in the
/// spec; it's the first placed.
fn walker_contact(world: &World, physics: &Physics) -> Option<(usize, Touch)> {
    let player = &world.player;
    for thing in world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
    {
        let Some(Behavior::Walker {
            contact: Some(contact),
            ..
        }) = thing.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        let stance = pose::stance(player, &contact, physics)?;
        let across = player.pos.x.floor() - thing.pos.x.floor();
        let above = thing.pos.y.floor() - player.pos.y.floor();
        if !contact
            .reach(stance, player.facing)
            .is_some_and(|r| r.contains(across, above))
        {
            continue;
        }
        let touch = if stance == Stance::Rolling {
            Touch::Defeat
        } else if stance == Stance::Air
            && !player.grounded()
            && player.vel.y >= -contact.bounce_rise
            && above >= i64::from(contact.bounce_above)
        {
            Touch::Bounce
        } else {
            Touch::Hurt
        };
        return Some((thing.index, touch));
    }
    None
}

/// Whether `thing` is a walker with a contact table, which decides its
/// own touches (see [`walker_contact`]).
fn has_contact(thing: &Thing) -> bool {
    matches!(
        thing.archetype.map(|a| a.behavior),
        Some(Behavior::Walker {
            contact: Some(_),
            ..
        })
    )
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
        if thing.phase != Phase::Ready || !archetype.behavior.is_enemy() || has_contact(thing) {
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
    let reach = attack_box(
        world,
        physics[Param::RollAttackWidth],
        physics[Param::RollAttackOffset],
    );
    strike(world, reach, |b| b.is_enemy())
}

/// Knocks out every walker and thrower the ground action's attack box
/// touches on an attack frame, from the player's position after this
/// step's move and theirs before they move, on whole pixels. The box is
/// `ground_attack_width` wide, centred `ground_attack_offset` in front of
/// the player, as tall as the body. Returns the things knocked out,
/// which are put out of action once they've moved this frame. A thrower
/// knocked out throws no more.
///
/// The spec measured walkers walking at full speed on the player's floor,
/// against which this gives its ranges, and a thrower only from its own
/// ledge; other enemies aren't knocked out.
fn ground_attacks(world: &mut World, physics: &Physics) -> Vec<usize> {
    let width = physics[Param::GroundAttackWidth];
    let attack_frames = physics[Param::GroundAttackFrames];
    let attacking = world.player.ground_action.is_some_and(|a| {
        clamber_pack::physics::steps(attack_frames).any(|n| n == u32::from(a.frame))
    });
    if width <= Fixed::ZERO || !attacking {
        return Vec::new();
    }
    let reach = attack_box(world, width, physics[Param::GroundAttackOffset]);
    strike(world, reach, |b| b.fall().is_some())
}

/// The player's attack box on whole pixels: `width` wide, centred
/// `offset` in front of the player, as tall as the body.
fn attack_box(world: &World, width: Fixed, offset: Fixed) -> (Fixed, Fixed, Fixed, Fixed) {
    let player = &world.player;
    let at = whole_pixels(player.pos);
    let middle = match player.facing {
        Facing::Left => at.x - offset,
        Facing::Right => at.x + offset,
    };
    let half_width = Fixed::from_bits(width.to_bits() / 2);
    (
        middle - half_width,
        at.y - HEIGHT,
        middle + half_width,
        at.y,
    )
}

/// The things whose behavior `kind` accepts that `reach` touches, before
/// they move, on whole pixels. Each is noted as defeated.
fn strike(
    world: &mut World,
    reach: (Fixed, Fixed, Fixed, Fixed),
    kind: impl Fn(Behavior) -> bool,
) -> Vec<usize> {
    let mut struck = Vec::new();
    for thing in world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
    {
        let Some(archetype) = thing.archetype else {
            continue;
        };
        if kind(archetype.behavior)
            && overlap(
                reach,
                archetype.bounds(whole_pixels(thing.pos), going(thing)),
            )
        {
            struck.push(thing.index);
            world.flow.events.push(FlowEvent::Defeat(thing.index));
        }
    }
    struck
}

/// The enemy that hurts the player on the next step, if any: one its
/// hurt box touches now, on whole pixels. The box is `hurt_width` wide,
/// centred `hurt_offset` in front of the player, or `hurt_fidget_width`
/// and `hurt_fidget_offset` in the fidget pose, or `hurt_air_width` and
/// `hurt_air_offset` in the airborne pose ([`pose::hurt_pose`]), and
/// reaches from the top of the body to the feet, or to `hurt_jump_lift`
/// (or the walker's own) above them while the airborne pose lifts it. A
/// rolling player isn't hurt, nor one in the first `roll_stop_guard`
/// frames of the stop after a roll.
fn hurt_ahead(world: &World, physics: &Physics) -> Option<usize> {
    let player = &world.player;
    let guarded = player.roll_stop > 0 && player.roll_stop <= frames(physics[Param::RollStopGuard]);
    if player.state == State::Roll || guarded {
        return None;
    }
    let at = whole_pixels(player.pos);
    let long = |p: Param| physics[p].floor().clamp(0, u32::MAX.into()) as u32;
    let fidget_time = (
        long(Param::FidgetStart),
        long(Param::FidgetLength),
        long(Param::FidgetEvery),
    );
    let fidget =
        physics[Param::HurtFidgetWidth] > Fixed::ZERO && pose::fidgeting(&player.pose, fidget_time);
    let (airborne, lifted) = pose::hurt_pose(player, physics);
    let (width, offset) = if airborne && physics[Param::HurtAirWidth] > Fixed::ZERO {
        (physics[Param::HurtAirWidth], physics[Param::HurtAirOffset])
    } else if fidget {
        (
            physics[Param::HurtFidgetWidth],
            physics[Param::HurtFidgetOffset],
        )
    } else {
        (physics[Param::HurtWidth], physics[Param::HurtOffset])
    };
    let middle = match player.facing {
        Facing::Left => at.x - offset,
        Facing::Right => at.x + offset,
    };
    let half_width = Fixed::from_bits(width.to_bits() / 2);
    let reach = |archetype: &Archetype| {
        let lift = match archetype.behavior {
            _ if !lifted => Fixed::ZERO,
            Behavior::Walker {
                hurt_jump_lift: Some(lift),
                ..
            } => lift,
            _ => physics[Param::HurtJumpLift],
        };
        (
            middle - half_width,
            at.y - HEIGHT,
            middle + half_width,
            at.y - lift,
        )
    };
    world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready && !has_contact(t))
        .find(|t| {
            t.archetype.is_some_and(|a| {
                a.behavior.is_enemy() && overlap(reach(&a), a.bounds(whole_pixels(t.pos), going(t)))
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
    let riding = player.riding.is_some();
    let rolling = player.state == State::Roll && !attack_boxes;
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let Some(archetype) = thing.archetype else {
            continue;
        };
        if thing.phase != Phase::Ready || has_contact(thing) {
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
            Behavior::Carry { .. }
            | Behavior::Collect { .. }
            | Behavior::Thrower { .. }
            | Behavior::Ball(_)
            | Behavior::Stinger(_) => {}
            _ if riding => {}
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
