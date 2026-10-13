//! Level flow: reaching the goal, losing lives, checkpoints and pickups.
//!
//! Four entity kinds take part, each triggered when the player's body
//! overlaps its box ([`reach`]):
//!
//! - `goal`: completes the level.
//! - `checkpoint`: the first time it's touched, it becomes where the
//!   player comes back after losing a life. Its `at` is where the
//!   player's feet go.
//! - `gem` and `relic`: picked up and counted; relics are the rare ones.
//!   Pickups stay picked up after losing a life.
//!
//! Getting hurt, by touching a hazard surface or an enemy
//! ([`things`](crate::things)), or falling below the level costs a life.
//! The player stays where it was for [`DYING_FRAMES`], or is knocked out
//! if the physics say so (`knockout_jump`), then comes back at the last
//! checkpoint reached, or the level's start. Losing the last life ends
//! the world with [`Outcome::GameOver`].

use clamber_fixed::Fixed;
use clamber_level::Vec2;

use clamber_pack::Param;

use crate::player::{Knockout, Player, State};
use crate::world::{Event, World};

pub const GOAL: &str = "goal";
pub const CHECKPOINT: &str = "checkpoint";
pub const GEM: &str = "gem";
pub const RELIC: &str = "relic";

/// Lives at the start of a level.
pub const START_LIVES: u8 = 3;

/// How long the player stays where it lost a life before coming back.
pub const DYING_FRAMES: u8 = 60;

/// Where the level stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Outcome {
    #[default]
    Playing,
    /// A life was just lost; the player comes back after this many more
    /// frames, or the game is over.
    Dying(u8),
    /// The goal was reached. The world no longer changes.
    Complete,
    /// The last life was lost. The world no longer changes.
    GameOver,
}

impl Outcome {
    /// The world no longer changes.
    pub fn is_over(self) -> bool {
        matches!(self, Outcome::Complete | Outcome::GameOver)
    }
}

/// Something in the level's flow that happened during a step. These
/// aren't trace vocabulary, so they're kept apart from
/// [`Event`](crate::Event).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlowEvent {
    /// The thing at this index was picked up.
    Collect(usize),
    /// The checkpoint at this index was reached.
    Checkpoint(usize),
    /// A life was lost.
    Die,
    /// The player came back after losing a life.
    Respawn,
    Complete,
    GameOver,
    /// The enemy at this index was defeated.
    Defeat(usize),
    /// The launcher at this index fired.
    Launch(usize),
}

/// Progress through the level.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flow {
    pub outcome: Outcome,
    pub lives: u8,
    pub gems: u32,
    pub relics: u32,
    /// Indices of the checkpoints reached, in order; the player comes
    /// back at the last.
    pub checkpoints: Vec<usize>,
    /// What happened during the last step, in order.
    pub events: Vec<FlowEvent>,
}

impl Flow {
    pub fn new() -> Flow {
        Flow {
            lives: START_LIVES,
            ..Flow::default()
        }
    }
}

/// An entity's box, as `(left, top, right, bottom)` around `at`, if its
/// kind takes part in level flow.
pub fn reach(kind: &str, at: Vec2) -> Option<(Fixed, Fixed, Fixed, Fixed)> {
    let int = Fixed::from_int;
    // Half width, height above `at`, depth below it.
    let (half, up, down) = match kind {
        GOAL => (int(16), int(64), int(0)),
        CHECKPOINT => (int(8), int(48), int(0)),
        GEM | RELIC => (int(8), int(8), int(8)),
        _ => return None,
    };
    Some((at.x - half, at.y - up, at.x + half, at.y + down))
}

/// Whether two `(left, top, right, bottom)` boxes overlap.
pub(crate) fn overlap(a: (Fixed, Fixed, Fixed, Fixed), b: (Fixed, Fixed, Fixed, Fixed)) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// Counts down a lost life; true while the player is out of play.
pub(crate) fn dying(world: &mut World) -> bool {
    let Outcome::Dying(left) = world.flow.outcome else {
        return false;
    };
    if world.player.knockout.is_some() {
        // The knock-out plays out in steps of its own first.
        return false;
    }
    if left > 1 {
        world.flow.outcome = Outcome::Dying(left - 1);
    } else if world.flow.lives == 0 {
        world.flow.outcome = Outcome::GameOver;
        world.flow.events.push(FlowEvent::GameOver);
    } else {
        let at = match world.flow.checkpoints.last() {
            Some(&i) => world.level.entities[i].at,
            None => world.level.start,
        };
        let character = world.player.character;
        world.player = Player::new(&world.level, at);
        world.player.character = character;
        world.flow.outcome = Outcome::Playing;
        world.flow.events.push(FlowEvent::Respawn);
    }
    true
}

/// Applies what the player touched during the step just taken.
/// `hurt_by` is the enemy that hurt the player on this step, if one did
/// from the step before.
pub(crate) fn update(world: &mut World, hurt_by: Option<usize>) {
    let knockout = world.player_physics()[Param::KnockoutJump] > Fixed::ZERO;
    let level = &world.level;
    let flow = &mut world.flow;
    let body = world.player.body();

    for thing in world.things.iter_mut().filter(|t| t.active) {
        let Some(area) = reach(&thing.kind, thing.pos) else {
            continue;
        };
        if !overlap(body, area) {
            continue;
        }
        match thing.kind.as_str() {
            GOAL => {
                flow.outcome = Outcome::Complete;
                flow.events.push(FlowEvent::Complete);
                return;
            }
            CHECKPOINT if !flow.checkpoints.contains(&thing.index) => {
                flow.checkpoints.push(thing.index);
                flow.events.push(FlowEvent::Checkpoint(thing.index));
            }
            GEM | RELIC => {
                if thing.kind == GEM {
                    flow.gems += 1;
                } else {
                    flow.relics += 1;
                }
                thing.active = false;
                flow.events.push(FlowEvent::Collect(thing.index));
            }
            _ => {}
        }
    }

    let hazard = world
        .player
        .touched
        .iter()
        .any(|&c| level.surface(c).hazard);
    if hazard && !world.events.contains(&Event::Hurt) {
        world.events.push(Event::Hurt);
    }
    let hurt = world.events.contains(&Event::Hurt);
    if hurt {
        world.player.state = State::Hurt;
    }
    let fell = body.1 > level.size.y;
    if hurt || fell {
        flow.lives = flow.lives.saturating_sub(1);
        flow.outcome = Outcome::Dying(DYING_FRAMES);
        flow.events.push(FlowEvent::Die);
    }
    if hurt && !fell && knockout {
        // The life is lost on the step after the knock-out's last.
        world.player.knockout = Some(Knockout::new(hurt_by));
        flow.outcome = Outcome::Dying(1);
    }
}
