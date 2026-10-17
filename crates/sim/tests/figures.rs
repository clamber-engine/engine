//! Figures and the figure count in Classic 1 (spec/behavior/figure.md).
//! Figures are `idol`s; the figure count is the relic count.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{
    Event, Facing, FlowEvent, InputFrame, RelicImage, START_LIVES, World, generic_feel, step,
};

use Action::{Down, Jump};

/// The player stands at x = 600 on flat ground at y = 400.
const X: i32 = 600;
const F: i32 = 400;

/// A level with the player standing at (`X`, `F`) and a figure at each
/// of `at`.
fn world(at: &[(i32, i32)]) -> World {
    let mut src = format!(
        r#"
        format = 1
        name = "figures"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]
        "#
    );
    for (x, y) in at {
        src += &format!("\n[[entities]]\ntype = \"idol\"\nat = [{x}, {y}]\n");
    }
    let level = Arc::new(Level::from_toml_str(&src, "figures").unwrap());
    let world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    assert!(world.player.grounded());
    world
}

/// Steps once with `keys` held after `before`; whether the figure was
/// taken on it.
fn step_keys(world: &mut World, before: &[Action], keys: &[Action]) -> bool {
    let held = |k: &[Action]| k.iter().copied().collect::<Actions>();
    step(
        world,
        &InputFrame::single(Frame::after(held(before), held(keys))),
    );
    world.flow.events.contains(&FlowEvent::Collect(0))
}

/// Whether a standing player facing `facing` takes a figure at
/// `X - xf` = `dx` and `D` = `below` on the first step.
fn taken_standing(dx: i32, below: i32, facing: Facing) -> bool {
    let mut w = world(&[(X - dx, F - below)]);
    w.player.facing = facing;
    step_keys(&mut w, &[], &[])
}

#[test]
fn standing_takes_a_figure_inside_the_standing_ranges() {
    for (i, d) in [(-22, -27), (31, 37), (-22, 37), (31, -27), (0, 0)] {
        assert!(taken_standing(i, d, Facing::Right), "I = {i}, D = {d}");
    }
    for (i, d) in [(-23, 0), (32, 0), (0, -28), (0, 38)] {
        assert!(!taken_standing(i, d, Facing::Right), "I = {i}, D = {d}");
    }
}

#[test]
fn facing_left_the_ranges_are_three_px_further_right() {
    // figure.md: facing left, X - xf from -28 to 25.
    assert!(taken_standing(-28, 0, Facing::Left));
    assert!(taken_standing(25, 0, Facing::Left));
    assert!(!taken_standing(-29, 0, Facing::Left));
    assert!(!taken_standing(26, 0, Facing::Left));
}

/// The step on which a player holding Down first takes a figure at `I` =
/// `past`, `D` = `below`, if within 30.
fn crouch_take(past: i32, below: i32) -> Option<usize> {
    let mut w = world(&[(X - past, F - below)]);
    let mut before: &[Action] = &[];
    (0..30).find(|_| {
        let taken = step_keys(&mut w, before, &[Down]);
        before = &[Down];
        taken
    })
}

#[test]
fn crouching_has_its_own_ranges() {
    // Taken once crouching starts (on the second step), inside the
    // crouching ranges but not the standing ones; never just past them.
    assert_eq!(crouch_take(-25, 0), Some(1));
    assert_eq!(crouch_take(-26, 0), None);
    assert_eq!(crouch_take(0, -31), Some(1));
    assert_eq!(crouch_take(0, -32), None);
    assert_eq!(crouch_take(31, 0), Some(0), "standing on the first step");
}

/// A step's player x and feet line, whole px, and how many steps before
/// its jump started (`None`: not jumping).
type JumpRow = (i32, i32, Option<usize>);

/// A jump straight up from standing with Jump held for `held` steps: the
/// keys for each step, and its [`JumpRow`].
fn jump_rows(held: usize) -> (Vec<&'static [Action]>, Vec<JumpRow>) {
    let mut keys: Vec<&[Action]> = vec![&[]; 3];
    keys.extend(vec![&[Jump][..]; held]);
    keys.extend(vec![&[][..]; 90 - held]);
    let mut w = world(&[]);
    let mut start = None;
    let mut rows = Vec::new();
    let mut before: &[Action] = &[];
    for (n, k) in keys.iter().enumerate() {
        step_keys(&mut w, before, k);
        before = k;
        if w.events.contains(&Event::JumpStart) {
            start = Some(n);
        }
        if w.events.contains(&Event::Land) {
            start = None;
        }
        let p = &w.player;
        let feet = -((-p.pos.y).floor() as i32);
        rows.push((p.pos.x.floor() as i32, feet, start.map(|s| n - s)));
    }
    (keys, rows)
}

#[test]
fn a_jump_has_the_standing_ranges_for_six_steps_then_its_own() {
    for held in [40, 4] {
        let (keys, rows) = jump_rows(held);
        for past in [-23, -22, 27, 28, 29, 31, 32] {
            for below in -30..60 {
                let want = rows.iter().position(|&(x, feet, age)| {
                    let [i0, i1, d0, d1] = match age {
                        Some(a) if a >= 6 => [-22, 28, -6, 54],
                        _ => [-22, 31, -27, 37],
                    };
                    let (i, d) = (x - (X - past), feet - (F - below));
                    (i0..=i1).contains(&i) && (d0..=d1).contains(&d)
                });
                let mut w = world(&[(X - past, F - below)]);
                let mut before: &[Action] = &[];
                let got = keys.iter().position(|k| {
                    let taken = step_keys(&mut w, before, k);
                    before = k;
                    taken
                });
                assert_eq!(got, want, "jump held {held}: I = {past}, D = {below}");
            }
        }
    }
}

#[test]
fn a_figure_adds_one_to_the_figure_count_and_nothing_else() {
    let mut w = world(&[(X, F - 4)]);
    w.flow.relics = 2;
    assert!(step_keys(&mut w, &[], &[]));
    for _ in 0..120 {
        assert!(!step_keys(&mut w, &[], &[]), "taken once");
    }
    assert_eq!(w.flow.relics, 3);
    assert_eq!(w.flow.gems, 0);
    assert_eq!(w.flow.lives, START_LIVES);
}

/// The relic image in each picture from the one made on the step a
/// figure at (`xf`, `yf`) is taken by a standing player, over `steps`.
fn images(xf: i32, yf: i32, preset: &str, steps: usize) -> Vec<Option<RelicImage>> {
    let mut w = world(&[(xf, yf)]);
    w.feel = Arc::new(generic_feel(preset).unwrap());
    assert!(step_keys(&mut w, &[], &[]));
    let mut out = vec![w.relic_image()];
    for _ in 1..steps {
        step_keys(&mut w, &[], &[]);
        out.push(w.relic_image());
    }
    out
}

/// figure.md, "How the taken figure is shown": the frame `A` the image
/// is first at its resting place, counted from `P`.
fn arrival(images: &[Option<RelicImage>]) -> usize {
    images
        .iter()
        .position(|i| i.is_some_and(|i| (i.x, i.y) == (74, 2)))
        .unwrap()
}

#[test]
fn the_taken_figure_flies_to_its_resting_place() {
    // Figures as high above the floor as a standing player reaches.
    for (xf, yf) in [(X + 20, F - 37), (X, F - 37), (X - 20, F - 37)] {
        let images = images(xf, yf, "classic-1", 120);
        assert_eq!(images[0], None, "not drawn in the picture made on P");
        let a = arrival(&images);
        assert!((29..=33).contains(&a), "A = P + {a} for ({xf}, {yf})");
        // Slowing as it gets near: each frame's move is no bigger than the
        // one before, and never past the resting place.
        let at: Vec<_> = images[1..=a].iter().map(|i| i.unwrap()).collect();
        assert_eq!(at[0].size, 34);
        let moves: Vec<_> = at
            .windows(2)
            .map(|w| ((w[1].x - w[0].x), (w[1].y - w[0].y)))
            .collect();
        for w in moves.windows(2) {
            assert!(w[1].0.abs() <= w[0].0.abs() && w[1].1.abs() <= w[0].1.abs());
        }
        for i in &at {
            assert!((i.x - 74).signum() * (at[0].x - 74).signum() >= 0);
            assert!((i.y - 2).signum() * (at[0].y - 2).signum() >= 0);
        }
    }
}

#[test]
fn the_taken_figure_rests_then_rises_out_of_the_picture() {
    let images = images(X, F - 37, "classic-1", 140);
    let a = arrival(&images);
    for (n, image) in images.iter().enumerate().skip(a) {
        let image = image.map(|i| (i.x, i.y));
        match n - a {
            0..=62 => assert_eq!(image, Some((74, 2)), "A + {}", n - a),
            k @ 63..=70 => {
                // The bottom row of the box: 31 on A + 63 … 3 on A + 70.
                let bottom = 31 - 4 * (k as i32 - 63);
                assert_eq!(image, Some((74, bottom - 33)), "A + {k}");
            }
            k => assert_eq!(image, None, "A + {k}"),
        }
    }
}

#[test]
fn without_a_relic_image_none_is_shown() {
    assert!(images(X, F - 4, "default", 60).iter().all(Option::is_none));
}
