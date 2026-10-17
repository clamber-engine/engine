//! A level's placement list (`spec/formats/placements.md`): the objects
//! that appear as the view comes near them, each at a level x and a
//! height above the level's bottom edge, read into entries and groups and
//! turned into Clamber level entities.
//!
//! A list whose first entry has class 9 is divided into sections, and
//! its section table is read with it ("Lists with sections").
//!
//! Where its reference's [`Kind`] is known ("Kinds"), an entity is the
//! engine archetype that behaves as that kind; otherwise its kind names
//! its reference (`placed-1234`), for a pack to give each one a
//! definition.

use clamber_level::{Entity, Section, Spot, Vec2};
use toml::{Table, Value};

use crate::Kind;
use crate::kinds::WALKER;

/// The kind of entity that starts the dimming, as in `clamber-sim`.
pub const DIM: &str = "dim";

/// How the kind of an object whose kind isn't known starts; its
/// reference in hex follows.
const PLACED: &str = "placed-";

/// Bytes in an entry.
pub const ENTRY_SIZE: usize = 8;

/// Bytes in a section table's record.
pub const RECORD_SIZE: usize = 8;

/// How far left of the view, and right of its left edge, an entry's `x`
/// can be and still be in the window.
const WINDOW: std::ops::RangeInclusive<i64> = -31..=288;

/// One 8-byte entry of a placement list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub class: u16,
    pub x: u16,
    /// Height above the level's bottom edge.
    pub h: u16,
    pub reference: u16,
}

/// What an entry places, by its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// One object of the definition at the entry's reference (classes 1
    /// and 2, and class 15, which also needs the vertical window: see
    /// [`Entry::needs_rows`]).
    Object,
    /// The members listed at the entry's reference + 8 (class 5).
    Group,
    /// No object (class 8). The first level's starts the dimming
    /// (`spec/behavior/dimming.md`).
    Nothing,
    /// No object (class 9): only ever a list's first entry, whose
    /// reference is the list's section table.
    Sections,
    /// A class the spec doesn't cover.
    Other(u16),
}

impl Entry {
    fn read(bytes: &[u8]) -> Entry {
        let word = |i: usize| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]);
        Entry {
            class: word(0),
            x: word(1),
            h: word(2),
            reference: word(3),
        }
    }

    fn is_end(self) -> bool {
        self == Entry {
            class: 0,
            x: 0,
            h: 0,
            reference: 0,
        }
    }

    pub fn kind(self) -> Class {
        match self.class {
            1 | 2 | 15 => Class::Object,
            5 => Class::Group,
            8 => Class::Nothing,
            9 => Class::Sections,
            c => Class::Other(c),
        }
    }

    /// Whether the entry's object also has to be inside the vertical
    /// window to appear (class 15).
    pub fn needs_rows(self) -> bool {
        self.class == 15
    }

    /// The level y of the entry's position, for a level whose bottom edge
    /// is at `bottom`.
    pub fn level_y(self, bottom: i64) -> i64 {
        bottom - i64::from(self.h)
    }

    /// Whether the entry is in the window of a view at whole-px level x
    /// `view`.
    pub fn in_window(self, view: i64) -> bool {
        WINDOW.contains(&(i64::from(self.x) - view))
    }
}

/// A list entry, with its members if it's a group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub entry: Entry,
    /// In stored order; empty unless the entry is a group.
    pub members: Vec<Entry>,
}

/// One 8-byte record of a section table: two sections, each given by the
/// numbers of its first and last entries, and the point where they meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub first: (u8, u8),
    pub x: u16,
    /// Height above the level's bottom edge, as an entry's.
    pub h: u16,
    pub second: (u8, u8),
}

impl Record {
    fn read(bytes: &[u8]) -> Record {
        let word = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        Record {
            first: (bytes[0], bytes[1]),
            x: word(2),
            h: word(4),
            second: (bytes[6], bytes[7]),
        }
    }

    /// The level y of the point, for a level whose bottom edge is at
    /// `bottom`.
    pub fn level_y(self, bottom: i64) -> i64 {
        bottom - i64::from(self.h)
    }
}

/// A level's placement list, in stored order, with its section table if
/// it has sections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placements {
    pub list: Vec<Placement>,
    /// Empty unless the list's first entry has class 9.
    pub records: Vec<Record>,
}

impl Placements {
    /// Reads the list at address `list`. `bank` gives the bytes of the
    /// list's bank from an address to the bank's end, or `None` where the
    /// bank doesn't have that address. Members of a group, and a section
    /// table, are read from the same bank. A member that is itself a group
    /// isn't followed.
    pub fn read<'a>(
        bank: impl Fn(u16) -> Option<&'a [u8]>,
        list: u16,
    ) -> Result<Placements, String> {
        let stored = entries(&bank, list)?;
        let records = match stored.first() {
            Some(&first) if first.kind() == Class::Sections => records(&bank, first.reference)?,
            _ => Vec::new(),
        };
        let list = stored
            .into_iter()
            .map(|entry| {
                let members = match entry.kind() {
                    Class::Group => {
                        let start = entry.reference.checked_add(ENTRY_SIZE as u16);
                        let start = start.ok_or_else(|| {
                            format!("the group at {:04x} runs past its bank", entry.reference)
                        })?;
                        entries(&bank, start)?
                    }
                    _ => Vec::new(),
                };
                Ok(Placement { entry, members })
            })
            .collect::<Result<_, String>>()?;
        Ok(Placements { list, records })
    }

    /// The objects as level entities, for a level whose bottom edge is at
    /// `bottom`. Each is at its entry's position (moved down by
    /// [`Kind::below_entry`]), the entity
    /// [`Kind::entity`] gives for the kind `kind` gives its reference, or
    /// else of kind `placed-` and its reference in hex. A group's walkers,
    /// and its members of unknown kinds, get `group`, the group entry's
    /// index in the list plus one, so the walkers
    /// appear together when one of them comes near the view (in the first
    /// level the group entry is at its first member). Entries that place
    /// nothing are [`DIM`] entities at their entry's position, and classes
    /// the spec doesn't cover have no entity.
    pub fn entities(&self, bottom: i64, kind: impl Fn(u16) -> Option<Kind>) -> Vec<Entity> {
        self.placed(bottom, kind).0
    }

    /// The [`entities`](Placements::entities), and the level's sections
    /// ("In a list with sections"): one for each of the table's sections,
    /// meeting the next at its record's point, or for a list without
    /// sections one covering the whole list ("Stored order"). Each entry
    /// that places objects is a spot: an object at its entry's position,
    /// needing the view's rows too for class 15 and, in a list with
    /// sections, class 2 ("Class 1 and class 2 entries in these lists"),
    /// or a group, at its entry's `x`. A class 8 entry is a spot of its
    /// [`DIM`] entity, clear only in the window. Other entries that place
    /// nothing hold nothing back, so they aren't spots.
    ///
    /// Whether class 2 entries far above or below the view appear in a
    /// list without sections isn't covered; as the engine's choice they
    /// need only the window, as in "When an entry appears".
    pub fn objects(
        &self,
        bottom: i64,
        kind: impl Fn(u16) -> Option<Kind>,
    ) -> (Vec<Entity>, Vec<Section>) {
        let (entities, of_entry) = self.placed(bottom, kind);
        let sectioned = !self.records.is_empty();
        let spot = |i: usize| {
            let placement = &self.list[i];
            let entry = placement.entry;
            let spot_entities = of_entry[i].clone();
            let at = Vec2::ints(i32::from(entry.x), entry.level_y(bottom) as i32);
            match entry.kind() {
                Class::Object | Class::Nothing => Some(Spot {
                    at: (entities[spot_entities[0]].at != at).then_some(at),
                    entities: spot_entities,
                    rows: entry.needs_rows() || sectioned && entry.class == 2,
                }),
                Class::Group if !spot_entities.is_empty() => Some(Spot {
                    at: (entities[spot_entities[0]].at.x != at.x).then_some(at),
                    entities: spot_entities,
                    rows: false,
                }),
                _ => None,
            }
        };
        let Some(first) = self.records.first() else {
            let whole = Section {
                spots: (0..self.list.len()).filter_map(spot).collect(),
                meets: None,
            };
            return (entities, vec![whole]);
        };
        let runs = std::iter::once(first.first).chain(self.records.iter().map(|r| r.second));
        let sections = runs
            .enumerate()
            .map(|(n, (a, b))| {
                let last = usize::from(b).min(self.list.len().saturating_sub(1));
                Section {
                    spots: (usize::from(a)..=last).filter_map(spot).collect(),
                    meets: self
                        .records
                        .get(n)
                        .map(|r| Vec2::ints(i32::from(r.x), r.level_y(bottom) as i32)),
                }
            })
            .collect();
        (entities, sections)
    }

    /// The entities, and the indices of each entry's among them.
    fn placed(
        &self,
        bottom: i64,
        kind: impl Fn(u16) -> Option<Kind>,
    ) -> (Vec<Entity>, Vec<Vec<usize>>) {
        let entity = |entry: Entry| {
            let (kind, params, below) = match kind(entry.reference) {
                Some(kind) => {
                    let (name, params) = kind.entity();
                    (name.to_string(), params, kind.below_entry())
                }
                None => (format!("{PLACED}{:04x}", entry.reference), Table::new(), 0),
            };
            Entity {
                kind,
                id: None,
                // A u16 x and a bottom edge inside a level fit an i32.
                at: Vec2::ints(i32::from(entry.x), entry.level_y(bottom) as i32 + below),
                params,
            }
        };
        let mut entities = Vec::new();
        let mut of_entry = Vec::new();
        for (i, placement) in self.list.iter().enumerate() {
            let before = entities.len();
            match placement.entry.kind() {
                Class::Object => entities.push(entity(placement.entry)),
                Class::Group => {
                    for &member in &placement.members {
                        if member.kind() != Class::Object {
                            continue;
                        }
                        let mut member = entity(member);
                        if member.kind == WALKER || member.kind.starts_with(PLACED) {
                            let group = Value::Integer(i as i64 + 1);
                            member.params.insert("group".into(), group);
                        }
                        entities.push(member);
                    }
                }
                // The spec only covers the first level's class 8 entry,
                // which starts the dimming.
                Class::Nothing => entities.push(Entity {
                    kind: DIM.to_string(),
                    params: Table::new(),
                    ..entity(placement.entry)
                }),
                Class::Sections | Class::Other(_) => {}
            }
            of_entry.push((before..entities.len()).collect());
        }
        (entities, of_entry)
    }
}

/// The entries from `start` up to the next entry of all zeros.
fn entries<'a>(bank: &impl Fn(u16) -> Option<&'a [u8]>, start: u16) -> Result<Vec<Entry>, String> {
    let bytes = bank(start).ok_or_else(|| format!("no placement list at {start:04x}"))?;
    let mut entries = Vec::new();
    for chunk in bytes.chunks(ENTRY_SIZE) {
        if chunk.len() < ENTRY_SIZE {
            break;
        }
        let entry = Entry::read(chunk);
        if entry.is_end() {
            return Ok(entries);
        }
        entries.push(entry);
    }
    Err(format!(
        "the placement list at {start:04x} runs past its bank without an end entry"
    ))
}

/// The records of the section table at `start`, up to the record whose
/// first two bytes are both 0.
fn records<'a>(bank: &impl Fn(u16) -> Option<&'a [u8]>, start: u16) -> Result<Vec<Record>, String> {
    let bytes = bank(start).ok_or_else(|| format!("no section table at {start:04x}"))?;
    let mut records = Vec::new();
    for chunk in bytes.chunks(RECORD_SIZE) {
        // Only the end record's first two bytes have to be there.
        if chunk.len() >= 2 && chunk[..2] == [0, 0] {
            return Ok(records);
        }
        if chunk.len() < RECORD_SIZE {
            break;
        }
        records.push(Record::read(chunk));
    }
    Err(format!(
        "the section table at {start:04x} runs past its bank without an end record"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clamber_level::Level;
    use clamber_sim::{Opening, Placed, Window};

    /// The vectors' bank fragment starts at this address.
    const BASE: u16 = 0x9000;
    const LIST: u16 = 0x9008;
    const BOTTOM: i64 = 512;
    const SECTIONS_BOTTOM: i64 = 1024;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/placements/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The bank fragment's bytes, checking each line's address.
    fn fragment() -> Vec<u8> {
        fragment_of("bank.hex")
    }

    fn fragment_of(name: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for line in vector(name).lines().filter(|l| !l.trim().is_empty()) {
            let (address, data) = line.split_once(':').unwrap();
            let address = usize::from_str_radix(address.trim(), 16).unwrap();
            assert_eq!(address, usize::from(BASE) + bytes.len(), "{line}");
            bytes.extend(
                data.split_whitespace()
                    .map(|b| u8::from_str_radix(b, 16).unwrap()),
            );
        }
        bytes
    }

    fn read(bytes: &[u8]) -> Result<Placements, String> {
        Placements::read(|a| bytes.get(usize::from(a.checked_sub(BASE)?)..), LIST)
    }

    /// The rows of a table after its header, split at tabs.
    fn rows(name: &str) -> Vec<Vec<String>> {
        let rows: Vec<Vec<String>> = vector(name)
            .lines()
            .skip(1)
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect();
        assert!(!rows.is_empty(), "{name} is empty");
        rows
    }

    /// Every entry with its index as in the vectors (`3`, `3.0`, …).
    fn indexed(placements: &Placements) -> Vec<(String, Option<usize>, Entry)> {
        let mut out = Vec::new();
        for (i, p) in placements.list.iter().enumerate() {
            out.push((i.to_string(), None, p.entry));
            for (j, &m) in p.members.iter().enumerate() {
                out.push((format!("{i}.{j}"), Some(i), m));
            }
        }
        out
    }

    #[test]
    fn entries_match_the_spec_vectors() {
        let placements = read(&fragment()).unwrap();
        let got = indexed(&placements);
        let want = rows("entries.tsv");
        assert_eq!(got.len(), want.len());
        for ((index, group, entry), row) in got.iter().zip(&want) {
            let group = group.map_or("-".to_string(), |g| g.to_string());
            let got = [
                index.clone(),
                group,
                entry.class.to_string(),
                entry.x.to_string(),
                entry.h.to_string(),
                entry.level_y(BOTTOM).to_string(),
                format!("{:04x}", entry.reference),
            ];
            assert_eq!(&got[..], &row[..], "entry {index}");
        }
    }

    #[test]
    fn appearances_match_the_spec_vectors() {
        let placements = read(&fragment()).unwrap();
        let (placed, entries) = engine_sections(&placements, BOTTOM);
        assert_eq!(placed.sections.runs.len(), 1);
        assert_eq!(placed.sections.runs[0], 0..=placed.spots.len() - 1);
        let mut there = vec![None; placed.spots.len()];
        let mut open = Opening::begin(&placed.sections, 0);
        let window = Window {
            across: -31..=288,
            down: -31..=256,
        };
        for n in 0..300 {
            let view = (-40 + 3 * n, 0);
            let up = open.step(&placed.sections, &placed.spots, &window, view, |i| {
                there[i].is_some()
            });
            for i in up {
                there[i] = Some(n);
            }
        }
        let mut got = Vec::new();
        for (spot, at) in there.iter().enumerate() {
            let entry = entries[spot];
            let names: Vec<String> = match placements.list[entry].entry.kind() {
                Class::Group => (0..placed.entities[spot].len())
                    .map(|j| format!("{entry}.{j}"))
                    .collect(),
                // Class 8 places nothing, so it isn't in the vectors.
                Class::Nothing => Vec::new(),
                _ => vec![entry.to_string()],
            };
            for name in names {
                got.push(match at {
                    Some(n) => vec![name, n.to_string(), (-40 + 3 * n).to_string()],
                    None => vec![name, "never".to_string(), "-".to_string()],
                });
            }
        }
        assert_eq!(got, rows("appear.tsv"));
    }

    /// The lists-with-sections vectors' bank fragment.
    fn sections_fragment() -> Placements {
        let bytes = fragment_of("sections-bank.hex");
        read(&bytes).unwrap()
    }

    #[test]
    fn section_records_match_the_spec_vectors() {
        let placements = sections_fragment();
        assert_eq!(placements.list[0].entry.kind(), Class::Sections);
        let want = rows("sections-records.tsv");
        assert_eq!(placements.records.len(), want.len());
        for (i, (r, row)) in placements.records.iter().zip(&want).enumerate() {
            let got = [
                i.to_string(),
                r.first.0.to_string(),
                r.first.1.to_string(),
                r.x.to_string(),
                r.h.to_string(),
                r.level_y(SECTIONS_BOTTOM).to_string(),
                r.second.0.to_string(),
                r.second.1.to_string(),
            ];
            assert_eq!(&got[..], &row[..], "record {i}");
        }
    }

    /// The vectors' list as a level's sections, through a level file,
    /// as the simulation takes them, with each spot's entry number.
    fn engine_sections(p: &Placements, bottom: i64) -> (Placed, Vec<usize>) {
        let (entities, sections) = p.objects(bottom, |_| None);
        let src = "format = 1\nsize = [2000, 1024]\nstart = [10, 10]\n";
        let mut level = Level::from_toml_str(src, "t").unwrap();
        (level.entities, level.sections) = (entities, sections);
        let level = Level::from_toml_str(&level.to_toml_string(), "saved").unwrap();
        let placed = Placed::of(&level).unwrap();
        let of_entry = p.placed(bottom, |_| None).1;
        let entry = |e: usize| of_entry.iter().position(|of| of.contains(&e)).unwrap();
        let entries = placed.entities.iter().map(|es| entry(es[0])).collect();
        (placed, entries)
    }

    #[test]
    fn section_appearances_match_the_spec_vectors() {
        let placements = sections_fragment();
        let (placed, entries) = engine_sections(&placements, SECTIONS_BOTTOM);
        let (spots, sections) = (&placed.spots, &placed.sections);
        let window = Window {
            across: -31..=288,
            down: -31..=256,
        };
        let frames = rows("sections-frames.tsv");
        let view = |row: &[String]| (row[1].parse().unwrap(), row[2].parse().unwrap());
        let mut open = Opening::begin(sections, view(&frames[0]).1);
        // No object is ever removed.
        let mut there = vec![None; spots.len()];
        for (n, row) in frames.iter().enumerate() {
            assert_eq!(row[0], n.to_string());
            let up = open.step(sections, spots, &window, view(row), |i| there[i].is_some());
            for &i in &up {
                there[i] = Some(n);
            }
            let got: Vec<String> = up.iter().map(|&i| entries[i].to_string()).collect();
            let got = if got.is_empty() {
                "-".to_string()
            } else {
                got.join(",")
            };
            assert_eq!(got, row[3], "frame {n}");
        }
        for row in rows("sections-appear.tsv") {
            let entry: usize = row[0].parse().unwrap();
            let i = entries.iter().position(|&e| e == entry).unwrap();
            let got = there[i].map_or("never".to_string(), |n| n.to_string());
            assert_eq!(got, row[4], "entry {i}");
        }
    }

    #[test]
    fn class_15_entries_are_objects_and_class_9_places_nothing() {
        let placements = sections_fragment();
        let entities = placements.entities(SECTIONS_BOTTOM, |_| None);
        // Entries 1–14: 13 objects and a group of 2.
        assert_eq!(entities.len(), 15);
        assert_eq!(entities[0].kind, "placed-a000");
        assert_eq!(entities[0].at, Vec2::ints(100, 900));
    }

    #[test]
    fn a_list_with_sections_gives_the_levels_sections() {
        let (entities, sections) = sections_fragment().objects(SECTIONS_BOTTOM, |_| None);
        let meets: Vec<_> = sections.iter().map(|s| s.meets).collect();
        assert_eq!(
            meets,
            [Some(Vec2::ints(960, 724)), Some(Vec2::ints(60, 324)), None]
        );
        let spots: Vec<Vec<(Vec<usize>, bool)>> = sections
            .iter()
            .map(|s| {
                s.spots
                    .iter()
                    .map(|p| (p.entities.clone(), p.rows))
                    .collect()
            })
            .collect();
        // Entry 4 is the group of two; class 1 entries don't need rows.
        let one = |e, rows| (vec![e], rows);
        assert_eq!(
            spots,
            [
                vec![
                    one(0, true),
                    one(1, true),
                    one(2, false),
                    (vec![3, 4], false),
                    one(5, true),
                    one(6, true)
                ],
                vec![one(7, true), one(8, true), one(9, true), one(10, true)],
                vec![one(11, true), one(12, false), one(13, true), one(14, true)],
            ]
        );
        // The group is at its entry's x, 10 px right of its first member.
        let ats: Vec<_> = sections
            .iter()
            .flat_map(|s| &s.spots)
            .map(|p| p.at)
            .collect();
        assert_eq!(entities[3].at, Vec2::ints(690, 924));
        assert_eq!(ats[3], Some(Vec2::ints(700, 924)));
        assert_eq!(ats.iter().flatten().count(), 1);
        assert_eq!(entities.len(), 15);
        // A list without sections is one section, with no point to meet
        // another at, and class 2 entries need only the window.
        let sections = read(&fragment()).unwrap().objects(BOTTOM, |_| None).1;
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].meets, None);
        assert!(sections[0].spots.iter().all(|p| !p.rows));
    }

    #[test]
    fn a_section_table_must_end_inside_its_bank() {
        // A class 9 entry whose table is at 0x10: one record, then the end.
        let mut bank = vec![9, 0, 0, 0, 0, 0, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        bank.extend([1, 1, 0, 0, 0, 0, 2, 2, 0, 0]);
        let read = |bytes: &[u8]| Placements::read(|a| bytes.get(usize::from(a)..), 0);
        assert_eq!(read(&bank).unwrap().records.len(), 1);
        // Cut inside the end record's first two bytes, then inside the
        // record.
        assert!(read(&bank[..0x19]).is_err());
        assert!(read(&bank[..0x14]).is_err());
        assert!(Placements::read(|a| (a == 0).then_some(&bank[..]), 0).is_err());
    }

    #[test]
    fn the_example_in_the_format() {
        let entry = |x| Entry {
            class: 1,
            x,
            h: 175,
            reference: 0,
        };
        assert_eq!(entry(300).level_y(512), 337);
        assert!(!entry(300).in_window(11));
        assert!(entry(300).in_window(12));
        assert!(!entry(900).in_window(611));
        assert!(entry(900).in_window(612));
        assert!(entry(0).in_window(31));
        assert!(!entry(0).in_window(32));
    }

    #[test]
    fn entities_are_at_their_entries() {
        let kind = |r| (r == 0x1300).then_some(Kind::SmallWalker);
        let entities = read(&fragment()).unwrap().entities(BOTTOM, kind);
        let summary: Vec<(String, Vec2, Option<i64>)> = entities
            .iter()
            .map(|e| {
                let group = e.params.get("group").and_then(Value::as_integer);
                (e.kind.clone(), e.at, group)
            })
            .collect();
        let at = Vec2::ints;
        let one = |kind: &str, x, y| (kind.to_string(), at(x, y), None);
        let member = |kind: &str, x, y| (kind.to_string(), at(x, y), Some(4));
        assert_eq!(
            summary,
            [
                one("placed-1234", 16, 412),
                one("placed-2000", 40, 312),
                one("placed-1234", 40, 212),
                member(WALKER, 500, 392),
                member(WALKER, 560, 372),
                member("placed-1310", 420, 416),
                one(DIM, 600, 256),
                one("placed-1240", 700, 0),
                one("placed-1250", 650, 512),
                one("placed-ffff", 65000, 505),
            ]
        );
    }

    /// animal-crate.md: a crate's position is its entry's level y plus 1.
    #[test]
    fn an_animal_crate_is_1_px_below_its_entry() {
        let kind = |r| (r == 0x2000).then_some(Kind::AnimalCrate);
        let entities = read(&fragment()).unwrap().entities(BOTTOM, kind);
        let crate_ = entities.iter().find(|e| e.kind == "hutch").unwrap();
        assert_eq!(crate_.at, Vec2::ints(40, 313));
    }

    #[test]
    fn class_8_entries_start_the_simulations_dimming() {
        assert_eq!(DIM, clamber_sim::DIM);
    }

    #[test]
    fn a_list_must_end_inside_its_bank() {
        let bytes = fragment();
        // Cut inside the group's members, then inside the main list.
        assert!(read(&bytes[..0x70]).is_err());
        assert!(read(&bytes[..0x4C]).is_err());
        assert!(read(&bytes[..0x50]).is_err());
        assert!(read(&bytes[..0x78]).is_ok());
        assert!(Placements::read(|_| None, LIST).is_err());
        let group_at_end = [5, 0, 0, 0, 0, 0, 0xF8, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(Placements::read(|a| group_at_end.get(usize::from(a)..), 0).is_err());
    }
}
