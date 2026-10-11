//! Metric definitions (`spec/traces/METRICS.md`) and comparisons, on
//! small synthetic traces.

use clamber_trace::compare::{CurveDetail, Detail, Divergence, compare_one};
use clamber_trace::metrics::compute;
use clamber_trace::{Comparison, Curve, Decimal, Event, Field, Metric, Trace};

/// Builds a trace from space-separated rows:
/// `frame input x y vx vy state ground facing [events]`.
fn trace(rows: &str) -> Trace {
    let mut text = String::from(
        "# clamber-trace v1\n# scenario: t\n# source: engine\n\
         frame\tinput\tx\ty\tvx\tvy\tstate\tground\tfacing\tevents\n",
    );
    for row in rows.lines().map(str::trim).filter(|r| !r.is_empty()) {
        text.push_str(&row.split_whitespace().collect::<Vec<_>>().join("\t"));
        text.push('\n');
    }
    Trace::parse(&text, "inline").unwrap()
}

fn d(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}

fn metric(m: Metric, t: &Trace) -> Option<String> {
    compute(m, t).map(|v| v.to_string())
}

/// Run, jump, rise to 8.5 px above ground, land.
fn jump() -> Trace {
    trace(
        "0 R    0    0    0    0    idle 1 R
         1 R    1    0    1    0    walk 1 R
         2 R    3    0    2    0    run  1 R
         3 RJ   5    -4   2    -4   jump 0 R jump_start
         4 RJ   7    -7   2    -3   jump 0 R
         5 R    9    -8.5 2    -1.5 jump 0 R jump_release
         6 R    11   -8   2.5  0.5  fall 0 R
         7 R    13.5 0    2.5  8    run  1 R land
         8 R    16   0    2.5  0    run  1 R",
    )
}

#[test]
fn jump_metrics() {
    let t = jump();
    // Peak -y is 8.5; -y on the jump_start frame is 4.
    assert_eq!(metric(Metric::ApexHeight, &t).as_deref(), Some("4.5"));
    // Rows 4..=7: land counts, jump_start doesn't.
    assert_eq!(metric(Metric::Airtime, &t).as_deref(), Some("4"));
    assert_eq!(metric(Metric::TopSpeed, &t).as_deref(), Some("2.5"));
    // First non-zero vx on row 1, first 2.5 on row 6.
    assert_eq!(metric(Metric::FramesToTop, &t).as_deref(), Some("5"));
    assert_eq!(metric(Metric::FinalX, &t).as_deref(), Some("16"));
    assert_eq!(metric(Metric::FinalY, &t).as_deref(), Some("0"));
}

#[test]
fn metrics_without_their_event_are_missing() {
    let t = jump();
    for m in [
        Metric::RollDistance,
        Metric::CoyoteFrames,
        Metric::BounceHeight,
    ] {
        assert_eq!(compute(m, &t), None, "{m}");
    }
    // A jump that never lands has no apex or airtime.
    let t = trace(
        "0 J 0 0  0 0  idle 1 R
         1 J 0 -4 0 -4 jump 0 R jump_start
         2 J 0 -7 0 -3 jump 0 R",
    );
    assert_eq!(compute(Metric::ApexHeight, &t), None);
    assert_eq!(compute(Metric::Airtime, &t), None);
    // Never moving horizontally: no frames_to_top, but top_speed is 0.
    assert_eq!(compute(Metric::FramesToTop, &t), None);
    assert_eq!(metric(Metric::TopSpeed, &t).as_deref(), Some("0"));
}

#[test]
fn top_speed_is_absolute() {
    let t = trace(
        "0 L 0    0 0     0 idle 1 L
         1 L -1   0 -1    0 walk 1 L
         2 L -2.5 0 -1.5  0 walk 1 L
         3 L -3.5 0 -1    0 walk 1 L",
    );
    assert_eq!(metric(Metric::TopSpeed, &t).as_deref(), Some("1.5"));
    assert_eq!(metric(Metric::FramesToTop, &t).as_deref(), Some("1"));
    assert_eq!(metric(Metric::FinalX, &t).as_deref(), Some("-3.5"));
}

#[test]
fn only_the_first_occurrence_counts() {
    let t = trace(
        "0 J 0 0  0 0  jump 0 R jump_start
         1 . 0 -2 0 0  fall 0 R
         2 . 0 0  0 0  idle 1 R land
         3 J 0 -9 0 -9 jump 0 R jump_start
         4 . 0 0  0 0  idle 1 R land",
    );
    assert_eq!(metric(Metric::ApexHeight, &t).as_deref(), Some("2"));
    assert_eq!(metric(Metric::Airtime, &t).as_deref(), Some("2"));
}

#[test]
fn roll_distance() {
    let t = trace(
        "0 R  0  0 0 0 idle 1 R
         1 RA 2  0 2 0 roll 1 R roll_start
         2 R  5  0 3 0 roll 1 R
         3 R  8  0 3 0 roll 1 R
         4 R  10 0 2 0 walk 1 R roll_end",
    );
    assert_eq!(metric(Metric::RollDistance, &t).as_deref(), Some("8"));
}

#[test]
fn coyote_frames() {
    let t = trace(
        "0 R  0 0  1 0  walk 1 R
         1 R  1 1  1 1  fall 0 R leave_ground
         2 R  2 2  1 1  fall 0 R
         3 RJ 3 -2 1 -4 jump 0 R jump_start
         4 R  4 0  1 4  walk 1 R land",
    );
    assert_eq!(metric(Metric::CoyoteFrames, &t).as_deref(), Some("2"));

    // Landing before any jump: missing.
    let t = trace(
        "0 R  0 0 1 0 walk 1 R leave_ground
         1 R  1 2 1 2 fall 0 R
         2 R  2 0 1 0 walk 1 R land
         3 RJ 3 0 1 0 jump 0 R jump_start",
    );
    assert_eq!(compute(Metric::CoyoteFrames, &t), None);
}

#[test]
fn bounce_height() {
    let t = trace(
        "0 . 0 0  0 0  fall   0 R
         1 . 0 -1 0 -6 bounce 0 R bounce
         2 . 0 -6 0 -2 bounce 0 R
         3 . 0 -5 0 1  fall   0 R
         4 . 0 3  0 0  idle   1 R land",
    );
    assert_eq!(metric(Metric::BounceHeight, &t).as_deref(), Some("5"));
}

#[test]
fn height_windows_include_the_land_frame_and_need_it() {
    // Landing on a ledge higher than anywhere the jump reached before.
    let t = trace(
        "0 J 0 0   0 0  idle 1 R
         1 J 0 -4  0 -4 jump 0 R jump_start
         2 J 0 -6  0 -2 jump 0 R
         3 J 0 -10 0 0  idle 1 R land",
    );
    assert_eq!(metric(Metric::ApexHeight, &t).as_deref(), Some("6"));
    // A bounce cut off before landing has no height, rather than the
    // height up to the end of the trace.
    let t = trace(
        "0 . 0 0  0 0  fall   0 R
         1 . 0 -1 0 -6 bounce 0 R bounce
         2 . 0 -6 0 -2 bounce 0 R",
    );
    assert_eq!(compute(Metric::BounceHeight, &t), None);
}

fn scalar(metric: Metric, tol: &str) -> Comparison {
    Comparison::Scalar {
        metric,
        tol: d(tol),
    }
}

#[test]
fn scalar_comparison_tolerance_is_inclusive() {
    let reference = trace("0 R 1.1 0 0 0 idle 1 R");
    let at_tol = trace("0 R 1.35 0 0 0 idle 1 R");
    let past_tol = trace("0 R 1.36 0 0 0 idle 1 R");
    assert!(compare_one(&scalar(Metric::FinalX, "0.25"), &reference, &at_tol).pass);
    let outcome = compare_one(&scalar(Metric::FinalX, "0.25"), &reference, &past_tol);
    assert!(!outcome.pass);
    assert_eq!(
        outcome.detail,
        Detail::Scalar {
            reference: Some(d("1.1")),
            candidate: Some(d("1.36")),
            tol: d("0.25")
        }
    );
    assert_eq!(
        outcome.to_string(),
        format!(
            "FAIL  {:<32} reference 1.1, candidate 1.36 (tol 0.25)",
            "final_x"
        )
    );
}

#[test]
fn missing_metric_fails_unless_missing_from_both() {
    let jumped = jump();
    let stood = trace("0 . 0 0 0 0 idle 1 R");
    let c = scalar(Metric::Airtime, "1");
    assert!(compare_one(&c, &stood, &stood).pass);
    assert!(!compare_one(&c, &jumped, &stood).pass);
    assert!(!compare_one(&c, &stood, &jumped).pass);
    assert!(
        compare_one(&c, &stood, &stood)
            .to_string()
            .contains("reference missing, candidate missing")
    );
}

fn curve(fields: &[Field], event: Event, frames: usize, tol: &str) -> Comparison {
    Comparison::Curve(Curve {
        fields: fields.to_vec(),
        event,
        frames,
        tol: d(tol),
    })
}

#[test]
fn curve_aligns_on_the_event_and_compares_positions_relatively() {
    let reference = jump();
    // Same jump starting two frames later and 100 px further right.
    let candidate = trace(
        "0 R  100   0    0   0    idle 1 R
         1 R  100   0    0   0    idle 1 R
         2 R  101   0    1   0    walk 1 R
         3 R  103   0    2   0    run  1 R
         4 RJ 105   -4   2   -4   jump 0 R jump_start
         5 RJ 107   -7   2   -3   jump 0 R
         6 R  109   -8.5 2   -1.5 jump 0 R jump_release
         7 R  111   -8   2.5 0.5  fall 0 R
         8 R  113.5 0    2.5 8    run  1 R land",
    );
    let c = curve(
        &[Field::X, Field::Y, Field::Vx, Field::Vy],
        Event::JumpStart,
        5,
        "0",
    );
    let outcome = compare_one(&c, &reference, &candidate);
    assert!(outcome.pass, "{outcome}");
    assert_eq!(
        outcome.detail,
        Detail::Curve(CurveDetail::Matched {
            frames: 5,
            tol: d("0")
        })
    );
}

#[test]
fn curve_reports_the_first_divergence() {
    let reference = jump();
    let mut candidate = jump();
    candidate.rows[5].vy = d("-1"); // 0.5 off on the jump_start frame + 2
    candidate.rows[6].y = d("-7"); // further off, but later
    let c = curve(&[Field::Y, Field::Vy], Event::JumpStart, 5, "0.25");
    let outcome = compare_one(&c, &reference, &candidate);
    assert!(!outcome.pass);
    assert_eq!(
        outcome.detail,
        Detail::Curve(CurveDetail::Diverged(Divergence {
            offset: 2,
            reference_frame: 5,
            candidate_frame: 5,
            field: Field::Vy,
            reference: d("-1.5"),
            candidate: d("-1"),
            tol: d("0.25"),
        }))
    );
    assert!(outcome.to_string().ends_with(
        "first divergence at +2 (reference frame 5, candidate frame 5): vy reference -1.5, candidate -1 (tol 0.25)"
    ));

    // Positions are reported relative to the aligned frame: -7 - -4.
    let c = curve(&[Field::Y], Event::JumpStart, 5, "0.25");
    let Detail::Curve(CurveDetail::Diverged(d6)) = compare_one(&c, &reference, &candidate).detail
    else {
        panic!("expected a divergence");
    };
    assert_eq!(
        (d6.offset, d6.reference, d6.candidate),
        (3, d("-4"), d("-3"))
    );
}

#[test]
fn curve_fails_when_a_trace_ends_early() {
    let reference = jump(); // jump_start on row 3 of 9: 6 rows from it
    let c = curve(&[Field::Y], Event::JumpStart, 7, "1");
    let outcome = compare_one(&c, &reference, &reference);
    assert!(!outcome.pass);
    assert_eq!(
        outcome.detail,
        Detail::Curve(CurveDetail::TooShort {
            needed: 7,
            reference: 6,
            candidate: 6
        })
    );
    assert!(
        compare_one(
            &curve(&[Field::Y], Event::JumpStart, 6, "1"),
            &reference,
            &reference
        )
        .pass
    );
}

#[test]
fn curve_without_its_event() {
    let jumped = jump();
    let stood = trace("0 . 0 0 0 0 idle 1 R");
    let c = curve(&[Field::Y], Event::JumpStart, 3, "1");

    let outcome = compare_one(&c, &jumped, &stood);
    assert!(!outcome.pass);
    assert!(
        outcome
            .to_string()
            .ends_with("no `jump_start` in the candidate")
    );
    assert!(
        compare_one(&c, &stood, &jumped)
            .to_string()
            .ends_with("no `jump_start` in the reference")
    );

    // Same rule as scalar metrics: missing from both is not a failure.
    let outcome = compare_one(&c, &stood, &stood);
    assert!(outcome.pass);
    assert!(
        outcome
            .to_string()
            .ends_with("no `jump_start` in both traces")
    );
}
