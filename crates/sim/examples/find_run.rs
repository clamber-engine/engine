//! Searches for a run from a level's start to its goal that loses no
//! life, on a preset (`default` if not given), and prints it as scenario
//! `[inputs]` lines. This is how `tests/playthrough/` runs are recorded:
//!
//! ```text
//! cargo run --release -p clamber-sim --example find_run -- <file.level> [beam width] [preset]
//! ```
//!
//! It's a beam search over inputs held for a few frames at a time,
//! keeping the runs that got furthest. Further means right, except in
//! regions with a sideways camera lock, which are climbs: there it means
//! up, from the floor where the region starts to the ground just past it.

use std::collections::HashSet;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Area, Dir, Level, Vec2};
use clamber_sim::{Fixed, FlowEvent, InputFrame, Outcome, START_LIVES, World, generic_feel, step};

/// Frames each choice of input is held for.
const HOLD: usize = 6;

/// Give up after this many frames.
const LIMIT: usize = 20 * 60 * 60;

/// How much a pixel of height counts for against one of distance,
/// outside climbs and in them.
const HEIGHT: f64 = 0.6;
const CLIMB: f64 = 10.0;

/// Worth of a pickup, and cost of changing what's held, in pixels.
const PICKUP: f64 = 4.0;
const CHANGE: f64 = 1.5;

#[derive(Clone)]
struct Run {
    world: World,
    held: Vec<Actions>,
    pickups: u32,
    changes: u32,
}

/// A climb: from `x0` to `x1`, up from `floor` to `top`.
struct Climb {
    x0: f64,
    x1: f64,
    floor: f64,
    top: f64,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: find_run <file.level> [beam width] [preset]");
        return ExitCode::from(2);
    };
    let beam = args
        .get(1)
        .map_or(400, |s| s.parse().expect("a beam width"));
    let preset = args.get(2).map_or("default", String::as_str);
    let Some(feel) = generic_feel(preset) else {
        eprintln!("no preset named `{preset}`");
        return ExitCode::from(2);
    };
    let level = match Level::load(Path::new(path)) {
        Ok(level) => level,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let climbs = climbs(&level);
    let start_y = level.start.y.to_f64();
    let world = World::new(Arc::new(level), Arc::new(feel));
    match search(world, beam, |run| score(run, &climbs, start_y)) {
        Some(run) => {
            eprintln!(
                "reached the goal in {} frames, {} pickups",
                run.held.len(),
                run.pickups
            );
            println!("frames: {}", run.held.len());
            for line in inputs(&run.held) {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        None => ExitCode::FAILURE,
    }
}

fn climbs(level: &Level) -> Vec<Climb> {
    let ground = |x: f64| {
        let from = Vec2::new(Fixed::from_f64(x).unwrap(), Fixed::ZERO);
        let hit = level.collision.cast(from, Dir::Down, level.size.y);
        hit.map_or(level.size.y.to_f64(), |h| h.point.y.to_f64())
    };
    let mut climbs: Vec<Climb> = level
        .regions
        .iter()
        .filter(|r| r.camera.is_some_and(|c| c.lock_x.is_some()))
        .filter_map(|r| match &r.area {
            Area::Rect(r) => {
                let (x0, x1) = (r.x.to_f64(), (r.x + r.w).to_f64());
                Some(Climb {
                    x0,
                    x1,
                    floor: ground(x0 + 8.0),
                    top: ground(x1 + 8.0),
                })
            }
            Area::Polygon(_) => None,
        })
        .collect();
    climbs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    climbs
}

/// How far along the run is: distance right, less height lost, with
/// climbs counted by height gained, all continuous across the edges of
/// climbs.
fn score(run: &Run, climbs: &[Climb], start_y: f64) -> f64 {
    let p = &run.world.player;
    let (x, y) = (p.pos.x.to_f64(), p.pos.y.to_f64());
    let bonus = PICKUP * f64::from(run.pickups) - CHANGE * f64::from(run.changes);
    let (mut base, mut from_x, mut from_y) = (0.0, 0.0, start_y);
    for c in climbs {
        if x < c.x0 {
            break;
        }
        base += (c.x0 - from_x) - HEIGHT * (c.floor - from_y);
        if x <= c.x1 {
            // Height above the top earns nothing: jumping higher than the
            // ledge isn't progress until the run lands on it.
            return base + CLIMB * (c.floor - y.max(c.top)) + 0.1 * (x - c.x0) + bonus;
        }
        base += CLIMB * (c.floor - c.top);
        (from_x, from_y) = (c.x1, c.top);
    }
    base + (x - from_x) - HEIGHT * (y - from_y) + bonus
}

fn search(world: World, beam: usize, score: impl Fn(&Run) -> f64) -> Option<Run> {
    use Action::{Down, Jump, Left, Right};
    let run_on = Action::Action;
    let choices: Vec<Actions> = [
        &[Right, run_on][..],
        &[Right, run_on, Jump],
        &[Right, Jump],
        &[Right],
        &[run_on],
        &[Right, run_on, Down],
        &[Left, run_on],
        &[Left, Jump],
        &[],
        &[Jump],
    ]
    .iter()
    .map(|a| a.iter().copied().collect())
    .collect();

    let mut runs = vec![Run {
        world,
        held: Vec::new(),
        pickups: 0,
        changes: 0,
    }];
    while runs[0].held.len() < LIMIT {
        let mut next = Vec::new();
        let mut seen = HashSet::new();
        for run in &runs {
            for &held in &choices {
                let Some(run) = extend(run, held) else {
                    continue;
                };
                if run.world.flow.outcome == Outcome::Complete {
                    return Some(run);
                }
                let p = &run.world.player;
                let key = (
                    p.pos.x.floor() / 3,
                    p.pos.y.floor() / 3,
                    (p.vel.x.to_f64() * 2.0) as i64,
                    p.vel.y.floor(),
                    p.state,
                );
                if seen.insert(key) {
                    next.push(run);
                }
            }
        }
        if next.is_empty() {
            let p = &runs[0].world.player;
            eprintln!("no way on from ({}, {})", p.pos.x, p.pos.y);
            return None;
        }
        next.sort_by(|a, b| score(b).total_cmp(&score(a)));
        next.truncate(beam);
        runs = next;
        let frames = runs[0].held.len();
        if frames % 600 == 0 {
            let p = &runs[0].world.player;
            eprintln!("frame {frames}: best at ({}, {})", p.pos.x, p.pos.y);
        }
    }
    eprintln!("no run within {LIMIT} frames");
    None
}

/// `run` with `held` for [`HOLD`] more frames, unless a life is lost.
fn extend(run: &Run, held: Actions) -> Option<Run> {
    let mut run = run.clone();
    let mut previous = run.held.last().copied().unwrap_or(Actions::NONE);
    if held != previous {
        run.changes += 1;
    }
    for _ in 0..HOLD {
        step(
            &mut run.world,
            &InputFrame::single(Frame::after(previous, held)),
        );
        previous = held;
        run.held.push(held);
        let flow = &run.world.flow;
        run.pickups += flow
            .events
            .iter()
            .filter(|e| matches!(e, FlowEvent::Collect(_)))
            .count() as u32;
        if flow.lives < START_LIVES {
            return None;
        }
        if flow.outcome == Outcome::Complete {
            break;
        }
    }
    Some(run)
}

/// Scenario input lines for `held`: one per run of frames that an
/// action is held, those with the same frames together.
fn inputs(held: &[Actions]) -> Vec<String> {
    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    for action in Action::ALL {
        let mut start = None;
        for (i, h) in held.iter().chain([&Actions::NONE]).enumerate() {
            match (h.contains(action), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    match spans.iter_mut().find(|(a, b, _)| (*a, *b) == (s, i - 1)) {
                        Some(span) => span.2.push(action.code()),
                        None => spans.push((s, i - 1, action.code().to_string())),
                    }
                    start = None;
                }
                _ => {}
            }
        }
    }
    spans.sort();
    spans
        .into_iter()
        .map(|(a, b, codes)| {
            let frames = if a == b {
                a.to_string()
            } else {
                format!("{a}-{b}")
            };
            format!("{frames:<10}{codes}")
        })
        .collect()
}
