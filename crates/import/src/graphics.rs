//! A level's background pictures (`spec/formats/graphics.md`): colours
//! and palette rows, tiles, packed tile data, tile entries, each ground
//! type's cell picture, and the grid picture made of them, turned into
//! Clamber level layers drawn from a tileset image.
//!
//! The grid is the ground grid of `ground.rs`, 16 bands tall, column by
//! column: a cell's picture comes from the same cell word as its ground.

use std::collections::BTreeMap;

use clamber_fixed::Fixed;
use clamber_level::{Depth, Layer, LayerContent, Rgb, Tile, TileLayer, Tileset, Vec2};
use clamber_sprite::{Rgba, RgbaImage, encode_png};

use crate::COLUMN_BANDS;

/// px in a tile, either way.
pub const TILE: u32 = 8;

/// Tile entries in a cell picture, either way.
const CELL_SQUARES: usize = 4;

/// Tile entries down a grid picture's column.
const COLUMN_SQUARES: usize = CELL_SQUARES * COLUMN_BANDS as usize;

/// Grid columns, and bands, that the far layer is made of.
const FAR_COLUMNS: usize = 16;
const FAR_BANDS: usize = 8;

/// Tiles across a tileset image that [`Background::new`] writes.
const TILESET_COLUMNS: u32 = 32;

/// A colour's red, green and blue, each 0 to 31.
pub fn channels(colour: u16) -> [u8; 3] {
    [0, 5, 10].map(|shift| (colour >> shift & 0x1F) as u8)
}

/// A colour as 8-bit red, green and blue, each channel's intensity
/// rounded to the nearest of 256 steps.
pub fn rgb(colour: u16) -> Rgb {
    channels(colour).map(|c| ((u32::from(c) * 255 + 15) / 31) as u8)
}

/// 8 palette rows of 16 colours. Colour 0 of a row is never drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteSet {
    rows: [[u16; 16]; 8],
}

impl PaletteSet {
    pub fn new(bytes: &[u8]) -> Result<PaletteSet, String> {
        let words = words(bytes);
        if words.len() != 128 {
            return Err(format!("a palette set is 256 bytes, not {}", bytes.len()));
        }
        let mut rows = [[0; 16]; 8];
        for (row, colours) in rows.iter_mut().zip(words.chunks(16)) {
            row.copy_from_slice(colours);
        }
        Ok(PaletteSet { rows })
    }

    /// Colour `number` of palette row `row`, as stored.
    pub fn colour(&self, row: u8, number: u8) -> u16 {
        self.rows[usize::from(row & 7)][usize::from(number & 15)]
    }

    /// What a pixel of colour `number` in palette row `row` shows.
    pub fn rgba(&self, row: u8, number: u8) -> Rgba {
        if number == 0 {
            return [0; 4];
        }
        let [r, g, b] = rgb(self.colour(row, number));
        [r, g, b, 255]
    }
}

/// Unpacks packed tile data: a 128-byte pair table, then commands up to
/// the end command. Bytes after the end are ignored.
pub fn unpack(packed: &[u8]) -> Result<Vec<u8>, String> {
    let short = || "packed tile data ends before its end command".to_string();
    let pairs = packed.get(..128).ok_or_else(short)?;
    let mut stream = packed[128..].iter().copied();
    let mut next = || stream.next().ok_or_else(short);
    let mut out = Vec::new();
    loop {
        let b = next()?;
        let n = usize::from(b & 0x3F);
        match b >> 6 {
            0 if b == 0 => return Ok(out),
            0 => {
                for _ in 0..n {
                    out.push(next()?);
                }
            }
            1 => {
                let byte = next()?;
                out.extend(std::iter::repeat_n(byte, n));
            }
            2 => {
                let q = usize::from(u16::from_le_bytes([next()?, next()?]));
                if q >= out.len() {
                    return Err(format!(
                        "packed tile data copies from position {q} of an output of {} bytes",
                        out.len()
                    ));
                }
                for i in q..q + n {
                    out.push(out[i]);
                }
            }
            _ => out.extend_from_slice(&pairs[2 * n..2 * n + 2]),
        }
    }
}

/// Tiles, numbered from 0, each pixel's colour number row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileSet {
    tiles: Vec<[u8; 64]>,
}

impl TileSet {
    /// Reads four-plane tiles, 32 bytes each.
    pub fn new(bytes: &[u8]) -> Result<TileSet, String> {
        TileSet::read(bytes, 4)
    }

    /// Reads two-plane tiles, 16 bytes each.
    pub fn two_plane(bytes: &[u8]) -> Result<TileSet, String> {
        TileSet::read(bytes, 2)
    }

    fn read(bytes: &[u8], planes: usize) -> Result<TileSet, String> {
        let size = 8 * planes;
        if !bytes.len().is_multiple_of(size) {
            return Err(format!(
                "a tile set of {} bytes isn't whole {size}-byte tiles",
                bytes.len()
            ));
        }
        let tiles = bytes
            .chunks(size)
            .map(|tile| {
                let mut pixels = [0; 64];
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let (x, y) = (i % 8, i / 8);
                    for p in 0..planes {
                        let byte = tile[16 * (p / 2) + 2 * y + p % 2];
                        *pixel |= (byte >> (7 - x) & 1) << p;
                    }
                }
                pixels
            })
            .collect();
        Ok(TileSet { tiles })
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The colour number at column `x`, row `y` of tile `tile`.
    pub fn pixel(&self, tile: u16, x: u32, y: u32) -> Option<u8> {
        let pixels = self.tiles.get(usize::from(tile))?;
        Some(pixels[(y * TILE + x) as usize])
    }

    fn blank(&self, tile: u16) -> bool {
        self.tiles
            .get(usize::from(tile))
            .is_some_and(|p| p.iter().all(|&c| c == 0))
    }
}

/// Which tile to draw in an 8 × 8 px square, and how.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileEntry(pub u16);

impl TileEntry {
    pub const RAISED: u16 = 1 << 13;
    pub const FLIP_LR: u16 = 1 << 14;
    pub const FLIP_TB: u16 = 1 << 15;

    pub fn tile(self) -> u16 {
        self.0 & 0x3FF
    }

    pub fn palette_row(self) -> u8 {
        (self.0 >> 10 & 7) as u8
    }

    /// Drawn after the other layers' tiles that aren't raised.
    pub fn raised(self) -> bool {
        self.0 & TileEntry::RAISED != 0
    }

    pub fn flip_lr(self) -> bool {
        self.0 & TileEntry::FLIP_LR != 0
    }

    pub fn flip_tb(self) -> bool {
        self.0 & TileEntry::FLIP_TB != 0
    }
}

/// Each type's 32 × 32 px picture: 4 × 4 tile entries, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pictures {
    pictures: Vec<[TileEntry; 16]>,
}

impl Pictures {
    pub fn new(table: &[u8]) -> Result<Pictures, String> {
        if !table.len().is_multiple_of(32) {
            return Err(format!(
                "a picture table of {} bytes isn't whole 32-byte pictures",
                table.len()
            ));
        }
        let pictures = words(table)
            .chunks(16)
            .map(|p| std::array::from_fn(|i| TileEntry(p[i])))
            .collect();
        Ok(Pictures { pictures })
    }

    /// The tile entry at square (`cx`, `cy`) of a cell with cell word
    /// `word`, or `None` if its type has no picture.
    pub fn entry(&self, word: u16, cx: usize, cy: usize) -> Option<TileEntry> {
        let picture = self.pictures.get(usize::from(word & 0x3FFF))?;
        let (mut x, mut y, mut flips) = (cx, cy, 0);
        if word & 1 << 14 != 0 {
            x = CELL_SQUARES - 1 - x;
            flips |= TileEntry::FLIP_LR;
        }
        if word & 1 << 15 != 0 {
            y = CELL_SQUARES - 1 - y;
            flips |= TileEntry::FLIP_TB;
        }
        Some(TileEntry(picture[CELL_SQUARES * y + x].0 ^ flips))
    }
}

/// The tile entries of the grid's cells: 4 squares across a column, 64
/// down, square (0, 0) at the grid picture's top left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridPicture {
    columns: usize,
    /// Column of squares by column of squares.
    entries: Vec<TileEntry>,
}

impl GridPicture {
    /// The picture of a column grid (`ground.rs`'s `Layout::Columns`).
    pub fn new(grid: &[u8], pictures: &Pictures) -> Result<GridPicture, String> {
        let cells = words(grid);
        let bands = COLUMN_BANDS as usize;
        if !grid.len().is_multiple_of(2 * bands) {
            return Err(format!(
                "a ground grid of {} bytes isn't whole {}-byte columns",
                grid.len(),
                2 * bands
            ));
        }
        let columns = cells.len() / bands;
        let mut entries = vec![TileEntry::default(); columns * CELL_SQUARES * COLUMN_SQUARES];
        for (i, &word) in cells.iter().enumerate() {
            let (column, band) = (i / bands, i % bands);
            for cx in 0..CELL_SQUARES {
                for cy in 0..CELL_SQUARES {
                    let entry = pictures.entry(word, cx, cy).ok_or_else(|| {
                        format!(
                            "column {column} band {band} is type {}, which has no picture",
                            word & 0x3FFF
                        )
                    })?;
                    let sx = column * CELL_SQUARES + cx;
                    entries[sx * COLUMN_SQUARES + band * CELL_SQUARES + cy] = entry;
                }
            }
        }
        Ok(GridPicture { columns, entries })
    }

    /// Squares across.
    pub fn width(&self) -> usize {
        self.columns * CELL_SQUARES
    }

    /// Squares down.
    pub fn height(&self) -> usize {
        COLUMN_SQUARES
    }

    /// The entry for the square at square column `sx`, row `sy`.
    pub fn entry(&self, sx: usize, sy: usize) -> TileEntry {
        assert!(
            sx < self.width() && sy < self.height(),
            "square out of bounds"
        );
        self.entries[sx * COLUMN_SQUARES + sy]
    }
}

/// The pixel that `entry` shows at column `x`, row `y` of its square.
fn shown(tiles: &TileSet, palettes: &PaletteSet, entry: TileEntry, x: u32, y: u32) -> Rgba {
    let x = if entry.flip_lr() { TILE - 1 - x } else { x };
    let y = if entry.flip_tb() { TILE - 1 - y } else { y };
    let number = tiles.pixel(entry.tile(), x, y).unwrap_or(0);
    palettes.rgba(entry.palette_row(), number)
}

/// Squares `squares` of the grid picture's top `bands` bands as an image,
/// drawing only the squares whose entry is `raised` or not.
fn draw(
    grid: &GridPicture,
    tiles: &TileSet,
    palettes: &PaletteSet,
    squares: std::ops::Range<usize>,
    bands: usize,
    raised: bool,
) -> RgbaImage {
    let rows = bands * CELL_SQUARES;
    let mut image = RgbaImage::new(squares.len() as u32 * TILE, rows as u32 * TILE);
    for (i, sx) in squares.enumerate() {
        for sy in 0..rows {
            let entry = grid.entry(sx, sy);
            if entry.raised() != raised {
                continue;
            }
            for y in 0..TILE {
                for x in 0..TILE {
                    let p = shown(tiles, palettes, entry, x, y);
                    image.set(i as u32 * TILE + x, sy as u32 * TILE + y, p);
                }
            }
        }
    }
    image
}

/// A level's background as Clamber layers, with the images they use.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    /// From the back: the far layer, the ground layer, then the raised
    /// tiles of each where there are any.
    pub layers: Vec<Layer>,
    /// PNG files by path, relative to the level file.
    pub images: BTreeMap<String, Vec<u8>>,
}

impl Background {
    /// The far and ground layers, their images named starting `prefix`.
    ///
    /// The ground layer is a tile layer that moves with the view, its
    /// tileset holding each tile in each palette row it's drawn with. The
    /// far layer is a picture repeating both ways.
    ///
    /// The spec doesn't yet say how the far layer moves with the view, so
    /// it scrolls at half the view's speed for now; nor where characters
    /// are drawn against the layers, so all of them are behind entities.
    pub fn new(
        grid: &GridPicture,
        tiles: &TileSet,
        palettes: &PaletteSet,
        prefix: &str,
    ) -> Result<Background, String> {
        if grid.columns < FAR_COLUMNS {
            return Err(format!(
                "a grid of {} columns has no far layer, which needs {FAR_COLUMNS}",
                grid.columns
            ));
        }
        let mut images = BTreeMap::new();
        let mut back = Vec::new();
        let mut front = Vec::new();

        let far = (grid.columns - FAR_COLUMNS) * CELL_SQUARES..grid.width();
        for raised in [false, true] {
            let any = far
                .clone()
                .any(|sx| (0..FAR_BANDS * CELL_SQUARES).any(|sy| grid.entry(sx, sy).raised()));
            if raised && !any {
                continue;
            }
            let name = if raised { "far-raised" } else { "far" };
            let path = format!("{prefix}{name}.png");
            let image = draw(grid, tiles, palettes, far.clone(), FAR_BANDS, raised);
            images.insert(path.clone(), encode_png(&image));
            let half = Fixed::from_ratio_pow2(1, 1);
            (if raised { &mut front } else { &mut back }).push(layer(
                name,
                Vec2::new(half, half),
                Vec2::default(),
                LayerContent::Image {
                    path,
                    normals: None,
                    repeat_x: true,
                    repeat_y: true,
                },
            ));
        }

        // The tileset: each tile and palette row a square shows, in the
        // order first met, without blank tiles.
        let mut index: BTreeMap<(u16, u8), u32> = BTreeMap::new();
        let mut order = Vec::new();
        let mut ground = [Vec::new(), Vec::new()];
        for sy in 0..grid.height() {
            for sx in 0..grid.width() {
                let entry = grid.entry(sx, sy);
                let mut tile = [Tile(0), Tile(0)];
                if !tiles.blank(entry.tile()) {
                    if usize::from(entry.tile()) >= tiles.len() {
                        return Err(format!(
                            "a square shows tile {}, past the {} tiles",
                            entry.tile(),
                            tiles.len()
                        ));
                    }
                    let key = (entry.tile(), entry.palette_row());
                    let i = *index.entry(key).or_insert_with(|| {
                        order.push(key);
                        order.len() as u32 - 1
                    });
                    let mut t = i + 1;
                    if entry.flip_lr() {
                        t |= Tile::FLIP_X;
                    }
                    if entry.flip_tb() {
                        t |= Tile::FLIP_Y;
                    }
                    tile[usize::from(entry.raised())] = Tile(t);
                }
                ground[0].push(tile[0]);
                ground[1].push(tile[1]);
            }
        }
        let tileset_path = format!("{prefix}tiles.png");
        let down = (order.len() as u32).div_ceil(TILESET_COLUMNS).max(1);
        let mut tileset = RgbaImage::new(TILESET_COLUMNS * TILE, down * TILE);
        for (i, &(tile, row)) in order.iter().enumerate() {
            let (tx, ty) = (i as u32 % TILESET_COLUMNS, i as u32 / TILESET_COLUMNS);
            for y in 0..TILE {
                for x in 0..TILE {
                    let p = shown(
                        tiles,
                        palettes,
                        TileEntry(tile | u16::from(row) << 10),
                        x,
                        y,
                    );
                    tileset.set(tx * TILE + x, ty * TILE + y, p);
                }
            }
        }
        images.insert(tileset_path.clone(), encode_png(&tileset));
        for (raised, tiles) in ground.into_iter().enumerate() {
            if raised == 1 && tiles.iter().all(|&t| t == Tile(0)) {
                continue;
            }
            let name = if raised == 1 {
                "ground-raised"
            } else {
                "ground"
            };
            // Grid picture row r is at level y r - 1.
            (if raised == 1 { &mut front } else { &mut back }).push(layer(
                name,
                Vec2::ints(1, 1),
                Vec2::ints(0, -1),
                LayerContent::Tiles(TileLayer {
                    tileset: Tileset {
                        image: tileset_path.clone(),
                        normals: None,
                        tile_width: TILE,
                        tile_height: TILE,
                        margin: 0,
                        spacing: 0,
                    },
                    columns: grid.width() as u32,
                    tiles,
                }),
            ));
        }
        back.append(&mut front);
        Ok(Background {
            layers: back,
            images,
        })
    }
}

fn layer(name: &str, parallax: Vec2, offset: Vec2, content: LayerContent) -> Layer {
    Layer {
        name: name.to_string(),
        depth: Depth::Back,
        parallax,
        offset,
        content,
        scale: 1,
        pixel_art: true,
        // Its colours are the background's rows, which the dimming
        // changes (`spec/behavior/dimming.md`).
        dim: true,
    }
}

/// 16-bit words stored low byte first. A last odd byte is dropped.
fn words(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&w| u16::from_le_bytes(w))
        .collect()
}

#[cfg(test)]
mod tests {
    use clamber_sprite::{Image, decode_png};

    use super::*;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/graphics/{name}",
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

    fn num<T: std::str::FromStr>(s: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        s.parse().unwrap()
    }

    #[test]
    fn unpacking_matches_the_spec_vectors() {
        for name in [
            "empty",
            "literals",
            "fills",
            "copy",
            "copy-overlap",
            "pairs",
            "mixed",
        ] {
            let packed = hex(&format!("packed/{name}.in.hex"));
            let want = hex(&format!("packed/{name}.out.hex"));
            assert_eq!(unpack(&packed).unwrap(), want, "{name}");
        }
    }

    #[test]
    fn unpacking_refuses_broken_data() {
        assert!(unpack(&[0; 127]).is_err());
        // No end command.
        assert!(unpack(&[[0; 128].as_slice(), &[0x02, 1]].concat()).is_err());
        // A copy from past the output.
        assert!(unpack(&[[0; 128].as_slice(), &[0x01, 7, 0x82, 1, 0, 0]].concat()).is_err());
    }

    #[test]
    fn the_synthetic_example_in_the_format() {
        let mut packed = vec![0; 128];
        packed[10..12].copy_from_slice(&[0x7e, 0x7f]);
        packed.extend([
            0x03, 0x0a, 0x0b, 0x0c, 0x42, 0xff, 0x85, 0x01, 0x00, 0xc5, 0x00,
        ]);
        assert_eq!(
            unpack(&packed).unwrap(),
            [
                0x0a, 0x0b, 0x0c, 0xff, 0xff, 0x0b, 0x0c, 0xff, 0xff, 0x0b, 0x7e, 0x7f
            ]
        );

        let mut tile = [0; 32];
        tile[0] = 0x80;
        tile[16] = 0x80;
        let tiles = TileSet::new(&tile).unwrap();
        assert_eq!(tiles.pixel(0, 0, 0), Some(5));
        assert!((1..64).all(|i| tiles.pixel(0, i % 8, i / 8) == Some(0)));

        let e = TileEntry(0x6A5B);
        assert_eq!((e.tile(), e.palette_row()), (603, 2));
        assert!(e.raised() && e.flip_lr() && !e.flip_tb());

        let mut table = vec![0; 4 * 32];
        table[96..98].copy_from_slice(&[0x10, 0x00]);
        table[102..104].copy_from_slice(&[0x11, 0x40]);
        let pictures = Pictures::new(&table).unwrap();
        assert_eq!(pictures.entry(0x4003, 0, 0), Some(TileEntry(0x0011)));
        assert_eq!(pictures.entry(0x4003, 3, 0), Some(TileEntry(0x4010)));
    }

    #[test]
    fn tiles_match_the_spec_vectors() {
        let bytes = hex("tiles.hex");
        let four = TileSet::new(&bytes[..4 * 32]).unwrap();
        let two = TileSet::two_plane(&bytes[4 * 32..]).unwrap();
        assert_eq!((four.len(), two.len()), (4, 1));
        let text = vector("tiles.txt");
        let mut tables = text.split('#').filter(|t| !t.trim().is_empty());
        for t in 0..5u16 {
            let table = tables.next().unwrap();
            let (set, tile) = if t < 4 { (&four, t) } else { (&two, 0) };
            for (y, line) in table.lines().skip(1).enumerate() {
                for (x, c) in line.split_whitespace().enumerate() {
                    assert_eq!(
                        set.pixel(tile, x as u32, y as u32),
                        Some(u8::from_str_radix(c, 16).unwrap()),
                        "tile {t} x {x} y {y}"
                    );
                }
            }
        }
        assert!(TileSet::new(&[0; 33]).is_err());
        assert!(TileSet::two_plane(&[0; 17]).is_err());
    }

    #[test]
    fn colours_match_the_spec_vectors() {
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        let rows = rows("palette.tsv");
        assert_eq!(rows.len(), 128);
        for row in rows {
            let (r, n): (u8, u8) = (num(&row[0]), num(&row[1]));
            let want: [u8; 3] = [num(&row[2]), num(&row[3]), num(&row[4])];
            assert_eq!(channels(palettes.colour(r, n)), want, "row {r} colour {n}");
        }
        assert!(PaletteSet::new(&[0; 255]).is_err());
        assert_eq!(rgb(0x7FFF), [255; 3]);
        assert_eq!(rgb(0x8000), [0; 3]);
        assert_eq!(rgb(0x0010), [132, 0, 0]);
        assert_eq!(palettes.rgba(1, 0), [0; 4]);
    }

    #[test]
    fn entries_match_the_spec_vectors() {
        for row in rows("entries.tsv") {
            let e = TileEntry(u16::from_str_radix(&row[0], 16).unwrap());
            let got = (
                e.tile(),
                e.palette_row(),
                e.raised(),
                e.flip_lr(),
                e.flip_tb(),
            );
            let flag = |s: &String| s == "1";
            let want = (
                num(&row[1]),
                num(&row[2]),
                flag(&row[3]),
                flag(&row[4]),
                flag(&row[5]),
            );
            assert_eq!(got, want, "entry {}", row[0]);
        }
    }

    fn grid_picture() -> GridPicture {
        let pictures = Pictures::new(&hex("pictures.hex")).unwrap();
        GridPicture::new(&hex("grid.hex"), &pictures).unwrap()
    }

    #[test]
    fn cells_match_the_spec_vectors() {
        let grid = grid_picture();
        assert_eq!((grid.width(), grid.height()), (32, 64));
        let rows = rows("cells.tsv");
        assert_eq!(rows.len(), 128);
        for row in rows {
            let (x, y): (usize, usize) = (num(&row[0]), num(&row[1]));
            let want = TileEntry(u16::from_str_radix(&row[2], 16).unwrap());
            assert_eq!(grid.entry(x / 8, y / 8), want, "x {x} y {y}");
        }
    }

    #[test]
    fn grids_need_a_picture_for_each_type() {
        let pictures = Pictures::new(&hex("pictures.hex")).unwrap();
        let mut grid = vec![0; 32];
        grid[0] = 6;
        assert!(GridPicture::new(&grid, &pictures).is_err());
        assert!(GridPicture::new(&[0; 31], &pictures).is_err());
        assert!(Pictures::new(&[0; 31]).is_err());
    }

    fn rgba(png: &[u8]) -> RgbaImage {
        match decode_png(png).unwrap() {
            Image::Rgba(image) => image,
            _ => unreachable!(),
        }
    }

    /// A grid of `columns` columns whose cells use every vector picture
    /// in turn, every way flipped, with the vector palettes.
    fn sample(columns: usize) -> (GridPicture, TileSet, PaletteSet) {
        let pictures = Pictures::new(&hex("pictures.hex")).unwrap();
        let grid: Vec<u8> = (0..columns * 16)
            .flat_map(|i| (((i % 6) as u16) | ((i / 6 % 4) as u16) << 14).to_le_bytes())
            .collect();
        let grid = GridPicture::new(&grid, &pictures).unwrap();
        // The pictures show tiles up to 95: the vector tiles over and
        // over, so some are blank.
        let tiles = TileSet::new(&hex("tiles.hex")[..4 * 32].repeat(32)).unwrap();
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        (grid, tiles, palettes)
    }

    /// Drawing the ground layers' tiles from the tileset gives the grid
    /// picture, a pixel at a time.
    #[test]
    fn the_ground_layers_draw_the_grid_picture() {
        let (grid, tiles, palettes) = sample(17);
        let background = Background::new(&grid, &tiles, &palettes, "bg/").unwrap();
        let names: Vec<&str> = background.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["far", "ground", "far-raised", "ground-raised"]);
        let tileset = rgba(&background.images["bg/tiles.png"]);
        let mut seen = 0;
        for l in background
            .layers
            .iter()
            .filter(|l| l.name.starts_with("ground"))
        {
            assert_eq!(
                (l.parallax, l.offset),
                (Vec2::ints(1, 1), Vec2::ints(0, -1))
            );
            let LayerContent::Tiles(tl) = &l.content else {
                panic!("{} isn't tiles", l.name);
            };
            assert_eq!(tl.tileset.image, "bg/tiles.png");
            assert_eq!(tl.columns as usize, grid.width());
            let raised = l.name == "ground-raised";
            for sy in 0..grid.height() {
                for sx in 0..grid.width() {
                    let entry = grid.entry(sx, sy);
                    let tile = tl.tiles[sy * grid.width() + sx];
                    for y in 0..TILE {
                        for x in 0..TILE {
                            let want = if entry.raised() == raised {
                                shown(&tiles, &palettes, entry, x, y)
                            } else {
                                [0; 4]
                            };
                            let got = match tile.index() {
                                None => [0; 4],
                                Some(i) => {
                                    let tx = if tile.0 & Tile::FLIP_X != 0 { 7 - x } else { x };
                                    let ty = if tile.0 & Tile::FLIP_Y != 0 { 7 - y } else { y };
                                    let (cx, cy) = (i % TILESET_COLUMNS, i / TILESET_COLUMNS);
                                    tileset.get(cx * TILE + tx, cy * TILE + ty)
                                }
                            };
                            assert_eq!(got, want, "{} square {sx},{sy} px {x},{y}", l.name);
                            seen += usize::from(got[3] != 0);
                        }
                    }
                }
            }
        }
        assert!(seen > 0);
    }

    #[test]
    fn the_far_layer_is_the_last_columns_top_bands() {
        let (grid, tiles, palettes) = sample(17);
        let background = Background::new(&grid, &tiles, &palettes, "").unwrap();
        for (name, raised) in [("far", false), ("far-raised", true)] {
            let l = background.layers.iter().find(|l| l.name == name).unwrap();
            let LayerContent::Image {
                path,
                repeat_x: true,
                repeat_y: true,
                ..
            } = &l.content
            else {
                panic!("{name} isn't a repeating image");
            };
            let image = rgba(&background.images[path]);
            assert_eq!((image.width, image.height), (512, 256));
            for y in 0..256 {
                for x in 0..512 {
                    let entry = grid.entry(4 + x as usize / 8, y as usize / 8);
                    let want = if entry.raised() == raised {
                        shown(&tiles, &palettes, entry, x % 8, y % 8)
                    } else {
                        [0; 4]
                    };
                    assert_eq!(image.get(x, y), want, "{name} {x},{y}");
                }
            }
        }
        assert!(Background::new(&sample(15).0, &tiles, &palettes, "").is_err());
    }

    #[test]
    fn layers_without_raised_tiles_are_left_out() {
        let pictures = Pictures::new(&hex("pictures.hex")).unwrap();
        // Type 0 everywhere: no raised entries.
        let grid: Vec<u8> = (0..16 * 16).flat_map(|_| 0u16.to_le_bytes()).collect();
        let grid = GridPicture::new(&grid, &pictures).unwrap();
        let tiles = TileSet::new(&hex("tiles.hex")[..4 * 32]).unwrap();
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        let background = Background::new(&grid, &tiles, &palettes, "").unwrap();
        let names: Vec<&str> = background.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["far", "ground"]);
        let LayerContent::Tiles(tl) = &background.layers[1].content else {
            unreachable!()
        };
        // Type 0 shows only tile 0, palette row 0, which is not blank in
        // the vector tiles, so it's in the tileset once.
        assert!(tl.tiles.iter().all(|&t| t == Tile(1)));
    }
}
