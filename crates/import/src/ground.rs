//! A level's ground (`spec/formats/level-ground.md`): a grid of 32-px
//! cells in 32-line bands, each cell a type whose two halves have a shape,
//! read into where the surface is in each 1-px column and turned into
//! Clamber collision. A ceiling grid's halves can also be ceilings, with
//! ground above a line.
//!
//! Lines are level y in whole px, growing downward. A surface at line `L`
//! is stood on with the feet at `L`, and its ground is lines `L` + 1 to
//! its band's bottom line. As collision, that ground is a solid area from
//! y = `L` down to y = the band's bottom line.

use std::ops::Range;

use clamber_level::{Collider, Shape, Vec2};

/// Bands in a column grid.
pub const COLUMN_BANDS: u32 = 16;

/// Lines in a band, and px in a cell.
const CELL: i64 = 32;

/// Cell words in a row of a row grid.
const ROW_CELLS: i64 = 64;

/// The bits of a cell word that give its type.
const TYPE_MASK: u16 = 0x3FFF;
const MIRRORED: u16 = 1 << 14;

const SHAPE_MASK: u8 = 0x3F;
const PASSABLE: u8 = 1 << 6;
const FLIPPED: u8 = 1 << 7;

/// How a grid's cells are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// 16 bands tall, column by column.
    Columns,
    /// Row by row, 64 cells to a row, as many bands as there are rows.
    Rows,
    /// Stored as [`Layout::Rows`], with its own shapes, half of them
    /// ceilings.
    Ceilings,
}

/// A level's ground grid with its type table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ground {
    layout: Layout,
    cells: Vec<u16>,
    /// Each type's left and right half bytes.
    types: Vec<[u8; 2]>,
}

/// A surface in one band of a 1-px column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub line: i64,
    pub passable_from_below: bool,
}

/// What one half of a cell gives in a 1-px column of one band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Half {
    Empty,
    /// A floor, with its surface if it has one there.
    Floor(Option<Surface>),
    /// A ceiling (only in a ceiling grid): ground above `line`, which is
    /// `u` lines above the band's bottom line. `u` is 31 or less and can
    /// be below 0, putting the line below the band.
    Ceiling {
        line: i64,
        u: i64,
    },
}

impl Half {
    fn is_ceiling(self) -> bool {
        matches!(self, Half::Ceiling { .. })
    }
}

/// The surface that something at a point stands on or lands on next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Below {
    Line(i64),
    /// Higher than any point in the level.
    Above,
    None,
}

impl Ground {
    /// Reads a grid stored as `layout` and its type table, two bytes a
    /// type.
    pub fn new(layout: Layout, grid: &[u8], types: &[u8]) -> Result<Ground, String> {
        let unit = match layout {
            Layout::Columns => 2 * COLUMN_BANDS as usize,
            Layout::Rows | Layout::Ceilings => 2 * ROW_CELLS as usize,
        };
        if !grid.len().is_multiple_of(unit) {
            return Err(format!(
                "a ground grid of {} bytes isn't whole {}-byte {}",
                grid.len(),
                unit,
                match layout {
                    Layout::Columns => "columns",
                    Layout::Rows | Layout::Ceilings => "rows",
                }
            ));
        }
        if !types.len().is_multiple_of(2) {
            return Err(format!(
                "a ground type table of {} bytes isn't whole types",
                types.len()
            ));
        }
        Ok(Ground {
            layout,
            cells: grid
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&w| u16::from_le_bytes(w))
                .collect(),
            types: types.as_chunks::<2>().0.to_vec(),
        })
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// How many bands the grid has.
    pub fn bands(&self) -> i64 {
        match self.layout {
            Layout::Columns => i64::from(COLUMN_BANDS),
            Layout::Rows | Layout::Ceilings => self.cells.len() as i64 / ROW_CELLS,
        }
    }

    /// The level x the grid covers: its columns, or a row's width.
    pub fn width(&self) -> i64 {
        match self.layout {
            Layout::Columns => self.cells.len() as i64 / i64::from(COLUMN_BANDS) * CELL,
            Layout::Rows | Layout::Ceilings => ROW_CELLS * CELL,
        }
    }

    /// The cell word for level x `x` in `band`, if the grid has one.
    fn cell(&self, x: i64, band: i64) -> Option<u16> {
        if x < 0 || band < 0 {
            return None;
        }
        let index = match self.layout {
            Layout::Columns if band >= i64::from(COLUMN_BANDS) => return None,
            Layout::Columns => x / CELL * i64::from(COLUMN_BANDS) + band,
            // Past the end of a row, this reads on into the next one.
            Layout::Rows | Layout::Ceilings => band * ROW_CELLS + x / CELL,
        };
        self.cells.get(usize::try_from(index).ok()?).copied()
    }

    /// The half byte that level x column `x` uses in `band`, and the
    /// position in that half, unless the cell is empty.
    fn half_byte(&self, x: i64, band: i64) -> Option<(u8, i64)> {
        let word = self.cell(x, band).filter(|&w| w != 0)?;
        let halves = self.types.get(usize::from(word & TYPE_MASK))?;
        let mut p = x.rem_euclid(CELL);
        if word & MIRRORED != 0 {
            p = CELL - 1 - p;
        }
        let half = halves[usize::from(p >= CELL / 2)];
        let mut c = p % (CELL / 2);
        if half & FLIPPED != 0 {
            c = CELL / 2 - 1 - c;
        }
        Some((half, c))
    }

    /// The shape number of the half that level x column `x` uses in
    /// `band`, 0 if the cell is empty.
    fn shape(&self, x: i64, band: i64) -> u8 {
        self.half_byte(x, band)
            .map_or(0, |(half, _)| half & SHAPE_MASK)
    }

    /// What the half that level x column `x` uses in `band` gives there.
    pub fn half(&self, x: i64, band: i64) -> Half {
        let Some((half, c)) = self.half_byte(x, band) else {
            return Half::Empty;
        };
        let shape = half & SHAPE_MASK;
        let bottom = CELL * band + CELL;
        let floor = |rise: Option<i64>| {
            Half::Floor(rise.map(|rise| Surface {
                line: bottom - rise,
                passable_from_below: half & PASSABLE != 0,
            }))
        };
        match (self.layout, shape) {
            (_, 0) => Half::Empty,
            (Layout::Columns | Layout::Rows, _) => floor(rise(shape, c)),
            (Layout::Ceilings, 1..=31) => floor(
                ceiling_grid_value(shape, c)
                    .filter(|&v| v >= 0)
                    .map(|v| v.min(31)),
            ),
            (Layout::Ceilings, _) => {
                let u = match ceiling_grid_value(shape - 32, c) {
                    Some(v) if v >= 1 => 31 - v,
                    _ => 31,
                };
                Half::Ceiling {
                    line: bottom - u,
                    u,
                }
            }
        }
    }

    /// The surface in level x column `x` in `band`, if there is one. In a
    /// ceiling grid, only floors have surfaces.
    pub fn surface(&self, x: i64, band: i64) -> Option<Surface> {
        match self.half(x, band) {
            Half::Floor(surface) => surface,
            _ => None,
        }
    }

    /// The ground below level x column `x` for feet at line `feet`.
    pub fn below(&self, x: i64, feet: i64) -> Below {
        if self.layout == Layout::Ceilings {
            return self.below_in_ceilings(x, feet);
        }
        let rows = self.layout == Layout::Rows;
        let mut band = (feet - 1).div_euclid(CELL);
        if !rows {
            band = band.clamp(0, i64::from(COLUMN_BANDS) - 1);
        }
        let top = CELL * band + 1;
        let line = |s: Option<Surface>| s.map(|s| s.line);
        match line(self.surface(x, band)) {
            Some(here) if here != top => Below::Line(here),
            Some(_) => match line(self.surface(x, band - 1)) {
                _ if band == 0 && !rows => Below::Above,
                Some(1) if !rows => Below::Above,
                Some(up) => Below::Line(up),
                None => Below::Line(top),
            },
            None if rows && (x.div_euclid(CELL)) % 16 == 0 => Below::None,
            None => line(self.surface(x, band + 1))
                .or_else(|| line(self.surface(x, band + 2)))
                .map_or(Below::None, Below::Line),
        }
    }

    /// [`Ground::below`] in a ceiling grid.
    fn below_in_ceilings(&self, x: i64, feet: i64) -> Below {
        let band = (feet - 1).div_euclid(CELL);
        let top = CELL * band + 1;
        let here = self.half(x, band);
        match here {
            Half::Ceiling { line, u } if u >= 1 => return Below::Line(line),
            Half::Floor(Some(s)) if s.line != top => return Below::Line(s.line),
            Half::Floor(Some(_)) => {
                return Below::Line(match self.half(x, band - 1) {
                    Half::Ceiling { line, .. } => line - CELL,
                    Half::Floor(Some(s)) => s.line,
                    _ => top,
                });
            }
            _ => {}
        }
        // Nothing at the feet.
        let found = |half| match half {
            Half::Floor(Some(s)) => Some(s.line),
            Half::Ceiling { line, u } if u >= 0 => Some(line),
            _ => None,
        };
        let next = self.half(x, band + 1);
        if here.is_ceiling() != next.is_ceiling() {
            return if next == Half::Empty || self.shape(x, band + 1) == 32 {
                Below::Line(CELL * band + CELL)
            } else {
                Below::None
            };
        }
        found(next)
            .or_else(|| found(self.half(x, band + 2)))
            .map_or(Below::None, Below::Line)
    }

    /// The ground in level x `xs` as collision, every collider of surface
    /// type `surface`.
    ///
    /// Each cell's ground is a solid area under its surface, cut where
    /// the surface ends or its passability changes. Ground passable from
    /// below is one-way. A surface on its band's bottom line has no ground
    /// and is a one-way segment. A surface on its band's top line meets
    /// the ground of a surface in the band above, as it does for the
    /// ground below, so stacked cells have no gap between them.
    ///
    /// In a ceiling grid only the floors are collision: what a ceiling
    /// does to the player isn't specified yet.
    pub fn colliders(&self, xs: Range<i64>, surface: &str) -> Vec<Collider> {
        let mut colliders = Vec::new();
        let mut push = |shape| {
            colliders.push(Collider {
                shape,
                surface: surface.to_string(),
            })
        };
        let first = xs.start.div_euclid(CELL);
        let last = (xs.end - 1).div_euclid(CELL);
        for band in 0..self.bands() {
            let bottom = CELL * band + CELL;
            for cell in first..=last {
                let cell_xs = (cell * CELL).max(xs.start)..(cell * CELL + CELL).min(xs.end);
                // Each column's top of ground, and whether it's one-way.
                let tops: Vec<Option<(i64, bool)>> = cell_xs
                    .clone()
                    .map(|x| {
                        let s = self.surface(x, band)?;
                        let joins = s.line == bottom - CELL + 1
                            && band > 0
                            && self.surface(x, band - 1).is_some();
                        let top = if joins { bottom - CELL } else { s.line };
                        Some((top, s.passable_from_below))
                    })
                    .collect();
                let mut i = 0;
                while i < tops.len() {
                    let Some((top, one_way)) = tops[i] else {
                        i += 1;
                        continue;
                    };
                    let flat = top == bottom;
                    let mut end = i + 1;
                    while let Some(Some((t, o))) = tops.get(end) {
                        if (*t == bottom) != flat || (!flat && *o != one_way) {
                            break;
                        }
                        end += 1;
                    }
                    let x0 = cell_xs.start + i as i64;
                    let x1 = cell_xs.start + end as i64;
                    if flat {
                        push(Shape::Segment {
                            a: point(x0, bottom),
                            b: point(x1, bottom),
                            one_way: true,
                        });
                    } else {
                        let mut points = vec![point(x0, top)];
                        for j in i + 1..end {
                            let (before, after) = (tops[j - 1].unwrap().0, tops[j].unwrap().0);
                            if before != after {
                                let x = cell_xs.start + j as i64;
                                points.push(point(x, before));
                                points.push(point(x, after));
                            }
                        }
                        let last_top = tops[end - 1].unwrap().0;
                        points.extend([point(x1, last_top), point(x1, bottom), point(x0, bottom)]);
                        push(Shape::Solid { points, one_way });
                    }
                    i = end;
                }
            }
        }
        colliders
    }
}

fn point(x: i64, y: i64) -> Vec2 {
    // Grids are at most 64 Ki cells of 32 px, well inside an i32.
    Vec2::ints(x as i32, y as i32)
}

/// The value `v` of floor shape `shape` (0 to 31) of a ceiling grid at
/// position `c` (0 to 15) of a half, if the shape has one.
fn ceiling_grid_value(shape: u8, c: i64) -> Option<i64> {
    let s = i64::from(shape);
    Some(match shape {
        1..=4 => 31 - 8 * (s - 1),
        5..=12 => c / 4 + 4 * (s - 5),
        13..=16 => c / 2 + 8 * (s - 13),
        17..=21 => c + 8 * (s - 18),
        22..=26 => 4 * c + 16 * (s - 25),
        27 => (4 * c - 16).min(15),
        28 => (4 * c - 16).max(15),
        29 => 31,
        30 => 15,
        _ => return None,
    })
}

/// The rise of `shape` at position `c` (0 to 15) of a half, 0 to 31, or
/// `None` where it has no surface.
fn rise(shape: u8, c: i64) -> Option<i64> {
    let s = i64::from(shape);
    let v = match shape {
        1..=8 => 31 - 4 * (s - 1),
        9..=16 => c / 4 + 4 * (s - 9),
        17..=25 => c / 2 + 4 * (s - 18),
        26..=35 => (2 * (c + 1) + 2) / 3 + 4 * (s - 28),
        36..=46 => c + 4 * (s - 39),
        47..=53 => 2 * c + 8 * (s - 50),
        54..=58 => 4 * c + 16 * (s - 57),
        59 => (4 * c - 16).min(15),
        60 => (4 * c - 16).max(15),
        61 => 31,
        62 => 15,
        // 0 has no surface; 63 isn't defined.
        _ => return None,
    };
    (v >= 0).then_some(v.min(31))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use clamber_fixed::Fixed;
    use clamber_level::Collision;

    use super::*;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/level-ground/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn hex(name: &str) -> Vec<u8> {
        vector(name)
            .split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    fn ground(layout: Layout, grid: &str) -> Ground {
        Ground::new(layout, &hex(grid), &hex("types.hex")).unwrap()
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

    #[test]
    fn surfaces_match_the_spec_vectors() {
        for (layout, grid, table) in [
            (Layout::Columns, "grid.hex", "surfaces.tsv"),
            (Layout::Rows, "rows-grid.hex", "rows-surfaces.tsv"),
        ] {
            let ground = ground(layout, grid);
            for row in rows(table) {
                let x: i64 = row[0].parse().unwrap();
                for (band, want) in row[1..].iter().enumerate() {
                    let got = ground.surface(x, band as i64).map(|s| s.line);
                    match want.as_str() {
                        "past" => continue,
                        "none" => assert_eq!(got, None, "{table} x {x} band {band}"),
                        line => assert_eq!(got, line.parse().ok(), "{table} x {x} band {band}"),
                    }
                }
            }
        }
    }

    #[test]
    fn passability_matches_the_spec_vectors() {
        let ground = ground(Layout::Columns, "grid.hex");
        for row in rows("passable.tsv") {
            let x: i64 = row[0].parse().unwrap();
            for (band, want) in row[1..].iter().enumerate() {
                let got = match ground.surface(x, band as i64) {
                    None => "none",
                    Some(s) if s.passable_from_below => "yes",
                    Some(_) => "no",
                };
                assert_eq!(got, want, "x {x} band {band}");
            }
        }
    }

    #[test]
    fn the_ground_below_matches_the_spec_vectors() {
        for (layout, grid, table) in [
            (Layout::Columns, "grid.hex", "ground-below.tsv"),
            (Layout::Rows, "rows-grid.hex", "rows-ground-below.tsv"),
        ] {
            let ground = ground(layout, grid);
            for row in rows(table) {
                let x: i64 = row[0].parse().unwrap();
                let feet: i64 = row[1].parse().unwrap();
                let want = match row[2].as_str() {
                    "none" => Below::None,
                    "above" => Below::Above,
                    line => Below::Line(line.parse().unwrap()),
                };
                assert_eq!(ground.below(x, feet), want, "{table} x {x} feet {feet}");
            }
        }
    }

    fn ceilings() -> Ground {
        Ground::new(
            Layout::Ceilings,
            &hex("ceiling-grid.hex"),
            &hex("ceiling-types.hex"),
        )
        .unwrap()
    }

    #[test]
    fn ceiling_grid_halves_match_the_spec_vectors() {
        let ground = ceilings();
        for row in rows("ceiling-cells.tsv") {
            let x: i64 = row[0].parse().unwrap();
            for (band, want) in row[1..].iter().enumerate() {
                let got = match ground.half(x, band as i64) {
                    Half::Empty => "empty".to_string(),
                    Half::Floor(None) => "floor:none".to_string(),
                    Half::Floor(Some(s)) => format!("floor:{}", s.line),
                    Half::Ceiling { line, .. } => format!("ceiling:{line}"),
                };
                assert_eq!(&got, want, "x {x} band {band}");
            }
        }
    }

    #[test]
    fn the_ground_below_in_a_ceiling_grid_matches_the_spec_vectors() {
        let ground = ceilings();
        for row in rows("ceiling-ground-below.tsv") {
            let x: i64 = row[0].parse().unwrap();
            let feet: i64 = row[1].parse().unwrap();
            let want = match row[2].as_str() {
                "none" => Below::None,
                line => Below::Line(line.parse().unwrap()),
            };
            assert_eq!(ground.below(x, feet), want, "x {x} feet {feet}");
        }
    }

    #[test]
    fn the_ceiling_grid_example_in_the_format() {
        // Type 1 floor shape 3, type 2 ceiling shape 35, type 3 ceiling
        // shape 33.
        let types = [0, 0, 0x03, 0x03, 0x23, 0x23, 0x21, 0x21];
        let mut grid = vec![0; 3 * 128];
        grid[128] = 0x02;
        grid[130] = 0x03;
        grid[256] = 0x01;
        let ground = Ground::new(Layout::Ceilings, &grid, &types).unwrap();
        assert_eq!(ground.below(5, 40), Below::Line(48));
        assert_eq!(ground.below(5, 60), Below::Line(48));
        assert_eq!(ground.below(5, 70), Below::Line(81));
        assert_eq!(ground.below(5, 10), Below::None);
        assert_eq!(ground.below(37, 40), Below::Line(64));
        assert_eq!(ground.below(37, 10), Below::None);
        assert_eq!(ground.below(2053, 8), Below::Line(16));
    }

    #[test]
    fn the_synthetic_examples_in_the_format() {
        // Types 0 to 2: type 1 flat at rise 15, type 2 shape 12 with its
        // right half flipped.
        let types = [0, 0, 0x05, 0x05, 0x0C, 0x8C];
        let mut grid = vec![0; 32];
        grid[20] = 0x01;
        grid[22..24].copy_from_slice(&[0x02, 0x40]);
        let ground = Ground::new(Layout::Columns, &grid, &types).unwrap();
        let line = |x, band| ground.surface(x, band).map(|s| s.line);
        assert_eq!(line(3, 10), Some(337));
        assert_eq!(line(3, 11), Some(372));
        assert_eq!(line(20, 11), Some(370));
        assert_eq!(ground.below(3, 300), Below::Line(337));
        assert_eq!(ground.below(3, 360), Below::Line(372));
        assert_eq!(ground.below(3, 380), Below::Line(372));
    }

    #[test]
    fn grids_must_be_whole() {
        assert!(Ground::new(Layout::Columns, &[0; 31], &[]).is_err());
        assert!(Ground::new(Layout::Rows, &[0; 32], &[]).is_err());
        assert!(Ground::new(Layout::Columns, &[0; 32], &[0]).is_err());
        let rows = Ground::new(Layout::Rows, &[0; 3 * 128], &[]).unwrap();
        assert_eq!((rows.bands(), rows.width()), (3, 2048));
        assert_eq!(ground(Layout::Columns, "grid.hex").width(), 37 * 32);
    }

    /// Every surface in the vectors' grids is the top of collision in its
    /// column, one-way if it's passable from below, and every piece of
    /// collision is some surface's ground.
    #[test]
    fn collision_follows_the_surfaces() {
        for (layout, grid) in [
            (Layout::Columns, "grid.hex"),
            (Layout::Rows, "rows-grid.hex"),
        ] {
            let ground = ground(layout, grid);
            let collision = Collision::new(ground.colliders(0..ground.width(), "normal"));
            assert!(collision.colliders.iter().all(|c| c.surface == "normal"));
            for x in 0..ground.width() {
                let middle = Fixed::from_int(x as i32) + Fixed::from_ratio_pow2(1, 1);
                // Each span's bottom, by its top.
                let spans: BTreeMap<i64, (i64, bool)> = collision
                    .spans(middle)
                    .into_iter()
                    .map(|s| (s.top.floor(), (s.bottom.floor(), s.one_way)))
                    .collect();
                let mut found = 0;
                for band in 0..ground.bands() {
                    let Some(s) = ground.surface(x, band) else {
                        continue;
                    };
                    let bottom = CELL * band + CELL;
                    let joined =
                        s.line == bottom - CELL + 1 && ground.surface(x, band - 1).is_some();
                    let top = if joined { bottom - CELL } else { s.line };
                    let one_way = s.passable_from_below || s.line == bottom;
                    assert_eq!(
                        spans.get(&top),
                        Some(&(bottom, one_way)),
                        "{grid} x {x} band {band}"
                    );
                    found += 1;
                }
                assert_eq!(spans.len(), found, "{grid} x {x}: {spans:?}");
            }
        }
    }

    #[test]
    fn collision_is_cut_to_the_level() {
        let ground = ground(Layout::Columns, "grid.hex");
        let colliders = ground.colliders(40..70, "normal");
        assert!(!colliders.is_empty());
        for c in &colliders {
            let xs: Vec<Fixed> = match &c.shape {
                Shape::Solid { points, .. } => points.iter().map(|p| p.x).collect(),
                Shape::Segment { a, b, .. } => vec![a.x, b.x],
                _ => unreachable!(),
            };
            assert!(
                xs.iter()
                    .all(|&x| x >= Fixed::from_int(40) && x <= Fixed::from_int(70))
            );
        }
    }
}
