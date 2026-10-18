//! The level map's points and routes (`spec/formats/level-map.md`):
//! where each stop and corner is, which stop each direction leads to,
//! and the points a walk between two stops passes through.
//!
//! For titles the spec covers, also what kind of place each stop is, how
//! a new game begins and which paths are open then ("Kinds of stops", "A
//! new game", "Progress"), written into a pack as a world map by
//! [`LevelMapDecoder`]. How the lead walks a route
//! (`behavior/level-map.md`) is up to the world map's walker.

use crate::{Data, Decoder, FIRST_LEVEL, Image, Known, LEVELS, PackWriter, locate};

/// The folder in a pack that holds its world maps.
pub const MAPS: &str = "maps";

/// The id, and file name without `.map`, of an imported title's level
/// map.
pub const LEVEL_MAP: &str = "level-map";

/// The lowest corner id. Ids from here to [`LAST_CORNER`] are corners;
/// every other id but 0 is a stop.
pub const FIRST_CORNER: u8 = 112;

/// The highest corner id.
pub const LAST_CORNER: u8 = 143;

/// Bytes in each of the seven point tables.
pub const TABLE_SIZE: usize = 256;

/// Bytes in a corner record.
pub const CORNER_SIZE: usize = 4;

/// How far right of a stop's position its appearance point is.
pub const APPEARANCE_OFFSET: u32 = 8;

/// A direction that can start a walk from a stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Right,
    Left,
    Up,
    Down,
}

impl Direction {
    /// In the order of their point tables (4 to 7).
    pub const ALL: [Direction; 4] = [
        Direction::Right,
        Direction::Left,
        Direction::Up,
        Direction::Down,
    ];
}

/// Whether `id` is a corner id.
pub fn is_corner(id: u8) -> bool {
    (FIRST_CORNER..=LAST_CORNER).contains(&id)
}

/// How many corner records an image of `title` has, if the spec says.
pub fn corner_count(title: &str) -> Option<usize> {
    match title {
        "title-1" => Some(4),
        _ => None,
    }
}

/// What a press at a link point does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// Moves the lead to this stop.
    Stop(u8),
    /// Begins the level of this level stop.
    Level(u8),
    /// The spec doesn't say.
    Unknown,
}

/// What kind of place a stop is, and so what a press there does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopKind {
    /// Begins a level.
    Level,
    Link(Link),
    /// Opens a screen that isn't a level, of the first, second or third
    /// kind.
    Place(u8),
}

/// What the spec says about a title's stops and progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Where a new game begins.
    pub start: u8,
    /// On a new game the lead walks this way from the start, with no
    /// input, on frame [`Rules::walk`].
    pub start_walk: Direction,
    pub walk: u32,
    /// The stop that begins the first level.
    pub first_level: u8,
    /// The first level's first finish opens the path this way from its
    /// stop, both ways; it is also the direction given on the map after
    /// that level is finished.
    pub onward: Direction,
    /// The paths open at a new game, (from, to); only that way.
    pub open: &'static [(u8, u8)],
}

impl Rules {
    /// The spec's rules for `title`, if it has them.
    pub fn of(title: &str) -> Option<Rules> {
        match title {
            "title-1" => Some(Rules {
                start: 235,
                start_walk: Direction::Right,
                walk: 64,
                first_level: 22,
                onward: Direction::Down,
                open: &[(235, 236), (20, 240), (240, 20), (66, 245), (245, 66)],
            }),
            _ => None,
        }
    }

    /// What kind of place stop `id` is.
    pub fn kind(&self, id: u8) -> StopKind {
        match id {
            230 => StopKind::Link(Link::Stop(48)),
            231 => StopKind::Link(Link::Stop(64)),
            232 => StopKind::Link(Link::Stop(36)),
            233 => StopKind::Link(Link::Stop(165)),
            234 => StopKind::Link(Link::Level(1)),
            236 => StopKind::Link(Link::Stop(22)),
            237 => StopKind::Link(Link::Stop(217)),
            235 => StopKind::Link(Link::Unknown),
            238..=243 => StopKind::Place(1),
            244..=249 => StopKind::Place(2),
            250..=255 => StopKind::Place(3),
            _ => StopKind::Level,
        }
    }
}

/// When a walk from one stop to another is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Open {
    Always,
    /// Once the level of this stop has been finished.
    Finished(u8),
    /// Not covered by the spec, so never yet.
    Unknown,
}

/// A map position in whole pixels: `x` across the map, `h` its height.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: u32,
    pub h: u32,
}

/// The level map's point tables and corner records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelMap {
    /// The seven point tables, one after the other.
    tables: Vec<u8>,
    /// Record `k` belongs to corner id `FIRST_CORNER + k`.
    corners: Vec<[u8; CORNER_SIZE]>,
}

impl LevelMap {
    /// Reads the seven point tables from the start of `points` and
    /// `count` corner records from the start of `corners`.
    pub fn read(points: &[u8], corners: &[u8], count: usize) -> Result<LevelMap, String> {
        let corner_ids = usize::from(LAST_CORNER - FIRST_CORNER) + 1;
        if count > corner_ids {
            return Err(format!(
                "{count} corner records, but there are only {corner_ids} corner ids"
            ));
        }
        let tables = points
            .get(..7 * TABLE_SIZE)
            .ok_or("the level map's point tables run past the end of the image")?;
        let records = corners
            .get(..count * CORNER_SIZE)
            .ok_or("the level map's corner records run past the end of the image")?;
        Ok(LevelMap {
            tables: tables.to_vec(),
            corners: records.as_chunks::<CORNER_SIZE>().0.to_vec(),
        })
    }

    /// Reads the level map of an image of `title`, where the spec says
    /// where it is.
    pub fn from_image(image: &Image, title: &str) -> Result<LevelMap, String> {
        let at = |data| {
            let location = locate(title, data)
                .ok_or_else(|| format!("the spec doesn't say where {title}'s level map is"))?;
            image
                .at(location)
                .ok_or_else(|| "the level map isn't in the image".to_string())
        };
        let count = corner_count(title)
            .ok_or_else(|| format!("the spec doesn't say how many corners {title} has"))?;
        LevelMap::read(at(Data::MapPoints)?, at(Data::MapCorners)?, count)
    }

    /// Byte `id` of point table `table` (1 to 7).
    fn table(&self, table: usize, id: u8) -> u8 {
        self.tables[(table - 1) * TABLE_SIZE + usize::from(id)]
    }

    /// The position of `id`.
    pub fn position(&self, id: u8) -> Point {
        Point {
            x: 256 * u32::from(self.table(1, id)) + u32::from(self.table(2, id)),
            h: u32::from(self.table(3, id)),
        }
    }

    /// Where the lead stands at `stop` when the map appears.
    pub fn appearance(&self, stop: u8) -> Point {
        let Point { x, h } = self.position(stop);
        Point {
            x: x + APPEARANCE_OFFSET,
            h,
        }
    }

    /// The stop that `direction` leads to from `stop`, or `None` if it
    /// takes no path from there.
    pub fn neighbour(&self, stop: u8, direction: Direction) -> Option<u8> {
        let table = 4 + Direction::ALL.iter().position(|d| *d == direction).unwrap();
        Some(self.table(table, stop)).filter(|&n| n != 0)
    }

    /// The ids with a position or a neighbour that isn't 0, in order.
    pub fn ids(&self) -> impl Iterator<Item = u8> + '_ {
        (1..=255).filter(|&id| (1..=7).any(|t| self.table(t, id) != 0))
    }

    /// The corner record of `id`, if it is a corner with one.
    fn record(&self, id: u8) -> Option<[u8; CORNER_SIZE]> {
        self.corners
            .get(usize::from(id.checked_sub(FIRST_CORNER)?))
            .copied()
            .filter(|_| is_corner(id))
    }

    /// The route of a walk from `stop` in `direction`: the ids whose
    /// positions are the walk's points, in order, ending with the
    /// neighbour. `None` if the direction takes no path. An error for a
    /// route the spec doesn't expect: more than one corner fitting at
    /// the start, a corner without a way on, or a point visited twice.
    pub fn route(&self, stop: u8, direction: Direction) -> Result<Option<Vec<u8>>, String> {
        let Some(to) = self.neighbour(stop, direction) else {
            return Ok(None);
        };
        let next_toward = |record: [u8; CORNER_SIZE], destination: u8| {
            if record[0] == destination {
                Some(record[1])
            } else if record[2] == destination {
                Some(record[3])
            } else {
                None
            }
        };
        let mut first = (FIRST_CORNER..=LAST_CORNER).filter(|&corner| {
            self.record(corner).is_some_and(|record| {
                next_toward(record, stop) == Some(stop) && next_toward(record, to).is_some()
            })
        });
        let start = first.next();
        if let Some(other) = first.next() {
            return Err(format!(
                "corners {} and {other} both start the walk from stop {stop} to {to}",
                start.unwrap()
            ));
        }
        let mut route = vec![start.unwrap_or(to)];
        while let Some(&last) = route.last().filter(|&&id| is_corner(id)) {
            let next = self
                .record(last)
                .and_then(|record| next_toward(record, to))
                .ok_or_else(|| {
                    format!("the walk from stop {stop} to {to} has no way on from corner {last}")
                })?;
            if route.contains(&next) || next == stop {
                return Err(format!(
                    "the walk from stop {stop} to {to} comes back to point {next}"
                ));
            }
            route.push(next);
        }
        if route.last() != Some(&to) {
            return Err(format!(
                "the walk from stop {stop} to {to} doesn't end there"
            ));
        }
        Ok(Some(route))
    }

    /// The map as an engine world map (`.map`) in legs mode, starting at
    /// stop `start`, walked as `behavior/level-map.md` says. Each pair of
    /// neighbouring stops is one path, taken by the directions the point
    /// tables give from either end. Every stop is a waypoint and every
    /// path is open, as for a title whose stops and progress the spec
    /// doesn't cover.
    pub fn to_world_map(&self, id: &str, start: u8) -> Result<String, String> {
        self.world_map(id, start, None)
    }

    /// The map as an engine world map with `rules`: a new game starts at
    /// the start point and walks off on its own, link points are warps,
    /// the first level's stop is the level [`crate::FIRST_LEVEL`], and
    /// paths are open as at a new game, the first level's finish opening
    /// its onward path. Paths whose opening the spec doesn't cover open
    /// with a flag named after them, `opened:stop-A:stop-B`, which nothing
    /// sets yet. Other level stops and places, whose levels and screens
    /// the spec doesn't cover, are `level-stop` and `place` events.
    pub fn to_title_world_map(&self, id: &str, rules: &Rules) -> Result<String, String> {
        self.world_map(id, rules.start, Some(rules))
    }

    fn world_map(&self, id: &str, start: u8, rules: Option<&Rules>) -> Result<String, String> {
        use std::collections::BTreeMap;
        use std::fmt::Write;

        let stops: Vec<u8> = self.ids().filter(|&id| !is_corner(id)).collect();
        if !stops.contains(&start) {
            return Err(format!("the map has no stop {start}"));
        }
        let onward = match rules {
            Some(r) => {
                let to = self.neighbour(r.first_level, r.onward).ok_or_else(|| {
                    format!(
                        "the first level's stop {} has no {:?} neighbour",
                        r.first_level, r.onward
                    )
                })?;
                Some((r.first_level, to))
            }
            None => None,
        };
        let open = |from: u8, to: u8| match rules {
            None => Open::Always,
            Some(r) if r.open.contains(&(from, to)) => Open::Always,
            Some(_)
                if onward.is_some_and(|(a, b)| (a, b) == (from, to) || (b, a) == (from, to)) =>
            {
                Open::Finished(onward.unwrap().0)
            }
            Some(_) => Open::Unknown,
        };
        // (lower stop, higher stop) -> each end's directions and route
        type Way = (Vec<Direction>, Option<Vec<u8>>);
        let mut pairs: BTreeMap<(u8, u8), [Way; 2]> = BTreeMap::new();
        for &stop in &stops {
            for direction in Direction::ALL {
                let Some(route) = self.route(stop, direction)? else {
                    continue;
                };
                let to = *route.last().unwrap();
                let end = usize::from(stop > to);
                let way = &mut pairs.entry((stop.min(to), stop.max(to))).or_default()[end];
                let corners = route[..route.len() - 1].to_vec();
                if way.1.as_ref().is_some_and(|r| *r != corners) {
                    return Err(format!(
                        "two directions from stop {stop} to {to} take different routes"
                    ));
                }
                way.0.push(direction);
                way.1 = Some(corners);
            }
        }

        let pages = stops
            .iter()
            .map(|&s| self.position(s).x / 256 + 1)
            .max()
            .unwrap_or(1);
        let mut out = String::new();
        let w = &mut out;
        let point = |id: u8| {
            let Point { x, h } = self.position(id);
            format!("[{x}, {h}]")
        };
        let points = |ids: &[u8]| {
            let list: Vec<String> = ids.iter().map(|&id| point(id)).collect();
            format!("[{}]", list.join(", "))
        };
        let dirs = |dirs: &[Direction]| {
            let names: Vec<String> = dirs
                .iter()
                .map(|d| format!("\"{}\"", format!("{d:?}").to_lowercase()))
                .collect();
            format!("[{}]", names.join(", "))
        };
        let _ = writeln!(
            w,
            "format = 1\nid = \"{id}\"\nsize = [{}, 256]",
            pages * 256
        );
        let _ = writeln!(w, "mode = \"legs\"\nstart = \"stop-{start}\"\n");
        let dir_name = |d: Direction| format!("{d:?}").to_lowercase();
        let _ = writeln!(
            w,
            "[legs]\nleg_frames = 64\nremember = 31\npage_width = 256\npage_wait = 64\n\
             appear_offset = [{APPEARANCE_OFFSET}, 0]\nappear_wait = 64\ny_up = true\n\
             fade_step = 2"
        );
        // behavior/level-map.md, "After finishing a level". The black
        // frames were 52 or 53 when finishing again; the engine takes 52.
        let _ = writeln!(
            w,
            "\n[legs.finish.first]\nblack = 49\nstill = [163, 50]\ngate = 242\n\
             \n[legs.finish.again]\nblack = 52\ngate = 64"
        );
        if let Some(r) = rules {
            // behavior/level-map.md, "A new game"
            let _ = writeln!(
                w,
                "\n[legs.new_game]\nwalk = {}\ntoward = \"{}\"",
                r.walk,
                dir_name(r.start_walk)
            );
        }
        for &stop in &stops {
            let _ = write!(
                w,
                "\n[[nodes]]\nid = \"stop-{stop}\"\nat = {}\n",
                point(stop)
            );
            let Some(r) = rules else {
                let _ = writeln!(w, "kind = \"waypoint\"");
                continue;
            };
            let level_stop = |n: u8| {
                format!("kind = \"event\"\nevent = \"level-stop\"\nparams = {{ stop = {n} }}")
            };
            let kind = match r.kind(stop) {
                StopKind::Level if stop == r.first_level => format!(
                    "kind = \"level\"\nlevel = \"{LEVELS}/{FIRST_LEVEL}.level\"\nonward = \"{}\"",
                    dir_name(r.onward)
                ),
                StopKind::Level => level_stop(stop),
                StopKind::Link(Link::Level(n)) => level_stop(n),
                StopKind::Link(Link::Stop(to)) => {
                    if !stops.contains(&to) {
                        return Err(format!(
                            "link point {stop} leads to stop {to}, which the map doesn't have"
                        ));
                    }
                    format!("kind = \"warp\"\nmap = \"{id}\"\nnode = \"stop-{to}\"")
                }
                StopKind::Link(Link::Unknown) => "kind = \"waypoint\"".to_string(),
                StopKind::Place(n) => {
                    format!("kind = \"event\"\nevent = \"place\"\nparams = {{ kind = {n} }}")
                }
            };
            let _ = writeln!(w, "{kind}");
        }
        for ((a, b), [there, back]) in &pairs {
            // the more open way first, so `unlock` doesn't hold it back
            let swap = there.0.is_empty() || (!back.0.is_empty() && open(*b, *a) < open(*a, *b));
            let (from, to, there, back) = if swap {
                (b, a, back, there)
            } else {
                (a, b, there, back)
            };
            let via = there.1.clone().unwrap_or_default();
            let _ = writeln!(
                w,
                "\n[[paths]]\nfrom = \"stop-{from}\"\nto = \"stop-{to}\"\nvia = {}\nleave = {}",
                points(&via),
                dirs(&there.0)
            );
            if let Some(route) = &back.1 {
                let _ = writeln!(w, "back = {}", dirs(&back.0));
                let reversed: Vec<u8> = via.iter().rev().copied().collect();
                if *route != reversed {
                    let _ = writeln!(w, "back_via = {}", points(route));
                }
            }
            let condition = |o: Open| match o {
                Open::Always => None,
                Open::Finished(stop) => Some(format!("{{ cleared = \"stop-{stop}\" }}")),
                Open::Unknown => Some(format!("{{ flag = \"opened:stop-{a}:stop-{b}\" }}")),
            };
            let way = open(*from, *to);
            if let Some(c) = condition(way) {
                let _ = writeln!(w, "unlock = {c}");
            }
            if back.1.is_some() {
                let back_way = open(*to, *from);
                if back_way != way
                    && let Some(c) = condition(back_way)
                {
                    let _ = writeln!(w, "back_unlock = {c}");
                }
            }
        }
        Ok(out)
    }

    /// Checks every route the map has, so an image whose map the spec
    /// doesn't describe is reported.
    pub fn check(&self) -> Result<(), String> {
        for stop in self.ids().filter(|&id| !is_corner(id)) {
            for direction in Direction::ALL {
                self.route(stop, direction)?;
            }
        }
        Ok(())
    }
}

/// Writes a title's level map into the pack as the world map
/// [`LEVEL_MAP`], with the spec's [`Rules`].
pub struct LevelMapDecoder {
    pub title: String,
}

impl Decoder for LevelMapDecoder {
    fn name(&self) -> &str {
        "Level map"
    }

    fn applies_to(&self, image: &Known) -> bool {
        image.title == self.title
    }

    fn decode(&self, image: &Image, pack: &mut PackWriter) -> Result<(), String> {
        let rules = Rules::of(&self.title)
            .ok_or_else(|| format!("the spec doesn't say what {}'s stops are", self.title))?;
        let map = LevelMap::from_image(image, &self.title)?;
        map.check()?;
        let text = map.to_title_world_map(LEVEL_MAP, &rules)?;
        pack.write(&format!("{MAPS}/{LEVEL_MAP}.map"), text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/level-map/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The bytes of a hex dump whose lines start with the address of
    /// their first byte, checking that they start at `start` and follow
    /// on.
    fn hex(name: &str, start: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        for line in vector(name).lines().filter(|l| !l.trim().is_empty()) {
            let (address, data) = line.split_once(':').unwrap();
            let address = usize::from_str_radix(address.trim(), 16).unwrap();
            assert_eq!(address, start + bytes.len(), "{name}: {line}");
            bytes.extend(
                data.split_whitespace()
                    .map(|b| u8::from_str_radix(b, 16).unwrap()),
            );
        }
        bytes
    }

    fn map() -> LevelMap {
        let map = LevelMap::read(&hex("points.hex", 0xa000), &hex("corners.hex", 0xc000), 3);
        map.unwrap()
    }

    fn name(direction: Direction) -> &'static str {
        match direction {
            Direction::Right => "right",
            Direction::Left => "left",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    #[test]
    fn points_match_the_spec_vectors() {
        let map = map();
        let mut rows = vec![vector("points.tsv").lines().next().unwrap().to_string()];
        for id in map.ids() {
            let Point { x, h } = map.position(id);
            let (kind, appear) = if is_corner(id) {
                ("corner", "-".to_string())
            } else {
                ("stop", map.appearance(id).x.to_string())
            };
            let neighbours: Vec<String> = Direction::ALL
                .iter()
                .map(|&d| map.neighbour(id, d).unwrap_or(0).to_string())
                .collect();
            rows.push(format!(
                "{id}\t{kind}\t{x}\t{h}\t{appear}\t{}",
                neighbours.join("\t")
            ));
        }
        let expected: Vec<String> = vector("points.tsv").lines().map(str::to_string).collect();
        assert_eq!(rows, expected);
    }

    #[test]
    fn routes_match_the_spec_vectors() {
        let map = map();
        map.check().unwrap();
        let mut rows = vec![vector("routes.tsv").lines().next().unwrap().to_string()];
        for stop in map.ids().filter(|&id| !is_corner(id)) {
            for direction in Direction::ALL {
                let Some(route) = map.route(stop, direction).unwrap() else {
                    continue;
                };
                let points: Vec<String> = route
                    .iter()
                    .map(|&id| {
                        let Point { x, h } = map.position(id);
                        format!("{id}@{x},{h}")
                    })
                    .collect();
                let to = route.last().unwrap();
                rows.push(format!(
                    "{stop}\t{}\t{to}\t{}",
                    name(direction),
                    points.join(" ")
                ));
            }
        }
        let expected: Vec<String> = vector("routes.tsv").lines().map(str::to_string).collect();
        assert_eq!(rows, expected);
    }

    /// Point tables with the given positions and neighbours (Right,
    /// Left, Up, Down).
    fn tables(points: &[(u8, [u8; 3], [u8; 4])]) -> Vec<u8> {
        let mut tables = vec![0; 7 * TABLE_SIZE];
        for &(id, position, neighbours) in points {
            for (t, byte) in position.iter().chain(&neighbours).enumerate() {
                tables[t * TABLE_SIZE + usize::from(id)] = *byte;
            }
        }
        tables
    }

    #[test]
    fn the_world_map_walks_the_spec_vectors_routes() {
        use clamber_worldmap::{Dir, Mode, Progress, WorldMap};

        let map = map();
        let text = map.to_world_map("level-map", 10).unwrap();
        let world = WorldMap::from_toml_str(&text, "level-map.map").unwrap();
        let Mode::Legs(legs) = &world.mode else {
            panic!("not a legs map")
        };
        assert!(legs.y_up);
        let finish = legs.finish.as_ref().unwrap();
        assert_eq!(
            (finish.first.black, finish.first.still, finish.first.gate),
            (49, Some((163, 50)), 242)
        );
        assert_eq!(
            (finish.again.black, finish.again.still, finish.again.gate),
            (52, None, 64)
        );
        assert_eq!(world.start.as_deref(), Some("stop-10"));
        let p = Progress::default();
        let mut walks = 0;
        for row in vector("routes.tsv").lines().skip(1) {
            let cols: Vec<&str> = row.split('\t').collect();
            let from = world.node(&format!("stop-{}", cols[0])).unwrap();
            let dir = Dir::parse(cols[1]).unwrap();
            let (_, to, points) = world.leg_route(from, dir, &p).unwrap();
            assert_eq!(world.nodes[to].id, format!("stop-{}", cols[2]), "{row}");
            let expected: Vec<clamber_level::Vec2> = cols[3]
                .split(' ')
                .map(|p| {
                    let (x, h) = p.split_once('@').unwrap().1.split_once(',').unwrap();
                    clamber_level::Vec2::ints(x.parse().unwrap(), h.parse().unwrap())
                })
                .collect();
            assert_eq!(points, expected, "{row}");
            walks += 1;
        }
        // and no others
        let all = world
            .nodes
            .iter()
            .enumerate()
            .flat_map(|(i, _)| Dir::ORDER.map(|d| world.leg_route(i, d, &p)))
            .flatten()
            .count();
        assert_eq!(all, walks);
        assert!(map.to_world_map("level-map", 112).is_err());
    }

    #[test]
    fn the_formats_example() {
        let points = tables(&[
            (20, [0, 0, 0], [0, 0, 30, 0]),
            (30, [0, 0, 0], [40, 20, 0, 20]),
        ]);
        let map = LevelMap::read(&points, &[20, 20, 30, 113, 30, 30, 20, 112], 2).unwrap();
        let route = |stop, direction| map.route(stop, direction).unwrap().unwrap();
        assert_eq!(route(20, Direction::Up), [112, 113, 30]);
        assert_eq!(route(30, Direction::Left), [113, 112, 20]);
        assert_eq!(route(30, Direction::Down), [113, 112, 20]);
        assert_eq!(route(30, Direction::Right), [40]);
        assert_eq!(map.route(20, Direction::Right), Ok(None));
    }

    #[test]
    fn routes_the_spec_doesnt_expect_are_reported() {
        let points = tables(&[(20, [0, 0, 0], [30, 0, 0, 0])]);
        // Two corners start the same walk.
        let two = LevelMap::read(&points, &[20, 20, 30, 30, 20, 20, 30, 30], 2).unwrap();
        assert!(two.route(20, Direction::Right).is_err());
        assert!(two.check().is_err());
        // A loop between two corners.
        let looped = LevelMap::read(&points, &[20, 20, 30, 113, 0, 0, 30, 112], 2).unwrap();
        assert!(looped.route(20, Direction::Right).is_err());
        // A corner pointing at a corner without a record.
        let missing = LevelMap::read(&points, &[20, 20, 30, 120], 1).unwrap();
        assert!(missing.route(20, Direction::Right).is_err());
    }

    #[test]
    fn reads_the_map_where_the_locators_say() {
        let mut bytes = vec![0; 61 * crate::BANK_SIZE];
        let mut put = |data, contents: &[u8]| {
            let at = locate("title-1", data)
                .unwrap()
                .position(bytes.len())
                .unwrap();
            bytes[at..at + contents.len()].copy_from_slice(contents);
        };
        put(Data::MapPoints, &hex("points.hex", 0xa000));
        put(Data::MapCorners, &hex("corners.hex", 0xc000));
        let read = LevelMap::from_image(&Image::new(bytes), "title-1").unwrap();
        // Title-1 has four corner records; the vectors' fourth is zeros.
        assert_eq!(
            read.route(30, Direction::Left),
            map().route(30, Direction::Left)
        );
        assert_eq!(read.position(200), Point { x: 511, h: 7 });
        assert!(LevelMap::from_image(&Image::new(vec![0; 16]), "title-1").is_err());
        assert!(LevelMap::from_image(&Image::new(vec![0; 16]), "title-9").is_err());
    }

    #[test]
    fn tables_past_the_end_are_reported() {
        assert!(LevelMap::read(&[0; 7 * TABLE_SIZE - 1], &[], 0).is_err());
        assert!(LevelMap::read(&[0; 7 * TABLE_SIZE], &[0; 7], 2).is_err());
        assert!(LevelMap::read(&[0; 7 * TABLE_SIZE], &[0; 200], 33).is_err());
    }

    /// A made-up map with title-1's stop ids: the start point 235 and the
    /// first level's link point 236, one leg of 64 px apart; the first
    /// level's stop 22 with 12 below it; 20 and 240, 66 and 245 joined
    /// both ways; and 234, which begins the level of stop 1.
    fn title_1_like() -> LevelMap {
        let points = tables(&[
            (235, [0, 16, 100], [236, 0, 0, 0]),
            (236, [0, 88, 100], [0, 235, 0, 0]),
            (22, [1, 44, 60], [0, 0, 0, 12]),
            (12, [1, 44, 20], [0, 0, 22, 0]),
            (20, [0, 40, 50], [240, 0, 0, 0]),
            (240, [0, 80, 50], [0, 20, 0, 0]),
            (66, [0, 120, 30], [0, 0, 0, 245]),
            (245, [0, 120, 10], [0, 0, 66, 0]),
            (234, [0, 200, 100], [0, 0, 0, 0]),
        ]);
        LevelMap::read(&points, &[], 0).unwrap()
    }

    fn title_1_world_map() -> clamber_worldmap::WorldMap {
        let rules = Rules::of("title-1").unwrap();
        let text = title_1_like()
            .to_title_world_map(LEVEL_MAP, &rules)
            .unwrap();
        clamber_worldmap::WorldMap::from_toml_str(&text, "level-map.map").unwrap()
    }

    #[test]
    fn title_1_s_kinds_of_stops() {
        let rules = Rules::of("title-1").unwrap();
        let kinds = [1, 22, 111, 144, 229]
            .map(|id| (id, StopKind::Level))
            .into_iter()
            .chain([
                (230, StopKind::Link(Link::Stop(48))),
                (231, StopKind::Link(Link::Stop(64))),
                (232, StopKind::Link(Link::Stop(36))),
                (233, StopKind::Link(Link::Stop(165))),
                (234, StopKind::Link(Link::Level(1))),
                (235, StopKind::Link(Link::Unknown)),
                (236, StopKind::Link(Link::Stop(22))),
                (237, StopKind::Link(Link::Stop(217))),
                (238, StopKind::Place(1)),
                (243, StopKind::Place(1)),
                (244, StopKind::Place(2)),
                (249, StopKind::Place(2)),
                (250, StopKind::Place(3)),
                (255, StopKind::Place(3)),
            ]);
        for (id, kind) in kinds {
            assert_eq!(rules.kind(id), kind, "stop {id}");
        }
        assert_eq!((rules.start, rules.first_level), (235, 22));
        assert!(Rules::of("title-2").is_none());
    }

    #[test]
    fn title_1_s_stops_in_the_world_map() {
        use clamber_worldmap::{Dir, Mode, NodeKind};

        let world = title_1_world_map();
        assert_eq!(world.start.as_deref(), Some("stop-235"));
        let Mode::Legs(legs) = &world.mode else {
            panic!("not a legs map")
        };
        let new_game = legs.new_game.as_ref().unwrap();
        assert_eq!((new_game.walk, new_game.toward), (64, Dir::Right));
        let node = |id: u8| &world.nodes[world.node(&format!("stop-{id}")).unwrap()];
        let event = |id: u8| match &node(id).kind {
            NodeKind::Event { event, params } => (event.clone(), params.to_string()),
            other => panic!("stop {id}: {other:?}"),
        };
        assert_eq!(
            node(22).kind,
            NodeKind::Level {
                level: "levels/level-1.level".into(),
                exits: vec!["normal".into()],
            }
        );
        assert_eq!(node(22).onward, Some(Dir::Down));
        assert_eq!(
            node(236).kind,
            NodeKind::Warp {
                map: "level-map".into(),
                node: "stop-22".into(),
            }
        );
        assert_eq!(node(235).kind, NodeKind::Waypoint);
        assert_eq!(event(20), ("level-stop".into(), "stop = 20\n".into()));
        assert_eq!(event(12), ("level-stop".into(), "stop = 12\n".into()));
        assert_eq!(event(234), ("level-stop".into(), "stop = 1\n".into()));
        assert_eq!(event(240), ("place".into(), "kind = 1\n".into()));
        assert_eq!(event(245), ("place".into(), "kind = 2\n".into()));
        assert!(
            world
                .nodes
                .iter()
                .all(|n| n.id == "stop-22" || n.onward.is_none())
        );
    }

    /// The (from, to) of every walk a direction would start.
    fn open_walks(
        world: &clamber_worldmap::WorldMap,
        p: &clamber_worldmap::Progress,
    ) -> Vec<(String, String)> {
        let mut walks: Vec<(String, String)> = (0..world.nodes.len())
            .flat_map(|i| {
                clamber_worldmap::Dir::ORDER.map(|d| world.leg_route(i, d, p).map(|r| (i, r.1)))
            })
            .flatten()
            .map(|(from, to)| (world.nodes[from].id.clone(), world.nodes[to].id.clone()))
            .collect();
        walks.sort();
        walks
    }

    #[test]
    fn title_1_s_progress() {
        let world = title_1_world_map();
        let walk = |a: &str, b: &str| (a.to_string(), b.to_string());
        let mut p = clamber_worldmap::Progress::default();
        let new_game = [
            walk("stop-20", "stop-240"),
            walk("stop-235", "stop-236"),
            walk("stop-240", "stop-20"),
            walk("stop-245", "stop-66"),
            walk("stop-66", "stop-245"),
        ];
        assert_eq!(open_walks(&world, &p), new_game);
        // the first level's first finish opens 22 to 12, both ways
        p.clear("level-map:stop-22", "normal");
        let mut finished = new_game.to_vec();
        finished.extend([walk("stop-12", "stop-22"), walk("stop-22", "stop-12")]);
        finished.sort();
        assert_eq!(open_walks(&world, &p), finished);
        // the others wait on progress the spec doesn't cover
        p.flags.insert("opened:stop-235:stop-236".into());
        finished.push(walk("stop-236", "stop-235"));
        finished.sort();
        assert_eq!(open_walks(&world, &p), finished);
    }

    /// The example in spec/behavior/level-map.md, "A new game", on the
    /// imported map.
    #[test]
    fn a_new_game_on_title_1_s_map() {
        use clamber_worldmap::{Buttons, LegsScreen, Mode, NodeKind, Progress};

        let world = title_1_world_map();
        let Mode::Legs(legs) = &world.mode else {
            panic!("not a legs map")
        };
        let mut screen = LegsScreen::new_game(&world, legs).unwrap();
        let p = Progress::default();
        let jump = Buttons::enter(0);
        let mut seen = Vec::new();
        for frame in 0..280 {
            let held = match frame {
                100 | 140 | 230 | 240 => jump,
                180..=200 => Buttons::DOWN,
                _ => Buttons::NONE,
            };
            let step = screen.step(&world, legs, &p, held);
            let id = |n: usize| world.nodes[n].id.as_str();
            for (what, node) in [
                ("start", step.step.started.map(|i| world.paths[i].from)),
                ("arrive", step.step.arrived),
                ("enter", step.step.entered),
                ("appear", step.appeared),
                ("leave", step.left),
            ] {
                if let Some(n) = node {
                    seen.push((frame, what, id(n).to_string()));
                }
            }
            if frame == 172 {
                assert_eq!(screen.lead.position(), clamber_level::Vec2::ints(308, 60));
            }
        }
        let at = |f: u32, what: &'static str, id: &str| (f, what, id.to_string());
        assert_eq!(
            seen,
            [
                at(64, "start", "stop-235"),
                at(127, "arrive", "stop-236"),
                at(140, "enter", "stop-236"),
                at(172, "appear", "stop-22"),
                at(240, "enter", "stop-22"),
                at(272, "leave", "stop-22"),
            ]
        );
        let NodeKind::Level { level, .. } = &world.nodes[world.node("stop-22").unwrap()].kind
        else {
            panic!("stop 22 isn't a level")
        };
        assert_eq!(level, "levels/level-1.level");
    }

    #[test]
    fn the_decoder_writes_the_level_map_into_the_pack() {
        let mut bytes = vec![0; 61 * crate::BANK_SIZE];
        let at = locate("title-1", Data::MapPoints)
            .unwrap()
            .position(bytes.len())
            .unwrap();
        bytes[at..at + 7 * TABLE_SIZE].copy_from_slice(&title_1_like().tables);
        let dir = std::env::temp_dir().join(format!("clamber-level-map-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut pack = PackWriter {
            dir: dir.clone(),
            written: Default::default(),
            skipped: Vec::new(),
        };
        let decoder = LevelMapDecoder {
            title: "title-1".into(),
        };
        decoder.decode(&Image::new(bytes), &mut pack).unwrap();
        let text = std::fs::read_to_string(dir.join("maps/level-map.map")).unwrap();
        let world = clamber_worldmap::WorldMap::from_toml_str(&text, "level-map.map").unwrap();
        assert_eq!(world, title_1_world_map());
        std::fs::remove_dir_all(&dir).unwrap();

        // an image whose map has no start point
        let decoder = LevelMapDecoder {
            title: "title-1".into(),
        };
        let mut pack = PackWriter {
            dir: dir.clone(),
            written: Default::default(),
            skipped: Vec::new(),
        };
        let err = decoder
            .decode(&Image::new(vec![0; 61 * crate::BANK_SIZE]), &mut pack)
            .unwrap_err();
        assert!(err.contains("no stop 235"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
