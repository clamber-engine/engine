//! A level's background pictures (`spec/formats/graphics.md`): colours
//! and palette rows, tiles, packed tile data, tile entries, each ground
//! type's cell picture, and the grid picture made of them, turned into
//! Clamber level layers drawn from a tileset image; screen pictures,
//! drawn as one image; and the first level's scene, which has both.
//!
//! The grid is the ground grid of `ground.rs`, 16 bands tall, column by
//! column: a cell's picture comes from the same cell word as its ground.

use std::collections::BTreeMap;

use clamber_fixed::Fixed;
use clamber_level::{Band, Depth, Dim, Layer, LayerContent, Rgb, Tile, TileLayer, Tileset, Vec2};
use clamber_sprite::{Rgba, RgbaImage, encode_png};

use crate::COLUMN_BANDS;

/// px in a tile, either way.
pub const TILE: u32 = 8;

/// Tile entries in a cell picture, either way.
const CELL_SQUARES: usize = 4;

/// Tile entries down a grid picture's column.
const COLUMN_SQUARES: usize = CELL_SQUARES * COLUMN_BANDS as usize;

/// Grid columns, and bands, that the far layer is made of: the grid's
/// last 16 columns ("The layers"), or the scene's first 8 ("The first
/// level's scene").
const FAR_COLUMNS: usize = 16;
const SCENE_FAR_COLUMNS: usize = 8;
const FAR_BANDS: usize = 8;

/// Rows between the view's top and the scene's far and front pictures'
/// top rows ("Where the layers are").
const SCENE_FAR_ROW: i32 = 5;
const SCENE_FRONT_ROW: i32 = 1;

/// Tile entries in a screen map, either way.
const SCREEN_SQUARES: usize = 32;

/// Tile entries in the horizon map, across and down ("The horizon
/// layer").
const HORIZON_COLUMNS: usize = 32;
const HORIZON_ROWS: usize = 6;

/// The picture row the horizon picture's top row is at, and its bands'
/// rows from the top: each band `k` moves at (`k` + 2)/16 of the view's
/// speed.
const HORIZON_TOP: i32 = 64;
const HORIZON_BANDS: [u32; 4] = [8, 8, 8, 24];

/// Picture rows the backdrop's image covers, which is more than a
/// picture has.
const BACKDROP_ROWS: usize = 256;

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

/// The colour the first level's far layer shows for a pixel of
/// `colour` ("The far layer's colours"): each channel 4 less, at least
/// 0, then halved.
pub fn far_colour(colour: u16) -> u16 {
    let [r, g, b] = channels(colour).map(|c| u16::from(c.saturating_sub(4) / 2));
    r | g << 5 | b << 10
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

    /// The set with every colour as the far layer shows it
    /// ([`far_colour`]).
    pub fn far(&self) -> PaletteSet {
        PaletteSet {
            rows: self.rows.map(|row| row.map(far_colour)),
        }
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

    /// What a pixel of a two-plane tile shows, of colour `number` (0 to
    /// 3) in an entry with palette row `row`: colour 4 `row` + `number`
    /// of the set's 128 ("Colours of two-plane tiles").
    pub fn two_plane_rgba(&self, row: u8, number: u8) -> Rgba {
        if number & 3 == 0 {
            return [0; 4];
        }
        let n = 4 * (row & 7) + (number & 3);
        self.rgba(n / 16, n % 16)
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

/// A screen map: the 32 × 32 tile entries of a 256 × 256 px screen
/// picture, square (0, 0) at its top left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenMap {
    /// Row of squares by row of squares.
    entries: Vec<TileEntry>,
}

impl ScreenMap {
    /// Bytes in a screen map.
    pub const BYTES: usize = 2 * SCREEN_SQUARES * SCREEN_SQUARES;

    pub fn new(bytes: &[u8]) -> Result<ScreenMap, String> {
        if bytes.len() != ScreenMap::BYTES {
            return Err(format!(
                "a screen map is {} bytes, not {}",
                ScreenMap::BYTES,
                bytes.len()
            ));
        }
        let entries = words(bytes).into_iter().map(TileEntry).collect();
        Ok(ScreenMap { entries })
    }

    /// The entry for the square at square column `sx`, row `sy`.
    pub fn entry(&self, sx: usize, sy: usize) -> TileEntry {
        assert!(
            sx < SCREEN_SQUARES && sy < SCREEN_SQUARES,
            "square out of bounds"
        );
        self.entries[sy * SCREEN_SQUARES + sx]
    }
}

/// A row-colour list ("Row-colour lists"): a colour for each row of
/// the picture from row 0 down, the last one's for every row below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowColours {
    /// Each entry's row count and colour.
    entries: Vec<(u8, u16)>,
}

impl RowColours {
    /// Reads a list from the start of `bytes` up to its end byte, the
    /// colour numbers of whose entries must be `number`.
    pub fn read(bytes: &[u8], number: u8) -> Result<RowColours, String> {
        let mut entries = Vec::new();
        let mut rest = bytes;
        loop {
            match *rest {
                [0, ..] if entries.is_empty() => {
                    return Err("a row-colour list has no entries".to_string());
                }
                [0, ..] => return Ok(RowColours { entries }),
                [n, ..] if n & 0x80 != 0 => {
                    return Err(format!(
                        "a row-colour list's row count {n:#04x} has bit 7 set"
                    ));
                }
                [n, a, b, lo, hi, ..] => {
                    if (a, b) != (number, number) {
                        return Err(format!(
                            "a row-colour list for colour {number} gives colour {a}, {b}"
                        ));
                    }
                    entries.push((n, u16::from_le_bytes([lo, hi])));
                    rest = &rest[5..];
                }
                _ => return Err("a row-colour list runs past its data".to_string()),
            }
        }
    }

    /// Rows the entries cover.
    pub fn rows(&self) -> usize {
        self.entries.iter().map(|&(n, _)| usize::from(n)).sum()
    }

    /// The colour of picture row `row`.
    pub fn colour(&self, row: usize) -> u16 {
        self.entries[self.entry(row)].1
    }

    /// The index of the entry that covers picture row `row`.
    pub fn entry(&self, row: usize) -> usize {
        let mut top = 0;
        for (i, &(n, _)) in self.entries.iter().enumerate() {
            top += usize::from(n);
            if row < top {
                return i;
            }
        }
        self.entries.len().saturating_sub(1)
    }
}

/// The first level's horizon block ("The horizon layer"): its map of 6
/// rows of 32 tile entries, then two-plane tiles up to the last one the
/// map shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HorizonBlock {
    /// Row of squares by row of squares.
    map: Vec<TileEntry>,
    tiles: TileSet,
}

impl HorizonBlock {
    /// Bytes in the horizon map.
    pub const MAP_BYTES: usize = 2 * HORIZON_COLUMNS * HORIZON_ROWS;

    /// Reads the block from the start of `bytes`.
    pub fn read(bytes: &[u8]) -> Result<HorizonBlock, String> {
        let short = || "the horizon block runs past its data".to_string();
        let map: Vec<TileEntry> = words(bytes.get(..HorizonBlock::MAP_BYTES).ok_or_else(short)?)
            .into_iter()
            .map(TileEntry)
            .collect();
        let count = map
            .iter()
            .map(|e| usize::from(e.tile()) + 1)
            .max()
            .unwrap_or(0);
        let tiles = &bytes[HorizonBlock::MAP_BYTES..];
        let tiles = tiles.get(..count * 16).ok_or_else(short)?;
        Ok(HorizonBlock {
            map,
            tiles: TileSet::two_plane(tiles)?,
        })
    }

    /// The horizon picture, 256 by 48 px: two-plane colours, but colour
    /// number 1 of palette row 0 from `colour_1` by the picture row the
    /// pixel is shown on.
    fn picture(&self, palettes: &PaletteSet, colour_1: &RowColours) -> RgbaImage {
        let side = TILE * HORIZON_COLUMNS as u32;
        let mut image = RgbaImage::new(side, TILE * HORIZON_ROWS as u32);
        for (i, &entry) in self.map.iter().enumerate() {
            let (sx, sy) = ((i % HORIZON_COLUMNS) as u32, (i / HORIZON_COLUMNS) as u32);
            for y in 0..TILE {
                for x in 0..TILE {
                    let (tx, ty) = flipped(entry, x, y);
                    let number = self.tiles.pixel(entry.tile(), tx, ty).unwrap_or(0);
                    let row = sy * TILE + y;
                    let p = match (entry.palette_row(), number) {
                        (0, 1) => {
                            let picture_row = (HORIZON_TOP as u32 + row) as usize;
                            let [r, g, b] = rgb(colour_1.colour(picture_row));
                            [r, g, b, 255]
                        }
                        (row, number) => palettes.two_plane_rgba(row, number),
                    };
                    image.set(sx * TILE + x, row, p);
                }
            }
        }
        image
    }
}

/// A backdrop list's night colour table ("Night colour tables"): a
/// colour for each of the list's first entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NightColours {
    colours: Vec<u16>,
}

impl NightColours {
    /// Reads the table of `entries` colours from the start of `bytes`.
    pub fn read(bytes: &[u8], entries: usize) -> Result<NightColours, String> {
        let bytes = bytes
            .get(..2 * entries)
            .ok_or_else(|| "a night colour table runs past its data".to_string())?;
        let colours = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        Ok(NightColours { colours })
    }

    /// The night colour of list entry `entry`: black after the table's
    /// last.
    pub fn colour(&self, entry: usize) -> u16 {
        self.colours.get(entry).copied().unwrap_or(0)
    }
}

/// What's behind the first level's far layer: the backdrop's row-colour
/// list and night colour table, and the horizon layer's list and block
/// ("The layers").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sky {
    pub backdrop: RowColours,
    pub night: NightColours,
    pub horizon: RowColours,
    pub block: HorizonBlock,
}

/// The pixel that `entry` shows at column `x`, row `y` of its square.
fn shown(tiles: &TileSet, palettes: &PaletteSet, entry: TileEntry, x: u32, y: u32) -> Rgba {
    let (x, y) = flipped(entry, x, y);
    let number = tiles.pixel(entry.tile(), x, y).unwrap_or(0);
    palettes.rgba(entry.palette_row(), number)
}

/// The pixel of `entry`'s tile shown at column `x`, row `y` of its
/// square.
fn flipped(entry: TileEntry, x: u32, y: u32) -> (u32, u32) {
    let x = if entry.flip_lr() { TILE - 1 - x } else { x };
    let y = if entry.flip_tb() { TILE - 1 - y } else { y };
    (x, y)
}

/// A screen map's squares, row by row.
fn screen_squares() -> impl Iterator<Item = (usize, usize)> {
    (0..SCREEN_SQUARES).flat_map(|sy| (0..SCREEN_SQUARES).map(move |sx| (sx, sy)))
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
    /// The first level's layers ("The layers"), their images named
    /// starting `prefix`: the backdrop and the horizon layer, if `sky`
    /// is given, then the far and ground layers.
    ///
    /// The ground layer is a tile layer that moves with the view, its
    /// tileset holding each tile in each palette row it's drawn with. The
    /// far layer is a picture repeating both ways, at half the view's
    /// speed across and a quarter up and down ("Where the far layer is"),
    /// in its palette colours and drawn darker ("The far layer's
    /// colours"), so it dims as `spec/behavior/dimming.md` says ("The far
    /// layer"). The backdrop and the horizon layer stay at
    /// the same rows of the picture; the backdrop's rows are a row lower
    /// after odd frames ("The backdrop"), and the horizon layer's bands
    /// move across at their own speeds.
    ///
    /// The spec doesn't say where characters are drawn against the
    /// layers, so all of them are behind entities.
    pub fn new(
        grid: &GridPicture,
        tiles: &TileSet,
        palettes: &PaletteSet,
        sky: Option<&Sky>,
        prefix: &str,
    ) -> Result<Background, String> {
        if grid.columns < FAR_COLUMNS {
            return Err(format!(
                "a grid of {} columns has no far layer, which needs {FAR_COLUMNS}",
                grid.columns
            ));
        }
        let far = (grid.columns - FAR_COLUMNS) * CELL_SQUARES..grid.width();
        let place = (
            Vec2::new(Fixed::from_ratio_pow2(1, 1), Fixed::from_ratio_pow2(1, 2)),
            // Picture row j shows far picture row ⌊T / 4⌋ + 1 + j.
            Vec2::ints(0, -1),
        );
        let mut background = Background::grid(grid, tiles, palettes, far, place, prefix)?;
        for layer in &mut background.layers {
            layer.darker = layer.name.starts_with("far");
        }
        if let Some(sky) = sky {
            let mut layers = Background::sky(sky, palettes, prefix, &mut background.images);
            layers.append(&mut background.layers);
            background.layers = layers;
        }
        Ok(background)
    }

    /// The backdrop and the horizon layer, adding their images to
    /// `images`.
    fn sky(
        sky: &Sky,
        palettes: &PaletteSet,
        prefix: &str,
        images: &mut BTreeMap<String, Vec<u8>>,
    ) -> Vec<Layer> {
        // Image row 0 is the first entry's colour again, for the row the
        // list covers more after odd frames.
        let width = TILE;
        let mut image = RgbaImage::new(width, BACKDROP_ROWS as u32 + 1);
        for row in 0..=BACKDROP_ROWS {
            let [r, g, b] = rgb(sky.backdrop.colour(row.saturating_sub(1)));
            for x in 0..width {
                image.set(x, row as u32, [r, g, b, 255]);
            }
        }
        let backdrop = format!("{prefix}backdrop.png");
        images.insert(backdrop.clone(), encode_png(&image));
        // Each entry goes to its night colour ("The backdrop and the
        // horizon layer"); the night image has the backdrop's rows.
        let mut night = RgbaImage::new(width, BACKDROP_ROWS as u32 + 1);
        for row in 0..=BACKDROP_ROWS {
            let entry = sky.backdrop.entry(row.saturating_sub(1));
            let [r, g, b] = rgb(sky.night.colour(entry));
            for x in 0..width {
                night.set(x, row as u32, [r, g, b, 255]);
            }
        }
        let backdrop_night = format!("{prefix}backdrop-night.png");
        images.insert(backdrop_night.clone(), encode_png(&night));
        let mut backdrop = layer(
            "backdrop",
            Vec2::default(),
            Vec2::ints(0, -1),
            LayerContent::Image {
                path: backdrop,
                normals: None,
                repeat_x: true,
                repeat_y: false,
            },
        );
        backdrop.alternate = Vec2::ints(0, 1);
        backdrop.dim = Dim::Night(backdrop_night);

        let horizon = format!("{prefix}horizon.png");
        let image = sky.block.picture(palettes, &sky.horizon);
        images.insert(horizon.clone(), encode_png(&image));
        let mut horizon = layer(
            "horizon",
            Vec2::default(),
            Vec2::ints(0, HORIZON_TOP),
            LayerContent::Image {
                path: horizon,
                normals: None,
                repeat_x: true,
                repeat_y: false,
            },
        );
        // Every colour of the horizon goes to black ("The backdrop and the
        // horizon layer").
        let night = format!("{prefix}horizon-night.png");
        let mut black = RgbaImage::new(1, 1);
        black.set(0, 0, [0, 0, 0, 255]);
        images.insert(night.clone(), encode_png(&black));
        horizon.dim = Dim::Night(night);
        horizon.bands = (0..)
            .zip(HORIZON_BANDS)
            .map(|(k, rows)| Band {
                rows,
                parallax: Fixed::from_ratio_pow2(k + 2, 4),
            })
            .collect();
        vec![backdrop, horizon]
    }

    /// The first level's scene's layers ("The first level's scene"): the
    /// far and ground layers as from [`Background::new`], the far picture
    /// made of the grid's first 8 columns, then the raised tiles of the
    /// front map, drawn with the two-plane `front_tiles`. `view` is the
    /// view's top left in the scene, which never moves: the far picture's
    /// row 5 and the front picture's row 1 are at its top.
    ///
    /// The front layer stays fixed on screen. Its tiles that aren't
    /// raised aren't drawn: the drawing order has none.
    pub fn scene(
        grid: &GridPicture,
        tiles: &TileSet,
        palettes: &PaletteSet,
        front: (&ScreenMap, &TileSet),
        view: Vec2,
        prefix: &str,
    ) -> Result<Background, String> {
        if grid.columns < SCENE_FAR_COLUMNS {
            return Err(format!(
                "a grid of {} columns has no far layer, which needs {SCENE_FAR_COLUMNS}",
                grid.columns
            ));
        }
        let (map, front_tiles) = front;
        if let Some(entry) = screen_squares()
            .map(|(sx, sy)| map.entry(sx, sy))
            .find(|e| e.raised() && usize::from(e.tile()) >= front_tiles.len())
        {
            return Err(format!(
                "the front map shows tile {}, past the {} front tiles",
                entry.tile(),
                front_tiles.len()
            ));
        }
        // The far layer scrolls at half speed both ways, in its palette
        // colours.
        let half = Fixed::from_ratio_pow2(1, 1);
        let far = Vec2::new(
            view.x * half,
            view.y * half - Fixed::from_int(SCENE_FAR_ROW),
        );
        let squares = 0..SCENE_FAR_COLUMNS * CELL_SQUARES;
        let place = (Vec2::new(half, half), far);
        let mut background = Background::grid(grid, tiles, palettes, squares, place, prefix)?;

        let side = SCREEN_SQUARES as u32 * TILE;
        let mut image = RgbaImage::new(side, side);
        for (sx, sy) in screen_squares() {
            let entry = map.entry(sx, sy);
            if !entry.raised() {
                continue;
            }
            for y in 0..TILE {
                for x in 0..TILE {
                    let (tx, ty) = flipped(entry, x, y);
                    let number = front_tiles.pixel(entry.tile(), tx, ty).unwrap_or(0);
                    let p = palettes.two_plane_rgba(entry.palette_row(), number);
                    image.set(sx as u32 * TILE + x, sy as u32 * TILE + y, p);
                }
            }
        }
        let path = format!("{prefix}front.png");
        background.images.insert(path.clone(), encode_png(&image));
        background.layers.push(layer(
            "front",
            Vec2::default(),
            Vec2::new(Fixed::ZERO, -Fixed::from_int(SCENE_FRONT_ROW)),
            LayerContent::Image {
                path,
                normals: None,
                repeat_x: false,
                repeat_y: false,
            },
        ));
        Ok(background)
    }

    /// The far layer, repeating the grid picture's square columns
    /// `far` from its top in `far_palettes`, with the parallax and the
    /// offset `place`; then the ground layer.
    fn grid(
        grid: &GridPicture,
        tiles: &TileSet,
        palettes: &PaletteSet,
        far: std::ops::Range<usize>,
        place: (Vec2, Vec2),
        prefix: &str,
    ) -> Result<Background, String> {
        let mut images = BTreeMap::new();
        let mut back = Vec::new();
        let mut front = Vec::new();

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
            (if raised { &mut front } else { &mut back }).push(layer(
                name,
                place.0,
                place.1,
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
            let mut tiled = layer(
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
            );
            if raised == 1 {
                // The characters are in front of it in play and behind
                // it in the walk-outs (`spec/formats/graphics.md`).
                tiled.depth = Depth::Raised;
                front.push(tiled);
            } else {
                back.push(tiled);
            }
        }
        back.append(&mut front);
        Ok(Background {
            layers: back,
            images,
        })
    }

    /// The layers of a screen picture, their images named starting
    /// `prefix` ("Screen pictures"): the picture fixed to level
    /// coordinates, with the backdrop (colour 0 of palette row 0) where
    /// it's transparent, then its raised tiles if it has any.
    pub fn screen(
        map: &ScreenMap,
        tiles: &TileSet,
        palettes: &PaletteSet,
        prefix: &str,
    ) -> Result<Background, String> {
        let squares = screen_squares;
        if let Some(entry) = squares()
            .map(|(sx, sy)| map.entry(sx, sy))
            .find(|e| usize::from(e.tile()) >= tiles.len())
        {
            return Err(format!(
                "the screen map shows tile {}, past the {} tiles",
                entry.tile(),
                tiles.len()
            ));
        }
        let [r, g, b] = rgb(palettes.colour(0, 0));
        let backdrop = [r, g, b, 255];
        let side = SCREEN_SQUARES as u32 * TILE;
        let mut images = BTreeMap::new();
        let mut layers = Vec::new();
        for raised in [false, true] {
            let any = squares().any(|(sx, sy)| map.entry(sx, sy).raised());
            if raised && !any {
                continue;
            }
            let mut image = RgbaImage::new(side, side);
            for (sx, sy) in squares() {
                let entry = map.entry(sx, sy);
                for y in 0..TILE {
                    for x in 0..TILE {
                        let mut p = match entry.raised() == raised {
                            true => shown(tiles, palettes, entry, x, y),
                            false => [0; 4],
                        };
                        if !raised && p[3] == 0 {
                            p = backdrop;
                        }
                        image.set(sx as u32 * TILE + x, sy as u32 * TILE + y, p);
                    }
                }
            }
            let name = if raised { "screen-raised" } else { "screen" };
            let path = format!("{prefix}{name}.png");
            images.insert(path.clone(), encode_png(&image));
            layers.push(layer(
                name,
                Vec2::ints(1, 1),
                Vec2::default(),
                LayerContent::Image {
                    path,
                    normals: None,
                    repeat_x: false,
                    repeat_y: false,
                },
            ));
        }
        Ok(Background { layers, images })
    }
}

/// Columns and rows of tile entries in a map page picture.
pub const PAGE_COLUMNS: usize = 32;
pub const PAGE_ROWS: usize = 28;

/// Bytes in a map page picture's tile map and tile set
/// (`spec/formats/level-map.md`, "The pages' pictures").
pub const PAGE_MAP_BYTES: usize = 2 * PAGE_COLUMNS * PAGE_ROWS;
pub const PAGE_SET_BYTES: usize = 896 * 32;

/// A map page picture, 256 × 224 px: its tile map's 32 × 28 entries
/// drawn with its tile set and palette set, the backdrop (colour 0 of
/// row 0) where a tile is transparent.
pub fn page_picture(
    map: &[u8],
    tiles: &TileSet,
    palettes: &PaletteSet,
) -> Result<RgbaImage, String> {
    if map.len() != PAGE_MAP_BYTES {
        return Err(format!(
            "a page's tile map is {PAGE_MAP_BYTES} bytes, not {}",
            map.len()
        ));
    }
    let entries: Vec<TileEntry> = words(map).into_iter().map(TileEntry).collect();
    if let Some(e) = entries
        .iter()
        .find(|e| usize::from(e.tile()) >= tiles.len())
    {
        return Err(format!(
            "a page's tile map shows tile {}, past the {} tiles",
            e.tile(),
            tiles.len()
        ));
    }
    let [r, g, b] = rgb(palettes.colour(0, 0));
    let mut image = RgbaImage::new(PAGE_COLUMNS as u32 * TILE, PAGE_ROWS as u32 * TILE);
    for (i, &entry) in entries.iter().enumerate() {
        let (sx, sy) = ((i % PAGE_COLUMNS) as u32, (i / PAGE_COLUMNS) as u32);
        for y in 0..TILE {
            for x in 0..TILE {
                let mut p = shown(tiles, palettes, entry, x, y);
                if p[3] == 0 {
                    p = [r, g, b, 255];
                }
                image.set(sx * TILE + x, sy * TILE + y, p);
            }
        }
    }
    Ok(image)
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
        dim: Dim::Palette,
        darker: false,
        opacity: 1.0,
        sway: None,
        bands: Vec::new(),
        alternate: Vec2::default(),
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
        let background = Background::new(&grid, &tiles, &palettes, None, "bg/").unwrap();
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
        let background = Background::new(&grid, &tiles, &palettes, None, "").unwrap();
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
            // Half the view's speed across and a quarter up and down,
            // row ⌊T / 4⌋ + 1 at the top ("Where the far layer is").
            let (half, quarter) = (Fixed::from_ratio_pow2(1, 1), Fixed::from_ratio_pow2(1, 2));
            assert_eq!(
                (l.parallax, l.offset),
                (Vec2::new(half, quarter), Vec2::ints(0, -1))
            );
            let image = rgba(&background.images[path]);
            assert_eq!((image.width, image.height), (512, 256));
            // In the palette colours, drawn darker ("The far layer's
            // colours").
            assert!(l.darker && l.dim == Dim::Palette, "{name}");
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
        assert!(Background::new(&sample(15).0, &tiles, &palettes, None, "").is_err());
    }

    fn colour(hex: &str) -> u16 {
        u16::from_str_radix(hex, 16).unwrap()
    }

    fn opaque(colour: u16) -> Rgba {
        let [r, g, b] = rgb(colour);
        [r, g, b, 255]
    }

    #[test]
    fn far_colours_match_the_spec_vectors() {
        for row in rows("far-colours.tsv") {
            let (from, shown) = (colour(&row[0]), colour(&row[4]));
            assert_eq!(far_colour(from), shown, "{}", row[0]);
            assert_eq!(
                channels(shown).map(u32::from),
                [5, 6, 7].map(|i| num::<u32>(&row[i]))
            );
        }
        assert_eq!(channels(far_colour(31 | 10 << 5 | 3 << 10)), [13, 3, 0]);
    }

    /// The vectors' backdrop list, then their horizon list.
    fn row_lists() -> (RowColours, RowColours) {
        let bytes = hex("rows.hex");
        let backdrop = RowColours::read(&bytes, 0).unwrap();
        // Each list is followed by two bytes that aren't part of it.
        let next = 5 * backdrop.entries.len() + 1 + 2;
        let horizon = RowColours::read(&bytes[next..], 1).unwrap();
        (backdrop, horizon)
    }

    #[test]
    fn night_colour_table_matches_the_spec_vector() {
        // "Night colour tables": m = 3, later entries black.
        let table = NightColours::read(&[0, 0x10, 0, 0x0C, 0, 0, 0xFF], 3).unwrap();
        let got: Vec<u16> = (0..5).map(|i| table.colour(i)).collect();
        assert_eq!(got, [0x1000, 0x0C00, 0, 0, 0]);
        assert!(NightColours::read(&[0, 0x10, 0], 3).is_err());
    }

    #[test]
    fn row_colour_lists_match_the_spec_vectors() {
        let (backdrop, horizon) = row_lists();
        assert_eq!((backdrop.rows(), horizon.rows()), (134, 98));
        let sky = Sky {
            backdrop,
            night: NightColours::read(&[], 0).unwrap(),
            horizon,
            block: HorizonBlock::read(&hex("horizon.hex")).unwrap(),
        };
        let mut images = BTreeMap::new();
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        let layers = Background::sky(&sky, &palettes, "", &mut images);
        let image = rgba(&images["backdrop.png"]);
        let rows = rows("rows.tsv");
        assert_eq!(rows.len(), 224);
        for row in rows {
            let j: u32 = num(&row[0]);
            let (even, odd) = (colour(&row[1]), colour(&row[2]));
            // After even frames the image is drawn a row higher.
            assert_eq!(image.get(3, j + 1), opaque(even), "row {j}");
            assert_eq!(image.get(3, j), opaque(odd), "row {j}");
            assert_eq!(sky.horizon.colour(j as usize), colour(&row[3]), "row {j}");
        }
        let backdrop = &layers[0];
        assert_eq!(backdrop.name, "backdrop");
        assert_eq!(
            (backdrop.parallax, backdrop.offset, backdrop.alternate),
            (Vec2::default(), Vec2::ints(0, -1), Vec2::ints(0, 1))
        );

        let list = |bytes: &[u8]| RowColours::read(bytes, 0);
        assert!(list(&[0]).is_err());
        assert!(list(&[2, 0, 0, 0x1F]).is_err());
        assert!(list(&[0x82, 0, 0, 0x1F, 0, 0]).is_err());
        assert!(list(&[2, 1, 1, 0x1F, 0, 0]).is_err());
        let example = [2, 0, 0, 0x1F, 0, 1, 0, 0, 0xE0, 3, 3, 0, 0, 0, 0x7C, 0, 9];
        let example = list(&example).unwrap();
        let colours: Vec<u16> = (0..8).map(|row| example.colour(row)).collect();
        assert_eq!(
            colours,
            [0x1F, 0x1F, 0x3E0, 0x7C00, 0x7C00, 0x7C00, 0x7C00, 0x7C00]
        );
    }

    #[test]
    fn the_horizon_layer_matches_the_spec_vectors() {
        let (backdrop, horizon) = row_lists();
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        let block = HorizonBlock::read(&hex("horizon.hex")).unwrap();
        assert_eq!(block.tiles.len(), 6);
        let picture = block.picture(&palettes, &horizon);
        assert_eq!((picture.width, picture.height), (256, 48));
        for row in rows("horizon-pixels.tsv") {
            let j: usize = num(&row[4]);
            let want = colour(&row[8]);
            if row[5] == "-" || row[7] == "0" {
                let backdrop = match row[2].as_str() {
                    "even" => backdrop.colour(j),
                    _ => backdrop.colour(j.saturating_sub(1)),
                };
                assert_eq!(backdrop, want, "{row:?}");
            }
            if row[5] == "-" {
                continue;
            }
            let (x, y): (u32, u32) = (num(&row[5]), num(&row[6]));
            let got = picture.get(x, y);
            match row[7].as_str() {
                "0" => assert_eq!(got, [0; 4], "{row:?}"),
                _ => assert_eq!(got, opaque(want), "{row:?}"),
            }
        }

        let sky = Sky {
            backdrop,
            night: NightColours::read(&[], 0).unwrap(),
            horizon,
            block,
        };
        let mut images = BTreeMap::new();
        let layers = Background::sky(&sky, &palettes, "", &mut images);
        let horizon = &layers[1];
        assert_eq!(
            (horizon.name.as_str(), horizon.parallax, horizon.offset),
            ("horizon", Vec2::default(), Vec2::ints(0, 64))
        );
        let bands: Vec<_> = horizon.bands.iter().map(|b| (b.rows, b.parallax)).collect();
        let sixteenths = |n| Fixed::from_ratio_pow2(n, 4);
        assert_eq!(
            bands,
            [
                (8, sixteenths(2)),
                (8, sixteenths(3)),
                (8, sixteenths(4)),
                (24, sixteenths(5))
            ]
        );
        assert!(HorizonBlock::read(&hex("horizon.hex")[..400]).is_err());
    }

    #[test]
    fn screen_maps_match_the_spec_vectors() {
        let map = ScreenMap::new(&hex("screen.hex")).unwrap();
        let rows = rows("screen.tsv");
        assert_eq!(rows.len(), 1024);
        for row in rows {
            let (sx, sy): (usize, usize) = (num(&row[0]), num(&row[1]));
            assert_eq!(
                (num::<usize>(&row[2]), num::<usize>(&row[3])),
                (8 * sx, 8 * sy)
            );
            let want = TileEntry(u16::from_str_radix(&row[4], 16).unwrap());
            assert_eq!(map.entry(sx, sy), want, "square {sx},{sy}");
        }
        assert!(ScreenMap::new(&[0; 2047]).is_err());
    }

    /// The screen layers, the raised one over the other, show the vector
    /// pixels, and the backdrop where they're transparent.
    #[test]
    fn the_screen_layers_draw_the_screen_picture() {
        let map = ScreenMap::new(&hex("screen.hex")).unwrap();
        let tiles = TileSet::new(&hex("tiles.hex")[..4 * 32]).unwrap();
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        let background = Background::screen(&map, &tiles, &palettes, "room-").unwrap();
        let names: Vec<&str> = background.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["screen", "screen-raised"]);
        let images: Vec<RgbaImage> = background
            .layers
            .iter()
            .map(|l| {
                assert_eq!(
                    l.depth,
                    if l.name == "ground-raised" {
                        Depth::Raised
                    } else {
                        Depth::Back
                    }
                );
                assert_eq!((l.parallax, l.offset), (Vec2::ints(1, 1), Vec2::default()));
                let LayerContent::Image {
                    path,
                    repeat_x: false,
                    repeat_y: false,
                    ..
                } = &l.content
                else {
                    panic!("{} isn't one image", l.name);
                };
                assert_eq!(path, &format!("room-{}.png", l.name));
                let image = rgba(&background.images[path]);
                assert_eq!((image.width, image.height), (256, 256));
                image
            })
            .collect();
        let [r, g, b] = rgb(palettes.colour(0, 0));
        let rows = rows("screen-pixels.tsv");
        assert!(rows.len() >= 256);
        for row in rows {
            let (x, y): (u32, u32) = (num(&row[0]), num(&row[1]));
            let want = match row[5].as_str() {
                "transparent" => [r, g, b, 255],
                hex => {
                    let [r, g, b] = rgb(u16::from_str_radix(hex, 16).unwrap());
                    [r, g, b, 255]
                }
            };
            let (screen, raised) = (images[0].get(x, y), images[1].get(x, y));
            assert_eq!(screen[3], 255, "{x},{y}");
            let got = if raised[3] == 0 { screen } else { raised };
            assert_eq!(got, want, "{x},{y}");
        }
    }

    /// Raised tiles go on a layer of their own, in front; under them the
    /// screen layer shows the backdrop.
    #[test]
    fn a_screen_picture_s_raised_tiles_are_in_front() {
        let mut bytes = hex("screen.hex");
        // Square 1 (tile 0, palette row 1) raised, square 2 past the tiles.
        bytes[3] |= 0x20;
        let tiles = TileSet::new(&hex("tiles.hex")[..4 * 32]).unwrap();
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        let map = ScreenMap::new(&bytes).unwrap();
        let background = Background::screen(&map, &tiles, &palettes, "").unwrap();
        let names: Vec<&str> = background.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["screen", "screen-raised"]);
        let screen = rgba(&background.images["screen.png"]);
        let raised = rgba(&background.images["screen-raised.png"]);
        let [r, g, b] = rgb(palettes.colour(0, 0));
        let entry = map.entry(1, 0);
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(screen.get(8 + x, y), [r, g, b, 255]);
                assert_eq!(raised.get(8 + x, y), shown(&tiles, &palettes, entry, x, y));
            }
        }
        assert_eq!(raised.get(16, 0), [0; 4]);

        bytes[4..6].copy_from_slice(&4u16.to_le_bytes());
        let map = ScreenMap::new(&bytes).unwrap();
        let err = Background::screen(&map, &tiles, &palettes, "").unwrap_err();
        assert!(err.contains("tile 4"), "{err}");
    }

    /// The scene's front layer, every vector entry raised, shows the
    /// vector pixels: two-plane colours counted through the whole set.
    #[test]
    fn the_front_layer_matches_the_spec_vectors() {
        let mut bytes = hex("front.hex");
        for b in bytes.iter_mut().skip(1).step_by(2) {
            *b |= 0x20;
        }
        let map = ScreenMap::new(&bytes).unwrap();
        let front_tiles = TileSet::two_plane(&hex("front-tiles.hex")).unwrap();
        let (grid, tiles, palettes) = sample(8);
        let background = Background::scene(
            &grid,
            &tiles,
            &palettes,
            (&map, &front_tiles),
            Vec2::default(),
            "scene-",
        )
        .unwrap();
        let front = rgba(&background.images["scene-front.png"]);
        assert_eq!((front.width, front.height), (256, 256));
        let rows = rows("front-pixels.tsv");
        assert!(rows.len() >= 176);
        for row in rows {
            let (x, y): (u32, u32) = (num(&row[0]), num(&row[1]));
            let want = match row[7].as_str() {
                "transparent" => [0; 4],
                hex => {
                    let [r, g, b] = rgb(u16::from_str_radix(hex, 16).unwrap());
                    [r, g, b, 255]
                }
            };
            assert_eq!(front.get(x, y), want, "{x},{y}");
        }
        assert_eq!(palettes.two_plane_rgba(5, 3), palettes.rgba(1, 7));
        assert_eq!(palettes.two_plane_rgba(7, 0), [0; 4]);
    }

    /// "The first level's scene": the far picture is the grid's first 8
    /// columns, at row 5 of the view's top; the front picture's raised
    /// tiles, fixed on screen at its row 1, are in front of everything.
    #[test]
    fn the_scene_s_layers() {
        let (grid, tiles, palettes) = sample(17);
        let mut bytes = hex("front.hex");
        // Only square 1 raised; the others aren't drawn.
        for b in bytes.iter_mut().skip(1).step_by(2) {
            *b &= !0x20;
        }
        bytes[3] |= 0x20;
        let map = ScreenMap::new(&bytes).unwrap();
        let front_tiles = TileSet::two_plane(&hex("front-tiles.hex")).unwrap();
        let view = Vec2::ints(33_000, 40);
        let front = (&map, &front_tiles);
        let background = Background::scene(&grid, &tiles, &palettes, front, view, "").unwrap();
        let names: Vec<&str> = background.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            ["far", "ground", "far-raised", "ground-raised", "front"]
        );
        let half = Fixed::from_ratio_pow2(1, 1);
        for l in &background.layers {
            assert_eq!(
                l.depth,
                if l.name == "ground-raised" {
                    Depth::Raised
                } else {
                    Depth::Back
                }
            );
            let want = match l.name.as_str() {
                "far" | "far-raised" => (Vec2::new(half, half), Vec2::ints(16_500, 15)),
                "front" => (Vec2::default(), Vec2::ints(0, -1)),
                _ => (Vec2::ints(1, 1), Vec2::ints(0, -1)),
            };
            assert_eq!((l.parallax, l.offset), want, "{}", l.name);
            // On screen: the layer's top left less the view's, scaled.
            if l.name.starts_with("far") {
                let at = l.offset.y - view.y * l.parallax.y;
                assert_eq!(at, Fixed::from_int(-5));
                assert_eq!((l.offset.x - view.x * l.parallax.x).floor() % 256, 0);
            }
        }
        for (name, raised) in [("far", false), ("far-raised", true)] {
            let image = rgba(&background.images[&format!("{name}.png")]);
            assert_eq!((image.width, image.height), (256, 256));
            for y in 0..256 {
                for x in 0..256 {
                    let entry = grid.entry(x as usize / 8, y as usize / 8);
                    let want = if entry.raised() == raised {
                        shown(&tiles, &palettes, entry, x % 8, y % 8)
                    } else {
                        [0; 4]
                    };
                    assert_eq!(image.get(x, y), want, "{name} {x},{y}");
                }
            }
        }
        let image = rgba(&background.images["front.png"]);
        // Square 1: tile 0, palette row 1, colour x mod 4.
        for x in 0..8 {
            let want = palettes.two_plane_rgba(1, (x % 4) as u8);
            assert_eq!(image.get(8 + x, 3), want);
        }
        assert!(
            (0..256).all(|y| (0..256).all(|x| (8..16).contains(&x) || image.get(x, y)[3] == 0))
        );

        // A raised square past the front tiles.
        bytes[5] |= 0x20;
        bytes[4] = 4;
        let map = ScreenMap::new(&bytes).unwrap();
        let front = (&map, &front_tiles);
        let err = Background::scene(&grid, &tiles, &palettes, front, view, "").unwrap_err();
        assert!(err.contains("tile 4, past the 4 front tiles"), "{err}");
        assert!(Background::scene(&sample(7).0, &tiles, &palettes, front, view, "").is_err());
    }

    #[test]
    fn layers_without_raised_tiles_are_left_out() {
        let pictures = Pictures::new(&hex("pictures.hex")).unwrap();
        // Type 0 everywhere: no raised entries.
        let grid: Vec<u8> = (0..16 * 16).flat_map(|_| 0u16.to_le_bytes()).collect();
        let grid = GridPicture::new(&grid, &pictures).unwrap();
        let tiles = TileSet::new(&hex("tiles.hex")[..4 * 32]).unwrap();
        let palettes = PaletteSet::new(&hex("palette.hex")).unwrap();
        let background = Background::new(&grid, &tiles, &palettes, None, "").unwrap();
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
