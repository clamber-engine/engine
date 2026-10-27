//! Test places built from a v2 scenario's `[geometry]` and `[start]`
//! (`spec/scenarios/FORMAT.md`, "Version 2").

use clamber_fixed::Fixed;
use clamber_level::{Level, Vec2};
use clamber_pack::Param;
use clamber_trace::Decimal;
use clamber_trace::setup::{Columns, Follower, Geometry, Mode, Setup};
use clamber_trace::{Scenario, State as TraceState};

use crate::archetype::Behavior;
use crate::follow::{self, Follow};
use crate::player::{Act, Facing, Hands, State, frames};
use crate::rope::{OnRope, ROPE};
use crate::world::Phase;
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
    let toml = toml(
        &setup.geometry,
        scenario.frames,
        (setup.start.mode == Mode::Carrying)
            .then_some(setup.start.with.as_deref())
            .flatten(),
    );
    Level::from_toml_str(&toml, &scenario.id).map_err(|e| e.to_string())
}

/// Puts a freshly started `world` into the scenario's start. The world
/// is on a level from [`level`]; the player stands at column 0.
pub fn start(world: &mut World, setup: &Setup) {
    let s = &setup.start;
    let carried =
        (s.mode == Mode::Carrying).then(|| world.things.iter().position(|t| t.kind == "crate"));
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
    if let Some(Some(thing)) = carried {
        // The pick-up is over: the next step is the first of carrying.
        let ticks = frames(world.player_physics()[Param::PickUpTime]);
        world.things[thing].phase = Phase::Held;
        world.player.hands = Some(Hands {
            thing,
            act: Act::PickUp,
            frame: ticks.saturating_sub(1),
            press: None,
        });
    }
    if s.mode == Mode::Rope {
        hold_rope(world);
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

/// The player holds the rope in column 0, which is the rope last held
/// (behavior/ropes.md, "A rope in a scenario"). The rope starts where
/// it's placed at full speed to the right, as every rope does.
fn hold_rope(world: &mut World) {
    let Some(thing) = world.things.iter().find(|t| t.kind == ROPE) else {
        return;
    };
    let Some(Behavior::Rope(values)) = thing.archetype.map(|a| a.behavior) else {
        return;
    };
    let on = OnRope {
        thing: thing.index,
        values,
        turn: None,
        climbing: 0,
    };
    world.player.ground = None;
    world.player.catch_rope(on);
}

fn fixed(d: Decimal) -> Fixed {
    Fixed::from_f64(d.to_string().parse().unwrap_or(0.0)).unwrap_or(Fixed::ZERO)
}

/// What this engine can't build, by the spec's own rule that a runner
/// which can't build a start says so.
fn check_supported(setup: &Setup) -> Result<(), Unsupported> {
    let g = &setup.geometry;
    let s = &setup.start;
    if let Some(e) = g.entities.iter().find(|e| {
        !matches!(
            e.kind.as_str(),
            "walker" | "sign" | "collectible" | "critter" | "crate" | "thrower" | "rope"
        )
    }) {
        return Err(format!(
            "can't build `entity {}` in test places yet",
            e.kind
        ));
    }
    if s.mode == Mode::Carrying && !matches!(s.with.as_deref(), Some("barrel" | "holder")) {
        return Err("mode carrying carries a `barrel` or a `holder` only".into());
    }
    if s.mode == Mode::Riding {
        return Err(format!("can't build start mode {:?} yet", s.mode).to_lowercase());
    }
    let default = match s.mode {
        Mode::Airborne if s.vy.is_negative() => TraceState::Jump,
        Mode::Airborne => TraceState::Fall,
        Mode::Swimming => TraceState::Swim,
        Mode::Rope => TraceState::Rope,
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
fn toml(g: &Geometry, frames: u32, carried: Option<&str>) -> String {
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
    // A sign is scenery that nothing touches and that changes no count
    // (behavior/signs.md, "What a sign does"), so it adds nothing here.
    // Nor does a collectible: taking one moves nothing and no traced
    // quantity counts them (behavior/collectibles.md). A critter is
    // scenery nothing touches either (behavior/critter.md).
    for e in g.entities.iter().filter(|e| e.kind == "walker") {
        // The spec's plain walker (behavior/walker.md): full speed from
        // the start, easing by an eighth of the gap, in the small walker's
        // sizes (walker-contact.md).
        let facing = if e.facing_left { "left" } else { "right" };
        out += &format!(
            "\n[[entities]]\ntype = \"crawler\"\nat = [{}, {}]\nparams = {{ speed = 1, response = 0.125, facing = \"{facing}\", size = [20, 12], offset = 0.5, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }}\n",
            x(e.column),
            y(e.height),
        );
    }
    // The object a carrying start holds (behavior/carrying.md): a
    // barrel, or one that holds the missing character. It starts at the
    // player's feet and is lifted by the pick-up's last step.
    if let Some(kind) = carried {
        out += &format!(
            "\n[[entities]]\ntype = \"crate\"\nat = [{}, {GROUND}]\nparams = {{ holds_character = {} }}\n",
            origin,
            kind == "holder",
        );
    }
    // A rope hangs with its bottom end `H` px under level 0, at the left
    // edge of column 0, where the player holds it (behavior/ropes.md,
    // "A rope in a scenario"); the preset's archetype has its values.
    for e in g.entities.iter().filter(|e| e.kind == "rope") {
        out += &format!(
            "\n[[entities]]\ntype = \"rope\"\nat = [{}, {}]\n",
            x(e.column),
            y(e.height),
        );
    }
    // A crate breaks under a player coming down on it and lets out its
    // animal (behavior/animal-crate.md); the preset has its ranges.
    for e in g.entities.iter().filter(|e| e.kind == "crate") {
        out += &format!(
            "\n[[entities]]\ntype = \"hutch\"\nat = [{}, {}]\n",
            x(e.column),
            y(e.height),
        );
    }
    // A thrower stands 1 px above the floor of its ledge, faces left and
    // throws as the preset's archetype does, which has the spec's default
    // throws (scenarios/FORMAT.md, "entity thrower"; formats/placements.md,
    // "Thrower settings").
    for e in g.entities.iter().filter(|e| e.kind == "thrower") {
        out += &format!(
            "\n[[entities]]\ntype = \"thrower\"\nat = [{}, {}]\n",
            x(e.column),
            y(e.height) - 1,
        );
    }
    out
}
