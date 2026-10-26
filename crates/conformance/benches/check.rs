//! Benchmark for `clamber-trace check`: the whole spec corpus, best of
//! several runs, and the slowest single scenario.
//!
//! Run with `cargo bench -p clamber-conformance`. In an optimized build it
//! fails when a scenario takes more than `BUDGET_PER_SCENARIO` on average
//! (load, simulation and comparison together), so the checker stays fast as
//! the corpus grows.

use std::time::{Duration, Instant};

use clamber_conformance::check::{self, Verdict};
use clamber_conformance::root;

const RUNS: usize = 20;
const BUDGET_PER_SCENARIO: Duration = Duration::from_millis(2);

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
    if cfg!(debug_assertions) {
        println!("debug build: budget not enforced");
    } else if mean > BUDGET_PER_SCENARIO {
        eprintln!("over the budget of {BUDGET_PER_SCENARIO:?} per scenario");
        std::process::exit(1);
    }
}
