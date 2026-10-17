//! Slopes from `spec/behavior/slopes.md`: 1-in-4 slopes on the
//! `classic-1` preset, and "In `title-2`" on `classic-2`. Heights in the
//! spec are below the flat floor at its left; here that floor is at
//! y = 300.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Event, Facing, Fixed, InputFrame, State, World, generic_feel, step};

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

/// A `classic-1` level whose floor in column `c` is `height(c)` px below
/// y = 300, with the player standing at `x`.
fn ground(height: impl Fn(i32) -> i32, x: f64) -> World {
    ground_on("classic-1", 600, height, x)
}

/// The same on `preset`, with columns 0 to `width` - 1.
fn ground_on(preset: &str, width: i32, height: impl Fn(i32) -> i32, x: f64) -> World {
    let mut points: Vec<[i32; 2]> = (0..width)
        .flat_map(|c| [[c, FLOOR + height(c)], [c + 1, FLOOR + height(c)]])
        .collect();
    points.extend([[width, 580], [0, 580]]);
    points.dedup();
    let src = format!(
        "format = 1\nname = \"slopes\"\nsize = [{width}, 600]\nstart = [{x}, {}]\n\
         [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n",
        FLOOR + height(x as i32)
    );
    let level = Level::from_toml_str(&src, "slopes").unwrap();
    let mut world = World::new(Arc::new(level), Arc::new(generic_feel(preset).unwrap()));
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

/// Walks left up the slope at 2 px per frame from `x`, with jump pressed
/// on frame 3, so T is frame 4. Gives the rows of T and T + 1.
fn jump_walking_up(x: f64) -> [(Fixed, Fixed, i64, i64, bool); 2] {
    let mut world = slope(x);
    world.player.vel.x = Fixed::from_int(-2);
    world.player.facing = Facing::Left;
    let l = [Action::Left];
    let lj = [Action::Left, Action::Jump];
    let mut rows = Vec::new();
    for frame in 1..=5 {
        match frame {
            ..3 => press(&mut world, &l, &l),
            3 => press(&mut world, &l, &lj),
            _ => press(&mut world, &lj, &lj),
        }
        assert_eq!(world.events.contains(&Event::JumpStart), frame == 4);
        rows.push(row(&world));
    }
    [rows[3], rows[4]]
}

#[test]
fn a_jump_up_the_slope_starts_from_the_higher_column() {
    // "Jumping from a slope": going uphill, T + 1 starts from the floor of
    // the column under its new `x` when that is higher than `y` on T.
    let y = Fixed::from_int;
    // T + 1 reaches a higher column (floor 2 under 106.5): 1 px higher
    // than on flat floor.
    assert_eq!(
        jump_walking_up(116.5),
        [
            (fx(108.5), y(3), -512, -1792, true),
            (fx(106.5), fx(-4.71875), -512, -1720, false),
        ]
    );
    // T + 1 stays at T's height (floor 3 under 108.5): from `y` on T.
    assert_eq!(
        jump_walking_up(118.5),
        [
            (fx(110.5), y(3), -512, -1792, true),
            (fx(108.5), fx(-3.71875), -512, -1720, false),
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

/// The synthetic floor of "Examples (`title-2`)": columns 0 to 15 flat
/// at 0, then two sloped pieces going down 1 px every 2 columns to the
/// right (16 to 47), then flat at 16.
fn height_2(c: i32) -> i32 {
    match c {
        ..16 => 0,
        16..48 => (c - 16) / 2,
        _ => 16,
    }
}

/// A `classic-2` level with the floor of [`height_2`], with the player
/// standing at `x`.
fn slope_2(x: f64) -> World {
    ground_on("classic-2", 600, height_2, x)
}

/// `y` below the flat floor at the left, as in the spec.
fn below(world: &World) -> Fixed {
    world.player.pos.y - Fixed::from_int(FLOOR)
}

/// 1/256 px.
fn grain() -> Fixed {
    Fixed::from_ratio_pow2(1, 8)
}

#[test]
fn classic_2_walking_down_follows_the_floor_of_each_column() {
    // "Examples (`title-2`)", "Walking down": right at 2.25 px per frame
    // from `x` = 30.5, over a 1-in-2 slope.
    let mut world = slope_2(30.5);
    world.player.vel.x = fx(2.25);
    let r = [Action::Right];
    for (x, y) in [(32.75, 8), (35.0, 9), (37.25, 10), (39.5, 11)] {
        press(&mut world, &r, &r);
        assert_eq!(row(&world), (fx(x), Fixed::from_int(y), 576, 0, true));
    }
}

/// Runs at `vx` from `x`, Action held with the direction since before,
/// with jump pressed on G, the first frame, and held. Every frame up to
/// T has `y` on the floor of its column. Gives `x`, `y` and `vy` on T
/// and T + 1.
fn classic_2_takeoff(x: f64, vx: f64) -> [(Fixed, Fixed, Fixed); 2] {
    let mut world = slope_2(x);
    let p = &mut world.player;
    p.vel.x = fx(vx);
    p.ran = true;
    let dir = if vx < 0.0 {
        p.facing = Facing::Left;
        Action::Left
    } else {
        Action::Right
    };
    let run = [dir, Action::Action];
    let jump = [dir, Action::Action, Action::Jump];
    press(&mut world, &run, &jump);
    for frame in ["G", "G + 1", "T"] {
        if frame != "G" {
            press(&mut world, &jump, &jump);
        }
        let column = world.player.pos.x.floor() as i32;
        assert!(world.player.grounded(), "{frame}");
        assert_eq!(below(&world), Fixed::from_int(height_2(column)), "{frame}");
        assert_eq!(world.events.contains(&Event::JumpStart), frame == "T");
    }
    let t = (world.player.pos.x, below(&world), world.player.vel.y);
    press(&mut world, &jump, &jump);
    assert!(!world.player.grounded());
    [t, (world.player.pos.x, below(&world), world.player.vel.y)]
}

#[test]
fn classic_2_jump_takes_off_from_the_higher_floor() {
    // "Jumping from a slope (`title-2`)" and "Examples (`title-2`)",
    // "Taking off": T + 1 starts from the higher of `y` on T and the floor
    // under T + 1's `x`.
    let on = |x: f64, y: i32, vy: f64| (fx(x), Fixed::from_int(y), fx(vy));
    let air = |x: f64, y: f64, vy: f64| (fx(x), fx(y), fx(vy));
    // Downhill, from `y` on T.
    assert_eq!(
        classic_2_takeoff(25.875, 3.375),
        [on(36.0, 10, -8.21875), air(39.375, 2.1328125, -7.8671875)]
    );
    // Uphill, from the floor under T + 1's `x`, 1 px higher than on T.
    assert_eq!(
        classic_2_takeoff(49.5, -3.375),
        [on(39.375, 11, -8.21875), air(36.0, 2.1328125, -7.8671875)]
    );
}

/// One frame falling at `vy` from `x` and `y` (below the flat floor at
/// the left), moving right at 2.25 with Right held.
fn classic_2_fall_from(x: f64, y: Fixed, vy: f64) -> World {
    let mut world = slope_2(x);
    let p = &mut world.player;
    p.pos.y = y + Fixed::from_int(FLOOR);
    p.vel.x = fx(2.25);
    p.vel.y = fx(vy);
    p.ground = None;
    p.state = State::Fall;
    let r = [Action::Right];
    press(&mut world, &r, &r);
    world
}

/// Whether that frame lands, and `y` after it.
fn classic_2_falls(x: f64, y: Fixed, vy: f64) -> (bool, Fixed) {
    let world = classic_2_fall_from(x, y, vy);
    assert_eq!(world.player.grounded(), world.events.contains(&Event::Land));
    (world.player.grounded(), below(&world))
}

#[test]
fn classic_2_lands_within_3_px_of_a_sloped_piece() {
    // "Landing (`title-2`)" and "Examples (`title-2`)", "Landing on a
    // sloped piece": from 40.5 to 42.75 (floor 13), moving by 8 to 12.
    let int = Fixed::from_int;
    assert_eq!(classic_2_falls(40.5, int(4), 8.0), (true, int(13)));
    // Exactly 3 px above lands; 1/256 px short of it doesn't.
    assert_eq!(classic_2_falls(40.5, int(2), 8.0), (true, int(13)));
    assert_eq!(
        classic_2_falls(40.5, int(2) - grain(), 8.0),
        (false, int(10) - grain())
    );
    // The first columns of a slope, still at the flat floor's height,
    // belong to a sloped piece: from 14.5 to 16.75 (floor 0), moving by 8
    // to 3 px above it.
    assert_eq!(classic_2_falls(14.5, int(-11), 8.0), (true, int(0)));
}

#[test]
fn classic_2_lands_only_on_reaching_a_flat_piece() {
    // "Examples (`title-2`)", "Not landing on a flat piece": from 50.5 to
    // 52.75 (floor 16), moving by 8 to 0.5 px above it, then landing on
    // the next frame.
    let int = Fixed::from_int;
    let mut world = classic_2_fall_from(50.5, fx(7.5), 8.0);
    assert_eq!((world.player.grounded(), below(&world)), (false, fx(15.5)));
    let r = [Action::Right];
    press(&mut world, &r, &r);
    assert!(world.events.contains(&Event::Land));
    assert_eq!(row(&world), (fx(55.0), int(16), 576, 0, true));
    // Reaching the floor exactly lands; 1/256 px short of it doesn't.
    assert_eq!(classic_2_falls(50.5, int(8), 8.0), (true, int(16)));
    assert_eq!(
        classic_2_falls(50.5, int(8) - grain(), 8.0),
        (false, int(16) - grain())
    );
    // No margin on the first flat column after the slope: from 45.75 to
    // 48 (floor 16), moving by 8 to 2 px above it.
    assert_eq!(classic_2_falls(45.75, int(6), 8.0), (false, int(14)));
}

/// Hills of 16-column pieces, repeating every 128 columns: flat, two
/// pieces down 1 px every 2 columns, flat, two up 1 px every 4 columns
/// (the first columns still at the flat floor's height), one up 1 px
/// every 2 columns and flat again.
fn hills(c: i32) -> i32 {
    let i = c.rem_euclid(16);
    match c.rem_euclid(128) / 16 {
        1 => i / 2,
        2 => 8 + i / 2,
        3 => 16,
        4 => 16 - i / 4,
        5 => 12 - i / 4,
        6 => 8 - i / 2,
        _ => 0,
    }
}

#[test]
fn classic_2_slopes_dont_change_horizontal_motion() {
    // "Horizontal speed (`title-2`)": the same inputs on hills and on flat
    // floor give the same `vx`, target, state and facing, and `vy`, on
    // every frame up to the first one grounded on one floor and not on
    // the other. "Grounded on a slope (`title-2`)": `y` is the floor of
    // the column on each grounded frame.
    use Action::{Action as Act, Down, Jump, Left, Right};
    let choices: [&[Action]; 12] = [
        &[],
        &[Right],
        &[Left],
        &[Right, Act],
        &[Left, Act],
        &[Act],
        &[Down],
        &[Down, Right],
        &[Right, Jump],
        &[Left, Act, Jump],
        &[Jump],
        &[Right, Act, Jump],
    ];
    let mut seed = 0x2545_f491_u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed >> 8
    };
    let (mut on_slope, mut rolling) = (0, 0);
    for run in 0..80 {
        let x = 320.0 + f64::from(run * 3) + 0.25 * f64::from(run % 4);
        let mut on_hills = ground_on("classic-2", 768, hills, x);
        let mut on_flat = ground_on("classic-2", 768, |_| 0, x);
        let (mut previous, mut held): (&[Action], &[Action]) = (&[], &[]);
        let mut left = 0;
        for frame in 0..300 {
            if left == 0 {
                held = choices[next() as usize % choices.len()];
                left = 1 + next() % 30;
            }
            left -= 1;
            press(&mut on_hills, previous, held);
            press(&mut on_flat, previous, held);
            previous = held;
            let (h, f) = (&on_hills.player, &on_flat.player);
            // The ends of the level aren't part of this.
            let inside = |x: Fixed| (64..704).contains(&x.floor());
            if h.grounded() != f.grounded() || !inside(h.pos.x) || !inside(f.pos.x) {
                break;
            }
            assert_eq!(
                (h.vel.x, h.vel.y, h.target, h.state, h.facing),
                (f.vel.x, f.vel.y, f.target, f.state, f.facing),
                "run {run}, frame {frame}"
            );
            if h.grounded() {
                let column = h.pos.x.floor() as i32;
                let height = Fixed::from_int(hills(column));
                assert_eq!(below(&on_hills), height, "run {run}, frame {frame}");
                if !matches!(column.rem_euclid(128) / 16, 0 | 3 | 7) {
                    on_slope += 1;
                    rolling += usize::from(h.state == State::Roll);
                }
            }
        }
    }
    assert!(on_slope > 2500 && rolling > 500, "{on_slope}, {rolling}");
}
