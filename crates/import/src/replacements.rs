//! Replacement art packs: new pictures for an imported game's sprite
//! frames, tiles and images, drawn instead of the originals at a higher
//! resolution.
//!
//! A replacement pack is a folder in the [`art_dir`] holding an
//! [`ART_MANIFEST`] and a [`PICTURES`] folder (with any subfolders) of
//! PNGs, each named after the [`picture hash`](frame_hash) of the
//! picture it replaces: `<hash>.png`, or `<hash>.<palette>.png` for a
//! sprite frame in one of its sheet's named palettes only. A replacement
//! is a whole number of times the original's size, the same number across
//! and down, and is drawn in the original's place at its size. Packs hold
//! only new art, so a pack works with any copy of the game and can be
//! shared. Several packs stack: where two replace the same picture, the
//! one whose folder name sorts first wins.
//!
//! Pictures are matched by their hashes:
//!
//! - A **sprite frame** of an indexed sheet by its picture hash as
//!   `spec/formats/sprites.md` ("Picture hashes") gives it: the box of
//!   its opaque pixels from the frame's origin, then a colour number per
//!   pixel of the box ([`frame_hash`]). Its replacement covers that box.
//! - A **tile** of a tileset, or a whole **image** a layer draws, by the
//!   engine's own hash of its colours ([`image_hash`]).
//!
//! [`replace_sheet`], [`replace_tileset`] and [`replace_image`] make an
//! original's replaced copy, scaled up so it's drawn the same size;
//! [`dump_pictures`] writes a pack's own pictures under their hashes, as
//! templates for artists.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use clamber_level::{LayerContent, Level, Tileset};
use clamber_sprite::{
    Image, IndexImage, Palette, Pixels, Rect, RgbaImage, SpriteSheet, decode_png, encode_png,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{LEVELS, SPRITES, Sha256Hash, packs_dir};

/// The file that marks a folder as a replacement pack.
pub const ART_MANIFEST: &str = "art.toml";

/// The folder in a replacement pack that holds its pictures.
pub const PICTURES: &str = "pictures";

/// The version of the replacement pack format this engine reads.
pub const ART_FORMAT: u32 = 1;

/// The largest number of times a picture's size its replacement can be.
pub const MAX_FACTOR: u32 = 16;

/// The index [`dump_pictures`] writes next to the pictures.
pub const DUMP_INDEX: &str = "pictures.tsv";

/// Where replacement packs go: an `art` folder next to the
/// [`packs_dir`]. `None` when the environment doesn't say where that is.
pub fn art_dir() -> Option<PathBuf> {
    Some(packs_dir()?.parent()?.join("art"))
}

/// A replacement pack's manifest.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtManifest {
    pub format: u32,
    /// What the pack is called, as shown to the player.
    pub name: String,
}

/// The pictures of every replacement pack in a folder, by the hash of
/// the picture each replaces. They're read as they're used.
#[derive(Clone, Debug, Default)]
pub struct Replacements {
    /// By hash, then palette (`None` for any palette).
    files: HashMap<Sha256Hash, BTreeMap<Option<String>, PathBuf>>,
    /// The packs' names, in priority order.
    pub names: Vec<String>,
}

impl Replacements {
    /// The packs in `art_dir`, with why any folder there couldn't be read
    /// as one. There are none if the folder doesn't exist. Folders
    /// starting with `.` are skipped.
    pub fn load(art_dir: &Path) -> (Replacements, Vec<String>) {
        let mut problems = Vec::new();
        let mut dirs: Vec<PathBuf> = match fs::read_dir(art_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .filter(|p| {
                    !p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        dirs.sort();
        let mut replacements = Replacements::default();
        for dir in dirs {
            if let Err(e) = replacements.add_pack(&dir) {
                problems.push(format!("{}: {e}", dir.display()));
            }
        }
        (replacements, problems)
    }

    /// Adds the pack in `dir` below the ones added before.
    pub fn add_pack(&mut self, dir: &Path) -> Result<(), String> {
        let src = fs::read_to_string(dir.join(ART_MANIFEST))
            .map_err(|e| format!("{ART_MANIFEST}: {e}"))?;
        let manifest: ArtManifest =
            toml::from_str(&src).map_err(|e| format!("{ART_MANIFEST}: {}", e.message()))?;
        if manifest.format != ART_FORMAT {
            return Err(format!(
                "{ART_MANIFEST}: format {} isn't {ART_FORMAT}, the one this version reads",
                manifest.format
            ));
        }
        let mut files = Vec::new();
        find_pngs(&dir.join(PICTURES), &mut files);
        files.sort();
        for path in files {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let (hex, palette) = match stem.split_once('.') {
                Some((hex, palette)) => (hex, Some(palette.to_string())),
                None => (&*stem, None),
            };
            let Some(hash) = Sha256Hash::from_hex(&hex.to_ascii_lowercase()) else {
                continue;
            };
            self.files
                .entry(hash)
                .or_default()
                .entry(palette)
                .or_insert(path);
        }
        self.names.push(manifest.name);
        Ok(())
    }

    /// Whether there are no replacement pictures at all.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The replacement for the picture `hash` in `palette` (its own
    /// replacement for that palette, or else the one for any palette), if
    /// there is one, read as it's asked for. A file that can't be read is
    /// a problem, and no replacement.
    fn get(
        &self,
        hash: Sha256Hash,
        palette: Option<&str>,
        problems: &mut Vec<String>,
    ) -> Option<RgbaImage> {
        let by = self.files.get(&hash)?;
        let path = palette
            .and_then(|p| by.get(&Some(p.to_string())))
            .or_else(|| by.get(&None))?;
        let read = fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| decode_png(&b).map_err(|e| e.0));
        match read {
            Ok(image) => Some(image.to_rgba()),
            Err(e) => {
                problems.push(format!("{}: {e}", path.display()));
                None
            }
        }
    }

    /// The replacement for a picture `size` big, with how many times its
    /// size it is, if it has one of a size that fits.
    fn fitting(
        &self,
        hash: Sha256Hash,
        palette: Option<&str>,
        size: (u32, u32),
        problems: &mut Vec<String>,
    ) -> Option<(RgbaImage, u32)> {
        let image = self.get(hash, palette, problems)?;
        match factor(size, (image.width, image.height)) {
            Some(k) => Some((image, k)),
            None => {
                problems.push(format!(
                    "the replacement for {hash} is {}x{}, which isn't 1 to {MAX_FACTOR} times \
                     the original's {}x{}",
                    image.width, image.height, size.0, size.1
                ));
                None
            }
        }
    }
}

fn find_pngs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        if path.is_dir() {
            find_pngs(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
        {
            out.push(path);
        }
    }
}

/// How many times `size` `replacement` is, if it's a whole number from 1
/// to [`MAX_FACTOR`], the same across and down.
fn factor(size: (u32, u32), replacement: (u32, u32)) -> Option<u32> {
    if size.0 == 0 || size.1 == 0 || !replacement.0.is_multiple_of(size.0) {
        return None;
    }
    let k = replacement.0 / size.0;
    (1..=MAX_FACTOR)
        .contains(&k)
        .then_some(k)
        .filter(|&k| replacement.1 == size.1 * k)
}

/// A sprite frame's picture hash (`spec/formats/sprites.md`, "Picture
/// hashes"), for the frame whose colour numbers are `rect` of `indices`,
/// drawn at `offset` from its origin, with the box of its opaque pixels
/// in `indices`. `None` when no pixel is opaque.
pub fn frame_hash(
    indices: &IndexImage,
    rect: Rect,
    offset: (i32, i32),
) -> Option<(Sha256Hash, Rect)> {
    let opaque = |x, y| indices.get(rect.x + x, rect.y + y) != 0;
    let rows: Vec<u32> = (0..rect.h)
        .filter(|&y| (0..rect.w).any(|x| opaque(x, y)))
        .collect();
    let cols: Vec<u32> = (0..rect.w)
        .filter(|&x| (0..rect.h).any(|y| opaque(x, y)))
        .collect();
    let (&top, &bottom) = (rows.first()?, rows.last()?);
    let (&left, &right) = (cols.first()?, cols.last()?);
    let (w, h) = (right - left + 1, bottom - top + 1);
    let mut bytes = Vec::with_capacity(8 + (w * h) as usize);
    bytes.extend(&(offset.0 + left as i32).to_le_bytes()[..2]);
    bytes.extend(&(offset.1 + top as i32).to_le_bytes()[..2]);
    bytes.extend(&w.to_le_bytes()[..2]);
    bytes.extend(&h.to_le_bytes()[..2]);
    for y in top..=bottom {
        bytes.extend((left..=right).map(|x| indices.get(rect.x + x, rect.y + y)));
    }
    let hash = Sha256Hash(Sha256::digest(&bytes).into());
    let at = Rect {
        x: rect.x + left,
        y: rect.y + top,
        w,
        h,
    };
    Some((hash, at))
}

/// The engine's hash of a full-colour picture, `rect` of `image`: the
/// SHA-256 of its width and height, each as an unsigned 32-bit number low
/// byte first, then each pixel's red, green, blue and alpha, row by row
/// from the top and left to right within a row, with a pixel whose alpha
/// is 0 as four 0 bytes. `None` when every pixel is transparent.
pub fn image_hash(image: &RgbaImage, rect: Rect) -> Option<Sha256Hash> {
    let mut hasher = Sha256::new();
    hasher.update(rect.w.to_le_bytes());
    hasher.update(rect.h.to_le_bytes());
    let mut any = false;
    for y in rect.y..rect.y + rect.h {
        for x in rect.x..rect.x + rect.w {
            let p = image.get(x, y);
            any |= p[3] != 0;
            hasher.update(if p[3] == 0 { [0; 4] } else { p });
        }
    }
    any.then(|| Sha256Hash(hasher.finalize().into()))
}

/// `image` scaled up `k` times, each pixel a `k` × `k` block.
fn scale_up<P: Copy + Default>(image: &Pixels<P>, k: u32) -> Pixels<P> {
    let mut out = Pixels::new(image.width * k, image.height * k);
    for y in 0..out.height {
        for x in 0..out.width {
            out.set(x, y, image.get(x / k, y / k));
        }
    }
    out
}

fn scale_rect(r: Rect, k: u32) -> Rect {
    Rect {
        x: r.x * k,
        y: r.y * k,
        w: r.w * k,
        h: r.h * k,
    }
}

/// One factor all of `factors` divide, the largest of them if that would
/// be more than [`MAX_FACTOR`].
fn common_factor(factors: impl Iterator<Item = u32>) -> u32 {
    let gcd = |mut a: u32, mut b: u32| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    let (mut lcm, mut max) = (1u32, 1u32);
    for k in factors {
        lcm = lcm / gcd(lcm, k) * k;
        max = max.max(k);
    }
    if lcm <= MAX_FACTOR { lcm } else { max }
}

/// A picture placed at `at` of an image scaled up `big` times, from a
/// replacement `k` times the original's size.
fn paste(out: &mut RgbaImage, replacement: &RgbaImage, k: u32, at: Rect, big: u32) {
    let at = scale_rect(at, big);
    for y in 0..at.h {
        for x in 0..at.w {
            let p = replacement.get(x * k / big, y * k / big);
            out.set(at.x + x, at.y + y, p);
        }
    }
}

/// `sheet` with the frames that have replacements drawn with them: a
/// full-colour sheet with as many more texels per world unit as the
/// largest replacement needs, with a variant for each palette the
/// original had. `None` when the sheet isn't indexed or has no frame
/// with a replacement.
pub fn replace_sheet(
    sheet: &SpriteSheet,
    replacements: &Replacements,
    problems: &mut Vec<String>,
) -> Option<SpriteSheet> {
    let Image::Indexed { indices, palette } = &sheet.image else {
        return None;
    };
    if sheet.normals.is_some() || replacements.is_empty() {
        return None;
    }
    let palettes: Vec<(Option<&str>, &Palette)> = std::iter::once((None, palette))
        .chain(
            sheet
                .sheet
                .palettes
                .iter()
                .map(|(n, p)| (Some(n.as_str()), p)),
        )
        .collect();
    // The replacements for each palette, with where they go and how many
    // times the original's size they are.
    let mut found: Vec<Vec<(Rect, RgbaImage, u32)>> = vec![Vec::new(); palettes.len()];
    for frame in &sheet.sheet.frames {
        let Some((hash, at)) = frame_hash(indices, frame.rect, frame.offset) else {
            continue;
        };
        for (i, &(name, _)) in palettes.iter().enumerate() {
            if let Some((image, k)) = replacements.fitting(hash, name, (at.w, at.h), problems) {
                found[i].push((at, image, k));
            }
        }
    }
    if found.iter().all(Vec::is_empty) {
        return None;
    }
    let big = common_factor(found.iter().flatten().map(|f| f.2));
    let mut images = palettes.iter().zip(&found).map(|(&(_, palette), found)| {
        let mut image = scale_up(&palette.apply(indices), big);
        for (at, replacement, k) in found {
            paste(&mut image, replacement, *k, *at, big);
        }
        Image::Rgba(image)
    });
    let image = images.next()?;
    let variants: BTreeMap<String, Image> = palettes[1..]
        .iter()
        .filter_map(|&(n, _)| n.map(str::to_string))
        .zip(images)
        .collect();
    let mut meta = sheet.sheet.clone();
    meta.scale *= big;
    for frame in &mut meta.frames {
        frame.rect = scale_rect(frame.rect, big);
        frame.offset = (frame.offset.0 * big as i32, frame.offset.1 * big as i32);
        for area in &mut frame.boxes {
            area.x *= big as i32;
            area.y *= big as i32;
            area.w *= big;
            area.h *= big;
        }
    }
    meta.palettes.clear();
    meta.variants = variants
        .keys()
        .map(|n| (n.clone(), format!("{n}.png")))
        .collect();
    Some(SpriteSheet {
        sheet: meta,
        image,
        variants,
        normals: None,
    })
}

/// Each tile's place in the image of `set`.
fn tile_rects(set: &Tileset, image: &RgbaImage) -> Vec<Rect> {
    let fit = |size: u32, tile: u32| {
        (size.saturating_sub(2 * set.margin) + set.spacing) / (tile + set.spacing).max(1)
    };
    let across = fit(image.width, set.tile_width);
    let down = fit(image.height, set.tile_height);
    (0..across * down)
        .map(|i| Rect {
            x: set.margin + i % across * (set.tile_width + set.spacing),
            y: set.margin + i / across * (set.tile_height + set.spacing),
            w: set.tile_width,
            h: set.tile_height,
        })
        .collect()
}

/// The image of tileset `set` with the tiles that have replacements drawn
/// with them, and how many times the original's size it is: the tileset's
/// tile sizes, margin and spacing are all that many times as big. `None`
/// when no tile has a replacement.
pub fn replace_tileset(
    image: &RgbaImage,
    set: &Tileset,
    replacements: &Replacements,
    problems: &mut Vec<String>,
) -> Option<(RgbaImage, u32)> {
    if replacements.is_empty() {
        return None;
    }
    let found: Vec<(Rect, RgbaImage, u32)> = tile_rects(set, image)
        .into_iter()
        .filter_map(|at| {
            let hash = image_hash(image, at)?;
            let (replacement, k) = replacements.fitting(hash, None, (at.w, at.h), problems)?;
            Some((at, replacement, k))
        })
        .collect();
    if found.is_empty() {
        return None;
    }
    let big = common_factor(found.iter().map(|f| f.2));
    let mut out = scale_up(image, big);
    for (at, replacement, k) in &found {
        paste(&mut out, replacement, *k, *at, big);
    }
    Some((out, big))
}

/// A layer's whole `image`'s replacement, and how many times the
/// original's size it is, if it has one.
pub fn replace_image(
    image: &RgbaImage,
    replacements: &Replacements,
    problems: &mut Vec<String>,
) -> Option<(RgbaImage, u32)> {
    if replacements.is_empty() {
        return None;
    }
    let all = Rect {
        x: 0,
        y: 0,
        w: image.width,
        h: image.height,
    };
    let hash = image_hash(image, all)?;
    replacements.fitting(hash, None, (all.w, all.h), problems)
}

/// Writes every picture of the pack in `pack_dir` that a replacement can
/// stand in for into `out` (made if it isn't there), each named after its
/// hash as a replacement for it would be, with an index ([`DUMP_INDEX`])
/// saying what each is. Sprite frames are in their sheet's own colours.
/// Returns how many different pictures it wrote.
///
/// These are the player's own pictures from their own copy of the game:
/// templates to paint over, never to be shared.
pub fn dump_pictures(pack_dir: &Path, out: &Path) -> Result<usize, String> {
    let err = |path: &Path, e: String| format!("{}: {e}", path.display());
    fs::create_dir_all(out).map_err(|e| err(out, e.to_string()))?;
    let mut index = String::from("hash\tkind\tsize\twhere\n");
    let mut written = std::collections::HashSet::new();
    let mut write = |hash: Sha256Hash, image: RgbaImage, kind: &str, place: String| {
        let _ = writeln!(
            index,
            "{hash}\t{kind}\t{}x{}\t{place}",
            image.width, image.height
        );
        if !written.insert(hash) {
            return Ok(());
        }
        let path = out.join(format!("{hash}.png"));
        fs::write(&path, encode_png(&image)).map_err(|e| err(&path, e.to_string()))
    };

    for path in files_in(&pack_dir.join(SPRITES), "json") {
        let sheet = SpriteSheet::load(&path).map_err(|e| e.to_string())?;
        let Image::Indexed { indices, palette } = &sheet.image else {
            continue;
        };
        let colours = palette.apply(indices);
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        for (i, frame) in sheet.sheet.frames.iter().enumerate() {
            let Some((hash, at)) = frame_hash(indices, frame.rect, frame.offset) else {
                continue;
            };
            let label = sheet
                .sheet
                .animations
                .iter()
                .find(|a| a.frames.contains(&i))
                .map_or_else(|| i.to_string(), |a| a.name.clone());
            write(hash, colours.crop(at), "frame", format!("{name} {label}"))?;
        }
    }

    for path in files_in(&pack_dir.join(LEVELS), "level") {
        let level = Level::load(&path).map_err(|e| e.to_string())?;
        let dir = path.parent().unwrap_or(Path::new(""));
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        let read = |file: &str| {
            let at = dir.join(file);
            let bytes = fs::read(&at).map_err(|e| err(&at, e.to_string()))?;
            decode_png(&bytes)
                .map(|i| i.to_rgba())
                .map_err(|e| err(&at, e.0))
        };
        for layer in level.layers.iter().filter(|l| l.pixel_art) {
            let place = format!("{name} {}", layer.name);
            match &layer.content {
                LayerContent::Tiles(tiles) => {
                    let image = read(&tiles.tileset.image)?;
                    for (i, at) in tile_rects(&tiles.tileset, &image).into_iter().enumerate() {
                        if let Some(hash) = image_hash(&image, at) {
                            write(hash, image.crop(at), "tile", format!("{place} {i}"))?;
                        }
                    }
                }
                LayerContent::Image { path, .. } => {
                    let image = read(path)?;
                    let all = Rect {
                        x: 0,
                        y: 0,
                        w: image.width,
                        h: image.height,
                    };
                    if let Some(hash) = image_hash(&image, all) {
                        write(hash, image, "image", place)?;
                    }
                }
                LayerContent::Props(props) => {
                    for prop in props {
                        let image = read(&prop.image)?;
                        let all = Rect {
                            x: 0,
                            y: 0,
                            w: image.width,
                            h: image.height,
                        };
                        if let Some(hash) = image_hash(&image, all) {
                            write(hash, image, "image", place.clone())?;
                        }
                    }
                }
            }
        }
    }
    let count = written.len();
    let path = out.join(DUMP_INDEX);
    fs::write(&path, index).map_err(|e| err(&path, e.to_string()))?;
    Ok(count)
}

/// The files in `dir` with extension `ext`, in name order.
fn files_in(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use clamber_sprite::{Rgba, decode_png};

    use super::*;
    use crate::{Picture, SpritePalette, sprite_sheet};

    /// A made-up folder for a test, empty.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clamber-art-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A replacement pack called `name` in `art`, with `pictures` by file
    /// name.
    fn art_pack(art: &Path, name: &str, pictures: &[(String, &RgbaImage)]) {
        let dir = art.join(name);
        fs::create_dir_all(dir.join(PICTURES).join("some")).unwrap();
        fs::write(
            dir.join(ART_MANIFEST),
            format!("format = 1\nname = \"{name}\"\n"),
        )
        .unwrap();
        for (file, image) in pictures {
            // Subfolders are for the artist's own order.
            let path = dir.join(PICTURES).join("some").join(file);
            fs::write(path, encode_png(image)).unwrap();
        }
    }

    fn filled(w: u32, h: u32, colour: Rgba) -> RgbaImage {
        let mut image = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                image.set(x, y, colour);
            }
        }
        image
    }

    /// A picture of colour numbers, its top left at `(x, y)` from the
    /// origin.
    fn picture(x: i32, y: i32, rows: &[&[u8]]) -> Picture {
        let mut pixels = IndexImage::new(rows[0].len() as u32, rows.len() as u32);
        for (py, row) in rows.iter().enumerate() {
            for (px, &c) in row.iter().enumerate() {
                pixels.set(px as u32, py as u32, c);
            }
        }
        Picture { x, y, pixels }
    }

    fn sprite_palette(first: u16) -> SpritePalette {
        let bytes: Vec<u8> = (0..15u16).flat_map(|i| (first + i).to_le_bytes()).collect();
        SpritePalette::new(&bytes).unwrap()
    }

    /// A sheet made by the importer from `pictures`, with a named palette
    /// `other`.
    fn sheet(pictures: &[(String, Picture)]) -> SpriteSheet {
        let (main, other) = (sprite_palette(0x1000), sprite_palette(0x0010));
        let files = sprite_sheet(pictures, &main, &[("other", &other)], "s.png").unwrap();
        let meta = clamber_sprite::Sheet::from_json_str(&files.json, "s.json").unwrap();
        SpriteSheet::new(meta, decode_png(&files.png).unwrap()).unwrap()
    }

    #[test]
    fn frame_hashes_are_the_spec_picture_hashes() {
        // Spec example (synthetic): colour 3 at (-1, -2), 5 at (0, -1),
        // here with a transparent border around it in the sheet.
        let p = picture(-2, -3, &[&[0, 0, 0, 0], &[0, 3, 0, 0], &[0, 0, 5, 0]]);
        let want = Sha256Hash(Sha256::digest(p.hash_input().unwrap()).into());
        let rect = Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 3,
        };
        let (hash, at) = frame_hash(&p.pixels, rect, (-2, -3)).unwrap();
        assert_eq!(hash, want);
        assert_eq!(
            p.hash_input().unwrap(),
            [0xff, 0xff, 0xfe, 0xff, 2, 0, 2, 0, 3, 0, 0, 5]
        );
        assert_eq!(
            at,
            Rect {
                x: 1,
                y: 1,
                w: 2,
                h: 2
            }
        );
        let empty = IndexImage::new(2, 2);
        assert_eq!(
            frame_hash(&empty, Rect { w: 2, h: 2, ..rect }, (0, 0)),
            None
        );
    }

    #[test]
    fn image_hashes_ignore_the_colour_of_transparent_pixels() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 1,
        };
        let mut a = filled(2, 1, [9, 9, 9, 255]);
        let mut b = a.clone();
        a.set(1, 0, [1, 2, 3, 0]);
        b.set(1, 0, [0, 0, 0, 0]);
        assert_eq!(image_hash(&a, rect), image_hash(&b, rect));
        assert_ne!(
            image_hash(&a, rect),
            image_hash(&filled(2, 1, [9, 9, 9, 255]), rect)
        );
        assert_eq!(image_hash(&filled(2, 1, [5, 5, 5, 0]), rect), None);
    }

    #[test]
    fn replaces_sheet_frames_at_a_higher_resolution() {
        let pictures = vec![
            ("big".to_string(), picture(-1, -2, &[&[1, 2], &[3, 0]])),
            ("kept".to_string(), picture(0, 0, &[&[4]])),
        ];
        let sheet = sheet(&pictures);
        let big = &sheet.sheet.frames[0];
        let Image::Indexed { indices, .. } = &sheet.image else {
            panic!("the importer's sheets are indexed");
        };
        let (hash, _) = frame_hash(indices, big.rect, big.offset).unwrap();
        let red = filled(4, 4, [255, 0, 0, 255]);
        let blue = filled(4, 4, [0, 0, 255, 255]);
        let art = scratch("sheet");
        art_pack(
            &art,
            "1-hd",
            &[
                (format!("{hash}.png"), &red),
                (format!("{hash}.other.png"), &blue),
            ],
        );
        let (replacements, problems) = Replacements::load(&art);
        assert_eq!(problems, Vec::<String>::new());
        let mut problems = Vec::new();
        let replaced = replace_sheet(&sheet, &replacements, &mut problems).unwrap();
        assert!(problems.is_empty(), "{problems:?}");

        // The same frames, the same size in the world.
        assert_eq!(replaced.sheet.scale, 2);
        for (old, new) in sheet.sheet.frames.iter().zip(&replaced.sheet.frames) {
            assert_eq!(new.rect, scale_rect(old.rect, 2));
            assert_eq!(new.offset, (old.offset.0 * 2, old.offset.1 * 2));
        }
        assert_eq!(replaced.sheet.animations, sheet.sheet.animations);
        // Drawn with the replacement, in each palette, and the frame
        // without one as it was, scaled up.
        let r = replaced.sheet.frames[0].rect;
        let image = replaced.image.to_rgba();
        assert_eq!(image.get(r.x + 3, r.y + 3), [255, 0, 0, 255]);
        let other = replaced.variants["other"].to_rgba();
        assert_eq!(other.get(r.x, r.y), [0, 0, 255, 255]);
        assert!(replaced.sheet.palettes.is_empty());
        assert_eq!(replaced.sheet.variants["other"], "other.png");
        let k = replaced.sheet.frames[1].rect;
        let original = sheet.image.to_rgba();
        let o = sheet.sheet.frames[1].rect;
        for (x, y) in [(0, 0), (1, 1)] {
            assert_eq!(image.get(k.x + x, k.y + y), original.get(o.x, o.y));
        }
        fs::remove_dir_all(&art).unwrap();
    }

    #[test]
    fn a_sheet_without_replacements_stays_as_it_is() {
        let sheet = sheet(&[("a".to_string(), picture(0, 0, &[&[1]]))]);
        let art = scratch("none");
        art_pack(
            &art,
            "hd",
            &[(format!("{}.png", "ab".repeat(32)), &filled(1, 1, [0; 4]))],
        );
        let (replacements, _) = Replacements::load(&art);
        assert!(replace_sheet(&sheet, &replacements, &mut Vec::new()).is_none());
        fs::remove_dir_all(&art).unwrap();
    }

    fn tileset() -> Tileset {
        Tileset {
            image: "tiles.png".into(),
            normals: None,
            tile_width: 2,
            tile_height: 2,
            margin: 0,
            spacing: 0,
        }
    }

    #[test]
    fn replaces_tiles_and_takes_the_first_pack_s() {
        let mut image = filled(4, 2, [10, 10, 10, 255]);
        for y in 0..2 {
            image.set(2, y, [20, 20, 20, 255]);
            image.set(3, y, [20, 20, 20, 255]);
        }
        let second = Rect {
            x: 2,
            y: 0,
            w: 2,
            h: 2,
        };
        let hash = image_hash(&image, second).unwrap();
        let art = scratch("tiles");
        let green = filled(6, 6, [0, 255, 0, 255]);
        let grey = filled(6, 6, [7, 7, 7, 255]);
        art_pack(&art, "a", &[(format!("{hash}.png"), &green)]);
        art_pack(&art, "b", &[(format!("{hash}.png"), &grey)]);
        let (replacements, _) = Replacements::load(&art);
        assert_eq!(replacements.names, ["a", "b"]);
        let (out, k) = replace_tileset(&image, &tileset(), &replacements, &mut Vec::new()).unwrap();
        assert_eq!((k, out.width, out.height), (3, 12, 6));
        assert_eq!(out.get(0, 0), [10, 10, 10, 255]);
        assert_eq!(out.get(5, 5), [10, 10, 10, 255]);
        assert_eq!(out.get(6, 0), [0, 255, 0, 255]);
        assert_eq!(out.get(11, 5), [0, 255, 0, 255]);
        fs::remove_dir_all(&art).unwrap();
    }

    #[test]
    fn replacements_of_the_wrong_size_are_problems() {
        let image = filled(2, 2, [10, 10, 10, 255]);
        let all = Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
        };
        let hash = image_hash(&image, all).unwrap();
        let art = scratch("size");
        art_pack(
            &art,
            "hd",
            &[(format!("{hash}.png"), &filled(4, 6, [1, 1, 1, 255]))],
        );
        let (replacements, _) = Replacements::load(&art);
        let mut problems = Vec::new();
        assert!(replace_image(&image, &replacements, &mut problems).is_none());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("4x6"), "{problems:?}");
        fs::remove_dir_all(&art).unwrap();
    }

    #[test]
    fn mixed_factors_share_one_scale() {
        assert_eq!(common_factor([2, 3].into_iter()), 6);
        assert_eq!(common_factor([4, 2].into_iter()), 4);
        assert_eq!(common_factor([16, 3].into_iter()), 16);
        assert_eq!(factor((2, 3), (4, 6)), Some(2));
        assert_eq!(factor((2, 3), (4, 3)), None);
        assert_eq!(factor((1, 1), (17, 17)), None);
    }

    #[test]
    fn folders_that_aren_t_packs_are_problems() {
        let art = scratch("broken");
        fs::create_dir_all(art.join("no-manifest")).unwrap();
        fs::create_dir_all(art.join(".hidden")).unwrap();
        fs::create_dir_all(art.join("future")).unwrap();
        fs::write(
            art.join("future").join(ART_MANIFEST),
            "format = 2\nname = \"x\"\n",
        )
        .unwrap();
        let (replacements, problems) = Replacements::load(&art);
        assert!(replacements.is_empty());
        assert_eq!(problems.len(), 2, "{problems:?}");
        fs::remove_dir_all(&art).unwrap();
    }

    #[test]
    fn dumps_a_pack_s_pictures_under_their_hashes() {
        let pack = scratch("dump-pack");
        let pictures = vec![("one".to_string(), picture(0, -1, &[&[1, 2]]))];
        let (main, other) = (sprite_palette(0x1000), sprite_palette(0x0010));
        let files = sprite_sheet(&pictures, &main, &[("other", &other)], "c.png").unwrap();
        fs::create_dir_all(pack.join(SPRITES)).unwrap();
        fs::write(pack.join(SPRITES).join("c.json"), &files.json).unwrap();
        fs::write(pack.join(SPRITES).join("c.png"), &files.png).unwrap();
        fs::create_dir_all(pack.join(LEVELS)).unwrap();
        let mut tiles = filled(4, 2, [10, 10, 10, 255]);
        tiles.set(3, 1, [0, 0, 0, 0]);
        fs::write(pack.join(LEVELS).join("tiles.png"), encode_png(&tiles)).unwrap();
        fs::write(
            pack.join(LEVELS).join("a.level"),
            "format = 1\nsize = [10, 10]\nstart = [1, 1]\n[[layers]]\nname = \"g\"\n\
             pixel_art = true\ntiles = { tileset = \"tiles.png\", tile_size = [2, 2], \
             columns = 1, data = [1] }\n",
        )
        .unwrap();

        let out = scratch("dump-out");
        assert_eq!(dump_pictures(&pack, &out).unwrap(), 3);
        let index = fs::read_to_string(out.join(DUMP_INDEX)).unwrap();
        let rows: Vec<Vec<&str>> = index
            .lines()
            .skip(1)
            .map(|l| l.split('\t').collect())
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][1..], ["frame", "2x1", "c one"]);
        assert_eq!(rows[1][1..], ["tile", "2x2", "a g 0"]);
        assert_eq!(rows[2][1..], ["tile", "2x2", "a g 1"]);
        // Each picture is a template for its replacement, in the sheet's
        // own colours.
        let frame = decode_png(&fs::read(out.join(format!("{}.png", rows[0][0]))).unwrap())
            .unwrap()
            .to_rgba();
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!(frame.get(0, 0), main.palette().0[1]);
        // A template painted over at twice the size replaces the tile.
        let hash = Sha256Hash::from_hex(rows[2][0]).unwrap();
        let art = scratch("dump-art");
        art_pack(
            &art,
            "hd",
            &[(format!("{hash}.png"), &filled(4, 4, [1, 2, 3, 255]))],
        );
        let (replacements, _) = Replacements::load(&art);
        let (out_tiles, k) =
            replace_tileset(&tiles, &tileset(), &replacements, &mut Vec::new()).unwrap();
        assert_eq!(k, 2);
        assert_eq!(out_tiles.get(7, 3), [1, 2, 3, 255]);
        for dir in [pack, out, art] {
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
