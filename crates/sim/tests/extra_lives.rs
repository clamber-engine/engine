//! Extra-life items in Classic 1 (spec/behavior/extra-lives.md). They
//! are `heart`s; the rising kinds have a `rise`.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Arrival, Level, Vec2};
use clamber_sim::{
    Facing, Fixed, FlowEvent, InputFrame, LivesDisplay, Phase, START_LIVES, World, generic_feel,
    step,
};

use Action::{Left, Right};

/// The player stands at x = 600 on flat ground at y = 400.
const X: i32 = 600;
const F: i32 = 400;

const RISE: &str = "rise = 0.00390625";

/// A level with the player standing at (`X`, `F`) and an extra-life item
/// at each of `at`, with `params`.
fn world(at: &[(i32, i32)], params: &str) -> World {
    world_with(at, params, "")
}

/// [`world`], with the entities in `more` after the items.
fn world_with(at: &[(i32, i32)], params: &str, more: &str) -> World {
    let mut src = format!(
        r#"
        format = 1
        name = "hearts"
        size = [4000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 4000, 200]
        "#
    );
    for (x, y) in at {
        src += &format!(
            "\n[[entities]]\ntype = \"heart\"\nat = [{x}, {y}]\nparams = {{ {params} }}\n"
        );
    }
    src += more;
    let level = Arc::new(Level::from_toml_str(&src, "hearts").unwrap());
    let mut world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    // Without the lives display shown when play starts.
    world.flow.play_from = u32::MAX;
    assert!(world.player.grounded());
    world
}

/// Steps once with `keys` held; whether the item was taken on it.
fn step_keys(world: &mut World, keys: &[Action]) -> bool {
    let held = keys.iter().copied().collect::<Actions>();
    step(world, &InputFrame::single(Frame::after(held, held)));
    world.flow.events.contains(&FlowEvent::Collect(0))
}

/// Whether a standing player facing `facing` takes an item at `X - xf`
/// = `dx` and `D` = `below` on the first step.
fn taken_standing(dx: i32, below: i32, facing: Facing) -> bool {
    let mut w = world(&[(X - dx, F - below)], "");
    w.player.facing = facing;
    step_keys(&mut w, &[])
}

#[test]
fn standing_takes_an_item_inside_the_standing_ranges() {
    for (i, d) in [(-20, -29), (23, 35), (-20, 35), (23, -29), (0, 0)] {
        assert!(taken_standing(i, d, Facing::Right), "I = {i}, D = {d}");
    }
    for (i, d) in [(-21, 0), (24, 0), (0, -30), (0, 36)] {
        assert!(!taken_standing(i, d, Facing::Right), "I = {i}, D = {d}");
    }
}

#[test]
fn facing_left_the_ranges_are_three_px_further_left() {
    // extra-lives.md: facing left, X - xf from -26 to 17.
    assert!(taken_standing(-26, 0, Facing::Left));
    assert!(taken_standing(17, 0, Facing::Left));
    assert!(!taken_standing(-27, 0, Facing::Left));
    assert!(!taken_standing(18, 0, Facing::Left));
}

/// [`world`] with the second character as the player.
fn second_world(at: &[(i32, i32)]) -> World {
    let mut w = world(at, "");
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(clamber_pack::CharacterFeel {
        physics: clamber_pack::Presets::builtin()
            .slot("classic-1", 1)
            .unwrap(),
        abilities: feel.roster[0].abilities.clone(),
    });
    w.feel = Arc::new(feel);
    w.player.character = 1;
    w
}

/// Whether the second character, standing facing `facing`, takes an
/// item at `X - xf` = `dx` and `D` = `below` on the first step.
fn second_takes(dx: i32, below: i32, facing: Facing, keys: &[Action]) -> bool {
    let mut w = second_world(&[(X - dx, F - below)]);
    w.player.facing = facing;
    step_keys(&mut w, keys)
}

#[test]
fn the_second_character_standing_has_its_own_ranges() {
    // extra-lives.md, "The second character leading": I -17 to 13, D -31
    // to 33; facing left, the mirror image 3 px further left.
    for (i, d) in [(-17, -31), (13, 33), (-17, 33), (13, -31)] {
        assert!(second_takes(i, d, Facing::Right, &[]), "I = {i}, D = {d}");
    }
    for (i, d) in [(-18, 0), (14, 0), (0, -32), (0, 34)] {
        assert!(!second_takes(i, d, Facing::Right, &[]), "I = {i}, D = {d}");
    }
    assert!(second_takes(-16, 0, Facing::Left, &[]));
    assert!(second_takes(14, 0, Facing::Left, &[]));
    assert!(!second_takes(-17, 0, Facing::Left, &[]));
    assert!(!second_takes(15, 0, Facing::Left, &[]));
}

#[test]
fn the_second_character_crouching_has_its_own_ranges() {
    let mut w = second_world(&[(X, F)]);
    // Crouch first, with the item out of reach, then move the item in.
    w.things[0].pos.x = Fixed::from_int(X + 100);
    for _ in 0..30 {
        assert!(!step_keys(&mut w, &[Action::Down]));
    }
    for (i, d, want) in [
        (-24, -30, true),
        (16, 20, true),
        (-25, 0, false),
        (0, 21, false),
    ] {
        w.things[0].pos.x = Fixed::from_int(X - i);
        w.things[0].pos.y = Fixed::from_int(F - d);
        assert_eq!(step_keys(&mut w, &[Action::Down]), want, "I = {i}, D = {d}");
        w.things[0].active = true;
        w.things[0].phase = Phase::Ready;
    }
}

#[test]
fn taking_an_item_gives_its_lives_on_that_step_and_it_is_gone() {
    for lives in [1u8, 2, 3] {
        let mut w = world(&[(X, F)], &format!("lives = {lives}"));
        assert!(step_keys(&mut w, &[]));
        assert_eq!(w.flow.lives, START_LIVES + lives);
        let new = w.flow.events.iter().filter(|&&e| e == FlowEvent::Life);
        assert_eq!(new.count(), usize::from(lives));
        assert!(!w.things[0].active);
        assert_eq!(w.things[0].phase, Phase::Gone);
        for _ in 0..10 {
            assert!(!step_keys(&mut w, &[]));
        }
        assert_eq!(w.flow.lives, START_LIVES + lives);
    }
}

#[test]
fn a_rising_item_rises_faster_every_frame() {
    // extra-lives.md, "Rising": risen n (n + 1) / 512 px at the end of
    // frame a + n. In view at the level's start, it appears on frame 0.
    let y0 = 337;
    let mut w = world(&[(X + 200, y0)], RISE);
    assert!(w.things[0].active);
    let mut risen = Vec::new();
    for _ in 0..=120 {
        step_keys(&mut w, &[]);
        risen.push(Fixed::from_int(y0) - w.things[0].pos.y);
    }
    let px = |n: i64| Fixed::from_bits((n << 32) / 256);
    for (n, want) in [(0, 0), (1, 1), (2, 3), (10, 55), (60, 1830), (120, 7260)] {
        assert_eq!(risen[n], px(want), "frame a + {n}");
    }
}

#[test]
fn a_rising_item_is_gone_on_the_first_frame_it_would_reach_the_top() {
    // extra-lives.md's example: from level y 337 it's at about 1.43 at
    // the end of frame a + 414, and removed on a + 415.
    let mut w = world(&[(X + 200, 337)], RISE);
    for n in 0..=414 {
        step_keys(&mut w, &[]);
        assert!(w.things[0].active, "frame {n}");
    }
    let y = w.things[0].pos.y;
    assert!(y > Fixed::from_int(1) && y < Fixed::from_int(2), "{y:?}");
    step_keys(&mut w, &[]);
    assert!(!w.things[0].active);
    assert_eq!(w.things[0].phase, Phase::Broken);
}

#[test]
fn a_rising_items_y_is_rounded_up_for_the_pickup() {
    // Placed 40 px below the floor, it rises up through it. D = F - Y
    // reaches -29 once Y, its y rounded up, is 429: at the end of frame
    // 75 (risen 5 700 / 512, about 11.13), not 74 (about 10.84, so
    // y 429.16, which rounded down would already be in range).
    let mut w = world(&[(X, F + 40)], RISE);
    let taken = (0..200).find(|_| step_keys(&mut w, &[]));
    assert_eq!(taken, Some(75));
}

/// The lives display after each step from the one an item giving `lives`
/// is taken on, `P`, for `steps` steps.
fn display_after_taking(lives: u8, steps: usize) -> Vec<Option<LivesDisplay>> {
    let mut w = world(&[(X, F)], &format!("lives = {lives}"));
    assert!(step_keys(&mut w, &[]));
    let mut shown = vec![w.lives_display()];
    for _ in 1..steps {
        step_keys(&mut w, &[]);
        shown.push(w.lives_display());
    }
    shown
}

#[test]
fn the_lives_display_comes_in_steps_up_and_goes_out() {
    // extra-lives.md, "The lives display": with S = P + 44 (Clamber's
    // choice in the spec's 41 to 47), one more every 16 frames up to
    // E = S + 32 for 3 lives.
    let l = START_LIVES;
    let shown = display_after_taking(3, 250);
    let at = |k: usize| shown[k].map(|d| (d.number, d.y));
    assert_eq!(at(6), None);
    let ys: Vec<i32> = (7..=12).map(|k| shown[k].unwrap().y).collect();
    assert_eq!(ys, [-12, -8, -4, 0, 4, 8]);
    assert_eq!(at(13), Some((l, 10)));
    assert_eq!(at(43), Some((l, 10)));
    assert_eq!(at(44), Some((l + 1, 10)));
    assert_eq!(at(59), Some((l + 1, 10)));
    assert_eq!(at(60), Some((l + 2, 10)));
    let e = 44 + 32;
    assert_eq!(at(e - 1), Some((l + 2, 10)));
    assert_eq!(at(e), Some((l + 3, 10)));
    assert_eq!(at(e + 121), Some((l + 3, 10)));
    let ys: Vec<i32> = (e + 122..=e + 127).map(|k| shown[k].unwrap().y).collect();
    assert_eq!(ys, [6, 2, -2, -6, -10, -14]);
    assert_eq!(at(e + 128), None);
    assert_eq!(shown[e].unwrap().x, 220);
}

#[test]
fn an_item_that_leaves_the_window_untaken_never_comes_back() {
    // Out of reach above the floor, in the window to the right at the
    // start. Walk left until it's removed, then back right past it.
    let mut w = world(&[(X + 150, F - 200)], "");
    assert!(w.things[0].active);
    let mut removed = None;
    for n in 0..400 {
        step_keys(&mut w, &[Left]);
        if !w.things[0].active {
            removed = Some(n);
            break;
        }
    }
    assert!(removed.is_some(), "never removed");
    assert_eq!(w.things[0].phase, Phase::Broken);
    for _ in 0..600 {
        step_keys(&mut w, &[Right]);
        assert!(!w.things[0].active);
    }
    assert!(w.player.pos.x > Fixed::from_int(X + 150));
}

#[test]
fn an_item_goes_away_past_the_window_s_left_edge() {
    // extra-lives.md, "Going away untaken": there on every frame that
    // starts with the view's position V at x + 31 or less, removed on
    // the first that starts with it further right, and not there again
    // when the view comes back over it. Out of reach above the floor.
    let x = X - 100;
    for params in ["", RISE] {
        let mut w = world(&[(x, F - 200)], params);
        assert!(w.things[0].active, "{params}");
        let mut removed = false;
        for _ in 0..300 {
            let v = w.view.unwrap().x();
            step_keys(&mut w, &[Right]);
            assert_eq!(w.things[0].active, v <= i64::from(x) + 31, "V {v} {params}");
            if !w.things[0].active {
                removed = true;
                break;
            }
        }
        assert!(removed, "never removed {params}");
        assert_eq!(w.things[0].phase, Phase::Broken);
        for _ in 0..400 {
            step_keys(&mut w, &[Left]);
            assert!(!w.things[0].active, "{params}");
        }
        assert!(w.player.pos.x < Fixed::from_int(x));
    }
}

/// A still walker at level x `at`, to be knocked out by.
fn walker(at: i32) -> String {
    format!("\n[[entities]]\ntype = \"crawler\"\nat = [{at}, {F}]\nparams = {{ speed = 0 }}\n")
}

/// Walks right into the walker and waits for play to come back after
/// the knock-out.
fn knocked_out(w: &mut World) {
    let mut hit = false;
    for _ in 0..600 {
        step_keys(w, if hit { &[] } else { &[Right] });
        hit |= w.flow.events.contains(&FlowEvent::Die);
        if w.flow.events.contains(&FlowEvent::Respawn) {
            assert!(hit, "came back after the hit");
            return;
        }
    }
    panic!("never came back");
}

#[test]
fn an_item_gone_past_the_left_edge_is_there_again_after_a_knock_out() {
    // restart-barrel.md, "Extra-life items that went away untaken", for
    // an item that stays where it's placed and a rising one: gone past
    // the left edge, there again on resuming at the level's start, and
    // taking it gives its lives.
    let x = X - 40;
    for params in ["lives = 2", "lives = 2, rise = 0.00390625"] {
        let mut w = world_with(&[(x, F)], params, &walker(X + 400));
        assert!(w.things[0].active, "{params}");
        knocked_out(&mut w);
        assert_eq!(w.player.pos, w.level.start);
        let lives = w.flow.lives;
        assert_eq!(lives, START_LIVES - 1, "{params}");
        let thing = &w.things[0];
        assert!(thing.active, "there again {params}");
        assert_eq!(thing.phase, Phase::Ready);
        assert_eq!(thing.pos, w.level.entities[0].at);
        let mut taken = false;
        for _ in 0..60 {
            taken |= step_keys(&mut w, &[Left]);
        }
        assert!(taken, "{params}");
        assert_eq!(w.flow.lives, lives + 2, "{params}");
    }
}

#[test]
fn a_rising_item_that_rose_out_untaken_is_there_again_after_a_knock_out() {
    // restart-barrel.md, "Extra-life items that went away untaken": at
    // the level's start and at a restart point, it appears when its
    // entry is in the window and rises as on a first visit.
    let at = (X + 100, F - 200);
    for barrel in [None, Some(X + 200)] {
        let more = walker(X + 400)
            + &barrel.map_or(String::new(), |b| {
                format!("\n[[entities]]\ntype = \"keg\"\nat = [{b}, {}]\n", F - 9)
            });
        let mut w = world_with(&[at], RISE, &more);
        while w.things[0].active {
            step_keys(&mut w, &[]);
        }
        assert_eq!(w.things[0].phase, Phase::Broken);
        knocked_out(&mut w);
        if barrel.is_some() {
            assert_eq!(w.flow.checkpoints, [2], "restart point reached");
            assert_ne!(w.player.pos, w.level.start);
        }
        let thing = &w.things[0];
        assert!(thing.active, "there again {barrel:?}");
        assert_eq!(thing.pos, w.level.entities[0].at);
        // Everything waits while the player is in the barrel; from its
        // first move it rises as on a first visit.
        let mut risen = Vec::new();
        for _ in 0..200 {
            step_keys(&mut w, &[]);
            risen.push(Fixed::from_int(at.1) - w.things[0].pos.y);
        }
        let first = risen.iter().position(|&r| r > Fixed::ZERO).unwrap() - 1;
        let px = |n: i64| Fixed::from_bits((n << 32) / 256);
        let got = [0, 1, 2, 10].map(|n| risen[first + n]);
        assert_eq!(got, [0, 1, 3, 55].map(px), "{barrel:?}");
    }
}

#[test]
fn a_jump_up_from_under_an_item_takes_it_by_six_frames_after_take_off() {
    // extra-lives.md, "Pickup ranges": D = 40 is out of the standing
    // range, and the jump's later row takes it from T + 6 at the latest.
    for past in [-20, 0, 20] {
        let mut w = world(&[(X - past, F - 40)], "");
        assert!(!step_keys(&mut w, &[]));
        let mut start = None;
        let taken = (1..60).find(|&n| {
            let before = if n == 1 {
                Actions::default()
            } else {
                [Action::Jump].into_iter().collect()
            };
            let keys = [Action::Jump].into_iter().collect();
            step(&mut w, &InputFrame::single(Frame::after(before, keys)));
            let taken = w.flow.events.contains(&FlowEvent::Collect(0));
            if w.events.contains(&clamber_sim::Event::JumpStart) {
                start = Some(n);
            }
            taken
        });
        let (start, taken) = (start.unwrap(), taken.unwrap());
        assert!(taken <= start + 6, "I = {past}: T = {start}, P = {taken}");
    }
}

#[test]
fn crouching_has_its_own_ranges() {
    // Down held: taken once crouching starts, inside the crouching
    // ranges (I from -23 to 14, D from -33 to 26) but not the standing
    // ones.
    let crouch_take = |past: i32, below: i32| {
        let mut w = world(&[(X - past, F - below)], "");
        (0..30).find(|_| step_keys(&mut w, &[Action::Down]))
    };
    assert_eq!(crouch_take(-23, 0), Some(1));
    assert_eq!(crouch_take(-24, 0), None);
    assert_eq!(crouch_take(0, -33), Some(1));
    assert_eq!(crouch_take(0, -34), None);
    assert_eq!(crouch_take(20, 0), Some(0), "standing on the first step");
}

#[test]
fn past_99_lives_the_number_is_kept_and_shown_as_99() {
    // extra-lives.md, "More than 99 lives": an `extra-life-3` with 97
    // lives shows 98, 99 and 99 on its steps, and gives 100.
    for (before, steps) in [
        (96, [97, 98, 99]),
        (97, [98, 99, 99]),
        (99, [99; 3]),
        (150, [99; 3]),
    ] {
        let mut w = world(&[(X, F)], "lives = 3");
        w.flow.lives = before;
        assert!(step_keys(&mut w, &[]));
        assert_eq!(w.flow.lives, before + 3);
        let mut shown = vec![w.lives_display()];
        for _ in 1..250 {
            step_keys(&mut w, &[]);
            shown.push(w.lives_display());
        }
        let number = |k: usize| shown[k].map(|d| d.number);
        assert_eq!(number(13), Some(before.min(99)), "{before}");
        let got = [44, 60, 76].map(|k| number(k).unwrap());
        assert_eq!(got, steps, "{before}");
        assert!(shown.iter().flatten().all(|d| d.number <= 99));
        assert_eq!(shown[76].unwrap().x, 215, "two digits");
    }
}

/// Falls out of the level from where the player stands and plays on
/// until play comes back. Returns what the lives display shows after
/// the loss.
fn fall_out(w: &mut World) -> u8 {
    w.player.pos.y = Fixed::from_int(F + 900);
    w.player.ground = None;
    let (mut since, mut shown) = (None, None);
    for _ in 0..200 {
        step_keys(w, &[]);
        if w.flow.events.contains(&FlowEvent::Respawn) {
            return shown.expect("shown after the loss");
        }
        if w.flow.events.contains(&FlowEvent::Die) {
            since = Some(0);
        }
        since = since.map(|n| n + 1);
        if since == Some(60) {
            shown = w.lives_display().map(|d| d.number);
        }
    }
    panic!("play comes back");
}

#[test]
fn lost_lives_take_one_off_the_number_kept_past_99() {
    // extra-lives.md, "More than 99 lives", the example: the display
    // after each lost life tells apart the 99, 100 and 101 lives an
    // `extra-life-3` gave.
    for (before, after) in [(96, [98, 97, 96]), (97, [99, 98, 97]), (98, [99, 99, 98])] {
        let mut w = world(&[(X + 40, F)], "lives = 3");
        w.flow.lives = before;
        let mut taken = false;
        for _ in 0..60 {
            taken |= step_keys(&mut w, &[Right]);
        }
        assert!(taken, "{before}");
        assert_eq!(w.flow.lives, before + 3);
        let shown = after.map(|_| fall_out(&mut w));
        assert_eq!(shown, after, "{before}");
    }
}

#[test]
fn a_lost_life_from_100_shows_99_before_and_after() {
    let mut w = world(&[], "");
    w.flow.lives = 100;
    w.player.pos.y = Fixed::from_int(F + 900);
    w.player.ground = None;
    let mut numbers = Vec::new();
    for _ in 0..80 {
        step_keys(&mut w, &[]);
        numbers.extend(w.lives_display().map(|d| d.number));
    }
    assert_eq!(w.flow.lives, 99);
    assert!(!numbers.is_empty() && numbers.iter().all(|&n| n == 99));
}

/// Two `lives = 3` items taken on frames `p1` and `p2` with 5 lives
/// before (extra-lives.md, "Several items"): the lives display in the
/// picture made on each frame from 0 to 499.
fn two_items(p1: usize, p2: usize) -> Vec<Option<(u8, i32)>> {
    let mut w = world(&[(X + 60, F - 200), (X + 90, F - 200)], "lives = 3");
    w.flow.lives = 5;
    let mut shown = Vec::new();
    for n in 0..500 {
        for (i, p) in [p1, p2].into_iter().enumerate() {
            if n == p {
                w.things[i].pos = w.player.pos;
            }
        }
        step_keys(&mut w, &[]);
        shown.push(w.lives_display().map(|d| (d.number, d.y)));
    }
    assert_eq!(w.flow.lives, 11);
    shown
}

/// The frames the number shown changed on, after `from`.
fn steps(shown: &[Option<(u8, i32)>], from: usize) -> Vec<usize> {
    let number = |n: usize| shown[n].map(|d| d.0);
    (from + 1..shown.len())
        .filter(|&n| number(n - 1).is_some() && number(n).is_some() && number(n) != number(n - 1))
        .collect()
}

#[test]
fn an_item_taken_while_steps_are_due_holds_them_until_its_arrival() {
    // The spec's example with Clamber's arrival 34 frames after each
    // pickup: the first image arrives on 134 and 6 shows from 144. The
    // second, taken on 145, holds 145 to 178, so the next step comes on
    // the 16th frame not held after 144.
    let shown = two_items(100, 145);
    assert_eq!(shown[106], None);
    let ys: Vec<i32> = (107..=112).map(|n| shown[n].unwrap().1).collect();
    assert_eq!(ys, [-12, -8, -4, 0, 4, 8]);
    assert_eq!(shown[143], Some((5, 10)));
    assert_eq!(shown[144], Some((6, 10)));
    assert_eq!(steps(&shown, 144), [194, 210, 226, 242, 258]);
    assert_eq!(shown[258], Some((11, 10)));
    assert_eq!(shown[258 + 121], Some((11, 10)));
    let ys: Vec<i32> = (380..=385).map(|n| shown[n].unwrap().1).collect();
    assert_eq!(ys, [6, 2, -2, -6, -10, -14]);
    assert!(shown[386..].iter().all(Option::is_none));
}

#[test]
fn an_item_taken_on_the_frame_after_the_last_step_steps_on_its_arrival() {
    // The first item's steps are on 144, 160 and 176 (E).
    let shown = two_items(100, 177);
    assert_eq!(steps(&shown, 100), [144, 160, 176, 211, 227, 243]);
    // A frame later, the next step is 10 frames after the arrival.
    let shown = two_items(100, 178);
    assert_eq!(steps(&shown, 100), [144, 160, 176, 222, 238, 254]);
    assert!(shown[176..=254 + 121].iter().all(|d| d.unwrap().1 == 10));
}

#[test]
fn items_taken_together_step_from_the_later_arrival() {
    let shown = two_items(100, 100);
    assert_eq!(steps(&shown, 100), [144, 160, 176, 192, 208, 224]);
    let shown = two_items(100, 120);
    assert_eq!(steps(&shown, 100), [164, 180, 196, 212, 228, 244]);
}

#[test]
fn an_item_taken_while_the_display_leaves_turns_it_back() {
    // E = 176: leaving from 298 at y 6, 2, -2, ...
    let shown = two_items(100, 176 + 124);
    assert_eq!(shown[299].unwrap().1, 2);
    assert_eq!(shown[300].unwrap().1, 6);
    assert_eq!(shown[301].unwrap().1, 10);
    assert_eq!(steps(&shown, 176), [344, 360, 376]);
    assert!(shown[301..=376 + 121].iter().all(|d| d.unwrap().1 == 10));
    assert_eq!(shown[376 + 122].unwrap().1, 6);

    // Taken on E + 122, it stays at 10.
    let shown = two_items(100, 176 + 122);
    assert!(
        shown[298..=342 + 32 + 121]
            .iter()
            .all(|d| d.unwrap().1 == 10)
    );

    // Taken on E + 131, from y -26: not drawn at -22 and -18.
    let shown = two_items(100, 176 + 131);
    assert_eq!(shown[306], None);
    assert_eq!(shown[307], None);
    assert_eq!(shown[308], None);
    assert_eq!(shown[309], Some((8, -14)));
    assert_eq!(shown[315], Some((8, 10)));

    // Taken on E + 132, a new display comes in as for a single item.
    let q = 176 + 132;
    let shown = two_items(100, q);
    assert!(shown[q - 4..q + 7].iter().all(Option::is_none));
    assert_eq!(shown[q + 7], Some((8, -12)));
    assert_eq!(steps(&shown, q + 7), [q + 44, q + 60, q + 76]);
}

#[test]
fn a_rising_item_holds_still_after_the_hit_and_keeps_its_speed() {
    // extra-lives.md, "While the player is knocked out": it rises as
    // usual on H, doesn't move on H + 1 to H + 10, and from H + 11 rises
    // as if those ten frames hadn't happened.
    let mut w = world_with(&[(X - 40, 337)], RISE, &walker(X + 60));
    let mut ys = vec![w.things[0].pos.y];
    let mut hit = None;
    for n in 0..300 {
        let was = w.player.knockout.is_some();
        step_keys(&mut w, if hit.is_some() { &[] } else { &[Right] });
        ys.push(w.things[0].pos.y);
        if !was && w.player.knockout.is_some() {
            hit = Some(n);
        }
    }
    let h = hit.expect("never hit");
    // ys[n + 1] is where it is at the end of frame n.
    let risen = |n: usize| ys[n] - ys[n + 1];
    let px = |n: i64| Fixed::from_bits((n << 32) / 256);
    // It appeared on frame 0, so it rises n / 256 px on frame n.
    let h64 = h as i64;
    assert_eq!(risen(h), px(h64), "H");
    for k in 1..=10 {
        assert_eq!(risen(h + k), Fixed::ZERO, "H + {k}");
    }
    for k in 11..=40 {
        assert_eq!(risen(h + k), px(h64 + k as i64 - 10), "H + {k}");
    }
}

/// Coming back to `world`'s level through an exit, at `x` on the floor.
fn back_at(world: &World, x: i32) -> World {
    let arrival = Arrival {
        name: "back".into(),
        at: Vec2::ints(x, F),
        walk: None,
        entrance: None,
        drop: false,
    };
    world.coming_back(&arrival)
}

#[test]
fn a_rising_item_gone_untaken_is_there_again_after_an_exit() {
    // exits.md, "Coming back to the level": whether it rose out of the
    // level or went past the window's left edge, it's there again as
    // when the level begins, and taking it gives its lives.
    let params = "lives = 2, rise = 0.00390625";
    for (x, way) in [(X + 100, None), (X - 40, Some(Right))] {
        let mut w = world(&[(x, F)], params);
        assert!(w.things[0].active);
        while w.things[0].active {
            assert!(!step_keys(&mut w, way.as_slice()), "taken at {x}");
        }
        assert_eq!(w.things[0].phase, Phase::Broken);
        let lives = w.flow.lives;
        let mut back = back_at(&w, x);
        let thing = &back.things[0];
        assert!(thing.active, "there again at {x}");
        assert_eq!(thing.phase, Phase::Ready);
        assert_eq!(thing.pos, back.level.entities[0].at);
        assert!(step_keys(&mut back, &[]), "taken at {x}");
        assert_eq!(back.flow.lives, lives + 2);
    }
}

/// The lives display in the picture made on each frame from 0 to
/// `frames` - 1, with `lives` before, item `i` of `w` put on the player
/// on frame `take[i]`, and `each` run before every step.
fn played(
    w: &mut World,
    lives: u8,
    take: &[usize],
    frames: usize,
    each: impl Fn(&mut World, usize),
) -> Vec<Option<(u8, i32)>> {
    w.flow.lives = lives;
    let mut shown = Vec::new();
    for n in 0..frames {
        for (i, &p) in take.iter().enumerate() {
            if n == p {
                w.things[i].pos = w.player.pos;
            }
        }
        each(w, n);
        step_keys(w, &[]);
        shown.push(w.lives_display().map(|d| (d.number, d.y)));
    }
    shown
}

/// Items out of the player's reach, each giving 3 lives, with `params`.
fn far_items(count: i32, params: &str) -> World {
    let at: Vec<(i32, i32)> = (0..count).map(|i| (X + 60 + 30 * i, F - 200)).collect();
    world(&at, &format!("lives = 3, {params}"))
}

#[test]
fn a_late_item_counts_as_taken_a_frame_later() {
    // extra-lives.md, "Late items": the example, with Clamber's arrival
    // 34 frames after the pickup. The lives go up on the pickup.
    let mut w = far_items(1, "late = true");
    let shown = played(&mut w, 11, &[457], 700, |w, n| {
        if n == 458 {
            assert_eq!(w.flow.lives, 14);
        }
    });
    assert_eq!(shown[464], None);
    assert_eq!(shown[465], Some((11, -12)));
    assert_eq!(shown[471], Some((11, 10)));
    assert_eq!(steps(&shown, 465), [502, 518, 534]);
    assert_eq!(shown[534 + 121], Some((14, 10)));
    assert_eq!(shown[534 + 122], Some((14, 6)));

    // Not late, it comes in and steps a frame earlier.
    let mut w = far_items(1, "late = false");
    let shown = played(&mut w, 11, &[457], 600, |_, _| {});
    assert_eq!(shown[464], Some((11, -12)));
    assert_eq!(steps(&shown, 464), [501, 517, 533]);
}

#[test]
fn a_late_item_taken_with_another_holds_a_frame_later() {
    // Taken together, the late one's arrival is a frame after the
    // other's: the steps start 10 frames after that.
    let mut w = world_with(
        &[(X + 60, F - 200)],
        "lives = 3",
        &format!(
            "\n[[entities]]\ntype = \"heart\"\nat = [{}, {}]\nparams = {{ lives = 3, late = true }}\n",
            X + 90,
            F - 200
        ),
    );
    let shown = played(&mut w, 5, &[100, 100], 500, |_, _| {});
    assert_eq!(shown[107], Some((5, -12)));
    assert_eq!(steps(&shown, 107), [145, 161, 177, 193, 209, 225]);
}

#[test]
fn three_items_follow_the_same_rules() {
    // extra-lives.md, "Several items": the third holds 200 to 233.
    let mut w = far_items(3, "");
    let shown = played(&mut w, 5, &[100, 145, 200], 600, |_, _| {});
    assert_eq!(
        steps(&shown, 107),
        [144, 194, 244, 260, 276, 292, 308, 324, 340]
    );
    assert_eq!(shown[340], Some((14, 10)));
    assert_eq!(shown[340 + 121], Some((14, 10)));
    assert_eq!(shown[340 + 122], Some((14, 6)));
}

/// Before the step on frame `s`, the number shown goes from 99 to 0 on
/// it, with one more life.
fn reach_100(w: &mut World, n: usize, s: usize) {
    if n == s {
        (w.flow.shown, w.flow.gems) = (99, 0);
        w.flow.lives += 1;
    }
}

#[test]
fn the_count_reaching_100_after_an_item_steps_by_its_rules() {
    // extra-lives.md, "The count reaching 100": the item taken on 147
    // steps on 191, 207 and 223 (E).
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[147], 500, |w, n| reach_100(w, n, 224));
    // Later, with the number at y 10: the step comes on S + 12.
    assert_eq!(steps(&shown, 154), [191, 207, 223, 236]);
    assert_eq!(shown[236], Some((9, 10)));
    assert_eq!(shown[236 + 121], Some((9, 10)));
    assert_eq!(shown[236 + 122], Some((9, 6)));
    assert_eq!(shown[236 + 128], None);

    // On E: as an item taken on E + 1 with its arrival then.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[147], 500, |w, n| reach_100(w, n, 223));
    assert_eq!(steps(&shown, 154), [191, 207, 223, 224]);

    // On E + 121: it doesn't move up on E + 122.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[147], 500, |w, n| reach_100(w, n, 223 + 121));
    assert_eq!(shown[223 + 122], Some((8, 10)));
    assert_eq!(steps(&shown, 154), [191, 207, 223, 223 + 121 + 12]);

    // While it moves up, on E + 122: back at 10 on S + 1, the step on
    // S + 12.
    let s = 223 + 122;
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[147], 500, |w, n| reach_100(w, n, s));
    assert_eq!(shown[s], Some((8, 6)));
    assert_eq!(shown[s + 1], Some((8, 10)));
    assert_eq!(steps(&shown, 154), [191, 207, 223, s + 12]);

    // On E + 131: a new display.
    let s = 223 + 131;
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[147], 600, |w, n| reach_100(w, n, s));
    assert_eq!(shown[s + 7], None);
    assert_eq!(shown[s + 8], Some((8, -12)));
    assert_eq!(steps(&shown, s + 8), [s + 25]);
}

#[test]
fn the_count_reaching_100_before_an_item_steps_by_its_rules() {
    // The count's display comes in on S + 8 to S + 14 and steps on
    // S + 25 (E); the item taken on E + 1 steps on its arrival.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[147], 500, |w, n| reach_100(w, n, 121));
    assert_eq!(shown[128], None);
    assert_eq!(shown[129], Some((5, -12)));
    assert_eq!(shown[135], Some((5, 10)));
    assert_eq!(steps(&shown, 129), [146, 181, 197, 213]);
    assert_eq!(shown[213], Some((9, 10)));
}

/// Before the step on frame `j`, letter 1 joins letters 2, 3 and 4.
fn last_letter(w: &mut World, n: usize, j: usize) {
    if n == j {
        w.flow.letters = 0b1110;
        w.flow.joining.push((j as u32, 1));
    }
}

#[test]
fn the_four_letter_ending_holds_the_items_steps() {
    // extra-lives.md, "The four-letter ending": J = 500, G = 663. The
    // spec's example, with Clamber's arrival 34 frames after the pickup:
    // taken on 506, its image arrives on 540.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[506], 900, |w, n| last_letter(w, n, 500));
    assert_eq!(w.flow.lives, 9);
    assert_eq!(shown[506], None);
    assert_eq!(shown[507], Some((5, -12)));
    assert_eq!(shown[513], Some((5, 10)));
    assert_eq!(steps(&shown, 507), [550, 663, 679, 695]);
    assert_eq!(shown[695 + 121], Some((9, 10)));
    assert_eq!(shown[695 + 122], Some((9, 6)));
    assert_eq!(shown[695 + 128], None);

    // Its image arriving while held: the steps start on G.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[566], 900, |w, n| last_letter(w, n, 500));
    assert_eq!(steps(&shown, 507), [663, 679, 695, 711]);
}

#[test]
fn the_four_letter_ending_steps_on_g_with_or_without_items() {
    // No item: the step comes on G, and the number moves up from
    // G + 122.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[], 900, |w, n| last_letter(w, n, 500));
    assert_eq!(steps(&shown, 507), [663]);
    assert_eq!(shown[662], Some((5, 10)));
    assert_eq!(shown[663 + 121], Some((6, 10)));
    assert_eq!(shown[663 + 122], Some((6, 6)));

    // Taken before J, its steps over by J + 65: one more on G, and at
    // 10 up to G + 121.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[400], 900, |w, n| last_letter(w, n, 500));
    assert_eq!(steps(&shown, 407), [444, 460, 476, 663]);
    assert!(shown[413..=663 + 121].iter().all(|d| d.unwrap().1 == 10));
    assert_eq!(shown[663 + 122], Some((9, 6)));

    // Taken on G + 1: its first step on its arrival. Taken later: 10
    // frames after.
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[664], 900, |w, n| last_letter(w, n, 500));
    assert_eq!(steps(&shown, 507), [663, 698, 714, 730]);
    let mut w = far_items(1, "");
    let shown = played(&mut w, 5, &[670], 900, |w, n| last_letter(w, n, 500));
    assert_eq!(steps(&shown, 507), [663, 714, 730, 746]);
}

/// The last frame after the landing frame `L` (0 for `L` itself) on which
/// an item `past` px behind the player and `above` px above the feet
/// line, one only the jumping row reaches, is taken after a jump in
/// place with Jump held for `hold` frames; `None` if it is not taken on
/// `L` either.
fn last_taken_after_landing(mut w: World, hold: usize, past: i32, above: i32) -> Option<usize> {
    let jump: Actions = [Action::Jump].into_iter().collect();
    let none = Actions::default();
    let mut before = none;
    let mut landed = false;
    let mut after_landing = Vec::new();
    for n in 0..200 {
        let held = if n < hold { jump } else { none };
        let mut next = w.clone();
        step(&mut next, &InputFrame::single(Frame::after(before, held)));
        if landed || next.events.contains(&clamber_sim::Event::Land) {
            // The step from `w` is one of the landing's.
            after_landing.push((w.clone(), before, held));
            landed = true;
            if after_landing.len() > 24 {
                break;
            }
        }
        w = next;
        before = held;
    }
    let mut last = None;
    for (k, (mut w, before, held)) in after_landing.into_iter().enumerate() {
        w.things[0].pos.x = w.player.pos.x - Fixed::from_int(past);
        w.things[0].pos.y = Fixed::from_int(F - above);
        step(&mut w, &InputFrame::single(Frame::after(before, held)));
        if w.flow.events.contains(&FlowEvent::Collect(0)) {
            last = Some(k);
        }
    }
    last
}

/// extra-lives.md, "Landing": the jumping row goes on up to `L` + 3, or
/// `L` + 5 or `L` + 6 after Jump was held for 8 to 15 frames.
#[test]
fn the_jumping_row_goes_on_after_a_landing() {
    for hold in 1..=40 {
        let w = world(&[(X + 60, F - 45)], "");
        let up_to = match hold {
            8 | 9 | 14 | 15 => 6,
            10..=13 => 5,
            _ => 3,
        };
        assert_eq!(
            last_taken_after_landing(w, hold, 0, 45),
            Some(up_to),
            "Jump held for {hold}"
        );
    }
}

/// "The second character leading": one row through `L` + 16.
#[test]
fn the_second_character_s_row_goes_on_through_l_plus_16() {
    for hold in [1, 4, 10, 20, 30] {
        let w = second_world(&[(X + 60, F - 45)]);
        assert_eq!(
            last_taken_after_landing(w, hold, -20, 0),
            Some(16),
            "Jump held for {hold}"
        );
    }
}
