//! Swimming in Classic 2 (spec/behavior/swimming.md, "In `title-2`"), in
//! open water: the faster targets after Action, the Up stroke, the
//! deeper sinking limit kept by Up, and facing through turns.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::Fixed;
use clamber_sim::{Event, Facing, InputFrame, State, World, generic_feel, step};

/// A level that is all water, with no ground.
fn open() -> Play {
    let src = r#"
        format = 1
        name = "pool"
        size = [4096, 2000]
        start = [1000, 1000]

        [[regions]]
        rect = [0, 0, 4096, 2000]
        water = true
        "#;
    let level = Arc::new(Level::from_toml_str(src, "pool").unwrap());
    let feel = generic_feel("classic-2").unwrap();
    let world = World::new(level, Arc::new(feel));
    assert_eq!(world.player.state, State::Swim);
    Play {
        world,
        held: Actions::NONE,
        turned: false,
    }
}

struct Play {
    world: World,
    held: Actions,
    /// The last step had a `turn` event.
    turned: bool,
}

impl Play {
    /// One step with `keys` held, after what was held on the last.
    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        self.turned = self.world.events.contains(&Event::Turn);
    }

    /// `vx` and `vy` in 1/256 px per frame.
    fn v(&self) -> (i64, i64) {
        let v = self.world.player.vel;
        (n256(v.x), n256(v.y))
    }

    fn facing(&self) -> Facing {
        self.world.player.facing
    }

    /// Sets the speeds `(vx, vy)` in 1/256 px per frame.
    fn speeds(&mut self, vx: i64, vy: i64) {
        let p = &mut self.world.player;
        p.vel.x = Fixed::from_bits(vx << 24);
        p.vel.y = Fixed::from_bits(vy << 24);
    }

    /// Steps through `rows` of keys held and the `vx`, `vy` and facing
    /// after each, checking that `x` and `y` change by the speeds.
    fn rows(&mut self, rows: &[(&[Action], i64, i64, Facing)]) {
        for (i, &(keys, vx, vy, facing)) in rows.iter().enumerate() {
            let p = &self.world.player;
            let (x, y) = (p.pos.x, p.pos.y);
            self.step(keys);
            assert_eq!(self.v(), (vx, vy), "row {i}");
            assert_eq!(self.facing(), facing, "row {i}");
            let p = &self.world.player;
            assert_eq!(p.pos.x - x, Fixed::from_bits(vx << 24), "row {i}");
            assert_eq!(p.pos.y - y, Fixed::from_bits(vy << 24), "row {i}");
        }
    }
}

/// `v` in 1/256 px per frame, which it's always a whole number of.
fn n256(v: Fixed) -> i64 {
    assert_eq!(v.to_bits() % (1 << 24), 0, "{v:?}");
    v.to_bits() >> 24
}

const R: Action = Action::Right;
const L: Action = Action::Left;
const U: Action = Action::Up;
const D: Action = Action::Down;
const J: Action = Action::Jump;
const A: Action = Action::Action;
const FR: Facing = Facing::Right;
const FL: Facing = Facing::Left;

#[test]
fn targets_of_one_and_a_half_or_two_and_a_half_after_action() {
    let mut play = open();
    let vxs: Vec<_> = (0..8)
        .map(|_| {
            play.step(&[R]);
            play.v().0
        })
        .collect();
    assert_eq!(vxs, [12, 23, 34, 44, 54, 64, 74, 83]);
    while play.v().0 != 384 {
        play.step(&[R]);
    }

    let mut play = open();
    let vxs: Vec<_> = (0..8)
        .map(|_| {
            play.step(&[R, A]);
            play.v().0
        })
        .collect();
    assert_eq!(vxs, [12, 31, 50, 68, 85, 102, 118, 134]);
    while play.v().0 != 640 {
        play.step(&[R, A]);
    }
}

#[test]
fn action_with_no_direction_on_the_frame_before_doesnt_count() {
    let mut play = open();
    play.step(&[A]);
    play.step(&[R, A]);
    assert_eq!(play.v().0, 12);
    play.step(&[R]);
    assert_eq!(play.v().0, 12 + (640 - 12) / 32);
}

#[test]
fn worked_values_speed_and_strokes() {
    // As after P - 1, with nothing held.
    let mut play = open();
    play.speeds(77, 148);
    play.rows(&[
        (&[J, R], 86, -500, FR),
        (&[R], 95, -488, FR),
        (&[R], 104, -476, FR),
        (&[R], 112, -464, FR),
        (&[R], 120, -452, FR),
        (&[R, A], 128, -440, FR),
        (&[R, A], 144, -428, FR),
        (&[R, A], 159, -416, FR),
        (&[J, U, R, A], 174, -692, FR),
        (&[R], 188, -680, FR),
        (&[R], 194, -668, FR),
    ]);
}

#[test]
fn worked_values_a_turn_with_a_stroke() {
    let mut play = open();
    play.speeds(65, -188);
    play.rows(&[
        (&[L], 51, -176, FR),
        (&[L], 38, -164, FR),
        (&[J, L], 25, -500, FR),
        (&[L], 13, -488, FL),
        (&[L], 1, -476, FL),
        (&[L], -11, -464, FL),
        (&[L], -22, -452, FL),
        (&[R], -10, -440, FL),
        (&[R], 2, -428, FL),
        (&[R], 13, -416, FL),
        (&[R], 24, -404, FL),
        (&[R], 35, -392, FL),
        (&[R], 45, -380, FR),
    ]);
}

#[test]
fn worked_values_up_after_down_keeps_the_deeper_limit() {
    let mut play = open();
    play.speeds(0, 544);
    let rows: [(&[Action], i64); 9] = [
        (&[D], 556),
        (&[D], 568),
        (&[D], 576),
        (&[U], 576),
        (&[U], 576),
        (&[U], 576),
        (&[U], 576),
        (&[], 320),
        (&[], 320),
    ];
    for (i, (keys, vy)) in rows.into_iter().enumerate() {
        play.step(keys);
        assert_eq!(play.v(), (0, vy), "Q - 2 + {i}");
    }
    // Up after a frame with neither doesn't bring the deeper limit back.
    play.step(&[U]);
    assert_eq!(play.v().1, 320);
}

#[test]
fn up_alone_keeps_the_usual_limit() {
    let mut play = open();
    play.speeds(0, 400);
    play.step(&[U]);
    assert_eq!(play.v().1, 320);
}

/// Facing right in open water, holds Left from frame S to S + 5 and Right
/// after that, with a stroke on `stroke` frames after S if any. The
/// frames after S on which facing changes.
fn turns(stroke: Option<u32>) -> Vec<u32> {
    let mut play = open();
    assert_eq!(play.facing(), FR);
    let mut changes = Vec::new();
    for n in 0..40 {
        let dir = if n <= 5 { L } else { R };
        if stroke == Some(n) {
            play.step(&[dir, J]);
        } else {
            play.step(&[dir]);
        }
        let facing = if changes.len() % 2 == 0 { FR } else { FL };
        assert_eq!(play.turned, play.facing() != facing, "S + {n}");
        if play.turned {
            changes.push(n);
        }
    }
    changes
}

#[test]
fn facing_changes_five_frames_into_a_twelve_frame_turn() {
    assert_eq!(turns(None), [5, 17]);
    // A stroke on S itself changes nothing.
    assert_eq!(turns(Some(0)), [5, 17]);
}

#[test]
fn a_stroke_during_a_turn_shortens_it() {
    // By the frame of the stroke after S: when facing changes (F) and
    // when a new turn can start (N), as offsets from S.
    let table = [
        (1, 2, 7),
        (2, 3, 7),
        (3, 4, 8),
        (4, 4, 9),
        (5, 5, 9),
        (6, 5, 10),
        (7, 5, 11),
        (8, 5, 11),
        (9, 5, 12),
        (10, 5, 10),
        (11, 5, 11),
    ];
    for (j, f, n) in table {
        assert_eq!(turns(Some(j)), [f, n + 5], "stroke on S + {j}");
    }
}

#[test]
fn a_turn_finishes_once_started() {
    let mut play = open();
    play.step(&[L]);
    for n in 1..5 {
        play.step(&[]);
        assert_eq!(play.facing(), FR, "S + {n}");
    }
    play.step(&[]);
    assert!(play.turned);
    assert_eq!(play.facing(), FL);
    // The held direction the same as facing starts nothing.
    for _ in 0..20 {
        play.step(&[L]);
        assert!(!play.turned);
    }
}
