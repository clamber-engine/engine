//! Springs on `classic-1` (spec/behavior/tyre.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Fixed, InputFrame, Phase, World, generic_feel, step};

use Action::{Jump as J, Right as R};

/// The floor's top.
const FLOOR: i32 = 300;

/// Flat ground at y = 300 from x = 0 to 2000, the player at `start`, and
/// a spring at `spring`.
fn world(start: (f64, f64), spring: (i32, i32)) -> World {
    let src = format!(
        r#"
        format = 1
        size = [2000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, {FLOOR}, 2000, 300]

        [[entities]]
        type = "spring"
        at = [{}, {}]
        "#,
        start.0, start.1, spring.0, spring.1
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// What a test reads after each frame.
#[derive(Clone, Copy, Debug)]
struct Row {
    /// The player's `x` rounded down less the spring's.
    i: i64,
    /// How far the player's position is above the spring's, with its
    /// fraction.
    u: Fixed,
    vy: Fixed,
    ground: bool,
    phase: Phase,
}

fn row(w: &World) -> Row {
    let spring = &w.things[0];
    Row {
        i: w.player.pos.x.floor() - spring.pos.x.floor(),
        u: spring.pos.y - w.player.pos.y,
        vy: w.player.vel.y,
        ground: w.player.grounded(),
        phase: spring.phase,
    }
}

/// Plays `frames` frames holding what `held` says on each, and returns a
/// row for each.
fn play(w: &mut World, frames: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    let mut last = Actions::NONE;
    (0..frames)
        .map(|k| {
            let now: Actions = held(k).into_iter().collect();
            step(w, &InputFrame::single(Frame::after(last, now)));
            last = now;
            row(w)
        })
        .collect()
}

/// The first frame on which the spring caught the player, at or after
/// `from`.
fn catch(rows: &[Row], from: usize) -> usize {
    (from..rows.len())
        .find(|&k| matches!(rows[k].phase, Phase::Squashed { since: 0, .. }))
        .expect("no catch")
}

/// Walks the player right from `x` into a spring at rest on the floor
/// and releases Right once caught. Returns `frames` rows and the catch
/// frame.
fn walk_in(x: f64, frames: usize) -> (Vec<Row>, usize) {
    let mut w = world((x, 300.0), (200, FLOOR));
    let walk = play(&mut w, 200, |_| vec![R]);
    let c = catch(&walk, 0);
    let mut w = world((x, 300.0), (200, FLOOR));
    let rows = play(&mut w, frames, |k| if k <= c { vec![R] } else { vec![] });
    (rows, c)
}

#[test]
fn the_example() {
    // Walking right at 2 px a frame, from where the player is caught at
    // I = -24 as in the spec.
    let (rows, c) = walk_in(101.0, 200);
    assert_eq!(rows[c - 1].i, -26, "not touching before C");
    assert_eq!(rows[c].i, -24);
    assert_eq!((rows[c].u, rows[c].ground), (fx(18.0), true), "caught");
    for r in &rows[c + 1..=c + 12] {
        assert_eq!((r.u, r.vy, r.ground), (fx(14.0), Fixed::ZERO, true));
    }
    assert_eq!(rows[c + 13].u, fx(18.0));
    assert_eq!(rows[c + 14].u, fx(14.0));
    let l = c + 15;
    let at = |n: usize| {
        let r = rows[l + n];
        (r.u, r.vy, r.ground)
    };
    assert_eq!(at(0), (fx(6.0), fx(-6.0), true), "L: thrown");
    assert_eq!(at(1), (fx(6.0), fx(-6.0), true), "L + 1: nothing changes");
    assert_eq!(at(2), (fx(11.5625), fx(-5.5625), false));
    assert_eq!(
        at(14),
        (fx(44.1875), fx(-0.3125), false),
        "the top of the arc"
    );
    assert_eq!(at(24), (fx(23.25), fx(4.0625), false));
    assert_eq!(at(25), (fx(18.75), Fixed::ZERO, true), "caught again");
    assert_eq!(at(26), (fx(14.75), Fixed::ZERO, true), "squashed");
    assert!(matches!(
        rows[l + 25].phase,
        Phase::Squashed {
            since: 0,
            from_air: true
        }
    ));
}

#[test]
fn the_throw_follows_its_formula() {
    let (rows, c) = walk_in(100.0, 200);
    let l = c + 15;
    let base = rows[l].u;
    for n in 2..=24 {
        let m = (n - 1) as f64;
        let vy = -6.0 + 0.4375 * m;
        let y = -6.0 * m + 0.4375 * m * n as f64 / 2.0;
        assert_eq!(rows[l + n].vy, fx(vy), "L + {n}");
        assert_eq!(base - rows[l + n].u, fx(y), "L + {n}");
    }
}

#[test]
fn the_wobble_and_the_rest_after_a_throw() {
    // Thrown from a spring at rest on the floor and caught again on
    // L + 25, with no input.
    let (rows, c) = walk_in(100.0, 200);
    let l = c + 15;
    assert!(matches!(
        rows[l].phase,
        Phase::Wobbling {
            m: 0,
            thrown: Some(0)
        }
    ));
    assert!(matches!(rows[l + 24].phase, Phase::Wobbling { m: 24, .. }));
    assert_eq!(catch(&rows, c + 1), l + 25);
    // Ready from L + 9: touched without a catch before that, it wobbles
    // on.
    assert!(matches!(
        rows[l + 8].phase,
        Phase::Wobbling {
            m: 8,
            thrown: Some(8)
        }
    ));
}

#[test]
fn throws_with_no_input_keep_the_fraction() {
    // From a whole-pixel start, the spec's table: the catch is on L + 25
    // while the fraction on L is below 0.25, else L + 26, and from the
    // second cycle on a group of five repeats.
    let table = [
        (0.0, 25, 18.75),
        (0.75, 26, 18.5625),
        (0.5625, 26, 18.375),
        (0.375, 26, 18.1875),
        (0.1875, 25, 18.9375),
        (0.9375, 26, 18.75),
    ];
    let (rows, c) = walk_in(100.0, 600);
    let mut l = c + 15;
    for cycle in 0..9 {
        let (f, after, u) = table[if cycle < 6 { cycle } else { cycle - 5 }];
        let n = cycle + 1;
        assert_eq!(
            (rows[l].u, rows[l].vy),
            (fx(6.0 + f), fx(-6.0)),
            "cycle {n}: L"
        );
        let k = catch(&rows, l);
        assert_eq!(k - l, after, "cycle {n}: caught on L + {}", k - l);
        assert_eq!(
            (rows[k].u, rows[k].vy),
            (fx(u), Fixed::ZERO),
            "cycle {n}: caught"
        );
        l = k + 15;
    }
    let catches: Vec<usize> = (0..7)
        .scan(c, |from, _| {
            let k = catch(&rows, *from);
            *from = k + 1;
            Some(k)
        })
        .collect();
    let lengths: Vec<usize> = catches.windows(2).map(|p| p[1] - p[0]).collect();
    assert_eq!(lengths, [40, 41, 41, 41, 40, 41]);
    assert_eq!(catches[6] - catches[1], 204, "a group of five cycles");
}

/// Stands the player on a spring at rest on the floor, caught walking
/// in, then presses Jump on frame `p` after the catch, held for `k`
/// frames. Returns the top of the arc above the position on `p - 1`.
fn spring_jump_height(p: usize, k: usize) -> Fixed {
    let mut w = world((100.0, 300.0), (200, FLOOR));
    let walk = play(&mut w, 200, |_| vec![R]);
    let c = catch(&walk, 0);
    let mut w = world((100.0, 300.0), (200, FLOOR));
    let rows = play(&mut w, c + p + 80, |n| {
        if n <= c {
            vec![R]
        } else if n >= c + p && n < c + p + k {
            vec![J]
        } else {
            vec![]
        }
    });
    let before = rows[c + p - 1].u;
    assert!(!rows[c + p + 1].ground, "a spring jump on P");
    assert_eq!(rows[c + p].vy, fx(-9.6875));
    assert_eq!(rows[c + p].u - before, fx(9.6875));
    rows[c + p..].iter().map(|r| r.u).max().unwrap() - before
}

#[test]
fn spring_jump_heights() {
    for (k, height) in [
        (1, 112.125),
        (2, 114.875),
        (3, 117.5),
        (4, 120.0),
        (5, 122.5),
        (8, 129.3125),
        (10, 133.4375),
        (12, 137.1875),
        (15, 142.125),
        (20, 148.625),
        (25, 152.8125),
        (30, 154.875),
        (31, 155.0),
        (40, 155.0),
    ] {
        assert_eq!(spring_jump_height(3, k), fx(height), "K = {k}");
    }
}

#[test]
fn a_press_on_the_catch_frame_jumps_from_eight_px_below_the_top() {
    let mut w = world((100.0, 300.0), (200, FLOOR));
    let walk = play(&mut w, 200, |_| vec![R]);
    let c = catch(&walk, 0);
    let mut w = world((100.0, 300.0), (200, FLOOR));
    let rows = play(&mut w, c + 20, |n| {
        if n < c {
            vec![R]
        } else if n < c + 10 {
            vec![R, J]
        } else {
            vec![]
        }
    });
    assert_eq!(rows[c].u, fx(18.0));
    // An ordinary jump: it takes off on C + 1, 8 px down.
    assert_eq!(
        (rows[c + 1].u, rows[c + 1].vy, rows[c + 1].ground),
        (fx(10.0), fx(-7.0), true)
    );
    assert!(!rows[c + 2].ground);
    assert_eq!(rows[c + 2].vy, fx(-6.71875));
}

/// Drops the player onto a spring at rest on the floor from 100 px above
/// it, presses Jump on frame `c + offset` (the catch on `c`) and holds it
/// for 30 frames, and returns the frame the spring jump started on,
/// counted from the catch, if one did before the throw.
fn from_the_air(offset: i64) -> Option<i64> {
    let start = (205.0, f64::from(FLOOR) - 100.0);
    let mut w = world(start, (200, FLOOR));
    let rows = play(&mut w, 120, |_| vec![]);
    let c = catch(&rows, 0) as i64;
    let p = c + offset;
    let mut w = world(start, (200, FLOOR));
    let rows = play(&mut w, 120, |n| {
        let n = n as i64;
        if n >= p && n < p + 30 {
            vec![J]
        } else {
            vec![]
        }
    });
    assert_eq!(catch(&rows, 0) as i64, c);
    // A spring jump moves the player 9.6875 up on its first frame.
    (c + 1..c + 16)
        .find(|&n| rows[n as usize].vy == fx(-9.6875))
        .map(|n| n - c)
}

#[test]
fn caught_in_the_air_a_spring_jump_waits_two_frames() {
    // E = C + 2: a press from C - 9 to E, held, jumps on E.
    for offset in [-9, -5, -1, 0, 1, 2] {
        assert_eq!(from_the_air(offset), Some(2), "press on C {offset:+}");
    }
    // Held since before C - 9: never.
    assert_eq!(from_the_air(-10), None);
    // Later presses jump at once.
    for offset in [3, 5, 14] {
        assert_eq!(from_the_air(offset), Some(offset), "press on C {offset:+}");
    }
}

#[test]
fn walking_off_the_top() {
    // Caught in the air near the right end, then walking right off it.
    let start = (220.0, f64::from(FLOOR) - 100.0);
    let mut w = world(start, (200, FLOOR));
    let fall = play(&mut w, 120, |_| vec![]);
    let c = catch(&fall, 0);
    let walk_off = |press_on_leave: bool| {
        let mut w = world(start, (200, FLOOR));
        let rows = play(&mut w, c + 40, |n| if n > c { vec![R] } else { vec![] });
        let f = (c + 1..c + 15)
            .find(|&n| rows[n].u < rows[n - 1].u - fx(4.0))
            .unwrap();
        if !press_on_leave {
            return (rows, f);
        }
        let mut w = world(start, (200, FLOOR));
        let rows = play(&mut w, c + 40, |n| match n {
            n if n > f && n <= f + 10 => vec![R, J],
            n if n > c => vec![R],
            _ => vec![],
        });
        (rows, f)
    };
    let (rows, f) = walk_off(false);
    assert!(rows[f].i > 22, "not touching on F");
    assert_eq!(rows[f - 1].u - rows[f].u, fx(8.0), "8 px lower on F");
    assert_eq!((rows[f].vy, rows[f].ground), (Fixed::ZERO, true));
    assert_eq!(rows[f].u - rows[f + 1].u, fx(0.44140625), "the leave frame");
    assert_eq!(
        (rows[f + 1].vy, rows[f + 1].ground),
        (fx(0.00390625), false)
    );
    assert_eq!(rows[f + 2].vy, fx(0.44140625));
    assert_eq!(rows[f + 1].u - rows[f + 2].u, fx(0.44140625));

    // A press on F + 1 jumps with take-off on F + 2.
    let (rows, f) = walk_off(true);
    assert_eq!(rows[f].u - rows[f + 1].u, fx(0.28515625));
    assert_eq!(rows[f + 1].u - rows[f + 2].u, fx(0.28515625));
    assert_eq!(rows[f + 2].vy, fx(-7.0));
}

#[test]
fn a_spring_up_to_51_px_up_lifts_a_player_walking_under_it() {
    for (height, lifted) in [(51, true), (52, false)] {
        let mut w = world((100.0, 300.0), (200, FLOOR - height));
        let rows = play(&mut w, 120, |_| vec![R]);
        let caught = rows
            .iter()
            .position(|r| matches!(r.phase, Phase::Squashed { .. }));
        assert_eq!(caught.is_some(), lifted, "{height} px up");
        if let Some(c) = caught {
            assert_eq!(rows[c].u, fx(18.0));
            assert!(rows[c].ground);
        }
    }
}

#[test]
fn a_falling_player_is_caught_only_near_the_top() {
    // A spring 50 px up: falling past its side from above, the player is
    // caught at once when its feet line comes level with the top - 13
    // or above, never lower.
    let mut w = world((180.0, f64::from(FLOOR) - 200.0), (200, FLOOR - 50));
    let rows = play(&mut w, 120, |_| vec![]);
    let c = catch(&rows, 0);
    assert_eq!(rows[c].u.floor(), 18, "the fraction is kept");
    assert!(rows[c - 1].u.floor() > 18, "above the top before");
}
