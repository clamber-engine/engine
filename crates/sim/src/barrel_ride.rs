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
//! Engine choices where the spec says nothing:
//!
//! - Only the first character catches, and not while holding an object,
//!   riding an animal, swapping, lost or hurt.
//! - The ride ends when the barrel no longer moves right (it turned at a
//!   wall, or it is gone): the character is put on the floor at its own
//!   `x` on that frame, or falls if there is none.
//! - The height follows the barrel's `y` exactly; the spec's wobble of
//!   under 1 px isn't copied.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::Vec2;
use clamber_pack::Param;

use crate::archetype::Behavior;
use crate::columns;
use crate::player::{Astride, Facing, State};
use crate::world::{Event, Phase, World};

/// How high above the barrel's `y` the character's feet are.
const RISE: f64 = 16.5;
/// How far behind the barrel's `x` the character is on the catch frame.
const GAP: f64 = 7.75;
/// The lowest a falling character's feet can end a frame, above the
/// barrel's `y`, for the barrel to catch it.
const REACH: f64 = 17.5;

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).expect("a value Fixed holds")
}

/// The distances (character's `x` minus the barrel's) it is caught at.
fn range(facing: Facing) -> (Fixed, Fixed) {
    match facing {
        Facing::Left => (fx(-26.324), fx(24.176)),
        Facing::Right => (fx(-23.084), fx(27.477)),
    }
}

/// The speed across of the thrown barrel `thing` if it's a metal one
/// rolling right.
fn rolling_right(world: &World, thing: usize) -> Option<Fixed> {
    let thing = &world.things[thing];
    let turns = matches!(
        thing.archetype.as_ref()?.behavior,
        Behavior::Carry {
            turns_at_walls: true,
            ..
        }
    );
    match thing.phase {
        Phase::Thrown(f) if thing.active && turns && f.vx > Fixed::ZERO => Some(f.vx),
        _ => None,
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
    let vx = rolling_right(world, on.thing);
    let p = &mut world.player;
    p.touched.clear();
    p.update_jump_held(input);
    // A press never waits: before the ride allows it, it is lost.
    p.jump_buffer.take();
    on.age = on.age.saturating_add(1);
    match vx {
        Some(vx) if on.age >= 2 && input.pressed.contains(Action::Jump) => {
            p.jump_off_barrel(vx, -physics[Param::JumpSpeed]);
            world.events.push(Event::JumpStart);
        }
        _ => p.astride = Some(on),
    }
    true
}

/// After the things have moved on a step: a falling character is caught,
/// a rider goes with its barrel or is put down.
pub(crate) fn after(world: &mut World) {
    if world.player.astride.is_some() {
        ride_on(world);
    } else {
        catch(world);
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
    let Some(vx) = rolling_right(world, on.thing) else {
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
    p.pos.y = barrel.y - fx(RISE);
    p.astride = Some(on);
}

/// Ends a ride: the character stands on the floor at its own `x`.
fn put_down(world: &mut World) {
    let column = columns::column(world.player.pos.x);
    let feet = world.player.pos.y + fx(RISE);
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

fn catch(world: &mut World) {
    let p = &world.player;
    if p.state != State::Fall
        || p.grounded()
        || !p.vel.y.to_bits().is_positive()
        || p.character != 0
        || p.hands.is_some()
        || p.riding.is_some()
        || p.rope.is_some()
        || p.knockout.is_some()
        || world.swap.is_some()
        || world.losing()
    {
        return;
    }
    let (low, high) = range(p.facing);
    let found = (0..world.things.len()).find(|&i| {
        rolling_right(world, i).is_some() && {
            let barrel = world.things[i].pos;
            let d = p.pos.x - barrel.x;
            low <= d && d <= high && p.pos.y >= barrel.y - fx(REACH)
        }
    });
    let Some(thing) = found else {
        return;
    };
    let barrel = world.things[thing].pos;
    let at = Vec2::new(barrel.x - fx(GAP), barrel.y - fx(RISE));
    world.player.catch_on_barrel(
        Astride {
            thing,
            age: 0,
            gap: fx(GAP),
        },
        at,
    );
}
