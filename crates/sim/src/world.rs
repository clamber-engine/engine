//! The world and its fixed step.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_fixed::Fixed;
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Feel, Param, Physics, Presets};

use crate::archetype::{Archetype, ArchetypeError, Archetypes, Behavior};
use crate::carry::{self, Flight};
use crate::collect;
use crate::dimming::{self, Dimming};
use crate::ending::{self, Ending};
use crate::flow::{self, Flow, FlowEvent, Outcome};
use crate::follow::{self, Lead, Mode};
use crate::join::{self, Join};
use crate::loss::{self, Loss, Lost};
use crate::player::{Act, Facing, Player, ROLL, Roll, frames};
use crate::ride;
use crate::script::{InputFrame, Script};
use crate::swap::{self, Move, Other, Recent, Swap};
use crate::things;
use crate::view::{self, View};

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
    /// A swimming stroke began.
    Stroke,
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
            Event::Stroke => "stroke",
        }
    }
}

/// One of the level's entities, as it is now.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Thing {
    /// Index into the level's entities, or for a thrown ball, which comes
    /// after them, into the things.
    pub index: usize,
    pub kind: String,
    pub pos: Vec2,
    /// Gone things, such as collected or defeated ones, stay in the list
    /// so indices are stable.
    pub active: bool,
    /// How it behaves, if its kind is an archetype.
    pub archetype: Option<Archetype>,
    /// The way it looks, which it's drawn with.
    pub facing: Facing,
    /// The way a walker is heading: the way its speed eases toward.
    pub heading: Facing,
    /// Frames until a walker's facing follows its heading, or 0.
    pub turn: u8,
    /// Horizontal speed, px per frame, for a walker.
    pub vx: Fixed,
    /// Steps it has moved, which a patrol follows its path by, or since
    /// a thrower appeared.
    pub clock: u32,
    /// Steps a walker that just appeared waits before it moves.
    pub wait: u8,
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
    /// A defeated walker falling out of the level at `vy`, after holding
    /// still for `hold` more frames.
    Falling { vy: Fixed, hold: u8 },
    /// A walker knocked back by a thrown object: moving along the floor
    /// on the step after the hit (`vy` is `None`), then falling out of
    /// the level at `vy`.
    Knocked { vy: Option<Fixed> },
    /// A launcher that fired this many frames ago.
    Fired(u8),
    /// An object in the player's hands, which move it.
    Held,
    /// An object thrown, until it breaks or is removed, or dropped, until
    /// it lands.
    Thrown(Flight),
    /// An object just set down, which comes to rest on the next step.
    Placed,
    /// A ball moving at `vy` down, ignoring floors until it's below the
    /// one it bounced on, if `ignoring` has it. `kept` says it bounced or
    /// stopped ignoring floors on the last step. Its `clock` counts its
    /// steps, but not those.
    Flying {
        vy: Fixed,
        ignoring: Option<Fixed>,
        kept: bool,
    },
    /// A crate holding an animal, broken this many frames ago, counting
    /// up to when the animal stands on the crate's position.
    Opened(u8),
    /// An animal the player mounted this many frames ago.
    Mounted(u8),
    /// An animal the player rides.
    Ridden,
    /// An animal whose rider was knocked off, running off at `vx`.
    Bolting { vx: Fixed },
    /// A stinger swaying around its start, with its vertical speed and
    /// its target speeds (see [`stinger`](crate::stinger)).
    Swaying { vy: Fixed, tx: Fixed, ty: Fixed },
}

/// Everything the simulation advances.
#[derive(Clone, Debug)]
pub struct World {
    pub level: Arc<Level>,
    pub feel: Arc<Feel>,
    /// Steps taken so far.
    pub frame: u32,
    pub player: Player,
    /// The character that isn't under control, when the roster has
    /// another one and characters swap places (`swap_moves` above 0).
    pub other: Option<Other>,
    /// The swap under way, if there is one.
    pub swap: Option<Swap>,
    /// What was held lately, for whether a swap press starts a swap.
    pub recent: Recent,
    /// The loss of a character under way, until protection after it
    /// ends.
    pub loss: Option<Loss>,
    /// A character lost on a hit, until it has left the level.
    pub lost: Option<Lost>,
    /// A missing character joining the player, until it stands there.
    pub join: Option<Join>,
    pub things: Vec<Thing>,
    /// The view, when the preset has one (`view_width` above 0).
    pub view: Option<View>,
    /// What happened during the last step, in order.
    pub events: Vec<Event>,
    /// Goal, lives, checkpoints and pickups.
    pub flow: Flow,
    /// The walk-out after passing the level's end marker, once it has
    /// started.
    pub ending: Option<Ending>,
    /// The dimming of the level's background, once it has started.
    pub dimming: Option<Dimming>,
    /// Frames the whole game should stand still after the last step
    /// before the next, as during a knock-out. Whatever runs the world in
    /// real time holds the screen for them; they aren't steps, and a
    /// trace has no rows for them.
    pub pause: u8,
}

impl World {
    /// The level at its start, with the lead character at the start
    /// position, and the next one there too if characters swap places.
    /// `feel` needs at least one character.
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
        // Both characters start where play starts (following.md).
        let other = (feel.roster.len() > 1 && feel.world[Param::SwapMoves] > Fixed::ZERO)
            .then(|| Other::new(1, player.pos, player.facing, player.pos));
        let archetypes = Archetypes::builtin();
        let physics = feel.roster[0].physics;
        let view = view::enabled(&physics).then(|| View::settled(&player, &level, &physics));
        let mut things: Vec<Thing> = level
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
                    heading: facing,
                    turn: 0,
                    vx,
                    clock: 0,
                    wait: 0,
                    phase: Phase::Ready,
                })
            })
            .collect::<Result<_, _>>()?;
        // With a view, walkers whose start is near it are in the level as
        // placed, and the others appear later.
        if let Some(v) = view {
            let present = view::at_start(&things, &level, v.x(), &physics);
            for (thing, present) in things.iter_mut().zip(present) {
                thing.active &= present;
            }
        }
        Ok(World {
            level,
            feel,
            frame: 0,
            player,
            other,
            swap: None,
            recent: Recent::default(),
            loss: None,
            lost: None,
            join: None,
            things,
            view,
            events: Vec::new(),
            flow: Flow::new(),
            ending: None,
            dimming: None,
            pause: 0,
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

    /// The level `x` of the middle of the view, with its left edge
    /// rounded down to a whole px as it's drawn, if there is a view.
    pub fn view_middle(&self) -> Option<Fixed> {
        let view = self.view?;
        let half = self.feel.world[Param::ViewWidth].floor() / 2;
        Some(Fixed::from_int((view.x() + half) as i32))
    }

    /// The level `y` of the middle of the view, with its top edge
    /// rounded down to a whole px as it's drawn, if there is a view that
    /// moves vertically.
    pub fn view_middle_y(&self) -> Option<Fixed> {
        let view = self.view?;
        let height = self.feel.world[Param::ViewHeight].floor();
        (height > 0).then(|| Fixed::from_int((view.y() + height / 2) as i32))
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

        player_bytes(&mut out, &self.player);

        match &self.other {
            Some(o) => {
                out.push(1);
                count(&mut out, o.character);
                vec2(&mut out, o.pos);
                out.push(o.facing as u8);
                let f = &o.follow;
                count(&mut out, f.trail.len());
                for &(x, y) in &f.trail {
                    out.extend(x.to_le_bytes());
                    out.extend(y.to_le_bytes());
                }
                out.push(f.catching_up.into());
                match f.mode {
                    Mode::Trail => out.push(0),
                    Mode::Rest { target, gained } => {
                        out.push(1);
                        out.extend(target.to_le_bytes());
                        out.push(gained.into());
                    }
                    Mode::Wait => out.push(2),
                }
                out.push(f.turn.map_or(0, |n| n.saturating_add(1)));
            }
            None => out.push(0),
        }
        let moving = |out: &mut Vec<u8>, m: &Move| {
            vec2(out, m.from);
            out.extend(m.across.to_le_bytes());
            for v in [m.target, m.vy, m.gravity, m.max_fall] {
                fixed(out, v);
            }
            out.extend(m.first.to_le_bytes());
            out.extend(m.moves.to_le_bytes());
        };
        match &self.swap {
            Some(s) => {
                out.push(1);
                out.extend(s.frame.to_le_bytes());
                out.extend(s.end.to_le_bytes());
                moving(&mut out, &s.player);
                moving(&mut out, &s.other);
            }
            None => out.push(0),
        }
        let r = &self.recent;
        match r.down {
            Some((since, up)) => {
                out.push(1);
                out.extend(since.to_le_bytes());
                out.extend(up.map_or(0, |n| n.saturating_add(1)).to_le_bytes());
            }
            None => out.push(0),
        }
        match r.jump {
            Some((age, kept)) => {
                out.push(1);
                out.extend(age.to_le_bytes());
                out.push(kept.into());
            }
            None => out.push(0),
        }
        out.push(r.landed.into());
        match self.loss {
            Some(l) => {
                out.push(1);
                out.extend(l.frame.to_le_bytes());
                vec2(&mut out, l.spot);
                out.extend(l.hop.map_or(0, |t| t.saturating_add(1)).to_le_bytes());
            }
            None => out.push(0),
        }
        match &self.lost {
            Some(l) => {
                out.push(1);
                player_bytes(&mut out, &l.body);
                fixed(&mut out, l.gravity);
                out.push(l.landed.map_or(0, |n| n.saturating_add(1)));
                out.push(l.apart.into());
            }
            None => out.push(0),
        }
        match self.join {
            Some(j) => {
                out.push(1);
                out.push(j.frame);
                for v in [j.from.0, j.from.1, j.to.0, j.to.1] {
                    out.extend(v.to_le_bytes());
                }
                fixed(&mut out, j.lift);
            }
            None => out.push(0),
        }

        count(&mut out, self.things.len());
        for t in &self.things {
            count(&mut out, t.index);
            count(&mut out, t.kind.len());
            out.extend(t.kind.as_bytes());
            vec2(&mut out, t.pos);
            out.push(t.active.into());
            out.extend([t.facing, t.heading].map(|f| match f {
                Facing::Left => 0,
                Facing::Right => 1,
            }));
            out.push(t.turn);
            fixed(&mut out, t.vx);
            out.extend(t.clock.to_le_bytes());
            out.push(t.wait);
            out.extend(match t.phase {
                Phase::Ready => [0, 0],
                Phase::Defeated(left) => [1, left],
                Phase::Fired(since) => [2, since],
                Phase::Falling { hold, .. } => [3, hold],
                Phase::Held => [4, 0],
                Phase::Thrown(_) => [5, 0],
                Phase::Placed => [6, 0],
                Phase::Knocked { vy } => [7, vy.is_some().into()],
                Phase::Flying { ignoring, kept, .. } => {
                    [8, u8::from(ignoring.is_some()) | u8::from(kept) << 1]
                }
                Phase::Opened(since) => [9, since],
                Phase::Mounted(since) => [10, since],
                Phase::Ridden => [11, 0],
                Phase::Swaying { .. } => [12, 0],
                Phase::Bolting { .. } => [13, 0],
            });
            if let Phase::Falling { vy, .. }
            | Phase::Knocked { vy: Some(vy) }
            | Phase::Flying { vy, .. }
            | Phase::Bolting { vx: vy } = t.phase
            {
                fixed(&mut out, vy);
            }
            if let Phase::Swaying { vy, tx, ty } = t.phase {
                for v in [vy, tx, ty] {
                    fixed(&mut out, v);
                }
            }
            if let Phase::Flying {
                ignoring: Some(floor),
                ..
            } = t.phase
            {
                fixed(&mut out, floor);
            }
            if let Phase::Thrown(f) = t.phase {
                for v in [f.vx, f.vy, f.target, f.response, f.gravity, f.max_fall] {
                    fixed(&mut out, v);
                }
                for v in [f.margin, f.bounce_min, f.bounce] {
                    fixed(&mut out, v);
                }
                out.extend([f.thrown.into(), f.rolling.into(), f.age]);
            }
        }

        match self.view {
            Some(v) => {
                out.push(1);
                fixed(&mut out, v.left);
                fixed(&mut out, v.speed);
                out.push(v.hold);
                fixed(&mut out, v.top);
                fixed(&mut out, v.gap);
                out.push(v.shake.map_or(0, |n| n.saturating_add(1)));
            }
            None => out.push(0),
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
        match self.ending {
            Some(e) => {
                out.push(1);
                out.extend(e.frame.to_le_bytes());
                out.push(e.fade.map_or(0, |n| n.saturating_add(1)));
                out.push(e.due.into());
            }
            None => out.push(0),
        }
        match self.dimming {
            Some(d) => {
                out.push(1);
                out.extend(d.started.to_le_bytes());
            }
            None => out.push(0),
        }
        out.push(self.pause);
        out.extend(f.gems.to_le_bytes());
        out.extend(f.relics.to_le_bytes());
        count(&mut out, f.checkpoints.len());
        for &i in &f.checkpoints {
            count(&mut out, i);
        }
        count(&mut out, f.due.len());
        for &d in &f.due {
            out.extend(d.to_le_bytes());
        }
        count(&mut out, f.releases.len());
        for r in &f.releases {
            out.extend([r.left, r.wait, r.every]);
            count(&mut out, r.thing);
        }
        out.push(f.letters);
        count(&mut out, f.joining.len());
        for &(at, k) in &f.joining {
            out.extend(at.to_le_bytes());
            out.push(k);
        }
        match f.letter_taken {
            Some(at) => {
                out.push(1);
                out.extend(at.to_le_bytes());
            }
            None => out.push(0),
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
                FlowEvent::Life => (8, 0),
                FlowEvent::Letter(k) => (9, usize::from(k)),
            };
            out.push(tag);
            count(&mut out, index);
        }
        out
    }
}

/// Appends a character's state to `out`, for [`World::state_bytes`].
fn player_bytes(out: &mut Vec<u8>, p: &Player) {
    let fixed = |out: &mut Vec<u8>, v: Fixed| out.extend(v.to_bits().to_le_bytes());
    let vec2 = |out: &mut Vec<u8>, v: Vec2| {
        fixed(out, v.x);
        fixed(out, v.y);
    };
    let count = |out: &mut Vec<u8>, n: usize| out.extend((n as u64).to_le_bytes());
    count(out, p.character);
    vec2(out, p.pos);
    vec2(out, p.vel);
    fixed(out, p.target);
    match p.ground {
        Some(collider) => {
            out.push(1);
            count(out, collider);
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
    out.push(p.takeoff_wait.map_or(0, |n| n.saturating_add(1)));
    out.extend([p.takeoff_crouched, p.takeoff_rolled].map(u8::from));
    out.push(p.swap);
    out.extend([p.crouch.held, p.crouch.rising, p.crouch.turn_lock]);
    out.push(p.turn.map_or(0, |n| n.saturating_add(1)));
    out.push(p.short_turn.into());
    let roll = |out: &mut Vec<u8>, roll: Option<Roll>| match roll {
        Some(roll) => {
            out.extend([
                1,
                roll.left,
                roll.dir as u8,
                roll.size as u8,
                roll.boosts,
                roll.defeated.into(),
                roll.held.into(),
                roll.carry.into(),
                roll.still.into(),
                roll.since_press,
                roll.air,
            ]);
            fixed(out, roll.speed);
        }
        None => out.push(0),
    };
    roll(out, p.roll);
    out.push(p.roll_stop);
    roll(out, p.stopped);
    out.push(p.air_stopped.into());
    match p.ground_action {
        Some(a) => out.extend([1, a.frame, a.dir as u8]),
        None => out.push(0),
    }
    match p.boost {
        Some(b) => out.extend([1, b.dir as u8, b.size as u8, b.boosts]),
        None => out.push(0),
    }
    match p.roll_press {
        Some((age, window, hold)) => out.extend([1, age, window, hold.into()]),
        None => out.push(0),
    }
    out.push(p.since_run.map_or(0, |n| n.saturating_add(1)));
    out.extend([p.lone_run, p.roll_jump].map(u8::from));
    let h = p.holds;
    out.extend([h.held, h.last, h.gap, h.before_press.0, h.before_press.1]);
    match p.off_face {
        Some((way, n)) => out.extend([1, way as u8, n]),
        None => out.push(0),
    }
    out.push(p.after_face.map_or(0, |way| way as u8 + 1));
    out.push(p.jump_age.map_or(0, |n| n.saturating_add(1)));
    out.extend([
        p.walked_off.into(),
        p.fall_step.into(),
        p.fall_facing,
        p.ran.into(),
    ]);
    out.push(p.since_land.map_or(0, |n| n.saturating_add(1)));
    match p.hurt_ahead {
        Some(i) => {
            out.push(1);
            count(out, i);
        }
        None => out.push(0),
    }
    let pose = p.pose;
    out.push(pose.air.index() as u8);
    out.push(pose.since_land.map_or(0, |n| n.saturating_add(1)));
    match pose.rolled {
        Some((n, on)) => out.extend([1, n, on.into()]),
        None => out.push(0),
    }
    out.extend(
        pose.standing
            .map_or(0, |n| n.saturating_add(1))
            .to_le_bytes(),
    );
    out.push(pose.rolling.into());
    match p.knockout {
        Some(k) => {
            out.push(1);
            match k.by {
                Some(i) => {
                    out.push(1);
                    count(out, i);
                }
                None => out.push(0),
            }
            out.extend(k.frame.to_le_bytes());
            out.push(k.landings);
            out.push(k.rest.map_or(0, |n| n.saturating_add(1)));
            out.extend(k.hop.map_or(0, |n| n.saturating_add(1)).to_le_bytes());
        }
        None => out.push(0),
    }
    out.push(p.hop.map_or(0, |n| n.saturating_add(1)));
    out.push(p.unseated.into());
    match p.hands {
        Some(h) => {
            out.push(1);
            count(out, h.thing);
            out.push(match h.act {
                Act::PickUp => 0,
                Act::Carry => 1,
                Act::Throw => 2,
                Act::PutDown => 3,
            });
            out.push(h.frame);
            out.push(h.press.map_or(0, |n| n.saturating_add(1)));
        }
        None => out.push(0),
    }
    match p.pick {
        Some(i) => {
            out.push(1);
            count(out, i);
        }
        None => out.push(0),
    }
}

/// Advances the world one logic frame (1/60 s). Once the level is
/// complete or the game is over, nothing changes.
pub fn step(world: &mut World, input: &InputFrame) {
    world.events.clear();
    world.flow.events.clear();
    world.pause = 0;
    if world.flow.outcome.is_over() {
        return;
    }
    // The dimming starts by where the view was at the start of the step.
    dimming::start(world);
    if world.join.is_some() {
        // Everything else waits while a character joins.
        join::step(world);
        world.frame += 1;
        return;
    }
    let start_view = world.view;
    if flow::dying(world) {
        if world.flow.events.contains(&FlowEvent::Respawn) {
            view::restart(world);
            world.dimming = None;
        } else if follow::due(world).is_some() {
            // The other character goes on following a player that fell
            // out of the level.
            follow::update(world, Lead::of(&world.player));
            follow::note(world);
        }
        collect::update(world, false, start_view);
        world.frame += 1;
        return;
    }
    if world.ending.is_some() {
        ending::step(world);
        world.frame += 1;
        return;
    }
    loss::tick(world);
    // Walkers appear and are removed by where the view was at the start
    // of the step.
    let view_left = start_view.map(|v| v.x());
    if let Some(left) = view_left {
        view::walkers(world, left, true);
    }
    let physics = world.player_physics();
    let previous_feet = world.player.pos.y;
    let frozen = world.player.frozen > 0;
    // Nothing follows on the frames the player is held still.
    if !frozen && follow::due(world) == Some(true) {
        follow::update(world, Lead::of(&world.player));
    }
    let facing = world.player.facing;
    if world.player.knockout.is_some() {
        world
            .player
            .knocked_out(&world.level, &physics, &mut world.events);
        things::update(world, previous_feet, frozen);
    } else {
        // What hurting a swapping player does isn't in the spec yet;
        // nothing does.
        let hurt_by = world
            .player
            .hurt_ahead
            .take()
            .filter(|_| world.swap.is_none() && !world.protected() && !world.losing());
        if hurt_by.is_some() {
            world.player.hit(&mut world.events);
        } else if loss::update(world) {
            // The loss moves the characters.
        } else if ride::unseat(world) {
            // Knocked off the animal, the player doesn't move.
        } else if !swap::update(world, &physics, &input.players[0]) {
            carry::before(world, &input.players[0]);
            // After a swap, the physics are the new player's.
            let physics = world.player_physics();
            world.player.update(
                &world.level,
                &world.feel,
                &physics,
                &input.players[0],
                &mut world.events,
            );
        }
        let hurt_now = things::update(world, previous_feet, frozen);
        flow::update(world, hurt_by.or(hurt_now), facing);
        ending::pass(world, &input.players[0]);
    }
    if !frozen && follow::due(world) == Some(false) {
        follow::update(world, Lead::of(&world.player));
    }
    if !frozen && follow::due(world).is_some() {
        follow::note(world);
    }
    if let Some(left) = view_left {
        view::walkers(world, left, false);
    }
    view::update(world);
    carry::after(world, &input.players[0]);
    collect::update(world, true, start_view);
    let landed = world.events.contains(&Event::Land);
    world.recent.note(&input.players[0], landed);
    if let Some(knockout) = world.player.knockout
        && u32::from(knockout.frame) + 1 == u32::from(frames(physics[Param::KnockoutPauseAfter]))
    {
        world.pause = frames(physics[Param::KnockoutPause]);
    }
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
