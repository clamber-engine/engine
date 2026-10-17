//! The level map's points and routes (`spec/formats/level-map.md`):
//! where each stop and corner is, which stop each direction leads to,
//! and the points a walk between two stops passes through.
//!
//! Which paths are open, what each stop is, and how the lead walks a
//! route (`behavior/level-map.md`) are elsewhere.

use crate::{Data, Image, locate};

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
    /// path is open: what the stops are and the progress that opens
    /// paths aren't covered by the spec.
    pub fn to_world_map(&self, id: &str, start: u8) -> Result<String, String> {
        use std::collections::BTreeMap;
        use std::fmt::Write;

        let stops: Vec<u8> = self.ids().filter(|&id| !is_corner(id)).collect();
        if !stops.contains(&start) {
            return Err(format!("the map has no stop {start}"));
        }
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
        for &stop in &stops {
            let _ = writeln!(
                w,
                "\n[[nodes]]\nid = \"stop-{stop}\"\nkind = \"waypoint\"\nat = {}",
                point(stop)
            );
        }
        for ((a, b), [there, back]) in &pairs {
            let (from, to, there, back) = if there.0.is_empty() {
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
}
