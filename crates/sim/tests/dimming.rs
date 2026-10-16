//! The dimming near a level's end in Classic 1
//! (spec/behavior/dimming.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{InputFrame, World, generic_feel, step};

const R: Action = Action::Right;
const L: Action = Action::Left;

/// Where the dim entity is.
const DIM_X: i64 = 900;

/// Flat ground from x = 0 to 3000 with the lead standing at x = 100 and a
/// dim entity at `DIM_X`, on `preset`.
fn world(preset: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "dim"
        size = [3000, 700]
        start = [100, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]

        [[entities]]
        type = "dim"
        at = [{DIM_X}, 300]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "dim").unwrap());
    World::new(level, Arc::new(generic_feel(preset).unwrap()))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// Holds Right until the dimming starts, and returns T, the frame it
    /// started on, checking it's the first frame that starts with the
    /// view's left edge at most 288 px left of the entity.
    fn until_started(&mut self) -> u32 {
        while self.world.dimming.is_none() {
            let left = self.world.view.unwrap().x();
            let frame = self.world.frame;
            self.step(&[R]);
            let started = self.world.dimming.is_some();
            assert_eq!(started, DIM_X - left <= 288, "frame {frame}, view {left}");
            assert!(frame < 1000);
        }
        let t = self.world.dimming.unwrap().started;
        assert_eq!(t, self.world.frame - 1);
        t
    }
}

fn play(preset: &str) -> Play {
    Play {
        world: world(preset),
        held: Actions::NONE,
    }
}

/// Holds `keys` until 400 frames after T, and returns the step after
/// each frame from T on.
fn steps_from(play: &mut Play, t: u32, keys: impl Fn(u32) -> &'static [Action]) -> Vec<u32> {
    let mut steps = vec![play.world.dim_step().0];
    while play.world.frame <= t + 400 {
        let keys = keys(play.world.frame);
        play.step(keys);
        steps.push(play.world.dim_step().0);
    }
    steps
}

/// Checks that the steps after each frame from T on are as the spec says:
/// none up to T + 3, step 1 on one of T + 4 to T + 11, and one more
/// every 8 frames up to 32.
fn check(steps: &[u32], t: u32) {
    let first = steps.iter().position(|&s| s == 1).unwrap();
    assert!((4..=11).contains(&first), "step 1 on T + {first}");
    // The engine places the cycle on multiples of 8.
    assert_eq!((t + first as u32) % 8, 0);
    for (j, &s) in steps.iter().enumerate() {
        let expected = if j < first {
            0
        } else {
            ((j - first) / 8 + 1).min(32) as u32
        };
        assert_eq!(s, expected, "T + {j}");
    }
    assert_eq!(*steps.last().unwrap(), 32);
}

#[test]
fn the_steps_after_the_entity_comes_into_the_window() {
    let mut play = play("classic-1");
    assert_eq!(play.world.dim_step(), (0, 32));
    let t = play.until_started();
    let steps = steps_from(&mut play, t, |_| &[R]);
    check(&steps, t);
    assert_eq!(play.world.dim_step(), (32, 32));
}

#[test]
fn step_one_comes_4_to_11_frames_after_t_whatever_t_is() {
    // Waiting 0 to 7 frames before setting off puts T on every place in
    // the cycle.
    let mut firsts = Vec::new();
    for wait in 0..8 {
        let mut play = play("classic-1");
        for _ in 0..wait {
            play.step(&[]);
        }
        let t = play.until_started();
        let steps = steps_from(&mut play, t, |_| &[]);
        check(&steps, t);
        firsts.push(steps.iter().position(|&s| s == 1).unwrap());
    }
    firsts.sort();
    firsts.dedup();
    assert_eq!(firsts, (4..=11).collect::<Vec<_>>());
}

#[test]
fn turning_back_or_standing_still_doesnt_stop_it() {
    let mut play = play("classic-1");
    let t = play.until_started();
    // Back left until the entity is well out of the window, then still.
    let steps = steps_from(
        &mut play,
        t,
        move |frame| {
            if frame < t + 120 { &[L] } else { &[] }
        },
    );
    let left = play.world.view.unwrap().x();
    assert!(DIM_X - left > 288 + 31, "view at {left}");
    check(&steps, t);
}

#[test]
fn presets_without_dimming_never_start_it() {
    let mut play = play("default");
    for _ in 0..600 {
        play.step(&[R]);
    }
    assert_eq!(play.world.dimming, None);
    assert_eq!(play.world.dim_step(), (0, 0));
}
