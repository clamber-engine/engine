//! The ground action on `classic-1` at a ledge without a face, at steep
//! faces and on slopes (spec/behavior/ground-action.md, "At a ledge
//! without a face", "At a steep face" and "On a slope"). Heights in the
//! spec are relative to the starting floor; here that floor is at
//! y = 300.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Event, Facing, InputFrame, State, World, generic_feel, step};

use Action::{Action as A, Down as D, Jump as J, Left as L, Right as R};

const FLOOR: i32 = 300;

/// A level whose floor in column `c` is `height(c)` px below y = 300
/// (negative is above), or none, for columns 0 to 599, with the player
/// standing at `x`.
fn ground(height: impl Fn(i32) -> Option<i32>, x: f64) -> World {
    let mut solids = Vec::new();
    let mut points: Vec<[i32; 2]> = Vec::new();
    for c in 0..=600 {
        let h = if c < 600 { height(c) } else { None };
        match h {
            Some(h) => points.extend([[c, FLOOR + h], [c + 1, FLOOR + h]]),
            None if !points.is_empty() => {
                let end = points.last().unwrap()[0];
                let start = points[0][0];
                points.extend([[end, 590], [start, 590]]);
                points.dedup();
                solids.push(std::mem::take(&mut points));
            }
            None => {}
        }
    }
    let collision: String = solids
        .iter()
        .map(|p| format!("[[collision]]\nshape = \"polygon\"\npoints = {p:?}\n"))
        .collect();
    let start = FLOOR + height(x as i32).unwrap();
    let src = format!(
        "format = 1\nname = \"terrain\"\nsize = [600, 600]\nstart = [{x}, {start}]\n{collision}"
    );
    let level = Level::from_toml_str(&src, "terrain").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One frame of play and where it left the player, `x` and `y` relative
/// to where they were before the first frame.
#[derive(Debug)]
struct Row {
    state: State,
    events: Vec<Event>,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    facing: Facing,
    grounded: bool,
    crouching: bool,
}

impl Row {
    fn has(&self, event: Event) -> bool {
        self.events.contains(&event)
    }
}

/// Plays `n` frames from `w`, holding `held(frame)` on each.
fn play(w: &mut World, n: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    let (x0, y0) = (w.player.pos.x.to_f64(), w.player.pos.y.to_f64());
    let mut last = Actions::NONE;
    (0..n)
        .map(|frame| {
            let now: Actions = held(frame).into_iter().collect();
            step(w, &InputFrame::single(Frame::after(last, now)));
            last = now;
            let p = &w.player;
            Row {
                state: p.state,
                events: w.events.clone(),
                x: p.pos.x.to_f64() - x0,
                y: p.pos.y.to_f64() - y0,
                vx: p.vel.x.to_f64(),
                vy: p.vel.y.to_f64(),
                facing: p.facing,
                grounded: p.grounded(),
                crouching: p.crouch.held > 0,
            }
        })
        .collect()
}

/// The frames of `rows` in the ground action.
fn acting(rows: &[Row]) -> Vec<usize> {
    (0..rows.len())
        .filter(|&i| rows[i].state == State::GroundAction)
        .collect()
}

/// Checks `rows` against trace rows of `frame x y vx vy state ground`.
fn check(rows: &[Row], expected: &[(usize, f64, f64, f64, f64, State, bool)]) {
    for &(f, x, y, vx, vy, state, ground) in expected {
        let r = &rows[f];
        assert_eq!(
            (r.x, r.y, r.vx, r.vy, r.state, r.grounded),
            (x, y, vx, vy, state, ground),
            "frame {f}"
        );
    }
}

/// Raised ground ending flat at column 156, with floor 176 px below it
/// from there on, and the player standing at x = 100.
fn ledge() -> World {
    ground(|c| Some(if c < 156 { 0 } else { 176 }), 100.0)
}

/// [`ledge`] with floor `drop` px below.
fn ledge_over(drop: i32) -> World {
    ground(|c| Some(if c < 156 { 0 } else { drop }), 100.0)
}

/// Down, Action and Right on frame 5, `then(frame)` from frame 6.
fn pressed_on_5(then: impl Fn(usize) -> Vec<Action>) -> impl Fn(usize) -> Vec<Action> {
    move |f| match f {
        0..5 => vec![],
        5 => vec![R, D, A],
        _ => then(f),
    }
}

use State::{GroundAction as Act, Idle, Walk};

#[test]
fn going_over_a_ledge_without_a_face() {
    // "At a ledge without a face", the example: L is P + 25, the landing
    // G is P + 55 and E is P + 59.
    let rows = play(&mut ledge(), 70, pressed_on_5(|_| vec![]));
    check(
        &rows,
        &[
            (5, 0.375, 0.0, 0.375, 0.0, Act, true),
            (29, 54.5546875, 0.0, 2.8828125, 0.0, Act, true),
            (30, 57.44921875, 3.4375, 2.89453125, 0.00390625, Act, false),
            (31, 60.35546875, 3.87890625, 2.90625, 0.44140625, Act, false),
            (58, 140.74609375, 158.3203125, 3.0, 8.0, Act, false),
            (59, 143.74609375, 166.3203125, 3.0, 8.0, Act, false),
            (60, 146.74609375, 176.0, 3.0, 0.0, Act, true),
            (63, 155.74609375, 176.0, 3.0, 0.0, Act, true),
            (64, 158.74609375, 176.0, 3.0, 0.0, Walk, true),
            (65, 161.37109375, 176.0, 2.625, 0.0, Walk, true),
        ],
    );
    assert!(rows[30].has(Event::LeaveGround));
    assert!(rows[60].has(Event::Land));
    assert_eq!(acting(&rows), (5..64).collect::<Vec<_>>());
    assert!(rows[5..65].iter().all(|r| r.facing == Facing::Right));
    // Left, Right, Up, Down and Action make no difference in the air.
    let busy = play(
        &mut ledge(),
        70,
        pressed_on_5(|f| match f % 3 {
            0 => vec![L, A],
            1 => vec![Action::Up, D],
            _ => vec![R],
        }),
    );
    for f in 5..64 {
        assert_eq!((busy[f].x, busy[f].vx), (rows[f].x, rows[f].vx), "{f}");
    }
    assert_eq!(acting(&busy), acting(&rows));
}

#[test]
fn jumping_at_a_ledge_during_the_action() {
    let jump_on = |q: usize, hold: usize| {
        play(
            &mut ledge(),
            70,
            pressed_on_5(move |f| {
                if (q..q + hold).contains(&f) {
                    vec![J]
                } else {
                    vec![]
                }
            }),
        )
    };
    // On L - 1 (frame 29): the take-off is on L, and the action ends on
    // the press frame.
    let rows = jump_on(29, 1);
    assert_eq!(acting(&rows), (5..29).collect::<Vec<_>>());
    assert!(rows[30].has(Event::JumpStart));
    // On L: L is the leave frame, and the take-off is on L + 1.
    let rows = jump_on(30, 1);
    assert_eq!(acting(&rows), (5..30).collect::<Vec<_>>());
    assert!(rows[30].has(Event::LeaveGround));
    assert!(!rows[30].grounded);
    assert!(rows[31].has(Event::JumpStart));
    // In the air, a press does nothing.
    let plain = play(&mut ledge(), 70, pressed_on_5(|_| vec![]));
    let rows = jump_on(40, 1);
    assert_eq!(acting(&rows), (5..64).collect::<Vec<_>>());
    assert!(rows.iter().all(|r| !r.has(Event::JumpStart)));
    for f in 0..70 {
        assert_eq!((rows[f].x, rows[f].y), (plain[f].x, plain[f].y), "{f}");
    }
}

#[test]
fn jump_held_through_the_landing() {
    // The landing G is frame 60 (P + 55). A press from G - 10 to G + 1,
    // held through G + 1, takes off on G + 2; the action ends on G + 1.
    for press in [50, 55, 60, 61] {
        let rows = play(
            &mut ledge(),
            70,
            pressed_on_5(move |f| if f >= press { vec![J] } else { vec![] }),
        );
        assert!(rows[60].has(Event::Land), "{press}");
        assert_eq!(acting(&rows), (5..61).collect::<Vec<_>>(), "{press}");
        assert!(rows[62].has(Event::JumpStart), "{press}");
        // The action's speed on G + 1.
        assert_eq!(rows[61].vx, 3.0, "{press}");
    }
    // On G - 11, no jump on landing.
    let rows = play(
        &mut ledge(),
        70,
        pressed_on_5(|f| if f >= 49 { vec![J] } else { vec![] }),
    );
    assert!(rows.iter().all(|r| !r.has(Event::JumpStart)));
    assert_eq!(acting(&rows), (5..64).collect::<Vec<_>>());
    // On G + 2, the take-off is on the frame after it.
    let rows = play(
        &mut ledge(),
        70,
        pressed_on_5(|f| if f >= 62 { vec![J] } else { vec![] }),
    );
    assert_eq!(acting(&rows), (5..62).collect::<Vec<_>>());
    assert!(rows[63].has(Event::JumpStart));
}

#[test]
fn the_action_ending_in_the_air() {
    // A longer fall: still in the air on E (frame 64), `vx` is
    // unchanged there, then the air rules apply (target 0 with nothing
    // held).
    let rows = play(&mut ledge_over(280), 80, pressed_on_5(|_| vec![]));
    assert!(!rows[64].grounded);
    assert_eq!(acting(&rows), (5..64).collect::<Vec<_>>());
    assert_eq!(rows[64].vx, 3.0);
    assert!(rows[65].vx < 3.0);
    let land = rows.iter().position(|r| r.has(Event::Land)).unwrap();
    assert!(land > 65);
}

#[test]
fn down_after_the_landing() {
    // With the landing after E, Down does nothing on the landing frame
    // G and on G + 1: the first crouch frame is G + 2.
    let rows = play(&mut ledge_over(280), 90, pressed_on_5(|_| vec![D]));
    let land = rows.iter().position(|r| r.has(Event::Land)).unwrap();
    assert!(land > 64);
    assert!(!rows[land].crouching && !rows[land + 1].crouching);
    assert!(rows[land + 2].crouching);
    // With the landing during the action, it's E + 1 (frame 65).
    let rows = play(&mut ledge(), 70, pressed_on_5(|_| vec![D]));
    assert!(rows[..65].iter().all(|r| !r.crouching));
    assert!(rows[65].crouching);
}

/// Flat to column 167, then a face of 8 columns falling to the right
/// (columns 168 to 175, 3 to 31 px below), a gap, and floor 48 px below
/// from column 200 on. The player stands at x = 100, 68 px before the
/// face.
fn face_down() -> World {
    ground(
        |c| match c {
            ..168 => Some(0),
            168..176 => Some(3 + 4 * (c - 168)),
            176..200 => None,
            _ => Some(48),
        },
        100.0,
    )
}

#[test]
fn going_down_a_face() {
    // "At a steep face", the example: onto the face on frame 34, L is
    // P + 30 and the landing G on the floor beyond the gap is P + 42.
    let rows = play(&mut face_down(), 60, pressed_on_5(|_| vec![]));
    check(
        &rows,
        &[
            (5, 0.375, 0.0, 0.375, 0.0, Act, true),
            (6, 1.078125, 0.0, 0.703125, 0.0, Act, true),
            (33, 66.19921875, 0.0, 2.92578125, 0.0, Act, true),
            (34, 69.1328125, 7.0, 2.93359375, 0.0, Act, true),
            (35, 72.26171875, 10.4375, 3.12890625, 0.00390625, Act, false),
            (36, 75.375, 10.87890625, 3.11328125, 0.44140625, Act, false),
            (
                46,
                106.04296875,
                39.35546875,
                3.04296875,
                4.81640625,
                Act,
                false,
            ),
            (47, 109.08203125, 48.0, 3.0390625, 0.0, Act, true),
            (48, 112.1171875, 48.0, 3.03515625, 0.0, Act, true),
            (50, 118.17578125, 48.0, 3.02734375, 0.0, Act, true),
            (51, 121.17578125, 48.0, 3.0, 0.0, Act, true),
            (55, 133.17578125, 48.0, 3.0, 0.0, Act, true),
        ],
    );
    assert!(rows[35].has(Event::LeaveGround));
    assert!(rows[47].has(Event::Land));
    assert_eq!(acting(&rows), (5..60).collect::<Vec<_>>());
}

#[test]
fn jumping_on_and_onto_a_face() {
    let jump_on = |q: usize| {
        play(
            &mut face_down(),
            60,
            pressed_on_5(move |f| if f == q { vec![J] } else { vec![] }),
        )
    };
    let plain = play(&mut face_down(), 60, pressed_on_5(|_| vec![]));
    // On frame 34, which moves onto the face: the action ends there, and
    // the take-off is on 35, which starts on the face. On 35 the player
    // first leaves the face as without the jump.
    let rows = jump_on(34);
    assert_eq!(acting(&rows), (5..34).collect::<Vec<_>>());
    assert_eq!(rows[34].vx, plain[34].vx);
    assert!(rows[35].has(Event::JumpStart));
    assert!(rows[35].vy < 0.0);
    // On frame 35, which starts on the face: nothing.
    let rows = jump_on(35);
    assert!(rows.iter().all(|r| !r.has(Event::JumpStart)));
    assert_eq!(acting(&rows), acting(&plain));
    for f in 0..60 {
        assert_eq!((rows[f].x, rows[f].y), (plain[f].x, plain[f].y), "{f}");
    }
}

#[test]
fn pressing_on_a_face() {
    // Down + Action pressed on a frame that starts on a long face: no
    // ground action.
    let mut w = ground(
        |c| match c {
            ..168 => Some(0),
            168..200 => Some(3 + 4 * (c - 168)),
            _ => Some(131),
        },
        168.5,
    );
    let rows = play(&mut w, 8, |_| vec![R, D, A]);
    assert!(acting(&rows).is_empty());
    assert!(rows[0].vx > 0.0);
    assert!(rows.iter().all(|r| !r.has(Event::RollStart)));
}

/// A vertical step of 64 px rising to the left: columns up to 99 are
/// 64 px higher than those from 100 on.
fn step_up_left(x: f64) -> World {
    ground(|c| Some(if c < 100 { -64 } else { 0 }), x)
}

#[test]
fn toward_a_vertical_step_up() {
    // "At a steep face", the example: direction -1 from standing facing
    // right 14 px to the right of the foot of the step, Left held on P
    // and P + 1, nothing after P + 3.
    let rows = play(&mut step_up_left(114.0), 66, |f| match f {
        5 | 6 => vec![L, D, A],
        7 | 8 => vec![D, A],
        _ => vec![],
    });
    check(
        &rows,
        &[
            (4, 0.0, 0.0, 0.0, 0.0, Idle, true),
            (5, -0.375, 0.0, -0.375, 0.0, Act, true),
            (6, -1.078125, 0.0, -0.703125, 0.0, Act, true),
            (7, -2.06640625, 0.0, -0.98828125, 0.0, Act, true),
            (8, -3.3046875, 0.0, -1.23828125, 0.0, Act, true),
            (12, -10.1875, 0.0, -1.9609375, 0.0, Act, true),
            (13, -12.27734375, 0.0, -2.08984375, 0.0, Act, true),
            (14, -13.0, 0.0, 0.0, 0.0, Act, true),
            (15, -13.0, 0.0, 0.0, 0.0, Act, true),
            (62, -13.0, 0.0, 0.0, 0.0, Act, true),
            (63, -13.0, 0.0, 0.0, 0.0, Act, true),
            (64, -13.0, 0.0, 0.0, 0.0, Idle, true),
            (65, -13.0, 0.0, 0.0, 0.0, Idle, true),
        ],
    );
    assert!(rows.iter().all(|r| r.facing == Facing::Right));
    // Started 4 px to the right of the foot, the player doesn't move at
    // all and faces left from P.
    let rows = play(&mut step_up_left(104.0), 66, |f| match f {
        5 | 6 => vec![L, D, A],
        _ => vec![],
    });
    assert_eq!(acting(&rows), (5..64).collect::<Vec<_>>());
    assert!(rows.iter().all(|r| r.x == 0.0 && r.vx == 0.0));
    assert!(rows[..5].iter().all(|r| r.facing == Facing::Right));
    assert!(rows[5..].iter().all(|r| r.facing == Facing::Left));
}

/// Flat, then a face rising to the right from column 200
/// (`steep-faces.md`, "Walking into a rise").
fn rise(x: f64) -> World {
    ground(
        |c| {
            Some(match c {
                ..200 => 0,
                200..240 => -1 - 4 * (c - 200),
                _ => -160,
            })
        },
        x,
    )
}

#[test]
fn moving_start_next_to_a_rise() {
    // Walking left away from the face, then Right + Down + Action: the
    // speed goes toward 2 on the side of the previous frame's target,
    // away from the rise, and the player turns right on P.
    let mut w = rise(199.5);
    w.player.facing = Facing::Left;
    let rows = play(&mut w, 70, |f| match f {
        0..3 => vec![L],
        3 => vec![R, D, A],
        _ => vec![],
    });
    assert_eq!(rows[2].facing, Facing::Left);
    let vx: Vec<f64> = rows[3..7].iter().map(|r| r.vx).collect();
    assert_eq!(vx, [-0.25, -0.46875, -0.66015625, -0.82421875]);
    assert_eq!(acting(&rows), (3..62).collect::<Vec<_>>());
    assert!(rows[3..63].iter().all(|r| r.facing == Facing::Right));
    // Toward 2, reached well before E, whose one more step keeps it.
    assert!(rows[3..62].windows(2).all(|w| w[1].vx <= w[0].vx));
    assert!(rows[50..63].iter().all(|r| r.vx == -2.0));

    // Walking right toward it: the same speeds toward it until blocked,
    // then 0 to E.
    let mut w = rise(196.5);
    let rows = play(&mut w, 70, |f| match f {
        0..3 => vec![R],
        3 => vec![R, D, A],
        _ => vec![],
    });
    let vx: Vec<f64> = rows[3..5].iter().map(|r| r.vx).collect();
    assert_eq!(vx, [0.25, 0.46875]);
    let blocked = (3..62).find(|&f| rows[f].vx == 0.0).unwrap();
    assert!(rows[blocked..63].iter().all(|r| r.vx == 0.0));
    assert_eq!(rows[blocked].x + 196.5, 199.0);

    // Standing facing left next to it: no move at all, facing right
    // from P.
    let mut w = rise(197.5);
    w.player.facing = Facing::Left;
    let rows = play(&mut w, 70, |f| if f == 3 { vec![R, D, A] } else { vec![] });
    assert_eq!(acting(&rows), (3..62).collect::<Vec<_>>());
    assert!(rows.iter().all(|r| r.x == 0.0));
    assert!(rows[3..63].iter().all(|r| r.facing == Facing::Right));
}

#[test]
fn off_a_vertical_step_down() {
    // A vertical step down is a ledge without a face: the action goes
    // on over it and after the landing.
    let mut w = ground(|c| Some(if c < 156 { 0 } else { 64 }), 100.0);
    let rows = play(&mut w, 66, pressed_on_5(|_| vec![]));
    let leave = rows.iter().position(|r| r.has(Event::LeaveGround)).unwrap();
    let land = rows.iter().position(|r| r.has(Event::Land)).unwrap();
    assert!(leave < land && land < 63);
    assert_eq!(acting(&rows), (5..64).collect::<Vec<_>>());
}

#[test]
fn on_a_slope() {
    // "On a slope", the example: a dip of 1 in 4 down from column 100,
    // flat at 16 from 160 to 191, back up to 0 at 252.
    let dip = |c: i32| {
        Some(match c {
            ..100 => 0,
            100..160 => (c - 100) / 4 + 1,
            160..192 => 16,
            192..252 => 15 - (c - 192) / 4,
            _ => 0,
        })
    };
    let x0 = 74.26953125;
    let mut w = ground(dip, x0);
    let rows = play(&mut w, 68, |f| if f == 0 { vec![D, A, R] } else { vec![R] });
    for (f, x, y, vx) in [
        (0, 74.64453125, 0.0, 96),
        (13, 98.42578125, 0.0, 647),
        (14, 101.01171875, 1.0, 662),
        (15, 103.6484375, 1.0, 675),
        (16, 106.328125, 2.0, 686),
        (17, 109.046875, 3.0, 696),
        (35, 161.109375, 16.0, 758),
        (39, 173.015625, 16.0, 768),
        (45, 191.015625, 16.0, 768),
        (46, 194.015625, 15.0, 768),
        (47, 197.015625, 14.0, 768),
        (58, 230.015625, 6.0, 768),
        (59, 233.015625, 5.0, 768),
        (60, 235.890625, 5.0, 736),
        (66, 251.296875, 1.0, 615),
        (67, 253.65234375, 0.0, 603),
    ] {
        let r = &rows[f];
        assert_eq!(
            (r.x + x0, r.y, r.vx * 256.0, r.vy, r.grounded),
            (x, y, f64::from(vx), 0.0, true),
            "frame {f}"
        );
    }
    assert_eq!(acting(&rows), (0..59).collect::<Vec<_>>());

    // Direction 0 on a slope: no slide.
    let mut w = ground(dip, 130.5);
    let rows = play(&mut w, 62, |f| if f == 0 { vec![D, A] } else { vec![] });
    assert!(rows.iter().all(|r| r.x == 0.0 && r.vx == 0.0 && r.grounded));
}

#[test]
fn a_jump_during_the_action_takes_off_on_its_press_when_immediate() {
    // #437: the spec has take-off on Q + 1; with the feel immediate it is
    // on Q, and the action still ends on Q.
    let take_off = |immediate: bool| {
        let mut w = ground(|_| Some(0), 100.0);
        w.feel = Arc::new(clamber_pack::Feel {
            immediate,
            ..clamber_pack::Feel::clone(&w.feel)
        });
        let rows = play(
            &mut w,
            30,
            pressed_on_5(|f| if f == 20 { vec![J] } else { vec![] }),
        );
        assert_eq!(acting(&rows), (5..20).collect::<Vec<_>>());
        rows.iter().position(|r| r.has(Event::JumpStart))
    };
    assert_eq!(take_off(false), Some(21));
    assert_eq!(take_off(true), Some(20));
}
