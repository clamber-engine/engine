//! Scenario format v2 against the spec's parser vectors
//! (`spec/vectors/scenario-v2/`).

use std::path::{Path, PathBuf};

use clamber_trace::Scenario;
use clamber_trace::setup::{Columns, Follower, Mode, Setup};

fn vectors() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors/scenario-v2")
}

fn table(name: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(vectors().join(name)).unwrap();
    text.lines()
        .skip(1)
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect()
}

fn columns(c: Columns) -> String {
    format!("{}..{}", c.first, c.last)
}

fn setup_of(file: &str) -> Setup {
    Scenario::load(&vectors().join(file))
        .unwrap_or_else(|e| panic!("{e}"))
        .setup
        .expect("a v2 scenario has a setup")
}

#[test]
fn valid_files_parse_to_the_expected_values() {
    let rows = table("expected.tsv");
    assert!(!rows.is_empty());
    for row in rows {
        let [
            file,
            floor_columns,
            floor_heights,
            ceiling,
            x_offset,
            mode,
            height,
            vx,
            facing,
            lead,
            follower,
            idle_frames,
        ] = &row[..]
        else {
            panic!("bad row {row:?}");
        };
        let s = setup_of(file);
        let g = &s.geometry;

        let first = g.floors.first().unwrap().columns.first;
        let last = g.floors.last().unwrap().columns.last;
        assert_eq!(format!("{first}..{last}"), *floor_columns, "{file}");
        let heights: Vec<String> = g
            .floors
            .iter()
            .map(|f| match f.top {
                Some(h) => format!("{}={h}", columns(f.columns)),
                None => format!("{}=none", columns(f.columns)),
            })
            .collect();
        assert_eq!(heights.join(";"), *floor_heights, "{file}");
        let ceilings: Vec<String> = g
            .ceilings
            .iter()
            .map(|c| format!("{}={}", columns(c.columns), c.at))
            .collect();
        let ceilings = if ceilings.is_empty() {
            "-".to_string()
        } else {
            ceilings.join(";")
        };
        assert_eq!(ceilings, *ceiling, "{file}");
        assert_eq!(g.x_offset.to_string(), *x_offset, "{file}");

        let st = &s.start;
        let mode_name = match st.mode {
            Mode::Grounded => "grounded",
            Mode::Airborne => "airborne",
            Mode::Riding => "riding",
            Mode::Carrying => "carrying",
            Mode::Swimming => "swimming",
            Mode::Rope => "rope",
        };
        assert_eq!(mode_name, mode, "{file}");
        assert_eq!(st.height.to_string(), *height, "{file}");
        assert_eq!(st.vx.to_string(), *vx, "{file}");
        assert_eq!(if st.facing_left { "L" } else { "R" }, facing, "{file}");
        assert_eq!(
            if st.second_leads { "second" } else { "first" },
            lead,
            "{file}"
        );
        let follower_text = match st.follower {
            Follower::None => "none".to_string(),
            Follower::Settled => "settled".to_string(),
            Follower::At(dx, dy) => format!("at {dx} {dy}"),
        };
        assert_eq!(follower_text, *follower, "{file}");
        assert_eq!(st.idle_frames.to_string(), *idle_frames, "{file}");
    }
}

#[test]
fn invalid_files_are_rejected_with_the_reason() {
    let rows = table("rejected.tsv");
    assert!(!rows.is_empty());
    for row in rows {
        let [file, reason] = &row[..] else {
            panic!("bad row {row:?}");
        };
        let e = Scenario::load(&vectors().join(file)).expect_err(file);
        assert!(
            e.to_string().contains(reason.as_str()),
            "{file}: `{e}` should say `{reason}`"
        );
    }
}

#[test]
fn objects_and_the_probe_flag_are_read() {
    let s = Scenario::load(&vectors().join("valid-objects.scn")).unwrap();
    let setup = s.setup.unwrap();
    assert!(setup.probe);
    assert_eq!(setup.geometry.walls.len(), 1);
    assert_eq!(setup.geometry.walls[0].top, -40);
    assert_eq!(setup.geometry.platforms[0].at, -24);
    assert_eq!(setup.geometry.water[0].columns.first, 30);
    assert!(setup.geometry.entities[0].facing_left);
    assert_eq!(setup.start.with.as_deref(), Some("barrel"));
    assert!(!setup_of("valid-pit.scn").probe);
}

fn parse(text: &str) -> Result<Scenario, String> {
    Scenario::parse(text, "inline").map_err(|e| e.to_string())
}

const BASE: &str =
    "# clamber-scenario v2\nid: t\nsummary: s\nframes: 5\n\n[geometry]\nfloor 0..9 0\n\n[start]\n";

#[test]
fn start_defaults_follow_the_mode() {
    let state = |mode: &str, extra: &str| {
        let text = format!("{BASE}mode: {mode}\n{extra}");
        parse(&text).unwrap().setup.unwrap().start.state.name()
    };
    assert_eq!(state("grounded", ""), "idle");
    assert_eq!(state("airborne", "vy: -3\n"), "jump");
    assert_eq!(state("airborne", ""), "fall");
    assert_eq!(state("airborne", "state: bounce\n"), "bounce");
    let s = parse(BASE).unwrap().setup.unwrap();
    assert_eq!(s.start.idle_frames, 30);
    assert_eq!(s.start.follower, Follower::Settled);
    assert_eq!(s.geometry.floor_at(50), Some(0));
    assert_eq!(s.geometry.floor_at(-5), Some(0));
}

#[test]
fn start_and_geometry_mistakes_are_errors() {
    let cases: &[(&str, &str)] = &[
        (
            "mode: grounded\nvx: 1\n",
            "`vx` is only for modes airborne and riding",
        ),
        (
            "mode: grounded\nheight: 4\n",
            "`height` is only for mode airborne",
        ),
        (
            "mode: grounded\nwith: crate\n",
            "`with` is only for modes riding and carrying",
        ),
        ("mode: carrying\n", "need `with`"),
        ("mode: grounded\nmode: grounded\n", "`mode` appears twice"),
        ("mode: flying\n", "unknown mode `flying`"),
        ("mode: swimming\n", "no water over column 0"),
        ("mode: rope\n", "no matching entity"),
        ("mode: grounded\nfoo: 1\n", "unknown start key `foo`"),
        (
            "mode: grounded\nstate: walk\nidle_frames: 3\n",
            "`idle_frames` is only for",
        ),
    ];
    for (start, message) in cases {
        let text = format!("{BASE}{start}");
        let e = parse(&text).unwrap_err();
        assert!(e.contains(message), "`{start}`: {e}");
    }
    let geometry: &[(&str, &str)] = &[
        ("floor 0..9 0\nfloor 5..12 0\n", "overlap"),
        ("floor 1..9 0\n", "column 0 has no floor"),
        ("floor 0..9 4\n", "must be at height 0"),
        ("floor 0..9 0\nx 1\n", "below 1"),
        (
            "floor 0..9 0\nentity ghost 1 0\n",
            "unknown entity kind `ghost`",
        ),
        ("floor 0..9 0\nbump 1\n", "unknown geometry statement"),
        ("floor 9..0 0\n", "run backwards"),
    ];
    for (statements, message) in geometry {
        let text = format!(
            "# clamber-scenario v2\nid: t\nsummary: s\nframes: 5\n\n[geometry]\n{statements}\n[start]\nmode: grounded\n"
        );
        let e = parse(&text).unwrap_err();
        assert!(e.contains(message), "`{statements}`: {e}");
    }
}

#[test]
fn sections_are_required_and_ordered() {
    let no_start =
        "# clamber-scenario v2\nid: t\nsummary: s\nframes: 5\n\n[geometry]\nfloor 0..9 0\n";
    assert!(
        parse(no_start)
            .unwrap_err()
            .contains("missing required section `[start]`")
    );
    let swapped = "# clamber-scenario v2\nid: t\nsummary: s\nframes: 5\n\n[start]\nmode: grounded\n[geometry]\nfloor 0..9 0\n";
    assert!(parse(swapped).unwrap_err().contains("out of order"));
    let v1 = "# clamber-scenario v1\nid: t\nsummary: s\nrequires: r\nstart: s\nframes: 5\n\n[geometry]\n";
    assert!(parse(v1).unwrap_err().contains("unknown section"));
}
