//! Riding the animal into an exit and coming back on `classic-1`
//! (spec/behavior/exits.md, "Coming back to the level", "Riding into the
//! exit"): the ridden walk-out is the same as on foot, the lead is on
//! foot in the areas the exit leads to, and back in the level it rides
//! from frame 0, after walking in as on foot or, out of the room, up to
//! the landing frame as on foot.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Arrival, Level};
use clamber_sim::{
    Behavior, Facing, Fixed, InputFrame, Outcome, Phase, RidePart, Riding, World, generic_feel,
    step,
};

const R: Action = Action::Right;
const J: Action = Action::Jump;

/// The floor's top and the exit's `y`.
const F: i32 = 400;
const EXIT: usize = 0;
const HUTCH: usize = 1;

/// A flat floor with an exit at x = 300, an animal crate further right,
/// and the arrivals coming back from the scene (walking in) and the room
/// (out of the room-return entrance).
fn level() -> Arc<Level> {
    let src = format!(
        r#"
        format = 1
        name = "exit-riding"
        size = [3000, 700]
        start = [320, {F}]

        [[arrivals]]
        name = "from-scene"
        at = [700, {F}]
        walk = "right"

        [[arrivals]]
        name = "from-room"
        at = [900, {}]
        entrance = "room-return"

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 3000, 100]

        [[entities]]
        type = "exit"
        at = [300, {F}]

        [[entities]]
        type = "hutch"
        at = [1500, {F}]

        [[entities]]
        type = "entrance"
        id = "room-return"
        at = [900, {}]
        "#,
        F - 80,
        F - 80,
    );
    Arc::new(Level::from_toml_str(&src, "exit-riding").unwrap())
}

/// The level with the lead standing within the exit's reach facing
/// `facing`, riding the animal if `riding`.
fn start(riding: bool, facing: Facing) -> World {
    let mut world = World::new(level(), Arc::new(generic_feel("classic-1").unwrap()));
    world.player.facing = facing;
    if riding {
        let Some(Behavior::Hutch(hutch)) = world.things[HUTCH].archetype.map(|a| a.behavior) else {
            panic!("no animal crate");
        };
        world.things[HUTCH].phase = Phase::Ridden;
        world.things[HUTCH].pos = world.player.pos;
        world.player.riding = Some(Riding {
            on_floor: true,
            was_on_floor: true,
            mount: None,
            ..Riding::new(hutch.ride, Fixed::ZERO)
        });
    }
    world
}

fn nothing() -> InputFrame {
    InputFrame::single(Frame::default())
}

/// The lead's position and speed.
fn motion(world: &World) -> (Fixed, Fixed, Fixed, Fixed) {
    let p = &world.player;
    (p.pos.x, p.pos.y, p.vel.x, p.vel.y)
}

/// 1/256 px per frame.
fn grains(v: Fixed) -> i64 {
    v.to_bits() >> 24
}

/// Whether the animal is ridden, under the lead at its `x` in whole px.
fn under_the_lead(world: &World) -> bool {
    let animal = &world.things[HUTCH];
    animal.active
        && animal.phase == Phase::Ridden
        && animal.pos.x.floor() == world.player.pos.x.floor()
}

/// Plays `world` with nothing held until it has left through the exit,
/// checking the animal on every frame if it's ridden. Returns the
/// frames each step ended on, with the step the exit took over on.
fn leave(world: &mut World) -> (Vec<(Fixed, Fixed, Fixed, Fixed)>, usize) {
    let riding = world.player.riding.is_some();
    let mut rows = Vec::new();
    let mut took_over = None;
    while world.flow.outcome == Outcome::Playing {
        assert!(rows.len() < 600, "never left");
        step(world, &nothing());
        if took_over.is_none() && world.leaving.is_some() {
            took_over = Some(rows.len());
        }
        rows.push(motion(world));
        if riding {
            assert!(world.player.riding.is_some(), "step {}", rows.len());
            assert!(under_the_lead(world), "step {}", rows.len());
        }
    }
    assert_eq!(world.flow.outcome, Outcome::Exited(EXIT));
    (rows, took_over.expect("taken over"))
}

/// The ridden walk-out is the on-foot one frame for frame, with the
/// animal under the lead all the way, facing either way.
#[test]
fn the_ridden_walk_out_is_the_one_on_foot() {
    for facing in [Facing::Left, Facing::Right] {
        let (mut ridden, mut on_foot) = (start(true, facing), start(false, facing));
        let (rows, t) = leave(&mut ridden);
        let (control, control_t) = leave(&mut on_foot);
        assert_eq!((rows.len(), t), (control.len(), control_t), "{facing:?}");
        assert_eq!(rows[t..], control[t..], "{facing:?}");
        let ye = Fixed::from_int(F + 1);
        assert_eq!(rows[t].1, ye, "{facing:?}");
        assert_eq!(rows[t + 1].0, rows[t].0, "{facing:?}");
        assert_eq!(rows[t + 1].2, Fixed::from_int(-2), "{facing:?}");
        for k in t + 2..rows.len() {
            assert_eq!(rows[k].0, rows[k - 1].0 - Fixed::from_int(2), "step {k}");
            assert_eq!(rows[k].1, ye, "step {k}");
        }
    }
}

/// The level after leaving it through the exit, riding if `riding`.
fn left(riding: bool) -> World {
    let mut world = start(riding, Facing::Left);
    leave(&mut world);
    world
}

/// The area the exit leads to, as play comes into it from `from`.
fn elsewhere(from: &World) -> World {
    let arrival = Arrival {
        name: "elsewhere".into(),
        at: from.level.start,
        walk: None,
        entrance: None,
        drop: false,
    };
    let mut other = World::arriving(level(), from.feel.clone(), &arrival).carrying(from);
    other.things.clear();
    other
}

fn back(world: &World, arrival: &str) -> World {
    let arrival = world.level.arrival(arrival).unwrap().clone();
    world.coming_back_from(&arrival, &elsewhere(world))
}

/// Elsewhere the lead is on foot, without the animal.
#[test]
fn elsewhere_the_lead_is_on_foot() {
    let mut other = elsewhere(&left(true));
    for _ in 0..60 {
        assert!(other.player.riding.is_none());
        step(&mut other, &nothing());
    }
}

/// Exactly one animal, ridden, and nothing at the crate's place.
fn one_animal(world: &World) {
    let hutches = world
        .things
        .iter()
        .filter(|t| matches!(t.archetype.map(|a| a.behavior), Some(Behavior::Hutch(_))))
        .count();
    assert_eq!(hutches, 1);
    assert!(world.player.riding.is_some());
    assert!(under_the_lead(world));
}

/// After the riding speed takes over on step `from`: 200 frames riding,
/// then Right and a ridden jump.
fn rides_on(world: &mut World) {
    for frame in 0..200 {
        step(world, &nothing());
        assert!(world.player.riding.is_some(), "frame {frame}");
        one_animal(world);
    }
    let mut held = Actions::NONE;
    for keys in std::iter::repeat_n(&[R][..], 40).chain([&[R, J][..]]) {
        let now: Actions = keys.iter().copied().collect();
        step(world, &InputFrame::single(Frame::after(held, now)));
        held = now;
    }
    assert!(world.player.vel.x > Fixed::ZERO);
    let part = world.player.riding.map(|r| r.part);
    assert!(matches!(part, Some(RidePart::Jump(_))), "{part:?}");
}

/// Coming back after the scene, the lead rides from frame 0 and walks in
/// as on foot up to frame 27; from frame 28 the speed eases off as a
/// rider's.
#[test]
fn back_after_the_scene_riding() {
    let (mut ridden, mut on_foot) = (
        back(&left(true), "from-scene"),
        back(&left(false), "from-scene"),
    );
    assert!(ridden.player.riding.is_some());
    for frame in 0..=27 {
        step(&mut ridden, &nothing());
        step(&mut on_foot, &nothing());
        assert_eq!(motion(&ridden), motion(&on_foot), "frame {frame}");
        assert!(ridden.player.riding.is_some(), "frame {frame}");
        assert_eq!(under_the_lead(&ridden), frame >= 1, "frame {frame}");
        assert!(on_foot.player.riding.is_none());
        assert_eq!(on_foot.things[HUTCH].phase, Phase::Ready);
    }
    assert_eq!(grains(ridden.player.vel.x), 768);
    for vx in [744, 721] {
        step(&mut ridden, &nothing());
        assert_eq!(grains(ridden.player.vel.x), vx);
        one_animal(&ridden);
    }
    rides_on(&mut ridden);
}

/// Coming back after the room, the level begins as on foot up to and
/// including the landing frame; from the next the speed eases off as a
/// rider's.
#[test]
fn back_after_the_room_riding() {
    let (mut ridden, mut on_foot) = (
        back(&left(true), "from-room"),
        back(&left(false), "from-room"),
    );
    let mut frame = 0;
    loop {
        assert!(frame < 300, "never landed");
        assert!(ridden.player.riding.is_some(), "frame {frame}");
        step(&mut ridden, &nothing());
        step(&mut on_foot, &nothing());
        assert_eq!(motion(&ridden), motion(&on_foot), "frame {frame}");
        if frame > 0 && on_foot.player.grounded() {
            break;
        }
        frame += 1;
    }
    assert!(ridden.player.grounded());
    let landing = grains(ridden.player.vel.x);
    assert!(landing > 0);
    step(&mut ridden, &nothing());
    assert_eq!(grains(ridden.player.vel.x), landing - (landing >> 5));
    one_animal(&ridden);
    rides_on(&mut ridden);
}
