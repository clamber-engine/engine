//! Sprites drawn from their sheets' textures.
//!
//! Each sheet is one [`Texture`], filtered smoothly unless it's pixel art,
//! and drawn at its own number of texels per world unit, so sheets made at
//! any resolution are the same size in the world. An indexed sheet gets a
//! texture for each of its palettes, coloured when it's added, and any
//! sheet one for each of its variant images, so palette swaps cost nothing
//! to draw; hit flashes recolour as they're drawn. A sheet's normal map,
//! if it has one, is a texture too, shared by its palettes and variants,
//! and lights the sheet whatever colours it's drawn in.

use std::sync::Arc;

use clamber_sprite::{Image, Rgba, Sheet, SpriteSheet};

use crate::surface::{Flip, Quad, Surface, Texture};

/// A sheet added to a [`SpriteBank`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SheetId(usize);

/// One of a sheet's named palettes or variant images.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaletteId(usize);

#[derive(Clone, Debug)]
struct Entry {
    sheet: Sheet,
    /// The sheet's image, then one per named palette or variant.
    textures: Vec<Arc<Texture>>,
    /// The sheet's named palettes and variants, by their index in
    /// `textures`, in name order.
    named: Vec<(String, PaletteId)>,
    normals: Option<Arc<Texture>>,
}

/// Collects sheets for a [`SpriteBank`].
#[derive(Default)]
pub struct SpriteBankBuilder {
    sheets: Vec<Entry>,
}

impl SpriteBankBuilder {
    pub fn new() -> SpriteBankBuilder {
        SpriteBankBuilder::default()
    }

    pub fn add(&mut self, sprite: &SpriteSheet) -> SheetId {
        let pixel_art = sprite.sheet.pixel_art;
        let mut entry = Entry {
            sheet: sprite.sheet.clone(),
            textures: vec![Texture::new(sprite.image.to_rgba(), pixel_art)],
            named: Vec::new(),
            normals: sprite
                .normals
                .as_ref()
                .map(|n| Texture::new(n.to_rgba(), pixel_art)),
        };
        if let Image::Indexed { indices, .. } = &sprite.image {
            for (name, palette) in &sprite.sheet.palettes {
                entry
                    .named
                    .push((name.clone(), PaletteId(entry.textures.len())));
                entry
                    .textures
                    .push(Texture::new(palette.apply(indices), pixel_art));
            }
        }
        for (name, image) in &sprite.variants {
            entry
                .named
                .push((name.clone(), PaletteId(entry.textures.len())));
            entry
                .textures
                .push(Texture::new(image.to_rgba(), pixel_art));
        }
        entry.named.sort_by(|a, b| a.0.cmp(&b.0));
        self.sheets.push(entry);
        SheetId(self.sheets.len() - 1)
    }

    pub fn build(self) -> SpriteBank {
        SpriteBank {
            sheets: self.sheets,
        }
    }
}

/// Sprite sheets ready to draw.
#[derive(Clone, Debug, Default)]
pub struct SpriteBank {
    sheets: Vec<Entry>,
}

/// One sprite to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    pub sheet: SheetId,
    /// Index into the sheet's frames.
    pub frame: usize,
    /// Where the frame's origin goes, in view units.
    pub x: f32,
    pub y: f32,
    /// Mirror left to right about the origin, as for facing left.
    pub flip_x: bool,
    /// Mirror top to bottom about the origin.
    pub flip_y: bool,
    /// One of the sheet's named palettes or variants instead of its own.
    pub palette: Option<PaletteId>,
    /// Draw every visible pixel in this colour, keeping its alpha.
    pub flash: Option<Rgba>,
    /// How opaque it's drawn, from 0 (not at all) to 255 (as it is).
    pub opacity: u8,
}

impl Sprite {
    /// A frame drawn as it is in the sheet, with its origin at `(x, y)`.
    pub fn new(sheet: SheetId, frame: usize, x: f32, y: f32) -> Sprite {
        Sprite {
            sheet,
            frame,
            x,
            y,
            flip_x: false,
            flip_y: false,
            palette: None,
            flash: None,
            opacity: 255,
        }
    }
}

impl SpriteBank {
    pub fn sheet(&self, id: SheetId) -> &Sheet {
        &self.sheets[id.0].sheet
    }

    /// The sheet's palette or variant called `name`, if it has one.
    pub fn palette(&self, id: SheetId, name: &str) -> Option<PaletteId> {
        let named = &self.sheets[id.0].named;
        named.iter().find(|(n, _)| n == name).map(|&(_, id)| id)
    }

    /// The sheet's palettes and variants, in name order.
    pub fn palettes(&self, id: SheetId) -> impl Iterator<Item = PaletteId> + '_ {
        self.sheets[id.0].named.iter().map(|&(_, id)| id)
    }

    pub fn draw(&self, surface: &mut dyn Surface, sprite: &Sprite) {
        let entry = &self.sheets[sprite.sheet.0];
        let frame = &entry.sheet.frames[sprite.frame];
        let texture = sprite
            .palette
            .and_then(|p| entry.textures.get(p.0))
            .unwrap_or(&entry.textures[0]);
        let scale = entry.sheet.scale as f32;
        let (w, h) = (frame.rect.w as f32 / scale, frame.rect.h as f32 / scale);
        let offset = (frame.offset.0 as f32 / scale, frame.offset.1 as f32 / scale);
        let left = match sprite.flip_x {
            false => sprite.x + offset.0,
            true => sprite.x - offset.0 - w,
        };
        let top = match sprite.flip_y {
            false => sprite.y + offset.1,
            true => sprite.y - offset.1 - h,
        };
        surface.quad(&Quad {
            texture: texture.clone(),
            src: frame.rect,
            dst: [left, top, left + w, top + h],
            flip: Flip {
                x: sprite.flip_x,
                y: sprite.flip_y,
                diagonal: false,
            },
            flash: sprite.flash,
            opacity: sprite.opacity,
            normals: entry.normals.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use clamber_sprite::{Palette, Pixels};

    use super::*;
    use crate::canvas::{Canvas, Color, rgb};

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
                "pixelArt": true,
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
            assert_eq!(draw(&bank, Sprite::new(id, 0, 3.0, 3.0)), [
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
        (builder.build(), id)
    }

    #[test]
    fn mirrors_about_the_origin() {
        let (bank, id) = bank(&indexed());
        let sprite = Sprite {
            flip_x: true,
            ..Sprite::new(id, 0, 3.0, 3.0)
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
            ..Sprite::new(id, 0, 3.0, 1.0)
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
            ..Sprite::new(id, 0, 3.0, 3.0)
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
            ..Sprite::new(id, 0, 3.0, 3.0)
        };
        let half_green = rgb(4, 132, 4);
        #[rustfmt::skip]
        assert_eq!(draw(&bank, sprite)[1..3], [
            [BG, BG, G, G, BG, BG],
            [BG, BG, half_green, G, G, BG],
        ]);
    }

    #[test]
    fn draws_partly_see_through_or_not_at_all() {
        let (bank, id) = bank(&indexed());
        let faded = |opacity| {
            let rows = draw(
                &bank,
                Sprite {
                    opacity,
                    flash: Some([0, 255, 0, 255]),
                    ..Sprite::new(id, 0, 3.0, 3.0)
                },
            );
            rows[1][3]
        };
        assert_eq!(faded(255), G);
        assert_eq!(faded(0), BG);
        let half = faded(128);
        assert!(half != G && half != BG, "between: {half:06x}");
    }

    #[test]
    fn swaps_in_variant_images() {
        let json = SHEET.replace(
            r##""palettes": { "alt": ["#000000", "#00ff00", "#0000ff", "#ffffff"] }"##,
            r#""variants": { "blue": "blue.png" }"#,
        );
        let sheet = Sheet::from_json_str(&json, "test").unwrap();
        let image = rgba().image;
        let mut blue = Pixels::new(4, 2);
        blue.data = vec![BLUE; 8];
        let variants = [("blue".to_string(), Image::Rgba(blue))].into();
        let sheet = SpriteSheet::new(sheet, image)
            .and_then(|s| s.with_variants(variants))
            .unwrap();
        let (bank, id) = bank(&sheet);
        let sprite = Sprite {
            palette: bank.palette(id, "blue"),
            ..Sprite::new(id, 0, 3.0, 3.0)
        };
        assert!(sprite.palette.is_some());
        assert_eq!(
            bank.palettes(id).collect::<Vec<_>>(),
            [sprite.palette.unwrap()]
        );
        #[rustfmt::skip]
        assert_eq!(draw(&bank, sprite)[1..3], [
            [BG, BG, B, B, B, BG],
            [BG, BG, B, B, B, BG],
        ]);
    }

    #[test]
    fn clips_to_the_canvas() {
        let (bank, id) = bank(&rgba());
        #[rustfmt::skip]
        assert_eq!(draw(&bank, Sprite::new(id, 0, 0.0, 1.0)), [
            [R, R, BG, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
        ]);
        let far = draw(&bank, Sprite::new(id, 0, 100.0, -100.0));
        assert!(far.iter().flatten().all(|&c| c == BG));
    }

    #[test]
    fn draws_sheets_at_their_scale_the_same_size_in_the_world() {
        // Four texels per unit: the frame is 2 by 1 units, origin at the
        // bottom left of its 8 by 4 texels.
        let json = r#"{
            "frames": [{ "frame": { "x": 0, "y": 0, "w": 8, "h": 4 }, "ticks": 1 }],
            "meta": { "image": "hd.png", "clamber": { "origin": { "x": 0, "y": 4 }, "scale": 4 } }
        }"#;
        let sheet = Sheet::from_json_str(json, "test").unwrap();
        let mut image = Pixels::new(8, 4);
        image.data = vec![RED; 32];
        let (bank, id) = bank(&SpriteSheet::new(sheet, Image::Rgba(image)).unwrap());
        #[rustfmt::skip]
        assert_eq!(draw(&bank, Sprite::new(id, 0, 1.0, 3.0)), [
            [BG, BG, BG, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
            [BG, R, R, BG, BG, BG],
            [BG, BG, BG, BG, BG, BG],
        ]);
        // At three pixels a unit it covers 6 by 3 pixels.
        let mut canvas = Canvas::scaled((6, 4), 3);
        canvas.clear(BG);
        bank.draw(&mut canvas, &Sprite::new(id, 0, 1.0, 3.0));
        let red = canvas.pixels.iter().filter(|&&c| c == R).count();
        assert_eq!(red, 6 * 3);
    }
}
