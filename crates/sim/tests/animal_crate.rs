//! The animal crate in Classic 1: breaking it, the animal coming out,
//! and mounting it (spec/behavior/animal-crate.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{Event, Facing, InputFrame, Phase, State, World, generic_feel, step};

const L: Action = Action::Left;
const R: Action = Action::Right;

/// animal-crate.md's "Example": a crate at (1000, 400) on a floor at
/// level y 400, with the player starting well away from it.
const LEVEL: &str = r#"
format = 1
name = "animal-crate"
size = [3000, 800]
start = [300, 400]

[[collision]]
shape = "rect"
rect = [0, 400, 3000, 200]

[[entities]]
type = "hutch"
at = [1000, 400]
"#;

const XC: i64 = 1000;
const YC: i64 = 400;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new() -> Play {
        let level = Arc::new(Level::from_toml_str(LEVEL, "animal-crate").unwrap());
        let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
        Play {
            world,
            held: Actions::NONE,
        }
    }

    /// Puts the player in the air in a jump at `(x, y)`, moving at `vx`
    /// and `vy`, facing `facing`, with Jump not held.
    fn fall_from(&mut self, x: f64, y: f64, vx: f64, vy: f64, facing: Facing) {
        let player = &mut self.world.player;
        player.pos = Vec2::new(f(x), f(y));
        player.vel = Vec2::new(f(vx), f(vy));
        player.facing = facing;
        player.ground = None;
        player.state = State::Fall;
        player.jump_age = Some(20);
        player.jump_hold = false;
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

    fn player(&self) -> (Fixed, Fixed, Fixed, Fixed) {
        let p = &self.world.player;
        (p.pos.x, p.pos.y, p.vel.x, p.vel.y)
    }

    fn phase(&self) -> Phase {
        self.world.things[0].phase
    }
}

/// The example's break, facing left with Left held, then the animal
/// coming out and standing.
#[test]
fn the_example_breaks_the_crate_and_the_animal_comes_out() {
    let mut play = Play::new();
    play.fall_from(1016.5, 359.21875, -2.0, 5.78125, Facing::Left);
    // Frame 2: the crate breaks and the player bounces off it.
    play.step(&[L]);
    assert_eq!(play.player(), (f(1014.5), f(365.4375), f(-2.0), f(-7.125)));
    assert!(play.world.events.contains(&Event::Bounce));
    assert_eq!(play.phase(), Phase::Opened(0));
    // Frames 3 and 4 are frozen.
    for _ in 3..=4 {
        play.step(&[L]);
        assert_eq!(play.player(), (f(1014.5), f(365.4375), f(-2.0), f(-7.125)));
    }
    play.step(&[L]);
    let (x, y, _, vy) = play.player();
    assert_eq!((x, y, vy), (f(1012.5), f(358.75), f(-6.6875)));
    // Out on B + 18 = frame 20, hopping, then standing from frame 24.
    play.hold(&[], 14);
    let animal = |play: &Play| play.world.things[0].pos;
    assert_eq!(animal(&play), Vec2::ints(1000, 400));
    for (frame, y) in [
        (20, 398.4375),
        (21, 400.0 - 2.6875),
        (22, 400.0 - 3.375),
        (23, 400.0 - 3.625),
        (24, 400.0),
        (25, 400.0),
    ] {
        play.step(&[]);
        assert_eq!(animal(&play), Vec2::new(f(1000.0), f(y)), "frame {frame}");
    }
    // It breaks only once.
    assert!(matches!(play.phase(), Phase::Opened(_)));
}

/// Breaks a crate, then waits away from it for the animal to stand.
fn opened() -> Play {
    let mut play = Play::new();
    play.fall_from(1016.5, 359.21875, -2.0, 5.78125, Facing::Left);
    play.step(&[]);
    play.fall_from(600.0, 380.0, 0.0, 1.0, Facing::Right);
    play.hold(&[], 40);
    assert!(play.world.player.grounded());
    assert_eq!(play.phase(), Phase::Opened(22));
    play
}

/// The example's mount: facing right, `vx` 2, Right held.
#[test]
fn the_example_mounts_the_animal() {
    let mut play = opened();
    play.fall_from(988.25, 353.5, 2.0, 5.8125, Facing::Right);
    play.step(&[R]);
    assert_eq!(play.phase(), Phase::Mounted(0));
    assert_eq!(play.player(), (f(1000.25), f(399.75), f(2.0), f(0.0)));
    // Still for 22 frames, whatever is pressed.
    for n in 1..=22 {
        let keys: &[Action] = match n {
            5 => &[Action::Jump],
            10.. => &[L],
            _ => &[R],
        };
        play.step(keys);
        assert_eq!(
            play.player(),
            (f(1000.25), f(399.75), f(2.0), f(0.0)),
            "M + {n}"
        );
    }
    // Riding from the 23rd: the feet on the crate's position, on the
    // ground.
    play.step(&[]);
    assert_eq!(play.world.player.pos.y, f(400.0));
    assert!(play.world.player.grounded());
    assert_eq!(play.phase(), Phase::Ridden);
}

/// A player too high up a frame before doesn't mount (`D` = 46).
#[test]
fn too_high_doesnt_mount() {
    let mut play = opened();
    play.fall_from(986.25, 347.6875, 2.0, 5.375, Facing::Right);
    play.step(&[R]);
    assert_eq!(play.player().1, f(353.5));
    assert!(matches!(play.phase(), Phase::Opened(_)));
}

/// The player's offsets from the crate where it would be at the end of
/// a frame without it, and whether the crate broke on that frame, for a
/// player put in the air `(across, above)` from it, facing `facing`,
/// falling at `vy`, rising if negative.
fn try_break(across: i64, above: i64, vy: f64, facing: Facing) -> ((i64, i64), bool) {
    let mut play = Play::new();
    let x = (XC + across) as f64 + 0.5;
    let y = (YC - above) as f64 - 0.25;
    play.fall_from(x, y, 0.0, vy, facing);
    let mut without = Play::new();
    without.fall_from(x, y, 0.0, vy, facing);
    without.world.things[0].active = false;
    play.step(&[]);
    without.step(&[]);
    let p = without.world.player.pos;
    let offsets = (p.x.floor() - XC, YC + (-p.y).floor());
    (offsets, play.phase() != Phase::Ready)
}

#[test]
fn the_crate_breaks_within_the_ranges_for_the_facing() {
    for (facing, (from, to)) in [(Facing::Right, (-28, 40)), (Facing::Left, (-42, 26))] {
        for across in from - 3..=to + 3 {
            for above in [-2, 0, 4, 8, 20, 40, 44] {
                let ((i, d), broke) = try_break(across, above, 2.0, facing);
                let want = (from..=to).contains(&i) && (5..=36).contains(&d);
                assert_eq!(broke, want, "{facing:?} I {i} D {d}");
            }
        }
    }
}

#[test]
fn a_rising_player_doesnt_break_the_crate() {
    for across in [-20, 0, 20] {
        let (_, broke) = try_break(across, 30, -3.0, Facing::Right);
        assert!(!broke);
    }
}

#[test]
fn walking_through_the_crate_does_nothing() {
    let mut play = Play::new();
    play.world.player.pos = Vec2::ints(940, 400);
    play.hold(&[R], 120);
    assert!(play.world.player.pos.x > f(1060.0));
    assert_eq!(play.phase(), Phase::Ready);
}

/// The animal's mount ranges, with the player in a jump: the left range
/// is the right one moved 1 px right.
#[test]
fn the_animal_is_mounted_within_the_ranges_for_the_facing() {
    for (facing, (from, to)) in [(Facing::Right, (-27, 19)), (Facing::Left, (-26, 20))] {
        for across in from - 2..=to + 2 {
            for above in [3, 6, 30, 44, 50] {
                let mut play = opened();
                let x = (XC + across) as f64 + 0.5;
                let y = (YC - above) as f64 - 0.25;
                play.fall_from(x, y, 0.0, 2.0, facing);
                let mut without = opened();
                without.fall_from(x, y, 0.0, 2.0, facing);
                without.world.things[0].active = false;
                play.step(&[]);
                without.step(&[]);
                let p = without.world.player.pos;
                let (i, d) = (p.x.floor() - XC, YC + (-p.y).floor());
                let want = (from..=to).contains(&i) && (5..=45).contains(&d);
                let mounted = play.phase() == Phase::Mounted(0);
                assert_eq!(mounted, want, "{facing:?} I {i} D {d}");
            }
        }
    }
}

/// Falling without a jump, as after walking off a ledge, doesn't mount.
#[test]
fn a_fall_without_a_jump_doesnt_mount() {
    let mut play = opened();
    play.fall_from(1000.5, 370.0, 0.0, 2.0, Facing::Right);
    play.world.player.jump_age = None;
    play.world.player.walked_off = true;
    play.step(&[]);
    assert!(matches!(play.phase(), Phase::Opened(_)));
}
