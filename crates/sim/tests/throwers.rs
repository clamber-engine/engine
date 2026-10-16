//! Throwers and their balls in Classic 1 (spec/behavior/thrower.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{BALL, InputFrame, Phase, START_LIVES, Thing, World, generic_feel, step};

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// A level with ground at y = 381 from x = 0 to 3000, the player at
/// `start` on it, then `more` (collision and entities).
fn world(start: i32, more: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "throwers"
        size = [3000, 600]
        start = [{start}, 381]

        [[collision]]
        shape = "rect"
        rect = [0, 381, 3000, 100]
        {more}
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "throwers").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

fn thrower(x: i32, y: i32) -> String {
    format!("[[entities]]\ntype = \"thrower\"\nat = [{x}, {y}]\n")
}

/// The thrower.md "Example" floors in column 1000: thin ledges at y 301
/// and 331 over the ground at 381.
const LEDGES: &str = "[[collision]]\nshape = \"rect\"\nrect = [990, 301, 20, 2]\n\
    [[collision]]\nshape = \"rect\"\nrect = [990, 331, 20, 2]\n";

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

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// Thrown balls, active or not.
    fn thrown(&self) -> &[Thing] {
        &self.world.things[self.world.level.entities.len()..]
    }

    fn balls(&self) -> Vec<&Thing> {
        self.thrown().iter().filter(|t| t.active).collect()
    }
}

fn vy(ball: &Thing) -> Fixed {
    match ball.phase {
        Phase::Flying { vy, .. } => vy,
        other => panic!("{other:?}"),
    }
}

/// thrower.md, "Example": a ball thrown straight up on `A` + 2 from a
/// thrower at (1000, 300), bouncing on the ledge at 301, dropping through
/// it and the one at 331, and bouncing on the ground at 381. The engine
/// has the ball one frame on on `T`.
#[test]
fn a_ball_bounces_drops_through_ledges_and_bounces_again() {
    let mut play = Play::new(world(1000, &format!("{LEDGES}{}", thrower(1000, 300))));
    assert!(play.world.things[0].active, "the thrower is near the view");
    // Steps 0 and 1 are A and A + 1.
    play.step(&[]);
    play.step(&[]);
    assert!(play.balls().is_empty());
    play.step(&[]);
    let index = play.world.level.entities.len();
    let ball = |play: &Play| play.world.things[index].clone();
    assert_eq!(ball(&play).kind, BALL);
    let mut t = 1;
    for (n, y, v) in [
        (1, 285.4375, -6.5625),
        (15, 239.5, -0.4375),
        (16, 239.5, 0.0),
        (31, 292.0, 6.5625),
        (32, 301.0, -5.0),
        (33, 296.4375, -4.5625),
        (43, 274.875, -0.1875),
        (54, 301.6875, 4.625),
        (59, 331.375, 6.8125),
        (61, 346.3125, 7.6875),
        (62, 354.3125, 8.0),
        (63, 362.3125, 8.0),
        (64, 370.3125, 8.0),
        (65, 381.0, -5.0),
    ] {
        while t < n {
            play.step(&[]);
            t += 1;
        }
        let b = ball(&play);
        assert!(b.active, "T + {n}");
        assert_eq!(b.pos, Vec2::new(f(1000.0), f(y)), "T + {n}");
        assert_eq!(vy(&b), f(v), "T + {n}");
        let ignoring = match b.phase {
            Phase::Flying { ignoring, .. } => ignoring,
            _ => unreachable!(),
        };
        let want = match n {
            32..=61 => Some(f(301.0)),
            65 => Some(f(381.0)),
            _ => None,
        };
        assert_eq!(ignoring, want, "T + {n}");
    }
}

/// thrower.md, "When it throws": up on `A` + 2, right on `A` + 33 and
/// left on `A` + 144 of every 173 frames, at the start's speeds.
#[test]
fn throws_on_the_timetable() {
    // The player stands where no ball reaches it.
    let mut play = Play::new(world(1070, &thrower(1000, 300)));
    let first = play.world.level.entities.len();
    let mut throws = Vec::new();
    for frame in 0..400u32 {
        let before: Vec<bool> = play.thrown().iter().map(|t| t.active).collect();
        play.step(&[]);
        for (i, ball) in play.thrown().iter().enumerate() {
            if ball.active && !before.get(i).copied().unwrap_or(false) {
                assert_eq!(ball.pos.y, f(285.4375), "one frame on");
                throws.push((frame, ball.vx, ball.pos.x));
            }
        }
        assert!(
            play.world.things.len() <= first + 4,
            "removed balls' places are taken again"
        );
    }
    let right = f(3.125);
    let left = -right;
    let expected: Vec<_> = [
        (2, Fixed::ZERO),
        (33, right),
        (144, left),
        (175, Fixed::ZERO),
        (206, right),
        (317, left),
        (348, Fixed::ZERO),
        (379, right),
    ]
    .into_iter()
    .map(|(frame, vx)| (frame, vx, f(1000.0) + vx))
    .collect();
    assert_eq!(throws, expected);
    assert_eq!(play.world.flow.lives, START_LIVES);
}

/// thrower.md, "The thrower": a thrower comes into the level as a walker
/// would, starting its timetable then.
#[test]
fn appears_with_the_view() {
    let mut play = Play::new(world(200, &thrower(700, 300)));
    let thrower = 0;
    assert!(!play.world.things[thrower].active);
    let mut appeared = None;
    for frame in 0..400u32 {
        let left = play.world.view.unwrap().x();
        play.step(&[Action::Right]);
        if play.world.things[thrower].active {
            assert!(700 - left <= 288, "appears in the window");
            assert!(700 - left >= 287, "on the first frame it's in");
            appeared = Some(frame);
            break;
        }
    }
    let appeared = appeared.expect("the thrower appears");
    // Stand still from here; the first ball is thrown up on A + 2.
    let mut first_throw = None;
    for frame in appeared + 1..appeared + 10 {
        play.step(&[]);
        if !play.balls().is_empty() {
            first_throw = Some(frame);
            break;
        }
    }
    assert_eq!(first_throw, Some(appeared + 2));
}

/// thrower.md, "Leaving the picture": a ball thrown left from just inside
/// the view's left edge is kept while its x is 31 px left of the view and
/// removed once it's 32, by where the view was at the start of the frame.
#[test]
fn balls_leave_the_picture() {
    let left = Play::new(world(1000, "")).world.view.unwrap().x() as i32;
    let top = Play::new(world(1000, "")).world.view.unwrap().y() as i32;
    let mut play = Play::new(world(1000, &thrower(left + 10, top + 100)));
    assert!(play.world.things[0].active);
    let mut leaving = None;
    for _ in 0..200 {
        let left = play.world.view.unwrap().x();
        let before: Vec<bool> = play.thrown().iter().map(|t| t.active).collect();
        play.step(&[]);
        for (i, ball) in play.thrown().iter().enumerate() {
            let was = before.get(i).copied().unwrap_or(false);
            let gap = ball.pos.x.floor() - left;
            if !ball.vx.is_negative() {
                continue;
            }
            if was && !ball.active {
                leaving = Some(gap);
            } else if ball.active {
                assert!(gap >= -31, "kept to -31, not {gap}");
            }
        }
        if leaving.is_some() {
            break;
        }
    }
    let gap = leaving.expect("the left ball leaves the picture");
    assert!((-35..=-32).contains(&gap), "{gap}");
}

/// thrower.md, "Balls placed in a level": without a view, a placed ball
/// starts at its placed speeds on the first step, and bounces on the
/// ground.
#[test]
fn a_placed_ball_moves_from_its_placed_speeds() {
    let src = r#"
        format = 1
        name = "placed"
        size = [3000, 600]
        start = [100, 381]

        [[collision]]
        shape = "rect"
        rect = [0, 381, 3000, 100]

        [[entities]]
        type = "ball"
        at = [500, 360]
        params = { velocity = [-1.171875, -3] }
    "#;
    let level = Arc::new(Level::from_toml_str(src, "placed").unwrap());
    let mut play = Play::new(World::new(
        level,
        Arc::new(generic_feel("default").unwrap()),
    ));
    play.step(&[]);
    let ball = &play.world.things[0];
    assert_eq!(ball.pos, Vec2::new(f(498.828125), f(357.4375)));
    assert_eq!(vy(ball), f(-2.5625));
    let mut bounced = false;
    for _ in 0..40 {
        play.step(&[]);
        let ball = &play.world.things[0];
        if vy(ball) == f(-5.0) {
            assert_eq!(ball.pos.y, f(381.0));
            bounced = true;
            break;
        }
    }
    assert!(bounced);
}

/// thrower.md, "Balls placed in a level": with a view, a placed ball
/// appears once its x is in the walker window, one frame on, and doesn't
/// come back once removed.
#[test]
fn a_placed_ball_appears_with_the_view() {
    let ball = "[[entities]]\ntype = \"ball\"\nat = [700, 300]\n\
        params = { velocity = [-1.171875, -3] }\n";
    let mut play = Play::new(world(200, ball));
    assert!(!play.world.things[0].active);
    let mut appeared = false;
    for _ in 0..400 {
        let left = play.world.view.unwrap().x();
        play.step(&[Action::Right]);
        let b = &play.world.things[0];
        if b.active {
            assert!(700 - left <= 288);
            assert_eq!(b.pos.x, f(700.0) - f(1.171875), "one frame on");
            appeared = true;
            break;
        }
    }
    assert!(appeared);
}
