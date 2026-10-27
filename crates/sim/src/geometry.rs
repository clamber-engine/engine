//! Test places built from a v2 scenario's `[geometry]` and `[start]`
//! (`spec/scenarios/FORMAT.md`, "Version 2").

use clamber_fixed::Fixed;
use clamber_level::{Level, Vec2};
use clamber_trace::Decimal;
use clamber_trace::setup::{Columns, Follower, Geometry, Mode, Setup};
use clamber_trace::{Scenario, State as TraceState};

use crate::follow::{self, Follow};
use crate::player::{Facing, State};
use crate::world::World;

/// Where level 0 is in a built level, px from its top.
const GROUND: i64 = 1024;
/// How far the built level reaches left of the first column and right of
/// the last, px. The floor continues flat that far.
const LEFT_MARGIN: i64 = 256;
const RIGHT_MARGIN: i64 = 4096;
/// How thick a platform is, px.
const PLATFORM: i64 = 16;
/// How tall a ceiling block is, px.
const CEILING: i64 = 256;

/// Why a scenario can't be built here.
pub type Unsupported = String;

/// The test level for a v2 scenario, or why this engine can't build it.
pub fn level(scenario: &Scenario) -> Result<Level, Unsupported> {
    let setup = scenario.setup.as_ref().ok_or("not a v2 scenario")?;
    check_supported(setup)?;
    let toml = toml(&setup.geometry, scenario.frames);
    Level::from_toml_str(&toml, &scenario.id).map_err(|e| e.to_string())
}

/// Puts a freshly started `world` into the scenario's start. The world
/// is on a level from [`level`]; the player stands at column 0.
pub fn start(world: &mut World, setup: &Setup) {
    let s = &setup.start;
    let p = &mut world.player;
    p.facing = if s.facing_left {
        Facing::Left
    } else {
        Facing::Right
    };
    if s.mode == Mode::Airborne {
        p.pos.y -= fixed(s.height);
        p.vel = Vec2 {
            x: fixed(s.vx),
            y: fixed(s.vy),
        };
        p.ground = None;
        p.state = if s.state == TraceState::Jump {
            State::Jump
        } else {
            State::Fall
        };
    }
    if s.second_leads {
        // The roster's second character plays; the first one follows.
        world.player.character = 1;
        if let Some(other) = &mut world.other {
            other.character = 0;
        }
    }
    let lead = world.player.pos;
    match s.follower {
        Follower::None => world.other = None,
        Follower::Settled => {}
        Follower::At(dx, dy) => {
            if let Some(other) = &mut world.other {
                other.pos = Vec2 {
                    x: lead.x + Fixed::from_int(dx),
                    y: lead.y + Fixed::from_int(dy),
                };
                other.follow = Follow::at(follow::point(other.pos));
            }
        }
    }
}

fn fixed(d: Decimal) -> Fixed {
    Fixed::from_f64(d.to_string().parse().unwrap_or(0.0)).unwrap_or(Fixed::ZERO)
}

/// What this engine can't build, by the spec's own rule that a runner
/// which can't build a start says so.
fn check_supported(setup: &Setup) -> Result<(), Unsupported> {
    let g = &setup.geometry;
    let s = &setup.start;
    if let Some(e) = g.entities.first() {
        return Err(format!(
            "can't build `entity {}` in test places yet",
            e.kind
        ));
    }
    if matches!(s.mode, Mode::Riding | Mode::Carrying | Mode::Rope) {
        return Err(format!("can't build start mode {:?} yet", s.mode).to_lowercase());
    }
    let default = match s.mode {
        Mode::Airborne if s.vy.is_negative() => TraceState::Jump,
        Mode::Airborne => TraceState::Fall,
        Mode::Swimming => TraceState::Swim,
        _ => TraceState::Idle,
    };
    if s.state != default {
        return Err(format!("can't start in state `{}` yet", s.state.name()));
    }
    if matches!(s.mode, Mode::Grounded | Mode::Swimming) && s.vy != Decimal::ZERO {
        return Err(format!("`vy` in start mode {:?} can't be built", s.mode).to_lowercase());
    }
    Ok(())
}

/// The level file text. Column `c` is x = `origin + c`, level 0 is
/// y = [`GROUND`].
fn toml(g: &Geometry, frames: u32) -> String {
    let first = g.floors.first().map_or(0, |f| i64::from(f.columns.first));
    let last = g.floors.last().map_or(0, |f| i64::from(f.columns.last));
    let origin = LEFT_MARGIN - first.min(0);
    let width = origin + last.max(0) + 1 + RIGHT_MARGIN;
    let deepest = g
        .floors
        .iter()
        .filter_map(|f| f.top)
        .chain(g.walls.iter().map(|w| w.bottom))
        .chain(g.water.iter().map(|w| w.at))
        .max()
        .unwrap_or(0)
        .max(0);
    // Room to fall for the whole scenario at 8 px per frame.
    let height = GROUND + i64::from(deepest) + 8 * i64::from(frames) + 512;

    let x = |c: i32| origin + i64::from(c);
    let y = |v: i32| GROUND + i64::from(v);
    let mut out = String::from("format = 1\nname = \"scenario\"\n");
    out += &format!("size = [{width}, {height}]\n");
    // The offset is in 0..1, written "0" or "0.5".
    let offset = g.x_offset.to_string();
    let fraction = offset.strip_prefix('0').unwrap_or("");
    out += &format!("start = [{origin}{fraction}, {GROUND}]\n");

    let rect = |out: &mut String, left: i64, top: i64, right: i64, bottom: i64, extra: &str| {
        out.push_str(&format!(
            "\n[[collision]]\nshape = \"rect\"\nrect = [{left}, {top}, {}, {}]\n{extra}",
            right - left,
            bottom - top
        ));
    };
    for (i, f) in g.floors.iter().enumerate() {
        let Some(top) = f.top else { continue };
        // The outer runs go on flat to the level's edges.
        let left = if i == 0 { 0 } else { x(f.columns.first) };
        let right = if i == g.floors.len() - 1 {
            width
        } else {
            x(f.columns.last) + 1
        };
        rect(&mut out, left, y(top), right, height, "");
    }
    for w in &g.walls {
        rect(
            &mut out,
            x(w.columns.first),
            y(w.top),
            x(w.columns.last) + 1,
            y(w.bottom),
            "",
        );
    }
    for p in &g.platforms {
        let top = y(p.at);
        rect(
            &mut out,
            x(p.columns.first),
            top,
            x(p.columns.last) + 1,
            top + PLATFORM,
            "one_way = true\n",
        );
    }
    for c in &g.ceilings {
        let under = y(c.at);
        rect(
            &mut out,
            x(c.columns.first),
            under - CEILING,
            x(c.columns.last) + 1,
            under,
            "hangs = true\n",
        );
    }
    for w in &g.water {
        let Columns { first, last } = w.columns;
        out += &format!(
            "\n[[regions]]\nname = \"water\"\nrect = [{}, {}, {}, {}]\nwater = true\n",
            x(first),
            y(w.at),
            i64::from(last - first) + 1,
            height - y(w.at),
        );
    }
    out
}
