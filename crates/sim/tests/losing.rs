//! Losing a character on a hit, and the protection after it, in Classic 1
//! (spec/behavior/losing-a-character.md, protection.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Param, Presets};
use clamber_sim::{
    Behavior, Event, Facing, FlowEvent, InputFrame, Other, Phase, Riding, START_LIVES, State,
    World, generic_feel, step,
};

/// Flat ground at y = 400 from x = 0 to 2000, a still walker far off
/// at x = 1800, and the player and the other character on the floor.
const FLAT: &str = r#"
    format = 1
    name = "flat"
    size = [2000, 600]
    start = [200, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 400, 2000, 200]

    [[entities]]
    type = "crawler"
    at = [1800, 400]
    params = { speed = 0, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }
"#;

const FLOOR: f64 = 400.0;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(f(x), f(y))
}

/// Classic 1 with both characters.
fn feel() -> Arc<clamber_pack::Feel> {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    // The other character stands where it's put in these examples;
    // following is tested in `following.rs`.
    feel.world.set(Param::FollowTrail, Fixed::ZERO);
    Arc::new(feel)
}

/// What a step left behind.
#[derive(Clone, Debug)]
struct Row {
    character: usize,
    pos: Vec2,
    vel: Vec2,
    state: State,
    ground: bool,
    facing: Facing,
    events: Vec<Event>,
    drawn: bool,
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// The first character standing at `at` facing `facing`, and the
    /// second standing at `other` facing right.
    fn new(at: Vec2, facing: Facing, other: Vec2) -> Play {
        Play::in_level(FLAT, at, facing, other)
    }

    fn in_level(source: &str, at: Vec2, facing: Facing, other: Vec2) -> Play {
        let level = Arc::new(Level::from_toml_str(source, "flat").unwrap());
        let mut world = World::new(level, feel());
        world.player.pos = at;
        world.player.facing = facing;
        world.other = Some(Other::new(1, other, Facing::Right, at));
        let mut play = Play {
            world,
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        assert!(play.world.player.grounded());
        play
    }

    /// The example's start: the first character at x = 200.25, the
    /// second at x = 138.
    fn example() -> Play {
        Play::new(v(200.25, FLOOR), Facing::Right, v(138.0, FLOOR))
    }

    fn step(&mut self, keys: &[Action]) -> Row {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        let w = &self.world;
        let p = &w.player;
        Row {
            character: p.character,
            pos: p.pos,
            vel: p.vel,
            state: p.state,
            ground: p.grounded(),
            facing: p.facing,
            events: w.events.clone(),
            drawn: w.player_drawn(),
        }
    }

    fn hold(&mut self, keys: &[Action], frames: usize) -> Vec<Row> {
        (0..frames).map(|_| self.step(keys)).collect()
    }

    /// The walker hurts the player on the next step, and `frames` rows
    /// from there are returned, H first.
    fn hit(&mut self, keys: &[Action], frames: usize) -> Vec<Row> {
        self.world.player.hurt_ahead = Some(0);
        self.hold(keys, frames)
    }
}

/// The row the hop lands on, by index from H.
fn landing(rows: &[Row], t: usize) -> usize {
    t + rows[t..].iter().position(|r| r.ground).unwrap()
}

#[test]
fn the_hit_character_is_thrown_up_and_lands_with_no_life_lost() {
    let mut play = Play::example();
    let rows = play.hit(&[], 37);
    let h = &rows[0];
    assert_eq!(h.events, [Event::Hurt]);
    assert_eq!((h.pos, h.vel), (v(200.25, FLOOR), v(0.0, -8.0)));
    assert_eq!(
        (h.state, h.facing, h.ground),
        (State::Hurt, Facing::Right, true)
    );
    let w = &play.world;
    assert_eq!(w.flow.lives, START_LIVES);
    assert!(w.player.knockout.is_none());

    let mut vy = f(-8.0);
    let mut y = f(FLOOR);
    for (n, row) in rows.iter().enumerate().take(35).skip(1) {
        vy += f(0.4375);
        y += vy;
        assert_eq!(row.pos, Vec2::new(f(200.25), y), "H + {n}");
        assert_eq!((row.vel.x, row.vel.y), (Fixed::ZERO, vy), "H + {n}");
        assert_eq!(
            (row.state, row.ground, row.character),
            (State::Hurt, false, 0)
        );
        assert!(row.events.is_empty(), "H + {n}: {:?}", row.events);
    }
    assert_eq!(rows[18].pos.y, f(FLOOR - 69.1875));
    let land = &rows[35];
    assert_eq!(land.events, [Event::Land]);
    assert_eq!(
        (land.pos.y, land.vel.y, land.ground),
        (f(FLOOR), Fixed::ZERO, true)
    );
    assert_eq!(land.state, State::Hurt);

    // H + 36: the trace follows the other character, still where it
    // stood, off the ground.
    let other = &rows[36];
    assert_eq!(other.character, 1);
    assert_eq!((other.pos, other.vel), (v(138.0, FLOOR), Vec2::default()));
    assert_eq!((other.state, other.ground), (State::Swap, false));
    assert!(other.events.is_empty());
    let w = &play.world;
    assert!(w.other.is_none());
    assert!(w.flow.events.iter().all(|e| *e != FlowEvent::Die));
}

#[test]
fn the_lost_character_holds_still_then_runs_right_and_is_gone() {
    let mut play = Play::example();
    play.hit(&[], 37);
    // Landed on H + 35, still on H + 36.
    let lost = play.world.leaving().unwrap().clone();
    assert_eq!(lost.pos, v(200.25, FLOOR));
    assert_eq!(lost.facing, Facing::Right);
    play.hold(&[], 1);
    assert_eq!(play.world.leaving().unwrap().pos, v(204.25, FLOOR));
    play.hold(&[], 10);
    assert_eq!(play.world.leaving().unwrap().pos, v(244.25, FLOOR));
    // Out of the level after the far end.
    play.hold(&[], 460);
    assert!(play.world.leaving().is_none());
    assert!(play.world.lost.is_none());
}

/// A run held up by a wall stays there, with no time limit while the
/// view stays near it (losing-a-character.md, "A wall").
#[test]
fn a_run_held_up_by_a_wall_stays() {
    let wall = "\n[[collision]]\nshape = \"rect\"\nrect = [230, 300, 40, 100]\n";
    let source = format!("{FLAT}{wall}");
    let mut play = Play::in_level(&source, v(200.0, FLOOR), Facing::Right, v(138.0, FLOOR));
    play.hit(&[], 37);
    play.hold(&[], 300);
    let lost = play.world.leaving().expect("run is gone");
    assert!(lost.pos.x < f(230.0));
    let stopped = lost.pos;
    play.hold(&[], 300);
    assert_eq!(play.world.leaving().unwrap().pos, stopped);
}

/// In the open the run is gone on the first frame its `x` rounded down
/// is 289 or more right of the view's left edge.
#[test]
fn a_run_is_gone_289_px_right_of_the_view() {
    let mut play = Play::example();
    play.hit(&[], 37);
    for _ in 0..200 {
        let Some(lost) = play.world.leaving() else {
            break;
        };
        let left = play.world.view.expect("no view").x();
        assert!(lost.pos.x.floor() - left < 289);
        play.hold(&[], 1);
    }
    assert!(play.world.leaving().is_none());
}

#[test]
fn the_other_character_runs_to_the_hit_spot_and_hops() {
    let mut play = Play::example();
    let rows = play.hit(&[], 120);
    let xs: Vec<_> = rows[37..42].iter().map(|r| r.pos.x).collect();
    assert_eq!(xs, [141.875, 145.5625, 149.0, 152.1875, 155.1875].map(f));
    assert_eq!(rows[37].vel, v(3.875, 0.0));
    for n in 42..=56 {
        assert_eq!(rows[n].pos.x, rows[n - 1].pos.x + f(3.0), "H + {n}");
    }
    assert_eq!(rows[56].pos.x, f(200.1875));
    for row in &rows[36..57] {
        assert_eq!(row.pos.y, f(FLOOR));
        assert_eq!((row.state, row.ground), (State::Swap, false));
        assert!(row.events.is_empty(), "{:?}", row.events);
    }

    // T = H + 57.
    let t = &rows[57];
    assert_eq!(t.pos, v(200.5, FLOOR - 0.5));
    assert_eq!(t.vel, v(0.0, -10.0));
    assert_eq!(t.state, State::Swap);
    let vys: Vec<_> = rows[58..62].iter().map(|r| r.vel.y).collect();
    assert_eq!(vys, [-9.5625, -9.125, -7.9140625, -7.4765625].map(f));
    let top = rows[57..].iter().map(|r| r.pos.y).min().unwrap();
    assert_eq!(top, f(FLOOR - 94.7421875));
    assert_eq!(rows[57 + 21].pos.y, top);

    // It lands on T + 41 where it started, in control.
    let e = landing(&rows, 57);
    assert_eq!(e, 57 + 41);
    for row in &rows[58..e] {
        assert_eq!((row.state, row.ground), (State::Swap, false));
        assert!(row.vel.y <= f(8.0));
    }
    let land = &rows[e];
    assert_eq!(land.events, [Event::Land]);
    assert_eq!(
        (land.pos, land.state, land.character),
        (v(200.5, FLOOR), State::Idle, 1)
    );
    assert!(!play.world.losing());
}

#[test]
fn a_far_other_character_comes_down_through_the_air() {
    // losing-a-character.md's vertical example: 78 px above the target.
    let mut play = Play::example();
    play.world.other.as_mut().unwrap().pos = v(200.0, FLOOR - 78.0);
    let rows = play.hit(&[], 100);
    let ys: Vec<_> = rows[37..39]
        .iter()
        .map(|r| FLOOR - r.pos.y.to_f64())
        .collect();
    assert_eq!(ys, [78.0 - 4.875, 78.0 - 4.875 - 4.5625]);
    let t = rows.iter().position(|r| r.vel == v(0.0, -10.0)).unwrap();
    let last: Vec<_> = rows[t - 3..t].iter().map(|r| r.vel.y).collect();
    assert_eq!(last, [3.0, 3.0, 1.0].map(f));
    assert_eq!(rows[t - 1].pos.y, f(FLOOR - 0.6875));
}

#[test]
fn input_counts_from_the_frame_after_the_hop_starts() {
    let mut quiet = Play::example();
    let still = quiet.hit(&[], 100);
    let mut play = Play::example();
    let rows = play.hit(&[Action::Left], 100);
    for n in 0..=57 {
        assert_eq!(rows[n].pos, still[n].pos, "H + {n}");
        assert_eq!(rows[n].facing, still[n].facing, "H + {n}");
    }
    // The second character faced right: Left turns it on T + 1, and
    // steers the hop.
    assert!(rows[58].events.contains(&Event::Turn));
    assert_eq!(rows[58].facing, Facing::Left);
    assert!(rows[58].vel.x.is_negative());
    assert_eq!(rows[58].state, State::Swap);
}

#[test]
fn a_jump_held_into_the_landing_starts_after_the_lockout() {
    let mut play = Play::example();
    let mut rows = play.hit(&[], 93);
    // The hop lands on H + 98; press jump 5 frames before and hold it.
    rows.extend(play.hold(&[Action::Jump], 10));
    let e = landing(&rows, 57);
    assert_eq!(e, 98);
    assert!(
        rows[..e]
            .iter()
            .all(|r| !r.events.contains(&Event::JumpStart))
    );
    let jump = rows
        .iter()
        .position(|r| r.events.contains(&Event::JumpStart));
    assert!(jump.is_some_and(|j| j > e), "{jump:?}");
}

#[test]
fn a_held_jump_keeps_its_lighter_gravity_in_the_throw() {
    let mut play = Play::example();
    play.hold(&[Action::Jump], 4);
    assert!(play.world.player.jump_hold);
    let y = play.world.player.pos.y;
    // Let go of jump on the hit frame; the lighter gravity stays.
    let rows = play.hit(&[], 3);
    assert_eq!(rows[0].pos.y, y);
    assert_eq!(rows[0].vel.y, f(-8.0));
    assert_eq!(rows[1].vel.y, f(-8.0 + 0.28125));
    assert_eq!(rows[2].vel.y, f(-8.0 + 2.0 * 0.28125));
}

#[test]
fn a_hit_facing_left_turns_right() {
    let mut play = Play::new(v(200.25, FLOOR), Facing::Left, v(138.0, FLOOR));
    let rows = play.hit(&[], 1);
    assert_eq!(rows[0].events, [Event::Hurt, Event::Turn]);
    assert_eq!(rows[0].facing, Facing::Right);
}

#[test]
fn protection_lasts_to_h_plus_149_and_a_contact_hurts_on_h_plus_150() {
    // protection.md's example, with the hit on frame 0: a walker touches
    // the character that took over from H + 110 on.
    let mut play = Play::example();
    let rows = play.hit(&[], 110);
    assert_eq!(rows[98].character, 1);
    let at = play.world.player.pos;
    // The walker is parked away from the view, out of the level; bring
    // it in.
    play.world.things[0].pos = at;
    play.world.things[0].active = true;
    for (n, row) in play.hold(&[], 40).iter().enumerate() {
        assert!(!row.events.contains(&Event::Hurt), "H + {}", 110 + n);
    }
    assert!(play.world.player.hurt_ahead.is_some());
    // H + 150. The second character being hit is a hit with one
    // character: it's knocked out.
    assert_eq!(play.step(&[]).events, [Event::Hurt]);
    assert!(play.world.player.knockout.is_some());
    assert!(play.world.loss.is_none());
}

#[test]
fn the_other_character_flashes_two_frames_in_four() {
    let mut play = Play::example();
    let rows = play.hit(&[], 160);
    let t = 57;
    for (n, row) in rows.iter().enumerate() {
        if !(38..134).contains(&n) || n == t + 1 {
            assert!(row.drawn, "H + {n}");
        }
    }
    let first = (38..42).find(|&n| !rows[n].drawn).unwrap();
    for (n, row) in rows.iter().enumerate().take(134).skip(38) {
        if n != t + 1 {
            let off = (n + 4 - first) % 4 < 2;
            assert_eq!(row.drawn, !off, "H + {n}");
        }
    }
}

#[test]
fn with_no_other_character_a_hit_knocks_out() {
    let mut play = Play::example();
    play.world.other = None;
    let rows = play.hit(&[], 1);
    assert_eq!(rows[0].events, [Event::Hurt]);
    assert!(play.world.player.knockout.is_some());
    assert!(play.world.loss.is_none());
}

/// The second character under control at `at`, the first standing at
/// `other` facing right.
fn second(at: Vec2, other: Vec2) -> Play {
    let mut play = Play::new(at, Facing::Right, other);
    play.world.player.character = 1;
    play.world.other = Some(Other::new(0, other, Facing::Right, at));
    play.hold(&[], 2);
    play
}

#[test]
fn the_second_character_hit_flies_like_the_first_and_the_first_waits_a_frame_longer() {
    let mut play = second(v(200.25, FLOOR), v(130.0, FLOOR));
    let rows = play.hit(&[], 70);
    assert_eq!(rows[0].events, [Event::Hurt]);
    assert_eq!(rows[0].vel, v(0.0, -8.0));
    assert_eq!(play.world.flow.lives, START_LIVES);
    let mut vy = f(-8.0);
    for (n, row) in rows.iter().enumerate().take(35).skip(1) {
        vy += f(0.4375);
        assert_eq!(
            (row.vel.y, row.state, row.character),
            (vy, State::Hurt, 1),
            "H + {n}"
        );
    }
    assert_eq!(rows[35].events, [Event::Land]);
    // H + 36: the first character, as it has been, on the ground, in
    // an ordinary state.
    let other = &rows[36];
    assert_eq!(other.character, 0);
    assert_eq!((other.pos, other.vel), (v(130.0, FLOOR), Vec2::default()));
    assert!(other.ground && other.state != State::Swap);
    assert!(other.events.is_empty());
    // Swap state from H + 37, still until H + 37, moving from H + 38.
    assert_eq!(
        (rows[37].state, rows[37].pos),
        (State::Swap, v(130.0, FLOOR))
    );
    assert_eq!(rows[38].pos.x, f(130.0 + 70.0 / 16.0));
    // The hop: T is the first row with the hop's start placed half a
    // pixel above the floor.
    let t = rows.iter().position(|r| r.pos.y == f(FLOOR - 0.5)).unwrap();
    assert_eq!(rows[t + 1].pos.y, f(FLOOR - 0.5 - 9.5625));
    assert_eq!(rows[t + 2].pos.y, f(FLOOR - 0.5 - 9.5625 - 9.125));
    assert_eq!(rows[t + 3].pos.y, f(FLOOR - 0.5 - 9.5625 - 9.125 - 6.5625));
}

#[test]
fn the_second_character_hit_with_jump_held_falls_under_the_lighter_gravity() {
    let mut play = second(v(200.25, FLOOR), v(130.0, FLOOR));
    play.hold(&[Action::Jump], 8);
    assert!(play.world.player.jump_hold);
    let rows = play.hit(&[Action::Jump], 3);
    assert_eq!(rows[1].vel.y, f(-8.0 + 0.3515625));
}

#[test]
fn a_hit_during_a_roll_loses_the_character_too() {
    let mut play = Play::example();
    play.hold(&[Action::Right, Action::Action], 6);
    assert!(play.world.player.roll.is_some() || play.world.player.state == State::Roll);
    let rows = play.hit(&[Action::Right, Action::Action], 4);
    assert_eq!(rows[0].events, [Event::Hurt]);
    assert_eq!((rows[0].vel, rows[0].state), (v(0.0, -8.0), State::Hurt));
    assert!(play.world.player.knockout.is_none());
    assert!(play.world.loss.is_some());
}

/// The animal that was ridden when a character is lost is gone with it:
/// nothing is left under the character that takes over.
#[test]
fn no_animal_is_left_under_the_character_that_takes_over() {
    let source = format!("{FLAT}\n[[entities]]\ntype = \"hutch\"\nat = [40, 400]\n");
    let mut play = Play::in_level(&source, v(200.25, FLOOR), Facing::Right, v(138.0, FLOOR));
    let w = &mut play.world;
    let Some(Behavior::Hutch(hutch)) = w.things[1].archetype.map(|a| a.behavior) else {
        panic!("no animal crate");
    };
    w.things[1].phase = Phase::Ridden;
    w.player.riding = Some(Riding {
        on_floor: true,
        was_on_floor: true,
        mount: None,
        ..Riding::new(hutch.ride, Fixed::ZERO)
    });
    play.hold(&[], 2);
    play.hit(&[], 80);
    let w = &play.world;
    assert_eq!(w.player.character, 1);
    assert!(w.player.riding.is_none());
    assert!(
        w.things
            .iter()
            .all(|t| !(t.active && t.phase == Phase::Ridden))
    );
}
