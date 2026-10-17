//! Exits in walls in Classic 1, opened by a thrown barrel
//! (spec/behavior/wall-exits.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{
    Archetypes, Behavior, Facing, FlowEvent, InputFrame, Outcome, Phase, State, World,
    generic_feel, step,
};

const R: Action = Action::Right;
const L: Action = Action::Left;
const D: Action = Action::Down;
const J: Action = Action::Jump;
const A: Action = Action::Action;

/// The wall exit's place in most tests: on the floor.
const XE: i32 = 500;
const YE: i32 = 449;

/// Flat ground with its top at y = 449 from x = 0 to 3000, the lead
/// standing at `start` on it, a wall exit at `wall` (entity 0), then
/// `more` entities.
fn world(start: i32, wall: (i32, i32), more: &str) -> World {
    let (xe, ye) = wall;
    let src = format!(
        r#"
        format = 1
        name = "wall-exit"
        size = [3000, 700]
        start = [{start}, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]

        [[entities]]
        type = "wall-exit"
        at = [{xe}, {ye}]
        {more}
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "wall-exit").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

/// A crate in reach right of a lead standing at `start`.
fn with_crate(start: i32, wall: (i32, i32)) -> World {
    let at = start + 20;
    world(
        start,
        wall,
        &format!("[[entities]]\ntype = \"crate\"\nat = [{at}, 449]"),
    )
}

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn fx(n: i32) -> Fixed {
    Fixed::from_int(n)
}

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

    /// The lead standing at `x` facing `facing`, with the wall exit at
    /// `wall` already open.
    fn open(x: i32, facing: Facing, wall: (i32, i32)) -> Play {
        let mut play = Play::new(world(x, wall, ""));
        play.world.things[0].phase = Phase::Open;
        play.world.player.facing = facing;
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

    /// Picks up the crate standing still and carries it, ready to let go
    /// of action on the next step, Q.
    fn pick_up(&mut self) {
        self.hold(&[], 2);
        self.hold(&[A], 21);
        assert!(self.world.player.hands.is_some());
    }

    fn taken(&self) -> bool {
        self.world.leaving.is_some()
    }

    fn opened(&self) -> bool {
        self.world.things[0].phase == Phase::Open
    }

    fn x(&self) -> i64 {
        self.world.player.pos.x.floor()
    }

    /// How far left of the wall exit the lead is: `D`.
    fn d(&self) -> i64 {
        i64::from(XE) - self.x()
    }
}

/// The table in "Opening": whether a barrel `t` frames out of the hands
/// at `I` and `U` from the wall exit hits it.
fn in_range(t: u8, i: i64, u: Fixed) -> bool {
    if t <= 16 {
        (-49..=25).contains(&i) && u >= fx(-50) && u < fx(67)
    } else {
        (-24..=24).contains(&i) && u >= fx(-26) && u < fx(34)
    }
}

#[test]
fn the_crate_opens_wall_exits_by_the_measured_ranges() {
    let resolved = Archetypes::builtin()
        .resolve(
            &clamber_level::Entity {
                kind: "crate".into(),
                id: None,
                at: Vec2::ints(0, 0),
                params: Default::default(),
            },
            "test",
            "e",
        )
        .unwrap()
        .unwrap();
    let Behavior::Carry {
        hit_early_time,
        opens: Some(opens),
        ..
    } = resolved.behavior
    else {
        panic!("{:?}", resolved.behavior);
    };
    assert_eq!(hit_early_time, 16);
    let (xe, ye) = (500, 300);
    let wall = Vec2::ints(xe, ye);
    for (t, hit) in [(16, opens.hit_early), (17, opens.hit)] {
        // Object positions 1/16 px apart in y and with a fraction in x,
        // around both ranges.
        for i in -60..=60 {
            for sixteenths in -70 * 16..=70 * 16 {
                let u = Fixed::from_ratio_pow2(sixteenths, 4);
                let object = Vec2::new(fx(xe + i) + f(0.6875), fx(ye) - u);
                let i = i64::from(i);
                assert_eq!(
                    hit.holds(object, true, wall),
                    in_range(t, i, u),
                    "t {t}, I {i}, U {u:?}"
                );
            }
        }
    }
}

/// "Throws by inputs alone", with a made-up wall exit on flat ground: a
/// barrel thrown right opens it on the first frame its end-of-frame
/// position is in range, and not before.
#[test]
fn a_barrel_thrown_right_opens_it_on_the_first_frame_in_range() {
    let mut opened_at = Vec::new();
    for xe in [260, 300, 340, 380, 440, 480, 520] {
        let mut play = Play::new(with_crate(200, (xe, 449)));
        play.pick_up();
        play.step(&[]);
        let mut was = None;
        for _ in 0..100 {
            play.step(&[]);
            let object = &play.world.things[1];
            let Phase::Thrown(flight) = object.phase else {
                // Still in the hands.
                assert!(object.active && was.is_none());
                assert!(!play.opened());
                continue;
            };
            let i = object.pos.x.floor() - i64::from(xe);
            let u = fx(449) - object.pos.y;
            let hit = in_range(flight.age, i, u);
            let want = was.is_some() || hit;
            assert_eq!(play.opened(), want, "xe {xe}, t {}", flight.age);
            if hit && was.is_none() {
                was = Some(flight.age);
            }
            // The object isn't stopped by the hit.
            assert!(flight.vx > Fixed::ZERO);
        }
        opened_at.push(was.unwrap_or_else(|| panic!("xe {xe}: never opened")));
    }
    // Both the larger early range and the later one were used.
    assert!(opened_at.iter().any(|&t| t <= 16), "{opened_at:?}");
    assert!(opened_at.iter().any(|&t| t > 16), "{opened_at:?}");
}

#[test]
fn a_barrel_thrown_left_or_carried_never_opens_it() {
    // Thrown left, from right of the wall exit.
    let mut play = Play::new(world(
        600,
        (XE, 449),
        "[[entities]]\ntype = \"crate\"\nat = [580, 449]",
    ));
    play.world.player.facing = Facing::Left;
    play.pick_up();
    for _ in 0..120 {
        play.step(&[]);
        assert!(!play.opened());
    }
    // Carried right up to it, and stood with next to it.
    let mut play = Play::new(with_crate(400, (XE, 449)));
    play.pick_up();
    for _ in 0..120 {
        play.step(&[R, A]);
        assert!(play.world.player.hands.is_some());
        assert!(!play.opened());
    }
    assert!(play.x() > i64::from(XE - 30), "{}", play.x());
}

#[test]
fn a_closed_wall_exit_never_takes_over() {
    let runs: [&[&[Action]]; 4] = [
        &[&[R]],
        &[&[R], &[R, A], &[R], &[]],
        &[&[R, J], &[R], &[D], &[]],
        &[&[R, D], &[D], &[], &[L]],
    ];
    for keys in runs {
        let mut play = Play::new(world(XE - 80, (XE, YE), ""));
        for n in 0..200 {
            play.step(keys[n % keys.len()]);
            assert!(!play.taken());
        }
        assert_eq!(play.world.things[0].phase, Phase::Ready);
    }
}

#[test]
fn standing_reaches() {
    for (facing, reach) in [(Facing::Right, 29), (Facing::Left, 35)] {
        for (d, taken) in [(reach, true), (reach + 1, false), (0, true), (-3, true)] {
            let mut play = Play::open(XE - d, facing, (XE, YE));
            play.step(&[]);
            assert_eq!(play.taken(), taken, "{facing:?} at {d}");
            if taken {
                assert_eq!(play.world.leaving.unwrap().way, Facing::Right);
            }
        }
    }
    // Not from as far right as the area is left from.
    let mut play = Play::open(XE + 4, Facing::Right, (XE, YE));
    play.step(&[]);
    assert!(!play.taken());
}

#[test]
fn the_vertical_range() {
    // The lead stands with y = 449: y - ye from -39 to 35.
    for (ye, taken) in [(488, true), (489, false), (414, true), (413, false)] {
        let mut play = Play::open(XE - 20, Facing::Right, (XE, ye));
        play.step(&[]);
        assert_eq!(play.taken(), taken, "y - ye = {}", 449 - ye);
    }
}

#[test]
fn walking_right_is_taken_over_at_29() {
    let mut play = Play::open(XE - 100, Facing::Right, (XE, 420));
    let mut before = None;
    while !play.taken() {
        before = Some(play.d());
        play.step(&[R]);
        assert!(play.world.frame < 200);
    }
    assert!(before.unwrap() > 29, "{before:?}");
    assert!(play.d() <= 29, "{}", play.d());
    assert_eq!(play.world.player.pos.y, fx(421));
}

#[test]
fn rolling_is_taken_over_within_27() {
    let mut play = Play::open(XE - 60, Facing::Right, (XE, YE));
    play.step(&[R, A]);
    let mut before = play.d();
    while !play.taken() {
        before = play.d();
        play.step(&[R]);
        assert!(play.world.frame < 200);
    }
    let p = &play.world.player;
    assert!(p.state == State::Roll || p.roll.is_some(), "{:?}", p.state);
    assert!(before > 27, "{before}");
    assert!(play.d() <= 27, "{}", play.d());
}

#[test]
fn crouching_is_taken_over_within_32_sliding_or_not() {
    for sliding in [true, false] {
        for (d, taken) in [(32, true), (33, false)] {
            let mut play = Play::open(XE - 200, Facing::Right, (XE, YE));
            play.step(&[]);
            let vx = if sliding { f(0.5) } else { Fixed::ZERO };
            let p = &mut play.world.player;
            p.vel.x = vx;
            p.target = vx;
            // Ends the frame at D, whether it slides on 0.5 or not.
            p.pos.x = fx(XE - d) - vx + f(0.25);
            play.step(&[D]);
            let p = &play.world.player;
            assert!(p.crouch.held > 0, "{:?}", p.crouch);
            assert_eq!(p.vel.x > Fixed::ZERO, sliding, "{:?}", p.vel);
            assert_eq!(play.d(), i64::from(d));
            assert_eq!(play.taken(), taken, "sliding {sliding}, D {d}");
        }
    }
}

#[test]
fn moving_left_is_never_taken_over() {
    let mut play = Play::open(XE + 2, Facing::Left, (XE, YE));
    for _ in 0..12 {
        play.step(&[L]);
        assert!(!play.taken());
    }
    assert!(play.d() > 0 && play.d() <= 35, "{}", play.d());
    assert!(play.world.player.vel.x < Fixed::ZERO);
    // Let go: taken over on the frame the speed comes to 0.
    while play.world.player.vel.x < Fixed::ZERO {
        assert!(!play.taken());
        play.step(&[]);
    }
    assert!(play.taken());
}

#[test]
fn in_the_air_only_on_landing() {
    let mut play = Play::open(XE - 20, Facing::Right, (XE, YE));
    play.world.player.pos.y = fx(400);
    play.world.player.ground = None;
    while !play.world.player.grounded() {
        assert!(!play.taken());
        play.step(&[]);
        assert!(play.world.frame < 200);
    }
    assert!(play.world.frame > 2);
    assert!(play.taken(), "taken over on the landing frame");
}

/// "Example": T from x 472.25 standing (D = 28, but 30 isn't in reach);
/// F = T + 17, and T + 47 is the last frame in the area, at 564.25.
#[test]
fn spec_example_numbers() {
    let mut play = Play::open(XE - 100, Facing::Right, (XE, YE));
    play.step(&[]);
    play.world.player.pos.x = f(470.5);
    play.step(&[]);
    assert!(!play.taken(), "30 px left standing isn't in reach");
    play.world.player.pos.x = f(472.25);
    play.step(&[]);
    assert!(play.taken());
    let t = play.world.player.pos;
    assert_eq!(t, Vec2::new(f(472.25), fx(YE + 1)));

    // T + 1: still, at 2.
    play.step(&[]);
    assert_eq!(play.world.player.pos, t);
    assert_eq!(play.world.player.vel.x, fx(2));

    let view = |play: &Play| play.world.view.map(|v| (v.x(), v.y()));
    let mut k = 1;
    let held = loop {
        k += 1;
        let before = view(&play);
        play.step(&[]);
        let p = &play.world.player;
        assert_eq!(p.pos, Vec2::new(t.x + fx(2 * (k - 1)), t.y), "T + {k}");
        assert_eq!(p.state, State::Walk);
        let fade = play.world.leaving.unwrap().fade;
        assert_eq!(fade.is_some(), k >= 17, "F = T + 17, at T + {k}");
        if k == 17 {
            assert_eq!(p.pos.x, f(504.25));
            assert_eq!(view(&play), before, "the view holds still on F");
            break view(&play);
        }
    };
    assert_eq!(play.world.brightness(), (15, 15));
    for j in 1..=30u32 {
        assert!(!play.world.flow.outcome.is_over(), "F + {j}");
        play.step(&[L, J, A]);
        assert_eq!(play.world.brightness(), (15 - j.div_ceil(2), 15), "F + {j}");
        assert_eq!(view(&play), held);
    }
    assert_eq!(play.world.player.pos.x, f(564.25));
    assert_eq!(play.world.flow.outcome, Outcome::Exited(0));
    assert!(play.world.flow.events.contains(&FlowEvent::Exited(0)));
}

#[test]
fn a_lead_facing_left_turns() {
    let mut play = Play::open(XE - 30, Facing::Left, (XE, YE));
    play.step(&[]);
    assert!(play.taken());
    for k in 1..=8 {
        play.step(&[]);
        let p = &play.world.player;
        assert_eq!(p.turn.is_some(), k >= 2, "T + {k}");
        let facing = if k < 6 { Facing::Left } else { Facing::Right };
        assert_eq!(p.facing, facing, "T + {k}");
    }
}

#[test]
fn walks_into_the_rise_and_through_it() {
    // A rise right of the wall exit, as in the first level.
    let src = format!(
        r#"
        format = 1
        name = "wall-exit"
        size = [3000, 700]
        start = [{start}, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]

        [[collision]]
        shape = "rect"
        rect = [{rise}, 300, 200, 149]

        [[entities]]
        type = "wall-exit"
        at = [{XE}, 447]
        "#,
        start = XE - 60,
        rise = XE - 4,
    );
    let level = Arc::new(Level::from_toml_str(&src, "wall-exit").unwrap());
    let mut play = Play::new(World::new(
        level,
        Arc::new(generic_feel("classic-1").unwrap()),
    ));
    // Closed, the rise stops the lead.
    play.hold(&[R], 60);
    assert!(!play.taken());
    assert!(play.x() < i64::from(XE - 4));
    play.world.things[0].phase = Phase::Open;
    play.step(&[R]);
    assert!(play.taken());
    assert_eq!(play.world.player.pos.y, fx(448));
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
    }
    assert!(play.x() > i64::from(XE + 60), "{}", play.x());
    assert_eq!(play.world.player.pos.y, fx(448));
}

#[test]
fn nothing_held_after_t_has_any_effect() {
    let run = |keys: &[&[Action]]| {
        let mut play = Play::open(XE - 100, Facing::Right, (XE, YE));
        while !play.taken() {
            play.step(&[R]);
        }
        let mut seen = Vec::new();
        let mut n = 0;
        while !play.world.flow.outcome.is_over() {
            play.step(keys[n % keys.len()]);
            n += 1;
            let p = &play.world.player;
            seen.push((p.pos, p.vel, p.facing, play.world.brightness()));
        }
        seen
    };
    let none = run(&[&[]]);
    let busy = run(&[&[L], &[L, J], &[R, A], &[D], &[J, A], &[R], &[], &[D, A]]);
    assert_eq!(none, busy);
}
