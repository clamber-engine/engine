//! Swimming in Classic 2 against the ground (spec/behavior/swimming.md,
//! "Walls, floors and ceilings (`title-2`)"): steps of up to 4 px, walls
//! that drop the fraction of `x`, landing on the surface with a margin on
//! sloped pieces, bumps under ground that isn't one-way, no push down,
//! and the swimmer held inside the picture.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::Fixed;
use clamber_sim::{InputFrame, State, World, generic_feel, step};

/// A level that is all water, with `collision` (TOML `[[collision]]`
/// tables) for its ground.
fn pool(collision: &str) -> Play {
    let src = format!(
        r#"
        format = 1
        name = "pool"
        size = [4096, 2000]
        start = [1000, 1000]

        [[regions]]
        rect = [0, 0, 4096, 2000]
        water = true

        {collision}
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "pool").unwrap());
    let feel = generic_feel("classic-2").unwrap();
    let world = World::new(level, Arc::new(feel));
    assert_eq!(world.player.state, State::Swim);
    Play {
        world,
        held: Actions::NONE,
    }
}

/// A solid rectangle, `extra` added to its table.
fn rect(x: i32, y: i32, w: i32, h: i32, extra: &str) -> String {
    format!("[[collision]]\nshape = \"rect\"\nrect = [{x}, {y}, {w}, {h}]\n{extra}\n")
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// One step with `keys` held, after what was held on the last.
    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// `vx` and `vy` in 1/256 px per frame.
    fn v(&self) -> (i64, i64) {
        let v = self.world.player.vel;
        (n256(v.x), n256(v.y))
    }

    fn x(&self) -> Fixed {
        self.world.player.pos.x
    }

    fn y(&self) -> Fixed {
        self.world.player.pos.y
    }

    fn on_floor(&self) -> bool {
        self.world.player.swim_floor
    }

    /// Puts the player at `(x, y)` with speeds `(vx, vy)` in 1/256 px per
    /// frame, and the picture's left edge 128 px left of it.
    fn put(&mut self, x: f64, y: f64, vx: i64, vy: i64) {
        let p = &mut self.world.player;
        p.pos.x = f(x);
        p.pos.y = f(y);
        p.vel.x = Fixed::from_bits(vx << 24);
        p.vel.y = Fixed::from_bits(vy << 24);
        self.picture(x.floor() as i32 - 128);
    }

    /// Puts the picture's left edge at `left`.
    fn picture(&mut self, left: i32) {
        let view = self.world.view.as_mut().unwrap();
        view.left = Fixed::from_int(left);
        view.ends = None;
    }

    /// Puts the player at `(x, y)` as [`Play::put`], with speeds before
    /// the frame that become `(vx, vy)` with `keys` held (`vx` 0, or
    /// ±384/256 with Left or Right), and steps.
    fn frame(&mut self, x: f64, y: f64, vx: i64, vy: i64, keys: &[Action]) {
        self.put(x, y, vx, vy - 12);
        self.step(keys);
    }
}

/// `v` in 1/256 px per frame, which it's always a whole number of.
fn n256(v: Fixed) -> i64 {
    assert_eq!(v.to_bits() % (1 << 24), 0, "{v:?}");
    v.to_bits() >> 24
}

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

const R: Action = Action::Right;
const L: Action = Action::Left;

#[test]
fn a_wall_drops_the_fraction_of_x() {
    // Column 101 has G = 192: U = 8.
    let mut play = pool(&rect(101, 192, 50, 100, ""));
    play.frame(100.25, 200.0, 384, 0, &[R]);
    assert_eq!((play.x(), play.y()), (f(100.0), f(200.0)));
    assert_eq!(play.v(), (0, 0));
    assert!(!play.on_floor());
}

#[test]
fn a_step_of_up_to_four_lifts_and_lands() {
    // Column 101 has G = 197: U = 3.
    let mut play = pool(&rect(101, 197, 50, 100, ""));
    play.frame(100.25, 200.5, 384, 0, &[R]);
    assert_eq!((play.x(), play.y()), (f(101.75), f(197.0)));
    assert_eq!(play.v(), (384, 0));
    assert!(play.on_floor());

    // U = 4 is still a step, 5 a wall.
    let mut play = pool(&rect(101, 196, 50, 100, ""));
    play.frame(100.25, 200.5, 384, 0, &[R]);
    assert_eq!((play.x(), play.y()), (f(101.75), f(196.0)));
    let mut play = pool(&rect(101, 195, 50, 100, ""));
    play.frame(100.25, 200.5, 384, 0, &[R]);
    assert_eq!((play.x(), play.y()), (f(100.0), f(200.5)));
}

#[test]
fn a_step_while_rising_lifts_and_the_rise_goes_on() {
    let mut play = pool(&rect(101, 197, 50, 100, ""));
    play.frame(100.25, 200.5, 384, -512, &[R]);
    assert_eq!((play.x(), play.y()), (f(101.75), f(195.5)));
    assert_eq!(play.v(), (384, -512));
    assert!(!play.on_floor());
}

#[test]
fn landing_on_a_flat_piece() {
    let ground = rect(90, 200, 40, 50, "");
    let mut play = pool(&ground);
    play.frame(100.25, 199.75, 0, 128, &[]);
    assert_eq!(play.y(), f(200.0));
    assert_eq!(play.v(), (0, 0));
    assert!(play.on_floor());

    let mut play = pool(&ground);
    play.frame(100.25, 199.25, 0, 128, &[]);
    assert_eq!(play.y(), f(199.75));
    assert_eq!(play.v(), (0, 128));
    assert!(!play.on_floor());
}

#[test]
fn landing_on_a_sloped_piece_from_three_above() {
    // Down 2 px a column over the piece of columns 96 to 111: at column
    // 104's middle the top is at 199.
    let slope = "[[collision]]\nshape = \"polygon\"\n\
                 points = [[96, 216], [112, 184], [112, 260], [96, 260]]\n";
    let mut play = pool(slope);
    play.frame(104.25, 195.75, 0, 128, &[]);
    assert_eq!(play.y(), f(199.0));
    assert_eq!(play.v(), (0, 0));
    assert!(play.on_floor());

    // Four above isn't a landing.
    let mut play = pool(slope);
    play.frame(104.25, 194.75, 0, 128, &[]);
    assert_eq!(play.y(), f(195.25));
    assert!(!play.on_floor());
}

#[test]
fn resting_keeps_y_on_the_surface() {
    let mut play = pool(&rect(90, 200, 40, 50, ""));
    play.put(100.0, 200.0, 0, 0);
    play.world.player.swim_floor = true;
    for _ in 0..20 {
        play.step(&[]);
        assert_eq!(play.y(), f(200.0));
        assert_eq!(play.v(), (0, 0));
        assert!(play.on_floor());
    }
}

#[test]
fn leaving_the_floor_without_a_stroke() {
    // Ground in columns 90 to 100 only.
    let mut play = pool(&rect(90, 200, 11, 50, ""));
    play.put(100.25, 200.0, 384, 0);
    play.world.player.swim_floor = true;
    play.step(&[R]);
    assert_eq!((play.x(), play.y()), (f(101.75), f(200.046875)));
    assert_eq!(play.v(), (384, 1));
    assert!(!play.on_floor());
    play.step(&[R]);
    assert_eq!(play.v(), (384, 13));
}

#[test]
fn rising_into_ground_bumps_unless_it_is_one_way() {
    // Ground from line 150 down to 209.
    let mut play = pool(&rect(90, 150, 40, 59, ""));
    play.frame(100.25, 210.25, 0, -384, &[]);
    assert_eq!(play.y(), f(210.75));
    assert_eq!(play.v(), (0, 0));

    let mut play = pool(&rect(90, 150, 40, 59, "one_way = true"));
    play.frame(100.25, 210.25, 0, -384, &[]);
    assert_eq!(play.y(), f(208.75));
    assert_eq!(play.v(), (0, -384));
}

#[test]
fn nothing_pushes_down_under_a_ceiling() {
    // Hanging ground whose underside is 1 px into the feet's row: across
    // into it is measured from its top, a wall.
    let mut play = pool(&rect(101, 150, 50, 51, "hangs = true"));
    play.frame(100.25, 200.0, 384, 0, &[R]);
    assert_eq!((play.x(), play.y()), (f(100.0), f(200.0)));
    assert_eq!(play.v(), (0, 0));
}

#[test]
fn in_the_ground_in_its_own_column_lifts_by_up_to_four() {
    let mut play = pool(&rect(90, 197, 40, 100, ""));
    play.frame(100.25, 200.0, 0, 0, &[]);
    assert_eq!((play.x(), play.y()), (f(100.25), f(197.0)));
    assert!(play.on_floor());

    // 5 or more is a wall: the fraction of x goes. Sinking, ⌊y⌋ is at G
    // or below, so the swimmer lands on the surface.
    let mut play = pool(&rect(90, 195, 40, 100, ""));
    play.frame(100.25, 200.0, 0, 0, &[]);
    assert_eq!((play.x(), play.y()), (f(100.0), f(195.0)));
    assert!(play.on_floor());
}

#[test]
fn held_sixteen_inside_each_side_of_the_picture() {
    let mut play = pool("");
    play.put(272.75, 1000.0, -384, -12);
    play.picture(256);
    play.step(&[L]);
    assert_eq!(play.x(), f(272.25));
    assert_eq!(play.v(), (-384, 0));

    play.put(496.25, 1000.0, 384, -12);
    play.picture(256);
    play.step(&[R]);
    assert_eq!(play.x(), f(496.75));
    assert_eq!(play.v(), (384, 0));
}
