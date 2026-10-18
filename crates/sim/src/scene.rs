//! An area that plays by itself, a level's [`Scene`], as
//! `spec/behavior/exits.md` has the first level's scene ("The scene").
//!
//! The characters walk in as into any area ([`walk_in`]). From the frame
//! after `walk_in_time` the lead stands still, facing the way it walked,
//! where it was at the end of that frame, and the other character stays
//! where it is; nothing held or pressed has any effect, and everything
//! else waits. The picture fades out a step every `fade_step` frames
//! from the scene's `fade` frame, at full brightness, and is black on
//! its `last` frame ([`World::brightness`]). At the end of the last frame
//! the area is over ([`Outcome::Ended`]), whatever was pressed.
//!
//! Engine choice where the spec says nothing: play that comes in without
//! walking in stands still from frame 0.
//!
//! [`Scene`]: clamber_level::Scene
//! [`walk_in`]: crate::WalkingIn
//! [`World::brightness`]: crate::World::brightness

use clamber_fixed::Fixed;

use crate::flow::{FlowEvent, Outcome};
use crate::player::State;
use crate::world::World;

impl World {
    /// The step of brightness while a scene's picture fades out, out of
    /// how many, or `None` before the fade or outside a scene.
    pub(crate) fn scene_brightness(&self) -> Option<(u32, u32)> {
        let scene = self.level.scene.as_ref()?;
        // The frame just shown.
        let shown = self.frame.checked_sub(1)?;
        let since = shown.checked_sub(scene.fade)?;
        let step = scene.fade_step.max(1);
        let full = (scene.last.saturating_sub(scene.fade) / step).max(1);
        Some((full - (since / step).min(full), full))
    }
}

/// On every frame of a scene once the characters have walked in, holds
/// the lead still and everything else; on its last frame, ends the area.
/// `walking` is whether this is a frame they walk in on. Returns whether
/// the area is a scene.
pub(crate) fn hold(world: &mut World, walking: bool) -> bool {
    let Some(last) = world.level.scene.as_ref().map(|s| s.last) else {
        return false;
    };
    if !walking {
        let player = &mut world.player;
        player.vel.x = Fixed::ZERO;
        player.vel.y = Fixed::ZERO;
        player.target = Fixed::ZERO;
        player.state = State::Idle;
        player.turn = None;
    }
    if world.frame == last {
        world.flow.outcome = Outcome::Ended;
        world.flow.events.push(FlowEvent::Ended);
    }
    true
}
