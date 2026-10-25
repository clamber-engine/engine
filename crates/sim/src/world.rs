//! The world and its fixed step.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_fixed::Fixed;
use clamber_level::{Arrival, Entity, Level, Vec2};
use clamber_pack::{CharacterFeel, Feel, Param, Physics, Presets};

use crate::archetype::{
    Archetype, ArchetypeError, Archetypes, Behavior, KEG_PIECES, Rope, Sling, Spring,
};
use crate::barrel_ride;
use crate::carry::{self, Flight};
use crate::collect;
use crate::dimming::{self, Dimming};
use crate::drop_in;
use crate::ending::{self, Ending};
use crate::entrance::{self, Entering};
use crate::exit::{self, Leaving};
use crate::flow::{self, Flow, FlowEvent, Outcome};
use crate::follow::{self, Lead, Mode};
use crate::hits;
use crate::hutch;
use crate::join::{self, Join};
use crate::keg::{self, Burst, Resume};
use crate::loss::{self, Loss, Lost};
use crate::player::{Act, Facing, Player, ROLL, Roll, frames};
use crate::ride;
use crate::rope;
use crate::scene;
use crate::script::{InputFrame, Script};
use crate::sections::{Placed, Placing};
use crate::sling;
use crate::swap::{self, Move, Other, Recent, Swap};
use crate::things;
use crate::view::{self, View};
use crate::walk_in::{self, WalkingIn};

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
    /// Caught a rope from a jump.
    RopeCatch,
    /// Let go of a rope past its end.
    RopeSlideOff,
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
            Event::RopeCatch => "rope_catch",
            Event::RopeSlideOff => "rope_slide_off",
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
    /// A walker's speed down while it's in the air off a ledge or above
    /// its start's floor (with a [`WalkerDrop`](crate::WalkerDrop)), or
    /// `None` on the floor.
    pub air: Option<Fixed>,
    pub phase: Phase,
    /// It held still on the last step: nothing moved it or counted it
    /// on, as on a bounce's frozen frames or while the player is knocked
    /// out. Not part of the trace.
    pub still: bool,
    /// A walker pushed by a player it threw back: its speed eases to
    /// nothing instead of toward its heading's, until it is slow enough.
    pub pushed: bool,
}

impl Thing {
    /// Entity `index`, `entity`, as it is at the level's start.
    pub(crate) fn placed(index: usize, entity: &Entity, archetype: Option<Archetype>) -> Thing {
        // A walker starts out at full speed.
        let (facing, vx) = match archetype.map(|a| a.behavior) {
            Some(Behavior::Walker { facing, speed, .. }) => match facing {
                Facing::Left => (facing, -speed),
                Facing::Right => (facing, speed),
            },
            _ => (Facing::Right, Fixed::ZERO),
        };
        Thing {
            index,
            kind: entity.kind.clone(),
            pos: entity.at,
            active: true,
            archetype,
            facing,
            heading: facing,
            turn: 0,
            vx,
            clock: 0,
            wait: 0,
            air: None,
            phase: Phase::Ready,
            still: false,
            pushed: false,
        }
    }
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
    /// still for `hold` more frames. With a `glide`, its speed across
    /// eases toward that as it falls; otherwise it keeps it.
    Falling {
        vy: Fixed,
        hold: u8,
        glide: Option<Fixed>,
    },
    /// A walker knocked back by a thrown object: moving along the floor
    /// on the step after the hit (`vy` is `None`), then falling out of
    /// the level at `vy`.
    Knocked { vy: Option<Fixed> },
    /// A launcher that fired this many frames ago.
    Fired(u8),
    /// A sling that caught the player this many frames ago (see
    /// [`sling`](crate::sling)).
    Caught(u8),
    /// An object in the player's hands, which move it.
    Held,
    /// An object thrown, until it breaks or is removed, or dropped, until
    /// it lands.
    Thrown(Flight),
    /// An object just set down, which comes to rest on the next step.
    Placed,
    /// An object let go at a knock-out, falling away at `vy` down and
    /// its `vx` across after `wait` more steps, touching nothing until
    /// it's removed (see [`carry`](crate::carry)).
    LetGo { vy: Fixed, wait: u8 },
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
    /// An animal its rider got off, where it was left: falling for this
    /// many frames, or standing there until it's got on again.
    Waiting(Option<u8>),
    /// A stinger swaying around its start, with its vertical speed and
    /// its target speeds (see [`stinger`](crate::stinger)).
    Swaying { vy: Fixed, tx: Fixed, ty: Fixed },
    /// Gone for the rest of the visit: a keg the player broke (see
    /// [`keg`](crate::keg)), or an extra-life item removed without being
    /// taken. It doesn't appear with the view again until play comes
    /// back to the level (see [`World::coming_back_from`]); an item
    /// also after a lost life, with `resume_afresh`.
    Broken,
    /// A hidden object's item in the air, moving at `vy` down and its
    /// `vx` across, after `wait` more steps (see
    /// [`hidden`](crate::hidden)).
    Item { vy: Fixed, wait: u8 },
    /// A hidden object's item on the floor, this many steps after landing.
    Landed(u8),
    /// A spring squashed by a catch this many steps ago, in the air or
    /// not (see [`spring`](crate::spring)).
    Squashed { since: u8, from_air: bool },
    /// A spring wobbling, `m` steps into the wobble, after a throw this
    /// many steps ago if it threw.
    Wobbling { m: u8, thrown: Option<u8> },
    /// A rope that has started swaying, toward `target`. It had the
    /// player in `reach` at the end of the last step (see
    /// [`rope`](crate::rope)).
    Rope { target: Fixed, reach: bool },
    /// A critter flying at `uy` down at flight height `z`, towards
    /// `target` (whole px; see [`critter`](crate::critter)).
    Flitting {
        uy: Fixed,
        z: Fixed,
        target: (i32, i32),
    },
    /// Gone for good: it doesn't appear with the view again.
    Gone,
    /// A wall exit a thrown object opened (see [`exit`](crate::exit)).
    Open,
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
    /// An enemy touching the character that takes over during a swap,
    /// which hurts it on the frame after control returns.
    pub swap_touch: Option<usize>,
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
    /// The thing whose touch hurt the player during the last step, if
    /// that's what hurt it.
    pub hit_by: Option<usize>,
    /// Goal, lives, checkpoints and pickups.
    pub flow: Flow,
    /// The walk-out after passing the level's end marker, once it has
    /// started.
    pub ending: Option<Ending>,
    /// The walk-out after an exit took over the lead, once it has
    /// started.
    pub leaving: Option<Leaving>,
    /// The dimming of the level's background, once it has started.
    pub dimming: Option<Dimming>,
    /// Things appearing by sections, if the level's do and there's a
    /// view.
    pub placing: Option<Placing>,
    /// Play resuming inside a keg, until the player is in play and the
    /// picture has faded in.
    pub resume: Option<Resume>,
    /// The pieces and flashes of kegs that broke or burst, and of the
    /// entrance.
    pub bursts: Vec<Burst>,
    /// The characters entering the level, if it has an entrance, until
    /// they're in play and the picture has faded in.
    pub entering: Option<Entering>,
    /// The characters walking into the area, when it began at an arrival
    /// they walk in by, until the picture has faded in.
    pub walking_in: Option<WalkingIn>,
    /// The lead dropped into the area, at an arrival it drops in by,
    /// until the picture has faded in.
    pub dropping_in: bool,
    /// The things out of the area on its frame 0 as the characters walk
    /// in, there again from frame 1.
    pub waiting: Vec<usize>,
    /// Frames the whole game should stand still after the last step
    /// before the next, as during a knock-out. Whatever runs the world in
    /// real time holds the screen for them; they aren't steps, and a
    /// trace has no rows for them.
    pub pause: u8,
    /// Walkers held still on the last step, as on a bounce's frozen
    /// frames, a defeat's pause or while the player is knocked out, or
    /// on a step on which nothing moved: the frames walker.md's
    /// "Counted frames" leaves out. Not part of the trace.
    pub still: bool,
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
        let start = level.start;
        World::begin(level, feel, start, None, None, false)
    }

    /// The level as play comes into it at `arrival`: the characters walk
    /// in to its point or the lead drops in there if it says so, and the
    /// level begins there as at its start otherwise.
    ///
    /// # Panics
    ///
    /// As [`World::new`].
    pub fn arriving(level: Arc<Level>, feel: Arc<Feel>, arrival: &Arrival) -> World {
        World::try_arriving(level, feel, arrival).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The level as play comes into it at `arrival`, or why an entity's
    /// params don't fit its archetype.
    pub fn try_arriving(
        level: Arc<Level>,
        feel: Arc<Feel>,
        arrival: &Arrival,
    ) -> Result<World, ArchetypeError> {
        assert!(!feel.roster.is_empty(), "a world needs a character");
        let walk = WalkingIn::of(arrival, &feel.world);
        let inside = arrival.entrance.as_deref();
        World::begin(level, feel, arrival.at, walk, inside, arrival.drop)
    }

    /// The level beginning with the lead at `start`, or walking in to it,
    /// or inside the entrance with the id `inside`, or dropping in at it.
    fn begin(
        level: Arc<Level>,
        feel: Arc<Feel>,
        start: Vec2,
        walk: Option<WalkingIn>,
        inside: Option<&str>,
        drop: bool,
    ) -> Result<World, ArchetypeError> {
        assert!(!feel.roster.is_empty(), "a world needs a character");
        let mut player = Player::new(&level, start);
        if let Some(walk) = walk {
            player.pos = walk.lead_at(0, &feel.world);
            player.facing = walk.facing;
        }
        if drop {
            player.facing = Facing::Right;
        }
        // Both characters start where play starts (following.md).
        let mut other = (feel.roster.len() > 1 && feel.world[Param::SwapMoves] > Fixed::ZERO)
            .then(|| Other::new(1, player.pos, player.facing, player.pos));
        let archetypes = Archetypes::builtin();
        let physics = feel.roster[0].physics;
        let mut things: Vec<Thing> = level
            .entities
            .iter()
            .enumerate()
            .map(|(index, e)| {
                let path = format!("entities[{index}]");
                let archetype = archetypes.resolve(e, &level.name, &path)?;
                let mut thing = Thing::placed(index, e, archetype);
                things::drop_from_start(&level, &mut thing);
                Ok(thing)
            })
            .collect::<Result<_, _>>()?;
        // A level with an object holding the missing character begins
        // without it; breaking the object frees it (holder-breaking.md).
        if things.iter().any(|t| {
            t.archetype
                .as_ref()
                .and_then(crate::carry::holder)
                .is_some()
        }) {
            other = None;
        }
        // A level with an entrance begins as an area does (view.md, "When
        // an area begins"), and so does one walked into; the view starts
        // settled on the lead otherwise. Walking in, the entrance is gone.
        // Dropping in, no entrance acts.
        let mut entering = (!drop)
            .then(|| entrance::begin(&level, &things, inside))
            .flatten();
        if walk.is_some()
            && let Some(e) = entering.take()
        {
            things[e.entrance].active = false;
            things[e.entrance].phase = Phase::Broken;
        }
        // An entrance that doesn't act isn't drawn, unless it says so
        // (level-start.md, "Coming back from the room").
        let acting = entering.map(|e| e.entrance);
        for thing in &mut things {
            if let Some(Behavior::Entrance(v)) = thing.archetype.map(|a| a.behavior)
                && Some(thing.index) != acting
                && thing.phase == Phase::Ready
                && !v.drawn_idle
            {
                thing.active = false;
            }
        }
        let view = view::enabled(&physics).then(|| {
            if entering.is_some() || walk.is_some() || drop {
                View::begun(&player, &level, &physics)
            } else {
                View::settled(&player, &level, &physics)
            }
        });
        // With a view, walkers whose start is near it are in the level as
        // placed, and the others appear later. Things in sections appear
        // from the first step.
        // Without things appearing by the view, they're all there.
        let by_view = view.filter(|_| view::by_view(&physics));
        let placing = by_view.and_then(|v| {
            let placed = Placed::of(&level)?;
            Some(Placing::begin(Arc::new(placed), v.y()))
        });
        if let Some(v) = by_view {
            let (left, physics) = view::window(&level, &feel, v.x(), &physics);
            let present = view::at_start(&things, &level, left, &physics);
            for (thing, present) in things.iter_mut().zip(present) {
                thing.active &= present;
            }
        }
        if let Some(placing) = &placing {
            for (thing, spot) in things.iter_mut().zip(&placing.placed.spot_of) {
                thing.active &= spot.is_none();
            }
        }
        Ok(World {
            level,
            feel,
            frame: 0,
            player,
            other,
            swap: None,
            swap_touch: None,
            recent: Recent::default(),
            loss: None,
            lost: None,
            join: None,
            things,
            view,
            events: Vec::new(),
            hit_by: None,
            flow: Flow::new(),
            ending: None,
            leaving: None,
            dimming: None,
            placing,
            resume: None,
            bursts: Vec::new(),
            entering,
            walking_in: walk,
            dropping_in: drop,
            waiting: Vec::new(),
            pause: 0,
            still: false,
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
        if self.feel.immediate {
            // A press is answered on its own frame (#437).
            physics.set(Param::JumpDelay, Fixed::ZERO);
            physics.set(Param::CarryJumpDelay, Fixed::ZERO);
            physics.set(Param::TakeoffWait, Fixed::ZERO);
        }
        physics
    }

    /// The player ends the last step on the floor of a steep face's
    /// column (`face_slope`), and isn't rolling.
    pub fn player_on_face(&self) -> bool {
        let player = &self.player;
        let slope = self.player_physics()[Param::FaceSlope];
        slope > Fixed::ZERO
            && player.ground.is_some()
            && player.roll.is_none()
            && crate::columns::face(
                &self.level,
                crate::columns::column(player.pos.x),
                player.pos.y,
                slope,
            )
            .is_some()
    }

    /// The player stands on flat floor that stops within `ahead` columns
    /// at the end it faces, with a drop or a pit beyond and no wall
    /// (`player-frames.md`, "Teetering at the end of a floor": 12 for the
    /// outermost 12 columns), its position rounded down to a pixel
    /// column. With `through_faces` (the first character), steep faces
    /// on the way are part of the floor and the count runs to the last
    /// column of the face, if the floor ends after it.
    pub fn player_at_floor_end(&self, ahead: i64, through_faces: bool) -> bool {
        use crate::columns;
        let player = &self.player;
        if player.ground.is_none() {
            return false;
        }
        let dir = match player.facing {
            Facing::Left => -1,
            Facing::Right => 1,
        };
        let slope = self.player_physics()[Param::FaceSlope];
        let (here, feet) = (columns::column(player.pos.x), player.pos.y);
        let mut in_face = false;
        for c in (1..=ahead).map(|k| here + dir * k) {
            let on_face = |top| columns::face(&self.level, c, top, slope).is_some();
            match columns::floor(&self.level, c, feet) {
                Some(f) if f.y == feet && !in_face => {}
                Some(f) if through_faces && on_face(f.y) => in_face = true,
                // A step, a wall or a slope.
                Some(_) => return false,
                None => {
                    let below = columns::face_below(&self.level, c, feet, slope);
                    if through_faces && in_face && below.is_some() {
                        continue;
                    }
                    return below.is_none();
                }
            }
        }
        false
    }

    /// The things that aren't in the level but would appear at their
    /// start if the view reached them, as they'd be on appearing, at
    /// their start: those near a view `width` px wide with its left edge
    /// at level x `left`, by the same rules as the world's own view
    /// (see [`View`]), with the view's top where it is now. Nothing
    /// changes. Without a view, or when things don't appear by it
    /// (`appear_by_view`), there are none.
    ///
    /// This is for showing more than the view does without things popping
    /// in: they can be drawn where they'll appear before they do.
    pub fn would_appear(&self, left: i64, width: i64) -> Vec<Thing> {
        match self.view.filter(|_| view::by_view(&self.player_physics())) {
            Some(view) => view::would_appear(self, (left, view.y()), width),
            None => Vec::new(),
        }
    }

    /// The level `x` of the middle of the view, with its left edge
    /// rounded down to a whole px as it's drawn, if there is a view.
    pub fn view_middle(&self) -> Option<Fixed> {
        let view = self.view?;
        let half = self.feel.world[Param::ViewWidth].floor() / 2;
        Some(Fixed::from_int((view.x() + half) as i32))
    }

    /// Where the window things come and go by is, as its left edge and
    /// width in whole px, if there's a view and the feel has a
    /// [`window`](Feel::window) wider than it (wide activation). A game
    /// that shows more than the view draws this much of the level, here,
    /// so nothing pops in.
    pub fn window(&self) -> Option<(i64, i64)> {
        let physics = self.feel.world;
        let view = self.view.filter(|_| view::by_view(&physics))?;
        let picture = physics[Param::ViewWidth].floor();
        let (left, physics) = view::window(&self.level, &self.feel, view.x(), &physics);
        let width = physics[Param::ViewWidth].floor();
        (width > picture).then_some((left, width))
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
        out.push(r.kept.into());
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
                Phase::Broken => [14, 0],
                Phase::Item { wait, .. } => [15, wait],
                Phase::Landed(since) => [16, since],
                Phase::Squashed { since, from_air } => [17 + u8::from(from_air), since],
                Phase::Wobbling { m, .. } => [19, m],
                Phase::Rope { reach, .. } => [20, reach.into()],
                Phase::Gone => [21, 0],
                Phase::Open => [22, 0],
                Phase::Caught(since) => [23, since],
                Phase::LetGo { wait, .. } => [24, wait],
                Phase::Flitting { .. } => [25, 0],
                Phase::Waiting(falling) => [26, falling.map_or(0, |n| n.saturating_add(1))],
            });
            if let Phase::Wobbling { thrown, .. } = t.phase {
                out.push(thrown.map_or(0, |n| n.saturating_add(1)));
            }
            if let Phase::Falling { vy, .. }
            | Phase::Knocked { vy: Some(vy) }
            | Phase::Flying { vy, .. }
            | Phase::Bolting { vx: vy }
            | Phase::Item { vy, .. }
            | Phase::LetGo { vy, .. } = t.phase
            {
                fixed(&mut out, vy);
            }
            if let Phase::Rope { target, .. } = t.phase {
                fixed(&mut out, target);
            }
            if let Phase::Falling {
                glide: Some(glide), ..
            } = t.phase
            {
                fixed(&mut out, glide);
            }
            if let Phase::Flitting { uy, z, target } = t.phase {
                fixed(&mut out, uy);
                fixed(&mut out, z);
                out.extend(target.0.to_le_bytes());
                out.extend(target.1.to_le_bytes());
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

        count(&mut out, self.waiting.len());
        for &i in &self.waiting {
            count(&mut out, i);
        }

        let f = &self.flow;
        match f.outcome {
            Outcome::Playing => out.push(0),
            Outcome::Dying(left) => out.extend([1, left]),
            Outcome::Complete => out.push(2),
            Outcome::GameOver => out.push(3),
            Outcome::Exited(i) => {
                out.push(4);
                count(&mut out, i);
            }
            Outcome::Ended => out.push(5),
        }
        out.push(f.lives);
        out.push(f.hits_taken);
        out.extend(f.recovery.map_or(0, |n| n.saturating_add(1)).to_le_bytes());
        match self.ending {
            Some(e) => {
                out.push(1);
                count(&mut out, e.marker);
                out.push(e.onward.into());
                out.extend(e.frame.to_le_bytes());
                out.push(e.fade.map_or(0, |n| n.saturating_add(1)));
                out.push(e.due.into());
            }
            None => out.push(0),
        }
        match self.leaving {
            Some(l) => {
                out.push(1);
                count(&mut out, l.exit);
                out.push(u8::from(l.way == Facing::Right));
                out.extend(l.frame.to_le_bytes());
                out.push(l.fade.map_or(0, |n| n.saturating_add(1)));
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
        match &self.placing {
            Some(p) => {
                out.push(1);
                count(&mut out, p.opening.home);
                count(&mut out, p.opening.crossing.map_or(0, |j| j + 1));
                out.extend(p.there.iter().map(|&t| u8::from(t)));
            }
            None => out.push(0),
        }
        match self.resume {
            Some(r) => {
                out.push(1);
                count(&mut out, r.keg);
                out.extend(r.frame.to_le_bytes());
                vec2(&mut out, r.at);
                out.push(r.landed.into());
            }
            None => out.push(0),
        }
        count(&mut out, self.bursts.len());
        for b in &self.bursts {
            count(&mut out, b.thing);
            out.push(b.frame);
            for v in b.pieces.iter().chain(&b.speeds) {
                vec2(&mut out, *v);
            }
            vec2(&mut out, b.flash);
            out.push(b.object.map_or(0, |right| 1 + u8::from(right)));
        }
        match self.entering {
            Some(e) => {
                out.push(1);
                count(&mut out, e.entrance);
                out.push(e.landed.into());
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
        count(&mut out, f.copies.len());
        for c in &f.copies {
            out.extend(c.at.to_le_bytes());
            out.extend([c.from]);
            out.extend(c.len.to_le_bytes());
            out.extend(c.start.0.to_le_bytes());
            out.extend(c.start.1.to_le_bytes());
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
        for shown in [f.letter_shown, f.letter_before] {
            match shown {
                Some(s) => {
                    out.push(1);
                    out.extend(s.from.to_le_bytes());
                    out.extend(s.last.to_le_bytes());
                }
                None => out.push(0),
            }
        }
        out.extend(f.shown.to_le_bytes());
        match f.last_step {
            Some(at) => {
                out.push(1);
                out.extend(at.to_le_bytes());
            }
            None => out.push(0),
        }
        count(&mut out, f.pickups.len());
        for p in &f.pickups {
            out.extend(p.start.to_le_bytes());
            out.push(p.owed);
            count(&mut out, p.thing);
        }
        match f.letter_end {
            Some(e) => {
                out.push(1);
                out.extend(e.at.to_le_bytes());
                out.extend([e.late, e.before, e.after]);
            }
            None => out.push(0),
        }
        count(&mut out, f.gains.len());
        for g in &f.gains {
            for n in [g.at, g.held.0, g.held.1, g.counts] {
                out.extend(n.to_le_bytes());
            }
            match g.step {
                Some(at) => {
                    out.push(1);
                    out.extend(at.to_le_bytes());
                }
                None => out.push(0),
            }
            out.extend([g.before, g.after]);
        }
        match f.relic_flight {
            Some(r) => {
                out.push(1);
                out.extend(r.taken.to_le_bytes());
                out.extend(r.at.0.to_le_bytes());
                out.extend(r.at.1.to_le_bytes());
                out.extend(r.arrived.map_or(0, |a| a + 1).to_le_bytes());
                count(&mut out, r.thing);
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
                FlowEvent::Exited(i) => (10, i),
                FlowEvent::Ended => (11, 0),
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
    out.extend([p.face_press, p.clear_start].map(u8::from));
    out.extend([p.carried_press, p.uphill_onto_face].map(u8::from));
    out.push(p.takeoff_wait.map_or(0, |n| n.saturating_add(1)));
    out.extend(
        [
            p.takeoff_crouched,
            p.takeoff_rolled,
            p.action_jump,
            p.action_fall,
        ]
        .map(u8::from),
    );
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
                roll.stopped.into(),
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
        Some(a) => {
            out.extend([1, a.frame, a.dir as u8, a.air.into()]);
            match a.speed {
                Some(v) => {
                    out.push(1);
                    fixed(out, v);
                }
                None => out.push(0),
            }
        }
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
    match p.face_target {
        Some(v) => {
            out.push(1);
            fixed(out, v);
        }
        None => out.push(0),
    }
    out.push(p.after_face.map_or(0, |way| way as u8 + 1));
    out.push(p.jump_age.map_or(0, |n| n.saturating_add(1)));
    out.push(p.approach.map_or(0, |n| n.saturating_add(1)));
    out.push(p.rolled_jump.into());
    out.extend([
        p.walked_off.into(),
        p.keg_fall.into(),
        p.fall_step.into(),
        p.fall_facing,
        p.ran.into(),
    ]);
    out.push(p.since_land.map_or(0, |n| n.saturating_add(1)));
    match p.since_bounce {
        Some((n, held)) => out.extend([1, n, held]),
        None => out.push(0),
    }
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
    out.push(p.hopped_off.map_or(0, |n| n.saturating_add(1)));
    out.push(p.jumping_off.into());
    match p.fall_top {
        Some(feet) => {
            out.push(1);
            out.extend(feet.to_le_bytes());
        }
        None => out.push(0),
    }
    match p.spring {
        Some(on) => {
            out.extend([1, on.since, on.from_air.into(), on.top.into()]);
            out.push(on.thrown.map_or(0, |n| n.saturating_add(1)));
            spring_bytes(out, &on.values);
        }
        None => out.push(0),
    }
    match p.spring_arc {
        Some(arc) => {
            out.extend([1, arc.held.into()]);
            fixed(out, arc.jump_gravity);
            fixed(out, arc.gravity);
            fixed(out, arc.max_fall);
        }
        None => out.push(0),
    }
    out.push(p.jump_held);
    match p.rope {
        Some(on) => {
            out.push(1);
            out.extend((on.thing as u64).to_le_bytes());
            out.push(on.turn.map_or(0, |n| n.saturating_add(1)));
            rope_bytes(out, &on.values);
        }
        None => out.push(0),
    }
    match p.astride {
        Some(a) => {
            out.push(1);
            count(out, a.thing);
            out.push(a.age);
            fixed(out, a.gap);
        }
        None => out.push(0),
    }
    out.extend(p.rope_last.map_or(u64::MAX, |n| n as u64).to_le_bytes());
    out.extend([
        p.rope_dir.map_or(0, |f| 1 + u8::from(f == Facing::Right)),
        p.rope_run.into(),
        p.rope_fall.into(),
    ]);
    match p.sling {
        Some(s) => {
            out.push(1);
            count(out, s.thing);
            out.extend(s.at.0.to_le_bytes());
            out.extend(s.at.1.to_le_bytes());
            out.extend(s.since.to_le_bytes());
            sling_bytes(out, &s.values);
        }
        None => out.push(0),
    }
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
    out.push(p.thrown.map_or(0, |n| n.saturating_add(1)));
    out.push(p.carried_jump.into());
    match p.pick {
        Some(i) => {
            out.push(1);
            count(out, i);
        }
        None => out.push(0),
    }
    out.extend([p.swim_floor, p.swim_fast, p.swim_deep].map(u8::from));
    match p.swim_turn {
        Some(t) => out.extend([
            1,
            t.age,
            t.stroke.map_or(0, |j| j.saturating_add(1)),
            t.turned.into(),
        ]),
        None => out.push(0),
    }
}

/// Appends a spring's values to `out`, for [`World::state_bytes`].
fn spring_bytes(out: &mut Vec<u8>, s: &Spring) {
    let fixed = |out: &mut Vec<u8>, v: Fixed| out.extend(v.to_bits().to_le_bytes());
    let mut px = |n: i32| out.extend(n.to_le_bytes());
    for (from, to) in [s.across[0], s.across[1], s.reach, s.throw_up] {
        px(from);
        px(to);
    }
    for n in [s.top, s.stand, s.catch_below] {
        px(n);
    }
    for (top, frames) in s.squash.iter().chain(&s.wobble) {
        px(*top);
        px(i32::from(*frames));
    }
    out.extend([
        s.squash_runs,
        s.wobble_runs,
        s.throw_rest,
        s.jump_wait,
        s.jump_early,
    ]);
    for v in [
        s.throw_speed,
        s.drop,
        s.leave_drop,
        s.jump_speed,
        s.jump_gravity,
        s.gravity,
        s.max_fall,
    ] {
        fixed(out, v);
    }
}

/// Appends the values of a sling that has caught the player to `out`,
/// for [`World::state_bytes`]: those that matter after the catch.
fn sling_bytes(out: &mut Vec<u8>, s: &Sling) {
    out.extend([s.hold, s.glide, s.breaks.into(), s.rest]);
    for v in [
        s.shot.x,
        s.shot.y,
        s.steer,
        s.steer_response,
        s.gravity,
        s.max_fall,
    ] {
        out.extend(v.to_bits().to_le_bytes());
    }
}

/// Appends a rope's values to `out`, for [`World::state_bytes`].
fn rope_bytes(out: &mut Vec<u8>, r: &Rope) {
    let fixed = |out: &mut Vec<u8>, v: Fixed| out.extend(v.to_bits().to_le_bytes());
    for n in [r.swing.0, r.swing.1, r.below, r.near, r.far, r.view_top] {
        out.extend(n.to_le_bytes());
    }
    out.extend([r.turn_frames, r.turn_facing]);
    for v in [
        r.speed,
        r.sway_response,
        r.slow,
        r.up[0],
        r.up[1],
        r.down[0],
        r.down[1],
        r.climb_response,
        r.jump_speed,
    ] {
        fixed(out, v);
    }
}

/// Advances the world one logic frame (1/60 s). Once the level is
/// complete or the game is over, nothing changes.
pub fn step(world: &mut World, input: &InputFrame) {
    advance(world, input);
    // A ridden animal is under its rider on every frame, those on which
    // everything else waits too.
    hutch::under_rider(world);
}

/// One logic frame of [`step`].
fn advance(world: &mut World, input: &InputFrame) {
    world.events.clear();
    // Things that take a step this frame say so.
    for thing in &mut world.things {
        thing.still = true;
    }
    world.still = true;
    world.hit_by = None;
    world.flow.events.clear();
    world.pause = 0;
    if world.flow.outcome.is_over() {
        return;
    }
    // A burst's pieces and flash give up their pictures on the step they
    // go, so they're free from the next.
    if let Some(placing) = &mut world.placing {
        placing.held_pictures = world.bursts.len() * (KEG_PIECES + 1);
    }
    // With `bounce_holds_all`, they hold still on a bounce's frozen
    // frames.
    if !things::holds_all(world, world.player.frozen > 0) {
        keg::bursts(world);
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
            world.flow.play_from = world.frame;
            view::restart(world);
            keg::begin(world);
            flow::afresh(world);
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
    if world.leaving.is_some() {
        exit::step(world);
        world.frame += 1;
        return;
    }
    if drop_in::hold(world) {
        // Everything waits on the frame the lead drops in.
        world.frame += 1;
        return;
    }
    let walking = walk_in::hold(world);
    if scene::hold(world, walking) || walking {
        // Everything waits while the characters walk in, and in a scene.
        world.frame += 1;
        return;
    }
    let mut begin = entrance::view_begin(world);
    if keg::hold(world, &input.players[0]) || entrance::hold(world) {
        // Everything waits while the player is in the keg or the
        // entrance, but the view from frame 1 when it begins as an area
        // does. Its top keeps its aim while the player is in a keg.
        let physics = world.player_physics();
        begin.hold_y |= world.resume.is_some();
        if world.frame > 0 && physics[Param::ViewBeginBack] > Fixed::ZERO {
            view::update(world, begin);
        }
        world.frame += 1;
        return;
    }
    loss::tick(world);
    hits::tick(world);
    // Input is ignored for a while after a hit that cost a hit point.
    let stunned = InputFrame::default();
    let input = if world.hit_stunned() { &stunned } else { input };
    // Walkers appear and are removed by where the view was at the start
    // of the step.
    let view_at = start_view.map(|v| (v.x(), v.y()));
    if let Some(at) = view_at {
        view::walkers(world, at, true);
    }
    let physics = world.player_physics();
    let start_pos = world.player.pos;
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
        flow::flight_stopped(world);
        things::update(world, start_pos, frozen);
    } else {
        // Nothing hurts a swapping player; what touched the other
        // character hurts it after the swap (`swap_touch`).
        let hurt_by = world
            .player
            .hurt_ahead
            .take()
            .filter(|_| world.swap.is_none() && !world.protected() && !world.losing());
        if hurt_by.is_some() {
            world.player.hit(&mut world.events);
        } else if entrance::throw(world, &input.players[0]) {
            // Thrown out of the entrance, the player moves by its rules.
        } else if sling::fly(world, &input.players[0]) {
            // Held or shot by a sling, the player moves by its rules.
        } else if loss::update(world) {
            // The loss moves the characters.
        } else if ride::unseat(world) {
            // Knocked off the animal, the player doesn't move.
        } else if barrel_ride::hold(world, &input.players[0]) {
            // Riding a metal barrel, the player moves by its rules.
        } else if rope::hold(world, &input.players[0]) {
            // Holding a rope, the player moves by its rules.
        } else if !swap::update(world, &physics, &input.players[0]) {
            carry::before(world, &input.players[0]);
            // After a swap, the physics are the new player's.
            let physics = world.player_physics();
            let picture = world.view.map(|v| v.x());
            world.player.update(
                &world.level,
                &world.feel,
                &physics,
                &input.players[0],
                picture,
                &mut world.events,
            );
            rope::catch(world);
            sling::catch(world);
        }
        let hurt_now = things::update(world, start_pos, frozen);
        barrel_ride::after(world, start_pos.y);
        flow::update(world, hurt_by.or(hurt_now), facing);
        ending::pass(world, &input.players[0]);
        exit::take_over(world);
    }
    if !frozen && follow::due(world) == Some(false) {
        follow::update(world, Lead::of(&world.player));
    }
    if !frozen && follow::due(world).is_some() {
        follow::note(world);
    }
    entrance::after(world, start_pos);
    if let Some(at) = view_at {
        view::walkers(world, at, false);
    }
    view::update(world, begin);
    carry::after(world);
    keg::after(world);
    drop_in::after(world);
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
        window: None,
        immediate: false,
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
