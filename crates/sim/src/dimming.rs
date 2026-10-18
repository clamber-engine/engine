//! The dimming of a level's background near its end, as
//! `spec/behavior/dimming.md` describes.
//!
//! A [`DIM`] entity starts it, with a preset whose `dim_steps` is above
//! 0 and a view: on the first frame that starts with the entity near the
//! view, by the rule for a walker's start (`appear_left` and
//! `appear_right`), call it T. Nothing changes on T. The first step is on
//! the first step frame from T + `dim_delay`, and the others follow every
//! `dim_step_time` frames up to `dim_steps` steps. Step frames are the
//! frames whose number is a whole multiple of `dim_step_time`: the spec
//! leaves the cycle's place to the engine, as long as it doesn't move.
//! Once started, the dimming runs to the end whatever happens: the view
//! moving back, the player standing still or falling out.
//!
//! The simulation only counts the steps ([`World::dim_step`]); what they
//! do to the colours is up to whatever draws the level.
//!
//! Engine choice where the spec says nothing: when the level starts
//! again after a lost life, the dimming starts again too.

use clamber_fixed::Fixed;
use clamber_pack::Param;

use crate::player::frames;
use crate::view;
use crate::world::World;

/// The kind of entity that starts the dimming.
pub const DIM: &str = "dim";

/// The dimming, once started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dimming {
    /// The frame its entity came near the view on: T.
    pub started: u32,
}

impl World {
    /// The dimming's step on the last frame taken, and its number of
    /// steps: `(0, steps)` before the first step or without the dimming,
    /// `(steps, steps)` after the last.
    pub fn dim_step(&self) -> (u32, u32) {
        let physics = &self.feel.world;
        let steps = u32::from(frames(physics[Param::DimSteps]));
        let Some(dimming) = self.dimming else {
            return (0, steps);
        };
        let every = u32::from(frames(physics[Param::DimStepTime]).max(1));
        let earliest = dimming.started + u32::from(frames(physics[Param::DimDelay]));
        let first = earliest.div_ceil(every) * every;
        let Some(last) = self.frame.checked_sub(1).filter(|&f| f >= first) else {
            return (0, steps);
        };
        (((last - first) / every + 1).min(steps), steps)
    }
}

/// Starts the dimming if a dim entity is near the view the step starts
/// with.
pub(crate) fn start(world: &mut World) {
    if world.dimming.is_some() || world.feel.world[Param::DimSteps] <= Fixed::ZERO {
        return;
    }
    let Some(left) = world.view.map(|v| v.x()) else {
        return;
    };
    let (left, physics) = view::window(&world.level, &world.feel, left, &world.player_physics());
    let near = world
        .level
        .entities
        .iter()
        .any(|e| e.kind == DIM && view::near(e.at.x.floor(), left, &physics));
    if near {
        world.dimming = Some(Dimming {
            started: world.frame,
        });
    }
}
