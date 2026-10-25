//! Hits during a swap and while carrying an object, with both characters
//! in Classic 1 (spec/behavior/losing-a-character.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Param, Presets};
use clamber_sim::{Event, Facing, InputFrame, Other, Phase, World, generic_feel, step};

const S: Action = Action::Swap;
const A: Action = Action::Action;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(f(x), f(y))
}

fn feel() -> Arc<clamber_pack::Feel> {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    feel.world.set(Param::FollowTrail, Fixed::ZERO);
    Arc::new(feel)
}

/// Flat ground at y = 400, the entity `extra` and the characters `player`
/// (a roster slot) at `at` facing `facing` and the other one at `other`.
fn world(extra: &str, player: usize, at: f64, facing: Facing, other: f64) -> World {
    let src = format!(
        r#"
        format = 1
        name = "flat"
        size = [3000, 600]
        start = [{at}, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 3000, 200]

        {extra}
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "flat").unwrap());
    let mut world = World::new(level, feel());
    world.player.character = player;
    world.player.pos = v(at, 400.0);
    world.player.facing = facing;
    world.other = Some(Other::new(
        1 - player,
        v(other, 400.0),
        Facing::Left,
        v(at, 400.0),
    ));
    world
}

fn walker(speed: u32, at: f64, facing: &str) -> String {
    format!(
        r#"[[entities]]
        type = "crawler"
        at = [{at}, 400]
        params = {{ speed = {speed}, facing = "{facing}", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }}"#
    )
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(world: World) -> Play {
        let mut play = Play {
            world,
            held: Actions::NONE,
        };
        play.hold(&[], 2);
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

    fn hurt(&self) -> bool {
        self.world.events.contains(&Event::Hurt)
    }

    /// Presses swap on Q, and returns whether anyone was hurt on each of
    /// the frames Q to `e` and the character under control on `e`.
    fn swap_hurts(&mut self, e: usize) -> (Vec<bool>, usize) {
        self.step(&[S]);
        let mut hurts = vec![self.hurt()];
        for _ in 1..=e {
            self.step(&[]);
            hurts.push(self.hurt());
        }
        (hurts, self.world.player.character)
    }
}

#[test]
fn ground_walkers_stand_still_from_the_frame_after_the_press_to_the_frame_before_e() {
    for (player, e) in [(0, 79), (1, 72)] {
        let mut play = Play::new(world(
            &walker(1, 520.0, "left"),
            player,
            400.0,
            Facing::Right,
            340.0,
        ));
        let x = |play: &Play| play.world.things[0].pos.x;
        let before = x(&play);
        play.step(&[S]);
        assert_ne!(x(&play), before, "Q: moves as without the swap");
        let at_q = x(&play);
        for n in 1..e {
            play.step(&[]);
            assert_eq!(x(&play), at_q, "player {player}, Q + {n}");
        }
        play.step(&[]);
        assert_ne!(x(&play), at_q, "player {player}: moves again on E");
    }
}

#[test]
fn what_touches_the_character_that_takes_over_hurts_it_on_the_frame_after_e() {
    for (player, e) in [(0, 79), (1, 72)] {
        // Standing on the character that takes over, facing right so that
        // the quirk on E doesn't apply.
        let other = 340.0;
        let mut play = Play::new(world(
            &walker(0, other, "right"),
            player,
            400.0,
            Facing::Right,
            other,
        ));
        let (hurts, who) = play.swap_hurts(e);
        assert!(hurts.iter().all(|&h| !h), "player {player}: {hurts:?}");
        assert_eq!(who, 1 - player);
        play.step(&[]);
        assert!(play.hurt(), "player {player}: hurt on E + 1");
        assert!(play.world.loss.is_some());
        assert_eq!(play.world.player.character, 1 - player);
    }
}

#[test]
fn a_walker_not_touching_by_the_end_of_e_hurts_nobody_after_the_swap() {
    let mut play = Play::new(world(
        &walker(0, 900.0, "left"),
        0,
        400.0,
        Facing::Right,
        340.0,
    ));
    let (hurts, _) = play.swap_hurts(79);
    assert!(hurts.iter().all(|&h| !h));
    play.hold(&[], 5);
    assert!(!play.hurt() && play.world.loss.is_none());
}

#[test]
fn a_walker_heading_left_next_to_the_first_character_hurts_the_second_on_e() {
    // The first character ends at 340; the walker is 20 to its right.
    let mut play = Play::new(world(
        &walker(0, 360.0, "left"),
        0,
        400.0,
        Facing::Right,
        340.0,
    ));
    let (hurts, who) = play.swap_hurts(79);
    assert!(hurts[..79].iter().all(|&h| !h), "{hurts:?}");
    assert!(hurts[79], "hurt on E");
    assert_eq!(who, 1);
    assert!(play.world.loss.is_some());
    assert_eq!(play.world.player.pos, v(400.0, 400.0));
}

#[test]
fn the_quirk_on_e_has_a_range_and_is_for_the_first_character_only() {
    // 31 px: out of range.
    let mut play = Play::new(world(
        &walker(0, 371.0, "left"),
        0,
        400.0,
        Facing::Right,
        340.0,
    ));
    let (hurts, _) = play.swap_hurts(79);
    assert!(!hurts[79]);
    // 29 px, but the second character is the player.
    let mut play = Play::new(world(
        &walker(0, 369.0, "left"),
        1,
        400.0,
        Facing::Right,
        340.0,
    ));
    let (hurts, _) = play.swap_hurts(72);
    assert!(!hurts[72]);
    // Heading right.
    let mut play = Play::new(world(
        &walker(0, 360.0, "right"),
        0,
        400.0,
        Facing::Right,
        340.0,
    ));
    let (hurts, _) = play.swap_hurts(79);
    assert!(!hurts[79]);
}

fn carrying(facing: Facing) -> Play {
    let crate_at = if facing == Facing::Left { 180 } else { 220 };
    let extra = format!(
        r#"[[entities]]
        type = "crate"
        at = [{crate_at}, 400]"#
    );
    let mut play = Play::new(world(&extra, 0, 200.0, facing, 138.0));
    play.hold(&[A], 21);
    assert!(play.world.player.hands.is_some());
    assert_eq!(play.world.player.facing, facing);
    play
}

#[test]
fn the_first_characters_barrel_is_let_go_on_h_and_moves_from_the_next_frame() {
    for (facing, way) in [(Facing::Right, 1.0), (Facing::Left, -1.0)] {
        let mut play = carrying(facing);
        let held = play.world.things[0].pos;
        play.world.player.hurt_ahead = Some(0);
        play.step(&[A]);
        assert!(play.hurt());
        let t = &play.world.things[0];
        assert_eq!(t.pos, held, "H: where it was held");
        assert_eq!(t.vx, f(way));
        assert!(
            matches!(t.phase, Phase::LetGo { vy, .. } if vy == f(-4.0)),
            "{:?}",
            t.phase
        );
        assert_eq!(play.world.player.facing, Facing::Right);
        play.step(&[A]);
        let t = &play.world.things[0];
        assert_eq!(t.pos, Vec2::new(held.x + f(way), held.y + f(-4.0 + 0.4375)));
    }
}

#[test]
fn the_second_characters_barrel_is_gone_the_frame_before_h() {
    let extra = r#"[[entities]]
        type = "crate"
        at = [220, 400]

        [[entities]]
        type = "stinger"
        at = [300, 380]
        params = { reach = [6, 6, 3, 3], velocity = [0, 0] }"#;
    let mut play = Play::new(world(extra, 1, 200.0, Facing::Right, 138.0));
    play.hold(&[A], 21);
    assert!(play.world.player.hands.is_some(), "carrying");
    // Walking into the stinger: it touches the character at the end of
    // H - 1 and hurts it on H.
    let mut walked = 0;
    while play.world.things[0].active {
        assert!(!play.hurt() && walked < 200);
        play.step(&[A, Action::Right]);
        walked += 1;
    }
    assert!(!play.hurt(), "gone on H - 1");
    assert!(play.world.player.hands.is_none());
    play.step(&[A, Action::Right]);
    assert!(play.hurt(), "hurt on H");
    assert!(play.world.loss.is_some());
}
