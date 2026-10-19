//! Steep faces on the `classic-1` preset, from the examples in
//! `spec/behavior/steep-faces.md`, `walk-off.md` ("Jumping near the
//! edge") and `rolling-off-ledges.md` ("Stopping before a ledge"). Heights in the spec are relative to the starting floor; here
//! that floor is at y = 300. The `classic_2_*` tests are on the `classic-2`
//! preset, from the same file's "In `title-2`".

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    Event, Facing, Fixed, Gait, InputFrame, Roll, RollSize, State, World, generic_feel, step,
};

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
    world.player.target = Fixed::from_int(-2);
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

/// Steps once per entry of `keys` from `world`, and returns `x` relative
/// to `x0`, `vx`, facing and whether it turned on each frame.
fn rise_rows(world: &mut World, x0: f64, keys: &[&[Action]]) -> Vec<(Fixed, Fixed, Facing, bool)> {
    let mut previous: &[Action] = &[];
    let mut rows = Vec::new();
    for held in keys {
        press(world, previous, held);
        previous = held;
        let p = &world.player;
        assert!(p.grounded());
        rows.push((
            p.pos.x - fx(x0),
            p.vel.x,
            p.facing,
            world.events.contains(&Event::Turn),
        ));
    }
    rows
}

#[test]
fn holding_toward_a_rise_keeps_the_target_and_turns_at_once() {
    // Walking left away from the face, then Right: the target stays at
    // walking speed to the left until the column 5 ahead is no longer
    // 8 px higher, and facing turns right at once.
    let mut world = ground(rise, 199.0);
    world.player.facing = Facing::Left;
    let (l, r): (&[Action], &[Action]) = (&[Action::Left], &[Action::Right]);
    let rows = rise_rows(&mut world, 199.0, &[l, l, r, r]);
    let (left, right) = (Facing::Left, Facing::Right);
    assert_eq!(
        rows,
        [
            (fx(-0.25), fx(-0.25), left, false),
            (fx(-0.71875), fx(-0.46875), left, false),
            (fx(-1.37890625), fx(-0.66015625), right, true),
            (fx(-2.203125), fx(-0.82421875), right, false),
        ]
    );
}

#[test]
fn turns_next_to_a_rise_start_by_the_kept_target() {
    // Facing right 5.125 px from the foot of a 64 px step rising to the
    // left.
    let mut world = ground(|c| Some(if c < 100 { -64 } else { 0 }), 105.125);
    let (l, r): (&[Action], &[Action]) = (&[Action::Left], &[Action::Right]);
    let rows = rise_rows(
        &mut world,
        105.125,
        &[l, l, l, r, r, r, r, l, l, l, l, l, l],
    );
    let (left, right) = (Facing::Left, Facing::Right);
    assert_eq!(
        rows,
        [
            (fx(-0.25), fx(-0.25), right, false),
            (fx(-0.71875), fx(-0.46875), left, true),
            (fx(-1.37890625), fx(-0.66015625), left, false),
            (fx(-1.70703125), fx(-0.328125), left, false),
            (fx(-1.74609375), fx(-0.0390625), right, true),
            (fx(-1.53125), fx(0.21484375), right, false),
            (fx(-1.09375), fx(0.4375), right, false),
            (fx(-0.4609375), fx(0.6328125), left, true),
            (fx(0.33984375), fx(0.80078125), left, false),
            (fx(0.79296875), fx(0.453125), left, false),
            (fx(0.94140625), fx(0.1484375), left, false),
            (fx(0.82421875), fx(-0.1171875), left, false),
            (fx(0.47265625), fx(-0.3515625), right, true),
        ]
    );
}

#[test]
fn a_kept_running_target_outlasts_releasing_action() {
    // Speeding up toward a running target of 3 in front of the face, then
    // Right held without Action: the target stays 3 until the move is
    // blocked, and is 0 from then on.
    let mut world = ground(rise, 197.0);
    world.player.vel.x = fx(0.5);
    world.player.target = Fixed::from_int(3);
    let mut rows = Vec::new();
    for _ in 0..5 {
        press(
            &mut world,
            &[Action::Right, Action::Action],
            &[Action::Right],
        );
        rows.push((world.player.pos.x, grains(world.player.vel.x)));
    }
    assert_eq!(
        rows,
        [
            (fx(197.8125), 208),
            (fx(198.8984375), 278),
            (fx(198.0), 0),
            (fx(198.0), 0),
            (fx(198.0), 0),
        ]
    );
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
fn just_off_a_face_it_is_a_rise_and_its_frame_counts_as_target_0() {
    // "Leaving a face onto floor", the example: carried down the face
    // with Right held, the face is a rise behind the player, its frame
    // counts as target 0, and the target stays 0 until a frame starts 4
    // columns from it. Facing turns right at once.
    let mut world = ground(fall_left, 100.0);
    world.player.pos = Vec2::new(fx(100.76171875), Fixed::from_int(FLOOR + 39));
    world.player.vel.x = fx(-0.9765625);
    world.player.facing = Facing::Left;
    // Carried down the face, its push is under way.
    world.player.push = fx(-1.5);
    let expected = [
        (99.72265625, -1.0390625, Facing::Left),
        (98.8125, -0.91015625, Facing::Right),
        (98.015625, -0.796875, Facing::Right),
        (97.31640625, -0.69921875, Facing::Right),
        (96.703125, -0.61328125, Facing::Right),
        (96.4140625, -0.2890625, Facing::Right),
        (96.41015625, -0.00390625, Facing::Right),
        (96.65625, 0.24609375, Facing::Right),
    ];
    for (frame, &(x, vx, facing)) in expected.iter().enumerate() {
        hold(&mut world, &[Action::Right]);
        let p = &world.player;
        assert!(p.grounded(), "frame {}", frame + 1);
        assert_eq!(
            (p.pos.x, p.pos.y, p.vel.x, p.facing),
            (fx(x), Fixed::from_int(FLOOR + 40), fx(vx), facing),
            "frame {}",
            frame + 1
        );
    }
}

#[test]
fn left_held_on_the_last_frame_on_a_face_keeps_its_walking_target() {
    // With Left held on the last frame on the face, the target stays -2
    // on the frames next to it with Right held, and is +2 from the first
    // frame 4 columns away.
    let mut world = ground(fall_left, 100.0);
    world.player.pos = Vec2::new(fx(100.76171875), Fixed::from_int(FLOOR + 39));
    world.player.vel.x = fx(-1.5);
    world.player.facing = Facing::Left;
    world.player.push = fx(-1.5);
    hold(&mut world, &[Action::Left]);
    assert!(world.player.pos.x < Fixed::from_int(100));
    // The gap to the target, ÷ 8, toward zero.
    let ease = |vx: i64, target: i64| vx + (target - vx) / 8;
    let mut away = false;
    for _ in 0..12 {
        let column = world.player.pos.x.floor();
        let vx = grains(world.player.vel.x);
        hold(&mut world, &[Action::Right]);
        assert!(world.player.grounded());
        let target = if column >= 97 && !away { -512 } else { 512 };
        away |= column < 97;
        assert_eq!(
            grains(world.player.vel.x),
            ease(vx, target),
            "column {column}"
        );
    }
    assert!(away);
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

/// [`ledge`], with flat floor `drop` px below the starting floor beyond
/// the face.
fn faced_ledge(drop: i32) -> impl Fn(i32) -> Option<i32> {
    move |c| ledge(c).or(Some(drop))
}

/// One frame from a landing: `vx` (1/256), facing, gait, whether a turn
/// is under way, whether the frame turned, whether a roll started, and
/// `x`.
type FallRow = (i64, Facing, Gait, bool, bool, bool, Fixed);

/// Runs right off [`faced_ledge`] at full speed from x = 250, Right and
/// Action held up to the frame that leaves the face, then `air` until
/// the landing frame G, then `held(n)` on G + n. The rows from G to
/// G + 20, if it lands within a second.
fn fall_from_face(
    drop: i32,
    air: &[Action],
    held: impl Fn(usize) -> Vec<Action>,
) -> Option<Vec<FallRow>> {
    let mut world = ground(faced_ledge(drop), 250.0);
    world.player.vel.x = Fixed::from_int(3);
    let mut previous = vec![Action::Right, Action::Action];
    let mut step_with = |world: &mut World, keys: Vec<Action>| {
        press(world, &previous, &keys);
        previous = keys;
        let p = &world.player;
        (
            grains(p.vel.x),
            p.facing,
            p.gait,
            p.turn.is_some(),
            world.events.contains(&Event::Turn),
            world.events.contains(&Event::RollStart),
            p.pos.x,
        )
    };
    while !world.events.contains(&Event::LeaveGround) {
        step_with(&mut world, vec![Action::Right, Action::Action]);
        assert!(world.player.pos.x < Fixed::from_int(310));
    }
    assert!(world.player.walked_off_face);
    for _ in 0..60 {
        let row = step_with(&mut world, air.to_vec());
        assert_eq!(row.1, Facing::Right, "facing in the fall");
        if world.events.contains(&Event::Land) {
            let mut rows = vec![row];
            for n in 1..=20 {
                rows.push(step_with(&mut world, held(n)));
            }
            return Some(rows);
        }
    }
    None
}

#[test]
fn after_a_fall_from_a_face_the_mode_waits_11_frames() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/ground-speed.md, "After a fall or a bounce", the
    // example: Action alone through the fall and the first landing
    // frames, and Right added from G + 4 or G + 5. The fall's height
    // isn't given; this one gives the example's `vx` on G.
    let run_from = |from: usize| move |n: usize| if n >= from { vec![A, R] } else { vec![A] };
    let drop = (20..270)
        .find(|&d| fall_from_face(d, &[A], run_from(4)).is_some_and(|r| r[0].0 == 113))
        .expect("a fall landing at 113");
    for (from, g10, g11, mode) in [(4, 495, 529, "running"), (5, 452, 491, "walking")] {
        let rows = fall_from_face(drop, &[A], run_from(from)).unwrap();
        assert_eq!((rows[0].0, rows[3].0), (113, 77), "from G + {from}");
        assert_eq!(rows[4].0, if from == 4 { 163 } else { 68 });
        assert_eq!((rows[10].0, rows[11].0), (g10, g11), "from G + {from}");
        assert!(rows[..11].iter().all(|r| matches!(r.2, Gait::Landing(_))));
        let started = match rows[11].2 {
            Gait::Running(s) if s.step == 1 => "running",
            Gait::Walking(s) if s.step == 1 => "walking",
            _ => "other",
        };
        assert_eq!(started, mode, "from G + {from}");
    }

    // However long the fall: facing stays as it is on the landing frames
    // whatever is held, no turn starts on them, and on G + 11 the other
    // way held starts a turn, which changes facing on G + 15. Back at the
    // foot of the ledge, the other way is toward a rise, with a target of
    // 0: no turn starts, and facing turns at once from G + 12
    // (steep-faces.md, "Holding toward a rise"; G + 11 isn't covered).
    for drop in [40, 100, 176, 260] {
        for air in [&[][..], &[L], &[A, L]] {
            let rows = fall_from_face(drop, air, |_| vec![L]).unwrap();
            for (n, row) in rows[..11].iter().enumerate() {
                assert!(matches!(row.2, Gait::Landing(_)), "{drop} G + {n}");
                assert_eq!((row.1, row.3, row.4), (Facing::Right, false, false));
            }
            // The column 5 to the left is the ledge's.
            if rows[10].6 < Fixed::from_int(309) {
                let row = &rows[11];
                assert_eq!(
                    (row.1, row.3, row.4),
                    (Facing::Right, false, false),
                    "{drop}"
                );
                let row = &rows[12];
                assert_eq!((row.1, row.3, row.4), (Facing::Left, false, true), "{drop}");
                continue;
            }
            assert!(rows[11].3, "{drop}: a turn on G + 11");
            assert_eq!(rows[14].1, Facing::Right);
            assert_eq!((rows[15].1, rows[15].4), (Facing::Left, true));
        }
        // Nothing held on G + 11: standing.
        let rows = fall_from_face(drop, &[], |_| vec![]).unwrap();
        assert!(matches!(rows[10].2, Gait::Landing(_)));
        assert_eq!(rows[11].2, Gait::Standing);
        // An Action press on a landing frame starts a roll on it.
        let rows = fall_from_face(drop, &[], |n| if n >= 3 { vec![A] } else { vec![] }).unwrap();
        assert!(rows[3].5 && !rows[2].5, "{drop}");
    }
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
    let mut world = walking_down_the_face();
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
        (1, 217.40234375, -64, -562),
        (2, 215.203125, -61, -563),
        (3, 213.0, -53, -564),
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

/// The example "Walking down, then holding uphill", from a speed the
/// limits keep on frame 3: the example's own start, 2.25 px per frame,
/// is limited there to 2 (spec#48). From frame 3 on it's the example.
fn walking_down_the_face() -> World {
    let mut world = ground_in("classic-2", face_64, 219.59765625);
    world.player.vel.x = fx(-560.0 / 256.0);
    world.player.facing = Facing::Left;
    world
}

#[test]
fn classic_2_crossing_three_face_columns_leaves_the_face() {
    let mut world = walking_down_the_face();
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

/// A world on [`ledge`] with the player at `x`, moving right at `vx`,
/// as the lead character (`slot` 0) or the second one (1) under control.
fn at_the_ledge(slot: usize, x: f64, vx: f64) -> World {
    let mut world = ground(ledge, x);
    if slot == 1 {
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(clamber_pack::CharacterFeel {
            physics: clamber_pack::Presets::builtin()
                .slot("classic-1", 1)
                .unwrap(),
            abilities: Default::default(),
        });
        world = World::new(world.level.clone(), Arc::new(feel));
        world.player.character = 1;
        if let Some(other) = &mut world.other {
            other.character = 0;
        }
    }
    world.player.vel.x = fx(vx);
    world
}

/// Goes right off [`ledge`] for 40 frames from `world`, holding `base`,
/// and with `jump` as `(p, held)`, Jump from frame `p` (counting from 1)
/// for `held` frames. A row for each frame: height below the starting
/// floor, `vy`, and whether the frame started a jump.
fn off_the_ledge(
    world: impl Fn() -> World,
    base: &[Action],
    jump: Option<(usize, usize)>,
) -> Vec<(f64, f64, bool)> {
    let mut world = world();
    let mut previous = base.to_vec();
    let mut rows = Vec::new();
    for frame in 1..=40 {
        let mut keys = base.to_vec();
        if jump.is_some_and(|(p, held)| (p..p + held).contains(&frame)) {
            keys.push(Action::Jump);
        }
        press(&mut world, &previous, &keys);
        previous = keys;
        let p = &world.player;
        rows.push((
            (p.pos.y - Fixed::from_int(FLOOR)).to_f64(),
            p.vel.y.to_f64(),
            world.events.contains(&Event::JumpStart),
        ));
        assert!(!world.events.contains(&Event::Land));
    }
    rows
}

/// The leave frame of a walk off [`ledge`] without a press, counting from
/// 1 as [`off_the_ledge`] does, and the first frame starting on the face.
fn leave_frame(world: impl Fn() -> World, base: &[Action]) -> (usize, Option<usize>) {
    let mut world = world();
    let mut on_face = None;
    for frame in 1..=40 {
        let was = world.player.pos.x;
        hold(&mut world, base);
        if world.events.contains(&Event::LeaveGround) {
            return (frame, on_face);
        }
        if on_face.is_none() && was >= Fixed::from_int(300) {
            on_face = Some(frame);
        }
    }
    panic!("never left the ground");
}

#[test]
fn a_press_that_gives_no_jump_at_the_face_lightens_the_fall() {
    // walk-off.md, "Jumping near the edge", rule 2: the press on the
    // frame that moves onto the face counts, since that frame doesn't
    // start on it, but gives no jump, since the next one does. While
    // Jump stays held the fall from L uses the lighter gravity.
    for (slot, x, vx, gravity) in [(0, 292.0, 2.0, 0.28125), (1, 292.25, 2.25, 0.3515625)] {
        let world = || at_the_ledge(slot, x, vx);
        let walk = [Action::Right];
        let (l, face) = leave_frame(world, &walk);
        let face = face.unwrap();
        assert!(face < l, "slot {slot}: frames before L start on the face");
        let fall = off_the_ledge(world, &walk, None);
        let at = |rows: &[(f64, f64, bool)], n: usize| rows[l - 1 + n];
        // Heights after L − 1, which ends on the face.
        let before = fall[l - 2].0;
        for held in [20, 1] {
            let rows = off_the_ledge(world, &walk, Some((face - 1, held)));
            assert!(rows.iter().all(|r| !r.2), "slot {slot}: no jump");
            // With Jump let go after its press, the fall is the plain one.
            let g = if held > 1 { gravity } else { 0.4375 };
            assert_eq!(at(&rows, 0).0 - before, 3.0 + g, "slot {slot}, L");
            for n in 0..=3 {
                let vy = 0.00390625 + g * n as f64;
                assert_eq!(at(&rows, n).1, vy, "slot {slot}, held {held}, L + {n}");
            }
        }
        // Presses on frames that start on the face, or in the air, do
        // nothing.
        for p in face..l + 4 {
            assert_eq!(
                off_the_ledge(world, &walk, Some((p, 20))),
                fall,
                "slot {slot}, press {p}"
            );
        }
        // The press before still jumps, taking off on the move onto the
        // face.
        let rows = off_the_ledge(world, &walk, Some((face - 2, 20)));
        let starts: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].2).collect();
        assert_eq!(starts, [face - 2], "slot {slot}");
    }
}

#[test]
fn running_over_the_face_keeps_the_grace_frame() {
    // walk-off.md, "Jumping near the edge": running, the player can pass
    // the face without a frame starting on it, and presses on L − 1 and
    // L still jump. The second character's heights are its running
    // table in second-character.md, "Walking off a ledge"; the lead's
    // are walk-off.md's "Jumping at a ledge without a face".
    let run = [Action::Right, Action::Action];
    for (slot, x, vx, table) in [
        (
            0,
            290.0,
            3.0,
            [
                [0.0, 0.0, 0.0, 0.0],
                [3.4375, -6.71875, 3.28125, 3.28125],
                [3.87890625, -13.15625, -3.4375, 3.56640625],
                [4.7578125, -19.3125, -9.875, -3.15234375],
                [6.07421875, -25.1875, -16.03125, -9.58984375],
            ],
        ),
        (
            1,
            289.0,
            3.375,
            [
                [0.0, 0.0, 0.0, 0.0],
                [3.4375, -8.0, 3.3515625, 3.3515625],
                [3.87890625, -15.6484375, -4.6484375, 3.70703125],
                [4.7578125, -22.9453125, -12.296875, -4.29296875],
                [6.07421875, -29.890625, -19.59375, -11.94140625],
            ],
        ),
    ] {
        let world = || at_the_ledge(slot, x, vx);
        let (l, face) = leave_frame(world, &run);
        assert_eq!(face, None, "slot {slot}: no frame starts on the face");
        let ledge_y = |rows: &[(f64, f64, bool)]| rows[l - 3].0;
        let columns = [None, Some(l - 2), Some(l - 1), Some(l)];
        for (column, press) in columns.into_iter().enumerate() {
            let rows = off_the_ledge(world, &run, press.map(|p| (p, 20)));
            let top = ledge_y(&rows);
            for (n, row) in table.iter().enumerate() {
                let frame = l - 2 + n;
                assert_eq!(
                    rows[frame].0 - top,
                    row[column],
                    "slot {slot}, press {press:?}, L {:+}",
                    n as i64 - 1
                );
            }
            // T is the frame after the press.
            if let Some(p) = press {
                assert!(rows[p].2, "slot {slot}, press {p}");
            }
        }
        // A press on L + 1 does nothing.
        assert_eq!(
            off_the_ledge(world, &run, Some((l + 1, 20))),
            off_the_ledge(world, &run, None)
        );
    }
}

/// The floor of "Examples (jumps)": flat, a face falling to the right over
/// columns 300 to 311, and flat again 47 px down from column 312.
fn short_fall(c: i32) -> Option<i32> {
    Some(match c {
        ..300 => 0,
        300..312 => 2 + 4 * (c - 300),
        _ => 47,
    })
}

/// A frame of [`jump_rows`]: `x`, `y` below the starting floor, `vx` in
/// 1/256, `vy`, and whether the frame started a jump.
type JumpRow = (Fixed, Fixed, i64, Fixed, bool);

/// Steps `world` for `frames` frames, holding `keys(frame)` on each
/// (counting from 1).
fn jump_rows(mut world: World, frames: usize, keys: impl Fn(usize) -> Vec<Action>) -> Vec<JumpRow> {
    let mut previous = keys(0);
    let mut rows = Vec::new();
    for frame in 1..=frames {
        let held = keys(frame);
        press(&mut world, &previous, &held);
        previous = held;
        let p = &world.player;
        rows.push((
            p.pos.x,
            p.pos.y - Fixed::from_int(FLOOR),
            grains(p.vel.x),
            p.vel.y,
            world.events.contains(&Event::JumpStart),
        ));
    }
    rows
}

/// Coasting down [`short_fall`] from `x` = 299 at 1 px per frame, with
/// Jump pressed on frame `press` and held for `held` frames.
fn coast_and_press(press: Option<(usize, usize)>) -> Vec<JumpRow> {
    let mut world = ground(short_fall, 299.0);
    world.player.vel.x = Fixed::ONE;
    jump_rows(world, 20, |f| {
        if press.is_some_and(|(p, held)| (p..p + held).contains(&f)) {
            vec![Action::Jump]
        } else {
            vec![]
        }
    })
}

fn starts(rows: &[JumpRow]) -> Vec<usize> {
    (0..rows.len())
        .filter(|&i| rows[i].4)
        .map(|i| i + 1)
        .collect()
}

#[test]
fn a_press_while_carried_down_a_face_takes_off_past_it() {
    // steep-faces.md, "Jumping on or next to a face", the first example.
    let rows = coast_and_press(Some((4, 20)));
    let plain = coast_and_press(None);
    for (frame, x, y, vx) in [
        (3, 301.49609375, 6.0, 219),
        (4, 302.4296875, 10.0, 239),
        (5, 303.43359375, 14.0, 257),
        (12, 311.78515625, 46.0, 332),
        (13, 313.10546875, 47.0, 338),
        (14, 314.26171875, 47.0, 296),
        (15, 315.2734375, 47.0, 259),
        (16, 316.16015625, 40.28125, 227),
    ] {
        let (rx, ry, rvx, _, _) = rows[frame - 1];
        assert_eq!((rx, ry, rvx), (fx(x), fx(y), vx), "frame {frame}");
    }
    // Until T the press changes nothing.
    assert_eq!(rows[..14], plain[..14]);
    assert_eq!(starts(&rows), [15]);
    assert_eq!(rows[14].3, fx(-7.0));
    assert_eq!(rows[15].3, fx(-6.71875));

    // Let go before Q, no jump starts.
    for held in [1, 5, 10] {
        assert_eq!(coast_and_press(Some((4, held))), plain, "held {held}");
    }
    // Pressed on the frame that moves onto the face, T would be P + 13:
    // too late.
    assert_eq!(coast_and_press(Some((2, 20)))[..17], plain[..17]);
    assert_eq!(starts(&coast_and_press(Some((2, 20)))), Vec::<usize>::new());
    // Pressed on frame 3, T = P + 12 is still in time.
    assert_eq!(starts(&coast_and_press(Some((3, 20)))), [15]);
}

#[test]
fn a_jump_from_in_front_of_a_rise_moves_on_its_take_off_frame() {
    // steep-faces.md, "Jumping on or next to a face", standing in front
    // of a rise: Right and Jump pressed together on frame 1.
    let keys = |f: usize| {
        if f == 0 {
            vec![]
        } else {
            vec![Action::Right, Action::Jump]
        }
    };
    let rows = jump_rows(ground(rise, 199.0), 2, keys);
    let o = Fixed::ZERO;
    assert_eq!(rows[0], (fx(199.0), o, 0, o, false));
    assert_eq!(rows[1], (fx(199.25), o, 64, fx(-7.0), true));

    let rows = jump_rows(ground(rise, 199.875), 2, keys);
    assert_eq!(rows[0], (fx(199.875), o, 0, o, false));
    // The move to 200.125 is into face column 200: blocked.
    assert_eq!(rows[1], (fx(199.0), o, 0, fx(-7.0), true));
}

#[test]
fn a_direction_up_the_face_counts_again_on_the_take_off_frame() {
    // "Leaving a face onto floor" ignores Left on Q, but not on T.
    let mut world = ground(short_fall, 299.0);
    world.player.vel.x = Fixed::ONE;
    let rows = jump_rows(world, 15, |f| match f {
        4..14 => vec![Action::Jump],
        14.. => vec![Action::Jump, Action::Left],
        _ => vec![],
    });
    assert_eq!(starts(&rows), [15]);
    assert_eq!(rows[13].2, 296);
    // Eased an eighth of the way to −2.
    assert_eq!(rows[14].2, 296 - 101);
}

/// Moving left at 1 px per frame onto [`fall_left`] from its top,
/// holding `base`, with Jump pressed on the frame that moves onto the
/// face and held. The frames that started a jump, and that frame.
fn onto_the_left_face(base: &[Action]) -> (Vec<usize>, usize) {
    let run = |press: Option<usize>| {
        let mut world = ground(fall_left, 110.5);
        world.player.vel.x = -Fixed::ONE;
        world.player.facing = Facing::Left;
        jump_rows(world, 12, |f| {
            let mut keys = base.to_vec();
            if press.is_some_and(|p| f >= p) {
                keys.push(Action::Jump);
            }
            keys
        })
    };
    let plain = run(None);
    let onto = 1 + plain.iter().position(|r| r.0 < fx(110.0)).unwrap();
    (starts(&run(Some(onto))), onto)
}

#[test]
fn moving_onto_a_face_with_only_uphill_held_the_next_frame_can_take_off() {
    // "When the jump takes off": the exception for the frame that moves
    // onto a face falling to the left with only Right held.
    let (jumps, onto) = onto_the_left_face(&[Action::Right]);
    assert_eq!(jumps, [onto + 1]);
    // With nothing held, or Left, the next frame starts on the face, and
    // the press is carried.
    for base in [&[][..], &[Action::Left]] {
        let (jumps, onto) = onto_the_left_face(base);
        assert!(!jumps.contains(&(onto + 1)), "{base:?}: {jumps:?}");
    }
}

#[test]
fn a_carried_press_held_through_a_fall_from_the_face_takes_off_after_landing() {
    // "When the jump takes off": leaving the ground on the face and
    // landing on other floor, Q = L + 1 and T = L + 2.
    let floor = |c: i32| {
        Some(match c {
            ..300 => 0,
            300..304 => 2 + 4 * (c - 300),
            _ => 30,
        })
    };
    let run = |jump: Option<usize>| {
        let mut world = ground(floor, 292.0);
        world.player.vel.x = Fixed::from_int(2);
        let mut landed = None;
        let mut leave = None;
        let mut on_face = None;
        let mut jumps = Vec::new();
        let mut previous = vec![Action::Right];
        for frame in 1..=30 {
            if on_face.is_none() && world.player.pos.x >= Fixed::from_int(300) {
                on_face = Some(frame);
            }
            let mut keys = vec![Action::Right];
            if jump.is_some_and(|p| frame >= p) {
                keys.push(Action::Jump);
            }
            press(&mut world, &previous, &keys);
            previous = keys;
            if world.events.contains(&Event::Land) && landed.is_none() {
                landed = Some(frame);
            }
            if world.events.contains(&Event::LeaveGround) && leave.is_none() {
                leave = Some(frame);
            }
            if world.events.contains(&Event::JumpStart) {
                jumps.push(frame);
            }
        }
        (jumps, on_face.unwrap(), leave.unwrap(), landed.unwrap())
    };
    let (_, face, leave, l) = run(None);
    // Presses on the frames that start on the face, the leave frame
    // included. Presses in the air are as in jump-input.md.
    assert!(face < leave);
    for p in face..=leave {
        let (jumps, ..) = run(Some(p));
        let expected = if l + 2 <= p + 12 { vec![l + 2] } else { vec![] };
        assert_eq!(jumps, expected, "press on {p}, L {l}");
    }
}

/// A roll's `x`, `vx` in 1/256 and `y` from [`jump_rows`].
fn roll_rows(
    world: World,
    frames: usize,
    keys: impl Fn(usize) -> Vec<Action>,
) -> Vec<(Fixed, i64, Fixed)> {
    jump_rows(world, frames, keys)
        .into_iter()
        .map(|(x, y, vx, _, _)| (x, vx, y))
        .collect()
}

/// Standing at 195 in front of [`rise`], Right and Action on S and Right
/// held until the roll is blocked at the face's foot.
fn rolled_into_the_rise() -> World {
    let mut world = ground(rise, 195.0);
    press(&mut world, &[], &[Action::Right, Action::Action]);
    assert_eq!(world.player.state, State::Roll);
    for _ in 0..10 {
        if world.player.vel.x == Fixed::ZERO {
            break;
        }
        hold(&mut world, &[Action::Right]);
    }
    assert_eq!(
        (world.player.pos.x, world.player.vel.x),
        (fx(199.0), Fixed::ZERO)
    );
    assert!(world.player.roll.unwrap().stopped);
    world
}

/// A world with a timed roll of `size` going `dir` at `x`, already some
/// frames in, moving at `vx`.
fn rolling_at(
    height: impl Fn(i32) -> Option<i32>,
    x: f64,
    dir: Facing,
    size: RollSize,
    vx: f64,
    stopped: bool,
) -> World {
    let mut world = ground(height, x);
    let speed = match size {
        RollSize::Slow => fx(1.0),
        RollSize::Normal => fx(3.0),
        RollSize::Fast => fx(4.0),
    };
    world.player.state = State::Roll;
    world.player.facing = dir;
    world.player.vel.x = fx(vx);
    world.player.roll = Some(Roll {
        left: 20,
        dir,
        size,
        speed,
        boosts: 0,
        defeated: false,
        held: false,
        carry: false,
        still: false,
        from_stop: false,
        size_later: false,
        stopped,
        since_press: 5,
        air: 0,
        frame: 5,
    });
    world
}

#[test]
fn a_roll_stopped_at_the_foot_of_a_rise_rolls_away_only_the_other_way() {
    // steep-faces.md, "Examples (rolls)", the first example: Right held
    // on k + 1 to k + 3, Left on k + 4 to k + 6, then nothing.
    let rows = roll_rows(rolled_into_the_rise(), 8, |f| match f {
        0..=3 => vec![Action::Right],
        4..=6 => vec![Action::Left],
        _ => vec![],
    });
    let xs: Vec<_> = rows.iter().map(|r| (r.0 - fx(200.0), r.1)).collect();
    assert_eq!(
        xs,
        [
            (fx(-1.0), 0),
            (fx(-1.0), 0),
            (fx(-1.0), 0),
            (fx(-3.0), -512),
            (fx(-5.125), -544),
            (fx(-7.359375), -572),
            (fx(-9.6875), -596),
            (fx(-12.09765625), -617),
        ]
    );
    assert!(rows.iter().all(|r| r.2 == Fixed::ZERO));

    // Left held only on k + 1: the roll goes on at 2.
    let rows = roll_rows(rolled_into_the_rise(), 4, |f| match f {
        0 => vec![Action::Right],
        1 => vec![Action::Left],
        _ => vec![],
    });
    let xs: Vec<_> = rows.iter().map(|r| (r.0, r.1)).collect();
    assert_eq!(
        xs,
        [
            (fx(197.0), -512),
            (fx(195.0), -512),
            (fx(193.0), -512),
            (fx(191.0), -512)
        ]
    );

    // With nothing held it stays where it is.
    let rows = roll_rows(rolled_into_the_rise(), 6, |_| vec![]);
    assert!(rows.iter().all(|r| (r.0, r.1) == (fx(199.0), 0)));
}

#[test]
fn a_stopped_roll_on_a_face_is_carried_down_to_its_foot() {
    // steep-faces.md, "Examples (rolls)", the second example: stopped on
    // the face at c0 + 6, the roll pointing right, Right held throughout.
    let world = rolling_at(rise, 206.0, Facing::Right, RollSize::Normal, 0.0, true);
    let rows = roll_rows(world, 11, |_| vec![Action::Right]);
    let row = |k: usize| {
        let (x, vx, y) = rows[k - 1];
        (x - fx(200.0), -y, vx)
    };
    assert_eq!(row(1), (fx(5.8125), fx(21.0), -48));
    assert_eq!(row(2), (fx(5.4609375), fx(21.0), -90));
    assert_eq!(row(3), (fx(4.96875), fx(17.0), -126));
    assert_eq!(row(8), (fx(0.92578125), fx(1.0), -250));
    assert_eq!(row(9), (fx(-0.11328125), Fixed::ZERO, -266));
    assert_eq!(row(10), (fx(-1.0), Fixed::ZERO, 0));
    assert_eq!(row(11), (fx(-1.0), Fixed::ZERO, 0));
    assert_eq!(rows.len(), 11);
}

#[test]
fn on_a_face_a_roll_going_down_it_goes_faster_and_steering_does_nothing() {
    // steep-faces.md, "Speed on a face" and "Steering on a face": on
    // [`rise`], which falls to the left, from a standstill.
    let first = |dir, size, keys: Vec<Action>| {
        // Low enough on the face for the floor 15 px downhill to count.
        let mut world = rolling_at(rise, 205.0, dir, size, 0.0, false);
        hold(&mut world, &keys);
        (grains(world.player.vel.x), world.player.roll.unwrap().dir)
    };
    // Downhill: the roll's target plus 1.5. A slow roll with its own
    // direction held stays slow, and the other direction doesn't turn it.
    assert_eq!(
        first(Facing::Left, RollSize::Slow, vec![]),
        (-80, Facing::Left)
    );
    assert_eq!(
        first(Facing::Left, RollSize::Slow, vec![Action::Left]),
        (-80, Facing::Left)
    );
    assert_eq!(
        first(Facing::Left, RollSize::Slow, vec![Action::Right]),
        (-80, Facing::Left)
    );
    assert_eq!(
        first(Facing::Left, RollSize::Normal, vec![]),
        (-144, Facing::Left)
    );
    assert_eq!(
        first(Facing::Left, RollSize::Fast, vec![]),
        (-176, Facing::Left)
    );
    // Uphill: the roll's target alone.
    assert_eq!(
        first(Facing::Right, RollSize::Slow, vec![Action::Left]),
        (32, Facing::Right)
    );
    assert_eq!(
        first(Facing::Right, RollSize::Normal, vec![]),
        (96, Facing::Right)
    );
}

#[test]
fn a_roll_up_a_face_is_blocked_by_its_next_column_then_carried_down() {
    // steep-faces.md, "Stopped rolls": a roll pointing up a face moves up
    // it until it enters a higher face column, then is carried down.
    let world = rolling_at(rise, 210.5, Facing::Right, RollSize::Normal, 0.0, false);
    let rows = roll_rows(world, 2, |_| vec![]);
    // 210.5 + 0.375 stays in column 210; 210.875 + 0.703125 is blocked.
    assert_eq!((rows[0].0, rows[0].1), (fx(210.875), 96));
    assert_eq!((rows[1].0, rows[1].1), (fx(210.0), 0));
    let world = rolling_at(rise, 210.5, Facing::Right, RollSize::Normal, 0.0, false);
    let rows = roll_rows(world, 3, |_| vec![Action::Right]);
    // Then carried down, Right held or not.
    assert_eq!((rows[2].0, rows[2].1), (fx(209.8125), -48));
}

#[test]
fn a_roll_is_blocked_by_floor_more_than_a_step_higher() {
    // steep-faces.md, "Blocked": floor more than 8 px higher than the
    // floor before the frame blocks a roll, even off a face.
    let climb = |top: i32| {
        move |c: i32| {
            Some(match c {
                ..200 => 0,
                200..203 => -top * (c - 199) / 3,
                _ => -top,
            })
        }
    };
    for (top, blocked) in [(9, true), (6, false)] {
        let world = rolling_at(
            climb(top),
            199.5,
            Facing::Right,
            RollSize::Normal,
            3.0,
            false,
        );
        let rows = roll_rows(world, 1, |_| vec![]);
        if blocked {
            assert_eq!(
                (rows[0].0, rows[0].1, rows[0].2),
                (fx(199.0), 0, Fixed::ZERO)
            );
        } else {
            assert_eq!(rows[0].0, fx(202.5));
            assert_eq!(rows[0].2, fx(f64::from(-top)));
        }
    }
}

/// Flat floor to column 299, then floor `drop` px lower.
fn drop_off(drop: i32) -> impl Fn(i32) -> Option<i32> {
    move |c| Some(if c < 300 { 0 } else { drop })
}

#[test]
fn floor_down_to_47_px_below_counts_for_stopping_before_a_ledge() {
    // rolling-off-ledges.md, "Stopping before a ledge": standing 10 px
    // from the edge facing it, Action alone.
    let roll = |height: Box<dyn Fn(i32) -> Option<i32>>| {
        let world = ground(height, 290.0);
        roll_rows(
            world,
            3,
            |f| if f == 1 { vec![Action::Action] } else { vec![] },
        )
    };
    for (drop, stops) in [(16, false), (47, false), (49, true), (64, true)] {
        let rows = roll(Box::new(drop_off(drop)));
        assert_eq!(
            rows.iter().all(|r| r.0 == fx(290.0)),
            stops,
            "{drop} px below"
        );
    }
    // Floor higher than the feet counts, and no floor at all doesn't.
    assert!(
        roll(Box::new(drop_off(-64)))
            .iter()
            .any(|r| r.0 != fx(290.0))
    );
    assert!(
        roll(Box::new(|c| (c < 300).then_some(0)))
            .iter()
            .all(|r| r.0 == fx(290.0))
    );
}

#[test]
fn a_roll_stopped_before_a_ledge_pushes_off_away_from_it() {
    // rolling-off-ledges.md, "Pushing off": holding the direction away
    // from the edge turns the roll and pushes off on the same frame.
    let world = ground(|c| (c < 300).then_some(0), 290.0);
    let rows = roll_rows(world, 5, |f| match f {
        1 => vec![Action::Action],
        4 | 5 => vec![Action::Left],
        _ => vec![],
    });
    let xs: Vec<_> = rows.iter().map(|r| (r.0, r.1)).collect();
    assert_eq!(
        xs,
        [
            (fx(290.0), 0),
            (fx(290.0), 0),
            (fx(290.0), 0),
            (fx(288.0), -512),
            (fx(285.875), -544),
        ]
    );
}

/// A `classic-2` world on [`face_64`] standing at `x` with `vx` (1/256),
/// as at the end of a landing frame: the next frame is a face frame with
/// no pull.
fn landed_on_face_64(x: f64, vx: i64) -> World {
    let mut world = ground_in("classic-2", face_64, x);
    world.player.vel.x = fx(vx as f64 / 256.0);
    world
}

#[test]
fn classic_2_limits_come_before_the_block_after_a_landing() {
    // 233/256 is kept, and it goes into the higher column 207: blocked.
    let mut world = landed_on_face_64(206.21875, 184);
    let rows = rows_holding(&mut world, &[Action::Right], 2);
    assert_eq!(rows[0], (fx(206.0), 0, Fixed::from_int(-25), true));
    // Then the pull: 40/256, limited to 0.75 down the face.
    assert_eq!(rows[1], (fx(205.25), -192, Fixed::from_int(-21), true));
    // 331/256 is limited to 0.75 down the face, so nothing blocks it.
    let mut world = landed_on_face_64(206.65625, 296);
    let rows = rows_holding(&mut world, &[Action::Right], 1);
    assert_eq!(rows[0], (fx(205.90625), -192, Fixed::from_int(-21), true));
}

/// A piece of floor: how far down its `n`th column is, px.
type Piece = fn(i32) -> i32;

/// Floor going down to the right over the piece of columns 96 to 111 by
/// `piece(n)` px at its `n`th column, then a step down of `drop` px onto
/// flat floor, or with `onto_slope` a 1-in-4 slope, from column 112.
fn step_down(piece: Piece, drop: i32, onto_slope: bool) -> impl Fn(i32) -> Option<i32> {
    move |c| {
        Some(match c {
            ..96 => 0,
            96..112 => piece(c - 96),
            _ => piece(15) + drop + if onto_slope { (c - 112).min(15) / 4 } else { 0 },
        })
    }
}

/// One frame walking right from column 111 of [`step_down`] into column
/// 112: whether still grounded, and how far down the feet went.
fn step_down_frame(piece: Piece, drop: i32, onto_slope: bool) -> (bool, Fixed) {
    let mut world = ground_in("classic-2", step_down(piece, drop, onto_slope), 111.5);
    world.player.vel.x = fx(1.0);
    let before = world.player.pos.y;
    hold(&mut world, &[Action::Right]);
    assert_eq!(world.player.pos.x, fx(112.65625));
    (world.player.grounded(), world.player.pos.y - before)
}

#[test]
fn classic_2_the_floor_is_followed_down_by_the_reach_of_its_piece() {
    // `steep-faces.md`, "Following the floor down (`title-2`)".
    let pieces: [(&str, Piece, i32); 6] = [
        ("flat", |_| 0, 3),
        ("1-in-4", |n| n / 4, 3),
        ("1-in-2", |n| n / 2, 3),
        ("2-in-3", |n| 2 * n / 3, 4),
        ("1-in-1", |n| n, 5),
        ("2-in-1", |n| 2 * n, 6),
    ];
    for (name, piece, reach) in pieces {
        for (onto_slope, most) in [(false, reach), (true, reach + 3)] {
            let followed = step_down_frame(piece, most, onto_slope);
            assert_eq!(
                followed,
                (true, Fixed::from_int(most)),
                "{name} {onto_slope}"
            );
            // 1 px more leaves the ground, down the reach and a frame of
            // gravity.
            let left = step_down_frame(piece, most + 1, onto_slope);
            let fall = Fixed::from_int(reach) + fx(0.4375);
            assert_eq!(left, (false, fall), "{name} {onto_slope}");
        }
    }
}

/// Flat floor at y = 0 to column 111, then flat floor `drop` px lower.
fn flat_step(drop: i32) -> impl Fn(i32) -> Option<i32> {
    move |c| Some(if c < 112 { 0 } else { drop })
}

#[test]
fn classic_2_after_leaving_at_a_small_step_down_the_fall_has_no_step() {
    // `steep-faces.md`, "Steps down from flat floor": (x, y, vy, ground).
    let rows = |drop, frames| {
        let mut world = ground_in("classic-2", flat_step(drop), 110.37109375);
        world.player.vel.x = fx(457.0 / 256.0);
        (0..frames)
            .map(|_| {
                hold(&mut world, &[Action::Right]);
                let p = &world.player;
                let y = p.pos.y - Fixed::from_int(FLOOR);
                (p.pos.x, y, grains(p.vel.y), p.grounded())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rows(4, 3),
        [
            (fx(112.2109375), fx(3.4375), 1, false),
            (fx(114.1015625), fx(3.87890625), 113, false),
            (fx(116.03515625), fx(4.0), 0, true),
        ]
    );
    let deep = rows(8, 4);
    let ys: Vec<_> = deep.iter().map(|r| (r.1, r.2, r.3)).collect();
    assert_eq!(
        ys,
        [
            (fx(3.4375), 1, false),
            (fx(4.875), 368, false),
            (fx(6.75), 480, false),
            (fx(8.0), 0, true),
        ]
    );
}

#[test]
fn classic_2_a_step_down_onto_a_slope_is_walked_down() {
    // A 1-in-4 slope whose first columns are 4 px lower, and a 2-in-3
    // slope whose first column is 6 px lower.
    let slope_at = |drop: i32, piece: Piece| {
        move |c: i32| {
            Some(if c < 112 {
                0
            } else {
                drop + piece((c - 112).min(15))
            })
        }
    };
    for (drop, piece) in [(4, (|n| n / 4) as Piece), (6, |n| 2 * n / 3)] {
        let mut world = ground_in("classic-2", slope_at(drop, piece), 110.37109375);
        world.player.vel.x = fx(457.0 / 256.0);
        hold(&mut world, &[Action::Right]);
        let p = &world.player;
        assert!(p.grounded(), "{drop}");
        assert_eq!(p.pos.y, Fixed::from_int(FLOOR + drop));
    }
}
