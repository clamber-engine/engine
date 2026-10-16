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
//! Collectibles add to the collectible count ([`Release`]).
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
mod flow;
mod follow;
mod loss;
mod player;
mod record;
mod script;
mod swap;
mod things;
mod throwers;
mod view;
mod world;

pub use archetype::{
    Archetype, ArchetypeError, Archetypes, Arrive, ArriveRule, Ball, Behavior, Fall, Hit,
    MAX_AIR_STEPS, MAX_ARRIVE_RULES, MAX_THROWS, Reach, Span, Throw,
};
pub use carry::Flight;
pub use clamber_fixed::Fixed;
pub use collect::Release;
pub use flow::{
    CHECKPOINT, DYING_FRAMES, Flow, FlowEvent, GEM, GOAL, Outcome, RELIC, START_LIVES, reach,
};
pub use follow::{Follow, Mode, Point, point as follow_point};
pub use loss::{Loss, Lost};
pub use player::{
    Act, Boost, Crouch, Facing, Gait, GroundAction, HALF_WIDTH, HEIGHT, Hands, Knockout, Player,
    ROLL, Roll, RollSize, SWAP_FRAMES, State, Stride,
};
pub use record::record;
pub use script::{InputFrame, Script};
pub use swap::{Move, Other, Recent, Swap};
pub use things::{DEFEATED_FRAMES, LAUNCH_REST};
pub use throwers::BALL;
pub use view::View;
pub use world::{Event, Phase, Thing, World, generic_feel, run, step};

/// Logic frames per second.
pub const STEPS_PER_SECOND: u32 = 60;
