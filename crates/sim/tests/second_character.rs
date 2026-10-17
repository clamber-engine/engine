//! The second character's movement in Classic 1
//! (spec/behavior/second-character.md), with the second character under
//! control from the start.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{Event, Facing, InputFrame, ROLL, RollSize, State, World, generic_feel, step};

/// Flat ground at y = 400 from x = 0 to 4096.
const FLAT: &str = r#"
    format = 1
    name = "flat"
    size = [4096, 600]
    start = [400, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 400, 4096, 200]
"#;

/// A world on `FLAT` with Classic 1's two roster slots, both able to
/// roll, the second under control, and what was held on its last step.
struct Play {
    world: World,
    held: Actions,
}

fn second() -> Play {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::from([(ROLL.to_string(), BTreeMap::new())]),
    });
    let level = Arc::new(Level::from_toml_str(FLAT, "flat").unwrap());
    let mut world = World::new(level, Arc::new(feel));
    world.player.character = 1;
    if let Some(other) = &mut world.other {
        other.character = 0;
    }
    let mut play = Play {
        world,
        held: Actions::NONE,
    };
    // settle on the ground
    hold(&mut play, &[], 2);
    assert!(play.world.player.grounded());
    play
}

/// Holds `keys` for `frames` steps and returns the events of each step.
/// Action is never pressed, only held: as in the spec's measurements,
/// holding it means running, never a roll.
fn hold(play: &mut Play, keys: &[Action], frames: usize) -> Vec<Vec<Event>> {
    let held: Actions = keys.iter().copied().collect();
    let action = held & Actions::from(Action::Action);
    let mut events = Vec::new();
    for _ in 0..frames {
        let input = InputFrame::single(Frame::after(play.held | action, held));
        play.held = held;
        step(&mut play.world, &input);
        events.push(play.world.events.clone());
    }
    events
}

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

const R: Action = Action::Right;
const L: Action = Action::Left;
const A: Action = Action::Action;
const J: Action = Action::Jump;

#[test]
fn ground_speed_changes() {
    // "Ground speed", summary: from, input held, first three `vx`, the
    // frame the target is reached and the distance until then.
    let walk: &[Action] = &[R];
    let run: &[Action] = &[R, A];
    for (before, after, target, first, reached, distance) in [
        (
            &[][..],
            walk,
            2.25,
            [0.28125, 0.52734375, 0.7421875],
            37,
            69.4765625,
        ),
        (
            &[],
            run,
            3.375,
            [0.421875, 0.7890625, 1.109375],
            40,
            114.390625,
        ),
        (
            walk,
            &[],
            0.0,
            [1.96875, 1.72265625, 1.5078125],
            37,
            16.0234375,
        ),
        (
            run,
            &[A],
            0.0,
            [2.953125, 2.5859375, 2.265625],
            40,
            23.984375,
        ),
        (
            run,
            walk,
            2.25,
            [3.234375, 3.11328125, 3.0078125],
            32,
            82.3671875,
        ),
        (
            walk,
            run,
            3.375,
            [2.390625, 2.51171875, 2.6171875],
            32,
            103.2578125,
        ),
    ] {
        let mut w = second();
        hold(&mut w, before, 60);
        let start = w.world.player.pos.x;
        let mut vx = Vec::new();
        for _ in 0..=reached {
            hold(&mut w, after, 1);
            vx.push(w.world.player.vel.x);
        }
        let case = format!("{before:?} to {after:?}");
        assert_eq!(vx[..3], first.map(f), "{case}");
        assert_ne!(vx[reached - 1], f(target), "{case}");
        assert_eq!(vx[reached], f(target), "{case}");
        assert_eq!(w.world.player.pos.x - start, f(distance), "{case}");
    }
}

#[test]
fn reversing() {
    // "Ground speed": reversing from walking and from running speed.
    for (before, after, first, fifth, reached, top) in [
        (
            &[R][..],
            &[L][..],
            [1.6875, 1.1953125, 0.765625, 0.390625, 0.0625],
            -0.2265625,
            42,
            -2.25,
        ),
        (
            &[R, A],
            &[L, A],
            [2.53125, 1.79296875, 1.1484375, 0.5859375, 0.09375],
            -0.33984375,
            45,
            -3.375,
        ),
    ] {
        let mut w = second();
        hold(&mut w, before, 60);
        let mut vx = Vec::new();
        for _ in 0..=reached {
            hold(&mut w, after, 1);
            vx.push(w.world.player.vel.x);
        }
        assert_eq!(vx[..5], first.map(f), "{before:?}");
        assert_eq!(vx[5], f(fifth), "{before:?}");
        assert_ne!(vx[reached - 1], f(top), "{before:?}");
        assert_eq!(vx[reached], f(top), "{before:?}");
        if before == [R] {
            // how much further right the player travels
            let further = vx[..5].iter().fold(Fixed::ZERO, |sum, &v| sum + v);
            assert_eq!(further, f(4.1015625));
        }
    }
}

#[test]
fn crouching_recovers_over_thirteen_frames() {
    // "Ground speed", crouching example: walking at +2.25, Down also held
    // on frames 60 to 89.
    let mut w = second();
    let mut vx = Vec::new();
    for frame in 0..104 {
        let keys: &[Action] = if (60..90).contains(&frame) {
            &[R, Action::Down]
        } else {
            &[R]
        };
        hold(&mut w, keys, 1);
        vx.push(w.world.player.vel.x);
    }
    assert_eq!(vx[59], f(2.25));
    assert_eq!(vx[60], f(2.25));
    assert_eq!(vx[61], f(1.96875));
    assert_eq!(vx[97], f(0.02734375));
    assert_eq!(vx[98..103], [Fixed::ZERO; 5]);
    assert_eq!(vx[103], f(0.28125));
}

/// A jump pressed on step 0 with `before` held, Jump held for `held`
/// steps and `air` held from step `air_from` (1 is the take-off step):
/// `y` relative to the start, `vy` and `x` after each step, up to and
/// including the landing.
fn jump(
    w: &mut Play,
    before: &[Action],
    held: usize,
    air: &[Action],
    air_from: usize,
) -> Vec<(f64, f64, f64)> {
    let floor = w.world.player.pos.y;
    let mut rows = Vec::new();
    while rows.len() < 200 {
        let mut keys = if rows.is_empty() {
            before.to_vec()
        } else if rows.len() >= air_from {
            air.to_vec()
        } else {
            Vec::new()
        };
        if rows.len() < held {
            keys.push(J);
        }
        let events = hold(w, &keys, 1).remove(0);
        if rows.len() == 1 {
            assert!(events.contains(&Event::JumpStart));
        }
        let p = &w.world.player;
        rows.push((
            (p.pos.y - floor).to_f64(),
            p.vel.y.to_f64(),
            p.pos.x.to_f64(),
        ));
        if events.contains(&Event::Land) {
            return rows;
        }
    }
    panic!("never landed");
}

#[test]
fn jump_arc_held_from_take_off_to_landing() {
    // "Jump arc", held from take-off to landing. T is step 1.
    let rows = jump(&mut second(), &[], usize::MAX, &[], 1);
    for (n, y, vy) in [
        (0, 0.0, -8.3515625),
        (1, -8.0, -8.0),
        (2, -15.6484375, -7.6484375),
        (5, -36.484375, -6.59375),
        (10, -64.1796875, -4.8359375),
        (20, -93.203125, -1.3203125),
        (23, -95.0546875, -0.265625),
        (24, -94.96875, 0.0859375),
        (30, -87.0703125, 2.1953125),
        (40, -45.78125, 5.7109375),
        (45, -11.953125, 7.46875),
        (46, 0.0, 0.0),
    ] {
        let (sy, svy, _) = rows[1 + n];
        assert_eq!((sy, svy), (y, vy), "T + {n}");
    }
    assert_eq!(rows.len(), 1 + 47);
}

#[test]
fn jump_height_and_airtime_by_hold() {
    // "Jump arc": `apex_height` and `airtime` for H frames held from the
    // press.
    for (held, apex, airtime) in [
        (1, 75.5546875, 37),
        (2, 75.5546875, 37),
        (3, 77.1875, 37),
        (4, 78.734375, 38),
        (5, 80.1953125, 38),
        (8, 84.1796875, 39),
        (10, 86.5, 40),
        (12, 88.5, 41),
        (15, 91.078125, 42),
        (20, 93.9296875, 43),
        (25, 95.0546875, 44),
        (30, 95.0546875, 45),
        (40, 95.0546875, 46),
        (usize::MAX, 95.0546875, 46),
    ] {
        let rows = jump(&mut second(), &[], held, &[], 1);
        let top = rows.iter().map(|r| -r.0).fold(0.0, f64::max);
        assert_eq!(top, apex, "held {held}");
        // the jump starts on step 1 and lands on the last
        assert_eq!(rows.len() - 2, airtime, "held {held}");
    }
}

#[test]
fn jump_distances() {
    // "Jump distances": jump held until landing, distance from the
    // take-off frame to the landing frame and `vx` on landing.
    for (before, air, distance, landing_vx) in [
        (&[][..], &[R][..], 89.4453125, 2.25),
        (&[], &[R, A], 131.265625, 3.375),
        (&[R], &[R], 103.5, 2.25),
        (&[R], &[], 14.0546875, 0.0),
        (&[R], &[L], -75.5546875, -2.25),
        (&[R, A], &[R, A], 155.25, 3.375),
        (&[R, A], &[R], 110.6328125, 2.25),
    ] {
        let mut w = second();
        hold(&mut w, before, 60);
        // Right and Action from standing are held from T + 1
        let air_from = if before.is_empty() && air.contains(&A) {
            2
        } else {
            1
        };
        let rows = jump(&mut w, before, usize::MAX, air, air_from);
        let case = format!("{before:?} then {air:?}");
        assert_eq!(rows.len(), 1 + 47, "{case}");
        assert_eq!(rows[47].2 - rows[1].2, distance, "{case}");
        assert_eq!(w.world.player.vel.x, f(landing_vx), "{case}");
    }
}

const DOWN: Action = Action::Down;

/// One step of `steps`.
struct Row {
    x: Fixed,
    vx: Fixed,
    vy: Fixed,
    facing: Facing,
    state: State,
    events: Vec<Event>,
}

/// Plays `n` steps holding `keys(step)` on each; a key held after a step
/// without it is a press.
fn steps(play: &mut Play, n: usize, keys: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    (0..n)
        .map(|i| {
            let held: Actions = keys(i).into_iter().collect();
            let input = InputFrame::single(Frame::after(play.held, held));
            play.held = held;
            step(&mut play.world, &input);
            let p = &play.world.player;
            Row {
                x: p.pos.x,
                vx: p.vel.x,
                vy: p.vel.y,
                facing: p.facing,
                state: p.state,
                events: play.world.events.clone(),
            }
        })
        .collect()
}

fn frames_with(rows: &[Row], event: Event) -> Vec<usize> {
    (0..rows.len())
        .filter(|&i| rows[i].events.contains(&event))
        .collect()
}

/// D, the frame after a roll started on step 0.
const D: usize = 24;

#[test]
fn roll_worked_values() {
    // "Rolls", worked values: `on_s` held on S, `then` after.
    let walking = || {
        let mut w = second();
        hold(&mut w, &[R], 60);
        w
    };
    let running = || {
        let mut w = second();
        hold(&mut w, &[R, A], 60);
        hold(&mut w, &[R], 1);
        assert_eq!(w.world.player.vel.x, f(3.234375));
        w
    };
    for (mut w, on_s, then, size, first, on_d, to_d, to_stop) in [
        (
            second(),
            &[A][..],
            &[][..],
            RollSize::Slow,
            [0.140625, 0.26171875, 0.3671875],
            1.0703125,
            20.28125,
            22.7578125,
        ),
        (
            second(),
            &[A, R],
            &[R],
            RollSize::Normal,
            [0.421875, 0.7890625, 1.109375],
            3.2421875,
            61.31640625,
            68.82421875,
        ),
        (
            walking(),
            &[A, R],
            &[R],
            RollSize::Normal,
            [2.390625, 2.51171875, 2.6171875],
            3.3203125,
            76.53125,
            84.20703125,
        ),
        (
            running(),
            &[A, R],
            &[R],
            RollSize::Fast,
            [3.390625, 3.52734375, 3.6484375],
            4.44140625,
            103.703125,
            113.97265625,
        ),
    ] {
        let case = format!("{on_s:?} at {}", w.world.player.vel.x);
        let x0 = w.world.player.pos.x;
        let rows = steps(&mut w, D + 4, |i| if i == 0 { on_s } else { then }.to_vec());
        assert_eq!(rows[0].events, [Event::RollStart], "{case}");
        assert_eq!(w.world.player.stopped.unwrap().size, size, "{case}");
        let vx: Vec<_> = rows[..3].iter().map(|r| r.vx).collect();
        assert_eq!(vx, first.map(f), "{case}");
        assert!(rows[..D].iter().all(|r| r.state == State::Roll), "{case}");
        assert_eq!(frames_with(&rows, Event::RollEnd), [D], "{case}");
        assert_eq!(rows[D].vx, f(on_d), "{case}");
        assert_eq!(rows[D].x - x0, f(to_d), "{case}");
        assert_eq!(rows[D + 3].x - x0, f(to_stop), "{case}");
        assert_eq!(rows[D + 3].vx, Fixed::ZERO, "{case}");
    }
}

#[test]
fn roll_reversing_and_steering() {
    // "Rolls": reversing a normal roll from walking speed.
    let mut w = second();
    hold(&mut w, &[R], 60);
    let x0 = w.world.player.pos.x;
    let rows = steps(&mut w, D + 1, |i| if i == 0 { vec![A, R] } else { vec![L] });
    let vx: Vec<_> = rows[..5].iter().map(|r| r.vx).collect();
    assert_eq!(
        vx,
        [2.390625, 1.671875, 1.04296875, 0.4921875, 0.01171875].map(f)
    );
    assert_eq!(rows[5].vx, f(-0.41015625));
    assert_eq!(rows[D].vx, f(-3.12890625));
    assert_eq!(rows[D].x - x0, f(-39.625));

    // A slow roll steered its way from S + 10 goes toward 3, not 3.375.
    let mut w = second();
    let rows = steps(&mut w, D + 1, |i| match i {
        0 => vec![A],
        1..10 => vec![],
        _ => vec![R],
    });
    assert_eq!(rows[9].vx, f(0.8203125));
    assert_eq!(rows[10].vx, f(1.08984375));
    assert_eq!(rows[D].vx, f(2.6953125));
}

/// From standing, pressing Action alone on `presses` and holding Right
/// from `right`.
fn presses(presses: &[usize], right: usize, n: usize) -> Vec<Row> {
    let mut w = second();
    steps(&mut w, n, |i| {
        let mut keys = Vec::new();
        if presses.contains(&i) {
            keys.push(A);
        }
        if i >= right {
            keys.push(R);
        }
        keys
    })
}

#[test]
fn roll_lasts_24_frames_and_the_stop_after_it() {
    // "Rolls": a roll lasts 24 frames, then the stop of roll-speed.md.
    let rows = presses(&[0], 30, 50);
    assert_eq!(frames_with(&rows, Event::RollStart), [0]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [D]);
    assert!(rows[D + 3..D + 18].iter().all(|r| r.vx == Fixed::ZERO));
    assert_ne!(rows[D + 2].x, rows[D + 3].x);
    assert!(rows[D + 4..D + 18].iter().all(|r| r.x == rows[D + 3].x));
    assert_eq!(rows[D + 18].vx, f(0.28125));
}

#[test]
fn next_roll_after_a_stop() {
    // "Rolls", length: a press from D - 2 to D + 16 is remembered to
    // D + 17, and a roll starting on D + 17 lasts 25 frames and doesn't
    // move on its first, whether its press was remembered or new.
    for p in D - 2..=D + 17 {
        let rows = presses(&[0, p], usize::MAX, 80);
        assert_eq!(frames_with(&rows, Event::RollStart), [0, D + 17], "{p}");
        assert_eq!(frames_with(&rows, Event::RollEnd), [D, D + 42], "{p}");
        assert_eq!(rows[D + 17].vx, Fixed::ZERO, "{p}");
        assert_eq!(rows[D + 17].x, rows[D + 16].x, "{p}");
        assert!(rows[D + 18].vx > Fixed::ZERO, "{p}");
    }
    // Earlier presses are ignored.
    let rows = presses(&[0, D - 3], usize::MAX, 80);
    assert_eq!(frames_with(&rows, Event::RollStart), [0]);
    // A press on D + 18 or later starts a 24-frame roll.
    for p in [D + 18, D + 21] {
        let rows = presses(&[0, p], usize::MAX, 80);
        assert_eq!(frames_with(&rows, Event::RollStart), [0, p], "{p}");
        assert_eq!(frames_with(&rows, Event::RollEnd), [D, p + D], "{p}");
    }
}

#[test]
fn roll_carries_on_to_d_plus_6() {
    // "Rolls", carrying on: Action alone held through a slow roll.
    let mut w = second();
    let rows = steps(&mut w, D + 9, |_| vec![A]);
    assert_eq!(frames_with(&rows, Event::RollStart), [0]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [D + 6]);
    assert!(rows[..D + 6].iter().all(|r| r.state == State::Roll));
    assert_eq!(rows[D + 6].vx, f(1.09375));
    assert_eq!(rows[D + 7].vx, f(0.95703125));

    // The next roll after carrying on: Action released on D + 1 and
    // pressed on P >= D + 2 starts it on max(P, D + 7).
    for p in D + 2..=D + 11 {
        let mut w = second();
        let rows = steps(&mut w, D + 14, |i| {
            if i == D + 1 || (D + 1..p).contains(&i) {
                vec![R]
            } else {
                vec![A, R]
            }
        });
        let start = p.max(D + 7);
        assert_eq!(frames_with(&rows, Event::RollStart), [0, start], "{p}");
        assert_eq!(frames_with(&rows, Event::RollEnd), [D + 6], "{p}");
    }
}

#[test]
fn down_and_action_roll() {
    // "Rolls": Down held on the press frame makes no difference.
    let mut w = second();
    let rows = steps(&mut w, 3, |_| vec![DOWN, A]);
    assert_eq!(rows[0].events, [Event::RollStart]);
    assert_eq!(rows[1].state, State::Roll);
}

/// Standing still with Action held and no roll after landing, one or
/// more running frames from standing (`run`), then `between` (never with
/// Action) and a press with `on_s`. Returns the roll's size.
fn size_after_run(run: usize, between: &[&[Action]], on_s: &[Action]) -> RollSize {
    let mut w = second();
    hold(&mut w, &[J], 1);
    hold(&mut w, &[], 2);
    // Pressed too long before landing to roll on it.
    let landing = hold(&mut w, &[A], 60);
    assert!(landing.iter().any(|e| e.contains(&Event::Land)));
    assert!(!landing.iter().any(|e| e.contains(&Event::RollStart)));
    assert_eq!(w.world.player.vel.x, Fixed::ZERO);
    hold(&mut w, &[A, R], run);
    for keys in between {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(w.held, held));
        w.held = held;
        step(&mut w.world, &input);
    }
    let rows = steps(&mut w, 1, |_| on_s.to_vec());
    assert_eq!(rows[0].events, [Event::RollStart]);
    w.world.player.roll.unwrap().size
}

#[test]
fn a_single_run_from_standing_and_a_neutral_press_roll_slowly() {
    // "Rolls", running recently: a single running frame from standing
    // still, and neither Left nor Right on S: slow at every gap, even
    // with a direction held in between.
    let none: &[Action] = &[];
    let right: &[Action] = &[R];
    for gap in 2..=14 {
        let between = vec![none; gap - 1];
        assert_eq!(size_after_run(1, &between, &[A]), RollSize::Slow, "{gap}");
        // With Right held on S it is fast within the window.
        let size = size_after_run(1, &between, &[A, R]);
        assert_eq!(size, RollSize::Fast, "{gap}");
    }
    for gap in [3, 10] {
        let mut before_s = vec![none; gap - 2];
        before_s.push(right);
        let size = size_after_run(1, &before_s, &[A]);
        assert_eq!(size, RollSize::Slow, "{gap}, Right before S");
        let size = size_after_run(1, &vec![right; gap - 1], &[A]);
        assert_eq!(size, RollSize::Slow, "{gap}, Right throughout");
    }
    // Two running frames in a row follow the window rule.
    assert_eq!(size_after_run(2, &[none; 5], &[A]), RollSize::Fast);
}

#[test]
fn facing_in_the_stop() {
    // "Rolls", facing in the stop: a direction pressed on D + 1 to D + 17
    // turns the player on that frame, and the stop's speeds don't change.
    let plain = presses(&[0], usize::MAX, D + 19);
    for k in [1, 3, 6, 14, 16, 17] {
        let mut w = second();
        let rows = steps(&mut w, D + 18, |i| {
            let mut keys = if i == 0 { vec![A] } else { vec![] };
            if i >= D + k {
                keys.push(L);
            }
            keys
        });
        assert_eq!(rows[D + k - 1].facing, Facing::Right, "D + {k}");
        assert_eq!(rows[D + k].facing, Facing::Left, "D + {k}");
        for i in 0..D + 18 {
            assert_eq!(rows[i].vx, plain[i].vx, "D + {k}, step {i}");
            assert_eq!(rows[i].x, plain[i].x, "D + {k}, step {i}");
        }
    }
    // Pressed on D + 18, it starts a normal ground turn.
    let mut w = second();
    let rows = steps(&mut w, D + 24, |i| match i {
        0 => vec![A],
        i if i >= D + 18 => vec![L],
        _ => vec![],
    });
    assert_eq!(rows[D + 21].facing, Facing::Right);
    assert_eq!(rows[D + 22].facing, Facing::Left);
}

#[test]
fn jumping_out_of_a_roll() {
    // "Rolls", jumping out: from T = P + 1 the targets are this
    // character's, with its own jump.
    for p in [1, 5, 12, 22, 23, D] {
        let mut w = second();
        let rows = steps(&mut w, p + 3, |i| match i {
            0 => vec![A, R],
            i if i == p => vec![R, J],
            _ => vec![R],
        });
        assert_eq!(frames_with(&rows, Event::RollEnd), [p], "{p}");
        assert_eq!(frames_with(&rows, Event::JumpStart), [p + 1], "{p}");
        assert_eq!(rows[p + 1].vy, f(-8.3515625), "{p}");
        for t in p + 1..p + 3 {
            // the speed rule toward 2.25, in 1/256 units
            let before = (rows[t - 1].vx.to_f64() * 256.0) as i64;
            let step = (576 - before) / 8;
            let expect = if step == 0 { 576 } else { before + step };
            assert_eq!(rows[t].vx, f(expect as f64 / 256.0), "{p}, step {t}");
        }
    }
}
