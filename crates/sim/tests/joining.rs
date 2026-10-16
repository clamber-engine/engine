//! The missing character joining in Classic 1 when an object holding it
//! breaks (spec/behavior/holder-breaking.md, and following.md,
//! "Joining").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{
    FlowEvent, InputFrame, Other, Phase, Point, World, follow_point, generic_feel, step,
};

const A: Action = Action::Action;
const R: Action = Action::Right;

/// The values walkers in walker.md have, with walker-contact.md's small
/// walker's box.
const SMALL_WALKER: &str = "speed = 1, facing = \"left\", size = [20, 12], offset = 0.5, \
    response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, \
    defeat_max_fall = 8";

/// A thrower that throws nothing while the tests run.
const QUIET_THROWER: &str = "params = { period = 5000, throws = [[4000, 0, -7]] }";

/// The player at x = 200 on a floor at y = 400, a crate holding the
/// missing character in reach to its right, then `more` (collision and
/// entities).
fn level(more: &str) -> String {
    format!(
        r#"
        format = 1
        name = "joining"
        size = [3000, 800]
        start = [200, 400]

        [[entities]]
        type = "crate"
        at = [220, 400]
        params = {{ holds_character = true }}
        {more}
        "#
    )
}

/// Flat floor at y = 400 from x = 0 to 3000.
const FLAT: &str = "[[collision]]\nshape = \"rect\"\nrect = [0, 400, 3000, 200]";

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// 1/256 px per frame.
fn speed(n: i32) -> Fixed {
    Fixed::from_ratio_pow2(n, 8)
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// Classic 1 with both characters in the roster, the first leading
    /// alone, the crate picked up standing still and carried, ready to
    /// let go of action on the next step, Q.
    fn new(src: &str) -> Play {
        let mut play = Play::both(src);
        play.world.other = None;
        play
    }

    /// As [`Play::new`], with the second character there too.
    fn both(src: &str) -> Play {
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::new(),
        });
        let level = Arc::new(Level::from_toml_str(src, "joining").unwrap());
        let mut play = Play {
            world: World::new(level, Arc::new(feel)),
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        play.hold(&[A], 21);
        assert!(play.world.player.hands.is_some());
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

    /// Lets go and steps through Q + `n`.
    fn throw_to(&mut self, n: usize) {
        self.hold(&[], n + 1);
    }

    fn other(&self) -> &Other {
        self.world.other.as_ref().expect("a character is there")
    }
}

/// following.md, "Joining": where the joining character is on S + `k`,
/// from `from` (the object) to `to` (the lead), `k` = 1 to 32, worked
/// out on its own.
fn flight(from: Point, to: Point) -> Vec<Vec2> {
    let mut a = 0.0_f64;
    (1..=32_i64)
        .map(|k| {
            let x = from.0 + (k * (to.0 - from.0)).div_euclid(32);
            let line = from.1 + (k * (to.1 - from.1) + 31).div_euclid(32);
            let u = 6.0 + 6.0 * k as f64 - 0.1875 * (k * (k + 1)) as f64;
            a = a.fract() + u;
            Vec2::new(Fixed::from_int(x as i32), f(line as f64 - a))
        })
        .collect()
}

/// Checks the join from S, the step just taken: the character appears
/// at `appears`, flies to the lead while the lead holds still with right
/// held, stands at it on S + 33 with a fresh trail, and the lead goes on
/// from where it was on S + 34.
fn joins(play: &mut Play, appears: Point) {
    let lead = play.world.player.pos;
    let to = follow_point(lead);
    let other = play.other();
    assert_eq!(other.character, 1);
    assert_eq!(follow_point(other.pos), appears);
    assert_eq!(other.pos.x, Fixed::from_int(appears.0 as i32));
    let path = flight(appears, to);
    for (k, want) in path.iter().enumerate() {
        play.step(&[R]);
        assert_eq!(play.other().pos, *want, "S + {}", k + 1);
        assert_eq!(play.world.player.pos, lead, "S + {}", k + 1);
    }
    play.step(&[R]);
    let other = play.other();
    assert_eq!(follow_point(other.pos), to, "S + 33");
    assert_eq!(other.follow.trail.len(), 16);
    assert!(other.follow.trail.iter().all(|&p| p == to));
    assert_eq!(play.world.player.pos, lead, "S + 33");
    assert!(play.world.join.is_none());
}

/// After [`joins`], the lead walks on with right held on S + 34.
fn walks_on(play: &mut Play) {
    let lead = play.world.player.pos;
    play.step(&[R]);
    assert!(play.world.player.pos.x > lead.x, "S + 34");
}

/// holder-breaking.md, "Examples": thrown onto flat floor, it breaks on
/// its first touch, on Q + 34, and the character appears at (117, 54)
/// from the object's start.
#[test]
fn breaks_on_its_first_touch_of_the_floor() {
    let mut play = Play::new(&level(FLAT));
    play.throw_to(33);
    let start = Vec2::new(f(200.0), f(346.0));
    let thing = &play.world.things[0];
    assert!(thing.active);
    assert_eq!(
        thing.pos,
        Vec2::new(start.x + f(111.9296875), start.y + f(46.125))
    );
    assert!(play.world.other.is_none());
    play.step(&[]);
    let thing = &play.world.things[0];
    assert!(!thing.active, "Q + 34");
    assert_eq!(
        thing.pos,
        Vec2::new(start.x + f(117.64453125), start.y + f(54.0))
    );
    joins(&mut play, (317, 400));
    walks_on(&mut play);
}

/// following.md, "Joining": the height above the line is at most 51 px
/// and a fraction, and on S + 32 it's only the fraction.
#[test]
fn flies_on_an_arc_above_the_line() {
    let lift: Vec<Fixed> = flight((100, 400), (100, 400))
        .iter()
        .map(|p| f(400.0) - p.y)
        .collect();
    let most = *lift.iter().max().unwrap();
    assert!(most >= f(51.0) && most < f(52.0), "{most:?}");
    assert!(lift[31] >= Fixed::ZERO && lift[31] < Fixed::ONE);
}

/// holder-breaking.md, "Examples", "A wall": with a face rising to the
/// right from x = 60 and floor at y = 4 from the start, it breaks on
/// Q + 26 without moving across, but falls that frame: the character
/// appears at (50, 12).
#[test]
fn breaks_against_a_wall_after_falling_in_place() {
    let face = "[[collision]]\nshape = \"rect\"\nrect = [260, 350, 200, 50]";
    let mut play = Play::new(&level(&format!("{FLAT}\n{face}")));
    play.throw_to(25);
    let start = Vec2::new(f(200.0), f(346.0));
    assert_eq!(play.world.things[0].pos.x, start.x + f(50.2109375));
    play.step(&[]);
    let thing = &play.world.things[0];
    assert!(!thing.active, "Q + 26");
    assert_eq!(
        thing.pos,
        Vec2::new(start.x + f(50.2109375), start.y + f(11.5625))
    );
    joins(&mut play, (250, 358));
    walks_on(&mut play);
}

/// holder-breaking.md, "Examples", "A walker": hit on Q + 32, S, the
/// walker holds still until S + 33 and is knocked on from S + 34.
#[test]
fn breaks_on_a_walker_which_waits_for_the_join() {
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    play.world.things[1].pos.x = start.x + f(160.25);
    play.hold(&[], 11);
    assert!(!play.world.things[0].active, "Q + 32");
    assert!(play.world.flow.events.contains(&FlowEvent::Defeat(1)));
    let object = play.world.things[0].pos;
    assert_eq!(
        object,
        Vec2::new(start.x + f(105.90234375), start.y + f(39.875))
    );
    let walker = |play: &Play| {
        let w = &play.world.things[1];
        let vy = match w.phase {
            Phase::Knocked { vy } => vy.unwrap_or(Fixed::ZERO),
            _ => Fixed::ZERO,
        };
        (w.pos.x - start.x, w.vx, vy)
    };
    assert_eq!(walker(&play), (f(149.25), speed(-1024), Fixed::ZERO));
    let mut stand = play.clone_world();
    for n in 1..=33 {
        stand.step(&[]);
        assert_eq!(
            walker(&stand),
            (f(149.25), speed(-1024), Fixed::ZERO),
            "S + {n}"
        );
    }
    stand.step(&[]);
    assert_eq!(
        walker(&stand),
        (f(145.5), speed(-960), speed(-1536)),
        "S + 34"
    );
    joins(&mut play, (305, 386));
    walks_on(&mut play);
}

impl Play {
    fn clone_world(&self) -> Play {
        Play {
            world: self.world.clone(),
            held: self.held,
        }
    }
}

/// holder-breaking.md, "The thrower and its balls": early on, it breaks
/// on a thrower whose `x` is from 55 px ahead of it, which is knocked
/// out: it holds still to S + 33, gets `vy` −5 on S + 34 and rises from
/// S + 35.
#[test]
fn breaks_on_a_thrower_and_knocks_it_out() {
    // On Q + 22 the object is at x 214.4375 and y 347.4375.
    for (x, breaks) in [(269, true), (270, false)] {
        let thrower = format!("[[entities]]\ntype = \"thrower\"\nat = [{x}, 348]\n{QUIET_THROWER}");
        let mut play = Play::new(&level(&format!("{FLAT}\n{thrower}")));
        play.throw_to(22);
        assert_eq!(
            play.world.things[0].pos,
            Vec2::new(f(214.4375), f(347.4375))
        );
        assert_eq!(!play.world.things[0].active, breaks, "{x}");
        if !breaks {
            continue;
        }
        assert!(play.world.flow.events.contains(&FlowEvent::Defeat(1)));
        let at = play.world.things[1].pos;
        for n in 1..=34 {
            play.step(&[]);
            assert_eq!(play.world.things[1].pos, at, "S + {n}");
        }
        assert_eq!(
            play.world.things[1].phase,
            Phase::Falling {
                vy: f(-5.0),
                hold: 0
            }
        );
        play.step(&[]);
        assert_eq!(play.world.things[1].pos.y, at.y - f(5.0 - 0.4375), "S + 35");
    }
}

/// holder-breaking.md, "The thrower and its balls": early on, it breaks
/// on a ball whose `x` is from 47 px ahead of it, and the ball is gone.
#[test]
fn breaks_on_a_ball() {
    for (x, breaks) in [(261, true), (262, false)] {
        let ball = format!("[[entities]]\ntype = \"ball\"\nat = [{x}, 300]");
        let mut play = Play::new(&level(&format!("{FLAT}\n{ball}")));
        play.throw_to(21);
        // It moves to 339.5625 on Q + 22, falling 0 then.
        let ball = &mut play.world.things[1];
        assert!(ball.active);
        ball.pos = Vec2::new(Fixed::from_int(x), f(339.5625));
        ball.phase = Phase::Flying {
            vy: f(-0.4375),
            ignoring: None,
        };
        play.step(&[]);
        assert_eq!(play.world.things[1].pos.y, f(339.5625));
        assert_eq!(!play.world.things[0].active, breaks, "{x}");
        assert_eq!(!play.world.things[1].active, breaks, "{x}");
        assert_eq!(play.world.join.is_some(), breaks, "{x}");
    }
}

/// holder-breaking.md, "Outside the picture": thrown over a drop, it
/// breaks once its whole-pixel place is outside the window around the
/// picture where the view was at the start of the frame.
#[test]
fn breaks_outside_the_picture() {
    let ledge = "[[collision]]\nshape = \"rect\"\nrect = [0, 400, 250, 200]";
    let mut play = Play::new(&level(ledge));
    play.throw_to(21);
    let inside = |pos: Vec2, (v, w): (i64, i64)| {
        let x = pos.x.floor() - v;
        let y = -(-pos.y).floor() - w;
        (-31..=288).contains(&x) && (-31..=256).contains(&y)
    };
    for _ in 0..200 {
        let view = play.world.view.unwrap();
        let picture = (view.x(), view.y());
        play.step(&[]);
        let thing = &play.world.things[0];
        if !thing.active {
            assert!(!inside(thing.pos, picture), "{:?} {picture:?}", thing.pos);
            assert_eq!(follow_point(play.other().pos), follow_point(thing.pos));
            return;
        }
        assert!(inside(thing.pos, picture));
    }
    panic!("it never broke");
}

/// following.md, "Joining": with both characters there, it still breaks,
/// but nobody comes out and nothing waits.
#[test]
fn frees_nobody_with_both_there() {
    let mut play = Play::both(&level(FLAT));
    play.throw_to(34);
    assert!(!play.world.things[0].active);
    assert!(play.world.join.is_none());
    let lead = play.world.player.pos;
    play.step(&[R]);
    assert!(play.world.player.pos.x > lead.x);
}

/// holder-breaking.md, "Before it is thrown": a walker breaks it in the
/// hands from Q + 1, but not on Q.
#[test]
fn breaks_in_the_hands_during_a_throw() {
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    // The box in the hands reaches to x = 238; the walker's to 233.5 on
    // Q, after walking 1 px.
    play.world.things[1].pos.x = f(244.0);
    play.step(&[]);
    assert!(play.world.things[0].active, "Q");
    assert_eq!(play.world.things[1].phase, Phase::Ready);
    play.step(&[]);
    assert!(!play.world.things[0].active, "Q + 1");
    assert!(matches!(play.world.things[1].phase, Phase::Knocked { .. }));
    let head = (200, 368);
    assert_eq!(follow_point(play.world.player.pos), (200, 400));
    joins(&mut play, head);
    // The throw goes on from where it was, but the object is never
    // thrown.
    play.hold(&[], 60);
    assert!(play.world.player.hands.is_none());
    assert!(!play.world.things[0].active);
}
