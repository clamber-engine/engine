//! How the walk-outs are written in a trace on `classic-1`: after the
//! level's end marker (spec/behavior/level-end.md, "In a trace") and
//! through an exit or a wall exit (exits.md and wall-exits.md, "In a
//! trace").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{InputFrame, Phase, World, generic_feel, record, row, step};
use clamber_trace::{Decimal, Event, Row, Scenario, State, trace::Facing};

const R: Action = Action::Right;
const L: Action = Action::Left;
const J: Action = Action::Jump;
const A: Action = Action::Action;

fn d(v: f64) -> Decimal {
    Decimal::parse(&v.to_string()).unwrap()
}

/// Flat ground with its top at y = 449, the lead standing at `start` on
/// it, and an entity of `kind` at x = 1000 on the ground.
fn level(start: i32, kind: &str) -> Level {
    let src = format!(
        r#"
        format = 1
        name = "walk-out"
        size = [3000, 700]
        start = [{start}, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]

        [[entities]]
        type = "{kind}"
        at = [1000, 449]
        "#
    );
    Level::from_toml_str(&src, "walk-out").unwrap()
}

fn scenario(inputs: &str, frames: u32) -> Scenario {
    Scenario::parse(
        &format!(
            "# clamber-scenario v1\n\
             id: t\nsummary: s\nrequires: r\nstart: s\nframes: {frames}\n\
             [inputs]\n{inputs}\n"
        ),
        "test",
    )
    .unwrap()
}

fn record_end(start: i32, inputs: &str) -> Vec<Row> {
    let feel = generic_feel("classic-1").unwrap();
    let trace = record(
        Arc::new(level(start, "end")),
        Arc::new(feel),
        &scenario(inputs, 400),
    )
    .unwrap();
    trace.rows
}

/// Checks the rows of a walk-out after the end marker from `T`, the row
/// before the lead leaves the ground, to the level's last frame, and
/// gives `T`.
fn level_end_rows(rows: &[Row]) -> usize {
    let t = rows
        .iter()
        .position(|r| r.has(Event::LeaveGround))
        .expect("the lead rises off the ground")
        - 1;
    assert!(rows.len() < 400, "no rows after the level's last frame");
    let on_t = &rows[t];
    assert_eq!(on_t.x, rows[t - 1].x, "T doesn't move");
    assert_eq!((on_t.vx, on_t.vy), (d(2.0), Decimal::ZERO));
    assert!(on_t.ground);
    assert_eq!(on_t.state, State::Walk);
    assert!(on_t.events.is_empty());

    let mut risen = false;
    let mut f0 = None;
    for (k, r) in rows.iter().enumerate().skip(t + 1) {
        assert!(!r.ground, "T + {}", k - t);
        assert_eq!(r.state, State::Fall, "T + {}", k - t);
        assert_eq!(r.vx, d(2.0), "T + {}", k - t);
        match f0 {
            None if r.vy == d(-0.125) => risen = true,
            None if r.vy.is_zero() => f0 = Some(k),
            Some(_) => assert!(r.vy.is_zero(), "no rise from F0 on"),
            None => panic!("vy {} on T + {}", r.vy, k - t),
        }
    }
    assert!(risen);
    assert_eq!(
        rows.len() - 1,
        f0.expect("F0") + 28,
        "F0 + 28 is the last row"
    );
    assert!(rows[t + 1].events.contains(&Event::LeaveGround));
    t
}

#[test]
fn after_the_end_marker_the_lead_falls_off_the_ground_until_the_level_is_over() {
    let rows = record_end(900, "0-399 R");
    let t = level_end_rows(&rows);
    for r in &rows[t + 1..] {
        assert_eq!(r.facing, Facing::Right);
    }
    assert!(rows[t + 2..].iter().all(|r| r.events.is_empty()));
}

#[test]
fn a_lead_facing_left_on_t_turns_on_t_plus_5() {
    // Standing past the marker facing left: Right passes it before the
    // turn is done.
    let rows = record_end(1050, "0 L\n31-399 R");
    let t = level_end_rows(&rows);
    assert_eq!(rows[t].facing, Facing::Left);
    for (k, r) in rows.iter().enumerate().skip(t + 2) {
        let turned = k == t + 5;
        assert_eq!(r.events, if turned { vec![Event::Turn] } else { vec![] });
        let facing = if k < t + 5 {
            Facing::Left
        } else {
            Facing::Right
        };
        assert_eq!(r.facing, facing, "T + {}", k - t);
    }
}

/// Steps `world` until its area is over, holding `keys(frame, taken)`
/// with `taken` once an exit took over, and gives each step's trace row
/// and `T`, the step it took over on.
fn walk_out(mut world: World, keys: impl Fn(usize, bool) -> Vec<Action>) -> (Vec<Row>, usize) {
    let start = world.player.pos;
    let mut held = Actions::NONE;
    let (mut rows, mut t) = (Vec::new(), None);
    while !world.flow.outcome.is_over() {
        let frame = rows.len();
        assert!(frame < 1000);
        let now: Actions = keys(frame, t.is_some()).into_iter().collect();
        step(&mut world, &InputFrame::single(Frame::after(held, now)));
        held = now;
        rows.push(row(&world, frame as u32, Default::default(), start, true));
        if t.is_none() && world.leaving.is_some() {
            t = Some(frame);
        }
    }
    (rows, t.expect("an exit took over"))
}

/// The rows from `T` + 1 to the area's last frame, walking `vx` with
/// `turn` only on `T` + 6 if the lead faced away from the walk-out on
/// `T`.
fn exit_rows(rows: &[Row], t: usize, vx: f64) {
    let away = match vx < 0.0 {
        true => Facing::Right,
        false => Facing::Left,
    };
    let turns = rows[t].facing == away;
    for (k, r) in rows.iter().enumerate().skip(t + 1) {
        let at = format!("T + {}", k - t);
        assert_eq!(r.state, State::Walk, "{at}");
        assert!(r.ground, "{at}");
        assert_eq!((r.vx, r.vy), (d(vx), Decimal::ZERO), "{at}");
        let turned = turns && k == t + 6;
        assert_eq!(
            r.events,
            if turned { vec![Event::Turn] } else { vec![] },
            "{at}"
        );
    }
    assert!(rows.len() > t + 30);
}

#[test]
fn through_an_exit_every_row_walks_whatever_is_held() {
    let busy = |frame: usize| match frame % 4 {
        0 => vec![L],
        1 => vec![L, J],
        2 => vec![R, A],
        _ => vec![J],
    };
    let world = World::new(
        Arc::new(level(1100, "exit")),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    // Left until the exit takes over, then anything.
    let (rows, t) = walk_out(world, |frame, taken| match taken {
        true => busy(frame),
        false => vec![L],
    });
    exit_rows(&rows, t, -2.0);
}

#[test]
fn a_trace_through_an_exit_ends_with_the_areas_last_frame() {
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let world = World::new(Arc::new(level(1100, "exit")), feel.clone());
    let (rows, _) = walk_out(world, |_, _| vec![L]);
    let trace = record(
        Arc::new(level(1100, "exit")),
        feel,
        &scenario("0-399 L", 400),
    )
    .unwrap();
    assert_eq!(trace.rows.len(), rows.len());
}

#[test]
fn through_a_wall_exit_every_row_walks_right() {
    for (start, facing) in [(900, None), (970, Some(L))] {
        let mut world = World::new(
            Arc::new(level(start, "wall-exit")),
            Arc::new(generic_feel("classic-1").unwrap()),
        );
        world.things[0].phase = Phase::Open;
        if facing.is_some() {
            world.player.facing = clamber_sim::Facing::Left;
        }
        let (rows, t) = walk_out(world, |_, _| match facing {
            Some(_) => vec![],
            None => vec![R],
        });
        assert_eq!(
            rows[t].facing,
            if facing.is_some() {
                Facing::Left
            } else {
                Facing::Right
            }
        );
        exit_rows(&rows, t, 2.0);
    }
}
