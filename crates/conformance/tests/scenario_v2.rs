//! Scenario format v2: the engine builds its test place from the file.

use std::path::Path;

use clamber_conformance::record;
use clamber_trace::{Decimal, Scenario, State};

fn run(
    geometry: &str,
    start: &str,
    inputs: &str,
    frames: u32,
) -> Result<clamber_trace::Trace, String> {
    let text = format!(
        "# clamber-scenario v2\nid: t\nsummary: s\nframes: {frames}\n\n[geometry]\n{geometry}\n[start]\n{start}\n[inputs]\n{inputs}\n"
    );
    let scenario = Scenario::parse(&text, "inline").map_err(|e| e.to_string())?;
    record(&scenario, Path::new("unused.level"), "default")
}

fn d(s: &str) -> Decimal {
    Decimal::parse(s).unwrap()
}

#[test]
fn walks_right_on_flat_ground_from_the_stated_start() {
    let trace = run("floor 0..99 0\n", "mode: grounded\n", "0-29 R\n", 30).unwrap();
    assert_eq!(trace.rows.len(), 30);
    let last = trace.rows.last().unwrap();
    assert!(last.x > d("10"), "{}", last.x);
    assert_eq!(last.y, Decimal::ZERO);
    assert!(last.ground);
}

#[test]
fn a_ledge_in_the_geometry_drops_the_player() {
    let trace = run(
        "floor 0..39 0\nfloor 40..199 48\n",
        "mode: grounded\n",
        "0-89 R\n",
        90,
    )
    .unwrap();
    let last = trace.rows.last().unwrap();
    assert_eq!(last.y, d("48"));
    assert!(last.ground);
    assert!(trace.rows.iter().any(|r| r.state == State::Fall));
}

#[test]
fn a_pit_has_no_bottom() {
    let trace = run(
        "floor 0..19 0\nfloor 20..60 none\n",
        "mode: grounded\n",
        "0-59 R\n",
        60,
    )
    .unwrap();
    assert!(trace.rows.last().unwrap().y > d("48"));
    assert!(!trace.rows.last().unwrap().ground);
}

#[test]
fn an_airborne_start_falls_from_its_height() {
    let trace = run(
        "floor 0..99 0\n",
        "mode: airborne\nheight: 24\nvx: 1.5\nfacing: L\n",
        "",
        60,
    )
    .unwrap();
    let first = &trace.rows[0];
    assert_eq!(first.state, State::Fall);
    assert!(!first.ground);
    assert_eq!(first.facing, clamber_trace::trace::Facing::Left);
    // It lands 24 px below where it started.
    assert_eq!(trace.rows.last().unwrap().y, d("24"));
    assert!(trace.rows.last().unwrap().ground);
}

#[test]
fn a_wall_stops_the_player() {
    let trace = run(
        "floor 0..99 0\nwall 30..39 -64 0\n",
        "mode: grounded\n",
        "0-79 R\n",
        80,
    )
    .unwrap();
    let last = trace.rows.last().unwrap();
    assert!(last.x < d("30"), "{}", last.x);
}

#[test]
fn what_it_cannot_build_is_said_so() {
    let e = run(
        "floor 0..9 0\nentity walker 5 0\n",
        "mode: grounded\n",
        "",
        5,
    )
    .unwrap_err();
    assert!(e.contains("can't build `entity walker`"), "{e}");
    let e = run("floor 0..9 0\nentity rope 0 0\n", "mode: rope\n", "", 5).unwrap_err();
    assert!(
        e.contains("entity rope") || e.contains("start mode rope"),
        "{e}"
    );
}

#[test]
fn a_swimming_start_is_swimming_on_frame_0() {
    let trace = run(
        "floor 0..99 0\nwater 0..99 -40\n",
        "mode: swimming\n",
        "0-19 R\n",
        20,
    )
    .unwrap();
    assert!(trace.rows.iter().all(|r| r.state == State::Swim));
    assert!(trace.rows.last().unwrap().x > d("2"));
}

#[test]
fn a_button_held_before_frame_0_is_not_pressed_on_it() {
    let floor = "floor 0..99 0\n";
    let fresh = run(floor, "mode: grounded\n", "0-9 J\n", 10).unwrap();
    assert!(fresh.rows.iter().any(|r| r.state == State::Jump));
    let held = run(floor, "mode: grounded\nheld: J\n", "0-9 J\n", 10).unwrap();
    assert!(held.rows.iter().all(|r| r.state != State::Jump));
}
