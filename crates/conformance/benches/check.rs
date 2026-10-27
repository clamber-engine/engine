//! Benchmark for `clamber-trace check`: the whole spec corpus, best of
//! several runs, and the slowest single scenario.
//!
//! Run with `cargo bench -p clamber-conformance`. In an optimized build it
//! fails when a scenario takes more than `BUDGET_PER_SCENARIO` on average
//! (load, simulation and comparison together), so the checker stays fast as
//! the corpus grows.

use std::time::{Duration, Instant};

use clamber_conformance::check::{self, Verdict};
use clamber_conformance::{record, root};
use clamber_trace::Scenario;

const RUNS: usize = 20;
const BUDGET_PER_SCENARIO: Duration = Duration::from_millis(2);
/// A made-up 120-frame v2 scenario: building its place from the geometry and
/// simulating it should stay well under a millisecond.
const V2_RUNS: u32 = 500;
const V2_BUDGET: Duration = Duration::from_millis(1);
const V2_SCENARIO: &str = "# clamber-scenario v2\nid: bench\nsummary: s\nframes: 120\n\n\
[geometry]\nfloor 0..39 0\nfloor 40..79 none\nfloor 80..199 16\n\n\
[start]\nmode: grounded\n\n[inputs]\n0-59 R\n20 A\n";

fn main() {
    let root = root();
    let mut best: Option<(Duration, Vec<check::Checked>)> = None;
    for _ in 0..RUNS {
        let start = Instant::now();
        let results = check::run(&root, 0).unwrap_or_else(|e| panic!("{e}"));
        let total = start.elapsed();
        if best.as_ref().is_none_or(|(t, _)| total < *t) {
            best = Some((total, results));
        }
    }
    let (total, results) = best.unwrap();
    let ran: Vec<_> = results
        .iter()
        .filter(|r| !matches!(r.verdict, Verdict::Error(_)) || !r.title.is_empty())
        .collect();
    let slowest = ran.iter().max_by_key(|r| r.elapsed).expect("no scenarios");
    let mean = total / ran.len().max(1) as u32;
    println!(
        "check: {} scenarios in {:.2} ms (best of {RUNS}); mean {:.3} ms per scenario; slowest {} at {:.3} ms",
        ran.len(),
        total.as_secs_f64() * 1000.0,
        mean.as_secs_f64() * 1000.0,
        slowest.key,
        slowest.elapsed.as_secs_f64() * 1000.0,
    );
    let scenario = Scenario::parse(V2_SCENARIO, "bench").unwrap_or_else(|e| panic!("{e}"));
    let start = Instant::now();
    for _ in 0..V2_RUNS {
        record(&scenario, std::path::Path::new("unused.level"), "default")
            .unwrap_or_else(|e| panic!("{e}"));
    }
    let v2_mean = start.elapsed() / V2_RUNS;
    println!(
        "v2: 120-frame geometry scenario, mean {:.3} ms over {V2_RUNS} runs",
        v2_mean.as_secs_f64() * 1000.0
    );
    if cfg!(debug_assertions) {
        println!("debug build: budget not enforced");
    } else if mean > BUDGET_PER_SCENARIO || v2_mean > V2_BUDGET {
        eprintln!(
            "over the budget of {BUDGET_PER_SCENARIO:?} (spec) or {V2_BUDGET:?} (v2) per scenario"
        );
        std::process::exit(1);
    }
}
