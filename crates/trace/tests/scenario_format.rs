//! Layer 2: the scenario parser against fixture files (TESTING.md).

use std::path::{Path, PathBuf};

use clamber_trace::scenario::InputLine;
use clamber_trace::{Buttons, Comparison, Curve, Decimal, Event, Field, Metric, Scenario};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scenarios")
        .join(path)
}

fn d(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}

fn buttons(s: &str) -> Buttons {
    Buttons::parse_scenario(s).unwrap()
}

#[test]
fn parses_every_form() {
    let s = Scenario::load(&fixture("valid/basic.scn")).unwrap();
    assert_eq!(s.id, "basic");
    assert_eq!(s.summary, "Fixture scenario.");
    assert_eq!(s.requires, "anything at all, even: colons");
    assert_eq!(s.start, "grounded, facing right");
    assert_eq!(s.frames, 10);
    assert_eq!(
        s.inputs,
        [
            InputLine {
                first: 0,
                last: 9,
                buttons: buttons("R")
            },
            InputLine {
                first: 2,
                last: 4,
                buttons: buttons("AJ")
            },
            InputLine {
                first: 7,
                last: 7,
                buttons: buttons("L")
            },
        ]
    );
    assert_eq!(
        s.metrics,
        [
            Comparison::Scalar {
                metric: Metric::ApexHeight,
                tol: d("0.5")
            },
            Comparison::Scalar {
                metric: Metric::Airtime,
                tol: d("1")
            },
            Comparison::Curve(Curve {
                fields: vec![Field::X, Field::Vy],
                event: Event::JumpStart,
                frames: 3,
                tol: d("0.25"),
            }),
        ]
    );
}

#[test]
fn input_lines_combine() {
    let s = Scenario::load(&fixture("valid/basic.scn")).unwrap();
    let held: Vec<String> = (0..s.frames).map(|f| s.held(f).to_string()).collect();
    assert_eq!(
        held,
        ["R", "R", "RJA", "RJA", "RJA", "R", "R", "LR", "R", "R"]
    );
    // Past the end, nothing is held.
    assert_eq!(s.held(10), Buttons::NONE);
}

#[test]
fn inputs_and_metrics_are_optional() {
    let s = Scenario::load(&fixture("valid/record-only.scn")).unwrap();
    assert!(s.inputs.is_empty());
    assert!(s.metrics.is_empty());
    assert_eq!(s.held(0), Buttons::NONE);
}

#[test]
fn labels_match_conformance_status_keys() {
    let s = Scenario::load(&fixture("valid/basic.scn")).unwrap();
    let labels: Vec<String> = s.metrics.iter().map(Comparison::label).collect();
    assert_eq!(
        labels,
        ["apex_height", "airtime", "curve x,vy from jump_start"]
    );
    assert_eq!(
        s.metrics[2].to_string(),
        "curve x,vy from jump_start for 3 tol 0.25"
    );
}

/// Each malformed fixture, with the line and part of the message its
/// error must have. Line 0 means the file as a whole.
const INVALID: &[(&str, usize, &str)] = &[
    ("wrong-version", 1, "unsupported scenario format `v2`"),
    (
        "missing-version",
        1,
        "first line must be `# clamber-scenario v1`",
    ),
    (
        "version-not-first",
        1,
        "first line must be `# clamber-scenario v1`",
    ),
    ("missing-id", 0, "missing required header `id`"),
    ("missing-frames", 0, "missing required header `frames`"),
    ("bad-id", 2, "id `Bad_Id` may only contain a-z, 0-9 and `-`"),
    (
        "id-mismatch",
        0,
        "id `some-other-id` doesn't match the file name; expected `id-mismatch`",
    ),
    ("unknown-key", 7, "unknown header key `author`"),
    ("duplicate-key", 7, "header `frames` appears twice"),
    ("empty-value", 3, "header `summary` is empty"),
    (
        "frames-zero",
        6,
        "frames must be a positive whole number, found `0`",
    ),
    (
        "frames-not-number",
        6,
        "frames must be a positive whole number, found `ten`",
    ),
    ("not-key-value", 7, "expected `key: value`"),
    ("unknown-section", 7, "unknown section `[outputs]`"),
    ("duplicate-section", 9, "section `[inputs]` appears twice"),
    (
        "header-after-section",
        9,
        "header lines must come before the first section",
    ),
    ("backwards-range", 8, "range `9-2` runs backwards"),
    (
        "range-past-end",
        8,
        "frame 10 is past the end; the scenario has frames 0-9",
    ),
    ("bad-frames", 8, "`-3` is not a frame or an inclusive range"),
    ("unknown-button", 8, "unknown button `X`"),
    ("repeated-button", 8, "button `R` listed twice"),
    ("input-extra-field", 8, "expected `FRAMES BUTTONS`"),
    ("input-missing-buttons", 8, "expected `FRAMES BUTTONS`"),
    ("unknown-metric", 8, "unknown metric `max_speed`"),
    ("missing-tol", 8, "expected `NAME tol T`"),
    ("negative-tol", 8, "tolerance `-1` is negative"),
    ("exponent-tol", 8, "`1e-3` is not a plain decimal number"),
    ("duplicate-metric", 9, "`airtime` is compared twice"),
    (
        "curve-unknown-field",
        8,
        "`state` can't be compared in a curve",
    ),
    ("curve-repeated-field", 8, "field `x` listed twice"),
    ("curve-unknown-event", 8, "unknown event `takeoff`"),
    (
        "curve-zero-frames",
        8,
        "curve length must be a positive whole number",
    ),
    (
        "curve-malformed",
        8,
        "expected `curve FIELDS from EVENT for N tol T`",
    ),
];

#[test]
fn rejects_malformed_files_with_a_useful_error() {
    let mut problems = Vec::new();
    for &(name, line, message) in INVALID {
        let path = fixture(&format!("invalid/{name}.scn"));
        match Scenario::load(&path) {
            Ok(_) => problems.push(format!("{name}: accepted")),
            Err(e) => {
                if e.line != line
                    || !e.message.contains(message)
                    || e.origin != path.display().to_string()
                {
                    problems.push(format!(
                        "{name}: got `{e}`, want line {line} containing `{message}`"
                    ));
                }
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn every_invalid_fixture_is_checked() {
    for entry in std::fs::read_dir(fixture("invalid")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap();
        assert!(
            INVALID.iter().any(|(n, _, _)| *n == name),
            "no expectation for {}",
            path.display()
        );
    }
}

#[test]
fn error_display_names_file_and_line() {
    let e = Scenario::load(&fixture("invalid/unknown-key.scn")).unwrap_err();
    assert!(e.to_string().ends_with("invalid/unknown-key.scn:7: unknown header key `author`; expected one of id, summary, requires, start, frames"));
    let e = Scenario::parse("# clamber-scenario v1\nid: x\n", "inline").unwrap_err();
    assert_eq!(e.to_string(), "inline: missing required header `frames`");
    let e = Scenario::parse("", "inline").unwrap_err();
    assert_eq!(e.to_string(), "inline: file is empty");
}
