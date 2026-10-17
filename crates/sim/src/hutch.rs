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

use clamber_fixed::Fixed;
use clamber_pack::{Param, Physics};

use crate::archetype::{Behavior, Hutch, within};
use crate::player::{State, frames};
use crate::ride::Riding;
use crate::world::{Phase, World};

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
                // animal's position.
                thing.phase = Phase::Ridden;
                world.player.riding = Some(Riding::new(hutch.ride, origin.y));
            }
            Phase::Mounted(since) => thing.phase = Phase::Mounted(since + 1),
            Phase::Ridden => thing.pos = world.player.pos,
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
                if since >= hutch.standing() && mounts(world, &hutch, origin.x, origin.y) =>
            {
                world.things[i].phase = Phase::Mounted(0);
                world
                    .player
                    .mount(origin.x.floor(), origin.y.floor(), hutch.mount_hold);
                return;
            }
            _ => {}
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
/// and not rolling, is placed to mount an animal at `(x, y)`.
fn mounts(world: &World, hutch: &Hutch, x: Fixed, y: Fixed) -> bool {
    let player = &world.player;
    let jumped = player.jump_age.is_some() || player.state == State::Bounce;
    falling(world)
        && jumped
        && within(
            hutch.mount_across,
            hutch.mount_above,
            player.facing,
            offsets(world, x, y),
        )
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
