//! Thrown objects in Classic 1 bouncing, rolling, breaking, hitting
//! walkers and passing through the characters
//! (spec/behavior/thrown-objects.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Feel, Fixed, Presets};
use clamber_sim::{Facing, Flight, FlowEvent, InputFrame, Phase, View, World, generic_feel, step};

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
    face_of("")
}

/// [`face`], with `more` added to its collider.
fn face_of(more: &str) -> String {
    let mut points = vec![[0, 600], [0, 400], [400, 400]];
    for k in 0..10 {
        let y = 399 - 4 * k;
        points.extend([[400 + k, y], [401 + k, y]]);
    }
    points.extend([[600, 363], [600, 600]]);
    format!("[[collision]]\nshape = \"polygon\"\npoints = {points:?}\n{more}")
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
        Play::with_feel(src, generic_feel("classic-1").unwrap())
    }

    /// [`Play::new`] with both characters, the other following.
    fn with_other(src: &str) -> Play {
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::new(),
        });
        let play = Play::with_feel(src, feel);
        assert!(play.world.other.is_some());
        play
    }

    fn with_feel(src: &str, feel: Feel) -> Play {
        let level = Arc::new(Level::from_toml_str(src, "thrown").unwrap());
        let world = World::new(level, Arc::new(feel));
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
    assert!(play.world.bursts.is_empty());
    play.step(&[]);
    assert!(!play.world.things[0].active);
    // A flash and four pieces are left where it was, the flash mirrored
    // as it was moving right, and last as long as the flash does.
    let [burst] = play.world.bursts[..] else {
        panic!("{:?}", play.world.bursts);
    };
    assert_eq!((burst.thing, burst.object, burst.frame), (0, Some(true), 0));
    assert_eq!(burst.flash, play.world.things[0].pos);
    for _ in 0..18 {
        play.step(&[]);
    }
    assert_eq!(play.world.bursts[0].frame, 18);
    play.step(&[]);
    assert!(play.world.bursts.is_empty());
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

/// thrown-objects.md, "Leaving the picture": removed once its x rounded
/// down is 128 or more left of the view's left edge as it was at the
/// start of the frame, and kept at 127.
#[test]
fn is_removed_far_left_of_the_view() {
    let mut play = Play::new(&level(FLAT));
    play.world.player.facing = Facing::Left;
    play.throw_to(21);
    let with_view = |play: &Play, left: i64| {
        let mut next = Play {
            world: play.world.clone(),
            held: play.held,
        };
        next.world.view = Some(View {
            left: Fixed::from_int(left as i32),
            ..play.world.view.unwrap()
        });
        next.step(&[]);
        next
    };
    // Until it nears the level's left end, past which it falls out.
    for _ in 0..30 {
        assert!(play.world.things[0].active);
        // Where it ends this step, with the view clear of it.
        let x = with_view(&play, play.world.things[0].pos.x.floor())
            .world
            .things[0]
            .pos
            .x
            .floor();
        assert!((0..200).contains(&x), "thrown left: {x}");
        assert!(!with_view(&play, x + 128).world.things[0].active, "{x}");
        play = with_view(&play, x + 127);
    }
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

/// thrown-objects.md, "When a walker is hit" and "The walker": a medium
/// walker facing left is hit by the object moving right on Q + 37 (t =
/// 16) with I = −51, by the larger area (−55 to 26), out of it on the
/// frame before. On flat floor it's there for the last time on K + 40,
/// 95 px below its floor.
#[test]
fn hits_a_medium_walker_which_falls_out_like_a_small_one() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {medium} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    play.world.things[1].pos.x = start.x + f(200.25);
    let mut k = None;
    let mut i = 0;
    for n in 22..=100 {
        play.step(&[]);
        let w = &play.world.things[1];
        let before = i;
        i = play.world.things[0].pos.x.floor() - w.pos.x.floor();
        if k.is_none() && w.phase != Phase::Ready {
            assert_eq!((n, i), (37, -51));
            assert!(before < -55, "{before}");
            k = Some(n);
        }
        let Some(k) = k else { continue };
        let there = play.world.things[1].active;
        assert_eq!(there, n <= k + 40, "K + {}", n - k);
        if n == k + 40 {
            assert_eq!(play.world.things[1].pos.y.floor(), 495);
        }
    }
    assert!(k.is_some());
}

/// A thrower at `(x, y)` that throws nothing for a long while.
fn thrower(x: i32, y: i32) -> String {
    format!(
        "[[entities]]\ntype = \"thrower\"\nat = [{x}, {y}]\n\
         params = {{ period = 5000, throws = [[4999, 0, -7]] }}"
    )
}

/// The thrower's `y` and whether it's been knocked out.
fn knocked_thrower(play: &Play) -> (Fixed, bool) {
    let t = &play.world.things[1];
    (t.pos.y, matches!(t.phase, Phase::Falling { .. }))
}

/// thrown-objects.md, "Hitting the thrower", "Example": the object
/// thrown right knocks out a thrower 115 px right of and 20 px below its
/// start on Q + 26, and goes on as without it; the thrower holds still
/// on K + 1 and falls from K + 2.
#[test]
fn knocks_out_the_thrower_and_goes_on() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(315, 366))));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(start, Vec2::new(f(200.0), f(346.0)));
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    for n in 22..=30 {
        play.step(&[]);
        control.step(&[]);
        let (y, knocked) = knocked_thrower(&play);
        assert_eq!(knocked, n >= 26, "Q + {n}");
        let want = match n {
            ..=27 => 20.0,
            28 => 15.4375,
            29 => 11.3125,
            _ => 7.625,
        };
        assert_eq!(y - start.y, f(want), "Q + {n}");
        assert!(play.world.things[0].active, "Q + {n}");
        assert_eq!(play.world.things[0].phase, control.world.things[0].phase);
        assert_eq!(play.world.things[0].pos, control.world.things[0].pos);
    }
    assert!(
        play.world
            .flow
            .events
            .iter()
            .all(|e| *e != FlowEvent::Defeat(1))
    );
}

/// Throws the crate, right or left, and on Q + `n` (from 22) puts the
/// thrower `i` px left of where the object ends the frame (I, both `x`
/// rounded down) and `h` px below it (H, both `y` rounded up). Returns
/// whether the thrower was knocked out on that frame.
fn thrower_hit(right: bool, n: usize, i: i64, h: i64) -> bool {
    let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(300, 100))));
    if !right {
        play.world.player.facing = Facing::Left;
    }
    play.throw_to(n - 1);
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.step(&[]);
    let object = control.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    let x = object.x.floor() - i;
    let y = up(object.y) + h;
    play.world.things[1].pos = Vec2::new(
        Fixed::from_int(x as i32) + f(0.75),
        Fixed::from_int(y as i32) - f(0.25),
    );
    play.step(&[]);
    assert_eq!(play.world.things[0].pos, object);
    knocked_thrower(&play).1
}

/// thrown-objects.md, "When the thrower is hit": the ranges of I and H,
/// ends included, early (t up to 16) and late, moving right and left.
#[test]
fn hits_the_thrower_in_the_measured_ranges() {
    for (right, n, (i_from, i_to), (h_from, h_to)) in [
        (true, 22, (-55, 37), (-47, 65)),
        (true, 37, (-55, 37), (-47, 65)),
        (true, 38, (-30, 36), (-23, 32)),
        (false, 22, (-36, 56), (-47, 65)),
        (false, 38, (-35, 31), (-23, 32)),
    ] {
        for (i, h, want) in [
            (i_from, 0, true),
            (i_to, 0, true),
            (i_from - 1, 0, false),
            (i_to + 1, 0, false),
            (0, h_from, true),
            (0, h_to, true),
            (0, h_from - 1, false),
            (0, h_to + 1, false),
        ] {
            let got = thrower_hit(right, n, i, h);
            assert_eq!(got, want, "right {right} Q + {n} I {i} H {h}");
        }
    }
}

/// thrown-objects.md, "What happens on a hit": still in the hands during
/// a throw, from Q + 1 to Q + 20, it breaks on a thrower by the early
/// ranges, and the throw goes on as without it. Not on Q or Q + 21.
#[test]
fn breaks_on_the_thrower_in_the_hands() {
    for n in 0..=21 {
        let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(300, 100))));
        let mut control = Play {
            world: play.world.clone(),
            held: play.held,
        };
        control.world.things[1].active = false;
        if n > 0 {
            play.throw_to(n - 1);
            control.throw_to(n - 1);
        }
        // The thrower 55 px right of where the object ends the step on
        // the player's head, the early range's end.
        let mut ahead = Play {
            world: control.world.clone(),
            held: control.held,
        };
        ahead.step(&[]);
        let head = ahead.world.things[0].pos;
        play.world.things[1].pos = Vec2::new(head.x + f(55.0), head.y);
        play.step(&[]);
        control.step(&[]);
        let hit = (1..=20).contains(&n);
        assert_eq!(knocked_thrower(&play).1, hit, "Q + {n}");
        assert_eq!(play.world.things[0].active, !hit, "Q + {n}");
        if hit {
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.hands, control.world.player.hands);
            // The rest of the throw goes on and throws nothing.
            play.throw_to(30);
            control.throw_to(30);
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.state, control.world.player.state);
            assert!(!play.world.things[0].active);
        }
    }
}

/// thrown-objects.md, "What happens on a hit": carried, it breaks on a
/// thrower by the late ranges, the way the player faces. The player
/// goes on as without it, carrying nothing, and letting go of action
/// throws nothing.
#[test]
fn breaks_on_the_thrower_while_carried() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(300, 100))));
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    // A jump, carrying.
    while play.world.player.grounded() {
        play.step(&[A, Action::Jump]);
        control.step(&[A, Action::Jump]);
    }
    play.world.player.facing = Facing::Left;
    control.world.player.facing = Facing::Left;
    // Where the object ends the next step, from the control.
    let mut ahead = Play {
        world: control.world.clone(),
        held: control.held,
    };
    ahead.step(&[A]);
    let head = ahead.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    // I of 32 isn't hit; 31, the late range's end facing left, is.
    let at = |i: i64| {
        Vec2::new(
            Fixed::from_int((head.x.floor() - i) as i32),
            Fixed::from_int(up(head.y) as i32),
        )
    };
    let mut miss = Play {
        world: play.world.clone(),
        held: play.held,
    };
    miss.world.things[1].pos = at(32);
    miss.step(&[A]);
    assert!(!knocked_thrower(&miss).1);
    assert!(miss.world.things[0].active);
    play.world.things[1].pos = at(31);
    play.step(&[A]);
    control.step(&[A]);
    assert!(knocked_thrower(&play).1);
    assert!(!play.world.things[0].active);
    assert!(play.world.player.hands.is_none());
    for _ in 0..40 {
        assert_eq!(play.world.player.pos, control.world.player.pos);
        assert_eq!(play.world.player.vel, control.world.player.vel);
        play.step(&[]);
        control.step(&[]);
    }
    assert!(play.world.player.hands.is_none());
    assert!(!play.world.things[0].active);
}

/// A one-way face breaks it as a solid one does (thrown-objects.md,
/// "Breaking against a wall"): the "Example" throw breaks on the same
/// frame.
#[test]
fn breaks_against_a_one_way_face() {
    let mut play = Play::new(&level(&face_of("one_way = true")));
    play.throw_to(53);
    assert!(play.world.things[0].active);
    play.step(&[]);
    assert!(!play.world.things[0].active);
}

/// Puts the thrown object at `(x, y)` with no speed across and `vy`
/// (before this step's gravity), and steps once. Returns its `y` and
/// `vy`.
fn from(play: &mut Play, x: f64, y: f64, vy: f64) -> (Fixed, Fixed) {
    let mut flight = play.object();
    (flight.vx, flight.target, flight.vy, flight.rolling) =
        (Fixed::ZERO, Fixed::ZERO, f(vy), false);
    play.world.things[0].pos = Vec2::new(f(x), f(y));
    play.world.things[0].phase = Phase::Thrown(flight);
    play.step(&[]);
    (play.world.things[0].pos.y, play.object().vy)
}

/// thrown-objects.md, "Ground passable from below": rising from below
/// into ground that isn't one-way, it keeps its whole px and stops; a
/// one-way platform it rises into.
#[test]
fn is_stopped_rising_into_solid_ground_from_below() {
    for (one_way, want) in [(false, (325.6875, 0.0)), (true, (319.6875, -5.5625))] {
        let ceiling = format!(
            "[[collision]]\nshape = \"rect\"\nrect = [180, 300, 100, 20]\none_way = {one_way}"
        );
        let mut play = Play::new(&level(&format!("{FLAT}\n{ceiling}")));
        play.throw_to(21);
        assert_eq!(
            from(&mut play, 200.5, 325.25, -6.0),
            (f(want.0), f(want.1)),
            "{one_way}"
        );
        // It falls from there.
        play.step(&[]);
        assert_eq!(play.object().vy, f(want.1 + 0.4375));
    }
}

/// thrown-objects.md, "In the air" and "The floor of a column": in a
/// platform's layer up to a step below its top and not moving down, it's
/// lifted by as much as the top is above its `y` rounded up; lower in the
/// layer it moves on as if there were no floor.
#[test]
fn is_lifted_onto_a_platform_a_step_above_it() {
    let platform = "[[collision]]\nshape = \"rect\"\nrect = [180, 300, 100, 16]\none_way = true";
    let src = level(&format!("{FLAT}\n{platform}"));
    for (y, vy, want) in [
        // New y 303.6875, lifted by 306 - 300.
        (305.25, -2.0, (297.6875, -1.5625)),
        // Not moving: lifted by 305 - 300.
        (304.5625, -0.4375, (299.5625, 0.0)),
        // More than a step below the top: no floor.
        (310.0, -2.0, (308.4375, -1.5625)),
        // Below the underside, it rises into the layer.
        (317.0, -2.0, (315.4375, -1.5625)),
    ] {
        let mut play = Play::new(&src);
        play.throw_to(21);
        assert_eq!(from(&mut play, 200.5, y, vy), (f(want.0), f(want.1)), "{y}");
    }
    // Moving down, it touches the top instead and bounces.
    let mut play = Play::new(&src);
    play.throw_to(21);
    let (y, vy) = from(&mut play, 200.5, 305.25, 2.0);
    assert_eq!(y, f(300.0));
    assert!(vy < Fixed::ZERO);
}

/// A ball placed out of the way, which tests put where they want with
/// [`put_ball`].
const BALL: &str = "[[entities]]\ntype = \"ball\"\nat = [1000, 100]";

/// Puts the ball (`things[1]`) at `pos`, falling at `vy` (before the
/// next step's gravity), after `steps` steps of its turn.
fn put_ball(play: &mut Play, pos: Vec2, vy: Fixed, steps: u32) {
    let ball = &mut play.world.things[1];
    ball.active = true;
    ball.pos = pos;
    ball.vx = Fixed::ZERO;
    ball.clock = steps;
    ball.phase = Phase::Flying {
        vy,
        ignoring: None,
        kept: false,
    };
}

/// thrown-objects.md, "Hitting a ball", "Example": a ball falling
/// straight down 74 px right of the object's start is hit on Q + 24,
/// not before, and the object goes on as without it.
#[test]
fn removes_a_ball_and_goes_on() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(start, Vec2::new(f(200.0), f(346.0)));
    // At the end of Q + 21, in the fourth part of its turn.
    put_ball(
        &mut play,
        Vec2::new(start.x + f(74.0), start.y - f(30.0)),
        f(3.0625),
        22,
    );
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    for (n, ball_y) in [(22, -26.5), (23, -22.5625), (24, 0.0), (25, 0.0)] {
        play.step(&[]);
        control.step(&[]);
        let ball = &play.world.things[1];
        assert_eq!(ball.active, n < 24, "Q + {n}");
        if ball.active {
            assert_eq!(ball.pos.y - start.y, f(ball_y), "Q + {n}");
        }
        assert!(play.world.things[0].active, "Q + {n}");
        assert_eq!(play.world.things[0].pos, control.world.things[0].pos);
        assert_eq!(play.world.things[0].phase, control.world.things[0].phase);
    }
    assert_eq!(play.world.things[0].pos.x - start.x, f(50.2109375));
}

/// Throws the crate, right or left, and before Q + `n` (from 22) puts
/// the ball, `steps` steps into its turn, `i` px left of where the
/// object ends the frame (I, both `x` rounded down) and `h` px below it
/// (H, both `y` rounded up). Returns whether the ball was hit on that
/// frame. Without a view, the ball isn't removed for leaving the
/// picture.
fn ball_hit(right: bool, n: usize, steps: u32, i: i64, h: i64) -> bool {
    let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
    if !right {
        play.world.player.facing = Facing::Left;
    }
    play.throw_to(n - 1);
    play.world.view = None;
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    control.step(&[]);
    let object = control.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    let x = object.x.floor() - i;
    let y = up(object.y) + h;
    put_ball(
        &mut play,
        Vec2::new(
            Fixed::from_int(x as i32) + f(0.75),
            Fixed::from_int(y as i32) - f(0.25),
        ),
        Fixed::ZERO,
        steps,
    );
    // Without the object, nothing else removes the ball.
    let mut alone = Play {
        world: play.world.clone(),
        held: play.held,
    };
    alone.world.things[0].active = false;
    alone.step(&[]);
    assert!(
        alone.world.things[1].active,
        "alone: right {right} Q + {n} steps {steps} I {i} H {h}"
    );
    play.step(&[]);
    assert_eq!(play.world.things[0].pos, object);
    assert!(play.world.things[0].active);
    !play.world.things[1].active
}

/// thrown-objects.md, "When a ball is hit": the ranges of I and H, ends
/// included, early (t up to 16) and late, moving right and left, in
/// each part of the ball's turn.
#[test]
fn hits_a_ball_in_the_measured_ranges() {
    for (right, n, (i_from, i_to), (h_from, h_to)) in [
        (true, 22, (-47, 27), (-54, 48)),
        (true, 37, (-47, 27), (-54, 48)),
        (true, 38, (-22, 26), (-30, 15)),
        (false, 22, (-28, 46), (-54, 48)),
        (false, 38, (-27, 21), (-30, 15)),
    ] {
        // Steps into the first to fourth parts, and the shifts of I
        // and H there.
        for (steps, di, dh) in [(0, 0, 0), (9, 0, -3), (15, 1, -3), (21, 1, 0)] {
            let (i_from, i_to, h_from, h_to) = (i_from + di, i_to + di, h_from + dh, h_to + dh);
            for (i, h, want) in [
                (i_from, 0, true),
                (i_to, 0, true),
                (i_from - 1, 0, false),
                (i_to + 1, 0, false),
                (0, h_from, true),
                (0, h_to, true),
                (0, h_from - 1, false),
                (0, h_to + 1, false),
            ] {
                let got = ball_hit(right, n, steps, i, h);
                assert_eq!(got, want, "right {right} Q + {n} steps {steps} I {i} H {h}");
            }
        }
    }
}

/// thrown-objects.md, "What happens on a hit": in the hands during a
/// throw, from Q + 1 to Q + 20, it breaks on a ball by the early
/// ranges and removes it, and the throw goes on as without it. Not on
/// Q or Q + 21.
#[test]
fn breaks_on_a_ball_in_the_hands() {
    for n in 0..=21 {
        let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
        if n > 0 {
            play.throw_to(n - 1);
        }
        let mut control = Play {
            world: play.world.clone(),
            held: play.held,
        };
        control.world.things[1].active = false;
        let mut ahead = Play {
            world: control.world.clone(),
            held: control.held,
        };
        ahead.step(&[]);
        let head = ahead.world.things[0].pos;
        // 47 px right of where the object ends the step, the early
        // range's end.
        put_ball(
            &mut play,
            Vec2::new(head.x + f(47.0), head.y),
            Fixed::ZERO,
            0,
        );
        play.step(&[]);
        control.step(&[]);
        let hit = (1..=20).contains(&n);
        assert_eq!(play.world.things[1].active, !hit, "Q + {n}");
        assert_eq!(play.world.things[0].active, !hit, "Q + {n}");
        if hit {
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.hands, control.world.player.hands);
            play.throw_to(30);
            control.throw_to(30);
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.state, control.world.player.state);
            assert!(!play.world.things[0].active);
        }
    }
}

/// thrown-objects.md, "What happens on a hit": carried, it breaks on a
/// ball by the late ranges, the way the player faces, and the player
/// carries nothing from then on.
#[test]
fn breaks_on_a_ball_while_carried() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
    play.world.player.facing = Facing::Left;
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    control.step(&[A]);
    let head = control.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    // Facing left, I of 22 isn't hit; 21, the late range's end, is.
    let at = |i: i64| {
        Vec2::new(
            Fixed::from_int((head.x.floor() - i) as i32),
            Fixed::from_int(up(head.y) as i32),
        )
    };
    let mut miss = Play {
        world: play.world.clone(),
        held: play.held,
    };
    put_ball(&mut miss, at(22), Fixed::ZERO, 0);
    miss.step(&[A]);
    assert!(miss.world.things[1].active);
    assert!(miss.world.things[0].active);
    put_ball(&mut play, at(21), Fixed::ZERO, 0);
    play.step(&[A]);
    assert!(!play.world.things[1].active);
    assert!(!play.world.things[0].active);
    assert!(play.world.player.hands.is_none());
    for _ in 0..30 {
        assert_eq!(play.world.player.pos, control.world.player.pos);
        play.step(&[]);
        control.step(&[]);
    }
    assert!(play.world.player.hands.is_none());
}

/// A 64 px vertical step up to the left at x = 100: floor at y = 336
/// left of it, the upper side, and at y = 400 from it on, the lower side.
const STEP_UP: &str = "[[collision]]\nshape = \"polygon\"\n\
    points = [[0, 600], [0, 336], [100, 336], [100, 400], [3000, 400], [3000, 600]]";

/// Puts the thrown object at `(x, y)` moving across at `vx` and down at
/// `vy` (before this step's gravity), and steps once. Returns whether
/// it's still there, and its `x`.
fn toward(play: &mut Play, x: f64, y: f64, vx: f64, vy: f64) -> (bool, Fixed) {
    let mut flight = play.object();
    (flight.vx, flight.target, flight.vy, flight.rolling) = (f(vx), f(vx), f(vy), false);
    play.world.things[0].pos = Vec2::new(f(x), f(y));
    play.world.things[0].phase = Phase::Thrown(flight);
    play.step(&[]);
    let thing = &play.world.things[0];
    (thing.active, thing.pos.x)
}

/// thrown-objects.md, "Breaking at a vertical step": moving into the
/// upper side, it breaks without moving if its `y` before the frame is
/// more than 8 px below the upper floor, whatever its `vy`; otherwise it
/// goes on into the column.
#[test]
fn breaks_at_a_vertical_step_more_than_8_px_below_its_top() {
    for vy in [-4.0, -1.0, 0.0, 1.0, 4.0] {
        for (y, breaks) in [
            (344.0625, true),
            (360.0, true),
            (399.0, true),
            (344.0, false),
        ] {
            let mut play = Play::new(&level(STEP_UP));
            play.throw_to(21);
            let (active, x) = toward(&mut play, 100.5, y, -1.0, vy);
            assert_eq!(active, !breaks, "y {y}, vy {vy}");
            if !breaks {
                assert_eq!(x, f(99.5), "y {y}, vy {vy}");
            }
        }
    }
    // Well above the upper floor, it flies on over it.
    let mut play = Play::new(&level(STEP_UP));
    play.throw_to(21);
    assert_eq!(toward(&mut play, 100.5, 300.0, -1.0, 0.0), (true, f(99.5)));
}

/// thrown-objects.md, "Breaking at a vertical step": it breaks on the
/// frame whose move would take `x` into the upper side's first column,
/// not on one ending on the left edge of the lower side's last column.
#[test]
fn breaks_at_a_vertical_step_only_once_inside_the_upper_side() {
    let mut play = Play::new(&level(STEP_UP));
    play.throw_to(21);
    assert_eq!(toward(&mut play, 100.5, 360.0, -0.5, 0.0), (true, f(100.0)));
    let mut play = Play::new(&level(STEP_UP));
    play.throw_to(21);
    assert!(!toward(&mut play, 100.5, 360.0, -0.5625, 0.0).0);
}

/// thrown-objects.md, "Breaking at a vertical step": an object in the
/// hands isn't broken by the step. Walking into it, the player is
/// stopped there; jumping, it gets onto the upper side.
#[test]
fn a_carried_object_isnt_broken_at_a_vertical_step() {
    const L: Action = Action::Left;
    const J: Action = Action::Jump;
    let mut play = Play::new(&level(STEP_UP));
    play.hold(&[A, L], 180);
    // Stopped at the left edge of its column (steep-faces.md, "Stopped
    // by a rise"): moving this fast, the one before the step's.
    assert_eq!(play.world.player.pos.x, f(101.0));
    assert_eq!(play.world.player.pos.y, f(400.0));
    assert!(play.world.player.hands.is_some());
    assert!(play.world.things[0].active);
    for _ in 0..4 {
        play.hold(&[A, L, J], 30);
        play.hold(&[A, L], 10);
    }
    assert!(play.world.player.pos.x < f(100.0));
    assert_eq!(play.world.player.pos.y, f(336.0));
    assert!(play.world.player.hands.is_some());
    assert!(play.world.things[0].active);
}

/// What a character's motion and its hurts are made of: the lead, the
/// other character, and the lives and hit points.
fn characters(play: &Play) -> String {
    let w = &play.world;
    format!("{:?} {:?} {:?} {:?}", w.player, w.other, w.flow, w.loss)
}

/// Steps 12 frames from `play` three ways: as it is, with the object gone,
/// and with `moved` (a character moved 200 px away) applied. Each
/// character moves as without the object, the object as without the
/// character, and no one is hurt.
fn passes_through(play: &Play, moved: impl Fn(&mut World), what: &str) {
    let mut with = Play {
        world: play.world.clone(),
        held: play.held,
    };
    let mut without = Play {
        world: play.world.clone(),
        held: play.held,
    };
    without.world.things[0].active = false;
    let mut away = Play {
        world: play.world.clone(),
        held: play.held,
    };
    moved(&mut away.world);
    for n in 1..=12 {
        with.step(&[]);
        without.step(&[]);
        away.step(&[]);
        assert_eq!(characters(&with), characters(&without), "{what}, frame {n}");
        let object = &with.world.things[0];
        assert_eq!(
            format!("{object:?}"),
            format!("{:?}", away.world.things[0]),
            "{what}, frame {n}"
        );
        assert!(with.world.hit_by.is_none(), "{what}, frame {n}");
    }
}

/// Puts the object `across` px from `at` and `up` px above it, moving
/// 3.5 px per frame toward `at` or away from it, in the air or rolling.
fn place(play: &mut Play, at: Vec2, across: i32, up: i32, toward: bool, rolling: bool) {
    let thing = &mut play.world.things[0];
    let Phase::Thrown(mut flight) = thing.phase else {
        panic!("{:?}", thing.phase);
    };
    thing.pos = Vec2::new(at.x + Fixed::from_int(across), at.y - Fixed::from_int(up));
    let right = (across < 0) == toward;
    flight.vx = if right { speed(896) } else { speed(-896) };
    flight.target = flight.vx;
    flight.rolling = rolling;
    if rolling {
        flight.vy = Fixed::ZERO;
    }
    thing.phase = Phase::Thrown(flight);
}

/// thrown-objects.md, "Meeting a character": a thrown object passes
/// through the lead, from t = 1 (Q + 22) while it's still in its throw,
/// standing facing either way or in a jump.
#[test]
fn passes_through_the_lead() {
    for t in [1, 9, 28, 40, 60] {
        for jump in [false, true] {
            for turn in [false, true] {
                let mut base = Play::new(&level(FLAT));
                base.throw_to(21 + t);
                if turn {
                    base.hold(&[Action::Left], 1);
                }
                if jump {
                    base.hold(&[Action::Jump], 6);
                }
                for across in (-24..=24).step_by(8) {
                    let ups: Vec<i32> = if jump {
                        (-48..=16).step_by(16).collect()
                    } else {
                        (0..=64).step_by(16).collect()
                    };
                    for up in ups {
                        for toward in [false, true] {
                            for rolling in [false, true] {
                                if rolling && (jump || up != 0) {
                                    continue;
                                }
                                let mut play = Play {
                                    world: base.world.clone(),
                                    held: base.held,
                                };
                                let at = play.world.player.pos;
                                place(&mut play, at, across, up, toward, rolling);
                                passes_through(
                                    &play,
                                    |w| w.player.pos.x += Fixed::from_int(200),
                                    &format!(
                                        "t = {t}, jump {jump}, turn {turn}, \
                                         {across} across, {up} up, toward {toward}, \
                                         rolling {rolling}"
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// thrown-objects.md, "Meeting a character": it passes through the
/// other character too, with the lead there, from t = 1 on.
#[test]
fn passes_through_the_other_character() {
    for t in [1, 9, 28, 40, 60] {
        let mut base = Play::with_other(&level(FLAT));
        base.throw_to(21 + t);
        for across in (-24..=24).step_by(8) {
            for up in (0..=64).step_by(16) {
                for toward in [false, true] {
                    for rolling in [false, true] {
                        if rolling && up != 0 {
                            continue;
                        }
                        let mut play = Play {
                            world: base.world.clone(),
                            held: base.held,
                        };
                        let at = play.world.other.as_ref().unwrap().pos;
                        place(&mut play, at, across, up, toward, rolling);
                        passes_through(
                            &play,
                            |w| w.other.as_mut().unwrap().pos.x += Fixed::from_int(200),
                            &format!(
                                "t = {t}, {across} across, {up} up, toward {toward}, \
                                 rolling {rolling}"
                            ),
                        );
                    }
                }
            }
        }
    }
}
