//! Riding the animal in Classic 1: on flat floor
//! (spec/behavior/riding.md) and on 1-in-4 slopes
//! (spec/behavior/riding-slopes.md). Positions in the spec are made
//! relative; here the floor is at y = 300, and on flat floor `x` starts at
//! 1000.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    Behavior, Facing, Fixed, InputFrame, Phase, RidePart, Riding, State, World, generic_feel, step,
};

const L: Action = Action::Left;
const R: Action = Action::Right;
const J: Action = Action::Jump;
const A: Action = Action::Action;

const FLOOR: i32 = 300;
const X0: f64 = 1000.0;

fn fx(x: f64) -> Fixed {
    Fixed::from_f64(x).unwrap()
}

/// 1/256 px per frame.
fn grains(v: Fixed) -> i64 {
    v.to_bits() >> 24
}

fn from_grains(n: i64) -> Fixed {
    Fixed::from_bits(n << 24)
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// A `classic-1` level 1200 px wide whose floor in column `c` is
    /// `height(c)` px below y = 300, with an animal crate near its left
    /// end, and the player riding at `(x, y)` below that line at `vx` (in
    /// 1/256), facing `facing`, on the floor, with `held` held on the frame
    /// before.
    fn on(
        height: impl Fn(i32) -> i32,
        (x, y): (f64, f64),
        vx: i64,
        facing: Facing,
        held: &[Action],
    ) -> Play {
        let width = 1200;
        let mut points: Vec<[i32; 2]> = (0..width)
            .flat_map(|c| [[c, FLOOR + height(c)], [c + 1, FLOOR + height(c)]])
            .collect();
        points.extend([[width, 580], [0, 580]]);
        points.dedup();
        let src = format!(
            "format = 1\nname = \"riding\"\nsize = [{width}, 600]\nstart = [{x}, {}]\n\
             [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n\
             [[entities]]\ntype = \"hutch\"\nat = [40, {}]\n",
            FLOOR + height(x as i32),
            FLOOR + height(40),
        );
        let level = Level::from_toml_str(&src, "riding").unwrap();
        let mut world = World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        );
        let Some(Behavior::Hutch(hutch)) = world.things[0].archetype.map(|a| a.behavior) else {
            panic!("no animal crate");
        };
        let player = &mut world.player;
        player.pos = Vec2::new(fx(x), fx(y) + Fixed::from_int(FLOOR));
        player.vel.x = from_grains(vx);
        player.facing = facing;
        assert!(player.grounded());
        player.riding = Some(Riding {
            on_floor: true,
            mount: None,
            ..Riding::new(hutch.ride, Fixed::ZERO)
        });
        Play {
            world,
            held: held.iter().copied().collect(),
        }
    }

    /// Riding on flat floor at `x` = 1000.
    fn flat(vx: i64, facing: Facing, held: &[Action]) -> Play {
        Play::on(|_| 0, (X0, 0.0), vx, facing, held)
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// `x`, `y` below the floor line, `vx` and `vy` in 1/256.
    fn row(&self) -> (Fixed, Fixed, i64, i64) {
        let p = &self.world.player;
        (
            p.pos.x,
            p.pos.y - Fixed::from_int(FLOOR),
            grains(p.vel.x),
            grains(p.vel.y),
        )
    }

    /// The same with `x` relative to 1000.
    fn flat_row(&self) -> (Fixed, Fixed, i64, i64) {
        let (x, y, vx, vy) = self.row();
        (x - fx(X0), y, vx, vy)
    }

    fn part(&self) -> RidePart {
        self.world.player.riding.unwrap().part
    }
}

#[test]
fn speeding_up_from_standing_still() {
    let mut play = Play::flat(0, Facing::Right, &[]);
    for (frame, vx, x) in [
        (0, 16, 0.0625),
        (1, 31, 0.18359375),
        (2, 46, 0.36328125),
        (3, 60, 0.59765625),
    ] {
        play.step(&[R]);
        let (got_x, _, got_vx, _) = play.flat_row();
        assert_eq!((got_x, got_vx), (fx(x), vx), "frame {frame}");
    }
    for _ in 4..=104 {
        play.step(&[R]);
        assert!(play.row().2 < 512);
    }
    play.step(&[R]);
    assert_eq!(play.flat_row(), (fx(147.5546875), Fixed::ZERO, 512, 0));
    assert_eq!(play.part(), RidePart::Riding);
}

#[test]
fn slowing_down_with_nothing_held() {
    let mut play = Play::flat(512, Facing::Right, &[R]);
    for vx in [496, 481, 466, 452] {
        play.step(&[]);
        assert_eq!(play.row().2, vx);
    }
    for _ in 4..=105 {
        play.step(&[]);
    }
    assert_eq!(play.flat_row(), (fx(64.4453125), Fixed::ZERO, 0, 0));
}

#[test]
fn speeding_up_with_action() {
    let mut play = Play::flat(0, Facing::Right, &[]);
    for vx in [28, 55, 81, 106] {
        play.step(&[R, A]);
        assert_eq!(play.row().2, vx);
    }
    for frame in 4..=123 {
        play.step(&[R, A]);
        assert_eq!(play.row().2 == 896, frame == 123, "frame {frame}");
        if frame == 123 {
            break;
        }
    }
}

#[test]
fn turning_at_speed() {
    let mut play = Play::flat(456, Facing::Right, &[R]);
    for (frame, vx) in [(0, 426), (1, 397), (2, 369), (3, 342)] {
        play.step(&[L]);
        assert_eq!(play.row().2, vx, "frame {frame}");
        assert_eq!(play.part(), RidePart::Turn(frame));
        assert_eq!(play.world.player.facing, Facing::Right);
    }
    play.step(&[]);
    assert_eq!(play.world.player.facing, Facing::Left);
    for _ in 5..=7 {
        play.step(&[]);
        assert!(matches!(play.part(), RidePart::Turn(_)));
    }
    play.step(&[]);
    assert_eq!(play.part(), RidePart::Riding);
}

/// riding.md's jump while moving left: Jump pressed on frame 0 only.
#[test]
fn the_example_jump() {
    let mut play = Play::flat(-422, Facing::Left, &[L]);
    let mut rows = Vec::new();
    for frame in 0..=44 {
        play.step(if frame == 0 { &[L, J] } else { &[L] });
        rows.push((play.flat_row(), play.part()));
    }
    let row = |frame: usize| {
        let ((_, y, vx, vy), part) = rows[frame];
        (vx, y, vy, part)
    };
    assert_eq!(row(0), (-424, fx(0.0), 0, RidePart::Jump(0)));
    assert_eq!(row(1), (-435, fx(0.0), 0, RidePart::Jump(1)));
    assert_eq!(row(2), (-444, fx(0.0), 0, RidePart::Jump(2)));
    assert_eq!(row(4), (-459, fx(0.0), 0, RidePart::Jump(4)));
    assert_eq!(row(5), (-465, fx(0.0), -7 * 256, RidePart::Jump(5)));
    assert_eq!(row(6).0, -470);
    assert_eq!((row(6).1, row(6).2), (fx(-6.5625), -1680));
    assert_eq!((row(21).0, row(21).1, row(21).2), (-503, fx(-52.5), 0));
    assert_eq!(row(35).1, fx(-6.5625));
    assert_eq!(row(36), (-512, fx(0.0), 0, RidePart::Landing(0)));
    assert_eq!(row(43).3, RidePart::Landing(7));
    assert_eq!(row(44), (-512, fx(0.0), 0, RidePart::Riding));
    assert!(play.world.player.grounded());
}

/// riding.md's charge from standing still, facing left, with Right held.
#[test]
fn the_example_charge() {
    let mut play = Play::flat(0, Facing::Left, &[]);
    let mut rows = Vec::new();
    for frame in 0..=40 {
        play.step(if frame == 0 { &[R, A] } else { &[R] });
        rows.push((play.flat_row(), play.part(), play.world.player.facing));
    }
    let row = |frame: usize| {
        let ((_, y, vx, vy), part, facing) = rows[frame];
        assert_eq!(facing, Facing::Left, "frame {frame}");
        (vx, y, vy, part)
    };
    let charge = |age| RidePart::Charge {
        age,
        steered: false,
    };
    let end = |age| RidePart::ChargeEnd {
        age,
        steered: false,
    };
    assert_eq!(row(0), (28, fx(0.0), 0, charge(0)));
    assert_eq!(row(1), (43, fx(0.0), 0, charge(1)));
    assert_eq!(row(10), (155, fx(0.0), -4 * 256, charge(10)));
    assert_eq!(row(11), (166, fx(-3.5625), -912, charge(11)));
    assert_eq!(row(26), (292, fx(0.0), 0, charge(26)));
    assert_eq!(row(28), (304, fx(0.0), 0, end(0)));
    assert_eq!(row(39), (361, fx(0.0), 0, end(11)));
    // The settling frame: Right is held and the rider faces left, so a
    // turn starts.
    assert_eq!(row(40), (365, fx(0.0), 0, RidePart::Turn(0)));
    // The hop's highest point.
    let top = (0..=40).map(|f| rows[f].0.1).min().unwrap();
    assert_eq!(top, fx(-16.3125));
    assert_eq!(rows[19].0.1, top);
}

/// A plain charge from riding: the hop leaves on C + 11 and lands on
/// C + 26, the charge end is C + 28 to C + 39, and C + 40 settles.
#[test]
fn a_charge_hops_lands_and_settles() {
    let mut play = Play::flat(512, Facing::Right, &[R]);
    let mut parts = Vec::new();
    let mut grounded = Vec::new();
    for frame in 0..=41 {
        play.step(if frame == 0 { &[R, A] } else { &[R] });
        parts.push(play.part());
        grounded.push(play.world.player.grounded());
    }
    let air: Vec<_> = (0..=41).filter(|&f| !grounded[f]).collect();
    assert_eq!(air, (11..=25).collect::<Vec<_>>());
    assert_eq!(
        parts[28],
        RidePart::ChargeEnd {
            age: 0,
            steered: false
        }
    );
    assert_eq!(parts[40], RidePart::Settling);
    assert_eq!(parts[41], RidePart::Riding);
}

/// A jump from riding lands on frame 36 with Jump pressed on 0 only; the
/// landing frame from the example.
const LANDING: usize = 36;

/// Jumps on frame 0, then presses Jump on frame `press` and holds it.
/// Returns the frames on which a jump started after the first.
fn jump_then_press(press: usize) -> Vec<usize> {
    let mut play = Play::flat(0, Facing::Right, &[]);
    let mut starts = Vec::new();
    for frame in 0..=60 {
        let keys: &[Action] = if frame == 0 || frame >= press {
            &[J]
        } else {
            &[]
        };
        play.step(keys);
        if frame > 0 && play.part() == RidePart::Jump(0) {
            starts.push(frame);
        }
        if frame == LANDING {
            assert_eq!(play.part(), RidePart::Landing(0));
        }
    }
    starts
}

#[test]
fn a_press_made_in_the_air_is_remembered_for_11_frames() {
    assert_eq!(jump_then_press(LANDING - 10), [LANDING + 1]);
    assert_eq!(jump_then_press(LANDING - 11), Vec::<usize>::new());
    // Pressed on the landing frame, which is steered: the frame after.
    assert_eq!(jump_then_press(LANDING), [LANDING + 1]);
}

#[test]
fn holding_jump_never_starts_another_jump() {
    let mut play = Play::flat(0, Facing::Right, &[]);
    for frame in 0..=80 {
        play.step(&[J]);
        if frame > 0 {
            assert_ne!(play.part(), RidePart::Jump(0), "frame {frame}");
        }
    }
}

/// Jump held from the press uses the lighter gravity from T + 1.
#[test]
fn a_held_jump_uses_the_held_gravity() {
    let mut play = Play::flat(0, Facing::Right, &[]);
    for _ in 0..=5 {
        play.step(&[J]);
    }
    play.step(&[J]);
    assert_eq!(play.row().3, -7 * 256 + 72);
}

/// Jump and Action pressed together while riding give a steered charge.
#[test]
fn jump_and_action_together_give_a_steered_charge() {
    let mut play = Play::flat(256, Facing::Right, &[R]);
    play.step(&[R, J, A]);
    assert_eq!(
        play.part(),
        RidePart::Charge {
            age: 0,
            steered: true
        }
    );
    // Steered from the next frame: Left sets facing at once.
    play.step(&[L]);
    assert_eq!(play.world.player.facing, Facing::Left);
    assert!(!matches!(play.part(), RidePart::Turn(_)));
}

/// riding-slopes.md's jump going downhill.
#[test]
fn a_jump_going_downhill() {
    let height = |c: i32| if c < 100 { 0 } else { (c - 100) / 4 + 1 };
    let mut play = Play::on(height, (115.640625, 4.0), 512, Facing::Right, &[R]);
    let mut rows = Vec::new();
    for frame in 146..=153 {
        play.step(if frame == 146 { &[R, J] } else { &[R] });
        let (x, y, _, vy) = play.row();
        rows.push((x, y, vy));
    }
    let row = |frame: usize| rows[frame - 146];
    assert_eq!(row(146), (fx(117.640625), fx(5.0), 0));
    assert_eq!(row(150), (fx(125.640625), fx(7.0), 0));
    assert_eq!(row(151), (fx(127.640625), fx(7.0), -7 * 256));
    assert_eq!(row(152), (fx(129.640625), fx(0.4375), -168 * 10));
    assert_eq!(row(153), (fx(131.640625), fx(-5.6875), -1568));
}

/// riding-slopes.md's charge going uphill.
#[test]
fn a_charge_going_uphill() {
    let height = |c: i32| 11 - (c - 208).div_euclid(4);
    let mut play = Play::on(height, (189.640625, 16.0), 512, Facing::Right, &[R]);
    let mut rows = Vec::new();
    for frame in 183..=211 {
        play.step(if frame == 183 { &[R, A] } else { &[R] });
        let (x, y, vx, vy) = play.row();
        rows.push((x, y, vx, vy, play.part(), play.world.player.grounded()));
    }
    let row = |frame: usize| rows[frame - 183];
    let at = |frame: usize| {
        let (x, y, _, vy, _, _) = row(frame);
        (x, y, vy)
    };
    assert_eq!(row(183).2, 524);
    assert_eq!(row(184).2, 512);
    assert_eq!(at(183), (fx(191.6875), fx(16.0), 0));
    assert_eq!(at(192), (fx(209.6875), fx(11.0), 0));
    assert_eq!(at(193), (fx(211.6875), fx(11.0), -4 * 256));
    assert_eq!(at(194), (fx(213.6875), fx(6.4375), -912));
    assert_eq!(at(195), (fx(215.6875), fx(3.3125), -800));
    assert_eq!(at(206), (fx(237.6875), fx(-2.1875), 432));
    assert_eq!(at(207), (fx(239.6875), fx(4.0), 0));
    assert!(row(207).5);
    assert_eq!(at(211), (fx(247.6875), fx(2.0), 0));
    assert_eq!(
        row(211).4,
        RidePart::ChargeEnd {
            age: 0,
            steered: false
        }
    );
}

/// Riding down a slope at the fastest speed follows each column.
#[test]
fn riding_follows_a_slope_down_at_full_speed() {
    let height = |c: i32| if c < 100 { 0 } else { (c - 100) / 4 + 1 };
    let mut play = Play::on(height, (60.0, 0.0), 896, Facing::Right, &[R, A]);
    for _ in 0..100 {
        play.step(&[R, A]);
        let (x, y, _, vy) = play.row();
        assert!(play.world.player.grounded());
        assert_eq!((y, vy), (Fixed::from_int(height(x.floor() as i32)), 0));
    }
}

/// Off a ledge with no floor below, the rider leaves the floor as on a
/// walk-off's leave frame, and falls.
#[test]
fn riding_off_a_ledge_drops_and_falls() {
    let height = |c: i32| if c < 1010 { 0 } else { 200 };
    let mut play = Play::on(height, (1006.0, 0.0), 512, Facing::Right, &[R]);
    play.step(&[R]);
    assert!(play.world.player.grounded());
    play.step(&[R]);
    assert!(!play.world.player.grounded());
    assert_eq!(play.row().1, fx(3.4375));
    assert_eq!(play.row().3, 1);
    play.step(&[R]);
    assert_eq!(play.row().3, 113);
    assert_eq!(play.world.player.state, State::Fall);
}

/// Mounting through the crate: riding starts on M + 23, with the speed
/// from the mount, and a Jump press then doesn't start a jump.
#[test]
fn riding_starts_after_the_mount() {
    let mut play = Play::flat(0, Facing::Left, &[]);
    play.world.player.riding = None;
    // Break the crate, at (40, 300), and wait for the animal.
    let player = &mut play.world.player;
    player.pos = Vec2::new(fx(56.5), fx(259.21875));
    player.vel = Vec2::new(fx(-2.0), fx(5.78125));
    player.ground = None;
    player.state = State::Fall;
    player.jump_age = Some(20);
    play.step(&[]);
    assert_eq!(play.world.things[0].phase, Phase::Opened(0));
    // Away from it, so the bounce doesn't mount it.
    let player = &mut play.world.player;
    player.pos = Vec2::new(fx(300.0), fx(290.0));
    player.vel = Vec2::new(Fixed::ZERO, fx(1.0));
    player.jump_age = None;
    for _ in 0..60 {
        play.step(&[]);
    }
    // Mount it.
    let player = &mut play.world.player;
    player.pos = Vec2::new(fx(28.25), fx(253.5));
    player.vel = Vec2::new(fx(2.0), fx(5.8125));
    player.facing = Facing::Right;
    player.ground = None;
    player.state = State::Fall;
    player.jump_age = Some(20);
    play.step(&[R]);
    assert_eq!(play.world.things[0].phase, Phase::Mounted(0));
    for _ in 1..=22 {
        play.step(&[R]);
    }
    assert!(play.world.player.riding.is_some());
    play.step(&[R, J]);
    assert_eq!(play.part(), RidePart::Riding);
    assert_eq!(play.row().2, 512);
    assert_eq!(play.world.player.pos.x, fx(42.25));
    assert_eq!(play.world.things[0].pos, play.world.player.pos);
    play.step(&[R]);
    play.step(&[R, J]);
    assert_eq!(play.part(), RidePart::Jump(0));
}

/// riding-faces.md's floor for "Examples", in level y: 33 up to column
/// 99, a face falling to the right at columns 100 to 107 (36, 40, …, 64),
/// and no floor from column 108 on.
fn face_floor(c: i32) -> Option<i32> {
    match c {
        ..100 => Some(33),
        100..=107 => Some(36 + 4 * (c - 100)),
        _ => None,
    }
}

impl Play {
    /// A `classic-1` level whose floor in column `c` is at level y
    /// `floor(c)`, or none, with the player riding at `(x, y)` as in
    /// [`Play::on`], in the air unless there's floor right under it.
    fn columns(
        floor: impl Fn(i32) -> Option<i32>,
        (x, y): (f64, f64),
        vel: (i64, i64),
        facing: Facing,
        held: &[Action],
    ) -> Play {
        let width = 1200;
        let mut src = format!(
            "format = 1\nname = \"faces\"\nsize = [{width}, 600]\nstart = [{x}, {y}]\n\
             [[entities]]\ntype = \"hutch\"\nat = [40, {}]\n",
            floor(40).unwrap(),
        );
        let mut c = 0;
        while c < width {
            let at = floor(c);
            let end = (c..width).find(|&e| floor(e) != at).unwrap_or(width);
            if let Some(h) = at {
                src += &format!(
                    "[[collision]]\nshape = \"rect\"\nrect = [{c}, {h}, {}, {}]\n",
                    end - c,
                    600 - h
                );
            }
            c = end;
        }
        let level = Level::from_toml_str(&src, "faces").unwrap();
        let mut world = World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        );
        let Some(Behavior::Hutch(hutch)) = world.things[0].archetype.map(|a| a.behavior) else {
            panic!("no animal crate");
        };
        let player = &mut world.player;
        player.pos = Vec2::new(fx(x), fx(y));
        player.vel = Vec2::new(from_grains(vel.0), from_grains(vel.1));
        player.facing = facing;
        let on_floor = floor(x as i32) == Some(y as i32) && fx(y) == fx(y.floor());
        if !on_floor {
            player.ground = None;
        }
        assert_eq!(player.grounded(), on_floor);
        player.riding = Some(Riding {
            on_floor,
            mount: None,
            ..Riding::new(hutch.ride, Fixed::ZERO)
        });
        Play {
            world,
            held: held.iter().copied().collect(),
        }
    }

    fn riding(&mut self) -> &mut Riding {
        self.world.player.riding.as_mut().unwrap()
    }

    /// `x`, `y`, `vx` and `vy` in 1/256, and whether on the floor.
    fn face_row(&self) -> (Fixed, Fixed, i64, i64, bool) {
        let p = &self.world.player;
        (
            p.pos.x,
            p.pos.y,
            grains(p.vel.x),
            grains(p.vel.y),
            p.grounded(),
        )
    }
}

/// "Landing on a face, holding the uphill direction": a jump falling to
/// the left over the drop, Left held throughout.
#[test]
fn landing_on_a_face_soon_after_the_approach_frame() {
    let mut play = Play::columns(
        face_floor,
        (109.76953125, 4.5625),
        (-202, 1120),
        Facing::Left,
        &[L],
    );
    let r = play.riding();
    r.part = RidePart::Jump(20);
    r.steer = true;
    r.fell = true;
    let mut rows = vec![play.face_row()];
    let mut parts = vec![play.part()];
    for _ in 5..=20 {
        play.step(&[L]);
        rows.push(play.face_row());
        parts.push(play.part());
    }
    let row = |frame: usize| rows[frame - 4];
    let part = |frame: usize| parts[frame - 4];
    let air = |x, y, vx, vy| (fx(x), fx(y), vx, vy, false);
    let floor = |x, y, vx| (fx(x), fx(y), vx, 0, true);
    assert_eq!(row(5), air(108.83203125, 9.375, -240, 1232));
    assert_eq!(row(6), air(107.76171875, 14.625, -274, 1344));
    assert_eq!(play.world.player.riding.unwrap().approach, Some(14));
    assert_eq!(row(7), air(106.578125, 20.3125, -303, 1456));
    assert_eq!(row(9), air(103.921875, 33.0, -351, 1680));
    assert_eq!(row(10), floor(102.47265625, 44.0, -371));
    assert_eq!(part(10), RidePart::Landing(0));
    // Steered, Left ignored on the face, and blocked by the rise.
    assert_eq!(row(11), floor(102.0, 44.0, 0));
    assert_eq!(row(12), floor(102.0, 44.0, 0));
    assert_eq!(row(13), floor(102.046875, 44.0, 12));
    assert_eq!(row(14), floor(102.13671875, 44.0, 23));
    assert_eq!(row(18), floor(102.90234375, 44.0, 64));
    assert_eq!(row(19), floor(103.19140625, 48.0, 74));
    assert_eq!(part(19), RidePart::Landing(9));
    assert_eq!(row(20), floor(103.515625, 48.0, 83));
    assert_eq!(part(20), RidePart::Riding);
    assert_eq!(play.world.player.facing, Facing::Left);
}

/// "Leaving at the low end": at rest on the face, with the push already
/// down it, Left held throughout.
#[test]
fn leaving_at_the_low_end_of_a_face() {
    let mut play = Play::columns(face_floor, (106.0, 60.0), (0, 0), Facing::Left, &[L]);
    play.riding().push = fx(1.5);
    let mut rows = vec![play.face_row()];
    for _ in 1..=11 {
        play.step(&[L]);
        rows.push(play.face_row());
    }
    let floor = |x, y, vx| (fx(x), fx(y), vx, 0, true);
    let air = |x, y, vx, vy| (fx(x), fx(y), vx, vy, false);
    assert_eq!(rows[1], floor(106.046875, 60.0, 12));
    assert_eq!(rows[2], floor(106.13671875, 60.0, 23));
    assert_eq!(rows[6], floor(106.90234375, 60.0, 64));
    assert_eq!(rows[7], floor(107.19140625, 64.0, 74));
    assert_eq!(rows[8], air(107.515625, 67.4375, 83, 1));
    assert_eq!(rows[9], air(107.76953125, 67.87890625, 65, 113));
    assert_eq!(rows[10], air(107.953125, 68.7578125, 47, 225));
    assert_eq!(rows[11], air(108.0703125, 70.07421875, 30, 337));
}

/// Holding the downhill direction on a face: a target of 2 plus the push.
#[test]
fn the_push_adds_to_the_downhill_target() {
    let mut play = Play::columns(face_floor, (103.0, 48.0), (0, 0), Facing::Right, &[R]);
    play.riding().push = fx(1.5);
    play.step(&[R]);
    assert_eq!(play.face_row().2, 896 / 32);
    let mut play = Play::columns(face_floor, (103.0, 48.0), (0, 0), Facing::Right, &[R]);
    play.riding().push = fx(1.5);
    play.step(&[R, A]);
    assert_eq!(play.face_row().2, (896 + 384) / 32);
}

/// No jump starts on a face, and holding up it doesn't turn.
#[test]
fn no_jump_or_uphill_turn_on_a_face() {
    let mut play = Play::columns(face_floor, (103.0, 48.0), (0, 0), Facing::Right, &[]);
    play.step(&[L, J]);
    assert_eq!(play.part(), RidePart::Riding);
    assert_eq!(play.world.player.facing, Facing::Right);
    // Pressed on flat floor, it does.
    let mut play = Play::columns(face_floor, (60.0, 33.0), (0, 0), Facing::Right, &[]);
    play.step(&[J]);
    assert_eq!(play.part(), RidePart::Jump(0));
}
