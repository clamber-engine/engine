//! The second character's movement in Classic 1
//! (spec/behavior/second-character.md), with the second character under
//! control from the start.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{Event, InputFrame, World, generic_feel, step};

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

/// A world on `FLAT` with Classic 1's two roster slots, the second under
/// control, and what was held on its last step.
struct Play {
    world: World,
    held: Actions,
}

fn second() -> Play {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
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
