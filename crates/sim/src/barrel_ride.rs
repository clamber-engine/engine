//! Riding a thrown metal barrel (spec/behavior/metal-barrel.md, "The
//! characters").
//!
//! A first character falling in the air is caught by a metal barrel
//! rolling right on the frame its fall would end 17.5 px above the
//! barrel's `y` or lower, if the horizontal distance to the barrel's `x`
//! (as it is at the end of that frame) is in range for the way it faces.
//! The spec gives each end of a range as a gap between the last distance
//! caught and the first not; the engine uses the middle of the gap.
//!
//! The catch frame C puts the character 16.5 px above the barrel and
//! 7.75 behind it (the spec measured 7.3 to 8.2, varying from catch to
//! catch). On C + 1 it stays; from C + 2 on it moves with the barrel, at
//! the distance the barrel was ahead after C + 1. Nothing but Jump acts
//! while riding, and Jump only from C + 2: a jump taking off at the
//! barrel's speed across, which then eases away as in any jump.
//!
//! The second character is caught the same way, with ranges of its own
//! and a height band for where it is at the start of the frame (14.11 to
//! 45.89 px above the barrel's `y` caught, 47.14 or more and 11.48 or
//! less not; the middle of each gap is used). It rides 16.7 px above the
//! barrel and 8.15 behind it on the catch frame.
//!
//! A Jump press that began on C, C - 1 or C - 2 and is still held on C + 2
//! jumps on C + 2; one begun on C + 1 never does.
//!
//! A barrel still in the air after a throw catches the same way, the second
//! character too (spec v0.350.0); one standing on the floor is never
//! touched. A barrel rolling left catches the first character the same
//! way, mirrored, with ranges of its own and the character 8.4 px to its
//! right on the catch frame (spec v0.352.0); it catches no second
//! character, which the spec has not measured.
//!
//! A barrel with a rider falls more slowly: its downward speed grows by
//! 0.3125 px each frame instead of the free barrel's `throw_gravity`
//! (the spec saw 0.3086 once; the engine uses 0.3125 throughout).
//!
//! Engine choices where the spec says nothing:
//!
//! - Only the characters under control catch, and not while holding an
//!   object, riding an animal, swapping, lost or hurt.
//! - The ride ends when the barrel no longer moves right (it turned at a
//!   wall, or it is gone): the character is put on the floor at its own
//!   `x` on that frame, or falls if there is none.
//! - The height follows the barrel's `y` exactly; the spec's wobble of
//!   under 1 px isn't copied.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::Vec2;
use clamber_pack::{Param, Physics};

use crate::archetype::Behavior;
use crate::columns;
use crate::player::{Astride, Facing, State};
use crate::world::{Event, Phase, Thing, World};

/// How high above the barrel's `y` the first character's feet are.
const RISE: f64 = 16.5;
/// How far behind the barrel's `x` the first character is on the catch
/// frame.
const GAP: f64 = 7.75;
/// The same for a barrel rolling left, the character being to its right
/// (the spec measured 7.98 to 8.89).
const GAP_LEFT: f64 = 8.4;
/// The same for the second character.
const RISE_SECOND: f64 = 16.7;
const GAP_SECOND: f64 = 8.15;
/// The band the second character's `y` at the start of a frame is in, in
/// px above the barrel's `y`, for the barrel to catch it.
const BAND_SECOND: (f64, f64) = (12.795, 46.515);
/// How much a barrel's downward speed grows each frame while it carries
/// a rider, in px, instead of `throw_gravity`.
const RIDER_GRAVITY: f64 = 0.3125;
/// The lowest a falling character's feet can end a frame, above the
/// barrel's `y`, for the barrel to catch it.
const REACH: f64 = 17.5;

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).expect("a value Fixed holds")
}

/// The distances (character's `x` minus the barrel's) it is caught at.
fn range(character: usize, facing: Facing, left: bool) -> (Fixed, Fixed) {
    match (character, facing) {
        // Rolling left (spec v0.352.0), the first character only.
        (0, Facing::Left) if left => (fx(-26.537), fx(23.799)),
        (0, Facing::Right) if left => (fx(-23.615), fx(27.0685)),
        (0, Facing::Left) => (fx(-26.324), fx(24.176)),
        (0, Facing::Right) => (fx(-23.084), fx(27.477)),
        (_, Facing::Left) => (fx(-20.533), fx(23.664)),
        (_, Facing::Right) => (fx(-26.111), fx(17.9255)),
    }
}

/// How high above the barrel's `y` the character rides.
fn rise(character: usize) -> Fixed {
    fx(if character == 0 { RISE } else { RISE_SECOND })
}

/// The speed across of the thrown barrel `thing` if it's a metal one
/// rolling the way a rider with this `gap` behind it is carried: right
/// for a gap in front of the barrel's `x`, left for one beyond it.
fn rolling_with(world: &World, thing: usize, gap: Fixed) -> Option<Fixed> {
    let vx = rolling(world, thing)?;
    ((vx > Fixed::ZERO) == (gap > Fixed::ZERO)).then_some(vx)
}

/// The speed across of the thrown metal barrel `thing` if it's moving
/// sideways.
fn rolling(world: &World, thing: usize) -> Option<Fixed> {
    let thing = &world.things[thing];
    let turns = matches!(
        thing.archetype.as_ref()?.behavior,
        Behavior::Carry {
            turns_at_walls: true,
            ..
        }
    );
    match thing.phase {
        Phase::Thrown(f) if thing.active && turns && f.vx != Fixed::ZERO => Some(f.vx),
        _ => None,
    }
}

/// Sets the gravity of a thrown barrel `thing` for its next fall: the
/// rider's when it `carries` one, the preset's `throw_gravity` again once
/// it doesn't.
pub(crate) fn weigh(thing: &mut Thing, physics: &Physics, carries: bool) {
    if let Phase::Thrown(f) = &mut thing.phase {
        if carries {
            f.gravity = fx(RIDER_GRAVITY);
        } else if f.gravity == fx(RIDER_GRAVITY) {
            f.gravity = physics[Param::ThrowGravity];
        }
    }
}

/// One step of a player riding a barrel, if it is. Returns whether it
/// was, so the step's moves are done.
pub(crate) fn hold(world: &mut World, input: &Frame) -> bool {
    let Some(mut on) = world.player.astride else {
        return false;
    };
    if world.player.state != State::Astride {
        world.player.astride = None;
        return false;
    }
    let physics = world.player_physics();
    let vx = rolling_with(world, on.thing, on.gap);
    let p = &mut world.player;
    p.touched.clear();
    p.update_jump_held(input);
    // A press never waits: before the ride allows it, it is lost.
    p.jump_buffer.take();
    on.age = on.age.saturating_add(1);
    match vx {
        // A press begun on C or before and still held on C + 2 jumps then.
        Some(vx)
            if on.age >= 2
                && (input.pressed.contains(Action::Jump) || (on.age == 2 && p.jump_held >= 3)) =>
        {
            p.jump_off_barrel(vx, -physics[Param::JumpSpeed]);
            world.events.push(Event::JumpStart);
        }
        _ => p.astride = Some(on),
    }
    true
}

/// After the things have moved on a step: a falling character is caught,
/// a rider goes with its barrel or is put down.
pub(crate) fn after(world: &mut World, start_y: Fixed) {
    if world.player.astride.is_some() {
        ride_on(world);
    } else {
        catch(world, start_y);
    }
}

fn ride_on(world: &mut World) {
    let Some(mut on) = world.player.astride else {
        return;
    };
    if world.player.state != State::Astride {
        world.player.astride = None;
        return;
    }
    let Some(vx) = rolling_with(world, on.thing, on.gap) else {
        put_down(world);
        return;
    };
    let barrel = world.things[on.thing].pos;
    let p = &mut world.player;
    if on.age == 1 {
        on.gap = barrel.x - p.pos.x;
    } else if on.age >= 2 {
        p.pos.x = barrel.x - on.gap;
        p.vel.x = vx;
    }
    p.pos.y = barrel.y - rise(p.character);
    p.astride = Some(on);
}

/// Ends a ride: the character stands on the floor at its own `x`.
fn put_down(world: &mut World) {
    let column = columns::column(world.player.pos.x);
    let feet = world.player.pos.y + rise(world.player.character);
    let floor = columns::floor(&world.level, column, feet);
    let p = &mut world.player;
    p.astride = None;
    p.vel = Vec2::default();
    p.target = Fixed::ZERO;
    match floor {
        Some(floor) => {
            p.pos.y = floor.y;
            p.ground = Some(floor.collider);
            p.touched.push(floor.collider);
            p.state = State::Idle;
        }
        None => p.state = State::Fall,
    }
}

fn catch(world: &mut World, start_y: Fixed) {
    let p = &world.player;
    if p.state != State::Fall
        || p.grounded()
        || !p.vel.y.to_bits().is_positive()
        || p.hands.is_some()
        || p.riding.is_some()
        || p.rope.is_some()
        || p.knockout.is_some()
        || world.swap.is_some()
        || world.losing()
    {
        return;
    }
    let found = (0..world.things.len()).find_map(|i| {
        let vx = rolling(world, i)?;
        // Only the first character is known to be caught by a barrel
        // rolling left.
        let left = vx < Fixed::ZERO;
        if left && p.character != 0 {
            return None;
        }
        let (low, high) = range(p.character, p.facing, left);
        let caught = {
            let barrel = world.things[i].pos;
            let d = p.pos.x - barrel.x;
            let height = if p.character == 0 {
                p.pos.y >= barrel.y - fx(REACH)
            } else {
                let above = barrel.y - start_y;
                fx(BAND_SECOND.0) <= above && above <= fx(BAND_SECOND.1)
            };
            low <= d && d <= high && height
        };
        caught.then_some((i, left))
    });
    let Some((thing, left)) = found else {
        return;
    };
    let barrel = world.things[thing].pos;
    let (gap, rise) = if world.player.character == 0 {
        (fx(GAP), rise(0))
    } else {
        (fx(GAP_SECOND), rise(1))
    };
    let gap = if left { -fx(GAP_LEFT) } else { gap };
    let at = Vec2::new(barrel.x - gap, barrel.y - rise);
    world
        .player
        .catch_on_barrel(Astride { thing, age: 0, gap }, at);
}
