//! What the animal and its rider show while the first character rides
//! on flat floor (`spec/behavior/riding.md`, "Standing and walking" and
//! "Pictures").
//!
//! Every frame of riding draws two pictures at the player's position:
//! the animal, and the rider on it, drawn `d` px lower. [`Clock`]
//! follows the rider's parts ([`RidePart`]) to say which picture of each
//! and which `d` a frame has: standing (the animal's 60-frame cycle and
//! the rider's 284-frame loop), walking (the 15-step stride and the
//! rider's moving cycle), a turn, a jump, and a charge with its end.
//!
//! The second character has its own rider pictures, loop, cycle and
//! heights ("Pictures with the second character"); [`Clock`] follows
//! them when the player is not the first character.
//!
//! The pictures are in the pack's [`hutch_pictures::SHEET`]. The
//! animal's ([`ANIMAL_GROUPS`]) and the rider's ([`RIDER`], or
//! [`RIDER2`] for the second character) are shown only when the sheet
//! has the whole group; otherwise the engine's art stays.

use clamber_sim::{Player, RidePart};

use crate::hutch_pictures;

pub const TURN: [&str; 2] = ["ride-turn-1", "ride-turn-2"];

pub const WALK: [&str; 14] = [
    "ride-walk-1",
    "ride-walk-2",
    "ride-walk-3",
    "ride-walk-4",
    "ride-walk-5",
    "ride-walk-6",
    "ride-walk-7",
    "ride-walk-8",
    "ride-walk-9",
    "ride-walk-10",
    "ride-walk-11",
    "ride-walk-12",
    "ride-walk-13",
    "ride-walk-14",
];

pub const JUMP: [&str; 6] = [
    "ride-jump-1",
    "ride-jump-2",
    "ride-jump-3",
    "ride-jump-4",
    "ride-jump-5",
    "ride-jump-6",
];

pub const CHARGE: [&str; 2] = ["ride-charge-1", "ride-charge-2"];

/// The animal's groups that must all be in the sheet: its riding
/// pictures, and its standing ones (`animal-1` to `animal-6`).
pub const ANIMAL_GROUPS: [&[&str]; 5] = [&TURN, &WALK, &JUMP, &CHARGE, &hutch_pictures::STANDING];

pub const RIDER: [&str; 16] = [
    "rider-1", "rider-2", "rider-3", "rider-4", "rider-5", "rider-6", "rider-7", "rider-8",
    "rider-9", "rider-10", "rider-11", "rider-12", "rider-13", "rider-14", "rider-15", "rider-16",
];

/// The second character's rider pictures.
pub const RIDER2: [&str; 14] = [
    "rider2-1",
    "rider2-2",
    "rider2-3",
    "rider2-4",
    "rider2-5",
    "rider2-6",
    "rider2-7",
    "rider2-8",
    "rider2-9",
    "rider2-10",
    "rider2-11",
    "rider2-12",
    "rider2-13",
    "rider2-14",
];

/// The rider group a rider picture `label` belongs to.
pub fn rider_group(label: &str) -> Option<&'static [&'static str]> {
    if RIDER.contains(&label) {
        Some(&RIDER)
    } else if RIDER2.contains(&label) {
        Some(&RIDER2)
    } else {
        None
    }
}

/// The animal's picture of the standing cycle, by the place in its 60
/// frames: `animal-1` to `animal-6` and back, 6 frames each.
const STANDING_CYCLE: [usize; 10] = [0, 1, 2, 3, 4, 5, 4, 3, 2, 1];

/// The rider's standing loop: its picture and the frames it shows.
const LOOP: [(usize, u16); 14] = [
    (6, 58),
    (7, 10),
    (8, 10),
    (1, 10),
    (8, 10),
    (7, 10),
    (6, 58),
    (7, 10),
    (8, 10),
    (1, 10),
    (2, 10),
    (3, 58),
    (4, 10),
    (5, 10),
];
const LOOP_FRAMES: u16 = 284;
/// Where the loop is on S in general: entry 10 with 9 frames of it left.
const LOOP_START: u16 = 187;

/// The rider's moving cycle: pictures `rider-14`, `rider-15`,
/// `rider-16`, `rider-15`.
const CYCLE: [usize; 4] = [14, 15, 16, 15];

/// The second character's standing loop: its picture and the frames it
/// shows.
const LOOP2: [(usize, u16); 12] = [
    (1, 40),
    (2, 8),
    (3, 8),
    (4, 40),
    (3, 8),
    (2, 6),
    (3, 6),
    (4, 6),
    (5, 20),
    (4, 8),
    (3, 8),
    (2, 8),
];
const LOOP2_FRAMES: u16 = 166;
/// Where the second character's loop is on S in general: its first frame
/// is spent (`rider2-1` shows 39 frames, not 40).
const LOOP2_START: u16 = 1;
/// Where it is on S after a charge: the 5th frame of entry 1.
const LOOP2_AFTER_CHARGE: u16 = 4;

/// The second character's moving cycle: pictures `rider2-6` to
/// `rider2-9` and back, 6 frames each.
const CYCLE2: [usize; 6] = [6, 7, 8, 9, 8, 7];

/// The rider's height `d` by stride step.
const HEIGHT: [i32; 15] = [1, 1, 0, 0, -1, -2, -2, -3, -3, -3, -2, -2, -1, 0, 1];

/// The length of stride step `step` of 1 to 15 in counter units.
fn step_length(step: u8) -> i32 {
    match step {
        14 => 512,
        15 => 256,
        _ => 768,
    }
}

/// Slowest `v` (in 1/256 px per frame) of a first riding frame with
/// nothing held that walks.
const WALKS_AT: i64 = 224;
/// A step end below this speed can stand or start the walk again.
const STAND_BELOW: i64 = 48;
/// The slowest the stride counter goes down.
const SLOWEST_RATE: i32 = 192;
/// The farthest a jump's rider is above its floor for the fall limit.
const FALL_LIMIT: i64 = 72 << 32;

/// What a frame of riding shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub animal: &'static str,
    pub rider: &'static str,
    /// How many px lower than the animal the rider is drawn.
    pub d: i32,
    /// Both pictures are mirrored.
    pub flip: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Gait {
    #[default]
    None,
    /// Frames since S, and the place in the rider's loop.
    Standing {
        n: u32,
        pos: u16,
    },
    Walking {
        step: u8,
        counter: i32,
    },
}

/// A jump under way, from its take-off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Flight {
    /// The level y of the take-off, in fixed-point bits.
    takeoff: i64,
    /// The first frame that ended moving down, as frames since P.
    fell: Option<u8>,
    /// The first frame from then that ended at most 72 px above the
    /// take-off.
    low: Option<u8>,
}

/// What a frame of riding gives the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub part: RidePart,
    /// The frame was steered.
    pub steered: bool,
    /// Left or Right held alone: -1, 1, or 0.
    pub dir: i32,
    /// Facing, -1 or 1.
    pub facing: i32,
    /// The speed after the frame, in 1/256 px per frame.
    pub speed: i64,
    /// The level y, in fixed-point bits, and whether `vy` is positive.
    pub y: i64,
    pub falling: bool,
    /// Frames of a turn and of a jump's crouch.
    pub turn: u8,
    pub crouch: u8,
}

/// Follows a rider to say which pictures it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// The player is the second character.
    second: bool,
    last: Option<RidePart>,
    gait: Gait,
    /// Frames since the rider's moving cycle started.
    cycle: u32,
    /// The rider's height, once a step, turn or charge has set it.
    d: Option<i32>,
    /// Frames since the charge began.
    charge: Option<u32>,
    flight: Option<Flight>,
    /// The loop place of the last standing frame before a jump, and the
    /// entry it was in, still shown as the jump's end picture.
    saved: Option<u16>,
    ending: Option<usize>,
    /// The standing loop is to resume after a charge.
    charged: bool,
    shown: Option<Shown>,
}

/// The loop entry the place `pos` is in.
fn entry(pos: u16, second: bool) -> usize {
    let mut left = pos;
    let table: &[(usize, u16)] = if second { &LOOP2 } else { &LOOP };
    for (i, &(_, frames)) in table.iter().enumerate() {
        if left < frames {
            return i;
        }
        left -= frames;
    }
    0
}

impl Clock {
    /// What the player shows on this frame, if it's riding.
    pub fn shown(&self) -> Option<Shown> {
        self.shown
    }

    /// A clock showing `shown`.
    #[cfg(test)]
    pub fn showing(shown: Shown) -> Clock {
        Clock {
            shown: Some(shown),
            ..Clock::default()
        }
    }

    /// Moves on to `player` as it is after a logic frame.
    pub fn update(&mut self, player: &Player) {
        let Some(r) = player.riding else {
            *self = Clock::default();
            return;
        };
        self.second = player.character != 0;
        let dir = match player.target.to_bits().signum() {
            1 => 1,
            -1 => -1,
            _ => 0,
        };
        self.step(Frame {
            part: r.part,
            steered: r.steered,
            dir,
            facing: if player.facing == clamber_sim::Facing::Left {
                -1
            } else {
                1
            },
            speed: player.vel.x.abs().to_bits() >> 24,
            y: player.pos.y.to_bits(),
            falling: player.vel.y.to_bits() > 0,
            turn: r.ride.turn,
            crouch: r.ride.crouch,
        });
    }

    /// Moves on a frame.
    pub fn step(&mut self, f: Frame) {
        let prev = self.last.replace(f.part);
        let flip = f.facing < 0;
        let v = f.speed;
        let forward = f.dir != 0 && (f.steered || f.dir == f.facing);
        let after_turn = matches!(prev, Some(RidePart::Turn(a)) if a + 1 >= f.turn);
        let walking = matches!(self.gait, Gait::Walking { .. });
        let animal;
        let mut rider_n;
        match f.part {
            RidePart::Turn(age) => {
                if age == 0 {
                    if let Gait::Walking { step, counter } = self.gait {
                        let (step, _) = advance(step, counter, v);
                        self.d = Some(HEIGHT[usize::from(step) - 1]);
                        self.cycle += 1;
                    } else {
                        self.d = Some(1);
                        self.cycle = 0;
                    }
                } else {
                    self.cycle += 1;
                }
                self.gait = Gait::None;
                self.saved = None;
                self.ending = None;
                self.charged = false;
                animal = TURN[match age {
                    2..=5 => 1,
                    _ => 0,
                }];
                rider_n = self.moving();
            }
            RidePart::Jump(age) => {
                if age == 0 {
                    self.saved = match self.gait {
                        Gait::Standing { pos, .. } => Some(pos),
                        _ => None,
                    };
                    self.ending = self.saved.map(|pos| entry(pos, self.second));
                    self.charged = false;
                    self.flight = Some(Flight::default());
                }
                self.gait = Gait::None;
                let flight = self.flight.get_or_insert_with(Flight::default);
                if age == f.crouch {
                    flight.takeoff = f.y;
                }
                if age > f.crouch {
                    if flight.fell.is_none() && f.falling {
                        flight.fell = Some(age);
                    }
                    if flight.fell.is_some()
                        && flight.low.is_none()
                        && flight.takeoff - f.y <= FALL_LIMIT
                    {
                        flight.low = Some(age);
                    }
                }
                let late = matches!(
                    (flight.fell, flight.low),
                    (Some(fell), Some(low)) if age >= fell + 2 && age > low
                );
                let since = age.saturating_sub(f.crouch);
                (animal, rider_n) = match age {
                    a if a < f.crouch => (JUMP[0], 9),
                    _ if since < 3 => (JUMP[1], 10),
                    _ if since < 6 && !late => (JUMP[2], 11),
                    _ if late => (JUMP[4], 12),
                    _ => (JUMP[3], 11),
                };
                if self.second {
                    // The rider's last picture starts a frame before the
                    // animal's (S1 - 1).
                    let next = matches!(
                        (flight.fell, flight.low),
                        (Some(fell), Some(low)) if age + 1 >= fell + 2 && age + 1 > low
                    );
                    let fell = flight.fell.is_some();
                    let (n, d) = match age {
                        a if a < f.crouch => (10, 2),
                        _ if since < 3 => (11, 0),
                        _ if since < 6 => (11, -2),
                        _ if next => (14, -2),
                        _ if fell => (13, -3),
                        _ => (12, -4),
                    };
                    rider_n = n;
                    self.d = Some(d);
                }
            }
            RidePart::Landing(age) => {
                self.gait = Gait::None;
                (animal, rider_n) = if age < 3 { (JUMP[5], 13) } else { (JUMP[0], 9) };
                if self.second {
                    (rider_n, self.d) = if age < 3 {
                        (13, Some(2))
                    } else {
                        (11, Some(3))
                    };
                }
            }
            RidePart::Charge { age: 0, .. } => {
                self.d = Some(if self.second { 4 } else { 1 });
                self.charge = Some(0);
                self.charged = true;
                self.gait = Gait::None;
                self.saved = None;
                self.ending = None;
                animal = CHARGE[0];
                rider_n = if self.second { 10 } else { 9 };
            }
            RidePart::Rush { .. } => {
                // The frame a charge is started on: nothing changes.
                return;
            }
            RidePart::Charge { .. } | RidePart::ChargeEnd { .. } => {
                let c = self.charge.map_or(0, |c| c + 1);
                self.charge = Some(c);
                self.gait = Gait::None;
                animal = match f.part {
                    RidePart::ChargeEnd { age, .. } => match age {
                        0..=2 => WALK[9],
                        3..=5 => WALK[10],
                        6..=8 => WALK[12],
                        _ => WALK[13],
                    },
                    RidePart::Charge { age, .. } if age < 10 => CHARGE[0],
                    _ => CHARGE[1],
                };
                rider_n = if self.second {
                    if c >= 10 {
                        self.d = Some(1);
                    }
                    match c {
                        0..=1 => 10,
                        2..=4 => 11,
                        5..=7 => 12,
                        8..=10 => 13,
                        11..=26 => 14,
                        27..=29 => 13,
                        30..=32 => 12,
                        33..=35 => 11,
                        _ => 1,
                    }
                } else {
                    match c {
                        0..=5 => 9,
                        6..=8 => 10,
                        9..=32 => 11,
                        33..=37 => 10,
                        _ => 9,
                    }
                };
            }
            RidePart::Riding | RidePart::Settling => {
                let first = !matches!(prev, Some(RidePart::Riding | RidePart::Settling));
                let from_charge = self.charge.take().is_some();
                if from_charge {
                    self.charged = true;
                }
                let mut gait = self.gait;
                if after_turn {
                    self.d = Some(1);
                }
                match gait {
                    Gait::Standing { n, pos } if !first => {
                        gait = if forward {
                            Gait::Walking {
                                step: 0,
                                counter: 0,
                            }
                        } else {
                            Gait::Standing {
                                n: n + 1,
                                pos: (pos + 1) % self.loop_frames(),
                            }
                        };
                    }
                    Gait::Walking { step, counter } if !first => {
                        let (next, ended) = advance(step, counter, v);
                        let stand =
                            ended.is_some_and(|from| matches!(from, 4 | 15)) && v < STAND_BELOW;
                        gait = if stand && f.dir == 0 {
                            Gait::Standing {
                                n: 0,
                                pos: self.loop_start(),
                            }
                        } else if stand && forward {
                            Gait::Walking {
                                step: 0,
                                counter: 0,
                            }
                        } else {
                            Gait::Walking {
                                step: next,
                                counter: next_counter(step, counter, v),
                            }
                        };
                    }
                    _ => {
                        gait = if forward || v >= WALKS_AT {
                            Gait::Walking {
                                step: 0,
                                counter: 0,
                            }
                        } else {
                            Gait::Standing {
                                n: 0,
                                pos: self.resume_pos(),
                            }
                        };
                    }
                }
                // A walk starts at step 1 with its counter full.
                if let Gait::Walking { step: 0, .. } = gait {
                    gait = Gait::Walking {
                        step: 1,
                        counter: 768,
                    };
                    self.cycle = 0;
                    self.saved = None;
                    self.ending = None;
                    self.charged = false;
                } else if walking || matches!(gait, Gait::Walking { .. }) {
                    self.cycle += 1;
                }
                if after_turn {
                    self.cycle = 0;
                }
                self.gait = gait;
                match gait {
                    Gait::Walking { step, .. } => {
                        self.d = Some(HEIGHT[usize::from(step) - 1]);
                        animal = WALK[usize::from(step.min(14)) - 1];
                        rider_n = self.moving();
                    }
                    Gait::Standing { .. } | Gait::None => {
                        let (n, pos) = match gait {
                            Gait::Standing { n, pos } => (n, pos),
                            _ => (0, self.loop_start()),
                        };
                        animal = hutch_pictures::STANDING[STANDING_CYCLE[(n as usize / 6) % 10]];
                        let at = entry(pos, self.second);
                        if self.ending.is_some_and(|e| e != at) {
                            self.ending = None;
                        }
                        rider_n = if self.ending.is_some() {
                            if self.second { 11 } else { 9 }
                        } else if self.second {
                            LOOP2[at].0
                        } else {
                            LOOP[at].0
                        };
                    }
                }
            }
        }
        self.shown = Some(Shown {
            animal,
            rider: if self.second {
                RIDER2[rider_n - 1]
            } else {
                RIDER[rider_n - 1]
            },
            d: self.d.unwrap_or(1),
            flip,
        });
    }

    /// Where the rider's loop is on an S that follows a jump taken from
    /// standing, or on S in general.
    fn resume_pos(&mut self) -> u16 {
        match self.saved.take() {
            Some(pos) => {
                let next = (pos + 1) % self.loop_frames();
                self.ending = self.ending.filter(|&e| e == entry(next, self.second));
                next
            }
            None => {
                self.ending = None;
                if self.second && std::mem::take(&mut self.charged) {
                    LOOP2_AFTER_CHARGE
                } else {
                    self.loop_start()
                }
            }
        }
    }

    fn loop_frames(&self) -> u16 {
        if self.second {
            LOOP2_FRAMES
        } else {
            LOOP_FRAMES
        }
    }

    fn loop_start(&self) -> u16 {
        if self.second { LOOP2_START } else { LOOP_START }
    }

    /// The picture number of the rider's moving cycle.
    fn moving(&self) -> usize {
        let m = self.cycle as usize + 1;
        if self.second {
            CYCLE2[m / 6 % 6]
        } else {
            CYCLE[m / 8 % 4]
        }
    }
}

/// The step and counter after a frame of walking at `v`, and the step
/// that ended on it, if one did (the last, if several).
fn advance(step: u8, counter: i32, v: i64) -> (u8, Option<u8>) {
    let (mut step, mut counter, mut ended) = (step, counter - rate(v), None);
    while counter <= 0 {
        ended = Some(step);
        step = step % 15 + 1;
        counter += step_length(step);
    }
    (step, ended)
}

/// The counter [`advance`] leaves.
fn next_counter(step: u8, counter: i32, v: i64) -> i32 {
    let (mut step, mut counter) = (step, counter - rate(v));
    while counter <= 0 {
        step = step % 15 + 1;
        counter += step_length(step);
    }
    counter
}

/// How much the stride counter goes down at speed `v`.
fn rate(v: i64) -> i32 {
    ((v / 2 + v / 4) as i32).max(SLOWEST_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(part: RidePart, dir: i32, v: i64) -> Frame {
        Frame {
            part,
            steered: false,
            dir,
            facing: 1,
            speed: v,
            y: 0,
            falling: false,
            turn: 8,
            crouch: 5,
        }
    }

    fn run(clock: &mut Clock, f: Frame) -> Shown {
        clock.step(f);
        clock.shown().unwrap()
    }

    #[test]
    fn standing_starts_the_loop_at_its_tenth_entry() {
        let mut clock = Clock::default();
        let shown: Vec<_> = (0..20)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 0, 0)).rider)
            .collect();
        assert!(shown[..9].iter().all(|&r| r == "rider-1"));
        assert!(shown[9..19].iter().all(|&r| r == "rider-2"));
        assert_eq!(shown[19], "rider-3");
    }

    #[test]
    fn the_animal_stands_in_a_cycle_of_sixty() {
        let mut clock = Clock::default();
        let shown: Vec<_> = (0..61)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 0, 0)).animal)
            .collect();
        assert_eq!(shown[0], "animal-1");
        assert_eq!(shown[6], "animal-2");
        assert_eq!(shown[30], "animal-6");
        assert_eq!(shown[36], "animal-5");
        assert_eq!(shown[59], "animal-2");
        assert_eq!(shown[60], "animal-1");
    }

    #[test]
    fn a_jump_from_standing_keeps_the_loop_where_it_was() {
        // Taken on the 30th frame of entry 1 (place 29): 28 frames of it
        // are left.
        let mut clock = Clock {
            last: Some(RidePart::Riding),
            gait: Gait::Standing { n: 99, pos: 29 },
            ..Clock::default()
        };
        run(&mut clock, frame(RidePart::Jump(0), 0, 0));
        for age in 1..=5 {
            run(&mut clock, frame(RidePart::Jump(age), 0, 0));
        }
        run(&mut clock, frame(RidePart::Landing(0), 0, 0));
        for age in 1..8 {
            run(&mut clock, frame(RidePart::Landing(age), 0, 0));
        }
        let shown: Vec<_> = (0..30)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 0, 0)).rider)
            .collect();
        assert!(shown[..28].iter().all(|&r| r == "rider-9"));
        assert_eq!(shown[28], "rider-7");
    }

    #[test]
    fn a_jump_shows_the_spec_example() {
        // P = 0, T = 5, F = 22, S1 = 24, L = 36.
        let mut clock = Clock::default();
        run(&mut clock, frame(RidePart::Riding, 0, 0));
        let mut animals = Vec::new();
        for age in 0..36u8 {
            let mut f = frame(RidePart::Jump(age), 0, 0);
            f.falling = age >= 22;
            animals.push(run(&mut clock, f));
        }
        for age in 0..8 {
            animals.push(run(&mut clock, frame(RidePart::Landing(age), 0, 0)));
        }
        let at = |n: usize| (animals[n].animal, animals[n].rider);
        assert_eq!(at(0), ("ride-jump-1", "rider-9"));
        assert_eq!(at(4), ("ride-jump-1", "rider-9"));
        assert_eq!(at(5), ("ride-jump-2", "rider-10"));
        assert_eq!(at(8), ("ride-jump-3", "rider-11"));
        assert_eq!(at(11), ("ride-jump-4", "rider-11"));
        assert_eq!(at(23), ("ride-jump-4", "rider-11"));
        assert_eq!(at(24), ("ride-jump-5", "rider-12"));
        assert_eq!(at(35), ("ride-jump-5", "rider-12"));
        assert_eq!(at(36), ("ride-jump-6", "rider-13"));
        assert_eq!(at(39), ("ride-jump-1", "rider-9"));
        assert_eq!(at(43), ("ride-jump-1", "rider-9"));
    }

    #[test]
    fn walking_goes_through_the_stride_at_two_px_a_frame() {
        let mut clock = Clock::default();
        let shown: Vec<_> = (0..8)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 1, 512)))
            .collect();
        // Step 1 on W; at v = 512 the rate is 384, so a step lasts 2.
        assert_eq!(shown[0].animal, "ride-walk-1");
        assert_eq!(shown[1].animal, "ride-walk-1");
        assert_eq!(shown[2].animal, "ride-walk-2");
        assert_eq!(shown[0].d, 1);
        assert_eq!(shown[4].animal, "ride-walk-3");
        assert_eq!(shown[4].d, 0);
        assert_eq!(shown[0].rider, "rider-14");
        assert_eq!(shown[7].rider, "rider-15");
    }

    #[test]
    fn slow_walking_stops_at_the_end_of_step_four() {
        // From standing, Right held on one frame only: step 4 ends on
        // frame 16 with v below 48.
        let mut clock = Clock::default();
        run(&mut clock, frame(RidePart::Riding, 0, 0));
        run(&mut clock, frame(RidePart::Riding, 1, 16));
        let mut last = run(&mut clock, frame(RidePart::Riding, 0, 0));
        assert!(last.animal.starts_with("ride-walk"));
        for _ in 0..30 {
            last = run(&mut clock, frame(RidePart::Riding, 0, 0));
        }
        assert!(last.animal.starts_with("animal"));
    }

    #[test]
    fn a_turn_shows_its_pictures_and_sets_the_height() {
        let mut clock = Clock::default();
        run(&mut clock, frame(RidePart::Riding, 1, 512));
        let mut shown = Vec::new();
        for age in 0..8 {
            shown.push(run(&mut clock, frame(RidePart::Turn(age), -1, 400)));
        }
        let animals: Vec<_> = shown.iter().map(|s| s.animal).collect();
        assert_eq!(
            animals,
            [
                "ride-turn-1",
                "ride-turn-1",
                "ride-turn-2",
                "ride-turn-2",
                "ride-turn-2",
                "ride-turn-2",
                "ride-turn-1",
                "ride-turn-1"
            ]
        );
        let after = run(&mut clock, frame(RidePart::Riding, 0, 0));
        assert_eq!(after.d, 1);
    }

    #[test]
    fn a_charge_shows_the_rider_by_its_frames() {
        let mut clock = Clock::default();
        run(&mut clock, frame(RidePart::Riding, 0, 0));
        let mut riders = Vec::new();
        for age in 0..28 {
            let part = RidePart::Charge {
                age,
                steered: false,
            };
            riders.push(run(&mut clock, frame(part, 0, 0)));
        }
        for age in 0..12 {
            let part = RidePart::ChargeEnd {
                age,
                steered: false,
            };
            riders.push(run(&mut clock, frame(part, 0, 0)));
        }
        assert_eq!(riders[0].animal, "ride-charge-1");
        assert_eq!(riders[10].animal, "ride-charge-2");
        assert_eq!(riders[28].animal, "ride-walk-10");
        assert_eq!(riders[39].animal, "ride-walk-14");
        let r = |n: usize| riders[n].rider;
        assert_eq!((r(0), r(5)), ("rider-9", "rider-9"));
        assert_eq!((r(6), r(8)), ("rider-10", "rider-10"));
        assert_eq!((r(9), r(32)), ("rider-11", "rider-11"));
        assert_eq!((r(33), r(37)), ("rider-10", "rider-10"));
        assert_eq!((r(38), r(39)), ("rider-9", "rider-9"));
    }

    fn second() -> Clock {
        Clock {
            second: true,
            ..Clock::default()
        }
    }

    #[test]
    fn the_second_characters_standing_loop_starts_a_frame_in() {
        let mut clock = second();
        let shown: Vec<_> = (0..48)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 0, 0)).rider)
            .collect();
        assert!(shown[..39].iter().all(|&r| r == "rider2-1"));
        assert!(shown[39..47].iter().all(|&r| r == "rider2-2"));
        assert_eq!(shown[47], "rider2-3");
    }

    #[test]
    fn the_second_characters_loop_is_166_frames() {
        let mut clock = second();
        let shown: Vec<_> = (0..167)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 0, 0)).rider)
            .collect();
        // Entry 1 has 39 frames the first time, 40 after the loop.
        assert_eq!(shown[164], "rider2-2");
        assert!(shown[165..].iter().all(|&r| r == "rider2-1"));
    }

    #[test]
    fn the_second_characters_jump_shows_the_spec_example() {
        // P = 0, T = 5, F = 22, S1 = 24, L = 36.
        let mut clock = second();
        run(&mut clock, frame(RidePart::Riding, 0, 0));
        let mut shown = Vec::new();
        for age in 0..36u8 {
            let mut f = frame(RidePart::Jump(age), 0, 0);
            f.falling = age >= 22;
            shown.push(run(&mut clock, f));
        }
        for age in 0..8 {
            shown.push(run(&mut clock, frame(RidePart::Landing(age), 0, 0)));
        }
        let at = |n: usize| (shown[n].rider, shown[n].d);
        assert_eq!(at(0), ("rider2-10", 2));
        assert_eq!(at(4), ("rider2-10", 2));
        assert_eq!(at(5), ("rider2-11", 0));
        assert_eq!(at(7), ("rider2-11", 0));
        assert_eq!(at(8), ("rider2-11", -2));
        assert_eq!(at(10), ("rider2-11", -2));
        assert_eq!(at(11), ("rider2-12", -4));
        assert_eq!(at(21), ("rider2-12", -4));
        assert_eq!(at(22), ("rider2-13", -3));
        assert_eq!(at(23), ("rider2-14", -2));
        assert_eq!(at(35), ("rider2-14", -2));
        assert_eq!(at(36), ("rider2-13", 2));
        assert_eq!(at(38), ("rider2-13", 2));
        assert_eq!(at(39), ("rider2-11", 3));
        assert_eq!(at(43), ("rider2-11", 3));
        // The animal's pictures are the first character's.
        assert_eq!(shown[24].animal, "ride-jump-5");
        // The height stays 3 after the jump.
        assert_eq!(run(&mut clock, frame(RidePart::Riding, 0, 0)).d, 3);
    }

    #[test]
    fn the_second_characters_charge_and_the_standing_after_it() {
        let mut clock = second();
        run(&mut clock, frame(RidePart::Riding, 0, 0));
        let mut shown = Vec::new();
        for age in 0..28 {
            let part = RidePart::Charge {
                age,
                steered: false,
            };
            shown.push(run(&mut clock, frame(part, 0, 0)));
        }
        for age in 0..12 {
            let part = RidePart::ChargeEnd {
                age,
                steered: false,
            };
            shown.push(run(&mut clock, frame(part, 0, 0)));
        }
        let at = |n: usize| (shown[n].rider, shown[n].d);
        assert_eq!(at(0), ("rider2-10", 4));
        assert_eq!(at(1), ("rider2-10", 4));
        assert_eq!(at(2), ("rider2-11", 4));
        assert_eq!(at(5), ("rider2-12", 4));
        assert_eq!(at(9), ("rider2-13", 4));
        assert_eq!(at(10), ("rider2-13", 1));
        assert_eq!(at(11), ("rider2-14", 1));
        assert_eq!(at(26), ("rider2-14", 1));
        assert_eq!(at(27).0, "rider2-13");
        assert_eq!(at(30).0, "rider2-12");
        assert_eq!(at(33).0, "rider2-11");
        assert_eq!(at(35).0, "rider2-11");
        assert_eq!(at(36).0, "rider2-1");
        assert_eq!(at(39).0, "rider2-1");
        // Standing starts on the 5th frame of the loop's first entry.
        let after: Vec<_> = (0..37)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 0, 0)).rider)
            .collect();
        assert!(after[..36].iter().all(|&r| r == "rider2-1"));
        assert_eq!(after[36], "rider2-2");
    }

    #[test]
    fn the_second_characters_moving_cycle_is_six_frames_a_picture() {
        let mut clock = second();
        let shown: Vec<_> = (0..40)
            .map(|_| run(&mut clock, frame(RidePart::Riding, 1, 512)).rider)
            .collect();
        assert!(shown[..5].iter().all(|&r| r == "rider2-6"));
        assert!(shown[5..11].iter().all(|&r| r == "rider2-7"));
        assert!(shown[11..17].iter().all(|&r| r == "rider2-8"));
        assert!(shown[17..23].iter().all(|&r| r == "rider2-9"));
        assert!(shown[23..29].iter().all(|&r| r == "rider2-8"));
        assert!(shown[29..35].iter().all(|&r| r == "rider2-7"));
        assert_eq!(shown[35], "rider2-6");
    }
}
