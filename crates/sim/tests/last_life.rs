//! The lives are the extra ones: 0 is still playable, and the world ends
//! only when a life is lost with none left.

use std::sync::Arc;

use clamber_input::{Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{FlowEvent, InputFrame, Outcome, World, generic_feel, step};

const PIT: &str = r#"
    format = 1
    name = "pit"
    size = [4096, 400]
    start = [400, 300]

    [[collision]]
    shape = "rect"
    rect = [0, 300, 1000, 100]
"#;

/// A world with `lives` extra lives whose player falls into the pit.
fn falling(lives: u8) -> World {
    let feel = generic_feel("classic-1").unwrap();
    let level = Arc::new(Level::from_toml_str(PIT, "pit").unwrap());
    let mut world = World::new(level, Arc::new(feel));
    world.flow.lives = lives;
    world.player.pos = Vec2::ints(1100, 100);
    world
}

/// Steps until the outcome stops being a lost life, returning the
/// events of all the steps.
fn until_settled(world: &mut World) -> Vec<FlowEvent> {
    let input = InputFrame::single(Frame::after(Actions::NONE, Actions::NONE));
    let mut events = Vec::new();
    for _ in 0..2000 {
        step(world, &input);
        events.extend(world.flow.events.iter().copied());
        if matches!(world.flow.outcome, Outcome::GameOver) || events.contains(&FlowEvent::Respawn) {
            return events;
        }
    }
    panic!("never settled: {:?}", world.flow.outcome);
}

#[test]
fn losing_a_life_with_one_left_keeps_playing() {
    let mut world = falling(1);
    let events = until_settled(&mut world);
    assert!(events.contains(&FlowEvent::Die));
    assert!(!events.contains(&FlowEvent::GameOver));
    assert_eq!(world.flow.outcome, Outcome::Playing);
    assert_eq!(world.flow.lives, 0);
    assert!(!world.flow.final_loss);
}

#[test]
fn losing_a_life_with_none_left_is_game_over() {
    let mut world = falling(0);
    let events = until_settled(&mut world);
    assert!(events.contains(&FlowEvent::Die));
    assert!(events.contains(&FlowEvent::GameOver));
    assert_eq!(world.flow.outcome, Outcome::GameOver);
    assert_eq!(world.flow.lives, 0, "never wraps");
}

#[test]
fn the_display_shows_no_drop_for_a_loss_on_none() {
    for (lives, before, after) in [(1, 1, 0), (0, 0, 0)] {
        let mut world = falling(lives);
        let input = InputFrame::single(Frame::after(Actions::NONE, Actions::NONE));
        let lost = loop {
            step(&mut world, &input);
            if let Some(lost) = world.flow.lost {
                break lost;
            }
            assert!(world.frame < 2000, "no life lost");
        };
        assert_eq!((lost.before, lost.after), (before, after));
    }
}
