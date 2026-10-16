//! Thrown objects in Classic 1 bouncing, rolling, breaking and hitting
//! walkers (spec/behavior/thrown-objects.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{Flight, FlowEvent, InputFrame, Phase, World, generic_feel, step};

const A: Action = Action::Action;

/// The values walkers in walker.md have, with walker-contact.md's small
/// walker's box.
const SMALL_WALKER: &str = "speed = 1, facing = \"left\", size = [20, 12], offset = 0.5, \
    response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, \
    defeat_max_fall = 8";

/// The player at x = 200 on a floor at y = 400, a crate in reach to its
/// right, then `more` (collision and entities).
fn level(more: &str) -> String {
    format!(
        r#"
        format = 1
        name = "thrown"
        size = [3000, 800]
        start = [200, 400]

        [[entities]]
        type = "crate"
        at = [220, 400]
        {more}
        "#
    )
}

/// Flat floor at y = 400 from x = 0 to 3000.
const FLAT: &str = "[[collision]]\nshape = \"rect\"\nrect = [0, 400, 3000, 200]";

/// The flat floor of thrown-objects.md's "Example" to x = 400, where a
/// face rising to the right starts: its first column's floor is 1 px
/// higher than the flat floor, and each one after it 4 px higher again.
fn face() -> String {
    let mut points = vec![[0, 600], [0, 400], [400, 400]];
    for k in 0..10 {
        let y = 399 - 4 * k;
        points.extend([[400 + k, y], [401 + k, y]]);
    }
    points.extend([[600, 363], [600, 600]]);
    format!("[[collision]]\nshape = \"polygon\"\npoints = {points:?}")
}

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
    /// Picks up the crate standing still and carries it, ready to let
    /// go of action on the next step, Q.
    fn new(src: &str) -> Play {
        let level = Arc::new(Level::from_toml_str(src, "thrown").unwrap());
        let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
        let mut play = Play {
            world,
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

    fn object(&self) -> Flight {
        match self.world.things[0].phase {
            Phase::Thrown(flight) => flight,
            other => panic!("{other:?}"),
        }
    }
}

/// thrown-objects.md, "Example": two bounces, rolling, and breaking
/// against a face. Positions from the object's on Q + 21.
#[test]
fn bounces_rolls_and_breaks_against_a_face() {
    let mut play = Play::new(&level(&face()));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(start, Vec2::new(f(200.0), f(346.0)));
    let mut q = 21;
    for (n, x, y, vx, vy) in [
        (22, 14.4375, 1.4375, 3696, Some(368)),
        (30, 92.71875, 28.6875, 1860, Some(1264)),
        (33, 111.9296875, 46.125, 1543, Some(1600)),
        (34, 117.64453125, 54.0, 1463, Some(-800)),
        (35, 123.0859375, 51.3125, 1393, Some(-688)),
        (41, 151.60546875, 44.375, 1121, Some(-16)),
        (42, 155.875, 44.75, 1093, Some(96)),
        (45, 168.1640625, 48.5, 1029, Some(432)),
        (46, 172.12109375, 54.0, 1013, Some(-216)),
        (47, 176.0234375, 53.59375, 999, Some(-104)),
        (48, 179.87890625, 54.0, 987, None),
        (49, 183.69140625, 54.0, 976, None),
        (53, 198.61328125, 54.0, 945, None),
    ] {
        play.hold(&[], n - q);
        q = n;
        let thing = &play.world.things[0];
        let flight = play.object();
        assert!(thing.active, "Q + {n}");
        assert_eq!(thing.pos.x - start.x, f(x), "Q + {n}");
        assert_eq!(thing.pos.y - start.y, f(y), "Q + {n}");
        assert_eq!(flight.vx, speed(vx), "Q + {n}");
        assert_eq!(flight.rolling, vy.is_none(), "Q + {n}");
        if let Some(vy) = vy {
            assert_eq!(flight.vy, speed(vy), "Q + {n}");
        }
    }
    // The move would take it into a face column higher than it.
    play.step(&[]);
    assert!(!play.world.things[0].active);
}

/// A wall breaks it too, though the spec only covers faces.
#[test]
fn breaks_against_a_wall() {
    let wall = "[[collision]]\nshape = \"rect\"\nrect = [300, 200, 20, 200]";
    let mut play = Play::new(&level(&format!("{FLAT}\n{wall}")));
    play.throw_to(21);
    let mut frames = 0;
    while play.world.things[0].active {
        assert!(play.world.things[0].pos.x < f(300.0));
        play.step(&[]);
        frames += 1;
        assert!(frames < 60);
    }
}

/// thrown-objects.md, "Rolling": it rolls up and down slopes of 1 px per
/// 4 columns on their floor, without breaking.
#[test]
fn rolls_over_gentle_slopes() {
    // Flat to 400, past where it starts rolling, up 1 px every 4
    // columns for 40 px, flat, then down again, all before it's far
    // enough from the view to be removed.
    let mut points = vec![[0, 600], [0, 400], [400, 400]];
    for k in 0..10 {
        points.extend([[400 + 4 * k, 399 - k], [404 + 4 * k, 399 - k]]);
    }
    for k in 0..10 {
        points.extend([[450 + 4 * k, 391 + k], [454 + 4 * k, 391 + k]]);
    }
    points.extend([[3000, 400], [3000, 600]]);
    let src = level(&format!(
        "[[collision]]\nshape = \"polygon\"\npoints = {points:?}"
    ));
    let ground = Level::from_toml_str(&src, "thrown").unwrap().collision;
    let mut play = Play::new(&src);
    play.throw_to(48);
    let mut x = Fixed::ZERO;
    while play.world.things[0].active {
        let thing = &play.world.things[0];
        assert!(play.object().rolling);
        x = thing.pos.x;
        let floor = ground.spans(Fixed::from_int(x.floor() as i32) + f(0.5))[0].top;
        assert_eq!(thing.pos.y, floor, "{:?}", thing.pos);
        play.step(&[]);
    }
    assert!(x > f(500.0), "{x:?}");
}

/// thrown-objects.md, "Leaving the picture": removed once its x rounded
/// down is 385 or more right of the view's left edge.
#[test]
fn is_removed_far_right_of_the_view() {
    let mut play = Play::new(&level(FLAT));
    play.throw_to(21);
    let mut last = 0;
    while play.world.things[0].active {
        let view = play.world.view.unwrap().x();
        last = play.world.things[0].pos.x.floor() - view;
        assert!(last < 385, "{last}");
        play.step(&[]);
    }
    let view = play.world.view.unwrap().x();
    let x = match play.world.things[0].phase {
        Phase::Thrown(_) => play.world.things[0].pos.x.floor(),
        other => panic!("{other:?}"),
    };
    assert!(x - view >= 385, "{last} then {}", x - view);
}

/// The walker's `(x, y, vx, vy)`, `vy` 0 while it walks.
fn walker(play: &Play) -> (Fixed, Fixed, Fixed, Fixed) {
    let w = &play.world.things[1];
    let vy = match w.phase {
        Phase::Knocked { vy } => vy.unwrap_or(Fixed::ZERO),
        _ => Fixed::ZERO,
    };
    (w.pos.x, w.pos.y, w.vx, vy)
}

/// thrown-objects.md, "Hitting walkers", "Example": a small walker
/// heading left at full speed, `x` = 160.25 from the object's start on
/// Q + 21, is hit on Q + 32 and knocked back.
#[test]
fn hits_a_walker_in_its_first_frames_out_of_the_hands() {
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(play.world.things[1].vx, speed(-256));
    play.world.things[1].pos.x = start.x + f(160.25);
    let mut q = 21;
    for (n, x, y, vx, vy) in [
        (30, 151.25, 54.0, -256, 0),
        (31, 150.25, 54.0, -256, 0),
        (32, 149.25, 54.0, -1024, 0),
        (33, 145.5, 54.0, -960, -1536),
        (34, 141.96875, 48.4375, -904, -1424),
        (35, 138.62890625, 43.3125, -855, -1312),
    ] {
        play.hold(&[], n - q);
        q = n;
        let hit = play.world.flow.events.contains(&FlowEvent::Defeat(1));
        assert_eq!(hit, n == 32, "Q + {n}");
        let got = walker(&play);
        let want = (start.x + f(x), start.y + f(y), speed(vx), speed(vy));
        assert_eq!(got, want, "Q + {n}");
    }
    // The object goes on as it would have: it bounces on Q + 34.
    assert_eq!(play.world.things[0].pos.y - start.y, f(51.3125));
    assert_eq!(play.object().vy, speed(-688));
    // It falls through the floor and out of the level.
    play.hold(&[], 200);
    assert!(!play.world.things[1].active);
}

/// The same example's second case: 40 px further right, it's hit after
/// the object's first bounce, on Q + 43, by the smaller area.
#[test]
fn hits_a_walker_later_by_the_smaller_area() {
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    play.world.things[1].pos.x = start.x + f(200.25);
    for n in 22..=43 {
        play.step(&[]);
        let hit = play.world.things[1].phase != Phase::Ready;
        assert_eq!(hit, n == 43, "Q + {n}");
        if n == 43 {
            let object = play.world.things[0].pos;
            let w = play.world.things[1].pos;
            assert_eq!(object.x.floor() - w.x.floor(), -18);
            assert_eq!(w.y - object.y, f(8.4375));
        }
    }
}
