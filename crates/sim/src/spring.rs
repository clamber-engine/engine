//! Springs (spec/behavior/tyre.md): things whose archetype has a
//! [`Spring`] behavior.
//!
//! A spring never moves. Offsets from it are whole px: `across`, the
//! player's `x` rounded down less its own, and `up`, its `y` rounded down
//! less the player's feet line. Its top is a number of px above its `y`,
//! lower while it's squashed and on some frames of its wobble. Each step,
//! after the player's move, every spring acts on the player in turn:
//!
//! - A spring at rest, or wobbling and not in the frames after a throw,
//!   **catches** a player that touches it (by the top at the end of the
//!   last step) on the ground, not taking off, or in the air, falling,
//!   near enough its top. Caught, the player is moved straight up or down
//!   onto the top by whole px, keeping the fraction, and stands there
//!   ([`OnSpring`]); the spring squashes.
//! - While squashed, it puts a player that touches it and isn't rising
//!   back on the top.
//! - On the frame after the squash, it **throws** a player within its
//!   throw range ([`Player::throw_off_spring`]), then wobbles. A spring
//!   that a player touches without being caught doesn't wobble on that
//!   step.
//!
//! On the frames a bounce holds the player still, springs don't change.
//!
//! A player standing on the top goes down by the spring's `drop` with
//! its move, as if following the ground down, and the spring puts it
//! back. If it doesn't, the player leaves the top on the next step
//! (`leave_drop` down instead of the preset's). On the top, a Jump press
//! makes a spring jump (see [`Player::update`]).
//!
//! Engine choices where the spec says nothing:
//!
//! - Only the character under control meets springs, and not while
//!   riding the animal, being knocked out or lost.
//! - A player on the top that another spring moves stands on that one
//!   from then on.
//! - Where the floor is within the spring's `drop` under a player on the
//!   top, the player follows it down and stands on the floor.
//!
//! [`Player::update`]: crate::Player::update
//! [`Player::throw_off_spring`]: crate::Player::throw_off_spring

use clamber_fixed::Fixed;
use clamber_level::Vec2;

use crate::archetype::{Behavior, Spring};
use crate::player::{State, ceil};
use crate::world::{Phase, Thing, World};

/// The kind of entity a spring is.
pub const SPRING: &str = "spring";

/// The spring the player stands on, from the catch until it leaves the
/// top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OnSpring {
    pub values: Spring,
    /// Frames since the catch, at the end of the last step.
    pub since: u8,
    /// Caught in the air, not on the ground.
    pub from_air: bool,
    /// Put on the top at the end of the last step.
    pub top: bool,
    /// Thrown, this many steps ago.
    pub thrown: Option<u8>,
}

/// A throw or a spring jump under way, until the player lands or is
/// caught: its speed down grows by `jump_gravity` while `held`, and by
/// `gravity` otherwise, up to `max_fall`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpringArc {
    pub held: bool,
    pub jump_gravity: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
}

impl SpringArc {
    /// The speed down after a frame with Jump `held` or not, from `vy`.
    pub(crate) fn fall(&mut self, vy: Fixed, held: bool) -> Fixed {
        self.held &= held;
        let gravity = if self.held {
            self.jump_gravity
        } else {
            self.gravity
        };
        (vy + gravity).min(self.max_fall)
    }
}

/// The spring values of `thing`, if it is one.
fn spring(thing: &Thing) -> Option<Spring> {
    match thing.archetype?.behavior {
        Behavior::Spring(spring) => Some(spring),
        _ => None,
    }
}

/// The spring's top at the end of the last step, in `phase`.
pub fn top(spring: &Spring, phase: Phase) -> i32 {
    match phase {
        Phase::Squashed { since, .. } => spring.squash_top(u32::from(since)),
        Phase::Wobbling { m, .. } => spring.wobble_top(u32::from(m)),
        _ => spring.top,
    }
}

/// The player's offsets from a spring at `at`: its `x` rounded down less
/// the spring's, and the spring's `y` rounded down less its feet line.
fn offsets(world: &World, at: Vec2) -> (i64, i64) {
    let pos = world.player.pos;
    (pos.x.floor() - at.x.floor(), at.y.floor() - ceil(pos.y))
}

/// Whether the player touches `spring` at `at`, with its top `top` px up.
fn touches(world: &World, spring: &Spring, at: Vec2, top: i32) -> bool {
    let reach = (top + spring.reach.0, top + spring.reach.1);
    crate::archetype::within(
        spring.across,
        reach,
        world.player.facing,
        offsets(world, at),
    )
}

/// Whether the player is rising: taking off, or in the air and not
/// falling.
fn rising(world: &World) -> bool {
    let p = &world.player;
    if p.grounded() {
        p.takeoff
    } else {
        p.vel.y <= Fixed::ZERO
    }
}

/// Moves every spring on a step, after the player's move. `frozen` says
/// the player was held still on it; `untouchable` that springs don't
/// meet it.
pub(crate) fn update(world: &mut World, frozen: bool, untouchable: bool) {
    // A bounce's frozen frames change nothing.
    if frozen && world.player.state == State::Bounce {
        return;
    }
    let untouchable = untouchable || world.player.riding.is_some() || world.swap.is_some();
    for i in 0..world.things.len() {
        let thing = &world.things[i];
        let Some(s) = spring(thing).filter(|_| thing.active) else {
            continue;
        };
        let (at, phase) = (thing.pos, thing.phase);
        let touched = |world: &World, top: i32| !untouchable && touches(world, &s, at, top);
        world.things[i].phase = match phase {
            Phase::Squashed { since, from_air } => {
                let since = since.saturating_add(1);
                if u32::from(since) <= s.squash_frames() {
                    let top = s.squash_top(u32::from(since));
                    if touched(world, top) && !rising(world) {
                        put_on_top(world, i, &s, top, since, from_air);
                    }
                    Phase::Squashed { since, from_air }
                } else {
                    // The throw frame, the wobble's first.
                    let thrown = !untouchable && throws(world, &s, at);
                    if thrown {
                        world.player.throw_off_spring(&s);
                    }
                    Phase::Wobbling {
                        m: 0,
                        thrown: thrown.then_some(0),
                    }
                }
            }
            Phase::Wobbling { m, thrown } => {
                let thrown = thrown.map(|n| n.saturating_add(1));
                let ready = thrown.is_none_or(|n| n > s.throw_rest);
                let before = s.wobble_top(u32::from(m));
                if ready && touched(world, before) {
                    if catches(world, &s, at, before) {
                        catch(world, i, &s, before)
                    } else {
                        // It doesn't wobble on this step.
                        Phase::Wobbling { m, thrown }
                    }
                } else if u32::from(m) + 1 >= s.wobble_frames() {
                    Phase::Ready
                } else {
                    Phase::Wobbling { m: m + 1, thrown }
                }
            }
            _ => {
                if touched(world, s.top) && catches(world, &s, at, s.top) {
                    catch(world, i, &s, s.top)
                } else {
                    phase
                }
            }
        };
    }
}

/// Whether a player touching `spring` at `at`, whose top was `top`, is
/// caught: on the ground and not taking off, or in the air, falling, no
/// more than `catch_below` under the top.
fn catches(world: &World, spring: &Spring, at: Vec2, top: i32) -> bool {
    let p = &world.player;
    if p.grounded() {
        !p.takeoff
    } else {
        p.vel.y > Fixed::ZERO && offsets(world, at).1 >= i64::from(top - spring.catch_below)
    }
}

/// Catches the player on spring `i`, whose top was `top`, and returns its
/// phase.
fn catch(world: &mut World, i: usize, spring: &Spring, top: i32) -> Phase {
    let from_air = !world.player.grounded();
    put_on_top(world, i, spring, top, 0, from_air);
    Phase::Squashed { since: 0, from_air }
}

/// Puts the player on the top of spring `i`, `top` px up, `since` frames
/// after it was caught.
fn put_on_top(world: &mut World, i: usize, spring: &Spring, top: i32, since: u8, from_air: bool) {
    let feet = world.things[i].pos.y.floor() - i64::from(top + spring.stand);
    world.player.stand_on_spring(
        feet,
        OnSpring {
            values: *spring,
            since,
            from_air,
            top: true,
            thrown: None,
        },
    );
}

/// Whether the player is in `spring`'s throw range, at `at`.
fn throws(world: &World, spring: &Spring, at: Vec2) -> bool {
    crate::archetype::within(
        spring.across,
        spring.throw_up,
        world.player.facing,
        offsets(world, at),
    )
}
