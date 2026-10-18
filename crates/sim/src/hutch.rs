//! Crates holding an animal (spec/behavior/animal-crate.md): things
//! whose archetype has a [`Hutch`] behavior.
//!
//! A crate stands on its position and doesn't move. Walking, running or
//! rolling through it does nothing. A player in the air that moved down
//! on a frame, not rolling, whose offsets from the crate at the end of
//! the frame are within its break ranges, breaks it and bounces off it
//! as off an enemy. The crate breaks once.
//!
//! The animal comes out where the crate stood, hops, and then stands
//! there until it's mounted. A player in the air after a jump or a
//! bounce, moving down and not rolling, whose offsets after its move are
//! within the mount ranges, mounts it: it's put on the animal by whole
//! pixels, keeping its speed across, and holds still. After that it's on
//! the ground with its feet on the animal's position, and rides it (see
//! [`ride`](crate::ride)), with the animal under it. Only the character
//! under control breaks a crate or mounts an animal.
//!
//! An animal its rider got off (spec/behavior/getting-off.md) stays
//! where the rider was, falling to the floor if that's above it, and is
//! got on again as from the crate, with the ranges turned around when it
//! faces left.

use clamber_fixed::Fixed;
use clamber_level::{Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Behavior, Hutch, Ride, within};
use crate::columns;
use crate::player::{Facing, State, ceil, frames};
use crate::ride::{self, Riding};
use crate::world::{Phase, Thing, World};

/// Moves every crate and animal on a step, after the player has moved,
/// then breaks a crate or mounts an animal for the player, unless
/// `frozen` (the player was held still on the step) or `untouchable`
/// (nothing acts on the player now).
pub(crate) fn update(world: &mut World, physics: &Physics, frozen: bool, untouchable: bool) {
    for i in 0..world.things.len() {
        let thing = &world.things[i];
        let Some(Behavior::Hutch(hutch)) = thing.archetype.map(|a| a.behavior) else {
            continue;
        };
        if !thing.active {
            continue;
        }
        let origin = world.level.entities[thing.index].at;
        let thing = &mut world.things[i];
        match thing.phase {
            Phase::Opened(since) => {
                let since = since.saturating_add(1).min(hutch.standing());
                thing.phase = Phase::Opened(since);
                thing.pos.y = origin.y - hutch.height(since);
            }
            Phase::Mounted(since) if since + 1 >= hutch.mount_hold => {
                // Riding starts on the next frame, with the feet on the
                // animal's position, and the held gravity if Jump has been
                // held since the press of the jump mounted from.
                thing.phase = Phase::Ridden;
                let player = &mut world.player;
                let mounted_held = player.jump_hold && player.jump_held > hutch.mount_hold;
                player.jump_hold = false;
                player.riding = Some(Riding {
                    mounted_held,
                    ..Riding::new(hutch.ride, thing.pos.y)
                });
            }
            Phase::Mounted(since) => thing.phase = Phase::Mounted(since + 1),
            Phase::Ridden => match world.player.left.take() {
                Some((pos, facing)) => leave(&world.level, thing, pos, facing),
                None => thing.pos = world.player.pos,
            },
            Phase::Waiting(Some(n)) => fall(&world.level, thing, &hutch.ride, n),
            Phase::Bolting { vx } => ride::bolt(&world.level, thing, &hutch.ride, vx),
            _ => {}
        }
    }
    if frozen || untouchable || world.swap.is_some() {
        return;
    }
    for i in 0..world.things.len() {
        let thing = &world.things[i];
        let Some(Behavior::Hutch(hutch)) = thing.archetype.map(|a| a.behavior) else {
            continue;
        };
        if !thing.active {
            continue;
        }
        let origin = world.level.entities[thing.index].at;
        match thing.phase {
            Phase::Ready if breaks(world, &hutch, origin.x, origin.y) => {
                world.things[i].phase = Phase::Opened(0);
                world.player.bounce(
                    physics[Param::BounceSpeed],
                    frames(physics[Param::BounceFreeze]),
                    &mut world.events,
                );
                return;
            }
            Phase::Opened(since)
                if since >= hutch.standing() && mounts(world, &hutch, origin, Facing::Right) =>
            {
                world.things[i].phase = Phase::Mounted(0);
                world
                    .player
                    .mount(origin.x.floor(), origin.y.floor(), hutch.mount_hold);
                return;
            }
            Phase::Waiting(None) if mounts(world, &hutch, thing.pos, thing.facing) => {
                let at = thing.pos;
                world.things[i].phase = Phase::Mounted(0);
                world
                    .player
                    .mount(at.x.floor(), at.y.floor(), hutch.mount_hold);
                return;
            }
            _ => {}
        }
    }
}

/// Puts a ridden animal under its rider.
pub(crate) fn under_rider(world: &mut World) {
    for thing in world.things.iter_mut() {
        if thing.active && thing.phase == Phase::Ridden {
            thing.pos = world.player.pos;
        }
    }
}

/// The player's offsets from a crate or animal at `(x, y)`: its `x`
/// rounded down less `x`, and `y` less its feet line, all whole px.
fn offsets(world: &World, x: Fixed, y: Fixed) -> (i64, i64) {
    let pos = world.player.pos;
    (pos.x.floor() - x.floor(), y.floor() + (-pos.y).floor())
}

/// Whether the player, in the air, moving down and not rolling, is
/// placed to break a crate at `(x, y)`.
fn breaks(world: &World, hutch: &Hutch, x: Fixed, y: Fixed) -> bool {
    let player = &world.player;
    falling(world)
        && within(
            hutch.break_across,
            hutch.break_above,
            player.facing,
            offsets(world, x, y),
        )
}

/// Whether the player, in the air after a jump or a bounce, moving down
/// and not rolling, is placed to mount an animal at `at` facing
/// `animal`. The ranges are for an animal facing right, and turn around
/// for one facing left.
fn mounts(world: &World, hutch: &Hutch, at: Vec2, animal: Facing) -> bool {
    let player = &world.player;
    let jumped = player.jump_age.is_some() || player.state == State::Bounce;
    let [right, left] = hutch.mount_across;
    let turn = |(from, to): (i32, i32)| (-to, -from);
    let ranges = match animal {
        Facing::Right => [right, left],
        Facing::Left => [turn(left), turn(right)],
    };
    falling(world)
        && jumped
        && within(
            ranges,
            hutch.mount_above,
            player.facing,
            offsets(world, at.x, at.y),
        )
}

/// Leaves an animal where its rider was at `pos`, facing `facing`: at
/// `x` rounded down and the feet line, falling from the next frame if
/// that's above the floor.
fn leave(level: &Level, animal: &mut Thing, pos: Vec2, facing: Facing) {
    let at = Vec2::new(whole(pos.x.floor()), whole(ceil(pos.y)));
    animal.pos = at;
    animal.facing = facing;
    let standing = floor_under(level, at) == Some(at.y);
    animal.phase = Phase::Waiting(if standing { None } else { Some(0) });
}

/// Moves an animal left above the floor on the next frame of its fall,
/// `n` frames in, as `ride` says: it's put on the floor once a move
/// would leave it less than `drop_land` above it.
fn fall(level: &Level, animal: &mut Thing, ride: &Ride, n: u8) {
    let by = match n {
        0 => ride.drop_first,
        _ => ride.drop_second + ride.drop_gravity * Fixed::from_int(i32::from(n) - 1),
    };
    let to = animal.pos.y + by;
    match floor_under(level, animal.pos) {
        Some(floor) if floor - to < ride.drop_land => {
            animal.pos.y = floor;
            animal.phase = Phase::Waiting(None);
        }
        _ => {
            animal.pos.y = to;
            animal.phase = Phase::Waiting(Some(n.saturating_add(1)));
            if to > level.size.y {
                animal.active = false;
            }
        }
    }
}

/// The nearest floor top at or below `at` in its column.
fn floor_under(level: &Level, at: Vec2) -> Option<Fixed> {
    columns::spans(level, columns::column(at.x))
        .into_iter()
        .map(|s| s.top)
        .filter(|&top| top >= at.y)
        .min()
}

fn whole(n: i64) -> Fixed {
    Fixed::from_int(n as i32)
}

/// Whether the player is in the air, moved down on this step and isn't
/// rolling. The speed down after a move in the air is the one it moved
/// by.
fn falling(world: &World) -> bool {
    let player = &world.player;
    !player.grounded()
        && player.vel.y > Fixed::ZERO
        && player.state != State::Roll
        && player.roll.is_none()
}
