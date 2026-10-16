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
//! - If the player is hurt, knocked out, swaps or is out of play while
//!   holding it, it drops straight down.
//!
//! Thrown, it bounces on the floor and then rolls along it, breaks
//! against faces and walls, and hits walkers, as
//! `spec/behavior/thrown-objects.md` describes ([`fly`]). Touching
//! characters and other enemies isn't in the spec yet; it doesn't.
//! Dropped, it falls straight down and rests where it lands.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetype, Behavior};
use crate::columns::{self, column};
use crate::flow::Outcome;
use crate::player::{
    Act, Facing, HEIGHT, SPEED_GRAIN, STEP, State, TINY, ceil, ease, frames, is_wall,
};
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
    if !can_hold(world) {
        world.player.hands = None;
        if world.things[hands.thing].phase == Phase::Held {
            drop(&mut world.things[hands.thing], &physics);
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
    if thing.phase != Phase::Held && hands.act != Act::PickUp {
        return;
    }
    thing.phase = Phase::Held;
    thing.pos = Vec2::new(x, at(HEIGHT));
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
/// below the level.
pub(crate) fn fly(level: &Level, thing: &mut Thing, size: Vec2, mut flight: Flight, slope: Fixed) {
    flight.age = flight.age.saturating_add(1);
    flight.vx = if flight.response > Fixed::ZERO {
        ease(flight.vx, flight.target, flight.response)
    } else {
        flight.target
    };
    if flight.thrown {
        bounce_and_roll(level, thing, flight, slope);
    } else {
        drop_and_rest(level, thing, size, flight);
    }
    if thing.pos.y - size.y > level.size.y {
        thing.active = false;
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
fn bounce_and_roll(level: &Level, thing: &mut Thing, mut flight: Flight, slope: Fixed) {
    let pos = thing.pos;
    let x = pos.x + flight.vx;
    let to = column(x);
    let dir = if flight.right() { 1 } else { -1 };
    let from = column(pos.x);
    if to != from && columns::hits_side(level, from, to, dir, pos.y, slope) {
        // Its pieces aren't in the spec.
        thing.active = false;
        return;
    }
    if flight.rolling {
        match columns::floor(level, to, pos.y) {
            Some(f) => {
                thing.pos = Vec2::new(x, f.y);
                thing.phase = Phase::Thrown(flight);
                return;
            }
            // Rolling off a ledge isn't in the spec: it falls from rest.
            None => {
                flight.rolling = false;
                flight.vy = Fixed::ZERO;
            }
        }
    }
    let before = flight.vy;
    flight.vy = (flight.vy + flight.gravity).min(flight.max_fall);
    let y = pos.y + flight.vy;
    let touch = (flight.vy > Fixed::ZERO)
        .then(|| columns::floor_in_air(level, to, pos.y))
        .flatten()
        .filter(|f| y > f.y - flight.margin);
    thing.pos = match touch {
        Some(f) => {
            if before >= flight.bounce_min {
                flight.vy = -down_to_grain(before * flight.bounce);
            } else {
                flight.rolling = true;
                flight.vy = Fixed::ZERO;
            }
            Vec2::new(x, f.y)
        }
        None => Vec2::new(x, y),
    };
    thing.phase = Phase::Thrown(flight);
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
