//! Level flow played through whole worlds: pickups, checkpoints, losing
//! lives, game over and reaching the goal.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    DYING_FRAMES, FlowEvent, InputFrame, Outcome, Player, START_LIVES, World, generic_feel, step,
};

/// Flat ground at y = 400 from x = 0 to 1000 with a hazard strip on it at
/// x = 600..632, and a pit beyond x = 1000. Two gems and a relic lie in
/// the way, a checkpoint stands at x = 300 and the goal at x = 900.
const TRACK: &str = r#"
    format = 1
    name = "track"
    size = [1400, 600]
    start = [64, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 400, 1000, 200]

    [[collision]]
    shape = "rect"
    rect = [600, 396, 32, 4]
    surface = "hazard"

    [[entities]]
    type = "gem"
    at = [150, 390]

    [[entities]]
    type = "gem"
    at = [200, 390]

    [[entities]]
    type = "relic"
    at = [250, 390]

    [[entities]]
    type = "checkpoint"
    at = [300, 400]

    [[entities]]
    type = "goal"
    at = [900, 400]
"#;

fn world(src: &str) -> World {
    let level = Level::from_toml_str(src, "test").unwrap();
    World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
}

/// Holds `held` for up to `frames` steps, or until a step with a flow
/// event when `until_event`. Returns the flow events seen.
fn play(world: &mut World, held: Actions, frames: usize, until_event: bool) -> Vec<FlowEvent> {
    let mut previous = Actions::NONE;
    let mut events = Vec::new();
    for _ in 0..frames {
        step(world, &InputFrame::single(Frame::after(previous, held)));
        previous = held;
        events.extend(&world.flow.events);
        if until_event && !events.is_empty() {
            break;
        }
    }
    events
}

fn hold(world: &mut World, held: Actions, frames: usize) -> Vec<FlowEvent> {
    play(world, held, frames, false)
}

fn next_event(world: &mut World, held: Actions) -> Vec<FlowEvent> {
    play(world, held, 2000, true)
}

#[test]
fn picks_up_reaches_a_checkpoint_and_respawns_there_after_a_hazard() {
    let mut w = world(TRACK);
    let right: Actions = Action::Right.into();
    assert_eq!(w.flow.lives, START_LIVES);

    assert_eq!(next_event(&mut w, right), [FlowEvent::Collect(0)]);
    assert!(!w.things[0].active);
    assert_eq!(next_event(&mut w, right), [FlowEvent::Collect(1)]);
    assert_eq!(next_event(&mut w, right), [FlowEvent::Collect(2)]);
    assert_eq!((w.flow.gems, w.flow.relics), (2, 1));
    assert_eq!(next_event(&mut w, right), [FlowEvent::Checkpoint(3)]);
    assert!(w.things[3].active, "checkpoints stay");

    assert_eq!(next_event(&mut w, right), [FlowEvent::Die]);
    assert_eq!(w.flow.lives, START_LIVES - 1);
    assert_eq!(w.flow.outcome, Outcome::Dying(DYING_FRAMES));
    let at = w.player.pos;
    assert_eq!(w.player.ground, Some(1), "stepped up onto the strip");

    // Out of play until it comes back, whatever is pressed.
    let events = hold(&mut w, right, DYING_FRAMES as usize - 1);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(w.player.pos, at);
    assert_eq!(hold(&mut w, right, 1), [FlowEvent::Respawn]);
    assert_eq!(w.flow.outcome, Outcome::Playing);
    assert_eq!(w.player.pos, Vec2::ints(300, 400));
    assert!(w.player.grounded());

    // Pickups stay picked up, and a reached checkpoint doesn't fire again.
    assert_eq!((w.flow.gems, w.flow.relics), (2, 1));
    assert_eq!(hold(&mut w, Action::Left.into(), 30), []);
}

#[test]
fn falling_out_of_the_level_costs_a_life_and_the_last_one_ends_the_game() {
    // No hazard, and the goal out of reach, so walking right falls into
    // the pit.
    let src = TRACK
        .replace("surface = \"hazard\"", "surface = \"normal\"")
        .replace("at = [900, 400]", "at = [1300, 100]");
    let mut w = world(&src);
    let right: Actions = Action::Right.into();
    for lives in (0..START_LIVES).rev() {
        // Pickups and the checkpoint come first, the first time round.
        while w.flow.outcome == Outcome::Playing {
            next_event(&mut w, right);
        }
        assert_eq!(w.flow.events, [FlowEvent::Die]);
        assert!(w.player.pos.y - clamber_sim::HEIGHT > Vec2::ints(0, 600).y);
        assert_eq!(w.flow.lives, lives);
        let last = if lives == 0 {
            FlowEvent::GameOver
        } else {
            FlowEvent::Respawn
        };
        assert_eq!(hold(&mut w, Actions::NONE, DYING_FRAMES as usize), [last]);
    }
    assert_eq!(w.flow.outcome, Outcome::GameOver);
    assert_eq!(hold(&mut w, right, 1), []);
    let state = w.state_bytes();
    assert_eq!(hold(&mut w, right, 10), []);
    assert_eq!(w.state_bytes(), state, "a finished world doesn't change");
}

#[test]
fn below_the_level_or_inside_its_ground_costs_a_life_and_respawns_at_the_checkpoint() {
    let src = TRACK.replace("surface = \"hazard\"", "surface = \"normal\"");
    // Put below the level; inside the ground, which reaches the bottom
    // of the level, so its bottom edge is level with the bottom; and
    // inside the ground near its bottom edge.
    for at in [
        Vec2::ints(500, 700),
        Vec2::ints(500, 450),
        Vec2::ints(500, 599),
    ] {
        let mut w = world(&src);
        let right: Actions = Action::Right.into();
        while w.flow.checkpoints.is_empty() {
            next_event(&mut w, right);
        }
        w.player = Player::new(&w.level, at);
        assert!(!w.player.grounded(), "{at:?}: nothing to stand on in there");
        // Whatever is pressed, it falls out of the level.
        let events = next_event(&mut w, right);
        assert_eq!(events, [FlowEvent::Die], "{at:?}");
        assert_eq!(w.flow.lives, START_LIVES - 1);
        assert_eq!(
            hold(&mut w, right, DYING_FRAMES as usize),
            [FlowEvent::Respawn]
        );
        assert_eq!(w.player.pos, Vec2::ints(300, 400), "{at:?}");
        assert!(w.player.grounded());
    }
}

#[test]
fn reaching_the_goal_completes_the_level() {
    // Without the hazard strip, walking right reaches the goal.
    let src = TRACK.replace("surface = \"hazard\"", "surface = \"normal\"");
    let mut w = world(&src);
    let events = hold(&mut w, Action::Right.into(), 2000);
    assert_eq!(events.last(), Some(&FlowEvent::Complete));
    assert_eq!(w.flow.outcome, Outcome::Complete);
    assert!(w.frame < 2000);
    // the goal's box reaches 16 px either side of it
    assert!(w.player.pos.x > Vec2::ints(900 - 16 - 8, 0).x);
}

#[test]
fn flow_is_part_of_the_state() {
    let mut a = world(TRACK);
    let b = world(TRACK);
    assert_eq!(a.state_bytes(), b.state_bytes());
    a.flow.gems += 1;
    assert_ne!(a.state_bytes(), b.state_bytes());
}
