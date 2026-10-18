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
        glide: None,
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

/// Like [`world`], with a walker heading left at each of `starts`, all
/// in group 1.
fn group(starts: &[i32]) -> World {
    let mut src = String::from(
        r#"
        format = 1
        size = [4000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]
        "#,
    );
    for x in starts {
        src += &format!(
            r#"
            [[entities]]
            type = "crawler"
            at = [{x}, 400]
            params = {{ speed = 1, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, appear_wait = 0, group = 1, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }}
            "#
        );
    }
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One step with the view's left edge at `left` when it starts. Returns
/// each walker's `x` if it's in the level after the step.
fn group_step_at(world: &mut World, left: i32) -> Vec<Option<f64>> {
    world.view = Some(View {
        left: Fixed::from_int(left),
        ..View::default()
    });
    step(world, &InputFrame::default());
    world
        .things
        .iter()
        .map(|w| w.active.then(|| w.pos.x.to_f64()))
        .collect()
}

#[test]
fn a_group_appears_together_when_its_nearest_start_comes_near() {
    // walker-view.md, "Groups": three in a row, heading the same way,
    // appear on the frame the nearest one's start enters the window.
    let mut w = group(&[1000, 1040, 1080]);
    assert_eq!(group_step_at(&mut w, 711), [None, None, None]);
    assert_eq!(
        group_step_at(&mut w, 712),
        [Some(999.875), Some(1039.875), Some(1079.875)]
    );

    // Coming in from the right, the nearest start is the last one.
    let mut w = group(&[1000, 1040, 1080]);
    assert_eq!(group_step_at(&mut w, 1112), [None, None, None]);
    assert_eq!(
        group_step_at(&mut w, 1111),
        [Some(999.875), Some(1039.875), Some(1079.875)]
    );
}

#[test]
fn a_group_is_removed_together_once_all_of_it_could_be() {
    // The engine's own rule: walker-view.md doesn't say how a group is
    // removed.
    let mut w = group(&[1000, 1040, 1080]);
    group_step_at(&mut w, 712);
    for (thing, x) in w.things.iter_mut().zip([900.5, 940.5, 1100.5]) {
        thing.pos.x = fx(x);
        thing.vx = fx(-1.0);
    }
    // All starts are out of the window, and the first two are far, but
    // the last isn't: they all stay.
    assert_eq!(
        group_step_at(&mut w, 1200),
        [Some(899.5), Some(939.5), Some(1099.5)]
    );
    w.things[2].pos.x = fx(1072.5);
    assert_eq!(group_step_at(&mut w, 1200), [None, None, None]);
    // They come back together, afresh.
    assert_eq!(
        group_step_at(&mut w, 1111),
        [Some(999.875), Some(1039.875), Some(1079.875)]
    );
}

#[test]
fn a_defeated_member_stays_away_when_its_group_comes_back() {
    let mut w = group(&[1000, 1040, 1080]);
    group_step_at(&mut w, 712);
    w.things[1].phase = Phase::Falling {
        vy: Fixed::ZERO,
        hold: 0,
        glide: None,
    };
    w.things[1].active = false;
    for (thing, x) in w.things.iter_mut().zip([900.5, 0.0, 900.5]) {
        thing.pos.x = fx(x);
    }
    assert_eq!(group_step_at(&mut w, 1200), [None, None, None]);
    assert_eq!(
        group_step_at(&mut w, 712),
        [Some(999.875), None, Some(1079.875)]
    );
}

#[test]
fn a_group_near_the_view_at_the_start_is_all_there() {
    // The view starts at 229, so only the first start is in the window.
    let w = group(&[500, 600, 700]);
    assert!(w.things.iter().all(|t| t.active));
    let w = group(&[600, 700]);
    assert!(w.things.iter().all(|t| !t.active));
}

/// One step with the view's left edge at `left` and its top at `top`
/// when it starts. Returns whether the walker is in the level after it.
fn step_in(world: &mut World, left: i32, top: i32) -> bool {
    world.view = Some(View {
        left: Fixed::from_int(left),
        top: Fixed::from_int(top),
        ..View::default()
    });
    step(world, &InputFrame::default());
    world.things[0].active
}

/// The walker in the level, defeated and falling in `phase`, at `x`
/// after the next step's move, with nothing else moving it across.
fn falling(phase: Phase, x: f64) -> World {
    let mut w = world("left", 0);
    step_at(&mut w, 712);
    let walker = &mut w.things[0];
    walker.phase = phase;
    walker.vx = Fixed::ZERO;
    walker.pos.x = fx(x);
    w
}

const FALLING: Phase = Phase::Falling {
    vy: Fixed::ZERO,
    hold: 0,
    glide: None,
};

/// walker-view.md, "Defeated walkers falling out": removed once its `x`
/// rounded down, after the frame's move, is more than 31 px left of the
/// picture or more than 288 px right of its left edge, by the view at
/// the start of the frame.
#[test]
fn a_defeated_walker_is_removed_once_it_leaves_the_window_across() {
    for (x, stays) in [
        (1000.0 - 31.0, true),
        (1000.0 - 31.5, false),
        (1000.0 - 32.0, false),
        (1000.0 + 288.75, true),
        (1000.0 + 289.0, false),
    ] {
        let mut w = falling(FALLING, x);
        assert_eq!(step_in(&mut w, 1000, 300), stays, "X - V of {x} - 1000");
    }
}

/// The same below the picture: its `y` with its fraction more than 256
/// px below the picture's top, after the frame's move.
#[test]
fn a_defeated_walker_is_removed_once_it_leaves_the_window_below() {
    for (below, stays) in [(256.0, true), (256.0625, false)] {
        let mut w = falling(FALLING, 1100.0);
        // Where it would be after the move from 400.
        let mut probe = w.clone();
        step_in(&mut probe, 1000, 0);
        let moved = probe.things[0].pos.y - w.things[0].pos.y;
        w.things[0].pos.y = fx(300.0 + below) - moved;
        assert_eq!(step_in(&mut w, 1000, 300), stays, "{below} below");
        if stays {
            assert_eq!(w.things[0].pos.y, fx(556.0));
        }
    }
}

/// Where its start is doesn't matter: a defeated walker in the window
/// stays even with its start far outside it, and one outside it goes
/// even with its start inside.
#[test]
fn where_a_defeated_walkers_start_is_doesnt_matter() {
    // Start 1000: S - V of -300 and X - V of 0, then 10 and 310.
    let mut w = falling(FALLING, 1300.0);
    assert!(step_in(&mut w, 1300, 200), "start out, walker in: stays");
    let mut w = falling(FALLING, 1300.0);
    assert!(!step_in(&mut w, 990, 200), "start in, walker out: removed");
}

/// On the frames before its first falling one it stays wherever it is:
/// while it holds still after a bounce or a roll, and on the frame after
/// a thrown object hit it, when it moves along the floor. It's removed
/// on the first one checked if it's still outside then.
#[test]
fn a_defeated_walker_isnt_removed_before_its_first_falling_frame() {
    let holding = Phase::Falling {
        vy: fx(-6.0),
        hold: 1,
        glide: None,
    };
    let hit = Phase::Knocked { vy: None };
    for phase in [holding, hit] {
        for x in [1000.0 - 200.0, 1000.0 + 300.0] {
            let mut w = falling(phase, x);
            assert!(step_in(&mut w, 1000, 200), "{phase:?} at {x}: stays");
            assert!(!step_in(&mut w, 1000, 200), "{phase:?} at {x}: removed");
        }
    }
}

/// A walker hit by a thrown object, from the second frame after the hit
/// on, leaves by the same window.
#[test]
fn a_walker_hit_by_a_thrown_object_leaves_by_the_window() {
    for (x, stays) in [(1000.0 - 31.0, true), (1000.0 - 32.0, false)] {
        let mut w = falling(Phase::Knocked { vy: Some(fx(-6.0)) }, x);
        // Knocked back, it eases toward its glide; keep it in place.
        w.things[0].pos.x = fx(x);
        let mut probe = w.clone();
        step_in(&mut probe, 1000, 200);
        let moved = probe.things[0].pos.x - w.things[0].pos.x;
        w.things[0].pos.x = fx(x) - moved;
        assert_eq!(step_in(&mut w, 1000, 200), stays, "X - V of {x} - 1000");
    }
}

/// The `x` of each thing [`World::would_appear`] gives for a view
/// `width` px wide at `left`, checking that nothing changes.
fn would_appear(world: &World, left: i64, width: i64) -> Vec<f64> {
    let before = world.state_bytes();
    let things = world.would_appear(left, width);
    assert_eq!(world.state_bytes(), before, "nothing changes");
    things.iter().map(|t| t.pos.x.to_f64()).collect()
}

#[test]
fn says_which_walkers_a_wider_view_would_bring_in_at_their_start() {
    let mut w = world("left", 0);
    step_at(&mut w, 300);
    assert_eq!(would_appear(&w, 711, 256), [] as [f64; 0]);
    assert_eq!(would_appear(&w, 712, 256), [1000.0]);
    // A view 400 px wide reaches it from further left.
    assert_eq!(would_appear(&w, 568, 400), [1000.0]);
    assert_eq!(would_appear(&w, 567, 400), [] as [f64; 0]);
    let thing = &w.would_appear(568, 400)[0];
    assert!(thing.active && thing.vx == Fixed::ZERO, "as it appears");

    // One in the level isn't given, nor one that's defeated.
    step_at(&mut w, 712);
    assert_eq!(would_appear(&w, 712, 256), [] as [f64; 0]);
    w.things[0].active = false;
    w.things[0].phase = Phase::Falling {
        vy: Fixed::ZERO,
        hold: 0,
        glide: None,
    };
    assert_eq!(would_appear(&w, 712, 256), [] as [f64; 0]);
}

#[test]
fn a_wider_view_would_bring_in_a_group_whole() {
    let mut w = group(&[1000, 1040, 1080]);
    group_step_at(&mut w, 300);
    assert_eq!(would_appear(&w, 568, 400), [1000.0, 1040.0, 1080.0]);
}

/// `world`, its feel played in a window 400 px wide (wide activation).
fn widened(world: World) -> World {
    let mut feel = (*world.feel).clone();
    feel.window = Some(400);
    World::new(world.level.clone(), Arc::new(feel))
}

#[test]
fn in_a_wider_window_walkers_appear_and_are_removed_by_it() {
    // The window is centred on the picture, 72 px further left: it's
    // near the walker from a picture at 640, where one 256 px wide
    // would be from 712.
    let mut w = widened(world("left", 0));
    assert_eq!(step_at(&mut w, 639), None);
    assert!(step_at(&mut w, 640).is_some());
    // walker-view.md's second example, with the window's left edge in
    // place of the view's: removed with the window at 1032, so the
    // picture 72 px right of it.
    w.things[0].pos.x = fx(880.5);
    w.things[0].vx = fx(-1.0);
    assert_eq!(step_at(&mut w, 1032), Some(879.5), "start near: stays");
    assert_eq!(step_at(&mut w, 1103), Some(878.5), "start near: stays");
    assert_eq!(step_at(&mut w, 1104), None, "both hold: removed");
}

#[test]
fn a_wider_window_is_kept_inside_the_level() {
    let mut w = widened(world("left", 0));
    let mut at = |left| {
        w.view = Some(View {
            left: Fixed::from_int(left),
            ..View::default()
        });
        w.window()
    };
    assert_eq!(at(2000), Some((1928, 400)), "centred on the picture");
    assert_eq!(at(0), Some((0, 400)), "at the left end");
    assert_eq!(at(3800), Some((3600, 400)), "at the right end");
    assert_eq!(world("left", 0).window(), None, "none in the picture");
}
