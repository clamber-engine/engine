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

/// The example's landing frame, L.
const LANDING: u32 = 94;

/// What a run with `keys(frame)` held on each frame did from the
/// landing on: the first frame facing left, and the frames a jump and a
/// roll started on, if any, up to L + 18.
#[derive(Debug, PartialEq, Eq)]
struct Landing {
    left_from: Option<u32>,
    jump: Option<u32>,
    roll: Option<u32>,
}

fn landing(keys: impl Fn(u32) -> Vec<Action>) -> (Landing, Play) {
    landing_with(false, keys)
}

fn landing_with(immediate: bool, keys: impl Fn(u32) -> Vec<Action>) -> (Landing, Play) {
    let mut play = Play::new(1);
    play.world.feel = Arc::new(clamber_pack::Feel {
        immediate,
        ..clamber_pack::Feel::clone(&play.world.feel)
    });
    let mut seen = Landing {
        left_from: None,
        jump: None,
        roll: None,
    };
    let mut after_landing = None;
    while play.world.frame <= LANDING + 18 {
        let frame = play.world.frame;
        play.step(&keys(frame));
        if frame == LANDING {
            assert!(play.world.events.contains(&Event::Land), "frame {frame}");
            after_landing = Some(Play {
                world: play.world.clone(),
                held: play.held,
            });
        }
        if play.world.player.facing == Facing::Left {
            seen.left_from.get_or_insert(frame);
        }
        if play.world.events.contains(&Event::JumpStart) {
            seen.jump.get_or_insert(frame);
        }
        if play.world.events.contains(&Event::RollStart) {
            seen.roll.get_or_insert(frame);
        }
    }
    (seen, after_landing.unwrap())
}

fn held(keys: &[Action], from: u32, to: u32) -> impl Fn(u32) -> Vec<Action> {
    let keys = keys.to_vec();
    move |f| match (from..=to).contains(&f) {
        true => keys.clone(),
        false => Vec::new(),
    }
}

/// "The landing": a mode starts on L itself, set by what is held on L.
/// The examples' turns.
#[test]
fn a_turn_starts_on_the_landing_frame() {
    let l = LANDING;
    for (keys, left_from) in [
        (held(&[L], 30, l + 2), Some(l + 4)),
        (held(&[L], l, l), Some(l + 4)),
        (held(&[L], l + 1, l + 1), Some(l + 5)),
        (held(&[], 0, 0), None),
    ] {
        assert_eq!(landing(keys).0.left_from, left_from);
    }
}

/// With Action and a direction held on L, running starts on L, facing
/// still right even with Left held; with Right alone, walking. From
/// L + 1 the usual rules apply.
#[test]
fn running_or_walking_starts_on_the_landing_frame() {
    use clamber_sim::Gait;
    for (keys, running) in [(&[R, A][..], true), (&[L, A], true), (&[R], false)] {
        let (seen, play) = landing(held(keys, 30, LANDING + 18));
        let gait = play.world.player.gait;
        assert_eq!(play.world.player.facing, Facing::Right, "{keys:?}");
        assert_eq!(play.world.player.turn, None, "{keys:?}");
        match running {
            true => assert!(matches!(gait, Gait::Running(_)), "{keys:?}: {gait:?}"),
            false => assert!(matches!(gait, Gait::Walking(_)), "{keys:?}: {gait:?}"),
        }
        assert_ne!(seen.left_from, Some(LANDING), "{keys:?}");
    }
}

/// Jumps use the bounce press window, E = L + 2.
#[test]
fn jumps_use_the_bounce_window() {
    let l = LANDING;
    for (from, to, jump) in [
        (l - 10, l + 1, Some(l + 2)),
        (l - 11, l + 8, None),
        (30, l + 8, None),
        (l, l, None),
        (l + 1, l + 1, Some(l + 2)),
        (l + 2, l + 2, Some(l + 3)),
        (l - 4, l, None),
    ] {
        let (seen, _) = landing(held(&[J], from, to));
        assert_eq!(seen.jump, jump, "Jump held {from} to {to}");
    }
}

/// #437: with presses answered on their own frame, a press on L + 2
/// starts the jump on L + 2, not L + 3; the window before it is as is.
#[test]
fn an_immediate_jump_press_after_the_landing_starts_on_its_frame() {
    let l = LANDING;
    for (from, to, jump) in [
        (l + 2, l + 2, Some(l + 2)),
        (l + 5, l + 5, Some(l + 5)),
        (l - 10, l + 1, Some(l + 2)),
        (l, l, None),
    ] {
        let (seen, _) = landing_with(true, held(&[J], from, to));
        assert_eq!(seen.jump, jump, "Jump held {from} to {to}");
    }
}

/// A press of Action on L starts no roll, however long it is held, and
/// one in the air isn't remembered; a press on L + 1 or later rolls on
/// that frame (spec-v0.328.0).
#[test]
fn rolls_start_from_presses_on_the_ground() {
    let l = LANDING;
    for (from, to, roll) in [
        (l, l, None),
        (l, l + 33, None),
        (l - 2, l + 3, None),
        (l + 1, l + 1, Some(l + 1)),
        (l + 2, l + 2, Some(l + 2)),
    ] {
        let (seen, play) = landing(held(&[A], from, to));
        assert_eq!(seen.roll, roll, "Action held {from} to {to}");
        if from == l && to == l {
            assert!(matches!(
                play.world.player.gait,
                clamber_sim::Gait::Standing
            ));
        }
    }
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

/// "The second character" after the landing: on L + 1 it stays where it
/// was on L, on L + 2 it is at the lead's position of L, rounded down,
/// then it follows as usual (spec-v0.328.0).
#[test]
fn the_second_character_stays_on_the_frame_after_the_landing() {
    let mut play = Play::new(2);
    let other = |play: &Play| {
        let o = play.world.other.as_ref().unwrap().pos;
        (o.x.to_f64(), o.y.to_f64())
    };
    let mut lead_before = play.at();
    while !play.world.player.grounded() {
        lead_before = play.at();
        play.step(&[]);
    }
    let l = play.world.frame - 1;
    assert_eq!(l, LANDING);
    let lead_at_l = play.at();
    assert_eq!(other(&play), (lead_before.0.floor(), lead_before.1.floor()));
    let on_l = other(&play);
    play.step(&[]);
    assert_eq!(other(&play), on_l, "L + 1");
    play.step(&[]);
    assert_eq!(other(&play), (lead_at_l.0.floor(), play.at().1), "L + 2");
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

/// view-vertical.md, "When an area begins", the level's start: the
/// view's top keeps its frame 0 target (feet line 300, so 140.5), shakes
/// 5 px down two frames after the entrance bursts and 5 px up four frames
/// later, and from the fast frames aims by the feet.
#[test]
fn the_view_holds_its_target_and_shakes_vertically() {
    let mut play = Play::new(1);
    let top = |play: &Play| play.world.view.unwrap().top.to_f64();
    play.until(0, &[]);
    assert_eq!(top(&play), 140.5);
    play.until(42, &[]);
    assert_eq!(top(&play), 140.5);
    play.until(43, &[]);
    assert_eq!(top(&play), 145.5);
    play.until(47, &[]);
    // Q eased toward the 5 px gap above, and the step took 5 px off.
    assert!(top(&play) < 140.5 && top(&play) > 140.0);
    // Thrown up out of the entrance on the fast frames, the view rises.
    play.until(66, &[]);
    assert!(top(&play) < 135.0, "{}", top(&play));
}
