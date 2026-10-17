//! Entering a level (spec/behavior/level-start.md): a level with a thing
//! whose archetype has an [`Entrance`] behavior begins with the
//! characters inside it.
//!
//! Counting the level's frames from 0, every character waits at the
//! level's start point, hidden and not moving, whatever is held. The
//! entrance bursts on frame `burst`, leaving a [`Burst`] as a keg does,
//! and on frame `throw` it's gone and the lead is drawn and given its
//! throw speed, without moving yet. From the frame after, it's thrown out
//! until it lands, and from the frame after the
//! landing it plays as usual. The other character waits at the start
//! point up to and including the throw's frame; from the frame after
//! until the lead lands, it's where the lead was on the frame before,
//! rounded down to whole px, and from then it follows the lead. The
//! picture fades in from black over the level's first frames
//! ([`World::brightness`]).
//!
//! Engine choices where the spec says nothing:
//!
//! - Only a level's first entrance acts; a level without one begins
//!   with the characters in play at the start point.
//! - Up to and including the throw's frame, everything else waits too,
//!   as while the player is in a keg, but the view.
//!
//! With `view_begin_back` above 0 the view starts and follows as when an
//! area begins ([`view_begin`]); otherwise it starts settled on the lead
//! and doesn't move up to the throw's frame.
//! - The other character is drawn from the frame after the throw's.
//! - The burst's pieces and flash move as a keg's do, by the entrance's
//!   own values.
//! - A Jump press during the throw before the landing frame is
//!   forgotten. One on the landing frame is noted as usual.
//! - A hit during the throw ends it: the player is hurt as usual.
//!
//! [`World::brightness`]: crate::World::brightness

use std::collections::VecDeque;

use clamber_fixed::Fixed;
use clamber_input::Frame;
use clamber_level::Vec2;
use clamber_pack::Param;

use crate::archetype::{Behavior, Entrance};
use crate::follow::{Follow, point};
use crate::keg::Burst;
use crate::player::{Facing, State, frames};
use crate::view::Begin;
use crate::world::{Phase, Thing, World};

/// The characters entering the level, from its first frame until the
/// lead has landed and the picture has faded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Entering {
    /// The entrance's thing.
    pub entrance: usize,
    /// The lead has landed after the throw.
    pub landed: bool,
}

/// How a level with `things` begins: inside its first entrance, if it
/// has one.
pub(crate) fn begin(things: &[Thing]) -> Option<Entering> {
    things.iter().find_map(|t| match t.archetype?.behavior {
        Behavior::Entrance(_) => Some(Entering {
            entrance: t.index,
            landed: false,
        }),
        _ => None,
    })
}

/// The entering under way and its entrance's values, if there is one.
fn entering(world: &World) -> Option<(Entering, Entrance)> {
    let e = world.entering?;
    match world.things.get(e.entrance)?.archetype?.behavior {
        Behavior::Entrance(values) => Some((e, values)),
        _ => None,
    }
}

impl World {
    /// Whether the lead is still hidden in the level's entrance.
    pub fn in_entrance(&self) -> bool {
        entering(self).is_some_and(|(_, v)| self.frame <= u32::from(v.throw))
    }

    /// Whether the other character is still hidden in the level's
    /// entrance.
    pub fn other_in_entrance(&self) -> bool {
        entering(self).is_some_and(|(_, v)| self.frame <= u32::from(v.throw) + 1)
    }

    /// The step of brightness while the picture fades in at the start of
    /// a level with an entrance, out of how many, or `None` outside the
    /// fade.
    pub(crate) fn entering_brightness(&self) -> Option<(u32, u32)> {
        let (_, v) = entering(self)?;
        let full = u32::from(v.fade_steps);
        // After frame `f`, the step is `f` / `fade_step`.
        let now = self
            .frame
            .checked_sub(1)
            .map_or(0, |f| f / u32::from(v.fade_step));
        (now < full).then_some((now, full))
    }
}

/// How the view follows on this frame while the characters enter, with
/// `view_begin_back` above 0 (view.md, "When an area begins"): up to the
/// throw's frame with `view_begin_back` in place of the back and facing
/// parts, and from the frame after until the lead lands with
/// `view_begin_back_far`, quickly from `view_begin_far` frames after the
/// throw's. To be taken before the frame's step changes the entering.
pub(crate) fn view_begin(world: &World) -> Begin {
    let Some((e, v)) = entering(world) else {
        return Begin::default();
    };
    let physics = world.player_physics();
    let back = physics[Param::ViewBeginBack];
    if e.landed || back <= Fixed::ZERO {
        return Begin::default();
    }
    let Some(since) = world
        .frame
        .checked_sub(u32::from(v.throw))
        .filter(|&n| n > 0)
    else {
        return Begin {
            back: Some(back),
            fast: 0,
        };
    };
    let far = u32::from(frames(physics[Param::ViewBeginFar]));
    Begin {
        back: Some(physics[Param::ViewBeginBackFar]),
        fast: since.saturating_sub(far),
    }
}

/// On the frames up to and including the throw's, everything waits
/// while the characters are in the entrance; the entrance bursts, and
/// then the lead is thrown. Returns whether this is such a frame.
pub(crate) fn hold(world: &mut World) -> bool {
    let Some((e, v)) = entering(world) else {
        return false;
    };
    let n = world.frame;
    if n > u32::from(v.throw) {
        return false;
    }
    let player = &mut world.player;
    player.vel = Vec2::default();
    player.ground = None;
    player.state = State::Fall;
    player.facing = Facing::Right;
    let at = world.things[e.entrance].pos;
    if n == u32::from(v.burst) {
        let moves = (v.piece_gravity, v.flash_rise, v.burst_time);
        world
            .bursts
            .push(Burst::of(e.entrance, at, v.pieces, moves));
    }
    if n == u32::from(v.throw) {
        let thing = &mut world.things[e.entrance];
        thing.active = false;
        thing.phase = Phase::Broken;
        world.player.vel = Vec2::new(v.throw_speed.x, -v.throw_speed.y);
    }
    true
}

/// A frame of the lead being thrown out, if it is: it moves by the
/// throw's rules. Returns whether it was.
pub(crate) fn throw(world: &mut World, input: &Frame) -> bool {
    let Some((e, v)) = entering(world) else {
        return false;
    };
    if e.landed || world.player.state == State::Hurt {
        return false;
    }
    let since = (world.frame - u32::from(v.throw)).min(u32::from(u16::MAX)) as u16;
    let physics = world.player_physics();
    world
        .player
        .thrown_out(&world.level, &physics, input, &v, since, &mut world.events);
    true
}

/// After a frame of the throw, the other character is where the lead
/// was at its start, `before`, rounded down; the entering is over once
/// the lead has landed and the picture has faded in.
pub(crate) fn after(world: &mut World, before: Vec2) {
    let Some((mut e, v)) = entering(world) else {
        return;
    };
    if world.frame <= u32::from(v.throw) {
        return;
    }
    if !e.landed {
        let lead = point(world.player.pos);
        if let Some(other) = &mut world.other {
            other.pos = Vec2::new(
                Fixed::from_int(before.x.floor() as i32),
                Fixed::from_int(before.y.floor() as i32),
            );
            let mut trail = VecDeque::from([point(other.pos)]);
            if lead != trail[0] {
                trail.push_back(lead);
            }
            other.follow = Follow {
                trail,
                ..Follow::default()
            };
        }
        e.landed = world.player.grounded() || world.player.state == State::Hurt;
    }
    world.entering = (!e.landed || world.entering_brightness().is_some()).then_some(e);
}
