//! Getting off the animal in Classic 1 (spec/behavior/getting-off.md):
//! the hop, the jump off, the animal left behind and getting on again.
//! Positions in the spec are made relative; here the floor is at y = 300
//! and the rider starts near `x` = 1000.

use std::sync::Arc;

use std::collections::BTreeMap;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Presets};
use clamber_sim::{
    Behavior, Event, Facing, Fixed, InputFrame, Phase, RidePart, Riding, World, generic_feel, step,
};

const L: Action = Action::Left;
const R: Action = Action::Right;
const D: Action = Action::Down;
const J: Action = Action::Jump;
const A: Action = Action::Action;
const T: Action = Action::Tandem;

const FLOOR: i32 = 300;
const ANIMAL: usize = 0;

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
    /// Where positions are made relative to.
    origin: Vec2,
}

impl Play {
    /// A `classic-1` level with long flat floor and the first level's
    /// crate far to the left.
    fn level() -> World {
        let src = format!(
            "format = 1\nname = \"getting off\"\nsize = [2000, 600]\nstart = [1000, {FLOOR}]\n\
             [[collision]]\nshape = \"rect\"\nrect = [0, {FLOOR}, 2000, 280]\n\
             [[entities]]\ntype = \"hutch\"\nat = [40, {FLOOR}]\n"
        );
        let level = Level::from_toml_str(&src, "getting off").unwrap();
        World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        )
    }

    /// The player riding on the floor at `x` at `vx` (1/256), facing
    /// `facing`, with `held` held on the frame before. Positions are made
    /// relative to `x`.
    fn riding(x: f64, vx: i64, facing: Facing, held: &[Action]) -> Play {
        let mut world = Play::level();
        let Some(Behavior::Hutch(hutch)) = world.things[ANIMAL].archetype.map(|a| a.behavior)
        else {
            panic!("no animal crate");
        };
        world.things[ANIMAL].phase = Phase::Ridden;
        let player = &mut world.player;
        player.pos = Vec2::new(fx(x), Fixed::from_int(FLOOR));
        player.vel.x = from_grains(vx);
        player.facing = facing;
        assert!(player.grounded());
        player.riding = Some(Riding {
            on_floor: true,
            was_on_floor: true,
            mount: None,
            ..Riding::new(hutch.ride, Fixed::ZERO)
        });
        world.things[ANIMAL].pos = player.pos;
        Play {
            world,
            held: held.iter().copied().collect(),
            origin: Vec2::new(fx(x), Fixed::from_int(FLOOR)),
        }
    }

    /// The player on the frame before riding starts after mounting the
    /// animal at `(xc, FLOOR)`: put at `xc` + `dx` and `dy` above the
    /// floor at `vx` (1/256), facing right, holding still for this frame.
    /// `jump_held` says Jump has been held since the press of the jump
    /// mounted from. Positions are made relative to the player.
    fn mounting(xc: i32, dx: f64, dy: f64, vx: i64, jump_held: bool) -> Play {
        let mut world = Play::level();
        let animal = &mut world.things[ANIMAL];
        animal.pos = Vec2::new(Fixed::from_int(xc), Fixed::from_int(FLOOR));
        animal.phase = Phase::Mounted(21);
        let player = &mut world.player;
        player.pos = Vec2::new(
            Fixed::from_int(xc) + fx(dx),
            Fixed::from_int(FLOOR) - fx(dy),
        );
        player.ground = None;
        player.vel = Vec2::new(from_grains(vx), Fixed::ZERO);
        player.facing = Facing::Right;
        player.frozen = 1;
        player.jump_hold = jump_held;
        player.jump_held = if jump_held { 40 } else { 0 };
        let origin = Vec2::new(player.pos.x, Fixed::from_int(FLOOR));
        Play {
            world,
            held: if jump_held {
                [R, J].into_iter().collect()
            } else {
                R.into()
            },
            origin,
        }
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// `x` and `y` made relative, `vx` in 1/256 and `vy`.
    fn row(&self) -> (Fixed, Fixed, i64, Fixed) {
        let p = &self.world.player;
        (
            p.pos.x - self.origin.x,
            p.pos.y - self.origin.y,
            grains(p.vel.x),
            p.vel.y,
        )
    }

    fn facing(&self) -> Facing {
        self.world.player.facing
    }

    fn animal(&self) -> (Fixed, Fixed, Facing, Phase) {
        let a = &self.world.things[ANIMAL];
        (
            a.pos.x - self.origin.x,
            a.pos.y - self.origin.y,
            a.facing,
            a.phase,
        )
    }

    fn jumped(&self) -> bool {
        self.world.events.contains(&Event::JumpStart)
    }

    fn landed(&self) -> bool {
        self.world.events.contains(&Event::Land)
    }
}

/// getting-off.md's hop example: riding right at about +1.65 with Right
/// held, Tandem pressed with Right, Right held to P + 20 and Left from
/// P + 21. The rider's `x` on P − 1 is 0.3125 past a whole pixel.
#[test]
fn the_hop_example() {
    let mut play = Play::riding(1000.3125, 422, Facing::Right, &[R]);
    let rows = [
        (0, 1.65625, -5.5625, 424, -5.5625, Facing::Right),
        (1, 3.35546875, -10.6875, 435, -5.125, Facing::Right),
        (12, 23.640625, -38.1875, 491, -0.3125, Facing::Right),
        (21, 40.67578125, -21.3125, 376, 3.625, Facing::Right),
        (24, 42.69140625, -7.8125, 83, 4.9375, Facing::Right),
        (25, 42.7265625, 0.0, 9, 0.0, Facing::Right),
        (26, 42.5078125, 0.0, -56, 0.0, Facing::Right),
        (33, 36.125, 0.0, -330, 0.0, Facing::Right),
        (34, 34.75, 0.0, -352, 0.0, Facing::Right),
        (38, 28.5703125, 0.0, -417, 0.0, Facing::Left),
    ];
    let mut rows = rows.into_iter().peekable();
    for frame in 0..=38 {
        let keys: &[Action] = match frame {
            0 => &[T, R],
            1..=20 => &[R],
            _ => &[L],
        };
        play.step(keys);
        assert_eq!(play.world.player.riding, None, "frame P + {frame}");
        assert_eq!(play.landed(), frame == 25, "frame P + {frame}");
        if frame == 34 {
            assert_eq!(play.world.player.turn, Some(0), "a turn starts on L + 9");
        } else if frame < 34 {
            assert_eq!(play.world.player.turn, None, "frame P + {frame}");
        }
        if rows.peek().is_some_and(|r| r.0 == frame) {
            let (_, x, y, vx, vy, facing) = rows.next().unwrap();
            assert_eq!(
                (play.row(), play.facing()),
                ((fx(x), fx(y), vx, fx(vy)), facing),
                "frame P + {frame}"
            );
        }
        // The animal stays where the rider was.
        assert_eq!(
            play.animal(),
            (
                fx(-0.3125),
                Fixed::ZERO,
                Facing::Right,
                Phase::Waiting(None)
            ),
            "frame P + {frame}"
        );
    }
}

/// From standing still, the hop rises 38.1875 px, highest on P + 12,
/// and lands on P + 25; holding Tandem doesn't get off again, nor does a
/// press with nothing to get off.
#[test]
fn a_hop_from_standing_still() {
    let mut play = Play::riding(1000.0, 0, Facing::Left, &[]);
    let mut top = (Fixed::ZERO, 0);
    for frame in 0..=30 {
        play.step(&[T]);
        let (x, y, vx, _) = play.row();
        assert_eq!((x, vx), (Fixed::ZERO, 0), "frame P + {frame}");
        if y < top.0 {
            top = (y, frame);
        }
        assert_eq!(play.landed(), frame == 25, "frame P + {frame}");
    }
    assert_eq!(top, (fx(-38.1875), 12));
    assert_eq!(play.facing(), Facing::Left);
}

/// Jump held on P from a riding jump that has had it held since its
/// press gives the hop the held gravity; let go, the usual one.
#[test]
fn a_hop_from_a_jump_with_jump_held_has_the_held_gravity() {
    for (keep, second) in [(true, -5.4375), (false, -5.28125)] {
        let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
        play.step(&[J]);
        for _ in 0..8 {
            play.step(&[J]);
        }
        assert!(matches!(
            play.world.player.riding.unwrap().part,
            RidePart::Jump(_)
        ));
        play.step(&[T, J]);
        let vy = play.row().3;
        assert_eq!(play.world.player.riding, None);
        // On P the held gravity: Jump has been held since the jump's press.
        assert_eq!(vy, fx(-5.71875));
        play.step(if keep { &[J] } else { &[] });
        assert_eq!(play.row().3, fx(second), "Jump held on P + 1: {keep}");
    }
}

/// The landing frames: running starts on L with Action and a direction
/// held; a Jump press before the landing gives a jump taking off on
/// L + 2; Down held on L + 1 alone doesn't crouch.
#[test]
fn starting_on_foot_after_a_hop() {
    // Running on L.
    let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
    play.step(&[T]);
    for _ in 1..25 {
        play.step(&[]);
    }
    play.step(&[L, A]);
    assert!(play.landed());
    assert!(matches!(
        play.world.player.gait,
        clamber_sim::Gait::Running(_)
    ));
    assert_eq!(play.facing(), Facing::Right, "facing doesn't change on L");

    // A Jump press on L − 5, held through L + 2.
    let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
    play.step(&[T]);
    for frame in 1..=30 {
        play.step(if frame >= 20 { &[J] } else { &[] });
        assert_eq!(play.jumped(), frame == 27, "frame P + {frame}");
    }

    // Down on L + 1 alone: no crouch; on L + 2: a crouch starts there.
    for (from, crouches) in [(26, false), (27, true)] {
        let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
        play.step(&[T]);
        for _ in 1..from {
            play.step(&[]);
        }
        play.step(&[D]);
        assert_eq!(
            play.world.player.crouch.held,
            u8::from(crouches),
            "Down on P + {from}"
        );
    }
}

/// An animal left in the air falls: 112/256 px, then 113/256, then
/// 112/256 more on each frame, until a move would leave it less than
/// 5 px above the floor.
#[test]
fn an_animal_left_in_the_air_falls_to_the_floor() {
    let mut play = Play::riding(1000.0, 0, Facing::Left, &[]);
    play.step(&[J]);
    for _ in 0..12 {
        play.step(&[J]);
    }
    let (_, y, _, _) = play.row();
    assert!(y < fx(-20.0), "the rider is up in a jump");
    play.step(&[T]);
    let (_, start, facing, phase) = play.animal();
    assert_eq!(start, Fixed::from_int(-(-y).floor() as i32));
    assert_eq!((facing, phase), (Facing::Left, Phase::Waiting(Some(0))));
    let mut at = start;
    let mut by = Fixed::ZERO;
    for n in 0..30 {
        play.step(&[]);
        by = match n {
            0 => from_grains(112),
            1 => from_grains(113),
            _ => by + from_grains(112),
        };
        let (_, y, _, phase) = play.animal();
        if -(at + by) < Fixed::from_int(5) {
            assert_eq!((y, phase), (Fixed::ZERO, Phase::Waiting(None)), "frame {n}");
            return;
        }
        at += by;
        assert_eq!(y, at, "frame {n}");
    }
    panic!("the animal didn't land");
}

/// getting-off.md's example after a mount: Tandem pressed on M + 23 with
/// Right held hops from where the mount left the player, not steered.
#[test]
fn a_hop_right_after_a_mount() {
    let mut play = Play::mounting(500, 0.25, 0.90625, 512, false);
    play.step(&[R]);
    assert!(
        play.world.player.riding.is_some(),
        "riding from the next frame"
    );
    assert_eq!(play.row(), (Fixed::ZERO, fx(-0.90625), 512, Fixed::ZERO));
    let rows = [
        (23, 2.0, -6.46875, -5.5625),
        (24, 4.0, -11.59375, -5.125),
        (35, 26.0, -39.09375, -0.3125),
        (47, 50.0, -8.71875, 4.9375),
        (48, 52.0, 0.0, 0.0),
    ];
    let mut rows = rows.into_iter().peekable();
    for frame in 23..=48 {
        play.step(if frame == 23 { &[T, R] } else { &[R] });
        assert_eq!(play.landed(), frame == 48, "frame M + {frame}");
        if rows.peek().is_some_and(|r| r.0 == frame) {
            let (_, x, y, vy) = rows.next().unwrap();
            assert_eq!(play.row(), (fx(x), fx(y), 512, fx(vy)), "frame M + {frame}");
        }
    }
    let (x, y, facing, phase) = play.animal();
    assert_eq!(
        (x, y, facing, phase),
        (fx(-0.25), Fixed::ZERO, Facing::Right, Phase::Waiting(None))
    );
}

/// The same with Jump held from the jump on foot through the press: the
/// hop has the held gravity while Jump stays held.
#[test]
fn a_hop_right_after_a_mount_with_jump_held() {
    let mut play = Play::mounting(500, 0.25, 0.375, 512, true);
    play.step(&[R, J]);
    let rider = play.world.player.riding.unwrap();
    assert!(rider.mounted_held);
    for (frame, vy) in [(23, -5.71875), (24, -5.4375), (25, -5.15625)] {
        play.step(if frame == 23 { &[T, R, J] } else { &[R, J] });
        assert_eq!(play.row().3, fx(vy), "frame M + {frame}");
    }
}

/// Tandem held from the mount isn't a press on the first riding frame.
#[test]
fn tandem_held_through_the_mount_isnt_a_press() {
    let mut play = Play::mounting(500, 0.25, 0.90625, 512, false);
    play.held = [R, T].into_iter().collect();
    play.step(&[R, T]);
    for _ in 23..40 {
        play.step(&[R, T]);
        assert!(play.world.player.riding.is_some());
    }
    play.step(&[R]);
    play.step(&[R, T]);
    assert!(play.world.player.riding.is_none());
}

/// The jump off: Tandem and Jump pressed together from a rider at rest.
/// P + 1 takes off; with Jump held 1 frame and Right held from P + 1 the
/// player lands on P + 33, with Jump held 30 frames on P + 47; with
/// nothing held it comes straight down onto the animal and gets on
/// again on P + 26.
#[test]
fn the_jump_off() {
    let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
    play.step(&[T, J]);
    assert_eq!(play.row().3, fx(-5.71875));
    assert!(!play.jumped());
    play.step(&[R]);
    assert!(play.jumped(), "P + 1 is the take-off frame");
    assert_eq!(play.row().3, fx(-7.0));
    assert_eq!(play.facing(), Facing::Right);
    for frame in 2..=33 {
        play.step(&[R]);
        assert_eq!(play.landed(), frame == 33, "frame P + {frame}");
    }

    let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
    play.step(&[T, J]);
    for frame in 1..=47 {
        play.step(if frame < 30 { &[R, J] } else { &[R] });
        assert_eq!(play.landed(), frame == 47, "frame P + {frame}");
    }

    let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
    play.step(&[T, J]);
    for frame in 1..=26 {
        play.step(&[]);
        let phase = play.world.things[ANIMAL].phase;
        assert_eq!(
            phase == Phase::Mounted(0),
            frame == 26,
            "frame P + {frame}: {phase:?}"
        );
    }
}

/// A jump on foot onto an animal left facing left gets on it within the
/// ranges turned around; a hop never does.
#[test]
fn getting_on_again() {
    // Hopping straight up comes down onto the animal without getting on.
    let mut play = Play::riding(1000.0, 0, Facing::Left, &[]);
    play.step(&[T]);
    for _ in 1..40 {
        play.step(&[]);
        assert_eq!(play.world.things[ANIMAL].phase, Phase::Waiting(None));
    }
    // Then a jump straight up from beside it gets on: the player facing
    // left at `I` = 27 (in range only for an animal facing left).
    let player = &mut play.world.player;
    player.pos.x = play.origin.x + Fixed::from_int(27);
    player.vel.x = Fixed::ZERO;
    for _ in 0..10 {
        play.step(&[]);
    }
    play.step(&[J]);
    let mut mounted = false;
    for _ in 0..60 {
        play.step(&[J]);
        if let Phase::Mounted(_) = play.world.things[ANIMAL].phase {
            mounted = true;
            break;
        }
    }
    assert!(mounted, "got on again");
    for _ in 0..23 {
        play.step(&[]);
    }
    assert!(play.world.player.riding.is_some());
    assert_eq!(play.world.things[ANIMAL].phase, Phase::Ridden);
}

/// After a jump off, the landing is as after any jump, with P + 1 as its
/// take-off frame: from a rider at rest, Jump held 1 frame and Right held
/// from P + 1, landing on P + 33 (L), walking starts on L + 7, or running
/// on L + 1 with Action held since before the landing.
#[test]
fn landing_after_a_jump_off() {
    for (action, starts) in [(false, 40), (true, 34)] {
        let mut play = Play::riding(1000.0, 0, Facing::Right, &[]);
        play.step(&[T, J]);
        for frame in 1..=starts {
            let keys: &[Action] = if action && frame >= 20 { &[R, A] } else { &[R] };
            play.step(keys);
            assert_eq!(play.landed(), frame == 33, "frame P + {frame}");
            let moving = matches!(
                play.world.player.gait,
                clamber_sim::Gait::Walking(_) | clamber_sim::Gait::Running(_)
            );
            assert_eq!(
                moving,
                frame == starts,
                "frame P + {frame}, Action: {action}"
            );
        }
        let running = matches!(play.world.player.gait, clamber_sim::Gait::Running(_));
        assert_eq!(running, action);
    }
}

impl Play {
    /// The second character as the player.
    fn second(mut self) -> Play {
        let mut feel = (*self.world.feel).clone();
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::new(),
        });
        self.world.feel = Arc::new(feel);
        self.world.player.character = 1;
        self
    }
}

/// "The second character": a hop has the first character's gravity and
/// lands on P + 25, with landing frames to L + 16; on-foot play (a turn
/// here) starts on L + 17.
#[test]
fn the_second_characters_hop() {
    let mut play = Play::riding(1000.0, 0, Facing::Right, &[]).second();
    play.step(&[T, R]);
    assert_eq!(play.row().3, fx(-5.5625));
    for frame in 1..=42 {
        play.step(&[L]);
        assert_eq!(play.landed(), frame == 25, "frame P + {frame}");
        assert_eq!(
            play.world.player.turn.is_some(),
            frame == 25 + 17,
            "frame P + {frame}"
        );
    }
}

/// "The second character": the jump-off has its own jump, landing on
/// P + 39 with Jump held 1 frame and Right from P + 1, on P + 47 with Jump
/// held 30 frames.
#[test]
fn the_second_characters_jump_off() {
    let mut play = Play::riding(1000.0, 0, Facing::Right, &[]).second();
    play.step(&[T, J]);
    assert_eq!(play.row().3, fx(-5.6484375));
    play.step(&[R]);
    assert!(play.jumped());
    assert_eq!(play.row().3, fx(-2138.0 / 256.0));
    for frame in 2..=39 {
        play.step(&[R]);
        assert_eq!(play.landed(), frame == 39, "frame P + {frame}");
    }
    for (held, lands) in [(5, 41), (10, 42), (30, 47)] {
        let mut play = Play::riding(1000.0, 0, Facing::Right, &[]).second();
        play.step(&[T, J]);
        for frame in 1..=lands {
            play.step(if frame < held { &[R, J] } else { &[R] });
            assert_eq!(play.landed(), frame == lands, "{held} held, P + {frame}");
        }
    }
}
