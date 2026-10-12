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
    World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
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
    assert_eq!(bounced, Some(-Fixed::from_int(5)));
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
    assert_eq!(w.player.vel.y, -Fixed::from_int(11));
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
