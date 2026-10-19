//! Which of its own pictures the player shows when it's held still: the
//! stop after a roll, a jump and its landing, a bounce's frozen frames
//! and the first standing pictures after them, and the first
//! character's walking, running and turning pictures
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
//! - The pictures after the ones given (standing on after the first
//!   standing pictures, the second character walking or running, a
//!   bounce after B + 4) are the engine's own art, as are the second
//!   character's in the air from T + 16 until it lands.
//! - Walking or running in the air after walking off a ledge, and
//!   crouching or getting up, show the engine's own art.
//! - The second character's landing pictures are shown after a jump
//!   that lands on the floor it took off from, the landings the spec
//!   says they're for.
//! - Jumps that land before their approach frame, landings after falls
//!   and bounces, and anything while carrying, swapping, hurt or losing
//!   a life show the engine's own art.

use clamber_pack::Param;
use clamber_sim::{Event, Gait, Outcome, State, World};

/// The sets of pictures shown together, each shown only when all of it
/// is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    /// The stop after a roll, or carrying on into running.
    Stop,
    /// The first standing pictures.
    Standing,
    /// A jump and its landing.
    Jump,
    /// A bounce's frozen frames.
    Bounce,
    /// Walking, running and turning on the ground.
    Walking,
}

/// Each set's labels, for the first character (slot 0) and the second
/// (slot 1).
pub const SETS: [&[(Set, &[&str])]; 2] = [
    &[
        (Set::Stop, &["roll-end", "stop", "stand-1"]),
        (
            Set::Standing,
            &["stand-1", "stand-2", "stand-3", "stand-4", "stand-5"],
        ),
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
        (Set::Walking, &WALKING),
    ],
    &[
        (
            Set::Stop,
            &["s-stop-1", "s-stop-2", "s-stop-3", "s-stop-4", "s-stop-5"],
        ),
        (Set::Standing, &["s-stand-1"]),
        (
            Set::Jump,
            &[
                "s-jump-1", "s-jump-2", "s-rise-1", "s-rise-2", "s-rise-3", "s-rise-4", "s-rise-5",
                "s-rise-6", "s-rise-7", "s-air-1", "s-air-2", "s-air-3", "s-air-4", "s-air-5",
                "s-air-6", "s-land-1", "s-land-2", "s-land-3", "s-land-4",
            ],
        ),
        (Set::Bounce, &["s-bounce-1", "s-bounce-2"]),
    ],
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

/// The labels of `set` for the character in roster slot `slot`.
pub fn labels(slot: usize, set: Set) -> &'static [&'static str] {
    SETS.get(slot)
        .and_then(|sets| sets.iter().find(|(s, _)| *s == set))
        .map_or(&[], |&(_, labels)| labels)
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
    /// On the floor it took off from.
    same_floor: bool,
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
    /// Frames of a carried-on roll from D, 0 on D.
    carried: Option<u8>,
    /// Frames standing after a stop or a landing, 0 on the first.
    standing: Option<u8>,
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

    /// Moves on to `world` as it is after a logic frame. `jump_next`:
    /// a jump starts on the next frame, so this is its T − 1.
    pub fn update(&mut self, world: &World, jump_next: bool) {
        let player = &world.player;
        if player.character != self.slot {
            *self = PictureClock {
                slot: player.character,
                ..PictureClock::default()
            };
        }
        let before = std::mem::take(&mut self.shown);
        let events = &world.events;
        let physics = world.player_physics();
        let first = self.slot == 0;

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
        if events.contains(&Event::Land) {
            self.landed = landing_age.and_then(|age| {
                let land = age.saturating_add(1);
                let approach = player.approach?;
                let stages = stages(approach, &physics);
                let n = stages.iter().filter(|&&o| approach + o <= land).count() as u8;
                let y = feet(player.pos.y);
                Some(Landed {
                    n,
                    same_floor: self.takeoff_y == Some(y),
                })
            });
        } else if !player.grounded() || events.contains(&Event::JumpStart) {
            self.landed = None;
        }
        self.carried = match player.roll {
            Some(roll) if roll.carry => Some(self.carried.map_or(0, |c| c.saturating_add(1))),
            _ => None,
        };

        let playing = matches!(world.flow.outcome, Outcome::Playing)
            && player.hands.is_none()
            && !matches!(
                player.state,
                State::Swap | State::Hurt | State::GroundAction | State::Swim | State::Hold
            );
        if !playing {
            self.standing = None;
            self.landed = None;
            return;
        }

        self.shown = if jump_next {
            Some((Set::Jump, if first { "jump-1" } else { "s-jump-1" }))
        } else if let Some((n, _)) = player.since_bounce.filter(|_| !player.grounded()) {
            bounce(first, n)
        } else if player.roll_stop > 0 && player.grounded() && player.roll.is_none() {
            stop(first, player.roll_stop - 1)
        } else if let Some(c) = self.carried {
            carried(first, c)
        } else if let Some(age) = jump_age {
            in_air(first, age, player.approach, &physics)
        } else if let Some(landed) = self.landed {
            if first {
                first_landing(landed, player.gait)
            } else {
                second_landing(landed, player.since_land, player.gait, player.state)
            }
        } else if first && player.grounded() {
            walking(player)
        } else {
            None
        };
        // Standing starts from a stop's last picture, or a landing's.
        let standing_now = self.shown.is_none()
            && player.state == State::Idle
            && player.grounded()
            && player.roll_stop == 0
            && matches!(player.gait, Gait::Standing);
        self.standing = match self.standing {
            _ if !standing_now => None,
            Some(c) => Some(c.saturating_add(1)),
            None => before
                .is_some_and(|(set, _)| matches!(set, Set::Stop | Set::Jump))
                .then_some(0),
        };
        if let Some(c) = self.standing {
            self.shown = standing(first, c);
        }
    }
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

/// On B + `n`.
fn bounce(first: bool, n: u8) -> Option<(Set, &'static str)> {
    let label = match (first, n) {
        (true, 0..=3) => "bounce-1",
        (true, 4) => "bounce-2",
        (false, 0..=3) => "s-bounce-1",
        (false, 4) => "s-bounce-2",
        _ => return None,
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

/// On frame `c` of standing, 0 on its first.
fn standing(first: bool, c: u8) -> Option<(Set, &'static str)> {
    const FIRST: [&str; 5] = ["stand-1", "stand-2", "stand-3", "stand-4", "stand-5"];
    let label = match first {
        true => *FIRST.get(usize::from(c / 3))?,
        false if c < 4 => "s-stand-1",
        false => return None,
    };
    Some((Set::Standing, label))
}

/// The first character walking, running or turning on the ground, on
/// the step its frame ends in or turn frame T + n.
fn walking(player: &clamber_sim::Player) -> Option<(Set, &'static str)> {
    if player.roll.is_some()
        || player.roll_stop > 0
        || player.crouch.held > 0
        || player.crouch.rising > 0
    {
        return None;
    }
    let label = match (player.turn, player.gait) {
        (Some(2..=5), _) => "turn-b",
        (Some(_), _) => "turn-a",
        (None, Gait::Walking(stride)) => *WALKING[..20].get(stride.step.checked_sub(1)? as usize)?,
        (None, Gait::Running(stride)) => {
            *WALKING[20..40].get(stride.step.checked_sub(1)? as usize)?
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
        (false, _) => return None,
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
fn first_landing(landed: Landed, gait: Gait) -> Option<(Set, &'static str)> {
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
/// that lands on the floor it took off from and doesn't run from L.
fn second_landing(
    landed: Landed,
    since_land: Option<u8>,
    gait: Gait,
    state: State,
) -> Option<(Set, &'static str)> {
    if !landed.same_floor || matches!(gait, Gait::Running(_)) || state == State::Run {
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

    use clamber_input::{Action, Actions, Frame};
    use clamber_level::Level;
    use clamber_pack::{CharacterFeel, Presets};
    use clamber_sim::{InputFrame, ROLL, generic_feel, step};

    use super::*;
    use crate::game::jump_next;

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
    }

    fn run(mut world: World, keys: &[Vec<Action>]) -> Run {
        let mut clock = PictureClock::default();
        let mut held = Actions::NONE;
        let mut out = Run {
            shown: Vec::new(),
            events: Vec::new(),
            roll_stop: Vec::new(),
        };
        for frame in keys {
            let now: Actions = frame.iter().copied().collect();
            let input = InputFrame::single(Frame::after(held, now));
            held = now;
            step(&mut world, &input);
            clock.update(&world, jump_next(&world, &input));
            out.shown.push(clock.shown().map(|(_, label)| label));
            out.events.push(world.events.clone());
            out.roll_stop.push(world.player.roll_stop);
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
                ("", 1),
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
                ("", 1),
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
                ("", 1),
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
        let end = second.frames(Event::RollEnd)[0];
        let d = (0..end).find(|&f| second.shown[f].is_some()).unwrap();
        second.check(d, &[("s-stop-1", 3), ("s-stop-2", 3), ("", 1)]);
    }

    #[test]
    fn the_second_character_s_jump_and_landing() {
        let run = run(world(FLAT, 1), &jump(10, 1, 120));
        let t = run.frames(Event::JumpStart)[0];
        let l = run.frames(Event::Land)[0];
        let mut want = vec![("s-jump-1", 1), ("s-jump-2", 2)];
        for r in [
            "s-rise-1", "s-rise-2", "s-rise-3", "s-rise-4", "s-rise-5", "s-rise-6", "s-rise-7",
        ] {
            want.push((r, 2));
        }
        want.push(("", l - (t + 16)));
        want.extend([
            ("s-land-1", 1),
            ("s-land-2", 2),
            ("s-land-3", 3),
            ("s-land-4", 8),
            ("s-land-3", 3),
            ("s-stand-1", 4),
            ("", 1),
        ]);
        run.check(t - 1, &want);
    }

    #[test]
    fn a_bounce_s_frozen_frames() {
        let level = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../content/test/bounce-walker.level"
        ))
        .unwrap();
        // Standing still, a hop that comes down on the walker coming.
        let run = (20..80)
            .map(|wait| run(world(&level, 0), &jump(wait, 1, 160)))
            .find(|run| !run.frames(Event::Bounce).is_empty())
            .expect("a hop bounces");
        let b = run.frames(Event::Bounce)[0];
        run.check(b, &[("bounce-1", 4), ("bounce-2", 1), ("", 1)]);
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
        run.check(10, &[("turn-a", 2), ("turn-b", 4), ("turn-a", 2), ("", 1)]);
    }

    #[test]
    fn the_sets_are_the_importer_s() {
        for (slot, sets) in SETS.iter().enumerate() {
            let decoder = clamber_import::CharacterDecoder::of("title-1").unwrap();
            let character = decoder.characters.iter().find(|c| c.slot == slot).unwrap();
            let names = ["stop", "standing", "jump", "bounce", "walking"];
            for ((set, labels), name) in sets.iter().zip(names) {
                let theirs = character.sets.iter().find(|s| s.name == name).unwrap();
                let theirs: Vec<&str> = theirs.pictures.iter().map(|&(l, _)| l).collect();
                assert_eq!(&theirs, labels, "{set:?}");
            }
        }
        assert_eq!(crate::cast::CHARACTER, clamber_import::CHARACTER_SHEET);
    }
}
