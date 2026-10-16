//! Entity archetypes: what an entity's `type` means.
//!
//! The built-in archetypes are in `content/base/archetypes.toml`, which
//! documents each behavior and its values. An entity's `params` change
//! its archetype's values for that entity alone.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use clamber_fixed::Fixed;
use clamber_level::{Entity, Vec2};
use serde::Deserialize;
use toml::{Table, Value};

use crate::player::Facing;

const BUILTIN: &str = include_str!("../../../content/base/archetypes.toml");
const BUILTIN_ORIGIN: &str = "content/base/archetypes.toml";

/// How a thing behaves, with the values its behavior uses.
// A collectible's reach makes it the largest by far; there are few
// things, and being `Copy` keeps them simple.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Behavior {
    /// An enemy that walks along the ground at `speed`, turning at walls
    /// and ledges, and at the ends of its `range` if it has one. Above 0,
    /// `response` is the part of the gap to that speed it closes each
    /// frame, as the player's `speed_response`; 0 changes speed at once.
    /// Its facing follows a turn `turn_delay` frames later. Defeated, it
    /// falls as `defeat` says. When it appears with the view (see
    /// [`View`](crate::View)), it waits `appear_wait` frames, counting
    /// the one it appears on, before it moves. Walkers with the same
    /// `group` above 0 appear and are removed with the view together.
    Walker {
        speed: Fixed,
        response: Fixed,
        facing: Facing,
        turn_delay: u8,
        appear_wait: u8,
        group: u32,
        range: Option<(Fixed, Fixed)>,
        defeat: Fall,
    },
    /// An enemy that flies from where it's placed to `path` away and
    /// back, taking `period` frames for the round trip.
    Patrol { path: Vec2, period: u32 },
    /// Fires the player off at `velocity` on touch.
    Launcher { velocity: Vec2 },
    /// An object the player can pick up, carry, throw and put down
    /// (see [`carry`](crate::carry)), from `reach` px away or closer.
    Carry { reach: Fixed },
    /// A collectible the player takes from within its [`Reach`] (see
    /// [`collect`](crate::collect)). It adds `worth` to the collectible
    /// count, one at a time: the first on taking it, then one every
    /// `release_every` frames, each `count_delay` frames after that.
    /// With a view, it can only be taken inside its `window`, and
    /// `arrive` decides when each one reaches the count.
    Collect {
        worth: u8,
        release_every: u8,
        count_delay: u8,
        /// For the first character, and for any other.
        reach: [Reach; 2],
        /// How far its `x` may be past the view's left edge, both whole
        /// px, ends included, for it to be taken. `None`: anywhere.
        window: Option<(i32, i32)>,
        arrive: Arrive,
    },
}

/// With a view, when each released part of a collectible reaches the
/// count, by where the collectible is in the view on its release step:
/// the first of `rules` it's inside gives the delay, and with none,
/// `otherwise` does. `None` never reaches the count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Arrive {
    /// Whether the view is the one from the start of the release step,
    /// before it moves; otherwise it's the one from its end.
    pub at_start: bool,
    pub rules: [ArriveRule; MAX_ARRIVE_RULES],
    /// Only the first `rule_count` are used.
    pub rule_count: u8,
    pub otherwise: Option<u8>,
}

/// One of [`Arrive`]'s rules: a collectible whose position, less the
/// view's left and top edges, all whole px, is in `x` and `y` (ends
/// included) reaches the count `delay` frames after its release, or
/// never. Without vertical view motion, `y` always holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ArriveRule {
    pub x: (i32, i32),
    pub y: (i32, i32),
    pub delay: Option<u8>,
}

/// The most rules an [`Arrive`] can have.
pub const MAX_ARRIVE_RULES: usize = 4;

/// Where a player can take a collectible from, by what it's doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reach {
    /// Standing still.
    pub stand: Span,
    /// Walking or running.
    pub walk: Span,
    /// Crouching.
    pub crouch: Span,
    pub roll: Span,
    /// The first `jump_early` frames of a jump, from the one it starts on.
    pub jump: Span,
    /// The rest of a jump, other times in the air, and the first
    /// `land_hold` frames after landing.
    pub air: Span,
    /// In the air after `jump`, the lowest `below` while rising: the
    /// first `(vy, below)` whose `vy` is at least the player's vertical
    /// speed gives it. Past them all, it's `air`'s. Only the first
    /// `air_steps` are used.
    pub air_below: [(Fixed, i32); MAX_AIR_STEPS],
    pub air_steps: u8,
    pub jump_early: u8,
    pub land_hold: u8,
}

/// The most `air_below` steps a [`Reach`] can have.
pub const MAX_AIR_STEPS: usize = 8;

/// Offsets from a collectible a player takes it from, in whole px, ends
/// included: how far the player is past it the way the player faces,
/// and how far the player's feet are below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub past: (i32, i32),
    pub below: (i32, i32),
}

impl Span {
    pub fn contains(self, past: i32, below: i32) -> bool {
        (self.past.0..=self.past.1).contains(&past)
            && (self.below.0..=self.below.1).contains(&below)
    }
}

/// How a defeated walker leaves: it jumps up at `jump`, then falls
/// under `gravity` up to `max_fall`, through the floor and out of the
/// level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fall {
    pub jump: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
}

impl Behavior {
    /// Whether it's an enemy: it hurts the player and can be defeated.
    pub fn is_enemy(self) -> bool {
        matches!(self, Behavior::Walker { .. } | Behavior::Patrol { .. })
    }
}

/// An entity's archetype with its own params applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Archetype {
    pub behavior: Behavior,
    /// The box it touches the player with: standing on its position for
    /// a walker or an object that can be carried, centred on it
    /// otherwise.
    pub size: Vec2,
    /// How far the box's middle is in front of its position, the way it's
    /// going.
    pub offset: Fixed,
}

impl Archetype {
    /// The box as `(left, top, right, bottom)` for a thing at `pos` going
    /// the way `heading` says.
    pub fn bounds(&self, pos: Vec2, heading: Facing) -> (Fixed, Fixed, Fixed, Fixed) {
        let half_w = half(self.size.x);
        let x = match heading {
            Facing::Left => pos.x - self.offset,
            Facing::Right => pos.x + self.offset,
        };
        let (top, bottom) = match self.behavior {
            Behavior::Walker { .. } | Behavior::Carry { .. } => (pos.y - self.size.y, pos.y),
            _ => {
                let half_h = half(self.size.y);
                (pos.y - half_h, pos.y + half_h)
            }
        };
        (x - half_w, top, x + half_w, bottom)
    }
}

fn half(v: Fixed) -> Fixed {
    Fixed::from_bits(v.to_bits() / 2)
}

/// An entity whose params don't fit its archetype.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchetypeError {
    pub origin: String,
    pub message: String,
}

impl fmt::Display for ArchetypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for ArchetypeError {}

/// Archetypes by name, as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Archetypes {
    tables: BTreeMap<String, Table>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchetypesFile {
    archetypes: BTreeMap<String, Table>,
}

impl Archetypes {
    /// The engine's archetypes, from `content/base/archetypes.toml`.
    pub fn builtin() -> &'static Archetypes {
        static BUILTIN_ARCHETYPES: OnceLock<Archetypes> = OnceLock::new();
        BUILTIN_ARCHETYPES.get_or_init(|| {
            Archetypes::from_toml_str(BUILTIN, BUILTIN_ORIGIN)
                .expect("built-in archetypes are valid")
        })
    }

    /// Reads a file of `[archetypes.<name>]` tables, checking that each
    /// is complete.
    pub fn from_toml_str(src: &str, origin: &str) -> Result<Archetypes, ArchetypeError> {
        let err = |message: String| ArchetypeError {
            origin: origin.to_string(),
            message,
        };
        let file: ArchetypesFile =
            toml::from_str(src).map_err(|e| err(e.to_string().trim_end().to_string()))?;
        for (name, table) in &file.archetypes {
            parse(table, &format!("archetypes.{name}")).map_err(err)?;
        }
        Ok(Archetypes {
            tables: file.archetypes,
        })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    /// What `entity` is: its archetype with its params applied, or `None`
    /// if its type isn't an archetype. `origin` and `path` name the
    /// entity in errors.
    pub fn resolve(
        &self,
        entity: &Entity,
        origin: &str,
        path: &str,
    ) -> Result<Option<Archetype>, ArchetypeError> {
        let Some(base) = self.tables.get(&entity.kind) else {
            return Ok(None);
        };
        let err = |message: String| ArchetypeError {
            origin: origin.to_string(),
            message,
        };
        let mut table = base.clone();
        for (key, value) in &entity.params {
            if key == "behavior" {
                return Err(err(format!("{path}.params.behavior: can't be changed")));
            }
            table.insert(key.clone(), value.clone());
        }
        parse(&table, &format!("{path}.params"))
            .map(Some)
            .map_err(err)
    }
}

/// Reads one archetype's table. Errors start with `path`.
fn parse(table: &Table, path: &str) -> Result<Archetype, String> {
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let number = |key: &str| -> Result<Fixed, String> {
        clamber_pack::read_number(get(key)?, "", &format!("{path}.{key}")).map_err(|e| e.message)
    };
    let pair = |key: &str| -> Result<Vec2, String> {
        let at = format!("{path}.{key}");
        match get(key)? {
            Value::Array(items) if items.len() == 2 => {
                let n = |i: usize| {
                    clamber_pack::read_number(&items[i], "", &format!("{at}[{i}]"))
                        .map_err(|e| e.message)
                };
                Ok(Vec2::new(n(0)?, n(1)?))
            }
            _ => Err(format!("{at}: expected [x, y]")),
        }
    };

    let behavior = match get("behavior")? {
        Value::String(s) => s.as_str(),
        _ => return Err(format!("{path}.behavior: expected a string")),
    };
    let known: &[&str] = match behavior {
        "walker" => &[
            "speed",
            "response",
            "facing",
            "turn_delay",
            "appear_wait",
            "group",
            "range",
            "defeat_jump",
            "defeat_gravity",
            "defeat_max_fall",
        ],
        "patrol" => &["path", "period"],
        "launcher" => &["velocity"],
        "carry" => &["reach"],
        "collect" => &[
            "worth",
            "release_every",
            "count_delay",
            "reach",
            "window",
            "arrive",
            "arrive_view",
            "arrive_else",
        ],
        _ => return Err(format!("{path}.behavior: unknown behavior `{behavior}`")),
    };
    if let Some(key) = table.keys().find(|k| {
        !matches!(k.as_str(), "behavior" | "size" | "offset") && !known.contains(&k.as_str())
    }) {
        return Err(format!("{path}.{key}: `{behavior}` has no such value"));
    }

    let size = pair("size")?;
    if size.x <= Fixed::ZERO || size.y <= Fixed::ZERO {
        return Err(format!("{path}.size: must be positive"));
    }
    let behavior = match behavior {
        "walker" => {
            let speed = number("speed")?;
            if speed.is_negative() {
                return Err(format!("{path}.speed: must not be negative"));
            }
            let facing = match get("facing")? {
                Value::String(s) if s == "left" => Facing::Left,
                Value::String(s) if s == "right" => Facing::Right,
                _ => return Err(format!("{path}.facing: expected \"left\" or \"right\"")),
            };
            let response = number("response")?;
            if response.is_negative() || response > Fixed::ONE {
                return Err(format!("{path}.response: must be from 0 to 1"));
            }
            let frames = |key: &str| match get(key)? {
                Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
                _ => Err(format!(
                    "{path}.{key}: expected a whole number of frames, 0 to 255"
                )),
            };
            let turn_delay = frames("turn_delay")?;
            let appear_wait = frames("appear_wait")?;
            let group = match get("group")? {
                Value::Integer(n) if (0..=i64::from(u32::MAX)).contains(n) => *n as u32,
                _ => return Err(format!("{path}.group: expected a whole number, 0 or more")),
            };
            let range = match get("range")? {
                Value::Array(items) if items.is_empty() => None,
                Value::Array(items) if items.len() == 2 => {
                    let r = pair("range")?;
                    if r.x > r.y {
                        return Err(format!("{path}.range: left end is past the right end"));
                    }
                    Some((r.x, r.y))
                }
                _ => return Err(format!("{path}.range: expected [] or [left, right]")),
            };
            let defeat = Fall {
                jump: number("defeat_jump")?,
                gravity: number("defeat_gravity")?,
                max_fall: number("defeat_max_fall")?,
            };
            if defeat.gravity <= Fixed::ZERO || defeat.max_fall <= Fixed::ZERO {
                return Err(format!(
                    "{path}: `defeat_gravity` and `defeat_max_fall` must be positive"
                ));
            }
            Behavior::Walker {
                speed,
                response,
                facing,
                turn_delay,
                appear_wait,
                group,
                range,
                defeat,
            }
        }
        "patrol" => {
            let period = match get("period")? {
                Value::Integer(n) if *n >= 2 && *n <= i64::from(u32::MAX) => *n as u32,
                _ => {
                    return Err(format!(
                        "{path}.period: expected a whole number of frames, at least 2"
                    ));
                }
            };
            Behavior::Patrol {
                path: pair("path")?,
                period,
            }
        }
        "carry" => {
            let reach = number("reach")?;
            if reach.is_negative() {
                return Err(format!("{path}.reach: must not be negative"));
            }
            Behavior::Carry { reach }
        }
        "collect" => {
            let small = |key: &str, min: i64| match get(key)? {
                Value::Integer(n) if (min..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
                _ => Err(format!(
                    "{path}.{key}: expected a whole number, {min} to {}",
                    u8::MAX
                )),
            };
            let reach = match get("reach")? {
                Value::Array(tables) if (1..=2).contains(&tables.len()) => {
                    let first = parse_reach(&tables[0], &format!("{path}.reach[0]"))?;
                    match tables.get(1) {
                        Some(t) => [first, parse_reach(t, &format!("{path}.reach[1]"))?],
                        None => [first; 2],
                    }
                }
                _ => return Err(format!("{path}.reach: expected one or two tables")),
            };
            let count_delay = small("count_delay", 0)?;
            let window = match table.get("window") {
                None => None,
                Some(Value::Array(items)) if items.is_empty() => None,
                Some(Value::Array(items)) if items.len() == 2 => {
                    let end = |i: usize| match &items[i] {
                        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
                        _ => Err(format!("{path}.window[{i}]: expected a whole number of px")),
                    };
                    let (from, to) = (end(0)?, end(1)?);
                    if from > to {
                        return Err(format!("{path}.window: it ends before it starts"));
                    }
                    Some((from, to))
                }
                Some(_) => return Err(format!("{path}.window: expected [] or [from, to]")),
            };
            Behavior::Collect {
                worth: small("worth", 1)?,
                release_every: small("release_every", 0)?,
                count_delay,
                reach,
                window,
                arrive: parse_arrive(table, path, count_delay)?,
            }
        }
        _ => Behavior::Launcher {
            velocity: pair("velocity")?,
        },
    };
    Ok(Archetype {
        behavior,
        size,
        offset: number("offset")?,
    })
}

/// Reads a collectible's `arrive`, `arrive_view` and `arrive_else`.
/// Left out, there are no rules, the view is the one from the end of the
/// step, and `otherwise` is `count_delay`.
fn parse_arrive(table: &Table, path: &str, count_delay: u8) -> Result<Arrive, String> {
    let delay = |value: &Value, at: &str| match value {
        Value::String(s) if s == "never" => Ok(None),
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(Some(*n as u8)),
        _ => Err(format!(
            "{at}: expected a whole number of frames, 0 to 255, or \"never\""
        )),
    };
    let at_start = match table.get("arrive_view") {
        None => false,
        Some(Value::String(s)) if s == "end" => false,
        Some(Value::String(s)) if s == "start" => true,
        Some(_) => {
            return Err(format!("{path}.arrive_view: expected \"start\" or \"end\""));
        }
    };
    let otherwise = match table.get("arrive_else") {
        None => Some(count_delay),
        Some(v) => delay(v, &format!("{path}.arrive_else"))?,
    };
    let none = Vec::new();
    let items = match table.get("arrive") {
        None => &none,
        Some(Value::Array(items)) if items.len() <= MAX_ARRIVE_RULES => items,
        Some(_) => {
            return Err(format!(
                "{path}.arrive: expected a list of up to {MAX_ARRIVE_RULES} tables"
            ));
        }
    };
    let mut rules = [ArriveRule::default(); MAX_ARRIVE_RULES];
    for (i, item) in items.iter().enumerate() {
        let at = format!("{path}.arrive[{i}]");
        let Value::Table(rule) = item else {
            return Err(format!("{at}: expected a table"));
        };
        const KEYS: &[&str] = &["x_from", "x_to", "y_from", "y_to", "delay"];
        if let Some(key) = rule.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!("{at}.{key}: no such value"));
        }
        let end = |key: &str, open: i32| match rule.get(key) {
            None => Ok(open),
            Some(Value::Integer(n)) if i32::try_from(*n).is_ok() => Ok(*n as i32),
            Some(_) => Err(format!("{at}.{key}: expected a whole number of px")),
        };
        let x = (end("x_from", i32::MIN)?, end("x_to", i32::MAX)?);
        let y = (end("y_from", i32::MIN)?, end("y_to", i32::MAX)?);
        if x.0 > x.1 || y.0 > y.1 {
            return Err(format!("{at}: a range ends before it starts"));
        }
        let Some(d) = rule.get("delay") else {
            return Err(format!("{at}: missing `delay`"));
        };
        rules[i] = ArriveRule {
            x,
            y,
            delay: delay(d, &format!("{at}.delay"))?,
        };
    }
    Ok(Arrive {
        at_start,
        rules,
        rule_count: items.len() as u8,
        otherwise,
    })
}

/// Reads one character's [`Reach`] table. Errors start with `path`.
fn parse_reach(value: &Value, path: &str) -> Result<Reach, String> {
    let Value::Table(table) = value else {
        return Err(format!("{path}: expected a table"));
    };
    const KEYS: &[&str] = &[
        "stand",
        "walk",
        "crouch",
        "roll",
        "jump",
        "air",
        "air_below",
        "jump_early",
        "land_hold",
    ];
    if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(format!("{path}.{key}: no such value"));
    }
    let get = |key: &str| {
        table
            .get(key)
            .ok_or_else(|| format!("{path}: missing `{key}`"))
    };
    let whole = |v: &Value, at: &str| match v {
        Value::Integer(n) if i32::try_from(*n).is_ok() => Ok(*n as i32),
        _ => Err(format!("{at}: expected a whole number of px")),
    };
    let span = |key: &str| -> Result<Span, String> {
        let at = format!("{path}.{key}");
        let n = match get(key)? {
            Value::Array(items) if items.len() == 4 => items
                .iter()
                .enumerate()
                .map(|(i, v)| whole(v, &format!("{at}[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(format!("{at}: expected [past from, to, below from, to]")),
        };
        if n[0] > n[1] || n[2] > n[3] {
            return Err(format!("{at}: a range ends before it starts"));
        }
        Ok(Span {
            past: (n[0], n[1]),
            below: (n[2], n[3]),
        })
    };
    let frames = |key: &str| match get(key)? {
        Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => Ok(*n as u8),
        _ => Err(format!(
            "{path}.{key}: expected a whole number of frames, 0 to 255"
        )),
    };
    let mut air_below = [(Fixed::ZERO, 0); MAX_AIR_STEPS];
    let steps = match get("air_below")? {
        Value::Array(items) if items.len() <= MAX_AIR_STEPS => items,
        _ => {
            return Err(format!(
                "{path}.air_below: expected a list of up to {MAX_AIR_STEPS} [vy, below] pairs"
            ));
        }
    };
    for (i, item) in steps.iter().enumerate() {
        let at = format!("{path}.air_below[{i}]");
        let Value::Array(pair) = item else {
            return Err(format!("{at}: expected [vy, below]"));
        };
        let [vy, below] = pair.as_slice() else {
            return Err(format!("{at}: expected [vy, below]"));
        };
        let vy = clamber_pack::read_number(vy, "", &format!("{at}[0]")).map_err(|e| e.message)?;
        if i > 0 && vy <= air_below[i - 1].0 {
            return Err(format!("{at}: speeds must go up"));
        }
        air_below[i] = (vy, whole(below, &format!("{at}[1]"))?);
    }
    Ok(Reach {
        stand: span("stand")?,
        walk: span("walk")?,
        crouch: span("crouch")?,
        roll: span("roll")?,
        jump: span("jump")?,
        air: span("air")?,
        air_below,
        air_steps: steps.len() as u8,
        jump_early: frames("jump_early")?,
        land_hold: frames("land_hold")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(kind: &str, params: &str) -> Entity {
        Entity {
            kind: kind.to_string(),
            id: None,
            at: Vec2::ints(0, 0),
            params: toml::from_str(params).unwrap(),
        }
    }

    #[test]
    fn builtin_archetypes_cover_enemies_objects_and_collectibles() {
        let a = Archetypes::builtin();
        let resolve = |kind| a.resolve(&entity(kind, ""), "test", "e").unwrap().unwrap();
        assert!(matches!(
            resolve("crawler").behavior,
            Behavior::Walker {
                facing: Facing::Left,
                ..
            }
        ));
        assert!(matches!(
            resolve("flutter").behavior,
            Behavior::Patrol { .. }
        ));
        assert!(matches!(
            resolve("launcher").behavior,
            Behavior::Launcher { .. }
        ));
        assert!(matches!(resolve("crate").behavior, Behavior::Carry { .. }));
        assert!(matches!(
            resolve("spark").behavior,
            Behavior::Collect { worth: 1, .. }
        ));
        assert!(matches!(
            resolve("cluster").behavior,
            Behavior::Collect { worth: 10, .. }
        ));
        assert_eq!(a.resolve(&entity("gem", ""), "test", "e"), Ok(None));
    }

    #[test]
    fn params_change_values_for_one_entity() {
        let a = Archetypes::builtin();
        let e = entity("launcher", "velocity = [4, -6.5]\nsize = [8, 8]");
        let got = a.resolve(&e, "test", "e").unwrap().unwrap();
        assert_eq!(
            got.behavior,
            Behavior::Launcher {
                velocity: Vec2::new(Fixed::from_int(4), Fixed::from_ratio_pow2(-13, 1))
            }
        );
        assert_eq!(got.size, Vec2::ints(8, 8));
        assert_eq!(got.bounds(Vec2::ints(10, 10), Facing::Right), {
            let i = Fixed::from_int;
            (i(6), i(6), i(14), i(14))
        });
    }

    #[test]
    fn walkers_can_have_a_range() {
        let a = Archetypes::builtin();
        let range = |params| match a.resolve(&entity("crawler", params), "test", "e") {
            Ok(Some(Archetype {
                behavior: Behavior::Walker { range, .. },
                ..
            })) => range,
            other => panic!("{other:?}"),
        };
        assert_eq!(range(""), None);
        assert_eq!(
            range("range = [-128, 127.5]"),
            Some((Fixed::from_int(-128), Fixed::from_ratio_pow2(255, 1)))
        );
    }

    #[test]
    fn collectibles_can_have_a_window_and_arrival_rules() {
        let a = Archetypes::builtin();
        let collect = |params| match a.resolve(&entity("spark", params), "test", "e") {
            Ok(Some(Archetype {
                behavior: Behavior::Collect { window, arrive, .. },
                ..
            })) => (window, arrive),
            other => panic!("{other:?}"),
        };
        let (window, arrive) = collect("");
        assert_eq!(window, Some((-39, 263)));
        assert!(!arrive.at_start);
        assert_eq!(arrive.otherwise, None);
        assert_eq!(arrive.rule_count, 2);
        assert_eq!(
            arrive.rules[0],
            ArriveRule {
                x: (8, 255),
                y: (i32::MIN, i32::MAX),
                delay: Some(33),
            }
        );

        let (window, arrive) = collect(
            "window = []\narrive = [{ y_from = 4, delay = 2 }]\narrive_else = 0\narrive_view = \"start\"",
        );
        assert_eq!(window, None);
        assert!(arrive.at_start);
        assert_eq!(arrive.otherwise, Some(0));
        assert_eq!(arrive.rule_count, 1);
        assert_eq!(arrive.rules[0].x, (i32::MIN, i32::MAX));
        assert_eq!(arrive.rules[0].y, (4, i32::MAX));
    }

    #[test]
    fn offset_moves_the_box_the_way_it_goes() {
        let a = Archetypes::builtin();
        let e = entity("crawler", "size = [20, 12]\noffset = 0.5\nresponse = 0.125");
        let got = a.resolve(&e, "test", "e").unwrap().unwrap();
        let x = |v: f64| Fixed::from_f64(v).unwrap();
        let at = Vec2::ints(100, 50);
        assert_eq!(
            got.bounds(at, Facing::Right),
            (x(90.5), x(38.0), x(110.5), x(50.0))
        );
        assert_eq!(
            got.bounds(at, Facing::Left),
            (x(89.5), x(38.0), x(109.5), x(50.0))
        );
    }

    #[test]
    fn bad_params_are_reported_with_their_path() {
        let a = Archetypes::builtin();
        let check = |kind, params, want: &str| {
            let e = a
                .resolve(&entity(kind, params), "lvl", "entities[3]")
                .unwrap_err();
            assert_eq!(e.origin, "lvl");
            assert!(e.message.contains(want), "{}", e.message);
        };
        check(
            "crawler",
            "behavior = \"patrol\"",
            "entities[3].params.behavior",
        );
        check("crawler", "path = [1, 2]", "entities[3].params.path");
        check("crawler", "facing = \"up\"", "entities[3].params.facing");
        check("crawler", "speed = -1", "must not be negative");
        check("spark", "window = [5, 4]", "entities[3].params.window");
        check(
            "spark",
            "arrive = [{ x_from = 1 }]",
            "entities[3].params.arrive[0]: missing `delay`",
        );
        check(
            "spark",
            "arrive = [{ x = 1, delay = 0 }]",
            "entities[3].params.arrive[0].x",
        );
        check(
            "spark",
            "arrive_else = \"soon\"",
            "entities[3].params.arrive_else",
        );
        check("spark", "arrive_view = 1", "entities[3].params.arrive_view");
        check("crawler", "response = 1.5", "entities[3].params.response");
        check(
            "crawler",
            "turn_delay = -1",
            "entities[3].params.turn_delay",
        );
        check("crawler", "range = [1]", "expected [] or [left, right]");
        check(
            "crawler",
            "range = [8, -8]",
            "left end is past the right end",
        );
        check("crawler", "defeat_gravity = 0", "must be positive");
        check("flutter", "period = 1", "entities[3].params.period");
        check("launcher", "velocity = [1]", "expected [x, y]");
        check("launcher", "size = [0, 4]", "must be positive");
        check("crate", "reach = -1", "entities[3].params.reach");
        check("spark", "worth = 0", "entities[3].params.worth");
        check("spark", "reach = []", "expected one or two tables");
        check(
            "spark",
            "reach = [{ stand = [1, 2, 3] }]",
            "entities[3].params.reach[0]",
        );
    }

    #[test]
    fn files_are_checked_when_read() {
        let bad = "[archetypes.x]\nbehavior = \"walker\"\nsize = [1, 1]\noffset = 0\nspeed = 1\nresponse = 0\nturn_delay = 0\nrange = []\ndefeat_jump = 1\ndefeat_gravity = 1\ndefeat_max_fall = 1\n";
        let e = Archetypes::from_toml_str(bad, "f").unwrap_err();
        assert!(e.message.contains("archetypes.x: missing `facing`"), "{e}");
        let unknown = "[archetypes.x]\nbehavior = \"teleporter\"\nsize = [1, 1]\noffset = 0\n";
        assert!(Archetypes::from_toml_str(unknown, "f").is_err());
    }
}
