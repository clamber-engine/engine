//! Touching the thrower in Classic 1 (spec/behavior/thrower-contact.md):
//! when it hurts the player, when the player bounces off it, and what a
//! bounce does to it. Balls are removed as soon as they appear, as in
//! the spec's measurements.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{Event, Facing, InputFrame, Phase, State, World, generic_feel, step};

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// `v` rounded up.
fn up(v: Fixed) -> i64 {
    -(-v).floor()
}

/// Ground at y = 381 from x = 0 to 3000, the player at `start` on it,
/// and a thrower at (1000, `y`): with `y` 380 it sits on the ground as
/// on its ledge.
fn world(start: i32, y: i32) -> World {
    let src = format!(
        r#"
        format = 1
        name = "thrower contact"
        size = [3000, 600]
        start = [{start}, 381]

        [[collision]]
        shape = "rect"
        rect = [0, 381, 3000, 100]

        [[entities]]
        type = "thrower"
        at = [1000, {y}]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "thrower contact").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(start: i32, y: i32) -> Play {
        Play {
            world: world(start, y),
            held: Actions::NONE,
        }
    }

    /// One step, with the balls thrown on it removed; whether the player
    /// was hurt on it.
    fn step(&mut self, keys: &[Action]) -> bool {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        let placed = self.world.level.entities.len();
        for ball in &mut self.world.things[placed..] {
            ball.active = false;
        }
        self.world.events.contains(&Event::Hurt)
    }

    /// Settles the player on the ground, standing, with the thrower's
    /// clock far from its throws at the end of the last step.
    fn settle(&mut self) {
        for _ in 0..20 {
            self.step(&[]);
        }
        self.world.things[0].clock = 60;
    }

    /// Puts the player standing at `x`, facing `facing`.
    fn stand_at(&mut self, x: f64, facing: Facing) {
        self.world.player.pos.x = f(x);
        self.world.player.facing = facing;
    }
}

/// thrower-contact.md, "Being hurt": standing on the ledge only `across`
/// counts, upright's range: -26 to 33 facing right, -32 to 27 facing
/// left, one frame after it.
#[test]
fn on_the_ledge_the_upright_range_hurts_on_the_next_frame() {
    for (x, facing, hurts) in [
        (973.5, Facing::Right, false),
        (974.0, Facing::Right, true),
        (1033.75, Facing::Right, true),
        (1034.0, Facing::Right, false),
        (967.5, Facing::Left, false),
        (968.0, Facing::Left, true),
        (1027.0, Facing::Left, true),
        (1028.0, Facing::Left, false),
    ] {
        let mut play = Play::new(900, 380);
        play.settle();
        play.stand_at(x, facing);
        // Within range at the end of this step,
        assert!(!play.step(&[]), "{x} {facing:?}");
        assert_eq!(play.world.player.pos.x, f(x));
        // hurt on the next.
        assert_eq!(play.step(&[]), hurts, "{x} {facing:?}");
    }
}

/// thrower-contact.md, "Being hurt", 3: never on the frame before a
/// throw (its second, on frame 33 of its timetable); a player still
/// within reach then is hurt on the throw's frame.
#[test]
fn not_hurt_on_the_frame_before_a_throw() {
    let mut play = Play::new(900, 380);
    play.settle();
    play.stand_at(1000.0, Facing::Right);
    // This step is frame 31 of the timetable, the next is 32.
    play.world.things[0].clock = 31;
    assert!(!play.step(&[]));
    assert_eq!(play.world.player.hurt_ahead, None);
    // Frame 32, the one before the throw.
    assert!(!play.step(&[]));
    assert_eq!(play.world.player.hurt_ahead, Some(0));
    // Frame 33, the throw's.
    assert!(play.step(&[]));
}

/// thrower-contact.md, "Being hurt", 2: a player who lands on the ledge
/// within range isn't hurt on L + 1, and is on L + 2 if still within it
/// at the end of L + 1.
#[test]
fn landing_within_range_hurts_a_frame_later() {
    let mut play = Play::new(900, 380);
    play.settle();
    let mut held = vec![Action::Jump];
    let mut landed = false;
    for n in 0..120 {
        let in_air = !play.world.player.grounded();
        assert!(!play.step(&held), "frame {n}");
        if n > 0 {
            held.clear();
        }
        if in_air && play.world.player.grounded() {
            landed = true;
            break;
        }
    }
    assert!(landed);
    // Within range, facing right, at the end of the landing frame L.
    play.stand_at(1010.0, Facing::Right);
    assert!(!play.step(&[]), "L + 1");
    assert!(play.step(&[]), "L + 2");
}

/// Puts the player in the air at `pos` after its move on the next step,
/// falling: at `pos` less this step's fall, with `vy` before gravity.
fn fall_into(play: &mut Play, pos: Vec2, vy: f64, facing: Facing) {
    let player = &mut play.world.player;
    player.ground = None;
    player.state = State::Fall;
    player.facing = facing;
    player.vel = Vec2::new(Fixed::ZERO, f(vy));
    player.pos = pos;
}

/// thrower-contact.md, "Bouncing" and "What a bounce does to the
/// thrower": falling with I 32 and U 20, the player bounces; the thrower
/// holds still on B, B + 1 and B + 2, then rises at 5 less 7/16 a frame
/// and falls, and throws no more.
#[test]
fn a_bounce_knocks_the_thrower_out() {
    let mut play = Play::new(900, 200);
    play.settle();
    // In open air: U is 200 less the player's y, rounded up.
    fall_into(
        &mut play,
        Vec2::new(f(1032.0), f(179.0)),
        0.5,
        Facing::Right,
    );
    play.step(&[]);
    assert!(play.world.events.contains(&Event::Bounce));
    assert_eq!(up(play.world.player.pos.y), 180);
    let thrower = |play: &Play| play.world.things[0].clone();
    assert!(matches!(thrower(&play).phase, Phase::Falling { .. }));
    assert_eq!(thrower(&play).pos.y, f(200.0));
    // Not hurt on B + 1 though within range.
    assert!(!play.step(&[]));
    assert_eq!(thrower(&play).pos.y, f(200.0), "B + 1");
    play.step(&[]);
    assert_eq!(thrower(&play).pos.y, f(200.0), "B + 2");
    play.step(&[]);
    assert_eq!(thrower(&play).pos.y, f(200.0 - 4.5625), "B + 3");
    play.step(&[]);
    assert_eq!(thrower(&play).pos.y, f(200.0 - 4.5625 - 4.125), "B + 4");
    let placed = play.world.level.entities.len();
    for _ in 0..200 {
        play.step(&[]);
        assert!(play.world.things[placed..].iter().all(|t| !t.active));
        if !play.world.things[0].active {
            break;
        }
    }
}

/// thrower-contact.md, "Bouncing", 3 and 4: the range's edges, facing
/// right and left, a player up to 20 px below it bouncing too.
#[test]
fn the_bounce_range() {
    for (x, y, facing, bounces) in [
        (971.0, 190.0, Facing::Right, false),
        (972.0, 190.0, Facing::Right, true),
        (1043.0, 190.0, Facing::Right, true),
        (1044.0, 190.0, Facing::Right, false),
        (958.0, 190.0, Facing::Left, true),
        (957.5, 190.0, Facing::Left, false),
        (1029.0, 190.0, Facing::Left, true),
        (1030.0, 190.0, Facing::Left, false),
        // U 27 and 28, then -20 and -21.
        (1000.0, 173.0, Facing::Right, true),
        (1000.0, 172.0, Facing::Right, false),
        (1000.0, 220.0, Facing::Right, true),
        (1000.0, 221.0, Facing::Right, false),
    ] {
        let mut play = Play::new(900, 200);
        play.settle();
        // vy 0.5 before gravity, so y after the move is `y` + 0.5 plus
        // gravity: start far enough below a whole px that it rounds up
        // to `y`.
        fall_into(&mut play, Vec2::new(f(x), f(y - 1.0)), 0.0, facing);
        play.step(&[]);
        assert_eq!(up(play.world.player.pos.y), y as i64, "{x} {y}");
        assert_eq!(
            play.world.events.contains(&Event::Bounce),
            bounces,
            "{x} {y} {facing:?}"
        );
    }
}

/// thrower-contact.md, "Bouncing", 2: rising at 1.25 px a frame or
/// faster after gravity, the player doesn't bounce.
#[test]
fn rising_fast_doesnt_bounce() {
    for (vy, bounces) in [(-1.6875, false), (-1.6, true)] {
        let mut play = Play::new(900, 200);
        play.settle();
        fall_into(&mut play, Vec2::new(f(1000.0), f(195.0)), vy, Facing::Right);
        play.step(&[]);
        assert_eq!(
            play.world.events.contains(&Event::Bounce),
            bounces,
            "vy {vy}"
        );
    }
}
