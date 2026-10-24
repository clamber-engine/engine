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
use crate::pose::{Pose, SecondPose, Stance};

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
    /// of the 1-px column holding its `x`. With `stops_at_x`, it's its
    /// `x` rather than the front of its box that meets walls and ledges.
    /// Its facing follows a turn
    /// `turn_delay` frames later. Bounced on, its speed becomes
    /// `bounced_speed` away from the way the player faces, if it has one.
    /// A `heavy` one isn't bounced on by a player whose preset has a
    /// `bounce_back_rise`: it throws it back and is pushed instead.
    /// Defeated, it falls as `defeat` says. When it appears with the view
    /// (see [`View`](crate::View)), it waits `appear_wait` frames,
    /// counting the one it appears on, before it moves. Walkers with the
    /// same `group` above 0 appear and are removed with the view together.
    /// With a `contact`, it hurts the player or is bounced on as that
    /// says, instead of by the preset's boxes. With a `hurt_jump_lift`,
    /// the preset's hurt box rises that far against it when the
    /// airborne pose lifts it, instead of the preset's `hurt_jump_lift`. A player riding an
    /// animal meets it as `rider` says instead (see [`ride`](crate::ride)).
    /// With a `drop`, it walks off ledges and drops onto the floor from a
    /// start above it, as that says, instead of stopping at ledges. With
    /// a `thrown_lower` `(early, late)`, a thrown object hits it only
    /// down to that many px below its feet, for the object's first
    /// `hit_early_time` frames out of the hands and after them
    /// (thrown-objects.md, "When a walker is hit"); without one, only
    /// the object's `hit` box limits it. Its `pictures` names the kind of pictures it's drawn with, which
    /// doesn't change how it behaves.
    Walker {
        speed: Fixed,
        response: Response,
        move_first: bool,
        columns: bool,
        stops_at_x: bool,
        bounced_speed: Option<Fixed>,
        heavy: bool,
        contact: Option<WalkerContact>,
        hurt_jump_lift: Option<Fixed>,
        rider: RiderReach,
        drop: Option<WalkerDrop>,
        thrown_lower: Option<(Fixed, Fixed)>,
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
    /// (see [`carry`](crate::carry)), from within its [`Grab`].
    /// Thrown, it hits walkers its `hit` box touches, or its `hit_early`
    /// box for its first `hit_early_time` frames out of the hands. It
    /// knocks out throwers its `thrower` box holds, or its
    /// `thrower_early` box early on or in the hands during a throw
    /// ([`Hit::holds`]); moving left, both are turned around and then
    /// moved `thrower_turn` px left. With a `carried_turn`, carried, it
    /// breaks on walkers its `hit` box touches as if thrown right, moved
    /// that many px right with the player facing left. Its place in the
    /// hands during a pick-up, a throw and a put-down is `in_hands`'s.
    /// With `balls`, it meets balls. With a
    /// `holder`, it holds the missing character. With `opens`, it opens
    /// wall exits.
    Carry {
        reach: Grab,
        hit: Hit,
        hit_early: Hit,
        hit_early_time: u8,
        thrower: Hit,
        thrower_early: Hit,
        thrower_turn: Fixed,
        carried_turn: Option<Fixed>,
        in_hands: InHands,
        balls: Option<Meets>,
        holder: Option<Holder>,
        opens: Option<Opens>,
    },
    /// A collectible the player takes from within its [`Reach`] (see
    /// [`collect`](crate::collect)). It adds `worth` to the collectible
    /// count, one at a time: the first on taking it, then one every
    /// `release_every` frames, each `count_delay` frames after that.
    /// With a view, it can only be taken inside its `window`, and
    /// `arrive` decides when each one reaches the count. A `letter`
    /// above 0 is that letter of the level's word: taken, it joins the
    /// letters taken `join_delay` frames later (see
    /// [`Flow::letters`](crate::Flow::letters)). A `relic` adds one to
    /// [`Flow::relics`](crate::Flow::relics) instead. Taken, it gives
    /// `lives` more lives. Above 0, `rise` is how much its speed up grows
    /// each frame, from 0 on the frame it appears; it's gone once it
    /// would reach the level's top. With a view, one that's there `once`
    /// appears with the view and is removed past either edge of the
    /// window; removed, it doesn't appear again until play comes back
    /// after a lost life (with `resume_afresh`) or to the level.
    /// One that's `kept` is gone for good when it's taken: it stays gone
    /// when play comes back to the level from another area. One that's
    /// `late` acts on the lives display as if taken a frame later. Its
    /// `pictures` names how it's drawn, which doesn't change how it
    /// behaves.
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
        /// With a count display, the frames after it's taken that the
        /// display starts waiting on what it's worth.
        pickup_start: u8,
        relic: bool,
        /// Facing left, how far the player is past it is measured from
        /// this many px right of it.
        left_shift: i32,
        lives: u8,
        rise: Fixed,
        once: bool,
        kept: bool,
        late: bool,
        /// With a view, each part released that flies to the count (its
        /// arrival delay above 1) has a copy of its image drawn flying
        /// along the way (see [`FlyingCopy`](crate::FlyingCopy)).
        copy: Option<CopyFlight>,
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
        /// With a view, once knocked out, it's removed outside this
        /// window around the picture as a [`Ball`] is; without one, once
        /// its top is below the level.
        window: Option<(i32, i32, i32, i32)>,
        /// When it hurts the player, is bounced on or is rolled into, if
        /// it does.
        contact: Option<ThrowerContact>,
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
    /// The place the characters come out of when the level begins (see
    /// [`entrance`](crate::entrance)).
    Entrance(Entrance),
    /// A hidden object: not drawn, never in the way and never hurting,
    /// until it's set off and releases its item (see
    /// [`hidden`](crate::hidden)).
    Hidden(Hidden),
    /// A spring the player stands on, which squashes and throws it up
    /// (see [`spring`](crate::spring)).
    Spring(Spring),
    /// A rope that sways, which the player catches from a jump, climbs,
    /// turns round, jumps off and slides off the end of (see
    /// [`rope`](crate::rope)).
    Rope(Rope),
    /// A launch barrel, which catches the player, holds it and shoots it
    /// out (see [`sling`](crate::sling)).
    Sling(Sling),
    /// A critter: scenery that flies about, perching now and then (see
    /// [`critter`](crate::critter)).
    Critter(Critter),
    /// Scenery: it stands where it's placed and nothing touches it. With
    /// a view, it appears and is removed as a thrower does, and comes
    /// back each time its start is near the view again (signs.md). Its
    /// `picture` says which of a kind's pictures it is drawn with, which
    /// doesn't change how it behaves.
    Scenery { picture: u8 },
}

/// A spring (spec/behavior/tyre.md). It never moves. Offsets from it are
/// whole px: `across`, the player's `x` rounded down less its own, and
/// `up`, how far the player's feet line is above its `y` rounded down.
/// Its top is `top` px above its `y` at rest, and as `squash` and
/// `wobble` say after a catch, each a list of `[top, frames]` runs.
///
/// The player touches it with `across` within `across` (the range for
/// its facing) and `up` within `reach` of the top, at the end of a frame
/// after its move. A spring at rest or wobbling catches a player that
/// touches it on the ground, not taking off, or in the air falling with
/// `up` no more than `catch_below` under the top; it puts the player on
/// the top (`up` whole px `stand` above it) and squashes. While squashed
/// it puts a player that touches it and isn't rising back on the top.
/// On the frame after the squash, it throws a player with `across` in
/// range and `up` within `throw_up` up at `throw_speed`, and then
/// catches nothing for `throw_rest` frames. It wobbles until the wobble
/// is over, but not on frames a player touches it without being caught.
///
/// A player on the top that the spring didn't put back goes `drop` down;
/// on the frame after that it leaves the top, `leave_drop` down. A Jump
/// press there makes a spring jump at `jump_speed`, at once, but at the
/// earliest `jump_wait` frames after a catch in the air, and then for a
/// press made up to `jump_early` frames before the catch and held. Spring
/// jumps and throws fall under `gravity` up to `max_fall`, or
/// `jump_gravity` in a spring jump while Jump stays held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Spring {
    pub across: [(i32, i32); 2],
    pub reach: (i32, i32),
    pub top: i32,
    pub stand: i32,
    pub catch_below: i32,
    pub squash: [(i32, u8); MAX_SPRING_RUNS],
    /// Only the first `squash_runs` are used.
    pub squash_runs: u8,
    pub wobble: [(i32, u8); MAX_SPRING_RUNS],
    /// Only the first `wobble_runs` are used.
    pub wobble_runs: u8,
    pub throw_up: (i32, i32),
    pub throw_speed: Fixed,
    pub throw_rest: u8,
    pub drop: Fixed,
    pub leave_drop: Fixed,
    pub jump_speed: Fixed,
    pub jump_wait: u8,
    pub jump_early: u8,
    pub jump_gravity: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
}

/// The most runs a spring's squash or wobble has.
pub const MAX_SPRING_RUNS: usize = 12;

impl Spring {
    /// How long the squash lasts, in frames.
    pub fn squash_frames(&self) -> u32 {
        runs_length(&self.squash[..usize::from(self.squash_runs)])
    }

    /// How long the wobble lasts, in frames.
    pub fn wobble_frames(&self) -> u32 {
        runs_length(&self.wobble[..usize::from(self.wobble_runs)])
    }

    /// The top on the `n`th frame of the squash, counting from 1.
    pub fn squash_top(&self, n: u32) -> i32 {
        run_at(
            &self.squash[..usize::from(self.squash_runs)],
            n.saturating_sub(1),
        )
        .unwrap_or(self.top)
    }

    /// The top `m` frames into the wobble, counting from 0.
    pub fn wobble_top(&self, m: u32) -> i32 {
        run_at(&self.wobble[..usize::from(self.wobble_runs)], m).unwrap_or(self.top)
    }
}

fn runs_length(runs: &[(i32, u8)]) -> u32 {
    runs.iter().map(|&(_, n)| u32::from(n)).sum()
}

/// The value `at` frames into `runs`, if they last that long.
fn run_at(runs: &[(i32, u8)], mut at: u32) -> Option<i32> {
    for &(v, n) in runs {
        if at < u32::from(n) {
            return Some(v);
        }
        at -= u32::from(n);
    }
    None
}

/// A rope (spec/behavior/ropes.md). It hangs from out of sight down to
/// its bottom end, its `y`, and sways around where it's placed: its
/// target speed is `speed` one way or the other, turning once its `x`
/// rounded down is more than `swing` whole px `(left, right)` past the
/// place, and its speed closes `sway_response` of the gap to it each
/// frame.
///
/// It has a player in reach with the feet line no more than `below` px
/// under its bottom end and the player's `x` rounded down within `near`
/// px of its own (less than that), or `far` with the player moving at
/// `slow` or faster. Holding it, the player climbs toward `up` (with
/// Up, or Jump alone) or `down` (with Down), each `[without Action,
/// with Action]`, closing `climb_response` of the gap each frame, but
/// not up past `view_top` px under the view's top. A turn round the rope
/// lasts `turn_frames`, and facing changes on its `turn_facing`th. A
/// jump off starts at `jump_speed` up, plus what one frame of climbing
/// toward the target would add.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rope {
    pub speed: Fixed,
    pub swing: (i32, i32),
    pub sway_response: Fixed,
    pub below: i32,
    pub near: i32,
    pub far: i32,
    pub slow: Fixed,
    pub up: [Fixed; 2],
    pub down: [Fixed; 2],
    pub climb_response: Fixed,
    pub view_top: i32,
    pub turn_frames: u8,
    pub turn_facing: u8,
    pub jump_speed: Fixed,
}

/// A launch barrel (spec/behavior/launch-barrel.md). It never moves. It
/// catches the player within `reach` of it, by what the player is doing
/// at the end of a frame after its move, as a collectible's reach does,
/// measured `left_shift` px right of it facing left. It holds the player
/// for `hold` frames after the catch, then shoots it at `shot` (+y down),
/// breaking if it `breaks`. The player moves at that speed for `glide`
/// frames, and then flies until it lands: its speed across closes
/// `steer_response` of the gap to `steer` toward the direction held, or
/// to 0, rounded toward zero to a 1/256 px per frame, and its speed down
/// grows by `gravity` up to `max_fall`. After a catch it catches no one
/// for `rest` frames. A character other than the first is caught by
/// `second`'s reach and steers toward its `steer` (left out: as the
/// first's), and the first character standing for a `long_stand` has its
/// stand row changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sling {
    pub reach: Reach,
    pub second: Option<SecondSling>,
    pub long_stand: Option<LongStand>,
    pub left_shift: i32,
    pub hold: u8,
    pub shot: Vec2,
    pub glide: u8,
    pub breaks: bool,
    pub rest: u8,
    pub steer: Fixed,
    pub steer_response: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
}

/// What differs for a character other than the first in a launch barrel
/// (launch-barrel.md, "The second character").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondSling {
    pub reach: Reach,
    pub steer: Fixed,
}

/// The first character's standing catch range changes partway through a
/// long stand (launch-barrel.md, "A long stand"): on the steps `from` to
/// `to` into the standing pose it's `span` instead of `reach.stand`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LongStand {
    pub from: u32,
    pub to: u32,
    pub span: Span,
}

/// A critter (spec/behavior/critter.md): scenery that flies towards a
/// target and perches on reaching it. It perches when its `x` rounded
/// down less the target's is within `perch_across` and its drawn `y`
/// rounded up less the target's within `perch_down`, and stays there for
/// `perch` frames, flying again on the last. Its next target is then
/// `hop` from the old one across, at level y `hop` down, and it sets off
/// at `launch` (+y down). Its drawn `y` is its flight height raised by
/// `flutter`, one value a frame in turn, at place (frame + `flutter_from`)
/// of the list on each of the world's frames. A critter there at the
/// world's first frame waits `settle` frames before flying.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Critter {
    pub perch_across: (i32, i32),
    pub perch_down: (i32, i32),
    pub perch: u16,
    pub hop: (i32, i32),
    pub launch: Vec2,
    pub flutter: [u8; MAX_FLUTTER],
    pub flutter_len: u8,
    pub flutter_from: u8,
    pub settle: u8,
}

/// The most values a critter's flutter can have.
pub const MAX_FLUTTER: usize = 16;

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
    /// The `across` and `above` a long fall of a character other than
    /// the first is set off by, if they differ.
    pub second: Option<SecondFall>,
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

/// The ranges a character other than the first is set off by a long
/// fall by: `across` for facing right then left, and `above`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondFall {
    pub across: [(i32, i32); 2],
    pub above: (i32, i32),
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
///
/// The restart point is `restart` px from the keg's position (+y down),
/// [0, 0] if not given.
///
/// Input held on frame 1 can change the resume, if the keg has the
/// values for it. Right or Left held alone moves the player right by
/// `first_shift`'s `right` or `left`, `[without Action, with it]`, until
/// it's in play. Jump held, with `first_jump_lift`, makes it wait that
/// much higher, and it takes off on the frame after the burst, which
/// moves as a frame of the fall with the jump's gravity. Otherwise Down
/// and Action held without Up, with `first_action`, start the ground
/// action, which is on that frame of itself on the frame after the burst
/// and goes on in the air. Otherwise Left held alone, with `first_turn`,
/// starts a turn that is on that frame of itself on the frame after the
/// burst.
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
    pub restart: Vec2,
    /// `[right, left]`, each `[without Action, with it]`.
    pub first_shift: [[Fixed; 2]; 2],
    pub first_jump_lift: Option<Fixed>,
    pub first_action: Option<u8>,
    pub first_turn: Option<u8>,
}

/// A level's entrance (spec/behavior/level-start.md). Counting the
/// level's frames from 0, the characters wait hidden at the start point
/// until frame `throw`, and the entrance bursts on frame `burst` as a keg
/// does, with `pieces`, `piece_gravity`, `flash_rise` and `burst_time`.
/// On frame `throw` it's gone, and the lead is thrown out at
/// `throw_speed`, `[across, up]`, without moving yet. It then moves by
/// that speed for `rise` frames, and after that its speed down first
/// grows by `gravity` each frame, up to `max_fall`. From `steer_from`
/// frames after the throw until it lands, its speed across eases toward
/// the air's target speed by `steer_response` of the gap. The picture
/// fades in from black over the level's first frames, a step every
/// `fade_step` frames up to `fade_steps`. One that doesn't act, because
/// play came in at another, is drawn only if `drawn_idle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Entrance {
    pub burst: u8,
    pub throw: u8,
    pub throw_speed: Vec2,
    pub rise: u8,
    pub gravity: Fixed,
    pub max_fall: Fixed,
    pub steer_from: u8,
    pub steer_response: Fixed,
    pub fade_step: u8,
    pub fade_steps: u8,
    pub pieces: [Vec2; KEG_PIECES],
    pub piece_gravity: Fixed,
    pub flash_rise: Fixed,
    pub burst_time: u8,
    pub drawn_idle: bool,
}

/// How many pieces a keg breaks into.
pub const KEG_PIECES: usize = 4;

/// The most stages a keg's shake can have.
pub const MAX_SHAKE_STAGES: usize = 4;

/// How a stinger sways (spec/behavior/stinger.md). Its turning points
/// are `reach` whole px from its start: `(left, right, up, down)`. It
/// starts at `velocity`, and its target speeds keep that size, changing
/// sign at the turning points. It hurts the player as `contact` says,
/// whose ranges don't move with a turn, or as `second` says when the
/// character under control is not the first (left out: as `contact`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stinger {
    pub reach: (i32, i32, i32, i32),
    pub velocity: Vec2,
    pub contact: Contact,
    pub second: Option<SecondReach>,
}

/// Where a stinger hurts a character other than the first
/// (spec/behavior/stinger.md, "The second character under control"):
/// one range for each pose, "moving" being walking, running, rolling and
/// being in the air. Facing left they turn around as `contact`'s do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondReach {
    pub upright: Offsets,
    pub moving: Offsets,
    pub crouching: Offsets,
    pub fidget: Offsets,
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
    /// The ranges with the second character as the player, when they
    /// differ (spec/behavior/animal-crate.md, "The second character").
    pub second_break: Option<SecondBreak>,
    pub animal_after: u8,
    /// Only the first `hop_steps` are used.
    pub animal_hop: [Fixed; MAX_HOP_STEPS],
    pub hop_steps: u8,
    pub mount_across: [(i32, i32); 2],
    pub mount_above: (i32, i32),
    pub mount_hold: u8,
    pub ride: Ride,
}

/// A crate's break ranges for the second character
/// ([`Hutch::second_break`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondBreak {
    pub across: [(i32, i32); 2],
    pub above: (i32, i32),
}

/// The target speeds of the second character as the player riding
/// ([`Ride::second`]; spec/behavior/riding.md, "The second character").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondRide {
    pub speed: Fixed,
    pub run_speed: Fixed,
    /// The gravity of a hop off while Jump stays held (a jump off has the
    /// character's own `held_jump_gravity`).
    pub hop_gravity: Fixed,
    /// The landing frames after a hop off.
    pub off_landing: u8,
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
    /// Getting off (a Tandem press), the player's speed becomes `off_rise`
    /// up, plus the frame's gravity. After a hop off, `off_landing`
    /// landing frames follow the landing.
    pub off_rise: Fixed,
    pub off_landing: u8,
    /// An animal left above the floor falls: `drop_first` on the first
    /// frame, `drop_second` on the next, and `drop_gravity` more on each
    /// frame than the one before. It's put on the floor once a move would
    /// leave it less than `drop_land` above it.
    pub drop_first: Fixed,
    pub drop_second: Fixed,
    pub drop_gravity: Fixed,
    pub drop_land: Fixed,
    /// The target speeds with the second character as the player (left
    /// out: the first character's).
    pub second: Option<SecondRide>,
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
pub const MAX_THROWS: usize = 8;

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
    pub poses: Poses,
    /// How it reaches a character other than the first, if differently.
    pub second: Option<SecondBall>,
}

/// How a ball reaches a character other than the first under control
/// (spec/behavior/ball-contact.md, "The second character under
/// control"): three ranges instead of five, `moving` being walking,
/// running, rolling and the air, and its own pose timeline. Facing left
/// they turn around by the contact's `mirror`, and the ball's turn moves
/// them as it does the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondBall {
    pub standing: Offsets,
    pub moving: Offsets,
    pub crouching: Offsets,
    /// The moving range goes on up to this many frames after the
    /// landing frame of a jump standing still.
    pub air_hold: u8,
    /// The crouching range goes on for this many frames after Down is
    /// let go.
    pub crouch_hold: u8,
}

/// When the player has which [`Pose`](crate::Pose), for a ball's or a
/// thrower's ranges (spec/behavior/ball-contact.md, "Which pose the
/// player has").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Poses {
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

/// When a thrower hurts the player or is bounced on
/// (spec/behavior/thrower-contact.md), by offsets as a ball's: `across`,
/// the player's `x` less the thrower's, both rounded down, and `above`,
/// the player's `y` less the thrower's, both rounded up.
///
/// When the player's offsets at the end of a step are within the range
/// for its pose then, it's hurt on the next step: `upright`, or
/// `airborne`; crouching, the fidget and the rolling pose take
/// `upright`'s. A player rolling, or in the first `roll_stop_guard`
/// frames of the stop after a roll, isn't hurt. On the ground with `above` at `ledge`, on the
/// thrower's ledge, only `across` counts, and not at the end of the step
/// the player lands on. Not on the step before one of the thrower's
/// throws, though, nor on the one after a bounce.
///
/// A player in the air, not rising at the preset's `bounce_max_rise` or
/// faster, bounces off it with its offsets after its move within
/// `bounce`, which knocks it out.
///
/// With a `roll` range, a rolling player defeats it, which knocks it out,
/// on an attack frame (as for the preset's attack boxes) with its offsets
/// after its move within that range, and on the roll's first frame with
/// its offsets before it too.
///
/// Facing left, each `across` turns around: from `mirror` less its high
/// end to `mirror` less its low end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThrowerContact {
    pub upright: Offsets,
    pub airborne: Offsets,
    pub ledge: i32,
    pub bounce: Offsets,
    pub roll: Option<Offsets>,
    pub mirror: i32,
    pub poses: Poses,
    /// The `across` range of a crouching first character, and of one in
    /// the fidget pose, if not upright's (thrower-contact.md, "Crouching
    /// and the fidget pose"); `above` is upright's.
    pub crouching: Option<(i32, i32)>,
    pub fidget: Option<(i32, i32)>,
    /// The crouching range goes on for this many frames after Down is
    /// let go.
    pub crouch_hold: u8,
    /// How it reaches a character other than the first, if differently.
    pub second: Option<SecondThrower>,
}

/// How a thrower reaches a character other than the first under control
/// (spec/behavior/thrower-contact.md, "The second character under
/// control"): two hurt ranges, `standing` for standing still on the
/// ground and `moving` for everything else, and its own `bounce`. Facing
/// left they turn around by the contact's `mirror`. The ledge rule, the
/// guards and the knock-out are the first character's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SecondThrower {
    pub standing: Offsets,
    pub moving: Offsets,
    pub bounce: Offsets,
    /// The moving range goes on up to this many frames after the
    /// landing frame of a jump standing still.
    pub air_hold: u8,
    /// The `across` range while crouching, if not `standing`'s; `above`
    /// is `standing`'s.
    pub crouching: Option<(i32, i32)>,
    /// The `across` range in which its roll defeats the thrower, if not
    /// the contact's `roll`; `above` is the contact's.
    pub roll: Option<(i32, i32)>,
    /// The crouching range goes on for this many frames after Down is
    /// let go.
    pub crouch_hold: u8,
}

/// Reads an optional `[from, to]` list under `key` in `table`.
fn parse_across(table: &Table, key: &str, path: &str) -> Result<Option<(i32, i32)>, String> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };
    let at = format!("{path}.{key}");
    let n = match value {
        Value::Array(items) if items.len() == 2 => items
            .iter()
            .map(|v| match v {
                Value::Integer(n) if i32::try_from(*n).is_ok() => Some(*n as i32),
                _ => None,
            })
            .collect::<Option<Vec<_>>>(),
        _ => None,
    }
    .ok_or_else(|| format!("{at}: expected a list of 2 whole numbers"))?;
    if n[0] > n[1] {
        return Err(format!("{at}: a range ends before it starts"));
    }
    Ok(Some((n[0], n[1])))
}

/// Reads an optional `crouch_hold` in `table`: 0 if missing.
fn parse_crouch_hold(table: &Table, path: &str) -> Result<u8, String> {
    match table.get("crouch_hold") {
        None => Ok(0),
        Some(Value::Integer(n)) if (0..=254).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.crouch_hold: expected a whole number, 0 to 254"
        )),
    }
}

impl ThrowerContact {
    /// `offsets`, for facing right, for a player facing `facing`.
    pub fn facing(&self, offsets: Offsets, facing: Facing) -> Offsets {
        let (from, to) = offsets.across;
        match facing {
            Facing::Right => offsets,
            Facing::Left => Offsets {
                across: (self.mirror - to, self.mirror - from),
                above: offsets.above,
            },
        }
    }
}

/// The most parts a ball's turn can have.
pub const MAX_TURN_PARTS: usize = 8;

/// How many ways into the air [`Poses::land_hold`] tells apart.
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
    /// Which part of its turn, from 0, a ball is in after `steps` steps;
    /// `None` without parts.
    pub fn part(&self, steps: u32) -> Option<usize> {
        let parts = &self.turn[..usize::from(self.turn_parts)];
        let cycle: u32 = parts.iter().map(|p| p.0).sum();
        let mut at = if cycle > 0 { steps % cycle } else { 0 };
        for (n, &(length, ..)) in parts.iter().enumerate() {
            if at < length {
                return Some(n);
            }
            at -= length;
        }
        None
    }

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
        self.place(base, facing, steps)
    }

    /// The offsets a ball reaches a character other than the first from,
    /// when `second` says how, in `pose`.
    pub fn reach_second(
        &self,
        second: &SecondBall,
        pose: SecondPose,
        facing: Facing,
        steps: u32,
    ) -> Offsets {
        let base = match pose {
            SecondPose::Standing => second.standing,
            SecondPose::Moving => second.moving,
            SecondPose::Crouching => second.crouching,
        };
        self.place(base, facing, steps)
    }

    /// `base` turned for `facing` and moved by the turn after `steps`.
    fn place(&self, base: Offsets, facing: Facing, steps: u32) -> Offsets {
        let (from, to) = base.across;
        let across = match facing {
            Facing::Right => (from, to),
            Facing::Left => (self.mirror - to, self.mirror - from),
        };
        let shift = self
            .part(steps)
            .map_or((0, 0), |n| (self.turn[n].1, self.turn[n].2));
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

/// How a collectible's released parts show a copy of its image flying
/// to the count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CopyFlight {
    /// The first step after the release it's drawn on: 0 or 1.
    pub from: u8,
    /// How far left of and above the collectible's position the image's
    /// top-left corner is.
    pub corner: (i32, i32),
}

fn parse_copy(table: &Table, path: &str) -> Result<CopyFlight, String> {
    if let Some(key) = table
        .keys()
        .find(|k| !matches!(k.as_str(), "from" | "corner"))
    {
        return Err(format!("{path}.{key}: no such value"));
    }
    let from = match table.get("from") {
        Some(Value::Integer(n @ (0 | 1))) => *n as u8,
        _ => return Err(format!("{path}.from: expected 0 or 1")),
    };
    let corner = match table.get("corner") {
        Some(Value::Array(v)) => match v.as_slice() {
            [Value::Integer(x), Value::Integer(y)]
                if i32::try_from(*x).is_ok() && i32::try_from(*y).is_ok() =>
            {
                (*x as i32, *y as i32)
            }
            _ => return Err(format!("{path}.corner: expected [x, y] in whole px")),
        },
        _ => return Err(format!("{path}.corner: expected [x, y] in whole px")),
    };
    Ok(CopyFlight { from, corner })
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
    /// `land_hold` frames after landing (`land_ran` with Action and a
    /// direction held on the landing frame), counting the landing frame,
    /// and `land_drop`'s frames more after landing that far below where
    /// the player went into the air.
    pub air: Span,
    /// In the air after `jump`, the lowest `below` while rising: the
    /// first `(vy, below)` whose `vy` is at least the player's vertical
    /// speed gives it. Past them all, it's `air`'s. Only the first
    /// `air_steps` are used.
    pub air_below: [(Fixed, i32); MAX_AIR_STEPS],
    pub air_steps: u8,
    pub jump_early: u8,
    pub land_hold: u8,
    pub land_ran: u8,
    /// More than this many px lower, so many frames more. `None`: no
    /// landing is longer.
    pub land_drop: Option<(i32, u8)>,
    /// A landing after a jump Jump was held for a number of frames in
    /// `from..=to` is `more` frames longer, for the first such row; the
    /// rows past `land_held_rows` are not used.
    pub land_held: [(u8, u8, u8); MAX_LAND_HELD],
    pub land_held_rows: u8,
    /// `roll` goes on for so many frames after a roll ends, counting the
    /// first one after it: when it runs on into a run, then when it
    /// doesn't.
    pub roll_after: (u8, u8),
    /// A fall after walking off a ledge without a face, and the frames
    /// after its landing, have the ranges on the ground instead of `air`.
    pub upright_fall: bool,
    /// From a bounce's frame until it lands, but none on the frames the
    /// player holds still after it, and the ranges on the ground from
    /// its landing. `None`: a bounce is in the air as any other time.
    pub bounce: Option<Span>,
    /// From a pick-up to the end of a throw, even one a jump or a roll
    /// ends sooner, or to the frame before a put-down sets the object
    /// down, whatever else the player does. `None`: carrying changes
    /// nothing.
    pub carry: Option<Span>,
}

/// The most `air_below` steps a [`Reach`] can have.
pub const MAX_AIR_STEPS: usize = 8;

/// How many rows of [`Reach::land_held`] there can be.
pub const MAX_LAND_HELD: usize = 4;

/// Offsets from a collectible a player takes it from, in whole px, ends
/// included: how far the player is past it the way the player faces,
/// and how far the player's feet are below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub past: (i32, i32),
    pub below: (i32, i32),
}

impl Span {
    /// Takes nothing.
    pub const NONE: Span = Span {
        past: (1, 0),
        below: (1, 0),
    };

    pub fn contains(self, past: i32, below: i32) -> bool {
        (self.past.0..=self.past.1).contains(&past)
            && (self.below.0..=self.below.1).contains(&below)
    }
}

/// How a walker leaves the floor at a ledge, falls and lands
/// (walker.md, "Walking off a ledge", and walker-view.md, "A start above
/// the floor"). It goes on moving across as on the floor. On the frame it
/// leaves the floor it moves down by `leave` and ends with `leave_speed`
/// as its speed down; a walker that starts above the floor starts with
/// none. On each later frame in the air its speed down grows by
/// `gravity`, up to `max_fall`, and it moves down by it. It lands once
/// that leaves its feet less than `land` above the floor of the column
/// holding its `x`, or at or below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WalkerDrop {
    pub leave: Fixed,
    pub leave_speed: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
    pub land: Fixed,
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

/// The most frames of an [`InHands`] path.
pub const MAX_IN_HANDS: usize = 24;

/// Where an object is in the player's hands on each frame of a pick-up
/// from its second frame, and of a throw and a put-down from their first:
/// `(ahead, up)`, its `x` rounded down that many px in front of the
/// player's (the way the player faces) and its `y` rounded up that many
/// above the player's. On the first frame of a pick-up it's where it
/// lies, and past the end of a path, or with none, it's on the player's
/// head.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InHands {
    pub pick_up: HandsPath,
    pub throw: HandsPath,
    pub put_down: HandsPath,
    /// The paths with the second character, if it has its own.
    pub second: Option<SecondHands>,
}

/// The paths in the hands of the second character under control
/// ([`InHands::second`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SecondHands {
    pub pick_up: HandsPath,
    pub throw: HandsPath,
    pub put_down: HandsPath,
}

/// One action's path in [`InHands`]: its first `len` places.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HandsPath {
    pub places: [(i16, i16); MAX_IN_HANDS],
    pub len: u8,
}

impl HandsPath {
    /// The place on frame `n` of the path, if it reaches that far.
    pub fn at(&self, n: usize) -> Option<(i16, i16)> {
        self.places[..usize::from(self.len)].get(n).copied()
    }
}

/// When an object holding nothing meets a ball (thrown-objects.md,
/// "Hitting a ball"): thrown, or in the hands during a throw, its `early`
/// box holds where the ball was at the end of the step before, for its
/// first `hit_early_time` frames out of the hands; later, or carried, its
/// `hit` box does ([`Hit::holds`]), the way it was thrown or the player
/// faces. Moving left, the box is turned around and then moved `turn` px
/// left. While the ball is in the nth part of its turn
/// ([`Contact::part`]), the box is moved by the nth of `parts` (none
/// past the last): `across` px left and `above` px down, so that the
/// ranges of the object's `x` less the ball's and of the ball's `y` less
/// the object's move by that much. Thrown, it removes the ball and goes
/// on; in the hands, it breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Meets {
    pub hit: Hit,
    pub early: Hit,
    pub turn: Fixed,
    pub parts: [(i32, i32); MAX_TURN_PARTS],
}

impl Meets {
    /// Whether an object at `pos`, moving right if `right`, early on if
    /// `early`, meets a ball that was at `ball` in the `part` of its turn.
    pub fn holds(
        &self,
        pos: Vec2,
        right: bool,
        early: bool,
        ball: Vec2,
        part: Option<usize>,
    ) -> bool {
        let (across, above) = part.map_or((0, 0), |n| self.parts[n]);
        let turn = if right { Fixed::ZERO } else { self.turn };
        let at = Vec2::new(
            pos.x - turn - Fixed::from_int(across),
            pos.y + Fixed::from_int(above),
        );
        let hit = if early { self.early } else { self.hit };
        hit.holds(at, right, ball)
    }
}

/// What makes an object hold the missing character: it frees it when it
/// breaks (see [`carry`](crate::carry)). Thrown, it also breaks outside
/// `window` around the picture, as a [`Ball`] is removed, and on throwers
/// its [`Behavior::Carry`] boxes hold and balls its `ball` box holds, or
/// their early boxes for its first `hit_early_time` frames out of the
/// hands ([`Hit::holds`]).
///
/// Lying where it's placed, it shakes in a cycle of `look` frames
/// counted from the one it appeared on, looking still on the frames of
/// the cycle within either of `still` (ends included). While it shakes,
/// the player picks it up from within `shaking` rather than the
/// object's `reach`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Holder {
    pub window: (i32, i32, i32, i32),
    pub ball: Hit,
    pub ball_early: Hit,
    pub shaking: Grab,
    pub look: u32,
    pub still: [(i32, i32); 2],
}

impl Holder {
    /// Whether it looks still `age` frames after it appeared.
    pub fn still_at(&self, age: u32) -> bool {
        let frame = (age % self.look.max(1)) as i32;
        self.still
            .iter()
            .any(|&(from, to)| (from..=to).contains(&frame))
    }
}

/// Where a player can pick an object up from, in whole px, ends
/// included: the object's `x` rounded down less the player's, `right`
/// for a player facing right and `left` for one facing left, and the
/// object's `y` rounded up less the player's, `below`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Grab {
    pub right: (i32, i32),
    pub left: (i32, i32),
    pub below: (i32, i32),
}

impl Grab {
    /// Whether an object `across` and `below` the player, as above, is
    /// within it for a player facing right if `right`.
    pub fn holds(&self, right: bool, across: i64, below: i64) -> bool {
        let inside =
            |(from, to): (i32, i32), v: i64| (i64::from(from)..=i64::from(to)).contains(&v);
        inside(if right { self.right } else { self.left }, across) && inside(self.below, below)
    }
}

/// What makes a thrown object open wall exits (see
/// [`exit`](crate::exit)): moving right, it opens one its `hit` box holds
/// at the end of a frame, or its `hit_early` box for its first
/// `hit_early_time` frames out of the hands ([`Hit::holds`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Opens {
    pub hit: Hit,
    pub hit_early: Hit,
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
            "stops_at_x",
            "bounced_speed",
            "heavy",
            "contact",
            "hurt_jump_lift",
            "rider",
            "drop",
            "thrown_lower",
            "pictures",
        ],
        "patrol" => &["path", "period"],
        "scenery" => &["picture"],
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
            "hit_thrower_turn",
            "hit_carried_turn",
            "in_hands_pick_up",
            "in_hands_throw",
            "in_hands_put_down",
            "in_hands_second_pick_up",
            "in_hands_second_throw",
            "in_hands_second_put_down",
            "hit_ball",
            "hit_ball_early",
            "meets_ball",
            "meets_ball_early",
            "meets_ball_turn",
            "meets_ball_parts",
            "holder_reach",
            "holder_look",
            "holder_still",
            "hit_wall_exit",
            "hit_wall_exit_early",
        ],
        "thrower" => &[
            "period",
            "throws",
            "throw_from",
            "defeat_jump",
            "defeat_gravity",
            "defeat_max_fall",
            "window",
            "contact",
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
            "second_break_across",
            "second_break_above",
            "animal_after",
            "animal_hop",
            "mount_across",
            "mount_above",
            "mount_hold",
            "ride",
        ],
        "stinger" => &["reach", "velocity", "contact", "second"],
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
            "restart",
            "first_shift",
            "first_jump_lift",
            "first_action",
            "first_turn",
        ],
        "entrance" => &[
            "burst",
            "throw",
            "throw_speed",
            "rise",
            "gravity",
            "max_fall",
            "steer_from",
            "steer_response",
            "fade_step",
            "fade_steps",
            "pieces",
            "piece_gravity",
            "flash_rise",
            "burst_time",
            "drawn_idle",
        ],
        "hidden" => &[
            "across",
            "above",
            "second_across",
            "second_above",
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
        "spring" => &[
            "across",
            "reach",
            "top",
            "stand",
            "catch_below",
            "squash",
            "wobble",
            "throw_up",
            "throw_speed",
            "throw_rest",
            "drop",
            "leave_drop",
            "jump_speed",
            "jump_wait",
            "jump_early",
            "jump_gravity",
            "gravity",
            "max_fall",
        ],
        "rope" => &[
            "speed",
            "swing",
            "sway_response",
            "below",
            "near",
            "far",
            "slow",
            "up",
            "down",
            "climb_response",
            "view_top",
            "turn_frames",
            "turn_facing",
            "jump_speed",
        ],
        "sling" => &[
            "reach",
            "second",
            "long_stand",
            "left_shift",
            "hold",
            "shot",
            "glide",
            "breaks",
            "rest",
            "steer",
            "steer_response",
            "gravity",
            "max_fall",
        ],
        "critter" => &[
            "perch_across",
            "perch_down",
            "perch",
            "hop",
            "launch",
            "flutter",
            "flutter_from",
            "settle",
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
            "pickup_start",
            "relic",
            "left_shift",
            "lives",
            "rise",
            "once",
            "kept",
            "late",
            "copy",
            "pictures",
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
            let thrown_lower = match table.get("thrown_lower") {
                None => None,
                Some(_) => {
                    let v = pair("thrown_lower")?;
                    if v.x.is_negative() || v.y.is_negative() {
                        return Err(format!("{path}.thrown_lower: must not be negative"));
                    }
                    Some((v.x, v.y))
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
                stops_at_x: flag("stops_at_x")?,
                bounced_speed,
                heavy: flag("heavy")?,
                contact: match table.get("contact") {
                    None => None,
                    Some(v) => Some(parse_walker_contact(v, &format!("{path}.contact"))?),
                },
                hurt_jump_lift,
                rider: parse_rider(get("rider")?, &format!("{path}.rider"))?,
                drop: match table.get("drop") {
                    None => None,
                    Some(v) => Some(parse_walker_drop(v, &format!("{path}.drop"))?),
                },
                thrown_lower,
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
            let grab = |key: &str| parse_grab(get(key)?, &format!("{path}.{key}"));
            let reach = grab("reach")?;
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
            let thrower_turn = match table.contains_key("hit_thrower_turn") {
                true => number("hit_thrower_turn")?,
                false => Fixed::ZERO,
            };
            let carried_turn = match table.contains_key("hit_carried_turn") {
                true => Some(number("hit_carried_turn")?),
                false => None,
            };
            let hands_path = |key: &str| match table.get(key) {
                None => Ok(HandsPath::default()),
                Some(value) => parse_path(value, &format!("{path}.{key}")),
            };
            let in_hands = InHands {
                pick_up: hands_path("in_hands_pick_up")?,
                throw: hands_path("in_hands_throw")?,
                put_down: hands_path("in_hands_put_down")?,
                second: match table.contains_key("in_hands_second_throw") {
                    false => None,
                    true => Some(SecondHands {
                        pick_up: hands_path("in_hands_second_pick_up")?,
                        throw: hands_path("in_hands_second_throw")?,
                        put_down: hands_path("in_hands_second_put_down")?,
                    }),
                },
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
                    ball: hit("hit_ball")?,
                    ball_early: hit("hit_ball_early")?,
                    shaking: grab("holder_reach")?,
                    look: match get("holder_look")? {
                        Value::Integer(n) if (1..=i64::from(u32::MAX)).contains(n) => *n as u32,
                        _ => {
                            return Err(format!(
                                "{path}.holder_look: expected a whole number of frames, at least 1"
                            ));
                        }
                    },
                    still: parse_ranges(
                        get("holder_still")?,
                        &format!("{path}.holder_still"),
                        "frames of the cycle",
                    )?,
                }),
                Some(_) => return Err(format!("{path}.holds_character: expected true or false")),
            };
            let balls = match table.contains_key("meets_ball") {
                false => None,
                true => Some(Meets {
                    hit: hit("meets_ball")?,
                    early: hit("meets_ball_early")?,
                    turn: match table.contains_key("meets_ball_turn") {
                        true => number("meets_ball_turn")?,
                        false => Fixed::ZERO,
                    },
                    parts: match table.get("meets_ball_parts") {
                        None => [(0, 0); MAX_TURN_PARTS],
                        Some(value) => parse_shifts(value, &format!("{path}.meets_ball_parts"))?,
                    },
                }),
            };
            let opens = match (
                table.contains_key("hit_wall_exit"),
                table.contains_key("hit_wall_exit_early"),
            ) {
                (false, false) => None,
                _ => Some(Opens {
                    hit: hit("hit_wall_exit")?,
                    hit_early: hit("hit_wall_exit_early")?,
                }),
            };
            Behavior::Carry {
                reach,
                hit: hit("hit")?,
                hit_early: hit("hit_early")?,
                hit_early_time,
                thrower: hit("hit_thrower")?,
                thrower_early: hit("hit_thrower_early")?,
                thrower_turn,
                carried_turn,
                in_hands,
                balls,
                holder,
                opens,
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
            let relic = match table.get("relic") {
                None | Some(Value::Boolean(false)) => false,
                Some(Value::Boolean(true)) => true,
                Some(_) => return Err(format!("{path}.relic: expected true or false")),
            };
            let lives = optional("lives", u8::MAX)?;
            if worth == 0 && letter == 0 && !relic && lives == 0 {
                return Err(format!(
                    "{path}.worth: must be at least 1, unless it's a `letter`, a `relic` \
                     or gives `lives`"
                ));
            }
            let rise = match table.get("rise") {
                None => Fixed::ZERO,
                Some(_) => number("rise")?,
            };
            if rise < Fixed::ZERO {
                return Err(format!("{path}.rise: expected 0 or more px per frame"));
            }
            let once = match table.get("once") {
                None | Some(Value::Boolean(false)) => false,
                Some(Value::Boolean(true)) => true,
                Some(_) => return Err(format!("{path}.once: expected true or false")),
            };
            let kept = match table.get("kept") {
                None | Some(Value::Boolean(false)) => false,
                Some(Value::Boolean(true)) => true,
                Some(_) => return Err(format!("{path}.kept: expected true or false")),
            };
            let late = match table.get("late") {
                None | Some(Value::Boolean(false)) => false,
                Some(Value::Boolean(true)) => true,
                Some(_) => return Err(format!("{path}.late: expected true or false")),
            };
            let copy = match table.get("copy") {
                None => None,
                Some(Value::Table(t)) => Some(parse_copy(t, &format!("{path}.copy"))?),
                Some(_) => return Err(format!("{path}.copy: expected a table")),
            };
            let left_shift = match table.get("left_shift") {
                None => 0,
                Some(Value::Integer(n)) if i32::try_from(*n).is_ok() => *n as i32,
                Some(_) => return Err(format!("{path}.left_shift: expected a whole number of px")),
            };
            Behavior::Collect {
                worth,
                release_every: small("release_every", 0)?,
                count_delay,
                reach,
                window,
                arrive: parse_arrive(table, path, count_delay)?,
                letter,
                join_delay: optional("join_delay", u8::MAX)?,
                pickup_start: optional("pickup_start", u8::MAX)?,
                relic,
                left_shift,
                lives,
                rise,
                once,
                kept,
                late,
                copy,
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
                window: match table.get("window") {
                    None => None,
                    Some(v) => Some(edges(v, &format!("{path}.window"))?),
                },
                contact: match table.get("contact") {
                    None => None,
                    Some(v) => Some(parse_thrower_contact(v, &format!("{path}.contact"))?),
                },
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
                second: match table.get("second") {
                    None => None,
                    Some(v) => Some(parse_second_reach(v, &format!("{path}.second"))?),
                },
            })
        }
        "keg" => Behavior::Keg(parse_keg(table, path)?),
        "entrance" => Behavior::Entrance(parse_entrance(table, path)?),
        "hidden" => Behavior::Hidden(parse_hidden(table, path)?),
        "spring" => Behavior::Spring(parse_spring(table, path)?),
        "rope" => Behavior::Rope(parse_rope(table, path)?),
        "sling" => Behavior::Sling(parse_sling(table, path)?),
        "critter" => Behavior::Critter(parse_critter(table, path)?),
        "scenery" => Behavior::Scenery {
            picture: match table.get("picture") {
                None => 0,
                Some(Value::Integer(n)) if (0..=255).contains(n) => *n as u8,
                Some(_) => return Err(format!("{path}.picture: expected a whole number 0 to 255")),
            },
        },
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
    let pieces = parse_pieces(get("pieces")?, &format!("{path}.pieces"))?;
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
    let restart = match table.get("restart") {
        None => Vec2::default(),
        Some(Value::Array(xy)) if xy.len() == 2 => {
            let at = format!("{path}.restart");
            Vec2::new(
                number(&xy[0], &format!("{at}[0]"))?,
                number(&xy[1], &format!("{at}[1]"))?,
            )
        }
        Some(_) => return Err(format!("{path}.restart: expected [x, y]")),
    };
    let mut first_shift = [[Fixed::ZERO; 2]; 2];
    if let Some(shift) = table.get("first_shift") {
        let at = format!("{path}.first_shift");
        let Value::Table(shift) = shift else {
            return Err(format!(
                "{at}: expected {{ right = [a, b], left = [a, b] }}"
            ));
        };
        for (i, side) in ["right", "left"].into_iter().enumerate() {
            let at = format!("{at}.{side}");
            match shift.get(side) {
                Some(Value::Array(ab)) if ab.len() == 2 => {
                    for (j, v) in ab.iter().enumerate() {
                        first_shift[i][j] = number(v, &format!("{at}[{j}]"))?;
                    }
                }
                _ => return Err(format!("{at}: expected [without Action, with it]")),
            }
        }
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
        restart,
        first_shift,
        first_jump_lift: match table.get("first_jump_lift") {
            None => None,
            Some(v) => Some(number(v, &format!("{path}.first_jump_lift"))?),
        },
        first_action: match table.get("first_action") {
            None => None,
            Some(_) => Some(frames("first_action")?.max(1)),
        },
        first_turn: match table.get("first_turn") {
            None => None,
            Some(_) => Some(frames("first_turn")?.max(1)),
        },
    })
}

/// Reads an entrance's values. Errors start with `path`.
fn parse_entrance(table: &Table, path: &str) -> Result<Entrance, String> {
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
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let at = format!("{path}.throw_speed");
    let throw_speed = match get("throw_speed")? {
        Value::Array(xy) if xy.len() == 2 => {
            let n = |i: usize| {
                clamber_pack::read_number(&xy[i], "", &format!("{at}[{i}]")).map_err(|e| e.message)
            };
            Vec2::new(n(0)?, n(1)?)
        }
        _ => return Err(format!("{at}: expected [across, up]")),
    };
    let entrance = Entrance {
        burst: frames("burst")?,
        throw: frames("throw")?,
        throw_speed,
        rise: frames("rise")?,
        gravity: positive("gravity")?,
        max_fall: positive("max_fall")?,
        steer_from: frames("steer_from")?,
        steer_response: positive("steer_response")?,
        fade_step: frames("fade_step")?.max(1),
        fade_steps: frames("fade_steps")?,
        pieces: parse_pieces(get("pieces")?, &format!("{path}.pieces"))?,
        piece_gravity: positive("piece_gravity")?,
        flash_rise: number("flash_rise")?,
        burst_time: frames("burst_time")?,
        drawn_idle: match table.get("drawn_idle") {
            None | Some(Value::Boolean(false)) => false,
            Some(Value::Boolean(true)) => true,
            Some(_) => return Err(format!("{path}.drawn_idle: expected true or false")),
        },
    };
    if entrance.burst > entrance.throw {
        return Err(format!("{path}: `burst` can't be after `throw`"));
    }
    if entrance.steer_response > Fixed::ONE {
        return Err(format!("{path}.steer_response: must be at most 1"));
    }
    Ok(entrance)
}

/// Reads the pieces a keg or an entrance bursts into, each
/// `[across, up]`. Errors start with `at`.
fn parse_pieces(value: &Value, at: &str) -> Result<[Vec2; KEG_PIECES], String> {
    let number = |v: &Value, at: &str| -> Result<Fixed, String> {
        clamber_pack::read_number(v, "", at).map_err(|e| e.message)
    };
    let mut pieces = [Vec2::default(); KEG_PIECES];
    match value {
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
    Ok(pieces)
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
        second: match (table.get("second_across"), table.get("second_above")) {
            (None, None) => None,
            (Some(across), Some(above)) => Some(SecondFall {
                across: parse_ranges(across, &format!("{path}.second_across"), "right then left")?,
                above: parse_range(above, &format!("{path}.second_above"))?,
            }),
            _ => {
                return Err(format!(
                    "{path}: `second_across` and `second_above` go together"
                ));
            }
        },
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

/// Reads a spring's values. Errors start with `path`.
fn parse_spring(table: &Table, path: &str) -> Result<Spring, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let number = |key: &str| -> Result<Fixed, String> {
        let v = clamber_pack::read_number(get(key)?, "", &format!("{path}.{key}"))
            .map_err(|e| e.message)?;
        if v.is_negative() {
            return Err(format!("{path}.{key}: must not be negative"));
        }
        Ok(v)
    };
    let px = |key: &str| match get(key)? {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{path}.{key}: expected a whole number of px")),
    };
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let runs = |key: &str| -> Result<([(i32, u8); MAX_SPRING_RUNS], u8), String> {
        let at = format!("{path}.{key}");
        let items = match get(key)? {
            Value::Array(items) if items.len() <= MAX_SPRING_RUNS => items,
            _ => {
                return Err(format!(
                    "{at}: expected a list of up to {MAX_SPRING_RUNS} [top, frames]"
                ));
            }
        };
        let mut runs = [(0, 0); MAX_SPRING_RUNS];
        for (i, item) in items.iter().enumerate() {
            let parts = match item {
                Value::Array(parts) => parts.as_slice(),
                _ => &[],
            };
            let [Value::Integer(top), Value::Integer(n)] = parts else {
                return Err(format!("{at}[{i}]: expected [top, frames], whole numbers"));
            };
            let (Ok(top), Ok(n)) = (i32::try_from(*top), u8::try_from(*n)) else {
                return Err(format!("{at}[{i}]: out of range"));
            };
            runs[i] = (top, n);
        }
        Ok((runs, items.len() as u8))
    };
    let (squash, squash_runs) = runs("squash")?;
    let (wobble, wobble_runs) = runs("wobble")?;
    Ok(Spring {
        across: parse_ranges(get("across")?, &format!("{path}.across"), "right then left")?,
        reach: parse_range(get("reach")?, &format!("{path}.reach"))?,
        top: px("top")?,
        stand: px("stand")?,
        catch_below: px("catch_below")?,
        squash,
        squash_runs,
        wobble,
        wobble_runs,
        throw_up: parse_range(get("throw_up")?, &format!("{path}.throw_up"))?,
        throw_speed: number("throw_speed")?,
        throw_rest: frames("throw_rest")?,
        drop: number("drop")?,
        leave_drop: number("leave_drop")?,
        jump_speed: number("jump_speed")?,
        jump_wait: frames("jump_wait")?,
        jump_early: frames("jump_early")?,
        jump_gravity: number("jump_gravity")?,
        gravity: number("gravity")?,
        max_fall: number("max_fall")?,
    })
}

/// Reads a launch barrel's values. Errors start with `path`.
fn parse_sling(table: &Table, path: &str) -> Result<Sling, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let read = |v: &Value, at: &str| -> Result<Fixed, String> {
        clamber_pack::read_number(v, "", at).map_err(|e| e.message)
    };
    let positive = |key: &str| {
        let at = format!("{path}.{key}");
        let v = read(get(key)?, &at)?;
        if v <= Fixed::ZERO {
            return Err(format!("{at}: must be positive"));
        }
        Ok(v)
    };
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let at = format!("{path}.shot");
    let shot = match get("shot")? {
        Value::Array(items) if items.len() == 2 => Vec2::new(
            read(&items[0], &format!("{at}[0]"))?,
            read(&items[1], &format!("{at}[1]"))?,
        ),
        _ => return Err(format!("{at}: expected [x, y]")),
    };
    let left_shift = match get("left_shift")? {
        Value::Integer(n) if i32::try_from(*n).is_ok() => *n as i32,
        _ => return Err(format!("{path}.left_shift: expected a whole number of px")),
    };
    let breaks = match get("breaks")? {
        Value::Boolean(b) => *b,
        _ => return Err(format!("{path}.breaks: expected true or false")),
    };
    let steer_response = positive("steer_response")?;
    if steer_response > Fixed::ONE {
        return Err(format!("{path}.steer_response: must be from 0 to 1"));
    }
    let steer = read(get("steer")?, &format!("{path}.steer"))?;
    if steer.is_negative() {
        return Err(format!("{path}.steer: must not be negative"));
    }
    let second = match table.get("second") {
        None => None,
        Some(Value::Table(t)) => {
            if let Some(key) = t.keys().find(|k| !["reach", "steer"].contains(&k.as_str())) {
                return Err(format!("{path}.second.{key}: no such value"));
            }
            let at = format!("{path}.second");
            let reach = t
                .get("reach")
                .ok_or_else(|| format!("{at}: missing `reach`"))?;
            let steer = t
                .get("steer")
                .ok_or_else(|| format!("{at}: missing `steer`"))?;
            let steer = read(steer, &format!("{at}.steer"))?;
            if steer.is_negative() {
                return Err(format!("{at}.steer: must not be negative"));
            }
            Some(SecondSling {
                reach: parse_reach(reach, &format!("{at}.reach"))?,
                steer,
            })
        }
        Some(_) => return Err(format!("{path}.second: expected a table")),
    };
    let long_stand = match table.get("long_stand") {
        None => None,
        Some(Value::Table(t)) => {
            let at = format!("{path}.long_stand");
            if let Some(key) = t
                .keys()
                .find(|k| !["from", "to", "span"].contains(&k.as_str()))
            {
                return Err(format!("{at}.{key}: no such value"));
            }
            let step = |key: &str| match t.get(key) {
                Some(Value::Integer(n)) if u32::try_from(*n).is_ok() => Ok(*n as u32),
                _ => Err(format!("{at}.{key}: expected a number of steps")),
            };
            let (from, to) = (step("from")?, step("to")?);
            if from > to {
                return Err(format!("{at}: `from` is after `to`"));
            }
            let n = match t.get("span") {
                Some(Value::Array(items)) if items.len() == 4 => items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| match v {
                        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
                        _ => Err(format!("{at}.span[{i}]: expected a whole number of px")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(format!(
                        "{at}.span: expected [past from, to, below from, to]"
                    ));
                }
            };
            if n[0] > n[1] || n[2] > n[3] {
                return Err(format!("{at}.span: a range ends before it starts"));
            }
            Some(LongStand {
                from,
                to,
                span: Span {
                    past: (n[0], n[1]),
                    below: (n[2], n[3]),
                },
            })
        }
        Some(_) => return Err(format!("{path}.long_stand: expected a table")),
    };
    Ok(Sling {
        reach: parse_reach(get("reach")?, &format!("{path}.reach"))?,
        second,
        long_stand,
        left_shift,
        hold: frames("hold")?,
        shot,
        glide: frames("glide")?,
        breaks,
        rest: frames("rest")?,
        steer,
        steer_response,
        gravity: positive("gravity")?,
        max_fall: positive("max_fall")?,
    })
}

/// Reads a critter's values. Errors start with `path`.
fn parse_critter(table: &Table, path: &str) -> Result<Critter, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let whole = |v: &Value, at: &str| match v {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{at}: expected a whole number of px")),
    };
    let pair = |key: &str| {
        let at = format!("{path}.{key}");
        match get(key)? {
            Value::Array(items) if items.len() == 2 => Ok((
                whole(&items[0], &format!("{at}[0]"))?,
                whole(&items[1], &format!("{at}[1]"))?,
            )),
            _ => Err(format!("{at}: expected two whole numbers of px")),
        }
    };
    let range = |key: &str| {
        let (lo, hi) = pair(key)?;
        if lo > hi {
            return Err(format!(
                "{path}.{key}: the first must not be above the second"
            ));
        }
        Ok((lo, hi))
    };
    let count = |key: &str, max: i64| match get(key)? {
        Value::Integer(n) if (0..=max).contains(n) => Ok(*n),
        _ => Err(format!("{path}.{key}: expected a whole number, 0 to {max}")),
    };
    let at = format!("{path}.launch");
    let launch = match get("launch")? {
        Value::Array(items) if items.len() == 2 => {
            let n = |i: usize| {
                clamber_pack::read_number(&items[i], "", &format!("{at}[{i}]"))
                    .map_err(|e| e.message)
            };
            Vec2::new(n(0)?, n(1)?)
        }
        _ => return Err(format!("{at}: expected [x, y]")),
    };
    let at = format!("{path}.flutter");
    let mut flutter = [0; MAX_FLUTTER];
    let flutter_len = match get("flutter")? {
        Value::Array(items) if (1..=MAX_FLUTTER).contains(&items.len()) => {
            for (i, item) in items.iter().enumerate() {
                flutter[i] = match item {
                    Value::Integer(n) if (0..=255).contains(n) => *n as u8,
                    _ => return Err(format!("{at}[{i}]: expected 0 to 255 px")),
                };
            }
            items.len() as u8
        }
        _ => return Err(format!("{at}: expected 1 to {MAX_FLUTTER} values")),
    };
    let perch = count("perch", i64::from(u16::MAX))? as u16;
    if perch == 0 {
        return Err(format!("{path}.perch: must be at least 1"));
    }
    Ok(Critter {
        perch_across: range("perch_across")?,
        perch_down: range("perch_down")?,
        perch,
        hop: pair("hop")?,
        launch,
        flutter,
        flutter_len,
        flutter_from: count("flutter_from", 255)? as u8,
        settle: count("settle", 255)? as u8,
    })
}

/// Reads a rope's values. Errors start with `path`.
fn parse_rope(table: &Table, path: &str) -> Result<Rope, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let read = |v: &Value, at: &str| -> Result<Fixed, String> {
        let v = clamber_pack::read_number(v, "", at).map_err(|e| e.message)?;
        if v.is_negative() {
            return Err(format!("{at}: must not be negative"));
        }
        Ok(v)
    };
    let number = |key: &str| read(get(key)?, &format!("{path}.{key}"));
    let share = |key: &str| {
        let v = number(key)?;
        if v > Fixed::ONE {
            return Err(format!("{path}.{key}: must be from 0 to 1"));
        }
        Ok(v)
    };
    let px = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(i32::MAX)).contains(n) => Ok(*n as i32),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of px, 0 or more"
        )),
    };
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (1..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 1 to 255"
        )),
    };
    let speeds = |key: &str| -> Result<[Fixed; 2], String> {
        let at = format!("{path}.{key}");
        match get(key)? {
            Value::Array(items) if items.len() == 2 => Ok([
                read(&items[0], &format!("{at}[0]"))?,
                read(&items[1], &format!("{at}[1]"))?,
            ]),
            _ => Err(format!("{at}: expected [without Action, with Action]")),
        }
    };
    let swing = match get("swing")? {
        Value::Array(items) => match items.as_slice() {
            [Value::Integer(l), Value::Integer(r)] => i32::try_from(*l)
                .ok()
                .zip(i32::try_from(*r).ok())
                .filter(|&(l, r)| l >= 0 && r >= 0),
            _ => None,
        },
        _ => None,
    }
    .ok_or_else(|| format!("{path}.swing: expected [left, right], whole px, 0 or more"))?;
    let rope = Rope {
        speed: number("speed")?,
        swing,
        sway_response: share("sway_response")?,
        below: px("below")?,
        near: px("near")?,
        far: px("far")?,
        slow: number("slow")?,
        up: speeds("up")?,
        down: speeds("down")?,
        climb_response: share("climb_response")?,
        view_top: px("view_top")?,
        turn_frames: frames("turn_frames")?,
        turn_facing: frames("turn_facing")?,
        jump_speed: number("jump_speed")?,
    };
    if rope.turn_facing > rope.turn_frames {
        return Err(format!("{path}.turn_facing: must be within `turn_frames`"));
    }
    Ok(rope)
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

/// Reads a [`Grab`]: `{ right = [from, to], left = [from, to], below =
/// [from, to] }`. Errors start with `at`.
fn parse_grab(value: &Value, at: &str) -> Result<Grab, String> {
    let Value::Table(table) = value else {
        return Err(format!("{at}: expected {{ right, left, below }}"));
    };
    const KEYS: [&str; 3] = ["right", "left", "below"];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{at}.{key}: no such value"));
    }
    let range = |key: &str| match table.get(key) {
        Some(v) => parse_range(v, &format!("{at}.{key}")),
        None => Err(format!("{at}: missing `{key}`")),
    };
    Ok(Grab {
        right: range("right")?,
        left: range("left")?,
        below: range("below")?,
    })
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

/// Reads a walker's `drop` table. Errors start with `path`.
fn parse_walker_drop(value: &Value, path: &str) -> Result<WalkerDrop, String> {
    const KEYS: [&str; 5] = ["leave", "leave_speed", "gravity", "max_fall", "land"];
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: a drop table has no such value"));
    }
    let number = |key: &str| {
        let value = table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))?;
        let n = clamber_pack::read_number(value, "", &format!("{path}.{key}"))
            .map_err(|e| e.message)?;
        if n.is_negative() {
            return Err(format!("{path}.{key}: must not be negative"));
        }
        Ok(n)
    };
    let drop = WalkerDrop {
        leave: number("leave")?,
        leave_speed: number("leave_speed")?,
        gravity: number("gravity")?,
        max_fall: number("max_fall")?,
        land: number("land")?,
    };
    if drop.gravity == Fixed::ZERO || drop.max_fall == Fixed::ZERO {
        return Err(format!("{path}: `gravity` and `max_fall` must be positive"));
    }
    Ok(drop)
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
    let second_break = match (
        table.get("second_break_across"),
        table.get("second_break_above"),
    ) {
        (None, None) => None,
        (Some(_), Some(above)) => Some(SecondBreak {
            across: ranges("second_break_across")?,
            above: parse_range(above, &format!("{path}.second_break_above"))?,
        }),
        _ => {
            return Err(format!(
                "{path}: `second_break_across` and `second_break_above` go together"
            ));
        }
    };
    Ok(Hutch {
        break_across: ranges("break_across")?,
        break_above: parse_range(get("break_above")?, &format!("{path}.break_above"))?,
        second_break,
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
    const KEYS: [&str; 38] = [
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
        "off_rise",
        "off_landing",
        "drop_first",
        "drop_second",
        "drop_gravity",
        "drop_land",
        "second",
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
        off_rise: number("off_rise")?,
        off_landing: frames("off_landing")?,
        drop_first: number("drop_first")?,
        drop_second: number("drop_second")?,
        drop_gravity: number("drop_gravity")?,
        drop_land: number("drop_land")?,
        second: match table.get("second") {
            None => None,
            Some(Value::Table(t)) => {
                let at = format!("{path}.second");
                if let Some(key) = t.keys().find(|k| {
                    !["speed", "run_speed", "hop_gravity", "off_landing"].contains(&k.as_str())
                }) {
                    return Err(format!("{at}.{key}: no such value"));
                }
                let read = |key: &str| {
                    let v = t.get(key).ok_or_else(|| format!("{at}: missing `{key}`"))?;
                    clamber_pack::read_number(v, "", &format!("{at}.{key}")).map_err(|e| e.message)
                };
                Some(SecondRide {
                    speed: read("speed")?,
                    run_speed: read("run_speed")?,
                    hop_gravity: read("hop_gravity")?,
                    off_landing: match t.get("off_landing") {
                        Some(Value::Integer(n)) if (0..=i64::from(u8::MAX)).contains(n) => *n as u8,
                        _ => {
                            return Err(format!(
                                "{at}.off_landing: expected a whole number of frames, 0 to 255"
                            ));
                        }
                    },
                })
            }
            Some(_) => return Err(format!("{path}.second: expected a table")),
        },
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
        "second",
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

    Ok(Contact {
        upright: offsets("upright")?,
        airborne: offsets("airborne")?,
        rolling: offsets("rolling")?,
        crouching: offsets("crouching")?,
        fidget: offsets("fidget")?,
        mirror: whole(get("mirror")?, &format!("{path}.mirror"))?,
        turn,
        turn_parts: parts.len() as u8,
        poses: parse_poses(table, path)?,
        second: match table.get("second") {
            None => None,
            Some(v) => Some(parse_second_ball(v, &format!("{path}.second"))?),
        },
    })
}

/// Reads a ball's [`SecondBall`] table. Errors start with `path`.
fn parse_second_ball(value: &Value, path: &str) -> Result<SecondBall, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &["standing", "moving", "crouching", "air_hold", "crouch_hold"];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let offsets = |key: &str| -> Result<Offsets, String> {
        let at = format!("{path}.{key}");
        let Some(Value::Array(items)) = table.get(key) else {
            return Err(format!("{at}: expected a list of 4"));
        };
        let n = items
            .iter()
            .enumerate()
            .map(|(i, v)| match v {
                Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
                _ => Err(format!("{at}[{i}]: expected a whole number")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if n.len() != 4 {
            return Err(format!("{at}: expected a list of 4"));
        }
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Offsets {
            across: (n[0], n[1]),
            above: (n[2], n[3]),
        })
    };
    let count = |key: &str| match table.get(key) {
        Some(Value::Integer(n)) if (0..=254).contains(n) => Ok(*n as u8),
        _ => Err(format!("{path}.{key}: expected a whole number, 0 to 254")),
    };
    Ok(SecondBall {
        standing: offsets("standing")?,
        moving: offsets("moving")?,
        crouching: offsets("crouching")?,
        air_hold: count("air_hold")?,
        crouch_hold: count("crouch_hold")?,
    })
}

/// Reads a stinger's [`SecondReach`] table. Errors start with `path`.
fn parse_second_reach(value: &Value, path: &str) -> Result<SecondReach, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &["upright", "moving", "crouching", "fidget"];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let offsets = |key: &str| -> Result<Offsets, String> {
        let at = format!("{path}.{key}");
        let Some(Value::Array(items)) = table.get(key) else {
            return Err(format!("{at}: expected a list of 4"));
        };
        let n = items
            .iter()
            .enumerate()
            .map(|(i, v)| match v {
                Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
                _ => Err(format!("{at}[{i}]: expected a whole number")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if n.len() != 4 {
            return Err(format!("{at}: expected a list of 4"));
        }
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Offsets {
            across: (n[0], n[1]),
            above: (n[2], n[3]),
        })
    };
    Ok(SecondReach {
        upright: offsets("upright")?,
        moving: offsets("moving")?,
        crouching: offsets("crouching")?,
        fidget: offsets("fidget")?,
    })
}

/// Reads `[[across, above], ...]`, up to [`MAX_TURN_PARTS`] pairs of
/// whole px, one for each part of a ball's turn. Errors start with
/// `path`.
fn parse_shifts(value: &Value, path: &str) -> Result<[(i32, i32); MAX_TURN_PARTS], String> {
    let expected = || format!("{path}: expected a list of up to {MAX_TURN_PARTS} [across, above]");
    let Value::Array(items) = value else {
        return Err(expected());
    };
    if items.len() > MAX_TURN_PARTS {
        return Err(expected());
    }
    let whole = |v: &Value, at: &str| match v {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{at}: expected a whole number")),
    };
    let mut shifts = [(0, 0); MAX_TURN_PARTS];
    for (i, item) in items.iter().enumerate() {
        let at = format!("{path}[{i}]");
        let Value::Array(pair) = item else {
            return Err(format!("{at}: expected [across, above]"));
        };
        let [x, y] = pair.as_slice() else {
            return Err(format!("{at}: expected [across, above]"));
        };
        shifts[i] = (
            whole(x, &format!("{at}[0]"))?,
            whole(y, &format!("{at}[1]"))?,
        );
    }
    Ok(shifts)
}

/// Reads an [`InHands`] path: a list of up to [`MAX_IN_HANDS`] `[ahead,
/// up]`. Errors start with `path`.
fn parse_path(value: &Value, path: &str) -> Result<HandsPath, String> {
    let expected = || format!("{path}: expected a list of up to {MAX_IN_HANDS} [ahead, up]");
    let Value::Array(items) = value else {
        return Err(expected());
    };
    if items.len() > MAX_IN_HANDS {
        return Err(expected());
    }
    let whole = |v: &Value, at: &str| match v {
        Value::Integer(n) if i16::try_from(*n).is_ok() => Ok(*n as i16),
        _ => Err(format!("{at}: expected a whole number of px")),
    };
    let mut out = HandsPath {
        len: items.len() as u8,
        ..HandsPath::default()
    };
    for (i, item) in items.iter().enumerate() {
        let at = format!("{path}[{i}]");
        let Value::Array(pair) = item else {
            return Err(format!("{at}: expected [ahead, up]"));
        };
        let [x, y] = pair.as_slice() else {
            return Err(format!("{at}: expected [ahead, up]"));
        };
        out.places[i] = (
            whole(x, &format!("{at}[0]"))?,
            whole(y, &format!("{at}[1]"))?,
        );
    }
    Ok(out)
}

/// Reads a thrower's [`SecondThrower`] table. Errors start with `path`.
fn parse_second_thrower(value: &Value, path: &str) -> Result<SecondThrower, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &[
        "standing",
        "moving",
        "bounce",
        "air_hold",
        "crouching",
        "roll",
        "crouch_hold",
    ];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let offsets = |key: &str| -> Result<Offsets, String> {
        let at = format!("{path}.{key}");
        let Some(Value::Array(items)) = table.get(key) else {
            return Err(format!("{at}: expected a list of 4 whole numbers"));
        };
        let n = items
            .iter()
            .map(|v| match v {
                Value::Integer(n) if i32::try_from(*n).is_ok() => Some(*n as i32),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .filter(|n| n.len() == 4)
            .ok_or_else(|| format!("{at}: expected a list of 4 whole numbers"))?;
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Offsets {
            across: (n[0], n[1]),
            above: (n[2], n[3]),
        })
    };
    let air_hold = match table.get("air_hold") {
        Some(Value::Integer(n)) if (0..=254).contains(n) => *n as u8,
        _ => {
            return Err(format!(
                "{path}.air_hold: expected a whole number, 0 to 254"
            ));
        }
    };
    Ok(SecondThrower {
        standing: offsets("standing")?,
        moving: offsets("moving")?,
        bounce: offsets("bounce")?,
        air_hold,
        crouching: parse_across(table, "crouching", path)?,
        roll: parse_across(table, "roll", path)?,
        crouch_hold: parse_crouch_hold(table, path)?,
    })
}

/// Reads a thrower's [`ThrowerContact`] table. Errors start with `path`.
fn parse_thrower_contact(value: &Value, path: &str) -> Result<ThrowerContact, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &[
        "upright",
        "airborne",
        "ledge",
        "bounce",
        "roll",
        "mirror",
        "upright_jump",
        "land_hold",
        "roll_after",
        "fidget_time",
        "crouching",
        "fidget",
        "crouch_hold",
        "second",
    ];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let whole = |key: &str| match get(key)? {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{path}.{key}: expected a whole number")),
    };
    let offsets = |key: &str| -> Result<Offsets, String> {
        let at = format!("{path}.{key}");
        let n = match get(key)? {
            Value::Array(items) if items.len() == 4 => items
                .iter()
                .map(|v| match v {
                    Value::Integer(n) if i32::try_from(*n).is_ok() => Some(*n as i32),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
        .ok_or_else(|| format!("{at}: expected a list of 4 whole numbers"))?;
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Offsets {
            across: (n[0], n[1]),
            above: (n[2], n[3]),
        })
    };
    Ok(ThrowerContact {
        upright: offsets("upright")?,
        airborne: offsets("airborne")?,
        ledge: whole("ledge")?,
        bounce: offsets("bounce")?,
        roll: match table.get("roll") {
            None => None,
            Some(_) => Some(offsets("roll")?),
        },
        mirror: whole("mirror")?,
        poses: parse_poses(table, path)?,
        crouching: parse_across(table, "crouching", path)?,
        fidget: parse_across(table, "fidget", path)?,
        crouch_hold: parse_crouch_hold(table, path)?,
        second: match table.get("second") {
            None => None,
            Some(v) => Some(parse_second_thrower(v, &format!("{path}.second"))?),
        },
    })
}

/// Reads the [`Poses`] timings in a ball's or a thrower's contact
/// `table`. Errors start with `path`.
fn parse_poses(table: &Table, path: &str) -> Result<Poses, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
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
    let frames = |v: &Value, at: &str| count(v, at, i64::from(u8::MAX)).map(|n| n as u8);

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
    Ok(Poses {
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
        "land_ran",
        "land_drop",
        "land_held",
        "roll_after",
        "upright_fall",
        "bounce",
        "carry",
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
    let land_hold = frames("land_hold")?;
    let land_ran = match table.get("land_ran") {
        Some(_) => frames("land_ran")?,
        None => land_hold,
    };
    let land_drop = match table.get("land_drop") {
        None => None,
        Some(Value::Array(pair)) if pair.len() == 2 => {
            let at = format!("{path}.land_drop");
            let below = whole(&pair[0], &format!("{at}[0]"))?;
            let more = match &pair[1] {
                Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => *n as u8,
                _ => {
                    return Err(format!(
                        "{at}[1]: expected a whole number of frames, 0 to 255"
                    ));
                }
            };
            Some((below, more))
        }
        Some(_) => return Err(format!("{path}.land_drop: expected [px, frames]")),
    };
    let mut land_held = [(1, 0, 0); MAX_LAND_HELD];
    let mut land_held_rows = 0;
    match table.get("land_held") {
        None => {}
        Some(Value::Array(rows)) if rows.len() <= MAX_LAND_HELD => {
            for (i, row) in rows.iter().enumerate() {
                let at = format!("{path}.land_held[{i}]");
                let n = |v: &Value| match v {
                    Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
                    _ => Err(format!("{at}: expected [from, to, frames], each 0 to 255")),
                };
                let Value::Array(row) = row else {
                    return Err(format!("{at}: expected [from, to, frames]"));
                };
                let [from, to, more] = row.as_slice() else {
                    return Err(format!("{at}: expected [from, to, frames]"));
                };
                land_held[i] = (n(from)?, n(to)?, n(more)?);
            }
            land_held_rows = rows.len() as u8;
        }
        Some(_) => {
            return Err(format!(
                "{path}.land_held: expected up to {MAX_LAND_HELD} rows of [from, to, frames]"
            ));
        }
    }
    let roll_after = match table.get("roll_after") {
        None => (0, 0),
        Some(Value::Array(pair)) if pair.len() == 2 => {
            let n = |i: usize| match &pair[i] {
                Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
                _ => Err(format!(
                    "{path}.roll_after[{i}]: expected a whole number of frames, 0 to 255"
                )),
            };
            (n(0)?, n(1)?)
        }
        Some(_) => return Err(format!("{path}.roll_after: expected [on, stopped]")),
    };
    let upright_fall = match table.get("upright_fall") {
        None => false,
        Some(Value::Boolean(b)) => *b,
        Some(_) => return Err(format!("{path}.upright_fall: expected true or false")),
    };
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
        land_hold,
        land_ran,
        land_drop,
        land_held,
        land_held_rows,
        roll_after,
        upright_fall,
        bounce: match table.get("bounce") {
            None => None,
            Some(_) => Some(span("bounce")?),
        },
        carry: match table.get("carry") {
            None => None,
            Some(_) => Some(span("carry")?),
        },
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
        check(
            "spring",
            "squash = [[12]]",
            "squash[0]: expected [top, frames]",
        );
        check("spring", "wobble = [[12, 256]]", "wobble[0]: out of range");
        check("spring", "throw_up = [15, -54]", "it ends before it starts");
        check("spring", "jump_gravity = -1", "must not be negative");
        check(
            "rope",
            "swing = [3]",
            "params.swing: expected [left, right]",
        );
        check("rope", "up = [1.5, -2]", "up[1]: must not be negative");
        check("rope", "sway_response = 2", "from 0 to 1");
        check("rope", "turn_facing = 9", "within `turn_frames`");
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
    fn carried_objects_have_paths_in_the_hands() {
        let a = Archetypes::builtin();
        let resolve = |params| a.resolve(&entity("crate", params), "test", "e");
        let Behavior::Carry { in_hands, .. } = resolve("").unwrap().unwrap().behavior else {
            panic!("not carried");
        };
        let lens = (
            in_hands.pick_up.len,
            in_hands.throw.len,
            in_hands.put_down.len,
        );
        assert_eq!(lens, (14, 22, 14));
        assert_eq!(in_hands.pick_up.at(0), Some((20, 3)));
        assert_eq!(in_hands.throw.at(21), Some((0, 54)));
        assert_eq!(in_hands.throw.at(22), None);
        assert_eq!(in_hands.put_down.at(13), Some((16, 3)));
        assert!(resolve("in_hands_throw = [[1]]").is_err());
        assert!(resolve("in_hands_put_down = [[1, 40000]]").is_err());
    }

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
            thrower,
            thrower_early,
            holder: Some(holder),
            ..
        } = resolve("holds_character = true")
        else {
            panic!("no holder");
        };
        assert_eq!(holder.window, (31, 32, 31, 32));
        let rows = [
            (thrower_early, (-55, 37), (-47, 65)),
            (thrower, (-30, 36), (-23, 32)),
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
