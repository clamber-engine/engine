//! Leaving an area through an exit in Classic 1
//! (spec/behavior/exits.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::Fixed;
use clamber_sim::{Facing, FlowEvent, InputFrame, Outcome, State, World, generic_feel, step};

const R: Action = Action::Right;
const L: Action = Action::Left;
const D: Action = Action::Down;
const J: Action = Action::Jump;
const A: Action = Action::Action;

/// Flat ground with its top at y = 449 from x = 0 to 3000, the lead
/// standing at `start` on it, and an exit at `exit`, on `preset`.
fn world(preset: &str, start: i32, exit: (i32, i32)) -> World {
    let (xe, ye) = exit;
    let src = format!(
        r#"
        format = 1
        name = "exit"
        size = [3000, 700]
        start = [{start}, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]

        [[entities]]
        type = "exit"
        at = [{xe}, {ye}]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "exit").unwrap());
    World::new(level, Arc::new(generic_feel(preset).unwrap()))
}

/// The exit's place in most tests: on the floor.
const XE: i32 = 300;
const YE: i32 = 449;

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

    /// The lead standing at `x`, facing `facing`, after settling on the
    /// ground with the exit out of the way, then the exit at `exit`.
    fn standing(x: i32, facing: Facing, exit: (i32, i32)) -> Play {
        let mut play = Play::new(world("classic-1", x, exit));
        play.world.player.facing = facing;
        play
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// Holds `keys` until an exit takes over the lead, at most `most`
    /// frames, and says whether one did.
    fn until_taken(&mut self, keys: &[Action], most: usize) -> bool {
        for _ in 0..most {
            self.step(keys);
            if self.taken() {
                return true;
            }
        }
        false
    }

    fn taken(&self) -> bool {
        self.world.leaving.is_some()
    }

    fn x(&self) -> i64 {
        self.world.player.pos.x.floor()
    }

    /// How far right of the exit the lead is: `I`.
    fn i(&self) -> i64 {
        self.x() - i64::from(XE)
    }
}

fn fx(n: i32) -> Fixed {
    Fixed::from_int(n)
}

#[test]
fn walking_left_is_taken_over_at_31_and_walks_out() {
    let mut play = Play::new(world("classic-1", 400, (XE, 420)));
    let mut before = None;
    while !play.taken() {
        before = Some((play.i(), play.world.player.vel.x));
        play.step(&[L]);
        assert!(play.world.frame < 200);
    }
    let (i_before, _) = before.unwrap();
    assert!(i_before > 31, "{i_before}");
    assert!(play.i() <= 31, "{}", play.i());
    assert_eq!(play.world.player.facing, Facing::Left);
    // At the end of T, y is the exit's plus 1, whatever it was.
    let at = play.world.player.pos;
    assert_eq!(at.y, fx(421));

    // T + 1: still, at -2.
    play.step(&[L]);
    let p = &play.world.player;
    assert_eq!(p.pos, at);
    assert_eq!(p.vel.x, fx(-2));

    // From T + 2: 2 px left a frame, y as it is.
    let mut k = 1;
    loop {
        k += 1;
        play.step(&[L]);
        let p = &play.world.player;
        assert_eq!(p.pos.x, at.x - fx(2 * (k - 1)));
        assert_eq!(p.pos.y, at.y);
        assert_eq!(p.state, State::Walk);
        if play.world.leaving.unwrap().fade.is_some() {
            break;
        }
        assert!(play.x() > i64::from(XE - 4));
    }
    assert!(play.x() <= i64::from(XE - 4));
}

#[test]
fn the_view_holds_and_the_picture_fades() {
    let mut play = Play::new(world("classic-1", 400, (XE, 420)));
    assert!(play.until_taken(&[L], 200));
    let x_t = play.world.player.pos.x;
    assert_eq!(play.world.player.pos.y, fx(421));
    let view = |play: &Play| play.world.view.map(|v| (v.x(), v.y()));

    // F is the first frame ending with x rounded down at 296 or less.
    let mut k = 0;
    let f = loop {
        k += 1;
        let before = view(&play);
        play.step(&[]);
        let x = play.world.player.pos.x;
        let want = if k == 1 { x_t } else { x_t - fx(2 * (k - 1)) };
        assert_eq!(x, want, "T + {k}");
        if x.floor() <= 296 {
            assert_eq!(view(&play), before, "the view holds still on F");
            break k;
        }
        assert_eq!(play.world.brightness(), (15, 15));
    };
    let x_f = play.world.player.pos.x;
    let held = view(&play);

    // Brightness 15 on F, then 15 - j on F + 2j - 1 and F + 2j; the area's
    // last frame is F + 30.
    assert_eq!(play.world.brightness(), (15, 15));
    for j in 1..=30u32 {
        assert!(!play.world.flow.outcome.is_over(), "F + {j}");
        play.step(&[R, J, A]);
        assert_eq!(play.world.brightness(), (15 - j.div_ceil(2), 15), "F + {j}");
        assert_eq!(play.world.player.pos.x, x_f - fx(2 * j as i32));
        assert_eq!(view(&play), held);
    }
    assert_eq!(play.world.flow.outcome, Outcome::Exited(0));
    assert!(play.world.flow.events.contains(&FlowEvent::Exited(0)));
    // The world stands still after its last frame.
    let last = play.world.player.pos;
    play.step(&[L]);
    assert_eq!(play.world.player.pos, last);
    assert!(f >= 2, "{f}");
}

#[test]
fn spec_example_numbers() {
    // From x 331.75 on T, F is T + 19 and the last frame T + 49 at 235.75.
    let mut play = Play::new(world("classic-1", 400, (XE, 420)));
    play.step(&[]);
    let p = &mut play.world.player;
    p.facing = Facing::Left;
    p.pos.x = Fixed::from_f64(333.5).unwrap();
    play.step(&[]);
    assert!(!play.taken(), "33 px right standing isn't in reach");
    play.world.player.pos.x = Fixed::from_f64(331.75).unwrap();
    play.step(&[]);
    assert!(play.taken());
    assert_eq!(play.world.player.pos.x, Fixed::from_f64(331.75).unwrap());
    let mut frames = 0;
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        frames += 1;
        if frames == 19 {
            assert_eq!(play.world.leaving.unwrap().fade, Some(0), "F = T + 19");
            assert_eq!(play.world.player.pos.x, Fixed::from_f64(295.75).unwrap());
        }
    }
    assert_eq!(frames, 49);
    assert_eq!(play.world.player.pos.x, Fixed::from_f64(235.75).unwrap());
}

#[test]
fn standing_reaches() {
    for (facing, reach) in [(Facing::Left, 31), (Facing::Right, 37)] {
        for (i, taken) in [(reach, true), (reach + 1, false), (0, true), (-3, true)] {
            let mut play = Play::standing(XE + i, facing, (XE, YE));
            play.step(&[]);
            assert_eq!(play.taken(), taken, "{facing:?} at {i}");
        }
    }
    // Not from as far left as the area is left from.
    let mut play = Play::standing(XE - 4, Facing::Left, (XE, YE));
    play.step(&[]);
    assert!(!play.taken());
}

#[test]
fn the_vertical_range() {
    // The lead stands with y = 449.
    for (ye, taken) in [(488, true), (489, false), (414, true), (413, false)] {
        let mut play = Play::standing(XE + 36, Facing::Right, (XE, ye));
        play.step(&[]);
        assert_eq!(play.taken(), taken, "y - ye = {}", 449 - ye);
    }
}

#[test]
fn rolling_is_taken_over_within_29() {
    let mut play = Play::new(world("classic-1", XE + 50, (XE, YE)));
    play.world.player.facing = Facing::Left;
    play.hold_until_rolling();
    assert!(play.i() > 29);
    let mut before = play.i();
    while !play.taken() {
        before = play.i();
        play.step(&[L]);
        assert!(play.world.frame < 200);
    }
    let p = &play.world.player;
    assert!(p.state == State::Roll || p.roll.is_some(), "{:?}", p.state);
    assert!(before > 29, "{before}");
    assert!(play.i() <= 29, "{}", play.i());
}

impl Play {
    fn hold_until_rolling(&mut self) {
        self.step(&[L, A]);
        for _ in 0..3 {
            if self.world.player.state == State::Roll || self.world.player.roll.is_some() {
                return;
            }
            self.step(&[L]);
        }
        panic!("no roll");
    }
}

#[test]
fn a_crouching_slide_is_taken_over_within_34() {
    // Walk left at speed, then crouch, ending a frame sliding at each of
    // 35 and 34 in turn.
    for (i, taken) in [(34, true), (35, false)] {
        let mut play = Play::new(world("classic-1", 600, (XE, YE)));
        play.world.player.facing = Facing::Left;
        play.step(&[L]);
        let p = &mut play.world.player;
        p.vel.x = Fixed::from_f64(-0.5).unwrap();
        p.target = p.vel.x;
        p.pos.x = fx(XE + i) + Fixed::from_f64(0.75).unwrap();
        play.step(&[D]);
        let p = &play.world.player;
        assert!(p.crouch.held > 0 && p.vel.x < Fixed::ZERO, "{:?}", p.vel);
        assert_eq!(play.i(), i64::from(i));
        assert_eq!(play.taken(), taken, "{i}");
    }
}

#[test]
fn moving_right_is_never_taken_over() {
    let mut play = Play::new(world("classic-1", XE - 2, (XE, YE)));
    for _ in 0..12 {
        play.step(&[R]);
        assert!(!play.taken());
    }
    assert!(play.i() > 0 && play.i() <= 37, "{}", play.i());
    assert!(play.world.player.vel.x > Fixed::ZERO);
    // Let go: taken over on the frame the speed comes to 0.
    while play.world.player.vel.x > Fixed::ZERO {
        assert!(!play.taken());
        play.step(&[]);
    }
    assert!(play.taken());
}

#[test]
fn in_the_air_only_on_landing() {
    // A fall straight down, in reach all along.
    let mut play = Play::standing(XE + 20, Facing::Left, (XE, YE));
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

#[test]
fn a_lead_facing_right_turns() {
    let mut play = Play::standing(XE + 36, Facing::Right, (XE, YE));
    play.step(&[]);
    assert!(play.taken());
    for k in 1..=8 {
        play.step(&[]);
        let p = &play.world.player;
        assert_eq!(p.turn.is_some(), k >= 2, "T + {k}");
        let facing = if k < 6 { Facing::Right } else { Facing::Left };
        assert_eq!(p.facing, facing, "T + {k}");
    }
}

#[test]
fn nothing_held_after_t_has_any_effect() {
    let run = |keys: &[&[Action]]| {
        let mut play = Play::new(world("classic-1", 400, (XE, YE)));
        assert!(play.until_taken(&[L], 200));
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
    let busy = run(&[&[R], &[R, J], &[L, A], &[D], &[J, A], &[L], &[], &[D, A]]);
    assert_eq!(none, busy);
}

#[test]
fn walks_through_walls_and_past_the_left_end() {
    // An exit near the level's left end, with a wall left of it.
    let src = r#"
        format = 1
        name = "exit"
        size = [3000, 700]
        start = [60, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]

        [[collision]]
        shape = "rect"
        rect = [10, 300, 8, 149]

        [[entities]]
        type = "exit"
        at = [40, 449]
        "#;
    let level = Arc::new(Level::from_toml_str(src, "exit").unwrap());
    let mut play = Play::new(World::new(
        level,
        Arc::new(generic_feel("classic-1").unwrap()),
    ));
    play.world.player.facing = Facing::Left;
    assert!(play.until_taken(&[L], 50));
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
    }
    assert!(play.x() < 0, "{}", play.x());
}

#[test]
fn presets_without_exits_ignore_them() {
    let mut play = Play::new(world("default", XE + 10, (XE, YE)));
    play.world.player.facing = Facing::Left;
    for _ in 0..60 {
        play.step(&[L]);
        assert!(!play.taken());
    }
    assert!(play.x() < i64::from(XE - 20));
}
