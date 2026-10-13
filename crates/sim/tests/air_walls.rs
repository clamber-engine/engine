//! Walls in the air on the `classic-1` preset, from the examples in
//! `spec/behavior/air-walls.md`. Heights in the spec are relative to the
//! lowest floor; here that floor is at y = 300.

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
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
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
