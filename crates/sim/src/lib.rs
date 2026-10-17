//! The deterministic simulation core of the Clamber engine.
//!
//! A [`World`] holds a level, its entities (enemies and objects behave
//! as their [`Archetype`] says, and some can be carried), the player,
//! the [`Other`] character if there is one, and the player's progress
//! through the level ([`Flow`]), and advances one logic frame at a time
//! with [`step`], always at 60 Hz. Nothing
//! inside a step reads the clock, uses floating point or depends on
//! hash-map iteration order: the same world and the same inputs give the
//! same result, byte for byte ([`World::state_bytes`]), on every platform.
//!
//! Rendering and audio read a world; they never change it.
//!
//! An animal comes out of a crate the player breaks, and the player can
//! mount it ([`Hutch`]).
//!
//! A stinger sways around where it's placed and hurts on any contact
//! ([`Stinger`]).
//!
//! A character missing from the level joins the player when an object
//! holding it breaks ([`Join`]).
//!
//! Collectibles add to the collectible count ([`Release`]), and letters
//! to the letters taken ([`LetterRow`]).
//!
//! Walking past a level's [`END`] marker finishes it, after a walk-out
//! and a fade if the preset has them ([`Ending`]). A [`DIM`] entity
//! near the view starts the [`Dimming`] of the level's background.
//!
//! With a preset that has one, a [`View`] follows the player, and
//! walkers and throwers are in the level only near it.
//!
//! [`Script`] and [`run`] drive a world headlessly from recorded input,
//! for tests and the `clamber-sim` command, and [`record`] writes what
//! happens as a trace (`spec/traces/FORMAT.md`).

mod archetype;
mod carry;
mod collect;
mod columns;
mod dimming;
mod ending;
mod flow;
mod follow;
mod hutch;
mod join;
mod loss;
mod player;
mod pose;
mod record;
mod ride;
mod script;
mod stinger;
mod swap;
mod swim;
mod things;
mod throwers;
mod view;
mod world;

pub use archetype::{
    AIR_KINDS, Archetype, ArchetypeError, Archetypes, Arrive, ArriveRule, Ball, Behavior, Contact,
    Fall, Hit, Holder, Hutch, MAX_AIR_STEPS, MAX_ARRIVE_RULES, MAX_HOP_STEPS, MAX_LETTERS,
    MAX_RESPONSE_PARTS, MAX_THROWS, MAX_TURN_PARTS, Offsets, Reach, Response, Ride, RiderReach,
    Span, Stinger, Throw, WalkerContact,
};
pub use carry::Flight;
pub use clamber_fixed::Fixed;
pub use collect::{LetterRow, Release};
pub use dimming::{DIM, Dimming};
pub use ending::{END, Ending};
pub use flow::{
    CHECKPOINT, DYING_FRAMES, Flow, FlowEvent, GEM, GOAL, Outcome, RELIC, START_LIVES, reach,
};
pub use follow::{Follow, Mode, Point, point as follow_point};
pub use join::Join;
pub use loss::{Loss, Lost};
pub use player::{
    Act, Boost, Crouch, Facing, Gait, GroundAction, HALF_WIDTH, HEIGHT, Hands, Holds, Knockout,
    Player, ROLL, Roll, RollSize, SWAP_FRAMES, State, Stride,
};
pub use pose::{Air, Pose, PoseClock, Stance, pose, stance};
pub use record::record;
pub use ride::{RidePart, Riding};
pub use script::{InputFrame, Script};
pub use swap::{Move, Other, Recent, Swap};
pub use swim::swim_depth;
pub use things::{DEFEATED_FRAMES, LAUNCH_REST};
pub use throwers::BALL;
pub use view::View;
pub use world::{Event, Phase, Thing, World, generic_feel, run, step};

/// Logic frames per second.
pub const STEPS_PER_SECOND: u32 = 60;
