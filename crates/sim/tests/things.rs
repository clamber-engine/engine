//! Enemies and interactive objects played through whole worlds.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    DEFEATED_FRAMES, Event, Facing, Fixed, FlowEvent, InputFrame, LAUNCH_REST, Outcome, Phase,
    START_LIVES, State, World, generic_feel, step,
};

/// Ground at y = 400 from x = 0 to 600, with a wall from x = 100 to 120
/// standing on it, and the player at `start`. `entities` is appended.
fn world(start: [i32; 2], entities: &str) -> World {
    world_on("default", start, entities)
}

/// [`world`] on the given feel preset.
fn world_on(preset: &str, start: [i32; 2], entities: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "arena"
        size = [1000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 600, 200]

        [[collision]]
        shape = "rect"
        rect = [100, 300, 20, 100]

        {entities}
        "#,
        start[0], start[1]
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(Arc::new(level), Arc::new(generic_feel(preset).unwrap()))
}

/// What happened while holding `held` for `frames` steps.
#[derive(Default)]
struct Seen {
    events: Vec<Event>,
    flow: Vec<FlowEvent>,
}

fn hold(world: &mut World, held: Actions, frames: usize) -> Seen {
    let mut seen = Seen::default();
    let mut previous = Actions::NONE;
    for _ in 0..frames {
        step(world, &InputFrame::single(Frame::after(previous, held)));
        previous = held;
        seen.events.extend(&world.events);
        seen.flow.extend(&world.flow.events);
    }
    seen
}

#[test]
fn walkers_turn_at_ledges_and_walls() {
    // The player waits on top of the wall, out of the way.
    let mut w = world(
        [110, 300],
        r#"
        [[entities]]
        type = "crawler"
        at = [560, 400]
        params = { facing = "right" }
        "#,
    );
    let mut xs = Vec::new();
    for _ in 0..1600 {
        hold(&mut w, Actions::NONE, 1);
        xs.push(w.things[0].pos.x);
        assert_eq!(w.things[0].pos.y, Fixed::from_int(400));
    }
    let max = *xs.iter().max().unwrap();
    let min = *xs.iter().min().unwrap();
    // It turns with its front edge at the ledge and at the wall.
    assert_eq!(max, Fixed::from_int(590));
    assert!(
        min >= Fixed::from_int(130) && min < Fixed::from_int(131),
        "{min}"
    );
    assert!(w.things[0].active);
}

#[test]
fn touching_an_enemy_costs_a_life() {
    let mut w = world(
        [200, 400],
        r#"
        [[entities]]
        type = "crawler"
        at = [260, 400]
        "#,
    );
    let seen = hold(&mut w, Action::Right.into(), 60);
    assert!(seen.events.contains(&Event::Hurt));
    assert_eq!(seen.flow.first(), Some(&FlowEvent::Die));
    assert_eq!(w.flow.lives, START_LIVES - 1);
    assert_eq!(w.player.state, State::Hurt);
    assert!(matches!(w.flow.outcome, Outcome::Dying(_)));
    assert_eq!(w.things[0].phase, Phase::Ready);
}

#[test]
fn landing_on_an_enemy_defeats_it_and_bounces() {
    let mut w = world(
        [300, 300],
        r#"
        [[entities]]
        type = "crawler"
        at = [300, 400]
        params = { speed = 0 }
        "#,
    );
    let mut bounced = None;
    for _ in 0..60 {
        let seen = hold(&mut w, Actions::NONE, 1);
        if seen.events.contains(&Event::Bounce) {
            assert_eq!(seen.flow, [FlowEvent::Defeat(0)]);
            bounced = Some(w.player.vel.y);
            break;
        }
    }
    assert_eq!(bounced, Some(-Fixed::from_int(6)));
    assert_eq!(w.player.state, State::Bounce);
    assert_eq!(w.things[0].phase, Phase::Defeated(DEFEATED_FRAMES));

    // It shows as defeated for a while, then it's gone; the player is
    // bouncing until it lands.
    hold(&mut w, Actions::NONE, DEFEATED_FRAMES as usize - 1);
    assert!(w.things[0].active);
    assert_eq!(w.player.state, State::Bounce);
    let seen = hold(&mut w, Actions::NONE, 60);
    assert!(!w.things[0].active);
    assert!(seen.events.contains(&Event::Land));
    assert!(!seen.events.contains(&Event::Hurt));
    assert_eq!(w.flow.lives, START_LIVES);
}

#[test]
fn rolling_into_an_enemy_defeats_it() {
    let mut w = world(
        [140, 400],
        r#"
        [[entities]]
        type = "crawler"
        at = [380, 400]
        params = { speed = 0 }
        "#,
    );
    hold(
        &mut w,
        Actions::from(Action::Right) | Action::Action.into(),
        40,
    );
    let seen = hold(
        &mut w,
        Actions::from(Action::Down) | Action::Action.into(),
        120,
    );
    assert!(seen.events.contains(&Event::RollStart));
    assert!(seen.flow.contains(&FlowEvent::Defeat(0)), "{:?}", w.player);
    assert!(!seen.events.contains(&Event::Hurt));
    assert!(!seen.events.contains(&Event::Bounce));
    assert_eq!(w.flow.lives, START_LIVES);
}

#[test]
fn patrols_fly_their_path_and_back() {
    let mut w = world(
        [300, 400],
        r#"
        [[entities]]
        type = "flutter"
        at = [500, 200]
        params = { path = [-60, -40], period = 120 }
        "#,
    );
    hold(&mut w, Actions::NONE, 30);
    assert_eq!(w.things[0].pos, Vec2::ints(470, 180));
    assert_eq!(w.things[0].facing, Facing::Left);
    hold(&mut w, Actions::NONE, 30);
    assert_eq!(w.things[0].pos, Vec2::ints(440, 160));
    hold(&mut w, Actions::NONE, 1);
    assert_eq!(w.things[0].facing, Facing::Right);
    hold(&mut w, Actions::NONE, 59);
    assert_eq!(w.things[0].pos, Vec2::ints(500, 200));
}

#[test]
fn launchers_fire_the_player_then_rest() {
    let mut w = world(
        [200, 400],
        r#"
        [[entities]]
        type = "launcher"
        at = [260, 392]
        "#,
    );
    let mut fired = false;
    for _ in 0..60 {
        let seen = hold(&mut w, Action::Right.into(), 1);
        if seen.flow.contains(&FlowEvent::Launch(0)) {
            fired = true;
            break;
        }
    }
    assert!(fired);
    assert_eq!(w.player.vel.y, -Fixed::from_ratio_pow2(27, 1));
    assert!(!w.player.grounded());
    assert_eq!(w.things[0].phase, Phase::Fired(0));

    let seen = hold(&mut w, Actions::NONE, 1);
    assert!(w.player.vel.y.is_negative());
    assert_eq!(w.player.state, State::Fall);
    assert!(seen.flow.is_empty());
    hold(&mut w, Actions::NONE, LAUNCH_REST as usize - 1);
    assert_eq!(w.things[0].phase, Phase::Ready);
    assert!(w.player.pos.y < Fixed::from_int(250));
}

#[test]
fn entity_params_are_checked_against_their_archetype() {
    let src = r#"
        format = 1
        size = [100, 100]
        start = [10, 10]

        [[entities]]
        type = "crawler"
        at = [50, 50]
        params = { velocity = [1, 2] }
    "#;
    let level = Level::from_toml_str(src, "test").unwrap();
    let feel = Arc::new(generic_feel("default").unwrap());
    let err = World::try_new(Arc::new(level), feel).unwrap_err();
    assert!(err.message.contains("entities[0].params.velocity"), "{err}");
}

// Classic 1: bounces off walkers (spec/behavior/bounce.md,
// walker-contact.md) and how walkers move (walker.md).

/// A walker as in walker-contact.md's small walker, with the sizes the
/// classic-1 bounce box goes with.
fn small_walker(x: &str, speed: &str, facing: &str) -> String {
    format!(
        r#"
        [[entities]]
        type = "crawler"
        at = [{x}, 400]
        params = {{ speed = {speed}, response = 0.125, facing = "{facing}", size = [20, 12], offset = 0.5 }}
        "#
    )
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Steps `world` holding jump on the frames `jump` says, by frame count
/// from 0, and returns the player's `(y, vy)` and events after each.
fn play(
    world: &mut World,
    frames: usize,
    jump: impl Fn(usize) -> bool,
) -> Vec<(Fixed, Fixed, Vec<Event>)> {
    let mut previous = Actions::NONE;
    (0..frames)
        .map(|n| {
            let held = if jump(n) {
                Action::Jump.into()
            } else {
                Actions::NONE
            };
            step(world, &InputFrame::single(Frame::after(previous, held)));
            previous = held;
            (world.player.pos.y, world.player.vel.y, world.events.clone())
        })
        .collect()
}

/// A classic-1 player falling from 100 px up onto a still walker under
/// it: the world and the frame it bounces on.
fn classic_bounce() -> (World, usize) {
    let mut w = world_on("classic-1", [300, 300], &small_walker("300", "0", "left"));
    let steps = play(&mut w, 60, |_| false);
    let b = steps
        .iter()
        .position(|s| s.2.contains(&Event::Bounce))
        .expect("bounces");
    (
        world_on("classic-1", [300, 300], &small_walker("300", "0", "left")),
        b,
    )
}

#[test]
fn classic_bounce_sets_vy_then_holds_still_for_two_frames() {
    let (mut w, b) = classic_bounce();
    // Direction input in the frozen frames is ignored.
    let mut previous = Actions::NONE;
    let mut rows = Vec::new();
    for n in 0..=b + 3 {
        let held = if n > b {
            Action::Right.into()
        } else {
            Actions::NONE
        };
        step(&mut w, &InputFrame::single(Frame::after(previous, held)));
        previous = held;
        rows.push((w.player.pos, w.player.vel, w.player.state));
    }
    let (at_b, vel_b, state_b) = rows[b];
    assert_eq!(vel_b.y, fx(-7.125));
    assert_eq!(state_b, State::Bounce);
    assert_eq!(rows[b + 1], (at_b, vel_b, State::Bounce));
    assert_eq!(rows[b + 2], (at_b, vel_b, State::Bounce));
    // B + 3 moves again, under the heavier gravity: jump isn't held.
    let (at, vel, _) = rows[b + 3];
    assert_eq!(vel.y, fx(-7.125 + 0.4375));
    assert_eq!(at.y, at_b.y + vel.y);
    assert!(vel.x > Fixed::ZERO);
    assert_eq!(w.things[0].phase, Phase::Defeated(DEFEATED_FRAMES - 4));
}

#[test]
fn classic_bounce_height_by_how_long_jump_is_held_from_b_plus_3() {
    let (_, b) = classic_bounce();
    let table = [
        (0, 54.5),
        (1, 57.0),
        (2, 59.34375),
        (3, 61.6875),
        (4, 63.875),
        (5, 65.9375),
        (8, 71.625),
        (10, 74.90625),
        (15, 81.375),
        (20, 85.3125),
        (25, 86.71875),
        (40, 86.71875),
    ];
    for (k, height) in table {
        let (mut w, _) = classic_bounce();
        let steps = play(&mut w, b + 80, |n| n >= b + 3 && n < b + 3 + k);
        let y_b = steps[b].0;
        let landed = steps
            .iter()
            .position(|s| s.2.contains(&Event::Land))
            .unwrap();
        let top = steps[b..landed].iter().map(|s| s.0).min().unwrap();
        assert_eq!(y_b - top, fx(height), "K = {k}");
    }
}

#[test]
fn classic_bounce_hold_counts_only_from_b_plus_3() {
    let (_, b) = classic_bounce();
    let height = |jump: &dyn Fn(usize) -> bool| {
        let (mut w, _) = classic_bounce();
        let steps = play(&mut w, b + 40, jump);
        let top = steps[b..].iter().map(|s| s.0).min().unwrap();
        steps[b].0 - top
    };
    // Held through the bounce but let go on B + 3: no help.
    assert_eq!(height(&|n| n < b + 3), fx(54.5));
    // Held from before the bounce on: as a hold from B + 3.
    assert_eq!(height(&|_| true), fx(86.71875));
    // Pressed on B + 4 or later: never the lighter gravity.
    assert_eq!(height(&|n| n >= b + 4), fx(54.5));
    // A one-frame break gives the heavier gravity for the rest.
    assert_eq!(height(&|n| n >= b + 3 && n != b + 4), fx(57.0));
}

#[test]
fn classic_bounce_arc_follows_the_formula() {
    let (mut w, b) = classic_bounce();
    let steps = play(&mut w, b + 40, |_| false);
    let y_b = steps[b].0;
    let g = 0.4375;
    for n in 1..=15 {
        let (y, vy, _) = &steps[b + 2 + n];
        let n = n as f64;
        assert_eq!(*vy, fx(-7.125 + g * n), "n = {n}");
        assert_eq!(
            *y - y_b,
            fx(-7.125 * n + g * n * (n + 1.0) / 2.0),
            "n = {n}"
        );
    }
}

/// Whether a classic-1 player facing `facing`, falling straight down
/// `i` px from a still small walker heading `heading`, bounces off it.
fn classic_bounces_at(i: i32, facing: Facing, heading: &str) -> bool {
    let mut w = world_on(
        "classic-1",
        [300 + i, 340],
        &small_walker("300", "0", heading),
    );
    w.player.facing = facing;
    let steps = play(&mut w, 40, |_| false);
    steps.iter().any(|s| s.2.contains(&Event::Bounce))
}

#[test]
fn classic_bounce_ranges_for_the_small_walker() {
    let table = [
        (Facing::Right, "left", -21, 34),
        (Facing::Right, "right", -20, 35),
        (Facing::Left, "left", -35, 20),
        (Facing::Left, "right", -34, 21),
    ];
    for (facing, heading, low, high) in table {
        for (i, bounces) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                classic_bounces_at(i, facing, heading),
                bounces,
                "I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

#[test]
fn classic_bounce_needs_the_player_not_to_be_rising_fast() {
    // vy after this frame's gravity: -1.1875 bounces, -1.25 doesn't.
    for (vy, bounces) in [(-1.1875, true), (-1.25, false)] {
        let mut w = world_on("classic-1", [300, 380], &small_walker("300", "0", "left"));
        w.player.vel.y = fx(vy - 0.4375);
        let steps = play(&mut w, 1, |_| false);
        assert_eq!(steps[0].1 == fx(-7.125), bounces, "vy {vy}");
        assert_eq!(steps[0].2.contains(&Event::Bounce), bounces, "vy {vy}");
    }
}

#[test]
fn classic_walker_eases_its_speed() {
    // Heading left into the wall, it stops there, then speeds up to the
    // right: full speed on the 32nd frame (walker.md).
    let mut w = world_on("classic-1", [110, 300], &small_walker("160", "1", "left"));
    let mut speeds = Vec::new();
    for _ in 0..120 {
        hold(&mut w, Actions::NONE, 1);
        speeds.push(w.things[0].vx);
    }
    assert!(speeds[..20].iter().all(|&v| v == -Fixed::ONE));
    let stop = speeds.iter().position(|&v| v == Fixed::ZERO).unwrap();
    let grains: Vec<i64> = speeds[stop + 1..stop + 33]
        .iter()
        .map(|v| v.to_bits() * 256 / Fixed::ONE.to_bits())
        .collect();
    assert_eq!(
        grains,
        [
            32, 60, 84, 105, 123, 139, 153, 165, 176, 186, 194, 201, 207, 213, 218, 222, 226, 229,
            232, 235, 237, 239, 241, 242, 243, 244, 245, 246, 247, 248, 249, 256
        ]
    );
    assert_eq!(w.things[0].facing, Facing::Right);
}
