//! Sprite frames (`spec/formats/sprites.md`): the frame table, short and
//! full frames, their pieces and sheet tiles, the pictures they make,
//! mirrored or not, their picture hashes, and sprite palettes, turned
//! into a Clamber sprite sheet.
//!
//! A frame's origin is the sheet frame's origin, so the engine's
//! mirroring about the origin is the format's. Which frames each object
//! shows, which palette it uses, and where its origin goes against its
//! position belong to the object, not the sheet.

use std::collections::{BTreeMap, HashMap, HashSet};

use clamber_sprite::{IndexImage, Palette, Rect, encode_indexed_png, pack};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{Location, Sha256Hash, TILE, TileSet, rgb};

/// Bytes in a frame table entry.
pub const TABLE_ENTRY_SIZE: usize = 4;

/// The last short frame; frames after it are full.
pub const LAST_SHORT: u16 = 34;

/// Sheet tiles across a frame's sheet.
const SHEET_COLUMNS: u16 = 16;

/// Bytes in a four-plane tile.
const TILE_BYTES: usize = 32;

/// The fewest opaque pixels a full frame's picture has to be named by its
/// picture hash.
pub const HASHED_OPAQUE: usize = 32;

/// Where a sheet's frames put their origin in the untrimmed frame: past
/// the farthest a piece can be left of or above it.
const SHEET_ORIGIN: i32 = 128;

/// The untrimmed frame, either way: up to the farthest a large piece can
/// reach right of or below the origin.
const SHEET_FRAME: i32 = SHEET_ORIGIN + 127 + 16;

/// How a frame number says a frame is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Frame 0: no picture.
    None,
    /// A 5-byte header, drawn with the object's own tiles.
    Short,
    /// An 8-byte header, with its tiles after the positions.
    Full,
}

impl Form {
    pub fn of(frame: u16) -> Form {
        match frame {
            0 => Form::None,
            1..=LAST_SHORT => Form::Short,
            _ => Form::Full,
        }
    }
}

/// One entry of the frame table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableEntry {
    pub location: Location,
    /// What it does isn't in the spec yet.
    pub order: u8,
}

/// Where each frame is stored, by frame number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameTable {
    entries: Vec<TableEntry>,
}

impl FrameTable {
    /// Reads 4-byte entries for frames 0 up, as many as `bytes` holds.
    pub fn new(bytes: &[u8]) -> Result<FrameTable, String> {
        let (entries, rest) = bytes.as_chunks::<TABLE_ENTRY_SIZE>();
        if !rest.is_empty() {
            return Err(format!(
                "a frame table of {} bytes isn't whole {TABLE_ENTRY_SIZE}-byte entries",
                bytes.len()
            ));
        }
        let entries = entries
            .iter()
            .map(|&[lo, hi, bank, order]| TableEntry {
                location: Location::new(bank, u16::from_le_bytes([lo, hi])),
                order,
            })
            .collect();
        Ok(FrameTable { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Where frame `frame` is stored; `None` for frame 0 or past the
    /// table.
    pub fn entry(&self, frame: u16) -> Option<TableEntry> {
        if Form::of(frame) == Form::None {
            return None;
        }
        self.entries.get(usize::from(frame)).copied()
    }
}

/// A frame's header. A short frame has no tile runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// Large pieces.
    pub l: u8,
    /// Small pieces in the first small group, and its first sheet tile.
    pub s1: u8,
    pub t1: u8,
    /// Small pieces in the second small group, and its first sheet tile.
    pub s2: u8,
    pub t2: u8,
    pub runs: Option<Runs>,
}

/// A full frame's tile runs: `n1` tiles at sheet tiles 0 up, then `n2`
/// at `d2` up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Runs {
    pub n1: u8,
    pub d2: u8,
    pub n2: u8,
}

impl Runs {
    /// Each tile of the runs in stored order: the sheet tile it's at, its
    /// run (1 or 2), and which tile of the run it is.
    pub fn slots(self) -> impl Iterator<Item = (u16, u8, u16)> {
        let first = (0..u16::from(self.n1)).map(|i| (i, 1, i));
        let second = (0..u16::from(self.n2)).map(move |i| (u16::from(self.d2) + i, 2, i));
        first.chain(second)
    }

    fn tiles(self) -> usize {
        usize::from(self.n1) + usize::from(self.n2)
    }
}

impl Header {
    /// Bytes in a header of form `form`.
    fn size(form: Form) -> usize {
        if form == Form::Full { 8 } else { 5 }
    }

    fn pieces(self) -> usize {
        usize::from(self.l) + usize::from(self.s1) + usize::from(self.s2)
    }
}

/// How big a piece is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// 16 × 16 px, four sheet tiles.
    Large,
    /// 8 × 8 px, one sheet tile.
    Small,
}

impl Size {
    pub fn px(self) -> i32 {
        match self {
            Size::Large => 16,
            Size::Small => 8,
        }
    }
}

/// One piece of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub size: Size,
    /// Its top-left pixel, from the frame's origin.
    pub x: i32,
    pub y: i32,
    /// Its (top-left) sheet tile.
    pub tile: u16,
}

impl Piece {
    /// Its sheet tiles: top left, top right, bottom left, bottom right
    /// for a large piece.
    pub fn sheet_tiles(self) -> Vec<u16> {
        match self.size {
            Size::Large => vec![self.tile, self.tile + 1, self.tile + 16, self.tile + 17],
            Size::Small => vec![self.tile],
        }
    }

    /// The colour number of its pixel (`x`, `y`), from its top left.
    fn pixel(self, sheet: &TileSet, x: i32, y: i32) -> Option<u8> {
        let (column, row) = ((x / TILE as i32) as u16, (y / TILE as i32) as u16);
        let tile = self.tile + column + SHEET_COLUMNS * row;
        sheet.pixel(tile, x as u32 % TILE, y as u32 % TILE)
    }
}

/// How a picture is mirrored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mirror {
    pub left_right: bool,
    pub top_bottom: bool,
}

/// One frame: its header, pieces and, for a full frame, its sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub form: Form,
    pub header: Header,
    pub pieces: Vec<Piece>,
    /// A full frame's sheet tiles, numbered as its sheet, blank where its
    /// runs put no tile.
    tiles: Option<TileSet>,
}

impl Frame {
    /// Reads frame `number` from `bytes`, which start where the frame
    /// table says it's stored. Bytes after the frame aren't read.
    pub fn read(number: u16, bytes: &[u8]) -> Result<Frame, String> {
        let form = Form::of(number);
        if form == Form::None {
            return Err("frame 0 has no picture".into());
        }
        let short = || format!("frame {number} ends early");
        let head = bytes.get(..Header::size(form)).ok_or_else(short)?;
        let header = Header {
            l: head[0],
            s1: head[1],
            t1: head[2],
            s2: head[3],
            t2: head[4],
            runs: (form == Form::Full).then(|| Runs {
                n1: head[5],
                d2: head[6],
                n2: head[7] & 0x0F,
            }),
        };

        let start = head.len();
        let end = start + 2 * header.pieces();
        let positions = bytes.get(start..end).ok_or_else(short)?;
        let mut pieces = Vec::with_capacity(header.pieces());
        for (i, &[bx, by]) in positions.as_chunks::<2>().0.iter().enumerate() {
            let (size, tile) = if i < usize::from(header.l) {
                let i = i as u16;
                (Size::Large, (2 * i) % 16 + 32 * (2 * i / 16))
            } else if i < usize::from(header.l + header.s1) {
                let j = i - usize::from(header.l);
                (Size::Small, u16::from(header.t1) + j as u16)
            } else {
                let j = i - usize::from(header.l + header.s1);
                (Size::Small, u16::from(header.t2) + j as u16)
            };
            pieces.push(Piece {
                size,
                x: i32::from(bx) - 128,
                y: i32::from(by) - 128,
                tile,
            });
        }

        let tiles = match header.runs {
            None => None,
            Some(runs) => {
                let data = bytes
                    .get(end..end + TILE_BYTES * runs.tiles())
                    .ok_or_else(short)?;
                let last = runs.slots().map(|(at, ..)| at + 1).max().unwrap_or(0);
                let mut sheet = vec![0; TILE_BYTES * usize::from(last)];
                for (k, (at, ..)) in runs.slots().enumerate() {
                    let at = TILE_BYTES * usize::from(at);
                    sheet[at..at + TILE_BYTES]
                        .copy_from_slice(&data[TILE_BYTES * k..TILE_BYTES * (k + 1)]);
                }
                Some(TileSet::new(&sheet)?)
            }
        };
        Ok(Frame {
            form,
            header,
            pieces,
            tiles,
        })
    }

    /// Bytes the frame takes where it's stored.
    pub fn len(&self) -> usize {
        Header::size(self.form)
            + 2 * self.header.pieces()
            + TILE_BYTES * self.header.runs.map_or(0, Runs::tiles)
    }

    /// Whether it has no pieces.
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// The frame's picture. A full frame is drawn with its own tiles; a
    /// short frame with `object_tiles`, the tiles in place for the object
    /// that shows it.
    pub fn picture(
        &self,
        object_tiles: Option<&TileSet>,
        mirror: Mirror,
    ) -> Result<Picture, String> {
        let sheet = self
            .tiles
            .as_ref()
            .or(object_tiles)
            .ok_or("a short frame is drawn with its object's tiles")?;
        // Sheet tiles past a full frame's runs are blank; a short frame's
        // must all be in place.
        if self.tiles.is_none()
            && let Some(tile) = self
                .pieces
                .iter()
                .flat_map(|p| p.sheet_tiles())
                .find(|&t| usize::from(t) >= sheet.len())
        {
            return Err(format!(
                "a piece uses sheet tile {tile}, past the {} tiles in place",
                sheet.len()
            ));
        }

        let Some((left, top, right, bottom)) = self.bounds() else {
            return Ok(Picture {
                x: 0,
                y: 0,
                pixels: IndexImage::new(0, 0),
            });
        };
        let (w, h) = ((right - left) as u32, (bottom - top) as u32);
        // Earlier pieces are in front, as in the spec's vectors. In
        // `title-1` overlapping pixels match, so it never shows.
        let mut pixels = IndexImage::new(w, h);
        for piece in self.pieces.iter().rev() {
            let px = piece.size.px();
            for y in 0..px {
                for x in 0..px {
                    let number = piece.pixel(sheet, x, y).unwrap_or(0);
                    if number == 0 {
                        continue;
                    }
                    let (mut ix, mut iy) = (piece.x + x - left, piece.y + y - top);
                    if mirror.left_right {
                        ix = w as i32 - 1 - ix;
                    }
                    if mirror.top_bottom {
                        iy = h as i32 - 1 - iy;
                    }
                    pixels.set(ix as u32, iy as u32, number);
                }
            }
        }
        // Pixel x is drawn at -1 - x mirrored, so the left edge moves to
        // -right.
        Ok(Picture {
            x: if mirror.left_right { -right } else { left },
            y: if mirror.top_bottom { -bottom } else { top },
            pixels,
        })
    }

    /// The frame's picture hash ("Picture hashes"): `None` for a short
    /// frame or one with fewer than [`HASHED_OPAQUE`] opaque pixels.
    pub fn picture_hash(&self) -> Option<Sha256Hash> {
        if self.form != Form::Full {
            return None;
        }
        let picture = self.picture(None, Mirror::default()).ok()?;
        if picture.opaque() < HASHED_OPAQUE {
            return None;
        }
        Some(Sha256Hash(Sha256::digest(picture.hash_input()?).into()))
    }

    /// The pieces' extent from the origin: left, top, right and bottom
    /// edges, right and bottom exclusive.
    fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        self.pieces.iter().fold(None, |b, p| {
            let (l, t, r, bm) = (p.x, p.y, p.x + p.size.px(), p.y + p.size.px());
            Some(match b {
                None => (l, t, r, bm),
                Some((bl, bt, br, bb)) => (bl.min(l), bt.min(t), br.max(r), bb.max(bm)),
            })
        })
    }
}

/// A frame's picture: colour numbers, 0 transparent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// Where its top-left pixel is from the frame's origin.
    pub x: i32,
    pub y: i32,
    pub pixels: IndexImage,
}

impl Picture {
    /// How many of its pixels are opaque.
    pub fn opaque(&self) -> usize {
        let (w, h) = (self.pixels.width, self.pixels.height);
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| self.pixels.get(x, y) != 0)
            .count()
    }

    /// The bytes its picture hash is worked out from ("Picture hashes"):
    /// the box of its opaque pixels from the origin, then a colour number
    /// per pixel of the box. `None` when no pixel is opaque.
    pub fn hash_input(&self) -> Option<Vec<u8>> {
        let (w, h) = (self.pixels.width, self.pixels.height);
        let opaque = |x, y| self.pixels.get(x, y) != 0;
        let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| opaque(x, y))).collect();
        let cols: Vec<u32> = (0..w).filter(|&x| (0..h).any(|y| opaque(x, y))).collect();
        let (&top, &bottom) = (rows.first()?, rows.last()?);
        let (&left, &right) = (cols.first()?, cols.last()?);
        let mut bytes = Vec::new();
        bytes.extend((self.x + left as i32).to_le_bytes()[..2].iter());
        bytes.extend((self.y + top as i32).to_le_bytes()[..2].iter());
        bytes.extend((right - left + 1).to_le_bytes()[..2].iter());
        bytes.extend((bottom - top + 1).to_le_bytes()[..2].iter());
        for y in top..=bottom {
            bytes.extend((left..=right).map(|x| self.pixels.get(x, y)));
        }
        Some(bytes)
    }
}

/// A sprite palette: colours 1 to 15 of a palette row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpritePalette {
    colours: [u16; 15],
}

impl SpritePalette {
    /// Bytes in a stored sprite palette.
    pub const SIZE: usize = 30;

    pub fn new(bytes: &[u8]) -> Result<SpritePalette, String> {
        let (words, rest) = bytes.as_chunks::<2>();
        if words.len() != 15 || !rest.is_empty() {
            return Err(format!(
                "a sprite palette is {} bytes, not {}",
                SpritePalette::SIZE,
                bytes.len()
            ));
        }
        let mut colours = [0; 15];
        for (c, &w) in colours.iter_mut().zip(words) {
            *c = u16::from_le_bytes(w);
        }
        Ok(SpritePalette { colours })
    }

    /// Colour `number`, 1 to 15, as stored; `None` for 0, which is
    /// transparent.
    pub fn colour(&self, number: u8) -> Option<u16> {
        let i = usize::from(number).checked_sub(1)?;
        self.colours.get(i).copied()
    }

    /// As a Clamber palette, by colour number, 0 transparent.
    pub fn palette(&self) -> Palette {
        let mut colours = vec![[0; 4]];
        colours.extend(self.colours.iter().map(|&c| {
            let [r, g, b] = rgb(c);
            [r, g, b, 255]
        }));
        Palette(colours)
    }
}

/// A Clamber sprite sheet's files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetFiles {
    /// The metadata.
    pub json: String,
    /// The indexed image.
    pub png: Vec<u8>,
}

/// Packs `pictures` into a sprite sheet whose image is `image`, coloured
/// with `palette`, with `others` as its named palettes. Sheet frame `i`
/// is `pictures[i]`, with the frame's origin as its origin, shown for one
/// logic frame, and is an animation of its own named with its name.
pub fn sprite_sheet(
    pictures: &[(String, Picture)],
    palette: &SpritePalette,
    others: &[(&str, &SpritePalette)],
    image: &str,
) -> Result<SheetFiles, String> {
    sprite_sheet_in(pictures, &palette.palette(), others, image)
}

/// [`sprite_sheet`] with the image's whole palette given, for pictures
/// that use colour numbers past 15.
pub fn sprite_sheet_in(
    pictures: &[(String, Picture)],
    palette: &Palette,
    others: &[(&str, &SpritePalette)],
    image: &str,
) -> Result<SheetFiles, String> {
    let sizes: Vec<(u32, u32)> = pictures
        .iter()
        .map(|(_, p)| (p.pixels.width, p.pixels.height))
        .collect();
    let largest = sizes.iter().map(|&(w, h)| w.max(h)).max().unwrap_or(0);
    let mut page = largest.max(256).next_power_of_two();
    let placements = loop {
        let placed = pack(&sizes, page, 0).map_err(|e| e.to_string())?;
        if placed.iter().all(|p| p.page == 0) {
            break placed;
        }
        if page >= 8192 {
            return Err(format!(
                "{} pictures don't fit in one {page}x{page} image",
                pictures.len()
            ));
        }
        page *= 2;
    };
    let width = placements.iter().map(|p| p.rect.x + p.rect.w).max();
    let height = placements.iter().map(|p| p.rect.y + p.rect.h).max();
    let mut sheet = IndexImage::new(width.unwrap_or(0).max(1), height.unwrap_or(0).max(1));
    let mut frames = Vec::with_capacity(pictures.len());
    for ((name, picture), placed) in pictures.iter().zip(&placements) {
        let r = placed.rect;
        let full = Rect {
            x: 0,
            y: 0,
            w: r.w,
            h: r.h,
        };
        sheet.blit(&picture.pixels, full, r.x, r.y);
        frames.push(json!({
            "filename": name,
            "frame": { "x": r.x, "y": r.y, "w": r.w, "h": r.h },
            "trimmed": true,
            "spriteSourceSize": {
                "x": picture.x + SHEET_ORIGIN,
                "y": picture.y + SHEET_ORIGIN,
                "w": r.w,
                "h": r.h,
            },
            "sourceSize": { "w": SHEET_FRAME, "h": SHEET_FRAME },
            "ticks": 1,
        }));
    }
    let tags: Vec<_> = pictures
        .iter()
        .enumerate()
        .map(|(i, (name, _))| json!({ "name": name, "from": i, "to": i, "direction": "forward" }))
        .collect();
    let palettes: serde_json::Map<String, serde_json::Value> = others
        .iter()
        .map(|&(name, p)| {
            let colours: Vec<String> = p
                .palette()
                .0
                .iter()
                .map(|&[r, g, b, a]| format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
                .collect();
            (name.to_string(), json!(colours))
        })
        .collect();
    let meta = json!({
        "frames": frames,
        "meta": {
            "image": image,
            "size": { "w": sheet.width, "h": sheet.height },
            "scale": "1",
            "frameTags": tags,
            "clamber": {
                "origin": { "x": SHEET_ORIGIN, "y": SHEET_ORIGIN },
                "pixelArt": true,
                "palettes": palettes,
            },
        },
    });
    let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    Ok(SheetFiles {
        json,
        png: encode_indexed_png(&sheet, palette),
    })
}

/// Frames by number, read through `table` from `bank_at`, which gives the
/// bytes from a location to the end of the image.
pub fn read_frames<'a>(
    table: &FrameTable,
    numbers: impl IntoIterator<Item = u16>,
    bank_at: impl Fn(Location) -> Option<&'a [u8]>,
) -> Result<BTreeMap<u16, Frame>, String> {
    let mut frames = BTreeMap::new();
    for number in numbers {
        let entry = table
            .entry(number)
            .ok_or_else(|| format!("the frame table has no frame {number}"))?;
        let bytes = bank_at(entry.location)
            .ok_or_else(|| format!("frame {number} is outside the image"))?;
        frames.insert(number, Frame::read(number, bytes)?);
    }
    Ok(frames)
}

/// Which frame has each picture hash in `wanted`, looking through every
/// full frame of `table` ("Picture hashes"). Of frames with the same
/// picture, the first is kept. A frame that can't be read has no hash, so
/// one bad frame doesn't keep the others from being found.
pub fn find_pictures<'a>(
    table: &FrameTable,
    wanted: &HashSet<Sha256Hash>,
    bank_at: impl Fn(Location) -> Option<&'a [u8]>,
) -> HashMap<Sha256Hash, u16> {
    let mut found = HashMap::new();
    let last = u16::try_from(table.len()).unwrap_or(u16::MAX);
    for number in LAST_SHORT + 1..last {
        let Some(bytes) = table.entry(number).and_then(|e| bank_at(e.location)) else {
            continue;
        };
        let Some(hash) = Frame::read(number, bytes)
            .ok()
            .and_then(|f| f.picture_hash())
        else {
            continue;
        };
        if wanted.contains(&hash) {
            found.entry(hash).or_insert(number);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use clamber_sprite::{Image as SheetImage, SpriteSheet, decode_png};

    use super::*;
    use crate::Image;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/sprites/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// Bytes written as hex, after any `address:` at the start of a line.
    fn hex_lines(name: &str) -> Vec<(Option<u16>, Vec<u8>)> {
        vector(name)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| {
                let (at, bytes) = match line.split_once(':') {
                    Some((at, rest)) => (Some(u16::from_str_radix(at.trim(), 16).unwrap()), rest),
                    None => (None, line),
                };
                let bytes = bytes
                    .split_whitespace()
                    .map(|b| u8::from_str_radix(b, 16).unwrap())
                    .collect();
                (at, bytes)
            })
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

    const BANK: u8 = 0x5A;
    const TABLE: u16 = 0x8010;
    const TABLE_FRAMES: usize = 39;

    /// An image with the vectors' bank in place, and its frame table.
    fn image() -> (Image, FrameTable) {
        let at = Location::new(BANK, 0).position(usize::MAX).unwrap();
        let mut bytes = vec![0; at + 0x1_0000];
        for (address, line) in hex_lines("bank.hex") {
            let p = at + usize::from(address.unwrap());
            bytes[p..p + line.len()].copy_from_slice(&line);
        }
        let image = Image::new(bytes);
        let table = image.at(Location::new(BANK, TABLE)).unwrap();
        let table = FrameTable::new(&table[..TABLE_ENTRY_SIZE * TABLE_FRAMES]).unwrap();
        (image, table)
    }

    fn frames() -> BTreeMap<u16, Frame> {
        let (image, table) = image();
        read_frames(&table, 1..TABLE_FRAMES as u16, |l| image.at(l)).unwrap()
    }

    fn object_tiles() -> TileSet {
        let bytes: Vec<u8> = hex_lines("object-tiles.hex")
            .into_iter()
            .flat_map(|(_, b)| b)
            .collect();
        TileSet::new(&bytes).unwrap()
    }

    fn palette() -> SpritePalette {
        let bytes: Vec<u8> = hex_lines("palette.hex")
            .into_iter()
            .flat_map(|(_, b)| b)
            .collect();
        SpritePalette::new(&bytes).unwrap()
    }

    #[test]
    fn the_table_matches_the_spec_vectors() {
        let (_, table) = image();
        assert_eq!(table.len(), TABLE_FRAMES);
        let rows = rows("table.tsv");
        assert_eq!(rows.len(), TABLE_FRAMES);
        for row in rows {
            let [frame, bank, address, order, form] = &row[..] else {
                panic!("bad row {row:?}");
            };
            let frame: u16 = num(frame);
            let want_form = match form.as_str() {
                "none" => Form::None,
                "short" => Form::Short,
                "full" => Form::Full,
                f => panic!("form {f}"),
            };
            assert_eq!(Form::of(frame), want_form, "frame {frame}");
            let location = Location::new(
                u8::from_str_radix(bank, 16).unwrap(),
                u16::from_str_radix(address, 16).unwrap(),
            );
            if frame == 0 {
                assert_eq!(table.entry(0), None);
                assert_eq!(location, Location::new(0, 0));
                continue;
            }
            assert_eq!(
                table.entry(frame),
                Some(TableEntry {
                    location,
                    order: num(order)
                }),
                "frame {frame}"
            );
        }
        assert_eq!(table.entry(TABLE_FRAMES as u16), None);
        assert!(FrameTable::new(&[0; 5]).is_err());
    }

    #[test]
    fn headers_match_the_spec_vectors() {
        let frames = frames();
        for row in rows("frames.tsv") {
            let [number, form, l, s1, t1, s2, t2, n1, d2, n2, length] = &row[..] else {
                panic!("bad row {row:?}");
            };
            let frame = &frames[&num(number)];
            assert_eq!(frame.form == Form::Full, form == "full", "{row:?}");
            let runs = (form == "full").then(|| Runs {
                n1: num(n1),
                d2: num(d2),
                n2: num(n2),
            });
            let header = Header {
                l: num(l),
                s1: num(s1),
                t1: num(t1),
                s2: num(s2),
                t2: num(t2),
                runs,
            };
            assert_eq!(frame.header, header, "{row:?}");
            assert_eq!(frame.len(), num::<usize>(length), "{row:?}");
        }
    }

    #[test]
    fn frames_stop_at_their_end() {
        let (image, table) = image();
        for (&number, frame) in &frames() {
            let bytes = image.at(table.entry(number).unwrap().location).unwrap();
            let exact = &bytes[..frame.len()];
            assert_eq!(&Frame::read(number, exact).unwrap(), frame, "{number}");
            assert!(Frame::read(number, &exact[..exact.len() - 1]).is_err());
        }
        assert!(Frame::read(0, &[0; 8]).is_err());
    }

    #[test]
    fn pieces_match_the_spec_vectors() {
        let frames = frames();
        let mut by_frame: BTreeMap<u16, Vec<Piece>> = BTreeMap::new();
        for row in rows("pieces.tsv") {
            let [number, i, size, x, y, tiles] = &row[..] else {
                panic!("bad row {row:?}");
            };
            let tiles: Vec<u16> = tiles.split(',').map(num).collect();
            let piece = Piece {
                size: if size == "large" {
                    Size::Large
                } else {
                    Size::Small
                },
                x: num(x),
                y: num(y),
                tile: tiles[0],
            };
            assert_eq!(piece.sheet_tiles(), tiles, "{row:?}");
            let list = by_frame.entry(num(number)).or_default();
            assert_eq!(list.len(), num::<usize>(i));
            list.push(piece);
        }
        for (number, pieces) in by_frame {
            assert_eq!(frames[&number].pieces, pieces, "frame {number}");
        }
    }

    #[test]
    fn sheets_match_the_spec_vectors() {
        let frames = frames();
        let mut by_frame: BTreeMap<u16, Vec<(u16, u8, u16)>> = BTreeMap::new();
        for row in rows("sheets.tsv") {
            let [number, at, run, i] = &row[..] else {
                panic!("bad row {row:?}");
            };
            by_frame
                .entry(num(number))
                .or_default()
                .push((num(at), num(run), num(i)));
        }
        assert!(by_frame.len() >= 3);
        for (number, mut want) in by_frame {
            let runs = frames[&number].header.runs.unwrap();
            let mut got: Vec<_> = runs.slots().collect();
            got.sort();
            want.sort();
            assert_eq!(got, want, "frame {number}");
        }
    }

    /// Draws a picture as `pictures.txt` does.
    fn text(picture: &Picture) -> String {
        let (w, h) = (picture.pixels.width, picture.pixels.height);
        let (x, y) = (picture.x, picture.y);
        let mut out = format!("x {x}..{}, y {y}..{}\n", x + w as i32 - 1, y + h as i32 - 1);
        for py in 0..h {
            for px in 0..w {
                let c = picture.pixels.get(px, py);
                let origin = (x + px as i32, y + py as i32) == (0, 0);
                out.push(match c {
                    0 if origin => '+',
                    0 => '.',
                    c => char::from_digit(u32::from(c), 16).unwrap(),
                });
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn pictures_match_the_spec_vectors() {
        let frames = frames();
        let tiles = object_tiles();
        let text_file = vector("pictures.txt");
        let mut checked = 0;
        for section in text_file.split("## ").filter(|s| !s.trim().is_empty()) {
            let (title, want) = section.split_once('\n').unwrap();
            let (frame, how) = title.trim().split_once(", ").unwrap();
            let number: u16 = num(frame.strip_prefix("frame ").unwrap());
            let mirror = match how {
                "as stored" => Mirror::default(),
                "mirrored left-right" => Mirror {
                    left_right: true,
                    top_bottom: false,
                },
                "mirrored top-bottom" => Mirror {
                    left_right: false,
                    top_bottom: true,
                },
                "mirrored both ways" => Mirror {
                    left_right: true,
                    top_bottom: true,
                },
                h => panic!("{h}"),
            };
            let picture = frames[&number].picture(Some(&tiles), mirror).unwrap();
            assert_eq!(
                text(&picture),
                want.trim_end().to_string() + "\n",
                "{title}"
            );
            checked += 1;
        }
        assert_eq!(checked, 10);
    }

    #[test]
    fn short_frames_need_the_objects_tiles() {
        let frames = frames();
        assert!(frames[&5].picture(None, Mirror::default()).is_err());
        let few = TileSet::new(&[0; 32 * 9]).unwrap();
        assert!(frames[&5].picture(Some(&few), Mirror::default()).is_err());
        // Full frames use their own tiles, whatever is in place.
        assert_eq!(
            frames[&35].picture(None, Mirror::default()),
            frames[&35].picture(Some(&few), Mirror::default())
        );
    }

    #[test]
    fn the_palette_matches_the_spec_vectors() {
        let palette = palette();
        assert_eq!(palette.colour(0), None);
        for row in rows("palette.tsv").into_iter().skip(1) {
            let [n, r, g, b] = &row[..] else {
                panic!("bad row {row:?}");
            };
            let colour = palette.colour(num(n)).unwrap();
            let want = [num::<u8>(r), num(g), num(b)];
            assert_eq!(crate::channels(colour), want, "colour {n}");
            let [r8, g8, b8] = rgb(colour);
            assert_eq!(palette.palette().0[num::<usize>(n)], [r8, g8, b8, 255]);
        }
        assert_eq!(palette.palette().0[0][3], 0, "colour 0 is transparent");
        assert!(SpritePalette::new(&[0; 29]).is_err());
    }

    #[test]
    fn the_synthetic_example_in_the_format() {
        let mut bytes = vec![0x02, 0x03, 0x04, 0x01, 0x14, 0x07, 0x10, 0x05];
        bytes.extend([0x80, 0x70]);
        bytes.extend([0x80; 10]);
        bytes.extend([0; 12 * 32]);
        let frame = Frame::read(35, &bytes).unwrap();
        assert_eq!(frame.len(), 404);
        assert_eq!(frame.pieces[0].sheet_tiles(), [0, 1, 16, 17]);
        assert_eq!(frame.pieces[1].sheet_tiles(), [2, 3, 18, 19]);
        let small: Vec<u16> = frame.pieces[2..].iter().map(|p| p.tile).collect();
        assert_eq!(small, [4, 5, 6, 20]);
        assert_eq!((frame.pieces[0].x, frame.pieces[0].y), (0, -16));
    }

    /// Draws sheet frame `i` with its origin at (`ox`, `oy`) of a canvas
    /// of colour numbers.
    fn draw(sheet: &SpriteSheet, i: usize, canvas: &mut IndexImage, ox: i32, oy: i32) {
        let SheetImage::Indexed { indices, .. } = &sheet.image else {
            panic!("not indexed");
        };
        let f = &sheet.sheet.frames[i];
        for y in 0..f.rect.h {
            for x in 0..f.rect.w {
                let c = indices.get(f.rect.x + x, f.rect.y + y);
                let (cx, cy) = (ox + f.offset.0 + x as i32, oy + f.offset.1 + y as i32);
                if c != 0 {
                    canvas.set(cx as u32, cy as u32, c);
                }
            }
        }
    }

    #[test]
    fn sheets_show_the_pictures_from_their_origins() {
        let frames = frames();
        let tiles = object_tiles();
        let numbers = [5, 35, 36, 37];
        let pictures: Vec<(String, Picture)> = numbers
            .iter()
            .map(|&n| {
                let p = frames[&n].picture(Some(&tiles), Mirror::default());
                (format!("frame-{n}"), p.unwrap())
            })
            .collect();
        let files = sprite_sheet(&pictures, &palette(), &[], "frames.png").unwrap();
        let meta = clamber_sprite::Sheet::from_json_str(&files.json, "frames.json").unwrap();
        assert_eq!(meta.image, "frames.png");
        assert!(meta.pixel_art);
        let image = decode_png(&files.png).unwrap();
        let SheetImage::Indexed { palette: p, .. } = &image else {
            panic!("not indexed");
        };
        assert_eq!(p, &palette().palette());
        let sheet = SpriteSheet::new(meta, image).unwrap();
        assert_eq!(sheet.sheet.frames.len(), numbers.len());
        for (i, animation) in sheet.sheet.animations.iter().enumerate() {
            assert_eq!(animation.name, pictures[i].0);
            assert_eq!(animation.frames, [i]);
        }
        assert_eq!(sheet.sheet.animations.len(), numbers.len());

        // Frame 36 reaches from -128 to 134 each way.
        let o = 160;
        for (i, (_, picture)) in pictures.iter().enumerate() {
            let mut drawn = IndexImage::new(2 * o as u32, 2 * o as u32);
            draw(&sheet, i, &mut drawn, o, o);
            let mut want = IndexImage::new(2 * o as u32, 2 * o as u32);
            let (w, h) = (picture.pixels.width, picture.pixels.height);
            for y in 0..h {
                for x in 0..w {
                    let (cx, cy) = (o + picture.x + x as i32, o + picture.y + y as i32);
                    want.set(cx as u32, cy as u32, picture.pixels.get(x, y));
                }
            }
            assert_eq!(drawn, want, "{}", pictures[i].0);
        }
    }

    #[test]
    fn sheets_carry_their_other_palettes_by_name() {
        let frames = frames();
        let picture = frames[&35].picture(None, Mirror::default()).unwrap();
        let mut bytes = vec![0; SpritePalette::SIZE];
        bytes[0] = 0x1F;
        let red = SpritePalette::new(&bytes).unwrap();
        let files = sprite_sheet(
            &[("a".into(), picture)],
            &palette(),
            &[("red", &red)],
            "a.png",
        );
        let meta = clamber_sprite::Sheet::from_json_str(&files.unwrap().json, "a.json").unwrap();
        assert_eq!(meta.palettes.len(), 1);
        assert_eq!(meta.palettes["red"], red.palette());
    }

    #[test]
    fn engine_mirroring_matches_the_format() {
        // Mirroring a sheet frame about its origin, as the engine does,
        // gives the format's mirrored picture.
        let frames = frames();
        let stored = frames[&35].picture(None, Mirror::default()).unwrap();
        let mirrored = frames[&35]
            .picture(
                None,
                Mirror {
                    left_right: true,
                    top_bottom: false,
                },
            )
            .unwrap();
        let w = stored.pixels.width as i32;
        assert_eq!(mirrored.x, -stored.x - w);
        for y in 0..stored.pixels.height {
            for x in 0..stored.pixels.width {
                assert_eq!(
                    mirrored.pixels.get(w as u32 - 1 - x, y),
                    stored.pixels.get(x, y)
                );
            }
        }
    }

    #[test]
    fn picture_hashes_match_the_spec_vectors() {
        let frames = frames();
        let rows = rows("hashes.tsv");
        assert_eq!(rows.len(), 3);
        for row in rows {
            let [frame, left, top, width, height, opaque, bytes, sha] = &row[..] else {
                panic!("bad row {row:?}");
            };
            let frame = &frames[&num(frame)];
            let picture = frame.picture(None, Mirror::default()).unwrap();
            assert_eq!(picture.opaque(), num::<usize>(opaque), "{row:?}");
            let input = picture.hash_input().unwrap();
            assert_eq!(input.len(), num::<usize>(bytes), "{row:?}");
            let i16_at = |i: usize| i16::from_le_bytes([input[i], input[i + 1]]);
            let u16_at = |i: usize| u16::from_le_bytes([input[i], input[i + 1]]);
            assert_eq!(i16_at(0), num::<i16>(left), "{row:?}");
            assert_eq!(i16_at(2), num::<i16>(top), "{row:?}");
            assert_eq!(u16_at(4), num::<u16>(width), "{row:?}");
            assert_eq!(u16_at(6), num::<u16>(height), "{row:?}");
            assert_eq!(frame.picture_hash(), Sha256Hash::from_hex(sha), "{row:?}");
        }
    }

    #[test]
    fn hash_input_matches_the_spec_vector() {
        let want: Vec<u8> = hex_lines("hash-input-35.hex")
            .into_iter()
            .flat_map(|(_, b)| b)
            .collect();
        let picture = frames()[&35].picture(None, Mirror::default()).unwrap();
        assert_eq!(picture.hash_input().unwrap(), want);
    }

    #[test]
    fn hash_input_matches_the_spec_example() {
        // Colour 3 at (-1, -2) and colour 5 at (0, -1), in a picture
        // reaching past them.
        let mut pixels = IndexImage::new(4, 4);
        pixels.set(1, 0, 3);
        pixels.set(2, 1, 5);
        let picture = Picture {
            x: -2,
            y: -2,
            pixels,
        };
        assert_eq!(
            picture.hash_input().unwrap(),
            [0xff, 0xff, 0xfe, 0xff, 2, 0, 2, 0, 3, 0, 0, 5]
        );
        assert_eq!(picture.opaque(), 2);
        let empty = Picture {
            x: 0,
            y: 0,
            pixels: IndexImage::new(3, 3),
        };
        assert_eq!(empty.hash_input(), None);
    }

    #[test]
    fn only_full_frames_with_enough_opaque_pixels_have_hashes() {
        let frames = frames();
        assert_eq!(frames[&5].picture_hash(), None);
        // A full frame with one small piece of 4 opaque pixels.
        let mut bytes = vec![0, 1, 0, 0, 0, 1, 0, 0, 128, 128];
        let mut tile = [0u8; TILE_BYTES];
        tile[0] = 0xF0;
        bytes.extend(tile);
        let few = Frame::read(40, &bytes).unwrap();
        assert_eq!(few.picture(None, Mirror::default()).unwrap().opaque(), 4);
        assert_eq!(few.picture_hash(), None);
    }

    #[test]
    fn pictures_are_found_by_hash() {
        let (image, table) = image();
        let frames = frames();
        let hashes: Vec<Sha256Hash> = [35, 37]
            .iter()
            .map(|n| frames[n].picture_hash().unwrap())
            .collect();
        let missing = Sha256Hash([7; 32]);
        let wanted: HashSet<_> = hashes.iter().copied().chain([missing]).collect();
        let found = find_pictures(&table, &wanted, |l| image.at(l));
        assert_eq!(found.len(), 2);
        assert_eq!(found[&hashes[0]], 35);
        assert_eq!(found[&hashes[1]], 37);
        assert!(!found.contains_key(&missing));
    }

    #[test]
    fn frames_without_pieces_have_empty_pictures() {
        let frame = Frame::read(40, &[0; 8]).unwrap();
        assert!(frame.is_empty());
        let picture = frame.picture(None, Mirror::default()).unwrap();
        assert_eq!((picture.pixels.width, picture.pixels.height), (0, 0));
        let files = sprite_sheet(&[("empty".into(), picture)], &palette(), &[], "e.png").unwrap();
        let meta = clamber_sprite::Sheet::from_json_str(&files.json, "e.json").unwrap();
        let sheet = SpriteSheet::new(meta, decode_png(&files.png).unwrap()).unwrap();
        assert_eq!(sheet.sheet.frames[0].rect.w, 0);
    }
}
