//! The count display and the lives display in Classic 1
//! (spec/behavior/count-display.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{
    CountDisplay, FlowEvent, InputFrame, LivesDisplay, START_LIVES, World, generic_feel, step,
};

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
    // Corrected in spec-v0.114.0: drawn from S + 8 to S + 152, partly
    // above the picture's top on the first and last two.
    let mut expected = vec![None; 8];
    expected.extend([-12, -8, -4, 0, 4, 8].map(Some));
    expected.extend(std::iter::repeat_n(Some(10), 133));
    expected.extend([6, 2, -2, -6, -10, -14].map(Some));
    assert_eq!(ys, expected);
    assert_eq!(at(s + 153), None);

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

/// A small walker `dx` px right of the player heading left at full speed
/// toward it, as in walker-contact.md, with the sizes the classic-1 hurt
/// box goes with.
fn walked_into(dx: i32) -> World {
    let src = format!(
        r#"
        format = 1
        name = "count"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]

        [[entities]]
        type = "crawler"
        at = [{x}, {F}]
        params = {{ speed = 1, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }}
        "#,
        x = X + dx
    );
    let level = Arc::new(Level::from_toml_str(&src, "count").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

/// Plays standing still until play comes back after a lost life. Returns
/// the rows up to the level's last frame, the frame the life was lost
/// on, and the frame a knock-out's flight stopped on, if there was one.
fn lose_a_life(world: &mut World) -> (Vec<Shown>, u32, Option<u32>) {
    let (mut rows, mut lost, mut stopped) = (Vec::new(), None, None);
    for _ in 0..600 {
        let row = step_held(world, Actions::NONE);
        if world.flow.events.contains(&FlowEvent::Respawn) {
            return (rows, lost.expect("a life lost"), stopped);
        }
        if world.flow.events.contains(&FlowEvent::Die) {
            lost = Some(row.frame);
        }
        if stopped.is_none() && world.player.knockout.is_some_and(|k| k.rest.is_some()) {
            stopped = Some(row.frame);
        }
        rows.push(row);
    }
    panic!("play comes back");
}

/// The lives display on each frame from `from` to `to`, as `(y, number,
/// image)`.
fn lives_from(rows: &[Shown], from: u32, to: u32) -> Vec<Option<(i32, u8, bool)>> {
    (from..=to)
        .map(|n| {
            let r = rows.iter().find(|r| r.frame == n).expect("frame played");
            r.lives.map(|d| (d.y, d.number, d.image))
        })
        .collect()
}

/// count-display.md, "Losing a life": the table from `A` to `L`.
fn follows_the_table(rows: &[Shown], a: u32, g: u32, l: u32, before: u8, after: u8) {
    let at = |y: i32, n: u8, image: bool| Some((y, n, image));
    assert!(
        lives_from(rows, rows[0].frame, a - 1)
            .iter()
            .all(Option::is_none)
    );
    let slide: Vec<_> = [-12, -8, -4, 0, 4, 8].map(|y| at(y, before, true)).into();
    assert_eq!(lives_from(rows, a, a + 5), slide);
    assert!(
        lives_from(rows, a + 6, g - 1)
            .iter()
            .all(|d| *d == at(10, before, true))
    );
    assert_eq!(lives_from(rows, g, g + 2), [at(10, before, false); 3]);
    assert!(lives_from(rows, g + 3, g + 10).iter().all(Option::is_none));
    let slide: Vec<_> = [-12, -8, -4, 0, 4, 8].map(|y| at(y, after, true)).into();
    assert_eq!(lives_from(rows, g + 11, g + 16), slide);
    assert!(
        lives_from(rows, g + 17, l)
            .iter()
            .all(|d| *d == at(10, after, true))
    );
}

#[test]
fn a_knock_out_shows_the_lives_before_then_after_the_burst() {
    // A = H + 18, G = F + 31 = L − 122, the second display still at
    // y 10 on L.
    let mut w = walked_into(22);
    let (rows, h, f) = lose_a_life(&mut w);
    let (f, l) = (f.expect("the flight stops"), rows.last().unwrap().frame);
    assert_eq!(l, f + 153);
    let lives = START_LIVES;
    follows_the_table(&rows, h + 18, f + 31, l, lives, lives - 1);
    let first = rows
        .iter()
        .find(|r| r.frame == h + 18)
        .unwrap()
        .lives
        .unwrap();
    assert_eq!(first.x, 220, "one digit at x 220");
}

#[test]
fn a_fall_out_shows_them_from_z_plus_7_and_z_plus_35() {
    let mut w = world(&[]);
    w.flow.lives = 10;
    play(&mut w, 5);
    w.player.pos.y = clamber_sim::Fixed::from_int(F + 900);
    w.player.ground = None;
    let (rows, z, stopped) = lose_a_life(&mut w);
    assert_eq!(stopped, None);
    let l = rows.last().unwrap().frame;
    assert_eq!(l, z + 96);
    follows_the_table(&rows, z + 7, z + 35, l, 10, 9);
    let first = rows
        .iter()
        .find(|r| r.frame == z + 7)
        .unwrap()
        .lives
        .unwrap();
    let second = rows
        .iter()
        .find(|r| r.frame == z + 46)
        .unwrap()
        .lives
        .unwrap();
    assert_eq!((first.x, second.x), (215, 220), "two digits, then one");
}

#[test]
fn a_lives_display_at_rest_stays_until_the_burst() {
    // "A lives display already shown": from reaching 100, at y 10 on H.
    // The hit comes about 40 frames in, with that display at y 10 from
    // 14 frames after it started.
    let mut w = walked_into(62);
    w.flow.lives = 7;
    w.flow.lives_step = Some(clamber_sim::LivesStep {
        at: 0,
        before: 6,
        after: 7,
    });
    let (rows, h, f) = lose_a_life(&mut w);
    let g = f.unwrap() + 31;
    assert!((14..147).contains(&h), "H = {h}");
    assert_eq!(
        lives_from(&rows, h, h).first().copied().flatten(),
        Some((10, 7, true))
    );
    assert!(
        lives_from(&rows, h, g - 1)
            .iter()
            .all(|d| *d == Some((10, 7, true)))
    );
    assert_eq!(lives_from(&rows, g, g + 2), [Some((10, 7, false)); 3]);
    assert!(lives_from(&rows, g + 3, g + 10).iter().all(Option::is_none));
    assert_eq!(lives_from(&rows, g + 17, g + 17), [Some((10, 6, true))]);
}
