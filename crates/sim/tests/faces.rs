//! Steep faces on the `classic-1` preset, from the examples in
//! `spec/behavior/steep-faces.md` and `walk-off.md` ("Jumping near the
//! edge"). Heights in the spec are relative to the starting floor; here
//! that floor is at y = 300. The `classic_2_*` tests are on the `classic-2`
//! preset, from the same file's "In `title-2`".

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{Event, Facing, Fixed, InputFrame, State, World, generic_feel, step};

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
    ground_in("classic-1", height, x)
}

/// [`ground`] on the `preset` preset.
fn ground_in(preset: &str, height: impl Fn(i32) -> Option<i32>, x: f64) -> World {
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
    World::new(Arc::new(level), Arc::new(generic_feel(preset).unwrap()))
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
/// (counting from 1), holding it. Returns the frames with a jump start,
/// the first frame that started on the face, and the leave frame with
/// how far it moved down.
fn jump_at_the_ledge(press_on: usize) -> (Vec<usize>, usize, Option<(usize, Fixed)>) {
    let mut world = ground(ledge, 280.0);
    world.player.vel.x = Fixed::from_int(2);
    let mut jumps = Vec::new();
    let mut on_face = None;
    let mut leave = None;
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
        let y = world.player.pos.y;
        press(&mut world, before, held);
        if world.events.contains(&Event::JumpStart) {
            jumps.push(frame);
        }
        if world.events.contains(&Event::LeaveGround) {
            leave = Some((frame, world.player.pos.y - y));
        }
    }
    (jumps, on_face.unwrap(), leave)
}

#[test]
fn no_jump_starts_on_a_face() {
    let (_, face, leave) = jump_at_the_ledge(100);
    let (leave, drop) = leave.unwrap();
    assert_eq!(drop, fx(3.4375));
    // The press on the frame before the one that moves onto the face
    // takes off on that move.
    let (jumps, _, _) = jump_at_the_ledge(face - 2);
    assert_eq!(jumps, [face - 1]);
    // Presses whose next frame starts on the face don't jump, and nor do
    // presses on the leave frame or later: off a face there's no grace.
    for press in face - 1..face + 8 {
        let (jumps, _, _) = jump_at_the_ledge(press);
        assert_eq!(jumps, Vec::<usize>::new(), "press on {press}");
    }
    // A press on the leave frame doesn't change its drop either.
    assert_eq!(jump_at_the_ledge(leave).2, Some((leave, fx(3.4375))));
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

/// The floor of "Examples (landings)", whose `y` is 81 on the flat:
/// a face rising to the right over columns 100 to 115, and flat again
/// from column 116.
fn landing_face(c: i32) -> Option<i32> {
    Some(match c {
        ..100 => 0,
        100..116 => -1 - 4 * (c - 100),
        _ => -64,
    })
}

/// A world on [`landing_face`] in the air during a jump, at the end of
/// the frame `age` frames after the jump's start, with `x`, `y` (the
/// spec's level y), `vx` and `vy` (1/256) as given and jump let go of.
fn mid_jump(age: u8, x: f64, y: f64, vx: i64, vy: i64) -> World {
    let mut world = ground(landing_face, 90.0);
    let p = &mut world.player;
    p.pos = Vec2::new(fx(x), fx(y - 81.0) + Fixed::from_int(FLOOR));
    p.vel = Vec2::new(fx(vx as f64 / 256.0), fx(vy as f64 / 256.0));
    p.ground = None;
    p.state = if vy < 0 { State::Jump } else { State::Fall };
    p.jump_age = Some(age);
    world
}

/// A row of a landing example: frame, held, `x`, level `y`, `vx` and
/// `vy` (1/256), and whether grounded.
type LandingRow = (u32, Option<Action>, f64, f64, i64, i64, bool);

/// Steps through `rows`, from a world at the end of the frame before the
/// first, holding `before` on that frame.
fn check_rows(world: &mut World, before: Action, rows: &[LandingRow]) {
    let mut previous: Vec<Action> = vec![before];
    for &(frame, held, x, y, vx, vy, grounded) in rows {
        let held: Vec<Action> = held.into_iter().collect();
        press(world, &previous, &held);
        let p = &world.player;
        let got = (
            p.pos.x,
            p.pos.y - Fixed::from_int(FLOOR) + fx(81.0),
            grains(p.vel.x),
            grains(p.vel.y),
            p.grounded(),
        );
        assert_eq!(got, (fx(x), fx(y), vx, vy, grounded), "frame {frame}");
        previous = held;
    }
}

#[test]
fn a_plain_landing_on_a_face_waits_for_the_switch_frame() {
    // A jump from standing at x = 95.75, from frame 19.
    let mut world = mid_jump(19, 97.953125, 28.15625, 211, 296);
    let (l, r) = (Some(Action::Left), Some(Action::Right));
    check_rows(
        &mut world,
        Action::Right,
        &[
            (20, r, 98.921875, 29.75, 248, 408, false),
            (21, r, 100.01953125, 31.78125, 281, 520, false),
            (22, None, 100.98046875, 34.25, 246, 632, false),
            (23, None, 101.82421875, 37.15625, 216, 744, false),
            (24, l, 102.3125, 40.5, 125, 856, false),
            (25, r, 102.98828125, 44.28125, 173, 968, false),
            (26, r, 103.828125, 48.5, 215, 1080, false),
            (27, r, 104.8125, 53.15625, 252, 1192, false),
            // L: A = T + 20 and L = A + 8, so M = L + 2.
            (28, l, 105.42578125, 60.0, 157, 0, true),
            (29, l, 105.71484375, 60.0, 74, 0, true),
            // M, still moving uphill: no push yet.
            (30, l, 105.71875, 60.0, 1, 0, true),
            (31, l, 105.47265625, 60.0, -63, 0, true),
            (32, l, 104.8203125, 64.0, -167, 0, true),
            (33, l, 103.8125, 68.0, -258, 0, true),
        ],
    );
}

#[test]
fn a_slide_landing_waits_until_a_rise_stops_the_player() {
    let mut world = mid_jump(11, 111.7578125, 32.875, 407, -560);
    let r = Some(Action::Right);
    check_rows(
        &mut world,
        Action::Right,
        &[
            // Blocked in the air by face columns above the feet.
            (12, r, 111.0, 31.125, 0, -448, false),
            (13, r, 111.25, 29.8125, 64, -336, false),
            (14, r, 111.71875, 28.9375, 120, -224, false),
            (15, r, 112.37890625, 28.5, 169, -112, false),
            (16, r, 112.0, 28.5, 0, 0, false),
            // L, on face column 112; A = 20 on it too: a slide landing.
            (17, r, 112.25, 32.0, 64, 0, true),
            (18, r, 112.46875, 32.0, 56, 0, true),
            (19, r, 112.66015625, 32.0, 49, 0, true),
            (20, r, 112.828125, 32.0, 43, 0, true),
            (21, r, 112.9765625, 32.0, 38, 0, true),
            // M: blocked by the rise, with no speed.
            (22, r, 112.0, 32.0, 0, 0, true),
            (23, r, 111.8125, 36.0, -48, 0, true),
            (24, r, 111.4609375, 36.0, -90, 0, true),
        ],
    );
}

/// The `title-2` examples' floor: flat, a face rising to the right over
/// columns 200 to 215, and flat again 64 px up from column 216.
fn face_64(c: i32) -> Option<i32> {
    Some(match c {
        ..200 => 0,
        200..216 => -1 - 4 * (c - 200),
        _ => -64,
    })
}

/// Steps `frames` times holding `held`, and the rows after them.
fn rows_holding(
    world: &mut World,
    held: &[Action],
    frames: usize,
) -> Vec<(Fixed, i64, Fixed, bool)> {
    (0..frames)
        .map(|_| {
            hold(world, held);
            row(world)
        })
        .collect()
}

#[test]
fn classic_2_walking_into_the_face_repeats_a_cycle_at_its_foot() {
    let mut world = ground_in("classic-2", face_64, 193.5);
    world.player.vel.x = fx(2.25);
    let rows = rows_holding(&mut world, &[Action::Right], 13);
    let expected = [
        (195.75, 576),
        (198.0, 576),
        (198.0, 0),
        (198.28125, 72),
        (198.80859375, 135),
        (199.55078125, 190),
        (199.0, 0),
        (199.28125, 72),
        (199.80859375, 135),
        (199.0, 0),
        (199.28125, 72),
        (199.80859375, 135),
        (199.0, 0),
    ];
    for (n, ((x, vx, y, grounded), (ex, evx))) in rows.into_iter().zip(expected).enumerate() {
        assert_eq!(
            (x, vx, y, grounded),
            (fx(ex), evx, Fixed::ZERO, true),
            "frame {}",
            n + 1
        );
    }
}

#[test]
fn classic_2_running_into_the_face_repeats_a_shorter_cycle() {
    let mut world = ground_in("classic-2", face_64, 192.25);
    world.player.vel.x = fx(3.375);
    world.player.ran = true;
    let rows = rows_holding(&mut world, &[Action::Right, Action::Action], 8);
    let xs: Vec<_> = rows.iter().map(|r| (r.0, r.1)).collect();
    let expected = [
        (195.625, 864),
        (199.0, 864),
        (199.0, 0),
        (199.421875, 108),
        (199.0, 0),
        (199.421875, 108),
        (199.0, 0),
        (199.421875, 108),
    ];
    assert_eq!(xs, expected.map(|(x, vx)| (fx(x), vx)));
}

#[test]
fn classic_2_coasting_down_the_face() {
    let mut world = ground_in("classic-2", face_64, 217.5);
    world.player.vel.x = fx(-1.25);
    world.player.facing = Facing::Left;
    let rows = rows_holding(&mut world, &[], 21);
    for (frame, x, y, vx) in [
        (1, 216.40625, -64, -280),
        (2, 215.44921875, -61, -245),
        (3, 214.609375, -57, -215),
        (4, 213.75, -53, -220),
        (5, 212.875, -49, -224),
        (6, 211.984375, -45, -228),
        (7, 211.08203125, -45, -231),
        (18, 200.6796875, -1, -248),
        (19, 199.70703125, 0, -249),
        (20, 198.70703125, 0, -256),
        (21, 197.83203125, 0, -224),
    ] {
        assert_eq!(
            rows[frame - 1],
            (fx(x), vx, Fixed::from_int(y), true),
            "frame {frame}"
        );
    }
    assert!(rows.iter().all(|r| r.3));
}

#[test]
fn classic_2_walking_down_then_holding_uphill() {
    let mut world = ground_in("classic-2", face_64, 219.75);
    world.player.vel.x = fx(-2.25);
    world.player.facing = Facing::Left;
    let mut held: Vec<&[Action]> = vec![&[Action::Left]; 4];
    held.extend([&[Action::Right][..]; 18]);
    let mut facings = Vec::new();
    let mut rows = Vec::new();
    for h in held {
        hold(&mut world, h);
        rows.push(row(&world));
        facings.push(world.player.facing);
    }
    for (frame, x, y, vx) in [
        (1, 217.5, -64, -576),
        (2, 215.25, -61, -576),
        (3, 213.0, -53, -576),
        (4, 211.0, -45, -512),
        (5, 209.40625, -37, -408),
        (6, 208.16796875, -33, -317),
        (7, 207.23828125, -29, -238),
        (8, 206.48828125, -25, -192),
        (17, 199.73828125, 0, -192),
        (18, 199.23828125, 0, -128),
        (19, 199.08203125, 0, -40),
        (20, 199.2265625, 0, 37),
        (21, 199.6328125, 0, 104),
        (22, 199.0, 0, 0),
    ] {
        assert_eq!(
            rows[frame - 1],
            (fx(x), vx, Fixed::from_int(y), true),
            "frame {frame}"
        );
    }
    for frame in 9..=17 {
        assert_eq!(rows[frame - 1].1, -192, "frame {frame}");
    }
    // Holding uphill on the face doesn't turn the player.
    assert!(facings[..17].iter().all(|&f| f == Facing::Left));
}

#[test]
fn classic_2_crossing_three_face_columns_leaves_the_face() {
    let mut world = ground_in("classic-2", face_64, 219.75);
    world.player.vel.x = fx(-2.25);
    world.player.facing = Facing::Left;
    let rows = rows_holding(&mut world, &[Action::Left], 5);
    assert_eq!(rows[3], (fx(211.0), -512, Fixed::from_int(-45), true));
    // L: 8 px down, not 3.4375, with the pull's speed under the limit.
    assert_eq!(rows[4], (fx(208.84375), -552, Fixed::from_int(-37), false));
    assert!(world.events.contains(&Event::LeaveGround));
    assert_eq!(world.player.vel.y, fx(0.00390625));
    // L + 1: no step to 1.4375, and the pull once more.
    hold(&mut world, &[Action::Left]);
    assert_eq!(world.player.vel.y, fx(113.0 / 256.0));
    assert_eq!(grains(world.player.vel.x), -552 + (-832 + 552) / 8);
    assert_eq!(world.player.pos.y, fx(-37.0 + 113.0 / 256.0 + FLOOR as f64));
}

#[test]
fn classic_2_onto_a_lower_face_column_from_the_top_walks_off() {
    // The move ends in column 214, 7 px below the top floor.
    let mut world = ground_in("classic-2", face_64, 216.5);
    world.player.vel.x = fx(-2.25);
    world.player.facing = Facing::Left;
    hold(&mut world, &[Action::Left]);
    assert_eq!(row(&world), (fx(214.25), -576, fx(-64.0 + 3.4375), false));
    assert!(world.events.contains(&Event::LeaveGround));
}

#[test]
fn classic_2_on_the_face_the_player_turns_downhill_at_once() {
    let mut world = ground_in("classic-2", face_64, 210.5);
    world.player.facing = Facing::Right;
    hold(&mut world, &[]);
    assert_eq!(world.player.facing, Facing::Left);
    assert_eq!(world.player.turn, None);
    assert!(world.events.contains(&Event::Turn));
}

#[test]
fn classic_2_facing_waits_a_frame_after_landing_on_the_face() {
    let mut world = ground_in("classic-2", face_64, 210.5);
    world.player.facing = Facing::Right;
    world.player.pos.y -= Fixed::from_int(20);
    world.player.ground = None;
    let mut landed = None;
    for n in 0..20 {
        hold(&mut world, &[]);
        if world.events.contains(&Event::Land) {
            landed = Some(n);
            break;
        }
    }
    assert!(landed.is_some());
    assert_eq!(world.player.facing, Facing::Right);
    hold(&mut world, &[]);
    assert_eq!(world.player.facing, Facing::Right);
    hold(&mut world, &[]);
    assert_eq!(world.player.facing, Facing::Left);
}
