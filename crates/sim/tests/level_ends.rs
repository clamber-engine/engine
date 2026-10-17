//! The level's left and right ends in Classic 1
//! (spec/behavior/level-ends.md): the picture's left edge kept between
//! the limits, and the player stopped inside them.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Fixed, InputFrame, View, World, generic_feel, step};

const R: Action = Action::Right;
const L: Action = Action::Left;
const J: Action = Action::Jump;
const A: Action = Action::Action;
const D: Action = Action::Down;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat ground at y = 400 across a level 6000 wide, whose camera limits
/// across give the left limit `a` and the right limit `b` for a picture
/// 256 wide, with the player standing at `start` and `more` added.
fn world(a: i32, b: i32, start: i32, more: &str) -> World {
    let w = b + 256 - a;
    let src = format!(
        r#"
        format = 1
        size = [6000, 800]
        start = [{start}, 400]
        camera = {{ limits = [{a}, 0, {w}, 800] }}

        [[collision]]
        shape = "rect"
        rect = [0, 400, 6000, 200]
        {more}
        "#
    );
    let level = Level::from_toml_str(&src, "ends").unwrap();
    let world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    assert!(world.player.grounded());
    world
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(world: World) -> Play {
        Play {
            world,
            held: Actions::NONE,
        }
    }

    /// One frame holding `keys`: the player's `x` at its start, and its
    /// `x` and `vx` at its end.
    fn step(&mut self, keys: &[Action]) -> (Fixed, Fixed, Fixed) {
        let x0 = self.world.player.pos.x;
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        let p = &self.world.player;
        (x0, p.pos.x, p.vel.x)
    }
}

#[test]
fn the_picture_is_kept_between_the_limits() {
    // The example: A = 64 and B = 4800.
    let world = world(64, 4800, 1000, "");
    let view = world.view.unwrap();
    assert_eq!(view.ends, Some((64, 4800)));
    for (v, e) in [(4850.25, 4800), (1000.75, 1000), (20.5, 64)] {
        let view = View { left: f(v), ..view };
        assert_eq!(view.x(), e, "V = {v}");
    }
}

/// Plays `frames` frames holding `keys`, checking the rule on each, and
/// returns how many frames were stopped.
fn stopped_frames(play: &mut Play, keys: &[Action], frames: usize, a: i64, b: i64) -> usize {
    let mut stopped = 0;
    for frame in 0..frames {
        let (x0, x, vx) = play.step(keys);
        let at_right = x0.floor() >= b + 238;
        let at_left = x0.floor() < a + 18;
        if (at_right && vx >= Fixed::ZERO && x >= x0) || (at_left && vx <= Fixed::ZERO && x <= x0) {
            assert_eq!((x, vx), (x0, Fixed::ZERO), "frame {frame}");
            stopped += 1;
        }
    }
    stopped
}

#[test]
fn the_player_is_stopped_at_the_right_end() {
    let mut play = Play::new(world(64, 4800, 4900, ""));
    assert!(stopped_frames(&mut play, &[R, A], 150, 64, 4800) > 30);
    let x = play.world.player.pos.x;
    // A move that starts before the limit can end past it.
    assert!(x.floor() >= 5038 && x < f(5038.0) + f(6.0));
    // Moves away from the end aren't stopped.
    let (x0, x, _) = play.step(&[L]);
    assert!(x < x0);
}

#[test]
fn the_player_is_stopped_at_the_left_end() {
    let mut play = Play::new(world(64, 4800, 160, ""));
    assert!(stopped_frames(&mut play, &[L], 150, 64, 4800) > 30);
    let x = play.world.player.pos.x;
    assert!(x.floor() < 82 && x > f(70.0));
    let (x0, x, _) = play.step(&[R]);
    assert!(x > x0);
}

#[test]
fn jumps_rolls_and_down_are_stopped_too() {
    let mut play = Play::new(world(64, 4800, 4990, ""));
    stopped_frames(&mut play, &[R], 60, 64, 4800);
    let mut stopped = 0;
    for keys in [&[R, J][..], &[R], &[R, D], &[R, A], &[R]] {
        stopped += stopped_frames(&mut play, keys, 30, 64, 4800);
    }
    assert!(stopped > 100);
    assert!(play.world.player.pos.x < f(5046.0));
}

#[test]
fn placed_things_appear_by_the_picture() {
    // A copy of the first check: with B = 300, the player is stopped at
    // 538 and the view goes on past 300 while the picture stays there. A
    // walker 329 px right of the picture never appears.
    let crawler = r#"
        [[entities]]
        type = "crawler"
        at = [629, 400]
        params = { speed = 1, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, appear_wait = 1, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }
        "#;
    let mut play = Play::new(world(0, 300, 200, crawler));
    let mut past = false;
    for _ in 0..300 {
        play.step(&[R]);
        let view = play.world.view.unwrap();
        assert!(view.x() <= 300);
        past |= view.left.floor() > 300 + 50;
        assert!(!play.world.things[0].active, "the walker appeared");
    }
    assert!(past, "the view went past the limit");
}
