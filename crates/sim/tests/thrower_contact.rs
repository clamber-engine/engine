//! Touching the thrower in Classic 1 (spec/behavior/thrower-contact.md):
//! when it hurts the player, when the player bounces off it, and what a
//! bounce does to it. Balls are removed as soon as they appear, as in
//! the spec's measurements.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{
    Event, Facing, FlowEvent, InputFrame, Phase, ROLL, START_LIVES, State, World, generic_feel,
    step,
};

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// `v` rounded up.
fn up(v: Fixed) -> i64 {
    -(-v).floor()
}

/// Ground at y = 381 from x = 0 to 3000, the player at `start` on it,
/// and a thrower at (1000, `y`): with `y` 380 it sits on the ground as
/// on its ledge.
fn world(start: i32, y: i32) -> World {
    let src = format!(
        r#"
        format = 1
        name = "thrower contact"
        size = [3000, 600]
        start = [{start}, 381]

        [[collision]]
        shape = "rect"
        rect = [0, 381, 3000, 100]

        [[entities]]
        type = "thrower"
        at = [1000, {y}]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "thrower contact").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(start: i32, y: i32) -> Play {
        Play {
            world: world(start, y),
            held: Actions::NONE,
        }
    }

    /// One step, with the balls thrown on it removed; whether the player
    /// was hurt on it.
    fn step(&mut self, keys: &[Action]) -> bool {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        let placed = self.world.level.entities.len();
        for ball in &mut self.world.things[placed..] {
            ball.active = false;
        }
        self.world.events.contains(&Event::Hurt)
    }

    /// Settles the player on the ground, standing, with the thrower's
    /// clock far from its throws at the end of the last step.
    fn settle(&mut self) {
        for _ in 0..20 {
            self.step(&[]);
        }
        self.world.things[0].clock = 60;
    }

    /// Puts the player standing at `x`, facing `facing`.
    fn stand_at(&mut self, x: f64, facing: Facing) {
        self.world.player.pos.x = f(x);
        self.world.player.facing = facing;
    }
}

/// thrower-contact.md, "Being hurt": standing on the ledge only `across`
/// counts, upright's range: -26 to 33 facing right, -32 to 27 facing
/// left, one frame after it.
#[test]
fn on_the_ledge_the_upright_range_hurts_on_the_next_frame() {
    for (x, facing, hurts) in [
        (973.5, Facing::Right, false),
        (974.0, Facing::Right, true),
        (1033.75, Facing::Right, true),
        (1034.0, Facing::Right, false),
        (967.5, Facing::Left, false),
        (968.0, Facing::Left, true),
        (1027.0, Facing::Left, true),
        (1028.0, Facing::Left, false),
    ] {
        let mut play = Play::new(900, 380);
        play.settle();
        play.stand_at(x, facing);
        // Within range at the end of this step,
        assert!(!play.step(&[]), "{x} {facing:?}");
        assert_eq!(play.world.player.pos.x, f(x));
        // hurt on the next.
        assert_eq!(play.step(&[]), hurts, "{x} {facing:?}");
    }
}

/// thrower-contact.md, "Being hurt", 3: never on the frame before a
/// throw (its second, on frame 33 of its timetable); a player still
/// within reach then is hurt on the throw's frame.
#[test]
fn not_hurt_on_the_frame_before_a_throw() {
    let mut play = Play::new(900, 380);
    play.settle();
    play.stand_at(1000.0, Facing::Right);
    // This step is frame 31 of the timetable, the next is 32.
    play.world.things[0].clock = 31;
    assert!(!play.step(&[]));
    assert_eq!(play.world.player.hurt_ahead, None);
    // Frame 32, the one before the throw.
    assert!(!play.step(&[]));
    assert_eq!(play.world.player.hurt_ahead, Some(0));
    // Frame 33, the throw's.
    assert!(play.step(&[]));
}

/// thrower-contact.md, "Being hurt", 2: a player who lands on the ledge
/// within range isn't hurt on L + 1, and is on L + 2 if still within it
/// at the end of L + 1.
#[test]
fn landing_within_range_hurts_a_frame_later() {
    let mut play = Play::new(900, 380);
    play.settle();
    let mut held = vec![Action::Jump];
    let mut landed = false;
    for n in 0..120 {
        let in_air = !play.world.player.grounded();
        assert!(!play.step(&held), "frame {n}");
        if n > 0 {
            held.clear();
        }
        if in_air && play.world.player.grounded() {
            landed = true;
            break;
        }
    }
    assert!(landed);
    // Within range, facing right, at the end of the landing frame L.
    play.stand_at(1010.0, Facing::Right);
    assert!(!play.step(&[]), "L + 1");
    assert!(play.step(&[]), "L + 2");
}

/// Puts the player in the air at `pos` after its move on the next step,
/// falling: at `pos` less this step's fall, with `vy` before gravity.
fn fall_into(play: &mut Play, pos: Vec2, vy: f64, facing: Facing) {
    let player = &mut play.world.player;
    player.ground = None;
    player.state = State::Fall;
    player.facing = facing;
    player.vel = Vec2::new(Fixed::ZERO, f(vy));
    player.pos = pos;
}

/// thrower-contact.md, "Bouncing" and "What a bounce does to the
/// thrower": falling with I 32 and U 20, the player bounces; the thrower
/// holds still on B, B + 1 and B + 2, then rises at 5 less 7/16 a frame
/// and falls, and throws no more.
#[test]
fn a_bounce_knocks_the_thrower_out() {
    let mut play = Play::new(900, 200);
    play.settle();
    // In open air: U is 200 less the player's y, rounded up.
    fall_into(
        &mut play,
        Vec2::new(f(1032.0), f(179.0)),
        0.5,
        Facing::Right,
    );
    play.step(&[]);
    assert!(play.world.events.contains(&Event::Bounce));
    assert_eq!(up(play.world.player.pos.y), 180);
    let thrower = |play: &Play| play.world.things[0].clone();
    assert!(matches!(thrower(&play).phase, Phase::Falling { .. }));
    assert_eq!(thrower(&play).pos.y, f(200.0));
    // Not hurt on B + 1 though within range.
    assert!(!play.step(&[]));
    assert_eq!(thrower(&play).pos.y, f(200.0), "B + 1");
    play.step(&[]);
    assert_eq!(thrower(&play).pos.y, f(200.0), "B + 2");
    play.step(&[]);
    assert_eq!(thrower(&play).pos.y, f(200.0 - 4.5625), "B + 3");
    play.step(&[]);
    assert_eq!(thrower(&play).pos.y, f(200.0 - 4.5625 - 4.125), "B + 4");
    let placed = play.world.level.entities.len();
    for _ in 0..200 {
        play.step(&[]);
        assert!(play.world.things[placed..].iter().all(|t| !t.active));
        if !play.world.things[0].active {
            break;
        }
    }
}

/// thrower-contact.md, "Bouncing", 3 and 4: the range's edges, facing
/// right and left, a player up to 20 px below it bouncing too.
#[test]
fn the_bounce_range() {
    for (x, y, facing, bounces) in [
        (971.0, 190.0, Facing::Right, false),
        (972.0, 190.0, Facing::Right, true),
        (1043.0, 190.0, Facing::Right, true),
        (1044.0, 190.0, Facing::Right, false),
        (958.0, 190.0, Facing::Left, true),
        (957.5, 190.0, Facing::Left, false),
        (1029.0, 190.0, Facing::Left, true),
        (1030.0, 190.0, Facing::Left, false),
        // U 27 and 28, then -20 and -21.
        (1000.0, 173.0, Facing::Right, true),
        (1000.0, 172.0, Facing::Right, false),
        (1000.0, 220.0, Facing::Right, true),
        (1000.0, 221.0, Facing::Right, false),
    ] {
        let mut play = Play::new(900, 200);
        play.settle();
        // vy 0.5 before gravity, so y after the move is `y` + 0.5 plus
        // gravity: start far enough below a whole px that it rounds up
        // to `y`.
        fall_into(&mut play, Vec2::new(f(x), f(y - 1.0)), 0.0, facing);
        play.step(&[]);
        assert_eq!(up(play.world.player.pos.y), y as i64, "{x} {y}");
        assert_eq!(
            play.world.events.contains(&Event::Bounce),
            bounces,
            "{x} {y} {facing:?}"
        );
    }
}

/// thrower-contact.md, "Bouncing", 2: rising at 1.25 px a frame or
/// faster after gravity, the player doesn't bounce.
#[test]
fn rising_fast_doesnt_bounce() {
    for (vy, bounces) in [(-1.6875, false), (-1.6, true)] {
        let mut play = Play::new(900, 200);
        play.settle();
        fall_into(&mut play, Vec2::new(f(1000.0), f(195.0)), vy, Facing::Right);
        play.step(&[]);
        assert_eq!(
            play.world.events.contains(&Event::Bounce),
            bounces,
            "vy {vy}"
        );
    }
}

impl Play {
    /// Whether the thrower has been knocked out.
    fn knocked_out(&self) -> bool {
        self.world.things[0].phase != Phase::Ready
    }

    /// I, the player's `x` less the thrower's, both rounded down.
    fn across(&self) -> i64 {
        self.world.player.pos.x.floor() - self.world.things[0].pos.x.floor()
    }

    /// Starts a roll toward `way` from far enough away, and rolls on to
    /// its sixth frame.
    fn roll_to_frame_5(&mut self, way: Action) {
        self.step(&[way, Action::Action]);
        for _ in 0..5 {
            self.step(&[way]);
        }
        assert_eq!(self.world.player.state, State::Roll);
    }
}

/// thrower-contact.md, "When the thrower is defeated": on an attack
/// frame, I at the end of it from -24 to 33 facing right and from -32 to
/// 25 facing left, ends included; nothing else happens to a player
/// rolling outside them.
#[test]
fn a_roll_defeats_the_thrower_within_its_range() {
    for (way, start, facing, range) in [
        (Action::Right, 900, Facing::Right, -24..=33),
        (Action::Left, 1100, Facing::Left, -32..=25),
    ] {
        let mut seen = (false, false);
        for n in 0..240 {
            let mut play = Play::new(start, 380);
            play.settle();
            play.roll_to_frame_5(way);
            // Put the player 3/8 px apart around the range.
            play.world.player.pos.x = f(960.0 + f64::from(n) * 0.375);
            assert!(!play.step(&[way]));
            let across = play.across();
            assert_eq!(play.world.player.facing, facing);
            let inside = range.contains(&across);
            assert_eq!(play.knocked_out(), inside, "{facing:?} I {across}");
            seen.0 |= across == *range.start();
            seen.1 |= across == *range.end();
        }
        assert!(seen.0 && seen.1, "{facing:?}");
    }
}

/// thrower-contact.md, "After the defeat": the player pauses on K + 1
/// and K + 2 and rolls on from K + 3, and the thrower holds still on K to
/// K + 2, rises at 5 less 7/16 a frame from K + 3, and throws no more.
#[test]
fn after_a_roll_defeat_both_pause() {
    let mut play = Play::new(940, 380);
    play.settle();
    play.roll_to_frame_5(Action::Right);
    let mut defeat = None;
    for n in 0..60 {
        play.step(&[Action::Right]);
        if play.knocked_out() {
            defeat = Some(n);
            break;
        }
    }
    assert!(defeat.is_some());
    let at_k = play.world.player.pos.x;
    let thrower = |play: &Play| play.world.things[0].pos.y;
    assert_eq!(thrower(&play), f(380.0), "K");
    for k in 1..=2 {
        assert!(!play.step(&[Action::Right]));
        assert_eq!(play.world.player.pos.x, at_k, "K + {k}");
        assert_eq!(thrower(&play), f(380.0), "K + {k}");
    }
    play.step(&[Action::Right]);
    assert!(play.world.player.pos.x > at_k, "K + 3");
    assert_eq!(thrower(&play), f(380.0 - 4.5625), "K + 3");
    let placed = play.world.level.entities.len();
    for _ in 0..300 {
        play.step(&[]);
        assert!(play.world.things[placed..].iter().all(|t| !t.active));
    }
}

/// thrower-contact.md, "When the thrower is defeated": on S the
/// thrower is also defeated from the player's position at the end of
/// S - 1. Here the player ends S - 1 at I 33, on the frame before a
/// throw so it isn't hurt, and the roll's first frame moves it on.
#[test]
fn on_the_rolls_first_frame_where_it_was_counts_too() {
    let mut play = Play::new(900, 380);
    play.settle();
    play.stand_at(1033.5, Facing::Right);
    // S is frame 32 of the timetable, the one before a throw.
    play.world.things[0].clock = 31;
    play.world.player.vel.x = f(2.5);
    play.world.player.hurt_ahead = None;
    play.step(&[Action::Right, Action::Action]);
    assert_eq!(play.world.player.state, State::Roll);
    assert!(play.across() > 33, "I {}", play.across());
    assert!(play.knocked_out());
}

/// thrower-contact.md, "Being hurt around a roll": after a roll that
/// ends in a stop, on D + 8 to D + 11 the thrower neither is defeated
/// nor hurts the player; the earliest hurt is on D + 12.
#[test]
fn not_defeated_or_hurt_late_in_the_stop() {
    let mut play = Play::new(900, 380);
    play.settle();
    // A slow roll, far from the thrower, ends in a stop.
    play.step(&[Action::Action]);
    let mut d = None;
    for n in 1..80u32 {
        let hurt = play.step(&[]);
        play.world.things[0].clock = 60;
        if d.is_none() && play.world.player.roll_stop > 0 {
            d = Some(n);
        }
        let Some(d) = d else {
            assert!(!hurt);
            continue;
        };
        if n == d + 7 {
            // The player stands still by now; put it on the thrower.
            play.stand_at(1000.0, Facing::Right);
        }
        if n < d + 12 {
            assert!(!hurt && !play.knocked_out(), "D + {}", n - d);
        } else {
            assert!(hurt && !play.knocked_out(), "D + {}", n - d);
            return;
        }
    }
    panic!("no stop");
}

/// thrower-contact.md, "The knocked-out thrower": it falls through every
/// floor, and is removed on the first frame on which, after its move, it
/// is more than 256 px below the top of the picture as it was at the
/// start of the frame. It never comes back.
#[test]
fn a_knocked_out_thrower_leaves_by_the_balls_edges() {
    let mut play = Play::new(940, 380);
    play.settle();
    play.roll_to_frame_5(Action::Right);
    for _ in 0..60 {
        play.step(&[Action::Right]);
        if play.knocked_out() {
            break;
        }
    }
    assert!(play.knocked_out());
    let mut removed = false;
    for _ in 0..200 {
        let top = play.world.view.unwrap().y();
        play.step(&[]);
        let thrower = &play.world.things[0];
        let below = up(thrower.pos.y) - top;
        if !thrower.active {
            assert!(below > 256, "{below}");
            // Through the ground at 381.
            assert!(thrower.pos.y > f(381.0));
            removed = true;
            break;
        }
        assert!(below <= 256, "{below}");
    }
    assert!(removed);
    for _ in 0..100 {
        play.step(&[Action::Left]);
        assert!(!play.world.things[0].active);
    }
}

impl Play {
    /// Knocks the player out and waits for play to resume.
    fn resume(&mut self) {
        self.world.player.hurt_ahead = Some(0);
        for _ in 0..600 {
            self.step(&[]);
            if self.world.flow.events.contains(&FlowEvent::Respawn) {
                assert!(self.world.flow.lives < START_LIVES);
                return;
            }
        }
        panic!("play never resumed");
    }

    /// Whether the thrower is at its place, not knocked out.
    fn thrower_back(&self, y: i32) -> bool {
        let thrower = &self.world.things[0];
        thrower.active && thrower.phase == Phase::Ready && thrower.pos == Vec2::ints(1000, y)
    }
}

/// thrower-contact.md, "The knocked-out thrower", and restart-barrel.md,
/// "What is there again after a knock-out": a thrower knocked out by a
/// bounce or a roll, falling or already removed, is at its place again
/// once play resumes after the player's knock-out.
#[test]
fn a_thrower_knocked_out_by_a_bounce_or_a_roll_is_there_after_a_knock_out() {
    for wait in [5, 200] {
        let mut play = Play::new(900, 200);
        play.settle();
        fall_into(
            &mut play,
            Vec2::new(f(1032.0), f(179.0)),
            0.5,
            Facing::Right,
        );
        play.step(&[]);
        assert!(play.knocked_out(), "bounced on");
        for _ in 0..wait {
            play.step(&[]);
        }
        play.resume();
        play.step(&[]);
        assert!(play.thrower_back(200), "after a bounce, {wait} frames on");

        let mut play = Play::new(900, 380);
        play.settle();
        play.roll_to_frame_5(Action::Right);
        play.world.player.pos.x = f(1000.0);
        play.step(&[Action::Right]);
        assert!(play.knocked_out(), "rolled into");
        for _ in 0..wait {
            play.step(&[]);
        }
        play.resume();
        play.step(&[]);
        assert!(play.thrower_back(380), "after a roll, {wait} frames on");
    }
}

/// A play with the second character under control.
fn second_play(start: i32, y: i32) -> Play {
    let mut play = Play::new(start, y);
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::from([(ROLL.to_string(), BTreeMap::new())]),
    });
    play.world.feel = Arc::new(feel);
    play.world.player.character = 1;
    play
}

/// thrower-contact.md, "The second character under control": standing
/// on the ledge, I from −23 to 23 facing right and −22 to 24 facing
/// left, narrower than the first character's.
#[test]
fn the_second_character_standing_has_its_own_hurt_range() {
    for (x, facing, hurts) in [
        (976.5, Facing::Right, false),
        (977.0, Facing::Right, true),
        (1023.5, Facing::Right, true),
        (1024.0, Facing::Right, false),
        (977.5, Facing::Left, false),
        (978.0, Facing::Left, true),
        (1024.0, Facing::Left, true),
        (1025.0, Facing::Left, false),
    ] {
        let mut play = second_play(900, 380);
        play.settle();
        play.stand_at(x, facing);
        assert!(!play.step(&[]), "{x} {facing:?}");
        assert_eq!(play.step(&[]), hurts, "{x} {facing:?}");
    }
}

/// The same, walking: I from −29 to 24 facing right.
#[test]
fn the_second_character_walking_has_the_moving_range() {
    let mut play = second_play(900, 380);
    play.settle();
    play.stand_at(960.0, Facing::Right);
    let mut hurt_at = None;
    for _ in 0..60 {
        let hurt = play.step(&[Action::Right]);
        if hurt {
            hurt_at = Some(play.world.player.pos.x.floor() - 1000);
            break;
        }
    }
    // Hurt one frame after the first end of a frame with I −29 or more.
    let at = hurt_at.expect("hurt");
    assert!((-28..=-26).contains(&at), "{at}");
}

/// thrower-contact.md, "The second character under control": bounce
/// ranges I −38 to 29 facing right and −28 to 39 facing left, U from −13
/// to 34.
#[test]
fn the_second_character_bounce_range() {
    for (x, y, facing, bounces) in [
        (961.0, 190.0, Facing::Right, false),
        (962.0, 190.0, Facing::Right, true),
        (1029.0, 190.0, Facing::Right, true),
        (1030.0, 190.0, Facing::Right, false),
        (971.0, 190.0, Facing::Left, false),
        (972.0, 190.0, Facing::Left, true),
        (1039.0, 190.0, Facing::Left, true),
        (1040.0, 190.0, Facing::Left, false),
        // U 34 and 35, then −13 and −14.
        (1000.0, 166.0, Facing::Right, true),
        (1000.0, 165.0, Facing::Right, false),
        (1000.0, 213.0, Facing::Right, true),
        (1000.0, 214.0, Facing::Right, false),
    ] {
        let mut play = second_play(900, 200);
        play.settle();
        fall_into(&mut play, Vec2::new(f(x), f(y - 1.0)), 0.0, facing);
        play.step(&[]);
        assert_eq!(up(play.world.player.pos.y), y as i64, "{x} {y}");
        assert_eq!(
            play.world.events.contains(&Event::Bounce),
            bounces,
            "{x} {y} {facing:?}"
        );
    }
}
