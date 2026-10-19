//! Which of its own pictures the player shows when it's held still: the
//! stop after a roll, a jump and its landing, a bounce, the first
//! standing pictures after them, and the first character's roll,
//! walking, running and turning pictures, on the ground and in the air
//! after walking off a ledge without a face
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
//!   standing pictures, the second character walking or running, or
//!   bouncing after B + 4) are the engine's own art, as are the second
//!   character's in the air from T + 16 until it lands.
//! - Walking or running in the air after walking off a ledge with a
//!   face, and crouching or getting up, show the engine's own art, as
//!   does the second character's roll.
//! - The second character's landing pictures are shown after a jump
//!   that lands on the floor it took off from or lower: the spec gives
//!   them for landings on the fourth or a later `s-air-6` frame, which
//!   every jump measured onto the floor it took off from is, but not
//!   when `s-air-1` ends. A jump out of a roll pressed in the air takes
//!   off from where it was in the fall.
//! - Jumps that land before their approach frame, landings after falls,
//!   the second character's landings after bounces, and anything while
//!   carrying, swapping, hurt or losing a life show the engine's own art.

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
    /// A bounce, until the landing.
    Bounce,
    /// The roll cycle.
    Roll,
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
        (
            Set::Roll,
            &[
                "roll-1", "roll-2", "roll-3", "roll-4", "roll-5", "roll-6", "roll-7", "roll-8",
                "roll-9", "roll-end", "stop",
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
    /// On the floor it took off from, or lower.
    not_above: bool,
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
    /// The first character's roll cycle frame, 1 to 33, and the roll's
    /// own frame count it was moved on at.
    cycle: Option<(u8, u8)>,
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
                    not_above: self.takeoff_y.is_some_and(|t| y >= t),
                })
            });
        } else if !player.grounded() || events.contains(&Event::JumpStart) {
            self.landed = None;
        }
        self.carried = match player.roll {
            Some(roll) if roll.carry => Some(self.carried.map_or(0, |c| c.saturating_add(1))),
            _ => None,
        };
        self.cycle = match player.roll {
            Some(roll) if first && !roll.carry => {
                let restart = events.contains(&Event::RollStart) || player.boosted;
                let cycle = match self.cycle {
                    _ if restart => 2 - u8::from(roll.from_stop),
                    // The roll's first airborne frame, or its landing.
                    _ if roll.air == 1 || events.contains(&Event::Land) => 1,
                    // Held still by a defeat's pause.
                    Some((c, at)) if at == roll.frame => c,
                    Some((c, _)) => c % 33 + 1,
                    None => 2,
                };
                Some((cycle, roll.frame))
            }
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
        } else if let Some((c, _)) = self.cycle {
            Some((Set::Roll, labels(0, Set::Roll)[usize::from((c - 1) / 3)]))
        } else if let Some(age) = jump_age {
            in_air(first, age, player.approach, &physics)
        } else if let Some(landed) = self.landed {
            if first {
                first_landing(landed, player.gait)
            } else {
                second_landing(landed, player.since_land, player.gait, player.state)
            }
        } else if first && (player.grounded() || upright_fall(player)) {
            walking(player)
        } else {
            None
        };
        // Standing starts from a stop's last picture, a landing's, the
        // first character's walking, running or turning, or on the
        // landing frame after the first character's bounce. After a
        // walk-off without a face it starts, or goes on, in the air.
        let standing_now = self.shown.is_none()
            && (player.grounded()
                && matches!(player.state, State::Idle | State::Walk | State::Run)
                || first && upright_fall(player))
            && player.roll_stop == 0
            && player.crouch.held == 0
            && player.crouch.rising == 0
            && matches!(player.gait, Gait::Standing);
        self.standing = match self.standing {
            _ if !standing_now => None,
            Some(c) => Some(c.saturating_add(1)),
            None => before
                .is_some_and(|(set, _)| {
                    matches!(set, Set::Stop | Set::Jump)
                        || (first && matches!(set, Set::Bounce | Set::Walking))
                })
                .then_some(0),
        };
        if let Some(c) = self.standing {
            self.shown = standing(first, c);
        }
    }
}

/// In the air after walking or running off a ledge without a face,
/// where the mode goes on (`ground-speed.md`, "After a fall or a
/// bounce").
fn upright_fall(player: &clamber_sim::Player) -> bool {
    !player.grounded() && player.walked_off && !player.walked_off_face && player.roll.is_none()
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

/// On B + `n`: the first character's 16 pictures 2 frames each from
/// `bounce-2` on B + 4, round and round until the landing.
fn bounce(first: bool, n: u8) -> Option<(Set, &'static str)> {
    let label = match (first, n) {
        (true, 0..=3) => "bounce-1",
        (true, _) => labels(0, Set::Bounce)[usize::from((n - 2) / 2) % 16],
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

/// The first character walking, running or turning on the ground or in
/// a walk-off's fall, on the step its frame ends in or turn frame T + n.
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
        grounded: Vec<bool>,
        frozen: Vec<u8>,
    }

    fn run(mut world: World, keys: &[Vec<Action>]) -> Run {
        let mut clock = PictureClock::default();
        let mut held = Actions::NONE;
        let mut out = Run {
            shown: Vec::new(),
            events: Vec::new(),
            roll_stop: Vec::new(),
            grounded: Vec::new(),
            frozen: Vec::new(),
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
            out.grounded.push(world.player.grounded());
            out.frozen.push(world.player.frozen);
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
        second_jump_and_landing(&run(world(FLAT, 1), &jump(10, 1, 120)));
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

    #[test]
    fn a_bounce_s_frozen_frames() {
        let (run, b, _) = bounce_run(1, 1, &[]);
        run.check(b, &[("s-bounce-1", 4), ("s-bounce-2", 1), ("", 1)]);
    }

    #[test]
    fn the_first_character_s_bounce_pictures_until_the_landing() {
        let (run, b, g) = bounce_run(0, 20, &[Action::Jump]);
        let air = g - b;
        assert!(air > 34, "round again from B + 34: {air}");
        let mut want = vec![("bounce-1", 4)];
        for i in 2..=16 {
            want.push((labels(0, Set::Bounce)[i - 1], 2));
        }
        // Then round again from bounce-1, 2 frames each, up to G − 1.
        let mut f = b + 34;
        let mut i = 0;
        while f < g {
            let n = (g - f).min(2);
            want.push((labels(0, Set::Bounce)[i % 16], n));
            f += n;
            i += 1;
        }
        // No landing pictures: standing from G.
        for s in ["stand-1", "stand-2", "stand-3", "stand-4", "stand-5"] {
            want.push((s, 3));
        }
        run.check(b, &want);
    }

    #[test]
    fn walking_or_a_turn_from_a_bounce_s_landing_frame() {
        let (run, _, g) = bounce_run(0, 1, &[Action::Right]);
        run.check(g, &[("walk-1", 1)]);
        let (run, _, g) = bounce_run(0, 1, &[Action::Left]);
        run.check(g, &[("turn-a", 2)]);
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
                ("", 1),
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
                ("", 1),
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

    #[test]
    fn the_sets_are_the_importer_s() {
        for (slot, sets) in SETS.iter().enumerate() {
            let decoder = clamber_import::CharacterDecoder::of("title-1").unwrap();
            let character = decoder.characters.iter().find(|c| c.slot == slot).unwrap();
            let names = ["stop", "standing", "jump", "bounce", "roll", "walking"];
            for ((set, labels), name) in sets.iter().zip(names) {
                let theirs = character.sets.iter().find(|s| s.name == name).unwrap();
                let theirs: Vec<&str> = theirs.pictures.iter().map(|&(l, _)| l).collect();
                assert_eq!(&theirs, labels, "{set:?}");
            }
        }
        assert_eq!(crate::cast::CHARACTER, clamber_import::CHARACTER_SHEET);
    }
}
