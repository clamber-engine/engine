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

#[derive(Clone)]
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
    walks_out_and_fades(play);
}

/// From the end of `n`: the lead's walk-out and the fade, frame by frame,
/// as "The walk-out" and "The fade" give them.
fn walks_out_and_fades(mut play: Play) {
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

/// The first character following the second, which leads.
fn second_leading(start: i32, marker: (i32, i32)) -> World {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    let w = world("classic-1", start, marker, "");
    let mut w = World::new(w.level.clone(), Arc::new(feel));
    w.player.character = 1;
    if let Some(other) = &mut w.other {
        other.character = 0;
    }
    w
}

/// "The second character leading": the walk-out is the same frame for
/// frame, at 2 px a frame although the second character walks faster.
#[test]
fn with_the_second_character_leading_the_walk_out_is_the_same() {
    let mut play = Play::new(second_leading(900, (1000, 449)));
    assert!(play.until_passed(&[R], 200));
    assert_eq!(play.world.player.vel.x, f(2.25), "it walks faster on n");
    walks_out_and_fades(play);
}

/// ... and Right held together with Down passes the marker then too.
#[test]
fn with_the_second_character_leading_right_with_down_passes() {
    let mut play = Play::new(second_leading(1010, (1000, 449)));
    play.hold(&[], 10);
    assert!(!play.passed());
    play.step(&[R, Action::Down]);
    assert!(play.passed());
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

/// "Walkers and bunches": a walker goes on walking as usual on every
/// frame of the walk-out.
#[test]
fn walkers_go_on_walking_during_the_walk_out() {
    let walker = "[[entities]]\ntype = \"crawler\"\nat = [960, 449]\n";
    let mut play = Play::new(world("classic-1", 1010, (1000, 449), walker));
    let walker = play
        .world
        .things
        .iter()
        .position(|t| t.kind == "crawler")
        .unwrap();
    assert!(play.until_passed(&[R], 1));
    let mut x = play.world.things[walker].pos.x;
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        let thing = &play.world.things[walker];
        assert!(thing.active);
        assert_eq!(thing.pos.x, x - f(0.5), "frame {}", play.world.frame);
        x = thing.pos.x;
    }
}

/// "Walkers and bunches": a walker whose contact would hurt the lead on
/// `T` doesn't, and the lead walks on through its place. Found by
/// waiting for a walker coming toward the lead until, without the
/// marker, the lead would be hurt on exactly the frame after the one
/// Right is pressed on.
#[test]
fn a_walker_that_would_hurt_the_lead_on_t_doesnt() {
    let walker = "[[entities]]\ntype = \"crawler\"\nat = [1060, 449]\n";
    let hurt_on_t = |marker: (i32, i32), wait: usize| {
        let mut play = Play::new(world("classic-1", 1010, marker, walker));
        for _ in 0..wait {
            play.step(&[]);
            if play.world.events.contains(&Event::Hurt) {
                return None;
            }
        }
        play.step(&[R]);
        let passed = play.passed();
        play.step(&[R]);
        Some((passed, play.world.events.contains(&Event::Hurt)))
    };
    let wait = (0..120)
        .find(|&wait| hurt_on_t((1000, 300), wait) == Some((false, true)))
        .expect("a wait after which the lead is hurt on T");
    assert_eq!(hurt_on_t((1000, 449), wait), Some((true, false)));

    let mut play = Play::new(world("classic-1", 1010, (1000, 449), walker));
    play.hold(&[], wait);
    assert!(play.until_passed(&[R], 1));
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        assert!(!play.world.events.contains(&Event::Hurt));
        assert!(play.world.frame < 400);
    }
    assert_eq!(play.world.flow.outcome, Outcome::Complete);
}

/// A bunch `ahead` px right of the marker and `above` px above the
/// lead's feet line, the lead standing on the marker and passing it on
/// its first frame of Right. The frames after `T` on which the bunch is
/// taken, and on which its items reach the count, until the level is
/// over.
fn bunch_during_the_walk_out(ahead: i32, above: i32) -> (Option<u32>, Vec<u32>) {
    let bunch = format!(
        "[[entities]]\ntype = \"cluster\"\nat = [{}, {}]\n",
        1000 + ahead,
        449 - above
    );
    let mut play = Play::new(world("classic-1", 1000, (1000, 449), &bunch));
    assert!(play.until_passed(&[R], 1));
    let n = play.world.frame;
    let (mut taken, mut counted) = (None, Vec::new());
    let mut gems = play.world.flow.gems;
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        let k = play.world.frame - n - 1;
        if play
            .world
            .flow
            .events
            .iter()
            .any(|e| matches!(e, FlowEvent::Collect(_)))
        {
            assert!(taken.is_none());
            taken = Some(k);
        }
        if play.world.flow.gems > gems {
            counted.push(k);
            gems = play.world.flow.gems;
        }
    }
    (taken, counted)
}

/// "Walkers and bunches", the example: a bunch 33 px right of the
/// marker and 20 px above the feet line is taken on `T` + 9, and its
/// items reach the count from `T` + 26, every 6 frames until the level
/// is over.
#[test]
fn a_bunch_is_taken_during_the_walk_out() {
    let (taken, counted) = bunch_during_the_walk_out(33, 20);
    assert_eq!(taken, Some(9));
    assert!(!counted.is_empty());
    let expected: Vec<u32> = (0..counted.len() as u32).map(|i| 26 + 6 * i).collect();
    assert_eq!(counted, expected);
}

/// "Walkers and bunches": the standing ranges apply on every frame of
/// the walk-out, though the lead is off the ground, not the jumping ones.
#[test]
fn during_the_walk_out_a_bunch_is_taken_by_the_standing_ranges() {
    // The lead rises 1/8 px a frame from T + 1, so its feet line is 1 px
    // higher by T + 9, when I comes into range: D is `above` less 1, and
    // only gets smaller after that.
    assert_eq!(bunch_during_the_walk_out(33, -8).0, None);
    assert_eq!(bunch_during_the_walk_out(33, -7).0, Some(9));
    assert_eq!(bunch_during_the_walk_out(33, 41).0, Some(9));
    // D 46 to 49 while I is in range: in the jumping row's D, but not
    // the standing row's.
    assert_eq!(bunch_during_the_walk_out(33, 50).0, None);
}

/// A walker coming left toward a lead standing past the marker.
const WALKER: &str = "[[entities]]\ntype = \"crawler\"\nat = [1060, 449]\n";

/// The steps after [`Play::new`], with nothing held, until the lead is
/// hurt: the last of them is the hit frame `H`.
fn steps_to_hit(play: &Play) -> usize {
    let mut play = play.clone();
    (1..400)
        .find(|_| {
            play.step(&[]);
            play.world.events.contains(&Event::Hurt)
        })
        .expect("the lead is hurt")
}

/// "Hits and bounces": Right held from the hit frame `H` on never passes
/// the marker, though the lead is on the ground past it, for as long as
/// the knock-out lasts.
#[test]
fn right_held_from_the_hit_frame_on_doesnt_pass() {
    let mut play = Play::new(world("classic-1", 1010, (1000, 449), WALKER));
    let hit = steps_to_hit(&play);
    play.hold(&[], hit - 1);
    let x = play.world.player.pos.x;
    play.step(&[R]);
    assert!(play.world.events.contains(&Event::Hurt));
    assert!(play.world.player.pos.x >= x && play.x() >= 1000);
    assert!(!play.passed());
    // Until the knock-out is over and the lead starts again.
    while play.world.player.knockout.is_some() {
        play.step(&[R]);
        assert!(!play.passed(), "frame {}", play.world.frame);
        assert!(play.world.frame < 1000);
    }
}

/// "Hits and bounces": after the lead is hit, the other character can
/// pass the marker only from its hop's landing, not on its run to the
/// hit spot past the marker with Right held.
#[test]
fn the_other_character_passes_only_from_its_hops_landing() {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    let w = world("classic-1", 1010, (1000, 449), WALKER);
    let mut play = Play::new(World::new(w.level.clone(), Arc::new(feel)));
    assert!(play.world.other.is_some());
    let hit = steps_to_hit(&play);
    play.hold(&[], hit - 1);
    play.step(&[R]);
    assert!(play.world.events.contains(&Event::Hurt) && !play.passed());
    let mut ran_past = false;
    let mut hopped = false;
    for _ in 0..400 {
        play.step(&[R]);
        let losing = play.world.losing();
        let grounded = play.world.player.grounded();
        ran_past |= losing && grounded && play.x() >= 1000;
        hopped |= play.world.player.hop.is_some();
        if play.passed() {
            assert!(!losing && grounded && hopped);
            assert!(play.world.events.contains(&Event::Land));
            assert!(ran_past);
            return;
        }
    }
    panic!("the other character never passed the marker");
}

/// "Hits and bounces": no frame of a bounce off a walker passes the
/// marker, even past it with Right held; the frame the lead lands does.
#[test]
fn a_bounce_passes_the_marker_only_on_landing() {
    let start = Play::new(world("classic-1", 940, (1000, 449), WALKER));
    let mut bounced_past = 0;
    for wait in 0..60 {
        let mut play = start.clone();
        play.hold(&[R], wait);
        let mut bounced = false;
        let mut past = false;
        for _ in 0..120 {
            play.step(&[R, J]);
            if play.world.events.contains(&Event::Bounce) {
                bounced = true;
                break;
            }
            if play.passed() || play.world.events.contains(&Event::Hurt) {
                break;
            }
        }
        if !bounced {
            continue;
        }
        assert!(!play.passed());
        loop {
            past |= play.x() >= 1000;
            play.step(&[R]);
            if play.world.player.grounded() {
                break;
            }
            assert!(!play.passed(), "wait {wait}, frame {}", play.world.frame);
            assert!(play.world.frame < 1000);
        }
        assert!(play.world.events.contains(&Event::Land));
        assert!(play.x() < 1000 || play.passed(), "wait {wait}");
        bounced_past += usize::from(past && play.passed());
    }
    assert!(bounced_past > 0);
}
