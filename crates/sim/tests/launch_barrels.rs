//! Launch barrels in Classic 1 (spec/behavior/launch-barrel.md). They
//! are `sling`s; the one that shoots down has its own values.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Facing, Fixed, InputFrame, Phase, World, generic_feel, step};

use Action::{Down, Jump, Left, Right, Up};

/// The floor's level y.
const F: i32 = 337;

/// The values of the barrel that shoots down, as the importer gives them.
const DOWN: &str = "hold = 28, shot = [0, 8.46484375], glide = 1, breaks = true";

fn px256(n: i64) -> Fixed {
    Fixed::from_bits((n << 32) / 256)
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// A level with the player standing at `player` on flat ground at level y
/// `F` and a launch barrel at `at`, with `params`.
fn world(player: i32, at: (i32, i32), params: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "slings"
        size = [4000, 800]
        start = [{player}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 4000, 200]

        [[entities]]
        type = "sling"
        at = [{}, {}]
        params = {{ {params} }}
        "#,
        at.0, at.1
    );
    let level = Arc::new(Level::from_toml_str(&src, "slings").unwrap());
    let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    assert!(world.player.grounded());
    world
}

/// Steps once with `keys` held, pressed on this step if not held on the
/// one before.
fn step_keys(world: &mut World, before: &[Action], keys: &[Action]) {
    let held = keys.iter().copied().collect::<Actions>();
    let was = before.iter().copied().collect::<Actions>();
    step(world, &InputFrame::single(Frame::after(was, held)));
}

fn still(world: &mut World) {
    step_keys(world, &[], &[]);
}

fn caught(world: &World) -> bool {
    world.player.sling.is_some_and(|s| s.since == 0)
}

/// Whether a standing player facing `facing` is caught by a barrel at
/// `X − xb` = `dx` and `D` = `d` on the first step.
fn caught_standing(dx: i32, d: i32, facing: Facing) -> bool {
    let x = 600;
    let mut w = world(x, (x - dx, F - d), "");
    w.player.facing = facing;
    still(&mut w);
    caught(&w)
}

#[test]
fn a_standing_player_is_caught_inside_the_standing_ranges() {
    for (i, d) in [(-24, -34), (27, 32), (-24, 32), (27, -34), (0, 0)] {
        assert!(caught_standing(i, d, Facing::Right), "I = {i}, D = {d}");
    }
    for (i, d) in [(-25, 0), (28, 0), (0, -35), (0, 33)] {
        assert!(!caught_standing(i, d, Facing::Right), "I = {i}, D = {d}");
    }
    // Facing left, X − xb from −30 to 21.
    assert!(caught_standing(-30, 0, Facing::Left));
    assert!(caught_standing(21, 0, Facing::Left));
    assert!(!caught_standing(-31, 0, Facing::Left));
    assert!(!caught_standing(22, 0, Facing::Left));
}

#[test]
fn a_crouching_player_is_caught_inside_the_crouching_ranges() {
    let crouched = |dx: i32, d: i32| {
        let x = 600;
        let mut w = world(x, (x - 200, F), "");
        // Crouch long enough first, then move the barrel into place.
        for _ in 0..30 {
            step_keys(&mut w, &[Down], &[Down]);
        }
        w.things[0].pos = clamber_level::Vec2::ints(x - dx, F - d);
        step_keys(&mut w, &[Down], &[Down]);
        caught(&w)
    };
    for (i, d) in [(-27, -38), (18, 23), (0, 0)] {
        assert!(crouched(i, d), "I = {i}, D = {d}");
    }
    for (i, d) in [(-28, 0), (19, 0), (0, -39), (0, 24)] {
        assert!(!crouched(i, d), "I = {i}, D = {d}");
    }
}

/// launch-barrel.md, "Example": a barrel at level x 400 on the floor, and
/// a player walking right into it at 2 px a frame.
#[test]
fn the_spec_example() {
    let mut w = world(300, (400, F), "");
    w.player.pos.x = fx(374.2265625);
    w.player.vel.x = Fixed::from_int(2);
    w.player.target = Fixed::from_int(2);
    step_keys(&mut w, &[Right], &[Right]);
    let p = &w.player;
    assert!(caught(&w), "x {}", p.pos.x);
    assert_eq!((p.pos.x, p.pos.y), (fx(376.2265625), Fixed::from_int(F)));
    assert_eq!((p.vel.x, p.vel.y), (Fixed::ZERO, Fixed::ZERO));
    assert!(!p.grounded());
    assert_eq!(w.things[0].phase, Phase::Caught(1));
    // C + 1 to C + 15: held at the barrel, whatever is held.
    let keys = [
        &[Left][..],
        &[Right],
        &[Jump],
        &[Up],
        &[Down],
        &[Action::Action],
    ];
    for n in 1..=15 {
        let k = keys[n % keys.len()];
        step_keys(&mut w, &[], k);
        let p = &w.player;
        assert_eq!(p.pos.x, fx(400.2265625), "C + {n}");
        assert_eq!(p.pos.y, Fixed::from_int(F), "C + {n}");
        assert_eq!((p.vel.x, p.vel.y), (Fixed::ZERO, Fixed::ZERO), "C + {n}");
        assert_eq!(p.facing, Facing::Right, "C + {n}");
    }
    // C + 16: facing left at the shot's speed, not moving yet.
    step_keys(&mut w, &[], &[Right]);
    let shot = -px256(2167);
    assert_eq!(w.player.facing, Facing::Left);
    assert_eq!(w.player.vel.x, shot);
    assert_eq!(w.player.pos.x, fx(400.2265625));
    // C + 17 to C + 39: moving at it, no gravity.
    for n in 17..=39 {
        step_keys(&mut w, &[Right], &[Right]);
        let p = &w.player;
        assert_eq!(p.vel.x, shot, "C + {n}");
        assert_eq!(p.pos.y, Fixed::from_int(F), "C + {n}");
        assert_eq!(p.facing, Facing::Left, "C + {n}");
        if n == 17 {
            assert_eq!(p.pos.x, fx(391.76171875));
        }
    }
    assert_eq!(w.player.pos.x, fx(205.53515625));
    // C + 40: the flight's first frame lands.
    still(&mut w);
    let p = &w.player;
    assert_eq!(p.pos.x, fx(197.33203125));
    assert_eq!(p.vel.x, fx(-8.203125));
    assert!(p.grounded());
    assert!(p.sling.is_none());
    // From then the ground rules.
    still(&mut w);
    assert_eq!(w.player.vel.x, px256(-1838));
}

/// A player in the air at a barrel `above` px over the floor is caught,
/// shot and flies with `keys` held: the speeds across and down on the
/// flight's first frames.
fn flight(above: i32, params: &str, keys: &[Action]) -> (World, Vec<(Fixed, Fixed)>) {
    let x = 600;
    let mut w = world(x - 300, (x, F - above), params);
    w.player.pos = clamber_level::Vec2::ints(x, F - above);
    w.player.ground = None;
    still(&mut w);
    assert!(caught(&w));
    let mut speeds = Vec::new();
    while w.player.sling.is_some() {
        step_keys(&mut w, keys, keys);
        speeds.push((w.player.vel.x, w.player.vel.y));
    }
    (w, speeds)
}

#[test]
fn the_flight_steers_by_a_32nd_of_the_gap_and_falls() {
    let (_, speeds) = flight(75, "", &[]);
    // speeds[0] is C + 1; the flight starts on C + 40.
    let flown = &speeds[39..];
    assert_eq!(flown[0], (px256(-2100), fx(0.4375)));
    assert_eq!(flown[1], (px256(-2035), fx(0.875)));
    assert_eq!(flown[2], (px256(-1972), fx(1.3125)));
    let (_, speeds) = flight_left();
    assert_eq!(speeds[39].0, px256(-2124));
    assert_eq!(speeds[40].0, px256(-2082));
    // Down never goes past 8.
    let (w, speeds) = flight(150, "", &[Right]);
    assert!(speeds.iter().all(|&(_, vy)| vy <= Fixed::from_int(8)));
    assert!(speeds.iter().any(|&(_, vy)| vy == Fixed::from_int(8)));
    assert_eq!(w.player.facing, Facing::Left);
    assert!(w.player.grounded());
}

fn flight_left() -> (World, Vec<(Fixed, Fixed)>) {
    flight(75, "", &[Left, Action::Action])
}

#[test]
fn the_barrel_is_ready_again_63_frames_after_a_catch() {
    let mut w = world(400, (400, F), "");
    still(&mut w);
    assert!(caught(&w));
    // C + 1 to C + 40 hold, shoot and land.
    for _ in 1..=40 {
        still(&mut w);
    }
    assert!(w.player.grounded() && w.player.sling.is_none());
    // Put back at the barrel at the end of each frame up to C + 62.
    for n in 41..=63 {
        w.player.pos = clamber_level::Vec2::ints(400, F);
        w.player.vel = clamber_level::Vec2::default();
        still(&mut w);
        assert_eq!(caught(&w), n == 63, "C + {n}");
    }
}

#[test]
fn the_down_barrel_holds_28_frames_breaks_and_shoots_down() {
    let mut w = world(374, (400, F), DOWN);
    w.player.pos.x = fx(375.5);
    step_keys(&mut w, &[], &[Right]);
    assert!(!caught(&w));
    let mut c = None;
    for n in 0..10 {
        step_keys(&mut w, &[Right], &[Right]);
        if caught(&w) {
            c = Some(n);
            break;
        }
    }
    assert!(c.is_some());
    // walking right, caught on the first frame with I = −24.
    assert_eq!(w.player.pos.x.floor() - 400, -24);
    for n in 1..=28 {
        step_keys(&mut w, &[], &[Left]);
        assert!(w.things[0].active, "C + {n}");
        assert_eq!(w.player.facing, Facing::Right, "C + {n}");
    }
    // C + 29: gone, facing right, shot down.
    w.player.facing = Facing::Left;
    still(&mut w);
    assert!(!w.things[0].active);
    assert_eq!(w.things[0].phase, Phase::Broken);
    assert_eq!(w.player.facing, Facing::Right);
    assert_eq!(
        w.player.vel,
        clamber_level::Vec2::new(Fixed::ZERO, px256(2167))
    );
    assert_eq!(w.player.pos.y, Fixed::from_int(F));
    // On the floor, it lands on C + 30.
    still(&mut w);
    assert!(w.player.grounded());
    assert!(w.player.sling.is_none());
}

#[test]
fn the_down_barrel_in_the_air_falls_at_8_and_steers() {
    let (w, speeds) = flight(150, DOWN, &[Right]);
    // C + 29 the shot, C + 30 the glide, then the flight from C + 31.
    assert_eq!(speeds[28], (Fixed::ZERO, px256(2167)));
    assert_eq!(speeds[29], (Fixed::ZERO, px256(2167)));
    assert_eq!(speeds[30], (px256(24), Fixed::from_int(8)));
    assert_eq!(speeds[31], (px256(47), Fixed::from_int(8)));
    assert_eq!(speeds[32], (px256(69), Fixed::from_int(8)));
    assert_eq!(w.player.facing, Facing::Right);
    assert!(w.player.grounded());
}
