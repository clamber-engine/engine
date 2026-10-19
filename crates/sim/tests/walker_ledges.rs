//! Classic 1: walkers walking off ledges (spec/behavior/walker.md,
//! "Walking off a ledge") and walkers that start above the floor
//! (walker-view.md, "A start above the floor").

use std::sync::Arc;

use clamber_input::{Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{Facing, Fixed, InputFrame, World, generic_feel, step};

/// The values title-1 walkers have (walker.md), with the drop the
/// importer gives them.
const WALKER: &str = "speed = 1, response = 0.125, turn_delay = 4, stops_at_x = true, \
    size = [20, 12], offset = 0.5, defeat_jump = 6, defeat_gravity = 0.4375, \
    defeat_max_fall = 8, drop = { leave = 3.4375, leave_speed = 0.00390625, \
    gravity = 0.4375, max_fall = 8, land = 5 }";

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// A classic-1 world whose ground is the polygon `ground`, with the
/// player standing at `start` and a walker heading `facing` at `at`.
fn world(ground: &[[i32; 2]], start: [i32; 2], at: [f64; 2], facing: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "ledges"
        size = [1000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "polygon"
        points = {ground:?}

        [[entities]]
        type = "crawler"
        at = [{}, {}]
        params = {{ facing = "{facing}", {WALKER} }}
        "#,
        start[0], start[1], at[0], at[1]
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// Steps `world` with nothing held and returns the walker's `x` and `y`
/// after each step.
fn rows(world: &mut World, frames: usize) -> Vec<(Fixed, Fixed)> {
    (0..frames)
        .map(|_| {
            step(
                world,
                &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
            );
            let t = &world.things[0];
            (t.pos.x, t.pos.y)
        })
        .collect()
}

/// A floor at y = 200 to the end of column 99, 40 px lower from column
/// 100: walker.md's flat ledge.
const FLAT_LEDGE: [[i32; 2]; 6] = [
    [0, 600],
    [0, 200],
    [100, 200],
    [100, 240],
    [1000, 240],
    [1000, 600],
];

#[test]
fn classic_walker_walks_off_a_flat_ledge() {
    // walker.md's example: at full speed heading right, at 98.25 on
    // frame 0. Placed at 90.25, that's after 8 steps.
    let mut w = world(&FLAT_LEDGE, [170, 240], [90.25, 200.0], "right");
    let rows = rows(&mut w, 8 + 20);
    let frame = |f: usize| rows[7 + f];
    let want = [
        (0, 98.25, 200.0),
        (1, 99.25, 200.0),
        (2, 100.25, 203.4375),
        (3, 101.25, 203.87890625),
        (4, 102.25, 204.7578125),
        (5, 103.25, 206.07421875),
        (12, 110.25, 227.5390625),
        (13, 111.25, 232.35546875),
        (14, 112.25, 240.0),
    ];
    for (f, x, y) in want {
        assert_eq!(frame(f), (fx(x), fx(y)), "frame {f}");
    }
    // 7/16 more each frame between them.
    for f in 4..=13 {
        let moved = frame(f).1 - frame(f - 1).1;
        let want = fx(113.0 / 256.0) + fx(0.4375) * Fixed::from_int(f as i32 - 3);
        assert_eq!(moved, want, "frame {f}");
    }
    // It walks on along the lower floor, neither turning nor slowing.
    for f in 14..20 {
        assert_eq!(frame(f).1, fx(240.0), "frame {f}");
    }
    let t = &w.things[0];
    assert_eq!((t.vx, t.facing), (Fixed::ONE, Facing::Right));
}

#[test]
fn classic_walker_heading_left_walks_off_a_ledge() {
    // The flat ledge mirrored: floor at y = 200 from column 100 on, 40 px
    // lower up to column 99.
    let ground = [
        [0, 600],
        [0, 240],
        [100, 240],
        [100, 200],
        [1000, 200],
        [1000, 600],
    ];
    let mut w = world(&ground, [30, 240], [108.75, 200.0], "left");
    let rows = rows(&mut w, 30);
    // 100.75 after 8 steps, 99.75 after 9: off the floor.
    assert_eq!(rows[7], (fx(100.75), fx(200.0)));
    assert_eq!(rows[8], (fx(99.75), fx(203.4375)));
    assert_eq!(rows[9], (fx(98.75), fx(203.87890625)));
    assert_eq!(rows[20], (fx(87.75), fx(240.0)));
}

/// Floor at y = 180 to the end of column 94, then a steep face down
/// 4 px a column from column 95 to its lowest column, 99, at y = 200,
/// then `after`: the rest of the ground from x = 100.
fn face_then(after: &[[i32; 2]]) -> Vec<[i32; 2]> {
    let mut points = vec![[0, 600], [0, 180], [95, 180]];
    for (column, y) in (95..100).zip((184..).step_by(4)) {
        points.extend([[column, y], [column + 1, y]]);
    }
    points.extend_from_slice(after);
    points
}

/// The face over a pit: no floor from column 100 on.
fn face_over_a_pit() -> Vec<[i32; 2]> {
    face_then(&[[100, 600]])
}

#[test]
fn classic_walker_below_full_speed_leaves_a_face_on_its_second_frame_in_its_lowest_column() {
    // walker.md's example "The end of a face".
    let mut w = world(&face_over_a_pit(), [20, 180], [50.0, 180.0], "right");
    let t = &mut w.things[0];
    t.pos = Vec2::new(fx(98.578125), fx(196.0));
    t.vx = fx(0.48046875);
    let rows = rows(&mut w, 3);
    assert_eq!(rows[0], (fx(99.12109375), fx(200.0)));
    assert_eq!(rows[1], (fx(99.71875), fx(203.4375)));
    assert_eq!(rows[2], (fx(100.36328125), fx(203.87890625)));
    assert_eq!(w.things[0].vx, fx(0.64453125));
}

#[test]
fn classic_walker_at_full_speed_leaves_a_face_beyond_it() {
    let mut w = world(&face_over_a_pit(), [20, 180], [50.0, 180.0], "right");
    let t = &mut w.things[0];
    t.pos = Vec2::new(fx(98.578125), fx(196.0));
    let rows = rows(&mut w, 2);
    assert_eq!(rows[0], (fx(99.578125), fx(200.0)));
    assert_eq!(rows[1], (fx(100.578125), fx(203.4375)));
}

#[test]
fn classic_walker_falling_into_a_pit_is_gone_below_the_level() {
    let mut w = world(&face_over_a_pit(), [20, 180], [96.0, 188.0], "right");
    rows(&mut w, 120);
    assert!(!w.things[0].active);
}

#[test]
fn classic_walker_stays_on_the_floor_at_the_foot_of_a_face() {
    // The face meets floor 1 px lower: however many frames it ends in the
    // face's lowest column, the walker follows the floor.
    let ground = face_then(&[[100, 201], [1000, 201], [1000, 600]]);
    let mut w = world(&ground, [20, 180], [50.0, 180.0], "right");
    let t = &mut w.things[0];
    t.pos = Vec2::new(fx(98.578125), fx(196.0));
    t.vx = fx(0.125);
    let rows = rows(&mut w, 40);
    let in_99 = rows.iter().filter(|r| r.0.floor() == 99).count();
    assert!(in_99 >= 2, "{in_99} frames in column 99");
    for (x, y) in rows {
        let floor = match x.floor() {
            99 => 200.0,
            c if c >= 100 => 201.0,
            _ => 196.0,
        };
        assert_eq!(y, fx(floor), "at {x:?}");
    }
}

#[test]
fn classic_walker_starting_above_the_floor_drops_onto_it() {
    // walker-view.md's example: 12 px above flat floor at y = 300. Here
    // it isn't waiting to appear, so its first step is the first frame.
    let ground = [[0, 600], [0, 300], [1000, 300], [1000, 600]];
    let mut w = world(&ground, [500, 300], [400.0, 288.0], "left");
    let rows = rows(&mut w, 8);
    let want = [288.4375, 289.3125, 290.625, 292.375, 294.5625, 300.0, 300.0];
    for (n, &y) in want.iter().enumerate() {
        assert_eq!(rows[n].1, fx(y), "frame {}", n + 1);
        // Across, it moves as on the floor.
        assert_eq!(rows[n].0, fx(399.0 - n as f64), "frame {}", n + 1);
    }
}

#[test]
fn classic_walker_starting_100_px_up_falls_at_8_px_a_frame_from_its_19th_step() {
    let ground = [[0, 600], [0, 300], [1000, 300], [1000, 600]];
    let mut w = world(&ground, [500, 300], [400.0, 200.0], "left");
    let rows = rows(&mut w, 21);
    let moved = |n: usize| rows[n - 1].1 - if n > 1 { rows[n - 2].1 } else { fx(200.0) };
    assert_eq!(moved(18), fx(7.875));
    assert_eq!(moved(19), fx(8.0));
    assert_eq!(moved(20), fx(8.0));
    assert!(rows[19].1 < fx(295.0));
    assert_eq!(rows[20].1, fx(300.0));
}

#[test]
fn classic_walker_starting_less_than_5_px_up_lands_on_its_first_step() {
    let ground = [[0, 600], [0, 300], [1000, 300], [1000, 600]];
    let mut w = world(&ground, [500, 300], [400.0, 295.0], "left");
    assert_eq!(rows(&mut w, 1)[0].1, fx(300.0));
}
