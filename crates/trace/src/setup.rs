//! The `[geometry]` and `[start]` sections of scenario format v2
//! (`spec/scenarios/FORMAT.md`, "Version 2").

use crate::{Buttons, Decimal, State};

/// An inclusive run of 1 px columns, `first..=last`. Column 0 holds the
/// player's x on frame 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    pub first: i32,
    pub last: i32,
}

impl Columns {
    pub fn contains(self, column: i32) -> bool {
        (self.first..=self.last).contains(&column)
    }
}

/// A run of columns whose floor top is `top` px down from level 0, or
/// `None` for no ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Floor {
    pub columns: Columns,
    pub top: Option<i32>,
}

/// Something over a run of columns at one vertical position (`platform`,
/// `ceiling` underside, `water` surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub columns: Columns,
    pub at: i32,
}

/// A solid block from `top` to `bottom` px down from level 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wall {
    pub columns: Columns,
    pub top: i32,
    pub bottom: i32,
}

/// An object standing with its base at `height` in `column`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entity {
    pub kind: String,
    pub column: i32,
    pub height: i32,
    pub facing_left: bool,
}

/// The kinds `entity` and `with` may name.
pub const ENTITY_KINDS: &[&str] = &[
    "walker",
    "thrower",
    "critter",
    "crate",
    "rope",
    "launch_barrel",
    "metal_barrel",
    "collectible",
    "sign",
    "barrel",
    "holder",
];

/// The ground and objects of a scenario's test place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Geometry {
    /// How far past the left edge of column 0 the player's x is on
    /// frame 0, in `0..1`.
    pub x_offset: Decimal,
    /// Sorted, contiguous, and covering column 0 at height 0.
    pub floors: Vec<Floor>,
    pub walls: Vec<Wall>,
    pub platforms: Vec<Surface>,
    pub ceilings: Vec<Surface>,
    pub water: Vec<Surface>,
    pub entities: Vec<Entity>,
}

impl Geometry {
    /// The floor top in `column`: the covered columns' own, else that of
    /// the nearest covered column.
    pub fn floor_at(&self, column: i32) -> Option<i32> {
        let first = self.floors.first()?;
        let last = self.floors.last()?;
        let run = if column < first.columns.first {
            first
        } else if column > last.columns.last {
            last
        } else {
            self.floors.iter().find(|f| f.columns.contains(column))?
        };
        run.top
    }
}

/// The `mode` of a `[start]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Grounded,
    Airborne,
    Riding,
    Carrying,
    Swimming,
    Rope,
}

/// Where the other character is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follower {
    None,
    Settled,
    /// Offset from the lead, px, right and down positive.
    At(i32, i32),
}

/// The player's situation on frame 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub mode: Mode,
    pub facing_left: bool,
    pub vx: Decimal,
    pub vy: Decimal,
    /// The state recorded on frame 0, with the mode's default filled in.
    pub state: State,
    /// Px above the ground below, for `Mode::Airborne`.
    pub height: Decimal,
    pub second_leads: bool,
    pub follower: Follower,
    pub idle_frames: u32,
    /// Inputs held on the frame before frame 0.
    pub held: Buttons,
    /// The kind ridden or carried.
    pub with: Option<String>,
}

/// What a v2 scenario adds to a v1 one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub geometry: Geometry,
    pub start: Start,
    /// A start the original never produces in play: there is no
    /// reference trace to compare with.
    pub probe: bool,
}

fn parse_int(s: &str) -> Result<i32, String> {
    s.parse::<i32>()
        .map_err(|_| format!("`{s}` is not a whole number"))
}

fn parse_columns(s: &str) -> Result<Columns, String> {
    let (first, last) = match s.split_once("..") {
        Some((a, b)) => (parse_int(a)?, parse_int(b)?),
        None => {
            let c = parse_int(s)?;
            (c, c)
        }
    };
    if first > last {
        return Err(format!("columns `{s}` run backwards"));
    }
    Ok(Columns { first, last })
}

/// A geometry statement, added to `geometry`. Contiguity and the player's
/// column are checked later by [`Geometry::check`].
pub(crate) fn parse_geometry_line(
    geometry: &mut Geometry,
    seen_x: &mut bool,
    line: &str,
) -> Result<(), String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields[..] {
        ["x", offset] => {
            if std::mem::replace(seen_x, true) {
                return Err("`x` appears twice".into());
            }
            let offset = Decimal::parse(offset)?;
            if offset.is_negative() || offset >= Decimal::from_count(1) {
                return Err(format!("`x {offset}` must be at least 0 and below 1"));
            }
            geometry.x_offset = offset;
        }
        ["floor", columns, height] => {
            let top = if height == "none" {
                None
            } else {
                Some(parse_int(height)?)
            };
            geometry.floors.push(Floor {
                columns: parse_columns(columns)?,
                top,
            });
        }
        ["wall", columns, top, bottom] => geometry.walls.push(Wall {
            columns: parse_columns(columns)?,
            top: parse_int(top)?,
            bottom: parse_int(bottom)?,
        }),
        ["platform", columns, at] => geometry.platforms.push(Surface {
            columns: parse_columns(columns)?,
            at: parse_int(at)?,
        }),
        ["ceiling", columns, at] => geometry.ceilings.push(Surface {
            columns: parse_columns(columns)?,
            at: parse_int(at)?,
        }),
        ["water", columns, at] => geometry.water.push(Surface {
            columns: parse_columns(columns)?,
            at: parse_int(at)?,
        }),
        ["entity", kind, column, height, ref facing @ ..] => {
            if !ENTITY_KINDS.contains(&kind) {
                return Err(format!(
                    "unknown entity kind `{kind}`; expected one of {}",
                    ENTITY_KINDS.join(", ")
                ));
            }
            let facing_left = match facing {
                [] => false,
                ["facing", "L"] => true,
                ["facing", "R"] => false,
                _ => return Err(format!("expected `facing L` or `facing R`, found `{line}`")),
            };
            if kind == "thrower" && facing == ["facing", "R"] {
                return Err("a thrower faces left: `facing R` isn't covered".into());
            }
            geometry.entities.push(Entity {
                kind: kind.to_string(),
                column: parse_int(column)?,
                height: parse_int(height)?,
                facing_left,
            });
        }
        [word, ..]
            if [
                "x", "floor", "wall", "platform", "ceiling", "water", "entity",
            ]
            .contains(&word) =>
        {
            return Err(format!("wrong number of fields in `{line}`"));
        }
        _ => {
            return Err(format!(
                "unknown geometry statement `{line}`; expected x, floor, wall, platform, ceiling, water or entity"
            ));
        }
    }
    Ok(())
}

impl Geometry {
    /// Sorts the floors and checks that they don't overlap, leave gaps,
    /// or miss column 0 or give it a height other than 0.
    pub(crate) fn check(&mut self) -> Result<(), String> {
        self.floors.sort_by_key(|f| f.columns.first);
        for pair in self.floors.windows(2) {
            let (a, b) = (pair[0].columns, pair[1].columns);
            if b.first <= a.last {
                return Err(format!(
                    "floor columns {}..{} and {}..{} overlap",
                    a.first, a.last, b.first, b.last
                ));
            }
            if b.first > a.last + 1 {
                let (from, to) = (a.last + 1, b.first - 1);
                return Err(if from == to {
                    format!("floor column {from} is not covered (columns must be contiguous)")
                } else {
                    format!(
                        "floor columns {from} and {} are not covered (columns must be contiguous)",
                        to
                    )
                });
            }
        }
        match self.floors.iter().find(|f| f.columns.contains(0)) {
            None => Err("the player's column 0 has no floor".into()),
            Some(f) if f.top != Some(0) => {
                Err("the floor in column 0 must be at height 0 (that is level 0)".into())
            }
            Some(_) => Ok(()),
        }
    }
}

impl Start {
    /// The defaults of the table in the spec.
    fn defaults() -> Start {
        Start {
            mode: Mode::Grounded,
            facing_left: false,
            vx: Decimal::ZERO,
            vy: Decimal::ZERO,
            state: State::Idle,
            height: Decimal::ZERO,
            second_leads: false,
            follower: Follower::Settled,
            idle_frames: 30,
            held: Buttons::NONE,
            with: None,
        }
    }

    /// Builds a start from its `key: value` lines (key, value, line).
    /// Errors carry the line they belong to, 0 for the section as a whole.
    pub(crate) fn from_lines(lines: &[(&str, &str, usize)]) -> Result<Start, (usize, String)> {
        let mut start = Start::defaults();
        let mut seen: Vec<&str> = Vec::new();
        let mut state_given = false;
        let mut idle_given = false;
        for &(key, value, n) in lines {
            let fail = |m: String| (n, m);
            if seen.contains(&key) {
                return Err(fail(format!("`{key}` appears twice")));
            }
            seen.push(key);
            match key {
                "mode" => {
                    start.mode = match value {
                        "grounded" => Mode::Grounded,
                        "airborne" => Mode::Airborne,
                        "riding" => Mode::Riding,
                        "carrying" => Mode::Carrying,
                        "swimming" => Mode::Swimming,
                        "rope" => Mode::Rope,
                        _ => {
                            return Err(fail(format!(
                                "unknown mode `{value}`; expected grounded, airborne, riding, carrying, swimming or rope"
                            )));
                        }
                    }
                }
                "facing" => {
                    start.facing_left = match value {
                        "L" => true,
                        "R" => false,
                        _ => return Err(fail(format!("facing must be L or R, found `{value}`"))),
                    }
                }
                "vx" => start.vx = Decimal::parse(value).map_err(fail)?,
                "vy" => start.vy = Decimal::parse(value).map_err(fail)?,
                "state" => {
                    start.state = State::from_name(value).ok_or_else(|| {
                        fail(format!(
                            "unknown state `{value}` (see traces/VOCABULARY.md)"
                        ))
                    })?;
                    state_given = true;
                }
                "height" => {
                    start.height = Decimal::parse(value).map_err(fail)?;
                    if start.height.is_negative() {
                        return Err(fail("height can't be negative".into()));
                    }
                }
                "lead" => {
                    start.second_leads = match value {
                        "first" => false,
                        "second" => true,
                        _ => {
                            return Err(fail(format!(
                                "lead must be first or second, found `{value}`"
                            )));
                        }
                    }
                }
                "follower" => {
                    start.follower = match value.split_whitespace().collect::<Vec<_>>()[..] {
                        ["none"] => Follower::None,
                        ["settled"] => Follower::Settled,
                        ["at", dx, dy] => Follower::At(
                            parse_int(dx).map_err(fail)?,
                            parse_int(dy).map_err(|m| (n, m))?,
                        ),
                        _ => {
                            return Err(fail(format!(
                                "follower must be none, settled or `at DX DY`, found `{value}`"
                            )));
                        }
                    }
                }
                "idle_frames" => {
                    start.idle_frames = value
                        .parse()
                        .map_err(|_| fail(format!("`{value}` is not a whole number")))?;
                    idle_given = true;
                }
                "held" => start.held = Buttons::parse_scenario(value).map_err(fail)?,
                "with" => {
                    if !ENTITY_KINDS.contains(&value) {
                        return Err(fail(format!(
                            "unknown kind `{value}`; expected one of {}",
                            ENTITY_KINDS.join(", ")
                        )));
                    }
                    start.with = Some(value.to_string());
                }
                _ => {
                    return Err(fail(format!(
                        "unknown start key `{key}`; expected one of mode, facing, vx, vy, state, height, lead, follower, idle_frames, held, with"
                    )));
                }
            }
        }

        let none = |m: String| (0, m);
        if start.mode != Mode::Airborne && seen.contains(&"height") {
            return Err(none("`height` is only for mode airborne".into()));
        }
        let moving = matches!(start.mode, Mode::Airborne | Mode::Riding);
        if !moving && seen.contains(&"vx") {
            return Err(none("`vx` is only for modes airborne and riding".into()));
        }
        if idle_given
            && !(start.mode == Mode::Grounded && (!state_given || start.state == State::Idle))
        {
            return Err(none(
                "`idle_frames` is only for mode grounded with state idle".into(),
            ));
        }
        match (start.mode, &start.with) {
            (Mode::Riding | Mode::Carrying, None) => {
                return Err(none("mode riding and carrying need `with`".into()));
            }
            (Mode::Riding | Mode::Carrying, Some(_)) => {}
            (_, Some(_)) => {
                return Err(none("`with` is only for modes riding and carrying".into()));
            }
            (_, None) => {}
        }
        if !state_given {
            start.state = match start.mode {
                Mode::Grounded | Mode::Carrying => State::Idle,
                Mode::Airborne if start.vy.is_negative() => State::Jump,
                Mode::Airborne => State::Fall,
                Mode::Swimming => State::Swim,
                Mode::Rope => State::Rope,
                Mode::Riding if start.vx != Decimal::ZERO => State::Walk,
                Mode::Riding => State::Idle,
            };
        }
        Ok(start)
    }

    /// Checks the objects the mode needs against the geometry.
    pub(crate) fn check(&self, geometry: &Geometry) -> Result<(), String> {
        let in_column_0 = |kind: &str| {
            geometry
                .entities
                .iter()
                .any(|e| e.kind == kind && e.column == 0)
        };
        match (self.mode, &self.with) {
            (Mode::Riding, Some(kind)) if !in_column_0(kind) => Err(format!(
                "mode riding with no matching entity in the geometry (an `entity {kind}` in column 0)"
            )),
            (Mode::Rope, _) if !in_column_0("rope") => Err(
                "mode rope with no matching entity in the geometry (an `entity rope` in column 0)"
                    .into(),
            ),
            (Mode::Swimming, _) if !geometry.water.iter().any(|w| w.columns.contains(0)) => {
                Err("mode swimming with no water over column 0".into())
            }
            _ => Ok(()),
        }
    }
}
