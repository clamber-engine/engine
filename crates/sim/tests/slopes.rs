//! 1-in-4 slopes on the `classic-1` preset, from the examples in
//! `spec/behavior/slopes.md`. Heights in the spec are below the flat
//! floor at its left; here that floor is at y = 300.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Event, Facing, Fixed, InputFrame, World, generic_feel, step};

const FLOOR: i32 = 300;

fn fx(x: f64) -> Fixed {
    Fixed::from_f64(x).unwrap()
}

/// 1/256 px per frame.
fn grains(v: Fixed) -> i64 {
    v.to_bits() >> 24
}

/// The synthetic floor of "Examples": columns up to 99 at 0, then down
/// 1 px every 4 columns to the right.
fn height(c: i32) -> i32 {
    if c < 100 { 0 } else { (c - 100) / 4 + 1 }
}

/// A level with the floor of [`height`] for columns 0 to 599, with the
/// player standing at `x`.
fn slope(x: f64) -> World {
    ground(height, x)
}

/// The same with flat floor.
fn flat() -> World {
    ground(|_| 0, 94.26953125)
}

/// A level whose floor in column `c` is `height(c)` px below y = 300,
/// with the player standing at `x`.
fn ground(height: impl Fn(i32) -> i32, x: f64) -> World {
    let mut points: Vec<[i32; 2]> = (0..600)
        .flat_map(|c| [[c, FLOOR + height(c)], [c + 1, FLOOR + height(c)]])
        .collect();
    points.extend([[600, 580], [0, 580]]);
    points.dedup();
    let src = format!(
        "format = 1\nname = \"slopes\"\nsize = [600, 600]\nstart = [{x}, {}]\n\
         [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n",
        FLOOR + height(x as i32)
    );
    let level = Level::from_toml_str(&src, "slopes").unwrap();
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    world.player.pos.x = fx(x);
    world
}

/// Steps once holding `held` after `previous`.
fn press(world: &mut World, previous: &[Action], held: &[Action]) {
    let keys = |a: &[Action]| a.iter().copied().collect::<Actions>();
    step(
        world,
        &InputFrame::single(Frame::after(keys(previous), keys(held))),
    );
}

/// Row after a step: `x`, `y` below the flat floor as in the spec, `vx`
/// and `vy` in 1/256, and whether grounded.
fn row(world: &World) -> (Fixed, Fixed, i64, i64, bool) {
    let p = &world.player;
    (
        p.pos.x,
        p.pos.y - Fixed::from_int(FLOOR),
        grains(p.vel.x),
        grains(p.vel.y),
        p.grounded(),
    )
}

/// Walks right at 2 px per frame from `x` = 96.5, as in "Walking down".
fn walking_down() -> World {
    let mut world = slope(96.5);
    world.player.vel.x = Fixed::from_int(2);
    world
}

#[test]
fn walking_down_follows_the_floor_of_each_column() {
    let mut world = walking_down();
    let r = [Action::Right];
    for (x, y) in [
        (98.5, 0),
        (100.5, 1),
        (102.5, 1),
        (104.5, 2),
        (106.5, 2),
        (108.5, 3),
    ] {
        press(&mut world, &r, &r);
        assert_eq!(row(&world), (fx(x), Fixed::from_int(y), 512, 0, true));
    }

    // Back up the same slope: the same `y` for each column.
    world.player.vel.x = Fixed::from_int(-2);
    world.player.facing = Facing::Left;
    // Left held from before, so this isn't a turn.
    let l = [Action::Left];
    for (x, y) in [(106.5, 2), (104.5, 2), (102.5, 1), (100.5, 1), (98.5, 0)] {
        press(&mut world, &l, &l);
        let (got_x, got_y, _, vy, grounded) = row(&world);
        assert_eq!(
            (got_x, got_y, vy, grounded),
            (fx(x), Fixed::from_int(y), 0, true)
        );
    }
}

#[test]
fn a_jump_from_the_slope_starts_from_the_new_column() {
    // "Jumping from the slope": pressed on frame 5, so T is frame 6.
    let mut world = walking_down();
    let r = [Action::Right];
    let rj = [Action::Right, Action::Jump];
    let mut rows = Vec::new();
    for frame in 1..=9 {
        match frame {
            ..5 => press(&mut world, &r, &r),
            5 => press(&mut world, &r, &rj),
            _ => press(&mut world, &rj, &rj),
        }
        assert_eq!(world.events.contains(&Event::JumpStart), frame == 6);
        rows.push(row(&world));
    }
    let y = |y: f64| fx(y);
    assert_eq!(rows[4], (fx(106.5), y(2.0), 512, 0, true));
    assert_eq!(rows[5], (fx(108.5), y(3.0), 512, -7 * 256, true));
    let air: Vec<_> = rows[6..]
        .iter()
        .map(|&(x, y, _, vy, g)| (x, y, vy, g))
        .collect();
    assert_eq!(
        air,
        [
            (fx(110.5), y(-3.71875), -1720, false),
            (fx(112.5), y(-10.15625), -1648, false),
            (fx(114.5), y(-16.3125), -1576, false),
        ]
    );
}

#[test]
fn landing_across_a_step_lands_on_the_new_column() {
    // "Landing across a step": falling from `x` = 115.5 (floor 4) to
    // 117.5 (floor 5), 2.3125 px above the new column's floor.
    let mut world = slope(50.0);
    let p = &mut world.player;
    p.pos.x = fx(115.5);
    p.pos.y = fx(-3.25) + Fixed::from_int(FLOOR);
    p.vel.x = Fixed::from_int(2);
    p.vel.y = fx(5.5);
    p.ground = None;
    let r = [Action::Right];
    press(&mut world, &r, &r);
    assert!(world.events.contains(&Event::Land));
    let (x, y, _, vy, grounded) = row(&world);
    assert_eq!(x, fx(117.5));
    assert_eq!((y, vy, grounded), (Fixed::from_int(5), 0, true));
}

/// Plays `input(frame)` from standing still facing right at `x` =
/// 94.26953125, giving each frame's `x`, `y` (below the flat floor), `vx`
/// in 1/256, facing and whether there was a turn. Every frame must be
/// grounded with `vy` 0.
fn play(
    world: &mut World,
    frames: usize,
    input: impl Fn(usize) -> Vec<Action>,
) -> Vec<(Fixed, Fixed, i64, Facing, bool)> {
    world.player.pos.x = fx(94.26953125);
    let mut previous = Vec::new();
    (0..frames)
        .map(|frame| {
            let held = input(frame);
            press(world, &previous, &held);
            previous = held;
            let (x, y, vx, vy, grounded) = row(world);
            assert_eq!((vy, grounded), (0, true), "frame {frame}");
            let turn = world.events.contains(&Event::Turn);
            (x, y, vx, world.player.facing, turn)
        })
        .collect()
}

/// `rows` on the slope match `on_flat` but for `y`, which is the floor
/// of each frame's column.
fn same_as_flat(
    rows: &[(Fixed, Fixed, i64, Facing, bool)],
    on_flat: &[(Fixed, Fixed, i64, Facing, bool)],
) {
    for (frame, (s, f)) in rows.iter().zip(on_flat).enumerate() {
        assert_eq!((s.0, s.2, s.3, s.4), (f.0, f.2, f.3, f.4), "frame {frame}");
        assert_eq!(f.1, Fixed::ZERO, "frame {frame}");
        let column = s.0.floor() as i32;
        assert_eq!(s.1, Fixed::from_int(height(column)), "frame {frame}");
    }
}

#[test]
fn turning_around_on_a_slope() {
    // "Turning around on a slope": Right held from frame 0 to 23, then
    // Left; the turn starts on frame 24 and facing changes on 28.
    let input = |frame| {
        if frame < 24 {
            vec![Action::Right]
        } else {
            vec![Action::Left]
        }
    };
    let rows = play(&mut slope(94.26953125), 35, input);
    let r = Facing::Right;
    let l = Facing::Left;
    for (frame, x, y, vx, facing) in [
        (23, 128.65234375, 8, 1.90625, r),
        (24, 130.0703125, 8, 1.41796875, r),
        (25, 131.0625, 8, 0.9921875, r),
        (26, 131.68359375, 8, 0.62109375, r),
        (27, 131.98046875, 8, 0.296875, r),
        (28, 131.9921875, 8, 0.01171875, l),
        (29, 131.75390625, 8, -0.23828125, l),
        (30, 131.296875, 8, -0.45703125, l),
        (31, 130.6484375, 8, -0.6484375, l),
        (32, 129.83203125, 8, -0.81640625, l),
        (33, 128.87109375, 8, -0.9609375, l),
        (34, 127.78125, 7, -1.08984375, l),
    ] {
        let want = (
            fx(x),
            Fixed::from_int(y),
            (vx * 256.0) as i64,
            facing,
            frame == 28,
        );
        assert_eq!(rows[frame], want, "frame {frame}");
    }
    same_as_flat(&rows, &play(&mut flat(), 35, input));
}

#[test]
fn crouching_on_a_slope() {
    // "Crouching on a slope": Right held throughout, Down as well on
    // frames 20 to 24. The crouch lasts to frame 32.
    let input = |frame| {
        if (20..=24).contains(&frame) {
            vec![Action::Down, Action::Right]
        } else {
            vec![Action::Right]
        }
    };
    let rows = play(&mut slope(94.26953125), 46, input);
    for (frame, x, y, vx) in [
        (19, 121.1015625, 6, 1.8515625),
        (20, 122.96875, 6, 1.8671875),
        (21, 124.60546875, 7, 1.63671875),
        (22, 126.0390625, 7, 1.43359375),
        (23, 127.296875, 7, 1.2578125),
        (24, 128.3984375, 8, 1.1015625),
        (25, 129.36328125, 8, 0.96484375),
        (26, 130.2109375, 8, 0.84765625),
        (31, 133.125, 9, 0.44140625),
        (32, 133.51171875, 9, 0.38671875),
        (33, 133.8515625, 9, 0.33984375),
        (34, 134.15234375, 9, 0.30078125),
        (41, 135.48046875, 9, 0.12890625),
        (42, 135.59375, 9, 0.11328125),
        (43, 135.94140625, 9, 0.34765625),
        (44, 136.4921875, 10, 0.55078125),
        (45, 137.22265625, 10, 0.73046875),
    ] {
        let (got_x, got_y, got_vx, _, _) = rows[frame];
        let want = (fx(x), Fixed::from_int(y), (vx * 256.0) as i64);
        assert_eq!((got_x, got_y, got_vx), want, "frame {frame}");
    }
    same_as_flat(&rows, &play(&mut flat(), 46, input));
}

#[test]
fn standing_still_on_a_slope_stays_put() {
    // "Horizontal speed": no sliding, also while crouched.
    let mut world = slope(110.0);
    for held in [vec![], vec![Action::Down]] {
        for _ in 0..30 {
            press(&mut world, &held, &held);
            assert_eq!(row(&world), (fx(110.0), Fixed::from_int(3), 0, 0, true));
        }
    }
}
