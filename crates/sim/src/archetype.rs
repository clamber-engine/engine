//! Entity archetypes: what an entity's `type` means.
//!
//! The built-in archetypes are in `content/base/archetypes.toml`, which
//! documents each behavior and its values. An entity's `params` change
//! its archetype's values for that entity alone.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use clamber_fixed::Fixed;
use clamber_level::{Entity, Vec2};
use serde::Deserialize;
use toml::{Table, Value};

use crate::player::Facing;
use crate::pose::{Pose, Stance};

const BUILTIN: &str = include_str!("../../../content/base/archetypes.toml");
const BUILTIN_ORIGIN: &str = "content/base/archetypes.toml";

/// How a thing behaves, with the values its behavior uses.
// A collectible's reach makes it the largest by far; there are few
// things, and being `Copy` keeps them simple.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Behavior {
    /// An enemy that walks along the ground at `speed`, turning at walls
    /// and ledges, and at the ends of its `range` if it has one. Above 0,
    /// `response` is the part of the gap to that speed it closes each
    /// frame, as the player's `speed_response`, each of its parts rounded
    /// on its own and added up; 0 changes speed at once. With
    /// `move_first`, it moves by the speed it had before each frame's
    /// change, walking and falling. With `columns`, it stands on the floor
    /// of the 1-px column holding its `x`. Its facing follows a turn
    /// `turn_delay` frames later. Bounced on, its speed becomes
    /// `bounced_speed` away from the way the player faces, if it has one.
    /// Defeated, it falls as `defeat` says. When it appears with the view
    /// (see [`View`](crate::View)), it waits `appear_wait` frames,
    /// counting the one it appears on, before it moves. Walkers with the
    /// same `group` above 0 appear and are removed with the view together.
    /// With a `contact`, it hurts the player or is bounced on as that
    /// says, instead of by the preset's boxes. With a `hurt_jump_lift`,
    /// the preset's hurt box rises that far against it when the
    /// airborne pose lifts it, instead of the preset's `hurt_jump_lift`. A player riding an
    /// animal meets it as `rider` says instead (see [`ride`](crate::ride)).
    Walker {
        speed: Fixed,
        response: Response,
        move_first: bool,
        columns: bool,
        bounced_speed: Option<Fixed>,
        contact: Option<WalkerContact>,
        hurt_jump_lift: Option<Fixed>,
        rider: RiderReach,
        facing: Facing,
        turn_delay: u8,
        appear_wait: u8,
        group: u32,
        range: Option<(Fixed, Fixed)>,
        defeat: Fall,
    },
    /// An enemy that flies from where it's placed to `path` away and
    /// back, taking `period` frames for the round trip.
    Patrol { path: Vec2, period: u32 },
    /// Fires the player off at `velocity` on touch.
    Launcher { velocity: Vec2 },
    /// An object the player can pick up, carry, throw and put down
    /// (see [`carry`](crate::carry)), from `reach` px away or closer.
    /// Thrown, it hits walkers its `hit` box touches, or its `hit_early`
    /// box for its first `hit_early_time` frames out of the hands. With a
    /// `holder`, it holds the missing character.
    Carry {
        reach: Fixed,
        hit: Hit,
        hit_early: Hit,
        hit_early_time: u8,
        holder: Option<Holder>,
    },
    /// A collectible the player takes from within its [`Reach`] (see
    /// [`collect`](crate::collect)). It adds `worth` to the collectible
    /// count, one at a time: the first on taking it, then one every
    /// `release_every` frames, each `count_delay` frames after that.
    /// With a view, it can only be taken inside its `window`, and
    /// `arrive` decides when each one reaches the count. A `letter`
    /// above 0 is that letter of the level's word: taken, it joins the
    /// letters taken `join_delay` frames later (see
    /// [`Flow::letters`](crate::Flow::letters)).
    Collect {
        worth: u8,
        release_every: u8,
        count_delay: u8,
        /// For the first character, and for any other.
        reach: [Reach; 2],
        /// How far its `x` may be past the view's left edge, both whole
        /// px, ends included, for it to be taken. `None`: anywhere.
        window: Option<(i32, i32)>,
        arrive: Arrive,
        letter: u8,
        join_delay: u8,
    },
    /// Sits where it's placed and throws a [`BALL`](crate::BALL) on each
    /// of its first `throw_count` `throws`, `from` its position, on that
    /// frame of every `period` counted from the one it appeared on (see
    /// [`throwers`](crate::throwers)).
    Thrower {
        period: u32,
        throws: [Throw; MAX_THROWS],
        throw_count: u8,
        from: Vec2,
        /// How it leaves once knocked out: as a defeated walker, never
        /// knocked back.
        defeat: Fall,
    },
    /// A ball that flies, bounces on floors and drops through them (see
    /// [`throwers`](crate::throwers)).
    Ball(Ball),
    /// A crate holding an animal, which the player breaks by landing on
    /// it and then mounts (see [`hutch`](crate::hutch)).
    Hutch(Hutch),
    /// A flying enemy that sways around where it's placed and hurts on
    /// any contact (see [`stinger`](crate::stinger)).
    Stinger(Stinger),
    /// A restart barrel: touched, it breaks and the level's restart point
    /// is reached, and play resumes there inside one (see
    /// [`keg`](crate::keg)).
    Keg(Keg),
    /// A hidden object: not drawn, never in the way and never hurting,
    /// until it's set off and releases its item (see
    /// [`hidden`](crate::hidden)).
    Hidden(Hidden),
}

/// A hidden object (spec/behavior/hidden-objects.md). Offsets from it are
/// whole px: `across`, the player's `x` rounded down less its own, and
/// `above`, its `y` less the player's feet line. It's set off at the end
/// of a frame with the offsets within `across` (the range for the
/// player's facing) and `above`:
///
/// - with `fall_height` 0, by the ground action, on the attack frames
///   among the first `strike_count` of `strike_frames`; that counts as a
///   frame on which something is knocked out;
/// - otherwise, by a player in the air, not landing and not rolling,
///   whose feet line has been `fall_height` or more above its `y` at the
///   end of a frame since it last stood on a floor. The player bounces
///   off it as off a walker.
///
/// Set off, it's gone and its item is where it was, moving at
/// `item_speed`. It stays put for `item_wait` frames, then on each frame
/// outside the ground action's pause its speed down grows by
/// `item_gravity`, up to `item_max_fall`, and it moves. Moving down to
/// less than `item_touch` above the floor under it, it's put on that
/// floor; on the frame after it moves across once more, and on the one
/// after that it becomes a thing of kind `becomes`, `becomes_at` from
/// where it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hidden {
    pub across: [(i32, i32); 2],
    pub above: (i32, i32),
    pub strike_frames: [u8; MAX_STRIKE_FRAMES],
    /// Only the first `strike_count` are used.
    pub strike_count: u8,
    pub fall_height: i32,
    pub becomes: &'static str,
    pub becomes_at: Vec2,
    pub item_speed: Vec2,
    pub item_wait: u8,
    pub item_gravity: Fixed,
    pub item_max_fall: Fixed,
    pub item_touch: Fixed,
}

/// The most attack frames a hidden object can be struck on.
pub const MAX_STRIKE_FRAMES: usize = 4;

/// The archetypes a hidden object's item can become.
pub const HIDDEN_ITEMS: [&str; 3] = ["cluster", "crate", "canister"];

/// A restart barrel (spec/behavior/restart-barrel.md). The player touches
/// it when its offsets from it at the end of a step are within `contact`'s
/// ranges for its pose and facing. It breaks into `pieces`, each moving
/// across by its `x` every frame and up by its `y` at the start, less
/// `piece_gravity` a frame, and a flash rising `flash_rise` a frame, which
/// last `burst_time` frames.
///
/// Resuming at the restart point, counting the frames from 0: the view's
/// left edge starts `view_back` px left of it, the player waits in a keg
/// `lift` px above it from frame 1 until it bursts on frame `burst`, and
/// is in play from the one after. The keg shakes by `shake`'s stages: from
/// each one's first frame on, by the first offset on the first two of
/// every four frames and the second on the other two. The picture fades
/// in from black from frame `fade_from`, a step every `fade_step` frames.
/// The first landing after that has a landing lockout of `land_lockout`
/// frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Keg {
    pub contact: Contact,
    pub pieces: [Vec2; KEG_PIECES],
    pub piece_gravity: Fixed,
    pub flash_rise: Fixed,
    pub burst_time: u8,
    pub lift: Fixed,
    pub burst: u8,
    pub view_back: Fixed,
    pub shake: [(u8, i8, i8); MAX_SHAKE_STAGES],
    /// Only the first `shake_stages` are used.
    pub shake_stages: u8,
    pub fade_from: u8,
    pub fade_step: u8,
    pub land_lockout: u8,
}

/// How many pieces a keg breaks into.
pub const KEG_PIECES: usize = 4;

/// The most stages a keg's shake can have.
pub const MAX_SHAKE_STAGES: usize = 4;

/// How a stinger sways (spec/behavior/stinger.md). Its turning points
/// are `reach` whole px from its start: `(left, right, up, down)`. It
/// starts at `velocity`, and its target speeds keep that size, changing
/// sign at the turning points. It hurts the player as `contact` says,
/// whose ranges don't move with a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stinger {
    pub reach: (i32, i32, i32, i32),
    pub velocity: Vec2,
    pub contact: Contact,
}

/// One of a thrower's throws: on `frame` of its period, a ball at
/// `velocity`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Throw {
    pub frame: u32,
    pub velocity: Vec2,
}

/// A crate holding an animal (spec/behavior/animal-crate.md). Offsets
/// are whole px, ends included: `across` is the player's `x` rounded
/// down less the crate's, `above` the crate's `y` less the player's feet
/// line (its `y` rounded up), with the player where it is after its move
/// on a frame. Each `across` holds the range for facing right, then the
/// one for facing left.
///
/// A player in the air moving down, not rolling, within `break_across`
/// and `break_above` breaks it and bounces off it. `animal_after` frames
/// later the animal comes out, standing `animal_hop` px above the
/// crate's position on that frame and the ones after, then on it. Once
/// it stands there, a player that would be within `mount_across` and
/// `mount_above` after its move, in the air moving down after a jump or
/// a bounce and not rolling, mounts it: it's put on the animal and holds
/// still for `mount_hold` frames, then rides it as `ride` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hutch {
    pub break_across: [(i32, i32); 2],
    pub break_above: (i32, i32),
    pub animal_after: u8,
    /// Only the first `hop_steps` are used.
    pub animal_hop: [Fixed; MAX_HOP_STEPS],
    pub hop_steps: u8,
    pub mount_across: [(i32, i32); 2],
    pub mount_above: (i32, i32),
    pub mount_hold: u8,
    pub ride: Ride,
}

/// How a player rides an animal (spec/behavior/riding.md; see
/// [`ride`](crate::ride)). Speeds are px per frame, times whole frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ride {
    /// The target speed with Left or Right held.
    pub speed: Fixed,
    /// ... and with Action held too.
    pub run_speed: Fixed,
    /// The share of the gap to the target speed closed on a frame that
    /// isn't steered.
    pub response: Fixed,
    /// ... and on a steered one.
    pub steer_response: Fixed,
    /// How long a turn lasts, and on which of its frames facing changes.
    pub turn: u8,
    pub turn_face: u8,
    /// Crouch frames before a jump's take-off frame.
    pub crouch: u8,
    /// The upward speed set on the take-off frame.
    pub jump_speed: Fixed,
    /// How many frames before a jump can start a held press may have
    /// been made.
    pub jump_memory: u8,
    /// Landing frames after a jump.
    pub landing: u8,
    /// The frame after a charge's start that hops, and the hop's upward
    /// speed.
    pub hop_at: u8,
    pub hop_speed: Fixed,
    /// The first frame after a charge's start that can begin its end, and
    /// how long the end lasts.
    pub charge: u8,
    pub charge_end: u8,
    /// Floor up to this far lower is followed down; lower floor, or none,
    /// is left.
    pub follow: Fixed,
    /// How far down the frame that leaves the floor moves, besides the
    /// frame's gravity, and the speed down it ends with.
    pub leave_drop: Fixed,
    pub leave_speed: Fixed,
    /// The speed down a steep face added to the target speed on frames
    /// after one that ended on it.
    pub push: Fixed,
    /// A jump's approach frame is the first after it starts falling that
    /// ends with floor below the feet line at most `approach_reach` px
    /// down and `approach_rows` rows of `approach_row` px lower. A
    /// landing up to `approach_steer` frames after it stays steered until
    /// then, and riding resumes `approach_land` frames after it.
    pub approach_reach: i32,
    pub approach_rows: i32,
    pub approach_row: i32,
    pub approach_steer: u8,
    pub approach_land: u8,
    /// A walker in front of the animal on this frame of a charge or later
    /// is thrown, and the rider's speed becomes `throw_rise` up.
    pub throw_from: u8,
    pub throw_rise: Fixed,
    /// Knocked off, the player's speed becomes `knock_rise` up, and it
    /// holds still with everything else for `knock_hold` frames after.
    pub knock_rise: Fixed,
    pub knock_hold: u8,
    /// The animal left behind runs off at `bolt_speed` for `bolt_dash`
    /// frames after those, then eases toward `bolt_glide` by
    /// `bolt_response` of the gap a frame, rounded toward zero to 1/256
    /// px per frame (reaching it when that rounds to nothing).
    pub bolt_speed: Fixed,
    pub bolt_dash: u8,
    pub bolt_glide: Fixed,
    pub bolt_response: Fixed,
}

/// How a walker meets a player riding an animal
/// (spec/behavior/riding-walkers.md). Offsets are whole px: `across` is
/// the rider's `x` less the walker's, rounded down, and `above` how far
/// the rider is above the walker, both rounded toward the floor. Each
/// `*_across` is a pair of ranges for a rider facing right: for a walker
/// facing the same way, then the other way. For a rider facing left, the
/// pair's range for how the walker faces turns around: from minus its
/// end to minus its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RiderReach {
    /// The walker is in front of the animal within these, with the rider
    /// where it is after its move on a frame and the walker where it was
    /// before its own, and `above` at most `attack_above`.
    pub attack_across: [(i32, i32); 2],
    pub attack_above: i32,
    /// On the rider's floor, it knocks the rider off within these at the
    /// end of a frame, if the rider was on the floor at the end of that
    /// frame and the one before, or `above` is at most `knock_above`.
    pub knock_across: [(i32, i32); 2],
    pub knock_above: i32,
    /// Thrown, its speed across is the rider's times this.
    pub throw_speed: Fixed,
}

impl RiderReach {
    /// The range of `across` for a rider facing `rider`, from `ranges`,
    /// and a walker facing `walker`.
    pub fn across(ranges: [(i32, i32); 2], rider: Facing, walker: Facing) -> (i32, i32) {
        let (from, to) = ranges[usize::from(rider != walker)];
        match rider {
            Facing::Right => (from, to),
            Facing::Left => (-to, -from),
        }
    }
}

/// The most frames an animal's hop out of its crate can have.
pub const MAX_HOP_STEPS: usize = 8;

impl Hutch {
    /// Frames after the crate breaks from which the animal stands on the
    /// crate's position and can be mounted.
    pub fn standing(&self) -> u8 {
        self.animal_after.saturating_add(self.hop_steps)
    }

    /// How far above the crate's position the animal is `since` frames
    /// after the crate broke, once it's out.
    pub fn height(&self, since: u8) -> Fixed {
        since
            .checked_sub(self.animal_after)
            .filter(|&i| i < self.hop_steps)
            .map_or(Fixed::ZERO, |i| self.animal_hop[usize::from(i)])
    }
}

/// Whether `across` and `above` are within `ranges` (the one for
/// `facing`) and `above`, ends included.
pub(crate) fn within(
    ranges: [(i32, i32); 2],
    above: (i32, i32),
    facing: Facing,
    (x, y): (i64, i64),
) -> bool {
    let (from, to) = match facing {
        Facing::Right => ranges[0],
        Facing::Left => ranges[1],
    };
    (i64::from(from)..=i64::from(to)).contains(&x)
        && (i64::from(above.0)..=i64::from(above.1)).contains(&y)
}

/// The most throws a thrower can have.
pub const MAX_THROWS: usize = 4;

/// How a ball moves. Placed in a level, it starts at `velocity`. It falls
/// under `gravity` up to `max_fall`. Falling onto a floor no more than
/// `floor_reach` above where it was, to less than `touch` above it, it
/// bounces up at `bounce` and passes through floors until it's more than
/// `drop` below that one. With a view, it's removed outside `window`:
/// `(left, right, top, bottom)`, how far past each edge of the picture,
/// whole px, it may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ball {
    pub velocity: Vec2,
    pub gravity: Fixed,
    pub max_fall: Fixed,
    pub bounce: Fixed,
    pub floor_reach: Fixed,
    pub touch: Fixed,
    pub drop: Fixed,
    pub window: (i32, i32, i32, i32),
    /// When it hurts the player, if it does.
    pub contact: Option<Contact>,
}

/// When a ball hurts the player (spec/behavior/ball-contact.md): when
/// the player's offsets from it at the end of a step are within the
/// ranges for the player's [`Pose`](crate::Pose) then, moved by the
/// part of its turn it was in before its move, it hurts the player on
/// the next step. Not on a step on which it bounced or stopped ignoring
/// floors, though.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Contact {
    pub upright: Offsets,
    pub airborne: Offsets,
    pub rolling: Offsets,
    pub crouching: Offsets,
    pub fidget: Offsets,
    /// Facing left, `across` is turned around: from `mirror` less its
    /// high end to `mirror` less its low end.
    pub mirror: i32,
    /// The parts of the ball's turn, in order: how many steps each
    /// lasts, and how far it moves both ends of `across` and of `above`.
    /// Only the first `turn_parts` are used.
    pub turn: [(u32, i32, i32); MAX_TURN_PARTS],
    pub turn_parts: u8,
    /// A jump has the upright pose on the frames up to this many after
    /// the one it starts on, and the airborne one after.
    pub upright_jump: u8,
    /// After landing, the airborne pose goes on up to this many frames
    /// after the landing frame, by how the player went into the air (see
    /// [`Air`](crate::Air)).
    pub land_hold: [u8; AIR_KINDS],
    /// After a roll, the rolling pose goes on for this many frames when
    /// it runs on into a run, and for the second when it doesn't.
    pub roll_after: (u8, u8),
    /// Standing still, the fidget pose starts this many frames into the
    /// standing pose, lasts the second, and comes again every third (0:
    /// only once).
    pub fidget_time: (u32, u32, u32),
}

/// The most parts a ball's turn can have.
pub const MAX_TURN_PARTS: usize = 8;

/// How many ways into the air [`Contact::land_hold`] tells apart.
pub const AIR_KINDS: usize = 5;

/// Offsets of the player from a ball that it reaches, facing right, in
/// whole px, ends included: `across`, the player's `x` less the ball's,
/// both rounded down, and `above`, how far the ball is above the player,
/// the player's `y` less the ball's, both rounded up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Offsets {
    pub across: (i32, i32),
    pub above: (i32, i32),
}

impl Contact {
    /// The offsets a ball reaches a player in `pose` facing `facing`
    /// from, after `steps` steps of its turn.
    pub fn reach(&self, pose: Pose, facing: Facing, steps: u32) -> Offsets {
        let base = match pose {
            Pose::Upright => self.upright,
            Pose::Airborne => self.airborne,
            Pose::Rolling => self.rolling,
            Pose::Crouching => self.crouching,
            Pose::Fidget => self.fidget,
        };
        let (from, to) = base.across;
        let across = match facing {
            Facing::Right => (from, to),
            Facing::Left => (self.mirror - to, self.mirror - from),
        };
        let parts = &self.turn[..usize::from(self.turn_parts)];
        let cycle: u32 = parts.iter().map(|p| p.0).sum();
        let (mut at, mut shift) = (if cycle > 0 { steps % cycle } else { 0 }, (0, 0));
        for &(length, x, y) in parts {
            if at < length {
                shift = (x, y);
                break;
            }
            at -= length;
        }
        Offsets {
            across: (across.0 + shift.0, across.1 + shift.0),
            above: (base.above.0 + shift.1, base.above.1 + shift.1),
        }
    }
}

impl Offsets {
    pub fn contains(self, across: i64, above: i64) -> bool {
        let within =
            |(from, to): (i32, i32), v: i64| (i64::from(from)..=i64::from(to)).contains(&v);
        within(self.across, across) && within(self.above, above)
    }
}

/// With a view, when each released part of a collectible reaches the
/// count, by where the collectible is in the view on its release step:
/// the first of `rules` it's inside gives the delay, and with none,
/// `otherwise` does. `None` never reaches the count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Arrive {
    /// Whether the view is the one from the start of the release step,
    /// before it moves; otherwise it's the one from its end.
    pub at_start: bool,
    pub rules: [ArriveRule; MAX_ARRIVE_RULES],
    /// Only the first `rule_count` are used.
    pub rule_count: u8,
    pub otherwise: Option<u8>,
}

/// One of [`Arrive`]'s rules: a collectible whose position, less the
/// view's left and top edges, all whole px, is in `x` and `y` (ends
/// included) reaches the count `delay` frames after its release, or
/// never. Without vertical view motion, `y` always holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ArriveRule {
    pub x: (i32, i32),
    pub y: (i32, i32),
    pub delay: Option<u8>,
}

/// The most letters a level's word can have.
pub const MAX_LETTERS: u8 = 8;

/// The most rules an [`Arrive`] can have.
pub const MAX_ARRIVE_RULES: usize = 4;

/// Where a player can take a collectible from, by what it's doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reach {
    /// Standing still.
    pub stand: Span,
    /// Walking or running.
    pub walk: Span,
    /// Crouching.
    pub crouch: Span,
    pub roll: Span,
    /// The first `jump_early` frames of a jump, from the one it starts on.
    pub jump: Span,
    /// The rest of a jump, other times in the air, and the first
    /// `land_hold` frames after landing.
    pub air: Span,
    /// In the air after `jump`, the lowest `below` while rising: the
    /// first `(vy, below)` whose `vy` is at least the player's vertical
    /// speed gives it. Past them all, it's `air`'s. Only the first
    /// `air_steps` are used.
    pub air_below: [(Fixed, i32); MAX_AIR_STEPS],
    pub air_steps: u8,
    pub jump_early: u8,
    pub land_hold: u8,
}

/// The most `air_below` steps a [`Reach`] can have.
pub const MAX_AIR_STEPS: usize = 8;

/// Offsets from a collectible a player takes it from, in whole px, ends
/// included: how far the player is past it the way the player faces,
/// and how far the player's feet are below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub past: (i32, i32),
    pub below: (i32, i32),
}

impl Span {
    pub fn contains(self, past: i32, below: i32) -> bool {
        (self.past.0..=self.past.1).contains(&past)
            && (self.below.0..=self.below.1).contains(&below)
    }
}

/// How a defeated walker leaves: it jumps up at `jump`, then falls
/// under `gravity` up to `max_fall`, through the floor and out of the
/// level. Hit by a thrown object, it's knocked back at `knock` px per
/// frame first, which eases toward `knock_glide` as it falls. With
/// `move_first`, it moves before its fall speed changes on each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fall {
    pub jump: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
    pub knock: Fixed,
    pub knock_glide: Fixed,
    pub move_first: bool,
}

/// How a walker's speed eases toward its target: each frame, by each
/// part of the gap, rounded toward zero to a 1/256 px per frame on its
/// own, added up. When that adds up to nothing, the speed becomes the
/// target. All parts 0 change speed at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Response {
    pub parts: [Fixed; MAX_RESPONSE_PARTS],
}

/// The most parts a walker's [`Response`] can have.
pub const MAX_RESPONSE_PARTS: usize = 2;

impl Response {
    /// Whether it changes speed at once.
    pub fn at_once(self) -> bool {
        self.parts.iter().all(|&p| p <= Fixed::ZERO)
    }
}

/// When a walker touches the player, and whether the player is hurt or
/// bounces off it (spec/behavior/walker-contact.md, "In `title-2`"). It
/// touches when the player's offsets from it are within the ranges for
/// the player's [`Stance`](crate::Stance), with the player where it is
/// after its move on a frame and the walker where it was before its own.
/// `across` is the player's `x` less the walker's, `above` the walker's
/// `y` less the player's, all rounded down. The ranges are for facing
/// right; facing left, `across` turns around to from `turn` less its end
/// to `turn` less its start.
///
/// With a `roll` range, a rolling player defeats it within that range on
/// any roll frame, and a player in the stop after a roll is hurt within
/// it (spec/behavior/rolling-into-walkers.md, "In `title-2`"). Without
/// one, it doesn't touch a player that rolls or is in such a stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WalkerContact {
    pub stand: Offsets,
    pub crouch: Offsets,
    pub walk: Offsets,
    pub run: Offsets,
    pub air: Offsets,
    pub roll: Option<Offsets>,
    pub turn: i32,
    /// The air stance goes on after landing up to this many frames after
    /// the landing frame, or the second when Action and a direction are
    /// held on it.
    pub land_hold: (u8, u8),
    /// A player in the air that touches it bounces off if it rises at
    /// this or slower, after the frame's gravity, and `above` is
    /// `bounce_above` or more. Otherwise it's hurt.
    pub bounce_rise: Fixed,
    pub bounce_above: i32,
}

impl WalkerContact {
    /// The offsets a walker touches a player in `stance` facing `facing`
    /// from, if it has a range for that stance.
    pub fn reach(&self, stance: Stance, facing: Facing) -> Option<Offsets> {
        let base = match stance {
            Stance::Standing => self.stand,
            Stance::Crouching => self.crouch,
            Stance::Walking => self.walk,
            Stance::Running => self.run,
            Stance::Air => self.air,
            Stance::Rolling | Stance::Stopped => self.roll?,
        };
        let (from, to) = base.across;
        let across = match facing {
            Facing::Right => (from, to),
            Facing::Left => (self.turn - to, self.turn - from),
        };
        Some(Offsets {
            across,
            above: base.above,
        })
    }
}

/// A thrown object's box for hitting walkers, from its position with
/// `x` rounded down: `behind` and `ahead` of it the way it moves,
/// `above` and `below` it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Hit {
    pub behind: Fixed,
    pub ahead: Fixed,
    pub above: Fixed,
    pub below: Fixed,
}

impl Hit {
    /// The box as `(left, top, right, bottom)` for an object at `pos`
    /// moving right if `right`, left otherwise.
    pub fn bounds(&self, pos: Vec2, right: bool) -> (Fixed, Fixed, Fixed, Fixed) {
        let x = Fixed::from_int(pos.x.floor() as i32);
        let (left, right) = if right {
            (x - self.behind, x + self.ahead)
        } else {
            (x - self.ahead, x + self.behind)
        };
        (left, pos.y - self.above, right, pos.y + self.below)
    }

    /// Whether the box of an object at `pos` moving right if `right`
    /// holds `target`, as whole pixels: both `x` rounded down and both `y`
    /// rounded up, ends included.
    pub fn holds(&self, pos: Vec2, right: bool, target: Vec2) -> bool {
        let whole = |v: Vec2| (v.x.floor(), -(-v.y).floor());
        let ((x, y), (tx, ty)) = (whole(pos), whole(target));
        let (behind, ahead) = (self.behind.floor(), self.ahead.floor());
        let (from, to) = if right {
            (x - behind, x + ahead)
        } else {
            (x - ahead, x + behind)
        };
        (from..=to).contains(&tx) && (y - self.above.floor()..=y + self.below.floor()).contains(&ty)
    }
}

/// What makes an object hold the missing character: it frees it when it
/// breaks (see [`carry`](crate::carry)). Thrown, it also breaks outside
/// `window` around the picture, as a [`Ball`] is removed, and on throwers
/// and balls its `thrower` and `ball` boxes hold, or their early boxes
/// for its first `hit_early_time` frames out of the hands
/// ([`Hit::holds`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Holder {
    pub window: (i32, i32, i32, i32),
    pub thrower: Hit,
    pub thrower_early: Hit,
    pub ball: Hit,
    pub ball_early: Hit,
}

impl Behavior {
    /// Whether it's an enemy: it hurts the player and can be defeated.
    pub fn is_enemy(self) -> bool {
        matches!(self, Behavior::Walker { .. } | Behavior::Patrol { .. })
    }

    /// Whether it's a walker.
    pub fn is_walker(self) -> bool {
        matches!(self, Behavior::Walker { .. })
    }

    /// How it falls out of the level once defeated, if it does.
    pub fn fall(self) -> Option<Fall> {
        match self {
            Behavior::Walker { defeat, .. } | Behavior::Thrower { defeat, .. } => Some(defeat),
            _ => None,
        }
    }
}

/// An entity's archetype with its own params applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Archetype {
    pub behavior: Behavior,
    /// The box it touches the player with: standing on its position for
    /// a walker, an object that can be carried or a crate holding an
    /// animal, centred on it otherwise.
    pub size: Vec2,
    /// How far the box's middle is in front of its position, the way it's
    /// going.
    pub offset: Fixed,
}

impl Archetype {
    /// The box as `(left, top, right, bottom)` for a thing at `pos` going
    /// the way `heading` says.
    pub fn bounds(&self, pos: Vec2, heading: Facing) -> (Fixed, Fixed, Fixed, Fixed) {
        let half_w = half(self.size.x);
        let x = match heading {
            Facing::Left => pos.x - self.offset,
            Facing::Right => pos.x + self.offset,
        };
        let (top, bottom) = match self.behavior {
            Behavior::Walker { .. } | Behavior::Carry { .. } | Behavior::Hutch(_) => {
                (pos.y - self.size.y, pos.y)
            }
            _ => {
                let half_h = half(self.size.y);
                (pos.y - half_h, pos.y + half_h)
            }
        };
        (x - half_w, top, x + half_w, bottom)
    }
}

fn half(v: Fixed) -> Fixed {
    Fixed::from_bits(v.to_bits() / 2)
}

/// An entity whose params don't fit its archetype.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchetypeError {
    pub origin: String,
    pub message: String,
}

impl fmt::Display for ArchetypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for ArchetypeError {}

/// Archetypes by name, as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Archetypes {
    tables: BTreeMap<String, Table>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchetypesFile {
    archetypes: BTreeMap<String, Table>,
}

impl Archetypes {
    /// The engine's archetypes, from `content/base/archetypes.toml`.
    pub fn builtin() -> &'static Archetypes {
        static BUILTIN_ARCHETYPES: OnceLock<Archetypes> = OnceLock::new();
        BUILTIN_ARCHETYPES.get_or_init(|| {
            Archetypes::from_toml_str(BUILTIN, BUILTIN_ORIGIN)
                .expect("built-in archetypes are valid")
        })
    }

    /// Reads a file of `[archetypes.<name>]` tables, checking that each
    /// is complete.
    pub fn from_toml_str(src: &str, origin: &str) -> Result<Archetypes, ArchetypeError> {
        let err = |message: String| ArchetypeError {
            origin: origin.to_string(),
            message,
        };
        let file: ArchetypesFile =
            toml::from_str(src).map_err(|e| err(e.to_string().trim_end().to_string()))?;
        for (name, table) in &file.archetypes {
            parse(table, &format!("archetypes.{name}")).map_err(err)?;
        }
        Ok(Archetypes {
            tables: file.archetypes,
        })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    /// The archetype called `name` as written, if there is one.
    pub fn named(&self, name: &str) -> Option<Archetype> {
        parse(self.tables.get(name)?, &format!("archetypes.{name}")).ok()
    }

    /// What `entity` is: its archetype with its params applied, or `None`
    /// if its type isn't an archetype. `origin` and `path` name the
    /// entity in errors.
    pub fn resolve(
        &self,
        entity: &Entity,
        origin: &str,
        path: &str,
    ) -> Result<Option<Archetype>, ArchetypeError> {
        let Some(base) = self.tables.get(&entity.kind) else {
            return Ok(None);
        };
        let err = |message: String| ArchetypeError {
            origin: origin.to_string(),
            message,
        };
        let mut table = base.clone();
        for (key, value) in &entity.params {
            if key == "behavior" {
                return Err(err(format!("{path}.params.behavior: can't be changed")));
            }
            table.insert(key.clone(), value.clone());
        }
        parse(&table, &format!("{path}.params"))
            .map(Some)
            .map_err(err)
    }
}

/// Reads a window around the picture, `[left, right, top, bottom]` in
/// whole px. Errors start with `at`.
fn edges(value: &Value, at: &str) -> Result<(i32, i32, i32, i32), String> {
    match value {
        Value::Array(items) if items.len() == 4 => {
            let mut ends = [0; 4];
            for (i, item) in items.iter().enumerate() {
                ends[i] = match item {
                    Value::Integer(n) if i32::try_from(*n).is_ok() => *n as i32,
                    _ => return Err(format!("{at}[{i}]: expected a whole number of px")),
                };
            }
            Ok((ends[0], ends[1], ends[2], ends[3]))
        }
        _ => Err(format!("{at}: expected [left, right, top, bottom]")),
    }
}

/// Reads one archetype's table. Errors start with `path`.
fn parse(table: &Table, path: &str) -> Result<Archetype, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let number = |key: &str| -> Result<Fixed, String> {
        clamber_pack::read_number(get(key)?, "", &format!("{path}.{key}")).map_err(|e| e.message)
    };
    let pair = |key: &str| -> Result<Vec2, String> {
        let at = format!("{path}.{key}");
        match get(key)? {
            Value::Array(items) if items.len() == 2 => {
                let n = |i: usize| {
                    clamber_pack::read_number(&items[i], "", &format!("{at}[{i}]"))
                        .map_err(|e| e.message)
                };
                Ok(Vec2::new(n(0)?, n(1)?))
            }
            _ => Err(format!("{at}: expected [x, y]")),
        }
    };

    let behavior = match get("behavior")? {
        Value::String(s) => s.as_str(),
        _ => return Err(format!("{path}.behavior: expected a string")),
    };
    let known: &[&str] = match behavior {
        "walker" => &[
            "speed",
            "response",
            "facing",
            "turn_delay",
            "appear_wait",
            "group",
            "range",
            "defeat_jump",
            "defeat_gravity",
            "defeat_max_fall",
            "defeat_knock",
            "defeat_knock_glide",
            "move_first",
            "columns",
            "bounced_speed",
            "contact",
            "hurt_jump_lift",
            "rider",
        ],
        "patrol" => &["path", "period"],
        "launcher" => &["velocity"],
        "carry" => &[
            "reach",
            "hit",
            "hit_early",
            "hit_early_time",
            "holds_character",
            "holder_window",
            "hit_thrower",
            "hit_thrower_early",
            "hit_ball",
            "hit_ball_early",
        ],
        "thrower" => &[
            "period",
            "throws",
            "throw_from",
            "defeat_jump",
            "defeat_gravity",
            "defeat_max_fall",
        ],
        "ball" => &[
            "velocity",
            "gravity",
            "max_fall",
            "bounce",
            "floor_reach",
            "touch",
            "drop",
            "window",
            "contact",
        ],
        "hutch" => &[
            "break_across",
            "break_above",
            "animal_after",
            "animal_hop",
            "mount_across",
            "mount_above",
            "mount_hold",
            "ride",
        ],
        "stinger" => &["reach", "velocity", "contact"],
        "keg" => &[
            "contact",
            "pieces",
            "piece_gravity",
            "flash_rise",
            "burst_time",
            "lift",
            "burst",
            "view_back",
            "shake",
            "fade_from",
            "fade_step",
            "land_lockout",
        ],
        "hidden" => &[
            "across",
            "above",
            "strike_frames",
            "fall_height",
            "becomes",
            "becomes_at",
            "item_speed",
            "item_wait",
            "item_gravity",
            "item_max_fall",
            "item_touch",
        ],
        "collect" => &[
            "worth",
            "release_every",
            "count_delay",
            "reach",
            "window",
            "arrive",
            "arrive_view",
            "arrive_else",
            "letter",
            "join_delay",
        ],
        _ => return Err(format!("{path}.behavior: unknown behavior `{behavior}`")),
    };
    if let Some(key) = table.keys().find(|k| {
        !matches!(k.as_str(), "behavior" | "size" | "offset") && !known.contains(&k.as_str())
    }) {
        return Err(format!("{path}.{key}: `{behavior}` has no such value"));
    }

    let size = pair("size")?;
    if size.x <= Fixed::ZERO || size.y <= Fixed::ZERO {
        return Err(format!("{path}.size: must be positive"));
    }
    let behavior = match behavior {
        "walker" => {
            let speed = number("speed")?;
            if speed.is_negative() {
                return Err(format!("{path}.speed: must not be negative"));
            }
            let facing = match get("facing")? {
                Value::String(s) if s == "left" => Facing::Left,
                Value::String(s) if s == "right" => Facing::Right,
                _ => return Err(format!("{path}.facing: expected \"left\" or \"right\"")),
            };
            let response = parse_response(get("response")?, &format!("{path}.response"))?;
            let flag = |key: &str| match table.get(key) {
                None | Some(Value::Boolean(false)) => Ok(false),
                Some(Value::Boolean(true)) => Ok(true),
                Some(_) => Err(format!("{path}.{key}: expected true or false")),
            };
            let move_first = flag("move_first")?;
            let bounced_speed = match table.get("bounced_speed") {
                None => None,
                Some(_) => {
                    let v = number("bounced_speed")?;
                    if v.is_negative() {
                        return Err(format!("{path}.bounced_speed: must not be negative"));
                    }
                    Some(v)
                }
            };
            let hurt_jump_lift = match table.get("hurt_jump_lift") {
                None => None,
                Some(_) => {
                    let v = number("hurt_jump_lift")?;
                    if v.is_negative() {
                        return Err(format!("{path}.hurt_jump_lift: must not be negative"));
                    }
                    Some(v)
                }
            };
            let frames = |key: &str| match get(key)? {
                Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
                _ => Err(format!(
                    "{path}.{key}: expected a whole number of frames, 0 to 255"
                )),
            };
            let turn_delay = frames("turn_delay")?;
            let appear_wait = frames("appear_wait")?;
            let group = match get("group")? {
                Value::Integer(n) if (0..=i64::from(u32::MAX)).contains(n) => *n as u32,
                _ => return Err(format!("{path}.group: expected a whole number, 0 or more")),
            };
            let range = match get("range")? {
                Value::Array(items) if items.is_empty() => None,
                Value::Array(items) if items.len() == 2 => {
                    let r = pair("range")?;
                    if r.x > r.y {
                        return Err(format!("{path}.range: left end is past the right end"));
                    }
                    Some((r.x, r.y))
                }
                _ => return Err(format!("{path}.range: expected [] or [left, right]")),
            };
            let defeat = Fall {
                jump: number("defeat_jump")?,
                gravity: number("defeat_gravity")?,
                max_fall: number("defeat_max_fall")?,
                knock: number("defeat_knock")?,
                knock_glide: number("defeat_knock_glide")?,
                move_first,
            };
            if defeat.gravity <= Fixed::ZERO || defeat.max_fall <= Fixed::ZERO {
                return Err(format!(
                    "{path}: `defeat_gravity` and `defeat_max_fall` must be positive"
                ));
            }
            Behavior::Walker {
                speed,
                response,
                move_first,
                columns: flag("columns")?,
                bounced_speed,
                contact: match table.get("contact") {
                    None => None,
                    Some(v) => Some(parse_walker_contact(v, &format!("{path}.contact"))?),
                },
                hurt_jump_lift,
                rider: parse_rider(get("rider")?, &format!("{path}.rider"))?,
                facing,
                turn_delay,
                appear_wait,
                group,
                range,
                defeat,
            }
        }
        "patrol" => {
            let period = match get("period")? {
                Value::Integer(n) if *n >= 2 && *n <= i64::from(u32::MAX) => *n as u32,
                _ => {
                    return Err(format!(
                        "{path}.period: expected a whole number of frames, at least 2"
                    ));
                }
            };
            Behavior::Patrol {
                path: pair("path")?,
                period,
            }
        }
        "carry" => {
            let reach = number("reach")?;
            if reach.is_negative() {
                return Err(format!("{path}.reach: must not be negative"));
            }
            let hit = |key: &str| -> Result<Hit, String> {
                let at = format!("{path}.{key}");
                match get(key)? {
                    Value::Array(items) if items.len() == 4 => {
                        let n = |i: usize| {
                            clamber_pack::read_number(&items[i], "", &format!("{at}[{i}]"))
                                .map_err(|e| e.message)
                        };
                        Ok(Hit {
                            behind: n(0)?,
                            ahead: n(1)?,
                            above: n(2)?,
                            below: n(3)?,
                        })
                    }
                    _ => Err(format!("{at}: expected [behind, ahead, above, below]")),
                }
            };
            let hit_early_time = match get("hit_early_time")? {
                Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => *n as u8,
                _ => {
                    return Err(format!(
                        "{path}.hit_early_time: expected a whole number of frames, 0 to 255"
                    ));
                }
            };
            let holder = match table.get("holds_character") {
                None | Some(Value::Boolean(false)) => None,
                Some(Value::Boolean(true)) => Some(Holder {
                    window: edges(get("holder_window")?, &format!("{path}.holder_window"))?,
                    thrower: hit("hit_thrower")?,
                    thrower_early: hit("hit_thrower_early")?,
                    ball: hit("hit_ball")?,
                    ball_early: hit("hit_ball_early")?,
                }),
                Some(_) => return Err(format!("{path}.holds_character: expected true or false")),
            };
            Behavior::Carry {
                reach,
                hit: hit("hit")?,
                hit_early: hit("hit_early")?,
                hit_early_time,
                holder,
            }
        }
        "collect" => {
            let small = |key: &str, min: i64| match get(key)? {
                Value::Integer(n) if (min..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
                _ => Err(format!(
                    "{path}.{key}: expected a whole number, {min} to {}",
                    u8::MAX
                )),
            };
            let reach = match get("reach")? {
                Value::Array(tables) if (1..=2).contains(&tables.len()) => {
                    let first = parse_reach(&tables[0], &format!("{path}.reach[0]"))?;
                    match tables.get(1) {
                        Some(t) => [first, parse_reach(t, &format!("{path}.reach[1]"))?],
                        None => [first; 2],
                    }
                }
                _ => return Err(format!("{path}.reach: expected one or two tables")),
            };
            let count_delay = small("count_delay", 0)?;
            let window = match table.get("window") {
                None => None,
                Some(Value::Array(items)) if items.is_empty() => None,
                Some(Value::Array(items)) if items.len() == 2 => {
                    let end = |i: usize| match &items[i] {
                        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
                        _ => Err(format!("{path}.window[{i}]: expected a whole number of px")),
                    };
                    let (from, to) = (end(0)?, end(1)?);
                    if from > to {
                        return Err(format!("{path}.window: it ends before it starts"));
                    }
                    Some((from, to))
                }
                Some(_) => return Err(format!("{path}.window: expected [] or [from, to]")),
            };
            let optional = |key: &str, max: u8| match table.get(key) {
                None => Ok(0),
                Some(Value::Integer(n)) if (0..=i64::from(max)).contains(n) => Ok(*n as u8),
                Some(_) => Err(format!("{path}.{key}: expected a whole number, 0 to {max}")),
            };
            let letter = optional("letter", MAX_LETTERS)?;
            let worth = small("worth", 0)?;
            if worth == 0 && letter == 0 {
                return Err(format!(
                    "{path}.worth: must be at least 1, unless it's a `letter`"
                ));
            }
            Behavior::Collect {
                worth,
                release_every: small("release_every", 0)?,
                count_delay,
                reach,
                window,
                arrive: parse_arrive(table, path, count_delay)?,
                letter,
                join_delay: optional("join_delay", u8::MAX)?,
            }
        }
        "thrower" => {
            let period = match get("period")? {
                Value::Integer(n) if *n >= 1 && *n <= i64::from(u32::MAX) => *n as u32,
                _ => {
                    return Err(format!(
                        "{path}.period: expected a whole number of frames, at least 1"
                    ));
                }
            };
            let at = format!("{path}.throws");
            let items = match get("throws")? {
                Value::Array(items) if items.len() <= MAX_THROWS => items,
                _ => {
                    return Err(format!(
                        "{at}: expected a list of up to {MAX_THROWS} [frame, vx, vy]"
                    ));
                }
            };
            let mut throws = [Throw::default(); MAX_THROWS];
            for (i, item) in items.iter().enumerate() {
                let at = format!("{at}[{i}]");
                let Value::Array(parts) = item else {
                    return Err(format!("{at}: expected [frame, vx, vy]"));
                };
                let [frame, vx, vy] = parts.as_slice() else {
                    return Err(format!("{at}: expected [frame, vx, vy]"));
                };
                let frame = match frame {
                    Value::Integer(n) if (0..i64::from(period)).contains(n) => *n as u32,
                    _ => {
                        return Err(format!(
                            "{at}[0]: expected a whole number of frames, less than `period`"
                        ));
                    }
                };
                let n = |v: &Value, i: usize| {
                    clamber_pack::read_number(v, "", &format!("{at}[{i}]")).map_err(|e| e.message)
                };
                throws[i] = Throw {
                    frame,
                    velocity: Vec2::new(n(vx, 1)?, n(vy, 2)?),
                };
            }
            let defeat = Fall {
                jump: number("defeat_jump")?,
                gravity: number("defeat_gravity")?,
                max_fall: number("defeat_max_fall")?,
                knock: Fixed::ZERO,
                knock_glide: Fixed::ZERO,
                move_first: false,
            };
            if defeat.gravity <= Fixed::ZERO || defeat.max_fall <= Fixed::ZERO {
                return Err(format!(
                    "{path}: `defeat_gravity` and `defeat_max_fall` must be positive"
                ));
            }
            Behavior::Thrower {
                period,
                throws,
                throw_count: items.len() as u8,
                from: pair("throw_from")?,
                defeat,
            }
        }
        "ball" => {
            let window = edges(get("window")?, &format!("{path}.window"))?;
            let ball = Ball {
                velocity: pair("velocity")?,
                gravity: number("gravity")?,
                max_fall: number("max_fall")?,
                bounce: number("bounce")?,
                floor_reach: number("floor_reach")?,
                touch: number("touch")?,
                drop: number("drop")?,
                window,
                contact: match table.get("contact") {
                    None => None,
                    Some(v) => Some(parse_contact(v, &format!("{path}.contact"))?),
                },
            };
            if ball.gravity <= Fixed::ZERO || ball.max_fall <= Fixed::ZERO {
                return Err(format!("{path}: `gravity` and `max_fall` must be positive"));
            }
            Behavior::Ball(ball)
        }
        "hutch" => Behavior::Hutch(parse_hutch(table, path)?),
        "stinger" => {
            let at = format!("{path}.reach");
            let reach = edges(get("reach")?, &at)?;
            let (l, r, u, d) = reach;
            if [l, r, u, d].iter().any(|n| !(0..=255).contains(n)) {
                return Err(format!("{at}: each must be 0 to 255"));
            }
            Behavior::Stinger(Stinger {
                reach,
                velocity: pair("velocity")?,
                contact: parse_contact(get("contact")?, &format!("{path}.contact"))?,
            })
        }
        "keg" => Behavior::Keg(parse_keg(table, path)?),
        "hidden" => Behavior::Hidden(parse_hidden(table, path)?),
        _ => Behavior::Launcher {
            velocity: pair("velocity")?,
        },
    };
    Ok(Archetype {
        behavior,
        size,
        offset: number("offset")?,
    })
}

/// Reads a keg's values. Errors start with `path`.
fn parse_keg(table: &Table, path: &str) -> Result<Keg, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let number = |v: &Value, at: &str| -> Result<Fixed, String> {
        clamber_pack::read_number(v, "", at).map_err(|e| e.message)
    };
    let positive = |key: &str| -> Result<Fixed, String> {
        let at = format!("{path}.{key}");
        let v = number(get(key)?, &at)?;
        if v.is_negative() {
            return Err(format!("{at}: must not be negative"));
        }
        Ok(v)
    };
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let at = format!("{path}.pieces");
    let mut pieces = [Vec2::default(); KEG_PIECES];
    match get("pieces")? {
        Value::Array(items) if items.len() == KEG_PIECES => {
            for (i, item) in items.iter().enumerate() {
                let at = format!("{at}[{i}]");
                let Value::Array(xy) = item else {
                    return Err(format!("{at}: expected [across, up]"));
                };
                let [x, y] = xy.as_slice() else {
                    return Err(format!("{at}: expected [across, up]"));
                };
                pieces[i] = Vec2::new(
                    number(x, &format!("{at}[0]"))?,
                    number(y, &format!("{at}[1]"))?,
                );
            }
        }
        _ => {
            return Err(format!(
                "{at}: expected a list of {KEG_PIECES} [across, up]"
            ));
        }
    }
    let at = format!("{path}.shake");
    let stages = match get("shake")? {
        Value::Array(items) if items.len() <= MAX_SHAKE_STAGES => items,
        _ => {
            return Err(format!(
                "{at}: expected a list of up to {MAX_SHAKE_STAGES} [from, first, second]"
            ));
        }
    };
    let mut shake = [(0, 0, 0); MAX_SHAKE_STAGES];
    for (i, stage) in stages.iter().enumerate() {
        let at = format!("{at}[{i}]");
        let parts = match stage {
            Value::Array(parts) => parts.as_slice(),
            _ => &[],
        };
        let [Value::Integer(from), Value::Integer(a), Value::Integer(b)] = parts else {
            return Err(format!(
                "{at}: expected [from, first, second], whole numbers"
            ));
        };
        let (Ok(from), Ok(a), Ok(b)) = (u8::try_from(*from), i8::try_from(*a), i8::try_from(*b))
        else {
            return Err(format!("{at}: out of range"));
        };
        if i > 0 && from <= shake[i - 1].0 {
            return Err(format!("{at}: stages must start in order"));
        }
        shake[i] = (from, a, b);
    }
    Ok(Keg {
        contact: parse_contact(get("contact")?, &format!("{path}.contact"))?,
        pieces,
        piece_gravity: positive("piece_gravity")?,
        flash_rise: number(get("flash_rise")?, &format!("{path}.flash_rise"))?,
        burst_time: frames("burst_time")?,
        lift: number(get("lift")?, &format!("{path}.lift"))?,
        burst: frames("burst")?,
        view_back: number(get("view_back")?, &format!("{path}.view_back"))?,
        shake,
        shake_stages: stages.len() as u8,
        fade_from: frames("fade_from")?,
        fade_step: frames("fade_step")?.max(1),
        land_lockout: frames("land_lockout")?,
    })
}

/// Reads a hidden object's values. Errors start with `path`.
fn parse_hidden(table: &Table, path: &str) -> Result<Hidden, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let number = |key: &str| -> Result<Fixed, String> {
        clamber_pack::read_number(get(key)?, "", &format!("{path}.{key}")).map_err(|e| e.message)
    };
    let positive = |key: &str| -> Result<Fixed, String> {
        let v = number(key)?;
        if v <= Fixed::ZERO {
            return Err(format!("{path}.{key}: must be positive"));
        }
        Ok(v)
    };
    let pair = |key: &str| -> Result<Vec2, String> {
        let at = format!("{path}.{key}");
        match get(key)? {
            Value::Array(items) if items.len() == 2 => {
                let n = |i: usize| {
                    clamber_pack::read_number(&items[i], "", &format!("{at}[{i}]"))
                        .map_err(|e| e.message)
                };
                Ok(Vec2::new(n(0)?, n(1)?))
            }
            _ => Err(format!("{at}: expected [x, y]")),
        }
    };
    let at = format!("{path}.strike_frames");
    let items = match get("strike_frames")? {
        Value::Array(items) if items.len() <= MAX_STRIKE_FRAMES => items,
        _ => {
            return Err(format!(
                "{at}: expected a list of up to {MAX_STRIKE_FRAMES} frames"
            ));
        }
    };
    let mut strike_frames = [0; MAX_STRIKE_FRAMES];
    for (i, item) in items.iter().enumerate() {
        strike_frames[i] = match item {
            Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => *n as u8,
            _ => return Err(format!("{at}[{i}]: expected a frame, 0 to 255")),
        };
    }
    let fall_height = match get("fall_height")? {
        Value::Integer(n) if (0..=i64::from(i32::MAX)).contains(n) => *n as i32,
        _ => {
            return Err(format!(
                "{path}.fall_height: expected a whole number of px, 0 or more"
            ));
        }
    };
    let becomes = match get("becomes")? {
        Value::String(s) => HIDDEN_ITEMS.into_iter().find(|&k| k == s),
        _ => None,
    }
    .ok_or_else(|| format!("{path}.becomes: expected one of {HIDDEN_ITEMS:?}"))?;
    let item_wait = match get("item_wait")? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => *n as u8,
        _ => {
            return Err(format!(
                "{path}.item_wait: expected a whole number of frames, 0 to 255"
            ));
        }
    };
    Ok(Hidden {
        across: parse_ranges(get("across")?, &format!("{path}.across"), "right then left")?,
        above: parse_range(get("above")?, &format!("{path}.above"))?,
        strike_frames,
        strike_count: items.len() as u8,
        fall_height,
        becomes,
        becomes_at: pair("becomes_at")?,
        item_speed: pair("item_speed")?,
        item_wait,
        item_gravity: positive("item_gravity")?,
        item_max_fall: positive("item_max_fall")?,
        item_touch: number("item_touch")?,
    })
}

/// Reads `[from, to]`, whole px. Errors start with `at`.
fn parse_range(value: &Value, at: &str) -> Result<(i32, i32), String> {
    match value {
        Value::Array(items) if items.len() == 2 => {
            let end = |i: usize| match &items[i] {
                Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
                _ => Err(format!("{at}[{i}]: expected a whole number of px")),
            };
            let (from, to) = (end(0)?, end(1)?);
            if from > to {
                return Err(format!("{at}: it ends before it starts"));
            }
            Ok((from, to))
        }
        _ => Err(format!("{at}: expected [from, to]")),
    }
}

/// Reads `[[from, to], [from, to]]`, whole px, which come in the order
/// `order` says. Errors start with `at`.
fn parse_ranges(value: &Value, at: &str, order: &str) -> Result<[(i32, i32); 2], String> {
    match value {
        Value::Array(items) if items.len() == 2 => Ok([
            parse_range(&items[0], &format!("{at}[0]"))?,
            parse_range(&items[1], &format!("{at}[1]"))?,
        ]),
        _ => Err(format!("{at}: expected [[from, to], [from, to]], {order}")),
    }
}

/// Reads how a walker meets a rider. Errors start with `path`.
fn parse_rider(value: &Value, path: &str) -> Result<RiderReach, String> {
    const KEYS: [&str; 5] = [
        "attack_across",
        "attack_above",
        "knock_across",
        "knock_above",
        "throw_speed",
    ];
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: a rider table has no such value"));
    }
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let ranges = |key: &str| {
        parse_ranges(
            get(key)?,
            &format!("{path}.{key}"),
            "the same facing then the other",
        )
    };
    let px = |key: &str| match get(key)? {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{path}.{key}: expected a whole number of px")),
    };
    Ok(RiderReach {
        attack_across: ranges("attack_across")?,
        attack_above: px("attack_above")?,
        knock_across: ranges("knock_across")?,
        knock_above: px("knock_above")?,
        throw_speed: clamber_pack::read_number(
            get("throw_speed")?,
            "",
            &format!("{path}.throw_speed"),
        )
        .map_err(|e| e.message)?,
    })
}

/// Reads a crate holding an animal's values. Errors start with `path`.
fn parse_hutch(table: &Table, path: &str) -> Result<Hutch, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let ranges = |key: &str| parse_ranges(get(key)?, &format!("{path}.{key}"), "right then left");
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let at = format!("{path}.animal_hop");
    let items = match get("animal_hop")? {
        Value::Array(items) if items.len() <= MAX_HOP_STEPS => items,
        _ => {
            return Err(format!(
                "{at}: expected a list of up to {MAX_HOP_STEPS} heights"
            ));
        }
    };
    let mut animal_hop = [Fixed::ZERO; MAX_HOP_STEPS];
    for (i, item) in items.iter().enumerate() {
        animal_hop[i] =
            clamber_pack::read_number(item, "", &format!("{at}[{i}]")).map_err(|e| e.message)?;
    }
    Ok(Hutch {
        break_across: ranges("break_across")?,
        break_above: parse_range(get("break_above")?, &format!("{path}.break_above"))?,
        animal_after: frames("animal_after")?,
        animal_hop,
        hop_steps: items.len() as u8,
        mount_across: ranges("mount_across")?,
        mount_above: parse_range(get("mount_above")?, &format!("{path}.mount_above"))?,
        mount_hold: frames("mount_hold")?,
        ride: parse_ride(get("ride")?, &format!("{path}.ride"))?,
    })
}

/// Reads how an animal is ridden. Errors start with `path`.
fn parse_ride(value: &Value, path: &str) -> Result<Ride, String> {
    const KEYS: [&str; 31] = [
        "speed",
        "run_speed",
        "response",
        "steer_response",
        "turn",
        "turn_face",
        "crouch",
        "jump_speed",
        "jump_memory",
        "landing",
        "hop_at",
        "hop_speed",
        "charge",
        "charge_end",
        "follow",
        "leave_drop",
        "leave_speed",
        "push",
        "approach_reach",
        "approach_rows",
        "approach_row",
        "approach_steer",
        "approach_land",
        "throw_from",
        "throw_rise",
        "knock_rise",
        "knock_hold",
        "bolt_speed",
        "bolt_dash",
        "bolt_glide",
        "bolt_response",
    ];
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: a ride has no such value"));
    }
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let number = |key: &str| {
        clamber_pack::read_number(get(key)?, "", &format!("{path}.{key}")).map_err(|e| e.message)
    };
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let px = |key: &str| match get(key)? {
        Value::Integer(n) if (1..=i64::from(i32::MAX)).contains(n) => Ok(*n as i32),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of px above 0"
        )),
    };
    Ok(Ride {
        speed: number("speed")?,
        run_speed: number("run_speed")?,
        response: number("response")?,
        steer_response: number("steer_response")?,
        turn: frames("turn")?,
        turn_face: frames("turn_face")?,
        crouch: frames("crouch")?,
        jump_speed: number("jump_speed")?,
        jump_memory: frames("jump_memory")?,
        landing: frames("landing")?,
        hop_at: frames("hop_at")?,
        hop_speed: number("hop_speed")?,
        charge: frames("charge")?,
        charge_end: frames("charge_end")?,
        follow: number("follow")?,
        leave_drop: number("leave_drop")?,
        leave_speed: number("leave_speed")?,
        push: number("push")?,
        approach_reach: px("approach_reach")?,
        approach_rows: px("approach_rows")?,
        approach_row: px("approach_row")?,
        approach_steer: frames("approach_steer")?,
        approach_land: frames("approach_land")?,
        throw_from: frames("throw_from")?,
        throw_rise: number("throw_rise")?,
        knock_rise: number("knock_rise")?,
        knock_hold: frames("knock_hold")?,
        bolt_speed: number("bolt_speed")?,
        bolt_dash: frames("bolt_dash")?,
        bolt_glide: number("bolt_glide")?,
        bolt_response: number("bolt_response")?,
    })
}

/// Reads a collectible's `arrive`, `arrive_view` and `arrive_else`.
/// Left out, there are no rules, the view is the one from the end of the
/// step, and `otherwise` is `count_delay`.
fn parse_arrive(table: &Table, path: &str, count_delay: u8) -> Result<Arrive, String> {
    let delay = |value: &Value, at: &str| match value {
        Value::String(s) if s == "never" => Ok(None),
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(Some(*n as u8)),
        _ => Err(format!(
            "{at}: expected a whole number of frames, 0 to 255, or \"never\""
        )),
    };
    let at_start = match table.get("arrive_view") {
        None => false,
        Some(Value::String(s)) if s == "end" => false,
        Some(Value::String(s)) if s == "start" => true,
        Some(_) => {
            return Err(format!("{path}.arrive_view: expected \"start\" or \"end\""));
        }
    };
    let otherwise = match table.get("arrive_else") {
        None => Some(count_delay),
        Some(v) => delay(v, &format!("{path}.arrive_else"))?,
    };
    let none = Vec::new();
    let items = match table.get("arrive") {
        None => &none,
        Some(Value::Array(items)) if items.len() <= MAX_ARRIVE_RULES => items,
        Some(_) => {
            return Err(format!(
                "{path}.arrive: expected a list of up to {MAX_ARRIVE_RULES} tables"
            ));
        }
    };
    let mut rules = [ArriveRule::default(); MAX_ARRIVE_RULES];
    for (i, item) in items.iter().enumerate() {
        let at = format!("{path}.arrive[{i}]");
        let Value::Table(rule) = item else {
            return Err(format!("{at}: expected a table"));
        };
        const KEYS: &[&str] = &["x_from", "x_to", "y_from", "y_to", "delay"];
        if let Some(key) = rule.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!("{at}.{key}: no such value"));
        }
        let end = |key: &str, open: i32| match rule.get(key) {
            None => Ok(open),
            Some(Value::Integer(n)) if i32::try_from(*n).is_ok() => Ok(*n as i32),
            Some(_) => Err(format!("{at}.{key}: expected a whole number of px")),
        };
        let x = (end("x_from", i32::MIN)?, end("x_to", i32::MAX)?);
        let y = (end("y_from", i32::MIN)?, end("y_to", i32::MAX)?);
        if x.0 > x.1 || y.0 > y.1 {
            return Err(format!("{at}: a range ends before it starts"));
        }
        let Some(d) = rule.get("delay") else {
            return Err(format!("{at}: missing `delay`"));
        };
        rules[i] = ArriveRule {
            x,
            y,
            delay: delay(d, &format!("{at}.delay"))?,
        };
    }
    Ok(Arrive {
        at_start,
        rules,
        rule_count: items.len() as u8,
        otherwise,
    })
}

/// Reads a walker's `response`: a number, or a list of up to
/// [`MAX_RESPONSE_PARTS`], each from 0 to 1 and adding up to no more than
/// 1. Errors start with `path`.
fn parse_response(value: &Value, path: &str) -> Result<Response, String> {
    let one = [value.clone()];
    let items = match value {
        Value::Array(items) if (1..=MAX_RESPONSE_PARTS).contains(&items.len()) => items.as_slice(),
        Value::Array(_) => {
            return Err(format!(
                "{path}: expected a number or a list of 1 to {MAX_RESPONSE_PARTS}"
            ));
        }
        _ => &one,
    };
    let mut response = Response::default();
    let mut total = Fixed::ZERO;
    for (i, item) in items.iter().enumerate() {
        let at = if matches!(value, Value::Array(_)) {
            format!("{path}[{i}]")
        } else {
            path.to_string()
        };
        let part = clamber_pack::read_number(item, "", &at).map_err(|e| e.message)?;
        if part.is_negative() || part > Fixed::ONE {
            return Err(format!("{at}: must be from 0 to 1"));
        }
        response.parts[i] = part;
        total += part;
    }
    if total > Fixed::ONE {
        return Err(format!("{path}: the parts must add up to no more than 1"));
    }
    Ok(response)
}

/// Reads a walker's [`WalkerContact`] table. Errors start with `path`.
fn parse_walker_contact(value: &Value, path: &str) -> Result<WalkerContact, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &[
        "stand",
        "crouch",
        "walk",
        "run",
        "air",
        "roll",
        "turn",
        "land_hold",
        "bounce_rise",
        "bounce_above",
    ];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let whole = |v: &Value, at: &str| match v {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{at}: expected a whole number")),
    };
    let list = |key: &str, len: usize| -> Result<&Vec<Value>, String> {
        match get(key)? {
            Value::Array(items) if items.len() == len => Ok(items),
            _ => Err(format!("{path}.{key}: expected a list of {len}")),
        }
    };
    let offsets = |key: &str| -> Result<Offsets, String> {
        let at = format!("{path}.{key}");
        let n = list(key, 4)?
            .iter()
            .enumerate()
            .map(|(i, v)| whole(v, &format!("{at}[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Offsets {
            across: (n[0], n[1]),
            above: (n[2], n[3]),
        })
    };
    let speed = |key: &str| -> Result<Fixed, String> {
        let at = format!("{path}.{key}");
        let v = clamber_pack::read_number(get(key)?, "", &at).map_err(|e| e.message)?;
        if v.is_negative() {
            return Err(format!("{at}: must not be negative"));
        }
        Ok(v)
    };
    let hold = list("land_hold", 2)?;
    let frames = |i: usize| match &hold[i] {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.land_hold[{i}]: expected a whole number of frames, 0 to 255"
        )),
    };
    Ok(WalkerContact {
        stand: offsets("stand")?,
        crouch: offsets("crouch")?,
        walk: offsets("walk")?,
        run: offsets("run")?,
        air: offsets("air")?,
        roll: match table.get("roll") {
            None => None,
            Some(_) => Some(offsets("roll")?),
        },
        turn: whole(get("turn")?, &format!("{path}.turn"))?,
        land_hold: (frames(0)?, frames(1)?),
        bounce_rise: speed("bounce_rise")?,
        bounce_above: whole(get("bounce_above")?, &format!("{path}.bounce_above"))?,
    })
}

/// Reads a ball's [`Contact`] table. Errors start with `path`.
fn parse_contact(value: &Value, path: &str) -> Result<Contact, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &[
        "upright",
        "airborne",
        "rolling",
        "crouching",
        "fidget",
        "mirror",
        "turn",
        "upright_jump",
        "land_hold",
        "roll_after",
        "fidget_time",
    ];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let whole = |v: &Value, at: &str| match v {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{at}: expected a whole number")),
    };
    let count = |v: &Value, at: &str, most: i64| match v {
        Value::Integer(n) if (0..=most).contains(n) => Ok(*n),
        _ => Err(format!("{at}: expected a whole number, 0 to {most}")),
    };
    let list = |key: &str, len: usize| -> Result<&Vec<Value>, String> {
        match get(key)? {
            Value::Array(items) if items.len() == len => Ok(items),
            _ => Err(format!("{path}.{key}: expected a list of {len}")),
        }
    };
    let offsets = |key: &str| -> Result<Offsets, String> {
        let at = format!("{path}.{key}");
        let n = list(key, 4)?
            .iter()
            .enumerate()
            .map(|(i, v)| whole(v, &format!("{at}[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Offsets {
            across: (n[0], n[1]),
            above: (n[2], n[3]),
        })
    };
    let frames = |v: &Value, at: &str| count(v, at, i64::from(u8::MAX)).map(|n| n as u8);

    let parts = match get("turn")? {
        Value::Array(items) if items.len() <= MAX_TURN_PARTS => items,
        _ => {
            return Err(format!(
                "{path}.turn: expected a list of up to {MAX_TURN_PARTS} [steps, across, above]"
            ));
        }
    };
    let mut turn = [(0, 0, 0); MAX_TURN_PARTS];
    for (i, part) in parts.iter().enumerate() {
        let at = format!("{path}.turn[{i}]");
        let Value::Array(part) = part else {
            return Err(format!("{at}: expected [steps, across, above]"));
        };
        let [steps, x, y] = part.as_slice() else {
            return Err(format!("{at}: expected [steps, across, above]"));
        };
        turn[i] = (
            count(steps, &format!("{at}[0]"), i64::from(u32::MAX))? as u32,
            whole(x, &format!("{at}[1]"))?,
            whole(y, &format!("{at}[2]"))?,
        );
    }

    let Value::Table(hold) = get("land_hold")? else {
        return Err(format!("{path}.land_hold: expected a table"));
    };
    const AIRS: [&str; AIR_KINDS] = ["still", "hop", "moving", "roll", "fall"];
    if let Some(key) = hold.keys().find(|k| !AIRS.contains(&k.as_str())) {
        return Err(format!("{path}.land_hold.{key}: no such value"));
    }
    let mut land_hold = [0; AIR_KINDS];
    for (i, air) in AIRS.iter().enumerate() {
        let at = format!("{path}.land_hold.{air}");
        let v = hold.get(*air).ok_or_else(|| format!("{at}: missing"))?;
        land_hold[i] = frames(v, &at)?;
    }

    let roll_after = list("roll_after", 2)?;
    let fidget_time = list("fidget_time", 3)?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            count(v, &format!("{path}.fidget_time[{i}]"), i64::from(u32::MAX)).map(|n| n as u32)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Contact {
        upright: offsets("upright")?,
        airborne: offsets("airborne")?,
        rolling: offsets("rolling")?,
        crouching: offsets("crouching")?,
        fidget: offsets("fidget")?,
        mirror: whole(get("mirror")?, &format!("{path}.mirror"))?,
        turn,
        turn_parts: parts.len() as u8,
        upright_jump: frames(get("upright_jump")?, &format!("{path}.upright_jump"))?,
        land_hold,
        roll_after: (
            frames(&roll_after[0], &format!("{path}.roll_after[0]"))?,
            frames(&roll_after[1], &format!("{path}.roll_after[1]"))?,
        ),
        fidget_time: (fidget_time[0], fidget_time[1], fidget_time[2]),
    })
}

/// Reads one character's [`Reach`] table. Errors start with `path`.
fn parse_reach(value: &Value, path: &str) -> Result<Reach, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &[
        "stand",
        "walk",
        "crouch",
        "roll",
        "jump",
        "air",
        "air_below",
        "jump_early",
        "land_hold",
    ];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let whole = |v: &Value, at: &str| match v {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{at}: expected a whole number of px")),
    };
    let span = |key: &str| -> Result<Span, String> {
        let at = format!("{path}.{key}");
        let n = match get(key)? {
            Value::Array(items) if items.len() == 4 => items
                .iter()
                .enumerate()
                .map(|(i, v)| whole(v, &format!("{at}[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(format!("{at}: expected [past from, to, below from, to]")),
        };
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Span {
            past: (n[0], n[1]),
            below: (n[2], n[3]),
        })
    };
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let mut air_below = [(Fixed::ZERO, 0); MAX_AIR_STEPS];
    let steps = match get("air_below")? {
        Value::Array(items) if items.len() <= MAX_AIR_STEPS => items,
        _ => {
            return Err(format!(
                "{path}.air_below: expected a list of up to {MAX_AIR_STEPS} [vy, below] pairs"
            ));
        }
    };
    for (i, item) in steps.iter().enumerate() {
        let at = format!("{path}.air_below[{i}]");
        let Value::Array(pair) = item else {
            return Err(format!("{at}: expected [vy, below]"));
        };
        let [vy, below] = pair.as_slice() else {
            return Err(format!("{at}: expected [vy, below]"));
        };
        let vy = clamber_pack::read_number(vy, "", &format!("{at}[0]")).map_err(|e| e.message)?;
        if i > 0 && vy <= air_below[i - 1].0 {
            return Err(format!("{at}: speeds must go up"));
        }
        air_below[i] = (vy, whole(below, &format!("{at}[1]"))?);
    }
    Ok(Reach {
        stand: span("stand")?,
        walk: span("walk")?,
        crouch: span("crouch")?,
        roll: span("roll")?,
        jump: span("jump")?,
        air: span("air")?,
        air_below,
        air_steps: steps.len() as u8,
        jump_early: frames("jump_early")?,
        land_hold: frames("land_hold")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(kind: &str, params: &str) -> Entity {
        Entity {
            kind: kind.to_string(),
            id: None,
            at: Vec2::ints(0, 0),
            params: toml::from_str(params).unwrap(),
        }
    }

    #[test]
    fn builtin_archetypes_cover_enemies_objects_and_collectibles() {
        let a = Archetypes::builtin();
        let resolve = |kind| a.resolve(&entity(kind, ""), "test", "e").unwrap().unwrap();
        assert!(matches!(
            resolve("crawler").behavior,
            Behavior::Walker {
                facing: Facing::Left,
                ..
            }
        ));
        assert!(matches!(
            resolve("flutter").behavior,
            Behavior::Patrol { .. }
        ));
        assert!(matches!(
            resolve("launcher").behavior,
            Behavior::Launcher { .. }
        ));
        assert!(matches!(resolve("crate").behavior, Behavior::Carry { .. }));
        assert!(matches!(
            resolve("thrower").behavior,
            Behavior::Thrower {
                period: 173,
                throw_count: 3,
                ..
            }
        ));
        assert!(matches!(resolve("ball").behavior, Behavior::Ball(_)));
        assert_eq!(a.named("ball"), Some(resolve("ball")));
        assert!(matches!(
            resolve("spark").behavior,
            Behavior::Collect { worth: 1, .. }
        ));
        assert!(matches!(
            resolve("cluster").behavior,
            Behavior::Collect { worth: 10, .. }
        ));
        assert_eq!(a.resolve(&entity("gem", ""), "test", "e"), Ok(None));
    }

    #[test]
    fn params_change_values_for_one_entity() {
        let a = Archetypes::builtin();
        let e = entity("launcher", "velocity = [4, -6.5]\nsize = [8, 8]");
        let got = a.resolve(&e, "test", "e").unwrap().unwrap();
        assert_eq!(
            got.behavior,
            Behavior::Launcher {
                velocity: Vec2::new(Fixed::from_int(4), Fixed::from_ratio_pow2(-13, 1))
            }
        );
        assert_eq!(got.size, Vec2::ints(8, 8));
        assert_eq!(got.bounds(Vec2::ints(10, 10), Facing::Right), {
            let i = Fixed::from_int;
            (i(6), i(6), i(14), i(14))
        });
    }

    #[test]
    fn walkers_can_have_a_range() {
        let a = Archetypes::builtin();
        let range = |params| match a.resolve(&entity("crawler", params), "test", "e") {
            Ok(Some(Archetype {
                behavior: Behavior::Walker { range, .. },
                ..
            })) => range,
            other => panic!("{other:?}"),
        };
        assert_eq!(range(""), None);
        assert_eq!(
            range("range = [-128, 127.5]"),
            Some((Fixed::from_int(-128), Fixed::from_ratio_pow2(255, 1)))
        );
    }

    #[test]
    fn collectibles_can_have_a_window_and_arrival_rules() {
        let a = Archetypes::builtin();
        let collect = |params| match a.resolve(&entity("spark", params), "test", "e") {
            Ok(Some(Archetype {
                behavior: Behavior::Collect { window, arrive, .. },
                ..
            })) => (window, arrive),
            other => panic!("{other:?}"),
        };
        let (window, arrive) = collect("");
        assert_eq!(window, Some((-39, 263)));
        assert!(!arrive.at_start);
        assert_eq!(arrive.otherwise, None);
        assert_eq!(arrive.rule_count, 2);
        assert_eq!(
            arrive.rules[0],
            ArriveRule {
                x: (8, 255),
                y: (i32::MIN, i32::MAX),
                delay: Some(33),
            }
        );

        let (window, arrive) = collect(
            "window = []\narrive = [{ y_from = 4, delay = 2 }]\narrive_else = 0\narrive_view = \"start\"",
        );
        assert_eq!(window, None);
        assert!(arrive.at_start);
        assert_eq!(arrive.otherwise, Some(0));
        assert_eq!(arrive.rule_count, 1);
        assert_eq!(arrive.rules[0].x, (i32::MIN, i32::MAX));
        assert_eq!(arrive.rules[0].y, (4, i32::MAX));
    }

    #[test]
    fn offset_moves_the_box_the_way_it_goes() {
        let a = Archetypes::builtin();
        let e = entity("crawler", "size = [20, 12]\noffset = 0.5\nresponse = 0.125");
        let got = a.resolve(&e, "test", "e").unwrap().unwrap();
        let x = |v: f64| Fixed::from_f64(v).unwrap();
        let at = Vec2::ints(100, 50);
        assert_eq!(
            got.bounds(at, Facing::Right),
            (x(90.5), x(38.0), x(110.5), x(50.0))
        );
        assert_eq!(
            got.bounds(at, Facing::Left),
            (x(89.5), x(38.0), x(109.5), x(50.0))
        );
    }

    #[test]
    fn bad_params_are_reported_with_their_path() {
        let a = Archetypes::builtin();
        let check = |kind, params, want: &str| {
            let e = a
                .resolve(&entity(kind, params), "lvl", "entities[3]")
                .unwrap_err();
            assert_eq!(e.origin, "lvl");
            assert!(e.message.contains(want), "{}", e.message);
        };
        check(
            "crawler",
            "behavior = \"patrol\"",
            "entities[3].params.behavior",
        );
        check("crawler", "path = [1, 2]", "entities[3].params.path");
        check("crawler", "facing = \"up\"", "entities[3].params.facing");
        check("crawler", "speed = -1", "must not be negative");
        check("spark", "window = [5, 4]", "entities[3].params.window");
        check(
            "spark",
            "arrive = [{ x_from = 1 }]",
            "entities[3].params.arrive[0]: missing `delay`",
        );
        check(
            "spark",
            "arrive = [{ x = 1, delay = 0 }]",
            "entities[3].params.arrive[0].x",
        );
        check(
            "spark",
            "arrive_else = \"soon\"",
            "entities[3].params.arrive_else",
        );
        check("spark", "arrive_view = 1", "entities[3].params.arrive_view");
        check("crawler", "response = 1.5", "entities[3].params.response");
        check(
            "crawler",
            "turn_delay = -1",
            "entities[3].params.turn_delay",
        );
        check("crawler", "range = [1]", "expected [] or [left, right]");
        check(
            "crawler",
            "range = [8, -8]",
            "left end is past the right end",
        );
        check("crawler", "defeat_gravity = 0", "must be positive");
        check("flutter", "period = 1", "entities[3].params.period");
        check("launcher", "velocity = [1]", "expected [x, y]");
        check("launcher", "size = [0, 4]", "must be positive");
        check("crate", "reach = -1", "entities[3].params.reach");
        check(
            "thrower",
            "throws = [[173, 0, -7]]",
            "entities[3].params.throws[0][0]",
        );
        check("thrower", "throws = [[2, 0]]", "expected [frame, vx, vy]");
        check("thrower", "period = 0", "entities[3].params.period");
        check("thrower", "defeat_max_fall = 0", "must be positive");
        check("ball", "window = [1, 2, 3]", "entities[3].params.window");
        check(
            "stinger",
            "reach = [0, 0, 0, 256]",
            "entities[3].params.reach",
        );
        check("stinger", "velocity = 1", "entities[3].params.velocity");
        check("ball", "gravity = 0", "must be positive");
        check(
            "ball",
            "contact = { nope = 1 }",
            "contact.nope: no such value",
        );
        check("ball", "contact = 1", "contact: expected a table");
        check(
            "crawler",
            "rider = { attack_across = [[1, 0], [0, 1]] }",
            "rider.attack_across[0]: it ends before it starts",
        );
        check(
            "crawler",
            "rider = { attack_across = [[0, 1]] }",
            "the same facing then the other",
        );
        check(
            "crawler",
            "rider = { nope = 1 }",
            "rider.nope: a rider table has no such value",
        );
        check("spark", "worth = 0", "entities[3].params.worth");
        check("letter", "letter = 0", "entities[3].params.worth");
        check("letter", "letter = 9", "entities[3].params.letter");
        check("letter", "join_delay = -1", "entities[3].params.join_delay");
        check("spark", "reach = []", "expected one or two tables");
        check(
            "spark",
            "reach = [{ stand = [1, 2, 3] }]",
            "entities[3].params.reach[0]",
        );
    }

    #[test]
    fn files_are_checked_when_read() {
        let bad = "[archetypes.x]\nbehavior = \"walker\"\nsize = [1, 1]\noffset = 0\nspeed = 1\nresponse = 0\nturn_delay = 0\nrange = []\ndefeat_jump = 1\ndefeat_gravity = 1\ndefeat_max_fall = 1\n";
        let e = Archetypes::from_toml_str(bad, "f").unwrap_err();
        assert!(e.message.contains("archetypes.x: missing `facing`"), "{e}");
        let unknown = "[archetypes.x]\nbehavior = \"teleporter\"\nsize = [1, 1]\noffset = 0\n";
        assert!(Archetypes::from_toml_str(unknown, "f").is_err());
    }

    /// holder-breaking.md, "The thrower and its balls": the ranges of I
    /// and H, ends included, early and late, with the object moving
    /// right.
    #[test]
    fn holders_break_on_throwers_and_balls_in_the_measured_ranges() {
        let a = Archetypes::builtin();
        let resolve = |params| {
            a.resolve(&entity("crate", params), "test", "e")
                .unwrap()
                .unwrap()
                .behavior
        };
        assert!(matches!(resolve(""), Behavior::Carry { holder: None, .. }));
        let Behavior::Carry {
            holder: Some(holder),
            ..
        } = resolve("holds_character = true")
        else {
            panic!("no holder");
        };
        assert_eq!(holder.window, (31, 32, 31, 32));
        let rows = [
            (holder.thrower_early, (-55, 37), (-47, 65)),
            (holder.thrower, (-30, 36), (-23, 32)),
            (holder.ball_early, (-47, 27), (-52, 50)),
            (holder.ball, (-21, 27), (-25, 20)),
        ];
        let f = |n: i32, frac: i32| Fixed::from_int(n) + Fixed::from_ratio_pow2(frac, 4);
        for (hit, (i_from, i_to), (h_from, h_to)) in rows {
            // The object at x 500.75 and y 300.25 (301 rounded up); a
            // target at I and H from it, with fractions that round away.
            let object = Vec2::new(f(500, 12), f(300, 4));
            let holds = |i: i32, h: i32| {
                let target = Vec2::new(f(500 - i, 3), f(301 + h - 1, 1));
                hit.holds(object, true, target)
            };
            for (i, h, want) in [
                (i_from, 0, true),
                (i_to, 0, true),
                (i_from - 1, 0, false),
                (i_to + 1, 0, false),
                (0, h_from, true),
                (0, h_to, true),
                (0, h_from - 1, false),
                (0, h_to + 1, false),
            ] {
                assert_eq!(holds(i, h), want, "{hit:?} I {i} H {h}");
            }
        }
    }
}
