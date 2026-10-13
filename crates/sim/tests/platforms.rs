//! Platforms passed up through from below on the `classic-1` preset, from
//! the examples in `spec/behavior/platforms-from-below.md`. Heights in the
//! spec are relative to the floor beneath; here that floor is at y = 300.

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

/// Floor at 0 for columns 0 to 599, and a platform 15 px thick above it:
/// its top at −78 for columns 150 to 249 and its underside at −63, with
/// ends that are faces of 4 px per column (tops at −67, −71 and −75 for
/// columns 147 to 149, and the same mirrored for 250 to 252). One-way if
/// `one_way`, otherwise ground that isn't passable from below.
fn platform(one_way: bool) -> World {
    // Top and underside, left to right, above the floor.
    let outline = [
        (147, -63),
        (147, -67),
        (148, -67),
        (148, -71),
        (149, -71),
        (149, -75),
        (150, -75),
        (150, -78),
        (250, -78),
        (250, -75),
        (251, -75),
        (251, -71),
        (252, -71),
        (252, -67),
        (253, -67),
        (253, -63),
    ];
    let points: Vec<[i32; 2]> = outline.iter().map(|&(x, h)| [x, FLOOR + h]).collect();
    let src = format!(
        "format = 1\nname = \"platform\"\nsize = [600, 600]\nstart = [300, 0]\n\
         [[collision]]\nshape = \"rect\"\nrect = [0, {FLOOR}, 600, 280]\n\
         [[collision]]\nshape = \"polygon\"\npoints = {points:?}\none_way = {one_way}\n"
    );
    let level = Level::from_toml_str(&src, "platform").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// Puts the player in the air at `x`, `y` (above the floor) with speeds
/// in 1/256 px per frame.
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

/// Row after a step: `x`, `y` above the floor, `vx` and `vy` in 1/256,
/// and whether grounded.
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

/// Runs to the last frame in `expected`, holding `held`, and checks the
/// rows of the frames listed there.
fn check(world: &mut World, held: &[Action], expected: &[(u32, f64, f64, i64, i64, bool)]) {
    let last = expected.last().unwrap().0;
    for frame in 1..=last {
        hold(world, held);
        if let Some(&(_, x, y, vx, vy, grounded)) = expected.iter().find(|e| e.0 == frame) {
            assert_eq!(
                row(world),
                (fx(x), fx(y), vx, vy, grounded),
                "frame {frame}"
            );
        }
    }
}

/// Runs until the player lands, and returns where.
fn land(world: &mut World) -> Fixed {
    for _ in 0..200 {
        hold(world, &[]);
        if world.player.grounded() {
            assert!(world.events.contains(&Event::Land));
            return world.player.pos.y - Fixed::from_int(FLOOR);
        }
    }
    panic!("never landed");
}

#[test]
fn lifted_onto_the_top() {
    let mut world = platform(true);
    airborne(&mut world, 200.5, -70.25, 0, -704);
    world.player.inside = true;
    check(
        &mut world,
        &[],
        &[
            (1, 200.5, -72.5625, 0, -592, false),
            (2, 200.5, -74.4375, 0, -480, false),
            (3, 200.5, -75.875, 0, -368, false),
            // Started 2.125 px below the top: not lifted.
            (4, 200.5, -76.875, 0, -256, false),
            // Started 1.125 px below: up 2 px, then `vy`.
            (5, 200.5, -79.4375, 0, -144, false),
            (6, 200.5, -79.5625, 0, -32, false),
            (7, 200.5, -78.0, 0, 0, true),
        ],
    );
    assert!(world.events.contains(&Event::Land));
}

#[test]
fn falling_back_through() {
    let mut world = platform(true);
    airborne(&mut world, 200.5, -69.5, 0, -640);
    world.player.inside = true;
    check(
        &mut world,
        &[],
        &[
            (1, 200.5, -71.5625, 0, -528, false),
            (2, 200.5, -73.1875, 0, -416, false),
            (3, 200.5, -74.375, 0, -304, false),
            (4, 200.5, -75.125, 0, -192, false),
            (5, 200.5, -75.4375, 0, -80, false),
            // Falling: doesn't land on the top.
            (6, 200.5, -75.3125, 0, 32, false),
            (12, 200.5, -65.375, 0, 704, false),
            (13, 200.5, -62.1875, 0, 816, false),
        ],
    );
    assert!(!world.player.inside);
    assert_eq!(land(&mut world), Fixed::ZERO);
}

#[test]
fn passing_under_the_end_and_rising_inside() {
    let mut world = platform(true);
    airborne(&mut world, 147.25, -58.0, 512, -1100);
    world.player.jump_hold = true;
    check(
        &mut world,
        &[Action::Right, Action::Jump],
        &[
            // Feet below the underside: column 149's top is ignored.
            (1, 149.25, -62.015625, 512, -1028, false),
            (2, 151.25, -65.75, 512, -956, false),
            (3, 153.25, -69.203125, 512, -884, false),
            (4, 155.25, -72.375, 512, -812, false),
            (5, 157.25, -75.265625, 512, -740, false),
            (6, 159.25, -77.875, 512, -668, false),
            // Started 0.125 px below the top: up 1 px, then `vy`.
            (7, 161.25, -81.203125, 512, -596, false),
        ],
    );
}

#[test]
fn blocked_by_the_end() {
    let mut world = platform(true);
    airborne(&mut world, 145.5, -64.5, 512, -800);
    world.player.jump_hold = true;
    check(
        &mut world,
        &[Action::Right, Action::Jump],
        &[(1, 145.0, -67.34375, 0, -728, false)],
    );
}

#[test]
fn stepped_onto_the_flat_top_from_the_end() {
    // Feet in column 149's layer, but not risen into it from below.
    let mut world = platform(true);
    airborne(&mut world, 149.5, -71.0, 512, -600);
    check(
        &mut world,
        &[Action::Right],
        &[
            // Up 7 px to −78, then `vy`.
            (1, 151.5, -79.90625, 512, -488, false),
            (2, 153.5, -81.375, 512, -376, false),
            (3, 155.5, -82.40625, 512, -264, false),
            (4, 157.5, -83.0, 512, -152, false),
            (5, 159.5, -83.15625, 512, -40, false),
            (6, 161.5, -78.0, 512, 0, true),
        ],
    );
}

#[test]
fn a_fall_that_ends_below_the_underside_doesnt_land_on_the_top() {
    // Column 147's top is at −67, 4 px above the underside.
    let mut world = platform(true);
    airborne(&mut world, 147.5, -67.5, 0, 1168);
    check(&mut world, &[], &[(1, 147.5, -62.5, 0, 1280, false)]);
    assert_eq!(land(&mut world), Fixed::ZERO);
    // One that ends in the layer lands.
    let mut world = platform(true);
    airborne(&mut world, 147.5, -68.0, 0, 912);
    check(&mut world, &[], &[(1, 147.5, -67.0, 0, 0, true)]);
}

#[test]
fn stopped_by_ground_that_isnt_passable_from_below() {
    let mut world = platform(false);
    airborne(&mut world, 200.5, -58.25, 0, -1000);
    check(
        &mut world,
        &[],
        &[
            (1, 200.5, -61.71875, 0, -888, false),
            // The move to −64.75 would end in the ground: back down 3 px.
            (2, 200.5, -61.75, 0, 0, false),
            (3, 200.5, -61.3125, 0, 112, false),
            (4, 200.5, -60.4375, 0, 224, false),
        ],
    );
    assert_eq!(land(&mut world), Fixed::ZERO);
}
