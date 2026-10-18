//! Carrying objects (`pick_up_time` above 0): picking up an object that
//! can be carried, carrying it, throwing it and putting it down, as
//! `spec/behavior/carrying.md` describes.
//!
//! A pick-up starts on the frame after one that ends with the player
//! moving into an object within reach with action held, or on a frame
//! action is pressed with the player standing within reach. The player
//! does the rest ([`Hands`]); this module moves the object to match:
//!
//! - While held, it sits on the player's head. Where it's drawn isn't in
//!   the spec.
//! - Thrown, it leaves the hands on frame `throw_release` of the throw,
//!   `throw_lift` above the player's feet, and flies ([`Flight`]).
//! - Put down, it's set `put_down_ahead` in front of the player and
//!   `put_down_lift` above the floor on the put-down's last frame, and
//!   rests on the floor from the next.
//! - If the player is hurt, swaps or is out of play while holding it,
//!   it drops straight down.
//! - With a `let_go_speed`, one still in the hands at a knock-out is let
//!   go on the hit frame instead, even with its throw begun, and falls
//!   away touching nothing until it's removed, as
//!   `spec/behavior/knock-out.md` ("An object in the hands") describes
//!   ([`fall_away`]). A holder moves from the next frame, other objects
//!   once the player does, `knockout_hold` frames after the hit.
//!   Without a view it's removed once below the level, which the spec
//!   doesn't cover. One still in the hands on the walk-out's first step
//!   after the end marker is let go then the same way, moving from the
//!   next, as `spec/behavior/level-end.md` ("An object in the lead's
//!   hands") describes, and is also removed at `end_let_go_right` and
//!   `end_let_go_above`. So is one still in the hands on the walk-out's
//!   first step after an exit, as `spec/behavior/exits.md` ("An object
//!   in the lead's hands") describes, removed only as at a knock-out.
//!
//! Thrown, it bounces on the floor and then rolls along it, breaks
//! against faces and walls, and hits walkers and throwers, as
//! `spec/behavior/thrown-objects.md` describes ([`fly`]). Its floor in
//! a column can be up to a step above it, which lifts it when it isn't
//! moving down, and ground that isn't one-way stops it rising into it
//! from below; one-way ground breaks it as other ground does. In the hands
//! during a throw and carried, it breaks on throwers, which it knocks
//! out ([`things`](crate::things)); the player isn't held up, and the
//! carrying pose isn't kept until a jump lands. Touching characters and
//! other enemies isn't in the spec yet; it doesn't. Dropped, it falls
//! straight down and rests where it lands.
//!
//! An object that holds the missing character ([`Holder`]) moves the
//! same way, but breaks sooner, as `spec/behavior/holder-breaking.md`
//! describes: on its first touch of the floor, outside the picture, and
//! on throwers and balls as well as walkers ([`things`](crate::things)).
//! Against a wall it still falls on that frame. Breaking, it frees the
//! character ([`join`](crate::join)). The spec doesn't cover how it
//! moves when a bounce on a walker makes the player let go of it; here
//! the player keeps it. Barriers that stop walkers aren't in the engine.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetype, Behavior, Holder};
use crate::columns::{self, column};
use crate::flow::Outcome;
use crate::player::{
    Act, Facing, HEIGHT, Player, SPEED_GRAIN, STEP, State, TINY, ceil, ease, frames, is_wall,
};
use crate::throwers::Picture;
use crate::world::{Event, Phase, Thing, World};

/// A thrown or dropped object's flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Flight {
    /// Speed across, px per frame.
    pub vx: Fixed,
    /// Speed down, px per frame.
    pub vy: Fixed,
    /// The speed across that `vx` eases toward, by `response`.
    pub target: Fixed,
    pub response: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
    /// Thrown, rather than dropped: it bounces, rolls, breaks and hits
    /// walkers as the `throw_*` values below say. Dropped, it rests
    /// where it lands.
    pub thrown: bool,
    /// `throw_land_margin`, `throw_bounce_min` and `throw_bounce`.
    pub margin: Fixed,
    pub bounce_min: Fixed,
    pub bounce: Fixed,
    /// Rolling along the floor rather than in the air.
    pub rolling: bool,
    /// Frames since it left the hands, counting this one, up to 255.
    pub age: u8,
}

impl Flight {
    /// Whether it's moving right; still counts as right.
    pub fn right(&self) -> bool {
        !self.vx.is_negative()
    }
}

/// Before the player moves: an action press with the player standing
/// within reach of an object starts a pick-up on this step, and so does
/// one made late in a put-down and still held as it ends.
pub(crate) fn before(world: &mut World, input: &Frame) {
    let physics = world.player_physics();
    let p = &world.player;
    if physics[Param::PickUpTime] <= Fixed::ZERO || !p.grounded() {
        return;
    }
    let put_down = frames(physics[Param::PutDownTime]);
    let (free, regrab) = match p.hands {
        None => (true, false),
        Some(h) if h.act == Act::PutDown && h.frame.saturating_add(1) >= put_down => (
            true,
            h.frame.saturating_add(1) == put_down
                && h.press.is_some()
                && input.held.contains(Action::Action),
        ),
        Some(_) => (false, false),
    };
    let press = input.pressed.contains(Action::Action) && p.vel.x == Fixed::ZERO;
    if free && (press || regrab) {
        // Pressing action within reach doesn't roll.
        if let Some(thing) = in_reach(world, None) {
            world.player.pick = Some(thing);
        }
    }
}

/// After everything has moved: moves the object the player holds with
/// it, lets go of it where the player throws or sets it down, and finds
/// the object a pick-up starts with on the next step, if one does.
pub(crate) fn after(world: &mut World, input: &Frame) {
    world.player.pick = None;
    let physics = world.player_physics();
    let Some(hands) = world.player.hands else {
        world.player.pick = moving_into(world, input, &physics);
        return;
    };
    // With a `let_go_speed`, an object still in the hands on the
    // walk-out's first step, after the end marker or an exit, is let go
    // then.
    let let_go_speed = physics[Param::LetGoSpeed] > Fixed::ZERO;
    let walking_out = (world.ending.is_some_and(|e| e.frame >= 1)
        || world.leaving.is_some_and(|l| l.frame >= 1))
        && let_go_speed;
    if !can_hold(world) || walking_out {
        world.player.hands = None;
        let facing = world.player.facing;
        let let_go = (world.player.knockout.is_some() || walking_out) && let_go_speed;
        let thing = &mut world.things[hands.thing];
        if thing.phase == Phase::Held {
            if let_go {
                let held = frames(physics[Param::KnockoutHold]).saturating_sub(1);
                let holds_character = thing.archetype.as_ref().and_then(holder).is_some();
                let wait = if holds_character || walking_out {
                    0
                } else {
                    held
                };
                let ahead = match facing {
                    Facing::Left => -physics[Param::LetGoSpeed],
                    Facing::Right => physics[Param::LetGoSpeed],
                };
                thing.vx = ahead;
                thing.phase = Phase::LetGo {
                    vy: -physics[Param::LetGoRise],
                    wait,
                };
            } else {
                drop(thing, &physics);
            }
        }
        return;
    }
    let p = &world.player;
    let at = |lift: Fixed| Fixed::from_int(ceil(p.pos.y - lift) as i32);
    let x = Fixed::from_int(p.pos.x.floor() as i32);
    let ahead = match p.facing {
        Facing::Left => -Fixed::ONE,
        Facing::Right => Fixed::ONE,
    };
    let thing = &mut world.things[hands.thing];
    // An object that broke in the hands stays gone.
    if !thing.active || (thing.phase != Phase::Held && hands.act != Act::PickUp) {
        return;
    }
    thing.phase = Phase::Held;
    thing.pos = in_hands(p);
    let release = frames(physics[Param::ThrowRelease]);
    let set_down = frames(physics[Param::PutDownTime]).saturating_sub(1);
    match hands.act {
        Act::Throw if hands.frame == release => {
            thing.pos = Vec2::new(x, at(physics[Param::ThrowLift]));
            thing.phase = Phase::Thrown(Flight {
                vx: ahead * physics[Param::ThrowSpeed],
                vy: physics[Param::ThrowDrop],
                target: ahead * physics[Param::ThrowGlide],
                response: physics[Param::SpeedResponse],
                gravity: physics[Param::ThrowGravity],
                max_fall: physics[Param::ThrowMaxFall],
                thrown: true,
                margin: physics[Param::ThrowLandMargin],
                bounce_min: physics[Param::ThrowBounceMin],
                bounce: physics[Param::ThrowBounce],
                rolling: false,
                age: 0,
            });
        }
        Act::PutDown if hands.frame == set_down => {
            thing.pos = Vec2::new(
                x + ahead * physics[Param::PutDownAhead],
                at(physics[Param::PutDownLift]),
            );
            thing.phase = Phase::Placed;
        }
        _ => {}
    }
}

/// Where an object in `player`'s hands is: on its head, with `x` and `y`
/// whole px.
pub(crate) fn in_hands(player: &Player) -> Vec2 {
    Vec2::new(
        Fixed::from_int(player.pos.x.floor() as i32),
        Fixed::from_int(ceil(player.pos.y - HEIGHT) as i32),
    )
}

/// Whether the player can go on holding an object after this step.
fn can_hold(world: &World) -> bool {
    world.player.knockout.is_none()
        && !world.events.contains(&Event::Hurt)
        && !world.losing()
        && world.swap.is_none()
        && world.flow.outcome == Outcome::Playing
}

/// The object a player moving into it with action held picks up on the
/// next step: the nearest within reach in the way it's moving.
fn moving_into(world: &World, input: &Frame, physics: &Physics) -> Option<usize> {
    let p = &world.player;
    let moving = p.vel.x != Fixed::ZERO || p.state == State::Roll;
    if physics[Param::PickUpTime] <= Fixed::ZERO
        || !moving
        || !p.grounded()
        || !input.held.contains(Action::Action)
        || !can_hold(world)
    {
        return None;
    }
    let way = if p.vel.x.is_negative() {
        Facing::Left
    } else if p.vel.x > Fixed::ZERO {
        Facing::Right
    } else {
        p.facing
    };
    in_reach(world, Some(way))
}

/// The nearest object resting within reach of the player, or just set
/// down, only those on the side `way` says if it's given.
fn in_reach(world: &World, way: Option<Facing>) -> Option<usize> {
    let p = &world.player;
    world
        .things
        .iter()
        .filter(|t| t.active && matches!(t.phase, Phase::Ready | Phase::Placed))
        .filter_map(|t| {
            let reach = match t.archetype {
                Some(Archetype {
                    behavior: Behavior::Carry { reach, .. },
                    ..
                }) => reach,
                _ => return None,
            };
            let gap = t.pos.x - p.pos.x;
            let side = match way {
                Some(Facing::Left) => gap <= Fixed::ZERO,
                Some(Facing::Right) => gap >= Fixed::ZERO,
                None => true,
            };
            let level = (t.pos.y - p.pos.y).abs() <= STEP;
            (side && level && gap.abs() <= reach).then_some((gap.abs(), t.index))
        })
        .min()
        .map(|(_, index)| index)
}

/// Lets go of a held object where it is: it drops straight down.
fn drop(thing: &mut Thing, physics: &Physics) {
    thing.phase = Phase::Thrown(Flight {
        vx: Fixed::ZERO,
        vy: Fixed::ZERO,
        target: Fixed::ZERO,
        response: physics[Param::SpeedResponse],
        gravity: physics[Param::ThrowGravity],
        max_fall: physics[Param::ThrowMaxFall],
        thrown: false,
        margin: Fixed::ZERO,
        bounce_min: Fixed::ZERO,
        bounce: Fixed::ZERO,
        rolling: false,
        age: 0,
    });
}

/// One frame of a thrown or dropped object's flight. It's gone once it's
/// below the level. A thrown one holding the missing character breaks
/// instead, also once outside its window around the `picture`, where the
/// view was at the start of the step. Returns whether it broke holding
/// the character, which it then frees where it is.
pub(crate) fn fly(
    level: &Level,
    thing: &mut Thing,
    archetype: &Archetype,
    mut flight: Flight,
    slope: Fixed,
    picture: Option<Picture>,
) -> bool {
    flight.age = flight.age.saturating_add(1);
    flight.vx = if flight.response > Fixed::ZERO {
        ease(flight.vx, flight.target, flight.response)
    } else {
        flight.target
    };
    let holder = holder(archetype).filter(|_| flight.thrown);
    if flight.thrown {
        if bounce_and_roll(level, thing, flight, slope, holder.is_some()) {
            thing.active = false;
            return true;
        }
    } else {
        drop_and_rest(level, thing, archetype.size, flight);
    }
    let below = thing.pos.y - archetype.size.y > level.size.y;
    if let Some(holder) = holder
        && thing.active
        && (below || picture.is_some_and(|p| !p.holds(thing.pos, holder.window)))
    {
        thing.active = false;
        return true;
    }
    if below {
        thing.active = false;
    }
    false
}

/// One frame of an object let go at a knock-out or the level end: after
/// its wait, it falls by `throw_gravity` up to `throw_max_fall` and moves
/// by its speeds, through everything. It's removed once left of the
/// level, or with `picture`, where the view was at the start of the step,
/// once its `y` rounded down is at the bottom of the window objects
/// appear in (`appear_below` below the picture) or below; without one,
/// once it's below the level. During the walk-out (`ending`), it's also
/// removed once `end_let_go_right` right of the picture's left edge or
/// `end_let_go_above` above its top.
pub(crate) fn fall_away(
    level: &Level,
    thing: &mut Thing,
    size: Vec2,
    physics: &Physics,
    picture: Option<Picture>,
    ending: bool,
) {
    let Phase::LetGo { vy, wait } = thing.phase else {
        return;
    };
    if wait > 0 {
        thing.phase = Phase::LetGo { vy, wait: wait - 1 };
        return;
    }
    let vy = (vy + physics[Param::ThrowGravity]).min(physics[Param::ThrowMaxFall]);
    thing.pos = Vec2::new(thing.pos.x + thing.vx, thing.pos.y + vy);
    thing.phase = Phase::LetGo { vy, wait: 0 };
    let depth = physics[Param::ViewHeight].floor() + physics[Param::AppearBelow].floor();
    let (right, above) = match ending {
        true => (
            physics[Param::EndLetGoRight].floor(),
            physics[Param::EndLetGoAbove].floor(),
        ),
        false => (0, 0),
    };
    let below = match picture {
        Some(picture) => picture.below(thing.pos, depth) || picture.beyond(thing.pos, right, above),
        None => thing.pos.y - size.y > level.size.y,
    };
    if thing.pos.x < Fixed::ZERO || below {
        thing.active = false;
    }
}

/// What makes an object of `archetype` hold the missing character, if
/// it does.
pub(crate) fn holder(archetype: &Archetype) -> Option<Holder> {
    match archetype.behavior {
        Behavior::Carry { holder, .. } => holder,
        _ => None,
    }
}

/// A thrown object's frame once its `vx` has eased: it breaks if this
/// move would take it into a column it runs into as into a wall (see
/// [`columns::hits_side`]); otherwise it moves across, and then down
/// under gravity in the air or along the floor rolling. In the air, it
/// touches the floor once it's moving down and ends less than `margin`
/// above it, and is put on it: falling at `bounce_min` or faster before
/// this frame, it bounces up at `bounce` of that speed; slower, it rolls
/// from then on. Rolling off the floor, it falls again.
///
/// One that `holds` the missing character breaks instead of bouncing,
/// and also on passing such a column on the way to another. Against a
/// wall it stays where it is across but still falls in its own column,
/// and touching the floor there, is put on it. Returns whether it broke
/// holding the character; it's left where it broke.
fn bounce_and_roll(
    level: &Level,
    thing: &mut Thing,
    mut flight: Flight,
    slope: Fixed,
    holds: bool,
) -> bool {
    let pos = thing.pos;
    let x = pos.x + flight.vx;
    let to = column(x);
    let dir = if flight.right() { 1 } else { -1 };
    let from = column(pos.x);
    let wall = if holds {
        (1..=(to - from).abs()).any(|n| {
            let into = from + i64::from(dir) * n;
            columns::hits_side(level, into - i64::from(dir), into, dir, pos.y, slope)
        })
    } else {
        to != from && columns::hits_side(level, from, to, dir, pos.y, slope)
    };
    if wall && holds {
        let (y, touch) = fall(level, from, pos.y, &mut flight);
        thing.pos = Vec2::new(pos.x, touch.unwrap_or(y));
        return true;
    }
    if wall {
        // Its pieces aren't in the spec.
        thing.active = false;
        return false;
    }
    if flight.rolling {
        match columns::object_floor(level, to, pos.y) {
            Some(f) => {
                thing.pos = Vec2::new(x, f.y);
                thing.phase = Phase::Thrown(flight);
                return false;
            }
            // Rolling off a ledge isn't in the spec: it falls from rest.
            None => {
                flight.rolling = false;
                flight.vy = Fixed::ZERO;
            }
        }
    }
    let before = flight.vy;
    let (y, touch) = fall(level, to, pos.y, &mut flight);
    if let Some(floor) = touch
        && holds
        && before >= flight.bounce_min
    {
        thing.pos = Vec2::new(x, floor);
        return true;
    }
    thing.pos = match touch {
        Some(floor) => {
            if before >= flight.bounce_min {
                flight.vy = -down_to_grain(before * flight.bounce);
            } else {
                flight.rolling = true;
                flight.vy = Fixed::ZERO;
            }
            Vec2::new(x, floor)
        }
        None => Vec2::new(x, y),
    };
    thing.phase = Phase::Thrown(flight);
    false
}

/// A thrown object's fall in the air in `column` from `feet` on one
/// frame: `vy` grows by gravity up to its most. Not moving down, with the
/// column's floor ([`columns::object_floor`]) higher than `feet`, it's
/// lifted by as much as the floor is above `feet` rounded up. Rising from
/// below ground that holds from below into it, it's stopped there as a
/// rising player is, with `vy` 0 ([`columns::stops_rise`]). Otherwise it
/// touches the floor once it's moving down and ends less than `margin`
/// above it. Returns its `y` after the fall, and the floor's if it
/// touches it.
fn fall(level: &Level, column: i64, feet: Fixed, flight: &mut Flight) -> (Fixed, Option<Fixed>) {
    flight.vy = (flight.vy + flight.gravity).min(flight.max_fall);
    let y = feet + flight.vy;
    let floor = columns::object_floor(level, column, feet).map(|f| f.y);
    if let Some(floor) = floor
        && flight.vy <= Fixed::ZERO
        && floor < feet
    {
        return (y - (Fixed::from_int(ceil(feet) as i32) - floor), None);
    }
    if flight.vy < Fixed::ZERO
        && let Some(back) = columns::stops_rise(level, column, feet, y)
    {
        flight.vy = Fixed::ZERO;
        return (y + back, None);
    }
    let touch = floor.filter(|&f| flight.vy > Fixed::ZERO && y > f - flight.margin);
    (y, touch)
}

/// `v`, at least 0, rounded down to a whole [`SPEED_GRAIN`].
fn down_to_grain(v: Fixed) -> Fixed {
    let grain = SPEED_GRAIN.to_bits();
    Fixed::from_bits(v.to_bits() / grain * grain)
}

/// A dropped object's frame once its `vx` has eased: `vy` grows by
/// gravity up to its most, then it moves by both. A wall in the way stops
/// it against the wall, and it drops straight down from there; ground
/// below catches it, and it rests there.
fn drop_and_rest(level: &Level, thing: &mut Thing, size: Vec2, mut flight: Flight) {
    flight.vy = (flight.vy + flight.gravity).min(flight.max_fall);
    let pos = thing.pos;
    let mut x = pos.x + flight.vx;
    if flight.vx != Fixed::ZERO {
        let (dir, sign) = if flight.vx.is_negative() {
            (Dir::Left, -Fixed::ONE)
        } else {
            (Dir::Right, Fixed::ONE)
        };
        let half_width = Fixed::from_bits(size.x.to_bits() / 2);
        let speed = flight.vx.abs();
        let probe = Vec2::new(pos.x, pos.y - TINY);
        let reach = ((size.y - Fixed::ONE - TINY).max(Fixed::ZERO), Fixed::ZERO);
        let wall = level
            .collision
            .sweep(probe, dir, speed + half_width, reach, is_wall)
            .filter(|hit| hit.distance < speed + half_width);
        if let Some(hit) = wall {
            x = pos.x + sign * (hit.distance - half_width).max(Fixed::ZERO);
            flight.vx = Fixed::ZERO;
            flight.target = Fixed::ZERO;
        }
    }
    let from = Vec2::new(x, pos.y);
    match level.collision.cast(from, Dir::Down, flight.vy) {
        Some(hit) => {
            thing.pos = Vec2::new(x, hit.point.y);
            thing.phase = Phase::Ready;
        }
        None => {
            thing.pos = Vec2::new(x, pos.y + flight.vy);
            thing.phase = Phase::Thrown(flight);
        }
    }
}

/// An object set down comes to rest on the floor under it, or drops if
/// there's none within a step.
pub(crate) fn settle(level: &Level, thing: &mut Thing, physics: &Physics) {
    let from = Vec2::new(thing.pos.x, thing.pos.y - STEP);
    match level.collision.cast(from, Dir::Down, STEP + STEP) {
        Some(hit) => {
            thing.pos.y = hit.point.y;
            thing.phase = Phase::Ready;
        }
        None => drop(thing, physics),
    }
}
