//! Balls hurting the player in Classic 1 (spec/behavior/ball-contact.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{
    Archetypes, BALL, Behavior, Contact, Event, Facing, InputFrame, Phase, Pose, World,
    generic_feel, pose, step,
};

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn contact() -> Contact {
    match Archetypes::builtin().named(BALL).unwrap().behavior {
        Behavior::Ball(ball) => ball.contact.unwrap(),
        other => panic!("{other:?}"),
    }
}

/// Ground at y = 300 from x = 0 to 3000, the player standing at x 400,
/// and a ball placed in the level, which tests put where they want.
fn world() -> World {
    let src = r#"
        format = 1
        name = "balls"
        size = [3000, 600]
        start = [400, 300]

        [[collision]]
        shape = "rect"
        rect = [0, 300, 3000, 100]

        [[entities]]
        type = "ball"
        at = [2000, 100]
    "#;
    let level = Arc::new(Level::from_toml_str(src, "balls").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new() -> Play {
        Play {
            world: world(),
            held: Actions::NONE,
        }
    }

    /// Puts the ball at `pos`, falling at `vy` (before this step's
    /// gravity), with `steps` steps made.
    fn ball(&mut self, pos: Vec2, vy: f64, steps: u32) {
        let ball = &mut self.world.things[0];
        ball.active = true;
        ball.pos = pos;
        ball.vx = Fixed::ZERO;
        ball.clock = steps;
        ball.phase = Phase::Flying {
            vy: f(vy),
            ignoring: None,
            kept: false,
        };
    }

    /// One step; whether the player was hurt on it.
    fn step(&mut self, keys: &[Action]) -> bool {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        self.world.events.contains(&Event::Hurt)
    }

    fn pose(&self) -> Pose {
        pose(&self.world.player, &contact())
    }
}

/// ball-contact.md, "Examples": a ball falling straight down past a
/// player standing facing right at x 400.25. The ball is at 390.5,
/// 248 at the end of frame 99, falling at 6.625, so at 255.0625, 262.5625
/// and 270.5 at the end of frames 100 to 102.
#[test]
fn a_falling_ball_hurts_one_frame_after_it_is_within_reach() {
    // In the first, second and fourth parts of the turn on frame 100.
    for steps in [30, 33, 45] {
        let mut play = Play::new();
        play.step(&[]);
        play.world.player.pos.x = f(400.25);
        play.ball(Vec2::new(f(390.5), f(248.0)), 6.625, steps);
        assert!(!play.step(&[]), "frame 100, s {steps}");
        assert_eq!(play.world.things[0].pos.y, f(255.0625));
        assert_eq!(play.world.things[0].clock, steps + 1);
        assert!(!play.step(&[]), "frame 101, s {steps}");
        assert_eq!(play.world.things[0].pos.y, f(262.5625));
        assert!(play.step(&[]), "frame 102, s {steps}");
        assert_eq!(play.world.things[0].pos.y, f(270.5));
        // The ball isn't affected.
        assert!(play.world.things[0].active);
    }
}

/// ball-contact.md, "The ball's turn": in the second and third parts
/// the ball reaches 2 px higher, so A = 39 is within reach then but not
/// in the first. The ball is 39 px above the player's feet at the end
/// of frame 100.
#[test]
fn the_turn_moves_the_reach_up() {
    for (steps, hurts) in [
        (8, false),
        (9, true),
        (14, true),
        (15, true),
        (20, true),
        (21, false),
    ] {
        let mut play = Play::new();
        play.step(&[]);
        play.world.player.pos.x = f(400.25);
        // 261 after this step's fall of 0.4375 + 0.5625: A = 300 - 261.
        play.ball(Vec2::new(f(390.5), f(260.0)), 0.5625, steps);
        play.step(&[]);
        assert_eq!(play.world.things[0].pos.y, f(261.0));
        // On the next step it would fall within upright's reach anyway;
        // only the hurt on this one tells the turn apart.
        assert_eq!(play.step(&[]), hurts, "s {steps}");
    }
}

/// ball-contact.md, "When a ball hurts", 1: a ball that bounces within
/// reach hurts only once it's still within reach at the end of the next
/// frame.
#[test]
fn no_hurt_from_a_bounce_frame() {
    let mut play = Play::new();
    play.step(&[]);
    play.world.player.pos.x = f(400.25);
    // It lands on the floor at 300 on the next step, A = 0, I = 10.
    play.ball(Vec2::new(f(390.5), f(294.0)), 3.0, 0);
    assert!(!play.step(&[]));
    assert_eq!(play.world.things[0].pos.y, f(300.0));
    assert!(matches!(
        play.world.things[0].phase,
        Phase::Flying { kept: true, .. }
    ));
    // Its steps don't count the bounce.
    assert_eq!(play.world.things[0].clock, 0);
    // At 295.4375 at the end of the next frame, within reach: hurt on the one
    // after.
    assert!(!play.step(&[]));
    assert_eq!(play.world.things[0].pos.y, f(295.4375));
    assert!(play.step(&[]));
}

/// ball-contact.md, "Ranges": facing left, the range is turned around.
/// Upright facing right reaches I from -15 to 19; facing left, -21 to 13.
#[test]
fn ranges_turn_around_facing_left() {
    let c = contact();
    let right = c.reach(Pose::Upright, Facing::Right, 0);
    let left = c.reach(Pose::Upright, Facing::Left, 0);
    assert_eq!((right.across, right.above), ((-15, 19), (-3, 37)));
    assert_eq!((left.across, left.above), ((-21, 13), (-3, 37)));
    for (pose, right, left) in [
        (Pose::Airborne, (-15, 16), (-18, 13)),
        (Pose::Rolling, (-13, 19), (-21, 11)),
        (Pose::Crouching, (-18, 10), (-12, 16)),
        (Pose::Fidget, (-5, 22), (-24, 3)),
    ] {
        assert_eq!(c.reach(pose, Facing::Right, 0).across, right, "{pose:?}");
        assert_eq!(c.reach(pose, Facing::Left, 0).across, left, "{pose:?}");
    }
    // The fourth part of the turn: I + 2, A as in the table.
    let fourth = c.reach(Pose::Upright, Facing::Right, 45);
    assert_eq!((fourth.across, fourth.above), ((-13, 21), (-3, 37)));
    let third = c.reach(Pose::Upright, Facing::Left, 15);
    assert_eq!((third.across, third.above), ((-19, 15), (-1, 39)));
}

/// ball-contact.md, "Which pose the player has" and the jump example: a
/// jump is upright up to T + 5 and airborne from T + 6, so a ball 40 px
/// above is out of reach on T + 5 and within it on T + 6. A full-height
/// jump standing still is airborne up to L + 6 and upright from L + 7.
#[test]
fn a_jump_is_upright_then_airborne_until_after_landing() {
    let mut play = Play::new();
    for _ in 0..10 {
        play.step(&[]);
    }
    play.step(&[Action::Jump]);
    // Find the take-off frame and the landing.
    let mut poses = Vec::new();
    let mut take_off = None;
    let mut landed = None;
    for n in 0..120 {
        if play.world.events.contains(&Event::JumpStart) {
            take_off = Some(n);
        }
        if play.world.events.contains(&Event::Land) && landed.is_none() {
            landed = Some(n);
        }
        poses.push(play.pose());
        play.step(&[Action::Jump]);
    }
    let t = take_off.expect("a jump");
    let l = landed.expect("a landing");
    assert!(poses[t..=t + 5].iter().all(|&p| p == Pose::Upright));
    assert!(poses[t + 6..=l + 6].iter().all(|&p| p == Pose::Airborne));
    assert_eq!(poses[l + 7], Pose::Upright);
}

/// ball-contact.md, "Which pose the player has": a fall after walking off
/// a ledge without a face stays upright through the fall and the
/// landing.
#[test]
fn a_walk_off_without_a_face_stays_upright() {
    let src = r#"
        format = 1
        name = "ledge"
        size = [3000, 600]
        start = [400, 300]

        [[collision]]
        shape = "rect"
        rect = [0, 300, 450, 100]

        [[collision]]
        shape = "rect"
        rect = [0, 500, 3000, 100]
    "#;
    let level = Arc::new(Level::from_toml_str(src, "ledge").unwrap());
    let mut play = Play::new();
    play.world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    let mut left = false;
    let mut landed = None;
    for n in 0..120 {
        play.step(&[Action::Right]);
        left |= play.world.events.contains(&Event::LeaveGround);
        if left && play.world.events.contains(&Event::Land) {
            landed = Some(n);
        }
        if left {
            assert_eq!(play.pose(), Pose::Upright, "step {n}");
        }
        if landed.is_some_and(|l| n > l + 10) {
            break;
        }
    }
    assert!(landed.is_some());
}

/// ball-contact.md, "Which pose the player has": standing still, the
/// fidget pose is on frames 239 to 418 and 696 to 875 of the standing
/// pose; Down held on the ground is crouching.
#[test]
fn standing_still_fidgets_and_down_crouches() {
    let mut play = Play::new();
    play.step(&[]);
    assert_eq!(play.world.player.pose.standing, Some(0));
    let mut fidget = Vec::new();
    for n in 0..900u32 {
        if play.pose() == Pose::Fidget {
            fidget.push(n);
        }
        play.step(&[]);
    }
    let expected: Vec<u32> = (239..=418).chain(696..=875).collect();
    assert_eq!(fidget, expected);
    play.step(&[Action::Down]);
    assert_eq!(play.pose(), Pose::Crouching);
}
