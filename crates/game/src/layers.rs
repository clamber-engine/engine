//! Level layers: images and tiles, scrolled with parallax.
//!
//! A layer's top left is drawn at its offset less the view's top left
//! scaled by its parallax, so a parallax of 1 moves with the world and 0
//! stays put on screen. Positions are whole pixels, rounded down.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use clamber_level::{Depth, Layer, LayerContent, Level, Tile, TileLayer, Tileset};
use clamber_sprite::{Rect, RgbaImage, decode_png};

use crate::camera::point;
use crate::canvas::Canvas;
use crate::sprites::{Flip, blit};

/// The images a level's layers draw, by the path the level gives them.
#[derive(Clone, Debug, Default)]
pub struct LayerArt {
    images: BTreeMap<String, Arc<RgbaImage>>,
}

impl LayerArt {
    /// Reads every image `level`'s layers use, relative to `dir`, the
    /// level file's folder, and checks the tiles are in their tilesets.
    pub fn load(level: &Level, dir: &Path) -> Result<LayerArt, String> {
        let mut art = LayerArt::default();
        for layer in &level.layers {
            let path = image_path(layer);
            if art.images.contains_key(path) {
                continue;
            }
            let file = dir.join(path);
            let err = |e: String| format!("{}: {e}", file.display());
            let bytes = std::fs::read(&file).map_err(|e| err(e.to_string()))?;
            let image = decode_png(&bytes).map_err(|e| err(e.0))?;
            art.insert(path, image.to_rgba());
        }
        art.check(level)?;
        Ok(art)
    }

    /// Adds an image under the path layers name it by.
    pub fn insert(&mut self, path: &str, image: RgbaImage) {
        self.images.insert(path.to_string(), Arc::new(image));
    }

    fn check(&self, level: &Level) -> Result<(), String> {
        for layer in &level.layers {
            let LayerContent::Tiles(tiles) = &layer.content else {
                continue;
            };
            let Some(image) = self.images.get(&tiles.tileset.image) else {
                continue;
            };
            let count = tile_count(&tiles.tileset, image);
            let last = tiles.tiles.iter().filter_map(|t| t.index()).max();
            if let Some(index) = last.filter(|&i| i >= count) {
                return Err(format!(
                    "layer `{}`: tile {} isn't in the {count} tiles of `{}`",
                    layer.name,
                    index + 1,
                    tiles.tileset.image
                ));
            }
        }
        Ok(())
    }

    /// Draws `level`'s layers at `depth` in order, for a view whose top
    /// left is at `origin` in world pixels. Layers whose image isn't here
    /// are skipped.
    pub fn draw(&self, canvas: &mut Canvas, level: &Level, depth: Depth, origin: (f64, f64)) {
        for layer in level.layers.iter().filter(|l| l.depth == depth) {
            let Some(image) = self.images.get(image_path(layer)) else {
                continue;
            };
            let (offset, parallax) = (point(layer.offset), point(layer.parallax));
            let at = (
                (offset.0 - origin.0 * parallax.0).floor() as i32,
                (offset.1 - origin.1 * parallax.1).floor() as i32,
            );
            match &layer.content {
                LayerContent::Image {
                    repeat_x, repeat_y, ..
                } => draw_image(canvas, image, at, (*repeat_x, *repeat_y)),
                LayerContent::Tiles(tiles) => draw_tiles(canvas, image, tiles, at),
            }
        }
    }
}

fn image_path(layer: &Layer) -> &str {
    match &layer.content {
        LayerContent::Image { path, .. } => path,
        LayerContent::Tiles(tiles) => &tiles.tileset.image,
    }
}

fn draw_image(canvas: &mut Canvas, image: &RgbaImage, at: (i32, i32), repeat: (bool, bool)) {
    let (w, h) = (image.width as i32, image.height as i32);
    if w == 0 || h == 0 {
        return;
    }
    // Where copies start along an axis: once, or tiling the whole view.
    let starts = |at: i32, size: i32, view: u32, repeat: bool| {
        let (first, last) = match repeat {
            true => (at.rem_euclid(size) - size, view as i32 - 1),
            false => (at, at),
        };
        (first..=last).step_by(size as usize)
    };
    let rect = Rect {
        x: 0,
        y: 0,
        w: image.width,
        h: image.height,
    };
    for y in starts(at.1, h, canvas.height, repeat.1) {
        for x in starts(at.0, w, canvas.width, repeat.0) {
            blit(canvas, image, rect, (x, y), Flip::default(), |c| c);
        }
    }
}

fn draw_tiles(canvas: &mut Canvas, image: &RgbaImage, tiles: &TileLayer, at: (i32, i32)) {
    let set = &tiles.tileset;
    let (tw, th) = (set.tile_width as i32, set.tile_height as i32);
    let rows = (tiles.tiles.len() / tiles.columns as usize) as i32;
    // The cells on screen along an axis, and one more each side for tiles
    // that are wider than tall swapped onto their side.
    let visible = |at: i32, size: i32, count: i32, view: u32| {
        let first = ((-at).div_euclid(size) - 1).max(0);
        let end = ((view as i32 - at).div_euclid(size) + 2).min(count);
        first..end
    };
    for row in visible(at.1, th, rows, canvas.height) {
        for column in visible(at.0, tw, tiles.columns as i32, canvas.width) {
            let tile = tiles.tiles[(row * tiles.columns as i32 + column) as usize];
            let Some(rect) = tile.index().and_then(|i| tile_rect(set, image, i)) else {
                continue;
            };
            let flip = Flip {
                x: tile.0 & Tile::FLIP_X != 0,
                y: tile.0 & Tile::FLIP_Y != 0,
                diagonal: tile.0 & Tile::FLIP_DIAGONAL != 0,
            };
            let to = (at.0 + column * tw, at.1 + row * th);
            blit(canvas, image, rect, to, flip, |c| c);
        }
    }
}

/// Tiles across and down the tileset image.
fn tile_grid(set: &Tileset, image: &RgbaImage) -> (u32, u32) {
    let fit = |size: u32, tile: u32| {
        (size.saturating_sub(2 * set.margin) + set.spacing) / (tile + set.spacing)
    };
    (
        fit(image.width, set.tile_width),
        fit(image.height, set.tile_height),
    )
}

fn tile_count(set: &Tileset, image: &RgbaImage) -> u32 {
    let (across, down) = tile_grid(set, image);
    across * down
}

/// Where tile `index` is in the tileset image, if it's there.
fn tile_rect(set: &Tileset, image: &RgbaImage, index: u32) -> Option<Rect> {
    let (across, _) = tile_grid(set, image);
    if index >= tile_count(set, image) {
        return None;
    }
    Some(Rect {
        x: set.margin + index % across * (set.tile_width + set.spacing),
        y: set.margin + index / across * (set.tile_height + set.spacing),
        w: set.tile_width,
        h: set.tile_height,
    })
}

#[cfg(test)]
mod tests {
    use clamber_sprite::encode_png;

    use super::*;
    use crate::canvas::{Color, rgb};

    const BG: Color = rgb(9, 9, 9);

    fn level(layers: &str) -> Level {
        let src = format!("format = 1\nsize = [1000, 1000]\nstart = [10, 10]\n{layers}");
        Level::from_toml_str(&src, "test").unwrap()
    }

    /// An image whose every pixel says where it is: red is x, green is y.
    fn coords(w: u32, h: u32) -> RgbaImage {
        let mut image = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                image.set(x, y, [x as u8, y as u8, 0, 255]);
            }
        }
        image
    }

    fn at(x: i32, y: i32) -> Color {
        rgb(x as u8, y as u8, 0)
    }

    fn draw(art: &LayerArt, level: &Level, depth: Depth, origin: (f64, f64)) -> Canvas {
        let mut canvas = Canvas::new(8, 6);
        canvas.clear(BG);
        art.draw(&mut canvas, level, depth, origin);
        canvas
    }

    #[test]
    fn scrolls_by_parallax_from_the_offset() {
        let level = level(
            "[[layers]]\nname = \"hills\"\nimage = \"hills.png\"\n\
             parallax = [0.5, 0.25]\noffset = [10, 4]\n",
        );
        let mut art = LayerArt::default();
        art.insert("hills.png", coords(64, 32));
        // Top left at (10 - 100 * 0.5, 4 - 40 * 0.25) = (-40, -6).
        let canvas = draw(&art, &level, Depth::Back, (100.0, 40.0));
        assert_eq!(canvas.get(0, 0), Some(at(40, 6)));
        assert_eq!(canvas.get(7, 5), Some(at(47, 11)));
        // Fractions round down, and a layer stays put at parallax 0.
        let canvas = draw(&art, &level, Depth::Back, (101.0, 42.0));
        assert_eq!(canvas.get(0, 0), Some(at(41, 7)));
        let still = level.layers[0].clone();
        let level = Level {
            layers: vec![Layer {
                parallax: Default::default(),
                offset: clamber_level::Vec2::ints(2, 1),
                ..still
            }],
            ..level
        };
        let canvas = draw(&art, &level, Depth::Back, (500.0, 500.0));
        assert_eq!(canvas.get(1, 1), Some(BG));
        assert_eq!(canvas.get(2, 1), Some(at(0, 0)));
        assert_eq!(canvas.get(7, 5), Some(at(5, 4)));
        assert!(
            draw(&art, &level, Depth::Front, (0.0, 0.0))
                .pixels
                .iter()
                .all(|&c| c == BG)
        );
    }

    #[test]
    fn repeats_along_the_axes_asked() {
        let level = level(
            "[[layers]]\nname = \"sky\"\nimage = \"sky.png\"\nrepeat = [true, false]\noffset = [0, 2]\n",
        );
        let mut art = LayerArt::default();
        art.insert("sky.png", coords(3, 2));
        let canvas = draw(&art, &level, Depth::Back, (-5.0, 1.0));
        let row: Vec<_> = (0..8).map(|x| canvas.get(x, 1).unwrap()).collect();
        // The image's x = 0 is at screen x = 5, and repeats every 3.
        let expected: Vec<_> = (0..8).map(|x| at((x + 1) % 3, 0)).collect();
        assert_eq!(row, expected);
        assert_eq!(canvas.get(0, 2), Some(at(1, 1)));
        assert_eq!(canvas.get(0, 0), Some(BG));
        assert_eq!(canvas.get(0, 3), Some(BG));
    }

    /// Two 2x2 tiles with a margin of 1 and spacing of 1: tile 1 at
    /// (1, 1) and tile 2 at (4, 1).
    fn tileset() -> RgbaImage {
        coords(7, 4)
    }

    const TILES: &str = "[[layers]]\nname = \"ground\"\ndepth = \"front\"\noffset = [1, 0]\n\
        tiles = { tileset = \"ground.png\", tile_size = [2, 2], margin = 1, spacing = 1, columns = 3, data = DATA }\n";

    fn tiles(data: &[u32]) -> Level {
        level(&TILES.replace("DATA", &format!("{data:?}")))
    }

    #[test]
    fn draws_tiles_from_the_tileset() {
        let mut art = LayerArt::default();
        art.insert("ground.png", tileset());
        let level = tiles(&[1, 0, 2, 2, 1, 0]);
        let canvas = draw(&art, &level, Depth::Front, (0.0, 0.0));
        let grid: Vec<Vec<_>> = (0..4)
            .map(|y| (0..7).map(|x| canvas.get(x, y).unwrap()).collect())
            .collect();
        #[rustfmt::skip]
        assert_eq!(grid, [
            [BG, at(1, 1), at(2, 1), BG, BG, at(4, 1), at(5, 1)],
            [BG, at(1, 2), at(2, 2), BG, BG, at(4, 2), at(5, 2)],
            [BG, at(4, 1), at(5, 1), at(1, 1), at(2, 1), BG, BG],
            [BG, at(4, 2), at(5, 2), at(1, 2), at(2, 2), BG, BG],
        ]);
    }

    #[test]
    fn flips_tiles_mirroring_then_swapping() {
        let mut art = LayerArt::default();
        art.insert("ground.png", tileset());
        let tile = |flags: u32| {
            let canvas = draw(&art, &tiles(&[flags | 1, 0, 0]), Depth::Front, (0.0, 0.0));
            [
                [canvas.get(1, 0), canvas.get(2, 0)],
                [canvas.get(1, 1), canvas.get(2, 1)],
            ]
            .map(|row| row.map(Option::unwrap))
        };
        let (a, b, c, d) = (at(1, 1), at(2, 1), at(1, 2), at(2, 2));
        assert_eq!(tile(0), [[a, b], [c, d]]);
        assert_eq!(tile(Tile::FLIP_X), [[b, a], [d, c]]);
        assert_eq!(tile(Tile::FLIP_Y), [[c, d], [a, b]]);
        assert_eq!(tile(Tile::FLIP_DIAGONAL), [[a, c], [b, d]]);
        // Mirrored left to right, then swapped: a quarter turn anticlockwise.
        assert_eq!(tile(Tile::FLIP_X | Tile::FLIP_DIAGONAL), [[b, d], [a, c]]);
    }

    #[test]
    fn skips_layers_without_images() {
        let canvas = draw(
            &LayerArt::default(),
            &tiles(&[1, 0, 0]),
            Depth::Front,
            (0.0, 0.0),
        );
        assert!(canvas.pixels.iter().all(|&c| c == BG));
    }

    #[test]
    fn loads_images_beside_the_level() {
        let dir = std::env::temp_dir().join(format!("clamber-layers-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("art")).unwrap();
        let png = encode_png(&tileset());
        std::fs::write(dir.join("art/ground.png"), png).unwrap();
        let level = |data: &[u32]| {
            let src = TILES
                .replace("DATA", &format!("{data:?}"))
                .replace("ground.png", "art/ground.png");
            self::level(&src)
        };

        let art = LayerArt::load(&level(&[2, 0, 0]), &dir).unwrap();
        let canvas = draw(&art, &level(&[2, 0, 0]), Depth::Front, (0.0, 0.0));
        assert_eq!(canvas.get(1, 0), Some(at(4, 1)));

        let err = LayerArt::load(&level(&[3, 0, 0]), &dir).unwrap_err();
        assert_eq!(
            err,
            "layer `ground`: tile 3 isn't in the 2 tiles of `art/ground.png`"
        );
        std::fs::remove_dir_all(&dir).unwrap();
        let err = LayerArt::load(&level(&[1, 0, 0]), &dir).unwrap_err();
        assert!(err.contains("ground.png: "), "{err}");
    }
}
