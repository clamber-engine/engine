//! The count display and the lives display in Classic 1
//! (spec/behavior/count-display.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{CountDisplay, FlowEvent, InputFrame, LivesDisplay, World, generic_feel, step};

/// The player stands at x = 600 on flat ground at y = 400.
const X: i32 = 600;
const F: i32 = 400;

/// A level with the player standing at (`X`, `F`) and a thing of `kind`
/// at each of `at`.
fn world(things: &[(&str, i32, i32)]) -> World {
    let mut src = format!(
        r#"
        format = 1
        name = "count"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]
        "#
    );
    for (kind, x, y) in things {
        src += &format!("\n[[entities]]\ntype = \"{kind}\"\nat = [{x}, {y}]\n");
    }
    let level = Arc::new(Level::from_toml_str(&src, "count").unwrap());
    let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    assert!(world.player.grounded());
    world
}

/// What the picture made on a frame shows.
#[derive(Clone, Copy, Debug)]
struct Shown {
    frame: u32,
    count: Option<CountDisplay>,
    lives: Option<LivesDisplay>,
    taken: bool,
}

/// One step with `held` held.
fn step_held(world: &mut World, held: Actions) -> Shown {
    step(world, &InputFrame::single(Frame::after(held, held)));
    Shown {
        frame: world.frame - 1,
        count: world.count_display(),
        lives: world.lives_display(),
        taken: world
            .flow
            .events
            .iter()
            .any(|e| matches!(e, FlowEvent::Collect(_))),
    }
}

/// Steps `frames` times standing still.
fn play(world: &mut World, frames: u32) -> Vec<Shown> {
    (0..frames)
        .map(|_| step_held(world, Actions::NONE))
        .collect()
}

/// The frames a pickup was taken on.
fn taken(rows: &[Shown]) -> Vec<u32> {
    rows.iter().filter(|r| r.taken).map(|r| r.frame).collect()
}

/// The frames the count display was shown on, as `(from, to)` runs.
fn shown_runs(rows: &[Shown]) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for r in rows.iter().filter(|r| r.count.is_some()) {
        match runs.last_mut() {
            Some((_, to)) if *to + 1 == r.frame => *to = r.frame,
            _ => runs.push((r.frame, r.frame)),
        }
    }
    runs
}

fn number(rows: &[Shown], frame: u32) -> Option<u32> {
    rows.iter()
        .find(|r| r.frame == frame)
        .and_then(|r| r.count)
        .map(|c| c.number)
}

#[test]
fn not_shown_when_a_level_begins() {
    let mut w = world(&[]);
    let rows = play(&mut w, 900);
    assert!(rows.iter().all(|r| r.count.is_none() && r.lives.is_none()));
}

#[test]
fn a_single_shows_the_count_from_the_next_frame_to_59_after_its_step() {
    let mut w = world(&[("spark", X, F - 8)]);
    w.flow.gems = 9;
    w.flow.shown = 9;
    let rows = play(&mut w, 200);
    let [p] = taken(&rows)[..] else {
        panic!("one pickup");
    };
    let step = rows
        .iter()
        .find(|r| r.count.is_some_and(|c| c.number == 10));
    assert_eq!(step.map(|r| r.frame), Some(p + 33), "counted 33 after");
    assert_eq!(shown_runs(&rows), [(p + 1, p + 92)]);
    assert_eq!(number(&rows, p + 1), Some(9));
    assert_eq!(number(&rows, p + 32), Some(9));
    assert_eq!(number(&rows, p + 33), Some(10));

    let c = rows[(p + 1 - rows[0].frame) as usize].count.unwrap();
    assert_eq!(c.image, (16, 8));
    assert_eq!(c.digits, (32, 8));
    assert_eq!(c.gap, 8);
}

#[test]
fn a_second_pickup_keeps_it_shown_until_59_after_its_own_step() {
    // The player walks onto the second spark a while after the first.
    let mut w = world(&[("spark", X, F - 8), ("spark", X + 60, F - 8)]);
    let mut rows = play(&mut w, 20);
    let mut right = Actions::NONE;
    right.insert(Action::Right);
    while taken(&rows).len() < 2 {
        assert!(rows.len() < 400, "the second spark is reached");
        rows.push(step_held(&mut w, right));
    }
    rows.extend(play(&mut w, 200));
    let [p, q] = taken(&rows)[..] else {
        panic!("two pickups");
    };
    assert!(
        q > p + 1 && q < p + 60,
        "taken while the first is shown: {p}, {q}"
    );
    assert_eq!(shown_runs(&rows), [(p + 1, q + 33 + 59)]);
    assert_eq!(number(&rows, q + 32), Some(1));
    assert_eq!(number(&rows, q + 33), Some(2));
}

#[test]
fn a_bunch_shows_it_from_the_frame_it_is_taken_one_step_a_frame() {
    let mut w = world(&[("cluster", X, F - 12)]);
    let rows = play(&mut w, 200);
    let [p] = taken(&rows)[..] else {
        panic!("one pickup");
    };
    let first = shown_runs(&rows)[0].0;
    assert_eq!(first, p);
    // The number shown never goes up by more than one a frame, and is
    // never ahead of the count.
    let mut last = 0;
    for r in &rows {
        if let Some(c) = r.count {
            assert!(c.number == last || c.number == last + 1, "{r:?}");
            last = c.number;
        }
    }
    assert_eq!(last, 10);
    assert_eq!(w.flow.shown, 10);
}

#[test]
fn two_counted_together_step_on_that_frame_and_the_next() {
    let mut w = world(&[]);
    w.flow.gems = 2;
    w.frame = 100;
    let rows = play(&mut w, 70);
    assert_eq!(number(&rows, 100), Some(1));
    assert_eq!(number(&rows, 101), Some(2));
    assert_eq!(shown_runs(&rows), [(100, 101 + 59)]);
}

#[test]
fn reaching_100_slides_the_lives_number_in_and_out() {
    let mut w = world(&[("spark", X, F - 8)]);
    w.flow.gems = 99;
    w.flow.shown = 99;
    let before = w.flow.lives;
    let rows = play(&mut w, 300);
    let [p] = taken(&rows)[..] else {
        panic!("one pickup");
    };
    let s = p + 33;
    assert_eq!(number(&rows, s - 1), Some(99));
    assert_eq!(number(&rows, s), Some(0));
    assert_eq!(w.flow.lives, before + 1);

    let at = |frame: u32| rows[(frame - rows[0].frame) as usize].lives;
    let ys: Vec<Option<i32>> = (s..=s + 152).map(|f| at(f).map(|l| l.y)).collect();
    let mut expected = vec![None; 9];
    expected.extend([-8, -4, 0, 4, 8].map(Some));
    expected.extend(std::iter::repeat_n(Some(10), 133));
    expected.extend([6, 2, -2, -6].map(Some));
    expected.extend([None, None]);
    assert_eq!(ys, expected);

    let lives = |f: u32| at(f).map(|l| l.number);
    assert_eq!(lives(s + 24), Some(before));
    assert_eq!(lives(s + 25), Some(before + 1));
    assert_eq!(at(s + 9).map(|l| l.x), Some(220));
}

#[test]
fn a_part_that_never_arrives_is_not_waited_on() {
    // A single released outside every place its `arrive` rules count
    // it from never reaches the count, and the display doesn't wait for
    // it forever.
    let mut w = world(&[("spark", X + 1000, F - 8)]);
    let rows = play(&mut w, 1);
    assert!(!rows[0].taken);
    let thing = w.things.iter().position(|t| t.kind == "spark").unwrap();
    w.flow.pickups.push(clamber_sim::Pickup {
        start: w.frame,
        owed: 1,
        thing,
    });
    w.things[thing].active = false;
    w.flow.releases.push(clamber_sim::Release {
        left: 1,
        wait: 0,
        every: 0,
        thing,
    });
    // Move the spark out of every arrive rule's place.
    w.things[thing].pos.x = clamber_sim::Fixed::from_int(-1000);
    let rows = play(&mut w, 5);
    assert!(w.flow.pickups.is_empty());
    assert!(rows.iter().all(|r| r.count.is_none()));
}
