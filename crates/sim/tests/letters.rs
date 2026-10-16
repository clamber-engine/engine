//! Letters, the letters taken and the letter row in Classic 1
//! (spec/behavior/letters.md).

use std::sync::Arc;

use clamber_input::{Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Facing, FlowEvent, InputFrame, START_LIVES, World, generic_feel, step};

/// The player stands at x = 200 on flat ground at y = 400.
const X: i32 = 200;
const F: i32 = 400;

/// A level with the player standing at (`X`, `F`) and a letter at each
/// `(x, y, letter)`.
fn world(letters: &[(i32, i32, u8)]) -> World {
    let mut src = format!(
        r#"
        format = 1
        name = "letters"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]
        "#
    );
    for (x, y, k) in letters {
        src += &format!(
            "\n[[entities]]\ntype = \"letter\"\nat = [{x}, {y}]\nparams = {{ letter = {k} }}\n"
        );
    }
    let level = Arc::new(Level::from_toml_str(&src, "letters").unwrap());
    let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    assert!(world.player.grounded());
    world
}

fn idle(world: &mut World) {
    step(
        world,
        &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
    );
}

/// Whether a letter `past` px behind the player the way it faces and
/// `below` px above its feet is taken on the first step, standing still
/// facing `facing`.
fn taken_standing(past: i32, below: i32, facing: Facing) -> bool {
    let xl = match facing {
        Facing::Right => X - past,
        Facing::Left => X + past,
    };
    let mut w = world(&[(xl, F - below, 1)]);
    w.player.facing = facing;
    idle(&mut w);
    w.flow.events.contains(&FlowEvent::Collect(0))
}

#[test]
fn standing_takes_letters_inside_the_standing_ranges_either_way() {
    for facing in [Facing::Right, Facing::Left] {
        for (past, below) in [(-15, -8), (21, 40), (-15, 40), (21, -8), (0, 0)] {
            assert!(
                taken_standing(past, below, facing),
                "{past}, {below}, {facing:?}"
            );
        }
        for (past, below) in [(-16, 0), (22, 0), (0, -9), (0, 41)] {
            assert!(
                !taken_standing(past, below, facing),
                "{past}, {below}, {facing:?}"
            );
        }
    }
}

#[test]
fn facing_left_reverses_the_horizontal_range() {
    // letters.md: facing left, X - xl from -21 to 15.
    let at = |dx: i32| {
        let mut w = world(&[(X - dx, F - 10, 1)]);
        w.player.facing = Facing::Left;
        idle(&mut w);
        w.flow.events.contains(&FlowEvent::Collect(0))
    };
    assert!(at(-21) && at(15));
    assert!(!at(-22) && !at(16));
}

#[test]
fn a_letter_joins_thirty_frames_after_and_never_comes_back() {
    let mut w = world(&[(X, F - 10, 2)]);
    let mut joined = None;
    let mut taken = 0;
    for n in 0..60 {
        idle(&mut w);
        taken += w
            .flow
            .events
            .iter()
            .filter(|e| **e == FlowEvent::Collect(0))
            .count();
        if w.flow.events.contains(&FlowEvent::Letter(2)) {
            assert_eq!(joined, None);
            joined = Some(n);
        }
        if n < 30 {
            assert_eq!(w.flow.letters, 0, "frame {n}");
        }
    }
    assert_eq!(taken, 1);
    assert_eq!(joined, Some(30));
    assert_eq!(w.flow.letters, 0b10);
    assert_eq!(w.flow.gems, 0, "letters add nothing to the count");
    assert_eq!(w.flow.lives, START_LIVES);
}

#[test]
fn the_fourth_letter_gives_a_life_in_any_order() {
    for last in [4, 1] {
        let mut w = world(&[(X, F - 10, last)]);
        w.flow.letters = 0b1111 & !(1 << (last - 1));
        let mut life = None;
        for n in 0..40 {
            idle(&mut w);
            if w.flow.events.contains(&FlowEvent::Life) {
                life = Some(n);
            }
        }
        assert_eq!(life, Some(30), "letter {last} last");
        assert_eq!(w.flow.lives, START_LIVES + 1);
        assert_eq!(w.flow.letters, 0, "the letters taken start again");
    }
}

#[test]
fn the_letter_row_slides_in_stays_and_slides_out() {
    let mut w = world(&[(X, F - 10, 1)]);
    let mut ys = Vec::new();
    for _ in 0..140 {
        idle(&mut w);
        ys.push(w.letter_row().map(|r| r.y));
    }
    // Taken on the first step, P = 0.
    assert_eq!(ys[0], None);
    assert_eq!(ys[1], None);
    let slide: Vec<_> = ys[2..10].iter().map(|y| y.unwrap()).collect();
    assert_eq!(slide, [-12, -8, -4, 0, 4, 8, 12, 16]);
    assert!(ys[10..122].iter().all(|&y| y == Some(12)));
    let out: Vec<_> = ys[122..128].iter().map(|y| y.unwrap()).collect();
    assert_eq!(out, [8, 4, 0, -4, -8, -12]);
    assert!(ys[128..].iter().all(Option::is_none));
}

#[test]
fn the_row_shows_the_letters_taken_in_their_slots() {
    // letters.md's example: letters 1 and 2 taken, letter 3 taken on
    // frame 100.
    let mut w = world(&[(X + 1000, F - 10, 3)]);
    w.flow.letters = 0b011;
    for _ in 0..100 {
        idle(&mut w);
    }
    assert_eq!(w.letter_row(), None, "not shown during ordinary play");
    w.things[0].pos = w.player.pos;
    let mut rows = Vec::new();
    for _ in 100..230 {
        idle(&mut w);
        rows.push(w.letter_row());
    }
    let at = |frame: usize| rows[frame - 100];
    let row = at(110).unwrap();
    assert_eq!(
        (row.y, row.x[0], row.x[1], row.x[2], row.x[3]),
        (12, 84, 108, 132, 156)
    );
    assert_eq!(row.count, 4);
    assert_eq!(row.letters, 0b011);
    assert_eq!(at(130).unwrap().letters, 0b111);
    assert_eq!(at(221).unwrap().y, 12);
    assert!(at(228).is_none() && at(229).is_none());
}

#[test]
fn the_default_preset_has_no_letter_row() {
    let level = r#"
        format = 1
        size = [400, 300]
        start = [100, 200]
        [[collision]]
        shape = "rect"
        rect = [0, 200, 400, 100]
        [[entities]]
        type = "letter"
        at = [100, 190]
    "#;
    let level = Arc::new(Level::from_toml_str(level, "test").unwrap());
    let mut w = World::new(level, Arc::new(generic_feel("default").unwrap()));
    for _ in 0..40 {
        idle(&mut w);
        assert_eq!(w.letter_row(), None);
    }
    assert_eq!(w.flow.letters, 1);
    assert_eq!(w.flow.lives, START_LIVES);
}
