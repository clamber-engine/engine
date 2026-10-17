//! Finishing a level at its end marker in Classic 1
//! (spec/behavior/level-end.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{
    Event, Facing, FlowEvent, InputFrame, Outcome, World, follow_point, generic_feel, step,
};

const R: Action = Action::Right;
const L: Action = Action::Left;
const U: Action = Action::Up;
const J: Action = Action::Jump;
const A: Action = Action::Action;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat ground with its top at y = 449 from x = 0 to 3000, the lead
/// standing at `start` on it, and an end marker at `marker`, on `preset`.
fn world(preset: &str, start: i32, marker: (i32, i32), more: &str) -> World {
    let (xm, ym) = marker;
    let src = format!(
        r#"
        format = 1
        name = "end"
        size = [3000, 700]
        start = [{start}, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]
        {more}

        [[entities]]
        type = "end"
        at = [{xm}, {ym}]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "end").unwrap());
    World::new(level, Arc::new(generic_feel(preset).unwrap()))
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
        assert!(play.world.player.grounded());
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

    /// Holds `keys` until the lead passes the marker, at most `most`
    /// frames, and says whether it did.
    fn until_passed(&mut self, keys: &[Action], most: usize) -> bool {
        for _ in 0..most {
            self.step(keys);
            if self.passed() {
                return true;
            }
        }
        false
    }

    fn passed(&self) -> bool {
        self.world.ending.is_some()
    }

    fn x(&self) -> i64 {
        self.world.player.pos.x.floor()
    }
}

#[test]
fn walking_right_passes_the_marker_on_the_first_frame_at_or_right_of_it() {
    let mut play = Play::new(world("classic-1", 900, (1000, 449), ""));
    let mut before = play.x();
    while !play.passed() {
        before = play.x();
        play.step(&[R]);
        assert!(play.world.frame < 200);
    }
    assert!(before < 1000, "{before}");
    assert!(play.x() >= 1000, "{}", play.x());
    assert_eq!(play.world.flow.outcome, Outcome::Playing);
}

#[test]
fn the_walk_out_and_the_fade() {
    let mut play = Play::new(world("classic-1", 900, (1000, 449), ""));
    assert!(play.until_passed(&[R], 200));
    let at = play.world.player.pos;
    let view = play.world.view.unwrap().x();
    let full = (15, 15);
    assert_eq!(play.world.brightness(), full);

    // T: the lead doesn't move, at 2 px per frame, still on the ground.
    play.step(&[R]);
    let p = &play.world.player;
    assert_eq!(p.pos, at);
    assert_eq!(p.vel.x, Fixed::from_int(2));
    assert!(p.grounded());
    assert!(play.world.behind_scenery());

    // From T + 1: 2 px right and 1/8 px up a frame, off the ground, until
    // the frame after the one that ends 240 px right of the view's left.
    let mut k = 0;
    let mut rose = Fixed::ZERO;
    let mut due = play.x() - view >= 240;
    let f0 = loop {
        k += 1;
        play.step(&[R]);
        let p = &play.world.player;
        assert_eq!(p.pos.x, at.x + Fixed::from_int(2 * k));
        assert!(!p.grounded());
        assert_eq!(play.world.view.unwrap().x(), view, "the view holds still");
        if due {
            assert_eq!(p.pos.y, at.y - rose, "no rise on F0");
            assert_eq!(play.world.brightness(), (14, 15));
            break k;
        }
        rose += f(0.125);
        assert_eq!(p.pos.y, at.y - rose);
        assert_eq!(play.world.brightness(), full);
        due = play.x() - view >= 240;
    };
    assert!(f0 > 1);

    // The fade: one step of brightness every second frame, black and the
    // level over on F0 + 28.
    for j in 1..=28 {
        assert!(!play.world.flow.outcome.is_over());
        play.step(&[R]);
        assert_eq!(play.world.player.pos.y, at.y - rose, "no rise in the fade");
        assert_eq!(play.world.brightness(), (14 - j / 2, 15), "F0 + {j}");
    }
    assert_eq!(play.world.flow.outcome, Outcome::Complete);
    assert!(play.world.flow.events.contains(&FlowEvent::Complete));
    assert_eq!(play.world.brightness(), (0, 15));
}

#[test]
fn nothing_held_during_the_walk_out_has_any_effect() {
    let mut play = Play::new(world("classic-1", 900, (1000, 449), ""));
    assert!(play.until_passed(&[R, A], 200));
    let mut other = Play {
        world: play.world.clone(),
        held: play.held,
    };
    let mut third = Play {
        world: play.world.clone(),
        held: play.held,
    };
    while !play.world.flow.outcome.is_over() {
        play.step(&[R]);
        other.step(&[L, J, A]);
        third.step(&[]);
        assert_eq!(other.world.state_bytes(), play.world.state_bytes());
        assert_eq!(third.world.state_bytes(), play.world.state_bytes());
        assert!(play.world.frame < 400);
    }
}

#[test]
fn passing_needs_right_held() {
    // Standing 10 px past the marker: nothing held, and Up alone, don't
    // pass it; the first frame with Right held does.
    let mut play = Play::new(world("classic-1", 1010, (1000, 449), ""));
    play.hold(&[], 20);
    play.hold(&[U], 20);
    assert!(!play.passed());
    play.step(&[R, U]);
    assert!(play.passed());
}

#[test]
fn passing_needs_the_feet_line_within_4_px_of_the_marker() {
    for (ym, passes) in [(445, true), (453, true), (444, false), (454, false)] {
        let mut play = Play::new(world("classic-1", 1010, (1000, ym), ""));
        assert_eq!(play.until_passed(&[R], 5), passes, "{ym}");
    }
}

#[test]
fn a_jump_past_the_marker_passes_it_on_landing() {
    let mut play = Play::new(world("classic-1", 960, (1000, 449), ""));
    play.hold(&[R], 10);
    play.step(&[R, J]);
    let mut landed = false;
    for _ in 0..80 {
        play.step(&[R, J]);
        landed = play.world.events.contains(&Event::Land);
        if play.passed() || landed {
            break;
        }
    }
    assert!(landed && play.passed());
    assert!(play.x() > 1000);
}

#[test]
fn a_lead_facing_left_turns_right_during_the_walk_out() {
    // Standing past the marker facing left, Right starts a turn: the lead
    // still faces left when it passes the marker.
    let mut play = Play::new(world("classic-1", 1050, (1000, 449), ""));
    play.hold(&[L], 1);
    play.hold(&[], 30);
    assert_eq!(play.world.player.facing, Facing::Left);
    assert!(play.until_passed(&[R], 1));
    assert_eq!(play.world.player.facing, Facing::Left);
    // The turn stops on T, starts again on T + 1, and faces right on
    // T + 5.
    play.step(&[R]);
    assert_eq!(play.world.player.turn, None);
    for t in 1..=5 {
        play.step(&[R]);
        let turned = play.world.events.contains(&Event::Turn);
        assert_eq!(turned, t == 5, "T + {t}");
        let facing = if t < 5 { Facing::Left } else { Facing::Right };
        assert_eq!(play.world.player.facing, facing, "T + {t}");
    }
}

#[test]
fn walkers_dont_touch_a_lead_walking_out() {
    let walker = "[[entities]]\ntype = \"crawler\"\nat = [1060, 449]\n";
    let mut play = Play::new(world("classic-1", 1010, (1000, 449), walker));
    assert!(play.until_passed(&[R], 1));
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        assert!(!play.world.events.contains(&Event::Hurt));
        assert!(play.world.flow.events.iter().all(|e| *e != FlowEvent::Die));
        assert!(play.world.frame < 400);
    }
    assert_eq!(play.world.flow.outcome, Outcome::Complete);
}

#[test]
fn the_other_character_goes_on_following_2_px_a_frame() {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    let mut w = world("classic-1", 700, (1000, 449), "");
    w = World::new(w.level.clone(), Arc::new(feel));
    let mut play = Play::new(w);
    assert!(play.until_passed(&[R], 300));
    let gap = |play: &Play| {
        let other = play.world.other.as_ref().unwrap();
        play.world.player.pos.x - other.pos.x
    };
    play.step(&[]);
    let at_t = gap(&play);
    assert!(at_t > Fixed::from_int(20), "{at_t:?}");
    let feet = |play: &Play| follow_point(play.world.other.as_ref().unwrap().pos).1;
    let start_feet = feet(&play);
    // It's about 18 frames behind on the trail, and the lead rises a
    // whole px in 8.
    for _ in 0..40 {
        let x = play.world.other.as_ref().unwrap().pos.x;
        play.step(&[]);
        let other = play.world.other.as_ref().unwrap();
        assert_eq!(other.pos.x - x, Fixed::from_int(2));
        assert_eq!(gap(&play), at_t);
    }
    assert!(feet(&play) < start_feet, "it rises along the trail");
}

#[test]
fn without_a_walk_out_passing_completes_the_level_at_once() {
    let mut play = Play::new(world("default", 1010, (1000, 449), ""));
    play.step(&[R]);
    assert_eq!(play.world.flow.outcome, Outcome::Complete);
    assert!(!play.passed());
    assert_eq!(play.world.brightness(), (1, 1));
}

#[test]
fn the_marker_is_found_at_its_level_position() {
    let w = world("classic-1", 900, (1000, 449), "");
    let end = w.level.entities.iter().find(|e| e.kind == clamber_sim::END);
    assert_eq!(end.unwrap().at, Vec2::ints(1000, 449));
}

/// A room as wide as the picture, so its left edge is always at 0, with
/// flat ground at 449, the lead standing at `start`, and an end marker at
/// `xm` on the floor that leads back to another area if `onward`.
fn room(start: f64, xm: i32, onward: bool) -> World {
    let params = match onward {
        true => r#"params = { area = "level", arrival = "back" }"#,
        false => "",
    };
    let src = format!(
        r#"
        format = 1
        name = "room"
        size = [256, 700]
        start = [{start}, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 256, 100]

        [[entities]]
        type = "end"
        at = [{xm}, 449]
        {params}
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "room").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

/// Steps with nothing held until the area is over, and gives the frames
/// after T that took, the frame its fade started on, and the lead's `x`
/// on the last frame.
fn walk_out(play: &mut Play) -> (u32, u32, Fixed) {
    let t = play.world.frame + 1;
    let mut fade = None;
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        if fade.is_none() && play.world.brightness() != (15, 15) {
            fade = Some(play.world.frame - t);
        }
        assert!(play.world.frame < t + 200);
    }
    (play.world.frame - t, fade.unwrap(), play.world.player.pos.x)
}

#[test]
fn in_the_room_the_walk_outs_first_frame_doesnt_count_for_the_fade() {
    // level-end.md, "In the first level's room", synthetic example: the
    // marker passed ending at 241.25 or so, past 240 already on T.
    let mut play = Play::new(room(241.25, 236, true));
    assert!(play.until_passed(&[R], 1));
    let x = play.world.player.pos.x;
    assert!(x.floor() >= 240, "{x}");
    assert_eq!(play.world.view.unwrap().x(), 0);
    let (last, fade, at) = walk_out(&mut play);
    assert_eq!(fade, 2, "F0 = T + 2");
    assert_eq!(last, 31, "F0 + 29 is the room's last frame");
    assert_eq!(at, x + f(62.0), "T + 31 at 303.25");
    assert_eq!(play.world.brightness().0, 0);
    assert_eq!(play.world.flow.outcome, Outcome::Exited(0));
    assert!(play.world.flow.events.contains(&FlowEvent::Exited(0)));
    // The lead rose 1/8 px on T + 1 only.
    assert_eq!(play.world.player.pos.y, f(449.0) - f(0.125));
}

#[test]
fn in_the_room_the_fade_starts_as_in_the_level_and_ends_a_frame_later() {
    let pass = |onward| {
        let mut play = Play::new(room(200.0, 210, onward));
        assert!(play.until_passed(&[R], 100));
        assert!(play.x() < 240);
        walk_out(&mut play)
    };
    let (level_last, level_fade, level_at) = pass(false);
    let (room_last, room_fade, room_at) = pass(true);
    assert_eq!(room_fade, level_fade);
    assert_eq!(level_last, level_fade + 28);
    assert_eq!(room_last, room_fade + 29);
    assert_eq!(room_at, level_at + f(2.0));
}

#[test]
fn a_marker_leading_on_is_left_as_an_exit_is() {
    let mut play = Play::new(room(200.0, 210, true));
    assert!(play.until_passed(&[R], 100));
    walk_out(&mut play);
    assert_eq!(play.world.flow.outcome, Outcome::Exited(0));
    let level = play.world.level.clone();
    assert_eq!(clamber_sim::area(&level.entities[0]), Some("level"));
}
