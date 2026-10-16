//! Walls in the air on the `classic-1` preset, from the examples in
//! `spec/behavior/air-walls.md`, and "In `title-2`" on `classic-2`.
//! Heights in the spec are relative to the lowest floor; here that floor
//! is at y = 300.

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

/// A level whose floor in column `c` is `height(c)` px below y = 300
/// (negative is above), for columns 0 to 599, drawn as a staircase.
fn ground(height: impl Fn(i32) -> i32) -> World {
    ground_on("classic-1", height)
}

/// As [`ground`], on `preset`.
fn ground_on(preset: &str, height: impl Fn(i32) -> i32) -> World {
    let mut points = vec![[0, 580]];
    for c in 0..600 {
        let y = FLOOR + height(c);
        points.extend([[c, y], [c + 1, y]]);
    }
    points.push([600, 580]);
    points.dedup();
    let src = format!(
        "format = 1\nname = \"air\"\nsize = [600, 600]\nstart = [300, 0]\n\
         [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n"
    );
    let level = Level::from_toml_str(&src, "air").unwrap();
    World::new(Arc::new(level), Arc::new(generic_feel(preset).unwrap()))
}

/// Puts the player in the air at `x`, `y` (above the lowest floor) with
/// speeds in 1/256 px per frame.
fn airborne(world: &mut World, x: f64, y: f64, vx: i64, vy: i64) {
    let p = &mut world.player;
    p.pos.x = fx(x);
    p.pos.y = fx(y) + Fixed::from_int(FLOOR);
    p.vel.x = Fixed::from_bits(vx << 24);
    p.vel.y = Fixed::from_bits(vy << 24);
    p.ground = None;
    p.facing = if vx < 0 { Facing::Left } else { Facing::Right };
}

/// Steps once, holding `held` on this step and the one before.
fn hold(world: &mut World, held: &[Action]) {
    let keys = held.iter().copied().collect::<Actions>();
    step(world, &InputFrame::single(Frame::after(keys, keys)));
}

/// Row after a step: `x`, `y` above the lowest floor, `vx` and `vy` in
/// 1/256, and whether grounded.
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

/// Runs `frames` holding `held` and checks each row against `expected`.
fn check(world: &mut World, held: &[Action], expected: &[(f64, f64, i64, i64, bool)]) {
    for (n, &(x, y, vx, vy, grounded)) in expected.iter().enumerate() {
        hold(world, held);
        assert_eq!(
            row(world),
            (fx(x), fx(y), vx, vy, grounded),
            "frame {}",
            n + 1
        );
    }
}

/// Floor at 0 for columns 100 and up, and a step up to −48 for columns
/// 99 and below.
fn step_up(c: i32) -> i32 {
    if c < 100 { -48 } else { 0 }
}

#[test]
fn a_step_up_jumping_left() {
    let mut world = ground(step_up);
    airborne(&mut world, 104.5, -33.0, -512, -880);
    check(
        &mut world,
        &[Action::Left],
        &[
            (102.5, -36.0, -512, -768, false),
            (100.5, -38.5625, -512, -656, false),
            // Blocked: column 98 is 9.4375 px above the feet.
            (100.0, -40.6875, 0, -544, false),
            // Column 99 is 7.3125 px above: up 8 px, then `vy`.
            (99.75, -50.375, -64, -432, false),
            (99.28125, -51.625, -120, -320, false),
            (98.62109375, -52.4375, -169, -208, false),
            (97.796875, -52.8125, -211, -96, false),
            (96.828125, -48.0, -248, 0, true),
        ],
    );
    assert!(world.events.contains(&Event::Land));
}

#[test]
fn falling_past_a_step_holding_toward_it() {
    let mut world = ground(step_up);
    airborne(&mut world, 100.0, -20.0, 0, 144);
    check(
        &mut world,
        &[Action::Left],
        &[
            (100.0, -19.0, 0, 256, false),
            (100.0, -17.5625, 0, 368, false),
            (100.0, -15.6875, 0, 480, false),
        ],
    );
}

#[test]
fn a_face_rising_to_the_right() {
    let mut world = ground(|c| match c {
        ..200 => 0,
        _ => -1 - 4 * (c - 200),
    });
    airborne(&mut world, 196.25, -2.0, 512, -1372);
    world.player.jump_hold = true;
    check(
        &mut world,
        &[Action::Right, Action::Jump],
        &[
            (198.25, -7.078125, 512, -1300, false),
            (200.25, -11.875, 512, -1228, false),
            (202.25, -16.390625, 512, -1156, false),
            // Blocked: face column 204 is 0.609375 px above the feet.
            (202.0, -20.625, 0, -1084, false),
            (202.25, -24.578125, 64, -1012, false),
            (202.71875, -28.25, 120, -940, false),
            (203.37890625, -31.640625, 169, -868, false),
        ],
    );
}

#[test]
fn stepped_onto_while_falling_lands() {
    // Up to 8 px is stepped onto: falling, it lands at once; a little
    // more blocks.
    for (y, landed) in [(-40.0, true), (-39.99609375, false)] {
        let mut world = ground(step_up);
        airborne(&mut world, 100.25, y, -512, 64);
        hold(&mut world, &[Action::Left]);
        let (x, feet, vx, vy, grounded) = row(&world);
        if landed {
            assert_eq!(
                (x, feet, vx, vy, grounded),
                (fx(98.25), fx(-48.0), -512, 0, true)
            );
            assert!(world.events.contains(&Event::Land));
        } else {
            assert_eq!((x, vx, grounded), (fx(100.0), 0, false));
        }
    }
}

/// The synthetic floor of "Examples (`title-2`)": 0 up to column 199, a
/// face rising to the right over columns 200 to 215, and −64 from 216 on,
/// 3 px above the face's top column.
fn face_2(c: i32) -> i32 {
    match c {
        ..200 => 0,
        200..216 => -1 - 4 * (c - 200),
        _ => -64,
    }
}

/// A `classic-2` level with the floor of [`face_2`].
fn classic_2() -> World {
    ground_on("classic-2", face_2)
}

/// 1/256 px.
fn grain() -> f64 {
    1.0 / 256.0
}

#[test]
fn classic_2_jumping_into_the_face() {
    let mut world = classic_2();
    airborne(&mut world, 194.5, -3.25, 512, -1450);
    world.player.jump_hold = true;
    let held = [Action::Right, Action::Jump];
    hold(&mut world, &held);
    hold(&mut world, &held);
    check(
        &mut world,
        &held,
        &[
            // Column 200 (−1) is below the feet.
            (200.671875, -18.1328125, 533, -1180, false),
            (202.7734375, -22.390625, 538, -1090, false),
            (204.890625, -26.296875, 542, -1000, false),
            // Blocked: column 207 (−29) is 2.703125 px above the feet.
            (204.0, -29.8515625, 0, -910, false),
            (204.28125, -33.0546875, 72, -820, false),
            (204.80859375, -35.90625, 135, -730, false),
            // Column 205 (−21) is below the feet.
            (205.55078125, -38.40625, 190, -640, false),
        ],
    );
}

#[test]
fn classic_2_blocked_and_landed_on_a_face_column() {
    // Column 202 (−9) is 1.5 px above the feet: blocked. The feet move to
    // −5.890625, within 3 px of face column 201's floor (−5).
    let mut world = classic_2();
    airborne(&mut world, 201.75, -7.5, 200, 300);
    hold(&mut world, &[Action::Right]);
    assert_eq!(row(&world), (fx(201.0), fx(-5.0), 0, 0, true));
    assert!(world.events.contains(&Event::Land));
}

#[test]
fn classic_2_onto_the_floor_beyond_the_face_in_one_running_frame() {
    // The frame's `vx` is 744/256, taking `x` from 213.5 into column 216
    // past face columns 214 and 215.
    let run = |y: f64, vy: i64| {
        let mut world = classic_2();
        airborne(&mut world, 213.5, y, 768, vy);
        hold(&mut world, &[Action::Right]);
        row(&world)
    };
    // U 3 and 4, falling: stepped onto, and lands.
    assert_eq!(run(-60.5, 300), (fx(216.40625), fx(-64.0), 744, 0, true));
    assert_eq!(run(-59.125, 300), (fx(216.40625), fx(-64.0), 744, 0, true));
    // U 5: blocked, and 3.890625 px above face column 213's floor (−53),
    // still airborne.
    assert_eq!(run(-58.5, 300), (fx(213.0), fx(-56.890625), 0, 412, false));
    // U 3, rising: lifted by 3 whole pixels, then moved by `vy`.
    assert_eq!(
        run(-60.25, -600),
        (fx(216.40625), fx(-65.15625), 744, -488, false)
    );
}

#[test]
fn classic_2_wall_limits_are_whole_pixels() {
    // Rising into column `to` from `x` with the feet at `y`: whether the
    // player is blocked.
    let blocked = |x: f64, y: f64| {
        let mut world = classic_2();
        airborne(&mut world, x, y, 128, -600);
        hold(&mut world, &[Action::Right]);
        world.player.vel.x == Fixed::ZERO
    };
    // Face column 200 (−1): 1 px above the feet blocks, 255/256 doesn't.
    assert!(blocked(199.5, 0.0));
    assert!(!blocked(199.5, -grain()));
    // Column 216 (−64), not a face column: 4 255/256 px above the feet is
    // stepped onto, 5 blocks.
    assert!(!blocked(215.5, -59.0 - grain()));
    assert!(blocked(215.5, -59.0));
}

#[test]
fn classic_2_landing_margin_follows_the_column_at_a_face() {
    // One frame falling from `x` and `y`, ending `vy` px lower, with
    // nothing held and no horizontal speed: whether it lands, and `y`.
    let fall = |x: f64, y: f64, vy: f64| {
        let mut world = classic_2();
        // Gravity is 112/256.
        airborne(&mut world, x, y, 0, (vy * 256.0) as i64 - 112);
        hold(&mut world, &[]);
        (world.player.grounded(), row(&world).1)
    };
    // Face column 213 (−53): ending 3 px above lands, 1/256 short doesn't.
    assert_eq!(fall(213.5, -58.0, 2.0), (true, fx(-53.0)));
    assert_eq!(
        fall(213.5, -58.0 - grain(), 2.0),
        (false, fx(-56.0 - grain()))
    );
    // With no vertical speed too.
    assert_eq!(fall(213.5, -56.0, 0.0), (true, fx(-53.0)));
    // The flat columns sharing a piece with the face's top have no margin.
    assert_eq!(fall(218.5, -67.0, 2.0), (false, fx(-65.0)));
    assert_eq!(fall(218.5, -66.0, 2.0), (true, fx(-64.0)));
}
