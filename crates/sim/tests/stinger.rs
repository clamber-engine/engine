//! Stingers on `classic-1` (spec/behavior/stinger.md).

use std::sync::Arc;

use clamber_level::Level;
use clamber_sim::{Event, Fixed, InputFrame, Phase, View, World, generic_feel, step};

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Ground at y = 300 from x = 0 to 3000, the player standing at
/// `player`, and one stinger at `at` with `params`.
fn world(player: (i32, i32), at: (i32, i32), params: &str) -> World {
    let src = format!(
        r#"
        format = 1
        size = [3000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, 300, 3000, 100]

        [[entities]]
        type = "stinger"
        at = [{}, {}]
        params = {{ {params} }}
        "#,
        player.0, player.1, at.0, at.1
    );
    let level = Level::from_toml_str(&src, "stingers").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// stinger.md's example settings.
const EXAMPLE: &str = "reach = [6, 6, 3, 3], velocity = [-2, 1.5]";

/// The stinger's `x`, `y`, `vx`, `vy`, `tx` and `ty`.
fn sway(world: &World) -> [f64; 6] {
    let t = &world.things[0];
    let Phase::Swaying { vy, tx, ty } = t.phase else {
        panic!("{:?}", t.phase);
    };
    [t.pos.x, t.pos.y, t.vx, vy, tx, ty].map(Fixed::to_f64)
}

fn hurt(world: &World) -> bool {
    world.events.contains(&Event::Hurt)
}

/// stinger.md, "Example", with its start 200 px higher so that it stays
/// out of the player's reach.
#[test]
fn sways_as_the_example_says() {
    let mut w = world((1100, 300), (1000, 100), EXAMPLE);
    assert!(w.things[0].active);
    let rows = [
        [1000.0, 300.0, -2.0, 1.5, -2.0, 1.5],
        [998.0, 301.5, -2.0, 1.5, -2.0, 1.5],
        [996.0, 303.0, -2.0, 1.5, -2.0, 1.5],
        [994.0, 304.5, -2.0, 1.5, -2.0, 1.5],
        [992.0, 304.0, -2.0, 1.40625, -2.0, -1.5],
        [992.625, 305.40625, -1.875, 1.31640625, 2.0, -1.5],
        [
            990.87109375,
            306.72265625,
            -1.75390625,
            1.23046875,
            2.0,
            -1.5,
        ],
    ];
    for (frame, row) in rows.iter().enumerate() {
        step(&mut w, &InputFrame::default());
        let mut want = *row;
        want[1] -= 200.0;
        assert_eq!(sway(&w), want, "frame {frame}");
    }
    // Its furthest point left and its lowest are both on frame 26.
    let (mut left, mut low) = ((f64::MAX, 0), (f64::MIN, 0));
    for frame in 7..=60 {
        step(&mut w, &InputFrame::default());
        let [x, y, ..] = sway(&w);
        if x < left.0 {
            left = (x, frame);
        }
        if y > low.0 {
            low = (y, frame);
        }
    }
    assert_eq!(left, (975.8984375, 26));
    assert_eq!(low, (118.078125, 26));
}

/// stinger.md, "Moving": the swings of a stinger with reaches of 50 px
/// and of 0 at a start speed of -2.5.
#[test]
fn swings_well_past_its_turning_points() {
    let swings = |reach: i32| {
        let mut w = world(
            (1100, 300),
            (1000, 100),
            &format!("reach = [{reach}, {reach}, 0, 0], velocity = [-2.5, 0]"),
        );
        let mut xs = Vec::new();
        for _ in 0..2000 {
            step(&mut w, &InputFrame::default());
            xs.push(w.things[0].pos.x.to_f64());
            // It stays at the height of its start.
            assert_eq!(w.things[0].pos.y, f(100.0));
        }
        xs
    };
    let xs = swings(50);
    let first = xs.iter().copied().fold(f64::MAX, f64::min);
    assert!((first - 927.3).abs() < 0.05, "{first}");
    let late = &xs[1000..];
    let (low, high) = late
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), &x| (l.min(x), h.max(x)));
    assert!((low - 931.0).abs() < 1.0 && (high - 1070.0).abs() < 1.0);

    let xs = swings(0);
    let first = xs.iter().copied().fold(f64::MAX, f64::min);
    assert!((first - 977.3).abs() < 0.05, "{first}");
    assert!(xs[1000..].iter().all(|x| (x - 1000.0).abs() < 2.0));
}

#[test]
fn stays_at_its_start_without_speeds() {
    let mut w = world((1100, 300), (1000, 100), "reach = [6, 6, 3, 3]");
    for _ in 0..50 {
        step(&mut w, &InputFrame::default());
        assert_eq!(w.things[0].pos, clamber_level::Vec2::ints(1000, 100));
    }
}

/// stinger.md, "Example": a player standing facing right at x 976.5 is
/// within reach on frame 2, and hurt on frame 3. The stinger flies
/// through the ground to get there.
#[test]
fn hurts_one_frame_after_it_is_within_reach() {
    let mut w = world((976, 300), (1000, 300), EXAMPLE);
    w.player.pos.x = f(976.5);
    for frame in 0..=2 {
        step(&mut w, &InputFrame::default());
        assert!(!hurt(&w), "frame {frame}");
    }
    step(&mut w, &InputFrame::default());
    assert!(hurt(&w));
    assert_eq!(w.player.knockout.map(|k| k.by), Some(Some(0)));
}

/// stinger.md, "Touching the player": the upright ranges at their ends,
/// facing each way, with a stinger that doesn't move.
#[test]
fn reaches_an_upright_player_within_its_ranges() {
    // The player's x and y less the stinger's, and whether it hurts.
    let hurts = |left: bool, i: i32, a: i32| {
        let mut w = world((1000 + i, 300), (1000, 300 - a), "");
        if left {
            w.player.facing = clamber_sim::Facing::Left;
        }
        (0..3).any(|_| {
            step(&mut w, &InputFrame::default());
            hurt(&w)
        })
    };
    for (left, from, to) in [(false, -21, 32), (true, -27, 26)] {
        assert!(!hurts(left, from - 1, 0), "{left} {from}");
        assert!(hurts(left, from, 0), "{left} {from}");
        assert!(hurts(left, to, 0), "{left} {to}");
        assert!(!hurts(left, to + 1, 0), "{left} {to}");
    }
    // Above and below, through the floor.
    assert!(!hurts(false, 0, -21));
    assert!(hurts(false, 0, -20));
    assert!(hurts(false, 0, 29));
    assert!(!hurts(false, 0, 30));
}

/// stinger.md, "While the player is knocked out": the stinger that hurt
/// the player moves on H, holds still from H + 1 to H + 10 and moves on
/// from H + 11.
#[test]
fn holds_still_while_the_player_is_knocked_out() {
    let mut w = world((976, 300), (1000, 300), EXAMPLE);
    w.player.pos.x = f(976.5);
    for _ in 0..=2 {
        step(&mut w, &InputFrame::default());
    }
    let before = w.things[0].pos;
    step(&mut w, &InputFrame::default());
    assert!(hurt(&w));
    let at_hit = w.things[0].pos;
    assert_ne!(at_hit, before, "it moves on H");
    let mut moved_on = None;
    for _ in 0..200 {
        step(&mut w, &InputFrame::default());
        let Some(knockout) = w.player.knockout else {
            break;
        };
        if knockout.frame <= 10 {
            assert_eq!(w.things[0].pos, at_hit, "H + {}", knockout.frame);
        } else if moved_on.is_none() {
            moved_on = Some(knockout.frame);
            assert_ne!(w.things[0].pos, at_hit, "H + {}", knockout.frame);
        }
    }
    assert_eq!(moved_on, Some(11));
}

/// One step with the view's left edge at `left` when it starts. Returns
/// the stinger's `x` if it's in the level after the step.
fn step_at(world: &mut World, left: i32) -> Option<f64> {
    world.view = Some(View {
        left: Fixed::from_int(left),
        ..View::default()
    });
    step(world, &InputFrame::default());
    let t = &world.things[0];
    t.active.then(|| t.pos.x.to_f64())
}

/// stinger.md, "Appearing and being removed": the walkers' window,
/// `-31 <= X0 - V <= 288`, and removal at `X - V <= -128` or `>= 385`.
#[test]
fn appears_and_is_removed_with_the_view_and_comes_back_afresh() {
    let mut w = world((300, 300), (1600, 100), EXAMPLE);
    assert!(!w.things[0].active);
    // From the right: it appears at its start and doesn't move then.
    assert_eq!(step_at(&mut w, 1311), None);
    assert_eq!(step_at(&mut w, 1312), Some(1600.0));
    assert_eq!(step_at(&mut w, 1312), Some(1598.0));
    // Out of the window, but not yet far enough.
    let x = |w: &World| w.things[0].pos.x.floor() as i32;
    let near = x(&w) - 2 - 384;
    assert_eq!(step_at(&mut w, near), Some(1596.0));
    // Its x after this step's move is 1594.
    assert_eq!(step_at(&mut w, 1594 - 385), None);
    // It comes back afresh.
    assert_eq!(step_at(&mut w, 1600 + 31), Some(1600.0));
    assert_eq!(sway(&w), [1600.0, 100.0, -2.0, 1.5, -2.0, 1.5]);
    // From the left: removed once its x is 128 px left of the view.
    assert_eq!(step_at(&mut w, 1598 + 127), Some(1598.0));
    assert_eq!(step_at(&mut w, 1596 + 128), None);
    assert_eq!(step_at(&mut w, 1600 + 32), None);
}
