//! Which pose the player has, for whether a ball reaches it
//! (spec/behavior/ball-contact.md, "Which pose the player has").
//!
//! [`PoseClock`] keeps the facts a pose is read from up to date on every
//! step, after the player moves; [`pose`] reads the pose from them with
//! a ball's [`Contact`] timings.
//!
//! Engine choices where the spec says nothing:
//!
//! - A walk-off fall is airborne from the frame the player leaves the
//!   ground, not the frames before it.
//! - A jump let go of before it lands counts as a hop, and a running
//!   jump as a walking one. Any other time in the air (a bounce, a fall
//!   without a jump) counts as a walk-off fall.
//! - The standing pose starts on the first frame the player is idle on
//!   the ground, outside a turn, a crouch, the ground action and
//!   carrying.
//! - Bouncing is airborne. Carrying, swapping and being hurt take the
//!   pose they would have otherwise.

use clamber_fixed::Fixed;

use crate::archetype::Contact;
use crate::player::{Player, State};
use crate::world::Event;

/// A pose with its own ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pose {
    Upright,
    Airborne,
    Rolling,
    Crouching,
    Fidget,
}

/// How the player went into the air, which says how long it still has
/// the airborne pose after landing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Air {
    /// A jump standing still, held until it lands.
    Still,
    /// A jump standing still, let go of before it lands.
    Hop,
    /// A jump while moving.
    Moving,
    /// A jump out of a roll.
    Roll,
    /// In the air without jumping.
    Fall,
}

impl Air {
    /// Its place in [`Contact::land_hold`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// What the player's pose is read from, as of the last step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PoseClock {
    /// How the player went into the air the last time.
    pub air: Air,
    /// Steps since it landed, from 0 on the landing frame, while it
    /// stays on the ground; `None` in the air.
    pub since_land: Option<u8>,
    /// Steps since a roll ended, from 1 on the first one after it, and
    /// whether it ran on into a run.
    pub rolled: Option<(u8, bool)>,
    /// Steps into the standing pose, from 0 on its first.
    pub standing: Option<u32>,
    /// Rolling on the last step.
    pub rolling: bool,
}

impl Default for PoseClock {
    /// On the ground long since.
    fn default() -> PoseClock {
        PoseClock {
            air: Air::Fall,
            since_land: Some(u8::MAX),
            rolled: None,
            standing: None,
            rolling: false,
        }
    }
}

/// Whether `player` is rolling, as a pose: during a roll or a timed one.
fn rolling(player: &Player) -> bool {
    player.state == State::Roll || player.roll.is_some()
}

/// Brings `player`'s clock up to date after its move on a step that
/// had `events`.
pub(crate) fn note(player: &mut Player, events: &[Event]) {
    let mut clock = player.pose;
    let grounded = player.grounded();
    if events.contains(&Event::JumpStart) {
        clock.air = if clock.rolling || events.contains(&Event::RollEnd) {
            Air::Roll
        } else if player.vel.x != Fixed::ZERO {
            Air::Moving
        } else {
            Air::Still
        };
        clock.since_land = None;
    } else if !grounded {
        if clock.since_land.is_some() {
            clock.air = Air::Fall;
            clock.since_land = None;
        }
    } else {
        clock.since_land = Some(clock.since_land.map_or(0, |n| n.saturating_add(1)));
    }
    if clock.air == Air::Still && !player.jump_hold && clock.since_land.is_none() {
        clock.air = Air::Hop;
    }

    let now = rolling(player);
    clock.rolled = if now {
        None
    } else if clock.rolling {
        Some((1, player.state == State::Run))
    } else {
        clock.rolled.map(|(n, on)| (n.saturating_add(1), on))
    };
    clock.rolling = now;

    let standing = grounded
        && player.state == State::Idle
        && player.crouch.held == 0
        && player.ground_action.is_none()
        && player.turn.is_none()
        && player.hands.is_none();
    clock.standing = if standing {
        Some(clock.standing.map_or(0, |n| n.saturating_add(1)))
    } else {
        None
    };
    player.pose = clock;
}

/// The pose `player` has at the end of the last step, by `contact`'s
/// timings.
pub fn pose(player: &Player, contact: &Contact) -> Pose {
    let clock = player.pose;
    if let Some(age) = player.jump_age {
        return if age <= contact.upright_jump {
            Pose::Upright
        } else {
            Pose::Airborne
        };
    }
    if !player.grounded() {
        return Pose::Airborne;
    }
    if clock
        .since_land
        .is_some_and(|n| n <= contact.land_hold[clock.air.index()])
    {
        return Pose::Airborne;
    }
    let (on, stopped) = contact.roll_after;
    let rolled = clock
        .rolled
        .is_some_and(|(n, ran_on)| n <= if ran_on { on } else { stopped });
    if clock.rolling || rolled {
        return Pose::Rolling;
    }
    if player.crouch.held > 0 && player.ground_action.is_none() {
        return Pose::Crouching;
    }
    let (start, length, every) = contact.fidget_time;
    if let Some(n) = clock.standing
        && n >= start
    {
        let into = if every > 0 {
            (n - start) % every
        } else {
            n - start
        };
        if into < length {
            return Pose::Fidget;
        }
    }
    Pose::Upright
}
