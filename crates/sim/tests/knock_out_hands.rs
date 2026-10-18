//! An object in the hands at a knock-out on `classic-1`
//! (spec/behavior/knock-out.md, "An object in the hands").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{Act, Facing, InputFrame, Phase, World, generic_feel, step};

const A: Action = Action::Action;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat ground at y = 400, the player at x = 200 and a crate 20 px to
/// its right, holding the missing character if `holder`.
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
    /// Picks up the crate and carries it with action held, standing.
    fn carrying(holder: bool) -> Play {
        let mut play = Play {
            world: world(holder),
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        play.hold(&[A], 21);
        assert_eq!(play.world.player.hands.map(|h| h.act), Some(Act::Carry));
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

    /// Knocks the player out on the next step, H, with `keys` held.
    fn knock_out(&mut self, keys: &[Action]) {
        self.world.player.hurt_ahead = Some(0);
        self.step(keys);
        assert!(self.world.player.knockout.is_some());
    }

    fn object(&self) -> (Vec2, Fixed, Phase, bool) {
        let t = &self.world.things[0];
        (t.pos, t.vx, t.phase, t.active)
    }
}

/// `x`, `y` and `vy` relative to where the object was held, `n` moves
/// after it starts moving, facing `way` (1 right, -1 left).
fn after_moves(n: u32, way: f64) -> (f64, f64, f64) {
    let (mut y, mut vy) = (0.0, -4.0);
    for _ in 0..n {
        vy = f64::min(vy + 0.4375, 8.0);
        y += vy;
    }
    (way * f64::from(n), y, vy)
}

fn vy(phase: Phase) -> Fixed {
    match phase {
        Phase::LetGo { vy, .. } => vy,
        other => panic!("{other:?}"),
    }
}

/// Follows a let-go object from H, which `first` frames later makes its
/// first move, until it's removed, checking each frame against the rules
/// and the knock-out.md table. Returns the frame after H it's removed on.
fn falls_away(play: &mut Play, held_at: Vec2, way: f64, first: u32) -> u32 {
    let (pos, vx, phase, active) = play.object();
    assert!(active);
    assert_eq!(pos, held_at, "H: where it was held");
    assert_eq!(vx, f(way));
    assert_eq!(vy(phase), f(-4.0));
    assert!(play.world.player.hands.is_none());
    for n in 1..200u32 {
        let top = play.world.view.unwrap().y();
        // Input makes no difference.
        play.step(&[A, Action::Right]);
        let moves = (n + 1).saturating_sub(first);
        let (x, y, v) = after_moves(moves, way);
        let want = Vec2::new(held_at.x + f(x), held_at.y + f(y));
        let (pos, _, phase, active) = play.object();
        let gone = want.x < Fixed::ZERO || (want.y.floor() - top >= 256);
        if moves > 0 && gone {
            assert!(!active, "H + {n}: removed");
            return n;
        }
        assert!(active, "H + {n}");
        assert_eq!(pos, want, "H + {n}");
        assert_eq!(vy(phase), f(v), "H + {n}");
    }
    panic!("never removed");
}

/// knock-out.md, "An object in the hands": a barrel held with action
/// held, facing right, is let go on H, held still to H + 10 like the
/// player and falls away from H + 11, as in the table mirrored.
#[test]
fn a_barrel_is_let_go_and_falls_away_from_h_plus_11() {
    let mut play = Play::carrying(false);
    let held_at = play.world.things[0].pos;
    play.knock_out(&[A]);
    assert_eq!(play.world.player.facing, Facing::Right);
    // The table: H + 11 is the first move, H + 19 the highest point.
    assert_eq!(after_moves(1, -1.0), (-1.0, -3.5625, -3.5625));
    assert_eq!(after_moves(9, -1.0), (-9.0, -16.3125, -0.0625));
    assert_eq!(after_moves(10, -1.0), (-10.0, -15.9375, 0.375));
    assert_eq!(after_moves(28, -1.0), (-28.0, 65.375, 8.0));
    let removed = falls_away(&mut play, held_at, 1.0, 11);
    assert!(removed > 30, "{removed}");
}

/// A holder, facing left, moves from H + 1, touches nothing and keeps
/// the missing character.
#[test]
fn a_holder_is_let_go_and_falls_away_from_h_plus_1() {
    let mut play = Play::carrying(true);
    play.world.player.facing = Facing::Left;
    let held_at = play.world.things[0].pos;
    play.knock_out(&[]);
    let removed = falls_away(&mut play, held_at, -1.0, 1);
    assert!(removed > 30, "{removed}");
    assert!(play.world.join.is_none(), "it didn't break");
}

/// One whose throw began on H − 1 is let go the same way, and the throw
/// never happens.
#[test]
fn a_throw_begun_the_frame_before_never_happens() {
    let mut play = Play::carrying(false);
    play.step(&[]);
    assert_eq!(play.world.player.hands.map(|h| h.act), Some(Act::Throw));
    let held_at = play.world.things[0].pos;
    play.knock_out(&[]);
    falls_away(&mut play, held_at, 1.0, 11);
}

/// It's removed on the first frame its `x` is less than 0 after the
/// move.
#[test]
fn removed_left_of_the_level() {
    let mut play = Play::carrying(false);
    play.world.player.facing = Facing::Left;
    play.world.things[0].pos.x = f(5.0);
    let held_at = play.world.things[0].pos;
    play.knock_out(&[]);
    // x 5 - 6 on the sixth move, H + 16.
    assert_eq!(falls_away(&mut play, held_at, -1.0, 11), 16);
}
