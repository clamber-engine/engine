//! Swimming in Classic 1 (spec/behavior/swimming.md), inside a water
//! region.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::Fixed;
use clamber_sim::{Event, Facing, InputFrame, State, World, generic_feel, step};

/// A level that is all water, with `collision` (TOML `[[collision]]`
/// tables) for its ground, starting at `start`.
fn pool(collision: &str, start: (i32, i32)) -> Play {
    let src = format!(
        r#"
        format = 1
        name = "pool"
        size = [4096, 2000]
        start = [{}, {}]

        [[regions]]
        rect = [0, 0, 4096, 2000]
        water = true

        {collision}
        "#,
        start.0, start.1
    );
    let level = Arc::new(Level::from_toml_str(&src, "pool").unwrap());
    let feel = generic_feel("classic-1").unwrap();
    let world = World::new(level, Arc::new(feel));
    assert_eq!(world.player.state, State::Swim);
    Play {
        world,
        held: Actions::NONE,
    }
}

/// Open water, with no ground at all.
fn open() -> Play {
    pool("", (1000, 1000))
}

/// A solid rectangle.
fn rect(x: i32, y: i32, w: i32, h: i32) -> String {
    format!("[[collision]]\nshape = \"rect\"\nrect = [{x}, {y}, {w}, {h}]\n")
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

    /// Puts the player at `(x, y)` with speeds `(vx, vy)` in 1/256 px per
    /// frame.
    fn put(&mut self, x: f64, y: f64, vx: i64, vy: i64) {
        let p = &mut self.world.player;
        p.pos.x = f(x);
        p.pos.y = f(y);
        p.vel.x = Fixed::from_bits(vx << 24);
        p.vel.y = Fixed::from_bits(vy << 24);
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
const U: Action = Action::Up;
const D: Action = Action::Down;
const J: Action = Action::Jump;

#[test]
fn speed_across_eases_by_a_thirty_second_of_the_gap() {
    let mut play = open();
    let mut vxs = Vec::new();
    for _ in 0..106 {
        let x = play.x();
        play.step(&[R]);
        let vx = play.v().0;
        assert_eq!(play.x() - x, Fixed::from_bits(vx << 24));
        vxs.push(vx);
    }
    assert_eq!(vxs[..7], [16, 31, 46, 60, 74, 87, 100]);
    assert_eq!(vxs[104], 481);
    assert_eq!(vxs[105], 512);

    // Letting go: 496, 481, 466, … 32, 31, then 0, 106 frames to a stop.
    let mut vxs = Vec::new();
    while play.v().0 != 0 {
        play.step(&[]);
        vxs.push(play.v().0);
    }
    assert_eq!(vxs[..3], [496, 481, 466]);
    assert_eq!(vxs[vxs.len() - 3..], [32, 31, 0]);
    assert_eq!(vxs.len(), 106);

    // The opposite direction at full speed: through 0 to -2 in 128.
    for _ in 0..106 {
        play.step(&[R]);
    }
    assert_eq!(play.v().0, 512);
    let mut frames = 0;
    while play.v().0 != -512 {
        play.step(&[L]);
        frames += 1;
    }
    assert_eq!(frames, 128);
}

#[test]
fn facing_follows_left_or_right_at_once() {
    let mut play = open();
    for _ in 0..10 {
        play.step(&[R]);
    }
    assert_eq!(play.world.player.facing, Facing::Right);
    play.step(&[L]);
    assert_eq!(play.world.player.facing, Facing::Left);
    assert!(play.v().0 > 0);
    for keys in [&[][..], &[U], &[D], &[J]] {
        play.step(keys);
        assert_eq!(play.world.player.facing, Facing::Left);
    }
}

/// Sinking at 1.25 in open water, still.
fn sinking() -> Play {
    let mut play = open();
    for _ in 0..40 {
        play.step(&[]);
    }
    assert_eq!(play.v(), (0, 320));
    play
}

#[test]
fn worked_values() {
    let mut play = sinking();
    let rows: [(&[Action], i64, i64); 11] = [
        (&[J, R], 16, -500),
        (&[R], 31, -488),
        (&[R], 46, -476),
        (&[R], 60, -464),
        (&[R], 74, -452),
        (&[R], 87, -440),
        (&[R, D], 100, -428),
        (&[R, D], 112, -416),
        (&[R, D], 124, -404),
        (&[J, R, D], 136, -244),
        (&[R, D], 147, -232),
    ];
    for (i, (keys, vx, vy)) in rows.into_iter().enumerate() {
        let (x, y) = (play.x(), play.y());
        play.step(keys);
        assert_eq!(play.v(), (vx, vy), "P + {i}");
        assert_eq!(play.x() - x, Fixed::from_bits(vx << 24));
        assert_eq!(play.y() - y, Fixed::from_bits(vy << 24));
    }

    // Later, sinking at 2 with Right and Down held, until vx is 466.
    while play.v().0 != 466 {
        play.step(&[R, D]);
    }
    assert_eq!(play.v(), (466, 512));
    let rows: [(&[Action], i64, i64); 5] = [
        (&[R], 467, 320),
        (&[R], 468, 320),
        (&[R], 469, 320),
        (&[J, U], 455, -628),
        (&[], 441, -616),
    ];
    for (i, (keys, vx, vy)) in rows.into_iter().enumerate() {
        let (x, y) = (play.x(), play.y());
        play.step(keys);
        assert_eq!(play.v(), (vx, vy), "Q + {i}");
        assert_eq!(play.x() - x, Fixed::from_bits(vx << 24));
        assert_eq!(play.y() - y, Fixed::from_bits(vy << 24));
    }
}

#[test]
fn a_stroke_rises_for_a_while() {
    // Up for the press frame and the frames after it, how far in all,
    // and on which frame (the press is the 1st) `vy` reaches 1.25.
    for (with, rising, distance, limit_at) in [
        (None, 42, 41.671875, Some(70)),
        (Some(U), 53, 65.421875, None),
        (Some(D), 21, 10.171875, None),
    ] {
        let mut play = sinking();
        let start = play.y();
        let mut keys = vec![J];
        keys.extend(with);
        play.step(&keys);
        let mut frames = 1;
        while play.v().1 < 0 {
            play.step(&[]);
            frames += 1;
        }
        // The frame `vy` turns 0 or more doesn't move up.
        assert_eq!(frames - 1, rising, "{with:?}");
        let top = play.y() - Fixed::from_bits(play.v().1 << 24);
        assert_eq!(start - top, f(distance), "{with:?}");
        if let Some(limit_at) = limit_at {
            while play.v().1 < 320 {
                play.step(&[]);
                frames += 1;
            }
            assert_eq!(frames, limit_at);
        }
    }
}

#[test]
fn holding_jump_strokes_only_once_and_down_raises_the_limit() {
    let mut play = sinking();
    play.step(&[J]);
    play.step(&[J]);
    assert_eq!(play.v().1, -488);
    // Down while sinking: up to 2, and straight back to 1.25 without it.
    for _ in 0..100 {
        play.step(&[D]);
    }
    assert_eq!(play.v().1, 512);
    play.step(&[]);
    assert_eq!(play.v().1, 320);
}

#[test]
fn a_wall_stops_the_move_across_and_keeps_facing() {
    // Feet line 1000; column 101's floor is 8 px above it.
    let mut play = pool(&rect(101, 992, 50, 100), (100, 1000));
    // 132/256 eases to 128/256 = 0.5 with no direction held.
    play.put(100.75, 1000.0, 132, 0);
    play.world.player.facing = Facing::Left;
    play.step(&[]);
    assert_eq!(play.x(), f(100.25));
    assert_eq!(play.v().0, 0);
    // Pressing Right into it: no turn on the stopped frame.
    play.put(100.75, 1000.0, 132, 0);
    play.step(&[R]);
    assert_eq!(play.v().0, 0);
    assert_eq!(play.world.player.facing, Facing::Left);
    // Sinking goes on along the wall.
    assert!(play.v().1 > 0);
}

#[test]
fn a_step_up_keeps_the_speed() {
    let mut play = pool(&rect(101, 998, 50, 100), (100, 1000));
    // Rising a little, so sinking leaves `vy` at 0.
    play.put(100.75, 1000.0, 132, -12);
    play.step(&[]);
    assert_eq!(play.x(), f(101.25));
    assert_eq!(play.v().0, 128);
    // On the surface.
    assert_eq!(play.y(), f(998.0));
}

#[test]
fn moving_under_a_ceiling_pushes_down_to_two_below_its_line() {
    // The ceiling line, relative to the feet line 1000, and where the
    // feet line ends.
    for (line, feet) in [(-1, 1000.0), (0, 1002.0), (1, 1003.0), (2, 1004.0)] {
        let ceiling = rect(101, 980, 50, 20 + line);
        let mut play = pool(&ceiling, (100, 1000));
        // Rising a little, so sinking leaves `vy` at 0.
        play.put(100.75, 1000.0, 132, -12);
        play.step(&[]);
        assert_eq!(play.x(), f(101.25), "{line}");
        assert_eq!(play.y(), f(feet), "{line}");
    }
}

/// A solid rectangle that hangs, like a ceiling.
fn hanging(x: i32, y: i32, w: i32, h: i32) -> String {
    rect(x, y, w, h) + "hangs = true\n"
}

#[test]
fn hanging_ground_is_measured_from_its_underside() {
    // Column 101's ground from 998 to 1010; feet line 1000. Its top is
    // 2 px up, a step.
    let mut play = pool(&rect(101, 998, 50, 12), (100, 1000));
    play.put(100.75, 1000.0, 132, -12);
    play.step(&[]);
    assert_eq!((play.x(), play.y()), (f(101.25), f(998.0)));
    // Hanging, it's in by 10 to its underside: a wall.
    let mut play = pool(&hanging(101, 998, 50, 12), (100, 1000));
    play.put(100.75, 1000.0, 132, -12);
    play.step(&[]);
    assert_eq!((play.x(), play.v().0), (f(100.25), 0));
}

#[test]
fn with_hanging_ground_a_floor_is_measured_from_its_top() {
    // Column 101's ground from 990 to 1002; feet line 1000. Nearer its
    // underside, it's a ceiling coming down by 2, and pushes down.
    let mut play = pool(&rect(101, 990, 50, 12), (100, 1000));
    play.put(100.75, 1000.0, 132, -12);
    play.step(&[]);
    assert_eq!((play.x(), play.y()), (f(101.25), f(1004.0)));
    // In a level with hanging ground somewhere, it's a floor 10 px up: a
    // wall.
    let ground = rect(101, 990, 50, 12) + &hanging(2000, 0, 50, 50);
    let mut play = pool(&ground, (100, 1000));
    play.put(100.75, 1000.0, 132, -12);
    play.step(&[]);
    assert_eq!((play.x(), play.v().0), (f(100.25), 0));
}

#[test]
fn hanging_ground_on_a_floor_has_no_way_out_below() {
    // Hanging ground from 990 to 1000 resting on a floor to 1100; feet
    // line 999 is in it by 1 to its underside, but the floor goes on.
    let ground = hanging(101, 990, 50, 10) + &rect(101, 1000, 50, 100);
    let mut play = pool(&ground, (100, 999));
    play.put(100.75, 999.0, 132, -12);
    play.step(&[]);
    assert_eq!((play.x(), play.v().0), (f(100.25), 0));
}

#[test]
fn rising_into_a_ceiling_bumps() {
    // Ceiling line 990; feet line 991.
    let mut play = pool(&rect(80, 960, 100, 30), (100, 991));
    // -332/256 after sinking is -1.25.
    play.put(100.5, 991.0, 0, -332);
    play.step(&[]);
    assert_eq!(play.y(), f(990.75));
    assert_eq!(play.v().1, 0);
    play.step(&[]);
    assert_eq!(play.v().1, 12);
}

#[test]
fn landing_resting_and_leaving_the_floor() {
    // Floor at 1100 from x = 0 to 1200.
    let mut play = pool(&rect(0, 1100, 1200, 100), (1000, 1000));
    while play.v().1 != 0 || play.y() < f(1090.0) {
        play.step(&[]);
    }
    // Landed: the feet line on the floor, with the fraction kept.
    let y = play.y();
    assert_eq!(y.floor() + 1, 1100, "{y:?}");
    let fraction = y - Fixed::from_int(1099);
    assert!(fraction > Fixed::ZERO);
    assert!(play.world.player.swim_floor);
    // Resting: the height above the floor drops 12/256 a frame and
    // wraps round within the bottom pixel.
    let grain = Fixed::from_bits(12 << 24);
    let mut fraction = fraction;
    for _ in 0..30 {
        play.step(&[]);
        fraction += grain;
        if fraction > Fixed::from_int(1) {
            fraction -= Fixed::from_int(1);
        }
        assert_eq!(play.y(), Fixed::from_int(1099) + fraction);
        assert_eq!(play.v().1, 0);
    }
    // Swim right off the end of the floor: the first frame off moves
    // down 12/256 and ends at 1/256, then sinking goes on from there.
    let mut leaving = None;
    for _ in 0..200 {
        let y = play.y();
        play.step(&[R]);
        if !play.world.player.swim_floor {
            leaving = Some(play.y() - y);
            break;
        }
    }
    assert_eq!(leaving, Some(grain));
    assert_eq!(play.v().1, 1);
    play.step(&[R]);
    assert_eq!(play.v().1, 13);
    play.step(&[R]);
    assert_eq!(play.v().1, 25);
}

#[test]
fn a_stroke_leaves_the_floor_from_where_it_rests() {
    let mut play = pool(&rect(0, 1100, 1200, 100), (1000, 1000));
    while !play.world.player.swim_floor {
        play.step(&[]);
    }
    let y = play.y();
    play.step(&[J]);
    assert_eq!(play.y() - y, Fixed::from_bits(-500 << 24));
}

#[test]
fn the_left_end_stops_a_swimmer_below_eighteen() {
    let mut play = pool("", (30, 1000));
    for _ in 0..200 {
        play.step(&[L]);
    }
    assert_eq!(play.x().floor(), 17);
    play.world.player.facing = Facing::Right;
    play.step(&[L]);
    assert_eq!(play.v().0, 0);
    // A stopped frame doesn't turn.
    assert_eq!(play.world.player.facing, Facing::Right);
    play.step(&[R]);
    assert!(play.v().0 > 0);
}

#[test]
fn starting_in_the_ground_moves_up_without_moving_across() {
    // In by 2 at the start: up 2, `x` kept.
    let mut play = pool(&rect(0, 998, 1200, 100), (100, 998));
    play.put(100.5, 1000.0, 132, -12);
    play.step(&[]);
    assert_eq!(play.x(), f(100.5));
    assert_eq!(play.v().0, 128);
    assert_eq!(play.y(), f(998.0));
    // In by 4 or more: nothing.
    play.put(100.5, 1010.0, 132, -12);
    play.step(&[]);
    assert_eq!(play.x(), f(100.5));
    assert_eq!(play.y(), f(1010.0));
}

#[test]
fn out_of_the_water_the_player_falls() {
    let src = r#"
        format = 1
        name = "shore"
        size = [4096, 2000]
        start = [100, 1000]

        [[regions]]
        rect = [0, 900, 4096, 1100]
        water = true
    "#;
    let level = Arc::new(Level::from_toml_str(src, "shore").unwrap());
    let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    let mut play = Play {
        world,
        held: Actions::NONE,
    };
    play.step(&[]);
    assert_eq!(play.world.player.state, State::Swim);
    for _ in 0..100 {
        if play.y() < f(900.0) {
            break;
        }
        play.step(&[J, U]);
        play.step(&[]);
    }
    play.step(&[]);
    assert_eq!(play.world.player.state, State::Fall);
}

impl Play {
    fn events(&self) -> &[Event] {
        &self.world.events
    }
}

#[test]
fn trace_events_while_swimming() {
    // Floor at 1100 from x = 0 to 1200.
    let mut play = pool(&rect(0, 1100, 1200, 100), (1000, 1000));
    let mut lands = 0;
    while !play.world.player.swim_floor {
        play.step(&[]);
        assert_eq!(play.world.player.state, State::Swim);
        if play.world.player.swim_floor {
            assert_eq!(play.events(), [Event::Land]);
            lands += 1;
        } else {
            assert_eq!(play.events(), []);
        }
    }
    assert_eq!(lands, 1);
    // Resting frames have no events.
    for _ in 0..10 {
        play.step(&[]);
        assert_eq!(play.events(), []);
    }
    // A stroke from the floor is a stroke, not leaving the floor.
    play.step(&[J]);
    assert_eq!(play.events(), [Event::Stroke]);
    assert!(!play.world.player.swim_floor);
    play.step(&[J]);
    assert_eq!(play.events(), []);
    // Back down, then swim off the end of the floor: one leave_ground,
    // on the frame it's no longer on the floor.
    while !play.world.player.swim_floor {
        play.step(&[]);
    }
    let mut leaves = 0;
    for _ in 0..400 {
        let was = play.world.player.swim_floor;
        play.step(&[R]);
        let left_floor = was && !play.world.player.swim_floor;
        assert_eq!(play.events().contains(&Event::LeaveGround), left_floor);
        if left_floor {
            assert_eq!(play.v().1, 1);
            leaves += 1;
        }
    }
    assert_eq!(leaves, 1);
    // A turn on every frame facing changes, and never a land state's
    // events.
    play.step(&[L]);
    assert_eq!(play.events(), [Event::Turn]);
    play.step(&[L]);
    assert_eq!(play.events(), []);
    play.step(&[R, J]);
    assert_eq!(play.events(), [Event::Stroke, Event::Turn]);
}

#[test]
fn a_turn_the_left_end_stops_is_no_turn() {
    let mut play = open();
    play.put(10.5, 1000.0, -64, 0);
    assert_eq!(play.world.player.facing, Facing::Right);
    play.step(&[L]);
    assert_eq!(play.v().0, 0);
    assert_eq!(play.world.player.facing, Facing::Right);
    assert_eq!(play.events(), []);
}

#[test]
fn the_synthetic_trace_example() {
    // Resting on the floor facing left, with the fraction of its height
    // at least 24/256.
    let mut play = pool(&rect(0, 1100, 1200, 100), (1000, 1000));
    play.step(&[L]);
    while !play.world.player.swim_floor {
        play.step(&[]);
    }
    let frac = |play: &Play| n256(play.y() - Fixed::from_int(play.y().floor() as i32));
    while !(24..=255 - 24).contains(&frac(&play)) {
        play.step(&[]);
    }
    play.world.player.vel.x = Fixed::ZERO;
    let y0 = play.y();
    // (input, y, vy, ground, events), y in 1/256 px from the start.
    type Row<'a> = (&'a [Action], i64, i64, bool, &'a [Event]);
    let rows: [Row; 4] = [
        (&[], 12, 0, true, &[]),
        (&[], 24, 0, true, &[]),
        (&[J], -476, -500, false, &[Event::Stroke]),
        (&[J], -964, -488, false, &[]),
    ];
    for (frame, (keys, y, vy, ground, events)) in rows.into_iter().enumerate() {
        play.step(keys);
        assert_eq!(n256(play.y() - y0), y, "frame {frame}");
        assert_eq!(play.v(), (0, vy), "frame {frame}");
        assert_eq!(play.world.player.state, State::Swim);
        assert_eq!(play.world.player.swim_floor, ground, "frame {frame}");
        assert_eq!(play.events(), events, "frame {frame}");
        assert_eq!(play.world.player.facing, Facing::Left);
    }
}
