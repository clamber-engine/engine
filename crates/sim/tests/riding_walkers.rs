//! Riding the animal into walkers in Classic 1
//! (spec/behavior/riding-walkers.md). Positions in the spec are made
//! relative; here the floor is at y = 300 and the rider starts at
//! `x` = 1000.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    Behavior, Event, Facing, Fixed, InputFrame, Phase, RidePart, Riding, World, generic_feel, step,
};

const R: Action = Action::Right;
const J: Action = Action::Jump;
const A: Action = Action::Action;

const FLOOR: i32 = 300;
const X0: f64 = 1000.0;

/// The small walker as the importer places it, but for its facing.
const SMALL: &str = "speed = 1\nresponse = 0.125\nsize = [20, 12]\noffset = 0.5\n\
                     turn_delay = 4\ndefeat_jump = 6\ndefeat_gravity = 0.4375\n\
                     defeat_max_fall = 8\n";

/// The medium walker's rider table, as the importer gives it.
const MEDIUM_RIDER: &str = "rider = { attack_across = [[-53, 31], [-63, 21]], \
                            attack_above = 38, knock_across = [[-16, 33], [-26, 23]], \
                            knock_above = 10, throw_speed = 2 }\n";

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

const WALKER: usize = 1;

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// A `classic-1` level with long flat floor, the player riding at
    /// `x` = 1000 on it, at rest at `vx` (1/256), facing `facing`, on the
    /// floor for the last two frames, and a walker with `params` (and
    /// `facing`, walking that way at full speed) at `walker` from it.
    fn new(vx: i64, facing: Facing, walker: f64, walker_facing: Facing, params: &str) -> Play {
        let face = |f: Facing| match f {
            Facing::Left => "left",
            Facing::Right => "right",
        };
        let params = format!("{SMALL}{params}facing = \"{}\"\n", face(walker_facing));
        let src = format!(
            "format = 1\nname = \"riding\"\nsize = [2000, 600]\nstart = [{X0}, {FLOOR}]\n\
             [[collision]]\nshape = \"rect\"\nrect = [0, {FLOOR}, 2000, 280]\n\
             [[entities]]\ntype = \"hutch\"\nat = [40, {FLOOR}]\n\
             [[entities]]\ntype = \"crawler\"\nat = [{}, {FLOOR}]\n\
             [entities.params]\n{params}",
            X0 + walker,
        );
        let level = Level::from_toml_str(&src, "riding").unwrap();
        let mut world = World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        );
        let Some(Behavior::Hutch(hutch)) = world.things[0].archetype.map(|a| a.behavior) else {
            panic!("no animal crate");
        };
        assert!(world.things[WALKER].active);
        world.things[0].phase = Phase::Ridden;
        let player = &mut world.player;
        player.pos = Vec2::new(fx(X0), Fixed::from_int(FLOOR));
        player.vel.x = from_grains(vx);
        player.facing = facing;
        assert!(player.grounded());
        player.riding = Some(Riding {
            on_floor: true,
            was_on_floor: true,
            mount: None,
            ..Riding::new(hutch.ride, Fixed::ZERO)
        });
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

    /// The player's `x` and `y` from where it started, and `vy` in 1/256.
    fn player(&self) -> (Fixed, Fixed, i64) {
        let p = &self.world.player;
        (
            p.pos.x - fx(X0),
            p.pos.y - Fixed::from_int(FLOOR),
            grains(p.vel.y),
        )
    }

    /// The walker's `x` from where the player started.
    fn walker_x(&self) -> Fixed {
        self.world.things[WALKER].pos.x - fx(X0)
    }

    fn part(&self) -> Option<RidePart> {
        self.world.player.riding.map(|r| r.part)
    }
}

fn charge(age: u8) -> Option<RidePart> {
    Some(RidePart::Charge {
        age,
        steered: false,
    })
}

/// riding-walkers.md's first example: a small walker walks into the
/// front of a rider at rest, which charges at it without input and
/// throws it.
#[test]
fn a_walker_in_front_starts_a_charge_and_is_thrown() {
    // As at the end of frame −1.
    let mut play = Play::new(0, Facing::Left, -55.3671875, Facing::Right, "");
    play.step(&[]);
    // D: the charge shows from here, and counts from the next frame.
    assert_eq!(
        play.part(),
        Some(RidePart::Rush { steered: false }),
        "frame 0"
    );
    assert_eq!(play.walker_x(), fx(-54.3671875));
    for (frame, x) in [(1, -53.3671875), (2, -52.3671875)] {
        play.step(&[]);
        assert_eq!(play.part(), charge(frame - 1), "frame {frame}");
        assert_eq!(
            play.player(),
            (Fixed::ZERO, Fixed::ZERO, 0),
            "frame {frame}"
        );
        assert_eq!(play.walker_x(), fx(x), "frame {frame}");
    }
    // C + 2: thrown. The rider was at rest, so the walker doesn't move
    // across, and neither moves up yet.
    play.step(&[]);
    assert_eq!(play.player(), (Fixed::ZERO, Fixed::ZERO, -1536), "frame 3");
    assert_eq!(play.walker_x(), fx(-52.3671875));
    let walker = &play.world.things[WALKER];
    assert_eq!(walker.pos.y, Fixed::from_int(FLOOR));
    assert!(matches!(walker.phase, Phase::Falling { .. }));
    play.step(&[]);
    assert_eq!(play.player(), (Fixed::ZERO, fx(-5.5625), -1424), "frame 4");
    // The walker goes up as the rider does, and doesn't move across.
    let walker = &play.world.things[WALKER];
    assert_eq!(walker.pos.y - Fixed::from_int(FLOOR), fx(-5.5625));
    assert_eq!(play.walker_x(), fx(-52.3671875));
    for _ in 5..=16 {
        play.step(&[]);
    }
    assert_eq!(play.player(), (Fixed::ZERO, fx(-38.1875), -80), "frame 16");
    for frame in 17..29 {
        play.step(&[]);
        assert_eq!(play.part(), charge(frame - 1), "frame {frame}");
    }
    play.step(&[]);
    assert_eq!(play.player(), (Fixed::ZERO, Fixed::ZERO, 0), "frame 29");
    assert_eq!(
        play.part(),
        Some(RidePart::ChargeEnd {
            age: 0,
            steered: false
        })
    );
    for _ in 30..41 {
        play.step(&[]);
    }
    play.step(&[]);
    assert_eq!(play.part(), Some(RidePart::Settling), "frame 41");
    // The thrown walker fell out of the level.
    for _ in 0..120 {
        play.step(&[]);
    }
    assert!(!play.world.things[WALKER].active);
}

/// A thrown walker moves across at twice the rider's speed, at once.
/// A walker in front on C and C + 1 of a pressed charge isn't thrown
/// then.
#[test]
fn a_thrown_walker_moves_at_twice_the_riders_speed() {
    // Riding right at 2 px per frame toward a walker coming the other
    // way, well inside its attack range: Action pressed on C.
    let mut play = Play::new(512, Facing::Right, 40.5, Facing::Left, "");
    play.step(&[R, A]);
    assert_eq!(play.part(), charge(0));
    play.step(&[R, A]);
    assert_eq!(play.part(), charge(1));
    assert!(play.world.things[WALKER].phase == Phase::Ready);
    let before = play.walker_x();
    play.step(&[R, A]);
    let vx = play.world.player.vel.x;
    assert_eq!(play.player().2, -1536);
    let walker = &play.world.things[WALKER];
    assert!(matches!(walker.phase, Phase::Falling { .. }));
    assert_eq!(walker.vx, vx + vx);
    assert_eq!(play.walker_x(), before + vx + vx);
    assert_eq!(walker.pos.y, Fixed::from_int(FLOOR));
}

/// An automatic charge during a jump keeps the jump's steering and
/// lighter gravity, as a pressed one does.
#[test]
fn a_charge_from_a_jump_keeps_its_steering() {
    let mut play = Play::new(0, Facing::Right, 80.0, Facing::Left, "");
    play.step(&[J]);
    assert_eq!(play.part(), Some(RidePart::Jump(0)));
    let mut frame = 1;
    while play.part().is_some_and(|p| matches!(p, RidePart::Jump(_))) {
        play.step(&[J]);
        frame += 1;
        assert!(frame < 60, "no charge");
    }
    assert_eq!(play.part(), Some(RidePart::Rush { steered: true }));
    assert!(play.world.player.riding.unwrap().held);
    play.step(&[J]);
    assert_eq!(
        play.part(),
        Some(RidePart::Charge {
            age: 0,
            steered: true
        })
    );
}

/// riding-walkers.md's second example: a small walker walks into a
/// rider at rest from behind and knocks it off.
#[test]
fn a_walker_from_behind_knocks_the_rider_off() {
    // As at the end of frame −1: I = 26.
    let mut play = Play::new(0, Facing::Right, -25.95703125, Facing::Right, "");
    let animal_x = play.world.player.pos.x;
    play.step(&[]);
    // H: the player doesn't move, and is on foot.
    assert!(play.world.player.riding.is_none());
    assert_eq!(play.player(), (Fixed::ZERO, Fixed::ZERO, -1536), "frame 0");
    assert_eq!(play.walker_x(), fx(-24.95703125));
    assert_eq!(play.world.things[0].phase, Phase::Bolting { vx: fx(6.0) });
    for frame in 1..=15 {
        play.step(&[R]);
        assert_eq!(
            play.player(),
            (Fixed::ZERO, Fixed::ZERO, -1536),
            "frame {frame}"
        );
        assert_eq!(play.walker_x(), fx(-24.95703125), "frame {frame}");
        assert_eq!(play.world.things[0].pos.x, animal_x, "frame {frame}");
    }
    let animal = |play: &Play| play.world.things[0].pos.x - animal_x;
    play.step(&[]);
    assert_eq!(play.player(), (Fixed::ZERO, fx(-5.5625), -1424), "frame 16");
    assert_eq!(play.walker_x(), fx(-23.95703125));
    assert_eq!(animal(&play), fx(6.0));
    for _ in 17..20 {
        play.step(&[]);
    }
    play.step(&[]);
    assert_eq!(play.player(), (Fixed::ZERO, fx(-23.4375), -976), "frame 20");
    assert_eq!(play.walker_x(), fx(-19.95703125));
    assert_eq!(animal(&play), fx(30.0));
    play.step(&[]);
    assert_eq!(play.player(), (Fixed::ZERO, fx(-26.8125), -864), "frame 21");
    assert_eq!(play.walker_x(), fx(-18.95703125));
    assert_eq!(animal(&play), fx(35.75));
    // The animal eases toward 4 px per frame.
    for vx in [1416, 1367, 1325] {
        let before = animal(&play);
        play.step(&[]);
        assert_eq!(animal(&play) - before, from_grains(vx));
    }
    for _ in 25..=28 {
        play.step(&[]);
    }
    assert_eq!(play.player(), (Fixed::ZERO, fx(-38.1875), -80), "frame 28");
    assert_eq!(play.walker_x(), fx(-11.95703125));
    assert_eq!(play.world.player.facing, Facing::Right);
    // The walker walks under the falling player, who bounces off it.
    let mut bounced = false;
    for _ in 29..80 {
        play.step(&[]);
        bounced |= play.world.events.contains(&Event::Bounce);
        assert!(!play.world.events.contains(&Event::Hurt));
    }
    assert!(bounced);
}

/// With nothing below, a player knocked off lands on H + 41, on foot,
/// facing as it did.
#[test]
fn knocked_off_the_player_lands_on_foot() {
    let mut play = Play::new(0, Facing::Right, -25.95703125, Facing::Right, "");
    play.step(&[]);
    play.world.things[WALKER].pos.x += fx(600.0);
    // Holding the other way doesn't turn the player before it lands.
    for frame in 1..41 {
        play.step(&[Action::Left]);
        assert!(!play.world.player.grounded(), "frame {frame}");
        assert_eq!(play.world.player.facing, Facing::Right, "frame {frame}");
    }
    play.step(&[Action::Left]);
    assert!(play.world.player.grounded(), "frame 41");
    assert!(play.world.player.riding.is_none());
    assert!(play.world.events.contains(&Event::Land));
}

/// Knocked off in a jump's crouch with Jump held, the player keeps the
/// jump's lighter gravity if Jump stays held through the frozen frames.
#[test]
fn a_knock_off_keeps_a_held_jumps_lighter_gravity() {
    for (let_go, vy) in [(false, -1536 + 72), (true, -1536 + 112)] {
        // I = 28 at the end of frame −1: it knocks the rider off on 2.
        let mut play = Play::new(0, Facing::Right, -27.95703125, Facing::Right, "");
        play.step(&[J]);
        play.step(&[J]);
        assert_eq!(play.part(), Some(RidePart::Jump(1)));
        play.step(&[J]);
        assert!(play.world.player.riding.is_none());
        for _ in 0..15 {
            play.step(if let_go { &[] } else { &[J] });
        }
        play.step(&[J]);
        assert_eq!(play.player().2, vy, "let go: {let_go}");
    }
}

/// On flat floor a small walker only knocks off a rider on the floor;
/// a medium one also reaches a rider low in the air.
#[test]
fn a_medium_walker_reaches_a_rider_low_in_the_air() {
    for (params, knocked) in [("", false), (MEDIUM_RIDER, true)] {
        let mut play = Play::new(0, Facing::Right, -19.5, Facing::Right, params);
        play.world.player.pos.y -= fx(5.0);
        play.world.player.ground = None;
        let riding = play.world.player.riding.as_mut().unwrap();
        riding.on_floor = false;
        riding.was_on_floor = false;
        play.step(&[]);
        assert_eq!(play.world.player.riding.is_none(), knocked, "{params}");
    }
}

/// riding-walkers.md: ranges are for a rider facing left; facing right,
/// the range for the other walker facing turns around.
#[test]
fn ranges_turn_around_with_the_rider() {
    use clamber_sim::RiderReach;
    let attack = [(-55, 24), (-56, 23)];
    // A small walker facing left attacks from −24 to 55 when the rider
    // faces left, so one facing right from −55 to 24 when it faces right.
    assert_eq!(
        RiderReach::across(attack, Facing::Left, Facing::Left),
        (-24, 55)
    );
    assert_eq!(
        RiderReach::across(attack, Facing::Right, Facing::Right),
        (-55, 24)
    );
    assert_eq!(
        RiderReach::across(attack, Facing::Left, Facing::Right),
        (-23, 56)
    );
}
