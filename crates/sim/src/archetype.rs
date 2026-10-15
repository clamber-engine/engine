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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Behavior {
    /// An enemy that walks along the ground at `speed`, turning at walls
    /// and ledges, and at the ends of its `range` if it has one. Above 0,
    /// `response` is the part of the gap to that speed it closes each
    /// frame, as the player's `speed_response`; 0 changes speed at once.
    /// Its facing follows a turn `turn_delay` frames later. Defeated, it
    /// falls as `defeat` says.
    Walker {
        speed: Fixed,
        response: Fixed,
        facing: Facing,
        turn_delay: u8,
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
            if !table.contains_key(key) {
                return Err(err(format!(
                    "{path}.params.{key}: `{}` has no such value",
                    entity.kind
                )));
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
            "range",
            "defeat_jump",
            "defeat_gravity",
            "defeat_max_fall",
        ],
        "patrol" => &["path", "period"],
        "launcher" => &["velocity"],
        "carry" => &["reach"],
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
            let turn_delay = match get("turn_delay")? {
                Value::Integer(n) if (0..=i64::from(u8::MAX)).contains(n) => *n as u8,
                _ => {
                    return Err(format!(
                        "{path}.turn_delay: expected a whole number of frames, 0 to 255"
                    ));
                }
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
    fn builtin_archetypes_cover_two_enemies_and_two_objects() {
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
