//! Comparing a candidate trace against a reference trace, metric by
//! metric, as `spec/traces/METRICS.md` defines.

use std::fmt;

use crate::metrics::{self, Comparison, Curve, Field};
use crate::{Decimal, Event, Scenario, Trace};

/// The result of one `[metrics]` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// See [`Comparison::label`].
    pub label: String,
    pub pass: bool,
    pub detail: Detail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    /// `None` means the metric is missing from that trace.
    Scalar {
        reference: Option<Decimal>,
        candidate: Option<Decimal>,
        tol: Decimal,
    },
    Curve(CurveDetail),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CurveDetail {
    /// Every field stayed within tolerance on every frame.
    Matched {
        frames: usize,
        tol: Decimal,
    },
    /// The aligning event never occurs in one or both traces.
    EventMissing {
        event: Event,
        reference: bool,
        candidate: bool,
    },
    /// A trace ends before the curve does. Counts are rows available from
    /// the aligned frame on.
    TooShort {
        needed: usize,
        reference: usize,
        candidate: usize,
    },
    Diverged(Divergence),
}

/// The first frame of a curve where a field is outside tolerance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// Rows after the aligned frame (0 is the aligned frame itself).
    pub offset: usize,
    pub reference_frame: u32,
    pub candidate_frame: u32,
    pub field: Field,
    /// Compared values; relative to the aligned frame for positions.
    pub reference: Decimal,
    pub candidate: Decimal,
    pub tol: Decimal,
}

/// Both traces' titles, if both have one and they differ. Such traces
/// represent different titles and aren't compared: that is neither a pass
/// nor a fail.
pub fn title_mismatch<'a>(
    reference: &'a Trace,
    candidate: &'a Trace,
) -> Option<(&'a str, &'a str)> {
    match (reference.title(), candidate.title()) {
        (Some(r), Some(c)) if r != c => Some((r, c)),
        _ => None,
    }
}

/// Runs every comparison in the scenario's `[metrics]` section, in order.
pub fn compare(scenario: &Scenario, reference: &Trace, candidate: &Trace) -> Vec<Outcome> {
    scenario
        .metrics
        .iter()
        .map(|c| compare_one(c, reference, candidate))
        .collect()
}

pub fn compare_one(comparison: &Comparison, reference: &Trace, candidate: &Trace) -> Outcome {
    let label = comparison.label();
    match comparison {
        Comparison::Scalar { metric, tol } => {
            let r = metrics::compute(*metric, reference);
            let c = metrics::compute(*metric, candidate);
            let pass = match (r, c) {
                (Some(r), Some(c)) => c.within(r, *tol),
                // A missing metric fails unless both traces are missing it.
                (None, None) => true,
                _ => false,
            };
            Outcome {
                label,
                pass,
                detail: Detail::Scalar {
                    reference: r,
                    candidate: c,
                    tol: *tol,
                },
            }
        }
        Comparison::Curve(curve) => {
            let detail = compare_curve(curve, reference, candidate);
            let pass = match detail {
                CurveDetail::Matched { .. } => true,
                CurveDetail::EventMissing {
                    reference,
                    candidate,
                    ..
                } => reference && candidate,
                _ => false,
            };
            Outcome {
                label,
                pass,
                detail: Detail::Curve(detail),
            }
        }
    }
}

fn compare_curve(curve: &Curve, reference: &Trace, candidate: &Trace) -> CurveDetail {
    let (r_start, c_start) = match (
        metrics::first(&reference.rows, curve.event),
        metrics::first(&candidate.rows, curve.event),
    ) {
        (Some(r), Some(c)) => (r, c),
        (r, c) => {
            return CurveDetail::EventMissing {
                event: curve.event,
                reference: r.is_none(),
                candidate: c.is_none(),
            };
        }
    };

    let r_rows = &reference.rows[r_start..];
    let c_rows = &candidate.rows[c_start..];
    if r_rows.len() < curve.frames || c_rows.len() < curve.frames {
        return CurveDetail::TooShort {
            needed: curve.frames,
            reference: r_rows.len(),
            candidate: c_rows.len(),
        };
    }

    for offset in 0..curve.frames {
        let (r, c) = (&r_rows[offset], &c_rows[offset]);
        for &field in &curve.fields {
            let (mut rv, mut cv) = (field.get(r), field.get(c));
            if field.is_position() {
                rv = rv - field.get(&r_rows[0]);
                cv = cv - field.get(&c_rows[0]);
            }
            if !cv.within(rv, curve.tol) {
                return CurveDetail::Diverged(Divergence {
                    offset,
                    reference_frame: r.frame,
                    candidate_frame: c.frame,
                    field,
                    reference: rv,
                    candidate: cv,
                    tol: curve.tol,
                });
            }
        }
    }
    CurveDetail::Matched {
        frames: curve.frames,
        tol: curve.tol,
    }
}

fn or_missing(value: Option<Decimal>) -> String {
    value.map_or_else(|| "missing".into(), |v| v.to_string())
}

impl fmt::Display for Outcome {
    /// One line: verdict, label, and what was compared.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verdict = if self.pass { "PASS" } else { "FAIL" };
        write!(f, "{verdict}  {:<32} {}", self.label, self.detail)
    }
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Detail::Scalar {
                reference,
                candidate,
                tol,
            } => write!(
                f,
                "reference {}, candidate {} (tol {tol})",
                or_missing(*reference),
                or_missing(*candidate)
            ),
            Detail::Curve(CurveDetail::Matched { frames, tol }) => {
                write!(f, "{frames} frames within {tol}")
            }
            Detail::Curve(CurveDetail::EventMissing {
                event,
                reference,
                candidate,
            }) => {
                let place = match (reference, candidate) {
                    (true, true) => "both traces",
                    (true, false) => "the reference",
                    _ => "the candidate",
                };
                write!(f, "no `{event}` in {place}")
            }
            Detail::Curve(CurveDetail::TooShort {
                needed,
                reference,
                candidate,
            }) => write!(
                f,
                "needs {needed} frames from the event; reference has {reference}, candidate has {candidate}"
            ),
            Detail::Curve(CurveDetail::Diverged(d)) => write!(
                f,
                "first divergence at +{} (reference frame {}, candidate frame {}): {} reference {}, candidate {} (tol {})",
                d.offset,
                d.reference_frame,
                d.candidate_frame,
                d.field,
                d.reference,
                d.candidate,
                d.tol
            ),
        }
    }
}
