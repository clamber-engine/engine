//! `clamber-trace`: record engine traces, compare traces and summarise
//! conformance.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clamber_conformance::PRESET;
use clamber_trace::report::{self, Status};
use clamber_trace::{Scenario, Trace, compare, title_mismatch};

const USAGE: &str = "\
usage:
  clamber-trace record <scenario.scn> [-o <out.trace>] [--level <file.level>]
                       [--preset <id>]
  clamber-trace diff <reference.trace> <candidate.trace> --scenario <scenario.scn>
  clamber-trace report [--spec <dir>] [--candidates <dir>]
  clamber-trace check [--root <dir>] [--json <file>] [--csv <file>] [--threads <n>]

record  runs the engine headlessly with the scenario's inputs and writes
        its trace (default: to stdout). The level defaults to
        content/test/<id>.level and the preset to `default`
diff    compares two traces on the scenario's [metrics]; exits 1 if any fail,
        or 3 without comparing if the traces' `# title:` lines differ
report  compares each candidate trace (default tests/golden) against the
        spec's reference traces (default spec) and prints passing metrics
        per scenario
check   runs every spec scenario that has a reference trace, for each title,
        in parallel and headless, against the ratchet in
        tests/conformance-status.toml; exits 1 on any regression or
        unexpected pass. --root is the repository (default .); --json and
        --csv write the per-metric results (first diverging frame, quantity,
        expected and actual)
";

enum Failure {
    /// Bad command line: print usage.
    Usage(String),
    /// Couldn't do the work, such as an unreadable or malformed file.
    Error(String),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("record") => record(&args[1..]),
        Some("diff") => diff(&args[1..]),
        Some("report") => report(&args[1..]),
        Some("check") => check(&args[1..]),
        Some("help" | "-h" | "--help") => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        Some(other) => Err(Failure::Usage(format!("unknown command `{other}`"))),
        None => Err(Failure::Usage("missing command".into())),
    };
    match result {
        Ok(code) => code,
        Err(Failure::Usage(message)) => {
            eprintln!("clamber-trace: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(message)) => {
            eprintln!("clamber-trace: {message}");
            ExitCode::from(2)
        }
    }
}

/// Splits `args` into positionals and the values of the given `--options`.
/// `-o` is short for `--out`.
fn parse_args<'a>(
    args: &'a [String],
    options: &[&str],
) -> Result<(Vec<&'a str>, Vec<Option<&'a str>>), Failure> {
    let mut positional = Vec::new();
    let mut values = vec![None; options.len()];
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let arg = if arg == "-o" { "--out" } else { arg.as_str() };
        let Some(name) = arg.strip_prefix("--") else {
            positional.push(arg);
            continue;
        };
        let (name, inline) = match name.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (name, None),
        };
        let i = options
            .iter()
            .position(|o| *o == name)
            .ok_or_else(|| Failure::Usage(format!("unknown option `--{name}`")))?;
        let value = match inline {
            Some(v) => v,
            None => iter
                .next()
                .ok_or_else(|| Failure::Usage(format!("`--{name}` needs a value")))?,
        };
        values[i] = Some(value);
    }
    Ok((positional, values))
}

fn record(args: &[String]) -> Result<ExitCode, Failure> {
    let (positional, values) = parse_args(args, &["out", "level", "preset"])?;
    let [scenario] = positional[..] else {
        return Err(Failure::Usage("record takes one scenario".into()));
    };
    let scenario =
        Scenario::load(Path::new(scenario)).map_err(|e| Failure::Error(e.to_string()))?;
    let level = match values[1] {
        Some(level) => PathBuf::from(level),
        None => clamber_conformance::test_level(Path::new(""), &scenario.id),
    };
    let preset = values[2].unwrap_or(PRESET);
    let trace = clamber_conformance::record(&scenario, &level, preset).map_err(Failure::Error)?;
    match values[0] {
        Some(out) => std::fs::write(out, trace.to_string())
            .map_err(|e| Failure::Error(format!("can't write {out}: {e}")))?,
        None => print!("{trace}"),
    }
    Ok(ExitCode::SUCCESS)
}

fn diff(args: &[String]) -> Result<ExitCode, Failure> {
    let (positional, values) = parse_args(args, &["scenario"])?;
    let [reference, candidate] = positional[..] else {
        return Err(Failure::Usage(
            "diff takes a reference trace and a candidate trace".into(),
        ));
    };
    let scenario =
        values[0].ok_or_else(|| Failure::Usage("diff needs `--scenario <scenario.scn>`".into()))?;

    let error = |e: clamber_trace::ParseError| Failure::Error(e.to_string());
    let scenario = Scenario::load(Path::new(scenario)).map_err(error)?;
    let reference = Trace::load(Path::new(reference)).map_err(error)?;
    let candidate = Trace::load(Path::new(candidate)).map_err(error)?;

    for (name, trace) in [("reference", &reference), ("candidate", &candidate)] {
        if trace.scenario != scenario.id {
            return Err(Failure::Error(format!(
                "the {name} trace is for scenario `{}`, not `{}`",
                trace.scenario, scenario.id
            )));
        }
        warn_on_script_mismatch(name, trace, &scenario);
    }

    if scenario.metrics.is_empty() {
        println!("{}: no [metrics]; nothing to compare", scenario.id);
        return Ok(ExitCode::SUCCESS);
    }
    if let Some((r, c)) = title_mismatch(&reference, &candidate) {
        println!(
            "{}: title mismatch (reference `{r}`, candidate `{c}`); not compared",
            scenario.id
        );
        return Ok(ExitCode::from(3));
    }
    let outcomes = compare(&scenario, &reference, &candidate);
    for outcome in &outcomes {
        println!("{outcome}");
    }
    let passed = outcomes.iter().filter(|o| o.pass).count();
    println!("{}: {passed}/{} passed", scenario.id, outcomes.len());
    Ok(if passed == outcomes.len() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Traces should hold exactly the scenario's inputs for exactly its frame
/// count. A mismatch usually means the trace came from a different
/// version of the scenario; the metrics are still computed.
fn warn_on_script_mismatch(name: &str, trace: &Trace, scenario: &Scenario) {
    if trace.rows.len() != scenario.frames as usize {
        eprintln!(
            "warning: the {name} trace has {} frames; the scenario records {}",
            trace.rows.len(),
            scenario.frames
        );
    }
    if let Some(row) = trace
        .rows
        .iter()
        .find(|r| r.input != scenario.held(r.frame))
    {
        eprintln!(
            "warning: the {name} trace holds `{}` on frame {}; the scenario holds `{}`",
            row.input,
            row.frame,
            scenario.held(row.frame)
        );
    }
}

fn report(args: &[String]) -> Result<ExitCode, Failure> {
    let (positional, values) = parse_args(args, &["spec", "candidates"])?;
    if let Some(extra) = positional.first() {
        return Err(Failure::Usage(format!("unexpected argument `{extra}`")));
    }
    let spec = PathBuf::from(values[0].unwrap_or("spec"));
    let candidates = PathBuf::from(values[1].unwrap_or("tests/golden"));

    let entries = report::build(&spec, &candidates).map_err(|e| {
        Failure::Error(format!(
            "can't read {}: {e}",
            spec.join("scenarios").display()
        ))
    })?;

    let width = entries.iter().map(|e| e.id.len()).max().unwrap_or(0);
    let (mut passed, mut total, mut compared, mut invalid) = (0, 0, 0, 0);
    for entry in &entries {
        let summary = match &entry.status {
            Status::NoMetrics => "no metrics".to_string(),
            Status::NoReference => "no reference trace".to_string(),
            Status::NoCandidate => format!("no candidate trace in {}", candidates.display()),
            Status::Invalid(e) => {
                invalid += 1;
                format!("error: {e}")
            }
            Status::TitleMismatch {
                reference,
                candidate,
            } => format!("title mismatch (reference `{reference}`, candidate `{candidate}`)"),
            Status::Compared(outcomes) => {
                compared += 1;
                let ok = outcomes.iter().filter(|o| o.pass).count();
                passed += ok;
                total += outcomes.len();
                let failing: Vec<_> = outcomes
                    .iter()
                    .filter(|o| !o.pass)
                    .map(|o| o.label.as_str())
                    .collect();
                if failing.is_empty() {
                    format!("{ok}/{} passing", outcomes.len())
                } else {
                    format!(
                        "{ok}/{} passing; failing: {}",
                        outcomes.len(),
                        failing.join("; ")
                    )
                }
            }
        };
        println!("{:<width$}  {summary}", entry.id);
    }
    println!(
        "\n{} scenarios, {compared} with traces to compare: {passed}/{total} metrics passing",
        entries.len()
    );
    Ok(if invalid == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn check(args: &[String]) -> Result<ExitCode, Failure> {
    let (positional, values) = parse_args(args, &["root", "json", "csv", "threads"])?;
    if let Some(extra) = positional.first() {
        return Err(Failure::Usage(format!("unexpected argument `{extra}`")));
    }
    let threads = match values[3] {
        Some(n) => n
            .parse()
            .map_err(|_| Failure::Usage(format!("`--threads {n}` isn't a number")))?,
        None => 0,
    };
    let start = std::time::Instant::now();
    let results = clamber_conformance::check::run(Path::new(values[0].unwrap_or(".")), threads)
        .map_err(Failure::Error)?;
    let total = start.elapsed();
    for (path, text) in [
        (values[1], clamber_conformance::check::json(&results)),
        (values[2], clamber_conformance::check::csv(&results)),
    ] {
        if let Some(path) = path {
            std::fs::write(path, text)
                .map_err(|e| Failure::Error(format!("can't write {path}: {e}")))?;
        }
    }
    print!("{}", clamber_conformance::check::summary(&results, total));
    Ok(if results.iter().all(|r| r.verdict.ok()) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
