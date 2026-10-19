//! Walking into an area (spec/behavior/exits.md, "Walking in"): a world
//! begun at an [`Arrival`] that walks in has the characters walk in
//! towards its point, with `walk_in_speed` above 0.
//!
//! Counting the area's frames from 0, on frame `k` up to
//! `walk_in_time` − 1 the lead is `walk_in_back` − `walk_in_speed` × `k`
//! behind the point, facing the way it walks, and the other character is
//! `walk_in_other` behind the lead. On frame `walk_in_time` the lead
//! steps to `walk_in_ahead` past the point, and the other character
//! stays where it is. From the frame after, the player has control, the
//! lead on the ground with a speed of `walk_in_speed` the way it walked,
//! and the other character follows along the points the lead walked
//! through. Nothing held or pressed before then has any effect. The
//! picture fades in from black a step every `walk_in_fade_step` frames
//! ([`World::brightness`]).
//!
//! The view starts where it does when an area begins ([`View::begun`]),
//! doesn't move up to frame `walk_in_time` − 1, and on that frame aims
//! with `view_begin_back` in place of the back and facing parts (view.md,
//! "When an area begins").
//!
//! Nothing placed in the area is there on frame 0: what is there as the
//! area begins is there from frame 1 (placements.md, "In the first
//! level's room" and "In the first level's scene").
//!
//! Engine choices where the spec says nothing: while the characters walk
//! in, everything else waits, as while they're in the level's entrance,
//! and the level's entrance is gone, as after it burst. The characters
//! touch nothing. Objects wait for frame 1 in every area walked into,
//! the level after the scene too.
//!
//! [`World::brightness`]: crate::World::brightness
//! [`View::begun`]: crate::View::begun

use std::collections::VecDeque;

use clamber_fixed::Fixed;
use clamber_level::{Arrival, Dir, Vec2};
use clamber_pack::{Param, Physics};

use crate::follow::{Follow, Point, point};
use crate::player::{Facing, State, frames};
use crate::view::{self, Begin};
use crate::world::World;

/// The characters walking into the area, from its first frame until the
/// picture has faded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WalkingIn {
    /// The point they walk in to.
    pub to: Vec2,
    /// The way they walk.
    pub facing: Facing,
}

impl WalkingIn {
    /// How `arrival` walks in with `physics`, if it does.
    pub(crate) fn of(arrival: &Arrival, physics: &Physics) -> Option<WalkingIn> {
        let facing = match arrival.walk? {
            Dir::Left => Facing::Left,
            Dir::Right => Facing::Right,
            Dir::Up | Dir::Down => return None,
        };
        (physics[Param::WalkInSpeed] > Fixed::ZERO).then_some(WalkingIn {
            to: arrival.at,
            facing,
        })
    }

    /// +1 walking right, −1 walking left.
    fn sign(self) -> i64 {
        match self.facing {
            Facing::Right => 1,
            Facing::Left => -1,
        }
    }

    /// Where the lead is on frame `k` of the walk, up to `walk_in_time`.
    pub(crate) fn lead_at(self, k: u32, physics: &Physics) -> Vec2 {
        let time = u32::from(frames(physics[Param::WalkInTime]));
        let behind = if k >= time {
            -physics[Param::WalkInAhead]
        } else {
            let k = i32::try_from(k).unwrap_or(i32::MAX);
            physics[Param::WalkInBack] - physics[Param::WalkInSpeed] * Fixed::from_int(k)
        };
        Vec2::new(self.to.x - behind * self.sign_fixed(), self.to.y)
    }

    fn sign_fixed(self) -> Fixed {
        Fixed::from_int(self.sign() as i32)
    }
}

impl World {
    /// The step of brightness while the picture fades in as the
    /// characters walk in, out of how many, or `None` outside the fade.
    pub(crate) fn walking_in_brightness(&self) -> Option<(u32, u32)> {
        self.walking_in?;
        let physics = &self.feel.world;
        let step = u32::from(frames(physics[Param::WalkInFadeStep]).max(1));
        let full = physics[Param::WalkInFadeSteps].floor().max(1) as u32;
        // After frame `f`, the step is `f` / `walk_in_fade_step`.
        let now = self.frame.checked_sub(1).map_or(0, |f| f / step);
        (now < full).then_some((now, full))
    }

    /// Whether the characters walked in on their own on the last step.
    pub fn walking_in(&self) -> bool {
        let time = u32::from(frames(self.feel.world[Param::WalkInTime]));
        self.walking_in.is_some() && self.frame <= time + 1
    }
}

/// On the frames up to `walk_in_time`, the characters walk in and
/// everything else waits; on the frame after, the player takes over.
/// Returns whether this is a frame they walk in on.
pub(crate) fn hold(world: &mut World) -> bool {
    let Some(walk) = world.walking_in else {
        return false;
    };
    let physics = world.feel.world;
    let k = world.frame;
    let time = u32::from(frames(physics[Param::WalkInTime]));
    if k > time {
        if k == time + 1 {
            take_over(world, walk, &physics);
        }
        if world.walking_in_brightness().is_none() {
            world.walking_in = None;
        }
        return false;
    }
    // The area's objects are there from frame 1, none on frame 0.
    match k {
        0 => {
            world.waiting = (0..world.things.len())
                .filter(|&i| world.things[i].active)
                .collect();
            for &i in &world.waiting {
                world.things[i].active = false;
            }
        }
        1 => {
            for i in std::mem::take(&mut world.waiting) {
                world.things[i].active = true;
            }
        }
        _ => {}
    }
    let lead = walk.lead_at(k, &physics);
    let player = &mut world.player;
    player.pos = lead;
    player.vel = Vec2::new(physics[Param::WalkInSpeed] * walk.sign_fixed(), Fixed::ZERO);
    player.target = player.vel.x;
    player.facing = walk.facing;
    player.state = State::Walk;
    player.turn = None;
    if k < time
        && let Some(other) = &mut world.other
    {
        other.pos = Vec2::new(
            lead.x - physics[Param::WalkInOther] * walk.sign_fixed(),
            lead.y,
        );
        other.facing = walk.facing;
    }
    if k == time {
        if let Some(other) = &mut world.other {
            other.follow = Follow {
                trail: trail(walk, point(other.pos), point(lead), &physics),
                ..Follow::default()
            };
        }
        let back = world.player_physics()[Param::ViewBeginBack];
        let begin = Begin {
            back: (back > Fixed::ZERO).then_some(back),
            ..Begin::default()
        };
        view::update(world, begin);
    }
    true
}

/// The lead on the ground at the walk's speed, as the player takes over.
fn take_over(world: &mut World, walk: WalkingIn, physics: &Physics) {
    let player = &mut world.player;
    player.vel = Vec2::new(physics[Param::WalkInSpeed] * walk.sign_fixed(), Fixed::ZERO);
    player.target = player.vel.x;
    player.ground = world
        .level
        .collision
        .cast(player.pos, Dir::Down, Fixed::ZERO)
        .map(|hit| hit.collider);
    if !player.grounded() {
        player.state = State::Fall;
    }
}

/// The trail the other character follows from `from` once the lead is at
/// `lead`: the points the lead walked through ahead of it, then `lead`.
fn trail(walk: WalkingIn, from: Point, lead: Point, physics: &Physics) -> VecDeque<Point> {
    let time = u32::from(frames(physics[Param::WalkInTime]));
    let ahead = |p: Point| (p.0 - from.0) * walk.sign() > 0 && (lead.0 - p.0) * walk.sign() > 0;
    let mut trail = VecDeque::from([from]);
    trail.extend(
        (0..time)
            .map(|k| point(walk.lead_at(k, physics)))
            .filter(|&p| ahead(p)),
    );
    trail.push_back(lead);
    trail
}
