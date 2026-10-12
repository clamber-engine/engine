//! Sprites drawn from atlas pages.
//!
//! Sheets are packed into two sets of pages: full-colour frames, and
//! palette indices for indexed sheets. Indexed frames are coloured as they
//! are drawn from a table with one row per palette, so palette swaps and
//! hit flashes cost no extra image memory. This is the same split a GPU
//! path would use: index pages plus a palette texture.

use std::collections::HashMap;

use clamber_sprite::{
    Atlas, AtlasBuilder, AtlasError, Image, Pixels, Rect, Rgba, Sheet, SpriteSheet,
};

use crate::canvas::{Canvas, rgb};

/// Entries in each palette row; index images can't address more.
const PALETTE_WIDTH: usize = 256;

/// A sheet added to a [`SpriteBank`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SheetId(usize);

/// A row of the palette table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaletteId(usize);

/// Where a frame's pixels are: an entry in one of the two atlases.
#[derive(Clone, Copy, Debug)]
enum Source {
    Rgba(usize),
    Indexed(usize),
}

#[derive(Clone, Debug)]
struct Entry {
    sheet: Sheet,
    /// One per sheet frame.
    sources: Vec<Source>,
    /// The image's own palette, for indexed sheets.
    palette: Option<PaletteId>,
    /// The sheet's named palettes.
    named: Vec<(String, PaletteId)>,
}

/// Collects sheets to pack into a [`SpriteBank`].
#[derive(Default)]
pub struct SpriteBankBuilder {
    rgba: AtlasBuilder<Rgba>,
    indexed: AtlasBuilder<u8>,
    palettes: Vec<Rgba>,
    sheets: Vec<Entry>,
}

impl SpriteBankBuilder {
    pub fn new() -> SpriteBankBuilder {
        SpriteBankBuilder::default()
    }

    pub fn add(&mut self, sprite: &SpriteSheet) -> SheetId {
        // Frames that share pixels in the sheet share them in the atlas.
        let mut packed: HashMap<Rect, Source> = HashMap::new();
        let mut entry = Entry {
            sheet: sprite.sheet.clone(),
            sources: Vec::with_capacity(sprite.sheet.frames.len()),
            palette: None,
            named: Vec::new(),
        };
        for frame in &sprite.sheet.frames {
            let source = *packed
                .entry(frame.rect)
                .or_insert_with(|| match &sprite.image {
                    Image::Rgba(pixels) => Source::Rgba(self.rgba.add(pixels, frame.rect)),
                    Image::Indexed { indices, .. } => {
                        Source::Indexed(self.indexed.add(indices, frame.rect))
                    }
                });
            entry.sources.push(source);
        }
        if let Image::Indexed { palette, .. } = &sprite.image {
            entry.palette = Some(self.add_palette(&palette.0));
            for (name, palette) in &sprite.sheet.palettes {
                let id = self.add_palette(&palette.0);
                entry.named.push((name.clone(), id));
            }
        }
        self.sheets.push(entry);
        SheetId(self.sheets.len() - 1)
    }

    /// Adds a palette row; entries past the colours given are transparent.
    fn add_palette(&mut self, colors: &[Rgba]) -> PaletteId {
        let id = PaletteId(self.palettes.len() / PALETTE_WIDTH);
        let mut row = [[0; 4]; PALETTE_WIDTH];
        let n = colors.len().min(PALETTE_WIDTH);
        row[..n].copy_from_slice(&colors[..n]);
        self.palettes.extend_from_slice(&row);
        id
    }

    /// Packs everything added onto pages of `page_size` square.
    pub fn build(self, page_size: u32) -> Result<SpriteBank, AtlasError> {
        Ok(SpriteBank {
            rgba: self.rgba.build(page_size, 1)?,
            indexed: self.indexed.build(page_size, 1)?,
            palettes: self.palettes,
            sheets: self.sheets,
        })
    }
}

/// Sprite sheets packed for drawing.
#[derive(Clone, Debug)]
pub struct SpriteBank {
    rgba: Atlas<Rgba>,
    indexed: Atlas<u8>,
    /// The palette table, [`PALETTE_WIDTH`] colours per row.
    palettes: Vec<Rgba>,
    sheets: Vec<Entry>,
}

/// One sprite to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub sheet: SheetId,
    /// Index into the sheet's frames.
    pub frame: usize,
    /// Where the frame's origin goes, in canvas pixels.
    pub x: i32,
    pub y: i32,
    /// Mirror left to right about the origin, as for facing left.
    pub flip_x: bool,
    /// Mirror top to bottom about the origin.
    pub flip_y: bool,
    /// A palette for indexed sheets instead of the image's own.
    pub palette: Option<PaletteId>,
    /// Draw every visible pixel in this colour, keeping its alpha.
    pub flash: Option<Rgba>,
}

impl Sprite {
    /// A frame drawn as it is in the sheet, with its origin at `(x, y)`.
    pub fn new(sheet: SheetId, frame: usize, x: i32, y: i32) -> Sprite {
        Sprite {
            sheet,
            frame,
            x,
            y,
            flip_x: false,
            flip_y: false,
            palette: None,
            flash: None,
        }
    }
}

impl SpriteBank {
    pub fn sheet(&self, id: SheetId) -> &Sheet {
        &self.sheets[id.0].sheet
    }

    /// The sheet's palette called `name`, if it has one.
    pub fn palette(&self, id: SheetId, name: &str) -> Option<PaletteId> {
        let named = &self.sheets[id.0].named;
        named.iter().find(|(n, _)| n == name).map(|&(_, id)| id)
    }

    /// The atlas pages, full-colour and indexed, for inspection and for
    /// uploading to a GPU.
    pub fn pages(&self) -> (&[Pixels<Rgba>], &[Pixels<u8>]) {
        (&self.rgba.pages, &self.indexed.pages)
    }

    pub fn draw(&self, canvas: &mut Canvas, sprite: &Sprite) {
        let entry = &self.sheets[sprite.sheet.0];
        let frame = &entry.sheet.frames[sprite.frame];
        let flash = |c: Rgba| match sprite.flash {
            Some([r, g, b, _]) => [r, g, b, c[3]],
            None => c,
        };
        let (w, h) = (frame.rect.w as i32, frame.rect.h as i32);
        let left = match sprite.flip_x {
            false => sprite.x + frame.offset.0,
            true => sprite.x - frame.offset.0 - w,
        };
        let top = match sprite.flip_y {
            false => sprite.y + frame.offset.1,
            true => sprite.y - frame.offset.1 - h,
        };
        let flip = (sprite.flip_x, sprite.flip_y);
        match entry.sources[sprite.frame] {
            Source::Rgba(i) => {
                let at = self.rgba.entries[i];
                blit(
                    canvas,
                    &self.rgba.pages[at.page],
                    at.rect,
                    (left, top),
                    flip,
                    flash,
                );
            }
            Source::Indexed(i) => {
                let at = self.indexed.entries[i];
                let row = sprite.palette.or(entry.palette).map_or(0, |p| p.0);
                let colors = &self.palettes[row * PALETTE_WIDTH..][..PALETTE_WIDTH];
                blit(
                    canvas,
                    &self.indexed.pages[at.page],
                    at.rect,
                    (left, top),
                    flip,
                    |i| flash(colors[i as usize]),
                );
            }
        }
    }
}

/// Draws `rect` of `page` with its top left at `at`, mirrored as asked,
/// colouring each pixel with `color` and mixing by its alpha.
fn blit<P: Copy>(
    canvas: &mut Canvas,
    page: &Pixels<P>,
    rect: Rect,
    at: (i32, i32),
    flip: (bool, bool),
    color: impl Fn(P) -> Rgba,
) {
    let (w, h) = (rect.w as i32, rect.h as i32);
    let xs = (-at.0).max(0)..w.min(canvas.width as i32 - at.0);
    let ys = (-at.1).max(0)..h.min(canvas.height as i32 - at.1);
    for dy in ys {
        let sy = if flip.1 { h - 1 - dy } else { dy };
        let row = (rect.y as i32 + sy) as usize * page.width as usize + rect.x as usize;
        for dx in xs.clone() {
            let sx = if flip.0 { w - 1 - dx } else { dx };
            let [r, g, b, a] = color(page.data[row + sx as usize]);
            match a {
                0 => {}
                255 => canvas.plot(at.0 + dx, at.1 + dy, rgb(r, g, b)),
                a => canvas.blend(at.0 + dx, at.1 + dy, rgb(r, g, b), a),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use clamber_sprite::Palette;

    use super::*;
    use crate::canvas::Color;

    const SHEET: &str = r##"{
        "frames": [
            { "frame": { "x": 0, "y": 0, "w": 3, "h": 2 }, "ticks": 1,
              "spriteSourceSize": { "x": 1, "y": 0, "w": 3, "h": 2 } },
            { "frame": { "x": 3, "y": 0, "w": 1, "h": 1 }, "ticks": 1 },
            { "frame": { "x": 0, "y": 0, "w": 3, "h": 2 }, "ticks": 1 }
        ],
        "meta": {
            "image": "test.png",
            "clamber": {
                "origin": { "x": 2, "y": 2 },
                "palettes": { "alt": ["#000000", "#00ff00", "#0000ff", "#ffffff"] }
            }
        }
    }"##;

    const RED: Rgba = [255, 0, 0, 255];
    const BLUE: Rgba = [0, 0, 255, 255];
    const CLEAR: Rgba = [0, 0, 0, 0];
    const BG: Color = rgb(9, 9, 9);

    /// The test sheet as indices: frame 0 is `1 2 0 / 3 1 1`, where 0 is
    /// transparent.
    fn indexed() -> SpriteSheet {
        let sheet = Sheet::from_json_str(SHEET, "test").unwrap();
        let mut indices = Pixels::new(4, 2);
        indices.data = vec![1, 2, 0, 3, 3, 1, 1, 0];
        let palette = Palette(vec![CLEAR, RED, BLUE, [255, 255, 255, 128]]);
        SpriteSheet::new(sheet, Image::Indexed { indices, palette }).unwrap()
    }

    /// The same sheet in full colour.
    fn rgba() -> SpriteSheet {
        let indexed = indexed();
        SpriteSheet::new(indexed.sheet, Image::Rgba(indexed.image.to_rgba())).unwrap()
    }

    fn draw(bank: &SpriteBank, sprite: Sprite) -> Vec<Vec<Color>> {
        let mut canvas = Canvas::new(6, 4);
        canvas.clear(BG);
        bank.draw(&mut canvas, &sprite);
        canvas.pixels.chunks(6).map(<[_]>::to_vec).collect()
    }

    const R: Color = rgb(255, 0, 0);
    const B: Color = rgb(0, 0, 255);
    const G: Color = rgb(0, 255, 0);
    /// Half white over the background.
    const W: Color = rgb(132, 132, 132);

    #[test]
    fn draws_at_the_origin_plus_offset() {
        for sheet in [indexed(), rgba()] {
            let (bank, id) = bank(&sheet);
            // Origin (2, 2), trimmed to start one pixel in: offset (-1, -2).
            #[rustfmt::skip]
            assert_eq!(draw(&bank, Sprite::new(id, 0, 3, 3)), [
                [BG, BG, BG, BG, BG, BG],
                [BG, BG, R, B, BG, BG],
                [BG, BG, W, R, R, BG],
                [BG, BG, BG, BG, BG, BG],
            ]);
        }
    }

    fn bank(sheet: &SpriteSheet) -> (SpriteBank, SheetId) {
        let mut builder = SpriteBankBuilder::new();
        let id = builder.add(sheet);
        (builder.build(16).unwrap(), id)
    }

    #[test]
    fn mirrors_about_the_origin() {
        let (bank, id) = bank(&indexed());
        let sprite = Sprite {
            flip_x: true,
            ..Sprite::new(id, 0, 3, 3)
        };
        #[rustfmt::skip]
        assert_eq!(draw(&bank, sprite), [
            [BG, BG, BG, BG, BG, BG],
            [BG, BG, B, R, BG, BG],
            [BG, R, R, W, BG, BG],
            [BG, BG, BG, BG, BG, BG],
        ]);
        let sprite = Sprite {
            flip_y: true,
            ..Sprite::new(id, 0, 3, 1)
        };
        #[rustfmt::skip]
        assert_eq!(draw(&bank, sprite), [
            [BG, BG, BG, BG, BG, BG],
            [BG, BG, W, R, R, BG],
            [BG, BG, R, B, BG, BG],
            [BG, BG, BG, BG, BG, BG],
        ]);
    }

    #[test]
    fn swaps_palettes_and_flashes() {
        let (bank, id) = bank(&indexed());
        let alt = bank.palette(id, "alt").unwrap();
        assert_eq!(bank.palette(id, "missing"), None);
        let sprite = Sprite {
            palette: Some(alt),
            ..Sprite::new(id, 0, 3, 3)
        };
        let white = rgb(255, 255, 255);
        // Index 0 is opaque black in `alt`, so the whole frame shows.
        #[rustfmt::skip]
        assert_eq!(draw(&bank, sprite)[1..3], [
            [BG, BG, G, B, 0, BG],
            [BG, BG, white, G, G, BG],
        ]);
        let sprite = Sprite {
            flash: Some([0, 255, 0, 255]),
            ..Sprite::new(id, 0, 3, 3)
        };
        let half_green = rgb(4, 132, 4);
        #[rustfmt::skip]
        assert_eq!(draw(&bank, sprite)[1..3], [
            [BG, BG, G, G, BG, BG],
            [BG, BG, half_green, G, G, BG],
        ]);
    }

    #[test]
    fn clips_to_the_canvas() {
        let (bank, id) = bank(&rgba());
        #[rustfmt::skip]
        assert_eq!(draw(&bank, Sprite::new(id, 0, 0, 1)), [
            [R, R, BG, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
        ]);
        let far = draw(&bank, Sprite::new(id, 0, 100, -100));
        assert!(far.iter().flatten().all(|&c| c == BG));
    }

    #[test]
    fn packs_shared_frames_once() {
        let (bank, id) = bank(&indexed());
        let (rgba, indexed) = bank.pages();
        assert!(rgba.is_empty());
        assert_eq!(indexed.len(), 1);
        // Frames 0 and 2 share pixels, so the page holds two images.
        let used = indexed[0].data.iter().filter(|&&i| i != 0).count();
        assert_eq!(used, 5 + 1);
        assert_eq!(
            draw(&bank, Sprite::new(id, 2, 0, 0)),
            draw(&bank, Sprite::new(id, 0, -1, 0))
        );
    }
}
