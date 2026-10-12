//! Golden traces (TESTING.md layer 4): every spec scenario and every
//! playthrough, recorded now, must match its committed trace in
//! `tests/golden/` byte for byte. Re-record ("bless") them after an
//! intended behavior change with
//!
//! ```text
//! CLAMBER_BLESS=1 cargo test --test golden
//! ```

use std::collections::BTreeSet;
use std::fs;

use clamber_conformance::{GOLDEN_DIR, PRESET, playthrough_cases, record, root, spec_cases};

fn blessing() -> bool {
    std::env::var_os("CLAMBER_BLESS").is_some_and(|v| !v.is_empty() && v != "0")
}

#[test]
fn traces_match_goldens() {
    let root = root();
    let dir = root.join(GOLDEN_DIR);
    let mut cases = spec_cases(&root).unwrap();
    cases.extend(playthrough_cases(&root).unwrap());
    assert!(!cases.is_empty());

    let mut ids = BTreeSet::new();
    let mut problems = Vec::new();
    for case in &cases {
        let id = &case.scenario.id;
        assert!(ids.insert(id.clone()), "two scenarios are called `{id}`");
        let trace = record(&case.scenario, &case.level, PRESET)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .to_string();
        let path = dir.join(format!("{id}.trace"));
        if blessing() {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&path, &trace).unwrap();
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(golden) if golden == trace => {}
            Ok(golden) => {
                let line = golden
                    .lines()
                    .zip(trace.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or(golden.lines().count().min(trace.lines().count()));
                problems.push(format!(
                    "{id}: differs from {GOLDEN_DIR}/{id}.trace from line {}\n  \
                     golden:   {}\n  recorded: {}",
                    line + 1,
                    golden.lines().nth(line).unwrap_or("(end of file)"),
                    trace.lines().nth(line).unwrap_or("(end of file)"),
                ));
            }
            Err(_) => problems.push(format!("{id}: no golden trace at {GOLDEN_DIR}/{id}.trace")),
        }
    }

    // Goldens for scenarios that no longer exist.
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let is_trace = path.extension().is_some_and(|e| e == "trace");
        if !is_trace || !ids.contains(stem.as_ref()) {
            if blessing() {
                fs::remove_file(&path).unwrap();
                continue;
            }
            problems.push(format!("{}: no scenario records it", path.display()));
        }
    }

    assert!(
        problems.is_empty(),
        "{}\n\nIf the change is intended, bless the new traces with \
         `CLAMBER_BLESS=1 cargo test --test golden` and commit them on their own.",
        problems.join("\n")
    );
}

#[test]
fn recording_is_deterministic() {
    // The same run twice gives the same text, on top of the simulation's
    // own state check in crates/sim/tests/determinism.rs.
    let root = root();
    for case in spec_cases(&root).unwrap() {
        let first = record(&case.scenario, &case.level, PRESET).unwrap();
        let second = record(&case.scenario, &case.level, PRESET).unwrap();
        assert_eq!(
            first.to_string(),
            second.to_string(),
            "{}",
            case.scenario.id
        );
    }
}
