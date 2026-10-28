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
    run_on("default", geometry, start, inputs, frames)
}

fn run_on(
    preset: &str,
    geometry: &str,
    start: &str,
    inputs: &str,
    frames: u32,
) -> Result<clamber_trace::Trace, String> {
    let text = format!(
        "# clamber-scenario v2\nid: t\nsummary: s\nframes: {frames}\n\n[geometry]\n{geometry}\n[start]\n{start}\n[inputs]\n{inputs}\n"
    );
    let scenario = Scenario::parse(&text, "inline").map_err(|e| e.to_string())?;
    record(&scenario, Path::new("unused.level"), preset)
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
        "floor 0..9 0\nentity barrel 0 0\n",
        "mode: riding\nwith: barrel\n",
        "",
        5,
    )
    .unwrap_err();
    assert!(e.contains("riding") || e.contains("barrel"), "{e}");
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

#[test]
fn a_follower_at_an_offset_or_absent_builds_and_leaves_a_plain_walk_alone() {
    let floor = "floor 0..99 0\n";
    let settled = run(floor, "mode: grounded\n", "0-29 R\n", 30).unwrap();
    for follower in ["none", "at -40 0", "at 20 -8"] {
        let start = format!("mode: grounded\nfollower: {follower}\n");
        let trace = run(floor, &start, "0-29 R\n", 30).unwrap();
        assert_eq!(trace.rows, settled.rows, "{follower}");
    }
}

#[test]
fn a_start_with_the_second_character_leading_needs_a_second_character() {
    let start = "mode: grounded\nlead: second\n";
    match run("floor 0..99 0\n", start, "0-29 R\n", 30) {
        Ok(trace) => assert!(trace.rows.last().unwrap().x > d("10")),
        Err(e) => assert!(e.contains("no second character"), "{e}"),
    }
}

#[test]
fn a_walker_in_the_geometry_is_bounced_on() {
    // The v1 bounce-walker scenario, as geometry: a walker 76 columns to
    // the right of the player, heading left.
    let trace = run_on(
        "classic-1",
        "floor -200..823 0\nentity walker 76 0 facing L\n",
        "mode: grounded\n",
        "20-139 J\n",
        140,
    )
    .unwrap();
    assert!(trace.rows.iter().any(|r| r.state == State::Bounce));
}

#[test]
fn a_sign_in_the_geometry_changes_nothing() {
    // behavior/signs.md: play goes exactly as with the sign out of reach.
    let run = |geometry: &str| {
        run_on(
            "classic-1",
            geometry,
            "mode: grounded\n",
            "10-80 R\n20-30 J\n",
            100,
        )
        .unwrap()
    };
    let without = run("floor -200..823 0\n");
    let with = run("floor -200..823 0\nentity sign 20 0\n");
    assert_eq!(with.rows.len(), without.rows.len());
    for (a, b) in with.rows.iter().zip(&without.rows) {
        assert_eq!(a.state, b.state);
        assert_eq!((a.x, a.y), (b.x, b.y));
    }
}

#[test]
fn a_collectible_in_the_geometry_changes_no_traced_quantity() {
    // behavior/collectibles.md: taking one moves nothing.
    let run = |geometry: &str| {
        run_on(
            "classic-1",
            geometry,
            "mode: grounded\n",
            "10-80 R\n20-30 J\n",
            100,
        )
        .unwrap()
    };
    let without = run("floor -200..823 0\n");
    let with = run("floor -200..823 0\nentity collectible 20 0\n");
    assert_eq!(with.rows.len(), without.rows.len());
    for (a, b) in with.rows.iter().zip(&without.rows) {
        assert_eq!(a.state, b.state);
        assert_eq!((a.x, a.y), (b.x, b.y));
    }
}

#[test]
fn a_critter_in_the_geometry_changes_nothing() {
    // behavior/critter.md: play goes exactly as without it.
    let run = |geometry: &str| {
        run_on(
            "classic-1",
            geometry,
            "mode: grounded\n",
            "10-80 R\n20-30 J\n",
            100,
        )
        .unwrap()
    };
    let without = run("floor -200..823 0\n");
    let with = run("floor -200..823 0\nentity critter 20 40\n");
    assert_eq!(with.rows.len(), without.rows.len());
    for (a, b) in with.rows.iter().zip(&without.rows) {
        assert_eq!(a.state, b.state);
        assert_eq!((a.x, a.y), (b.x, b.y));
    }
}

#[test]
fn a_crate_is_bounced_off_when_landed_on() {
    let with = |entity: &str| {
        run_on(
            "classic-1",
            &format!("floor 0..99 0\n{entity}"),
            "mode: airborne\nheight: 60\nvx: 0\n",
            "",
            40,
        )
        .unwrap()
    };
    let bare = with("");
    let crated = with("entity crate 0 0\n");
    assert!(
        bare.rows
            .iter()
            .all(|r| !r.events.contains(&clamber_trace::Event::Bounce))
    );
    assert!(
        crated
            .rows
            .iter()
            .any(|r| r.events.contains(&clamber_trace::Event::Bounce)),
        "{:?}",
        crated.rows.iter().map(|r| &r.events).collect::<Vec<_>>()
    );
}

#[test]
fn walking_through_a_crate_does_nothing() {
    let with = |entity: &str| {
        run_on(
            "classic-1",
            &format!("floor 0..99 0\n{entity}"),
            "mode: grounded\n",
            "0-59 R\n",
            60,
        )
        .unwrap()
    };
    assert_eq!(with(""), with("entity crate 20 0\n"));
}

#[test]
fn a_thrower_hurts_a_player_standing_on_its_ledge() {
    let with = |entity: &str| {
        run_on(
            "classic-1",
            &format!("floor 0..99 0\n{entity}"),
            "mode: grounded\n",
            "",
            5,
        )
        .unwrap()
    };
    let hurt = |t: &clamber_trace::Trace| {
        t.rows
            .iter()
            .position(|r| r.events.contains(&clamber_trace::Event::Hurt))
    };
    // The ledge is U = -1 (thrower-contact.md), and only `across` counts
    // there. It doesn't hurt on frame 1, the one before its first throw
    // (thrower-contact.md), and no ball is in the air before frame 2.
    assert_eq!(hurt(&with("")), None);
    assert_eq!(hurt(&with("entity thrower 0 0\n")), Some(2));
}

#[test]
fn a_thrower_facing_right_is_an_error() {
    let e = run(
        "floor 0..9 0\nentity thrower 5 0 facing R\n",
        "mode: grounded\n",
        "",
        5,
    )
    .unwrap_err();
    assert!(e.contains("thrower faces left"), "{e}");
    run(
        "floor 0..9 0\nentity thrower 5 0 facing L\n",
        "mode: grounded\n",
        "",
        5,
    )
    .unwrap();
}

#[test]
fn a_carrying_start_holds_the_object_while_action_is_held() {
    let floor = "floor -200..999 0\n";
    for with in ["barrel", "holder"] {
        let start = format!("mode: carrying\nwith: {with}\n");
        let held = run_on("classic-1", floor, &start, "0-29 AR\n", 30).unwrap();
        let thrown = run_on("classic-1", floor, &start, "0-29 R\n", 30).unwrap();
        // Carrying, the player walks from frame 0; letting go of action on
        // frame 0 throws at once, which holds the player still first.
        assert!(held.rows[1].x > Decimal::ZERO, "{with}: {}", held.rows[1].x);
        assert_eq!(thrown.rows[1].x, Decimal::ZERO, "{with}");
        assert!(held.rows.iter().all(|r| r.state != State::Jump));
    }
}

#[test]
fn only_a_barrel_or_a_holder_is_carried() {
    let e = run("floor 0..9 0\n", "mode: carrying\nwith: crate\n", "", 5).unwrap_err();
    assert!(e.contains("barrel"), "{e}");
}

#[test]
fn a_rope_start_holds_the_rope_until_a_jump_off() {
    let start = "mode: rope\nfollower: none\n";
    let geometry = "entity rope 0 100\n";
    let held = run_on("classic-1", geometry, start, "0-9 U\n", 10).unwrap();
    assert!(held.rows.iter().all(|r| r.state == State::Rope));
    assert!(held.rows[9].y < Decimal::ZERO, "{}", held.rows[9].y);
    // Jump with a direction leaves the rope on frame 0; the rope is the
    // one last held, so it isn't caught again at once.
    let off = run_on("classic-1", geometry, start, "0 JR\n1-9 R\n", 10).unwrap();
    assert_ne!(off.rows[1].state, State::Rope);
}

#[test]
fn a_rope_end_above_the_feet_line_slides_off_at_once() {
    let e = run_on(
        "classic-1",
        "entity rope 0 -25\n",
        "mode: rope\nfollower: none\n",
        "",
        3,
    );
    assert!(e.is_err(), "{e:?}");
    let e = run_on("classic-1", "entity rope 0 0\n", "mode: rope\n", "", 3);
    assert!(e.unwrap_err().contains("follower"));
}

#[test]
fn a_riding_start_rides_the_animal_at_its_speed() {
    let geometry = "floor -9..60 0\nentity crate 0 0\n";
    let start = "mode: riding\nwith: crate\nvx: 1.5\n";
    let t = run_on("classic-1", geometry, start, "0-19 R\n", 20).unwrap();
    assert!(
        t.rows[19].x > t.rows[5].x,
        "{} {}",
        t.rows[19].x,
        t.rows[5].x
    );
    assert_eq!(t.rows[19].y, Decimal::ZERO);
    // Moving on frame 1 already: the animal carries its speed.
    assert!(t.rows[1].x > Decimal::ZERO, "{}", t.rows[1].x);
}

#[test]
fn a_riding_start_may_name_idle_or_walk_whatever_the_speed() {
    let geometry = "floor -9..60 0\nentity crate 0 0\n";
    let walk = run_on(
        "classic-1",
        geometry,
        "mode: riding\nwith: crate\nstate: walk\n",
        "",
        3,
    )
    .unwrap();
    assert_eq!(walk.rows[0].state, clamber_trace::State::Walk);
    let idle = run_on(
        "classic-1",
        geometry,
        "mode: riding\nwith: crate\nvx: 1.5\nstate: idle\n",
        "",
        3,
    )
    .unwrap();
    assert_eq!(idle.rows[0].state, clamber_trace::State::Idle);
}

#[test]
fn launch_and_metal_barrels_are_built() {
    let geometry = "floor -9..60 0\nentity launch_barrel 20 0\nentity down_launch_barrel 30 0\nentity metal_barrel 40 0\n";
    run_on("classic-1", geometry, "mode: grounded\n", "0-9 R\n", 10).unwrap();
}

#[test]
fn the_spec_riding_vector_is_built() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/vectors/scenario-v2/valid-riding.scn");
    let scenario = Scenario::load(&path).unwrap();
    let t = record(&scenario, Path::new("unused.level"), "classic-1").unwrap();
    assert_eq!(t.rows.len(), 10);
    assert!(t.rows[9].x < Decimal::ZERO, "{}", t.rows[9].x);
}
