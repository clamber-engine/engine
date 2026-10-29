//! The other character following the one under control (`follow_trail`
//! above 0), as `spec/behavior/following.md` describes.
//!
//! The character under control, the lead, leaves a trail: the whole-pixel
//! places it was at, one point for each frame it moved to a new one. The
//! other character, the follower, is always on one of those points, its
//! current point, and steps on along the trail while it's `follow_gap` or
//! more behind the newest. So it walks, jumps and falls where the lead
//! did, with no rules of its own.
//!
//! When the lead stands still near it, the follower rests: it walks to
//! `follow_gap` behind or in front of the lead at a speed of its own, and
//! stays there until the lead moves on. Then it makes its way back to the
//! lead's newest point over `follow_rejoin` points, and on along the
//! trail from there.
//!
//! The follower turns to face the lead, in a turn of `follow_turn` frames
//! on the ground, or at once in the air.
//!
//! A swap, the loss of a character and a character joining move the
//! characters their own ways; the follower doesn't follow during them,
//! and neither does it during the frames the lead is held still after a
//! bounce or a defeat.

use std::collections::VecDeque;

use clamber_fixed::{FRAC_BITS, Fixed};
use clamber_level::{Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::columns;
use crate::player::{Facing, Player, State, ceil, frames};
use crate::swap::Other;
use crate::world::World;

/// A whole-pixel place: `x` rounded down and the feet line, `y` rounded
/// up.
pub type Point = (i64, i64);

/// How the follower is following.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Follow {
    /// The lead's points, oldest first. The first is the follower's
    /// current point.
    pub trail: VecDeque<Point>,
    /// The newest point was `follow_gap` or more ahead on the last frame
    /// the follower stood on the ground.
    pub catching_up: bool,
    pub mode: Mode,
    /// How the lead moved on arriving at each of the newest points of
    /// the trail, oldest first. Lined up with the trail's end; the older
    /// points, if the trail has any, have none noted ([`Pace::Other`]).
    pub paces: VecDeque<Pace>,
    /// Frames since the turn under way began, counting its first as 1, if
    /// one is.
    pub turn: Option<u8>,
}

/// How the lead moved on arriving at a point of the trail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Pace {
    #[default]
    Other,
    Walking,
    Running,
}

/// What the follower does on its next frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Following the trail.
    #[default]
    Trail,
    /// Resting: walking to, or standing at, `target` (whole px across).
    /// `gained` once the trail has had a point added since resting began.
    Rest { target: i64, gained: bool },
    /// Staying where it is for a frame after resting, before following
    /// the trail again.
    Wait,
}

/// What the follower goes by of the lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lead {
    point: Point,
    vx: Fixed,
    target: Fixed,
    facing: Facing,
    grounded: bool,
}

impl Lead {
    pub(crate) fn of(player: &Player) -> Lead {
        Lead {
            point: point(player.pos),
            vx: player.vel.x,
            target: player.target,
            facing: player.facing,
            grounded: player.grounded(),
        }
    }
}

/// The whole-pixel place of a position.
pub fn point(pos: Vec2) -> Point {
    (pos.x.floor(), ceil(pos.y))
}

impl Follow {
    /// A trail of the single point `at`.
    pub fn at(at: Point) -> Follow {
        Follow {
            trail: VecDeque::from([at]),
            ..Follow::default()
        }
    }

    /// A trail from `from` back to `to`: `points` points that each go
    /// another share of the way, starting at `from`, then `to` itself.
    pub fn rejoin(from: Point, to: Point, points: i64) -> Follow {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let points = points.max(1);
        let mut trail: VecDeque<Point> = (0..points)
            .map(|k| {
                let across = (k * dx).div_euclid(points);
                let down = -(-k * dy).div_euclid(points);
                (from.0 + across, from.1 + down)
            })
            .collect();
        trail.push_back(to);
        Follow {
            trail,
            ..Follow::default()
        }
    }

    /// How the lead moved on arriving at the current point
    /// (`following.md`, "The follower's pictures").
    pub fn pace(&self) -> Pace {
        let missing = self.trail.len().saturating_sub(self.paces.len());
        match missing {
            0 => self.paces.front().copied().unwrap_or_default(),
            _ => Pace::Other,
        }
    }

    fn current(&self) -> Point {
        self.trail[0]
    }

    fn newest(&self) -> Point {
        self.trail[self.trail.len() - 1]
    }

    /// Makes the next point the current one, unless the current one is
    /// the newest.
    fn step_on(&mut self) {
        if self.trail.len() > 1 {
            self.trail.pop_front();
            if self.paces.len() > self.trail.len() {
                self.paces.pop_front();
            }
        }
    }
}

/// Whether the follower follows on this step, if it does: `Some(true)`
/// if it moves before anything else does, by where the lead was at the
/// end of the step before, and `Some(false)` if it moves after the lead.
pub(crate) fn due(world: &World) -> Option<bool> {
    let other = world.other.as_ref()?;
    let following = world.feel.world[Param::FollowTrail] > Fixed::ZERO
        && world.swap.is_none()
        && world.loss.is_none()
        && world.join.is_none();
    let physics = &world.feel.roster[other.character].physics;
    following.then_some(physics[Param::FollowLag] > Fixed::ZERO)
}

/// One frame of the follower following `lead`.
pub(crate) fn update(world: &mut World, lead: Lead) {
    let level = world.level.clone();
    let Some(other) = &mut world.other else {
        return;
    };
    let physics = world.feel.roster[other.character].physics;
    follow(&level, &physics, other, lead);
}

/// Adds the lead's place at the end of this step to the trail, if it's a
/// new one.
pub(crate) fn note(world: &mut World) {
    let at = point(world.player.pos);
    let most = world.feel.world[Param::FollowTrail].floor().max(1);
    let pace = match world.player.state {
        State::Walk => Pace::Walking,
        State::Run => Pace::Running,
        _ => Pace::Other,
    };
    let Some(other) = &mut world.other else {
        return;
    };
    let follow = &mut other.follow;
    if follow.newest() == at {
        return;
    }
    follow.trail.push_back(at);
    follow.paces.push_back(pace);
    if follow.trail.len() as i64 > most {
        follow.trail.pop_front();
    }
    while follow.paces.len() > follow.trail.len() {
        follow.paces.pop_front();
    }
    if let Mode::Rest { gained, .. } = &mut follow.mode {
        *gained = true;
    }
}

fn follow(level: &Level, physics: &Physics, other: &mut Other, lead: Lead) {
    let turn = frames(physics[Param::FollowTurn]);
    if let Some(n) = &mut other.follow.turn {
        *n = n.saturating_add(1);
        if *n == frames(physics[Param::FollowTurnFlip]) {
            other.facing = flip(other.facing);
        }
    }
    match other.follow.mode {
        Mode::Trail => trail(level, physics, other, lead),
        Mode::Rest { target, gained } => {
            let x = other.pos.x.floor();
            let share = physics[Param::FollowRestShare].floor().max(1);
            let speed = Fixed::from_bits(((target - x) << FRAC_BITS) / share);
            other.pos.x += speed;
            let feet = Fixed::from_int(ceil(other.pos.y) as i32);
            if let Some(floor) = columns::floor(level, other.pos.x.floor(), feet) {
                other.pos.y = floor.y;
            }
            face(level, other, lead);
            if gained {
                let points = physics[Param::FollowRejoin].floor();
                let newest = other.follow.newest();
                other.follow = Follow {
                    mode: Mode::Wait,
                    turn: other.follow.turn,
                    ..Follow::rejoin(point(other.pos), newest, points)
                };
            }
        }
        Mode::Wait => {
            face(level, other, lead);
            other.follow.mode = Mode::Trail;
        }
    }
    if other.follow.turn.is_some_and(|n| n >= turn) {
        other.follow.turn = None;
    }
}

/// A frame of following the trail: on to the current point, then maybe
/// on to the next, and maybe to rest.
fn trail(level: &Level, physics: &Physics, other: &mut Other, lead: Lead) {
    let (x, feet) = other.follow.current();
    other.pos = Vec2::new(
        Fixed::from_int(x as i32) + other.pos.x - Fixed::from_int(other.pos.x.floor() as i32),
        other.pos.y + Fixed::from_int((feet - ceil(other.pos.y)) as i32),
    );
    let follow = &mut other.follow;
    if follow.newest().0 != x {
        face(level, other, lead);
    }
    let follow = &mut other.follow;
    let gap = physics[Param::FollowGap].floor();
    if !stands(level, (x, feet)) {
        follow.step_on();
    } else if (follow.newest().0 - x).abs() >= gap {
        if follow.catching_up {
            follow.step_on();
        } else {
            follow.catching_up = true;
        }
    } else {
        follow.catching_up = false;
    }
    if let Some(target) = rest_spot(level, physics, other, lead) {
        other.follow.mode = Mode::Rest {
            target,
            gained: false,
        };
    }
}

/// Where the follower rests from this frame, if it starts to.
fn rest_spot(level: &Level, physics: &Physics, other: &Other, lead: Lead) -> Option<i64> {
    let gap = physics[Param::FollowGap].floor();
    let x = other.pos.x.floor();
    let still = lead.grounded
        && lead.target == Fixed::ZERO
        && lead.vx.abs() < physics[Param::FollowRestSpeed];
    if !still
        || !stands(level, other.follow.current())
        || ((lead.point.0 - x).abs() - gap).abs() < 2
    {
        return None;
    }
    let behind = match lead.facing {
        Facing::Left => [gap, -gap],
        Facing::Right => [-gap, gap],
    };
    let feet = ceil(other.pos.y);
    let reach = physics[Param::FollowRestReach].floor();
    behind.into_iter().map(|d| lead.point.0 + d).find(|&spot| {
        columns::spans(level, spot)
            .iter()
            .any(|s| (ceil(s.top) - feet).abs() < reach)
    })
}

/// Turns the follower toward the lead if it faces away, and isn't
/// turning already: at once off the ground, or else in a turn.
fn face(level: &Level, other: &mut Other, lead: Lead) {
    let x = other.pos.x.floor();
    let away = match other.facing {
        Facing::Left => lead.point.0 >= x,
        Facing::Right => lead.point.0 < x,
    };
    if !away || other.follow.turn.is_some() {
        return;
    }
    if stands(level, other.follow.current()) {
        other.follow.turn = Some(1);
    } else {
        other.facing = flip(other.facing);
    }
}

/// Whether a body at `at` stands on the ground: the ground below it is
/// at its feet line.
fn stands(level: &Level, (x, feet): Point) -> bool {
    columns::floor(level, x, Fixed::from_int(feet as i32)).is_some_and(|f| ceil(f.y) == feet)
}

fn flip(facing: Facing) -> Facing {
    match facing {
        Facing::Left => Facing::Right,
        Facing::Right => Facing::Left,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejoins_a_sixteenth_of_the_way_per_point() {
        // following.md's made-up example: from 976 back to 1005.
        let f = Follow::rejoin((976, 300), (1005, 300), 16);
        let xs: Vec<_> = f.trail.iter().map(|p| p.0).collect();
        assert_eq!(
            xs,
            [
                976, 977, 979, 981, 983, 985, 986, 988, 990, 992, 994, 995, 997, 999, 1001, 1003,
                1005
            ]
        );
        assert!(f.trail.iter().all(|p| p.1 == 300));
        // Rounding is down across and up and down.
        let f = Follow::rejoin((100, 300), (90, 290), 16);
        assert_eq!(f.trail[1], (99, 300));
        assert_eq!(f.trail[2], (98, 299));
    }
}
