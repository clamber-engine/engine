//! The `clamber-trace` binary end to end.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SCENARIO: &str = "\
# clamber-scenario v1
id: hop
summary: Synthetic CLI scenario.
requires: anything
start: anything
frames: 4

[inputs]
0-3 R
1   J

[metrics]
final_x tol 0.5
curve y from jump_start for 2 tol 0
";

fn trace(source: &str, final_x: &str, peak: &str) -> String {
    format!(
        "# clamber-trace v1\n# scenario: hop\n# source: {source}\n\
         frame\tinput\tx\ty\tvx\tvy\tstate\tground\tfacing\tevents\n\
         0\tR\t0\t0\t1\t0\twalk\t1\tR\n\
         1\tRJ\t1\t-2\t1\t-2\tjump\t0\tR\tjump_start\n\
         2\tR\t2\t{peak}\t1\t-1\tjump\t0\tR\n\
         3\tR\t{final_x}\t0\t1\t3\twalk\t1\tR\tland\n"
    )
}

/// A fresh directory with a spec checkout and a candidate directory.
fn workspace(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("spec/scenarios")).unwrap();
    fs::create_dir_all(dir.join("spec/traces/reference")).unwrap();
    fs::create_dir_all(dir.join("golden")).unwrap();
    fs::write(dir.join("spec/scenarios/hop.scn"), SCENARIO).unwrap();
    fs::write(
        dir.join("spec/traces/reference/hop.trace"),
        trace("reference", "3", "-3"),
    )
    .unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clamber-trace"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

const DIFF: &[&str] = &[
    "diff",
    "spec/traces/reference/hop.trace",
    "golden/hop.trace",
    "--scenario",
    "spec/scenarios/hop.scn",
];

#[test]
fn diff_passes() {
    let dir = workspace("diff-passes");
    fs::write(dir.join("golden/hop.trace"), trace("engine", "3.5", "-3")).unwrap();
    let out = run(&dir, DIFF);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        stderr(&out)
    );
    assert!(stdout(&out).contains("PASS  final_x"));
    assert!(stdout(&out).ends_with("hop: 2/2 passed\n"));
    assert_eq!(stderr(&out), "");
}

#[test]
fn diff_fails_with_first_divergence() {
    let dir = workspace("diff-fails");
    fs::write(dir.join("golden/hop.trace"), trace("engine", "3", "-2.5")).unwrap();
    let out = run(&dir, DIFF);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("FAIL  curve y from jump_start"), "{text}");
    assert!(text.contains("first divergence at +1 (reference frame 2, candidate frame 2): y reference -1, candidate -0.5 (tol 0)"), "{text}");
    assert!(text.ends_with("hop: 1/2 passed\n"));
}

#[test]
fn diff_warns_when_trace_does_not_follow_the_script() {
    let dir = workspace("diff-warns");
    let off_script = trace("engine", "3", "-3").replace("1\tRJ\t", "1\tR\t");
    fs::write(dir.join("golden/hop.trace"), off_script).unwrap();
    let out = run(&dir, DIFF);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stderr(&out)
            .contains("warning: the candidate trace holds `R` on frame 1; the scenario holds `RJ`")
    );
}

#[test]
fn diff_rejects_bad_input() {
    let dir = workspace("diff-rejects");
    fs::write(
        dir.join("golden/hop.trace"),
        "# clamber-trace v1\n# scenario: hop\n",
    )
    .unwrap();
    let out = run(&dir, DIFF);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        stderr(&out),
        "clamber-trace: golden/hop.trace: missing the column line\n"
    );

    fs::write(
        dir.join("golden/hop.trace"),
        trace("engine", "3", "-3").replace("scenario: hop", "scenario: other"),
    )
    .unwrap();
    let out = run(&dir, DIFF);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("the candidate trace is for scenario `other`, not `hop`"));

    let out = run(&dir, &["diff", "a.trace", "b.trace"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("diff needs `--scenario <scenario.scn>`"));
    assert!(stderr(&out).contains("usage:"));

    let out = run(&dir, &["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn report_summarises_each_scenario() {
    let dir = workspace("report");
    fs::write(dir.join("golden/hop.trace"), trace("engine", "9", "-3")).unwrap();
    // A scenario with a reference but no candidate, and one with no reference.
    fs::write(
        dir.join("spec/scenarios/no-candidate.scn"),
        SCENARIO.replace("id: hop", "id: no-candidate"),
    )
    .unwrap();
    fs::write(
        dir.join("spec/traces/reference/no-candidate.trace"),
        trace("reference", "3", "-3").replace("scenario: hop", "scenario: no-candidate"),
    )
    .unwrap();
    fs::write(
        dir.join("spec/scenarios/no-reference.scn"),
        SCENARIO.replace("id: hop", "id: no-reference"),
    )
    .unwrap();

    let out = run(&dir, &["report", "--spec", "spec", "--candidates=golden"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "hop           1/2 passing; failing: final_x\n\
         no-candidate  no candidate trace in golden\n\
         no-reference  no reference trace\n\
         \n3 scenarios, 1 with traces to compare: 1/2 metrics passing\n"
    );
}

#[test]
fn report_on_the_pinned_spec() {
    // Runs from the workspace root with the defaults, as
    // `cargo run --bin clamber-trace -- report` does.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = run(&root, &["report"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("run-jump-flat"));
}

#[test]
fn record_matches_the_golden_trace() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = run(&root, &["record", "spec/scenarios/walk-off-ledge.scn"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let golden = fs::read_to_string(root.join("tests/golden/walk-off-ledge.trace")).unwrap();
    assert_eq!(stdout(&out), golden);

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("record");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("out.trace");
    let out = run(
        &root,
        &[
            "record",
            "spec/scenarios/walk-off-ledge.scn",
            "-o",
            file.to_str().unwrap(),
            "--preset",
            "floaty",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("# preset: floaty\n"));
    assert_ne!(text, golden);

    let out = run(
        &root,
        &[
            "record",
            "spec/scenarios/walk-off-ledge.scn",
            "--preset",
            "nope",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(stderr(&out), "clamber-trace: no preset named `nope`\n");
}

#[test]
fn check_runs_the_spec_corpus_and_writes_json_and_csv() {
    let dir = workspace("check");
    let root = clamber_conformance::root();
    let (json, csv) = (dir.join("check.json"), dir.join("check.csv"));
    let out = run(
        &dir,
        &[
            "check",
            "--root",
            root.to_str().unwrap(),
            "--json",
            json.to_str().unwrap(),
            "--csv",
            csv.to_str().unwrap(),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        stderr(&out)
    );
    assert!(stdout(&out).contains("title-1: "), "{}", stdout(&out));
    let json = fs::read_to_string(json).unwrap();
    assert!(json.lines().count() > 0);
    assert!(json.lines().all(|l| l.starts_with("{\"scenario\":")));
    assert!(
        fs::read_to_string(csv)
            .unwrap()
            .starts_with("scenario,title,metric,frame,quantity,expected,actual\n")
    );
}
