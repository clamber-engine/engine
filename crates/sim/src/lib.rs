//! The deterministic simulation core of the Clamber engine.
//!
//! A [`World`] holds a level, its entities, the player and its progress
//! through the level ([`Flow`]), and advances one logic frame at a time
//! with [`step`], always at 60 Hz. Nothing
//! inside a step reads the clock, uses floating point or depends on
//! hash-map iteration order: the same world and the same inputs give the
//! same result, byte for byte ([`World::state_bytes`]), on every platform.
//!
//! Rendering and audio read a world; they never change it.
//!
//! [`Script`] and [`run`] drive a world headlessly from recorded input,
//! for tests, traces and the `clamber-sim` command.

mod flow;
mod player;
mod script;
mod world;

pub use clamber_fixed::Fixed;
pub use flow::{
    CHECKPOINT, DYING_FRAMES, Flow, FlowEvent, GEM, GOAL, Outcome, RELIC, START_LIVES, reach,
};
pub use player::{Facing, HALF_WIDTH, HEIGHT, Player, ROLL, SWAP_FRAMES, State};
pub use script::{InputFrame, Script};
pub use world::{Event, Thing, World, generic_feel, run, step};

/// Logic frames per second.
pub const STEPS_PER_SECOND: u32 = 60;
