//! Which of its own pictures the player shows when it's held still: the
//! stop after a roll, a jump and its landing, a bounce, both
//! characters' falls from a steep face and their landings, the standing
//! pictures after them, the roll, walking, running and turning
//! pictures, on the ground and in the air after walking off a ledge
//! without a face, crouching, and picking up, carrying, throwing and
//! putting down, being hurt, and the first character's ground action
//! (`spec/behavior/player-frames.md`).
//!
//! The pictures come from a character's sheet in a pack
//! (`character-<slot>`, as the importer writes them), named by their
//! labels in `player-frames.md`. [`PictureClock`] follows the player
//! frame by frame and gives the label for the frame, with the set it
//! belongs to; the player is drawn with it only if its sheet has the
//! whole set ([`SETS`]), and with the engine's own art otherwise. Roster
//! slot 0 shows the first character's pictures and slot 1 the second's.
//!
//! Engine choices where the spec says nothing:
//!
//! - The second character's walking, running and turning pictures,
//!   given on flat ground, show on any ground too, and go on in the air
//!   after a walk-off from a ledge without a face as when it passes over
//!   a face without a frame on it.
//! - Either character shows its fall pictures (`s-air-1`, first
//!   character `air-0`) on every frame that ends on a steep face, not
//!   only at the top of a ledge, and its landing frames after a fall
//!   from a face wherever it lands.
//! - The first character walking or running in the air after walking
//!   off a ledge with a face shows the engine's own art.
//! - The crouching pictures, given on flat floor, show on any ground,
//!   in a crouch started on a landing's frames too. A crouch that leaves
//!   the ground shows the engine's own art.
//! - The second character's roll list goes on on slopes as on flat
//!   floor.
//! - The second character's landing pictures are shown after a jump
//!   that lands on the floor it took off from or lower: the spec gives
//!   them for landings on the fourth or a later `s-air-6` frame, which
//!   every jump measured onto the floor it took off from is, but not
//!   when `s-air-1` ends. A jump out of a roll pressed in the air takes
//!   off from where it was in the fall.
//! - The carrying pictures, given on flat floor, show
//!   on any ground. A pick-up, throw or put-down that leaves the ground,
//!   and carrying in a fall that isn't a jump, show the engine's own art.
//! - Being hurt shows a character's pictures only when it is alone in
//!   the level and knocked out; a hit with another character present
//!   and a knock-out with a hop show the engine's own art. A flight whose first landing comes before the
//!   thirtieth frame is timed by the same table.
//! - Jumps that land before their approach frame, landings after falls
//!   (but those from a steep face), and anything
//!   while swapping, hurt or losing a life show the engine's own art.
//! - The standing list goes on on a slope or at a ledge as on flat
//!   floor, for either character.

use clamber_pack::Param;
use clamber_sim::{Act, Event, Gait, Hands, Outcome, State, World};

/// The sets of pictures shown together, each shown only when all of it
/// is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    /// The stop after a roll, or carrying on into running.
    Stop,
    /// The standing pictures.
    Standing,
    /// Teetering at the end of a floor.
    Teeter,
    /// A jump and its landing.
    Jump,
    /// A bounce, until the landing.
    Bounce,
    /// The roll cycle, or the second character's roll list.
    Roll,
    /// Walking, running and turning on the ground.
    Walking,
    /// Crouching and getting up.
    Crouch,
    /// Picking up, carrying, throwing and putting down an object.
    Carry,
    /// Being hurt, from the hit to the end of the level.
    Hurt,
    /// The first character's ground action.
    Act,
    /// The first character on a rope.
    Rope,
    /// The first character swimming.
    Swim,
    /// The pictures the leading character waits with in a scene.
    Scene,
    /// The pictures of the characters on the level map.
    Map,
    /// The lead's pictures on the map after a first finish.
    MapLeadFinish,
    /// The other character's pictures on the map after a first finish.
    MapOtherFinish,
    /// The lead's pictures on the map at a new game.
    MapStart,
}

/// Each set's labels, for the first character (slot 0) and the second
/// (slot 1).
pub const SETS: [&[(Set, &[&str])]; 2] = [
    &[
        (Set::Stop, &["roll-end", "stop", "stand-1"]),
        (Set::Standing, &STANDING),
        (Set::Teeter, &TEETER),
        (
            Set::Jump,
            &[
                "jump-1", "jump-2", "rise-1", "rise-2", "rise-3", "rise-4", "rise-5", "rise-6",
                "rise-7", "rise-8", "air-0", "air-1", "air-2", "air-3", "air-4", "air-5", "air-6",
                "land-1", "land-2", "land-3",
            ],
        ),
        (
            Set::Bounce,
            &[
                "bounce-1",
                "bounce-2",
                "bounce-3",
                "bounce-4",
                "bounce-5",
                "bounce-6",
                "bounce-7",
                "bounce-8",
                "bounce-9",
                "bounce-10",
                "bounce-11",
                "bounce-12",
                "bounce-13",
                "bounce-14",
                "bounce-15",
                "bounce-16",
            ],
        ),
        (
            Set::Roll,
            &[
                "roll-1", "roll-2", "roll-3", "roll-4", "roll-5", "roll-6", "roll-7", "roll-8",
                "roll-9", "roll-end", "stop",
            ],
        ),
        (Set::Walking, &WALKING),
        (Set::Crouch, &CROUCH),
        (Set::Carry, &CARRYING),
        (Set::Hurt, &HURT),
        (Set::Act, &ACT),
        (Set::Rope, &ROPE),
        (Set::Swim, &SWIM),
        (Set::Scene, &FIRST_SCENE),
        (Set::Map, &MAP),
        (Set::MapLeadFinish, &MAP_LEAD_FINISH),
        (Set::MapOtherFinish, &MAP_OTHER_FINISH),
        (Set::MapStart, &MAP_START),
    ],
    &[
        (
            Set::Stop,
            &["s-stop-1", "s-stop-2", "s-stop-3", "s-stop-4", "s-stop-5"],
        ),
        (Set::Standing, &SECOND_STANDING),
        (Set::Teeter, &SECOND_TEETER),
        (
            Set::Jump,
            &[
                "s-jump-1", "s-jump-2", "s-rise-1", "s-rise-2", "s-rise-3", "s-rise-4", "s-rise-5",
                "s-rise-6", "s-rise-7", "s-air-1", "s-air-2", "s-air-3", "s-air-4", "s-air-5",
                "s-air-6", "s-land-1", "s-land-2", "s-land-3", "s-land-4",
            ],
        ),
        (Set::Bounce, &SECOND_BOUNCE),
        (Set::Roll, &SECOND_ROLL),
        (Set::Walking, &SECOND_WALKING),
        (Set::Crouch, &SECOND_CROUCH),
        (Set::Carry, &SECOND_CARRYING),
        (Set::Hurt, &SECOND_HURT),
        (Set::Scene, &SECOND_SCENE),
    ],
];

/// The sheet the level map's pictures are in, drawn in the map's palette.
pub const MAP_SHEET: &str = "map-characters";

/// The lead's map pictures, then the other character's.
#[rustfmt::skip]
pub const MAP: [&str; 18] = [
    "map-lead-1", "map-lead-2", "map-lead-3", "map-lead-4", "map-lead-5", "map-lead-6",
    "map-lead-7", "map-lead-8", "map-lead-9", "map-lead-10", "map-other-1", "map-other-2",
    "map-other-3", "map-other-4", "map-other-5", "map-other-6", "map-other-7", "map-other-8",
];

/// The lead's map pictures after a level is finished for the first time.
#[rustfmt::skip]
pub const MAP_LEAD_FINISH: [&str; 11] = [
    "map-lead-finish-1", "map-lead-finish-2", "map-lead-finish-3", "map-lead-finish-4",
    "map-lead-finish-5", "map-lead-finish-6", "map-lead-finish-7", "map-lead-finish-8",
    "map-lead-finish-9", "map-lead-finish-10", "map-lead-finish-11",
];

/// The other character's, as [`MAP_LEAD_FINISH`].
#[rustfmt::skip]
pub const MAP_OTHER_FINISH: [&str; 11] = [
    "map-other-finish-1", "map-other-finish-2", "map-other-finish-3", "map-other-finish-4",
    "map-other-finish-5", "map-other-finish-6", "map-other-finish-7", "map-other-finish-8",
    "map-other-finish-9", "map-other-finish-10", "map-other-finish-11",
];

/// The lead's map pictures at a new game, before the first level's map.
#[rustfmt::skip]
pub const MAP_START: [&str; 9] = [
    "map-start-1", "map-start-2", "map-start-3", "map-start-4", "map-start-5", "map-start-6",
    "map-start-7", "map-start-8", "map-start-9",
];

/// The first character's pictures while it waits in the scene.
#[rustfmt::skip]
const FIRST_SCENE: [&str; 13] = [
    "scene-1", "scene-2", "scene-3", "scene-4", "scene-5", "scene-6", "scene-7", "scene-8",
    "scene-9", "scene-10", "scene-11", "scene-12", "scene-13",
];

/// The second character's pictures while it waits in the scene.
#[rustfmt::skip]
const SECOND_SCENE: [&str; 17] = [
    "s-scene-1", "s-scene-2", "s-scene-3", "s-scene-4", "s-scene-5", "s-scene-6", "s-scene-7",
    "s-scene-8", "s-scene-9", "s-scene-10", "s-scene-11", "s-scene-12", "s-scene-13",
    "s-scene-14", "s-scene-15", "s-scene-16", "s-scene-17",
];

/// The first character's walking, running and turning pictures: a
/// walking and a running picture per step of the stride cycle, then the
/// two turning pictures.
const WALKING: [&str; 42] = [
    "walk-1", "walk-2", "walk-3", "walk-4", "walk-5", "walk-6", "walk-7", "walk-8", "walk-9",
    "walk-10", "walk-11", "walk-12", "walk-13", "walk-14", "walk-15", "walk-16", "walk-17",
    "walk-18", "walk-19", "walk-20", "run-1", "run-2", "run-3", "run-4", "run-5", "run-6", "run-7",
    "run-8", "run-9", "run-10", "run-11", "run-12", "run-13", "run-14", "run-15", "run-16",
    "run-17", "run-18", "run-19", "run-20", "turn-a", "turn-b",
];

/// The second character's walking, running and turning pictures: one
/// per step of its walking and running cycles, then the two pictures
/// its turns show.
#[rustfmt::skip]
const SECOND_WALKING: [&str; 31] = [
    "s-walk-1", "s-walk-2", "s-walk-3", "s-walk-4", "s-walk-5", "s-walk-6", "s-walk-7",
    "s-walk-8", "s-walk-9", "s-walk-10", "s-walk-11", "s-walk-12", "s-walk-13", "s-walk-14",
    "s-walk-15", "s-walk-16", "s-run-1", "s-run-2", "s-run-3", "s-run-4", "s-run-5", "s-run-6",
    "s-run-7", "s-run-8", "s-run-9", "s-run-10", "s-run-11", "s-run-12", "s-run-13",
    "s-stand-5", "s-stand-7",
];

/// The second character's bounce pictures.
#[rustfmt::skip]
const SECOND_BOUNCE: [&str; 16] = [
    "s-bounce-1", "s-bounce-2", "s-bounce-3", "s-bounce-4", "s-bounce-5", "s-bounce-6",
    "s-bounce-7", "s-bounce-8", "s-bounce-9", "s-bounce-10", "s-bounce-11", "s-bounce-12",
    "s-bounce-13", "s-bounce-14", "s-bounce-15", "s-bounce-16",
];

/// The second character's roll pictures: `s-roll-1` to `s-roll-12`, then
/// the three stop pictures its roll list starts with.
#[rustfmt::skip]
const SECOND_ROLL: [&str; 15] = [
    "s-roll-1", "s-roll-2", "s-roll-3", "s-roll-4", "s-roll-5", "s-roll-6", "s-roll-7",
    "s-roll-8", "s-roll-9", "s-roll-10", "s-roll-11", "s-roll-12", "s-stop-3", "s-stop-4",
    "s-stop-5",
];

/// How many frames before frame 0 of a resume at a restart barrel the
/// standing list is counted from, for the first character with nothing held.
const RESUME_STANDING_BEFORE: u32 = 143;

/// The first character's standing pictures: `stand-1` to `stand-11`,
/// then `wait-1` to `wait-34`.
const STANDING: [&str; 45] = [
    "stand-1", "stand-2", "stand-3", "stand-4", "stand-5", "stand-6", "stand-7", "stand-8",
    "stand-9", "stand-10", "stand-11", "wait-1", "wait-2", "wait-3", "wait-4", "wait-5", "wait-6",
    "wait-7", "wait-8", "wait-9", "wait-10", "wait-11", "wait-12", "wait-13", "wait-14", "wait-15",
    "wait-16", "wait-17", "wait-18", "wait-19", "wait-20", "wait-21", "wait-22", "wait-23",
    "wait-24", "wait-25", "wait-26", "wait-27", "wait-28", "wait-29", "wait-30", "wait-31",
    "wait-32", "wait-33", "wait-34",
];

/// The second character's standing pictures: `s-stand-1` to
/// `s-stand-22`, then `s-wait-1` to `s-wait-17`.
#[rustfmt::skip]
const SECOND_STANDING: [&str; 39] = [
    "s-stand-1", "s-stand-2", "s-stand-3", "s-stand-4", "s-stand-5", "s-stand-6", "s-stand-7",
    "s-stand-8", "s-stand-9", "s-stand-10", "s-stand-11", "s-stand-12", "s-stand-13", "s-stand-14",
    "s-stand-15", "s-stand-16", "s-stand-17", "s-stand-18", "s-stand-19", "s-stand-20",
    "s-stand-21", "s-stand-22", "s-wait-1", "s-wait-2", "s-wait-3", "s-wait-4", "s-wait-5",
    "s-wait-6", "s-wait-7", "s-wait-8", "s-wait-9", "s-wait-10", "s-wait-11", "s-wait-12",
    "s-wait-13", "s-wait-14", "s-wait-15", "s-wait-16", "s-wait-17",
];

/// The first character's teetering pictures: `teeter-1` to `teeter-22`.
#[rustfmt::skip]
const TEETER: [&str; 22] = [
    "teeter-1", "teeter-2", "teeter-3", "teeter-4", "teeter-5", "teeter-6", "teeter-7",
    "teeter-8", "teeter-9", "teeter-10", "teeter-11", "teeter-12", "teeter-13", "teeter-14",
    "teeter-15", "teeter-16", "teeter-17", "teeter-18", "teeter-19", "teeter-20", "teeter-21",
    "teeter-22",
];

/// The second character's teetering pictures: `s-teeter-1` to
/// `s-teeter-18`.
#[rustfmt::skip]
const SECOND_TEETER: [&str; 18] = [
    "s-teeter-1", "s-teeter-2", "s-teeter-3", "s-teeter-4", "s-teeter-5", "s-teeter-6",
    "s-teeter-7", "s-teeter-8", "s-teeter-9", "s-teeter-10", "s-teeter-11", "s-teeter-12",
    "s-teeter-13", "s-teeter-14", "s-teeter-15", "s-teeter-16", "s-teeter-17", "s-teeter-18",
];

/// The first character's crouching pictures.
#[rustfmt::skip]
const CROUCH: [&str; 22] = [
    "crouch-1", "crouch-2", "crouch-3", "crouch-4", "crouch-5", "crouch-6", "crouch-7",
    "crouch-8", "crouch-9", "crouch-10", "crouch-11", "crouch-12", "crouch-13", "crouch-14",
    "crouch-15", "crouch-16", "crouch-17", "crouch-18", "crouch-19", "crouch-20", "crouch-21",
    "crouch-22",
];

/// The first character's ground action pictures: `act-1` to `act-30`.
#[rustfmt::skip]
const ACT: [&str; 30] = [
    "act-1", "act-2", "act-3", "act-4", "act-5", "act-6", "act-7", "act-8", "act-9", "act-10",
    "act-11", "act-12", "act-13", "act-14", "act-15", "act-16", "act-17", "act-18", "act-19",
    "act-20", "act-21", "act-22", "act-23", "act-24", "act-25", "act-26", "act-27", "act-28",
    "act-29", "act-30",
];

/// The first character's swimming pictures.
const SWIM: [&str; 15] = [
    "swim-1", "swim-2", "swim-3", "swim-4", "swim-5", "swim-6", "swim-7", "swim-8", "swim-9",
    "swim-10", "swim-11", "swim-12", "swim-13", "swim-14", "swim-15",
];

/// The first character's rope pictures.
const ROPE: [&str; 12] = [
    "rope-hang-1",
    "rope-hang-2",
    "rope-hang-3",
    "rope-hang-4",
    "rope-turn-1",
    "rope-turn-2",
    "rope-climb-1",
    "rope-climb-2",
    "rope-climb-3",
    "rope-climb-4",
    "rope-climb-5",
    "rope-climb-6",
];

/// The hanging cycle, 9 frames each ("On a rope", "Hanging").
const HANG: [&str; 6] = [
    "rope-hang-1",
    "rope-hang-2",
    "rope-hang-3",
    "rope-hang-4",
    "rope-hang-3",
    "rope-hang-2",
];

/// The climbing cycle ("Climbing").
const CLIMB: [&str; 10] = [
    "rope-climb-1",
    "rope-climb-2",
    "rope-climb-3",
    "rope-climb-4",
    "rope-climb-5",
    "rope-climb-6",
    "rope-climb-5",
    "rope-climb-4",
    "rope-climb-3",
    "rope-climb-2",
];

/// The first character's swimming cycle ("Swimming"): a counter that
/// falls by a rate each frame and is reloaded by 1024 when it reaches 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Swim {
    /// Index into [`SWIM`].
    picture: usize,
    counter: i32,
    /// Frames since the last stroke, 0 on its frame.
    stroked: Option<u32>,
}

impl Swim {
    /// The cycle on a swimming frame, given the frame before's, if it
    /// was swimming too. Where it stands on the first swimming frame
    /// isn't covered: the first picture with a full counter.
    fn next(before: Option<Swim>, stroke: bool) -> Swim {
        let Some(before) = before else {
            return Swim {
                picture: 0,
                counter: 1024,
                stroked: stroke.then_some(0),
            };
        };
        let stroked = if stroke {
            Some(0)
        } else {
            before.stroked.map(|k| k.saturating_add(1))
        };
        let rate = stroked.map_or(256, |k| 256.max(752 - 16 * i64::from(k.min(31)))) as i32;
        let counter = before.counter - rate;
        if counter > 0 {
            Swim {
                counter,
                stroked,
                ..before
            }
        } else {
            Swim {
                picture: (before.picture + 1) % SWIM.len(),
                counter: counter + 1024,
                stroked,
            }
        }
    }
}

/// What the first character does on a rope, as the pictures follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnRope {
    /// Turning round the rope.
    Turn,
    /// Hanging, this many frames since the cycle started.
    Hang(u32),
    /// Climbing: the picture in the cycle, the climb counter, and the
    /// direction (1 up, -1 down) that started the cycle.
    Climb { at: u8, counter: i32, dir: i8 },
}

/// The first character leaving a rope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OffRope {
    /// Jumping off, this many frames after P, and the frames since `vy`
    /// first was above 0.
    Jump { age: u8, falling: Option<u8> },
    /// Sliding off the end, this many frames after the let-go frame. The
    /// list holds from F on, landed or not.
    Slide(u8),
}

/// The first character's picture `k` frames after F, the first frame
/// after the let-go frame of a slide off the end of a rope, for the 30
/// frames F to F + 29 ("After the rope").
fn slide_off(k: u8) -> &'static str {
    const AIR: [&str; 5] = ["air-6", "air-5", "air-4", "air-3", "air-4"];
    match k {
        0 => "land-2",
        1..=2 => "land-1",
        3..=12 => AIR[usize::from(k - 3) / 2],
        13..=15 => "air-5",
        16..=19 => "air-6",
        20..=23 => "land-1",
        24..=26 => "land-2",
        _ => "land-3",
    }
}

/// Where the list of [`slide_off`] ends.
const SLIDE_LAST: u8 = 29;

/// The first character's being-hurt pictures: `hurt-1` to `hurt-14` and
/// `lie-1` to `lie-4`.
#[rustfmt::skip]
const HURT: [&str; 18] = [
    "hurt-1", "hurt-2", "hurt-3", "hurt-4", "hurt-5", "hurt-6", "hurt-7", "hurt-8", "hurt-9",
    "hurt-10", "hurt-11", "hurt-12", "hurt-13", "hurt-14", "lie-1", "lie-2", "lie-3", "lie-4",
];
/// Where `lie-1` is in [`HURT`].
const LIE_1: usize = 14;

/// The second character's being-hurt pictures: `s-hurt-1` to
/// `s-hurt-14` and `s-lie-1` to `s-lie-3`.
#[rustfmt::skip]
const SECOND_HURT: [&str; 17] = [
    "s-hurt-1", "s-hurt-2", "s-hurt-3", "s-hurt-4", "s-hurt-5", "s-hurt-6", "s-hurt-7",
    "s-hurt-8", "s-hurt-9", "s-hurt-10", "s-hurt-11", "s-hurt-12", "s-hurt-13", "s-hurt-14",
    "s-lie-1", "s-lie-2", "s-lie-3",
];

/// The index into [`SECOND_HURT`] of the picture `k` frames after the
/// hit H, as [`hurt`] ("Being hurt", "Second character").
fn second_hurt(k: u16, landings: [Option<u16>; 3]) -> usize {
    let since = |at: Option<u16>| at.and_then(|at| k.checked_sub(at));
    if let Some(f) = since(landings[2]) {
        // The last frame of each stretch of the lying pictures, and the
        // picture; after the table, the last one goes on.
        const LYING: [(u16, usize); 11] = [
            (29, 13),
            (35, 14),
            (41, 15),
            (52, 16),
            (57, 15),
            (67, 14),
            (72, 15),
            (81, 16),
            (85, 15),
            (93, 14),
            (97, 15),
        ];
        return match f {
            0..=11 => 10,
            12..=17 => 11,
            18..=23 => 12,
            _ => LYING
                .iter()
                .find(|&&(end, _)| f <= end)
                .map_or(16, |&(_, i)| i),
        };
    }
    if since(landings[1]).is_some() {
        return 9;
    }
    if let Some(l) = since(landings[0]) {
        // A flight whose first landing is H + 38 shows each of the next
        // two pictures for 5 frames.
        let long = landings[0] == Some(38);
        return match l {
            0..=3 => 6,
            4 if long => 6,
            4..=8 => 7,
            9 if long => 7,
            _ => 8,
        };
    }
    if k < 13 {
        0
    } else {
        usize::from((k - 13) / 5 + 1).min(5)
    }
}

/// The index into [`HURT`] of the picture `k` frames after the hit H,
/// given the frames after H of the first and second landing and of the
/// third, F ("Being hurt").
fn hurt(k: u16, landings: [Option<u16>; 3]) -> usize {
    let since = |at: Option<u16>| at.and_then(|at| k.checked_sub(at));
    if let Some(f) = since(landings[2]) {
        return match f {
            0..=27 => 10,
            28..=33 => 11,
            34..=39 => 12,
            40..=45 => 13,
            _ => LIE_1 + [0, 1, 2, 3, 2, 1][usize::from((f - 46) / 3 % 6)],
        };
    }
    if since(landings[1]).is_some() {
        return 9;
    }
    if let Some(l) = since(landings[0]) {
        return match l {
            0..=3 => 6,
            4..=8 => 7,
            _ => 8,
        };
    }
    if k < 10 {
        0
    } else {
        usize::from(k.min(30) / 5) - 1
    }
}

/// The first character's carrying pictures: `pick-1` to `pick-7`,
/// `carry-turn`, `carry-1` to `carry-10`, `throw-1` to `throw-19` and
/// `hold-1` to `hold-3`; the indexes below are into this list.
#[rustfmt::skip]
const CARRYING: [&str; 40] = [
    "pick-1", "pick-2", "pick-3", "pick-4", "pick-5", "pick-6", "pick-7", "carry-turn",
    "carry-1", "carry-2", "carry-3", "carry-4", "carry-5", "carry-6", "carry-7", "carry-8",
    "carry-9", "carry-10",
    "throw-1", "throw-2", "throw-3", "throw-4", "throw-5", "throw-6", "throw-7", "throw-8",
    "throw-9", "throw-10", "throw-11", "throw-12", "throw-13", "throw-14", "throw-15",
    "throw-16", "throw-17", "throw-18", "throw-19",
    "hold-1", "hold-2", "hold-3",
];
const PICK_7: usize = 6;
const CARRY_TURN: usize = 7;
const CARRY_1: usize = 8;
const THROW_1: usize = 18;
const THROW_19: usize = 36;
const HOLD_1: usize = 37;

/// The second character's carrying pictures: `s-pick-1` to `s-pick-5`,
/// `s-hold`, `s-carry-1` to `s-carry-8` and `s-throw-1` to `s-throw-14`.
#[rustfmt::skip]
const SECOND_CARRYING: [&str; 28] = [
    "s-pick-1", "s-pick-2", "s-pick-3", "s-pick-4", "s-pick-5", "s-hold",
    "s-carry-1", "s-carry-2", "s-carry-3", "s-carry-4", "s-carry-5", "s-carry-6", "s-carry-7",
    "s-carry-8",
    "s-throw-1", "s-throw-2", "s-throw-3", "s-throw-4", "s-throw-5", "s-throw-6", "s-throw-7",
    "s-throw-8", "s-throw-9", "s-throw-10", "s-throw-11", "s-throw-12", "s-throw-13",
    "s-throw-14",
];
const S_PICK_5: usize = 4;
const S_HOLD: usize = 5;
const S_CARRY_1: usize = 6;
const S_THROW_1: usize = 14;
/// How many frames each of `s-throw-1` to `s-throw-14` shows.
const S_THROW_FRAMES: [u8; 14] = [1, 2, 3, 5, 3, 2, 2, 2, 3, 5, 3, 3, 3, 3];

/// The second character's crouching pictures.
#[rustfmt::skip]
const SECOND_CROUCH: [&str; 13] = [
    "s-crouch-1", "s-crouch-2", "s-crouch-3", "s-crouch-4", "s-crouch-5", "s-crouch-6",
    "s-crouch-7", "s-crouch-8", "s-crouch-9", "s-crouch-10", "s-crouch-11", "s-crouch-12",
    "s-crouch-13",
];

/// A character's crouch ("Crouching"): which picture, for how many
/// frames, going down from D and getting up from U; the picture held
/// after going down, for how many frames before the first sway and
/// between sways; and the sway.
struct CrouchList {
    labels: &'static [&'static str],
    down: &'static [(u8, u8)],
    held: (u8, u32, u32),
    sway: &'static [(u8, u8)],
    up: &'static [(u8, u8)],
}

#[rustfmt::skip]
const CROUCHING: [CrouchList; 2] = [
    CrouchList {
        labels: &CROUCH,
        down: &[(1, 1), (2, 2), (3, 2), (4, 2), (5, 2), (7, 2), (8, 2)],
        held: (9, 17, 144),
        sway: &[
            (10, 3), (11, 3), (12, 3), (13, 17),
            (14, 3), (15, 3), (16, 17),
            (17, 3), (18, 3), (19, 3), (20, 3), (21, 3), (22, 17),
            (21, 3), (20, 3), (19, 3), (18, 3), (17, 3), (16, 17),
            (15, 3), (14, 3), (13, 3), (12, 3), (11, 3), (10, 3),
        ],
        up: &[(9, 1), (8, 1), (7, 1), (6, 1), (5, 1), (4, 1), (3, 1), (2, 1), (1, 1)],
    },
    CrouchList {
        labels: &SECOND_CROUCH,
        down: &[(1, 1), (2, 2), (3, 2), (4, 2), (5, 2)],
        held: (6, 15, 142),
        sway: &[
            (7, 5), (8, 5), (9, 17),
            (10, 3), (11, 3), (12, 3), (13, 17),
            (12, 3), (11, 3), (10, 3), (9, 17),
            (8, 3), (7, 3),
        ],
        up: &[(6, 2), (5, 2), (4, 2), (3, 2), (2, 2), (1, 2)],
    },
];

/// Frame `i` of `runs` of (picture, frames), if it's that long.
fn in_runs(runs: &[(u8, u8)], mut i: u32) -> Option<u8> {
    for &(n, count) in runs {
        if i < u32::from(count) {
            return Some(n);
        }
        i -= u32::from(count);
    }
    None
}

/// The frames `runs` lasts.
fn runs_len(runs: &[(u8, u8)]) -> u32 {
    runs.iter().map(|&(_, count)| u32::from(count)).sum()
}

/// On D + `d` of a crouch ("Crouching"), going down or held.
fn crouching(first: bool, d: u32) -> Option<(Set, &'static str)> {
    let list = &CROUCHING[usize::from(!first)];
    let down = runs_len(list.down);
    let (held, before, between) = list.held;
    let n = match d.checked_sub(down) {
        None => in_runs(list.down, d)?,
        Some(h) => match h.checked_sub(before) {
            None => held,
            Some(i) => in_runs(list.sway, i % (runs_len(list.sway) + between)).unwrap_or(held),
        },
    };
    Some((Set::Crouch, list.labels[usize::from(n) - 1]))
}

/// On U + `u` of getting up from a crouch ("Crouching").
fn getting_up(first: bool, u: u32) -> Option<(Set, &'static str)> {
    let list = &CROUCHING[usize::from(!first)];
    let n = in_runs(list.up, u)?;
    Some((Set::Crouch, list.labels[usize::from(n) - 1]))
}

/// On frame `f` of `act`, from G or Q, on the ground ("Carrying",
/// "First character"): the picture of a pick-up, a throw or a put-down.
fn handling(act: Act, f: u8) -> Option<usize> {
    let k = usize::from(f.saturating_sub(1) / 2);
    match (act, f) {
        (Act::PickUp, 0) => Some(0),
        (Act::PickUp, 1..=10) => Some(1 + k),
        (Act::PickUp, _) => Some(PICK_7),
        (Act::Throw, 0) => Some(THROW_1),
        (Act::Throw, 1..=34) => Some(THROW_1 + 1 + k),
        (Act::Throw, _) => Some(THROW_19),
        (Act::PutDown, 0) => Some(PICK_7),
        (Act::PutDown, 1..=12) => Some(PICK_7 - 1 - k),
        // Q + 13 is a standing frame, shown by the standing list.
        _ => None,
    }
}

/// The second character on frame `f` of `act`, from G or Q, on the
/// ground ("Carrying", "Second character"). G + 10 is a frame of
/// standing with the object.
fn second_handling(act: Act, f: u8) -> Option<usize> {
    match (act, f) {
        (Act::PickUp, 0..=9) => Some(usize::from(f / 2)),
        (Act::PickUp, 10) => Some(S_HOLD),
        (Act::Throw, 0..=39) => {
            let mut left = f;
            S_THROW_FRAMES.iter().enumerate().find_map(|(n, &frames)| {
                let here = left < frames;
                left = left.saturating_sub(frames);
                here.then_some(S_THROW_1 + n)
            })
        }
        (Act::PutDown, 0..=9) => Some(S_PICK_5 - usize::from(f / 2)),
        _ => None,
    }
}

/// The picture of standing with the object `c` frames from S: `hold-1`
/// × 12, `hold-2` × 9, `hold-3` × 12, `hold-2` × 9, over and over.
fn holding(c: u32) -> usize {
    HOLD_1
        + match c % 42 {
            0..=11 => 0,
            12..=20 | 33..=41 => 1,
            _ => 2,
        }
}

/// The second character's back and forth between `s-wait-14` and
/// `s-wait-17` ("Standing still"), 70 frames: which `s-wait-` picture,
/// for how many frames.
#[rustfmt::skip]
const SECOND_SWING: [(u8, u8); 18] = [
    (14, 2), (15, 2), (16, 2), (17, 2), (16, 2), (15, 2), (14, 2), (15, 2), (16, 2),
    (17, 3), (16, 3), (15, 3), (14, 3), (15, 3),
    (16, 4), (17, 5), (16, 6), (15, 22),
];

/// The sway ("Standing still"), 186 frames: which `stand-` picture, for
/// how many frames.
#[rustfmt::skip]
const SWAY: [(u8, u8); 39] = [
    (2, 3), (3, 3), (4, 3), (5, 3), (6, 10),
    (7, 3), (8, 3), (9, 3), (10, 3), (11, 15),
    (10, 3), (9, 3), (8, 3), (7, 3), (6, 5),
    (7, 3), (8, 3), (9, 3), (10, 3), (11, 10),
    (10, 3), (9, 3), (8, 3), (7, 3), (6, 15),
    (5, 3), (4, 3), (3, 3), (2, 3), (1, 25),
    (2, 3), (3, 3), (4, 3), (5, 3), (6, 10),
    (5, 3), (4, 3), (3, 3), (2, 3),
];

/// The label of the picture nearest to what a character in roster slot
/// `slot` does in `state`, for when it has none for this frame: the
/// first of the state's own list that `has` says the character's sheet
/// has, else its first standing picture, else any picture it has.
pub fn stand_in(slot: usize, state: State, has: impl Fn(&str) -> bool) -> Option<&'static str> {
    let first = slot == 0;
    let near: &[Set] = match state {
        State::Walk => &[Set::Walking],
        State::Run => &[Set::Walking],
        State::Roll => &[Set::Roll],
        State::Jump | State::Fall => &[Set::Jump],
        State::Bounce => &[Set::Bounce, Set::Jump],
        State::Hurt => &[Set::Hurt],
        State::GroundAction => &[Set::Act, Set::Crouch],
        State::Hold => &[Set::Rope],
        State::Swim => &[Set::Swim],
        _ => &[],
    };
    let air = |labels: &[&'static str]| match state {
        State::Fall => labels.iter().copied().find(|l| l.contains("air-")),
        _ => None,
    };
    let own = near.iter().find_map(|&set| {
        let labels = labels(slot, set);
        let by_run = |prefix: &str| {
            let prefix = format!("{}{prefix}", if first { "" } else { "s-" });
            labels
                .iter()
                .copied()
                .find(|l| l.starts_with(&prefix) && has(l))
        };
        match state {
            State::Run => by_run("run-"),
            State::Walk => by_run("walk-"),
            State::Jump => by_run("rise-"),
            _ => None,
        }
        .or_else(|| air(labels).filter(|l| has(l)))
        .or_else(|| labels.iter().copied().find(|l| has(l)))
    });
    own.or_else(|| {
        let sets = SETS.get(slot)?;
        let standing = sets.iter().find(|(s, _)| *s == Set::Standing);
        standing
            .into_iter()
            .chain(sets.iter())
            .flat_map(|(_, labels)| labels.iter().copied())
            .find(|l| has(l))
    })
}

/// The labels of `set` for the character in roster slot `slot`.
pub fn labels(slot: usize, set: Set) -> &'static [&'static str] {
    SETS.get(slot)
        .and_then(|sets| sets.iter().find(|(s, _)| *s == set))
        .map_or(&[], |&(_, labels)| labels)
}

/// The frame just shown in a scene, counting from 0, and the scene's
/// last frame for the character leading; None outside a scene.
fn scene_frame(world: &World) -> Option<(u32, u32)> {
    let scene = world.level.scene.as_ref()?;
    let cut = if world.player.character == 0 {
        0
    } else {
        scene.shorter
    };
    Some((world.frame.checked_sub(1)?, scene.last - cut))
}

/// The first character's `run-n` picture.
fn run(n: usize) -> (Set, &'static str) {
    (Set::Walking, WALKING[19 + n])
}

/// The second character's `s-run-n` picture.
fn second_run(n: usize) -> (Set, &'static str) {
    (Set::Walking, SECOND_WALKING[15 + n])
}

/// The picture of the character in roster slot `slot` leading in a scene
/// on its frame `frame`, counting from 0, when the scene's last frame is
/// `last` (`spec/behavior/exits.md`, "Pictures in the scene"). None for
/// the black frames, the last two, and the frames the spec didn't read
/// before the walk ends.
///
/// Engine choices where the spec says nothing: the second character's
/// frames 5 and 7, "not read", show the picture of the frame before, and
/// its dark frames 363 and 364 go on with its last picture.
pub fn scene_lead(slot: usize, frame: u32, last: u32) -> Option<(Set, &'static str)> {
    if frame < 3 || frame + 1 >= last {
        return None;
    }
    let f = frame as usize;
    let scene = |n: usize| {
        let labels = labels(slot, Set::Scene);
        (Set::Scene, labels[n - 1])
    };
    if slot == 0 {
        return Some(match f {
            3 => run(2),
            4 => run(3),
            5..=27 => {
                let (m, r) = ((f - 5) / 3, (f - 5) % 3);
                run(if r < 2 { 4 + 2 * m } else { 5 + 2 * m })
            }
            28..=99 => {
                // `scene-1` to `scene-7`, for how many frames each.
                const COUNTS: [usize; 7] = [7, 6, 6, 21, 14, 9, 9];
                let mut at = f - 28;
                let n = COUNTS.iter().position(|&c| {
                    let here = at < c;
                    at = at.saturating_sub(c);
                    here
                })?;
                scene(n + 1)
            }
            _ => {
                // The 62-frame list: pictures 8 to 11, back down to 8,
                // 6 frames each, then 12, 13, 12 on 6, 8 and 6.
                const LIST: [(usize, usize); 10] = [
                    (8, 6),
                    (9, 6),
                    (10, 6),
                    (11, 6),
                    (10, 6),
                    (9, 6),
                    (8, 6),
                    (12, 6),
                    (13, 8),
                    (12, 6),
                ];
                let mut at = (f - 100) % 62;
                let &(n, _) = LIST.iter().find(|&&(_, c)| {
                    let here = at < c;
                    at = at.saturating_sub(c);
                    here
                })?;
                scene(n)
            }
        });
    }
    Some(match f {
        3 => second_run(2),
        4 | 5 => second_run(3),
        6 | 7 => second_run(4),
        8 => second_run(5),
        9..=14 => second_run(6 + (f - 9) / 2),
        15 => second_run(9),
        16..=23 => second_run(10 + (f - 16) / 2),
        24 => second_run(1),
        25 | 26 => second_run(2),
        27 => second_run(3),
        28..=129 => {
            // `s-scene-1` to `s-scene-14`, for how many frames each.
            const COUNTS: [usize; 14] = [4, 19, 3, 3, 3, 3, 3, 3, 3, 3, 26, 3, 4, 22];
            let mut at = f - 28;
            let n = COUNTS.iter().position(|&c| {
                let here = at < c;
                at = at.saturating_sub(c);
                here
            })?;
            scene(n + 1)
        }
        130..=336 => {
            // Nine rounds of 27 − k frames, k = 0 to 8.
            let mut at = f - 130;
            let mut k = 0;
            while at >= 27 - k {
                at -= 27 - k;
                k += 1;
            }
            const HEAD: [(usize, usize); 5] = [(15, 3), (16, 3), (17, 5), (16, 2), (15, 5)];
            let n = HEAD
                .iter()
                .find(|&&(_, c)| {
                    let here = at < c;
                    at = at.saturating_sub(c);
                    here
                })
                .map_or(14, |&(n, _)| n);
            scene(n)
        }
        337..=339 => scene(15),
        340..=342 => scene(16),
        _ => scene(17),
    })
}

/// The picture of the other character, `other` the roster slot of it
/// and the one leading in front, on a scene's frame `frame`; the frames
/// the spec read it on, and none otherwise.
pub fn scene_other(other: usize, frame: u32, last: u32) -> Option<(Set, &'static str)> {
    if frame + 1 >= last {
        return None;
    }
    let f = frame as usize;
    if other == 1 {
        return match f {
            27 => Some(second_run(1)),
            28.. => Some((Set::Standing, SECOND_STANDING[0])),
            _ => None,
        };
    }
    match f {
        15..=19 => Some(run(16 + f - 15)),
        20..=28 => Some(run(f - 19)),
        32.. => Some((Set::Standing, STANDING[0])),
        _ => None,
    }
}

/// The stage frames from the approach frame A (`jump-input.md`, "The
/// switch frame M"): when A is T + 20, and when it's later.
const STAGES: [u8; 7] = [0, 2, 4, 6, 8, 11, 15];
const LATE_STAGES: [u8; 7] = [0, 1, 3, 5, 7, 10, 14];

/// The pictures from M to K − 1 by K − M ("A jump and its landing"):
/// how many frames each of `land-1`, `land-2` and `land-3` shows.
fn landing_counts(k_minus_m: u8) -> Option<[u8; 3]> {
    match k_minus_m {
        6 => Some([1, 2, 3]),
        9 => Some([3, 3, 3]),
        10 => Some([4, 3, 3]),
        _ => None,
    }
}

/// A jump's landing, as it was on its landing frame L.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Landed {
    /// The stage frames that are L or earlier.
    n: u8,
    /// On the floor it took off from, or lower.
    not_above: bool,
    /// After a jump off a rope: M is L.
    rope: bool,
}

/// Follows the player to say which of its pictures it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PictureClock {
    /// The roster slot followed.
    slot: usize,
    /// The jump's frames since T, kept from the frame before.
    jump_age: Option<u8>,
    /// The level y the jump took off from.
    takeoff_y: Option<i64>,
    /// The landing under way.
    landed: Option<Landed>,
    /// Frames of a carried-on roll from D, 0 on D, and the roll's own
    /// frame count it was moved on at.
    carried: Option<(u8, u8)>,
    /// The roll cycle frame, 1 to 33, or for the second character the
    /// roll list's place, 1 to 25, and the roll's own frame count it was
    /// moved on at.
    cycle: Option<(u8, u8)>,
    /// Frames standing after a stop or a landing, 0 on the first.
    standing: Option<u32>,
    /// The standing teeters at the end of a floor, decided once, on
    /// S + 3 (S + 4 for the second character).
    teeters: bool,
    /// Crouching or getting up on the frame before.
    crouched: bool,
    /// Frames of the crouch from D, 0 on D.
    crouch: Option<u32>,
    /// Walking, running or turning on the ground, or in a walk-off's
    /// fall, on the frame before.
    moved: bool,
    /// Frames standing with an object from S, 0 on S.
    holding: Option<u32>,
    /// The put-down's last frame, Q + 13, was shown on the frame before:
    /// it is S.
    put_down_end: bool,
    /// Frames since a jump carrying an object landed, 0 on L, while it
    /// stays on the ground.
    carry_land: Option<u8>,
    /// The frames after the hit on which a knock-out's three landings
    /// came.
    hurt_landings: [Option<u16>; 3],
    /// What the first character does on a rope, on the frame before.
    rope: Option<OnRope>,
    /// Leaving a rope.
    off_rope: Option<OffRope>,
    /// The swimming cycle on the frame before.
    swim: Option<Swim>,
    /// What the player shows on this frame.
    shown: Option<(Set, &'static str)>,
}

impl PictureClock {
    /// The set and label of the picture the player shows on this frame,
    /// if it's one of these.
    pub fn shown(&self) -> Option<(Set, &'static str)> {
        self.shown
    }

    /// A clock showing `label` of `set`, for tests.
    #[cfg(test)]
    pub fn showing(set: Set, label: &'static str) -> PictureClock {
        PictureClock {
            shown: Some((set, label)),
            ..PictureClock::default()
        }
    }

    /// Moves the other character's picture on to `world` as it is after a
    /// logic frame: in a scene, those the spec read, and none elsewhere.
    pub fn update_other(&mut self, world: &World) {
        self.shown = world.other.as_ref().and_then(|other| {
            let (frame, last) = scene_frame(world)?;
            scene_other(other.character, frame, last)
        });
    }

    /// Moves on to `world` as it is after a logic frame. `jump_in`: a
    /// jump starts on the frame this many frames on (1 on its T − 1, 2
    /// on the frame before that, which only matters carrying), 0 if none
    /// does.
    pub fn update(&mut self, world: &World, jump_in: u8) {
        let player = &world.player;
        if player.character != self.slot {
            *self = PictureClock {
                slot: player.character,
                ..PictureClock::default()
            };
        }
        if let Some((frame, last)) = scene_frame(world) {
            self.shown = scene_lead(self.slot, frame, last);
            return;
        }
        // In a launch barrel ("In a launch barrel"): the bounce pictures
        // one to a frame from the shot of a barrel that shoots left. The
        // shot of one that shoots down isn't covered and goes on as usual.
        if let Some(slung) = player.sling
            && slung.since > u16::from(slung.values.hold)
            && !crate::sling_pictures::shoots_down(&slung.values)
        {
            let n = usize::from(slung.since - u16::from(slung.values.hold) - 1);
            self.shown = Some((Set::Bounce, labels(self.slot, Set::Bounce)[n % 16]));
            self.standing = None;
            self.landed = None;
            return;
        }
        // Coming back at a restart barrel with nothing held
        // (restart-barrel.md, "The player"): the first character shows
        // the standing list as if it had begun 143 frames before the
        // resume's frame 0, in the air and after the landing alike.
        if let Some(r) = world.resume
            && self.slot == 0
            && world.player_drawn()
            && player.turn.is_none()
            && matches!(player.gait, Gait::Standing)
            && matches!(player.state, State::Fall | State::Idle)
        {
            let c = u32::from(r.frame) + RESUME_STANDING_BEFORE;
            self.shown = standing(true, c);
            self.standing = Some(c);
            self.teeters = false;
            self.landed = None;
            self.jump_age = None;
            return;
        }
        let before = std::mem::take(&mut self.shown);
        let crouched = std::mem::replace(
            &mut self.crouched,
            player.crouch.held > 0 || player.crouch.rising > 0,
        );
        let moved = std::mem::replace(
            &mut self.moved,
            (player.grounded() || upright_fall(player))
                && (player.turn.is_some() || !matches!(player.gait, Gait::Standing)),
        );
        let events = &world.events;
        let physics = world.player_physics();
        let first = self.slot == 0;
        let jump_next = jump_in == 1;
        let hold = self.holding.take();
        let put_down_end = std::mem::take(&mut self.put_down_end);

        // What the frame before left, moved on to this one.
        let jump_age = if events.contains(&Event::JumpStart) {
            self.takeoff_y = Some(feet(player.pos.y));
            Some(0)
        } else if player.grounded() || events.contains(&Event::Bounce) {
            None
        } else {
            self.jump_age.map(|a| a.saturating_add(1))
        };
        let landing_age = std::mem::replace(&mut self.jump_age, jump_age);
        self.carry_land = if events.contains(&Event::Land) && landing_age.is_some() {
            Some(0)
        } else if player.grounded() {
            self.carry_land.map(|n| n.saturating_add(1))
        } else {
            None
        };
        let off_rope_jump = matches!(self.off_rope, Some(OffRope::Jump { .. }));
        if events.contains(&Event::Land) {
            self.landed = landing_age.and_then(|age| {
                if off_rope_jump {
                    return Some(Landed {
                        n: 0,
                        not_above: true,
                        rope: true,
                    });
                }
                let land = age.saturating_add(1);
                let approach = player.approach?;
                let stages = stages(approach, &physics);
                let n = stages.iter().filter(|&&o| approach + o <= land).count() as u8;
                let y = feet(player.pos.y);
                Some(Landed {
                    n,
                    not_above: self.takeoff_y.is_some_and(|t| y >= t),
                    rope: false,
                })
            });
        } else if !player.grounded() || events.contains(&Event::JumpStart) {
            self.landed = None;
        }
        self.carried = match player.roll {
            Some(roll) if roll.carry => Some(match self.carried {
                // Held still by a defeat's pause.
                Some((c, at)) if at == roll.frame => (c, at),
                Some((c, _)) => (c.saturating_add(1), roll.frame),
                None => (0, roll.frame),
            }),
            _ => None,
        };
        self.cycle = match player.roll {
            Some(roll) if !roll.carry => {
                let cycle = match self.cycle {
                    _ if player.boosted => 2,
                    _ if events.contains(&Event::RollStart) => 2 - u8::from(roll.from_stop),
                    // The roll's first airborne frame, or its landing.
                    _ if roll.air == 1 || events.contains(&Event::Land) => 1,
                    // Held still by a defeat's pause.
                    Some((c, at)) if at == roll.frame => c,
                    Some((c, _)) if first => c % 33 + 1,
                    // Only places 6 to 25 of the list repeat.
                    Some((25, _)) => 6,
                    Some((c, _)) => c + 1,
                    None => 2,
                };
                Some((cycle, roll.frame))
            }
            _ => None,
        };

        let held = player.crouch.held;
        self.crouch = match (held, self.crouch) {
            (0, _) => None,
            (1, _) => Some(0),
            (_, Some(d)) => Some(d.saturating_add(1)),
            // Followed from a later frame.
            (h, None) => Some(u32::from(h) - 1),
        };
        // Getting up: the frames left of it count down to 0 on its last.
        let up = (held == 0 && player.crouch.rising > 0).then(|| {
            let recovery = physics[Param::CrouchRecovery].floor();
            u32::try_from(recovery - 1 - i64::from(player.crouch.rising)).unwrap_or(0)
        });

        let knocked = player
            .knockout
            .filter(|_| world.other.is_none())
            .filter(|_| physics[Param::KnockoutHopAt].to_bits() <= 0);
        if let Some(knockout) = knocked {
            let k = knockout.frame;
            for (n, at) in self.hurt_landings.iter_mut().enumerate() {
                if at.is_none() && usize::from(knockout.landings) > n {
                    *at = Some(k);
                }
            }
            self.standing = None;
            self.landed = None;
            self.shown = Some(if first {
                (Set::Hurt, HURT[hurt(k, self.hurt_landings)])
            } else {
                (Set::Hurt, SECOND_HURT[second_hurt(k, self.hurt_landings)])
            });
            return;
        }
        self.hurt_landings = [None; 3];

        // The ground action: `act-1` on its first frame, then each picture
        // on two ("The ground action"). A pause repeats the picture, as
        // the action's own count stands still. A Jump press ends it.
        if let Some(action) = player
            .ground_action
            .filter(|a| first && !a.air && a.frame <= 58 && player.state == State::GroundAction)
        {
            self.standing = None;
            self.landed = None;
            self.shown = Some(if jump_next {
                (Set::Jump, "jump-1")
            } else {
                (Set::Act, ACT[usize::from((action.frame + 3) / 2) - 1])
            });
            return;
        }

        self.swim = if first && player.state == State::Swim {
            Some(Swim::next(self.swim, events.contains(&Event::Stroke)))
        } else {
            None
        };
        if let Some(swim) = self.swim {
            self.standing = None;
            self.landed = None;
            self.shown = Some((Set::Swim, SWIM[swim.picture]));
            return;
        }

        if first && let Some(shown) = self.on_rope(world) {
            self.standing = None;
            self.landed = None;
            self.shown = Some(shown);
            return;
        }

        let hands = player.hands;
        let playing = matches!(world.flow.outcome, Outcome::Playing)
            && !matches!(
                player.state,
                State::Swap | State::Hurt | State::GroundAction | State::Swim | State::Hold
            );
        if !playing {
            self.standing = None;
            self.landed = None;
            return;
        }

        let from_rope = before.is_some_and(|(set, _)| set == Set::Rope);
        let after_rope = if first {
            self.after_rope(world, from_rope, jump_next)
        } else {
            None
        };
        self.shown = if let Some(shown) = after_rope {
            self.landed = None;
            Some(shown)
        } else if let Some(hands) = hands {
            self.landed = None;
            self.put_down_end = hands.act == Act::PutDown
                && i64::from(hands.frame) + 1 == physics[Param::PutDownTime].floor()
                && player.grounded();
            if self.put_down_end && first {
                standing(first, 0)
            } else {
                self.handling(hands, world, jump_in, hold)
            }
        } else if put_down_end && !first && player.grounded() {
            // Q + 10 is S, whatever is held.
            standing(first, 0)
        } else if jump_next {
            Some((Set::Jump, if first { "jump-1" } else { "s-jump-1" }))
        } else if let Some((n, _)) = player.since_bounce.filter(|_| !player.grounded()) {
            bounce(first, n)
        } else if player.roll_stop > 0 && player.grounded() && player.roll.is_none() {
            stop(first, player.roll_stop - 1)
        } else if let Some((c, _)) = self.carried {
            carried(first, c)
        } else if let Some((c, _)) = self.cycle {
            Some((Set::Roll, roll(first, c)))
        } else if let Some(age) = jump_age {
            in_air(first, age, player.approach, &physics)
        } else if let Some(d) = self.crouch.filter(|_| player.grounded()) {
            crouching(first, d)
        } else if let Some(u) = up.filter(|_| player.grounded()) {
            getting_up(first, u)
        } else if let Some(landed) = self.landed {
            if first {
                first_landing(landed, player.gait, player.since_land)
            } else {
                second_landing(landed, player.since_land, player.gait, player.state)
            }
            .or_else(|| walking(first, player))
        } else if let Some(shown) = face_fall(first, world) {
            Some(shown)
        } else if player.grounded() || upright_fall(player) {
            walking(first, player)
        } else {
            None
        };
        // Standing starts from a stop's last picture, a landing's,
        // walking, running or turning, on the landing frame after a
        // bounce, or once up from crouching. After a walk-off without a
        // face it starts, or goes on, in the air.
        let standing_now = (self.shown.is_none() || put_down_end && !first)
            && hands.is_none()
            && (player.grounded()
                && matches!(player.state, State::Idle | State::Walk | State::Run)
                || upright_fall(player))
            && player.roll_stop == 0
            && player.crouch.held == 0
            && player.crouch.rising == 0
            && player.turn.is_none()
            && matches!(player.gait, Gait::Standing);
        self.standing = match self.standing {
            _ if !standing_now => None,
            Some(c) => Some(c.saturating_add(1)),
            None => before
                .is_some_and(|(set, _)| {
                    matches!(
                        set,
                        Set::Stop | Set::Jump | Set::Bounce | Set::Carry | Set::Act
                    ) || (first && set == Set::Walking)
                })
                .then_some(0)
                .or((crouched || !first && moved).then_some(0)),
        };
        if put_down_end && standing_now {
            // The first character's Q + 13 was S; Q + 14 is S + 1. The
            // second's S is Q + 10, the frame after its put-down.
            self.standing = Some(u32::from(first));
        }
        match self.standing {
            Some(c) => {
                let decided = if first { 3 } else { 4 };
                if c < decided {
                    self.teeters = false;
                } else if c == decided {
                    self.teeters = world.player_at_floor_end(12, first);
                }
                self.shown = if self.teeters {
                    Some(teetering(first, c - decided))
                } else {
                    standing(first, c)
                };
            }
            None => self.teeters = false,
        }
    }
}

impl PictureClock {
    /// The first character's picture on a frame that ends holding a
    /// rope ("On a rope").
    fn on_rope(&mut self, world: &World) -> Option<(Set, &'static str)> {
        let player = &world.player;
        let Some(on) = player.rope else {
            self.rope = None;
            return None;
        };
        self.off_rope = None;
        let caught = world.events.contains(&Event::RopeCatch);
        let before = std::mem::take(&mut self.rope).filter(|_| !caught);
        let dir = on.climbing;
        let start = |dir: i8| OnRope::Climb {
            at: 0,
            counter: 512,
            dir,
        };
        if let Some(n) = on.turn {
            self.rope = Some(OnRope::Turn);
            let label = if (3..=6).contains(&n) {
                "rope-turn-2"
            } else {
                "rope-turn-1"
            };
            return Some((Set::Rope, label));
        }
        let now = match before {
            _ if caught => OnRope::Hang(0),
            None | Some(OnRope::Turn) if dir != 0 => start(dir),
            None | Some(OnRope::Turn) => OnRope::Hang(0),
            Some(OnRope::Hang(_)) if dir != 0 => start(dir),
            Some(OnRope::Hang(n)) => OnRope::Hang(n + 1),
            Some(OnRope::Climb {
                at,
                counter,
                dir: from,
            }) => {
                // The climbing speed, in 1/256 px a frame.
                let vy = player.vel.y.to_bits() >> 24;
                let u = vy.unsigned_abs() as i32;
                let rate = if vy < 0 { u / 2 + u / 4 } else { u / 4 + u / 8 }.max(112);
                let counter = counter - rate;
                if counter > 0 {
                    OnRope::Climb {
                        at,
                        counter,
                        dir: from,
                    }
                } else if at + 1 < 10 {
                    OnRope::Climb {
                        at: at + 1,
                        counter: counter + 512,
                        dir: from,
                    }
                } else if dir == 0 {
                    // The check: no direction held, so it hangs.
                    OnRope::Hang(0)
                } else {
                    OnRope::Climb {
                        at: 0,
                        counter: if dir == from { counter + 512 } else { 512 },
                        dir,
                    }
                }
            }
        };
        self.rope = Some(now);
        Some((
            Set::Rope,
            match now {
                OnRope::Turn => unreachable!("a turn returns above"),
                OnRope::Hang(n) => HANG[(n / 9) as usize % HANG.len()],
                OnRope::Climb { at, .. } => CLIMB[usize::from(at)],
            },
        ))
    }

    /// The first character's picture on the frames after it has left a
    /// rope ("After the rope"), where covered.
    fn after_rope(
        &mut self,
        world: &World,
        from_rope: bool,
        jump_next: bool,
    ) -> Option<(Set, &'static str)> {
        let player = &world.player;
        let events = &world.events;
        let slide = matches!(self.off_rope, Some(OffRope::Slide(_)));
        if player.rope.is_some()
            || !slide && (player.grounded() || events.contains(&Event::Land))
            // The list is left for a jump, a crouch or a roll after the
            // landing.
            || slide
                && player.grounded()
                && (jump_next || player.crouch.held > 0 || player.roll.is_some())
        {
            self.off_rope = None;
        }
        if events.contains(&Event::RopeSlideOff) {
            // The let-go frame itself isn't covered.
            self.off_rope = Some(OffRope::Slide(0));
            return None;
        }
        if events.contains(&Event::JumpStart) && from_rope {
            self.off_rope = Some(OffRope::Jump {
                age: 0,
                falling: None,
            });
        } else {
            self.off_rope = match self.off_rope {
                // Later than F + 14 the list holds only on the floor.
                Some(OffRope::Slide(n)) if n < SLIDE_LAST + 1 && (n < 15 || player.grounded()) => {
                    Some(OffRope::Slide(n + 1))
                }
                Some(OffRope::Slide(_)) => None,
                Some(OffRope::Jump { age, falling }) => {
                    let falling = match falling {
                        Some(m) => Some(m.saturating_add(1)),
                        None => (player.vel.y.to_bits() > 0).then_some(0),
                    };
                    Some(OffRope::Jump {
                        age: age.saturating_add(1),
                        falling,
                    })
                }
                None => None,
            };
        }
        let label = match self.off_rope? {
            OffRope::Slide(0) => return None,
            OffRope::Slide(n) => slide_off(n - 1),
            OffRope::Jump { age: 0, .. } => "land-1",
            OffRope::Jump {
                falling: Some(m), ..
            } => match m {
                0 => "air-4",
                1..=3 => "air-5",
                _ => "air-6",
            },
            OffRope::Jump { age, .. } => match age {
                1..=2 => "air-6",
                3..=4 => "air-5",
                5..=6 => "air-4",
                _ => "air-3",
            },
        };
        Some((Set::Jump, label))
    }

    /// A character with an object in its hands ("Carrying"). `hold`:
    /// frames standing with it before this one.
    fn handling(
        &mut self,
        hands: Hands,
        world: &World,
        jump_in: u8,
        hold: Option<u32>,
    ) -> Option<(Set, &'static str)> {
        let player = &world.player;
        let grounded = player.grounded();
        let first = self.slot == 0;
        // The picture shown around a jump, and the carrying stride's.
        let (list, jumping, first_step, steps, turning): (&[&str], _, _, _, _) = if first {
            (&CARRYING, CARRY_1, CARRY_1, 10, CARRY_TURN)
        } else {
            (&SECOND_CARRYING, S_CARRY_1 + 1, S_CARRY_1, 8, S_HOLD)
        };
        let n = if hands.act != Act::Carry {
            if first {
                handling(hands.act, hands.frame)
            } else {
                second_handling(hands.act, hands.frame)
            }
            .filter(|_| grounded)
        } else if !grounded {
            // Only a jump's air is covered.
            self.jump_age.map(|_| jumping)
        } else if (1..=2).contains(&jump_in)
            || world.events.contains(&Event::JumpStart)
            || self.carry_land.is_some_and(|n| n <= 1)
        {
            Some(jumping)
        } else if player.turn.is_some() {
            Some(turning)
        } else {
            match player.gait {
                Gait::Walking(stride) if (1..=steps).contains(&stride.step) => {
                    Some(first_step + stride.step as usize - 1)
                }
                Gait::Standing if first => {
                    let c = hold.map_or(0, |c| c.saturating_add(1));
                    self.holding = Some(c);
                    Some(holding(c))
                }
                Gait::Standing => Some(S_HOLD),
                _ => None,
            }
        };
        Some((Set::Carry, list[n?]))
    }
}

/// In the air after walking or running off a ledge without a face,
/// where the mode goes on (`ground-speed.md`, "After a fall or a
/// bounce").
fn upright_fall(player: &clamber_sim::Player) -> bool {
    !player.grounded()
        && (player.walked_off || player.keg_fall)
        && !player.walked_off_face
        && player.roll.is_none()
}

/// A character on a steep face, in the fall after it and on its
/// landing frames ("A fall from a steep face"), but not crouching.
fn face_fall(first: bool, world: &World) -> Option<(Set, &'static str)> {
    const LANDING: [&str; 7] = [
        "s-air-2", "s-air-3", "s-air-4", "s-air-5", "s-air-6", "s-land-1", "s-land-2",
    ];
    const FIRST_LANDING: [&str; 11] = [
        "air-1", "air-2", "air-3", "air-4", "air-5", "air-6", "land-1", "land-2", "land-2",
        "land-3", "land-3",
    ];
    let player = &world.player;
    if player.crouch.held > 0 || player.crouch.rising > 0 {
        return None;
    }
    let fell = !player.grounded()
        && player.walked_off
        && player.walked_off_face
        && !player.rolled_off
        && player.roll.is_none();
    let label = match player.gait {
        Gait::Landing(l) if player.grounded() && l.fixed && !l.bounce => {
            let list = if first {
                &FIRST_LANDING[..]
            } else {
                &LANDING[..]
            };
            *list.get(usize::from(l.frame))?
        }
        _ if fell || world.player_on_face() => {
            if first {
                "air-0"
            } else {
                "s-air-1"
            }
        }
        _ => return None,
    };
    Some((Set::Jump, label))
}

/// The feet line: level y rounded up to a whole px.
fn feet(y: clamber_fixed::Fixed) -> i64 {
    -clamber_fixed::Fixed::from_bits(-y.to_bits()).floor()
}

fn stages(approach: u8, physics: &clamber_pack::Physics) -> [u8; 7] {
    if i64::from(approach) > physics[Param::SwitchFrom].floor() {
        LATE_STAGES
    } else {
        STAGES
    }
}

/// On B + `n`: the character's first bounce picture to B + 3, then its
/// 16 pictures 2 frames each from the second on B + 4, round and round
/// until the landing.
fn bounce(first: bool, n: u8) -> Option<(Set, &'static str)> {
    let labels = labels(usize::from(!first), Set::Bounce);
    let label = match n {
        0..=3 => labels[0],
        _ => labels[usize::from((n - 2) / 2) % 16],
    };
    Some((Set::Bounce, label))
}

/// On D + `d` of the stop after a roll.
fn stop(first: bool, d: u8) -> Option<(Set, &'static str)> {
    let label = match (first, d) {
        (true, 0..=2) => "roll-end",
        (true, 3..=8) => "stop",
        (true, 9..=16) => "stand-1",
        (false, 0..=4) => "s-stop-1",
        (false, 5..=10) => "s-stop-2",
        (false, 11..=12) => "s-stop-3",
        (false, 13..=14) => "s-stop-4",
        (false, 15..=16) => "s-stop-5",
        _ => return None,
    };
    Some((Set::Stop, label))
}

/// On D + `d` of a roll carrying on into running.
fn carried(first: bool, d: u8) -> Option<(Set, &'static str)> {
    let label = match (first, d) {
        (true, 0..=2) => "stop",
        (false, 0..=2) => "s-stop-1",
        (false, 3..=5) => "s-stop-2",
        _ => return None,
    };
    Some((Set::Stop, label))
}

/// On cycle frame `c` of the first character's roll, or place `c` of the
/// second character's roll list.
fn roll(first: bool, c: u8) -> &'static str {
    let c = usize::from(c);
    match (first, c) {
        (true, _) => labels(0, Set::Roll)[(c - 1) / 3],
        (false, 1) => SECOND_ROLL[0],
        (false, 2..=4) => SECOND_ROLL[16 - c],
        (false, 5) => SECOND_ROLL[1],
        (false, _) => SECOND_ROLL[2 + (c - 6) / 2],
    }
}

/// On frame `c` of standing, 0 on its first (S).
fn standing(first: bool, c: u32) -> Option<(Set, &'static str)> {
    let label = match first {
        true => first_standing(c),
        false => second_standing(c),
    };
    Some((Set::Standing, label))
}

/// The teetering pictures `i` frames after they begin ("Teetering at
/// the end of a floor"), S + 3 for the first character and S + 4 for the
/// second.
fn teetering(first: bool, i: u32) -> (Set, &'static str) {
    if first {
        let n = match i {
            0..44 => 1 + i / 4,
            _ => teeter_loop((i - 44) % 290),
        };
        (Set::Teeter, TEETER[n as usize - 1])
    } else {
        let n = match i {
            0..36 => 1 + i / 6,
            _ => second_teeter_loop((i - 36) % 210),
        };
        (Set::Teeter, SECOND_TEETER[n as usize - 1])
    }
}

/// Frame `i` of the first character's teetering loop, 290 frames: the
/// picture steps one number at a time between these turning points.
fn teeter_loop(mut i: u32) -> u32 {
    const TURNS: [u32; 11] = [12, 17, 12, 17, 12, 22, 12, 22, 17, 22, 13];
    if i < 5 {
        return 12;
    }
    i -= 5;
    let mut n = 12;
    let mut shown = (12, 5);
    for turn in TURNS.windows(2) {
        let up = turn[1] > turn[0];
        while n != turn[1] {
            let from = n;
            n = if up { n + 1 } else { n - 1 };
            shown = (
                n,
                if n > 17 || (n == 17 && from == 18) {
                    3
                } else {
                    5
                },
            );
            if i < shown.1 {
                return shown.0;
            }
            i -= shown.1;
        }
    }
    shown.0
}

/// Frame `i` of the second character's teetering loop, 210 frames.
fn second_teeter_loop(mut i: u32) -> u32 {
    // Stretches of (first picture, last picture, frames each); the
    // pictures go up or down from the first to the last.
    const STRETCHES: [&[(u32, u32, u32)]; 6] = [
        &[(7, 8, 4), (9, 10, 3), (11, 17, 2), (18, 18, 1)],
        &[(7, 7, 1), (8, 18, 2)],
        &[(7, 9, 2), (10, 14, 3), (15, 18, 4)],
        &[(6, 4, 4), (5, 6, 3)],
        &[(18, 7, 2), (18, 7, 2)],
        &[(18, 17, 2), (16, 16, 2), (15, 11, 3), (10, 4, 4), (5, 6, 3)],
    ];
    for &(a, b, each) in STRETCHES.iter().copied().flatten() {
        let len = a.abs_diff(b) + 1;
        if i < len * each {
            let k = i / each;
            return if a <= b { a + k } else { a - k };
        }
        i -= len * each;
    }
    18
}

/// The first character on S + `c` ("Standing still"): `stand-1` × 3,
/// the sway, `stand-1` × 12 and the wait twice, `stand-1` × 3 and the
/// sway, then `stand-1` × 15 and the sway over and over.
fn first_standing(c: u32) -> &'static str {
    let i = match c {
        0..914 => c % 457,
        914..1103 => c - 914,
        _ => match (c - 1103) % 201 {
            0..15 => return STANDING[0],
            i => return sway(i - 15),
        },
    };
    match i {
        0..3 | 189..201 => STANDING[0],
        3..189 => sway(i - 3),
        _ => wait(i - 201),
    }
}

/// The second character on S + `c` ("Standing still"): one 275-frame
/// loop over and over.
fn second_standing(c: u32) -> &'static str {
    let wait = |n: u32| SECOND_STANDING[21 + n as usize];
    match c % 275 {
        i @ 0..88 => SECOND_STANDING[(i / 4) as usize],
        i @ 88..153 => wait(1 + (i - 88) / 5),
        i @ 153..223 => {
            let mut i = i - 153;
            for (n, count) in SECOND_SWING {
                if i < u32::from(count) {
                    return wait(n.into());
                }
                i -= u32::from(count);
            }
            wait(15)
        }
        i => wait(13 - (i - 223) / 4),
    }
}

/// Frame `i` of the sway.
fn sway(mut i: u32) -> &'static str {
    for (n, count) in SWAY {
        if i < u32::from(count) {
            return STANDING[usize::from(n) - 1];
        }
        i -= u32::from(count);
    }
    STANDING[0]
}

/// Frame `i` of the wait, 256 frames.
fn wait(i: u32) -> &'static str {
    let n = match i {
        0..36 => 1 + i / 4,
        36..38 | 218..220 => 10,
        38..62 => 11 + (i - 38) / 4,
        // Back and forth between `wait-17` and `wait-26`, 2 frames each.
        62..186 => {
            let legs = [
                (17, 26),
                (25, 17),
                (18, 26),
                (25, 17),
                (18, 26),
                (25, 18),
                (19, 26),
            ];
            legs.into_iter()
                .flat_map(|(a, b): (u32, u32)| {
                    let up = a <= b;
                    (0..=a.abs_diff(b)).map(move |k| if up { a + k } else { a - k })
                })
                .nth(((i - 62) / 2) as usize)
                .unwrap_or(26)
        }
        186..218 => 27 + (i - 186) / 4,
        _ => 9 - (i - 220).min(35) / 4,
    };
    STANDING[10 + n as usize]
}

/// Walking, running or turning on the ground or, for the first
/// character, in a walk-off's fall, on the step its frame ends in or turn
/// frame T + n.
fn walking(first: bool, player: &clamber_sim::Player) -> Option<(Set, &'static str)> {
    if player.roll.is_some()
        || player.roll_stop > 0
        || player.crouch.held > 0
        || player.crouch.rising > 0
    {
        return None;
    }
    let (labels, walks, runs) = match first {
        true => (&WALKING[..], 20, 20),
        false => (&SECOND_WALKING[..], 16, 13),
    };
    let label = match (player.turn, player.gait) {
        (Some(2..=5), _) => labels[walks + runs + 1],
        (Some(_), _) => labels[walks + runs],
        (None, Gait::Walking(stride)) => {
            *labels[..walks].get(stride.step.checked_sub(1)? as usize)?
        }
        (None, Gait::Running(stride)) => {
            *labels[walks..walks + runs].get(stride.step.checked_sub(1)? as usize)?
        }
        _ => return None,
    };
    Some((Set::Walking, label))
}

/// On T + `age` in the air, with the approach frame A − T if it has come.
fn in_air(
    first: bool,
    age: u8,
    approach: Option<u8>,
    physics: &clamber_pack::Physics,
) -> Option<(Set, &'static str)> {
    const FIRST_RISE: [&str; 8] = [
        "rise-1", "rise-2", "rise-3", "rise-4", "rise-5", "rise-6", "rise-7", "rise-8",
    ];
    const SECOND_RISE: [&str; 7] = [
        "s-rise-1", "s-rise-2", "s-rise-3", "s-rise-4", "s-rise-5", "s-rise-6", "s-rise-7",
    ];
    let label = match (first, age) {
        (true, 0..=1) => "jump-2",
        (false, 0..=1) => "s-jump-2",
        (true, 2..=17) => FIRST_RISE[usize::from((age - 2) / 2)],
        (false, 2..=15) => SECOND_RISE[usize::from((age - 2) / 2)],
        (true, _) => {
            let s = approach.filter(|&a| age >= a).map_or(0, |a| {
                let stages = stages(a, physics);
                stages.iter().filter(|&&o| a + o <= age).count().min(6)
            });
            air(s)
        }
        (false, _) => match approach.filter(|&a| age >= a) {
            None => "s-air-1",
            Some(a) => match age - a {
                0 => "s-air-2",
                1..=2 => "s-air-3",
                3..=4 => "s-air-4",
                5..=6 => "s-air-5",
                _ => "s-air-6",
            },
        },
    };
    Some((Set::Jump, label))
}

fn air(s: usize) -> &'static str {
    [
        "air-0", "air-1", "air-2", "air-3", "air-4", "air-5", "air-6",
    ][s.min(6)]
}

/// The first character on the frames from L until walking, running or
/// standing starts.
fn first_landing(
    landed: Landed,
    gait: Gait,
    since_land: Option<u8>,
) -> Option<(Set, &'static str)> {
    if landed.rope {
        // M = L: three pictures each of `land-1`, `land-2` and `land-3`,
        // whatever the gait says (`since_land` is 0 on L).
        let label = *["land-1", "land-2", "land-3"].get(usize::from(since_land? / 3))?;
        return Some((Set::Jump, label));
    }
    let Gait::Landing(landing) = gait else {
        return None;
    };
    if landing.fixed || landing.bounce {
        return None;
    }
    let (j, m) = (landing.frame, landing.switch);
    let label = if j < m {
        // L when M is later, then the air pictures two at a time.
        air(usize::from(landed.n) + 2 * usize::from(j))
    } else {
        let counts = landing_counts(landing.settle.checked_sub(m)?)?;
        let mut i = j - m;
        let mut label = None;
        for (count, name) in counts.into_iter().zip(["land-1", "land-2", "land-3"]) {
            if i < count {
                label = Some(name);
                break;
            }
            i -= count;
        }
        label?
    };
    Some((Set::Jump, label))
}

/// The second character on L + `since_land` up to L + 16, after a jump
/// that lands on the floor it took off from or lower and doesn't run from L.
fn second_landing(
    landed: Landed,
    since_land: Option<u8>,
    gait: Gait,
    state: State,
) -> Option<(Set, &'static str)> {
    if !landed.not_above || matches!(gait, Gait::Running(_)) || state == State::Run {
        return None;
    }
    let label = match since_land? {
        0 => "s-land-1",
        1..=2 => "s-land-2",
        3..=5 => "s-land-3",
        6..=13 => "s-land-4",
        14..=16 => "s-land-3",
        _ => return None,
    };
    Some((Set::Jump, label))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use clamber_fixed::Fixed;
    use clamber_input::{Action, Actions, Frame};
    use clamber_level::Level;
    use clamber_pack::{CharacterFeel, Presets};
    use clamber_sim::{Event, FlowEvent, InputFrame, ROLL, generic_feel, step};

    use super::*;
    use crate::game::jump_in;

    const FLAT: &str = r#"
        format = 1
        size = [2048, 480]
        start = [64, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 2048, 80]
    "#;

    /// A classic-1 world on `src` with both characters in the roster,
    /// the one in `slot` leading alone.
    fn world(src: &str, slot: usize) -> World {
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::from([(ROLL.to_string(), BTreeMap::new())]),
        });
        let level = Arc::new(Level::from_toml_str(src, "pictures").unwrap());
        let mut world = World::new(level, Arc::new(feel));
        world.player.character = slot;
        world.other = None;
        world
    }

    /// What's shown on each frame, with the frame's events, playing
    /// `keys` (what's held on each frame).
    struct Run {
        shown: Vec<Option<&'static str>>,
        events: Vec<Vec<Event>>,
        roll_stop: Vec<u8>,
        grounded: Vec<bool>,
        frozen: Vec<u8>,
        on_face: Vec<bool>,
    }

    fn run(world: World, keys: &[Vec<Action>]) -> Run {
        run_after(world, Actions::NONE, keys)
    }

    /// [`run`] with `held` held before the first frame.
    fn run_after(mut world: World, mut held: Actions, keys: &[Vec<Action>]) -> Run {
        let mut clock = PictureClock::default();
        let mut out = Run {
            shown: Vec::new(),
            events: Vec::new(),
            roll_stop: Vec::new(),
            grounded: Vec::new(),
            frozen: Vec::new(),
            on_face: Vec::new(),
        };
        for frame in keys {
            let now: Actions = frame.iter().copied().collect();
            let input = InputFrame::single(Frame::after(held, now));
            held = now;
            step(&mut world, &input);
            clock.update(&world, jump_in(&world, &input));
            out.shown.push(clock.shown().map(|(_, label)| label));
            out.events.push(world.events.clone());
            out.roll_stop.push(world.player.roll_stop);
            out.grounded.push(world.player.grounded());
            out.frozen.push(world.player.frozen);
            out.on_face.push(world.player_on_face());
        }
        out
    }

    impl Run {
        /// The frames carrying `event`.
        fn frames(&self, event: Event) -> Vec<usize> {
            (0..self.events.len())
                .filter(|&f| self.events[f].contains(&event))
                .collect()
        }

        /// Checks what's shown from `from` on: each label for its count
        /// of frames, then `None` for a frame.
        fn check(&self, from: usize, want: &[(&str, usize)]) {
            let mut f = from;
            for &(label, count) in want {
                for _ in 0..count {
                    let label = Some(label).filter(|l| !l.is_empty());
                    assert_eq!(self.shown[f], label, "frame {} after {from}", f - from);
                    f += 1;
                }
            }
        }
    }

    /// What the first character shows on frames 1 to `last` of a resume
    /// at a restart barrel, `keys` giving what is held on each frame.
    fn resumed(last: u32, keys: impl Fn(u32) -> Vec<Action>) -> Vec<Option<&'static str>> {
        let src = r#"
            format = 1
            size = [3000, 700]
            start = [900, 409]

            [[collision]]
            shape = "rect"
            rect = [0, 409, 3000, 100]

            [[entities]]
            type = "keg"
            at = [1000, 400]
        "#;
        let mut world = world(src, 0);
        let mut held = Actions::NONE;
        let mut go = |world: &mut World, keys: &[Action]| {
            let now: Actions = keys.iter().copied().collect();
            let input = InputFrame::single(Frame::after(held, now));
            held = now;
            step(world, &input);
            input
        };
        go(&mut world, &[]);
        for _ in 0..120 {
            go(&mut world, &[Action::Right]);
            if !world.flow.checkpoints.is_empty() {
                break;
            }
        }
        assert_eq!(world.flow.checkpoints, [0]);
        world.player.pos.y = Fixed::from_int(900);
        world.player.ground = None;
        for _ in 0..400 {
            go(&mut world, &[]);
            if world.flow.events.contains(&FlowEvent::Respawn) {
                break;
            }
        }
        assert_eq!(world.resume.map(|r| r.frame), Some(0));
        let mut clock = PictureClock::default();
        let mut shown = vec![None];
        for frame in 1..=last {
            let input = go(&mut world, &keys(frame));
            clock.update(&world, jump_in(&world, &input));
            shown.push(clock.shown().map(|(_, label)| label));
        }
        shown
    }

    /// restart-barrel.md, "The player", no input: from frame 63 the
    /// standing list as if frame 0 were its 143rd frame, in the air and
    /// after the landing.
    #[test]
    fn a_resume_with_nothing_held_shows_the_standing_list_from_wait_2() {
        let shown = resumed(300, |_| vec![]);
        for frame in 63..=300 {
            let want = first_standing(frame + 143);
            assert_eq!(shown[frame as usize], Some(want), "frame {frame}");
        }
        let at = |frame: u32| first_standing(frame + 143);
        assert_eq!((at(63), at(65)), ("wait-2", "wait-2"));
        assert_eq!((at(66), at(69)), ("wait-3", "wait-3"));
    }

    fn check_resume(shown: &[Option<&str>], from: usize, want: &[(&str, usize)]) {
        let mut f = from;
        for &(label, count) in want {
            for _ in 0..count {
                assert_eq!(shown[f], Some(label), "frame {f}");
                f += 1;
            }
        }
    }

    /// restart-barrel.md, "The player": Right held from frame 63.
    #[test]
    fn a_resume_with_right_held_from_frame_63_walks() {
        let shown = resumed(73, |f| if f >= 63 { vec![Action::Right] } else { vec![] });
        let want = [("walk-1", 3), ("walk-2", 3), ("walk-3", 2), ("walk-4", 3)];
        check_resume(&shown, 63, &want);
    }

    /// restart-barrel.md, "The player": Left held from frame 63.
    #[test]
    fn a_resume_with_left_held_from_frame_63_turns() {
        let shown = resumed(71, |f| if f >= 63 { vec![Action::Left] } else { vec![] });
        let want = [("turn-a", 2), ("turn-b", 4), ("turn-a", 2), ("walk-1", 1)];
        check_resume(&shown, 63, &want);
    }

    /// Nothing held for `wait` frames, then Jump held `hold` frames,
    /// then nothing, `total` frames in all.
    fn jump(wait: usize, hold: usize, total: usize) -> Vec<Vec<Action>> {
        (0..total)
            .map(|f| {
                if (wait..wait + hold).contains(&f) {
                    vec![Action::Jump]
                } else {
                    vec![]
                }
            })
            .collect()
    }

    fn air_then(t: usize, run: &Run) {
        // T − 1, then T to T + 17.
        let mut want = vec![("jump-1", 1), ("jump-2", 2)];
        for r in [
            "rise-1", "rise-2", "rise-3", "rise-4", "rise-5", "rise-6", "rise-7", "rise-8",
        ] {
            want.push((r, 2));
        }
        run.check(t - 1, &want);
    }

    #[test]
    fn a_standing_jump_held_one_frame() {
        // player-frames.md's first example: A = T + 20, L = T + 31,
        // n = 6, M = L + 1, K = L + 7.
        let run = run(world(FLAT, 0), &jump(10, 1, 120));
        let t = run.frames(Event::JumpStart)[0];
        assert_eq!(t, 11, "Jump pressed on T - 1");
        let l = run.frames(Event::Land)[0];
        assert_eq!(l, t + 31);
        air_then(t, &run);
        run.check(
            t + 18,
            &[
                ("air-0", 2),
                ("air-1", 2),
                ("air-2", 2),
                ("air-3", 2),
                ("air-4", 2),
                ("air-5", 3),
                ("air-6", 1),
                ("land-1", 1),
                ("land-2", 2),
                ("land-3", 3),
                ("stand-1", 3),
                ("stand-2", 3),
                ("stand-3", 3),
                ("stand-4", 3),
                ("stand-5", 3),
                ("stand-6", 10),
            ],
        );
    }

    #[test]
    fn a_standing_jump_held_twelve_frames() {
        // The example with M = L: A = T + 21, L = T + 37, K = L + 9.
        let run = run(world(FLAT, 0), &jump(10, 12, 120));
        let t = run.frames(Event::JumpStart)[0];
        let l = run.frames(Event::Land)[0];
        assert_eq!(l, t + 37);
        air_then(t, &run);
        run.check(
            t + 18,
            &[
                ("air-0", 3),
                ("air-1", 1),
                ("air-2", 2),
                ("air-3", 2),
                ("air-4", 2),
                ("air-5", 3),
                ("air-6", 6),
                ("land-1", 3),
                ("land-2", 3),
                ("land-3", 3),
                ("stand-1", 3),
            ],
        );
    }

    #[test]
    fn a_jump_out_of_a_roll_shows_a_standing_jump_s_pictures() {
        // player-frames.md: a jump out of a roll pressed on the ground
        // shows the same air, landing and standing pictures, from
        // `jump-1` on the press frame.
        for hold in [1, 12] {
            let still = run(world(FLAT, 0), &jump(10, hold, 120));
            let mut keys = jump(20, hold, 140);
            keys[10] = vec![Action::Action];
            let rolled = run(world(FLAT, 0), &keys);
            let (a, b) = (
                still.frames(Event::JumpStart)[0],
                rolled.frames(Event::JumpStart)[0],
            );
            assert_eq!(b, 21, "Jump pressed on T - 1");
            assert_eq!(rolled.frames(Event::RollStart), [10]);
            assert_eq!(
                rolled.shown[b - 1..b + 70],
                still.shown[a - 1..a + 70],
                "hold {hold}"
            );
        }
    }

    /// The first character, hit on the ground on flat floor: what it
    /// shows from H on, for `n` frames.
    fn hurt_run(slot: usize, n: usize) -> Vec<Option<&'static str>> {
        let mut world = world(FLAT, slot);
        world.player.state = State::Hurt;
        world.player.knockout = Some(clamber_sim::Knockout::new(None));
        let mut clock = PictureClock::default();
        clock.update(&world, 0);
        let mut shown = vec![clock.shown().map(|(_, label)| label)];
        for _ in 1..n {
            let input = InputFrame::single(Frame::after(Actions::NONE, Actions::NONE));
            step(&mut world, &input);
            clock.update(&world, 0);
            shown.push(clock.shown().map(|(_, label)| label));
        }
        shown
    }

    #[test]
    fn the_first_character_s_being_hurt_pictures() {
        // player-frames.md, "Being hurt": on flat floor the landings are
        // on H + 46, H + 67 and F = H + 72 (knock-out.md, "Worked
        // values"). The last frame, F + 153, is not on screen.
        let shown = hurt_run(0, 225);
        let mut want: Vec<&str> = Vec::new();
        for (label, count) in [
            ("hurt-1", 10),
            ("hurt-2", 5),
            ("hurt-3", 5),
            ("hurt-4", 5),
            ("hurt-5", 5),
            ("hurt-6", 16),
            ("hurt-7", 4),
            ("hurt-8", 5),
            ("hurt-9", 12),
            ("hurt-10", 5),
            ("hurt-11", 28),
            ("hurt-12", 6),
            ("hurt-13", 6),
            ("hurt-14", 6),
        ] {
            want.extend(std::iter::repeat_n(label, count));
        }
        for i in 0..225 - want.len() {
            want.push(["lie-1", "lie-2", "lie-3", "lie-4", "lie-3", "lie-2"][i / 3 % 6]);
        }
        let shown: Vec<_> = shown.into_iter().map(Option::unwrap).collect();
        assert_eq!(shown, want);
    }

    #[test]
    fn the_second_character_s_being_hurt_pictures() {
        // player-frames.md, "Being hurt", "Second character": the pause
        // comes between H + 10 and H + 11, and the last logic frame, F +
        // 101, is not on screen.
        let shown: Vec<_> = hurt_run(1, 173).into_iter().map(Option::unwrap).collect();
        let mut want: Vec<&str> = Vec::new();
        for (label, count) in [
            ("s-hurt-1", 13),
            ("s-hurt-2", 5),
            ("s-hurt-3", 5),
            ("s-hurt-4", 5),
            ("s-hurt-5", 5),
            ("s-hurt-6", 13),
            ("s-hurt-7", 4),
            ("s-hurt-8", 5),
            ("s-hurt-9", 12),
            ("s-hurt-10", 5),
            ("s-hurt-11", 12),
            ("s-hurt-12", 6),
            ("s-hurt-13", 6),
            ("s-hurt-14", 6),
            ("s-lie-1", 6),
            ("s-lie-2", 6),
            ("s-lie-3", 11),
            ("s-lie-2", 5),
            ("s-lie-1", 10),
            ("s-lie-2", 5),
            ("s-lie-3", 9),
            ("s-lie-2", 4),
            ("s-lie-1", 8),
            ("s-lie-2", 4),
            ("s-lie-3", 3),
        ] {
            want.extend(std::iter::repeat_n(label, count));
        }
        assert_eq!(shown, want);
    }

    /// Action pressed for a frame on frame 10, then `then` held.
    fn roll(then: &[Action], total: usize) -> Vec<Vec<Action>> {
        (0..total)
            .map(|f| match f {
                10 => vec![Action::Action],
                f if f > 10 => then.to_vec(),
                _ => vec![],
            })
            .collect()
    }

    #[test]
    fn the_first_character_s_stop_after_a_roll() {
        let run = run(world(FLAT, 0), &roll(&[], 120));
        let d = run.frames(Event::RollEnd)[0];
        assert_eq!(run.roll_stop[d], 1, "a stop");
        run.check(
            d,
            &[
                ("roll-end", 3),
                ("stop", 6),
                ("stand-1", 8),
                ("stand-1", 3),
                ("stand-2", 3),
                ("stand-3", 3),
                ("stand-4", 3),
                ("stand-5", 3),
                ("stand-6", 10),
            ],
        );
    }

    #[test]
    fn the_second_character_s_stop_after_a_roll() {
        let run = run(world(FLAT, 1), &roll(&[], 120));
        let d = run.frames(Event::RollEnd)[0];
        assert_eq!(run.roll_stop[d], 1, "a stop");
        run.check(
            d,
            &[
                ("s-stop-1", 5),
                ("s-stop-2", 6),
                ("s-stop-3", 2),
                ("s-stop-4", 2),
                ("s-stop-5", 2),
                ("s-stand-1", 4),
                ("s-stand-2", 4),
            ],
        );
    }

    #[test]
    fn a_roll_carrying_on_into_running() {
        let held = [Action::Action, Action::Right];
        let first = run(world(FLAT, 0), &roll(&held, 80));
        // `roll_end` comes on D + 3 for the first character.
        let d = first.frames(Event::RollEnd)[0] - 3;
        assert_eq!(first.roll_stop[d + 3], 0, "carried on");
        first.check(d, &[("stop", 3), ("walk-1", 1)]);
        let second = run(world(FLAT, 1), &roll(&held, 80));
        // And on D + 6 for the second, walking from then.
        let d = second.frames(Event::RollEnd)[0] - 6;
        assert_eq!(d, second.frames(Event::RollStart)[0] + 24);
        second.check(d, &[("s-stop-1", 3), ("s-stop-2", 3), ("s-walk-1", 1)]);
    }

    /// The second character's roll list, place 1 to 25.
    const PLACES: [&str; 25] = [
        "s-roll-1",
        "s-stop-5",
        "s-stop-4",
        "s-stop-3",
        "s-roll-2",
        "s-roll-3",
        "s-roll-3",
        "s-roll-4",
        "s-roll-4",
        "s-roll-5",
        "s-roll-5",
        "s-roll-6",
        "s-roll-6",
        "s-roll-7",
        "s-roll-7",
        "s-roll-8",
        "s-roll-8",
        "s-roll-9",
        "s-roll-9",
        "s-roll-10",
        "s-roll-10",
        "s-roll-11",
        "s-roll-11",
        "s-roll-12",
        "s-roll-12",
    ];

    /// Checks the second character's roll list on frames `from` to
    /// `to - 1`, with `from` on place `p`.
    fn check_list(run: &Run, from: usize, to: usize, mut p: usize) {
        for f in from..to {
            assert_eq!(run.shown[f], Some(PLACES[p - 1]), "frame {f}");
            p = if p == 25 { 6 } else { p + 1 };
        }
    }

    #[test]
    fn the_second_character_s_roll_and_its_stop() {
        // player-frames.md, "A roll", second character: a 24-frame roll
        // from place 2, then the stop.
        let run = run(world(FLAT, 1), &roll(&[], 80));
        let s = run.frames(Event::RollStart)[0];
        let d = run.frames(Event::RollEnd)[0];
        assert_eq!(d, s + 24);
        let mut want = vec![
            ("s-stop-5", 1),
            ("s-stop-4", 1),
            ("s-stop-3", 1),
            ("s-roll-2", 1),
        ];
        for r in [
            "s-roll-3",
            "s-roll-4",
            "s-roll-5",
            "s-roll-6",
            "s-roll-7",
            "s-roll-8",
            "s-roll-9",
            "s-roll-10",
            "s-roll-11",
            "s-roll-12",
        ] {
            want.push((r, 2));
        }
        want.push(("s-stop-1", 5));
        run.check(s, &want);
    }

    #[test]
    fn the_second_character_s_roll_from_d_plus_17_starts_at_place_1() {
        // The 25-frame roll on D + 17 of the stop starts at place 1, so
        // `s-roll-12` still shows on its last two frames; on D + 18 a
        // roll starts at place 2 again.
        let first = run(world(FLAT, 1), &roll(&[], 120));
        let d = first.frames(Event::RollEnd)[0];
        for (k, p, length) in [(17, 1, 25), (18, 2, 24)] {
            let mut keys = roll(&[], 120);
            keys[d + k] = vec![Action::Action];
            let run = run(world(FLAT, 1), &keys);
            let s = run.frames(Event::RollStart)[1];
            assert_eq!(s, d + k);
            let d2 = run.frames(Event::RollEnd)[1];
            assert_eq!(d2 - s, length, "the roll's length");
            check_list(&run, s, d2, p);
            run.check(d2 - 2, &[("s-roll-12", 2), ("s-stop-1", 1)]);
        }
    }

    #[test]
    fn the_second_character_s_roll_off_a_ledge_starts_over_on_l_and_g() {
        let level = walk_off_ledge()
            .replace("[1024, 480]", "[1024, 960]")
            .replace("[120, 448, 904, 32]", "[120, 928, 904, 32]");
        let keys: Vec<Vec<Action>> = (0..160)
            .map(|f| match f {
                10 => vec![Action::Right, Action::Action],
                _ => vec![Action::Right],
            })
            .collect();
        let run = run(world(&level, 1), &keys);
        let s = run.frames(Event::RollStart)[0];
        let l = (s..).find(|&f| !run.grounded[f]).unwrap();
        let g = run.frames(Event::Land)[0];
        let d = run.frames(Event::RollEnd)[0];
        assert!(l > s && g > l + 26, "a long fall: S {s}, L {l}, G {g}");
        assert_eq!(d, g + 25);
        check_list(&run, s, l, 2);
        check_list(&run, l, g, 1);
        check_list(&run, g, d, 1);
        run.check(d, &[("s-stop-1", 5)]);
    }

    #[test]
    fn the_second_character_s_roll_holds_its_picture_through_a_defeat_s_pause() {
        let level = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/bounce-walker.level"
        ))
        .unwrap();
        let keys: Vec<Vec<Action>> = (0..100)
            .map(|f| {
                if f == 25 {
                    vec![Action::Action]
                } else {
                    vec![]
                }
            })
            .collect();
        let run = run(world(&level, 1), &keys);
        let s = run.frames(Event::RollStart)[0];
        let k = (s..).find(|&f| run.frozen[f] > 0).unwrap();
        assert!(k + 3 < s + 22, "mid-roll: S {s}, K {k}");
        check_list(&run, s, k + 1, 2);
        assert_eq!(run.shown[k + 1], run.shown[k]);
        assert_eq!(run.shown[k + 2], run.shown[k]);
        // The roll's own D, then the carrying on into running.
        let d = run.frames(Event::RollEnd)[0] - 6;
        assert_eq!(d, s + 26);
        check_list(&run, k + 3, d, k + 3 - s);
        run.check(d, &[("s-stop-1", 3), ("s-stop-2", 3), ("s-walk-1", 1)]);
    }

    #[test]
    fn a_boost_starts_the_second_character_s_list_at_place_2() {
        let level = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/bounce-walker.level"
        ))
        .unwrap();
        let plain: Vec<Vec<Action>> = (0..100)
            .map(|f| {
                if f == 25 {
                    vec![Action::Action]
                } else {
                    vec![]
                }
            })
            .collect();
        let k = {
            let run = run(world(&level, 1), &plain);
            let s = run.frames(Event::RollStart)[0];
            (s..).find(|&f| run.frozen[f] > 0).unwrap()
        };
        // Action held again from K + 3, so P = K + 3.
        let keys: Vec<Vec<Action>> = (0..120)
            .map(|f| match f {
                25 => vec![Action::Action],
                f if f >= k + 3 => vec![Action::Action],
                _ => vec![],
            })
            .collect();
        let run = run(world(&level, 1), &keys);
        let p = k + 3;
        assert_eq!(run.frames(Event::RollEnd)[0], p + 30);
        check_list(&run, p, p + 24, 2);
    }

    /// Flat floor to a ledge with a steep face at the top, its edge at
    /// x = 1600, and floor 160 px lower after it; the player starts at
    /// `x`.
    fn faced_ledge(x: f64) -> String {
        let mut points = vec![[0, 400], [1600, 400]];
        for c in 1600..1604 {
            let y = 402 + 4 * (c - 1600);
            points.extend([[c, y], [c + 1, y]]);
        }
        points.extend([[1604, 560], [2048, 560], [2048, 599], [0, 599]]);
        format!(
            "format = 1\nsize = [2048, 600]\nstart = [{x}, 400]\n\
             [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n"
        )
    }

    #[test]
    fn a_stopped_roll_carrying_on_stands_from_d_plus_6() {
        // Stopped at the top of the face with Action alone held: the
        // roll list goes on, the carrying on shows its stop pictures,
        // then standing from D + 6 with `s-stand-1` x 4.
        let keys: Vec<Vec<Action>> = (0..80)
            .map(|f| {
                if f >= 10 {
                    vec![Action::Action]
                } else {
                    vec![]
                }
            })
            .collect();
        let run = run(world(&faced_ledge(1590.0), 1), &keys);
        let s = run.frames(Event::RollStart)[0];
        let d = run.frames(Event::RollEnd)[0] - 6;
        assert_eq!(d, s + 24);
        check_list(&run, s, d, 2);
        run.check(
            d,
            &[
                ("s-stop-1", 3),
                ("s-stop-2", 3),
                ("s-stand-1", 4),
                ("s-stand-2", 4),
            ],
        );
    }

    #[test]
    fn a_stopped_roll_carrying_on_stands_from_d_plus_3_first_character() {
        // spec/behavior/player-frames.md, "The roll carries on into
        // running": `stop` x 3 from D, then `stand-1` x 3 from D + 3 and
        // the standing pictures with D + 3 as S, 14 px from the end.
        let keys: Vec<Vec<Action>> = (0..80)
            .map(|f| {
                if f >= 10 {
                    vec![Action::Action]
                } else {
                    vec![]
                }
            })
            .collect();
        let run = run(world(&faced_ledge(1586.0), 0), &keys);
        let d = run.frames(Event::RollEnd)[0] - 3;
        run.check(d, &[("stop", 3)]);
        run.check(d + 3, &standing_list(80 - d - 3));
    }

    /// The second character walking right onto [`faced_ledge`]'s face
    /// from standing, with Right held to the frame that leaves it, then
    /// `then(n)` held on G + `n` (nothing before G): the run, F and G.
    fn second_face_fall(then: impl Fn(usize) -> Vec<Action>) -> (Run, usize, usize) {
        face_fall_run(1, then)
    }

    fn face_fall_run(slot: usize, then: impl Fn(usize) -> Vec<Action>) -> (Run, usize, usize) {
        let keys = |g: Option<usize>| -> Vec<Vec<Action>> {
            let probe = run(
                world(&faced_ledge(1580.0), slot),
                &vec![vec![Action::Right]; 60],
            );
            let leave = probe.grounded.iter().position(|g| !g).unwrap();
            (0..90)
                .map(|f| match g {
                    _ if f <= leave => vec![Action::Right],
                    Some(g) if f >= g => then(f - g),
                    _ => vec![],
                })
                .collect()
        };
        let probe = run(world(&faced_ledge(1580.0), slot), &keys(None));
        let g = probe.frames(Event::Land)[0];
        let run = run(world(&faced_ledge(1580.0), slot), &keys(Some(g)));
        assert_eq!(run.frames(Event::Land)[0], g);
        let f = run.on_face.iter().position(|&on| on).unwrap();
        assert!(run.grounded[f] && !run.grounded[f + 1..g].iter().all(|&g| g));
        (run, f, g)
    }

    /// `s-air-1` from F to G − 1, then the landing frames from G to
    /// G + `n` − 1 ("A fall from a steep face").
    fn face_fall_list(f: usize, g: usize, n: usize) -> Vec<(&'static str, usize)> {
        let mut want = vec![("s-air-1", g - f)];
        let landing = [
            "s-air-2", "s-air-3", "s-air-4", "s-air-5", "s-air-6", "s-land-1", "s-land-2",
        ];
        want.extend(landing[..n].iter().map(|&l| (l, 1)));
        want
    }

    #[test]
    fn the_second_character_s_fall_from_a_face_and_its_landing() {
        // player-frames.md, "A fall from a steep face", the example:
        // Right still held, walking from G + 7.
        let (run, f, g) = second_face_fall(|_| vec![Action::Right]);
        let mut want = face_fall_list(f, g, 7);
        want.push(("s-walk-1", 1));
        run.check(f, &want);
        // Nothing held: standing from G + 7, with G + 7 as S.
        let (run, f, g) = second_face_fall(|_| vec![]);
        let mut want = face_fall_list(f, g, 7);
        want.extend(second_standing_list(30));
        run.check(f, &want);
        // The other way held: a turn from G + 7.
        let (run, f, g) = second_face_fall(|_| vec![Action::Left]);
        let mut want = face_fall_list(f, g, 7);
        want.extend([("s-stand-5", 2), ("s-stand-7", 4), ("s-stand-5", 2)]);
        run.check(f, &want);
    }

    #[test]
    fn the_first_character_s_fall_from_a_face_and_its_landing() {
        // player-frames.md, "A fall from a steep face", "First
        // character", the example: `air-0` from F to G − 1, the table on
        // G to G + 10, then the mode that starts on G + 11.
        let table = [
            "air-1", "air-2", "air-3", "air-4", "air-5", "air-6", "land-1", "land-2", "land-2",
            "land-3", "land-3",
        ];
        let list = |f: usize, g: usize| {
            let mut want = vec![("air-0", g - f)];
            want.extend(table.iter().map(|&l| (l, 1)));
            want
        };
        let (run, f, g) = face_fall_run(0, |_| vec![Action::Right]);
        let mut want = list(f, g);
        want.push(("walk-1", 1));
        run.check(f, &want);
        // Nothing held: standing from G + 11, with G + 11 as S.
        let (run, f, g) = face_fall_run(0, |_| vec![]);
        let mut want = list(f, g);
        want.extend([("stand-1", 3), ("stand-2", 3)]);
        run.check(f, &want);
    }

    #[test]
    fn a_roll_or_a_jump_ends_the_second_character_s_landing_frames() {
        // A roll pressed on G + 3 starts there, from place 2 of its list;
        // one pressed on G starts on G + 1.
        let (run, f, g) = second_face_fall(|n| vec![Action::Action; usize::from(n >= 3)]);
        assert_eq!(run.frames(Event::RollStart), [g + 3]);
        run.check(f, &face_fall_list(f, g, 3));
        check_list(&run, g + 3, g + 10, 2);
        let (run, f, g) = second_face_fall(|_| vec![Action::Action]);
        assert_eq!(run.frames(Event::RollStart), [g + 1]);
        run.check(f, &face_fall_list(f, g, 1));
        check_list(&run, g + 1, g + 10, 2);
        // A jump pressed on G + 3: `s-jump-1` on its press frame, and it
        // starts on G + 4.
        let (run, f, g) = second_face_fall(|n| vec![Action::Jump; usize::from(n >= 3)]);
        assert_eq!(run.frames(Event::JumpStart), [g + 4]);
        let mut want = face_fall_list(f, g, 3);
        want.extend([("s-jump-1", 1), ("s-jump-2", 2)]);
        run.check(f, &want);
    }

    #[test]
    fn the_second_character_s_jump_and_landing() {
        let run = run(world(FLAT, 1), &jump(10, 1, 120));
        // Jump held 1 frame: A = T + 23 and L = T + 37.
        let t = run.frames(Event::JumpStart)[0];
        assert_eq!(run.shown[t + 22], Some("s-air-1"));
        assert_eq!(run.shown[t + 23], Some("s-air-2"));
        assert_eq!(run.frames(Event::Land)[0], t + 37);
        second_jump_and_landing(&run);
    }

    #[test]
    fn the_second_character_s_jump_out_of_a_roll_and_landing() {
        // The same pictures as a plain jump, from the press frame T − 1.
        for p in [11, 20, 33] {
            let mut keys = roll(&[], 120);
            keys[p..p + 10]
                .iter_mut()
                .for_each(|k| k.push(Action::Jump));
            let run = run(world(FLAT, 1), &keys);
            assert_eq!(run.frames(Event::RollStart), [10], "press {p}");
            assert_eq!(run.frames(Event::JumpStart)[0], p + 1, "press {p}");
            second_jump_and_landing(&run);
        }
    }

    #[test]
    fn the_second_character_s_jump_out_of_a_roll_in_the_air_and_landing() {
        // Walking off the ledge in a roll pressed on frame 30, then Jump
        // pressed `after` frames after it leaves the ground and held,
        // with Right, 10 frames: the same pictures from the press frame
        // P = T − 1, landing below.
        let keys = |jump: Option<usize>| -> Vec<Vec<Action>> {
            (0..160)
                .map(|f| {
                    let mut k = vec![];
                    if jump.is_none_or(|p| f < p + 10) {
                        k.push(Action::Right);
                    }
                    if f == 30 {
                        k.push(Action::Action);
                    }
                    if jump.is_some_and(|p| (p..p + 10).contains(&f)) {
                        k.push(Action::Jump);
                    }
                    k
                })
                .collect()
        };
        let roll = run(world(&walk_off_ledge(), 1), &keys(None));
        assert_eq!(roll.frames(Event::RollStart), [30]);
        let leave = roll.grounded.iter().skip(30).position(|g| !g).unwrap() + 30;
        for after in [0, 3, 8] {
            let p = leave + after;
            let run = run(world(&walk_off_ledge(), 1), &keys(Some(p)));
            assert_eq!(run.frames(Event::JumpStart), [p + 1], "P = L + {after}");
            second_jump_and_landing(&run);
        }
    }

    /// Checks the second character's pictures from T − 1 of `run`'s
    /// first jump to the first standing pictures after its landing.
    fn second_jump_and_landing(run: &Run) {
        let t = run.frames(Event::JumpStart)[0];
        let l = run.frames(Event::Land)[0];
        let mut want = vec![("s-jump-1", 1), ("s-jump-2", 2)];
        for r in [
            "s-rise-1", "s-rise-2", "s-rise-3", "s-rise-4", "s-rise-5", "s-rise-6", "s-rise-7",
        ] {
            want.push((r, 2));
        }
        // `s-air-1` up to A − 1, then the table of "A jump and its landing".
        let a = (t + 16..l)
            .find(|&f| run.shown[f] != Some("s-air-1"))
            .expect("approach frame");
        assert!(a >= t + 20, "A = T + {}", a - t);
        want.push(("s-air-1", a - (t + 16)));
        for (label, n) in [
            ("s-air-2", 1),
            ("s-air-3", 2),
            ("s-air-4", 2),
            ("s-air-5", 2),
        ] {
            want.push((label, n));
        }
        want.push(("s-air-6", l - (a + 7)));
        want.extend([
            ("s-land-1", 1),
            ("s-land-2", 2),
            ("s-land-3", 3),
            ("s-land-4", 8),
            ("s-land-3", 3),
            ("s-stand-1", 4),
            ("s-stand-2", 4),
        ]);
        run.check(t - 1, &want);
    }

    /// Standing still on the bounce-walker level, a hop held `hold`
    /// frames that comes down on the walker coming, with `then` held
    /// from the frame after its bounce frame B. B and the landing frame G.
    fn bounce_run(slot: usize, hold: usize, then: &[Action]) -> (Run, usize, usize) {
        let level = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/bounce-walker.level"
        ))
        .unwrap();
        (20..80)
            .find_map(|wait| {
                let keys = jump(wait, hold, 200);
                let b = run(world(&level, slot), &keys)
                    .frames(Event::Bounce)
                    .first()
                    .copied()?;
                let keys: Vec<Vec<Action>> = keys
                    .into_iter()
                    .enumerate()
                    .map(|(f, k)| if f > b { then.to_vec() } else { k })
                    .collect();
                let run = run(world(&level, slot), &keys);
                let g = *run.frames(Event::Land).iter().find(|&&g| g > b)?;
                Some((run, b, g))
            })
            .expect("a hop bounces")
    }

    /// `slot`'s bounce pictures from B to G − 1 ("A bounce"): the first
    /// to B + 3, then each 2 frames from the second on B + 4, then
    /// round again from the first on B + 34.
    fn bounce_list(slot: usize, b: usize, g: usize) -> Vec<(&'static str, usize)> {
        let labels = labels(slot, Set::Bounce);
        let mut want = vec![(labels[0], 4)];
        let mut f = b + 4;
        let mut i = 1;
        while f < g {
            let n = (g - f).min(2);
            want.push((labels[i % 16], n));
            f += n;
            i += 1;
        }
        want
    }

    /// A player standing on a barrel that shoots left, on flat floor.
    fn barrel_world(slot: usize) -> World {
        let src = r#"
            format = 1
            name = "barrel"
            size = [4000, 800]
            start = [600, 337]

            [[collision]]
            shape = "rect"
            rect = [0, 337, 4000, 200]

            [[entities]]
            type = "sling"
            at = [600, 337]
            "#;
        world(src, slot)
    }

    /// Each character's pictures from the shot of a barrel until the
    /// landing: one bounce picture to a frame, round and round
    /// ("In a launch barrel").
    #[test]
    fn the_bounce_pictures_one_to_a_frame_after_a_barrel_shoots() {
        for slot in 0..2 {
            let mut world = barrel_world(slot);
            let mut clock = PictureClock::default();
            let mut flight = Vec::new();
            for _ in 0..80 {
                let input = InputFrame::single(Frame::after(Actions::NONE, Actions::NONE));
                step(&mut world, &input);
                clock.update(&world, jump_in(&world, &input));
                if let Some(slung) = world.player.sling {
                    let shot = u16::from(slung.values.hold) + 1;
                    if slung.since >= shot {
                        flight.push((slung.since - shot, clock.shown().map(|(_, l)| l)));
                    }
                }
            }
            let labels = labels(slot, Set::Bounce);
            assert!(flight.len() > 20, "{}", flight.len());
            for (n, shown) in flight {
                assert_eq!(
                    shown,
                    Some(labels[usize::from(n) % 16]),
                    "slot {slot}, frame {n}"
                );
            }
        }
    }

    #[test]
    fn the_first_character_s_bounce_pictures_until_the_landing() {
        let (run, b, g) = bounce_run(0, 20, &[Action::Jump]);
        assert!(g - b > 34, "round again from B + 34: {}", g - b);
        let mut want = bounce_list(0, b, g);
        // No landing pictures: standing from G, as in "Standing still".
        want.extend(standing_list(16));
        run.check(b, &want);
    }

    #[test]
    fn the_second_character_s_bounce_pictures_until_the_landing() {
        for hold in [1, 20] {
            let (run, b, g) = bounce_run(1, hold, &[Action::Jump]);
            assert!(
                hold == 1 || g - b > 34,
                "round again from B + 34: {}",
                g - b
            );
            let mut want = bounce_list(1, b, g);
            // No landing pictures: standing from G, with G as S.
            want.extend(second_standing_list(30));
            run.check(b, &want);
        }
    }

    #[test]
    fn walking_or_a_turn_from_a_bounce_s_landing_frame() {
        let (run, _, g) = bounce_run(0, 1, &[Action::Right]);
        run.check(g, &[("walk-1", 1)]);
        let (run, _, g) = bounce_run(0, 1, &[Action::Left]);
        run.check(g, &[("turn-a", 2)]);
    }

    #[test]
    fn the_second_character_s_mode_from_a_bounce_s_landing_frame() {
        // "A bounce", "The second character, from B + 4 to the landing":
        // the mode that starts on G shows its own pictures from G.
        let (run, _, g) = bounce_run(1, 1, &[Action::Right]);
        run.check(g, &[("s-walk-1", 1)]);
        let (run, _, g) = bounce_run(1, 1, &[Action::Action, Action::Right]);
        run.check(g, &[("s-run-1", 1)]);
        let (run, _, g) = bounce_run(1, 1, &[Action::Left]);
        run.check(g, &[("s-stand-5", 2), ("s-stand-7", 4), ("s-stand-5", 2)]);
    }

    /// Nothing held for 10 frames, then `then` held for `hold` frames,
    /// then nothing, `total` frames in all.
    fn hold(then: &[Action], hold: usize, total: usize) -> Vec<Vec<Action>> {
        (0..total)
            .map(|f| {
                if (10..10 + hold).contains(&f) {
                    then.to_vec()
                } else {
                    vec![]
                }
            })
            .collect()
    }

    #[test]
    fn walking_from_standing_still() {
        // player-frames.md's example: Right held from frame 0.
        let run = run(world(FLAT, 0), &hold(&[Action::Right], 40, 60));
        run.check(
            10,
            &[
                ("walk-1", 3),
                ("walk-2", 3),
                ("walk-3", 2),
                ("walk-4", 3),
                ("walk-5", 2),
                ("walk-6", 2),
                ("walk-7", 2),
                ("walk-8", 1),
                ("walk-9", 1),
            ],
        );
    }

    #[test]
    fn running_shows_a_running_picture_per_step() {
        // A roll carried on into walking, then running.
        let run = run(world(FLAT, 0), &roll(&[Action::Action, Action::Right], 90));
        let walk = run.shown.iter().position(|&l| l == Some("walk-1")).unwrap();
        let from = run.shown.iter().position(|&l| l == Some("run-1")).unwrap();
        assert!(
            run.shown[walk..from]
                .iter()
                .all(|l| l.unwrap().starts_with("walk-"))
        );
        let steps: Vec<usize> = run.shown[from..]
            .iter()
            .map(|l| l.unwrap().strip_prefix("run-").unwrap().parse().unwrap())
            .collect();
        assert!(
            steps
                .windows(2)
                .all(|w| w[1] == w[0] || w[1] == w[0] % 20 + 1)
        );
        assert!(steps.contains(&20), "{steps:?}");
    }

    #[test]
    fn a_turn_from_standing_still() {
        let run = run(world(FLAT, 0), &hold(&[Action::Left], 1, 30));
        let turn = run.frames(Event::Turn);
        assert_eq!(turn, [14], "facing changes on T + 4");
        run.check(
            10,
            &[("turn-a", 2), ("turn-b", 4), ("turn-a", 2), ("stand-1", 3)],
        );
    }

    #[test]
    fn the_second_character_walking_and_running() {
        // player-frames.md's example: Right and Action held from frame 0
        // (here 10), Action held from before, so not pressed.
        let keys: Vec<Vec<Action>> = (0..40)
            .map(|f| match f {
                0..10 => vec![Action::Action],
                _ => vec![Action::Action, Action::Right],
            })
            .collect();
        let run = run_after(world(FLAT, 1), Action::Action.into(), &keys);
        run.check(
            10,
            &[
                ("s-walk-1", 2),
                ("s-walk-2", 2),
                ("s-run-1", 2),
                ("s-run-2", 2),
                ("s-run-3", 2),
                ("s-run-4", 2),
                ("s-run-5", 1),
            ],
        );
    }

    #[test]
    fn the_second_character_s_walking_cycle_and_standing_after_it() {
        let run = run(world(FLAT, 1), &hold(&[Action::Right], 60, 120));
        let steps: Vec<usize> = run.shown[10..70]
            .iter()
            .map(|l| l.unwrap().strip_prefix("s-walk-").unwrap().parse().unwrap())
            .collect();
        assert!(
            steps
                .windows(2)
                .all(|w| w[1] == w[0] || w[1] == w[0] % 16 + 1)
        );
        assert!(steps.contains(&16), "{steps:?}");
        let stand = run.shown[70..]
            .iter()
            .position(|&l| l == Some("s-stand-1"))
            .unwrap()
            + 70;
        assert!(
            run.shown[70..stand]
                .iter()
                .all(|l| l.unwrap().starts_with("s-walk-"))
        );
        run.check(stand, &[("s-stand-1", 4), ("s-stand-2", 4)]);
    }

    #[test]
    fn a_turn_of_the_second_character_from_standing_still() {
        let run = run(world(FLAT, 1), &hold(&[Action::Left], 1, 30));
        assert_eq!(run.frames(Event::Turn), [14], "facing changes on T + 4");
        run.check(
            10,
            &[
                ("s-stand-5", 2),
                ("s-stand-7", 4),
                ("s-stand-5", 2),
                ("s-stand-1", 4),
            ],
        );
    }

    /// Right (and Action) held from just after a standing jump's take-off
    /// through the landing, jump held a few frames: the run and its L.
    fn jump_then_hold(slot: usize, then: &[Action]) -> (Run, usize) {
        let keys: Vec<Vec<Action>> = (0..90)
            .map(|f| match f {
                10 => vec![Action::Jump],
                11..=14 => then.iter().copied().chain([Action::Jump]).collect(),
                f if f > 14 => then.to_vec(),
                _ => vec![],
            })
            .collect();
        let played = run(world(FLAT, slot), &keys);
        let l = played.frames(Event::Land)[0];
        (played, l)
    }

    #[test]
    fn the_first_character_runs_from_m_and_walks_from_k_after_a_landing() {
        // player-frames.md, "A jump and its landing": run-1 on M = L + 1,
        // walk-1 on K = L + 7.
        let (run, l) = jump_then_hold(0, &[Action::Right, Action::Action]);
        run.check(
            l + 1,
            &[("run-1", 2), ("run-2", 2), ("run-3", 1), ("run-4", 2)],
        );
        let (run, l) = jump_then_hold(0, &[Action::Right]);
        run.check(l + 1, &[("land-1", 1), ("land-2", 2), ("land-3", 3)]);
        run.check(
            l + 7,
            &[("walk-1", 2), ("walk-2", 2), ("walk-3", 1), ("walk-4", 2)],
        );
    }

    #[test]
    fn the_second_character_runs_from_l_after_a_landing() {
        let (held, l) = jump_then_hold(1, &[Action::Right, Action::Action]);
        held.check(
            l,
            &[
                ("s-run-1", 2),
                ("s-run-2", 2),
                ("s-run-3", 1),
                ("s-run-4", 2),
            ],
        );
    }

    #[test]
    fn the_second_character_walks_on_from_a_landing() {
        // From L + 17 with Right held: walking pictures, from s-walk-1.
        let keys: Vec<Vec<Action>> = (0..120)
            .map(|f| match f {
                10 => vec![Action::Jump],
                f if f > 10 => vec![Action::Right],
                _ => vec![],
            })
            .collect();
        let run = run(world(FLAT, 1), &keys);
        let l = run.frames(Event::Land)[0];
        assert_eq!(run.shown[l + 16], Some("s-land-3"));
        run.check(l + 17, &[("s-walk-1", 2), ("s-walk-2", 2)]);
    }

    #[test]
    fn standing_after_walking_or_a_turn() {
        let run = run(world(FLAT, 0), &hold(&[Action::Right], 6, 60));
        let stand = run
            .shown
            .iter()
            .position(|&l| l == Some("stand-1"))
            .unwrap();
        assert!(
            run.shown[10..stand]
                .iter()
                .all(|l| l.unwrap().starts_with("walk-"))
        );
        run.check(
            stand,
            &[
                ("stand-1", 3),
                ("stand-2", 3),
                ("stand-3", 3),
                ("stand-4", 3),
                ("stand-5", 3),
                ("stand-6", 10),
            ],
        );
    }

    fn walk_off_ledge() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/walk-off-ledge.level"
        ))
        .unwrap()
    }

    /// Plays `keys` off the ledge and on flat floor from the same x:
    /// the pictures are the same on every frame before `until`, in the
    /// air too. Gives the run off the ledge and its first airborne and
    /// landing frames.
    fn same_as_on_flat_floor(keys: &[Vec<Action>], until: usize) -> (Run, usize, usize) {
        let off = run(world(&walk_off_ledge(), 0), keys);
        let flat = run(world(FLAT, 0), keys);
        let l = off.grounded.iter().position(|g| !g).unwrap();
        let g = off.frames(Event::Land)[0];
        assert!(g > l + 10, "a fall: L {l}, G {g}");
        assert_eq!(off.shown[..until], flat.shown[..until]);
        (off, l, g)
    }

    #[test]
    fn walking_goes_on_in_the_air_after_a_walk_off() {
        let walk: Vec<Vec<Action>> = (0..80).map(|_| vec![Action::Right]).collect();
        let (run, l, g) = same_as_on_flat_floor(&walk, 80);
        assert!(
            run.shown[l..=g]
                .iter()
                .all(|l| l.unwrap().starts_with("walk-"))
        );
    }

    #[test]
    fn the_second_character_walks_on_in_the_air_after_a_walk_off() {
        // player-frames.md, "A fall from a steep face": passing over a
        // face without a frame on it, the stride cycle and its pictures
        // go on in the air; off a ledge without a face too.
        let walk: Vec<Vec<Action>> = (0..80).map(|_| vec![Action::Right]).collect();
        let off = run(world(&walk_off_ledge(), 1), &walk);
        let flat = run(world(FLAT, 1), &walk);
        let l = off.grounded.iter().position(|g| !g).unwrap();
        let g = off.frames(Event::Land)[0];
        assert!(g > l + 10, "a fall: L {l}, G {g}");
        assert_eq!(off.shown, flat.shown);
        assert!(off.on_face.iter().all(|&on| !on));
    }

    #[test]
    fn the_second_character_s_pictures_in_a_walk_off_follow_the_ground_rules() {
        // player-frames.md, "Second character": off a ledge without a
        // face the turn, running and standing pictures are those of the
        // ground, and the landing frame shows its step's picture.
        for then in [vec![], vec![Action::Left]] {
            let keys = off_the_ledge_then(&then);
            let off = run(world(&walk_off_ledge(), 1), &keys);
            let flat = run(world(FLAT, 1), &keys);
            let l = off.grounded.iter().position(|g| !g).unwrap();
            let g = off.frames(Event::Land)[0];
            assert!(g > l + 10, "a fall: L {l}, G {g}");
            assert_eq!(off.shown[..=g], flat.shown[..=g], "{then:?}");
            assert!(
                off.shown[l..=g]
                    .iter()
                    .all(|l| !l.unwrap().starts_with("s-air-") && !l.unwrap().starts_with("s-land"))
            );
        }
    }

    #[test]
    fn the_second_character_runs_in_the_air_after_a_walk_off() {
        // Action pressed in the air: on flat floor it would roll.
        let keys = off_the_ledge_then(&[Action::Right, Action::Action]);
        let run = run(world(&walk_off_ledge(), 1), &keys);
        let l = run.grounded.iter().position(|g| !g).unwrap();
        let g = run.frames(Event::Land)[0];
        let r = run
            .shown
            .iter()
            .position(|&l| l == Some("s-run-1"))
            .unwrap();
        assert!(
            l < r && r < g,
            "running starts in the air: L {l}, {r}, G {g}"
        );
        let gait = |l: &str| l.starts_with("s-walk-") || l.starts_with("s-run-");
        assert!(run.shown[l..=g].iter().all(|l| gait(l.unwrap())));
    }

    /// Right held from frame 10 until the fall's second frame, then
    /// `then` held.
    fn off_the_ledge_then(then: &[Action]) -> Vec<Vec<Action>> {
        let walk = run(world(&walk_off_ledge(), 0), &hold(&[Action::Right], 60, 80));
        let l = walk.grounded.iter().position(|g| !g).unwrap();
        (0..90)
            .map(|f| match f {
                0..10 => vec![],
                f if f <= l => vec![Action::Right],
                _ => then.to_vec(),
            })
            .collect()
    }

    #[test]
    fn a_turn_in_the_air_after_a_walk_off() {
        // Until the turn's end: walking back, it meets the ledge's side.
        let keys = off_the_ledge_then(&[Action::Left]);
        let flat = run(world(FLAT, 0), &keys);
        let t = flat
            .shown
            .iter()
            .position(|&l| l == Some("turn-a"))
            .unwrap();
        let (run, l, g) = same_as_on_flat_floor(&keys, t + 8);
        assert!(
            l < t && t + 8 < g,
            "the turn is in the air: L {l}, T {t}, G {g}"
        );
        run.check(t, &[("turn-a", 2), ("turn-b", 4), ("turn-a", 2)]);
        assert_eq!(run.frames(Event::Turn), [t + 4]);
    }

    #[test]
    fn standing_in_the_air_after_a_walk_off() {
        let (run, l, g) = same_as_on_flat_floor(&off_the_ledge_then(&[]), 90);
        let stand = run
            .shown
            .iter()
            .position(|&l| l == Some("stand-1"))
            .unwrap();
        assert!(
            l < stand && stand < g,
            "standing starts in the air: L {l}, {stand}, G {g}"
        );
        assert!(
            run.shown[l..stand]
                .iter()
                .all(|l| l.unwrap().starts_with("walk-"))
        );
        run.check(
            stand,
            &[
                ("stand-1", 3),
                ("stand-2", 3),
                ("stand-3", 3),
                ("stand-4", 3),
                ("stand-5", 3),
                ("stand-6", 1),
            ],
        );
    }

    #[test]
    fn running_started_in_the_air_after_a_walk_off() {
        // Action pressed in the air: on flat floor it would roll.
        let keys = off_the_ledge_then(&[Action::Right, Action::Action]);
        let run = run(world(&walk_off_ledge(), 0), &keys);
        let l = run.grounded.iter().position(|g| !g).unwrap();
        let g = run.frames(Event::Land)[0];
        let r = run.shown.iter().position(|&l| l == Some("run-1")).unwrap();
        assert!(
            l < r && r < g,
            "running starts in the air: L {l}, {r}, G {g}"
        );
        let gait = |l: &str| l.starts_with("walk-") || l.starts_with("run-");
        assert!(run.shown[l..=g].iter().all(|l| gait(l.unwrap())));
    }

    /// The picture on cycle frame `c` of a roll.
    fn cycle(c: usize) -> &'static str {
        labels(0, Set::Roll)[(c - 1) % 33 / 3]
    }

    /// Checks the roll cycle on frames `from` to `to - 1`, with `from` on
    /// cycle frame `c`.
    fn check_cycle(run: &Run, from: usize, to: usize, c: usize) {
        for f in from..to {
            assert_eq!(run.shown[f], Some(cycle(c + f - from)), "frame {f}");
        }
    }

    #[test]
    fn a_roll_on_flat_ground_and_its_stop() {
        let run = run(world(FLAT, 0), &roll(&[], 80));
        let s = run.frames(Event::RollStart)[0];
        let d = run.frames(Event::RollEnd)[0];
        assert_eq!(d, s + 29);
        let mut want = vec![("roll-1", 2)];
        for r in 2..=9 {
            want.push((cycle(3 * r - 2), 3));
        }
        want.extend([("roll-end", 3), ("roll-end", 3), ("stop", 6)]);
        run.check(s, &want);
    }

    #[test]
    fn the_next_roll_from_the_stop_s_last_frame_starts_a_frame_later() {
        for (k, c) in [(17, 1), (18, 2)] {
            let first = run(world(FLAT, 0), &roll(&[], 120));
            let d = first.frames(Event::RollEnd)[0];
            let mut keys = roll(&[], 120);
            keys[d + k] = vec![Action::Action];
            let run = run(world(FLAT, 0), &keys);
            let s = run.frames(Event::RollStart)[1];
            assert_eq!(s, d + k);
            let d2 = run.frames(Event::RollEnd)[1];
            assert_eq!(d2 - s, 29 + 2 - c, "the roll's length");
            check_cycle(&run, s, d2, c);
            run.check(d2 - 3, &[("roll-end", 6)]);
        }
    }

    #[test]
    fn a_roll_off_a_ledge_starts_the_cycle_over_in_the_air_and_on_landing() {
        let level = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/walk-off-ledge.level"
        ))
        .unwrap()
        // A longer drop, for a fall of more than 30 frames.
        .replace("[1024, 480]", "[1024, 960]")
        .replace("[120, 448, 904, 32]", "[120, 928, 904, 32]");
        let keys: Vec<Vec<Action>> = (0..160)
            .map(|f| match f {
                10 => vec![Action::Right, Action::Action],
                _ => vec![Action::Right],
            })
            .collect();
        let run = run(world(&level, 0), &keys);
        let s = run.frames(Event::RollStart)[0];
        let l = (s..).find(|&f| !run.grounded[f]).unwrap();
        let g = run.frames(Event::Land)[0];
        let d = run.frames(Event::RollEnd)[0];
        assert!(l > s && g > l + 33, "a long fall: S {s}, L {l}, G {g}");
        assert_eq!(d, g + 30);
        check_cycle(&run, s, l, 2);
        check_cycle(&run, l, g, 1);
        check_cycle(&run, g, d, 1);
        run.check(g + 27, &[("roll-end", 6), ("stop", 6)]);
    }

    #[test]
    fn a_roll_defeating_a_walker_holds_its_picture_through_the_pause() {
        let level = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/bounce-walker.level"
        ))
        .unwrap();
        // Pressed late enough that the walker is met mid-roll.
        let keys: Vec<Vec<Action>> = (0..100)
            .map(|f| {
                if f == 25 {
                    vec![Action::Action]
                } else {
                    vec![]
                }
            })
            .collect();
        let run = run(world(&level, 0), &keys);
        let s = run.frames(Event::RollStart)[0];
        let k = (s..).find(|&f| run.frozen[f] > 0).unwrap();
        assert!(k + 3 < s + 26, "mid-roll: S {s}, K {k}");
        // The roll's own D, with `roll_end` on D + 5 without a boost.
        let d = run.frames(Event::RollEnd)[0] - 5;
        assert_eq!(d, s + 29);
        check_cycle(&run, s, k + 1, 2);
        assert_eq!(run.shown[k + 1], run.shown[k]);
        assert_eq!(run.shown[k + 2], run.shown[k]);
        check_cycle(&run, k + 3, d + 2, k + 3 - s);
        run.check(d - 1, &[("roll-end", 3), ("stop", 3), ("walk-1", 1)]);
    }

    #[test]
    fn a_boost_starts_the_cycle_over() {
        let level = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/bounce-walker.level"
        ))
        .unwrap();
        // As above, with Action held again from K + 3, so P = K + 3.
        let keys: Vec<Vec<Action>> = (0..120)
            .map(|f| match f {
                25 | 48.. => vec![Action::Action],
                _ => vec![],
            })
            .collect();
        let run = run(world(&level, 0), &keys);
        let s = run.frames(Event::RollStart)[0];
        let k = (s..).find(|&f| run.frozen[f] > 0).unwrap();
        let p = k + 3;
        assert_eq!(p, 48);
        assert_eq!(run.frames(Event::RollEnd)[0], p + 32);
        check_cycle(&run, s, k + 1, 2);
        check_cycle(&run, p, p + 29, 2);
        run.check(p + 26, &[("roll-end", 3), ("stop", 3), ("walk-1", 1)]);
    }

    /// The sway, as in "Standing still".
    fn sway_list() -> Vec<(&'static str, usize)> {
        let s = |n: usize| labels(0, Set::Standing)[n - 1];
        let mut want = Vec::new();
        let mut run = |from: &[usize], last: usize, count: usize| {
            want.extend(from.iter().map(|&n| (s(n), 3)));
            want.push((s(last), count));
        };
        run(&[2, 3, 4, 5], 6, 10);
        run(&[7, 8, 9, 10], 11, 15);
        run(&[10, 9, 8, 7], 6, 5);
        run(&[7, 8, 9, 10], 11, 10);
        run(&[10, 9, 8, 7], 6, 15);
        run(&[5, 4, 3, 2], 1, 25);
        run(&[2, 3, 4, 5], 6, 10);
        want.extend([5, 4, 3, 2].map(|n| (s(n), 3)));
        assert_eq!(want.iter().map(|w| w.1).sum::<usize>(), 186);
        want
    }

    /// The wait, as in "Standing still".
    fn wait_list() -> Vec<(&'static str, usize)> {
        let w = |n: usize| labels(0, Set::Standing)[10 + n];
        let mut want: Vec<_> = (1..=9).map(|n| (w(n), 4)).collect();
        want.push((w(10), 2));
        want.extend((11..=16).map(|n| (w(n), 4)));
        let mut back_and_forth: Vec<usize> = (17..=26).collect();
        back_and_forth.extend((17..=25).rev());
        back_and_forth.extend(18..=26);
        back_and_forth.extend((17..=25).rev());
        back_and_forth.extend(18..=26);
        back_and_forth.extend((18..=25).rev());
        back_and_forth.extend(19..=26);
        assert_eq!(back_and_forth.len(), 62);
        want.extend(back_and_forth.into_iter().map(|n| (w(n), 2)));
        want.extend((27..=34).map(|n| (w(n), 4)));
        want.push((w(10), 2));
        want.extend((1..=9).rev().map(|n| (w(n), 4)));
        assert_eq!(want.iter().map(|w| w.1).sum::<usize>(), 256);
        want
    }

    /// The first character's standing pictures from S, `frames` of them.
    fn standing_list(frames: usize) -> Vec<(&'static str, usize)> {
        let mut want = Vec::new();
        for _ in 0..2 {
            want.push(("stand-1", 3));
            want.extend(sway_list());
            want.push(("stand-1", 12));
            want.extend(wait_list());
        }
        want.push(("stand-1", 3));
        want.extend(sway_list());
        assert_eq!(want.iter().map(|w| w.1).sum::<usize>(), 1103);
        while want.iter().map(|w| w.1).sum::<usize>() < frames {
            want.push(("stand-1", 15));
            want.extend(sway_list());
        }
        let mut left = frames;
        want.into_iter()
            .map_while(|(label, count)| {
                let n = count.min(left);
                left -= n;
                (n > 0).then_some((label, n))
            })
            .collect()
    }

    #[test]
    fn standing_still_after_a_roll_s_stop() {
        let run = run(world(FLAT, 0), &roll(&[], 2900));
        let d = run.frames(Event::RollEnd)[0];
        run.check(d + 17, &standing_list(2900 - d - 17));
    }

    /// `FLAT` with the player at `x`, or a platform ending at 120 with
    /// a lower floor beyond it, or the first slope and the flat bottom
    /// of `slope-dip.level`.
    fn stand_after_walking(level: &str, slot: usize, x: f64, walk: usize) -> Run {
        let src = level.replace("start = [64, 400]", &format!("start = [{x}, 400]"));
        run(world(&src, slot), &hold(&[Action::Right], walk, 1500))
    }

    /// A platform ending at column 119 with a lower floor beyond it.
    fn platform_end() -> String {
        FLAT.replace(
            "rect = [0, 400, 2048, 80]",
            "rect = [0, 400, 120, 80]\n[[collision]]\nshape = \"rect\"\nrect = [120, 440, 1928, 40]",
        )
    }

    /// The frame standing begins on (S) in a run of `slot`.
    fn first_stand(run: &Run, slot: usize) -> usize {
        let stand = ["stand-1", "s-stand-1"][slot];
        run.shown.iter().position(|&l| l == Some(stand)).unwrap()
    }

    #[test]
    fn the_teetering_loops_last_290_and_210_frames() {
        let first: Vec<u32> = (0..290).map(teeter_loop).collect();
        assert_eq!(first[0], 12);
        assert_eq!(first[289], 13);
        assert_eq!(first[5], 13);
        // 12 to 17 up, 12 down, 17 up, 12 down, then up to 22.
        let mut steps = first.clone();
        steps.dedup();
        assert_eq!(steps.len(), 70);
        let from_18 = first.windows(2).position(|w| w == [18, 17]).unwrap();
        assert!(first[from_18 + 1..].starts_with(&[17, 17, 17, 16]));
        assert_eq!(
            (0..210)
                .map(second_teeter_loop)
                .filter(|&n| n == 18)
                .count(),
            1 + 2 + 4 + 4 + 2
        );
        assert_eq!(second_teeter_loop(209), 6);
        assert_eq!(second_teeter_loop(210 - 55), 18);
    }

    #[test]
    fn standing_at_the_end_of_a_floor_teeters() {
        // player-frames.md, "Teetering at the end of a floor", the example.
        let level = platform_end();
        let run = stand_after_walking(&level, 0, 104.0, 3);
        let s = first_stand(&run, 0);
        let mut want = vec![("stand-1", 3)];
        want.extend((1..=11).map(|n| (TEETER[n - 1], 4)));
        want.extend([("teeter-12", 5), ("teeter-13", 5), ("teeter-14", 5)]);
        run.check(s, &want);
        // Much later, still the teetering loop.
        assert!(
            run.shown[s + 500..]
                .iter()
                .all(|l| l.unwrap().starts_with("teeter-"))
        );
    }

    #[test]
    fn the_second_character_teeters_at_the_end_of_a_floor() {
        let level = platform_end();
        let run = stand_after_walking(&level, 1, 104.0, 3);
        let s = first_stand(&run, 1);
        let mut want = vec![("s-stand-1", 4)];
        want.extend((1..=6).map(|n| (SECOND_TEETER[n - 1], 6)));
        want.extend([("s-teeter-7", 4), ("s-teeter-8", 4), ("s-teeter-9", 3)]);
        run.check(s, &want);
        assert!(
            run.shown[s + 500..]
                .iter()
                .all(|l| l.unwrap().starts_with("s-teeter-"))
        );
    }

    #[test]
    fn standing_after_a_runs_stop_teeters_at_the_end_of_a_floor() {
        // player-frames.md, "Teetering at the end of a floor": the 12-column
        // rule also holds when the player stops from a run. Running 45
        // frames from x, the stop comes 12 or fewer columns short of the
        // floor's end (column 299) for exactly 12 starting columns.
        let level = FLAT.replace(
            "rect = [0, 400, 2048, 80]",
            "rect = [0, 400, 300, 80]\n[[collision]]\nshape = \"rect\"\nrect = [300, 440, 1748, 40]",
        );
        for slot in [0, 1] {
            let teeters = |x: u32| {
                let src = level.replace("start = [64, 400]", &format!("start = [{x}, 400]"));
                let keys = hold(&[Action::Right, Action::Action], 45, 600);
                let run = run(world(&src, slot), &keys);
                let s = first_stand(&run, slot);
                run.shown[s..].iter().any(|l| l.unwrap().contains("teeter"))
            };
            let at: Vec<u32> = (100..168).filter(|&x| teeters(x)).collect();
            assert_eq!(at.len(), 12, "slot {slot}: {at:?}");
            assert_eq!(at[11] - at[0], 11, "slot {slot}: {at:?}");
        }
    }

    #[test]
    fn facing_away_from_the_end_does_not_teeter() {
        let level = platform_end();
        for slot in [0, 1] {
            let src = level.replace("start = [64, 400]", "start = [112, 400]");
            let run = run(world(&src, slot), &hold(&[Action::Left], 3, 800));
            let s = first_stand(&run, slot);
            assert!(
                run.shown[s..]
                    .iter()
                    .all(|l| !l.unwrap().contains("teeter"))
            );
        }
    }

    #[test]
    fn the_first_character_teeters_before_a_steep_face_with_a_drop_after_it() {
        // player-frames.md, "Teetering at the end of a floor", steep faces:
        // the count runs to the face's last column (1603) for the first
        // character, so standing near 1595 teeters and near 1570 doesn't.
        let level = faced_ledge(0.0);
        for (x, teeters) in [(1590.0, true), (1570.0, false)] {
            let src = level.replace("start = [0, 400]", &format!("start = [{x}, 400]"));
            let run = run(world(&src, 0), &hold(&[Action::Right], 3, 400));
            let s = first_stand(&run, 0);
            let any = run.shown[s..].iter().any(|l| l.unwrap().contains("teeter"));
            assert_eq!(any, teeters, "x = {x}");
        }
    }

    #[test]
    fn the_first_character_does_not_teeter_before_a_tall_steep_face() {
        // player-frames.md, "Teetering at the end of a floor": a face of
        // 128 px down (4 px per column) leading on to a lower floor 16
        // columns wide, after which the floor steps down again: standing
        // on the top 0 to 9 columns before the face's first column after
        // a walk's stop shows the standing pictures.
        let mut points = vec![[0, 300], [1600, 300]];
        points.extend((1601..=1632).map(|c| [c, 300 + 4 * (c - 1600)]));
        points.extend([[1648, 428], [1648, 520], [2048, 520], [2048, 599], [0, 599]]);
        let level = format!(
            "format = 1\nsize = [2048, 600]\nstart = [0, 300]\n\
             [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n"
        );
        for x in [1591.0, 1595.0, 1599.0] {
            let src = level.replace("start = [0, 300]", &format!("start = [{x}, 300]"));
            let run = run(world(&src, 0), &hold(&[Action::Right], 1, 300));
            let s = first_stand(&run, 0);
            assert!(run.grounded[s], "{x}");
            assert!(
                run.shown[s..]
                    .iter()
                    .all(|l| !l.unwrap().contains("teeter")),
                "{x}"
            );
        }
    }

    #[test]
    fn the_first_character_does_not_teeter_at_the_left_side_of_a_steep_face() {
        // player-frames.md, "Teetering at the end of a floor": a top with
        // a 64 px face (4 px per column) to its left and a lower floor at
        // its foot, no drop: standing after a walk's stop shows the
        // standing pictures.
        let mut points = vec![[0, 599], [0, 464], [984, 464]];
        points.extend((985..=1000).map(|c| [c, 464 - 4 * (c - 984)]));
        points.extend([[2048, 400], [2048, 599]]);
        let level = format!(
            "format = 1\nsize = [2048, 600]\nstart = [1010, 400]\n\
             [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n"
        );
        for walk in [1, 5, 10] {
            let run = run(world(&level, 0), &hold(&[Action::Left], walk, 200));
            let s = run
                .shown
                .iter()
                .position(|&l| l == Some("stand-1"))
                .unwrap();
            assert!(run.grounded[s], "{walk}");
            assert!(
                run.shown[s..]
                    .iter()
                    .all(|l| !l.unwrap().contains("teeter")),
                "{walk}"
            );
        }
    }

    #[test]
    fn standing_more_than_twelve_columns_from_the_end_does_not_teeter() {
        let level = platform_end();
        for slot in [0, 1] {
            // Column 119 is the last: 107 is the 13th from it.
            let run = stand_after_walking(&level, slot, 70.0, 3);
            let s = first_stand(&run, slot);
            assert!(
                run.shown[s..]
                    .iter()
                    .all(|l| !l.unwrap().contains("teeter"))
            );
        }
    }

    #[test]
    fn standing_at_a_ledge_shows_the_same_pictures_as_on_flat_floor() {
        for slot in [0, 1] {
            standing_at_a_ledge_matches_flat_floor(slot);
        }
    }

    fn standing_at_a_ledge_matches_flat_floor(slot: usize) {
        let ledge = FLAT.replace(
            "rect = [0, 400, 2048, 80]",
            "rect = [0, 400, 120, 80]\n[[collision]]\nshape = \"rect\"\nrect = [120, 440, 1928, 40]",
        );
        // More than 12 columns from the end it faces: no teetering.
        for (x, walk) in [(60.0, 0), (80.0, 0), (60.0, 6), (70.0, 3)] {
            let on_flat = stand_after_walking(FLAT, slot, x, walk);
            let at_ledge = stand_after_walking(&ledge, slot, x, walk);
            assert!(at_ledge.grounded.iter().all(|&g| g), "{slot} {x} {walk}");
            assert_eq!(at_ledge.shown, on_flat.shown, "{slot} {x} {walk}");
        }
    }

    #[test]
    fn standing_on_a_slope_shows_the_same_pictures_as_on_flat_floor() {
        for slot in [0, 1] {
            standing_on_a_slope_matches_flat_floor(slot);
        }
    }

    fn standing_on_a_slope_matches_flat_floor(slot: usize) {
        let dip = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/slope-dip.level"
        ))
        .unwrap();
        let dip = dip.replace("start = [64.26953125, 400]", "start = [64, 400]");
        for (x, walk) in [(480.0, 40), (500.0, 5), (520.0, 0), (570.0, 10)] {
            let on_flat = stand_after_walking(FLAT, slot, x, walk);
            let on_slope = stand_after_walking(&dip, slot, x, walk);
            assert_eq!(on_slope.shown, on_flat.shown, "{slot} {x} {walk}");
        }
    }

    /// A crouch ("Crouching") at the end of a platform, on the 1-in-4
    /// slope and at the top of a steep face shows the pictures of the same
    /// crouch on flat floor, with no teetering picture.
    #[test]
    fn crouching_at_a_ledge_and_on_a_slope_matches_flat_floor() {
        let ledge = FLAT.replace(
            "rect = [0, 400, 2048, 80]",
            "rect = [0, 400, 120, 80]\n[[collision]]\nshape = \"rect\"\nrect = [120, 440, 1928, 40]",
        );
        let dip = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/slope-dip.level"
        ))
        .unwrap();
        let dip = dip.replace("start = [64.26953125, 400]", "start = [64, 400]");
        let cases = [
            (&ledge, 100.0),
            (&ledge, 117.0),
            (&dip, 480.0),
            (&dip, 520.0),
        ];
        for slot in [0, 1] {
            for (level, x) in cases {
                let keys = crouch(&[], 200, 400);
                let on_flat = run(
                    world(
                        &FLAT.replace("start = [64, 400]", &format!("start = [{x}, 400]")),
                        slot,
                    ),
                    &keys,
                );
                let src = level.replace("start = [64, 400]", &format!("start = [{x}, 400]"));
                let there = run(world(&src, slot), &keys);
                // Standing again within 12 columns of a ledge's end
                // teeters, from 3 or 4 frames after the crouch.
                let teeter = there
                    .shown
                    .iter()
                    .position(|l| l.is_some_and(|l| l.contains("teeter")));
                assert_eq!(teeter.is_some(), x == 117.0, "{slot} {x}");
                let until = teeter.unwrap_or(on_flat.shown.len());
                assert_eq!(there.shown[..until], on_flat.shown[..until], "{slot} {x}");
                assert!(there.shown[30].is_some_and(|l| l.contains("crouch")));
            }
        }
    }

    #[test]
    fn standing_still_after_a_landing() {
        let run = run(world(FLAT, 0), &jump(10, 1, 1500));
        let k = run.frames(Event::JumpStart)[0] + 31 + 7;
        run.check(k, &standing_list(1500 - k));
    }

    #[test]
    fn up_while_standing_changes_nothing_and_a_tap_of_down_starts_over() {
        let still = run(world(FLAT, 0), &roll(&[], 900));
        let d = still.frames(Event::RollEnd)[0];
        let mut keys = roll(&[], 900);
        for f in [d + 40, d + 300, d + 301, d + 302] {
            keys[f] = vec![Action::Up];
        }
        assert_eq!(run(world(FLAT, 0), &keys).shown, still.shown);
        keys[d + 400] = vec![Action::Down];
        let tapped = run(world(FLAT, 0), &keys);
        // The shortest crouch, D to D + 12, its getting up, then S.
        let mut want = crouch_list(true, 13);
        want.extend(standing_list(900 - (d + 422)));
        tapped.check(d + 400, &want);
    }

    /// The second character's standing pictures from S, `frames` of
    /// them: the 275-frame loop over and over.
    fn second_standing_list(frames: usize) -> Vec<(&'static str, usize)> {
        let s = |n: usize| labels(1, Set::Standing)[n - 1];
        let w = |n: usize| labels(1, Set::Standing)[21 + n];
        let mut tour: Vec<_> = (1..=22).map(|n| (s(n), 4)).collect();
        tour.extend((1..=13).map(|n| (w(n), 5)));
        tour.extend([14, 15, 16, 17, 16, 15, 14, 15, 16].map(|n| (w(n), 2)));
        tour.extend([17, 16, 15, 14, 15].map(|n| (w(n), 3)));
        tour.extend([(16, 4), (17, 5), (16, 6), (15, 22)].map(|(n, c)| (w(n), c)));
        tour.extend((1..=13).rev().map(|n| (w(n), 4)));
        assert_eq!(tour.iter().map(|w| w.1).sum::<usize>(), 275);
        let mut left = frames;
        tour.into_iter()
            .cycle()
            .map_while(|(label, count)| {
                let n = count.min(left);
                left -= n;
                (n > 0).then_some((label, n))
            })
            .collect()
    }

    #[test]
    fn the_second_character_standing_still_after_a_roll_s_stop() {
        let run = run(world(FLAT, 1), &roll(&[], 1000));
        let d = run.frames(Event::RollEnd)[0];
        run.check(d + 17, &second_standing_list(1000 - d - 17));
    }

    #[test]
    fn the_second_character_standing_still_after_a_landing() {
        let run = run(world(FLAT, 1), &jump(10, 1, 700));
        let l = run.frames(Event::Land)[0];
        run.check(l + 17, &second_standing_list(700 - l - 17));
    }

    #[test]
    fn the_second_character_standing_after_walking_or_a_turn() {
        for key in [Action::Right, Action::Left] {
            let run = run(world(FLAT, 1), &hold(&[key], 6, 400));
            let s = run.shown[10..]
                .iter()
                .position(|&l| l == Some("s-stand-1"))
                .unwrap()
                + 10;
            assert!(s > 16, "{key:?}: standing from {s}");
            assert!(run.shown[..10].iter().all(Option::is_none));
            assert!(
                run.shown[10..s]
                    .iter()
                    .all(|l| SECOND_WALKING.contains(&l.unwrap())),
                "{key:?}"
            );
            run.check(s, &second_standing_list(400 - s));
        }
    }

    #[test]
    fn the_second_character_up_while_standing_changes_nothing_and_a_tap_of_down_starts_over() {
        let still = run(world(FLAT, 1), &roll(&[], 700));
        let d = still.frames(Event::RollEnd)[0];
        let mut keys = roll(&[], 700);
        for f in [d + 40, d + 300, d + 301, d + 302] {
            keys[f] = vec![Action::Up];
        }
        assert_eq!(run(world(FLAT, 1), &keys).shown, still.shown);
        keys[d + 400] = vec![Action::Down];
        let tapped = run(world(FLAT, 1), &keys);
        // The shortest crouch, D to D + 8, its getting up, then S.
        let mut want = crouch_list(false, 9);
        want.extend(second_standing_list(700 - (d + 421)));
        tapped.check(d + 400, &want);
    }

    /// A crouch's pictures ("Crouching") from D, for a crouch of `frames`
    /// frames (D to U − 1), then its getting up to U + 8 or U + 11.
    fn crouch_list(first: bool, frames: usize) -> Vec<(&'static str, usize)> {
        let c = |n: usize| labels(usize::from(!first), Set::Crouch)[n - 1];
        let three = |ns: Vec<usize>| ns.into_iter().map(move |n| (c(n), 3));
        let (down, held, before, between, sway, up) = if first {
            let mut sway: Vec<(&str, usize)> = three(vec![10, 11, 12]).collect();
            sway.push((c(13), 17));
            sway.extend(three(vec![14, 15]));
            sway.push((c(16), 17));
            sway.extend(three((17..=21).collect()));
            sway.push((c(22), 17));
            sway.extend(three((17..=21).rev().collect()));
            sway.push((c(16), 17));
            sway.extend(three((10..=15).rev().collect()));
            let down = [1, 2, 3, 4, 5, 7, 8].map(|n| (c(n), if n == 1 { 1 } else { 2 }));
            let up = (1..=9).rev().map(|n| (c(n), 1)).collect::<Vec<_>>();
            (down.to_vec(), 9, 17, 144, sway, up)
        } else {
            let mut sway = vec![(c(7), 5), (c(8), 5), (c(9), 17)];
            sway.extend(three(vec![10, 11, 12]));
            sway.push((c(13), 17));
            sway.extend(three(vec![12, 11, 10]));
            sway.push((c(9), 17));
            sway.extend(three(vec![8, 7]));
            let down = [1, 2, 3, 4, 5].map(|n| (c(n), if n == 1 { 1 } else { 2 }));
            let mut up = vec![(c(6), 2)];
            up.extend((1..=5).rev().map(|n| (c(n), 2)));
            (down.to_vec(), 6, 15, 142, sway, up)
        };
        assert_eq!(
            sway.iter().map(|w| w.1).sum::<usize>(),
            [85, 131][usize::from(first)]
        );
        let mut want = down;
        want.push((c(held), before));
        while want.iter().map(|w| w.1).sum::<usize>() < frames {
            want.extend(sway.iter().copied());
            want.push((c(held), between));
        }
        let mut left = frames;
        let mut want: Vec<_> = want
            .into_iter()
            .map_while(|(label, count)| {
                let n = count.min(left);
                left -= n;
                (n > 0).then_some((label, n))
            })
            .collect();
        want.extend(up);
        want
    }

    /// Nothing held, then Down held on frames 20 to `last` with `with`.
    fn crouch(with: &[Action], last: usize, total: usize) -> Vec<Vec<Action>> {
        (0..total)
            .map(|f| {
                let mut keys = with.to_vec();
                if (20..=last).contains(&f) {
                    keys.push(Action::Down);
                }
                keys
            })
            .collect()
    }

    /// A jump pressed on a crouch's frames, or on those of getting up
    /// ("A jump and its landing"), shows its first picture on the press
    /// frame in place of the crouch's or getting up's own; the frames
    /// before keep the crouch lists.
    #[test]
    fn a_jump_out_of_a_crouch_shows_its_first_picture_on_the_press() {
        for (slot, label, last) in [(0, "jump-1", 9), (1, "s-jump-1", 12)] {
            // Down held on frames 20 to 39, so U is 40.
            let base = run(world(FLAT, slot), &crouch(&[], 39, 200));
            for p in (20..=39).chain(40..=40 + last) {
                let mut keys = crouch(&[], 39, 200);
                keys[p].push(Action::Jump);
                let there = run(world(FLAT, slot), &keys);
                assert_eq!(there.shown[p], Some(label), "{slot} P = {p}");
                assert_eq!(there.shown[..p], base.shown[..p], "{slot} P = {p}");
                assert_eq!(there.frames(Event::JumpStart), [p + 1], "{slot} P = {p}");
            }
        }
    }

    /// Jump with Action pressed, then Down on the next frame and no Jump
    /// on P + 2: no jump, `jump-1` on P and the new crouch's list from P + 1.
    #[test]
    fn a_jump_with_action_then_down_shows_the_press_and_a_new_crouch() {
        for (slot, label) in [(0, "jump-1"), (1, "s-jump-1")] {
            let mut keys = vec![vec![]; 120];
            keys[30] = vec![Action::Jump, Action::Action];
            for k in &mut keys[31..90] {
                k.push(Action::Down);
            }
            keys[31].push(Action::Jump);
            let there = run(world(FLAT, slot), &keys);
            assert!(there.frames(Event::JumpStart).is_empty(), "{slot}");
            assert_eq!(there.shown[30], Some(label), "{slot}");
            let own = run(world(FLAT, slot), &crouch(&[], 89, 120));
            let c = labels(slot, Set::Crouch)[0];
            assert_eq!(own.shown[20], Some(c), "{slot}");
            assert_eq!(there.shown[31], Some(c), "{slot}");
            assert_eq!(there.shown[31..90], own.shown[20..79], "{slot}");
        }
    }

    #[test]
    fn the_first_character_crouching_a_long_while() {
        // player-frames.md's example: standing, Down held on frames 20 to
        // 719.
        let run = run(world(FLAT, 0), &crouch(&[], 719, 1200));
        assert_eq!(run.shown[33], Some("crouch-9"));
        assert_eq!(run.shown[50], Some("crouch-10"));
        assert_eq!(run.shown[181], Some("crouch-9"));
        assert_eq!(run.shown[325], Some("crouch-10"));
        assert_eq!(run.shown[719], Some("crouch-13"));
        assert_eq!(run.shown[720], Some("crouch-9"));
        assert_eq!(run.shown[728], Some("crouch-1"));
        let mut want = crouch_list(true, 700);
        want.extend(standing_list(1200 - 729));
        run.check(20, &want);
    }

    #[test]
    fn the_second_character_crouching_a_long_while() {
        // player-frames.md's example: standing, Down held on frames 20 to
        // 519.
        let run = run(world(FLAT, 1), &crouch(&[], 519, 900));
        assert_eq!(run.shown[29], Some("s-crouch-6"));
        assert_eq!(run.shown[44], Some("s-crouch-7"));
        assert_eq!(run.shown[129], Some("s-crouch-6"));
        assert_eq!(run.shown[271], Some("s-crouch-7"));
        assert_eq!(run.shown[498], Some("s-crouch-7"));
        assert_eq!(run.shown[519], Some("s-crouch-9"));
        assert_eq!(run.shown[521], Some("s-crouch-6"));
        assert_eq!(run.shown[531], Some("s-crouch-1"));
        let mut want = crouch_list(false, 500);
        want.extend(second_standing_list(900 - 532));
        run.check(20, &want);
    }

    #[test]
    fn the_second_character_s_short_press() {
        // player-frames.md's example: Down held on frame 20 only.
        let run = run(world(FLAT, 1), &crouch(&[], 20, 200));
        assert_eq!(run.shown[28], Some("s-crouch-5"));
        assert_eq!(run.shown[29], Some("s-crouch-6"));
        assert_eq!(run.shown[40], Some("s-crouch-1"));
        assert_eq!(run.shown[41], Some("s-stand-1"));
        let mut want = crouch_list(false, 9);
        want.extend(second_standing_list(200 - 41));
        run.check(20, &want);
    }

    #[test]
    fn a_crouch_while_walking_then_walking_from_u_plus_10_or_13() {
        for slot in [0, 1] {
            let first = slot == 0;
            let run = run(world(FLAT, slot), &crouch(&[Action::Right], 59, 120));
            let u = 60;
            let mut want = crouch_list(first, 40);
            let s = u + if first { 9 } else { 12 };
            want.push((if first { "stand-1" } else { "s-stand-1" }, 1));
            run.check(20, &want);
            let walking = labels(slot, Set::Walking);
            assert!(walking.contains(&run.shown[s + 1].unwrap()), "{slot}");
        }
    }

    #[test]
    fn down_again_while_getting_up_starts_the_list_over() {
        for (slot, ups) in [(0, 1..=9), (1, 1..=12)] {
            let first = slot == 0;
            for k in ups {
                let mut keys = crouch(&[], 39, 120);
                keys[40 + k] = vec![Action::Down];
                let run = run(world(FLAT, slot), &keys);
                let mut want = crouch_list(first, 20);
                want.truncate(want.len() - if first { 9 } else { 6 });
                let mut up = crouch_list(first, 20).split_off(want.len());
                let mut left = k;
                up.retain_mut(|(_, n)| {
                    let take = (*n).min(left);
                    left -= take;
                    *n = take;
                    take > 0
                });
                want.extend(up);
                want.extend(crouch_list(first, if first { 13 } else { 9 }));
                run.check(20, &want);
            }
        }
    }

    /// Nothing held, then Down and Action on frame 20 only.
    fn action_press(total: usize) -> Vec<Vec<Action>> {
        (0..total)
            .map(|f| {
                if f == 20 {
                    vec![Action::Down, Action::Action]
                } else {
                    vec![]
                }
            })
            .collect()
    }

    #[test]
    fn the_first_character_s_ground_action() {
        // player-frames.md, "The ground action": act-1 on 20, act-2 on
        // 21 and 22, ..., act-30 on 77 and 78; 79 = E shows stand-1.
        let run = run(world(FLAT, 0), &action_press(100));
        assert_eq!(run.shown[20], Some("act-1"));
        for n in 2..=30 {
            let label = labels(0, Set::Act)[n - 1];
            assert_eq!(run.shown[2 * n + 17], Some(label), "act-{n}");
            assert_eq!(run.shown[2 * n + 18], Some(label), "act-{n}");
        }
        assert_eq!(run.shown[79], Some("stand-1"));
    }

    #[test]
    fn the_ground_action_s_pictures_go_on_past_a_platform_s_end() {
        // player-frames.md, "The ground action": past a platform's end
        // the action goes on in the air with the list unchanged.
        const LEDGE: &str = r#"
            format = 1
            size = [2048, 1200]
            start = [64, 400]

            [[collision]]
            shape = "rect"
            rect = [0, 400, 72, 80]
        "#;
        let keys: Vec<Vec<Action>> = (0..100)
            .map(|f| match f {
                20 => vec![Action::Down, Action::Action, Action::Right],
                21..=80 => vec![Action::Right],
                _ => vec![],
            })
            .collect();
        let run = run(world(LEDGE, 0), &keys);
        assert!(run.grounded[20..79].iter().any(|g| !g), "left the floor");
        for n in 2..=30 {
            let label = labels(0, Set::Act)[n - 1];
            assert_eq!(run.shown[2 * n + 17], Some(label), "act-{n}");
            assert_eq!(run.shown[2 * n + 18], Some(label), "act-{n}");
        }
    }

    #[test]
    fn a_jump_press_ends_the_ground_action() {
        let mut keys = action_press(120);
        keys[30] = vec![Action::Jump];
        let run = run(world(FLAT, 0), &keys);
        assert_eq!(run.shown[29], Some("act-6"));
        assert_eq!(run.shown[30], Some("jump-1"));
    }

    #[test]
    fn the_second_character_s_ground_action_keeps_the_engine_s_art() {
        let run = run(world(FLAT, 1), &action_press(100));
        assert!(
            run.shown[20..79]
                .iter()
                .all(|l| !l.is_some_and(|l| l.starts_with("act-")))
        );
    }

    /// A rope at 500 ending at 110 beside a platform at 160, with a floor
    /// far below, and the first character standing under it.
    const ROPE_LEVEL: &str = r#"
        format = 1
        size = [2000, 800]
        start = [490, 160]

        [[collision]]
        shape = "rect"
        rect = [0, 160, 495, 20]

        [[collision]]
        shape = "rect"
        rect = [0, 700, 2000, 100]

        [[entities]]
        type = "rope"
        at = [500, 110]
    "#;

    /// Keys holding Jump up to and on the catch frame, then `after(k)`
    /// for each frame k after it; and the catch frame.
    fn rope_keys(after: impl Fn(usize) -> Vec<Action>, total: usize) -> (Vec<Vec<Action>>, usize) {
        let mut probe = world(ROPE_LEVEL, 0);
        let mut catch = None;
        let mut held = Actions::NONE;
        for k in 0..200 {
            let now: Actions = [Action::Jump].into_iter().collect();
            step(&mut probe, &InputFrame::single(Frame::after(held, now)));
            held = now;
            if probe.events.contains(&Event::RopeCatch) {
                catch = Some(k);
                break;
            }
        }
        let c = catch.expect("caught");
        let keys = (0..total)
            .map(|k| {
                if k <= c {
                    vec![Action::Jump]
                } else {
                    after(k - c)
                }
            })
            .collect();
        (keys, c)
    }

    #[test]
    fn the_first_character_climbs_a_rope_by_the_specs_example() {
        // player-frames.md, "On a rope": a catch on C, Up on C + 1 to C + 45.
        let (keys, c) = rope_keys(|k| if k <= 45 { vec![Action::Up] } else { vec![] }, 80);
        let run = run(world(ROPE_LEVEL, 0), &keys);
        assert_eq!(run.shown[c], Some("rope-hang-1"));
        let expect = [
            ("rope-climb-1", 1, 5),
            ("rope-climb-2", 6, 8),
            ("rope-climb-3", 9, 10),
            ("rope-climb-4", 11, 12),
            ("rope-climb-5", 13, 14),
            ("rope-climb-6", 15, 16),
            ("rope-climb-5", 17, 18),
            ("rope-climb-4", 19, 20),
            ("rope-climb-3", 21, 22),
            ("rope-climb-2", 23, 24),
            ("rope-climb-1", 25, 25),
        ];
        for (label, from, to) in expect {
            for k in from..=to {
                assert_eq!(run.shown[c + k], Some(label), "C + {k}");
            }
        }
    }

    #[test]
    fn turning_round_a_rope_shows_the_turn_pictures() {
        // Holding the way faced on C + 1 starts a turn there (T).
        let (keys, c) = rope_keys(|k| if k == 1 { vec![Action::Right] } else { vec![] }, 30);
        let run = run(world(ROPE_LEVEL, 0), &keys);
        let t = c + 1;
        assert_eq!(run.shown[c], Some("rope-hang-1"));
        let expect = [1, 1, 2, 2, 2, 2, 1, 1];
        for (k, n) in expect.into_iter().enumerate() {
            assert_eq!(run.shown[t + k], Some(ROPE[3 + n]), "T + {k}");
        }
        assert_eq!(run.shown[t + 8], Some("rope-hang-1"));
    }

    #[test]
    fn hanging_cycles_nine_frames_a_picture() {
        let (keys, c) = rope_keys(|_| vec![], 90);
        let run = run(world(ROPE_LEVEL, 0), &keys);
        let cycle = [1, 2, 3, 4, 3, 2, 1, 2];
        for (n, picture) in cycle.into_iter().enumerate() {
            for k in 0..9 {
                assert_eq!(run.shown[c + 9 * n + k], Some(HANG[picture - 1]), "{n} {k}");
            }
        }
    }

    #[test]
    fn jumping_off_a_rope_shows_the_air_pictures_in_order() {
        // Jump released on C + 1, pressed with Right on C + 2 = P.
        let (keys, c) = rope_keys(
            |k| match k {
                1 => vec![],
                _ => vec![Action::Jump, Action::Right],
            },
            30,
        );
        let run = run(world(ROPE_LEVEL, 0), &keys);
        let p = c + 2;
        assert_eq!(run.shown[p - 1], Some("rope-hang-1"));
        let expect = [
            "land-1", "air-6", "air-6", "air-5", "air-5", "air-4", "air-4", "air-3",
        ];
        for (k, label) in expect.into_iter().enumerate() {
            assert_eq!(run.shown[p + k], Some(label), "P + {k}");
        }
    }

    #[test]
    fn sliding_off_the_end_of_a_rope() {
        let (keys, c) = rope_keys(|_| vec![Action::Down], 40);
        let run = run(world(ROPE_LEVEL, 0), &keys);
        let l = (c..keys.len())
            .find(|&k| run.events[k].contains(&Event::RopeSlideOff))
            .expect("slid off");
        let f = l + 1;
        assert_eq!(run.shown[f], Some("land-2"));
        for (n, label) in [
            "land-1", "air-6", "air-5", "air-4", "air-3", "air-4", "air-5",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(run.shown[f + 1 + 2 * n], Some(label), "F + {}", 1 + 2 * n);
            assert_eq!(run.shown[f + 2 + 2 * n], Some(label), "F + {}", 2 + 2 * n);
        }
    }

    /// [`ROPE_LEVEL`] with the floor close below the rope.
    fn low_rope_level(floor: u32) -> String {
        ROPE_LEVEL.replace("700, 2000", &format!("{floor}, 2000"))
    }

    #[test]
    fn landing_after_jumping_off_a_rope_with_nothing_held() {
        // player-frames.md, "Landing after a rope": land-1, land-2 and
        // land-3 three times each from L, then standing from L + 9.
        let (keys, c) = rope_keys(
            |k| match k {
                2 => vec![Action::Jump, Action::Right],
                _ => vec![],
            },
            120,
        );
        let r = run(world(&low_rope_level(330), 0), &keys);
        let l = r.frames(Event::Land)[0];
        assert!(l > c + 25, "a jump of 25 frames or more");
        r.check(
            l,
            &[
                ("land-1", 3),
                ("land-2", 3),
                ("land-3", 3),
                ("stand-1", 3),
                ("stand-2", 3),
            ],
        );
    }

    #[test]
    fn landing_after_sliding_off_a_rope_shows_the_30_frame_list() {
        let (keys, c) = rope_keys(|k| if k <= 11 { vec![Action::Down] } else { vec![] }, 120);
        let r = run(world(&low_rope_level(190), 0), &keys);
        let l = r.frames(Event::Land)[0];
        let f = (c..keys.len())
            .find(|&k| r.events[k].contains(&Event::RopeSlideOff))
            .expect("slid off")
            + 1;
        assert!((f + 4..=f + 13).contains(&l), "F = {f}, L = {l}");
        let mut want = vec![("land-2", 1), ("land-1", 2)];
        for label in ["air-6", "air-5", "air-4", "air-3", "air-4"] {
            want.push((label, 2));
        }
        want.extend([
            ("air-5", 3),
            ("air-6", 4),
            ("land-1", 4),
            ("land-2", 3),
            ("land-3", 3),
            ("stand-1", 3),
            ("stand-2", 3),
        ]);
        r.check(f, &want);
    }

    /// All water, the first character in it.
    const POOL: &str = r#"
        format = 1
        size = [4096, 2000]
        start = [1000, 1000]

        [[regions]]
        rect = [0, 0, 4096, 2000]
        water = true
    "#;

    #[test]
    fn the_first_character_swims_by_the_specs_example() {
        // player-frames.md, "Swimming": Jump pressed on frame 2.
        let keys: Vec<Vec<Action>> = (0..13)
            .map(|k| if k == 2 { vec![Action::Jump] } else { vec![] })
            .collect();
        let run = run(world(POOL, 0), &keys);
        let expect = [1, 1, 1, 2, 3, 4, 4, 5, 6, 6, 7, 7, 8];
        for (k, n) in expect.iter().enumerate() {
            assert_eq!(run.shown[k], Some(SWIM[n - 1]), "frame {k}");
        }
    }

    #[test]
    fn the_second_character_keeps_the_engine_s_art_swimming() {
        let run = run(world(POOL, 1), &vec![vec![]; 10]);
        assert!(
            run.shown
                .iter()
                .all(|l| !l.is_some_and(|l| l.starts_with("swim-")))
        );
    }

    #[test]
    fn the_second_character_keeps_the_engine_s_art_on_a_rope() {
        let (keys, c) = rope_keys(|_| vec![], 20);
        let run = run(world(ROPE_LEVEL, 1), &keys);
        assert!(
            run.shown[c..]
                .iter()
                .all(|l| !l.is_some_and(|l| l.starts_with("rope-")))
        );
    }

    #[test]
    fn the_sets_are_the_importer_s() {
        for (slot, sets) in SETS.iter().enumerate() {
            let decoder = clamber_import::CharacterDecoder::of("title-1").unwrap();
            let character = decoder.characters.iter().find(|c| c.slot == slot).unwrap();
            for (set, labels) in sets.iter() {
                let name = match set {
                    Set::Stop => "stop",
                    Set::Standing => "standing",
                    Set::Jump => "jump",
                    Set::Bounce => "bounce",
                    Set::Roll => "roll",
                    Set::Walking => "walking",
                    Set::Crouch => "crouch",
                    Set::Carry => "carry",
                    Set::Hurt => "hurt",
                    Set::Act => "act",
                    Set::Rope => "rope",
                    Set::Swim => "swim",
                    Set::Scene => "scene",
                    Set::Map => "map",
                    Set::MapLeadFinish => "map-lead-finish",
                    Set::MapOtherFinish => "map-other-finish",
                    Set::MapStart => "map-start",
                    Set::Teeter => "teeter",
                };
                let theirs = character.sets.iter().find(|s| s.name == name).unwrap();
                let theirs: Vec<&str> = theirs.pictures.iter().map(|&(l, _)| l).collect();
                assert_eq!(&theirs, labels, "{set:?}");
            }
        }
        assert_eq!(crate::cast::CHARACTER, clamber_import::CHARACTER_SHEET);
        assert_eq!(MAP_SHEET, clamber_import::MAP_SHEET);
    }

    /// Flat floor with a crate 20 px in front of the player, within
    /// reach.
    const CRATE: &str = r#"
        format = 1
        size = [2048, 480]
        start = [64, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 2048, 80]

        [[entities]]
        type = "crate"
        at = [84, 400]
    "#;

    /// 3 idle frames, then `A` and `then` held for `carry` frames
    /// (the pick-up starts on the first), then `after` for the rest up to
    /// `total`.
    fn carrying(then: &[Action], carry: usize, after: &[Action], total: usize) -> Vec<Vec<Action>> {
        (0..total)
            .map(|f| match f {
                0..=2 => vec![],
                f if f < 3 + carry => [&[Action::Action][..], then].concat(),
                _ => after.to_vec(),
            })
            .collect()
    }

    fn at(run: &Run, label: &str) -> usize {
        run.shown
            .iter()
            .position(|s| *s == Some(label))
            .unwrap_or_else(|| panic!("{label} never shown"))
    }

    #[test]
    fn picking_up_standing_with_the_object_and_throwing() {
        // player-frames.md, "Carrying", "First character".
        let run = run(world(CRATE, 0), &carrying(&[], 57, &[], 160));
        let g = at(&run, "pick-1");
        assert_eq!(g, 3);
        let mut want = vec![("pick-1", 1)];
        for n in 2..=6 {
            want.push((["pick-2", "pick-3", "pick-4", "pick-5", "pick-6"][n - 2], 2));
        }
        want.push(("pick-7", 4));
        // From G + 15: standing with it, S on G + 15.
        want.extend([("hold-1", 12), ("hold-2", 9), ("hold-3", 12), ("hold-2", 9)]);
        run.check(g, &want);
        // Action let go on Q = G + 57: a throw of 43 frames, then
        // standing.
        let q = at(&run, "throw-1");
        assert_eq!(q, g + 57);
        let mut want = vec![("throw-1", 1)];
        for n in 2..=18 {
            want.push((CARRYING[THROW_1 + n - 1], 2));
        }
        want.push(("throw-19", 8));
        want.push(("stand-1", 1));
        run.check(q, &want);
    }

    #[test]
    fn a_pick_up_let_go_of_is_a_throw_from_q() {
        let run = run(world(CRATE, 0), &carrying(&[], 5, &[], 80));
        let q = at(&run, "throw-1");
        assert_eq!(q, 3 + 5);
        run.check(q, &[("throw-1", 1), ("throw-2", 2)]);
        assert_eq!(run.shown[q + 43], Some("stand-1"));
    }

    #[test]
    fn a_throw_ends_in_walking() {
        let run = run(world(CRATE, 0), &carrying(&[], 30, &[Action::Right], 100));
        let q = at(&run, "throw-1");
        run.check(q + 42, &[("throw-19", 1)]);
        assert_eq!(run.shown[q + 43], Some("walk-1"));
    }

    #[test]
    fn putting_down_is_the_pick_up_in_reverse() {
        // Down held on Q, then Right held throughout.
        let keys: Vec<Vec<Action>> = (0..100)
            .map(|f| match f {
                0..=2 => vec![],
                3..=32 => vec![Action::Action],
                33 => vec![Action::Down],
                _ => vec![Action::Right],
            })
            .collect();
        let run = run(world(CRATE, 0), &keys);
        let q = 33;
        let mut want = vec![("pick-7", 1)];
        for n in (1..=6).rev() {
            want.push((CARRYING[n - 1], 2));
        }
        run.check(q, &want);
        // Q + 13 is a standing frame whatever is held; walking starts on
        // the next.
        assert_eq!(run.shown[q + 13], Some("stand-1"));
        assert_eq!(run.shown[q + 14], Some("walk-1"));
    }

    #[test]
    fn moving_with_the_object_goes_through_the_carrying_stride() {
        // player-frames.md's example: standing with the object, Right
        // held from frame 0.
        let keys = carrying(&[], 37, &[Action::Action], 40)
            .into_iter()
            .chain((0..40).map(|_| vec![Action::Action, Action::Right]))
            .collect::<Vec<_>>();
        let run = run(world(CRATE, 0), &keys);
        let s = at(&run, "carry-1");
        assert_eq!(s, 40);
        run.check(
            s,
            &[
                ("carry-1", 5),
                ("carry-2", 3),
                ("carry-3", 2),
                ("carry-4", 2),
                ("carry-5", 1),
            ],
        );
    }

    #[test]
    fn a_turn_with_the_object_shows_one_picture_and_starts_at_once() {
        let keys = carrying(&[Action::Right], 40, &[], 40)
            .into_iter()
            .chain((0..30).map(|_| vec![Action::Action, Action::Left]))
            .collect::<Vec<_>>();
        let run = run(world(CRATE, 0), &keys);
        // Moving by then, on whatever step; the turn starts on the
        // frame Left is held.
        let t = at(&run, "carry-turn");
        assert_eq!(t, 40);
        run.check(t, &[("carry-turn", 8)]);
        assert_ne!(run.shown[t + 8], Some("carry-turn"));
    }

    #[test]
    fn a_jump_with_the_object_shows_carry_1_from_two_frames_before_take_off() {
        let keys: Vec<Vec<Action>> = (0..120)
            .map(|f| match f {
                0..=2 => vec![],
                3..=40 => vec![Action::Action],
                41..=45 => vec![Action::Action, Action::Jump],
                _ => vec![Action::Action],
            })
            .collect();
        let run = run(world(CRATE, 0), &keys);
        let t = run.frames(Event::JumpStart)[0];
        assert_eq!(t, 43);
        let l = run.frames(Event::Land)[0];
        run.check(t - 2, &[("carry-1", l + 2 - (t - 2))]);
        assert_ne!(run.shown[t - 3], Some("carry-1"));
        // L + 2 is standing again.
        assert_eq!(run.shown[l + 2], Some("hold-1"));
    }

    #[test]
    fn the_second_character_picks_up_stands_with_the_object_and_throws() {
        // player-frames.md, "Carrying", "Second character".
        let run = run(world(CRATE, 1), &carrying(&[], 49, &[], 140));
        let g = at(&run, "s-pick-1");
        assert_eq!(g, 3);
        run.check(
            g,
            &[
                ("s-pick-1", 2),
                ("s-pick-2", 2),
                ("s-pick-3", 2),
                ("s-pick-4", 2),
                ("s-pick-5", 2),
                ("s-hold", 39),
            ],
        );
        // Action let go on Q = G + 49: a throw of 40 frames, then standing.
        let q = at(&run, "s-throw-1");
        assert_eq!(q, g + 49);
        let mut want = vec![("s-throw-1", 1)];
        for (n, frames) in S_THROW_FRAMES.iter().enumerate().skip(1) {
            want.push((SECOND_CARRYING[S_THROW_1 + n], usize::from(*frames)));
        }
        want.push(("s-stand-1", 1));
        run.check(q, &want);
    }

    #[test]
    fn the_second_character_s_throw_ends_in_walking_or_a_turn() {
        let run = run(world(CRATE, 1), &carrying(&[], 30, &[Action::Right], 100));
        let q = at(&run, "s-throw-1");
        assert_eq!(run.shown[q + 39], Some("s-throw-14"));
        assert_eq!(run.shown[q + 40], Some("s-walk-1"));
    }

    #[test]
    fn the_second_character_s_pick_up_let_go_of_is_a_throw_from_q() {
        let run = run(world(CRATE, 1), &carrying(&[], 5, &[], 80));
        let q = at(&run, "s-throw-1");
        assert_eq!(q, 3 + 5);
        run.check(q, &[("s-throw-1", 1), ("s-throw-2", 2)]);
        assert_eq!(run.shown[q + 40], Some("s-stand-1"));
    }

    #[test]
    fn the_second_character_puts_down_in_reverse_and_stands_on_q_plus_10() {
        let keys: Vec<Vec<Action>> = (0..100)
            .map(|f| match f {
                0..=2 => vec![],
                3..=32 => vec![Action::Action],
                33 => vec![Action::Down],
                _ => vec![Action::Right],
            })
            .collect();
        let run = run(world(CRATE, 1), &keys);
        let q = 33;
        run.check(
            q,
            &[
                ("s-pick-5", 2),
                ("s-pick-4", 2),
                ("s-pick-3", 2),
                ("s-pick-2", 2),
                ("s-pick-1", 2),
            ],
        );
        // Q + 10 is S whatever is held; walking starts on the next.
        assert_eq!(run.shown[q + 10], Some("s-stand-1"));
        assert_eq!(run.shown[q + 11], Some("s-walk-1"));
    }

    #[test]
    fn the_second_character_moves_with_the_object_in_steps_of_three_frames() {
        // player-frames.md's example: Right held on frames 0 to 29, then
        // nothing; step 2 ends on 54 with v = 34, so it stands there.
        let keys = carrying(&[], 30, &[Action::Action], 20)
            .into_iter()
            .chain((0..30).map(|_| vec![Action::Action, Action::Right]))
            .chain((0..40).map(|_| vec![Action::Action]))
            .collect::<Vec<_>>();
        let run = run(world(CRATE, 1), &keys);
        let s = at(&run, "s-carry-1");
        assert_eq!(s, 20);
        let mut want = Vec::new();
        for n in 1..=8 {
            want.push((SECOND_CARRYING[S_CARRY_1 + n - 1], 3));
        }
        want.extend([("s-carry-1", 3), ("s-carry-2", 3)]);
        for n in 3..=7 {
            want.push((SECOND_CARRYING[S_CARRY_1 + n - 1], 3));
        }
        want.extend([("s-carry-8", 3), ("s-carry-1", 3), ("s-carry-2", 3)]);
        run.check(s, &want);
        assert_eq!(run.shown[s + 54], Some("s-hold"));
    }

    #[test]
    fn a_turn_with_the_object_shows_s_hold_and_a_jump_s_carry_2() {
        let keys = carrying(&[Action::Right], 40, &[], 40)
            .into_iter()
            .chain((0..30).map(|_| vec![Action::Action, Action::Left]))
            .collect::<Vec<_>>();
        let turned = run(world(CRATE, 1), &keys);
        turned.check(40, &[("s-hold", 8)]);
        let keys: Vec<Vec<Action>> = (0..120)
            .map(|f| match f {
                0..=2 => vec![],
                3..=40 => vec![Action::Action],
                41..=45 => vec![Action::Action, Action::Jump],
                _ => vec![Action::Action],
            })
            .collect();
        let jumped = run(world(CRATE, 1), &keys);
        let t = jumped.frames(Event::JumpStart)[0];
        let l = jumped.frames(Event::Land)[0];
        jumped.check(t - 2, &[("s-carry-2", l + 2 - (t - 2))]);
        assert_eq!(jumped.shown[l + 2], Some("s-hold"));
    }
}

#[cfg(test)]
mod stand_in_tests {
    use super::*;

    #[test]
    fn nearest_picture_is_of_the_same_character() {
        let only = |names: &'static [&'static str]| move |l: &str| names.contains(&l);
        let sheet = only(&["stand-1", "air-0", "air-1", "rise-1"]);
        assert_eq!(stand_in(0, State::Fall, sheet), Some("air-0"));
        assert_eq!(stand_in(0, State::Jump, sheet), Some("rise-1"));
        assert_eq!(stand_in(0, State::Walk, sheet), Some("stand-1"));
        assert_eq!(stand_in(0, State::Idle, sheet), Some("stand-1"));
        let second = only(&["s-air-1", "s-stand-1", "s-walk-1"]);
        assert_eq!(stand_in(1, State::Fall, second), Some("s-air-1"));
        assert_eq!(stand_in(1, State::Walk, second), Some("s-walk-1"));
        assert_eq!(stand_in(1, State::Roll, second), Some("s-stand-1"));
        assert_eq!(stand_in(0, State::Fall, only(&[])), None);
    }

    /// The labels of frames `from..=to` of the scene's lead.
    fn scene_run(slot: usize, last: u32, from: u32, to: u32) -> Vec<&'static str> {
        (from..=to)
            .map(|f| scene_lead(slot, f, last).unwrap().1)
            .collect()
    }

    /// Runs of the same label, with how long each is.
    fn runs(labels: Vec<&'static str>) -> Vec<(&'static str, usize)> {
        let mut out: Vec<(&str, usize)> = Vec::new();
        for l in labels {
            match out.last_mut() {
                Some((p, n)) if *p == l => *n += 1,
                _ => out.push((l, 1)),
            }
        }
        out
    }

    #[test]
    fn the_first_character_leads_the_scene_with_the_specs_pictures() {
        let at = |f| scene_lead(0, f, 396);
        assert_eq!(at(2), None);
        assert_eq!(at(3), Some((Set::Walking, "run-2")));
        assert_eq!(at(4), Some((Set::Walking, "run-3")));
        let walk = runs(scene_run(0, 396, 5, 27));
        assert_eq!(
            &walk[..4],
            &[("run-4", 2), ("run-5", 1), ("run-6", 2), ("run-7", 1)]
        );
        assert_eq!(&walk[walk.len() - 2..], &[("run-17", 1), ("run-18", 2)]);
        let wait = runs(scene_run(0, 396, 28, 99));
        let counts: Vec<_> = wait.iter().map(|&(_, n)| n).collect();
        assert_eq!(counts, [7, 6, 6, 21, 14, 9, 9]);
        assert_eq!(wait[6].0, "scene-7");
        assert_eq!(at(28), Some((Set::Scene, "scene-1")));
        // The 62-frame list repeats from frame 100 and is cut off at 394.
        let list = runs(scene_run(0, 396, 100, 161));
        assert_eq!(list.iter().map(|&(_, n)| n).sum::<usize>(), 62);
        assert_eq!(list[3], ("scene-11", 6));
        assert_eq!(list[8], ("scene-13", 8));
        assert_eq!(at(162), at(100));
        assert_eq!(at(348 + 46), Some((Set::Scene, "scene-12")));
        assert_eq!(at(394).unwrap().1, "scene-12");
        assert_eq!(at(395), None);
        assert_eq!(at(396), None);
    }

    #[test]
    fn the_second_character_leads_the_scene_with_the_specs_pictures() {
        let at = |f| scene_lead(1, f, 366);
        assert_eq!(at(0), None);
        assert_eq!(at(3), Some((Set::Walking, "s-run-2")));
        let walk = runs(scene_run(1, 366, 3, 27));
        assert_eq!(walk.iter().map(|&(_, n)| n).sum::<usize>(), 25);
        assert_eq!(at(8).unwrap().1, "s-run-5");
        assert_eq!(at(9).unwrap().1, "s-run-6");
        assert_eq!(at(15).unwrap().1, "s-run-9");
        assert_eq!(at(22).unwrap().1, "s-run-13");
        assert_eq!(at(24).unwrap().1, "s-run-1");
        assert_eq!(at(27).unwrap().1, "s-run-3");
        let wait = runs(scene_run(1, 366, 28, 129));
        let counts: Vec<_> = wait.iter().map(|&(_, n)| n).collect();
        assert_eq!(counts, [4, 19, 3, 3, 3, 3, 3, 3, 3, 3, 26, 3, 4, 22]);
        // Round k of 27 - k frames starts on frame 130 + the rounds before.
        let round = |k: u32| 130 + (0..k).map(|j| 27 - j).sum::<u32>();
        assert_eq!(round(8), 318);
        for k in 0..9 {
            let r = runs(scene_run(1, 366, round(k), round(k) + 26 - k));
            let counts: Vec<_> = r.iter().map(|&(_, n)| n as u32).collect();
            assert_eq!(counts, [3, 3, 5, 2, 5, 9 - k], "round {k}");
            assert_eq!(r[5].0, "s-scene-14");
        }
        let end = runs(scene_run(1, 366, 337, 362));
        assert_eq!(
            end,
            [("s-scene-15", 3), ("s-scene-16", 3), ("s-scene-17", 20)]
        );
        assert_eq!(at(365), None);
        assert_eq!(at(366), None);
    }

    #[test]
    fn the_other_character_is_shown_where_the_spec_read_it() {
        assert_eq!(scene_other(1, 26, 396), None);
        assert_eq!(scene_other(1, 27, 396), Some((Set::Walking, "s-run-1")));
        assert_eq!(scene_other(1, 28, 396), Some((Set::Standing, "s-stand-1")));
        assert_eq!(scene_other(1, 394, 396), Some((Set::Standing, "s-stand-1")));
        assert_eq!(scene_other(1, 395, 396), None);
        assert_eq!(scene_other(0, 14, 366), None);
        assert_eq!(scene_other(0, 15, 366), Some((Set::Walking, "run-16")));
        assert_eq!(scene_other(0, 19, 366), Some((Set::Walking, "run-20")));
        assert_eq!(scene_other(0, 20, 366), Some((Set::Walking, "run-1")));
        assert_eq!(scene_other(0, 28, 366), Some((Set::Walking, "run-9")));
        assert_eq!(scene_other(0, 31, 366), None);
        assert_eq!(scene_other(0, 32, 366), Some((Set::Standing, "stand-1")));
        assert_eq!(scene_other(0, 364, 366), Some((Set::Standing, "stand-1")));
        assert_eq!(scene_other(0, 365, 366), None);
    }
}
