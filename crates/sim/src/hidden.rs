//! Hidden objects (spec/behavior/hidden-objects.md): things whose
//! archetype has a [`Hidden`] behavior.
//!
//! A hidden object never moves, isn't drawn, doesn't stop or hold up the
//! player, and never hurts. Offsets from it are whole px: `across`, the
//! player's `x` rounded down less its own, and `above`, its `y` less the
//! player's feet line. Until it's set off, it leaves and comes back with
//! the view as other placed objects do. It's set off at the end of a
//! step, after the player's move, with the offsets within its ranges for
//! the player's facing, in one of two ways:
//!
//! - **Struck** by the ground action, on one of its `strike_frames`
//!   (those that are attack frames). That counts as a frame on which
//!   something is knocked out: the ground action's pause follows.
//! - **Landed on** after a long fall (`fall_height` above 0): by a player
//!   in the air, not landing and not rolling, whose feet line has been
//!   `fall_height` or more above the object at the end of a step since it
//!   last stood on a floor ([`Player::fall_top`]). It bounces off it as
//!   off a walker.
//!
//! Set off, the object is gone for the rest of the visit and its item is
//! where it was ([`Phase::Item`]), drawn as what it becomes. The item
//! stays put for `item_wait` steps, then falls under `item_gravity` while
//! moving across, until it's put on the floor under it ([`Phase::Landed`]).
//! It moves across once more on the step after, and on the one after
//! that it becomes what its kind says, `becomes_at` from where it stands:
//! a bunch, a barrel or a metal barrel, which from then on is like one
//! placed in the level. An item in the air holds still on the ground
//! action's pause frames. An item, and a bunch or barrel it became that
//! is standing, not in the hands or thrown, is removed once it's outside
//! the window placed objects appear in, and doesn't come back.
//!
//! Engine choices where the spec says nothing:
//!
//! - The second character has no ground action and its roll doesn't
//!   strike; it's set off by a long fall by `second` ranges if the
//!   object has them (hidden-objects.md, "The second character").
//! - Only the character under control sets off hidden objects, and not
//!   while riding the animal. Every object in range on a frame is set
//!   off, with one pause.
//! - The window an item leaves by is the one placed objects appear in,
//!   by where the view is at the end of the step, across only, as for
//!   most entries (placements.md). An item is also removed once it's
//!   below the level, with or without a view.
//! - A landed item holds still on pause frames too, until it becomes
//!   what it becomes.
//! - A barrel that was picked up and put down again is removed as one
//!   that never was; one that's thrown leaves as thrown objects do.
//! - The floor under an item is the highest top of the ground in the
//!   column of its new `x` that isn't above where it was before its move.
//!
//! [`Player::fall_top`]: crate::Player::fall_top

use clamber_fixed::Fixed;
use clamber_level::Vec2;
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetypes, Behavior, Hidden, within};
use crate::columns;
use crate::player::{State, ceil, frames};
use crate::sections::Window;
use crate::view;
use crate::world::{Phase, Thing, World};

/// The kind of entity a hidden object is.
pub const HIDDEN: &str = "hidden";

/// The hidden-object values of `thing`, if it is one, set off or not.
fn hidden(thing: &Thing) -> Option<Hidden> {
    match thing.archetype?.behavior {
        Behavior::Hidden(hidden) => Some(hidden),
        _ => None,
    }
}

/// The player's offsets from an object at `at`: its `x` rounded down less
/// the object's, and the object's `y` less its feet line, whole px.
fn offsets(world: &World, at: Vec2) -> (i64, i64) {
    let pos = world.player.pos;
    (pos.x.floor() - at.x.floor(), at.y.floor() - ceil(pos.y))
}

/// Notes the player's feet line for the falls that set objects off: the
/// smallest since it last stood on a floor, while it's in the air.
pub(crate) fn note_fall(world: &mut World) {
    let player = &mut world.player;
    player.fall_top = if player.grounded() {
        None
    } else {
        let feet = ceil(player.pos.y);
        Some(player.fall_top.map_or(feet, |top| top.min(feet)))
    };
}

/// Sets off every hidden object that the ground action strikes on this
/// step. Returns whether one was.
pub(crate) fn strike(world: &mut World, physics: &Physics) -> bool {
    let Some(action) = world.player.ground_action else {
        return false;
    };
    let attacking = clamber_pack::physics::steps(physics[Param::GroundAttackFrames])
        .any(|n| n == u32::from(action.frame));
    if !attacking || action.struck || world.player.riding.is_some() {
        return false;
    }
    let mut struck = false;
    for i in 0..world.things.len() {
        let thing = &world.things[i];
        let Some(h) = hidden(thing).filter(|_| ready(thing)) else {
            continue;
        };
        let frames = &h.strike_frames[..usize::from(h.strike_count)];
        if h.fall_height == 0
            && frames.contains(&action.frame)
            && within(
                h.across,
                h.above,
                world.player.facing,
                offsets(world, thing.pos),
            )
        {
            set_off(&mut world.things[i], &h);
            struck = true;
        }
    }
    struck
}

/// Sets off every hidden object the player lands on after a long fall
/// on this step, and bounces it off them.
pub(crate) fn land(world: &mut World, physics: &Physics) {
    let player = &world.player;
    let airborne = !player.grounded()
        && player.state != State::Roll
        && player.roll.is_none()
        && player.riding.is_none();
    let Some(top) = player.fall_top.filter(|_| airborne) else {
        return;
    };
    let mut landed = false;
    for i in 0..world.things.len() {
        let thing = &world.things[i];
        let Some(h) = hidden(thing).filter(|_| ready(thing)) else {
            continue;
        };
        let ranges = match h.second {
            Some(second) if world.player.character != 0 => (second.across, second.above),
            _ => (h.across, h.above),
        };
        if h.fall_height > 0
            && thing.pos.y.floor() - top >= i64::from(h.fall_height)
            && within(
                ranges.0,
                ranges.1,
                world.player.facing,
                offsets(world, thing.pos),
            )
        {
            set_off(&mut world.things[i], &h);
            landed = true;
        }
    }
    if landed {
        world.player.bounce(
            physics[Param::BounceSpeed],
            frames(physics[Param::BounceFreeze]),
            &mut world.events,
        );
    }
}

/// Whether `thing` is a hidden object in the level that hasn't been set
/// off.
fn ready(thing: &Thing) -> bool {
    thing.active && thing.phase == Phase::Ready
}

/// The object is gone, and its item is where it was.
fn set_off(thing: &mut Thing, hidden: &Hidden) {
    thing.kind = hidden.becomes.to_string();
    thing.vx = hidden.item_speed.x;
    thing.phase = Phase::Item {
        vy: hidden.item_speed.y,
        wait: hidden.item_wait,
    };
}

/// Moves the items in the air or landed on a step, before objects are
/// set off on it. `paused` says it's one of the ground action's pause
/// frames.
pub(crate) fn items(world: &mut World, paused: bool) {
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let Some(h) = hidden(thing) else {
            continue;
        };
        match thing.phase {
            Phase::Item { vy, wait } if wait > 0 => {
                thing.phase = Phase::Item { vy, wait: wait - 1 };
            }
            _ if paused => {}
            Phase::Item { vy, .. } => {
                let vy = (vy + h.item_gravity).min(h.item_max_fall);
                let was = thing.pos.y;
                let x = thing.pos.x + thing.vx;
                let mut y = was + vy;
                let floor = columns::spans(&world.level, columns::column(x))
                    .into_iter()
                    .map(|s| s.top)
                    .filter(|&top| top >= was)
                    .min();
                thing.phase = Phase::Item { vy, wait: 0 };
                if let Some(floor) = floor
                    && vy > Fixed::ZERO
                    && y > floor - h.item_touch
                {
                    y = floor;
                    thing.phase = Phase::Landed(0);
                }
                thing.pos = Vec2::new(x, y);
            }
            Phase::Landed(0) => {
                thing.pos.x += thing.vx;
                thing.phase = Phase::Landed(1);
            }
            Phase::Landed(_) => become_item(thing, &h),
            _ => {}
        }
    }
}

/// A landed item becomes a thing of the kind it was drawn as.
fn become_item(thing: &mut Thing, hidden: &Hidden) {
    thing.pos = Vec2::new(
        thing.pos.x + hidden.becomes_at.x,
        thing.pos.y + hidden.becomes_at.y,
    );
    thing.archetype = Archetypes::builtin().named(hidden.becomes);
    thing.vx = Fixed::ZERO;
    thing.phase = Phase::Ready;
}

/// Removes items, and the bunches and barrels they became that are
/// standing, once they're outside the window placed objects appear in.
/// They don't come back.
pub(crate) fn leave(world: &mut World, physics: &Physics) {
    let (left, physics) = match world.view.filter(|_| view::by_view(physics)) {
        Some(v) => {
            let (left, physics) = view::window(&world.level, &world.feel, v.x(), physics);
            (Some(left), physics)
        }
        None => (None, *physics),
    };
    let window = Window::of(&physics);
    let bottom = world.level.size.y;
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let released = thing.kind != HIDDEN
            && world
                .level
                .entities
                .get(thing.index)
                .is_some_and(|e| e.kind == HIDDEN);
        let standing = thing.phase == Phase::Ready && hidden(thing).is_none();
        let flying = matches!(thing.phase, Phase::Item { .. } | Phase::Landed(_));
        if !released || !(standing || flying) {
            continue;
        }
        let outside = left.is_some_and(|left| !window.holds_x(thing.pos.x.floor(), left));
        let gone = outside || thing.pos.y > bottom;
        if gone {
            thing.active = false;
            thing.phase = Phase::Broken;
        }
    }
}
