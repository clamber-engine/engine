//! Carrying objects (`pick_up_time` above 0): picking up an object that
//! can be carried, carrying it, throwing it and putting it down, as
//! `spec/behavior/carrying.md` describes.
//!
//! A pick-up starts on a frame with action held, if the frame before
//! ended with the player on the ground, holding nothing, not knocked
//! out and within the object's reach ([`Grab`](crate::Grab)), by the
//! player's facing then. Moving into it with action held, it starts on
//! the frame after the first that ends within reach; pressing action
//! within reach, on the press frame, with no roll. A holder lying where
//! it's placed is within reach by its look. The player does the rest
//! ([`Hands`]); this module moves the object to match:
//!
//! - While held, it sits on the player's head, except on the frames of a
//!   pick-up, a throw and a put-down its archetype's path in the hands
//!   covers ([`in_hands`]), as `spec/behavior/carrying.md` ("The object
//!   in the hands") describes; there it follows the path. Where it's
//!   drawn isn't in the spec.
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
//! carrying pose isn't kept until a jump lands. Carried, it also breaks
//! on walkers, which it knocks back, and then the carrying pose is kept
//! until a jump lands, as `spec/behavior/thrown-objects.md` ("A carried
//! object") describes. So it does during a pick-up, a throw or a
//! put-down, which then goes on to its end without it; a pick-up ends
//! with nothing in the hands. It passes through both characters, from
//! the frame it leaves the hands on (thrown-objects.md, "Meeting a
//! character"). Touching other enemies isn't in the spec yet; it
//! doesn't. Dropped, it falls straight down and rests where it lands.
//!
//! An object that holds the missing character ([`Holder`]) moves the
//! same way, but breaks sooner, as `spec/behavior/holder-breaking.md`
//! describes: on its first touch of the floor, outside the picture, and
//! on throwers and balls as well as walkers ([`things`](crate::things)).
//! Against a wall it still falls on that frame. Breaking, it frees the
//! character ([`join`](crate::join)). A bounce on a walker makes
//! the player let go of it, and it flies on at a steady speed
//! ([`release_on_bounce`]). The vertical step walkers turn at
//! (`spec/behavior/walker.md`, "Turning at a barrier") breaks either kind
//! as ground more than a step above it does (thrown-objects.md,
//! "Breaking at a vertical step"); one in the hands it doesn't.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::{Dir, Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Archetype, Behavior, HandsPath, Holder};
use crate::columns::{self, column};
use crate::flow::Outcome;
use crate::player::{
    Act, Facing, HEIGHT, Player, SPEED_GRAIN, STEP, TINY, ceil, ease, frames, is_wall,
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
    /// Frames it holds still before its first move: an object that left
    /// the hands in a bounce, on the bounce's frozen frames.
    pub wait: u8,
    /// Its first move is the one after a bounce's: it ends with `vy` at
    /// one speed grain.
    pub launch: bool,
    /// Thrown by the second character.
    pub second: bool,
}

impl Flight {
    /// Whether it's moving right; still counts as right.
    pub fn right(&self) -> bool {
        !self.vx.is_negative()
    }
}

/// Before the player moves: an action press made with the player
/// within reach of an object as a put-down ends starts a pick-up on this
/// step, and so does one made late in the put-down and still held.
pub(crate) fn before(world: &mut World, input: &Frame) {
    let physics = world.player_physics();
    let p = &world.player;
    let Some(h) = p.hands else {
        return;
    };
    let put_down = frames(physics[Param::PutDownTime]);
    if physics[Param::PickUpTime] <= Fixed::ZERO
        || !p.grounded()
        || h.act != Act::PutDown
        || h.frame.saturating_add(1) < put_down
    {
        return;
    }
    let regrab = h.press.is_some() && input.held.contains(Action::Action);
    if input.pressed.contains(Action::Action) || regrab {
        world.player.pick = in_reach(world);
    }
}

/// After everything has moved: moves the object the player holds with
/// it, lets go of it where the player throws or sets it down, and finds
/// the object a pick-up starts with on the next step if action is held
/// then.
pub(crate) fn after(world: &mut World) {
    world.player.pick = None;
    let physics = world.player_physics();
    let Some(hands) = world.player.hands else {
        if physics[Param::PickUpTime] > Fixed::ZERO && world.player.grounded() && can_hold(world) {
            world.player.pick = in_reach(world);
        }
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
        world.player.thrown = None;
        // A character lost on a hit lets go on the hit frame, the way it
        // faced before the hit turned it.
        let lost_now = world.loss.filter(|l| l.frame == 0);
        let facing = lost_now.map_or(world.player.facing, |l| l.faced);
        let let_go =
            (world.player.knockout.is_some() || walking_out || lost_now.is_some()) && let_go_speed;
        let thing = &mut world.things[hands.thing];
        if thing.phase == Phase::Held {
            if let_go {
                let held = frames(physics[Param::KnockoutHold]).saturating_sub(1);
                let holds_character = thing.archetype.as_ref().and_then(holder).is_some();
                let wait = if holds_character || walking_out || lost_now.is_some() {
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
    // A pick-up whose object broke in the hands ends with nothing in
    // them.
    let picked = frames(physics[Param::PickUpTime]);
    if !world.things[hands.thing].active
        && hands.act == Act::PickUp
        && hands.frame.saturating_add(1) >= picked
    {
        world.player.hands = None;
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
    thing.pos = in_hands(p, thing);
    let release = frames(physics[Param::ThrowRelease]);
    let set_down = frames(physics[Param::PutDownSet]);
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
                wait: 0,
                launch: false,
                second: world.player.character != 0,
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

/// Where `thing` is in `player`'s hands, with `x` and `y` whole px: on
/// the frames of a pick-up, a throw or a put-down its path in the hands
/// covers ([`InHands`](crate::InHands)), there; on a pick-up's first frame, where it
/// lies; otherwise on the player's head.
pub(crate) fn in_hands(player: &Player, thing: &Thing) -> Vec2 {
    let (x, y) = (player.pos.x.floor(), ceil(player.pos.y));
    let head = (0, ceil(player.pos.y - HEIGHT));
    let place = match (player.hands, thing.archetype.map(|a| a.behavior)) {
        (Some(hands), Some(Behavior::Carry { in_hands, .. })) => {
            let n = usize::from(hands.frame);
            let (pick_up, throw, put_down, carried) = match in_hands.second {
                Some(second) if player.character != 0 => (
                    second.pick_up,
                    second.throw,
                    second.put_down,
                    second.carried,
                ),
                _ => (
                    in_hands.pick_up,
                    in_hands.throw,
                    in_hands.put_down,
                    HandsPath::default(),
                ),
            };
            match hands.act {
                Act::PickUp if n == 0 => return thing.pos,
                Act::PickUp => pick_up.at(n - 1),
                Act::Throw => throw.at(n),
                Act::PutDown => put_down.at(n),
                Act::Carry => carried.at(0),
            }
        }
        _ => None,
    };
    let (ahead, above) = match place {
        Some((ahead, up)) => (i64::from(ahead), y - i64::from(up)),
        None => head,
    };
    let ahead = match player.facing {
        Facing::Left => -ahead,
        Facing::Right => ahead,
    };
    Vec2::new(
        Fixed::from_int((x + ahead) as i32),
        Fixed::from_int(above as i32),
    )
}

/// The second character's object is gone on the frame before a hit that
/// loses a character, whichever hurt it is going to (`losing-a-character.md`,
/// "Hits while carrying an object"). Only the barrel in the hands counts.
pub(crate) fn lose_second_barrel(world: &mut World) {
    let p = &world.player;
    let Some(hands) = p.hands else {
        return;
    };
    if p.character == 0
        || world.other.is_none()
        || world.swap.is_some()
        || world.protected()
        || world.losing()
        || world.player_physics()[Param::LoseJump] <= Fixed::ZERO
        || world.things[hands.thing].phase != Phase::Held
    {
        return;
    }
    world.things[hands.thing].active = false;
    world.player.hands = None;
}

/// Whether the player can go on holding an object after this step.
fn can_hold(world: &World) -> bool {
    world.player.knockout.is_none()
        && !world.events.contains(&Event::Hurt)
        && !world.losing()
        && world.swap.is_none()
        && world.flow.outcome == Outcome::Playing
}

/// Whether `thing` can be picked up: it rests, or it's a metal barrel
/// rolling along the floor (metal-barrel.md, "Picking it up").
fn lies_or_rolling(thing: &Thing) -> bool {
    let metal = matches!(
        thing.archetype.as_ref().map(|a| &a.behavior),
        Some(Behavior::Carry {
            turns_at_walls: true,
            ..
        })
    );
    match thing.phase {
        Phase::Ready | Phase::Placed => true,
        Phase::Thrown(f) => metal && f.rolling,
        _ => false,
    }
}

/// The object resting within the player's reach, or just set down: the
/// nearest across, if more than one is. A holder lying where it's placed
/// is within reach by its look ([`Holder::still_at`]), its `clock`
/// counting the frames it has been there, the one it appeared on
/// included.
fn in_reach(world: &World) -> Option<usize> {
    let p = &world.player;
    let (x, y) = (p.pos.x.floor(), ceil(p.pos.y));
    let right = p.facing == Facing::Right;
    world
        .things
        .iter()
        .filter(|t| t.active && lies_or_rolling(t))
        .filter_map(|t| {
            let Some(Archetype {
                behavior: Behavior::Carry { reach, holder, .. },
                ..
            }) = t.archetype
            else {
                return None;
            };
            let grab = match holder {
                Some(h) if t.phase == Phase::Ready && !h.still_at(t.clock.saturating_sub(1)) => {
                    h.shaking
                }
                _ => reach,
            };
            let across = t.pos.x.floor() - x;
            grab.holds(right, across, ceil(t.pos.y) - y)
                .then_some((across.abs(), t.index))
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
        wait: 0,
        launch: false,
        second: false,
    });
}

/// Lets the carried object go on a bounce or a push (thrown-objects.md,
/// "Carried into a bounce"): it moves to the player's `x` and flies the
/// way the player faces at `bounce_release_speed`, which `response` 0
/// keeps, after `wait` frames. Returns whether it left; it doesn't when
/// the preset keeps it in the hands.
pub(crate) fn release_on_bounce(world: &mut World, wait: u8) -> bool {
    let physics = world.player_physics();
    let speed = physics[Param::BounceReleaseSpeed];
    let Some(hands) = world.player.hands else {
        return false;
    };
    if speed <= Fixed::ZERO
        || hands.act != Act::Carry
        || world.things[hands.thing].phase != Phase::Held
    {
        return false;
    }
    let ahead = match world.player.facing {
        Facing::Left => -speed,
        Facing::Right => speed,
    };
    let held = in_hands(&world.player, &world.things[hands.thing]);
    let thing = &mut world.things[hands.thing];
    thing.pos = Vec2::new(Fixed::from_int(world.player.pos.x.floor() as i32), held.y);
    thing.phase = Phase::Thrown(Flight {
        vx: ahead,
        vy: physics[Param::BounceReleaseDrop],
        target: ahead,
        response: Fixed::ZERO,
        gravity: physics[Param::ThrowGravity],
        max_fall: physics[Param::ThrowMaxFall],
        thrown: true,
        margin: physics[Param::ThrowLandMargin],
        bounce_min: physics[Param::ThrowBounceMin],
        bounce: physics[Param::ThrowBounce],
        rolling: false,
        age: 0,
        wait,
        launch: true,
        second: world.player.character != 0,
    });
    world.player.hands = None;
    true
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
    if flight.wait > 0 {
        flight.wait -= 1;
        thing.phase = Phase::Thrown(flight);
        return false;
    }
    flight.age = flight.age.saturating_add(1);
    flight.vx = if flight.response > Fixed::ZERO {
        ease(flight.vx, flight.target, flight.response)
    } else {
        flight.target
    };
    let holder = holder(archetype).filter(|_| flight.thrown);
    let launch = flight.launch;
    flight.launch = false;
    if flight.thrown {
        let turns = matches!(
            archetype.behavior,
            Behavior::Carry {
                turns_at_walls: true,
                ..
            }
        );
        if bounce_and_roll(level, thing, flight, slope, holder.is_some(), turns) {
            thing.active = false;
            return true;
        }
        if launch && let Phase::Thrown(f) = &mut thing.phase {
            f.vy = SPEED_GRAIN;
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
/// move would take it into a column that breaks it (see
/// [`columns::breaks_object`]); otherwise it moves across, and then down
/// under gravity in the air or along the floor rolling. In the air, it
/// touches the floor once it's moving down and ends less than `margin`
/// above it, and is put on it: falling at `bounce_min` or faster before
/// this frame, it bounces up at `bounce` of that speed; slower, it rolls
/// from then on. Rolling off the floor, it falls again.
///
/// One that `holds` the missing character breaks instead of bouncing.
/// Breaking against a wall, it stays where it is across but still falls
/// in its own column, and touching the floor there, is put on it.
/// Returns whether it broke holding the character; it's left where it
/// broke.
///
/// One that `turns` doesn't break at a wall (metal-barrel.md): it doesn't
/// move across, losing the fraction of a pixel of its `x`, and goes on at
/// the speed it rolls at, away from the wall, while falling, bouncing or
/// rolling in its own column as it would have.
fn bounce_and_roll(
    level: &Level,
    thing: &mut Thing,
    mut flight: Flight,
    slope: Fixed,
    holds: bool,
    turns: bool,
) -> bool {
    let pos = thing.pos;
    let mut x = pos.x + flight.vx;
    let mut to = column(x);
    let dir = if flight.right() { 1 } else { -1 };
    let from = column(pos.x);
    let mut wall = columns::breaks_object(level, from, to, dir, pos.y, slope);
    if wall && turns && !holds {
        wall = false;
        x = Fixed::from_int(pos.x.floor() as i32);
        to = from;
        flight.target = -Fixed::from_int(dir) * flight.target.abs();
        flight.vx = flight.target;
    }
    if wall && holds {
        let (y, touch) = fall(level, from, pos.y, &mut flight);
        thing.pos = Vec2::new(pos.x, touch.unwrap_or(y));
        return true;
    }
    if wall {
        thing.active = false;
        thing.phase = Phase::Broken;
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
