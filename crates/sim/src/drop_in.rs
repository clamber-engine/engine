//! Dropping into an area (spec/behavior/wall-exits.md, "Coming back from
//! the second wall exit's area"): a world begun at an [`Arrival`] that
//! drops in has the lead come in in the air at its point, out of no
//! entrance.
//!
//! Counting the area's frames from 0, on frame 0 the lead is at the
//! point, not moving, facing right, and nothing held has any effect.
//! From frame 1 it plays by the usual rules, falling from a standstill.
//! Nothing placed in the area is there on frame 0: what is there as the
//! area begins is there from frame 1, after the lead's move, so it
//! touches the lead from frame 2 (a launch barrel at the point catches
//! it then). The picture fades in from black a step every
//! `walk_in_fade_step` frames, as when walking in ([`World::brightness`]).
//!
//! Engine choices where the spec says nothing (its runs had the lead
//! alone): the other character starts at the point too and follows as
//! usual; everything else waits on frame 0, as while walking in; the
//! view starts where it does when an area begins ([`View::begun`]) and
//! follows the lead as usual from frame 1; the level's entrance takes no
//! part, as for any entrance an arrival doesn't name.
//!
//! [`Arrival`]: clamber_level::Arrival
//! [`World::brightness`]: crate::World::brightness
//! [`View::begun`]: crate::View::begun

use clamber_pack::Param;

use crate::player::frames;
use crate::world::World;

impl World {
    /// The step of brightness while the picture fades in after dropping
    /// in, out of how many, or `None` outside the fade.
    pub(crate) fn dropping_in_brightness(&self) -> Option<(u32, u32)> {
        if !self.dropping_in {
            return None;
        }
        let physics = &self.feel.world;
        let step = u32::from(frames(physics[Param::WalkInFadeStep]).max(1));
        let full = physics[Param::WalkInFadeSteps].floor().max(1) as u32;
        // After frame `f`, the step is `f` / `walk_in_fade_step`.
        let now = self.frame.checked_sub(1).map_or(0, |f| f / step);
        (now < full).then_some((now, full))
    }
}

/// On frame 0 after dropping in, the lead and everything else wait, and
/// the area's objects aren't there. Returns whether this is that frame.
pub(crate) fn hold(world: &mut World) -> bool {
    if !world.dropping_in {
        return false;
    }
    if world.frame > 0 {
        if world.dropping_in_brightness().is_none() {
            world.dropping_in = false;
        }
        return false;
    }
    world.waiting = (0..world.things.len())
        .filter(|&i| world.things[i].active)
        .collect();
    for &i in &world.waiting {
        world.things[i].active = false;
    }
    true
}

/// At the end of frame 1 after dropping in, the area's objects are there.
pub(crate) fn after(world: &mut World) {
    if world.dropping_in && world.frame == 1 {
        for i in std::mem::take(&mut world.waiting) {
            world.things[i].active = true;
        }
    }
}
