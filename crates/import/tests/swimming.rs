//! An imported ceiling grid as the swimming player meets it: the depths
//! `spec/behavior/swimming.md` works out from the ground below ("In the
//! ground") against the swimmer's own reading of the collision.

use std::sync::Arc;

use clamber_import::{Below, Ground, Half, Layout, water};
use clamber_level::{Collision, Fixed, Level, Vec2};
use clamber_sim::{State, World, generic_feel, swim_depth};

fn vector(name: &str) -> String {
    let path = format!(
        "{}/../../spec/vectors/level-ground/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn hex(name: &str) -> Vec<u8> {
    vector(name)
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

fn ceilings() -> Ground {
    Ground::new(
        Layout::Ceilings,
        &hex("ceiling-grid.hex"),
        &hex("ceiling-types.hex"),
    )
    .unwrap()
}

fn level(ground: &Ground) -> Level {
    let src = format!(
        "format = 1\nname = \"ceilings\"\nsize = [{}, {}]\nstart = [0, 0]\n",
        ground.width(),
        ground.bands() * 32,
    );
    let mut level = Level::from_toml_str(&src, "ceilings").unwrap();
    level.collision = Collision::new(ground.colliders(0..ground.width(), "normal"));
    level
}

const DEEP: i64 = i64::MAX;

/// How far the spec puts a point in the ground, and whether out of it
/// is down: `None` if it isn't in. A ceiling half with no ground below
/// has no edge to be out of: [`DEEP`].
fn spec_depth(ground: &Ground, x: i64, feet: i64, below: Below) -> Option<(i64, bool)> {
    let ceiling = matches!(
        ground.half(x, (feet - 1).div_euclid(32)),
        Half::Ceiling { .. }
    );
    let px = match below {
        Below::Line(l) if ceiling => l - feet + 1,
        Below::Line(l) => feet - l,
        _ if ceiling => DEEP,
        _ => 0,
    };
    (px > 0).then_some((px, ceiling))
}

/// What a move across into a point does: nothing (`None`), a step up or a
/// push down by 1 to 3 px, or a wall (4).
fn meets(depth: Option<(i64, bool)>) -> Option<(i64, bool)> {
    depth.map(|(px, ceiling)| if px >= 4 { (4, false) } else { (px, ceiling) })
}

/// Whether the spec's ground below for feet at `feet` in column `x` is
/// a ceiling line 32 lines up, for a floor on its band's top line under a
/// ceiling. The collision has the floor's own top there.
fn under_a_ceiling_on_the_top_line(ground: &Ground, x: i64, feet: i64) -> bool {
    let band = (feet - 1).div_euclid(32);
    matches!(ground.half(x, band), Half::Floor(Some(s)) if s.line == 32 * band + 1)
        && matches!(ground.half(x, band - 1), Half::Ceiling { .. })
}

/// The importer marks ceilings as hanging, so the swimmer measures a
/// floor from its top and a ceiling from its underside, as the spec
/// does, and the two agree on which it is everywhere.
///
/// They differ only where the spec finds no ground below a ceiling (a
/// ceiling line at or below its band's bottom line over a floor in the
/// next band): that's in by 4 or more, a wall, but the collision has the
/// ceiling cut at the band's bottom line, with water below it.
///
/// Points past the grid's right edge are left out: the format reads
/// them from the next row, but the level ends there.
#[test]
fn depths_follow_the_ground_below() {
    let ground = ceilings();
    let level = level(&ground);
    let (mut points, mut moves_differ) = (0, 0);
    for line in vector("ceiling-ground-below.tsv").lines().skip(1) {
        let row: Vec<&str> = line.split('\t').collect();
        let x: i64 = row[0].parse().unwrap();
        let feet: i64 = row[1].parse().unwrap();
        if x >= ground.width() || under_a_ceiling_on_the_top_line(&ground, x, feet) {
            continue;
        }
        let below = match row[2] {
            "none" => Below::None,
            line => Below::Line(line.parse().unwrap()),
        };
        let spec = spec_depth(&ground, x, feet, below);
        let engine = swim_depth(&level, x, feet);
        let at = format!("x {x} feet {feet}: spec {spec:?}, engine {engine:?}");
        points += 1;
        match (spec, engine) {
            (None, None) => {}
            (Some((DEEP, true)), Some((_, true))) => {
                if meets(spec) != meets(engine) {
                    moves_differ += 1;
                }
            }
            (Some((want, ceiling)), Some((got, edge))) if ceiling == edge => {
                // The swimmer only tells 1 to 3 px from deeper.
                assert!(got == want || got.min(want) >= 4, "{at}");
            }
            _ => panic!("{at}"),
        }
    }
    assert_eq!((points, moves_differ), (26_336, 33));
}

/// An area on a ceiling grid is water all over, unless it starts the
/// player riding, and the player starts swimming anywhere in it that's
/// open.
#[test]
fn a_ceiling_grid_is_water_all_over() {
    let ground = ceilings();
    let mut level = level(&ground);
    let region = water(&ground, false).expect("a ceiling grid swims");
    let (width, height) = (ground.width(), ground.bands() * 32);
    for (x, y) in [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ] {
        assert!(region.area.contains(point(x, y)), "({x}, {y})");
    }
    assert!(!region.area.contains(point(width, 0)));
    assert!(!region.area.contains(point(0, height)));
    assert_eq!(water(&ground, true), None, "an area that starts riding");
    level.regions.push(region);

    let level = Arc::new(level);
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let mut starts = 0;
    for x in (8..width).step_by(97) {
        for feet in (1..height).step_by(29) {
            if swim_depth(&level, x, feet).is_some() {
                continue;
            }
            let mut here = (*level).clone();
            here.start = point(x, feet);
            let world = World::new(Arc::new(here), feel.clone());
            assert_eq!(world.player.state, State::Swim, "x {x} feet {feet}");
            starts += 1;
        }
    }
    assert!(starts > 100, "{starts} open starts");
}

/// Areas on a column grid or a row grid have no water.
#[test]
fn other_grids_are_dry() {
    let rows = Ground::new(Layout::Rows, &hex("rows-grid.hex"), &hex("types.hex")).unwrap();
    let columns = Ground::new(Layout::Columns, &hex("grid.hex"), &hex("types.hex")).unwrap();
    assert_eq!(water(&rows, false), None);
    assert_eq!(water(&columns, false), None);
}

fn point(x: i64, y: i64) -> Vec2 {
    let px = |n: i64| Fixed::from_int(n as i32);
    Vec2::new(px(x), px(y))
}
