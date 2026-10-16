//! Conformance (TESTING.md layer 5): for every spec scenario with a
//! reference trace for a title, the engine's trace on that title's preset
//! must pass each metric, except the ones `tests/conformance-status.toml`
//! expects to fail, and those must still fail.
//!
//! Each scenario and title is its own test, `conformance::<id>` for
//! `title-1` and `conformance::<title>/<id>` for the others, so
//! `cargo test --test conformance run-jump` runs one. This uses its own
//! small harness because the tests come from the spec's files.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clamber_conformance::{
    CONFORMANCE_TITLES, ConformanceTitle, STATUS_FILE, Status, record, root, spec_cases,
    title_level,
};
use clamber_trace::{Scenario, Trace, compare, title_mismatch};

/// A spec scenario compared against one title's reference trace.
struct Case {
    scenario: Scenario,
    title: ConformanceTitle,
    level: PathBuf,
    /// The scenario's key for this title (`ConformanceTitle::key`).
    key: String,
}

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

/// The scenario's reference trace for its title, if it has one and
/// metrics to compare.
fn reference(root: &Path, case: &Case) -> Option<PathBuf> {
    let path = case.title.reference(root, &case.scenario.id);
    (!case.scenario.metrics.is_empty() && path.exists()).then_some(path)
}

/// `None` if the scenario has nothing to compare yet for its title.
fn check(root: &Path, case: &Case, status: &Status) -> Option<Result<(), String>> {
    let scenario = &case.scenario;
    let reference = reference(root, case)?;
    let preset = case.title.preset;
    Some((|| {
        let reference = Trace::load(&reference).map_err(|e| e.to_string())?;
        let candidate = record(scenario, &case.level, preset)?;
        if let Some((r, c)) = title_mismatch(&reference, &candidate) {
            return Err(format!(
                "the reference trace is `{r}` but `{preset}` holds `{c}` values"
            ));
        }
        let outcomes = compare(scenario, &reference, &candidate);
        let problems = status.check(&case.key, &outcomes);
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
    let scenarios = spec_cases(&root).unwrap_or_else(|e| panic!("{e}"));
    let cases: Vec<Case> = CONFORMANCE_TITLES
        .iter()
        .flat_map(|title| {
            scenarios.iter().map(|c| Case {
                level: title_level(&root, &c.scenario.id, title.title),
                key: title.key(&c.scenario.id),
                scenario: c.scenario.clone(),
                title: *title,
            })
        })
        .collect();
    let status = Status::load(&root.join(STATUS_FILE)).unwrap_or_else(|e| panic!("{e}"));
    let name = |case: &Case| format!("conformance::{}", case.key);

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
            .find(|c| &c.key == id)
            .is_some_and(|c| reference(&root, c).is_some());
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
        match check(&root, case, &status) {
            None => {
                ignored += 1;
                println!(
                    "test {} ... ignored, no {} reference trace",
                    name(case),
                    case.title.title
                );
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
