//! Entering a level through its entrance on `classic-1`
//! (spec/behavior/level-start.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Presets};
use clamber_sim::{Event, Facing, Fixed, InputFrame, Phase, World, generic_feel, step};

const R: Action = Action::Right;
const L: Action = Action::Left;
const A: Action = Action::Action;
const J: Action = Action::Jump;

/// The spec's example: the start point at (100, 300), with an entrance
/// there, over a floor flat at y = 380.
fn level(entrance: bool) -> Arc<Level> {
    let mut src = r#"
        format = 1
        name = "entering"
        size = [3000, 700]
        start = [100, 300]

        [[collision]]
        shape = "rect"
        rect = [0, 380, 3000, 100]
        "#
    .to_string();
    if entrance {
        src += r#"
        [[entities]]
        type = "entrance"
        at = [100, 300]
        "#;
    }
    Arc::new(Level::from_toml_str(&src, "entering").unwrap())
}

fn world(characters: usize) -> World {
    let mut feel = generic_feel("classic-1").unwrap();
    for slot in 1..characters {
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", slot).unwrap(),
            abilities: BTreeMap::new(),
        });
    }
    World::new(level(true), Arc::new(feel))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(characters: usize) -> Play {
        Play {
            world: world(characters),
            held: Actions::NONE,
        }
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// Steps up to and including frame `last`, holding `keys`.
    fn until(&mut self, last: u32, keys: &[Action]) {
        while self.world.frame <= last {
            self.step(keys);
        }
    }

    fn at(&self) -> (f64, f64) {
        let p = self.world.player.pos;
        (p.x.to_f64(), p.y.to_f64())
    }

    fn speed(&self) -> (f64, f64) {
        let v = self.world.player.vel;
        (v.x.to_f64(), v.y.to_f64())
    }

    /// The speed across in 1/256 px per frame.
    fn vx256(&self) -> f64 {
        self.world.player.vel.x.to_f64() * 256.0
    }
}

/// level-start.md, "Example": every row of the table, with nothing held.
#[test]
fn the_examples_throw_frame_by_frame() {
    let mut play = Play::new(1);
    assert!(play.world.in_entrance());
    for frame in 0..=50 {
        play.step(&[]);
        assert_eq!(play.at(), (100.0, 300.0), "frame {frame}");
        assert!(!play.world.player_drawn(), "frame {frame}");
        assert_eq!(play.world.bursts.is_empty(), frame < 41, "frame {frame}");
        assert!(play.world.things[0].active, "frame {frame}");
    }
    // T = 51: thrown, drawn, the entrance gone, not moved yet.
    play.step(&[]);
    assert_eq!(play.at(), (100.0, 300.0));
    assert_eq!(play.speed(), (2.375, -2.375));
    assert!(play.world.player_drawn());
    assert!(!play.world.things[0].active);
    assert_eq!(play.world.things[0].phase, Phase::Broken);
    let rows = [
        (52, 102.375, 297.625, 608.0, -2.375),
        (65, 133.25, 266.75, 608.0, -2.375),
        (66, 135.625, 264.8125, 608.0, -1.9375),
        (70, 145.125, 261.4375, 608.0, -0.1875),
        (71, 147.5, 261.6875, 608.0, 0.25),
        (89, 190.25, 340.875, 608.0, 8.0),
        (90, 192.55078125, 348.875, 589.0, 8.0),
        (91, 194.78125, 356.875, 571.0, 8.0),
        (92, 196.9453125, 364.875, 554.0, 8.0),
        (93, 199.04296875, 372.875, 537.0, 8.0),
    ];
    for (frame, x, y, vx, vy) in rows {
        play.until(frame, &[]);
        assert_eq!(play.at(), (x, y), "frame {frame}");
        assert_eq!((play.vx256(), play.speed().1), (vx, vy), "frame {frame}");
        assert!(!play.world.player.grounded(), "frame {frame}");
        assert_eq!(play.world.player.facing, Facing::Right);
    }
    // Frame 94 lands, and 95 is on the ground.
    play.step(&[]);
    assert!(play.world.events.contains(&Event::Land));
    assert_eq!(play.at(), (201.078125, 380.0));
    assert_eq!((play.vx256(), play.speed().1), (521.0, 0.0));
    play.step(&[]);
    assert_eq!(play.vx256(), 456.0);
    assert!(play.world.entering.is_none());
}

/// level-start.md, "Frame by frame": nothing held through T + 38
/// changes the motion, and the lead faces right up to the landing.
#[test]
fn inputs_are_ignored_until_the_steering_starts() {
    for keys in [&[L][..], &[R, A], &[J], &[Action::Down]] {
        let mut play = Play::new(1);
        play.until(89, keys);
        assert_eq!(play.at(), (190.25, 340.875), "{keys:?}");
        assert_eq!(play.vx256(), 608.0, "{keys:?}");
        assert_eq!(play.world.player.facing, Facing::Right, "{keys:?}");
    }
}

/// "Steering": from T + 39 the speed eases toward the air's target by a
/// 32nd of the gap; with Right and Action held, toward 3 px per frame.
#[test]
fn steering_closes_a_32nd_of_the_gap() {
    let mut play = Play::new(1);
    play.until(89, &[]);
    play.step(&[R, A]);
    // (768 - 608) ÷ 32 = 5.
    assert_eq!(play.vx256(), 613.0);
    play.step(&[L]);
    // (-512 - 613) ÷ 32 → -35.
    assert_eq!(play.vx256(), 578.0);
    assert_eq!(play.world.player.facing, Facing::Right);
}

/// From the frame after the landing the usual rules apply: Jump pressed
/// on the landing frame starts a jump on the frame after it.
#[test]
fn a_jump_pressed_on_the_landing_frame_starts_on_the_next() {
    let mut play = Play::new(1);
    play.until(93, &[]);
    play.step(&[J]);
    assert!(play.world.events.contains(&Event::Land));
    play.step(&[J]);
    assert!(play.world.events.contains(&Event::JumpStart));
}

/// "The second character": at the start point up to and including T,
/// then where the lead was on the frame before, rounded down.
#[test]
fn the_second_character_is_a_frame_behind() {
    let mut play = Play::new(2);
    assert!(play.world.other.is_some());
    play.until(51, &[]);
    let at = |play: &Play| {
        let o = play.world.other.as_ref().unwrap().pos;
        (o.x.to_f64(), o.y.to_f64())
    };
    assert_eq!(at(&play), (100.0, 300.0));
    assert!(play.world.other_in_entrance());
    let mut before = play.at();
    while !play.world.player.grounded() {
        play.step(&[]);
        assert_eq!(at(&play), (before.0.floor(), before.1.floor()));
        assert!(!play.world.other_in_entrance());
        before = play.at();
    }
    assert_eq!(play.world.frame, 95);
    // Then it follows the lead, and comes to rest behind it on the
    // floor.
    for _ in 0..60 {
        play.step(&[]);
    }
    let (x, y) = at(&play);
    assert_eq!(y, 380.0);
    assert!((180.0..play.at().0).contains(&x), "{x}");
}

/// "The picture": after frame `f` the brightness is ⌊f / 3⌋ fifteenths,
/// full from frame 45.
#[test]
fn the_picture_fades_in_over_15_steps() {
    let mut play = Play::new(1);
    assert_eq!(play.world.brightness(), (0, 15));
    for f in 0..45 {
        play.step(&[]);
        assert_eq!(play.world.brightness(), (f / 3, 15), "frame {f}");
    }
    play.step(&[]);
    let (now, full) = play.world.brightness();
    assert_eq!(now, full);
}

/// Without an entrance, the level begins in play.
#[test]
fn without_an_entrance_play_starts_at_once() {
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let mut w = World::new(level(false), feel);
    assert!(w.entering.is_none() && !w.in_entrance());
    step(&mut w, &InputFrame::single(Frame::default()));
    assert!(w.player.pos.y > Fixed::from_int(300));
}

/// view.md, "When an area begins", example (c): the view through the
/// level's start, with nothing held.
#[test]
fn the_view_follows_the_entrance_as_the_example() {
    let mut play = Play::new(1);
    let rows: [(u32, f64, i32); 16] = [
        (0, 20.5, 0),
        (1, 20.5, -16),
        (2, 20.5, -30),
        (50, 20.13671875, -50),
        (51, 20.35546875, 1807),
        (52, 20.59375, 1965),
        (56, 21.84375, 3019),
        (57, 22.375, 4377),
        (58, 23.01953125, 5292),
        (60, 24.5390625, 6522),
        (61, 26.24609375, 7000),
        (66, 35.65625, 8039),
        (67, 39.63671875, 8159),
        (70, 50.953125, 7430),
        (71, 57.8984375, 7113),
        (80, 92.01953125, 2489),
    ];
    for (frame, v, s) in rows {
        play.until(frame, &[]);
        let view = play.world.view.unwrap();
        assert_eq!(view.left.to_f64(), v, "frame {frame}");
        assert_eq!(view.speed.to_f64() * 8192.0, f64::from(s), "frame {frame}");
    }
}
