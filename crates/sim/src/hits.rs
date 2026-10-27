//! Hit points (`hit_points` above 1), Clamber's own: a life takes that
//! many hits, and only the hit that takes the last hit point costs it.
//!
//! A hit that leaves hit points knocks the player back, away from the
//! enemy that hit it, or the way it doesn't face if it was something
//! else (`hit_knockback`, `hit_knockback_rise`). Input is ignored for
//! `hit_control_wait` frames from the hit, and nothing hurts the player
//! for `hit_protect_time` frames, while it blinks (`hit_flash_period`).
//! A [`TONIC`](crate::TONIC) gives back a hit point, and coming back
//! after a lost life gives them all back.

use clamber_fixed::Fixed;
use clamber_level::Vec2;
use clamber_pack::Param;

use crate::player::{Facing, State, frames};
use crate::world::World;

/// The hit points a life starts with: `hit_points`, or 1 if that's 0.
pub(crate) fn per_life(world: &World) -> u8 {
    frames(world.feel.world[Param::HitPoints]).max(1)
}

/// Counts the frames since a hit that cost a hit point, until its
/// protection is over.
pub(crate) fn tick(world: &mut World) {
    let Some(frame) = world.flow.recovery else {
        return;
    };
    let physics = &world.feel.world;
    let end = frames(physics[Param::HitProtectTime]).max(frames(physics[Param::HitControlWait]));
    let next = frame.saturating_add(1);
    world.flow.recovery = (next < u16::from(end)).then_some(next);
}

/// Counts down the protection after a rider was knocked off: it doesn't
/// run on the frames a bounce holds still (riding-hit.md, "The
/// protection").
pub(crate) fn tick_ride_guard(world: &mut World) {
    let held = world.player.frozen > 0 && world.player.state == State::Bounce;
    if !held {
        world.flow.ride_guard = world.flow.ride_guard.saturating_sub(1);
    }
}

/// On a hit that doesn't lose a character: takes a hit point if one
/// would be left, knocks the player back and starts the protection.
/// Returns whether it did, so the hit costs no life. `hurt_by` is the
/// enemy that hit the player, if one did.
pub(crate) fn take(world: &mut World, hurt_by: Option<usize>) -> bool {
    if world.flow.hits_taken.saturating_add(1) >= per_life(world) {
        return false;
    }
    world.flow.hits_taken += 1;
    world.flow.recovery = Some(0);
    let physics = &world.feel.world;
    let (across, rise) = (
        physics[Param::HitKnockback],
        physics[Param::HitKnockbackRise],
    );
    let p = &world.player;
    let back = match hurt_by.and_then(|i| world.things.get(i)) {
        Some(enemy) if enemy.pos.x != p.pos.x => {
            if enemy.pos.x < p.pos.x {
                Facing::Right
            } else {
                Facing::Left
            }
        }
        _ => match p.facing {
            Facing::Left => Facing::Right,
            Facing::Right => Facing::Left,
        },
    };
    let x = match back {
        Facing::Left => -across,
        Facing::Right => across,
    };
    if across > Fixed::ZERO || rise > Fixed::ZERO {
        world.player.launch(Vec2::new(x, -rise), &mut world.events);
    }
    true
}

impl World {
    /// Whether nothing hurts the player, knocked off the animal not long
    /// ago.
    pub(crate) fn ride_guarded(&self) -> bool {
        self.flow.ride_guard > 0
    }

    /// The hit points the player has left in this life, if a life has
    /// more than one (`hit_points` above 1).
    pub fn hit_points(&self) -> Option<(u8, u8)> {
        let max = per_life(self);
        (max > 1).then(|| (max.saturating_sub(self.flow.hits_taken), max))
    }

    /// Whether nothing hurts the player after a hit that cost a hit
    /// point.
    pub(crate) fn recovering(&self) -> bool {
        self.flow
            .recovery
            .is_some_and(|frame| frame < u16::from(frames(self.feel.world[Param::HitProtectTime])))
    }

    /// Whether input is ignored after a hit that cost a hit point.
    pub(crate) fn hit_stunned(&self) -> bool {
        self.flow
            .recovery
            .is_some_and(|frame| frame < u16::from(frames(self.feel.world[Param::HitControlWait])))
    }

    /// Whether the player blinks off on this frame after a hit that cost
    /// a hit point.
    pub(crate) fn hit_blink_off(&self) -> bool {
        let period = u16::from(frames(self.feel.world[Param::HitFlashPeriod]));
        self.recovering()
            && period > 0
            && self
                .flow
                .recovery
                .is_some_and(|frame| frame % period >= period.div_ceil(2))
    }
}
