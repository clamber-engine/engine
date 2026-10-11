//! The parsers against the pinned spec itself: its scenarios, reference
//! traces and format examples must parse, and the name lists compiled into
//! this crate must match the spec's. A spec bump that breaks any of these
//! fails here first.

use std::path::{Path, PathBuf};

use clamber_trace::{Event, Metric, Scenario, State, Trace};

fn spec(path: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec")
        .join(path);
    assert!(
        path.exists(),
        "{} is missing; run `git submodule update --init`",
        path.display()
    );
    path
}

fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .collect();
    paths.sort();
    paths
}

/// The first fenced code block in a markdown file.
fn example(markdown: &str) -> String {
    let text = std::fs::read_to_string(spec(markdown)).unwrap();
    let start = text.find("```\n").expect("no code block") + 4;
    let len = text[start..].find("```").expect("unterminated code block");
    text[start..start + len].to_string()
}

/// Backticked names in the first column of the table under `## heading`.
fn table_names(markdown: &str, heading: &str) -> Vec<String> {
    let text = std::fs::read_to_string(spec(markdown)).unwrap();
    let section = text
        .split(&format!("## {heading}\n"))
        .nth(1)
        .expect("no such heading");
    let section = section.split("\n## ").next().unwrap();
    section
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .map(|l| l.split('`').next().unwrap().to_string())
        .collect()
}

#[test]
fn every_scenario_parses() {
    let paths = files_with_extension(&spec("scenarios"), "scn");
    assert!(!paths.is_empty());
    for path in paths {
        Scenario::load(&path).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn every_reference_trace_parses_and_has_a_scenario() {
    let dir = spec("traces").join("reference");
    if !dir.exists() {
        return; // None approved yet at this spec version.
    }
    for path in files_with_extension(&dir, "trace") {
        let trace = Trace::load(&path).unwrap_or_else(|e| panic!("{e}"));
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert_eq!(trace.scenario, stem, "{}", path.display());
        assert!(
            spec("scenarios").join(format!("{stem}.scn")).exists(),
            "{} has no scenario",
            path.display()
        );
    }
}

#[test]
fn format_examples_parse() {
    let scenario = Scenario::parse(&example("scenarios/FORMAT.md"), "scenarios/FORMAT.md").unwrap();
    assert_eq!(scenario.id, "run-jump-flat");
    assert_eq!(scenario.metrics.len(), 5);
    let trace = Trace::parse(&example("traces/FORMAT.md"), "traces/FORMAT.md").unwrap();
    assert_eq!(trace.rows.len(), 4);
}

#[test]
fn vocabulary_matches_spec() {
    let names = |all: Vec<&str>| all.into_iter().map(String::from).collect::<Vec<_>>();
    assert_eq!(
        names(State::ALL.iter().map(|s| s.name()).collect()),
        table_names("traces/VOCABULARY.md", "States")
    );
    assert_eq!(
        names(Event::ALL.iter().map(|e| e.name()).collect()),
        table_names("traces/VOCABULARY.md", "Events")
    );
}

#[test]
fn metric_names_match_spec() {
    let text = std::fs::read_to_string(spec("traces/METRICS.md")).unwrap();
    let table = text.split("## Curves").next().unwrap();
    let spec_names: Vec<&str> = table
        .lines()
        .filter_map(|l| l.strip_prefix("| `"))
        .map(|l| l.split('`').next().unwrap())
        .collect();
    let ours: Vec<&str> = Metric::ALL.iter().map(|m| m.name()).collect();
    assert_eq!(ours, spec_names);
}
