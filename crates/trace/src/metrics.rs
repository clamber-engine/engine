//! Metrics from `spec/traces/METRICS.md`: what a scenario's `[metrics]`
//! section asks for, and how each is computed from a trace.
//!
//! "Frame" in the definitions means a trace row, so every count here
//! counts rows, not frame numbers.

use std::fmt;

use crate::trace::Row;
use crate::{Decimal, Event, Trace};

names! {
    /// A single-value metric.
    pub enum Metric {
        ApexHeight => "apex_height",
        Airtime => "airtime",
        TopSpeed => "top_speed",
        FramesToTop => "frames_to_top",
        RollDistance => "roll_distance",
        CoyoteFrames => "coyote_frames",
        BounceHeight => "bounce_height",
        FinalX => "final_x",
        FinalY => "final_y",
    }
}

names! {
    /// A numeric trace column that curves can compare.
    pub enum Field {
        X => "x",
        Y => "y",
        Vx => "vx",
        Vy => "vy",
    }
}

impl Field {
    /// Positions are compared relative to the aligned frame; velocities
    /// are compared directly.
    pub fn is_position(self) -> bool {
        matches!(self, Field::X | Field::Y)
    }

    pub fn get(self, row: &Row) -> Decimal {
        match self {
            Field::X => row.x,
            Field::Y => row.y,
            Field::Vx => row.vx,
            Field::Vy => row.vy,
        }
    }
}

/// `curve FIELDS from EVENT for N tol T`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Curve {
    pub fields: Vec<Field>,
    pub event: Event,
    pub frames: usize,
    pub tol: Decimal,
}

/// One line of a scenario's `[metrics]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// `NAME tol T`
    Scalar {
        metric: Metric,
        tol: Decimal,
    },
    Curve(Curve),
}

impl Comparison {
    /// The comparison's name in reports and in
    /// `tests/conformance-status.toml`, such as `airtime` or
    /// `curve y,vy from jump_start`.
    pub fn label(&self) -> String {
        match self {
            Comparison::Scalar { metric, .. } => metric.to_string(),
            Comparison::Curve(c) => {
                let fields: Vec<_> = c.fields.iter().map(|f| f.name()).collect();
                format!("curve {} from {}", fields.join(","), c.event)
            }
        }
    }
}

impl fmt::Display for Comparison {
    /// The comparison as written in a scenario file.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Comparison::Scalar { metric, tol } => write!(f, "{metric} tol {tol}"),
            Comparison::Curve(c) => write!(f, "{} for {} tol {}", self.label(), c.frames, c.tol),
        }
    }
}

/// Computes `metric` for `trace`, or `None` if it's missing because its
/// defining event never occurs.
pub fn compute(metric: Metric, trace: &Trace) -> Option<Decimal> {
    let rows = &trace.rows;
    match metric {
        Metric::ApexHeight => height_until_land(rows, Event::JumpStart),
        Metric::Airtime => {
            let (start, land) = span(rows, Event::JumpStart, Event::Land)?;
            Some(Decimal::from_count(land - start))
        }
        Metric::TopSpeed => top_speed(rows),
        Metric::FramesToTop => {
            let top = top_speed(rows)?;
            let moving = rows.iter().position(|r| !r.vx.is_zero())?;
            let at_top = rows.iter().position(|r| r.vx.abs() == top)?;
            Some(Decimal::from_count(at_top - moving))
        }
        Metric::RollDistance => {
            let (start, end) = span(rows, Event::RollStart, Event::RollEnd)?;
            Some(rows[end].x - rows[start].x)
        }
        Metric::CoyoteFrames => {
            let left = first(rows, Event::LeaveGround)?;
            for (i, row) in rows.iter().enumerate().skip(left + 1) {
                if row.has(Event::JumpStart) {
                    return Some(Decimal::from_count(i - left));
                }
                if row.has(Event::Land) {
                    return None;
                }
            }
            None
        }
        Metric::BounceHeight => height_until_land(rows, Event::Bounce),
        Metric::FinalX => rows.last().map(|r| r.x),
        Metric::FinalY => rows.last().map(|r| r.y),
    }
}

/// Index of the first row carrying `event`.
pub fn first(rows: &[Row], event: Event) -> Option<usize> {
    rows.iter().position(|r| r.has(event))
}

/// The first `start` and the next `end` strictly after it.
fn span(rows: &[Row], start: Event, end: Event) -> Option<(usize, usize)> {
    let i = first(rows, start)?;
    let j = rows[i + 1..].iter().position(|r| r.has(end))?;
    Some((i, i + 1 + j))
}

/// Largest `-y` from the first `event` to the next `land`, minus `-y` on
/// the `event` frame.
fn height_until_land(rows: &[Row], event: Event) -> Option<Decimal> {
    let (start, land) = span(rows, event, Event::Land)?;
    let peak = rows[start..=land].iter().map(|r| -r.y).max()?;
    Some(peak - -rows[start].y)
}

fn top_speed(rows: &[Row]) -> Option<Decimal> {
    rows.iter().map(|r| r.vx.abs()).max()
}
