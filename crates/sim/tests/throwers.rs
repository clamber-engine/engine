//! Throwers and their balls in Classic 1 (spec/behavior/thrower.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Param, Presets};
use clamber_sim::{
    BALL, Facing, FlowEvent, InputFrame, Other, Phase, START_LIVES, Thing, World, generic_feel,
    step,
};

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
    // The player stands where no ball reaches it.
    let mut play = Play::new(world(1070, &format!("{LEDGES}{}", thrower(1000, 300))));
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
    assert!(play.world.player.knockout.is_none());
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

/// Every ball and thrower there, with what decides how it goes on, in a
/// fixed order.
fn moving(world: &World) -> Vec<(String, Vec2, Fixed, u32, Phase)> {
    let mut all: Vec<_> = world
        .things
        .iter()
        .filter(|t| t.active)
        .map(|t| (t.kind.clone(), t.pos, t.vx, t.clock, t.phase))
        .collect();
    all.sort_by_key(|(kind, pos, ..)| (kind.clone(), pos.x.to_bits(), pos.y.to_bits()));
    all
}

/// Whether one of `a` and `b` has every ball the other has: they only
/// differ by balls the view removed in one run and not the other.
fn agree<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.iter().all(|x| b.contains(x)) || b.iter().all(|x| a.contains(x))
}

/// Plays `world` for `before` steps, then knocks the player out on H in
/// one copy and not in the other. Checks that on H the two are the same,
/// that the knocked-out run is unchanged on H + 1 to H + 10, and that from
/// H + 11 it is 10 frames behind the other for `after` frames. Returns how
/// many of those frames had something to compare.
fn ten_frames_late(mut world: World, before: u32, after: u32) -> usize {
    let still = InputFrame::single(Frame::default());
    for _ in 0..before {
        step(&mut world, &still);
    }
    let mut free = world.clone();
    // The hit lands on the next step, H.
    world.player.hurt_ahead = Some(0);
    step(&mut world, &still);
    step(&mut free, &still);
    assert!(world.player.knockout.is_some(), "knocked out on H");
    assert_eq!(moving(&world), moving(&free), "H as without the hit");
    let at_h = moving(&world);
    let view_at_h = world.view;
    for n in 1..=10 {
        step(&mut world, &still);
        assert_eq!(moving(&world), at_h, "H + {n} unchanged");
        if n < 10 {
            // So nothing can be outside the window then that wasn't on H.
            assert_eq!(world.view, view_at_h, "H + {n}: the view is still");
        }
    }
    let mut compared = 0;
    for n in 1..=after {
        step(&mut world, &still);
        step(&mut free, &still);
        assert!(free.player.knockout.is_none(), "the other run isn't hit");
        let (late, on_time) = (moving(&world), moving(&free));
        assert!(
            agree(&late, &on_time),
            "H + {}: {late:?} {on_time:?}",
            n + 10
        );
        compared += usize::from(!late.is_empty() && !on_time.is_empty());
    }
    compared
}

/// thrower.md, "While the player is knocked out": the thrower and its
/// balls stop on H + 1 to H + 10, no ball is thrown then, and from H + 11
/// everything is where it would have been 10 frames earlier, throws
/// included.
#[test]
fn the_thrower_and_its_balls_go_on_10_frames_late_after_a_knock_out() {
    // A ball in the air on H (thrown up on 2), and throws due on 33 and
    // 144 from the knock-out on 25.
    let compared = ten_frames_late(world(1070, &thrower(1000, 300)), 24, 130);
    assert!(compared > 100, "{compared}");
}

/// The same for a ball placed in the level.
#[test]
fn a_placed_ball_goes_on_10_frames_late_after_a_knock_out() {
    let ball = "[[entities]]\ntype = \"ball\"\nat = [1040, 300]\n\
        params = { velocity = [-1.171875, -3] }\n";
    let world = world(1070, &format!("{}{ball}", thrower(1000, 300)));
    let compared = ten_frames_late(world, 5, 60);
    assert!(compared > 40, "{compared}");
}

/// The same for a thrower already knocked out and falling, which isn't
/// removed on H + 1 to H + 10.
#[test]
fn a_falling_thrower_goes_on_10_frames_late_after_a_knock_out() {
    let mut w = world(1070, &thrower(1000, 300));
    for _ in 0..5 {
        step(&mut w, &InputFrame::single(Frame::default()));
    }
    w.things[0].phase = Phase::Falling {
        vy: f(-6.0),
        hold: 0,
    };
    let compared = ten_frames_late(w, 3, 40);
    assert!(compared > 10, "{compared}");
}

/// view.md and view-vertical.md, "Frames on which the view stays
/// still": when the player is knocked out on H, the view keeps where it
/// was at the end of H - 1 on H to H + 9, across and down, though it
/// would have moved on H without the hit, and moves again on H + 10.
#[test]
fn the_view_holds_still_from_a_knock_out_for_ten_frames() {
    let mut play = Play::new(world(400, ""));
    for _ in 0..40 {
        play.step(&[Action::Right]);
    }
    // Put the view behind and above its aims at the end of H - 1.
    let mut view = play.world.view.unwrap();
    view.left -= f(40.0);
    view.top -= f(40.0);
    play.world.view = Some(view);
    let mut free = play.world.clone();
    step(
        &mut free,
        &InputFrame::single(Frame::after(play.held, play.held)),
    );
    let moved = free.view.unwrap();
    assert!(moved.left != view.left && moved.top != view.top);

    play.world.player.hurt_ahead = Some(0);
    for n in 0..=9 {
        play.step(&[Action::Right]);
        assert!(play.world.player.knockout.is_some());
        assert_eq!(play.world.view, Some(view), "H + {n}");
    }
    play.step(&[Action::Right]);
    let moved = play.world.view.unwrap();
    assert!(moved.left != view.left && moved.top != view.top, "H + 10");
}

/// The frame a placed ball at x 700 appears on, walking right from 200
/// with a hit on `hit` if any, within `steps`; with the start-of-frame
/// view's left edge then and on the frame before.
fn appears_walking(hit: Option<u32>, steps: u32) -> Option<(u32, i64, i64)> {
    let ball = "[[entities]]\ntype = \"ball\"\nat = [700, 300]\n\
        params = { velocity = [-1.171875, -3] }\n";
    let mut play = Play::new(world(200, ball));
    let mut before = play.world.view.unwrap().x();
    for frame in 0..steps {
        if hit == Some(frame) {
            play.world.player.hurt_ahead = Some(0);
        }
        let left = play.world.view.unwrap().x();
        play.step(&[Action::Right]);
        let b = &play.world.things[0];
        if b.active {
            assert_eq!(b.pos.x, f(700.0) - f(1.171875), "one frame on");
            return Some((frame, left, before));
        }
        before = left;
    }
    None
}

/// thrower.md, "While the player is knocked out": a placed ball that
/// isn't there on H appears by the usual rule with the view at the start
/// of each frame, so not before H + 11, and only if the view comes far
/// enough; when it does it's one frame on and not held.
#[test]
fn a_placed_ball_appears_after_a_knock_out_only_once_the_view_moves() {
    let (on, ..) = appears_walking(None, 400).expect("appears without a hit");
    let mut seen = 0;
    for early in 1..=12 {
        let h = on - early;
        if let Some((frame, left, before)) = appears_walking(Some(h), on + 80) {
            assert!(frame >= h + 11, "{early} early: H + {}", frame - h);
            assert!(700 - left <= 288 && 700 - before > 288);
            seen += 1;
        }
    }
    assert!(seen > 0);
}

/// thrower.md, "Balls placed in a level": a placed ball appears only
/// once. After it is removed it doesn't come back however the view moves,
/// past the walker margins on both sides, until play resumes after a
/// knock-out, when it appears again by the same rule, one frame on and at
/// the same position.
#[test]
fn a_placed_ball_appears_again_only_after_a_knock_out() {
    let ball = "[[entities]]\ntype = \"ball\"\nat = [900, 300]\n\
        params = { velocity = [-1.171875, -3] }\n";
    let mut play = Play::new(world(200, ball));
    // Walks right until it appears; returns where it first was.
    let appears = |play: &mut Play| {
        for _ in 0..600 {
            play.step(&[Action::Right]);
            let b = &play.world.things[0];
            if b.active {
                return b.pos;
            }
        }
        panic!("never appeared");
    };
    let first = appears(&mut play);
    assert_eq!(first.x, f(900.0) - f(1.171875), "one frame on");
    let mut removed = false;
    for _ in 0..600 {
        play.step(&[Action::Right]);
        if !play.world.things[0].active {
            removed = true;
            break;
        }
    }
    assert!(removed, "left the picture");
    let mut farthest = (i64::MAX, i64::MIN);
    for (keys, steps) in [([Action::Right], 300), ([Action::Left], 900)] {
        for _ in 0..steps {
            play.step(&keys);
            assert!(!play.world.things[0].active, "doesn't come back");
            let x = 900 - play.world.view.unwrap().x();
            farthest = (farthest.0.min(x), farthest.1.max(x));
        }
    }
    assert!(farthest.0 < -400 && farthest.1 > 400, "{farthest:?}");

    play.world.player.hurt_ahead = Some(0);
    let mut resumed = false;
    for _ in 0..600 {
        play.step(&[]);
        if play.world.flow.events.contains(&FlowEvent::Respawn) {
            resumed = true;
            break;
        }
    }
    assert!(resumed, "play resumed");
    assert!(play.world.flow.lives < START_LIVES);
    assert_eq!(appears(&mut play), first, "the same position again");
}

/// [`world`] with both characters: the other one stands at `other`.
fn pair(start: i32, other: i32, more: &str) -> World {
    let world = world(start, more);
    let mut feel = (*world.feel).clone();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    // The other character stands where it's put.
    feel.world.set(Param::FollowTrail, Fixed::ZERO);
    let mut world = World::new(world.level.clone(), Arc::new(feel));
    let lead = world.player.pos;
    world.other = Some(Other::new(1, Vec2::ints(other, 381), Facing::Right, lead));
    world
}

/// Plays `world` for `before` steps, then hits the player on H with the
/// other character there in one copy, and not in the other. Checks that
/// a character is lost and no life, and that on H and the `after` frames
/// from there the thrower and the balls are the same in both runs, but
/// for the ones each run's own view removed. Returns how many of those
/// frames had something to compare, and on how many of them the two
/// runs had exactly the same.
fn as_without_the_loss(mut world: World, before: u32, after: u32) -> (usize, usize) {
    let still = InputFrame::single(Frame::default());
    for _ in 0..before {
        step(&mut world, &still);
    }
    let mut free = world.clone();
    world.player.hurt_ahead = Some(0);
    let (mut compared, mut same) = (0, 0);
    for n in 0..=after {
        step(&mut world, &still);
        step(&mut free, &still);
        if n == 0 {
            assert!(world.losing(), "a character is lost on H");
        }
        assert!(world.player.knockout.is_none(), "H + {n}: not knocked out");
        assert!(free.player.knockout.is_none(), "the other run isn't hit");
        assert_eq!(world.flow.lives, START_LIVES);
        let (lost, kept) = (moving(&world), moving(&free));
        assert!(agree(&lost, &kept), "H + {n}: {lost:?} {kept:?}");
        compared += usize::from(!lost.is_empty() && !kept.is_empty());
        same += usize::from(!lost.is_empty() && lost == kept);
    }
    (compared, same)
}

/// thrower.md, "While a character is lost": nothing stops. The thrower
/// and every ball go on from H as without the hit, throws on the usual
/// frames included.
#[test]
fn the_thrower_and_its_balls_go_on_while_a_character_is_lost() {
    // A ball in the air on H (thrown up on 2), and throws due on 33 and
    // 144 after the hit on 25.
    let world = pair(1070, 1040, &thrower(1000, 300));
    let (compared, same) = as_without_the_loss(world, 24, 160);
    assert!(compared > 150 && same > 150, "{compared} {same}");
}

/// The same for a ball placed in the level that is there on H.
#[test]
fn a_placed_ball_goes_on_while_a_character_is_lost() {
    let ball = "[[entities]]\ntype = \"ball\"\nat = [1040, 300]\n\
        params = { velocity = [-1.171875, -3] }\n";
    let world = pair(1070, 1100, &format!("{}{ball}", thrower(1000, 300)));
    let (compared, same) = as_without_the_loss(world, 5, 80);
    assert!(compared > 70 && same > 70, "{compared} {same}");
}

/// The same for a thrower already knocked out and falling, whether by a
/// bounce or, still holding, by a roll or the ground action.
#[test]
fn a_falling_thrower_goes_on_while_a_character_is_lost() {
    for hold in [0, 3, 10] {
        let mut w = pair(1070, 1040, &thrower(1000, 300));
        for _ in 0..5 {
            step(&mut w, &InputFrame::single(Frame::default()));
        }
        w.things[0].phase = Phase::Falling { vy: f(-6.0), hold };
        let (compared, same) = as_without_the_loss(w, 3, 60);
        assert!(compared > 20 && same > 20, "hold {hold}: {compared} {same}");
    }
}

/// Walks (or runs, with Action) left from 1800, with a ball placed at
/// `x`, until it comes into the window: where its x was from the
/// start-of-frame view's left edge on that frame, and whether it was
/// there at the end of the frame, with its x then. `None` if it never
/// came in.
fn appears_walking_left(x: i32, run: bool) -> Option<(i64, Option<Fixed>)> {
    let ball = format!(
        "[[entities]]\ntype = \"ball\"\nat = [{x}, 300]\n\
        params = {{ velocity = [-1.171875, -3] }}\n"
    );
    let mut play = Play::new(world(1800, &ball));
    let keys: &[Action] = if run {
        &[Action::Left, Action::Action]
    } else {
        &[Action::Left]
    };
    for _ in 0..600 {
        let left = play.world.view.unwrap().x();
        play.step(keys);
        let b = &play.world.things[0];
        if b.phase != Phase::Ready {
            let seen = b.active.then_some(b.pos.x);
            // It appears only once, so it doesn't come back while the view
            // goes on moving left.
            if seen.is_none() {
                for _ in 0..120 {
                    play.step(keys);
                    assert!(!play.world.things[0].active, "{x}: came back");
                }
            }
            return Some((i64::from(x) - left, seen));
        }
    }
    None
}

/// thrower.md, "Balls placed in a level": with the view moving left, a
/// ball starting left comes into the window at `x` - `V` = -31 or a
/// little more. "Leaving the picture" applies on its appearance frame, to
/// the position one frame on: at -31 and -30 it's removed then and never
/// seen; at -29 or more it's seen, one frame on.
#[test]
fn a_placed_ball_coming_in_on_the_left_can_leave_on_the_frame_it_appears() {
    let mut by_offset = BTreeMap::new();
    for run in [false, true] {
        for x in (900..1100).step_by(7) {
            let (offset, seen) = appears_walking_left(x, run).expect("came into the window");
            assert!((-31..=-27).contains(&offset), "{x}: {offset}");
            assert_eq!(seen.is_some(), offset >= -29, "{x}: at {offset}");
            if let Some(at) = seen {
                assert_eq!(at, Fixed::from_int(x) - f(1.171875), "one frame on");
            }
            *by_offset.entry(offset).or_insert(0) += 1;
        }
    }
    // Both cases came up.
    assert!(by_offset.keys().any(|&o| o < -29), "{by_offset:?}");
    assert!(by_offset.keys().any(|&o| o >= -29), "{by_offset:?}");
}

/// [`appears_walking`] with both characters, the other one 30 px behind,
/// and the lead hit on `hit` with it there, so a character is lost.
fn appears_walking_pair(hit: Option<u32>, steps: u32) -> Option<(u32, i64, i64)> {
    let ball = "[[entities]]\ntype = \"ball\"\nat = [700, 300]\n\
        params = { velocity = [-1.171875, -3] }\n";
    let mut play = Play::new(pair(200, 170, ball));
    let mut before = play.world.view.unwrap().x();
    for frame in 0..steps {
        if hit == Some(frame) {
            play.world.player.hurt_ahead = Some(0);
        }
        let left = play.world.view.unwrap().x();
        play.step(&[Action::Right]);
        if hit == Some(frame) {
            assert!(play.world.losing(), "a character is lost on H");
        }
        let b = &play.world.things[0];
        if b.active {
            assert_eq!(b.pos.x, f(700.0) - f(1.171875), "one frame on");
            return Some((frame, left, before));
        }
        before = left;
    }
    None
}

/// thrower.md, "While a character is lost": a placed ball that isn't
/// there yet on H appears by the usual rule with the view of the run
/// with the hit, one frame on and not held; on H itself, as without the
/// hit.
#[test]
fn a_placed_ball_appears_after_a_character_is_lost_by_that_run_s_view() {
    let (on, ..) = appears_walking_pair(None, 400).expect("appears without a hit");
    let mut seen = 0;
    for early in 0..=20 {
        let h = on - early;
        let Some((frame, left, before)) = appears_walking_pair(Some(h), on + 200) else {
            continue;
        };
        if early == 0 {
            assert_eq!(frame, on, "on H as without the hit");
        }
        assert!(frame >= on, "{early} early: not before");
        assert!(700 - left <= 288 && 700 - before > 288, "{early} early");
        seen += 1;
    }
    assert!(seen > 1, "{seen}");
}
