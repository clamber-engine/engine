//! The batch checker behind `clamber-trace check`: every spec scenario
//! with a reference trace, on every title that has one, run headlessly in
//! parallel and compared metric by metric.
//!
//! Each level is loaded once and shared by every scenario that plays it,
//! and each preset's feel is built once.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clamber_level::Level;
use clamber_pack::Feel;
use clamber_sim::generic_feel;
use clamber_trace::compare::{CurveDetail, Detail};
use clamber_trace::{Outcome, Scenario, Trace, compare, title_mismatch};

use crate::{CONFORMANCE_TITLES, STATUS_FILE, Status, spec_cases, title_level};

/// What came of one scenario on one title.
#[derive(Debug)]
pub struct Checked {
    /// [`crate::ConformanceTitle::key`]
    pub key: String,
    pub title: &'static str,
    pub verdict: Verdict,
    /// Time spent recording the engine's trace and comparing.
    pub elapsed: Duration,
}

#[derive(Debug)]
pub enum Verdict {
    /// Every metric is as `tests/conformance-status.toml` expects.
    Pass(Vec<Outcome>),
    /// Metrics that regressed or now pass though listed.
    Fail {
        problems: Vec<String>,
        outcomes: Vec<Outcome>,
    },
    /// The scenario couldn't be run or compared.
    Error(String),
}

impl Verdict {
    pub fn ok(&self) -> bool {
        matches!(self, Verdict::Pass(_))
    }

    fn name(&self) -> &'static str {
        match self {
            Verdict::Pass(_) => "pass",
            Verdict::Fail { .. } => "fail",
            Verdict::Error(_) => "error",
        }
    }
}

/// A scenario to run: its reference trace and level are known to exist.
struct Job {
    key: String,
    title: &'static str,
    preset: &'static str,
    scenario: Scenario,
    reference: PathBuf,
    level: PathBuf,
}

/// Runs every spec scenario that has metrics and a reference trace for a
/// title, on `threads` threads (0 for one per core), against the repository
/// at `root`. Results are sorted by key.
pub fn run(root: &Path, threads: usize) -> Result<Vec<Checked>, String> {
    let status = Status::load(&root.join(STATUS_FILE))?;
    let mut jobs = Vec::new();
    for case in spec_cases(root)? {
        if case.scenario.metrics.is_empty() {
            continue;
        }
        for title in CONFORMANCE_TITLES {
            let reference = title.reference(root, &case.scenario.id);
            if reference.exists() {
                jobs.push(Job {
                    key: title.key(&case.scenario.id),
                    title: title.title,
                    preset: title.preset,
                    level: title_level(root, &case.scenario.id, title.title),
                    scenario: case.scenario.clone(),
                    reference,
                });
            }
        }
    }

    let mut levels: HashMap<PathBuf, Result<Arc<Level>, String>> = HashMap::new();
    for job in &jobs {
        levels.entry(job.level.clone()).or_insert_with(|| {
            Level::load(&job.level)
                .map(Arc::new)
                .map_err(|e| format!("{}: {e}", job.level.display()))
        });
    }
    let mut feels: HashMap<&str, Option<Arc<Feel>>> = HashMap::new();
    for job in &jobs {
        feels
            .entry(job.preset)
            .or_insert_with(|| generic_feel(job.preset).map(Arc::new));
    }

    let threads = if threads == 0 {
        std::thread::available_parallelism().map_or(1, |n| n.get())
    } else {
        threads
    }
    .min(jobs.len().max(1));
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(jobs.len()));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while let Some(job) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let start = Instant::now();
                    let verdict = match (&levels[&job.level], &feels[job.preset]) {
                        (Err(e), _) => Verdict::Error(e.clone()),
                        (_, None) => Verdict::Error(format!("no preset named `{}`", job.preset)),
                        (Ok(level), Some(feel)) => run_one(job, level, feel, &status),
                    };
                    let checked = Checked {
                        key: job.key.clone(),
                        title: job.title,
                        verdict,
                        elapsed: start.elapsed(),
                    };
                    results.lock().unwrap().push(checked);
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| a.key.cmp(&b.key));

    // Entries that check nothing would let regressions hide.
    for id in status.expected_failures.keys() {
        if !results.iter().any(|r| &r.key == id) {
            results.push(Checked {
                key: id.clone(),
                title: "",
                verdict: Verdict::Error(format!(
                    "{STATUS_FILE} lists `{id}`, which has no reference trace or no spec scenario"
                )),
                elapsed: Duration::ZERO,
            });
        }
    }
    Ok(results)
}

fn run_one(job: &Job, level: &Arc<Level>, feel: &Arc<Feel>, status: &Status) -> Verdict {
    let reference = match Trace::load(&job.reference) {
        Ok(t) => t,
        Err(e) => return Verdict::Error(e.to_string()),
    };
    let candidate = match clamber_sim::record(level.clone(), feel.clone(), &job.scenario) {
        Ok(t) => t,
        Err(e) => return Verdict::Error(e.to_string()),
    };
    if let Some((r, c)) = title_mismatch(&reference, &candidate) {
        return Verdict::Error(format!(
            "the reference trace is `{r}` but `{}` holds `{c}` values",
            job.preset
        ));
    }
    let outcomes = compare(&job.scenario, &reference, &candidate);
    let problems = status.check(&job.key, &outcomes);
    if problems.is_empty() {
        Verdict::Pass(outcomes)
    } else {
        Verdict::Fail { problems, outcomes }
    }
}

/// One line of JSON per scenario: its key, title, verdict, time and every
/// metric with the first diverging frame, quantity and expected and actual
/// values where there are any.
pub fn json(results: &[Checked]) -> String {
    let mut out = String::new();
    for r in results {
        let _ = write!(
            out,
            "{{\"scenario\":{},\"title\":{},\"verdict\":\"{}\",\"micros\":{}",
            quote(&r.key),
            quote(r.title),
            r.verdict.name(),
            r.elapsed.as_micros()
        );
        match &r.verdict {
            Verdict::Error(e) => {
                let _ = write!(out, ",\"error\":{}", quote(e));
            }
            Verdict::Pass(outcomes) | Verdict::Fail { outcomes, .. } => {
                out.push_str(",\"metrics\":[");
                for (i, o) in outcomes.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let d = divergence(o);
                    let _ = write!(
                        out,
                        "{{\"metric\":{},\"pass\":{},\"frame\":{},\"quantity\":{},\"expected\":{},\"actual\":{}}}",
                        quote(&o.label),
                        o.pass,
                        d.frame.map_or("null".into(), |f| f.to_string()),
                        d.quantity.as_deref().map_or("null".into(), quote),
                        d.expected.as_deref().map_or("null".into(), quote),
                        d.actual.as_deref().map_or("null".into(), quote),
                    );
                }
                out.push(']');
            }
        }
        out.push_str("}\n");
    }
    out
}

/// One CSV row per failing metric or error:
/// `scenario,title,metric,frame,quantity,expected,actual`.
pub fn csv(results: &[Checked]) -> String {
    let mut out = String::from("scenario,title,metric,frame,quantity,expected,actual\n");
    for r in results {
        match &r.verdict {
            Verdict::Error(e) => {
                let _ = writeln!(out, "{},{},{},,,,", cell(&r.key), cell(r.title), cell(e));
            }
            Verdict::Pass(outcomes) | Verdict::Fail { outcomes, .. } => {
                for o in outcomes.iter().filter(|o| !o.pass) {
                    let d = divergence(o);
                    let _ = writeln!(
                        out,
                        "{},{},{},{},{},{},{}",
                        cell(&r.key),
                        cell(r.title),
                        cell(&o.label),
                        d.frame.map_or(String::new(), |f| f.to_string()),
                        cell(d.quantity.as_deref().unwrap_or("")),
                        cell(d.expected.as_deref().unwrap_or("")),
                        cell(d.actual.as_deref().unwrap_or("")),
                    );
                }
            }
        }
    }
    out
}

/// A short summary per title and a line per problem.
pub fn summary(results: &[Checked], total: Duration) -> String {
    let mut out = String::new();
    for r in results {
        match &r.verdict {
            Verdict::Pass(_) => {}
            Verdict::Fail { problems, .. } => {
                for p in problems {
                    let _ = writeln!(out, "FAIL  {}: {p}", r.key);
                }
            }
            Verdict::Error(e) => {
                let _ = writeln!(out, "ERROR {}: {e}", r.key);
            }
        }
    }
    for title in CONFORMANCE_TITLES {
        let of: Vec<_> = results.iter().filter(|r| r.title == title.title).collect();
        let passed = of.iter().filter(|r| r.verdict.ok()).count();
        let _ = writeln!(out, "{}: {passed}/{} scenarios ok", title.title, of.len());
    }
    let _ = writeln!(
        out,
        "{} scenarios in {:.1} ms",
        results.len(),
        total.as_secs_f64() * 1000.0
    );
    out
}

#[derive(Default)]
struct Divergence {
    frame: Option<u32>,
    quantity: Option<String>,
    expected: Option<String>,
    actual: Option<String>,
}

fn divergence(outcome: &Outcome) -> Divergence {
    match &outcome.detail {
        Detail::Scalar {
            reference,
            candidate,
            ..
        } => Divergence {
            frame: None,
            quantity: Some(outcome.label.clone()),
            expected: reference.map(|d| d.to_string()),
            actual: candidate.map(|d| d.to_string()),
        },
        Detail::Curve(CurveDetail::Diverged(d)) => Divergence {
            frame: Some(d.candidate_frame),
            quantity: Some(d.field.to_string()),
            expected: Some(d.reference.to_string()),
            actual: Some(d.candidate.to_string()),
        },
        Detail::Curve(_) => Divergence::default(),
    }
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote("a\"b\\\n"), "\"a\\\"b\\\\\\n\"");
        assert_eq!(cell("a,b"), "\"a,b\"");
        assert_eq!(cell("plain"), "plain");
    }
}
