//! Stingers (spec/behavior/stinger.md): things whose archetype has a
//! [`Stinger`] behavior.
//!
//! A stinger sways around its start without regard to floors: each
//! frame it turns at its turning points and eases its speeds toward its
//! targets, as [`sway`] says. It starts at its start on the frame it
//! comes into the level, at its starting speeds, without moving (see
//! [`start`]). With a view, it appears and is removed as a walker on its
//! own does (see [`view::walkers`](crate::view)), and comes back afresh.
//!
//! It hurts the player as [`hurt_ahead`] says, and is never bounced on,
//! rolled into or defeated. While the player is knocked out, it holds
//! still with the other enemies, also when it's the one that hurt the
//! player.
//!
//! Engine choices where the spec says nothing: it passes through walls,
//! it can hurt the player on the frame it appears, and it doesn't touch
//! thrown objects or other enemies.

use clamber_fixed::Fixed;
use clamber_level::Vec2;

use crate::archetype::{Behavior, Stinger};
use crate::player::ceil;
use crate::pose;
use crate::world::{Phase, Thing, World};

/// The grain a stinger's speeds ease by: 1/256 px per frame.
const GRAIN: i64 = 1 << (clamber_fixed::FRAC_BITS - 8);

/// How much of the gap to its target a speed closes each frame.
const EASE: i64 = 32;

/// Puts a stinger at its start `origin`, at its starting speeds, as on
/// the frame it comes into the level.
pub(crate) fn start(thing: &mut Thing, origin: Vec2, stinger: &Stinger) {
    let Vec2 { x: vx, y: vy } = stinger.velocity;
    thing.pos = origin;
    thing.vx = vx;
    thing.phase = Phase::Swaying { vy, tx: vx, ty: vy };
}

/// Moves a swaying stinger one frame around its start `origin`.
pub(crate) fn sway(thing: &mut Thing, origin: Vec2, stinger: &Stinger) {
    let Phase::Swaying { vy, mut tx, mut ty } = thing.phase else {
        return;
    };
    let (left, right, up, down) = stinger.reach;
    let px = |n: i32| Fixed::from_int(n);
    let half = Fixed::from_ratio_pow2(1, 1);
    let (x0, y0) = (origin.x.floor(), ceil(origin.y));
    let mut pos = thing.pos;

    // Heading left while the target is below 0, right otherwise.
    if tx.is_negative() {
        let turn = x0 - i64::from(left);
        if pos.x.floor() < turn {
            pos.x = px(turn as i32) + half;
            tx = -tx;
        }
    } else {
        let turn = x0 + i64::from(right);
        if pos.x.floor() > turn {
            pos.x = px(turn as i32) + half;
            tx = -tx;
        }
    }
    thing.vx = ease(thing.vx, tx);
    pos.x += thing.vx;

    // Heading down while the target is above 0, up otherwise.
    if ty > Fixed::ZERO {
        let turn = y0 + i64::from(down);
        if ceil(pos.y) > turn {
            pos.y = px(turn as i32) - half;
            ty = -ty;
        }
    } else {
        let turn = y0 - i64::from(up);
        if ceil(pos.y) < turn {
            pos.y = px(turn as i32) - half;
            ty = -ty;
        }
    }
    pos.y += vy;
    thing.pos = pos;
    thing.phase = Phase::Swaying {
        vy: ease(vy, ty),
        tx,
        ty,
    };
}

/// `v` moved toward `target` by 1/32 of the gap, rounded toward zero to
/// [`GRAIN`], or onto it if that rounds to 0.
fn ease(v: Fixed, target: Fixed) -> Fixed {
    let step = (target.to_bits() - v.to_bits()) / EASE / GRAIN * GRAIN;
    if step == 0 {
        target
    } else {
        Fixed::from_bits(v.to_bits() + step)
    }
}

/// The stinger that hurts the player on the next frame, if any: one
/// whose offsets from the player, at the end of this one, are within the
/// ranges of its contact for the player's pose and facing.
pub(crate) fn hurt_ahead(world: &World) -> Option<usize> {
    let player = &world.player;
    let x = player.pos.x.floor();
    let y = ceil(player.pos.y);
    world
        .things
        .iter()
        .filter(|t| t.active && matches!(t.phase, Phase::Swaying { .. }))
        .find(|t| {
            let Some(Behavior::Stinger(stinger)) = t.archetype.map(|a| a.behavior) else {
                return false;
            };
            let contact = stinger.contact;
            let reach = contact.reach(pose::pose(player, &contact.poses), player.facing, 0);
            reach.contains(x - t.pos.x.floor(), y - ceil(t.pos.y))
        })
        .map(|t| t.index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64) -> Fixed {
        Fixed::from_f64(v).unwrap()
    }

    #[test]
    fn eases_by_a_32nd_rounded_toward_zero() {
        // stinger.md, "Example", frames 5 and 6.
        assert_eq!(ease(f(-2.0), f(2.0)), f(-1.875));
        assert_eq!(ease(f(-1.875), f(2.0)), f(-1.75390625));
        assert_eq!(ease(f(1.31640625), f(-1.5)), f(1.23046875));
        // Less than 32/256 away, it reaches the target.
        assert_eq!(ease(f(1.0), f(1.0 + 31.0 / 256.0)), f(1.0 + 31.0 / 256.0));
        assert_eq!(ease(f(1.0), f(1.0 + 32.0 / 256.0)), f(1.0 + 1.0 / 256.0));
    }
}
