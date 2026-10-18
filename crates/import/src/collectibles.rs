//! A level's single collectibles (`spec/formats/collectibles.md`): its
//! group list places shapes from the shape table that all levels share,
//! and each cell of a shape that holds an item is one collectible, turned
//! into a Clamber level entity of the [`SPARK`] archetype.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use clamber_level::{Entity, Vec2};
use toml::Table;

/// The archetype of a single collectible, as in `content/base`.
pub const SPARK: &str = "spark";

/// Bytes in a group record.
pub const GROUP_SIZE: usize = 4;

/// The most rows a shape is known to have.
const MAX_ROWS: u8 = 16;

/// One record of a group list: a shape put at a level position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    /// Bit 0, which the spec doesn't cover when it's set.
    pub skip: bool,
    /// The shape's number in the shape table.
    pub shape: u16,
    /// The group's left edge, a level x in whole px.
    pub x: u16,
    /// The group's top edge, a level y in whole px.
    pub y: u16,
}

impl Group {
    fn read(bytes: &[u8]) -> Group {
        let n = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let bits = |from: u32, count: u32| ((n >> from) & ((1 << count) - 1)) as u16;
        Group {
            skip: n & 1 == 1,
            shape: bits(1, 9),
            y: 8 * bits(10, 11),
            x: 8 * bits(21, 11),
        }
    }
}

/// A shape: a grid of cells, some of which hold an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub columns: u8,
    pub rows: u8,
    /// Column by column from the left, each from the top row down.
    cells: Vec<bool>,
}

impl Shape {
    /// Reads shape `shape` of the table at `table`.
    fn read<'a>(
        bank: &impl Fn(u16) -> Option<&'a [u8]>,
        table: u16,
        shape: u16,
    ) -> Result<Shape, String> {
        let offset = bank(table.wrapping_add(2 * shape))
            .and_then(|b| b.get(..2))
            .ok_or_else(|| format!("shape {shape}'s offset is past the bank"))?;
        let start = table.wrapping_add(u16::from_le_bytes([offset[0], offset[1]]));
        let data = bank(start).ok_or_else(|| format!("shape {shape} is past the bank"))?;
        let (columns, rows) = match data {
            &[columns, rows, ..] => (columns, rows),
            _ => return Err(format!("shape {shape} runs past its bank")),
        };
        if columns == 0 || rows == 0 || rows > MAX_ROWS {
            return Err(format!(
                "shape {shape} has {columns} columns and {rows} rows, which the spec doesn't cover"
            ));
        }
        let count = usize::from(columns) * usize::from(rows);
        let words = data
            .get(2..2 + 2 * count.div_ceil(16))
            .ok_or_else(|| format!("shape {shape}'s cells run past its bank"))?;
        let bit = |i: usize| {
            let word = u16::from_le_bytes([words[2 * (i / 16)], words[2 * (i / 16) + 1]]);
            word >> (i % 16) & 1 == 1
        };
        // Within a column the bits go from the bottom row up.
        let h = usize::from(rows);
        let cells = (0..count)
            .map(|i| bit(i / h * h + (h - 1 - i % h)))
            .collect();
        Ok(Shape {
            columns,
            rows,
            cells,
        })
    }

    /// Whether the cell in `column`, `row` holds an item.
    pub fn holds(&self, column: u8, row: u8) -> bool {
        column < self.columns
            && row < self.rows
            && self.cells[usize::from(column) * usize::from(self.rows) + usize::from(row)]
    }
}

/// One item a group places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    /// The group's index in the list.
    pub group: usize,
    pub column: u8,
    pub row: u8,
    /// The level x of its image's leftmost column.
    pub left: i32,
    /// The level y of its image's top row.
    pub top: i32,
}

impl Item {
    /// Its position (`xc`, `yc`), 8 px right of and below its image's top
    /// left (`spec/behavior/collectibles.md`, "A collectible's position").
    pub fn position(self) -> Vec2 {
        Vec2::ints(self.left + 8, self.top + 8)
    }
}

/// A level's single collectibles: its group list in stored order, and the
/// items its groups place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collectibles {
    pub groups: Vec<Group>,
    /// By group, then by column, then by row.
    pub items: Vec<Item>,
}

impl Collectibles {
    /// Reads the group list at address `list`, in the bank `groups` gives,
    /// and the shapes its groups name from the shape table at address
    /// `table`, in the bank `shapes` gives. Each gives the bytes of its
    /// bank from an address to the bank's end, or `None` where the bank
    /// doesn't have that address.
    ///
    /// The spec doesn't cover groups whose `skip` bit is set; with the bit
    /// set, a group's items weren't drawn, so as the engine's choice such
    /// a group places none.
    pub fn read<'a, 'b>(
        groups: impl Fn(u16) -> Option<&'a [u8]>,
        list: u16,
        shapes: impl Fn(u16) -> Option<&'b [u8]>,
        table: u16,
    ) -> Result<Collectibles, String> {
        let mut out = Collectibles::default();
        let mut at = list;
        loop {
            let bytes = groups(at)
                .and_then(|b| b.get(..GROUP_SIZE))
                .ok_or_else(|| format!("the group list at {list:04x} runs past its bank"))?;
            if bytes[..2] == [0, 0] {
                break;
            }
            out.groups.push(Group::read(bytes));
            at = at
                .checked_add(GROUP_SIZE as u16)
                .ok_or_else(|| format!("the group list at {list:04x} runs past its bank"))?;
        }
        let mut read = BTreeMap::new();
        for (i, group) in out.groups.iter().enumerate() {
            if group.skip {
                continue;
            }
            let shape = match read.entry(group.shape) {
                Entry::Occupied(shape) => shape.into_mut(),
                Entry::Vacant(e) => e.insert(Shape::read(&shapes, table, group.shape)?),
            };
            for column in 0..shape.columns {
                for row in (0..shape.rows).filter(|&r| shape.holds(column, r)) {
                    out.items.push(Item {
                        group: i,
                        column,
                        row,
                        left: i32::from(group.x) + 8 * i32::from(column),
                        top: i32::from(group.y) + 8 * i32::from(row) + 1,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Each item as a [`SPARK`] entity at its position.
    pub fn entities(&self) -> Vec<Entity> {
        self.items
            .iter()
            .map(|item| Entity {
                kind: SPARK.to_string(),
                id: None,
                at: item.position(),
                params: Table::new(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the vectors' bank fragment starts, and its lists and table.
    const BASE: u16 = 0x6000;
    const TABLE: u16 = 0x6030;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/collectibles/{name}",
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

    fn read(bytes: &[u8], list: u16) -> Result<Collectibles, String> {
        let bank = |a: u16| bytes.get(usize::from(a.checked_sub(BASE)?)..);
        Collectibles::read(bank, list, bank, TABLE)
    }

    /// The rows of a table after its header, split at tabs.
    fn rows(name: &str) -> Vec<Vec<String>> {
        vector(name)
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect()
    }

    /// The lists named in a table's first column, in order.
    fn lists(rows: &[Vec<String>]) -> Vec<u16> {
        let mut lists: Vec<u16> = rows
            .iter()
            .map(|r| u16::from_str_radix(&r[0], 16).unwrap())
            .collect();
        lists.dedup();
        lists
    }

    #[test]
    fn groups_match_the_spec_vectors() {
        let bytes = fragment();
        let want = rows("groups.tsv");
        let mut got = Vec::new();
        for list in [0x6000, 0x6020] {
            let read = read(&bytes, list).unwrap();
            for (i, g) in read.groups.iter().enumerate() {
                got.push(vec![
                    format!("{list:04x}"),
                    i.to_string(),
                    u8::from(g.skip).to_string(),
                    g.shape.to_string(),
                    g.x.to_string(),
                    g.y.to_string(),
                ]);
            }
        }
        assert_eq!(got, want);
        assert_eq!(lists(&want), [0x6000]);
        assert!(read(&bytes, 0x6020).unwrap().groups.is_empty());
    }

    #[test]
    fn items_match_the_spec_vectors() {
        let bytes = fragment();
        let want = rows("items.tsv");
        let mut got = Vec::new();
        for list in lists(&want) {
            for item in read(&bytes, list).unwrap().items {
                let at = item.position();
                got.push(vec![
                    format!("{list:04x}"),
                    item.group.to_string(),
                    item.column.to_string(),
                    item.row.to_string(),
                    item.left.to_string(),
                    item.top.to_string(),
                    at.x.floor().to_string(),
                    at.y.floor().to_string(),
                ]);
            }
        }
        assert_eq!(got, want);
    }

    /// "Example (synthetic)": shape 2 and the group `04 64 40 0c`.
    #[test]
    fn the_spec_example() {
        let read = read(&fragment(), BASE).unwrap();
        assert_eq!(
            read.groups[0],
            Group {
                skip: false,
                shape: 2,
                x: 784,
                y: 200,
            }
        );
        let cells: Vec<(u8, u8)> = read
            .items
            .iter()
            .filter(|i| i.group == 0)
            .map(|i| (i.column, i.row))
            .collect();
        assert_eq!(
            cells,
            [
                (0, 0),
                (0, 1),
                (0, 4),
                (1, 2),
                (1, 3),
                (2, 0),
                (2, 3),
                (2, 4)
            ]
        );
        let item = read.items.iter().find(|i| (i.column, i.row) == (1, 2));
        let item = item.unwrap();
        assert_eq!((item.left, item.top), (792, 217));
        assert_eq!(item.position(), Vec2::ints(800, 225));
    }

    #[test]
    fn a_skipped_group_places_nothing_and_a_bad_shape_is_refused() {
        // A group of shape 0 at (8, 8), with and without its skip bit,
        // then the end; shape 0 has one cell, and shape 1 no rows.
        let group = |skip: u32, shape: u32| (skip | shape << 1 | 1 << 10 | 1 << 21).to_le_bytes();
        let mut bytes = Vec::new();
        bytes.extend(group(1, 0));
        bytes.extend(group(0, 0));
        bytes.extend([0; 4]);
        let table = bytes.len() as u16;
        bytes.extend([4, 0, 7, 0, 1, 1, 1, 0, 1, 0]);
        let bank = |a: u16| bytes.get(usize::from(a)..);
        let read = Collectibles::read(bank, 0, bank, table).unwrap();
        assert_eq!(read.groups.len(), 2);
        assert!(read.groups[0].skip);
        assert_eq!(read.items.len(), 1);
        assert_eq!(read.items[0].group, 1);
        let entities = read.entities();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].kind, SPARK);
        assert_eq!(entities[0].at, Vec2::ints(16, 17));

        let mut bad = bytes.clone();
        bad[4..8].copy_from_slice(&group(0, 1));
        let bank = |a: u16| bad.get(usize::from(a)..);
        let err = Collectibles::read(bank, 0, bank, table).unwrap_err();
        assert!(err.contains("shape 1"), "{err}");
    }
}
