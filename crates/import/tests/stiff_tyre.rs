//! The importer's stiff tyre in the simulation (`spec/behavior/tyre.md`,
//! "The stiff tyre"): placed as `Kind::StiffTyre` places it, on
//! `classic-1`.

use std::sync::Arc;

use clamber_import::Kind;
use clamber_input::{Action, Actions, Frame};
use clamber_level::{Entity, Level, Vec2};
use clamber_sim::{Fixed, InputFrame, Phase, World, generic_feel, step};

use Action::{Left as L, Right as R};

/// The floor's top.
const FLOOR: i32 = 300;

/// Flat ground at y = 300, the player at `start`, and a stiff tyre at
/// (200, `tyre_y`).
fn world(start: f64, tyre_y: i32) -> World {
    let src = format!(
        r#"
        format = 1
        size = [2000, 600]
        start = [{start}, 300]

        [[collision]]
        shape = "rect"
        rect = [0, {FLOOR}, 2000, 300]
        "#
    );
    let mut level = Level::from_toml_str(&src, "test").unwrap();
    let (kind, params) = Kind::StiffTyre.entity();
    level.entities.push(Entity {
        kind: kind.to_string(),
        id: None,
        at: Vec2::ints(200, tyre_y),
        params,
    });
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

#[derive(Clone, Copy, Debug)]
struct Row {
    /// The player's `x` rounded down less the tyre's.
    i: i64,
    /// How far the player's position is above the tyre's.
    u: Fixed,
    vy: Fixed,
    phase: Phase,
}

fn play(w: &mut World, frames: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    let mut last = Actions::NONE;
    (0..frames)
        .map(|k| {
            let now: Actions = held(k).into_iter().collect();
            step(w, &InputFrame::single(Frame::after(last, now)));
            last = now;
            let tyre = &w.things[0];
            Row {
                i: w.player.pos.x.floor() - tyre.pos.x.floor(),
                u: tyre.pos.y - w.player.pos.y,
                vy: w.player.vel.y,
                phase: tyre.phase,
            }
        })
        .collect()
}

fn catch(rows: &[Row], from: usize) -> Option<usize> {
    (from..rows.len()).find(|&k| matches!(rows[k].phase, Phase::Squashed { since: 0, .. }))
}

/// Walks the player from `x` toward the tyre holding `way`, releasing
/// it once caught. Returns the rows and the catch frame.
fn walk_in(x: f64, way: Action, frames: usize) -> (Vec<Row>, usize) {
    let mut w = world(x, FLOOR);
    let c = catch(&play(&mut w, 200, |_| vec![way]), 0).expect("no catch");
    let mut w = world(x, FLOOR);
    let rows = play(&mut w, frames, |k| if k <= c { vec![way] } else { vec![] });
    (rows, c)
}

#[test]
fn the_top_never_moves_and_the_throw_is_from_nine_up() {
    let (rows, c) = walk_in(101.0, R, 120);
    assert_eq!(rows[c].i, -24, "caught at the end of the range");
    for (k, r) in rows[c..=c + 14].iter().enumerate() {
        assert_eq!((r.u, r.vy), (fx(17.0), Fixed::ZERO), "C + {k}");
    }
    let l = c + 15;
    assert_eq!((rows[l].u, rows[l].vy), (fx(9.0), fx(-6.0)), "L: thrown");
}

#[test]
fn facing_left_the_range_reaches_22() {
    let (rows, c) = walk_in(300.0, L, 60);
    assert_eq!(rows[c - 1].i, 24, "not touching before C");
    assert_eq!(rows[c].i, 22);
}

#[test]
fn throws_with_no_input_take_41_frames() {
    let (rows, c) = walk_in(100.0, R, 600);
    let mut l = c + 15;
    for (n, f) in [0.0, 0.8125, 0.625, 0.4375, 0.25, 0.0625, 0.875]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            (rows[l].u, rows[l].vy),
            (fx(9.0 + f), fx(-6.0)),
            "throw {n}"
        );
        assert_eq!(rows[l + 25].u, fx(21.75 + f), "throw {n}: L + 25");
        let k = catch(&rows, l).unwrap();
        assert_eq!(k - l, 26, "throw {n}: caught on L + {}", k - l);
        assert_eq!(rows[k].u, fx(17.0 + (f + 0.8125) % 1.0), "throw {n}");
        l = k + 15;
    }
}

#[test]
fn a_tyre_up_to_52_px_up_lifts_a_player_walking_under_it() {
    for (height, lifted) in [(52, true), (53, false)] {
        let mut w = world(100.0, FLOOR - height);
        let rows = play(&mut w, 120, |_| vec![R]);
        assert_eq!(catch(&rows, 0).is_some(), lifted, "{height} px up");
    }
}
