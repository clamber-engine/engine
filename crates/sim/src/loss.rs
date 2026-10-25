//! Losing a character on a hit while the other character is in the level
//! (`lose_jump` above 0), and the protection after it.
//!
//! On the hit frame the hit character stops where it is, the hit spot,
//! and is thrown up. It lands and runs out of the level ([`Lost`]), and
//! plays no further part. No life is lost. The other character holds
//! still, becomes the one under control after `takeover_wait` frames,
//! makes its way to the hit spot through the air, and hops there. Input
//! is ignored until the hop's second frame, and the hop steers like any
//! time in the air. From its landing the character is the player in
//! every respect.
//!
//! From the hit for `protect_time` frames nothing hurts the character
//! under control, and for part of that time it flashes
//! ([`World::player_drawn`]).

use clamber_fixed::Fixed;
use clamber_level::{Dir, Vec2};
use clamber_pack::{Param, Physics};

use crate::player::{Facing, Player, State, ceil, frames};
use crate::world::{Event, World};

/// A loss under way, from the hit frame until protection and flashing
/// are over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Loss {
    /// Frames since the hit frame, which is 0.
    pub frame: u16,
    /// The hit spot, with the fraction of `x` dropped and `y` rounded
    /// toward the floor.
    pub spot: Vec2,
    /// The frame the other character's hop starts on, once its run to
    /// the hit spot is over.
    pub hop: Option<u16>,
}

/// A character lost on a hit, on its way out of the level.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Lost {
    pub body: Player,
    /// How much its vertical speed grows each frame.
    pub gravity: Fixed,
    /// Frames since it first landed, up to 1.
    pub landed: Option<u8>,
    /// It's no longer the character under control.
    pub apart: bool,
}

/// Whether a hurt player loses its character rather than a life: the
/// other character is in the level, and the hit isn't one the spec
/// leaves out (during a swap, a bounce or a roll).
pub(crate) fn can_start(world: &World) -> bool {
    let p = &world.player;
    world.other.is_some()
        && world.swap.is_none()
        && world.loss.is_none()
        && world.player_physics()[Param::LoseJump] > Fixed::ZERO
        && p.knockout.is_none()
        && !(p.state == State::Bounce && p.character != 0)
}

/// Starts losing the player's character on this, the hit frame: it
/// stays where it was, faces right and is thrown up.
pub(crate) fn start(world: &mut World) {
    let physics = world.player_physics();
    let p = &mut world.player;
    let held = physics[Param::HeldJumpGravity];
    let gravity = if p.jump_hold && held > Fixed::ZERO {
        held
    } else {
        physics[Param::Gravity]
    };
    if p.facing == Facing::Left {
        world.events.push(Event::Turn);
    }
    p.facing = Facing::Right;
    p.vel = Vec2::new(Fixed::ZERO, -physics[Param::LoseJump]);
    p.state = State::Hurt;
    p.jump_hold = false;
    p.end_roll();
    p.takeoff = false;
    p.takeoff_wait = None;
    p.turn = None;
    world.lost = Some(Lost {
        body: p.clone(),
        gravity,
        landed: None,
        apart: false,
    });
    world.loss = Some(Loss {
        frame: 0,
        spot: Vec2::new(
            Fixed::from_int(p.pos.x.floor() as i32),
            Fixed::from_int(ceil(p.pos.y) as i32),
        ),
        hop: None,
    });
}

/// Starts a step: counts the frame of the loss, and ends it once
/// protection and flashing are over and the hop has landed. Moves the
/// lost character on, and forgets it once it has left the level.
pub(crate) fn tick(world: &mut World) {
    if let Some(mut loss) = world.loss {
        loss.frame = loss.frame.saturating_add(1);
        let physics = world.player_physics();
        let end = frames(physics[Param::ProtectTime]).max(frames(physics[Param::FlashEnd]));
        let over = loss.frame >= u16::from(end) && loss.hop.is_some() && world.player.hop.is_none();
        world.loss = (!over).then_some(loss);
    }
    let Some(lost) = &mut world.lost else {
        return;
    };
    let physics = world.feel.roster[lost.body.character].physics;
    let mut events = Vec::new();
    let mut body = lost.body.clone();
    body.run_off(&world.level, &physics, lost, &mut events);
    lost.body = body;
    if !lost.apart {
        // Until the other character takes over, the trace follows this
        // one.
        let p = &mut world.player;
        p.pos = lost.body.pos;
        p.vel = Vec2::new(Fixed::ZERO, lost.body.vel.y);
        p.ground = lost.body.ground;
        p.touched.clear();
        world.events.extend(events);
    }
    let (left, top, _, _) = lost.body.body();
    if left > world.level.size.x || top > world.level.size.y {
        world.lost = None;
    }
}

/// Moves the characters on while the loss has control: the hit
/// character's flight until the other takes over, then the other's way
/// to the hit spot and the hop's first frame. Returns whether it did, so
/// the player doesn't move on its own on this step.
pub(crate) fn update(world: &mut World) -> bool {
    let Some(mut loss) = world.loss else {
        return false;
    };
    if loss.hop.is_some_and(|t| loss.frame > t) {
        return false;
    }
    let other = world.other.as_ref();
    let physics = match other {
        Some(o) => world.feel.roster[o.character].physics,
        None => world.player_physics(),
    };
    let wait = u16::from(frames(physics[Param::TakeoverWait]));
    let still = u16::from(frames(physics[Param::TakeoverStill]));
    if loss.frame < wait {
        world.player.state = State::Hurt;
        return true;
    }
    if loss.frame == wait {
        take_over(world, still > 0);
        return true;
    }
    if loss.frame < wait + still {
        return true;
    }
    let p = &mut world.player;
    if loss.frame == wait + still {
        // The character stops standing as it was and starts to wait.
        p.state = State::Swap;
        p.ground = None;
        return true;
    }
    match loss.hop {
        None => {
            let gap_x = loss.spot.x.floor() - p.pos.x.floor();
            let gap_y = ceil(loss.spot.y) - ceil(p.pos.y);
            let vel = Vec2::new(toward(gap_x, &physics), toward(gap_y, &physics));
            p.pos = Vec2::new(p.pos.x + vel.x, p.pos.y + vel.y);
            p.vel = vel;
            let there =
                p.pos.x.floor() == loss.spot.x.floor() && ceil(p.pos.y) == ceil(loss.spot.y);
            if there {
                loss.hop = Some(loss.frame + 1);
            }
        }
        Some(_) => {
            let nudge = physics[Param::TakeoverNudge];
            p.pos = Vec2::new(loss.spot.x + nudge, loss.spot.y - nudge);
            p.vel = Vec2::new(Fixed::ZERO, -physics[Param::TakeoverHop]);
            p.hop = Some(0);
        }
    }
    p.state = State::Swap;
    p.ground = None;
    p.spring = None;
    p.spring_arc = None;
    p.rope = None;
    p.sling = None;
    p.rope_fall = false;
    p.touched.clear();
    world.loss = Some(loss);
    true
}

/// The move along one axis toward a target `gap` whole px away.
fn toward(gap: i64, physics: &Physics) -> Fixed {
    let share = physics[Param::TakeoverShare].floor().max(1);
    let least = physics[Param::TakeoverMinMove];
    let gap = Fixed::from_int(gap as i32);
    let part = Fixed::from_bits(gap.to_bits() / share);
    if part.abs() >= least {
        part
    } else if gap.abs() < least {
        gap
    } else if gap.is_negative() {
        -least
    } else {
        least
    }
}

/// Makes the other character the one under control, standing still
/// where it is, with input ignored: off the ground in its waiting state,
/// or, with `keep` (`takeover_still`), in its ordinary ground state until
/// the frames it stands still are over. The lost character goes its own
/// way.
fn take_over(world: &mut World, keep: bool) {
    let Some(other) = world.other.take() else {
        return;
    };
    if let Some(lost) = &mut world.lost {
        lost.apart = true;
    }
    let mut player = Player::new(&world.level, other.pos);
    player.character = other.character;
    player.facing = other.facing;
    player.spring = None;
    player.spring_arc = None;
    player.rope = None;
    player.sling = None;
    player.rope_fall = false;
    if keep {
        let from = Vec2::new(player.pos.x, player.pos.y - Fixed::ONE);
        player.ground = world
            .level
            .collision
            .cast(from, Dir::Down, Fixed::from_int(2))
            .map(|hit| hit.collider);
    } else {
        player.ground = None;
        player.state = State::Swap;
    }
    world.player = player;
}

impl World {
    /// Whether the character under control can't be hurt now, after
    /// losing a character or a hit point.
    pub fn protected(&self) -> bool {
        self.recovering()
            || self.loss.is_some_and(|loss| {
                loss.frame < u16::from(frames(self.player_physics()[Param::ProtectTime]))
            })
    }

    /// Whether a character is being lost and the other hasn't yet landed
    /// from taking over. Things don't act on the player meanwhile.
    pub fn losing(&self) -> bool {
        self.loss
            .is_some_and(|loss| loss.hop.is_none() || self.player.hop.is_some())
    }

    /// Whether the player is drawn on this frame: not on the frames it
    /// flashes off.
    pub fn player_drawn(&self) -> bool {
        if self.in_keg() || self.in_entrance() {
            return false;
        }
        if self.hit_blink_off() {
            return false;
        }
        let Some(loss) = self.loss else {
            return true;
        };
        let physics = self.player_physics();
        let start = u16::from(frames(physics[Param::FlashStart]));
        let end = u16::from(frames(physics[Param::FlashEnd]));
        let period = u16::from(frames(physics[Param::FlashPeriod]));
        let n = loss.frame;
        if period == 0 || n < start || n >= end || loss.hop.is_some_and(|t| n == t + 1) {
            return true;
        }
        (n - start) % period >= period / 2
    }

    /// The lost character, once it's no longer the one under control.
    pub fn leaving(&self) -> Option<&Player> {
        self.lost.as_ref().filter(|l| l.apart).map(|l| &l.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64) -> Fixed {
        Fixed::from_f64(v).unwrap()
    }

    #[test]
    fn moves_a_share_of_the_gap_or_at_least_three() {
        let physics = clamber_pack::Presets::builtin().world("classic-1").unwrap();
        let moves = [62, 59, 55, 48, 4, 3, 1, 0, -78, -2];
        let want = [3.875, 3.6875, 3.4375, 3.0, 3.0, 3.0, 1.0, 0.0, -4.875, -2.0];
        for (gap, want) in moves.into_iter().zip(want) {
            assert_eq!(toward(gap, &physics), f(want), "{gap}");
        }
    }
}
