//! A character missing from the level joining the player when an object
//! holding it breaks, as `spec/behavior/following.md` ("Joining") and
//! `spec/behavior/holder-breaking.md` describe.
//!
//! On the step the object breaks, if the roster has a character that
//! isn't in the level and characters swap places, that character appears
//! where the object broke: its whole-pixel place, `x` rounded down and
//! `y` up. For `join_time` steps it flies to where the player was on that
//! step, along an arc above the straight line between the two; on the
//! step after, it stands there, as the [`Other`] character with a fresh
//! trail, and follows from the next one.
//!
//! Until it stands there, the rest of the world waits: the player,
//! walkers and throwers hold still, as the spec says, and so does
//! everything else here, which it doesn't cover. Input is ignored, and
//! once the character stands there the player forgets any press it was
//! still waiting to use. A button pressed during the wait and still held
//! isn't a press afterwards either, as presses come from what was held
//! on the step before.

use clamber_fixed::{FRAC_BITS, Fixed};
use clamber_level::Vec2;
use clamber_pack::Param;

use crate::follow::{Follow, Point, point};
use crate::player::{Facing, frames};
use crate::swap::Other;
use crate::world::World;

/// A character joining the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Join {
    /// Steps since the one the object broke on, which is 0.
    pub frame: u8,
    /// Where the object broke, as a whole-pixel place.
    pub from: Point,
    /// Where the player was at the end of that step.
    pub to: Point,
    /// How high above the straight line from `from` to `to` the
    /// character is on this step.
    pub lift: Fixed,
}

/// Frees the character missing from the level from an object that broke
/// at `at` on this step, after the player moved: it appears there and
/// starts to join. Nothing comes out if no character is missing.
pub(crate) fn start(world: &mut World, at: Vec2) {
    let feel = &world.feel;
    let swaps = feel.roster.len() > 1 && feel.world[Param::SwapMoves] > Fixed::ZERO;
    if !swaps || world.other.is_some() || world.join.is_some() {
        return;
    }
    let character = if world.player.character == 0 { 1 } else { 0 };
    let (from, to) = (point(at), point(world.player.pos));
    // A character that was lost, still running out of the level, is the
    // one coming out; it's not shown running out any more.
    if world.lost.as_ref().is_some_and(|l| l.apart) {
        world.lost = None;
    }
    let facing = if to.0 < from.0 {
        Facing::Left
    } else {
        Facing::Right
    };
    world.other = Some(Other {
        character,
        pos: whole(from),
        facing,
        follow: Follow::at(to),
    });
    world.join = Some(Join {
        frame: 0,
        from,
        to,
        lift: Fixed::ZERO,
    });
}

/// One step of a join under way: the character flies on, or stands at
/// the player once the flight is over, which ends the join.
///
/// On flight step `k` of `n` (`join_time`), its `x` is `k`/`n` of the
/// way across, rounded down, and its `y` is the straight line's, `k`/`n`
/// of the way down rounded up, less the lift: `join_lift` + `join_rise` ×
/// `k` − `join_sag` × `k` × (`k` + 1), plus the fraction of the lift on
/// the step before. Standing at the player, its trail is `follow_rejoin`
/// points from there back to the player's newest place, the first its
/// current point, as after resting but without the player's place
/// itself at the end.
pub(crate) fn step(world: &mut World) {
    let Some(mut join) = world.join else {
        return;
    };
    let lead = point(world.player.pos);
    let Some(other) = &mut world.other else {
        world.join = None;
        return;
    };
    let physics = world.feel.roster[other.character].physics;
    let world_physics = &world.feel.world;
    let time = i64::from(frames(world_physics[Param::JoinTime]));
    join.frame = join.frame.saturating_add(1);
    let k = i64::from(join.frame);
    if k > time {
        other.pos = whole(join.to);
        let points = physics[Param::FollowRejoin].floor();
        let mut follow = Follow::rejoin(join.to, lead, points);
        follow.trail.pop_back();
        other.follow = follow;
        world.join = None;
        world.player.forget_presses();
        return;
    }
    let (from, to) = (join.from, join.to);
    let x = from.0 + (k * (to.0 - from.0)).div_euclid(time);
    let line = from.1 - (-k * (to.1 - from.1)).div_euclid(time);
    let n = |v: i64| Fixed::from_int(v as i32);
    let up = world_physics[Param::JoinLift] + world_physics[Param::JoinRise] * n(k)
        - world_physics[Param::JoinSag] * n(k * (k + 1));
    join.lift = fraction(join.lift) + up;
    other.pos = Vec2::new(n(x), n(line) - join.lift);
    world.join = Some(join);
}

/// The position of a whole-pixel place.
fn whole((x, y): Point) -> Vec2 {
    Vec2::new(Fixed::from_int(x as i32), Fixed::from_int(y as i32))
}

/// `v` less `v` rounded down.
fn fraction(v: Fixed) -> Fixed {
    Fixed::from_bits(v.to_bits() & ((1 << FRAC_BITS) - 1))
}
