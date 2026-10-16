//! Walkers appearing and being removed with the view, on `classic-1`
//! (spec/behavior/walker-view.md). The view is put where each example
//! has it at the start of each step.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Fixed, InputFrame, Phase, View, World, generic_feel, step};

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat ground at y = 400, the player standing at x = 300, and one walker
/// with its start at x = 1000, heading `facing`, that waits `wait` steps
/// when it appears.
fn world(facing: &str, wait: u8) -> World {
    let src = format!(
        r#"
        format = 1
        size = [4000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]

        [[entities]]
        type = "crawler"
        at = [1000, 400]
        params = {{ speed = 1, facing = "{facing}", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, appear_wait = {wait}, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }}
        "#
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One step with the view's left edge at `left` when it starts. Returns
/// the walker's `x` if it's in the level after the step.
fn step_at(world: &mut World, left: i32) -> Option<f64> {
    world.view = Some(View {
        left: Fixed::from_int(left),
        ..View::default()
    });
    step(world, &InputFrame::default());
    let walker = &world.things[0];
    walker.active.then(|| walker.pos.x.to_f64())
}

#[test]
fn a_walker_away_from_the_view_at_the_start_isnt_in_the_level() {
    let w = world("left", 0);
    assert!(!w.things[0].active);
}

#[test]
fn appears_at_its_start_when_its_start_comes_near_the_view() {
    // walker-view.md's first example.
    let mut w = world("left", 0);
    assert_eq!(step_at(&mut w, 711), None);
    assert_eq!(step_at(&mut w, 712), Some(999.875));
    assert_eq!(step_at(&mut w, 714), Some(999.640625));
}

#[test]
fn a_walker_that_waits_takes_its_first_step_a_frame_later() {
    let mut w = world("left", 1);
    assert_eq!(step_at(&mut w, 712), Some(1000.0));
    assert_eq!(step_at(&mut w, 712), Some(999.875));
}

#[test]
fn is_removed_once_its_start_is_away_and_it_is_far_and_comes_back_afresh() {
    // walker-view.md's second example: the walker has walked left to
    // 880.5 at full speed.
    let mut w = world("left", 0);
    step_at(&mut w, 712);
    w.things[0].pos.x = fx(880.5);
    w.things[0].vx = fx(-1.0);
    assert_eq!(step_at(&mut w, 1031), Some(879.5), "start near: stays");
    assert_eq!(step_at(&mut w, 1032), None, "both hold: removed");
    assert_eq!(step_at(&mut w, 1100), None);
    // Back at its start, at rest, taking its first step.
    assert_eq!(step_at(&mut w, 1031), Some(999.875));
}

#[test]
fn is_removed_far_ahead_of_the_view() {
    // walker-view.md's third example: heading right, ahead of a view
    // standing at 1500.
    let mut w = world("right", 0);
    step_at(&mut w, 712);
    w.things[0].pos.x = fx(1883.0);
    w.things[0].vx = fx(1.0);
    assert_eq!(step_at(&mut w, 1500), Some(1884.0));
    assert_eq!(step_at(&mut w, 1500), None);
}

#[test]
fn a_defeated_walker_never_comes_back() {
    let mut w = world("left", 0);
    step_at(&mut w, 712);
    w.things[0].phase = Phase::Falling {
        vy: Fixed::ZERO,
        hold: 0,
    };
    w.things[0].pos.x = fx(700.0);
    assert_eq!(step_at(&mut w, 1032), None);
    for left in [1031, 900, 712] {
        assert_eq!(step_at(&mut w, left), None);
    }
}

#[test]
fn the_view_follows_the_player_on_its_own() {
    let mut w = world("left", 0);
    let start = w.view.unwrap().left;
    assert_eq!(start, Fixed::from_int(300 - 71));
    let mut held = Actions::NONE;
    for _ in 0..120 {
        let now = Action::Right.into();
        step(&mut w, &InputFrame::single(Frame::after(held, now)));
        held = now;
    }
    assert!(w.view.unwrap().left > start);
}
