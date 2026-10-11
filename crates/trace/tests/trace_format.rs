//! Layer 2: the trace parser against fixture files (TESTING.md).

use std::path::{Path, PathBuf};

use clamber_trace::trace::{Facing, Source};
use clamber_trace::{Buttons, Decimal, Event, Row, State, Trace};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/traces")
        .join(path)
}

fn d(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}

#[test]
fn parses_header_and_rows() {
    let t = Trace::load(&fixture("valid/basic.trace")).unwrap();
    assert_eq!(t.scenario, "basic");
    assert_eq!(t.source, Source::Engine);
    assert_eq!(
        t.extra,
        [("engine-commit".to_string(), "0123abc".to_string())]
    );
    assert_eq!(t.rows.len(), 4);
    assert_eq!(
        t.rows[3],
        Row {
            frame: 3,
            input: Buttons::parse_trace("LRJA").unwrap(),
            x: d("2"),
            y: d("-5.5"),
            vx: d("-0.0625"),
            vy: d("-2.5"),
            state: State::Jump,
            ground: false,
            facing: Facing::Left,
            events: vec![Event::JumpRelease, Event::Turn],
        }
    );
}

#[test]
fn events_column_may_be_absent_or_empty() {
    let t = Trace::load(&fixture("valid/basic.trace")).unwrap();
    // Row 0 has 9 columns, row 1 has an empty 10th.
    assert!(t.rows[0].events.is_empty());
    assert!(t.rows[1].events.is_empty());
    assert_eq!(t.rows[2].events, [Event::JumpStart]);
    assert_eq!(t.rows[0].input, Buttons::parse_trace("R").unwrap());
    assert!(t.rows[0].ground);
    assert_eq!(t.rows[0].facing, Facing::Right);
}

#[test]
fn accepts_crlf_line_endings() {
    let text = std::fs::read_to_string(fixture("valid/basic.trace")).unwrap();
    let crlf = Trace::parse(&text.replace('\n', "\r\n"), "crlf").unwrap();
    assert_eq!(crlf, Trace::load(&fixture("valid/basic.trace")).unwrap());
}

/// Each malformed fixture, with the line and part of the message its
/// error must have. Line 0 means the file as a whole.
const INVALID: &[(&str, usize, &str)] = &[
    ("empty", 0, "file is empty"),
    ("no-magic", 1, "first line must be `# clamber-trace v1`"),
    (
        "wrong-version",
        1,
        "unsupported trace format `# clamber-trace v2`",
    ),
    ("missing-scenario", 0, "missing `# scenario:` header"),
    ("missing-source", 0, "missing `# source:` header"),
    (
        "bad-source",
        3,
        "source must be `reference` or `engine`, found `emulator`",
    ),
    ("duplicate-scenario", 4, "header `scenario` appears twice"),
    (
        "header-not-key-value",
        4,
        "header lines must be `# key: value`",
    ),
    ("missing-columns", 4, "expected the column line"),
    ("wrong-columns", 4, "expected the column line"),
    ("space-columns", 4, "column line must be tab-separated"),
    ("no-rows", 0, "trace has no frames"),
    (
        "too-few-fields",
        5,
        "expected 10 tab-separated columns, found 8",
    ),
    (
        "too-many-fields",
        5,
        "expected 10 tab-separated columns, found 11",
    ),
    (
        "space-separated-row",
        5,
        "expected 10 tab-separated columns, found 1",
    ),
    ("bad-frame", 5, "frame must be a whole number, found `-1`"),
    (
        "frames-not-increasing",
        6,
        "frame 0 follows frame 0; frames must increase",
    ),
    (
        "input-out-of-order",
        5,
        "input: input `JR` is out of order; write it as `RJ`",
    ),
    ("input-unknown", 5, "input: unknown button `X`"),
    (
        "input-empty",
        5,
        "input: empty input; write `.` when nothing is held",
    ),
    (
        "exponent-decimal",
        5,
        "x: `1e3` is not a plain decimal number",
    ),
    (
        "trailing-dot-decimal",
        5,
        "vx: `5.` is not a plain decimal number",
    ),
    ("unknown-state", 5, "unknown state `sprint`"),
    ("bad-ground", 5, "ground must be `0` or `1`, found `2`"),
    ("bad-facing", 5, "facing must be `L` or `R`, found `U`"),
    ("unknown-event", 5, "unknown event `takeoff`"),
    ("repeated-event", 5, "event `land` listed twice"),
    ("empty-event", 5, "unknown event ``"),
];

#[test]
fn rejects_malformed_files_with_a_useful_error() {
    let mut problems = Vec::new();
    for &(name, line, message) in INVALID {
        let path = fixture(&format!("invalid/{name}.trace"));
        match Trace::load(&path) {
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
fn missing_file_is_an_error_not_a_panic() {
    let e = Trace::load(&fixture("valid/does-not-exist.trace")).unwrap_err();
    assert_eq!(e.line, 0);
    assert!(e.message.starts_with("can't read file"));
}
