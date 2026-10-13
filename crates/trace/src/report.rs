//! Conformance summary across every scenario in the spec.
//!
//! The candidate for each scenario is its committed golden trace
//! (`tests/golden/<id>.trace`), which the golden test keeps
//! byte-identical to what the engine records.

use std::io;
use std::path::{Path, PathBuf};

use crate::{Outcome, Scenario, Trace, compare, title_mismatch};

#[derive(Debug)]
pub struct Entry {
    pub id: String,
    pub status: Status,
}

#[derive(Debug)]
pub enum Status {
    /// The scenario has no `[metrics]` section.
    NoMetrics,
    /// No `spec/traces/reference/<id>.trace` yet.
    NoReference,
    /// No candidate trace for this scenario.
    NoCandidate,
    /// A file failed to parse or doesn't belong to the scenario.
    Invalid(String),
    /// The traces represent different titles, so they aren't compared.
    TitleMismatch {
        reference: String,
        candidate: String,
    },
    Compared(Vec<Outcome>),
}

/// Builds the summary. `spec` is the spec checkout; `candidates` holds
/// `<id>.trace` files to compare against the references.
pub fn build(spec: &Path, candidates: &Path) -> io::Result<Vec<Entry>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(spec.join("scenarios"))?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "scn"));
    paths.sort();

    Ok(paths
        .iter()
        .map(|path| {
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let status = status(path, &id, spec, candidates);
            Entry { id, status }
        })
        .collect())
}

fn status(path: &Path, id: &str, spec: &Path, candidates: &Path) -> Status {
    let scenario = match Scenario::load(path) {
        Ok(s) => s,
        Err(e) => return Status::Invalid(e.to_string()),
    };
    if scenario.metrics.is_empty() {
        return Status::NoMetrics;
    }
    let reference = spec.join("traces/reference").join(format!("{id}.trace"));
    if !reference.exists() {
        return Status::NoReference;
    }
    let candidate = candidates.join(format!("{id}.trace"));
    if !candidate.exists() {
        return Status::NoCandidate;
    }
    let load = |path: &Path| -> Result<Trace, String> {
        let trace = Trace::load(path).map_err(|e| e.to_string())?;
        if trace.scenario != id {
            return Err(format!(
                "{}: trace is for scenario `{}`, not `{id}`",
                path.display(),
                trace.scenario
            ));
        }
        Ok(trace)
    };
    match (load(&reference), load(&candidate)) {
        (Ok(r), Ok(c)) => match title_mismatch(&r, &c) {
            Some((reference, candidate)) => Status::TitleMismatch {
                reference: reference.to_string(),
                candidate: candidate.to_string(),
            },
            None => Status::Compared(compare(&scenario, &r, &c)),
        },
        (Err(e), _) | (_, Err(e)) => Status::Invalid(e),
    }
}
