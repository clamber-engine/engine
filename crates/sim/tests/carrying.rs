//! Picking up, carrying, throwing and putting down objects in Classic 1
//! (spec/behavior/carrying.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::Fixed;
use clamber_sim::{Act, Event, Facing, FlowEvent, InputFrame, Phase, World, generic_feel, step};

use Action::{Down, Jump, Left, Right, Up};

const A: Action = Action::Action;

const FLOOR: f64 = 400.0;

/// Flat ground at y = 400 from x = 0 to `floor_end`, the player at
/// x = 200 and a crate on the floor at `crate_x`.
fn level(floor_end: u32, crate_x: u32) -> String {
    walled(floor_end, crate_x, "")
}

/// The same with more `collision` after the floor.
fn walled(floor_end: u32, crate_x: u32, more: &str) -> String {
    format!(
        r#"
        format = 1
        name = "carry"
        size = [3000, 800]
        start = [200, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, {floor_end}, 200]
        {more}

        [[entities]]
        type = "crate"
        at = [{crate_x}, 400]
        "#
    )
}

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(f(x), f(y))
}

/// 1/256 px per frame.
fn speed(n: i32) -> Fixed {
    Fixed::from_ratio_pow2(n, 8)
}

/// What a step left behind.
#[derive(Clone, Debug)]
struct Row {
    pos: Vec2,
    vel: Vec2,
    ground: bool,
    facing: Facing,
    turn: Option<u8>,
    events: Vec<Event>,
    act: Option<Act>,
    thing: Vec2,
    phase: Phase,
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(floor_end: u32, crate_x: u32) -> Play {
        Play::from_src(&level(floor_end, crate_x))
    }

    fn from_src(src: &str) -> Play {
        let level = Arc::new(Level::from_toml_str(src, "carry").unwrap());
        let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
        let mut play = Play {
            world,
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        assert!(play.world.player.grounded());
        play
    }

    /// `near`, with the second character under control.
    fn second() -> Play {
        let level = Arc::new(Level::from_toml_str(&level(2000, 220), "carry").unwrap());
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(clamber_pack::CharacterFeel {
            physics: clamber_pack::Presets::builtin()
                .slot("classic-1", 1)
                .unwrap(),
            abilities: feel.roster[0].abilities.clone(),
        });
        let mut world = World::new(level, Arc::new(feel));
        world.player.character = 1;
        let mut play = Play {
            world,
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        assert!(play.world.player.grounded());
        play
    }

    /// Standing at x = 200 with a crate 20 px to the right, within reach.
    fn near() -> Play {
        Play::new(2000, 220)
    }

    fn step(&mut self, keys: &[Action]) -> Row {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        let w = &self.world;
        let p = &w.player;
        Row {
            pos: p.pos,
            vel: p.vel,
            ground: p.grounded(),
            facing: p.facing,
            turn: p.turn,
            events: w.events.clone(),
            act: p.hands.map(|h| h.act),
            thing: w.things[0].pos,
            phase: w.things[0].phase,
        }
    }

    fn hold(&mut self, keys: &[Action], frames: usize) -> Vec<Row> {
        (0..frames).map(|_| self.step(keys)).collect()
    }

    /// Picks up the crate standing still, and carries it for a while
    /// with action held alone.
    fn carrying() -> Play {
        let mut play = Play::near();
        let g = play.step(&[A]);
        assert_eq!(g.act, Some(Act::PickUp));
        play.hold(&[A], 20);
        assert_eq!(play.world.player.hands.unwrap().act, Act::Carry);
        play
    }
}

fn has(row: &Row, event: Event) -> bool {
    row.events.contains(&event)
}

#[test]
fn pressing_action_within_reach_picks_up_without_rolling() {
    let mut play = Play::near();
    let rows = play.hold(&[A], 16);
    assert!(rows.iter().all(|r| !has(r, Event::RollStart)));
    assert!(rows[..15].iter().all(|r| r.act == Some(Act::PickUp)));
    assert_eq!(rows[15].act, Some(Act::Carry));
    assert!(rows.iter().all(|r| r.phase == Phase::Held));
    // Out of reach, the same press rolls.
    let mut play = Play::new(2000, 240);
    let row = play.step(&[A]);
    assert_eq!(row.act, None);
    assert!(has(&row, Event::RollStart));
}

#[test]
fn moving_into_an_object_without_action_walks_past() {
    let mut play = Play::new(2000, 300);
    let rows = play.hold(&[Right], 120);
    assert!(rows.iter().all(|r| r.act.is_none()));
    assert!(rows.last().unwrap().pos.x > f(330.0));
}

/// carrying.md, "A pick-up at full running speed": Right and Action
/// held into the object. The pick-up starts on the frame after the first
/// that ends within reach.
#[test]
fn a_pick_up_at_full_running_speed() {
    let mut play = Play::new(2000, 600);
    let rows = play.hold(&[Right, A], 200);
    let g = rows.iter().position(|r| r.act.is_some()).unwrap();
    assert_eq!(rows[g].act, Some(Act::PickUp));
    // "Reach": facing right, the object's x less the player's, both
    // rounded down, is at most 38.
    let first_in_reach = rows
        .iter()
        .position(|r| 600 - r.pos.x.floor() <= 38)
        .unwrap();
    assert_eq!(g, first_in_reach + 1);
    assert_eq!(rows[g - 1].vel.x, speed(768));
    let x0 = rows[g - 1].pos.x;
    for (n, vx, x) in [
        (0, 672, 2.625),
        (1, 588, 4.921875),
        (9, 204, 15.51953125),
        (10, 274, 16.58984375),
        (14, 477, 22.984375),
        (15, 513, 24.98828125),
    ] {
        let row = &rows[g + n];
        assert_eq!(row.vel.x, speed(vx), "G + {n}");
        assert_eq!(row.pos.x - x0, f(x), "G + {n}");
    }
    assert_eq!(rows[g + 14].act, Some(Act::PickUp));
    assert_eq!(rows[g + 15].act, Some(Act::Carry));
}

#[test]
fn a_pick_up_keeps_facing_and_a_turn_starts_after_it() {
    let mut play = Play::near();
    play.step(&[A]);
    let rows = play.hold(&[Left, A], 14);
    assert!(
        rows.iter()
            .all(|r| r.facing == Facing::Right && r.turn.is_none())
    );
    let row = play.step(&[Left, A]);
    assert_eq!(row.act, Some(Act::Carry));
    assert_eq!(row.turn, Some(0));
}

/// A Jump press on G + 2, held for 20 frames, doesn't jump.
#[test]
fn jumps_during_a_pick_up_do_nothing() {
    let mut play = Play::near();
    let mut rows = play.hold(&[A], 2);
    rows.extend(play.hold(&[A, Jump], 20));
    rows.extend(play.hold(&[A], 20));
    assert!(rows.iter().all(|r| r.ground && !has(r, Event::JumpStart)));
}

/// carrying.md, "During the pick-up": letting go of action with down
/// held doesn't throw on that frame; the throw comes on the first frame
/// without down.
#[test]
fn letting_go_during_a_pick_up_with_down_held_throws_when_down_is_let_go_of() {
    let mut play = Play::near();
    play.hold(&[A], 5);
    for _ in 0..3 {
        let row = play.step(&[Down]);
        assert_ne!(row.act, Some(Act::Throw));
        assert_ne!(row.act, Some(Act::PutDown));
    }
    assert_eq!(play.step(&[]).act, Some(Act::Throw));

    // Without down it's a throw on the frame itself.
    let mut play = Play::near();
    play.hold(&[A], 5);
    assert_eq!(play.step(&[]).act, Some(Act::Throw));
}

/// Down held on every frame up to and including G + 15 puts the object
/// down from G + 16, whether or not down is still held then.
#[test]
fn down_held_through_the_pick_up_puts_the_object_down() {
    for last in [&[Down][..], &[][..]] {
        let mut play = Play::near();
        play.hold(&[A], 5);
        for _ in 0..11 {
            let row = play.step(&[Down]);
            assert_ne!(row.act, Some(Act::Throw));
            assert_ne!(row.act, Some(Act::PutDown));
        }
        assert_eq!(play.step(last).act, Some(Act::PutDown));
    }
}

/// Down let go of on G + 16 for the first time, after action was let go
/// of on G + 16 too, is a plain release: Down held puts down, none throws.
#[test]
fn letting_go_after_the_pick_up_is_as_before() {
    let mut play = Play::near();
    play.hold(&[A], 16);
    assert_eq!(play.step(&[Down]).act, Some(Act::PutDown));
    let mut play = Play::near();
    play.hold(&[A], 16);
    assert_eq!(play.step(&[]).act, Some(Act::Throw));
}

/// "The second character": the same with G + 10 as the last frame.
#[test]
fn the_second_character_s_throw_waits_for_down_the_same_way() {
    let mut play = Play::second();
    play.hold(&[A], 5);
    assert_ne!(play.step(&[Down]).act, Some(Act::Throw));
    assert_eq!(play.step(&[]).act, Some(Act::Throw));

    let mut play = Play::second();
    play.hold(&[A], 5);
    for _ in 0..6 {
        let row = play.step(&[Down]);
        assert_ne!(row.act, Some(Act::Throw));
        assert_ne!(row.act, Some(Act::PutDown));
    }
    assert_eq!(play.step(&[]).act, Some(Act::PutDown));
}

#[test]
fn carrying_moves_at_running_speed_and_down_and_up_do_nothing() {
    let mut play = Play::carrying();
    let rows = play.hold(&[Right, Down, A], 60);
    assert!(play.world.player.crouch.held == 0);
    assert_eq!(rows.last().unwrap().vel.x, speed(768));
    let rows = play.hold(&[Up, A], 60);
    assert_eq!(rows.last().unwrap().vel.x, Fixed::ZERO);
    assert!(rows.iter().all(|r| r.act == Some(Act::Carry)));
}

/// Carrying, a jump takes off on P + 2 rather than P + 1.
#[test]
fn a_jump_while_carrying_takes_off_a_frame_later() {
    let mut play = Play::carrying();
    let before = play.world.player.pos;
    let rows = play.hold(&[A, Jump], 3);
    assert!(!has(&rows[0], Event::JumpStart));
    assert!(!has(&rows[1], Event::JumpStart));
    let takeoff = &rows[2];
    assert!(has(takeoff, Event::JumpStart));
    assert_eq!(takeoff.vel.y, f(-7.0));
    assert!(takeoff.ground);
    assert_eq!(takeoff.pos.y, before.y);
    // Still holding it through the jump and after landing.
    let rows = play.hold(&[A, Jump], 60);
    assert!(rows.iter().all(|r| r.act == Some(Act::Carry)));
    assert!(rows.iter().any(|r| has(r, Event::Land)));

    // Without an object, on P + 1.
    let mut play = Play::new(2000, 1000);
    let rows = play.hold(&[Jump], 2);
    assert!(has(&rows[1], Event::JumpStart));
}

/// With the feel immediate (#437) a carried jump takes off on P, as a jump
/// without an object does.
#[test]
fn an_immediate_jump_while_carrying_takes_off_on_the_press() {
    let mut play = Play::carrying();
    let mut feel = (*play.world.feel).clone();
    feel.immediate = true;
    play.world.feel = Arc::new(feel);
    let rows = play.hold(&[A, Jump], 2);
    assert!(has(&rows[0], Event::JumpStart));
    assert_eq!(rows[0].vel.y, f(-7.0));
}

/// Letting go of action on P + 1 or P + 2 cancels the jump and throws
/// from the ground; on P + 3, the throw is in the air.
#[test]
fn letting_go_just_after_a_jump_press() {
    for wait in [1, 2] {
        let mut play = Play::carrying();
        play.hold(&[A, Jump], wait);
        let q = play.step(&[Jump]);
        assert_eq!(q.act, Some(Act::Throw), "P + {wait}");
        let rows = play.hold(&[Jump], 30);
        assert!(
            std::iter::once(&q)
                .chain(&rows)
                .all(|r| r.ground && !has(r, Event::JumpStart)),
            "P + {wait}"
        );
    }
    let mut play = Play::carrying();
    play.hold(&[A, Jump], 3);
    let q = play.step(&[Jump]);
    assert_eq!(q.act, Some(Act::Throw));
    assert!(!q.ground);
    // The object leaves the hands on Q + 21, its height rounded toward
    // the floor.
    let rows = play.hold(&[Jump], 21);
    let release = &rows[20];
    assert_ne!(release.pos.y.floor() as f64, release.pos.y.to_f64());
    let lift = Fixed::from_int((release.pos.y - f(54.0)).to_f64().ceil() as i32);
    assert!(matches!(release.phase, Phase::Thrown(_)));
    assert_eq!(
        release.thing,
        Vec2::new(Fixed::from_int(release.pos.x.floor() as i32), lift)
    );
    assert!(matches!(rows[19].phase, Phase::Held));
}

/// carrying.md, "A thrown object", thrown facing right from standing.
#[test]
fn a_thrown_object() {
    // The floor ends before the object comes down.
    let mut play = Play::new(240, 220);
    play.step(&[A]);
    play.hold(&[A], 20);
    let rows = play.hold(&[], 40);
    let at = |n: usize| &rows[n];
    assert_eq!(at(20).phase, Phase::Held);
    let start = at(21).thing;
    assert_eq!(start, v(200.0, FLOOR - 54.0));
    let flight = |n: usize| match at(n).phase {
        Phase::Thrown(flight) => flight,
        other => panic!("Q + {n}: {other:?}"),
    };
    for (n, x, y, vx, vy) in [
        (21, 0.0, 0.0, 4096, 256),
        (22, 14.4375, 1.4375, 3696, 368),
        (23, 27.5078125, 3.3125, 3346, 480),
        (24, 39.3828125, 5.625, 3040, 592),
        (25, 50.2109375, 8.375, 2772, 704),
        (30, 92.71875, 28.6875, 1860, 1264),
        (35, 123.0859375, 59.9375, 1393, 1824),
        (36, 128.28515625, 67.5, 1331, 1936),
        (37, 133.2734375, 75.5, 1277, 2048),
    ] {
        let row = at(n);
        assert_eq!(row.thing.x - start.x, f(x), "Q + {n}");
        assert_eq!(row.thing.y - start.y, f(y), "Q + {n}");
        assert_eq!(flight(n).vx, speed(vx), "Q + {n}");
        assert_eq!(flight(n).vy, speed(vy), "Q + {n}");
    }
    assert_eq!(flight(39).vy, f(8.0));
}

/// carrying.md, "A throw while running": carrying at full speed to the
/// right, action let go on Q with Right held throughout.
#[test]
fn a_throw_while_running() {
    let mut play = Play::carrying();
    let rows = play.hold(&[Right, A], 60);
    let before = rows.last().unwrap();
    assert_eq!(before.vel.x, speed(768));
    let x0 = before.pos.x;
    let rows = play.hold(&[Right], 44);
    for (n, vx, x) in [
        (0, 672, 2.625),
        (1, 588, 4.921875),
        (10, 179, 16.21875),
        (21, 43, 20.046875),
        (22, 101, 20.44140625),
        (23, 152, 21.03515625),
        (42, 480, 49.7890625),
        (43, 484, 51.6796875),
    ] {
        assert_eq!(rows[n].vel.x, speed(vx), "Q + {n}");
        assert_eq!(rows[n].pos.x - x0, f(x), "Q + {n}");
    }
    assert_eq!(rows[42].act, Some(Act::Throw));
    assert_eq!(rows[43].act, None);
}

/// On the ground, facing doesn't change during a throw; a turn starts on
/// Q + 43.
#[test]
fn facing_during_a_throw() {
    let mut play = Play::carrying();
    let rows = play.hold(&[Left], 44);
    assert!(
        rows[..43]
            .iter()
            .all(|r| r.facing == Facing::Right && r.turn.is_none())
    );
    assert!(rows[30].vel.x.is_negative());
    assert_eq!(rows[43].turn, Some(0));
}

#[test]
fn jumps_during_a_throw() {
    // Pressed before Q + 22: nothing, now or later.
    let mut play = Play::carrying();
    let mut rows = play.hold(&[], 10);
    rows.extend(play.hold(&[Jump], 30));
    assert!(rows.iter().all(|r| !has(r, Event::JumpStart)));

    // Pressed on Q + 22: a jump, taking off on the next frame.
    let mut play = Play::carrying();
    play.hold(&[], 22);
    let press = play.step(&[Jump]);
    assert_eq!(press.act, None);
    assert!(!has(&press, Event::JumpStart));
    assert!(has(&play.step(&[Jump]), Event::JumpStart));
}

#[test]
fn action_during_a_throw() {
    let rolls = |presses: &[(usize, usize)]| {
        let mut play = Play::carrying();
        let rows: Vec<Row> = (0..60)
            .map(|n| {
                let held = presses.iter().any(|&(from, to)| (from..to).contains(&n));
                play.step(if held { &[A][..] } else { &[] })
            })
            .collect();
        rows.iter().position(|r| has(r, Event::RollStart))
    };
    // Pressed on Q + 22 to Q + 42: a roll on the press frame.
    assert_eq!(rolls(&[(22, 30)]), Some(22));
    assert_eq!(rolls(&[(30, 40)]), Some(30));
    // Pressed on Q + 15 to Q + 21 and held on Q + 22: a roll on Q + 22.
    assert_eq!(rolls(&[(16, 30)]), Some(22));
    // Let go before Q + 22: nothing.
    assert_eq!(rolls(&[(16, 20)]), None);
    // Pressed on Q + 1 to Q + 14: nothing, even if held through Q + 22.
    assert_eq!(rolls(&[(5, 40)]), None);
}

/// carrying.md, "The second character", Jump and Action during a throw:
/// the first character's table three frames earlier.
#[test]
fn jumps_and_actions_during_the_second_character_s_throw() {
    let thrown = || {
        let mut play = Play::second();
        play.hold(&[A], 21);
        assert_eq!(play.world.player.hands.unwrap().act, Act::Carry);
        play
    };
    // Jump pressed before Q + 19: nothing, now or later.
    let mut play = thrown();
    let mut rows = play.hold(&[], 10);
    rows.extend(play.hold(&[Jump], 30));
    assert!(rows.iter().all(|r| !has(r, Event::JumpStart)));

    // Pressed on Q + 19: a jump taking off on the next frame, with the
    // second character's own values.
    let mut play = thrown();
    play.hold(&[], 19);
    assert!(!has(&play.step(&[Jump]), Event::JumpStart));
    let take_off = play.step(&[Jump]);
    assert!(has(&take_off, Event::JumpStart));
    assert_ne!(take_off.vel.y, f(-7.0));

    let rolls = |presses: &[(usize, usize)]| {
        let mut play = thrown();
        let rows: Vec<Row> = (0..60)
            .map(|n| {
                let held = presses.iter().any(|&(from, to)| (from..to).contains(&n));
                play.step(if held { &[A][..] } else { &[] })
            })
            .collect();
        rows.iter().position(|r| has(r, Event::RollStart))
    };
    assert_eq!(rolls(&[(19, 30)]), Some(19));
    assert_eq!(rolls(&[(30, 38)]), Some(30));
    assert_eq!(rolls(&[(12, 30)]), Some(19));
    assert_eq!(rolls(&[(11, 40)]), None);
    assert_eq!(rolls(&[(13, 17)]), None);
    assert_eq!(rolls(&[(5, 40)]), None);
}

#[test]
fn putting_down() {
    let mut play = Play::carrying();
    play.hold(&[Down, A], 3);
    let x = play.world.player.pos.x;
    let rows = play.hold(&[Down], 15);
    assert!(rows[..14].iter().all(|r| r.act == Some(Act::PutDown)));
    assert!(rows[..14].iter().all(|r| r.vel.x == Fixed::ZERO));
    assert_eq!(rows[13].phase, Phase::Placed);
    assert_eq!(rows[13].thing, Vec2::new(x + f(16.0), f(FLOOR - 3.0)));
    assert_eq!(rows[14].phase, Phase::Ready);
    assert_eq!(rows[14].thing, Vec2::new(x + f(16.0), f(FLOOR)));
    assert_eq!(rows[14].act, None);
    assert!(play.world.player.crouch.held > 0);

    // In the air, Down makes no difference.
    let mut play = Play::carrying();
    play.hold(&[A, Jump], 5);
    assert!(!play.world.player.grounded());
    assert_eq!(play.step(&[Down]).act, Some(Act::Throw));
}

#[test]
fn picking_up_again_after_putting_down() {
    let put_down = || {
        let mut play = Play::carrying();
        play.step(&[Down]);
        play
    };
    // A press on Q + 2 to Q + 13 still held on Q + 14 picks it up then.
    let mut play = put_down();
    play.hold(&[], 4);
    let rows = play.hold(&[A], 10);
    assert!(rows[..9].iter().all(|r| r.act == Some(Act::PutDown)));
    assert_eq!(rows[9].act, Some(Act::PickUp));
    assert!(rows.iter().all(|r| !has(r, Event::RollStart)));

    // On Q + 1, it doesn't. Held on, it's picked up on Q + 15 by
    // "Picking up": the put-down is over at the end of Q + 14.
    let mut play = put_down();
    let rows = play.hold(&[A], 20);
    assert_eq!(rows[13].act, None);
    assert_eq!(rows[14].act, Some(Act::PickUp));
    // A press within reach later does, on the press frame.
    let mut play = put_down();
    play.hold(&[], 20);
    play.step(&[]);
    assert_eq!(play.step(&[A]).act, Some(Act::PickUp));
}

#[test]
fn falling_out_of_the_level_drops_the_object() {
    let mut play = Play::carrying();
    // Off the floor: the player falls out of the level and loses a life.
    play.world.player.pos = v(2500.0, 300.0);
    play.world.player.ground = None;
    play.hold(&[A], 100);
    assert!(play.world.player.hands.is_none());
    assert!(matches!(play.world.things[0].phase, Phase::Thrown(_)));
    // When the player comes back, it's at its entry's place again
    // (`resume_afresh`).
    for _ in 0..300 {
        play.step(&[]);
        if play.world.flow.events.contains(&FlowEvent::Respawn) {
            break;
        }
    }
    let object = &play.world.things[0];
    assert!(object.active);
    assert_eq!(object.phase, Phase::Ready);
    assert_eq!(object.pos, play.world.level.entities[0].at);
}

/// player-frames.md, "Carrying", "Moving with the object": a turn starts
/// on the frame the other way is held, on any step, and the carrier
/// never runs.
#[test]
fn a_carrier_turns_at_once_and_never_runs() {
    let mut play = Play::carrying();
    for n in 0..25 {
        play.step(&[A, Right]);
        assert!(
            matches!(play.world.player.gait, clamber_sim::Gait::Walking(_)),
            "frame {n}"
        );
    }
    for _ in 0..12 {
        // Whatever step it's on, Left held starts the turn on that frame.
        play.step(&[A, Right]);
        let mut next = Play {
            world: play.world.clone(),
            held: play.held,
        };
        assert_eq!(next.step(&[A, Left]).turn, Some(0));
    }
}

/// carrying.md, "Putting down": Down held on Q put the object down; it
/// starts a crouch from Q + 14 only.
#[test]
fn a_put_down_starts_no_crouch_before_q_plus_14() {
    let mut play = Play::carrying();
    let rows = play.hold(&[Down], 14);
    assert!(rows.iter().all(|r| r.act == Some(Act::PutDown)));
    assert_eq!(play.world.player.crouch.held, 0);
    play.step(&[Down]);
    assert_eq!(play.world.player.crouch.held, 1);
}

/// carrying.md, "The second character": a pick-up with the target 0 for
/// G to G + 10, carrying from G + 11, a carrying target of 3.375, a
/// throw of 40 frames with the target 0 for 19, and free on Q + 40.
#[test]
fn the_second_character_picks_up_carries_and_throws_with_its_own_timing() {
    let mut play = Play::second();
    let rows = play.hold(&[A, Right], 12);
    // Pressed Right: the target is 0 through G + 10.
    assert!(rows[..11].iter().all(|r| r.act == Some(Act::PickUp)));
    assert!(rows[..11].iter().all(|r| r.vel.x == Fixed::ZERO));
    assert_eq!(rows[11].act, Some(Act::Carry));
    assert!(rows[11].vel.x > Fixed::ZERO);
    let rows = play.hold(&[A, Right], 60);
    assert_eq!(rows[59].vel.x, f(3.375));
    // Letting go on Q: 19 frames of target 0, then 2.25 with a direction.
    let rows = play.hold(&[Right], 40);
    assert!(rows[..40].iter().all(|r| r.act == Some(Act::Throw)));
    assert!(rows[18].vel.x.abs() < rows[0].vel.x.abs());
    assert!(rows[39].vel.x > f(2.0) && rows[39].vel.x <= f(2.25));
    let row = play.step(&[Right]);
    assert_eq!(row.act, None);
}

/// "The second character": a put-down of 10 frames.
#[test]
fn the_second_character_s_put_down_lasts_ten_frames() {
    let mut play = Play::second();
    play.hold(&[A], 30);
    let rows = play.hold(&[Down], 10);
    assert!(rows.iter().all(|r| r.act == Some(Act::PutDown)));
    assert_eq!(play.step(&[Down]).act, None);
}

/// "Jumps while carrying": the first character's values, `vy` -7 on the
/// take-off frame, whatever the second's own.
#[test]
fn the_second_character_s_carried_jump_has_the_first_character_s_values() {
    let mut play = Play::second();
    play.hold(&[A], 30);
    let rows = play.hold(&[A, Jump], 12);
    let t = rows.iter().position(|r| has(r, Event::JumpStart)).unwrap();
    assert_eq!(rows[t].vel.y, f(-7.0));
    // Held since the press: gravity 0.28125.
    assert_eq!(rows[t + 1].vel.y - rows[t].vel.y, f(0.28125));
}

/// "The object, second character": the object leaves the hands on Q + 18,
/// at the player's `x` and 9 px above its `y`, and is in the hands where
/// the table says before.
#[test]
fn the_second_character_throws_from_nine_px_up_on_q_plus_18() {
    let mut play = Play::second();
    play.hold(&[A], 30);
    let x = play.world.player.pos.x;
    let rows = play.hold(&[], 20);
    assert_eq!(rows[17].phase, Phase::Held);
    assert_eq!(rows[17].thing, Vec2::new(x, f(FLOOR - 9.0)));
    assert_eq!(rows[0].thing, Vec2::new(x + f(8.0), f(FLOOR - 11.0)));
    assert_eq!(rows[3].thing, Vec2::new(x - f(7.0), f(FLOOR - 11.0)));
    assert_eq!(rows[10].thing, Vec2::new(x - f(16.0), f(FLOOR - 9.0)));
    assert_eq!(rows[14].thing, Vec2::new(x - f(12.0), f(FLOOR - 9.0)));
    assert_eq!(rows[18].thing, Vec2::new(x, f(FLOOR - 9.0)));
    match rows[18].phase {
        Phase::Thrown(flight) => {
            assert_eq!(flight.vx, f(16.0));
            assert_eq!(flight.vy, f(1.0));
        }
        other => panic!("Q + 18: {other:?}"),
    }
}

/// The pick-up's table starts on its second frame; a put-down ends 12 px
/// ahead and 3 px above the floor on Q + 9.
#[test]
fn the_second_character_s_object_in_the_hands_of_a_pick_up_and_put_down() {
    let mut play = Play::second();
    let x = play.world.player.pos.x;
    let rows = play.hold(&[A], 12);
    assert_eq!(rows[1].thing, Vec2::new(x + f(12.0), f(FLOOR + 8.0)));
    assert_eq!(rows[4].thing, Vec2::new(x + f(16.0), f(FLOOR + 1.0)));
    assert_eq!(rows[9].thing, Vec2::new(x + f(12.0), f(FLOOR - 8.0)));
    let rows = play.hold(&[Down], 11);
    assert_eq!(rows[4].thing, Vec2::new(x + f(16.0), f(FLOOR + 1.0)));
    assert_eq!(rows[9].phase, Phase::Placed);
    assert_eq!(rows[9].thing, Vec2::new(x + f(12.0), f(FLOOR - 3.0)));
    assert_eq!(rows[10].phase, Phase::Ready);
}
