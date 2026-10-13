//! The world and its fixed step.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_fixed::Fixed;
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Feel, Physics, Presets};

use crate::archetype::{Archetype, ArchetypeError, Archetypes, Behavior};
use crate::flow::{self, Flow, FlowEvent, Outcome};
use crate::player::{Facing, Player, ROLL};
use crate::script::{InputFrame, Script};
use crate::things;

/// Something that happened during a step. Names follow
/// `spec/traces/VOCABULARY.md`, so traces can record them directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Event {
    /// Became grounded.
    Land,
    /// Stopped being grounded without jumping.
    LeaveGround,
    /// Facing direction changed.
    Turn,
    /// A jump began.
    JumpStart,
    /// Jump was let go while still rising.
    JumpRelease,
    /// A roll began.
    RollStart,
    /// A roll ended.
    RollEnd,
    /// Bounced off an enemy.
    Bounce,
    /// Took damage.
    Hurt,
}

impl Event {
    pub fn name(self) -> &'static str {
        match self {
            Event::Land => "land",
            Event::LeaveGround => "leave_ground",
            Event::Turn => "turn",
            Event::JumpStart => "jump_start",
            Event::JumpRelease => "jump_release",
            Event::RollStart => "roll_start",
            Event::RollEnd => "roll_end",
            Event::Bounce => "bounce",
            Event::Hurt => "hurt",
        }
    }
}

/// One of the level's entities, as it is now.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Thing {
    /// Index into the level's entities.
    pub index: usize,
    pub kind: String,
    pub pos: Vec2,
    /// Gone things, such as collected or defeated ones, stay in the list
    /// so indices are stable.
    pub active: bool,
    /// How it behaves, if its kind is an archetype.
    pub archetype: Option<Archetype>,
    /// The way it's going or looking.
    pub facing: Facing,
    /// Horizontal speed, px per frame, for a walker.
    pub vx: Fixed,
    /// Steps it has moved, which a patrol follows its path by.
    pub clock: u32,
    pub phase: Phase,
}

/// What an archetype thing is doing besides its behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Behaving as usual.
    #[default]
    Ready,
    /// Defeated; gone after this many more frames.
    Defeated(u8),
    /// A launcher that fired this many frames ago.
    Fired(u8),
}

/// Everything the simulation advances.
#[derive(Clone, Debug)]
pub struct World {
    pub level: Arc<Level>,
    pub feel: Arc<Feel>,
    /// Steps taken so far.
    pub frame: u32,
    pub player: Player,
    pub things: Vec<Thing>,
    /// What happened during the last step, in order.
    pub events: Vec<Event>,
    /// Goal, lives, checkpoints and pickups.
    pub flow: Flow,
}

impl World {
    /// The level at its start, with the lead character at the start
    /// position. `feel` needs at least one character.
    ///
    /// # Panics
    ///
    /// If an entity's params don't fit its archetype; [`World::try_new`]
    /// reports that instead.
    pub fn new(level: Arc<Level>, feel: Arc<Feel>) -> World {
        World::try_new(level, feel).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The level at its start, or why an entity's params don't fit its
    /// archetype in the built-in [`Archetypes`].
    pub fn try_new(level: Arc<Level>, feel: Arc<Feel>) -> Result<World, ArchetypeError> {
        assert!(!feel.roster.is_empty(), "a world needs a character");
        let player = Player::new(&level, level.start);
        let archetypes = Archetypes::builtin();
        let things = level
            .entities
            .iter()
            .enumerate()
            .map(|(index, e)| {
                let path = format!("entities[{index}]");
                let archetype = archetypes.resolve(e, &level.name, &path)?;
                // A walker starts out at full speed.
                let (facing, vx) = match archetype.map(|a| a.behavior) {
                    Some(Behavior::Walker { facing, speed, .. }) => match facing {
                        Facing::Left => (facing, -speed),
                        Facing::Right => (facing, speed),
                    },
                    _ => (Facing::Right, Fixed::ZERO),
                };
                Ok(Thing {
                    index,
                    kind: e.kind.clone(),
                    pos: e.at,
                    active: true,
                    archetype,
                    facing,
                    vx,
                    clock: 0,
                    phase: Phase::Ready,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(World {
            level,
            feel,
            frame: 0,
            player,
            things,
            events: Vec::new(),
            flow: Flow::new(),
        })
    }

    /// The player's physics where it is now: the character's own, then
    /// the regions it's in, then the surface it stands on.
    pub fn player_physics(&self) -> Physics {
        let player = &self.player;
        let mut physics = self.feel.roster[player.character].physics;
        for region in self.level.regions_at(player.pos) {
            if let Some(patch) = &region.physics {
                physics.apply(patch);
            }
        }
        if let Some(ground) = player.ground {
            physics.apply(&self.level.surface(ground).physics);
        }
        physics
    }

    /// The state the simulation advances, encoded as bytes. Two worlds
    /// built from the same level and feel are in the same state exactly
    /// when these are equal.
    pub fn state_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let fixed = |out: &mut Vec<u8>, v: Fixed| out.extend(v.to_bits().to_le_bytes());
        let vec2 = |out: &mut Vec<u8>, v: Vec2| {
            fixed(out, v.x);
            fixed(out, v.y);
        };
        let count = |out: &mut Vec<u8>, n: usize| out.extend((n as u64).to_le_bytes());

        out.extend(self.frame.to_le_bytes());

        let p = &self.player;
        count(&mut out, p.character);
        vec2(&mut out, p.pos);
        vec2(&mut out, p.vel);
        match p.ground {
            Some(collider) => {
                out.push(1);
                count(&mut out, collider);
            }
            None => out.push(0),
        }
        out.push(match p.facing {
            Facing::Left => 0,
            Facing::Right => 1,
        });
        out.push(p.state as u8);
        out.push(p.frozen);
        out.push(p.jump_buffer.age().map_or(0, |age| age.saturating_add(1)));
        out.push(p.coyote.map_or(0, |n| n.saturating_add(1)));
        out.extend([p.lockout, p.jump_needs_hold.into()]);
        out.extend([p.jump_held_since_press, p.jump_hold, p.takeoff].map(u8::from));
        out.push(p.swap);
        out.extend([p.crouch.held, p.crouch.rising]);
        out.push(p.turn.map_or(0, |n| n.saturating_add(1)));
        match p.roll {
            Some(roll) => out.extend([
                1,
                roll.left,
                roll.dir as u8,
                roll.size as u8,
                roll.held.into(),
                roll.carry.into(),
                roll.still.into(),
            ]),
            None => out.push(0),
        }
        out.push(p.roll_stop);
        match p.roll_press {
            Some((age, window, hold)) => out.extend([1, age, window, hold.into()]),
            None => out.push(0),
        }
        out.push(p.since_run.map_or(0, |n| n.saturating_add(1)));
        out.extend([p.lone_run, p.roll_jump].map(u8::from));

        count(&mut out, self.things.len());
        for t in &self.things {
            count(&mut out, t.index);
            count(&mut out, t.kind.len());
            out.extend(t.kind.as_bytes());
            vec2(&mut out, t.pos);
            out.push(t.active.into());
            out.push(match t.facing {
                Facing::Left => 0,
                Facing::Right => 1,
            });
            fixed(&mut out, t.vx);
            out.extend(t.clock.to_le_bytes());
            out.extend(match t.phase {
                Phase::Ready => [0, 0],
                Phase::Defeated(left) => [1, left],
                Phase::Fired(since) => [2, since],
            });
        }

        count(&mut out, self.events.len());
        out.extend(self.events.iter().map(|&e| e as u8));

        let f = &self.flow;
        match f.outcome {
            Outcome::Playing => out.push(0),
            Outcome::Dying(left) => out.extend([1, left]),
            Outcome::Complete => out.push(2),
            Outcome::GameOver => out.push(3),
        }
        out.push(f.lives);
        out.extend(f.gems.to_le_bytes());
        out.extend(f.relics.to_le_bytes());
        count(&mut out, f.checkpoints.len());
        for &i in &f.checkpoints {
            count(&mut out, i);
        }
        count(&mut out, f.events.len());
        for e in &f.events {
            let (tag, index) = match *e {
                FlowEvent::Collect(i) => (0, i),
                FlowEvent::Checkpoint(i) => (1, i),
                FlowEvent::Die => (2, 0),
                FlowEvent::Respawn => (3, 0),
                FlowEvent::Complete => (4, 0),
                FlowEvent::GameOver => (5, 0),
                FlowEvent::Defeat(i) => (6, i),
                FlowEvent::Launch(i) => (7, i),
            };
            out.push(tag);
            count(&mut out, index);
        }
        out
    }
}

/// Advances the world one logic frame (1/60 s). Once the level is
/// complete or the game is over, nothing changes.
pub fn step(world: &mut World, input: &InputFrame) {
    world.events.clear();
    world.flow.events.clear();
    if world.flow.outcome.is_over() {
        return;
    }
    if flow::dying(world) {
        world.frame += 1;
        return;
    }
    let physics = world.player_physics();
    let previous_feet = world.player.pos.y;
    world.player.update(
        &world.level,
        &world.feel,
        &physics,
        &input.players[0],
        &mut world.events,
    );
    things::update(world, previous_feet);
    flow::update(world);
    world.frame += 1;
}

/// Runs every step of `script`, calling `after_step` after each one.
pub fn run(world: &mut World, script: &Script, mut after_step: impl FnMut(&World)) {
    for input in script.frames() {
        step(world, &input);
        after_step(world);
    }
}

/// The feel for playing without a pack: one character that moves exactly
/// as the preset's world physics say and can roll. `None` if there's no
/// such preset.
pub fn generic_feel(preset: &str) -> Option<Feel> {
    let presets = Presets::builtin();
    let world = presets.world(preset)?;
    Some(Feel {
        preset: preset.to_string(),
        title: presets.get(preset)?.title.clone(),
        world,
        roster: vec![CharacterFeel {
            physics: world,
            abilities: BTreeMap::from([(ROLL.to_string(), BTreeMap::new())]),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::HALF_WIDTH;
    use clamber_input::{Action, Actions};

    /// Flat ground, a 45-degree slope up to a plateau, then a wall.
    const HILL: &str = r#"
        format = 1
        name = "hill"
        size = [512, 256]
        start = [32, 100]

        [[collision]]
        shape = "polygon"
        points = [[0, 200], [100, 200], [164, 136], [300, 136], [300, 0],
                  [316, 0], [316, 256], [0, 256]]
    "#;

    fn world(src: &str) -> World {
        let level = Level::from_toml_str(src, "test").unwrap();
        World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
    }

    fn hold(world: &mut World, held: Actions, frames: usize) -> Vec<Event> {
        let mut events = Vec::new();
        let script = Script {
            held: vec![held; frames],
        };
        run(world, &script, |w| events.extend(&w.events));
        events
    }

    #[test]
    fn falls_lands_climbs_and_stops_at_a_wall() {
        let mut w = world(HILL);
        assert!(!w.player.grounded());

        assert_eq!(hold(&mut w, Actions::NONE, 60), [Event::Land]);
        assert_eq!(w.player.pos, Vec2::ints(32, 200));

        let events = hold(&mut w, Action::Right.into(), 300);
        assert!(events.is_empty(), "{events:?}");
        assert!(w.player.grounded());
        assert_eq!(
            w.player.pos,
            Vec2::new(Fixed::from_int(300) - HALF_WIDTH, Fixed::from_int(136))
        );
        assert_eq!(w.player.vel.x, Fixed::ZERO);

        let events = hold(&mut w, Action::Left.into(), 1);
        assert_eq!(events, [Event::Turn]);
        assert_eq!(w.player.facing, Facing::Left);
        assert_eq!(w.frame, 361);
    }
}
