//! Which of its own pictures a walker shows on each frame
//! (`spec/behavior/walker.md`, "Pictures").
//!
//! A walker entity's `pictures` param names its kind's pictures:
//! `small-walker`, `medium-walker` or `large-walker` ([`Size`]). The
//! pictures come from the pack's sheet of that name, by their labels in
//! `walker.md`. [`WalkerClock`] follows the walker frame by frame and
//! gives the label for the frame, with the group it belongs to and the
//! way the picture faces; the walker is drawn with it only if the sheet
//! has the whole group ([`labels`]), and with the engine's own art,
//! facing that way, otherwise.
//!
//! Pictures step on counted frames: those on which the walker took a
//! step ([`Thing::still`]).
//!
//! Engine choices where the spec says nothing:
//!
//! - A walker shows its pictures only while it walks, turns, is pushed
//!   or is defeated; carried, thrown or anything else, it shows the
//!   engine's own art.
//! - Only large walkers are pushed; small and medium ones keep walking.
//! - A walker knocked back by a thrown object goes on walking on the
//!   frame it's hit, and is defeated from the next, when its `vy`
//!   becomes −6.

use clamber_fixed::Fixed;
use clamber_sim::{Behavior, Facing, Phase, Thing};

/// A walker kind, which has its own pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    /// The kind a `pictures` param names.
    pub fn named(name: &str) -> Option<Size> {
        match name {
            "small-walker" => Some(Size::Small),
            "medium-walker" => Some(Size::Medium),
            "large-walker" => Some(Size::Large),
            _ => None,
        }
    }

    /// The kind's name, which is also its sheet's.
    pub fn name(self) -> &'static str {
        match self {
            Size::Small => "small-walker",
            Size::Medium => "medium-walker",
            Size::Large => "large-walker",
        }
    }
}

/// The pictures shown together, each shown only when all of it is
/// there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Walking,
    Turn,
    /// A large walker's picture while pushed.
    Push,
    Defeat,
}

/// Each group's labels, by kind.
pub fn labels(size: Size, group: Group) -> &'static [&'static str] {
    match (size, group) {
        (Size::Small, Group::Walking) => &[
            "small-walk-1",
            "small-walk-2",
            "small-walk-3",
            "small-walk-4",
            "small-walk-5",
            "small-walk-6",
            "small-walk-7",
            "small-walk-8",
        ],
        (Size::Medium, Group::Walking) => &[
            "medium-walk-1",
            "medium-walk-2",
            "medium-walk-3",
            "medium-walk-4",
            "medium-walk-5",
            "medium-walk-6",
            "medium-walk-7",
            "medium-walk-8",
        ],
        (Size::Large, Group::Walking) => &[
            "large-walk-1",
            "large-walk-2",
            "large-walk-3",
            "large-walk-4",
            "large-walk-5",
            "large-walk-6",
            "large-walk-7",
            "large-walk-8",
        ],
        (Size::Small, Group::Turn) => &["small-turn-1", "small-turn-2"],
        (Size::Medium, Group::Turn) => &["medium-turn-1", "medium-turn-2"],
        (Size::Large, Group::Turn) => &["large-turn"],
        (Size::Large, Group::Push) => &["large-push"],
        (_, Group::Push) => &[],
        (Size::Small, Group::Defeat) => &[
            "small-defeat-1",
            "small-defeat-2",
            "small-defeat-3",
            "small-defeat-4",
            "small-defeat-5",
        ],
        (Size::Medium, Group::Defeat) => &[
            "medium-defeat-1",
            "medium-defeat-2",
            "medium-defeat-3",
            "medium-defeat-4",
            "medium-defeat-5",
            "medium-defeat-6",
            "medium-defeat-7",
            "medium-defeat-8",
        ],
        (Size::Large, Group::Defeat) => &[
            "large-defeat-1",
            "large-defeat-2",
            "large-defeat-3",
            "large-defeat-4",
            "large-defeat-5",
        ],
    }
}

/// Counted frames on each defeat picture but the last, which is held.
const SMALL_DEFEAT: [u16; 4] = [14, 3, 4, 4];
const MEDIUM_DEFEAT: [u16; 7] = [14, 2, 3, 3, 3, 3, 3];

/// Counted frames on each walking picture.
const WALK_STEP: u16 = 4;
/// Counted frames of a turn's pictures.
const TURN_FRAMES: u8 = 8;
/// The turn frame the facing changes on, from 0.
const TURN_FACING: u8 = 4;

/// The speed at or below which a pushed large walker walks again:
/// 12/256 px per frame.
fn eased() -> Fixed {
    Fixed::from_bits(Fixed::ONE.to_bits() * 12 / 256)
}

/// What the walker is doing, as far as its pictures go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Doing {
    /// Counted frames into the walking cycle; after a push, the turn
    /// waits for the first 4.
    Walking {
        c: u16,
        after_push: bool,
    },
    /// Counted frames into a turn, from U.
    Turning(u8),
    Pushed,
    /// Counted frames since it was defeated.
    Defeated(u16),
}

/// Follows a walker to say which of its pictures it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalkerClock {
    doing: Option<Doing>,
    /// The way the picture faces.
    facing: Option<Facing>,
    /// The walker's turn count on the frame before.
    turn: u8,
    /// What it shows on this frame: its kind, group and label.
    shown: Option<(Size, Group, &'static str)>,
}

impl WalkerClock {
    /// The kind, group and label of the picture the walker shows on this
    /// frame, if it's one of these.
    pub fn shown(&self) -> Option<(Size, Group, &'static str)> {
        self.shown
    }

    /// The way the walker's picture faces, if it's followed.
    pub fn facing(&self) -> Option<Facing> {
        self.facing.filter(|_| self.shown.is_some())
    }

    /// Moves on to `thing`, a walker of kind `size`, as it is after a
    /// logic frame.
    pub fn update(&mut self, thing: &Thing, size: Size) {
        let Some(Behavior::Walker { turn_delay, .. }) = thing.archetype.map(|a| a.behavior) else {
            *self = WalkerClock::default();
            return;
        };
        if !thing.active {
            *self = WalkerClock::default();
            return;
        }
        let turn = std::mem::replace(&mut self.turn, thing.turn);
        let counted = !thing.still;
        let defeated = matches!(
            thing.phase,
            Phase::Falling { .. } | Phase::Knocked { vy: Some(_) } | Phase::Defeated(_)
        );
        let walking = matches!(thing.phase, Phase::Ready | Phase::Knocked { vy: None });
        let facing = self.facing.unwrap_or(thing.facing);
        let step = |n: u16| if counted { n.saturating_add(1) } else { n };
        // A turn's pictures start on U, the first frame its speed steps
        // toward the new heading: the facing follows `turn_delay`
        // frames later.
        let turn_starts = turn_delay > 0 && thing.turn == turn_delay && turn != turn_delay;
        let pushed = size == Size::Large
            && matches!(thing.phase, Phase::Ready)
            && thing.vx.abs() > Fixed::ONE;
        let doing = match self.doing {
            _ if defeated => match self.doing {
                Some(Doing::Defeated(d)) => Doing::Defeated(step(d)),
                _ => Doing::Defeated(0),
            },
            _ if !walking => {
                *self = WalkerClock {
                    turn: thing.turn,
                    ..WalkerClock::default()
                };
                return;
            }
            // A walker that appears shows `walk-1` on its first 3 frames.
            None => Doing::Walking {
                c: 1,
                after_push: false,
            },
            Some(Doing::Pushed) if thing.vx.abs() <= eased() => Doing::Walking {
                c: 0,
                after_push: true,
            },
            Some(Doing::Pushed) => Doing::Pushed,
            _ if pushed => Doing::Pushed,
            Some(Doing::Walking {
                c,
                after_push: true,
            }) => match step(c) {
                WALK_STEP if thing.heading != facing => Doing::Turning(0),
                c => Doing::Walking {
                    c,
                    after_push: c < WALK_STEP,
                },
            },
            _ if turn_starts => Doing::Turning(0),
            Some(Doing::Turning(t)) => match u16::from(t) + u16::from(counted) {
                t if t >= u16::from(TURN_FRAMES) => Doing::Walking {
                    c: 0,
                    after_push: false,
                },
                t => Doing::Turning(t as u8),
            },
            Some(Doing::Walking { c, .. }) => Doing::Walking {
                c: step(c) % (WALK_STEP * 8),
                after_push: false,
            },
            Some(Doing::Defeated(d)) => Doing::Defeated(d),
        };
        self.facing = Some(match doing {
            // The facing changes on the turn's 5th frame.
            Doing::Turning(TURN_FACING) if self.doing != Some(doing) => turned(facing),
            // From the frame it's defeated, the facing doesn't change.
            Doing::Defeated(_) if !matches!(self.doing, Some(Doing::Defeated(_))) => thing.facing,
            Doing::Walking {
                after_push: false, ..
            } => thing.facing,
            _ => facing,
        });
        self.doing = Some(doing);
        self.shown = Some(picture(size, doing));
    }
}

fn turned(facing: Facing) -> Facing {
    match facing {
        Facing::Left => Facing::Right,
        Facing::Right => Facing::Left,
    }
}

/// The picture for what the walker is doing.
fn picture(size: Size, doing: Doing) -> (Size, Group, &'static str) {
    let (group, i) = match doing {
        Doing::Walking { c, .. } => (Group::Walking, usize::from(c / WALK_STEP % 8)),
        Doing::Turning(t) => match size {
            Size::Large => (Group::Turn, 0),
            // `turn-1` × 2, `turn-2` × 4, `turn-1` × 2.
            _ => (Group::Turn, usize::from((2..6).contains(&t))),
        },
        Doing::Pushed => (Group::Push, 0),
        Doing::Defeated(d) => {
            let counts: &[u16] = match size {
                Size::Medium => &MEDIUM_DEFEAT,
                _ => &SMALL_DEFEAT,
            };
            let mut left = d;
            let i = counts
                .iter()
                .position(|&n| {
                    let here = left < n;
                    left = left.saturating_sub(n);
                    here
                })
                .unwrap_or(counts.len());
            (Group::Defeat, i)
        }
    };
    (size, group, labels(size, group)[i])
}

#[cfg(test)]
mod tests {
    use clamber_level::Vec2;
    use clamber_sim::Archetypes;

    use super::*;

    fn walker(facing: Facing) -> Thing {
        let mut archetype = Archetypes::builtin().named("crawler").unwrap();
        if let Behavior::Walker { turn_delay, .. } = &mut archetype.behavior {
            *turn_delay = 4;
        }
        Thing {
            index: 0,
            kind: "crawler".into(),
            pos: Vec2::new(Fixed::ZERO, Fixed::ZERO),
            active: true,
            archetype: Some(archetype),
            facing,
            heading: facing,
            turn: 0,
            vx: Fixed::ZERO,
            clock: 0,
            wait: 0,
            air: None,
            phase: Phase::Ready,
            still: false,
        }
    }

    /// Steps `clock` on `thing` and gives the label and facing shown.
    fn step(clock: &mut WalkerClock, thing: &Thing, size: Size) -> (&'static str, Facing) {
        clock.update(thing, size);
        (clock.shown().unwrap().2, clock.facing().unwrap())
    }

    fn runs(frames: &[(&'static str, Facing)]) -> Vec<(&'static str, Facing, usize)> {
        let mut out: Vec<(&'static str, Facing, usize)> = Vec::new();
        for &(label, facing) in frames {
            match out.last_mut() {
                Some((l, f, n)) if *l == label && *f == facing => *n += 1,
                _ => out.push((label, facing, 1)),
            }
        }
        out
    }

    #[test]
    fn a_walker_that_appears_shows_walk_1_on_its_first_3_frames() {
        let thing = walker(Facing::Left);
        let mut clock = WalkerClock::default();
        let frames: Vec<_> = (0..39)
            .map(|_| step(&mut clock, &thing, Size::Small))
            .collect();
        let labels: Vec<_> = runs(&frames).iter().map(|r| (r.0, r.2)).collect();
        assert_eq!(
            labels,
            [
                ("small-walk-1", 3),
                ("small-walk-2", 4),
                ("small-walk-3", 4),
                ("small-walk-4", 4),
                ("small-walk-5", 4),
                ("small-walk-6", 4),
                ("small-walk-7", 4),
                ("small-walk-8", 4),
                ("small-walk-1", 4),
                ("small-walk-2", 4),
            ]
        );
    }

    #[test]
    fn frames_it_holds_still_on_dont_count() {
        let mut thing = walker(Facing::Left);
        let mut clock = WalkerClock::default();
        for _ in 0..3 {
            step(&mut clock, &thing, Size::Small);
        }
        thing.still = true;
        for _ in 0..10 {
            assert_eq!(
                step(&mut clock, &thing, Size::Small).0,
                "small-walk-1",
                "held"
            );
        }
        thing.still = false;
        assert_eq!(step(&mut clock, &thing, Size::Small).0, "small-walk-2");
    }

    /// walker.md, "Pictures", "Example (synthetic)": a small walker
    /// heading left turns at the end of its patrol on frame 7, having
    /// shown `small-walk-6` on frames 1 to 4.
    #[test]
    fn the_turn_example() {
        let mut thing = walker(Facing::Left);
        let mut clock = WalkerClock::default();
        // Into the cycle so that frame 1 is the first of `small-walk-6`.
        for _ in 0..19 {
            step(&mut clock, &thing, Size::Small);
        }
        let mut frames = Vec::new();
        for frame in 1..=22 {
            // The patrol's T: the heading changes, and the facing
            // follows on T + 4.
            if frame == 7 {
                thing.heading = Facing::Right;
                thing.turn = 4;
            } else if thing.turn > 0 {
                thing.turn -= 1;
                if thing.turn == 0 {
                    thing.facing = thing.heading;
                }
            }
            frames.push(step(&mut clock, &thing, Size::Small));
        }
        use Facing::{Left, Right};
        assert_eq!(
            runs(&frames),
            [
                ("small-walk-6", Left, 4),
                ("small-walk-7", Left, 2),
                ("small-turn-1", Left, 2),
                ("small-turn-2", Left, 2),
                ("small-turn-2", Right, 2),
                ("small-turn-1", Right, 2),
                ("small-walk-1", Right, 4),
                ("small-walk-2", Right, 4),
            ]
        );
    }

    /// walker.md, "Turning at a barrier": the turn's pictures start on
    /// S + 1, and the facing changes on S + 5.
    #[test]
    fn a_turn_at_a_barrier_starts_on_the_frame_after() {
        let mut thing = walker(Facing::Left);
        let mut clock = WalkerClock::default();
        step(&mut clock, &thing, Size::Large);
        // S: stopped, turning with the facing 5 frames later.
        thing.heading = Facing::Right;
        thing.turn = 5;
        let mut frames = vec![step(&mut clock, &thing, Size::Large)];
        for _ in 0..12 {
            thing.turn -= u8::from(thing.turn > 0);
            if thing.turn == 0 {
                thing.facing = thing.heading;
            }
            frames.push(step(&mut clock, &thing, Size::Large));
        }
        use Facing::{Left, Right};
        assert_eq!(
            runs(&frames),
            [
                ("large-walk-1", Left, 1),
                ("large-turn", Left, 4),
                ("large-turn", Right, 4),
                ("large-walk-1", Right, 4),
            ]
        );
    }

    /// walker.md, "Pictures", "Defeated", and the example: a large
    /// walker bounced on shows `large-defeat-1` on B, the 2 frozen frames
    /// and 13 more, then `large-defeat-2` on B + 16 to B + 18.
    #[test]
    fn the_defeat_lists() {
        for (size, expected) in [
            (
                Size::Large,
                &[
                    ("large-defeat-1", 16),
                    ("large-defeat-2", 3),
                    ("large-defeat-3", 4),
                    ("large-defeat-4", 4),
                    ("large-defeat-5", 20),
                ][..],
            ),
            (
                Size::Medium,
                &[
                    ("medium-defeat-1", 16),
                    ("medium-defeat-2", 2),
                    ("medium-defeat-3", 3),
                    ("medium-defeat-4", 3),
                    ("medium-defeat-5", 3),
                    ("medium-defeat-6", 3),
                    ("medium-defeat-7", 3),
                    ("medium-defeat-8", 14),
                ][..],
            ),
        ] {
            let mut thing = walker(Facing::Right);
            let mut clock = WalkerClock::default();
            step(&mut clock, &thing, size);
            thing.phase = Phase::Falling {
                vy: Fixed::from_int(-6),
                hold: 2,
                glide: None,
            };
            let mut frames = Vec::new();
            for b in 0..47 {
                thing.still = b == 1 || b == 2;
                // Its facing doesn't change from B on.
                thing.facing = if b > 5 { Facing::Left } else { Facing::Right };
                frames.push(step(&mut clock, &thing, size));
            }
            let runs: Vec<_> = runs(&frames).iter().map(|r| (r.0, r.2)).collect();
            assert_eq!(runs, expected, "{size:?}");
            assert_eq!(clock.facing(), Some(Facing::Right));
        }
    }

    /// walker.md, "Pictures", "Pushed": `large-push` until its speed has
    /// eased to 12/256 or less, then `large-walk-1` for 4 frames, and the
    /// turn right after them if it heads away from its facing.
    #[test]
    fn a_pushed_large_walker() {
        let mut thing = walker(Facing::Left);
        let mut clock = WalkerClock::default();
        step(&mut clock, &thing, Size::Large);
        thing.vx = Fixed::from_int(3);
        thing.heading = Facing::Right;
        let mut frames = Vec::new();
        for speed in [3.0, 2.0, 1.0, 0.5, 0.125, 0.046875, 0.5, 1.0, 1.0, 1.0] {
            thing.vx = Fixed::from_f64(speed).unwrap();
            frames.push(step(&mut clock, &thing, Size::Large));
        }
        thing.facing = Facing::Right;
        for _ in 0..9 {
            frames.push(step(&mut clock, &thing, Size::Large));
        }
        use Facing::{Left, Right};
        assert_eq!(
            runs(&frames),
            [
                ("large-push", Left, 5),
                ("large-walk-1", Left, 4),
                ("large-turn", Left, 4),
                ("large-turn", Right, 4),
                ("large-walk-1", Right, 2),
            ]
        );
    }

    #[test]
    fn a_small_walker_isnt_pushed() {
        let mut thing = walker(Facing::Left);
        let mut clock = WalkerClock::default();
        thing.vx = Fixed::from_int(3);
        assert_eq!(step(&mut clock, &thing, Size::Small).0, "small-walk-1");
    }

    #[test]
    fn the_labels_are_the_specs() {
        let path = format!(
            "{}/../../spec/behavior/walker.md",
            env!("CARGO_MANIFEST_DIR")
        );
        let spec = std::fs::read_to_string(&path).unwrap();
        let (_, table) = spec.split_once("| Picture | Picture hash").unwrap();
        let spec: Vec<&str> = table
            .lines()
            .skip(2)
            .take_while(|l| l.starts_with('|'))
            .map(|l| l.split('|').nth(1).unwrap().trim().trim_matches('`'))
            .collect();
        let mut ours: Vec<&str> = Vec::new();
        for size in [Size::Small, Size::Medium, Size::Large] {
            for group in [Group::Walking, Group::Turn, Group::Push, Group::Defeat] {
                ours.extend(labels(size, group));
            }
        }
        ours.sort();
        let mut spec = spec;
        spec.sort();
        assert_eq!(ours, spec);
    }
}
