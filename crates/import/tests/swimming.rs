//! An imported ceiling grid as the swimming player meets it: the depths
//! `spec/behavior/swimming.md` works out from the ground below ("In the
//! ground") against the swimmer's own reading of the collision.

use clamber_import::{Below, Ground, Half, Layout};
use clamber_level::{Collision, Level};
use clamber_sim::swim_depth;

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

/// The swimmer reads the collision without knowing which halves were
/// ceilings: it measures to the nearer of the ground's top and its
/// underside. So the depths agree with the spec's wherever the spec's
/// edge is the nearer one, and elsewhere the swimmer's edge is as near
/// or nearer.
///
/// Points past the grid's right edge are left out: the format reads
/// them from the next row, but the level ends there.
#[test]
fn depths_follow_the_ground_below() {
    let ground = ceilings();
    let level = level(&ground);
    let (mut same, mut nearer, mut moves_differ) = (0, 0, 0);
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
        if meets(spec) != meets(engine) {
            moves_differ += 1;
        }
        let at = format!("x {x} feet {feet}: spec {spec:?}, engine {engine:?}");
        match (spec, engine) {
            (None, None) => same += 1,
            (Some((want, ceiling)), Some((got, edge))) if ceiling == edge && want != DEEP => {
                // The swimmer only tells 1 to 3 px from deeper.
                assert!(got == want || got.min(want) >= 4, "{at}");
                same += 1;
            }
            (Some((want, _)), Some((got, _))) => {
                assert!(got <= want || got.min(want) >= 4, "{at}");
                nearer += 1;
            }
            _ => panic!("{at}"),
        }
    }
    // How many points a move across would meet differently: the ground
    // near a band's edge, mostly a floor down to its band's bottom line
    // or a ceiling from its band's top line, where the swimmer finds the
    // other edge within 3 px.
    assert_eq!((same + nearer, moves_differ), (26_336, 977));
}
