//! The second character's movement in Classic 1
//! (spec/behavior/second-character.md), with the second character under
//! control from the start.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{
    Event, Facing, Gait, InputFrame, ROLL, RollSize, State, World, generic_feel, step,
};

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
    second_on(FLAT)
}

/// [`second`] on the level `src`.
fn second_on(src: &str) -> Play {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::from([(ROLL.to_string(), BTreeMap::new())]),
    });
    let level = Arc::new(Level::from_toml_str(src, "flat").unwrap());
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

/// Walking left with Left held from frame 0 and Down also held on
/// `down` frames: the speed after each frame, to frame 119.
fn crouch_speeds(down: impl Fn(usize) -> bool) -> Vec<Fixed> {
    let mut w = second();
    (0..120)
        .map(|frame| {
            let keys: &[Action] = if down(frame) {
                &[L, Action::Down]
            } else {
                &[L]
            };
            hold(&mut w, keys, 1);
            w.world.player.vel.x.abs()
        })
        .collect()
}

/// The frames from `from` on where the speed starts to grow.
fn grows(speeds: &[Fixed], from: usize) -> Vec<usize> {
    let growing = |n: usize| speeds[n] > speeds[n - 1];
    (from..speeds.len())
        .filter(|&n| growing(n) && !growing(n - 1))
        .collect()
}

#[test]
fn the_shortest_crouch_is_nine_frames() {
    // "Values", crouching, short press: Down also held on frame 40 only.
    // The crouch is frames 40 to 48, U is 49, the target is 0 to 61 and
    // the speed grows again from 62. Held for 3 or 8 frames, the same.
    for held in [1, 3, 8] {
        let speeds = crouch_speeds(|n| (40..40 + held).contains(&n));
        assert_eq!(speeds[40], f(2.25), "{held}");
        assert_eq!(grows(&speeds, 41), [62], "{held}");
    }
    // Held for 10, 12 or 14 frames, U is the frame Down is released.
    for held in [10, 12, 14] {
        let speeds = crouch_speeds(|n| (40..40 + held).contains(&n));
        assert_eq!(grows(&speeds, 41), [40 + held + 13], "{held}");
    }
}

#[test]
fn down_again_on_u_plus_1_to_u_plus_12_starts_a_new_crouch() {
    // Down held on 40 to 59, so U is 60, then again for a frame on U + k:
    // a new 9-frame crouch whose D has the target 0 too, up to U + 12;
    // on U + 13 its D has the normal target.
    for k in 1..=13 {
        let d = 60 + k;
        let speeds = crouch_speeds(|n| (40..60).contains(&n) || n == d);
        let want = if k <= 12 {
            vec![d + 22]
        } else {
            vec![d, d + 22]
        };
        assert_eq!(grows(&speeds, 41), want, "U + {k}");
    }
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
    y: Fixed,
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
                y: p.pos.y,
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

/// Flat ground at y = 400 up to x = 1600, then a short steep face falling
/// right (four columns, 4 px apart) and flat floor `drop` px below
/// y = 400 beyond it, with the player standing at `x`.
fn faced_ledge(drop: i32, x: f64) -> Play {
    let mut points = vec![[0, 400], [1600, 400]];
    for c in 1600..1604 {
        let y = 402 + 4 * (c - 1600);
        points.extend([[c, y], [c + 1, y]]);
    }
    points.extend([
        [1604, 400 + drop],
        [4096, 400 + drop],
        [4096, 599],
        [0, 599],
    ]);
    let src = format!(
        "format = 1\nname = \"ledge\"\nsize = [4096, 600]\nstart = [{x}, 400]\n\
         [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n"
    );
    second_on(&src)
}

/// The edge of [`faced_ledge`]'s top: the face starts in this column.
const EDGE: f64 = 1600.0;

/// On [`faced_ledge`] with `drop`, walking right from x = 1400, then
/// Action and Right on frame S and Right alone after it, with S chosen
/// so that the roll leaves the ground on S + `lead`. The rows from S on,
/// `n` of them, with `adjust(frame, play)` called before each frame, and
/// the player's `y` on S - 1.
fn roll_off_faced_ledge(
    drop: i32,
    lead: usize,
    n: usize,
    adjust: impl Fn(usize, &mut Play),
) -> (Vec<Row>, Fixed) {
    roll_off_faced_ledge_jumping(drop, lead, n, 0..0, adjust)
}

/// [`roll_off_faced_ledge`], with Jump held too on the frames in `jump`,
/// counted from S.
fn roll_off_faced_ledge_jumping(
    drop: i32,
    lead: usize,
    n: usize,
    jump: std::ops::Range<usize>,
    adjust: impl Fn(usize, &mut Play),
) -> (Vec<Row>, Fixed) {
    roll_off_faced_ledge_pressing(drop, lead, n, |f| jump.contains(&f), adjust)
}

/// [`roll_off_faced_ledge`], with Jump held too on the frames, counted
/// from S, for which `jump` is true.
fn roll_off_faced_ledge_pressing(
    drop: i32,
    lead: usize,
    n: usize,
    jump: impl Fn(usize) -> bool,
    adjust: impl Fn(usize, &mut Play),
) -> (Vec<Row>, Fixed) {
    let held = |f: usize, s: usize| {
        let mut keys = if f == s { vec![A, R] } else { vec![R] };
        if f >= s && jump(f - s) {
            keys.push(J);
        }
        keys
    };
    // Walking rolls from starts moved in steps of 1/16 px, to find the
    // leave frames on or right after S, with S starting off the face.
    for nudge in 0..32 {
        let start = || {
            let mut w = faced_ledge(drop, 1400.0);
            w.world.player.pos.x += f(f64::from(nudge) / 16.0);
            w
        };
        for s in 40..120 {
            let rows = steps(&mut start(), s + lead + 1, |i| held(i, s));
            if frames_with(&rows, Event::LeaveGround).first() != Some(&(s + lead)) {
                continue;
            }
            assert_eq!(rows[s - 1].vx, f(2.25), "walking before S");
            // No roll starts on a frame that starts on the face
            // (steep-faces.md, "When a roll starts on a face").
            if frames_with(&rows, Event::RollStart) != [s] {
                continue;
            }
            let mut w = start();
            let before = steps(&mut w, s, |i| held(i, s));
            let rows = (0..n)
                .map(|i| {
                    adjust(i, &mut w);
                    steps(&mut w, 1, |_| held(s + i, s)).pop().unwrap()
                })
                .collect();
            return (rows, before[s - 1].y);
        }
    }
    panic!("no roll leaves the ground on S + {lead}");
}

#[test]
fn a_roll_off_a_ledge_hovers_by_when_it_leaves_the_ground() {
    // rolling-off-ledges.md, "The second character": which frames after
    // L hover by L - S, two more than the first character's. A hover
    // frame moves down 1/256 and ends with `vy` 1/256.
    let drop = 3.4375;
    let hover = 0.00390625;
    for lead in 0..=14 {
        let hovers = |after: usize| match lead {
            0 => (1..=10).contains(&after),
            1..=9 => after <= 11,
            10 => after == 0,
            _ => false,
        };
        let (rows, y0) = roll_off_faced_ledge(160, lead, lead + 18, |_, _| {});
        let mut vy = hover;
        for after in 0..18 {
            let moved = if hovers(after) {
                vy = hover;
                hover
            } else if after == 0 {
                vy = hover;
                drop
            } else {
                vy += 0.4375;
                vy
            };
            let i = lead + after;
            assert_eq!(rows[i].state, State::Roll, "lead {lead}, L + {after}");
            let before = if i == 0 { y0 } else { rows[i - 1].y };
            assert_eq!(
                (rows[i].y - before, rows[i].vy),
                (f(moved), f(vy)),
                "lead {lead}, L + {after}"
            );
        }
    }
}

#[test]
fn roll_off_a_ledge_hover_example() {
    // rolling-off-ledges.md, "The second character", the example of a
    // hover: L = S + 7, Action tapped. Below the height on L - 1.
    let (rows, _) = roll_off_faced_ledge(160, 7, 80, |_, _| {});
    let below = |after: usize| rows[7 + after].y - rows[6].y;
    for (after, fallen, vy) in [
        (0, 0.00390625, 0.00390625),
        (11, 0.046875, 0.00390625),
        (12, 0.48828125, 0.44140625),
        (13, 1.3671875, 0.87890625),
    ] {
        assert_eq!((below(after), rows[7 + after].vy), (f(fallen), f(vy)));
    }
    let land = frames_with(&rows, Event::Land)[0];
    assert!(rows[..land + 25].iter().all(|r| r.state == State::Roll));
    assert_eq!(frames_with(&rows, Event::RollEnd), [land + 25]);
}

#[test]
fn jumping_during_a_roll_hover_example() {
    // rolling-off-ledges.md, "The second character", jumping during the
    // fall: the hover example (L = S + 7) with Jump pressed on L + 3. P
    // stays a hover frame, then this character's jump.
    let (l, p) = (7, 10);
    let (rows, _) = roll_off_faced_ledge_jumping(160, l, p + 4, p..p + 12, |_, _| {});
    for i in l..=p {
        assert_eq!(
            (rows[i].y - rows[i - 1].y, rows[i].vy),
            (f(0.00390625), f(0.00390625)),
            "L + {}",
            i - l
        );
    }
    assert_eq!(frames_with(&rows, Event::RollEnd), [p]);
    assert_eq!(frames_with(&rows, Event::JumpStart), [p + 1]);
    for (i, moved, vy) in [
        (p, 0.00390625, 0.00390625),
        (p + 1, 0.35546875, -8.3515625),
        (p + 2, -8.0, -8.0),
        (p + 3, -7.6484375, -7.6484375),
    ] {
        let row = &rows[i];
        assert_eq!(
            (row.y - rows[i - 1].y, row.vy),
            (f(moved), f(vy)),
            "P + {}",
            i - p
        );
    }
    // From T, `vx` follows the speed rule toward 2.25. (The example's
    // `vx` itself depends on the speed on L, which isn't covered on
    // ledges with a face.)
    for t in p + 1..p + 4 {
        let before = (rows[t - 1].vx.to_f64() * 256.0) as i64;
        let expect = before + (576 - before) / 8;
        assert_eq!(rows[t].vx, f(expect as f64 / 256.0), "step {t}");
    }
    // Tapped on P alone.
    let (rows, _) = roll_off_faced_ledge_jumping(160, l, p + 3, p..p + 1, |_, _| {});
    assert_eq!(rows[p + 1].y - rows[p].y, f(0.44140625));
    assert_eq!(rows[p + 2].y - rows[p + 1].y, f(-7.9140625));
}

#[test]
fn jumping_on_every_hover_frame() {
    // rolling-off-ledges.md, "The second character", jumping during the
    // fall: a press on a hover frame keeps it one, and a press on an L
    // that isn't one moves down 3.3515625. `vy` is -8.3515625 on T.
    for lead in 1..=12 {
        let hovers = |after: usize| match lead {
            1..=9 => after <= 11,
            10 => after == 0,
            _ => false,
        };
        for after in 0..=12 {
            let p = lead + after;
            let (rows, _) = roll_off_faced_ledge_jumping(160, lead, p + 3, p..p + 12, |_, _| {});
            let fall_vy = rows[p - 1].vy;
            let (moved, vy) = if hovers(after) {
                (f(0.00390625), f(0.00390625))
            } else if after == 0 {
                (f(3.3515625), f(0.00390625))
            } else {
                let vy = (fall_vy + f(0.3515625)).min(f(8.0));
                (vy, vy)
            };
            let ctx = format!("lead {lead}, P = L + {after}");
            assert_eq!(
                (rows[p].y - rows[p - 1].y, rows[p].vy),
                (moved, vy),
                "{ctx}"
            );
            assert_eq!(
                (rows[p + 1].y - rows[p].y, rows[p + 1].vy),
                ((vy + f(0.3515625)).min(f(8.0)), f(-8.3515625)),
                "{ctx}"
            );
        }
    }
}

#[test]
fn landing_after_a_roll_jump_in_the_air() {
    // jump-input.md and ground-speed.md, "The second character": a jump
    // out of a roll pressed in the air, on a hover frame or not, lands
    // with M = L, so E = L + 2, and with Right held alone the player
    // walks from L + 17.
    for (lead, after) in [(7, 3), (7, 12), (0, 4), (10, 2)] {
        let p = lead + after;
        let ctx = format!("lead {lead}, P = L + {after}");
        let (rows, _) = roll_off_faced_ledge_jumping(160, lead, 150, p..p + 12, |_, _| {});
        assert_eq!(frames_with(&rows, Event::JumpStart), [p + 1], "{ctx}");
        let l = frames_with(&rows, Event::Land)[0];
        let gaits = std::cell::RefCell::new(Vec::new());
        let (rows, _) = roll_off_faced_ledge_jumping(160, lead, l + 19, p..p + 12, |_, w| {
            gaits.borrow_mut().push(w.world.player.gait)
        });
        assert_eq!(frames_with(&rows, Event::Land), [l], "{ctx}");
        // The gait after each frame from L on.
        let gaits = &gaits.borrow()[l + 1..];
        assert!(
            gaits[..17].iter().all(|g| matches!(g, Gait::Landing(_))),
            "{ctx}: {gaits:?}"
        );
        assert!(matches!(gaits[17], Gait::Walking(_)), "{ctx}: {gaits:?}");

        // Jump pressed again on L - 1 and held.
        let (rows, _) = roll_off_faced_ledge_pressing(
            160,
            lead,
            l + 6,
            |f| (p..p + 12).contains(&f) || f + 1 >= l,
            |_, _| {},
        );
        assert_eq!(frames_with(&rows, Event::Land), [l], "{ctx}");
        assert_eq!(
            frames_with(&rows, Event::JumpStart),
            [p + 1, l + 2],
            "{ctx}"
        );
    }
}

#[test]
fn a_roll_off_a_ledge_ends_25_frames_after_landing() {
    // rolling-off-ledges.md, "The second character", when the roll ends:
    // D = G + 25, except D = G after a fall of exactly 25 frames; falls of
    // 24 or 26 frames end at G + 25.
    let ends = |lead: usize, nudge: i32| {
        let (rows, _) = roll_off_faced_ledge(160, lead, 100, |i, w| {
            if i == lead + 5 {
                w.world.player.pos.y += Fixed::from_int(nudge);
            }
        });
        let leave = frames_with(&rows, Event::LeaveGround)[0];
        let land = frames_with(&rows, Event::Land)[0];
        assert!(rows[..land].iter().all(|r| r.state == State::Roll));
        let end = frames_with(&rows, Event::RollEnd)[0];
        (land - leave, end - land)
    };
    let falls: Vec<_> = (-60..=40).map(|px| ends(12, px)).collect();
    for fall in 24..=26 {
        let end = if fall == 25 { 0 } else { 25 };
        assert!(falls.contains(&(fall, end)), "{fall}-frame fall");
    }
    assert!(
        falls
            .iter()
            .all(|&(fall, end)| end == if fall == 25 { 0 } else { 25 })
    );
    // With a hover too.
    let falls: Vec<_> = (40..=140).map(|px| ends(3, px)).collect();
    assert!(falls.contains(&(25, 0)));
    assert!(
        falls
            .iter()
            .all(|&(fall, end)| end == if fall == 25 { 0 } else { 25 })
    );
}

#[test]
fn roll_off_a_ledge_example() {
    // rolling-off-ledges.md, "The second character", the first example:
    // standing about 33 px in front of the ledge, Right held and Action
    // pressed on frame 0. Held, the roll carries on to D + 6; tapped, it
    // stops on D = G + 25.
    for held in [true, false] {
        let mut w = faced_ledge(60, EDGE - 33.0);
        let rows = steps(&mut w, 80, |i| {
            if i == 0 || held { vec![A, R] } else { vec![R] }
        });
        assert_eq!(frames_with(&rows, Event::RollStart), [0]);
        let leave = frames_with(&rows, Event::LeaveGround)[0];
        let land = frames_with(&rows, Event::Land)[0];
        assert_eq!((leave, land), (16, 31), "held {held}");
        let d = land + 25;
        assert!(rows[..d].iter().all(|r| r.state == State::Roll));
        let end = if held { d + 6 } else { d };
        assert_eq!(frames_with(&rows, Event::RollEnd), [end], "held {held}");
    }
}

#[test]
fn a_roll_stopped_at_the_top_of_a_face_pushes_off_to_size_3() {
    // rolling-off-ledges.md, "The second character", pushing off: at the
    // top of the face within reach of the edge, Action alone stays put
    // until D; with Right from S + 1 it pushes off at 2 on S + 1, and
    // holding Right then makes the roll's size 3, not 3.375.
    let x = EDGE - 10.0;
    let mut w = faced_ledge(160, x);
    let rows = steps(&mut w, D + 1, |i| if i == 0 { vec![A] } else { vec![] });
    assert!(rows[..D].iter().all(|r| r.state == State::Roll));
    assert!(rows.iter().all(|r| r.x == f(x) && r.vx == Fixed::ZERO));
    assert_eq!(frames_with(&rows, Event::RollEnd), [D]);

    let mut w = faced_ledge(160, x);
    let rows = steps(&mut w, 3, |i| if i == 0 { vec![A] } else { vec![R] });
    assert_eq!((rows[0].x, rows[0].vx), (f(x), Fixed::ZERO));
    assert_eq!(rows[1].vx, f(2.0));
    assert_eq!(w.world.player.roll.unwrap().speed, f(3.0));
}

/// Mode and step, or "turn", and the stride counter in 1/512 of a step,
/// after each of `keys(frame)` held for a frame.
fn gaits(play: &mut Play, n: usize, keys: impl Fn(usize) -> Vec<Action>) -> Vec<(String, i64)> {
    (0..n)
        .map(|i| {
            hold(play, &keys(i), 1);
            let p = &play.world.player;
            let counter = |left: Fixed| (left.to_f64() * 512.0) as i64;
            match (p.turn, p.gait) {
                (Some(_), _) => ("turn".to_string(), 0),
                (None, Gait::Walking(s)) => (format!("walk {}", s.step), counter(s.left)),
                (None, Gait::Running(s)) => (format!("run {}", s.step), counter(s.left)),
                (None, g) => (format!("{g:?}"), 0),
            }
        })
        .collect()
}

#[test]
fn stride_cycle_example() {
    // ground-speed.md, "The second character's stride cycle": Right and
    // Action held from frame 0, standing still facing right.
    let mut w = second();
    let mut v = Vec::new();
    let mut gait = Vec::new();
    for _ in 0..14 {
        gait.extend(gaits(&mut w, 1, |_| vec![R, A]));
        v.push((w.world.player.vel.x.to_f64() * 256.0) as i64);
    }
    assert_eq!(
        v,
        [
            108, 202, 284, 356, 419, 474, 522, 564, 601, 633, 661, 686, 708, 727
        ]
    );
    let want = [
        ("walk 1", 512),
        ("walk 1", 256),
        ("walk 2", 512),
        ("walk 2", 256),
        ("run 1", 512),
        ("run 1", 256),
        ("run 2", 512),
        ("run 2", 256),
        ("run 3", 512),
        ("run 3", 256),
        ("run 4", 512),
        ("run 4", 256),
        ("run 5", 503),
        ("run 5", 232),
    ];
    let want: Vec<(String, i64)> = want.iter().map(|&(m, c)| (m.to_string(), c)).collect();
    assert_eq!(gait, want);
}

#[test]
fn stride_cycles_of_16_walking_and_13_running_steps() {
    let mut w = second();
    let walk = gaits(&mut w, 40, |_| vec![R]);
    let mut w = second();
    let run = gaits(&mut w, 120, |_| vec![R, A]);
    for (rows, last, first) in [(walk, "walk 16", "walk 1"), (run, "run 13", "run 1")] {
        let after: Vec<&str> = rows
            .windows(2)
            .filter(|p| p[0].0 == last && p[1].0 != last)
            .map(|p| p[1].0.as_str())
            .collect();
        assert!(!after.is_empty(), "{rows:?}");
        assert!(after.iter().all(|&m| m == first), "{after:?}");
    }
}

/// Right and Action held for `run` frames, to full speed, then
/// `then(i)` held on frame i after: the gaits, with v after each frame.
fn let_go(run: usize, then: impl Fn(usize) -> Vec<Action>) -> Vec<(String, i64)> {
    let mut w = second();
    gaits(&mut w, run, |_| vec![R, A]);
    (0..80)
        .map(|i| {
            let (mode, _) = gaits(&mut w, 1, |_| then(i)).remove(0);
            (mode, (w.world.player.vel.x.abs().to_f64() * 256.0) as i64)
        })
        .collect()
}

#[test]
fn running_with_action_let_go_walks_only_as_step_13_ends() {
    // Action let go after running at full speed with Right held:
    // running goes on at any speed until running step 13 ends with
    // v < 640.
    let mut slow_runs = 0;
    for run in 60..90 {
        let rows = let_go(run, |_| vec![R]);
        let walk = rows.iter().position(|(m, _)| m == "walk 1").unwrap();
        assert!(rows[..walk].iter().all(|(m, _)| m.starts_with("run ")));
        assert_eq!(rows[walk - 1].0, "run 13", "{rows:?}");
        assert!(rows[walk].1 < 640, "{rows:?}");
        slow_runs += rows[..walk].iter().filter(|(_, v)| *v < 640).count();
    }
    assert!(slow_runs > 0, "always below 640 only as step 13 ends");
}

#[test]
fn walking_at_the_end_of_step_13_comes_before_its_turn_check() {
    // The same, with Left held on the frame walking starts: walking
    // starts all the same, and no turn.
    let walk = let_go(60, |_| vec![R])
        .iter()
        .position(|(m, _)| m == "walk 1")
        .unwrap();
    let rows = let_go(60, |i| if i == walk { vec![L] } else { vec![R] });
    assert_eq!(rows[walk].0, "walk 1", "{rows:?}");
    assert!(rows.iter().all(|(m, _)| m != "turn"), "{rows:?}");
}

/// After a slow roll pressed on frame 0 with Action alone held to D + 5,
/// `then` held from D + 6: the facing on D + 5, then the turn, facing,
/// mode and counter on D + 6 and D + 7.
fn after_carrying_on(
    play: &mut Play,
    then: &[Action],
) -> (Facing, Vec<(bool, Facing, String, i64)>) {
    steps(play, D + 6, |_| vec![A]);
    let before = play.world.player.facing;
    let rows = (0..2)
        .map(|_| {
            let (mode, counter) = gaits(play, 1, |_| then.to_vec()).remove(0);
            let p = &play.world.player;
            (p.turn.is_some(), p.facing, mode, counter)
        })
        .collect();
    (before, rows)
}

#[test]
fn walking_starts_on_d_plus_6_after_carrying_on() {
    // ground-speed.md, "After a roll": walking starts on D + 6 at step 1
    // (counter 512) whatever is held, facing a direction held then
    // without a turn. From D + 7 the walking rules apply.
    for (then, facing) in [
        (&[][..], Facing::Right),
        (&[A], Facing::Right),
        (&[R], Facing::Right),
        (&[A, R], Facing::Right),
        (&[L], Facing::Left),
        (&[A, L], Facing::Left),
    ] {
        let mut w = second();
        let (before, rows) = after_carrying_on(&mut w, then);
        assert_eq!(before, Facing::Right, "{then:?}");
        assert_eq!(
            rows[0],
            (false, facing, "walk 1".to_string(), 512),
            "{then:?}"
        );
        assert_eq!(rows[1].2, "walk 1", "{then:?}");
        assert!(!rows[1].0, "{then:?}");
    }
}

#[test]
fn a_stopped_roll_carrying_on_stands_on_d_plus_6() {
    // "After a roll": when `vx` is already 0 on D + 6, as for a roll
    // stopped at the top of a face, the player stands on D + 6 instead.
    let x = EDGE - 10.0;
    let mut w = faced_ledge(160, x);
    let rows = steps(&mut w, D + 6, |_| vec![A]);
    assert!(
        rows.iter()
            .all(|r| r.state == State::Roll && r.vx == Fixed::ZERO)
    );
    let (mode, _) = gaits(&mut w, 1, |_| vec![A]).remove(0);
    assert!(w.world.events.contains(&Event::RollEnd));
    assert_eq!(mode, "Standing");
    assert_eq!(w.world.player.vel.x, Fixed::ZERO);
}
