//! Steep faces on the `classic-1` preset, from the examples in
//! `spec/behavior/steep-faces.md` and `walk-off.md` ("Jumping near the
//! edge"). Heights in the spec are relative to the starting floor; here
//! that floor is at y = 300.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{Event, Facing, Fixed, InputFrame, World, generic_feel, step};

const FLOOR: i32 = 300;

fn fx(x: f64) -> Fixed {
    Fixed::from_f64(x).unwrap()
}

/// 1/256 px per frame.
fn grains(v: Fixed) -> i64 {
    v.to_bits() >> 24
}

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
                points.extend([[end, 580], [start, 580]]);
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
        "format = 1\nname = \"faces\"\nsize = [600, 600]\nstart = [{x}, {start}]\n{collision}"
    );
    let level = Level::from_toml_str(&src, "faces").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// Flat, then a face rising to the right from column 200 (`steep-faces.md`,
/// "Walking into a rise").
fn rise(c: i32) -> Option<i32> {
    Some(match c {
        ..200 => 0,
        200..240 => -1 - 4 * (c - 200),
        _ => -160,
    })
}

/// Flat, then a face falling to the right from column 300 ("Coasting down
/// a face").
fn fall(c: i32) -> Option<i32> {
    Some(match c {
        ..300 => 0,
        300..340 => 2 + 4 * (c - 300),
        _ => 160,
    })
}

/// Steps once holding `held` after `previous`.
fn press(world: &mut World, previous: &[Action], held: &[Action]) {
    let keys = |a: &[Action]| a.iter().copied().collect::<Actions>();
    step(
        world,
        &InputFrame::single(Frame::after(keys(previous), keys(held))),
    );
}

/// Steps once, holding `held` on this step and the one before.
fn hold(world: &mut World, held: &[Action]) {
    press(world, held, held);
}

/// Row after a step: `x`, `vx` in 1/256, `y` above the starting floor
/// as in the spec, and whether grounded.
fn row(world: &World) -> (Fixed, i64, Fixed, bool) {
    let p = &world.player;
    (
        p.pos.x,
        grains(p.vel.x),
        p.pos.y - Fixed::from_int(FLOOR),
        p.grounded(),
    )
}

#[test]
fn walking_into_a_rise_stops_at_the_column_before_it() {
    let mut world = ground(rise, 193.5);
    world.player.vel.x = Fixed::from_int(2);
    let mut xs = Vec::new();
    for _ in 0..8 {
        hold(&mut world, &[Action::Right]);
        let (x, vx, y, grounded) = row(&world);
        assert_eq!(y, Fixed::ZERO);
        assert!(grounded);
        xs.push((x, vx));
    }
    let expected = [
        (195.5, 512),
        (197.5, 512),
        (199.5, 512),
        (199.0, 0),
        (199.0, 0),
        (199.0, 0),
        (199.0, 0),
        (199.0, 0),
    ];
    for (n, ((x, vx), (ex, evx))) in xs.into_iter().zip(expected).enumerate() {
        assert_eq!((x, vx), (fx(ex), evx), "frame {}", n + 1);
    }
}

#[test]
fn walking_left_into_a_wall_stops_at_the_edge_of_its_own_column() {
    // A vertical step of 64 px rising to the left, past column 99.
    let mut world = ground(|c| Some(if c < 100 { -64 } else { 0 }), 103.25);
    world.player.vel.x = Fixed::from_int(-2);
    world.player.facing = Facing::Left;
    let mut xs = Vec::new();
    for _ in 0..4 {
        hold(&mut world, &[Action::Left]);
        xs.push(row(&world));
    }
    let o = Fixed::ZERO;
    assert_eq!(
        xs,
        [
            (fx(101.25), -512, o, true),
            (fx(101.0), 0, o, true),
            (fx(101.0), 0, o, true),
            (fx(101.0), 0, o, true),
        ]
    );
}

#[test]
fn standing_in_front_of_a_rise() {
    // Column 201 is 5 px higher: the player starts, and is blocked on
    // reaching column 200.
    let mut world = ground(rise, 196.875);
    let mut rows = Vec::new();
    for _ in 0..6 {
        hold(&mut world, &[Action::Right]);
        let (x, vx, _, _) = row(&world);
        rows.push((x, vx));
    }
    assert_eq!(
        rows,
        [
            (fx(197.125), 64),
            (fx(197.59375), 120),
            (fx(198.25390625), 169),
            (fx(199.078125), 211),
            (fx(199.0), 0),
            (fx(199.0), 0),
        ]
    );

    // Column 202 is 9 px higher: the player doesn't start, however long
    // right is held, or pressed again, with or without action held (a
    // press of action would start a roll).
    let mut world = ground(rise, 197.0);
    for n in 0..20 {
        let action: &[Action] = if n < 10 { &[] } else { &[Action::Action] };
        let held = [&[Action::Right], action].concat();
        let previous = if n % 4 == 0 {
            action.to_vec()
        } else {
            held.clone()
        };
        press(&mut world, &previous, &held);
        assert_eq!(row(&world).0, fx(197.0), "frame {}", n + 1);
        assert_eq!(row(&world).1, 0, "frame {}", n + 1);
    }
    // Holding the other way walks away as usual.
    hold(&mut world, &[Action::Left]);
    assert_eq!(row(&world).1, -64);
}

#[test]
fn coasting_down_a_face() {
    let mut world = ground(fall, 299.0);
    world.player.vel.x = Fixed::ONE;
    let mut rows = Vec::new();
    for _ in 0..11 {
        hold(&mut world, &[]);
        rows.push(row(&world));
    }
    for (frame, vx, x, y) in [
        (1, 224, 299.875, 0),
        (2, 196, 300.640625, 2),
        (3, 219, 301.49609375, 6),
        (4, 239, 302.4296875, 10),
        (9, 308, 307.98046875, 30),
        (10, 317, 309.21875, 38),
        (11, 325, 310.48828125, 42),
    ] {
        assert_eq!(
            rows[frame - 1],
            (fx(x), vx, Fixed::from_int(y), true),
            "frame {frame}"
        );
    }
}

#[test]
fn walking_down_a_face_leaves_it() {
    let mut world = ground(fall, 297.75);
    world.player.vel.x = Fixed::from_int(2);
    let mut rows = Vec::new();
    for _ in 0..5 {
        hold(&mut world, &[Action::Right]);
        rows.push((row(&world), world.events.clone()));
    }
    let expected = [
        (512, 299.75, 0.0, true),
        (512, 301.75, 6.0, true),
        (560, 303.9375, 14.0, true),
        (602, 306.2890625, 17.4375, false),
        (591, 308.59765625, 17.87890625, false),
    ];
    for (n, (((x, vx, y, grounded), _), (evx, ex, ey, eg))) in rows.iter().zip(expected).enumerate()
    {
        assert_eq!(
            (*x, *vx, *y, *grounded),
            (fx(ex), evx, fx(ey), eg),
            "frame {}",
            n + 1
        );
    }
    assert!(rows[3].1.contains(&Event::LeaveGround));
    assert_eq!(world.player.vel.y, fx(0.44140625));
}

/// A face falling to the left from column 109 down to column 100, with
/// flat floor at its foot.
fn fall_left(c: i32) -> Option<i32> {
    Some(match c {
        ..100 => 40,
        100..110 => 39 - 4 * (c - 100),
        _ => 0,
    })
}

#[test]
fn a_direction_held_up_a_face_is_ignored_then_and_just_after() {
    let mut world = ground(fall_left, 110.5);
    world.player.vel.x = -Fixed::ONE;
    world.player.facing = Facing::Left;
    let mut off = Vec::new();
    for _ in 0..30 {
        let vx = grains(world.player.vel.x);
        let column = world.player.pos.x.floor();
        if column >= 110 {
            // Not on the face yet.
            hold(&mut world, &[]);
            continue;
        }
        hold(&mut world, &[Action::Right]);
        assert!(world.player.grounded());
        if column < 100 {
            off.push((vx, grains(world.player.vel.x), world.player.facing));
        } else {
            // On the face, right is ignored: the player faces left and
            // keeps speeding up down it.
            assert_eq!(world.player.facing, Facing::Left, "column {column}");
            assert!(grains(world.player.vel.x) < 0);
        }
    }
    assert!(off.len() > 3);
    // The gap to the target, ÷ 8, toward zero.
    let ease = |vx: i64, target: i64| vx + (target - vx) / 8;
    let (before, after, facing) = off[0];
    assert_eq!((after, facing), (ease(before, 0), Facing::Left));
    let (before, after, facing) = off[1];
    assert_eq!((after, facing), (ease(before, 0), Facing::Left));
    let (before, after, _) = off[2];
    assert_eq!(after, ease(before, 512));
}

/// Flat floor up to a ledge with a short face falling right at its top,
/// and a pit beyond.
fn ledge(c: i32) -> Option<i32> {
    match c {
        ..300 => Some(0),
        300..304 => Some(2 + 4 * (c - 300)),
        _ => None,
    }
}

/// Walks right toward the ledge and presses jump on frame `press`
/// (counting from 1), holding it. Returns the frames with a jump start
/// and the first frame that started on the face.
fn jump_at_the_ledge(press_on: usize) -> (Vec<usize>, usize) {
    let mut world = ground(ledge, 280.0);
    world.player.vel.x = Fixed::from_int(2);
    let mut jumps = Vec::new();
    let mut on_face = None;
    for frame in 1..=30 {
        if on_face.is_none() && world.player.pos.x >= Fixed::from_int(300) {
            on_face = Some(frame);
        }
        let held: &[Action] = if frame >= press_on {
            &[Action::Right, Action::Jump]
        } else {
            &[Action::Right]
        };
        let before: &[Action] = if frame > press_on {
            held
        } else {
            &[Action::Right]
        };
        press(&mut world, before, held);
        if world.events.contains(&Event::JumpStart) {
            jumps.push(frame);
        }
    }
    (jumps, on_face.unwrap())
}

#[test]
fn no_jump_starts_on_a_face() {
    let (_, face) = jump_at_the_ledge(100);
    // The press on the frame before the one that moves onto the face
    // takes off on that move.
    let (jumps, _) = jump_at_the_ledge(face - 2);
    assert_eq!(jumps, [face - 1]);
    // Presses whose next frame starts on the face don't jump.
    for press in [face - 1, face] {
        let (jumps, _) = jump_at_the_ledge(press);
        assert_eq!(jumps, Vec::<usize>::new(), "press on {press}");
    }
}

#[test]
fn faces_work_however_the_ground_is_drawn() {
    // The rise of "Walking into a rise" as one steep polygon edge
    // instead of a staircase: the floor under each column's middle climbs
    // 4 px per column all the same.
    let src = format!(
        "format = 1\nname = \"edge\"\nsize = [600, 600]\nstart = [193.5, {FLOOR}]\n\
         [[collision]]\nshape = \"polygon\"\npoints = [[0, {FLOOR}], [200, {FLOOR}], \
         [200, {a}], [240, {b}], [600, {b}], [600, 580], [0, 580]]\n",
        a = FLOOR - 1,
        b = FLOOR - 161,
    );
    let level = Level::from_toml_str(&src, "edge").unwrap();
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    world.player.vel.x = Fixed::from_int(2);
    for _ in 0..6 {
        hold(&mut world, &[Action::Right]);
    }
    assert_eq!(
        world.player.pos,
        Vec2::new(fx(199.0), Fixed::from_int(FLOOR))
    );
    assert_eq!(world.player.vel.x, Fixed::ZERO);
}
