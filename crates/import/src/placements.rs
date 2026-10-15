//! A level's placement list (`spec/formats/placements.md`): the objects
//! that appear as the view comes near them, each at a level x and a
//! height above the level's bottom edge, read into entries and groups and
//! turned into Clamber level entities.
//!
//! What kind of object each reference places isn't in the spec yet, so
//! an entity's kind names its reference (`placed-1234`), for a pack to
//! give each one a definition.

use clamber_level::{Entity, Vec2};
use toml::{Table, Value};

/// Bytes in an entry.
pub const ENTRY_SIZE: usize = 8;

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
    /// and 2).
    Object,
    /// The members listed at the entry's reference + 8 (class 5).
    Group,
    /// No object (class 8).
    Nothing,
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
            1 | 2 => Class::Object,
            5 => Class::Group,
            8 => Class::Nothing,
            c => Class::Other(c),
        }
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

/// A level's placement list, in stored order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placements(pub Vec<Placement>);

impl Placements {
    /// Reads the list at address `list`. `bank` gives the bytes of the
    /// list's bank from an address to the bank's end, or `None` where the
    /// bank doesn't have that address. Members of a group are read from
    /// the same bank. A member that is itself a group isn't followed.
    pub fn read<'a>(
        bank: impl Fn(u16) -> Option<&'a [u8]>,
        list: u16,
    ) -> Result<Placements, String> {
        let placements = entries(&bank, list)?
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
        Ok(Placements(placements))
    }

    /// The objects as level entities, for a level whose bottom edge is at
    /// `bottom`. Each is at its entry's position, of kind `placed-` and
    /// its reference in hex. A group's members get `group`, the group
    /// entry's index in the list, and `appears_at`, the group entry's x:
    /// they all appear when that is in the window. Entries that place
    /// nothing, and classes the spec doesn't cover, have no entity.
    pub fn entities(&self, bottom: i64) -> Vec<Entity> {
        let entity = |entry: Entry, params: Table| Entity {
            kind: format!("placed-{:04x}", entry.reference),
            id: None,
            // A u16 x and a bottom edge inside a level fit an i32.
            at: Vec2::ints(i32::from(entry.x), entry.level_y(bottom) as i32),
            params,
        };
        let mut entities = Vec::new();
        for (i, placement) in self.0.iter().enumerate() {
            match placement.entry.kind() {
                Class::Object => entities.push(entity(placement.entry, Table::new())),
                Class::Group => {
                    for &member in &placement.members {
                        if member.kind() != Class::Object {
                            continue;
                        }
                        let mut params = Table::new();
                        params.insert("group".into(), Value::Integer(i as i64));
                        params.insert(
                            "appears_at".into(),
                            Value::Integer(placement.entry.x.into()),
                        );
                        entities.push(entity(member, params));
                    }
                }
                Class::Nothing | Class::Other(_) => {}
            }
        }
        entities
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The vectors' bank fragment starts at this address.
    const BASE: u16 = 0x9000;
    const LIST: u16 = 0x9008;
    const BOTTOM: i64 = 512;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/placements/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The bank fragment's bytes, checking each line's address.
    fn fragment() -> Vec<u8> {
        let mut bytes = Vec::new();
        for line in vector("bank.hex").lines().filter(|l| !l.trim().is_empty()) {
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
        for (i, p) in placements.0.iter().enumerate() {
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
        // What appears, by index, with the x that decides when.
        let mut objects = Vec::new();
        for (i, p) in placements.0.iter().enumerate() {
            match p.entry.kind() {
                Class::Object => objects.push((i.to_string(), p.entry)),
                Class::Group => {
                    objects.extend((0..p.members.len()).map(|j| (format!("{i}.{j}"), p.entry)))
                }
                _ => {}
            }
        }
        for row in rows("appear.tsv") {
            let (_, decider) = objects
                .iter()
                .find(|(index, _)| *index == row[0])
                .unwrap_or_else(|| panic!("no object {}", row[0]));
            let frame = (0..300).find(|n| decider.in_window(-40 + 3 * n));
            let got = match frame {
                Some(n) => [n.to_string(), (-40 + 3 * n).to_string()],
                None => ["never".to_string(), "-".to_string()],
            };
            assert_eq!(&got[..], &row[1..], "object {}", row[0]);
        }
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
        let entities = read(&fragment()).unwrap().entities(BOTTOM);
        let summary: Vec<(String, Vec2, Option<i64>, Option<i64>)> = entities
            .iter()
            .map(|e| {
                let int = |k: &str| e.params.get(k).and_then(Value::as_integer);
                (e.kind.clone(), e.at, int("group"), int("appears_at"))
            })
            .collect();
        let at = Vec2::ints;
        let one = |kind: &str, x, y| (kind.to_string(), at(x, y), None, None);
        let member = |kind: &str, x, y| (kind.to_string(), at(x, y), Some(3), Some(500));
        assert_eq!(
            summary,
            [
                one("placed-1234", 16, 412),
                one("placed-2000", 40, 312),
                one("placed-1234", 40, 212),
                member("placed-1300", 500, 392),
                member("placed-1300", 560, 372),
                member("placed-1310", 420, 416),
                one("placed-1240", 700, 0),
                one("placed-1250", 650, 512),
                one("placed-ffff", 65000, 505),
            ]
        );
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
