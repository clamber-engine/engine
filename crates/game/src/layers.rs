//! Level layers: images and tiles, scrolled with parallax.
//!
//! A layer's top left is drawn at its offset less the view's top left
//! scaled by its parallax, so a parallax of 1 moves with the world and 0
//! stays put on screen. Its image covers one world unit per `scale`
//! texels, filtered smoothly unless the layer is pixel art, and is lit by
//! the level's lights if it has a normal map. A layer that dims is drawn
//! in the colours of the dimming's step (`crate::dim`).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use clamber_level::{Depth, Layer, LayerContent, Level, Tile, TileLayer, Tileset};
use clamber_sprite::{Rect, RgbaImage, decode_png};

use crate::camera::point;
use crate::dim::dim_image;
use crate::surface::{Flip, Quad, Surface, Texture};

/// The images a level's layers draw, by the path the level gives them and
/// whether they're pixel art.
#[derive(Clone, Debug, Default)]
pub struct LayerArt {
    textures: BTreeMap<(String, bool), Arc<Texture>>,
    /// Images in the dimming's colours, made as they're first drawn, by
    /// path, pixel art or not, step and steps.
    dimmed: Arc<Mutex<BTreeMap<DimKey, Arc<Texture>>>>,
}

type DimKey = (String, bool, (u32, u32));

impl LayerArt {
    /// Reads every image `level`'s layers use, relative to `dir`, the
    /// level file's folder, and checks the tiles are in their tilesets.
    pub fn load(level: &Level, dir: &Path) -> Result<LayerArt, String> {
        let mut art = LayerArt::default();
        let mut images: BTreeMap<&str, RgbaImage> = BTreeMap::new();
        for layer in &level.layers {
            for path in [Some(image_path(layer)), normals_path(layer)]
                .into_iter()
                .flatten()
            {
                if art.get(path, layer.pixel_art).is_some() {
                    continue;
                }
                if !images.contains_key(path) {
                    let file = dir.join(path);
                    let err = |e: String| format!("{}: {e}", file.display());
                    let bytes = std::fs::read(&file).map_err(|e| err(e.to_string()))?;
                    let image = decode_png(&bytes).map_err(|e| err(e.0))?;
                    images.insert(path, image.to_rgba());
                }
                art.insert(path, images[path].clone(), layer.pixel_art);
            }
        }
        art.check(level)?;
        Ok(art)
    }

    /// Adds an image under the path layers name it by, for layers that
    /// are pixel art or not.
    pub fn insert(&mut self, path: &str, image: RgbaImage, pixel_art: bool) {
        let texture = Texture::new(image, pixel_art);
        self.textures.insert((path.to_string(), pixel_art), texture);
    }

    fn get(&self, path: &str, pixel_art: bool) -> Option<&Arc<Texture>> {
        self.textures.get(&(path.to_string(), pixel_art))
    }

    fn check(&self, level: &Level) -> Result<(), String> {
        for layer in &level.layers {
            let image = self.get(image_path(layer), layer.pixel_art);
            let normals = normals_path(layer).and_then(|p| self.get(p, layer.pixel_art));
            if let (Some(image), Some(normals)) = (image, normals) {
                let size = |t: &Texture| (t.width(), t.height());
                if size(image) != size(normals) {
                    return Err(format!(
                        "layer `{}`: the normal map is {}x{}, but its image is {}x{}",
                        layer.name,
                        normals.width(),
                        normals.height(),
                        image.width(),
                        image.height()
                    ));
                }
            }
            let LayerContent::Tiles(tiles) = &layer.content else {
                continue;
            };
            let Some(texture) = self.get(&tiles.tileset.image, layer.pixel_art) else {
                continue;
            };
            let count = tile_count(&tiles.tileset, &texture.levels[0]);
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

    /// The image `path` in the colours of step `dim.0` of `dim.1` of the
    /// dimming.
    fn dimmed(&self, path: &str, pixel_art: bool, dim: (u32, u32)) -> Option<Arc<Texture>> {
        let texture = self.get(path, pixel_art)?;
        if dim.0 == 0 {
            return Some(texture.clone());
        }
        let mut dimmed = self.dimmed.lock().unwrap_or_else(|e| e.into_inner());
        let key = (path.to_string(), pixel_art, dim);
        let made = dimmed.entry(key).or_insert_with(|| {
            Texture::new(dim_image(&texture.levels[0], dim.0, dim.1), pixel_art)
        });
        Some(made.clone())
    }

    /// Draws `level`'s layers at `depth` in order, for a view whose top
    /// left is at `origin` in world units, with the dimming at step
    /// `dim.0` of `dim.1`. Layers whose image isn't here are skipped.
    pub fn draw(
        &self,
        surface: &mut dyn Surface,
        level: &Level,
        depth: Depth,
        origin: (f64, f64),
        dim: (u32, u32),
    ) {
        self.draw_some(surface, level, origin, dim, |l| l.depth == depth);
    }

    /// Draws the layers `keep` says to, in order, for a view whose top
    /// left is at `origin`, with the dimming at step `dim.0` of `dim.1`.
    pub fn draw_some(
        &self,
        surface: &mut dyn Surface,
        level: &Level,
        origin: (f64, f64),
        dim: (u32, u32),
        keep: impl Fn(&Layer) -> bool,
    ) {
        for layer in level.layers.iter().filter(|l| keep(l)) {
            let dim = if layer.dim { dim } else { (0, 0) };
            let Some(texture) = self.dimmed(image_path(layer), layer.pixel_art, dim) else {
                continue;
            };
            let normals = normals_path(layer).and_then(|p| self.get(p, layer.pixel_art));
            let art = Art {
                texture: &texture,
                normals,
            };
            let (offset, parallax) = (point(layer.offset), point(layer.parallax));
            let at = (
                (offset.0 - origin.0 * parallax.0) as f32,
                (offset.1 - origin.1 * parallax.1) as f32,
            );
            let scale = layer.scale as f32;
            match &layer.content {
                LayerContent::Image {
                    repeat_x, repeat_y, ..
                } => draw_image(surface, art, scale, at, (*repeat_x, *repeat_y)),
                LayerContent::Tiles(tiles) => draw_tiles(surface, art, scale, tiles, at),
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

/// A layer's image, and its normal map if it has one.
#[derive(Clone, Copy)]
struct Art<'a> {
    texture: &'a Arc<Texture>,
    normals: Option<&'a Arc<Texture>>,
}

fn normals_path(layer: &Layer) -> Option<&str> {
    match &layer.content {
        LayerContent::Image { normals, .. } => normals.as_deref(),
        LayerContent::Tiles(tiles) => tiles.tileset.normals.as_deref(),
    }
}

fn draw_image(
    surface: &mut dyn Surface,
    Art { texture, normals }: Art,
    scale: f32,
    at: (f32, f32),
    repeat: (bool, bool),
) {
    let (w, h) = (texture.width(), texture.height());
    if w == 0 || h == 0 {
        return;
    }
    let view = surface.view();
    // Where copies start along an axis, as how many images along from
    // `at`: once, or tiling the whole view.
    let copies = |at: f32, size: f32, view: u32, repeat: bool| match repeat {
        true => {
            let first = (-at / size).floor() as i32;
            first..((view as f32 - at) / size).floor() as i32 + 1
        }
        false => 0..1,
    };
    let size = (w as f32 / scale, h as f32 / scale);
    let src = Rect { x: 0, y: 0, w, h };
    let edge = |at: f32, size: f32, n: i32| at + n as f32 * size;
    for row in copies(at.1, size.1, view.1, repeat.1) {
        for column in copies(at.0, size.0, view.0, repeat.0) {
            surface.quad(&Quad {
                texture: texture.clone(),
                src,
                dst: [
                    edge(at.0, size.0, column),
                    edge(at.1, size.1, row),
                    edge(at.0, size.0, column + 1),
                    edge(at.1, size.1, row + 1),
                ],
                flip: Flip::default(),
                flash: None,
                normals: normals.cloned(),
            });
        }
    }
}

fn draw_tiles(
    surface: &mut dyn Surface,
    Art { texture, normals }: Art,
    scale: f32,
    tiles: &TileLayer,
    at: (f32, f32),
) {
    let set = &tiles.tileset;
    let image = &texture.levels[0];
    let (tw, th) = (
        set.tile_width as f32 / scale,
        set.tile_height as f32 / scale,
    );
    let rows = (tiles.tiles.len() / tiles.columns as usize) as i32;
    let view = surface.view();
    // The cells on screen along an axis, and one more each side for tiles
    // that are wider than tall swapped onto their side.
    let visible = |at: f32, size: f32, count: i32, view: u32| {
        let first = ((-at / size).floor() as i32 - 1).max(0);
        let end = (((view as f32 - at) / size).floor() as i32 + 2).min(count);
        first..end
    };
    let edge = |at: f32, size: f32, n: i32| at + n as f32 * size;
    for row in visible(at.1, th, rows, view.1) {
        for column in visible(at.0, tw, tiles.columns as i32, view.0) {
            let tile = tiles.tiles[(row * tiles.columns as i32 + column) as usize];
            let Some(src) = tile.index().and_then(|i| tile_rect(set, image, i)) else {
                continue;
            };
            let flip = Flip {
                x: tile.0 & Tile::FLIP_X != 0,
                y: tile.0 & Tile::FLIP_Y != 0,
                diagonal: tile.0 & Tile::FLIP_DIAGONAL != 0,
            };
            // A tile swapped onto its side stays in its cell, from the
            // cell's top left.
            let (w, h) = flip.size((tw, th));
            let (left, top) = (edge(at.0, tw, column), edge(at.1, th, row));
            let right = if w == tw {
                edge(at.0, tw, column + 1)
            } else {
                left + w
            };
            let bottom = if h == th {
                edge(at.1, th, row + 1)
            } else {
                top + h
            };
            surface.quad(&Quad {
                texture: texture.clone(),
                src,
                dst: [left, top, right, bottom],
                flip,
                flash: None,
                normals: normals.cloned(),
            });
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
    use clamber_sprite::{Rgba, encode_png};

    use super::*;
    use crate::canvas::{Canvas, Color, rgb};

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
        art.draw(&mut canvas, level, depth, origin, (0, 0));
        canvas
    }

    #[test]
    fn scrolls_by_parallax_from_the_offset() {
        let level = level(
            "[[layers]]\nname = \"hills\"\nimage = \"hills.png\"\npixel_art = true\n\
             parallax = [0.5, 0.25]\noffset = [10, 4]\n",
        );
        let mut art = LayerArt::default();
        art.insert("hills.png", coords(64, 32), true);
        // Top left at (10 - 100 * 0.5, 4 - 40 * 0.25) = (-40, -6).
        let canvas = draw(&art, &level, Depth::Back, (100.0, 40.0));
        assert_eq!(canvas.get(0, 0), Some(at(40, 6)));
        assert_eq!(canvas.get(7, 5), Some(at(47, 11)));
        // Pixel art snaps to the nearest pixel, and a layer stays put at
        // parallax 0.
        let canvas = draw(&art, &level, Depth::Back, (101.2, 42.4));
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
            "[[layers]]\nname = \"sky\"\nimage = \"sky.png\"\npixel_art = true\nrepeat = [true, false]\noffset = [0, 2]\n",
        );
        let mut art = LayerArt::default();
        art.insert("sky.png", coords(3, 2), true);
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

    const TILES: &str = "[[layers]]\nname = \"ground\"\ndepth = \"front\"\noffset = [1, 0]\npixel_art = true\n\
        tiles = { tileset = \"ground.png\", tile_size = [2, 2], margin = 1, spacing = 1, columns = 3, data = DATA }\n";

    fn tiles(data: &[u32]) -> Level {
        level(&TILES.replace("DATA", &format!("{data:?}")))
    }

    #[test]
    fn draws_tiles_from_the_tileset() {
        let mut art = LayerArt::default();
        art.insert("ground.png", tileset(), true);
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
        art.insert("ground.png", tileset(), true);
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

    #[test]
    fn loads_normal_maps_the_size_of_their_images() {
        let dir = std::env::temp_dir().join(format!("clamber-normals-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("wall.png"), encode_png(&coords(4, 2))).unwrap();
        let level = level(
            "[[layers]]\nname = \"wall\"\nimage = \"wall.png\"\nnormals = \"wall-normals.png\"\n",
        );
        std::fs::write(dir.join("wall-normals.png"), encode_png(&coords(4, 2))).unwrap();
        let art = LayerArt::load(&level, &dir).unwrap();
        assert!(art.get("wall-normals.png", false).is_some());

        std::fs::write(dir.join("wall-normals.png"), encode_png(&coords(2, 2))).unwrap();
        let err = LayerArt::load(&level, &dir).unwrap_err();
        assert_eq!(
            err,
            "layer `wall`: the normal map is 2x2, but its image is 4x2"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn layers_that_dim_are_drawn_in_the_steps_colours() {
        let level = level(
            "[[layers]]\nname = \"far\"\nimage = \"a.png\"\npixel_art = true\nrepeat = [true, true]\n\
             dim = true\n\
             [[layers]]\nname = \"near\"\nimage = \"b.png\"\npixel_art = true\n",
        );
        assert!(level.layers[0].dim && !level.layers[1].dim);
        let colour = [165, 99, 49, 255];
        let mut solid = RgbaImage::new(4, 2);
        solid.data.fill(colour);
        let mut art = LayerArt::default();
        art.insert("a.png", solid.clone(), true);
        art.insert("b.png", solid, true);
        let shown = |dim| {
            let mut canvas = Canvas::new(8, 6);
            art.draw(&mut canvas, &level, Depth::Back, (0.0, 0.0), dim);
            (canvas.get(0, 0).unwrap(), canvas.get(7, 5).unwrap())
        };
        let colour_of = |[r, g, b, _]: Rgba| rgb(r, g, b);
        let before = colour_of(colour);
        assert_eq!(shown((0, 32)), (before, before));
        let dimmed = colour_of(crate::dim::dim(colour, 16, 32));
        assert_ne!(dimmed, before);
        // The near layer covers the far one at the top left only.
        assert_eq!(shown((16, 32)), (before, dimmed));
        // Again, from the images already made.
        assert_eq!(shown((16, 32)), (before, dimmed));
    }
}
