//! Which of their own pictures a thrower and its balls show on each frame
//! (`spec/behavior/thrower.md`, "Pictures").
//!
//! The pictures come from the pack's [`THROWER`] and [`BALL`] sheets, by
//! their labels in `thrower.md`. [`shown`] gives the label for a thing
//! after a logic frame, with the group it belongs to and its mirroring;
//! the thing is drawn with it only if the sheet has the whole group
//! ([`labels`]), and with the engine's own art otherwise.
//!
//! Both follow the thing's clock, so they keep their picture on the
//! frames it holds still. A thrower's clock counts the steps since it
//! appeared, and its pictures go with its timetable: on the two frames
//! before each throw `throw-1` and `throw-2`, then the throw's own
//! pictures, then, until the next, the pictures after a throw to the
//! right or, for any other throw, `sway-2`; a list longer than the time
//! to the next throw is cut short, and its last picture is held when the
//! time is longer. Knocked out, it shows `thrower-out`. A ball's clock
//! counts its steps.
//!
//! Engine choices where the spec says nothing: a thrower knocked out by
//! the ground action or an object shows `thrower-out` too, and a ball
//! placed in a level shows the pictures a thrown one does, `ball-1`
//! before it starts moving. The engine's own art keeps a ball's
//! mirroring, and faces the way the thrower does.

use clamber_fixed::Fixed;
use clamber_sim::{Behavior, Phase, Thing};

/// The sheet a thrower's pictures come from.
pub const THROWER: &str = "thrower-pictures";
/// The sheet a ball's pictures come from.
pub const BALL: &str = "ball-pictures";

/// The pictures shown together, each shown only when all of it is
/// there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// A thrower's pictures before it's knocked out.
    Thrower,
    /// A knocked-out thrower's.
    Out,
    Ball,
}

impl Group {
    /// The sheet the group's pictures come from.
    pub fn sheet(self) -> &'static str {
        match self {
            Group::Thrower | Group::Out => THROWER,
            Group::Ball => BALL,
        }
    }
}

/// Each group's labels.
pub fn labels(group: Group) -> &'static [&'static str] {
    match group {
        Group::Thrower => &[
            "throw-1", "throw-2", "throw-3", "throw-4", "throw-5", "sway-1", "sway-2", "sway-3",
            "sway-4", "sway-5",
        ],
        Group::Out => &["thrower-out"],
        Group::Ball => &["ball-1", "ball-2", "ball-3"],
    }
}

/// What a thing shows on a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub group: Group,
    pub label: &'static str,
    /// Mirrored left to right.
    pub flip_x: bool,
    /// Mirrored top to bottom.
    pub flip_y: bool,
}

/// From a throw's frame `T` on: 27 frames, to `T` + 26.
const THROW: &[(&str, u32)] = &[
    ("throw-3", 8),
    ("throw-4", 1),
    ("throw-5", 1),
    ("sway-1", 2),
    ("sway-2", 3),
    ("sway-3", 4),
    ("sway-2", 4),
    ("sway-1", 4),
];

/// After a throw to the right, until `throw-1` before the next.
const AFTER_RIGHT: &[(&str, u32)] = &[
    ("sway-2", 7),
    ("sway-3", 7),
    ("sway-4", 7),
    ("sway-5", 36),
    ("sway-4", 8),
    ("sway-3", 8),
    ("sway-2", 8),
    ("sway-1", 1),
];

/// After any other throw.
const AFTER: &[(&str, u32)] = &[("sway-2", 1)];

/// A ball's pictures by its steps, 3 each: label, left–right and
/// top–bottom mirroring.
const BALL_TURN: [(&str, bool, bool); 8] = [
    ("ball-1", false, false),
    ("ball-2", false, false),
    ("ball-3", false, false),
    ("ball-2", false, true),
    ("ball-1", false, true),
    ("ball-2", true, true),
    ("ball-3", true, true),
    ("ball-2", true, false),
];

/// Steps on each of a ball's pictures.
const BALL_STEP: u32 = 3;

/// What `thing` shows after a logic frame, if it's a thrower or a ball
/// with pictures for it.
pub fn shown(thing: &Thing) -> Option<Shown> {
    let still = |group, label| Shown {
        group,
        label,
        flip_x: false,
        flip_y: false,
    };
    match thing.archetype?.behavior {
        Behavior::Thrower { .. } if thing.phase != Phase::Ready => {
            Some(still(Group::Out, "thrower-out"))
        }
        Behavior::Thrower {
            period,
            throws,
            throw_count,
            ..
        } => {
            // Its clock has already moved on for this frame: the frame
            // it appeared on shows the timetable's frame 0.
            let n = thing.clock.saturating_sub(1) % period.max(1);
            let throws = throws[..usize::from(throw_count)]
                .iter()
                .map(|t| (t.frame, t.velocity.x > Fixed::ZERO));
            Some(still(Group::Thrower, thrower(n, period, throws)?))
        }
        Behavior::Ball(_) => {
            let (label, flip_x, flip_y) =
                BALL_TURN[(thing.clock / BALL_STEP) as usize % BALL_TURN.len()];
            Some(Shown {
                group: Group::Ball,
                label,
                flip_x,
                flip_y,
            })
        }
        _ => None,
    }
}

/// A thrower's picture on frame `n` of its `period`, with `throws` on
/// their frames, each saying whether it's to the right. None without
/// throws.
fn thrower(n: u32, period: u32, throws: impl Iterator<Item = (u32, bool)>) -> Option<&'static str> {
    let period = period.max(1);
    // The throw whose `throw-1` came last, and how long ago.
    let (since, right) = throws
        .map(|(frame, right)| {
            let start = (frame % period + period - 2 % period) % period;
            ((n + period - start) % period, right)
        })
        .min_by_key(|&(since, _)| since)?;
    Some(match since {
        0 => "throw-1",
        1 => "throw-2",
        since => {
            let after = if right { AFTER_RIGHT } else { AFTER };
            let mut left = since - 2;
            let mut label = "throw-3";
            for &(l, n) in THROW.iter().chain(after) {
                label = l;
                if left < n {
                    break;
                }
                left -= n;
            }
            label
        }
    })
}

#[cfg(test)]
mod tests {
    use clamber_level::Vec2;
    use clamber_sim::{Archetypes, Facing};

    use super::*;

    fn thing(kind: &str) -> Thing {
        Thing {
            index: 0,
            kind: kind.into(),
            pos: Vec2::new(Fixed::ZERO, Fixed::ZERO),
            active: true,
            archetype: Some(Archetypes::builtin().named(kind).unwrap()),
            facing: Facing::Left,
            heading: Facing::Left,
            turn: 0,
            vx: Fixed::ZERO,
            clock: 0,
            wait: 0,
            air: None,
            phase: Phase::Ready,
            still: false,
            pushed: false,
        }
    }

    fn runs(labels: impl Iterator<Item = &'static str>) -> Vec<(&'static str, usize)> {
        let mut out: Vec<(&'static str, usize)> = Vec::new();
        for label in labels {
            match out.last_mut() {
                Some((l, n)) if *l == label => *n += 1,
                _ => out.push((label, 1)),
            }
        }
        out
    }

    /// The thrower's pictures from its clock after each of `frames`
    /// steps from the one it appeared on.
    fn thrower_runs(frames: std::ops::Range<u32>) -> Vec<(&'static str, usize)> {
        let mut thrower = thing("thrower");
        runs(frames.map(|n| {
            thrower.clock = n + 1;
            let shown = shown(&thrower).unwrap();
            assert_eq!(shown.group, Group::Thrower);
            assert!(!shown.flip_x && !shown.flip_y);
            shown.label
        }))
    }

    /// thrower.md, "Pictures", "Example (synthetic)": appearing on frame
    /// 100, it throws up on 102, right on 133 and left on 244.
    #[test]
    fn the_example() {
        assert_eq!(
            thrower_runs(0..41),
            [
                ("throw-1", 1),
                ("throw-2", 1),
                ("throw-3", 8),
                ("throw-4", 1),
                ("throw-5", 1),
                ("sway-1", 2),
                ("sway-2", 3),
                ("sway-3", 4),
                ("sway-2", 4),
                ("sway-1", 4),
                ("sway-2", 2),
                ("throw-1", 1),
                ("throw-2", 1),
                ("throw-3", 8),
            ]
        );
    }

    /// After the throw to the right, the long sway; after the one to the
    /// left, `sway-2` × 2 again, and the cycle repeats every 173 frames.
    #[test]
    fn the_whole_cycle() {
        let throw = [
            ("throw-1", 1),
            ("throw-2", 1),
            ("throw-3", 8),
            ("throw-4", 1),
            ("throw-5", 1),
            ("sway-1", 2),
            ("sway-2", 3),
            ("sway-3", 4),
            ("sway-2", 4),
            ("sway-1", 4),
        ];
        let mut want = Vec::new();
        want.extend(throw);
        want.push(("sway-2", 2));
        want.extend(throw);
        want.extend([
            ("sway-2", 7),
            ("sway-3", 7),
            ("sway-4", 7),
            ("sway-5", 36),
            ("sway-4", 8),
            ("sway-3", 8),
            ("sway-2", 8),
            ("sway-1", 1),
        ]);
        want.extend(throw);
        want.push(("sway-2", 2));
        assert_eq!(want.iter().map(|r| r.1).sum::<usize>(), 173);
        assert_eq!(thrower_runs(0..173), want);
        assert_eq!(thrower_runs(173..346), want);
    }

    #[test]
    fn a_knocked_out_thrower() {
        let mut thrower = thing("thrower");
        thrower.phase = Phase::Defeated(3);
        let shown = shown(&thrower).unwrap();
        assert_eq!((shown.group, shown.label), (Group::Out, "thrower-out"));
        assert!(!shown.flip_x && !shown.flip_y);
    }

    /// A list longer than the time to the next throw is cut short, and
    /// the last picture held when it's longer.
    #[test]
    fn other_timetables() {
        let labels = |period, throws: &[(u32, bool)]| {
            runs((0..period).map(|n| thrower(n, period, throws.iter().copied()).unwrap()))
        };
        assert_eq!(
            labels(20, &[(2, true)]),
            [
                ("throw-1", 1),
                ("throw-2", 1),
                ("throw-3", 8),
                ("throw-4", 1),
                ("throw-5", 1),
                ("sway-1", 2),
                ("sway-2", 3),
                ("sway-3", 3),
            ]
        );
        assert_eq!(
            labels(40, &[(5, false)]),
            [
                ("sway-2", 3),
                ("throw-1", 1),
                ("throw-2", 1),
                ("throw-3", 8),
                ("throw-4", 1),
                ("throw-5", 1),
                ("sway-1", 2),
                ("sway-2", 3),
                ("sway-3", 4),
                ("sway-2", 4),
                ("sway-1", 4),
                ("sway-2", 8),
            ]
        );
        assert_eq!(thrower(0, 10, std::iter::empty()), None);
    }

    /// thrower.md, "Balls": 3 steps each, repeating every 24, with "The
    /// ball's turn"'s mirroring.
    #[test]
    fn a_balls_pictures_follow_its_steps() {
        let mut ball = thing("ball");
        let mut got = Vec::new();
        for s in 0..27 {
            ball.clock = s;
            let shown = shown(&ball).unwrap();
            assert_eq!(shown.group, Group::Ball);
            got.push((shown.label, shown.flip_x, shown.flip_y));
        }
        let want: Vec<_> = BALL_TURN
            .iter()
            .flat_map(|&p| [p; 3])
            .chain([("ball-1", false, false); 3])
            .collect();
        assert_eq!(got, want);
        // Upside down from step 9 to 20, mirrored from 15 to 23.
        assert_eq!(got[8], ("ball-3", false, false));
        assert_eq!(got[9], ("ball-2", false, true));
        assert_eq!(got[15], ("ball-2", true, true));
        assert_eq!(got[21], ("ball-2", true, false));
    }

    #[test]
    fn the_labels_are_the_specs() {
        let path = format!(
            "{}/../../spec/behavior/thrower.md",
            env!("CARGO_MANIFEST_DIR")
        );
        let spec = std::fs::read_to_string(&path).unwrap();
        let (_, table) = spec.split_once("| Picture | Picture hash").unwrap();
        let mut spec: Vec<&str> = table
            .lines()
            .skip(2)
            .take_while(|l| l.starts_with('|'))
            .map(|l| l.split('|').nth(1).unwrap().trim().trim_matches('`'))
            .collect();
        let mut ours: Vec<&str> = [Group::Thrower, Group::Out, Group::Ball]
            .into_iter()
            .flat_map(labels)
            .copied()
            .collect();
        ours.sort();
        spec.sort();
        assert_eq!(ours, spec);
    }
}
