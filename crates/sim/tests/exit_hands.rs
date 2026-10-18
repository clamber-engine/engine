//! An object in the lead's hands when an exit takes over on `classic-1`
//! (spec/behavior/exits.md, "The walk-out", "An object in the lead's
//! hands"), and the reach of a lead standing still carrying one.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{Act, Facing, InputFrame, Outcome, Phase, World, generic_feel, step};

const L: Action = Action::Left;
const R: Action = Action::Right;
const A: Action = Action::Action;

/// The exit's place, on the floor.
const XE: i32 = 100;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat ground at y = 400, the player at x = 200, a crate 20 px to its
/// right, holding the missing character if `holder`, and an exit at
/// [`XE`] on the ground.
fn world(holder: bool) -> World {
    let src = format!(
        r#"
        format = 1
        name = "hands"
        size = [3000, 800]
        start = [200, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 3000, 200]

        [[entities]]
        type = "crate"
        at = [220, 400]
        params = {{ holds_character = {holder} }}

        [[entities]]
        type = "exit"
        at = [{XE}, 400]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "hands").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// Picks up the crate and carries it with action held, standing
    /// facing right.
    fn carrying(holder: bool) -> Play {
        let mut play = Play {
            world: world(holder),
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        play.hold(&[A], 21);
        assert_eq!(play.world.player.hands.map(|h| h.act), Some(Act::Carry));
        assert_eq!(play.world.player.facing, Facing::Right);
        assert!(play.world.leaving.is_none());
        play
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    fn hold(&mut self, keys: &[Action], frames: usize) {
        for _ in 0..frames {
            self.step(keys);
        }
    }

    /// Holds `keys` until the exit takes over the lead, on `T`.
    fn until_taken(&mut self, keys: &[Action]) {
        for _ in 0..300 {
            self.step(keys);
            if self.world.leaving.is_some() {
                return;
            }
        }
        panic!("never taken over");
    }

    /// Puts the standing lead at `x` facing `facing`; the object in its
    /// hands follows on the next step.
    fn put(&mut self, x: f64, facing: Facing) {
        self.world.player.pos.x = f(x);
        self.world.player.facing = facing;
    }

    fn object(&self) -> &clamber_sim::Thing {
        &self.world.things[0]
    }
}

fn vy(phase: Phase) -> Fixed {
    match phase {
        Phase::LetGo { vy, .. } => vy,
        other => panic!("{other:?}"),
    }
}

/// From the end of `T`, with the object still in the hands: checks it's
/// let go on `T` + 1 where it was held, `vx` 1 in the way the lead
/// faced (`way`, 1 right, -1 left), then follows it frame by frame
/// against the rules until it's removed. Returns the frame after `T` it's
/// removed on.
fn falls_away(play: &mut Play, way: f64) -> u32 {
    assert!(play.world.player.hands.is_some(), "held at the end of T");
    let held_at = play.object().pos;
    play.step(&[A, R]);
    let t = play.object();
    assert!(t.active);
    assert_eq!(t.pos, held_at, "T + 1: where it was held");
    assert_eq!(t.vx, f(way));
    assert_eq!(vy(t.phase), f(-4.0));
    assert!(play.world.player.hands.is_none());
    let mut v = f(-4.0);
    for k in 2..200u32 {
        let Vec2 { mut x, mut y } = play.object().pos;
        let top = play.world.view.unwrap().y();
        // Input makes no difference.
        let keys: &[Action] = if k % 2 == 0 { &[A] } else { &[L] };
        play.step(keys);
        v = (v + f(0.4375)).min(f(8.0));
        x += f(way);
        y += v;
        let gone = x < Fixed::ZERO || y.floor() - top >= 256;
        let t = play.object();
        if gone {
            assert!(!t.active, "T + {k}: removed");
            return k;
        }
        assert!(t.active, "T + {k}");
        assert_eq!(t.pos, Vec2::new(x, y), "T + {k}");
        assert_eq!(t.vx, f(way), "T + {k}");
        assert_eq!(vy(t.phase), v, "T + {k}");
    }
    panic!("never removed");
}

/// Walking left into the exit with action held: let go on `T` + 1, it
/// moves left from `T` + 2, through the floor, and is removed below the
/// picture before the area ends, which it does as without an object.
#[test]
fn carried_in_it_is_let_go_on_t_plus_1_and_falls_away() {
    let mut play = Play::carrying(false);
    play.until_taken(&[L, A]);
    assert_eq!(play.world.player.facing, Facing::Left);
    let removed = falls_away(&mut play, -1.0);
    assert!((30..45).contains(&removed), "{removed}");
    assert!(!play.world.flow.outcome.is_over());
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        assert!(play.world.frame < 400);
    }
    assert_eq!(play.world.flow.outcome, Outcome::Exited(1));
}

/// A lead standing still facing right lets it go to the right.
#[test]
fn standing_facing_right_lets_it_go_to_the_right() {
    let mut play = Play::carrying(false);
    play.put(f64::from(XE) + 32.5, Facing::Right);
    play.until_taken(&[A]);
    falls_away(&mut play, 1.0);
}

/// A holder is let go the same way and keeps the missing character.
#[test]
fn a_holder_is_let_go_the_same_way() {
    let mut play = Play::carrying(true);
    play.until_taken(&[L, A]);
    falls_away(&mut play, -1.0);
    assert!(play.world.join.is_none(), "it didn't break");
}

/// A throw begun before `T` that hasn't left the hands never happens.
#[test]
fn a_throw_still_under_way_never_happens() {
    let mut play = Play::carrying(false);
    play.step(&[]);
    assert_eq!(play.world.player.hands.map(|h| h.act), Some(Act::Throw));
    play.put(f64::from(XE) + 20.5, Facing::Right);
    play.until_taken(&[]);
    assert_eq!(play.object().phase, Phase::Held);
    falls_away(&mut play, 1.0);
}

/// Standing still carrying, the exit takes over a lead facing left with
/// its `x` rounded down 30 right of the exit but not 31 or 32, and one
/// facing right at 32 but not 33 or 34.
#[test]
fn reach_standing_still_carrying() {
    for (facing, fraction, reach) in [(Facing::Left, 0.68, 30), (Facing::Right, 0.17, 32)] {
        for ahead in reach - 3..=reach + 2 {
            let mut play = Play::carrying(false);
            play.put(f64::from(XE + ahead) + fraction, facing);
            play.hold(&[A], 3);
            assert_eq!(
                play.world.leaving.is_some(),
                ahead <= reach,
                "{facing:?} at {ahead}"
            );
        }
    }
}
