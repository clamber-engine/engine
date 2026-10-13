//! Conformance (TESTING.md layer 5): for every spec scenario with a
//! reference trace, the engine's trace must pass each metric, except the
//! ones `tests/conformance-status.toml` expects to fail, and those must
//! still fail.
//!
//! Each scenario is its own test, `conformance::<id>`, so
//! `cargo test --test conformance run-jump` runs one. This uses its own
//! small harness because the tests come from the spec's files.

use std::path::PathBuf;
use std::process::ExitCode;

use clamber_conformance::{
    CONFORMANCE_PRESET, Case, STATUS_FILE, Status, record, root, spec_cases,
};
use clamber_trace::{Trace, compare, title_mismatch};

struct Options {
    filters: Vec<String>,
    exact: bool,
    list: bool,
    /// Run only ignored tests; there are none.
    ignored: bool,
}

fn options() -> Options {
    let mut options = Options {
        filters: Vec::new(),
        exact: false,
        list: false,
        ignored: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exact" => options.exact = true,
            "--list" => options.list = true,
            "--ignored" => options.ignored = true,
            // libtest options that take a value.
            "--skip" | "--format" | "--test-threads" | "--color" | "-Z" | "--logfile" => {
                args.next();
            }
            _ if arg.starts_with('-') => {}
            _ => options.filters.push(arg),
        }
    }
    options
}

impl Options {
    fn selects(&self, name: &str) -> bool {
        !self.ignored
            && (self.filters.is_empty()
                || self.filters.iter().any(|f| {
                    if self.exact {
                        name == f
                    } else {
                        name.contains(f.as_str())
                    }
                }))
    }
}

/// The scenario's reference trace, if it has one and metrics to compare.
fn reference(case: &Case) -> Option<PathBuf> {
    let path = root()
        .join("spec/traces/reference")
        .join(format!("{}.trace", case.scenario.id));
    (!case.scenario.metrics.is_empty() && path.exists()).then_some(path)
}

/// `None` if the scenario has nothing to compare yet.
fn check(case: &Case, status: &Status) -> Option<Result<(), String>> {
    let scenario = &case.scenario;
    let reference = reference(case)?;
    Some((|| {
        let reference = Trace::load(&reference).map_err(|e| e.to_string())?;
        let candidate = record(scenario, &case.level, CONFORMANCE_PRESET)?;
        if let Some((r, c)) = title_mismatch(&reference, &candidate) {
            return Err(format!(
                "the reference trace is `{r}` but `{CONFORMANCE_PRESET}` holds `{c}` values"
            ));
        }
        let outcomes = compare(scenario, &reference, &candidate);
        let problems = status.check(&scenario.id, &outcomes);
        if problems.is_empty() {
            Ok(())
        } else {
            let all: Vec<String> = outcomes.iter().map(|o| format!("  {o}")).collect();
            Err(format!(
                "{}\n\nAll metrics:\n{}",
                problems.join("\n"),
                all.join("\n")
            ))
        }
    })())
}

fn main() -> ExitCode {
    let options = options();
    let root = root();
    let cases = spec_cases(&root).unwrap_or_else(|e| panic!("{e}"));
    let status = Status::load(&root.join(STATUS_FILE)).unwrap_or_else(|e| panic!("{e}"));
    let name = |case: &Case| format!("conformance::{}", case.scenario.id);

    if options.list {
        for case in &cases {
            println!("{}: test", name(case));
        }
        return ExitCode::SUCCESS;
    }

    // Every status entry must belong to a scenario that is compared, so
    // the file can't keep entries that check nothing.
    let mut failures = Vec::new();
    for id in status.expected_failures.keys() {
        let compared = cases
            .iter()
            .find(|c| &c.scenario.id == id)
            .is_some_and(|c| reference(c).is_some());
        if !compared {
            failures.push((
                STATUS_FILE.to_string(),
                format!("lists `{id}`, which has no reference trace or no spec scenario"),
            ));
        }
    }

    let selected: Vec<&Case> = cases.iter().filter(|c| options.selects(&name(c))).collect();
    println!("\nrunning {} tests", selected.len());
    let (mut passed, mut ignored) = (0, 0);
    for case in selected {
        match check(case, &status) {
            None => {
                ignored += 1;
                println!("test {} ... ignored, no reference trace", name(case));
            }
            Some(Ok(())) => {
                passed += 1;
                println!("test {} ... ok", name(case));
            }
            Some(Err(e)) => {
                println!("test {} ... FAILED", name(case));
                failures.push((name(case), e));
            }
        }
    }

    for (name, message) in &failures {
        println!("\n---- {name} ----\n{message}");
    }
    let result = if failures.is_empty() { "ok" } else { "FAILED" };
    println!(
        "\ntest result: {result}. {passed} passed; {} failed; {ignored} ignored\n",
        failures.len()
    );
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
