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
//! - A thrower with a contact table hurts the player and is bounced on
//!   by its own ranges, which knocks it out, and with a `roll` range is
//!   rolled into on the attack frames, which knocks it out as a defeat by
//!   an attack box does (see [`throwers`](crate::throwers)).
//! - With a `ground_attack_width` above 0, the ground action knocks out
//!   walkers and throwers its attack box touches on its attack frames
//!   (see [`ground_attacks`]). For `ground_attack_pause` frames after,
//!   the player, every walker and every thrower hold still, and nothing
//!   hurts the player.
//! - While the player is knocked out, things don't act on it, and
//!   enemies other than the one that hurt it hold still for the frames
//!   it holds still after the hit. Throwers, knocked out or not, and
//!   balls hold still then too, the one that hurt it as well, and no
//!   ball is thrown, so they go on that many frames late. So do rising
//!   collectibles, which then rise on at the speed they had.
//! - With `bounce_holds_all`, nothing else moves on a bounce's frozen
//!   frames (see [`holds_all`]), and goes on from where it was after.
//! - With `bounce_again`, the player can bounce off another enemy before
//!   landing from a bounce, however fast it rises, and walkers don't
//!   hurt it until it lands (see [`bounce_on_boxes`]).
//! - While a character is being lost, things don't act on the player,
//!   and while the one that took over is protected, nothing hurts it
//!   (see [`loss`](crate::loss)).
//! - A launcher sends the player off at its velocity, then rests for
//!   [`LAUNCH_REST`] frames before it can fire again.
//!
//! A stinger sways around its start and hurts the player on any contact
//! (see [`stinger`]).
//!
//! A spring catches the player, puts it back on its top while squashed
//! and throws it (see [`spring`]).
//!
//! A rope sways, and the player catches it from a jump (see [`rope`]).
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
//! A defeated walker jumps up and falls out of the level (see [`fall`]).
//! With a view, it's removed on the first step it falls on, after it
//! moves, that its own position is outside the window around the picture
//! as it was at the start of the step: left or right of it, or below its
//! bottom edge (see [`fall_out`]). That's from the step after the hold
//! for one bounced on or rolled into, and from the second after the hit
//! for one hit by a thrown object, and from the step after the pause for
//! one the ground action knocked out, which doesn't touch the player.
//! Other defeated enemies show as defeated for [`DEFEATED_FRAMES`], then
//! are gone. They stay gone after the player loses a life.

use clamber_fixed::Fixed;
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetype, Behavior, Fall, Hit, Response};
use crate::carry;
use crate::columns;
use crate::critter;
use crate::exit;
use crate::flow::{FlowEvent, overlap};
use crate::hidden;
use crate::hutch;
use crate::join;
use crate::player::{Act, Facing, HEIGHT, SPEED_GRAIN, STEP, State, TINY, frames, is_wall};
use crate::pose::{self, Stance};
use crate::ride;
use crate::rope;
use crate::sections::Window;
use crate::sling;
use crate::spring;
use crate::stinger;
use crate::throwers::{self, Picture};
use crate::view;
use crate::world::{Event, Phase, Thing, World};

/// How long a defeated enemy shows before it's gone.
pub const DEFEATED_FRAMES: u8 = 20;

/// How long a launcher rests after firing.
pub const LAUNCH_REST: u8 = 20;

/// Moves every thing one step, then applies what the player touched.
/// `previous` is the player's position before this step; `frozen` says
/// the player was held still on it. Returns the walker with a contact
/// table that hurt the player on this step, if one did.
pub(crate) fn update(world: &mut World, previous: Vec2, frozen: bool) -> Option<usize> {
    let previous_feet = previous.y;
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
            || world.leaving.is_some()
            // Nor while a player knocked off an animal holds still.
            || (world.player.unseated && (frozen || world.player.frozen > 0))
    };
    // A rider meets walkers only by their rider tables.
    let riding = world.player.riding.is_some();
    if riding && !frozen && !knocked(world) {
        ride::attack(world);
    }
    // With `bounce_again`, nothing is bounced on during a bounce's frozen
    // frames (walker-contact.md, "During a bounce").
    let again = physics[Param::BounceAgain] > Fixed::ZERO;
    let bounce_frozen = again && frozen && world.player.state == State::Bounce;
    let mut bounced = if boxes && !riding && !knocked(world) && !bounce_frozen {
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
    // Throwers with a contact table are bounced on after walkers, and
    // knocked out once things have moved this frame.
    let mut bounced_throwers = Vec::new();
    if !frozen
        && !riding
        && !knocked(world)
        && !world.events.contains(&Event::Bounce)
        && let Some(index) = throwers::bounced_on(world, physics[Param::BounceMaxRise])
    {
        world.player.bounce(
            physics[Param::BounceSpeed],
            frames(physics[Param::BounceFreeze]),
            &mut world.events,
        );
        world.flow.events.push(FlowEvent::Defeat(index));
        bounced_throwers.push(index);
    }
    let attacked = if attack_boxes && !frozen && !knocked(world) {
        roll_attacks(world, &physics, previous)
    } else {
        Vec::new()
    };
    let rolled_thrower = if !frozen && !riding && !knocked(world) {
        throwers::rolled_into(world, &physics, previous)
    } else {
        None
    };
    if let Some(index) = rolled_thrower {
        world.flow.events.push(FlowEvent::Defeat(index));
    }
    // Walkers and throwers, knocked out or not, hold still on the ground
    // action's pause frames, and so do hidden objects' items.
    let paused = frozen && world.player.ground_action.is_some();
    // With `bounce_holds_all`, nothing else moves on a bounce's frozen
    // frames.
    let held_by_bounce_all = holds_all(world, frozen);
    hidden::note_fall(world);
    hidden::items(world, paused || held_by_bounce_all);
    let (struck, struck_hidden) = if !frozen && !knocked(world) {
        (
            ground_attacks(world, &physics),
            hidden::strike(world, &physics),
        )
    } else {
        (Vec::new(), false)
    };
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
    // Throwers and balls stop then too, the one that hurt it as well,
    // and none is thrown.
    let thrower_held = held.is_some() || held_by_bounce_all;
    // Objects meet balls where they were at the end of the last step.
    let balls = ball_places(world);
    if !thrower_held {
        throwers::throw(world);
    }
    // Balls leave by where the view was at the start of the step.
    let picture = world
        .view
        .filter(|_| view::by_view(&physics))
        .map(|v| Picture::of(v, &world.level, &world.feel, &physics));
    let ending = world.ending.is_some();
    // Critters fly by the world's frame and where the picture was, and
    // hold still on the frames that hold walkers still.
    let frame = world.frame;
    let view = world.view.map_or((0, 0), |v| (v.x(), v.y()));
    let critters_held =
        held.is_some() || waiting || held_by_bounce || held_by_roll || held_by_bounce_all;
    let mut freed = Vec::new();
    for thing in world.things.iter_mut().filter(|t| t.active) {
        if let Some(Behavior::Critter(critter)) = thing.archetype.map(|a| a.behavior) {
            if !critters_held && let Some(entity) = world.level.entities.get(thing.index) {
                critter::step(thing, entity.at, &critter, frame, view);
            }
            continue;
        }
        if held_by_bounce_all {
            continue;
        }
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
        // So do rising collectibles, keeping their speed (extra-lives.md,
        // "While the player is knocked out").
        let rising = thing.archetype.is_some_and(
            |a| matches!(a.behavior, Behavior::Collect { rise, .. } if rise > Fixed::ZERO),
        );
        if rising && held.is_some() {
            continue;
        }
        let thrown = thing
            .archetype
            .is_some_and(|a| matches!(a.behavior, Behavior::Thrower { .. } | Behavior::Ball(_)));
        if thrown && thrower_held {
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
            if advance(&world.level, thing, &physics, picture, ending) {
                freed.push(thing.pos);
            }
        }
    }
    freed.extend(throw_hits(world));
    let (held_freed, carried_into) = hands_hits(world, &physics);
    freed.extend(held_freed);
    meet_balls(world, &balls, &physics);
    if let Some(&at) = freed.first() {
        join::start(world, at);
    }
    if let Some(view) = world.view.filter(|_| view::by_view(&physics)) {
        // Thrown objects are removed far from the view as walkers are.
        let (left, wide) = view::window(&world.level, &world.feel, view.x(), &physics);
        for thing in world.things.iter_mut().filter(|t| t.active) {
            if matches!(thing.phase, Phase::Thrown(f) if f.thrown)
                && view::far(thing.pos.x.floor(), left, &wide)
            {
                thing.active = false;
            }
        }
    }
    exit::open(world);
    let untouchable = knocked(world);
    hutch::update(world, &physics, frozen, untouchable);
    if !frozen && !untouchable && world.swap.is_none() {
        hidden::land(world, &physics);
    }
    hidden::leave(world, &physics);
    spring::update(world, frozen, untouchable);
    rope::update(world);
    sling::update(world);
    // Enemies bounced on still moved this frame, as they were. When
    // everything holds still on the frozen frames, so do they, without
    // counting them.
    let freeze = if physics[Param::BounceHoldsAll] > Fixed::ZERO {
        0
    } else {
        frames(physics[Param::BounceFreeze])
    };
    let facing = world.player.facing;
    for index in bounced {
        bounce_off(&mut world.things[index], facing, freeze);
    }
    // A thrower holds still on the bounce's frozen frames too.
    for index in bounced_throwers {
        defeat(&mut world.things[index], freeze);
    }
    for &index in &attacked {
        rolled_over(&mut world.things[index], roll_freeze);
    }
    if let Some(index) = rolled_thrower {
        defeat(&mut world.things[index], roll_freeze);
    }
    for index in rolled_into {
        bounce_off(&mut world.things[index], facing, roll_freeze);
    }
    if !attacked.is_empty() || rolled_thrower.is_some() {
        world.player.defeat_rolling(roll_freeze, &physics);
    }
    // They hold still on the pause frames instead of holding here.
    for &index in &struck {
        defeat(&mut world.things[index], 0);
    }
    let pause = frames(physics[Param::GroundAttackPause]);
    if !struck.is_empty() || struck_hidden {
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
        // Nor do they hurt one in the air after a bounce, with
        // `bounce_again` (walker-contact.md, "During a bounce").
        let bouncing = again && world.player.state == State::Bounce;
        let walker = if hurt_boxes && !riding && !world.player.unseated && !bouncing {
            hurt_ahead(world, &physics, carried_into)
        } else {
            None
        };
        world.player.hurt_ahead = walker
            .or_else(|| throwers::hurt_ahead(world))
            .or_else(|| throwers::thrower_hurt_ahead(world))
            .or_else(|| stinger::hurt_ahead(world));
    }
    hurt_by
}

/// Whether, with `bounce_holds_all`, this step is one of a bounce's
/// frozen frames, on which nothing but the player's count goes on.
/// `frozen` says the player was held still at the start of the step.
pub(crate) fn holds_all(world: &World, frozen: bool) -> bool {
    frozen
        && world.player.state == State::Bounce
        && world.player_physics()[Param::BounceHoldsAll] > Fixed::ZERO
}

/// Moves a thing one step, `ending` during the walk-out after the end
/// marker. Returns whether it was an object holding the missing
/// character that broke.
fn advance(
    level: &Level,
    thing: &mut Thing,
    physics: &Physics,
    picture: Option<Picture>,
    ending: bool,
) -> bool {
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
        Phase::Falling { vy, hold, glide } => {
            if let Some(defeat) = archetype.behavior.fall() {
                fall(level, thing, &archetype, defeat, vy, hold, glide);
            }
            // A knocked-out thrower that moved leaves the picture as a
            // ball does, with a view.
            if let (
                Behavior::Thrower {
                    window: Some(window),
                    ..
                },
                Some(picture),
            ) = (archetype.behavior, picture)
                && hold == 0
                && !picture.holds(thing.pos, window)
            {
                thing.active = false;
            }
            if archetype.behavior.is_walker() && hold == 0 {
                fall_out(thing, physics, picture);
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
        Phase::Held
        | Phase::Broken
        | Phase::Item { .. }
        | Phase::Landed(_)
        | Phase::Squashed { .. }
        | Phase::Wobbling { .. }
        | Phase::Rope { .. }
        | Phase::Caught(_)
        | Phase::Gone
        | Phase::Open => return false,
        Phase::Knocked { vy } => {
            if let Behavior::Walker {
                response,
                range,
                defeat,
                ..
            } = archetype.behavior
            {
                knocked(level, thing, &archetype, response, range, defeat, vy);
                // From the step after it moved along the floor.
                if vy.is_some() {
                    fall_out(thing, physics, picture);
                }
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
        Phase::LetGo { .. } => {
            carry::fall_away(level, thing, archetype.size, physics, picture, ending);
            return false;
        }
        Phase::Flying { .. } => {
            if let Behavior::Ball(ball) = archetype.behavior {
                throwers::fly(level, thing, &ball, archetype.size, picture);
            }
            return false;
        }
        // Crates holding an animal move in [`hutch::update`].
        Phase::Opened(_)
        | Phase::Mounted(_)
        | Phase::Ridden
        | Phase::Bolting { .. }
        | Phase::Waiting(_) => {
            return false;
        }
        Phase::Swaying { .. } => {
            if let Behavior::Stinger(stinger) = archetype.behavior {
                stinger::sway(thing, level.entities[thing.index].at, &stinger);
            }
            return false;
        }
        // Critters move in `update`.
        Phase::Flitting { .. } => return false,
        Phase::Ready => {
            if let Behavior::Stinger(stinger) = archetype.behavior {
                stinger::start(thing, level.entities[thing.index].at, &stinger);
                return false;
            }
        }
    }
    if let Behavior::Collect { rise, .. } = archetype.behavior
        && rise > Fixed::ZERO
        && let Some(entity) = level.entities.get(thing.index)
    {
        rise_from(thing, entity.at.y, rise);
        return false;
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
        | Behavior::Stinger(_)
        | Behavior::Keg(_)
        | Behavior::Entrance(_)
        | Behavior::Hidden(_)
        | Behavior::Spring(_)
        | Behavior::Rope(_)
        | Behavior::Sling(_)
        | Behavior::Critter(_)
        | Behavior::Scenery => {}
    }
    false
}

/// Moves a rising collectible on a step: `n` steps after the one it
/// appeared on at level y `from`, its speed up is `n` times `rise`, so
/// it has risen `rise` times n (n + 1) / 2 (extra-lives.md, "Rising").
/// It's gone for the rest of the visit on the first step that would take
/// it to the level's top or above it.
fn rise_from(thing: &mut Thing, from: Fixed, rise: Fixed) {
    let n = i64::from(thing.clock);
    thing.clock = thing.clock.saturating_add(1);
    let risen = n * (n + 1) / 2;
    let y = Fixed::from_bits(
        from.to_bits()
            .saturating_sub(rise.to_bits().saturating_mul(risen)),
    );
    if y <= Fixed::ZERO {
        thing.active = false;
        thing.phase = Phase::Broken;
    } else {
        thing.pos.y = y;
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

/// Defeats an enemy. A walker or a thrower falls out of the level after
/// holding still for `hold` frames; anything else shows as defeated for a
/// while.
fn defeat(thing: &mut Thing, hold: u8) {
    thing.phase = match thing.archetype.and_then(|a| a.behavior.fall()) {
        Some(defeat) => Phase::Falling {
            vy: -defeat.jump,
            hold,
            glide: None,
        },
        None => Phase::Defeated(DEFEATED_FRAMES),
    };
}

/// Defeats an enemy a roll's attack box touched, as [`defeat`] does. A
/// walker's speed across then eases toward the speed it was heading for
/// on this frame as it falls: none if it has just stopped at a wall or a
/// ledge, as its speed up only starts on the next frame
/// (rolling-into-walkers.md, "Walkers that are speeding up or turning").
fn rolled_over(thing: &mut Thing, hold: u8) {
    defeat(thing, hold);
    if let (Some(Behavior::Walker { speed, .. }), Phase::Falling { glide, .. }) =
        (thing.archetype.map(|a| a.behavior), &mut thing.phase)
    {
        *glide = Some(if thing.vx == Fixed::ZERO {
            Fixed::ZERO
        } else if thing.heading == Facing::Left {
            -speed
        } else {
            speed
        });
    }
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
/// the fall's gravity, through anything, at the speed it had across, or
/// with a `glide`, at its speed eased toward that by the walker's
/// `response` first. It's gone once its top is below the level.
fn fall(
    level: &Level,
    thing: &mut Thing,
    archetype: &Archetype,
    defeat: Fall,
    vy: Fixed,
    hold: u8,
    glide: Option<Fixed>,
) {
    if hold > 0 {
        thing.phase = Phase::Falling {
            vy,
            hold: hold - 1,
            glide,
        };
        return;
    }
    if let (Some(target), Behavior::Walker { response, .. }) = (glide, archetype.behavior) {
        thing.vx = ease(thing.vx, target, response);
    }
    let next = (vy + defeat.gravity).min(defeat.max_fall);
    let by = if defeat.move_first { vy } else { next };
    thing.pos = Vec2::new(thing.pos.x + thing.vx, thing.pos.y + by);
    thing.phase = Phase::Falling {
        vy: next,
        hold: 0,
        glide,
    };
    if thing.pos.y - archetype.size.y > level.size.y {
        thing.active = false;
    }
}

/// Removes a defeated walker that has left the window around the
/// picture where it was at the start of the step, wherever its start is
/// (walker-view.md, "Defeated walkers falling out"). Only with a view,
/// and only on the steps it falls on.
fn fall_out(thing: &mut Thing, physics: &Physics, picture: Option<Picture>) {
    if picture.is_some_and(|p| p.lost(thing.pos, &Window::of(physics))) {
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
/// An object holding nothing also knocks out the first thrower its
/// thrower box holds, and goes on as it was (see [`knock_out`]).
///
/// One holding the character that hits no walker breaks on the first
/// thrower or ball, in that order, whose position its thrower or ball box
/// holds (see [`Holder`](crate::Holder)). A thrower it breaks on is
/// knocked out the same way: it holds still on the next step, the last
/// of the join's (see [`join`]), and leaves from the one after. A ball
/// it breaks on is gone.
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
            thrower,
            thrower_early,
            thrower_turn,
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
        let walker = hit_walker(world, reach, right).is_some();
        if walker && holder.is_some() {
            world.things[i].active = false;
            freed.push(pos);
            continue;
        }
        let thrower = if early { thrower_early } else { thrower };
        let thrower = thrower_at(world, thrower, thrower_turn, pos, right);
        let Some(holder) = holder else {
            if let Some(index) = thrower {
                knock_out(world, index);
            }
            continue;
        };
        match thrower {
            Some(index) => knock_out(world, index),
            None => {
                let ball = if early {
                    holder.ball_early
                } else {
                    holder.ball
                };
                let target = world.things.iter_mut().find(|t| {
                    t.active
                        && t.archetype
                            .is_some_and(|a| matches!(a.behavior, Behavior::Ball(_)))
                        && matches!(t.phase, Phase::Ready | Phase::Flying { .. })
                        && ball.holds(pos, right, t.pos)
                });
                let Some(target) = target else {
                    continue;
                };
                target.active = false;
            }
        }
        world.things[i].active = false;
        freed.push(pos);
    }
    freed
}

/// The first thrower that isn't knocked out whose position an object at
/// `pos`, moving right if `right`, holds in its thrower box `hit`, moved
/// `turn` px left moving left (see [`Behavior::Carry`]).
fn thrower_at(world: &World, hit: Hit, turn: Fixed, pos: Vec2, right: bool) -> Option<usize> {
    let pos = if right {
        pos
    } else {
        Vec2::new(pos.x - turn, pos.y)
    };
    world.things.iter().position(|t| {
        t.active
            && t.phase == Phase::Ready
            && t.archetype
                .is_some_and(|a| matches!(a.behavior, Behavior::Thrower { .. }))
            && hit.holds(pos, right, t.pos)
    })
}

/// Knocks out a thrower an object met: it holds still on the next step
/// too, then falls as a thrower knocked out by a bounce does, a step
/// sooner (thrown-objects.md, "Hitting the thrower"). The speed across
/// it's given on the hit isn't kept: it's 0 again before it moves.
fn knock_out(world: &mut World, index: usize) {
    let thrower = &mut world.things[index];
    if let Some(defeat) = thrower.archetype.and_then(|a| a.behavior.fall()) {
        thrower.phase = Phase::Falling {
            vy: -defeat.jump,
            hold: 1,
            glide: None,
        };
        world.flow.events.push(FlowEvent::Defeat(thrower.index));
    }
}

/// Knocks back the first walker that's walking whose box overlaps
/// `reach`, the way an object moving right if `right` came, at `knock`.
/// Returns its index, if there was one.
fn hit_walker(
    world: &mut World,
    reach: (Fixed, Fixed, Fixed, Fixed),
    right: bool,
) -> Option<usize> {
    let walker = world.things.iter_mut().find(|t| {
        t.active
            && t.phase == Phase::Ready
            && t.archetype.is_some_and(|a| {
                matches!(a.behavior, Behavior::Walker { .. })
                    && in_reach(reach, &a, t.pos, t.facing)
            })
    });
    let walker = walker?;
    let Some(Behavior::Walker { defeat, .. }) = walker.archetype.map(|a| a.behavior) else {
        return None;
    };
    (walker.heading, walker.vx) = if right {
        (Facing::Left, -defeat.knock)
    } else {
        (Facing::Right, defeat.knock)
    };
    walker.phase = Phase::Knocked { vy: None };
    world.flow.events.push(FlowEvent::Defeat(walker.index));
    Some(walker.index)
}

/// A walker the object in the player's hands meets knocks back and
/// breaks it, as thrown-objects.md ("A carried object") describes. During
/// a throw, after its first frame and before the object leaves the
/// hands, that's a walker its `hit_early` box touches the way the player
/// faces. With a `carried_turn`, carried (after the frame the carrying
/// starts), on a pick-up's frames after its first and on a put-down's
/// before it's set down, it's one its `hit` box touches as if thrown
/// right, moved `carried_turn` px right with the player facing left. Not
/// on a frame the player bounces. The walker is knocked back as by one
/// thrown right, whichever way the player faces ("During a pick-up, a
/// throw or a put-down"). Where the object is in the hands is
/// [`carry::in_hands`].
///
/// One holding nothing breaks in the hands on a thrower its
/// `thrower_early` box holds during a throw, and while carried on one
/// its `thrower` box holds, the way the player faces, and knocks it out
/// (see [`knock_out`]); a walker in reach on the same frame isn't hit.
///
/// Carried, the player is no longer carrying it, and keeps the carrying
/// pose until it lands if it's in the air. A pick-up, a throw or a
/// put-down goes on to its end without it. The walker it broke on is
/// returned: it can still hurt the player on the next step it moves.
/// One holding the missing character frees it where it was in the hands
/// (holder-breaking.md, "Carried into a walker"); the world waits, and
/// the walker can hurt the player on the first step after the wait. In
/// the hands it doesn't break on throwers.
/// Engine choices: walkers of every size are hit by the box (the spec
/// measured medium and large walkers hit while carried and small ones out
/// of reach).
fn hands_hits(world: &mut World, physics: &Physics) -> (Option<Vec2>, Option<usize>) {
    let Some(hands) = world.player.hands else {
        return (None, None);
    };
    let release = frames(physics[Param::ThrowRelease]);
    let set_down = frames(physics[Param::PutDownTime]).saturating_sub(1);
    let throwing = hands.act == Act::Throw && hands.frame > 0 && hands.frame < release;
    let lifting = match hands.act {
        Act::PickUp => hands.frame > 0,
        Act::PutDown => hands.frame < set_down,
        _ => false,
    };
    let carried = hands.act == Act::Carry;
    if !throwing && !lifting && !carried {
        return (None, None);
    }
    let object = &world.things[hands.thing];
    if !object.active || object.phase != Phase::Held {
        return (None, None);
    }
    let Some(Behavior::Carry {
        hit,
        hit_early,
        thrower,
        thrower_early,
        thrower_turn,
        carried_turn,
        holder,
        ..
    }) = object.archetype.map(|a| a.behavior)
    else {
        return (None, None);
    };
    let pos = carry::in_hands(&world.player, object);
    let right = world.player.facing == Facing::Right;
    let box_ = if throwing { thrower_early } else { thrower };
    let thrower =
        thrower_at(world, box_, thrower_turn, pos, right).filter(|_| holder.is_none() && !lifting);
    let mut walker = None;
    if let Some(index) = thrower {
        knock_out(world, index);
    } else {
        let bounced = world.events.contains(&Event::Bounce);
        let held = carried && hands.frame > frames(physics[Param::PickUpTime]);
        let reach = if bounced {
            None
        } else if throwing {
            Some(hit_early.bounds(pos, right))
        } else {
            carried_turn.filter(|_| held || lifting).map(|turn| {
                let at = if right {
                    pos
                } else {
                    Vec2::new(pos.x + turn, pos.y)
                };
                hit.bounds(at, true)
            })
        };
        let Some(reach) = reach else {
            return (None, None);
        };
        walker = hit_walker(world, reach, true);
        if walker.is_none() {
            return (None, None);
        }
        if carried && !world.player.grounded() {
            world.player.pose.carried_air = true;
        }
    }
    // Its pieces aren't in the spec.
    world.things[hands.thing].active = false;
    if carried {
        world.player.hands = None;
    }
    let freed = holder.map(|_| pos);
    (freed, walker)
}

/// Each ball there is now: its index, where it is, and the part of its
/// turn it's in ([`Contact::part`](crate::Contact::part)).
fn ball_places(world: &World) -> Vec<(usize, Vec2, Option<usize>)> {
    world
        .things
        .iter()
        .enumerate()
        .filter(|(_, t)| t.active)
        .filter_map(|(i, t)| match t.archetype.map(|a| a.behavior) {
            Some(Behavior::Ball(ball)) => {
                let part = ball.contact.and_then(|c| c.part(t.clock));
                Some((i, t.pos, part))
            }
            _ => None,
        })
        .collect()
}

/// Objects holding nothing meet the balls in `before`, by where each was
/// and the part of its turn it was in at the end of the last step, with
/// the object where it is now (thrown-objects.md, "Hitting a ball"; see
/// [`Meets`](crate::Meets)). Every ball an object meets is gone. A thrown
/// one goes on as without them. One in the hands during a throw, after
/// its first frame and before it leaves them, or carried, breaks, and the
/// throw goes on without it; carried, the player is no longer carrying
/// it.
fn meet_balls(world: &mut World, before: &[(usize, Vec2, Option<usize>)], physics: &Physics) {
    if before.is_empty() {
        return;
    }
    let hands = world.player.hands;
    let release = frames(physics[Param::ThrowRelease]);
    for i in 0..world.things.len() {
        let object = &world.things[i];
        let Some(Behavior::Carry {
            balls: Some(meets),
            holder: None,
            hit_early_time,
            ..
        }) = object.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        if !object.active {
            continue;
        }
        let (pos, right, early, held) = match object.phase {
            Phase::Thrown(flight) if flight.thrown => (
                object.pos,
                flight.right(),
                flight.age <= hit_early_time,
                false,
            ),
            Phase::Held => {
                let Some(hands) = hands.filter(|h| h.thing == i) else {
                    continue;
                };
                let throwing = hands.act == Act::Throw && hands.frame > 0 && hands.frame < release;
                if !throwing && hands.act != Act::Carry {
                    continue;
                }
                let right = world.player.facing == Facing::Right;
                (
                    carry::in_hands(&world.player, object),
                    right,
                    throwing,
                    true,
                )
            }
            _ => continue,
        };
        let met: Vec<usize> = before
            .iter()
            .filter(|&&(ball, at, part)| {
                world.things[ball].active && meets.holds(pos, right, early, at, part)
            })
            .map(|&(ball, ..)| ball)
            .collect();
        if met.is_empty() {
            continue;
        }
        // What appears in a ball's place isn't in the spec.
        for ball in met {
            world.things[ball].active = false;
        }
        if held {
            // Its pieces aren't in the spec.
            world.things[i].active = false;
            if hands.is_some_and(|h| h.act == Act::Carry) {
                world.player.hands = None;
            }
        }
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
/// way it's heading by its `response`, before or after it moves by its
/// `move_first`. If a wall is in the way, it's put against it; if a
/// ledge is, it stays put. Either way it stops and turns around, its
/// facing following `turn_delay` frames after the next. With
/// `stops_at_x`, the front of its box is at its `x` for both, so a wall
/// stops it with its `x` at the wall (walker.md, "Turning at a
/// barrier").
fn walk(level: &Level, thing: &mut Thing, speed: Fixed, archetype: &Archetype, turn_delay: u8) {
    let Behavior::Walker {
        response,
        move_first,
        columns,
        stops_at_x,
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
    let front = match stops_at_x {
        true => Fixed::ZERO,
        false => Fixed::from_bits(size.x.to_bits() / 2),
    };
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
        .sweep(probe, cast_dir, speed + front, reach, is_wall)
        .filter(|hit| hit.distance < speed + front);
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
    let ground = ground_at(x + sign * front).and_then(|_| ground_at(foot));
    match (wall, ground) {
        (None, Some(hit)) => {
            thing.pos = Vec2::new(x, hit.point.y);
            thing.vx = eased;
        }
        _ => {
            if let Some(hit) = wall {
                let gap = (hit.distance - front).max(Fixed::ZERO);
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
/// rolling, or rising at `bounce_max_rise` or faster doesn't bounce,
/// nor one throwing after the throw's first frame and up to the one the
/// object leaves the hands on (walker-contact.md, "Carrying"). With
/// `bounce_again`, a player in the air after a bounce bounces however
/// fast it rises, and only off the first placed enemy in reach.
/// Returns the enemies bounced on, which are defeated once they've
/// moved this frame.
fn bounce_on_boxes(world: &mut World, physics: &Physics) -> Vec<usize> {
    let mut bounced = Vec::new();
    let player = &mut world.player;
    let max_rise = physics[Param::BounceMaxRise];
    let again = physics[Param::BounceAgain] > Fixed::ZERO;
    let rising_fast = if again && player.state == State::Bounce {
        false
    } else if max_rise > Fixed::ZERO {
        player.vel.y <= -max_rise
    } else {
        player.vel.y.is_negative()
    };
    let release = frames(physics[Param::ThrowRelease]);
    let throwing = player
        .hands
        .is_some_and(|h| h.act == Act::Throw && h.frame > 0 && h.frame <= release);
    if player.grounded() || player.state == State::Roll || rising_fast || throwing {
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
            if again {
                break;
            }
        }
    }
    bounced
}

/// Defeats an enemy the rolling player's attack box touches on an
/// attack frame, from the player's position after this step's move and
/// the enemies' before theirs, on whole pixels, and by the way they
/// faced then: every frame of a timed roll before its `roll_end` frame,
/// and the first `roll_attack_stop` frames of the stop after one. The
/// box is `roll_attack_width` wide, centred `roll_attack_offset` in
/// front of the player, as tall as the body. On a roll's first frame
/// (its `roll_start`, or a boost's first frame) it reaches back to where
/// the box was from `previous`, the player's position before this step.
/// With several in reach, it defeats the first placed, or all of them on
/// a boost's first frame. Returns the enemies defeated, which are put
/// out of action once they've moved this frame.
fn roll_attacks(world: &mut World, physics: &Physics, previous: Vec2) -> Vec<usize> {
    let player = &world.player;
    let stop = player.roll_stop > 0 && player.roll_stop <= frames(physics[Param::RollAttackStop]);
    if player.roll.is_none() && !stop {
        return Vec::new();
    }
    let width = physics[Param::RollAttackWidth];
    let offset = physics[Param::RollAttackOffset];
    let mut reach = attack_box(world, player.pos, width, offset);
    if player.boosted || world.events.contains(&Event::RollStart) {
        let (left, _, right, _) = attack_box(world, previous, width, offset);
        reach.0 = reach.0.min(left);
        reach.2 = reach.2.max(right);
    }
    // Several in reach: only a boost's first frame defeats more than one.
    let most = if player.boosted { usize::MAX } else { 1 };
    strike(world, reach, |b| b.is_enemy(), |t| t.facing, most)
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
    let reach = attack_box(
        world,
        world.player.pos,
        width,
        physics[Param::GroundAttackOffset],
    );
    strike(world, reach, |b| b.fall().is_some(), going, usize::MAX)
}

/// The player's attack box on whole pixels with the player at `pos`:
/// `width` wide, centred `offset` in front of it, as tall as the body.
fn attack_box(
    world: &World,
    pos: Vec2,
    width: Fixed,
    offset: Fixed,
) -> (Fixed, Fixed, Fixed, Fixed) {
    let player = &world.player;
    let at = whole_pixels(pos);
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

/// The first `most` things whose behavior `kind` accepts that `reach`
/// touches, before they move, on whole pixels, their boxes turned the
/// way `facing` says. Each is noted as defeated.
fn strike(
    world: &mut World,
    reach: (Fixed, Fixed, Fixed, Fixed),
    kind: impl Fn(Behavior) -> bool,
    facing: fn(&Thing) -> Facing,
    most: usize,
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
        if struck.len() < most
            && kind(archetype.behavior)
            && overlap(
                reach,
                archetype.bounds(whole_pixels(thing.pos), facing(thing)),
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
/// and `hurt_fidget_offset` in the fidget pose, `hurt_crouch_width` and
/// `hurt_crouch_offset` in the crouching pose ([`pose::hurt_crouching`]),
/// `hurt_carry_width` and `hurt_carry_offset` in the carrying pose
/// ([`pose::hurt_carrying`]), which is never airborne, or
/// `hurt_air_width` and `hurt_air_offset` in the airborne pose
/// ([`pose::hurt_pose`]), and reaches from the top of the body to the feet, or to `hurt_jump_lift`
/// (or the walker's own) above them while the airborne pose lifts it. A
/// rolling player isn't hurt, nor one in the first `roll_stop_guard`
/// frames of the stop after a roll. A walker a carried object knocked
/// back on this step, `carried_into`, still counts as walking.
fn hurt_ahead(world: &World, physics: &Physics, carried_into: Option<usize>) -> Option<usize> {
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
    let carrying =
        physics[Param::HurtCarryWidth] > Fixed::ZERO && pose::hurt_carrying(player, physics);
    let (airborne, lifted) = if carrying {
        (false, false)
    } else {
        pose::hurt_pose(player, physics)
    };
    let (width, offset) = if carrying {
        (
            physics[Param::HurtCarryWidth],
            physics[Param::HurtCarryOffset],
        )
    } else if airborne && physics[Param::HurtAirWidth] > Fixed::ZERO {
        (physics[Param::HurtAirWidth], physics[Param::HurtAirOffset])
    } else if !airborne
        && physics[Param::HurtCrouchWidth] > Fixed::ZERO
        && pose::hurt_crouching(player)
    {
        (
            physics[Param::HurtCrouchWidth],
            physics[Param::HurtCrouchOffset],
        )
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
        .filter(|t| {
            t.active
                && (t.phase == Phase::Ready || carried_into == Some(t.index))
                && !has_contact(t)
        })
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
    for (i, thing) in world.things.iter_mut().enumerate() {
        let Some(archetype) = thing.archetype.filter(|_| thing.active) else {
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
            | Behavior::Stinger(_)
            | Behavior::Keg(_)
            | Behavior::Entrance(_)
            | Behavior::Hidden(_)
            | Behavior::Spring(_)
            | Behavior::Rope(_)
            | Behavior::Sling(_)
            | Behavior::Scenery => {}
            _ if riding => {}
            _ => {
                let middle = Fixed::from_bits((area.1.to_bits() + area.3.to_bits()) / 2);
                let landed = !boxes && previous_feet <= middle && player.pos.y >= previous_feet;
                if !rolling && !landed {
                    if hurt_boxes {
                        continue;
                    }
                    world.events.push(Event::Hurt);
                    world.hit_by = Some(i);
                    return;
                }
                if !rolling {
                    player.bounce(bounce, freeze, &mut world.events);
                }
                let holds_all = physics[Param::BounceHoldsAll] > Fixed::ZERO;
                defeat(thing, if rolling || holds_all { 0 } else { freeze });
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
        let medium = resolve("crawler", "size = [25, 25]\noffset = 5.0");
        let large = resolve("crawler", "size = [26, 38]\noffset = 0.5");
        use Facing::{Left, Right};
        // Walker, its facing, the object moving right, then I's range and
        // U's limit early and late.
        let rows = [
            (small, Left, true, (-48, 28), (-23, 27), 63, 30),
            (small, Left, false, (-29, 47), (-28, 22), 63, 30),
            (small, Right, false, (-28, 48), (-27, 23), 63, 30),
            (small, Right, true, (-47, 29), (-22, 28), 63, 30),
            (medium, Left, true, (-55, 26), (-30, 25), 76, 43),
            (medium, Left, false, (-36, 45), (-35, 20), 76, 43),
            (medium, Right, false, (-26, 55), (-25, 30), 76, 43),
            (medium, Right, true, (-45, 36), (-20, 35), 76, 43),
            (large, Left, true, (-51, 31), (-26, 30), 89, 56),
            (large, Left, false, (-32, 50), (-31, 25), 89, 56),
            (large, Right, false, (-31, 51), (-30, 26), 89, 56),
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
