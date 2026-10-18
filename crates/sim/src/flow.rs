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
//!   Pickups stay picked up after losing a life, unless `resume_afresh`.
//! - `tonic`: gives back a hit point ([`hits`](crate::hits)). It's only
//!   taken when one is missing.
//!
//! Getting hurt, by touching a hazard surface or an enemy
//! ([`things`](crate::things)), or falling below the level costs a life,
//! unless the other character is there to take over
//! ([`loss`](crate::loss)), or the hit leaves hit points
//! ([`hits`](crate::hits)). With `fall_out_depth`, a fall costs the life
//! only that far below the bottom, and with `fall_out_time` the level
//! goes on that long after it (falling-out.md).
//! The player stays where it was for [`DYING_FRAMES`], or is knocked out
//! if the physics say so (`knockout_jump`), then comes back at the last
//! checkpoint reached, or the level's start. Losing the last life ends
//! the world with [`Outcome::GameOver`].
//!
//! With `resume_afresh` above 0, the level's things are as at its start
//! when the player comes back ([`afresh`]), as restart-barrel.md has it
//! after a knock-out ("What is there again after a knock-out"): the
//! counts, the lives and the restart points reached are kept, the
//! letters taken only when play comes back at a restart point (and
//! those letters stay gone), and the lead is the character that was
//! leading. Engine choices where the spec says nothing: this is so
//! after any lost life, a fall-out too; restart barrels and entrances
//! stay as they were; letters still joining are forgotten, and are in
//! the level again; and after an exit, what an exit kept gone
//! (exits.md) is there again too.

use clamber_fixed::Fixed;
use clamber_level::Vec2;

use clamber_pack::Param;

use crate::archetype::Behavior;
use crate::collect::{
    self, LetterEnd, LetterShow, LivesGain, LostLife, Pickup, Release, RelicFlight,
};
use crate::hits;
use crate::keg;
use crate::loss;
use crate::player::{Facing, Knockout, Player, State, frames};
use crate::sections::Placing;
use crate::swap::Other;
use crate::view;
use crate::world::{Event, Phase, Thing, World};

pub const GOAL: &str = "goal";
pub const CHECKPOINT: &str = "checkpoint";
pub const GEM: &str = "gem";
pub const RELIC: &str = "relic";
pub const TONIC: &str = "tonic";

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
    /// The lead left the area through the exit, or past the end marker
    /// leading to another area, at this entity index. The world no longer
    /// changes.
    Exited(usize),
    /// The area's scene is over, and play goes on where it leads
    /// ([`Scene`](clamber_level::Scene)). The world no longer changes.
    Ended,
}

impl Outcome {
    /// The world no longer changes.
    pub fn is_over(self) -> bool {
        matches!(
            self,
            Outcome::Complete | Outcome::GameOver | Outcome::Exited(_) | Outcome::Ended
        )
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
    /// One more life, from the collectible count or the letters.
    Life,
    /// This letter joined the letters taken.
    Letter(u8),
    /// The lead left the area through the exit, or past the end marker
    /// leading to another area, at this entity index.
    Exited(usize),
    /// The area's scene is over.
    Ended,
}

/// Progress through the level.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flow {
    pub outcome: Outcome,
    pub lives: u8,
    /// The collectible count: gems, and what collectibles add.
    pub gems: u32,
    /// Relics taken, and collectibles that are a `relic` (figure.md's
    /// figure count).
    pub relics: u32,
    /// Steps on which one more reaches the collectible count, one entry
    /// each.
    pub due: Vec<u32>,
    /// Taken collectibles still releasing what they're worth.
    pub releases: Vec<Release>,
    /// The letters taken, as bits: bit `k - 1` for letter `k`. A letter
    /// joins them a while after it's taken (its `join_delay`). With all
    /// of the level's word (`letter_count`), the player gets one more
    /// life, and they start again from none.
    pub letters: u8,
    /// Letters taken that haven't joined yet: the step each joins on,
    /// and the letter.
    pub joining: Vec<(u32, u8)>,
    /// The letter row shown after the last letter taken, and the one
    /// before it, which may still be sliding out
    /// ([`World::letter_rows`](crate::World::letter_rows)).
    pub letter_shown: Option<LetterShow>,
    pub letter_before: Option<LetterShow>,
    /// The letter row's ending, from the step the word's last letter
    /// joined on ([`World::letter_row`](crate::World::letter_row)).
    pub letter_end: Option<LetterEnd>,
    /// The number the count display shows
    /// ([`World::count_display`](crate::World::count_display)): it
    /// follows `gems` one step a frame.
    pub shown: u32,
    /// The step `shown` last went up on.
    pub last_step: Option<u32>,
    /// Pickups still owed steps of `shown`, in the order they were taken.
    pub pickups: Vec<Pickup>,
    /// The lives gained since the lives display last started, for it:
    /// by collectibles giving lives, by `shown` going from `count_life`
    /// less one to 0, and by the letter row's ending.
    pub gains: Vec<LivesGain>,
    /// The last life lost, for the lives display, until play comes
    /// back.
    pub lost: Option<LostLife>,
    /// The image of the relic taken last, for
    /// [`World::relic_image`](crate::World::relic_image).
    pub relic_flight: Option<RelicFlight>,
    /// Indices of the checkpoints reached, in order; the player comes
    /// back at the last.
    pub checkpoints: Vec<usize>,
    /// Hit points lost in this life (`hit_points`).
    pub hits_taken: u8,
    /// Frames since a hit that cost a hit point, until its protection
    /// and the wait for control are over.
    pub recovery: Option<u16>,
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
        GEM | RELIC | TONIC => (int(8), int(8), int(8)),
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
        world.player.vel = Vec2::default();
        world.flow.outcome = Outcome::Dying(left - 1);
    } else if world.flow.lives == 0 {
        world.flow.outcome = Outcome::GameOver;
        world.flow.events.push(FlowEvent::GameOver);
    } else {
        let at = match world.flow.checkpoints.last() {
            Some(&i) => world.restart_point(i),
            None => world.level.start,
        };
        let character = world.player.character;
        world.player = Player::new(&world.level, at);
        world.player.character = character;
        // Both characters come back where play starts, the same one
        // leading (following.md).
        let player = &world.player;
        if let Some(other) = &mut world.other {
            *other = Other::new(other.character, player.pos, player.facing, player.pos);
        }
        world.flow.outcome = Outcome::Playing;
        world.flow.hits_taken = 0;
        world.flow.lost = None;
        world.flow.events.push(FlowEvent::Respawn);
    }
    true
}

/// On the step a knock-out's flight stops, sets when the lives display
/// for the life it cost bursts.
pub(crate) fn flight_stopped(world: &mut World) {
    let stopped = world.player.knockout.is_some_and(|k| k.rest.is_some());
    let burst = frames(world.feel.world[Param::LostLivesBurst]);
    if let Some(lost) = world
        .flow
        .lost
        .as_mut()
        .filter(|l| stopped && l.burst.is_none())
    {
        lost.burst = Some(world.frame + u32::from(burst));
    }
}

/// On the step the player comes back after a lost life, once the view
/// is where it starts: with `resume_afresh` above 0, the level's things
/// are as at its start, there or not by that view, but for restart
/// barrels and entrances. Coming back at a restart point, the letters
/// taken stay gone; at the level's start, none are taken any more.
pub(crate) fn afresh(world: &mut World) {
    if world.feel.world[Param::ResumeAfresh] <= Fixed::ZERO {
        return;
    }
    let level = world.level.clone();
    let flow = &mut world.flow;
    let at_restart = !flow.checkpoints.is_empty();
    if !at_restart {
        flow.letters = 0;
        flow.letter_shown = None;
        flow.letter_before = None;
        flow.letter_end = None;
    }
    flow.joining.clear();
    let letters = flow.letters;
    // Thrown balls come after the level's entities.
    world.things.truncate(level.entities.len());
    let mut fresh = vec![false; world.things.len()];
    for (thing, fresh) in world.things.iter_mut().zip(&mut fresh) {
        match thing.archetype.map(|a| a.behavior) {
            Some(Behavior::Keg(_) | Behavior::Entrance(_)) => {}
            Some(Behavior::Collect { letter, .. })
                if letter > 0
                    && letters & 1u8.checked_shl(u32::from(letter) - 1).unwrap_or(0) != 0 =>
            {
                thing.active = false;
                thing.phase = Phase::Gone;
            }
            _ => {
                *thing = Thing::placed(thing.index, &level.entities[thing.index], thing.archetype);
                *fresh = true;
            }
        }
    }
    world.bursts.clear();
    world.waiting.clear();
    let physics = world.player_physics();
    let Some(v) = world.view.filter(|_| view::by_view(&physics)) else {
        return;
    };
    let (left, physics) = view::window(&level, &world.feel, v.x(), &physics);
    let present = view::at_start(&world.things, &level, left, &physics);
    for ((thing, present), fresh) in world.things.iter_mut().zip(present).zip(&fresh) {
        thing.active &= present || !fresh;
    }
    if let Some(placing) = &mut world.placing {
        *placing = Placing::begin(placing.placed.clone(), v.y());
        for ((thing, spot), fresh) in world
            .things
            .iter_mut()
            .zip(&placing.placed.spot_of)
            .zip(&fresh)
        {
            thing.active &= spot.is_none() || !fresh;
        }
    }
}

/// Applies what the player touched during the step just taken.
/// `hurt_by` is the enemy that hurt the player on this step, if one did;
/// `facing` is the way the player faced before it.
pub(crate) fn update(world: &mut World, hurt_by: Option<usize>, facing: Facing) {
    keg::touch(world);
    let knockout = world.player_physics()[Param::KnockoutJump] > Fixed::ZERO;
    let life = u32::try_from(world.feel.world[Param::CountLife].floor()).unwrap_or(0);
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
                    collect::count(flow, life);
                } else {
                    flow.relics += 1;
                }
                thing.active = false;
                flow.events.push(FlowEvent::Collect(thing.index));
            }
            TONIC if flow.hits_taken > 0 => {
                flow.hits_taken -= 1;
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
    if hazard && !world.events.contains(&Event::Hurt) && !world.protected() {
        world.events.push(Event::Hurt);
    }
    let hurt = world.events.contains(&Event::Hurt);
    // falling-out.md: past the bottom by more than the depth, measured
    // after this step's move.
    let depth = world.feel.world[Param::FallOutDepth];
    let fell = if depth > Fixed::ZERO {
        world.player.pos.y > level.size.y + depth
    } else {
        body.1 > level.size.y
    };
    let fall_out_time = frames(world.feel.world[Param::FallOutTime]);
    if hurt && !fell && loss::can_start(world) {
        // The hit character is lost instead of a life.
        loss::start(world);
        return;
    }
    if hurt && !fell && hits::take(world, hurt_by.or(world.hit_by)) {
        // The hit costs a hit point instead of a life.
        return;
    }
    let frame = world.frame;
    let flow = &mut world.flow;
    if hurt {
        world.player.state = State::Hurt;
    }
    if hurt || fell {
        flow.recovery = None;
        let before = flow.lives;
        flow.lives = flow.lives.saturating_sub(1);
        let physics = &world.feel.world;
        let (into, burst) = if fell {
            (Param::LostLivesFallIn, Some(Param::LostLivesFallBurst))
        } else {
            (Param::LostLivesIn, None)
        };
        let into = frames(physics[into]);
        flow.lost = (into > 0).then(|| LostLife {
            at: frame,
            into: frame + u32::from(into),
            burst: burst.map(|b| frame + u32::from(frames(physics[b]))),
            before,
            after: flow.lives,
            knocked: !fell,
        });
        flow.outcome = Outcome::Dying(DYING_FRAMES);
        flow.events.push(FlowEvent::Die);
    }
    if fell && fall_out_time > 0 {
        // The level's last frame is `fall_out_time` steps on; the player
        // comes back on the step after it.
        flow.outcome = Outcome::Dying(fall_out_time.saturating_add(1));
    }
    if hurt && !fell && knockout {
        // The life is lost on the step after the knock-out's last.
        world.player.knockout = Some(Knockout::new(hurt_by));
        flow.outcome = Outcome::Dying(1);
        let physics = world.player_physics();
        if physics[Param::KnockoutHold] == Fixed::ZERO {
            // The hit frame moved as any other; the flight starts on the
            // next. Facing changes only from the way it faced before.
            let toward = hurt_by
                .filter(|_| physics[Param::KnockoutWithEnemy] > Fixed::ZERO)
                .and_then(|i| world.things.get(i))
                .map(|t| t.heading);
            world.player.knock(&physics, toward);
            world.events.retain(|&e| e != Event::Turn);
            if world.player.facing != facing {
                world.events.push(Event::Turn);
            }
        }
    }
}
