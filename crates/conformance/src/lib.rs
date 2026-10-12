//! Engine traces checked against themselves and against the spec
//! (TESTING.md layers 4 and 5), and the `clamber-trace` CLI.
//!
//! Every scenario is recorded on the [`PRESET`] preset with its level:
//!
//! - spec scenarios (`spec/scenarios/<id>.scn`) play
//!   `content/test/<id>.level`, built to satisfy their `requires:` line;
//! - playthroughs (`tests/playthrough/<id>.scn`) name their level in
//!   `requires:`, relative to the repository root.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_level::Level;
use clamber_sim::generic_feel;
use clamber_trace::{Outcome, Scenario, Trace};
use serde::Deserialize;

/// The preset traces are recorded on. Reference traces will be compared
/// against it until the Classic presets have measured values.
pub const PRESET: &str = "default";

/// Expected conformance failures, relative to the repository root.
pub const STATUS_FILE: &str = "tests/conformance-status.toml";

/// Golden traces, relative to the repository root.
pub const GOLDEN_DIR: &str = "tests/golden";

/// A scenario and the level it plays.
#[derive(Clone, Debug)]
pub struct Case {
    pub scenario: Scenario,
    pub level: PathBuf,
}

/// The repository root, from this crate's manifest directory.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The test level for a spec scenario.
pub fn test_level(root: &Path, id: &str) -> PathBuf {
    root.join("content/test").join(format!("{id}.level"))
}

/// Every spec scenario with its test level, sorted by id.
pub fn spec_cases(root: &Path) -> Result<Vec<Case>, String> {
    scenarios_in(&root.join("spec/scenarios"))?
        .into_iter()
        .map(|scenario| {
            let level = test_level(root, &scenario.id);
            Ok(Case { scenario, level })
        })
        .collect()
}

/// Every playthrough with the level it names, sorted by id.
pub fn playthrough_cases(root: &Path) -> Result<Vec<Case>, String> {
    scenarios_in(&root.join("tests/playthrough"))?
        .into_iter()
        .map(|scenario| {
            let level = root.join(&scenario.requires);
            Ok(Case { scenario, level })
        })
        .collect()
}

fn scenarios_in(dir: &Path) -> Result<Vec<Scenario>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .and_then(|entries| entries.map(|e| e.map(|e| e.path())).collect())
        .map_err(|e: io::Error| format!("{}: {e}", dir.display()))?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "scn"));
    paths.sort();
    paths
        .iter()
        .map(|p| Scenario::load(p).map_err(|e| e.to_string()))
        .collect()
}

/// Records the scenario on `preset` in `level`.
pub fn record(scenario: &Scenario, level: &Path, preset: &str) -> Result<Trace, String> {
    let level = Level::load(level).map_err(|e| e.to_string())?;
    let feel = generic_feel(preset).ok_or_else(|| format!("no preset named `{preset}`"))?;
    clamber_sim::record(Arc::new(level), Arc::new(feel), scenario).map_err(|e| e.to_string())
}

/// `tests/conformance-status.toml`: per scenario, the labels of metrics
/// expected to fail today.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub expected_failures: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    expected_failures: Vec<String>,
}

impl Status {
    pub fn parse(text: &str, origin: &str) -> Result<Status, String> {
        let entries: BTreeMap<String, Entry> =
            toml::from_str(text).map_err(|e| format!("{origin}: {e}"))?;
        Ok(Status {
            expected_failures: entries
                .into_iter()
                .map(|(id, e)| (id, e.expected_failures))
                .collect(),
        })
    }

    pub fn load(path: &Path) -> Result<Status, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Status::parse(&text, &path.display().to_string())
    }

    /// Problems with a scenario's outcomes under the ratchet: failures not
    /// listed, and listed failures that pass now or don't name a metric of
    /// the scenario. Empty means the scenario is as expected.
    pub fn check(&self, id: &str, outcomes: &[Outcome]) -> Vec<String> {
        let expected = self
            .expected_failures
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut problems = Vec::new();
        for outcome in outcomes {
            let listed = expected.contains(&outcome.label);
            match (outcome.pass, listed) {
                (false, false) => problems.push(format!("regression: {outcome}")),
                (true, true) => problems.push(format!(
                    "`{}` passes now; remove it from {STATUS_FILE}",
                    outcome.label
                )),
                _ => {}
            }
        }
        for label in expected {
            if !outcomes.iter().any(|o| &o.label == label) {
                problems.push(format!(
                    "{STATUS_FILE} lists `{label}`, which isn't a metric of `{id}`"
                ));
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clamber_trace::Decimal;
    use clamber_trace::compare::Detail;

    fn outcome(label: &str, pass: bool) -> Outcome {
        Outcome {
            label: label.into(),
            pass,
            detail: Detail::Scalar {
                reference: Some(Decimal::ZERO),
                candidate: Some(Decimal::ZERO),
                tol: Decimal::ZERO,
            },
        }
    }

    #[test]
    fn ratchet() {
        let status = Status::parse(
            "[hop]\nexpected_failures = [\"airtime\", \"curve y from jump_start\"]\n",
            "test",
        )
        .unwrap();
        let ok = [
            outcome("airtime", false),
            outcome("curve y from jump_start", false),
            outcome("final_x", true),
        ];
        assert!(status.check("hop", &ok).is_empty());

        let regressed = [outcome("airtime", false), outcome("final_x", false)];
        let problems = status.check("hop", &regressed);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].starts_with("regression: "));
        assert!(problems[1].contains("lists `curve y from jump_start`"));

        let improved = [
            outcome("airtime", true),
            outcome("curve y from jump_start", false),
        ];
        assert_eq!(
            status.check("hop", &improved),
            ["`airtime` passes now; remove it from tests/conformance-status.toml"]
        );

        // Scenarios not listed must pass everything.
        assert!(
            status
                .check("other", &[outcome("final_x", true)])
                .is_empty()
        );
        assert_eq!(status.check("other", &[outcome("final_x", false)]).len(), 1);
    }

    #[test]
    fn status_rejects_typos() {
        assert!(Status::parse("[hop]\nexpected_failure = []\n", "t").is_err());
        assert!(Status::parse("", "t").unwrap().expected_failures.is_empty());
    }
}
