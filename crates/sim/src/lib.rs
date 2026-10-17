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
//! Touching a restart barrel breaks it and makes it where play resumes
//! after a lost life, inside one ([`Keg`], [`Resume`]).
//!
//! When a level with an entrance begins, the characters wait hidden in
//! it until it bursts and throws them out ([`Entering`]).
//!
//! A hidden object isn't drawn and does nothing until the ground action
//! strikes it or the player lands on it after a long fall; it then
//! releases an item that becomes a bunch or a barrel ([`HIDDEN`]).
//!
//! A spring catches a player that touches it, squashes and throws it
//! up, and a Jump press on its top makes a spring jump ([`SPRING`]).
//!
//! A rope sways; the player catches it from a jump, climbs it, turns
//! round it, and jumps or slides off it ([`ROPE`]).
//!
//! A launch barrel catches the player, holds it and shoots it out
//! ([`SLING`]).
//!
//! A character missing from the level joins the player when an object
//! holding it breaks ([`Join`]).
//!
//! Collectibles add to the collectible count ([`Release`]), shown by
//! the count display ([`CountDisplay`]), letters to the letters taken
//! ([`LetterRow`]), and a taken relic's image flies to the top of the
//! picture ([`RelicImage`]).
//!
//! Walking past a level's [`END`] marker finishes it, after a walk-out
//! and a fade if the preset has them ([`Ending`]). An [`EXIT`] takes the
//! lead out of the area, walking it out as the picture fades
//! ([`Leaving`]), and so does a [`WALL_EXIT`] once a thrown object has
//! opened it. A [`DIM`] entity near the view starts the [`Dimming`]
//! of the level's background.
//!
//! With a preset that has one, a [`View`] follows the player, and
//! walkers and throwers are in the level only near it. Placed objects
//! divided into [`Sections`] appear by the view's path ([`Opening`]).
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
mod entrance;
mod exit;
mod flow;
mod follow;
mod hidden;
mod hutch;
mod join;
mod keg;
mod loss;
mod player;
mod pose;
mod record;
mod ride;
mod rope;
mod script;
mod sections;
mod sling;
mod spring;
mod stinger;
mod swap;
mod swim;
mod things;
mod throwers;
mod view;
mod walk_in;
mod world;

pub use archetype::{
    AIR_KINDS, Archetype, ArchetypeError, Archetypes, Arrive, ArriveRule, Ball, Behavior, Contact,
    Entrance, Fall, HIDDEN_ITEMS, Hidden, Hit, Holder, Hutch, KEG_PIECES, Keg, MAX_AIR_STEPS,
    MAX_ARRIVE_RULES, MAX_HOP_STEPS, MAX_LETTERS, MAX_RESPONSE_PARTS, MAX_SHAKE_STAGES,
    MAX_SPRING_RUNS, MAX_STRIKE_FRAMES, MAX_THROWS, MAX_TURN_PARTS, Offsets, Opens, Reach,
    Response, Ride, RiderReach, Rope, Sling, Span, Spring, Stinger, Throw, WalkerContact,
};
pub use carry::Flight;
pub use clamber_fixed::Fixed;
pub use collect::{
    CountDisplay, LetterEnd, LetterRow, LivesDisplay, LivesStep, Pickup, Release, RelicFlight,
    RelicImage,
};
pub use dimming::{DIM, Dimming};
pub use ending::{END, Ending};
pub use entrance::Entering;
pub use exit::{EXIT, Leaving, WALL_EXIT, area};
pub use flow::{
    CHECKPOINT, DYING_FRAMES, Flow, FlowEvent, GEM, GOAL, Outcome, RELIC, START_LIVES, reach,
};
pub use follow::{Follow, Mode, Point, point as follow_point};
pub use hidden::HIDDEN;
pub use join::Join;
pub use keg::{Burst, Resume};
pub use loss::{Loss, Lost};
pub use player::{
    Act, Boost, Crouch, FaceSwitch, Facing, Gait, GroundAction, HALF_WIDTH, HEIGHT, Hands, Holds,
    Knockout, Landing, Player, ROLL, Roll, RollSize, SWAP_FRAMES, State, Stride,
};
pub use pose::{Air, Pose, PoseClock, Stance, hurt_pose, pose, stance};
pub use record::record;
pub use ride::{RidePart, Riding};
pub use rope::{OnRope, ROPE};
pub use script::{InputFrame, Script};
pub use sections::{Opening, Placed, Placing, Sections, Spot, Window};
pub use sling::{SLING, Slung};
pub use spring::{OnSpring, SPRING, SpringArc, top as spring_top};
pub use swap::{Move, Other, Recent, Swap};
pub use swim::swim_depth;
pub use things::{DEFEATED_FRAMES, LAUNCH_REST};
pub use throwers::BALL;
pub use view::View;
pub use walk_in::WalkingIn;
pub use world::{Event, Phase, Thing, World, generic_feel, run, step};

/// Logic frames per second.
pub const STEPS_PER_SECOND: u32 = 60;
