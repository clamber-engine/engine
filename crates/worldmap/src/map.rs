//! The world map format: TOML, like levels.

use std::collections::BTreeSet;
use std::fmt;

use clamber_level::{Fixed, Vec2};
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use toml::{Table, Value};

use crate::curve;
use crate::legs::{Dir, Finish, FinishTimes, Legs, NewGame};
use crate::progress::Progress;

pub const FORMAT: u32 = 1;

const MAX_COORD: f64 = (1u64 << 30) as f64;

#[derive(Clone, Debug, PartialEq)]
pub struct WorldMap {
    /// Names this map in progress (`id:node`) and in warps.
    pub id: String,
    pub name: String,
    pub size: Vec2,
    /// A level providing the map's visuals and, when free-roaming, the
    /// terrain to walk on.
    pub scene: Option<String>,
    pub mode: Mode,
    /// The node a new game starts on (path maps).
    pub start: Option<String>,
    pub nodes: Vec<Node>,
    pub paths: Vec<Path>,
    /// Animated elements drawn on the map.
    pub decorations: Vec<Decoration>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    /// The player moves between nodes along paths.
    Paths,
    /// The player moves freely over the scene's terrain and enters a node
    /// by touching it.
    Free(Free),
    /// The player moves between nodes along paths walked as straight legs
    /// from corner to corner, each direction taking the path the file
    /// gives it (see [`crate::LegWalker`]).
    Legs(Legs),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Free {
    pub start: Vec2,
    /// Pixels per frame.
    pub speed: Fixed,
    pub vehicle: Option<Vehicle>,
}

/// What the player travels in on a free-roaming map.
#[derive(Clone, Debug, PartialEq)]
pub struct Vehicle {
    pub sprite: String,
    pub animation: Option<String>,
    pub speed: Fixed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub at: Vec2,
    pub kind: NodeKind,
    /// `None` means always unlocked.
    pub unlock: Option<Condition>,
    /// Not shown at all until unlocked.
    pub hidden: bool,
    /// How close the player must come on a free-roaming map.
    pub radius: Fixed,
    pub sprite: Option<String>,
    pub animation: Option<String>,
    /// Legs maps, level nodes: the direction toward the next level, given
    /// as if pressed when the map appears after this level is finished
    /// ([`LegWalker::after_finish`](crate::LegWalker::after_finish)).
    pub onward: Option<Dir>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// Just a place paths meet.
    Waypoint,
    /// A level entrance. `exits` names the ways out of the level, which
    /// `cleared … by` conditions refer to.
    Level {
        level: String,
        exits: Vec<String>,
    },
    /// Leads to another map: into a regional map, or back out.
    Warp {
        map: String,
        node: String,
    },
    Save,
    /// Anything content defines: a shop, a character to talk to.
    Event {
        event: String,
        params: Table,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    /// Indices into [`WorldMap::nodes`].
    pub from: usize,
    pub to: usize,
    /// Points the curve passes through between the two nodes.
    pub via: Vec<Vec2>,
    pub unlock: Option<Condition>,
    pub hidden: bool,
    /// Can only be walked from `from` to `to`.
    pub one_way: bool,
    /// Legs maps: the directions that take this path from `from`.
    pub leave: Vec<Dir>,
    /// Legs maps: the directions that take this path back from `to`.
    pub back: Vec<Dir>,
    /// Legs maps: the corners walking back passes, when they aren't
    /// `via` reversed.
    pub back_via: Option<Vec<Vec2>>,
    /// Legs maps: walking back from `to` also needs this, so a path can
    /// be open one way before the other.
    pub back_unlock: Option<Condition>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decoration {
    pub sprite: String,
    pub animation: Option<String>,
    pub at: Vec2,
}

/// When something on the map is unlocked, in terms of [`Progress`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    /// A level (`map:node`) cleared, by a particular exit if given.
    Cleared {
        place: String,
        by: Option<String>,
    },
    Flag(String),
    Counter {
        name: String,
        at_least: u32,
    },
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

impl Condition {
    pub fn holds(&self, p: &Progress) -> bool {
        match self {
            Condition::Cleared { place, by: None } => p.is_cleared(place),
            Condition::Cleared {
                place,
                by: Some(exit),
            } => p.cleared_by(place, exit),
            Condition::Flag(f) => p.flags.contains(f),
            Condition::Counter { name, at_least } => p.counter(name) >= *at_least,
            Condition::All(cs) => cs.iter().all(|c| c.holds(p)),
            Condition::Any(cs) => cs.iter().any(|c| c.holds(p)),
            Condition::Not(c) => !c.holds(p),
        }
    }
}

/// A way out of a node along a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    pub path: usize,
    /// The node at the other end.
    pub to: usize,
    /// The curve to walk, starting at this node.
    pub line: Vec<Vec2>,
}

impl Exit {
    /// The direction the path leaves in.
    pub fn direction(&self) -> Vec2 {
        let (a, b) = (self.line[0], self.line[1]);
        Vec2::new(b.x - a.x, b.y - a.y)
    }
}

impl WorldMap {
    pub fn node(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// How progress names a node of this map: `map:node`.
    pub fn place(&self, node: usize) -> String {
        format!("{}:{}", self.id, self.nodes[node].id)
    }

    fn path_key(&self, path: usize) -> String {
        let p = &self.paths[path];
        format!(
            "{}:{}>{}",
            self.id, self.nodes[p.from].id, self.nodes[p.to].id
        )
    }

    pub fn node_unlocked(&self, node: usize, p: &Progress) -> bool {
        self.nodes[node].unlock.as_ref().is_none_or(|c| c.holds(p))
    }

    pub fn node_visible(&self, node: usize, p: &Progress) -> bool {
        !self.nodes[node].hidden || self.node_unlocked(node, p)
    }

    /// A path can be walked when it and the nodes at both ends are
    /// unlocked.
    pub fn path_open(&self, path: usize, p: &Progress) -> bool {
        let path_ = &self.paths[path];
        path_.unlock.as_ref().is_none_or(|c| c.holds(p))
            && self.node_unlocked(path_.from, p)
            && self.node_unlocked(path_.to, p)
    }

    /// Whether a legs map's path can be walked back, from `to` to `from`.
    pub fn path_open_back(&self, path: usize, p: &Progress) -> bool {
        self.path_open(path, p)
            && self.paths[path]
                .back_unlock
                .as_ref()
                .is_none_or(|c| c.holds(p))
    }

    pub fn path_visible(&self, path: usize, p: &Progress) -> bool {
        !self.paths[path].hidden || self.path_open(path, p)
    }

    /// The curve of a path, from its `from` node to its `to` node.
    pub fn path_line(&self, path: usize) -> Vec<Vec2> {
        let p = &self.paths[path];
        let mut points = vec![self.nodes[p.from].at];
        points.extend_from_slice(&p.via);
        points.push(self.nodes[p.to].at);
        curve::smooth(&points)
    }

    /// Open paths leading out of `node`, in file order.
    pub fn exits(&self, node: usize, p: &Progress) -> Vec<Exit> {
        (0..self.paths.len())
            .filter(|&i| self.path_open(i, p))
            .filter_map(|i| {
                let path = &self.paths[i];
                let mut line = self.path_line(i);
                if path.from == node {
                    Some(Exit {
                        path: i,
                        to: path.to,
                        line,
                    })
                } else if path.to == node && !path.one_way {
                    line.reverse();
                    Some(Exit {
                        path: i,
                        to: path.from,
                        line,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// The open path from `node` that best matches the direction pressed,
    /// if any leaves within 90° of it.
    pub fn exit_towards(&self, node: usize, direction: Vec2, p: &Progress) -> Option<Exit> {
        let mut best: Option<(Fixed, Exit)> = None;
        for exit in self.exits(node, p) {
            let Some(cos) = curve::cosine(direction, exit.direction()) else {
                continue;
            };
            if cos > Fixed::ZERO && best.as_ref().is_none_or(|(b, _)| cos > *b) {
                best = Some((cos, exit));
            }
        }
        best.map(|(_, e)| e)
    }

    /// On a legs map, the open path that `dir` takes from `node`: the
    /// path, the node at its other end and the points of the walk (the
    /// corners, then the other node).
    pub fn leg_route(
        &self,
        node: usize,
        dir: Dir,
        p: &Progress,
    ) -> Option<(usize, usize, Vec<Vec2>)> {
        (0..self.paths.len()).find_map(|i| {
            let path = &self.paths[i];
            let (to, mut points) = if path.from == node && path.leave.contains(&dir) {
                if !self.path_open(i, p) {
                    return None;
                }
                (path.to, path.via.clone())
            } else if path.to == node && path.back.contains(&dir) {
                if !self.path_open_back(i, p) {
                    return None;
                }
                let mut back = path.back_via.clone().unwrap_or_else(|| path.via.clone());
                if path.back_via.is_none() {
                    back.reverse();
                }
                (path.from, back)
            } else {
                return None;
            };
            points.push(self.nodes[to].at);
            Some((i, to, points))
        })
    }

    /// The nearest unlocked node whose radius `at` is within, for
    /// free-roaming maps.
    pub fn node_touching(&self, at: Vec2, p: &Progress) -> Option<usize> {
        (0..self.nodes.len())
            .filter(|&i| self.node_unlocked(i, p))
            .map(|i| (curve::distance(at, self.nodes[i].at), i))
            .filter(|&(d, i)| d <= self.nodes[i].radius)
            .min()
            .map(|(_, i)| i)
    }

    /// Hidden nodes and paths that are visible now but weren't shown
    /// before, as progress keys. Records them as revealed, so each reveal
    /// is reported once and the game can animate it.
    pub fn reveal(&self, p: &mut Progress) -> Vec<String> {
        let mut new = Vec::new();
        for i in 0..self.nodes.len() {
            if self.nodes[i].hidden && self.node_visible(i, p) {
                new.push(self.place(i));
            }
        }
        for i in 0..self.paths.len() {
            if self.paths[i].hidden && self.path_visible(i, p) {
                new.push(self.path_key(i));
            }
        }
        new.retain(|k| p.revealed.insert(k.clone()));
        new
    }

    pub fn load(path: &std::path::Path) -> Result<WorldMap, MapError> {
        let origin = path.display().to_string();
        let src = std::fs::read_to_string(path).map_err(|e| MapError::new(&origin, e))?;
        WorldMap::from_toml_str(&src, &origin)
    }

    pub fn from_toml_str(src: &str, origin: &str) -> Result<WorldMap, MapError> {
        let file: File =
            toml::from_str(src).map_err(|e| MapError::new(origin, e.to_string().trim_end()))?;
        Reader { origin }.map(file)
    }
}

/// Checks that every warp in a set of maps leads to a node that exists,
/// and that map ids are unique. Returns each problem found.
pub fn check_warps(maps: &[WorldMap]) -> Vec<String> {
    let mut problems = Vec::new();
    for (i, m) in maps.iter().enumerate() {
        if maps[..i].iter().any(|other| other.id == m.id) {
            problems.push(format!("map id `{}` is used twice", m.id));
        }
    }
    for m in maps {
        for n in &m.nodes {
            let NodeKind::Warp { map, node } = &n.kind else {
                continue;
            };
            match maps.iter().find(|t| &t.id == map) {
                None => problems.push(format!("{}:{} warps to unknown map `{map}`", m.id, n.id)),
                Some(t) if t.node(node).is_none() => problems.push(format!(
                    "{}:{} warps to `{map}:{node}`, which doesn't exist",
                    m.id, n.id
                )),
                Some(_) => {}
            }
        }
    }
    problems
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapError {
    pub origin: String,
    pub message: String,
}

impl MapError {
    fn new(origin: &str, message: impl fmt::Display) -> MapError {
        MapError {
            origin: origin.to_string(),
            message: message.to_string(),
        }
    }
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for MapError {}

/// A number within ±2^30.
#[derive(Clone, Copy)]
struct N(Fixed);

impl<'de> Deserialize<'de> for N {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<N, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = N;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a number")
            }
            fn visit_i64<E: de::Error>(self, n: i64) -> Result<N, E> {
                self.visit_f64(n as f64)
            }
            fn visit_u64<E: de::Error>(self, n: u64) -> Result<N, E> {
                self.visit_f64(n as f64)
            }
            fn visit_f64<E: de::Error>(self, x: f64) -> Result<N, E> {
                if x.is_nan() || x.abs() > MAX_COORD {
                    return Err(E::custom(format!("{x} is out of range (±2^30)")));
                }
                Ok(N(Fixed::from_f64(x).expect("within range")))
            }
        }
        d.deserialize_any(V)
    }
}

fn v2([x, y]: [N; 2]) -> Vec2 {
    Vec2::new(x.0, y.0)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    format: u32,
    id: String,
    #[serde(default)]
    name: String,
    size: [N; 2],
    scene: Option<String>,
    #[serde(default = "paths")]
    mode: String,
    start: Option<String>,
    free: Option<FreeFile>,
    legs: Option<LegsFile>,
    #[serde(default)]
    nodes: Vec<NodeFile>,
    #[serde(default, rename = "paths")]
    path_list: Vec<PathFile>,
    #[serde(default)]
    decorations: Vec<DecorationFile>,
}

fn paths() -> String {
    "paths".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FreeFile {
    start: [N; 2],
    speed: N,
    vehicle: Option<VehicleFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegsFile {
    leg_frames: u32,
    remember: u32,
    page_width: u32,
    page_wait: u32,
    appear_offset: [N; 2],
    appear_wait: u32,
    #[serde(default)]
    y_up: bool,
    fade_step: u32,
    finish: Option<FinishFile>,
    new_game: Option<NewGameFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewGameFile {
    walk: u32,
    toward: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishFile {
    first: FinishTimesFile,
    again: FinishTimesFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishTimesFile {
    black: u32,
    still: Option<[u32; 2]>,
    gate: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VehicleFile {
    sprite: String,
    animation: Option<String>,
    speed: N,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeFile {
    id: String,
    at: [N; 2],
    kind: String,
    level: Option<String>,
    exits: Option<Vec<String>>,
    map: Option<String>,
    node: Option<String>,
    event: Option<String>,
    params: Option<Table>,
    unlock: Option<Value>,
    #[serde(default)]
    hidden: bool,
    radius: Option<N>,
    sprite: Option<String>,
    animation: Option<String>,
    onward: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathFile {
    from: String,
    to: String,
    #[serde(default)]
    via: Vec<[N; 2]>,
    unlock: Option<Value>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    one_way: bool,
    #[serde(default)]
    leave: Vec<String>,
    #[serde(default)]
    back: Vec<String>,
    back_via: Option<Vec<[N; 2]>>,
    back_unlock: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecorationFile {
    sprite: String,
    animation: Option<String>,
    at: [N; 2],
}

fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

struct Reader<'a> {
    origin: &'a str,
}

/// Node ids and the exits of level nodes, for checking conditions.
struct Places<'a> {
    map: &'a str,
    levels: Vec<(&'a str, &'a [String])>,
    all: BTreeSet<&'a str>,
}

impl Reader<'_> {
    fn err(&self, path: &str, message: impl fmt::Display) -> MapError {
        MapError::new(self.origin, format!("{path}: {message}"))
    }

    fn map(&self, f: File) -> Result<WorldMap, MapError> {
        if f.format != FORMAT {
            return Err(self.err(
                "format",
                format!(
                    "version {} isn't supported; this engine reads {FORMAT}",
                    f.format
                ),
            ));
        }
        if !is_name(&f.id) {
            return Err(self.err("id", "use lowercase letters, digits, - and _"));
        }
        let size = v2(f.size);
        if size.x <= Fixed::ZERO || size.y <= Fixed::ZERO {
            return Err(self.err("size", "must be positive"));
        }

        // node ids and kinds first, so conditions and paths can refer to them
        let mut ids = BTreeSet::new();
        let mut kinds = Vec::new();
        for (i, n) in f.nodes.iter().enumerate() {
            let at = format!("nodes[{i}]");
            if !is_name(&n.id) {
                return Err(self.err(
                    &format!("{at}.id"),
                    "use lowercase letters, digits, - and _",
                ));
            }
            if !ids.insert(n.id.as_str()) {
                return Err(self.err(&format!("{at}.id"), format!("`{}` is used twice", n.id)));
            }
            kinds.push(self.kind(&at, n)?);
        }
        let levels = f
            .nodes
            .iter()
            .zip(&kinds)
            .filter_map(|(n, k)| match k {
                NodeKind::Level { exits, .. } => Some((n.id.as_str(), exits.as_slice())),
                _ => None,
            })
            .collect();
        let places = Places {
            map: &f.id,
            levels,
            all: ids,
        };

        let mut nodes = Vec::new();
        for (i, (n, kind)) in f.nodes.iter().zip(kinds.iter().cloned()).enumerate() {
            let at = format!("nodes[{i}]");
            let radius = n.radius.map_or(Fixed::from_int(12), |r| r.0);
            if radius <= Fixed::ZERO {
                return Err(self.err(&format!("{at}.radius"), "must be positive"));
            }
            nodes.push(Node {
                id: n.id.clone(),
                at: v2(n.at),
                kind,
                unlock: n
                    .unlock
                    .as_ref()
                    .map(|c| self.condition(&format!("{at}.unlock"), c, &places))
                    .transpose()?,
                hidden: n.hidden,
                radius,
                sprite: n.sprite.clone(),
                animation: n.animation.clone(),
                onward: n
                    .onward
                    .as_ref()
                    .map(|d| self.dir(&format!("{at}.onward"), d))
                    .transpose()?,
            });
        }

        let index = |at: &str, id: &str| {
            nodes
                .iter()
                .position(|n| n.id == id)
                .ok_or_else(|| self.err(at, format!("no node `{id}`")))
        };
        let mut seen = BTreeSet::new();
        let mut paths = Vec::new();
        for (i, p) in f.path_list.iter().enumerate() {
            let at = format!("paths[{i}]");
            let (from, to) = (
                index(&format!("{at}.from"), &p.from)?,
                index(&format!("{at}.to"), &p.to)?,
            );
            if from == to {
                return Err(self.err(&at, "a path needs two different nodes"));
            }
            if !seen.insert((from.min(to), from.max(to))) {
                return Err(self.err(
                    &at,
                    format!("`{}` and `{}` are already joined", p.from, p.to),
                ));
            }
            paths.push(Path {
                from,
                to,
                via: p.via.iter().copied().map(v2).collect(),
                unlock: p
                    .unlock
                    .as_ref()
                    .map(|c| self.condition(&format!("{at}.unlock"), c, &places))
                    .transpose()?,
                hidden: p.hidden,
                one_way: p.one_way,
                leave: self.dirs(&format!("{at}.leave"), &p.leave)?,
                back: self.dirs(&format!("{at}.back"), &p.back)?,
                back_via: p
                    .back_via
                    .as_ref()
                    .map(|v| v.iter().copied().map(v2).collect()),
                back_unlock: p
                    .back_unlock
                    .as_ref()
                    .map(|c| self.condition(&format!("{at}.back_unlock"), c, &places))
                    .transpose()?,
            });
        }

        if f.legs.is_some() && f.mode != "legs" {
            return Err(self.err("legs", "only for maps with mode = \"legs\""));
        }
        let mode = match (f.mode.as_str(), f.free) {
            ("paths", None) => Mode::Paths,
            ("legs", None) => {
                let legs = f
                    .legs
                    .ok_or_else(|| self.err("legs", "a legs map needs a [legs] table"))?;
                Mode::Legs(self.legs(legs)?)
            }
            ("free", Some(free)) => Mode::Free(Free {
                start: v2(free.start),
                speed: free.speed.0,
                vehicle: free.vehicle.map(|v| Vehicle {
                    sprite: v.sprite,
                    animation: v.animation,
                    speed: v.speed.0,
                }),
            }),
            ("paths" | "legs", Some(_)) => {
                return Err(self.err("free", "only for maps with mode = \"free\""));
            }
            ("free", None) => {
                return Err(self.err("free", "a free-roaming map needs a [free] table"));
            }
            (m, _) => {
                return Err(self.err(
                    "mode",
                    format!("`{m}`; use \"paths\", \"legs\" or \"free\""),
                ));
            }
        };
        if matches!(mode, Mode::Legs(_)) {
            self.check_legs(&paths, &nodes)?;
        } else if let Some(i) = paths.iter().position(|p| {
            !p.leave.is_empty()
                || !p.back.is_empty()
                || p.back_via.is_some()
                || p.back_unlock.is_some()
        }) {
            return Err(self.err(
                &format!("paths[{i}]"),
                "leave, back, back_via and back_unlock are only for maps with mode = \"legs\"",
            ));
        } else if let Some(i) = nodes.iter().position(|n| n.onward.is_some()) {
            return Err(self.err(
                &format!("nodes[{i}].onward"),
                "only for maps with mode = \"legs\"",
            ));
        }
        if let Some(start) = &f.start {
            index("start", start)?;
        } else if !matches!(mode, Mode::Free(_)) && !nodes.is_empty() {
            return Err(self.err("start", "a path map needs a start node"));
        }

        Ok(WorldMap {
            id: f.id,
            name: f.name,
            size,
            scene: f.scene,
            mode,
            start: f.start,
            nodes,
            paths,
            decorations: f
                .decorations
                .into_iter()
                .map(|d| Decoration {
                    sprite: d.sprite,
                    animation: d.animation,
                    at: v2(d.at),
                })
                .collect(),
        })
    }

    fn dir(&self, at: &str, name: &str) -> Result<Dir, MapError> {
        Dir::parse(name)
            .ok_or_else(|| self.err(at, format!("`{name}`; use right, left, up or down")))
    }

    fn dirs(&self, at: &str, names: &[String]) -> Result<Vec<Dir>, MapError> {
        let mut dirs = Vec::new();
        for name in names {
            let dir = self.dir(at, name)?;
            if dirs.contains(&dir) {
                return Err(self.err(at, format!("`{name}` is listed twice")));
            }
            dirs.push(dir);
        }
        Ok(dirs)
    }

    fn legs(&self, l: LegsFile) -> Result<Legs, MapError> {
        // a power of two divides a whole number of pixels exactly
        if !l.leg_frames.is_power_of_two() || l.leg_frames > 1 << 16 {
            return Err(self.err("legs.leg_frames", "use a power of two up to 65536"));
        }
        for (field, n) in [
            ("remember", l.remember),
            ("page_width", l.page_width),
            ("fade_step", l.fade_step),
        ] {
            if n == 0 {
                return Err(self.err(&format!("legs.{field}"), "must be positive"));
            }
        }
        Ok(Legs {
            leg_frames: l.leg_frames,
            remember: l.remember,
            page_width: l.page_width,
            page_wait: l.page_wait,
            appear_offset: v2(l.appear_offset),
            appear_wait: l.appear_wait,
            y_up: l.y_up,
            fade_step: l.fade_step,
            finish: match l.finish {
                Some(f) => Some(Finish {
                    first: self.finish_times("first", f.first)?,
                    again: self.finish_times("again", f.again)?,
                }),
                None => None,
            },
            new_game: match l.new_game {
                Some(n) => Some(NewGame {
                    walk: n.walk,
                    toward: self.dir("legs.new_game.toward", &n.toward)?,
                }),
                None => None,
            },
        })
    }

    fn finish_times(&self, name: &str, t: FinishTimesFile) -> Result<FinishTimes, MapError> {
        if let Some([from, n]) = t.still
            && u64::from(from) + u64::from(n) > u64::from(t.gate)
        {
            return Err(self.err(
                &format!("legs.finish.{name}.still"),
                "the still frames must end by the gate",
            ));
        }
        Ok(FinishTimes {
            black: t.black,
            still: t.still.map(|[from, n]| (from, n)),
            gate: t.gate,
        })
    }

    /// Every path of a legs map is taken by some direction, and no
    /// direction takes two paths from the same node.
    fn check_legs(&self, paths: &[Path], nodes: &[Node]) -> Result<(), MapError> {
        if let Some(i) = nodes
            .iter()
            .position(|n| n.onward.is_some() && !matches!(n.kind, NodeKind::Level { .. }))
        {
            return Err(self.err(&format!("nodes[{i}].onward"), "only for level nodes"));
        }
        let mut taken = BTreeSet::new();
        for (i, p) in paths.iter().enumerate() {
            let at = format!("paths[{i}]");
            if p.leave.is_empty() && p.back.is_empty() {
                return Err(self.err(&at, "a legs map's path needs leave or back directions"));
            }
            if p.one_way && (!p.back.is_empty() || p.back_via.is_some() || p.back_unlock.is_some())
            {
                return Err(self.err(
                    &at,
                    "a one-way path can't have back, back_via or back_unlock",
                ));
            }
            for (node, dirs) in [(p.from, &p.leave), (p.to, &p.back)] {
                for &dir in dirs {
                    if !taken.insert((node, dir)) {
                        return Err(self.err(
                            &at,
                            format!(
                                "{} from `{}` already takes another path",
                                dir.name(),
                                nodes[node].id
                            ),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn kind(&self, at: &str, n: &NodeFile) -> Result<NodeKind, MapError> {
        // each kind takes only its own fields
        let fields = [
            ("level", n.level.is_some()),
            ("exits", n.exits.is_some()),
            ("map", n.map.is_some()),
            ("node", n.node.is_some()),
            ("event", n.event.is_some()),
            ("params", n.params.is_some()),
        ];
        let allowed: &[&str] = match n.kind.as_str() {
            "level" => &["level", "exits"],
            "warp" => &["map", "node"],
            "event" => &["event", "params"],
            "waypoint" | "save" => &[],
            k => {
                return Err(self.err(
                    &format!("{at}.kind"),
                    format!("`{k}`; use level, warp, save, event or waypoint"),
                ));
            }
        };
        if let Some((field, _)) = fields.iter().find(|(f, set)| *set && !allowed.contains(f)) {
            return Err(self.err(
                &format!("{at}.{field}"),
                format!("not used by {} nodes", n.kind),
            ));
        }
        let need = |v: &Option<String>, field: &str| {
            v.clone()
                .ok_or_else(|| self.err(at, format!("{} nodes need `{field}`", n.kind)))
        };
        Ok(match n.kind.as_str() {
            "level" => {
                let exits = n.exits.clone().unwrap_or_else(|| vec!["normal".into()]);
                if exits.is_empty() || !exits.iter().all(|e| is_name(e)) {
                    return Err(self.err(
                        &format!("{at}.exits"),
                        "list exit names: lowercase letters, digits, - and _",
                    ));
                }
                NodeKind::Level {
                    level: need(&n.level, "level")?,
                    exits,
                }
            }
            "warp" => NodeKind::Warp {
                map: need(&n.map, "map")?,
                node: need(&n.node, "node")?,
            },
            "event" => NodeKind::Event {
                event: need(&n.event, "event")?,
                params: n.params.clone().unwrap_or_default(),
            },
            "save" => NodeKind::Save,
            _ => NodeKind::Waypoint,
        })
    }

    fn condition(&self, at: &str, v: &Value, places: &Places) -> Result<Condition, MapError> {
        let t = v
            .as_table()
            .ok_or_else(|| self.err(at, "expected a table such as { cleared = \"node\" }"))?;
        let keys: Vec<&str> = t.keys().map(String::as_str).collect();
        let string = |k: &str| {
            t[k].as_str()
                .map(str::to_string)
                .ok_or_else(|| self.err(&format!("{at}.{k}"), "expected a string"))
        };
        let list = |k: &str| -> Result<Vec<Condition>, MapError> {
            let items = t[k]
                .as_array()
                .ok_or_else(|| self.err(&format!("{at}.{k}"), "expected a list of conditions"))?;
            items
                .iter()
                .enumerate()
                .map(|(i, c)| self.condition(&format!("{at}.{k}[{i}]"), c, places))
                .collect()
        };
        Ok(match keys.as_slice() {
            ["cleared"] | ["cleared", "by"] | ["by", "cleared"] => {
                let target = string("cleared")?;
                let by = t.contains_key("by").then(|| string("by")).transpose()?;
                let place = if target.contains(':') {
                    target
                } else {
                    let Some((_, exits)) = places.levels.iter().find(|(id, _)| *id == target)
                    else {
                        let why = if places.all.contains(target.as_str()) {
                            "isn't a level node"
                        } else {
                            "no such node"
                        };
                        return Err(
                            self.err(&format!("{at}.cleared"), format!("`{target}`: {why}"))
                        );
                    };
                    if let Some(exit) = &by
                        && !exits.contains(exit)
                    {
                        return Err(self.err(
                            &format!("{at}.by"),
                            format!("`{target}` has no exit `{exit}`"),
                        ));
                    }
                    format!("{}:{target}", places.map)
                };
                Condition::Cleared { place, by }
            }
            ["flag"] => Condition::Flag(string("flag")?),
            ["counter", "at_least"] | ["at_least", "counter"] => {
                let n = t["at_least"]
                    .as_integer()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| {
                        self.err(&format!("{at}.at_least"), "expected a whole number")
                    })?;
                Condition::Counter {
                    name: string("counter")?,
                    at_least: n,
                }
            }
            ["all"] => Condition::All(list("all")?),
            ["any"] => Condition::Any(list("any")?),
            ["not"] => Condition::Not(Box::new(self.condition(
                &format!("{at}.not"),
                &t["not"],
                places,
            )?)),
            _ => {
                return Err(self.err(
                    at,
                    "use one of: cleared (with optional by), flag, counter with at_least, all, any, not",
                ));
            }
        })
    }
}
