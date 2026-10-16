//! Falling out of the level in Classic 1 (spec/behavior/falling-out.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{FlowEvent, InputFrame, Outcome, START_LIVES, World, generic_feel, step};

/// Ground at y = 300 with a gap from x = 1000 to 1200. The level's
/// bottom `Yb` is 400, as in the made-up example.
const GAP: &str = r#"
    format = 1
    name = "gap"
    size = [4096, 400]
    start = [400, 300]

    [[collision]]
    shape = "rect"
    rect = [0, 300, 1000, 100]

    [[collision]]
    shape = "rect"
    rect = [1200, 300, 2896, 100]
"#;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new() -> Play {
        let feel = generic_feel("classic-1").unwrap();
        let level = Arc::new(Level::from_toml_str(GAP, "gap").unwrap());
        let mut play = Play {
            world: World::new(level, Arc::new(feel)),
            held: Actions::NONE,
        };
        play.hold(&[], 10);
        play
    }

    /// The player falling at 8 px per frame in the gap, its feet at
    /// level y `y`.
    fn falling(y: f64) -> Play {
        let mut play = Play::new();
        play.world.player.pos = Vec2::ints(1100, 100);
        while play.world.player.vel.y < f(8.0) {
            play.step(&[]);
        }
        assert_eq!(play.world.flow.outcome, Outcome::Playing);
        play.world.player.pos.y = f(y);
        play
    }

    fn step(&mut self, keys: &[Action]) -> Vec<FlowEvent> {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        self.world.flow.events.clone()
    }

    fn hold(&mut self, keys: &[Action], frames: usize) {
        for _ in 0..frames {
            self.step(keys);
        }
    }
}

#[test]
fn falls_out_once_more_than_48_px_below_the_bottom() {
    // The made-up example: frames ending at 396.5 to 444.5 aren't out,
    // the one ending at 452.5 is Z.
    let mut play = Play::falling(388.5);
    for _ in 0..7 {
        assert!(play.step(&[]).is_empty());
        assert_eq!(play.world.flow.outcome, Outcome::Playing);
    }
    assert_eq!(play.world.player.pos.y, f(444.5));
    assert_eq!(play.step(&[]), [FlowEvent::Die]);
    assert_eq!(play.world.player.pos.y, f(452.5));
    assert_eq!(play.world.flow.lives, START_LIVES - 1);
}

#[test]
fn ending_exactly_48_px_below_is_not_enough() {
    let mut play = Play::falling(440.0);
    assert!(play.step(&[]).is_empty());
    assert_eq!(play.world.player.pos.y, f(448.0));
    assert_eq!(play.step(&[]), [FlowEvent::Die]);
    assert_eq!(play.world.player.pos.y, f(456.0));
}

#[test]
fn the_level_goes_on_96_frames_after_z_with_everything_still() {
    let mut play = Play::falling(444.5);
    assert_eq!(play.step(&[]), [FlowEvent::Die]);
    let at = play.world.player.pos;
    let view = play.world.view;
    // Z + 1 to Z + 96: input ignored, the player and the view still.
    for k in 1..=96 {
        let keys: &[Action] = if k % 2 == 0 {
            &[Action::Right, Action::Jump]
        } else {
            &[Action::Left, Action::Action]
        };
        assert!(play.step(keys).is_empty(), "Z + {k}");
        let p = &play.world.player;
        assert_eq!(p.pos, at, "Z + {k}");
        assert_eq!(p.vel, Vec2::default(), "Z + {k}");
        assert!(!p.grounded(), "Z + {k}");
        assert_eq!(play.world.view, view, "Z + {k}");
    }
    // The level is over; play comes back at the start.
    assert_eq!(play.step(&[]), [FlowEvent::Respawn]);
    assert_eq!(play.world.player.pos, Vec2::ints(400, 300));
}
