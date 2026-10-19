//! Level layers: images, tiles and props, scrolled with parallax.
//!
//! A layer's top left is drawn at its offset less the view's top left
//! scaled by its parallax, so a parallax of 1 moves with the world and 0
//! stays put on screen. Its images cover one world unit per `scale`
//! texels, filtered smoothly unless the layer is pixel art, and are lit
//! by the level's lights if they have a normal map. A layer that dims is
//! drawn in the colours of the dimming's step, and a darker layer darker
//! still (`crate::dim`). A layer that sways leans each image about its
//! bottom edge, by the time alone, so it never touches the simulation.
//!
//! An image can be a whole number of times its size in the level file
//! says, as a replacement art pack's are ([`LayerArt::replaced`]): it's
//! drawn with that many more texels per unit, tiles and all, so it covers
//! the same part of the world.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use clamber_import::{Replacements, replace_image, replace_tileset};
use clamber_level::{
    Depth, Dim, Fixed, Layer, LayerContent, Level, Prop, Sway, Tile, TileLayer, Tileset,
};
use clamber_sprite::{Rect, RgbaImage, decode_png};

use crate::camera::point;
use crate::dim::{darker_image, dim_image, toward_image};
use crate::surface::{Flip, Quad, Surface, Texture};

/// The images a level's layers draw, by the path the level gives them and
/// whether they're pixel art.
#[derive(Clone, Debug, Default)]
pub struct LayerArt {
    /// With how many times the size the level gives it each one is.
    textures: BTreeMap<(String, bool), (Arc<Texture>, u32)>,
    /// Images in the dimming's colours or darker, made as they're first
    /// drawn, by path, pixel art or not, how and at which step.
    dimmed: Arc<Mutex<BTreeMap<DimKey, Arc<Texture>>>>,
}

type DimKey = (String, bool, Shade);

/// How a layer's image is coloured on a picture: its dimming, the step of
/// how many, and whether it's darker.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Shade {
    night: Option<String>,
    step: (u32, u32),
    darker: bool,
}

impl Shade {
    fn of(layer: &Layer, step: (u32, u32)) -> Shade {
        let step = match layer.dim {
            Dim::None => (0, 0),
            _ if step.0 == 0 => (0, 0),
            _ => step,
        };
        let night = match &layer.dim {
            Dim::Night(night) if step.0 > 0 => Some(night.clone()),
            _ => None,
        };
        Shade {
            night,
            step,
            darker: layer.darker,
        }
    }
}

impl LayerArt {
    /// Reads every image `level`'s layers use, relative to `dir`, the
    /// level file's folder, and checks the tiles are in their tilesets.
    pub fn load(level: &Level, dir: &Path) -> Result<LayerArt, String> {
        let mut art = LayerArt::default();
        let mut decoded: BTreeMap<&str, RgbaImage> = BTreeMap::new();
        for layer in &level.layers {
            let night = match &layer.dim {
                Dim::Night(night) => Some(night.as_str()),
                _ => None,
            };
            for path in images(layer)
                .into_iter()
                .flat_map(|(image, normals)| [Some(image), normals])
                .chain([night])
                .flatten()
            {
                if art.get(path, layer.pixel_art).is_some() {
                    continue;
                }
                if !decoded.contains_key(path) {
                    let file = dir.join(path);
                    let err = |e: String| format!("{}: {e}", file.display());
                    let bytes = std::fs::read(&file).map_err(|e| err(e.to_string()))?;
                    let image = decode_png(&bytes).map_err(|e| err(e.0))?;
                    decoded.insert(path, image.to_rgba());
                }
                art.insert(path, decoded[path].clone(), layer.pixel_art);
            }
        }
        art.check(level)?;
        Ok(art)
    }

    /// Adds an image under the path layers name it by, for layers that
    /// are pixel art or not.
    pub fn insert(&mut self, path: &str, image: RgbaImage, pixel_art: bool) {
        self.insert_scaled(path, image, pixel_art, 1);
    }

    /// Adds an image `factor` times the size layers draw it at.
    fn insert_scaled(&mut self, path: &str, image: RgbaImage, pixel_art: bool, factor: u32) {
        let texture = Texture::new(image, pixel_art);
        self.textures
            .insert((path.to_string(), pixel_art), (texture, factor));
    }

    fn get(&self, path: &str, pixel_art: bool) -> Option<&Arc<Texture>> {
        self.textures
            .get(&(path.to_string(), pixel_art))
            .map(|t| &t.0)
    }

    /// How many times the size `level` gives it the image `path` is.
    fn factor(&self, path: &str, pixel_art: bool) -> u32 {
        self.textures
            .get(&(path.to_string(), pixel_art))
            .map_or(1, |t| t.1)
    }

    /// This art with the pixel-art images of `level`'s layers that
    /// `replacements` has pictures for replaced: tiles in tilesets, and
    /// other images whole. Images with normal maps are kept as they are.
    /// `None` when nothing is replaced. Replacements that don't fit are
    /// added to `problems`.
    pub fn replaced(
        &self,
        level: &Level,
        replacements: &Replacements,
        problems: &mut Vec<String>,
    ) -> Option<LayerArt> {
        let mut art = LayerArt {
            textures: self.textures.clone(),
            dimmed: Arc::default(),
        };
        let mut any = false;
        let mut done = std::collections::BTreeSet::new();
        for layer in level.layers.iter().filter(|l| l.pixel_art) {
            for (path, normals) in images(layer) {
                if normals.is_some() || !done.insert(path) {
                    continue;
                }
                let Some(texture) = self.get(path, true) else {
                    continue;
                };
                let image = &texture.levels[0];
                let made = match &layer.content {
                    LayerContent::Tiles(tiles) => {
                        replace_tileset(image, &tiles.tileset, replacements, problems)
                    }
                    _ => replace_image(image, replacements, problems),
                };
                if let Some((image, factor)) = made {
                    art.insert_scaled(path, image, true, factor);
                    any = true;
                }
            }
        }
        any.then_some(art)
    }

    fn check(&self, level: &Level) -> Result<(), String> {
        for layer in &level.layers {
            for (image, normals) in images(layer) {
                let get = |p| self.get(p, layer.pixel_art);
                let (Some(image), Some(normals)) = (get(image), normals.and_then(get)) else {
                    continue;
                };
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

    /// The image `path` coloured as `shade` says.
    fn dimmed(&self, path: &str, pixel_art: bool, shade: Shade) -> Option<Arc<Texture>> {
        let texture = self.get(path, pixel_art)?;
        if shade == Shade::default() {
            return Some(texture.clone());
        }
        let night = match &shade.night {
            Some(night) => Some(self.get(night, pixel_art)?.clone()),
            None => None,
        };
        let mut dimmed = self.dimmed.lock().unwrap_or_else(|e| e.into_inner());
        let (step, steps) = shade.step;
        let darker = shade.darker;
        let key = (path.to_string(), pixel_art, shade);
        let made = dimmed.entry(key).or_insert_with(|| {
            let image = &texture.levels[0];
            let mut image = match (&night, step) {
                (_, 0) => image.clone(),
                (Some(night), _) => toward_image(image, &night.levels[0], step, steps),
                (None, _) => dim_image(image, step, steps),
            };
            if darker {
                image = darker_image(&image);
            }
            Texture::new(image, pixel_art)
        });
        Some(made.clone())
    }

    /// Draws `level`'s layers at `depth` in order, as `viewing` says.
    /// Images that aren't here are skipped.
    pub fn draw(&self, surface: &mut dyn Surface, level: &Level, depth: Depth, viewing: &Viewing) {
        self.draw_some(surface, level, viewing, |_, l| l.depth == depth);
    }

    /// Draws the layers `keep` says to, by index and layer, in order, as
    /// `viewing` says.
    pub fn draw_some(
        &self,
        surface: &mut dyn Surface,
        level: &Level,
        viewing: &Viewing,
        keep: impl Fn(usize, &Layer) -> bool,
    ) {
        let time = viewing.time;
        let kept = level.layers.iter().enumerate().filter(|(i, l)| keep(*i, l));
        for (_, layer) in kept {
            let shade = Shade::of(layer, viewing.dim);
            let art = |(image, normals): (&str, Option<&str>)| {
                let texture = self.dimmed(image, layer.pixel_art, shade.clone())?;
                let normals = normals.and_then(|p| self.get(p, layer.pixel_art)).cloned();
                Some((texture, normals))
            };
            let mut offset = point(layer.offset);
            if viewing.odd() {
                let alternate = point(layer.alternate);
                offset = (offset.0 + alternate.0, offset.1 + alternate.1);
            }
            let parallax = point(layer.parallax);
            let at = (
                (offset.0 - viewing.scroll_x(parallax.0)) as f32,
                (offset.1 - viewing.scroll_y(parallax.1)) as f32,
            );
            let look = |path: &str| Look {
                scale: (layer.scale * self.factor(path, layer.pixel_art)) as f32,
                opacity: (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
                sway: layer.sway,
                time,
            };
            match &layer.content {
                LayerContent::Image {
                    path,
                    normals,
                    repeat_x,
                    repeat_y,
                } => {
                    let Some((texture, normals)) = art((path, normals.as_deref())) else {
                        continue;
                    };
                    let art = Art {
                        texture: &texture,
                        normals: normals.as_ref(),
                    };
                    let look = look(path);
                    let lean = look.lean(offset.0, texture.height() as f32 / look.scale);
                    let repeat = (*repeat_x, *repeat_y);
                    let (w, h) = (texture.width(), texture.height());
                    if layer.bands.is_empty() {
                        let whole = Rect { x: 0, y: 0, w, h };
                        draw_image(surface, art, look, lean, at, whole, repeat);
                        continue;
                    }
                    // Each band's rows, in texels, scrolled at its own rate.
                    let texels = look.scale as u32;
                    let mut top = 0;
                    for band in &layer.bands {
                        let rows = (band.rows * texels).min(h.saturating_sub(top));
                        let src = Rect {
                            x: 0,
                            y: top,
                            w,
                            h: rows,
                        };
                        let x = offset.0 - viewing.scroll_x(band.parallax.to_f64());
                        let y = at.1 + (top / texels) as f32;
                        draw_image(surface, art, look, lean, (x as f32, y), src, repeat);
                        top += rows;
                    }
                }
                LayerContent::Tiles(tiles) => {
                    let set = &tiles.tileset;
                    let Some((texture, normals)) = art((&set.image, set.normals.as_deref())) else {
                        continue;
                    };
                    let art = Art {
                        texture: &texture,
                        normals: normals.as_ref(),
                    };
                    let factor = self.factor(&set.image, layer.pixel_art);
                    let set = Tileset {
                        tile_width: set.tile_width * factor,
                        tile_height: set.tile_height * factor,
                        margin: set.margin * factor,
                        spacing: set.spacing * factor,
                        ..set.clone()
                    };
                    draw_tiles(surface, art, look(&set.image), &set, tiles, at);
                }
                LayerContent::Props(props) => {
                    for prop in props {
                        let Some((texture, normals)) = art((&prop.image, prop.normals.as_deref()))
                        else {
                            continue;
                        };
                        let art = Art {
                            texture: &texture,
                            normals: normals.as_ref(),
                        };
                        draw_prop(surface, art, look(&prop.image), prop, at);
                    }
                }
            }
        }
    }
}

/// Each image a layer draws, and its normal map if it has one.
fn images(layer: &Layer) -> Vec<(&str, Option<&str>)> {
    match &layer.content {
        LayerContent::Image { path, normals, .. } => vec![(path, normals.as_deref())],
        LayerContent::Tiles(tiles) => {
            vec![(&tiles.tileset.image, tiles.tileset.normals.as_deref())]
        }
        LayerContent::Props(props) => props
            .iter()
            .map(|p| (p.image.as_str(), p.normals.as_deref()))
            .collect(),
    }
}

/// How a layer's images are drawn, whatever they are.
#[derive(Clone, Copy)]
struct Look {
    /// Texels per view unit.
    scale: f32,
    opacity: u8,
    sway: Option<Sway>,
    time: f64,
}

impl Look {
    /// How far right the top of an image `height` units tall leans, for
    /// an image at `x` in the layer: neighbours swing out of step.
    fn lean(&self, x: f64, height: f32) -> f32 {
        let Some(sway) = self.sway else {
            return 0.0;
        };
        let phase = std::f64::consts::TAU * self.time / sway.period + x / 29.0;
        // A taller image leans further, but never more than half its
        // height, so a short tuft doesn't fold over.
        (sway.amount * phase.sin()).clamp(-height as f64 / 2.0, height as f64 / 2.0) as f32
    }
}

/// Where, when and how a level's layers are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewing {
    /// The view's top left, in world units.
    pub origin: (f64, f64),
    /// When it's drawn on the original's pixel grid, how much further
    /// across layers are scrolled before they're kept to whole pixels:
    /// an eighth of the view's position past its left edge, in
    /// 1/256 px (`spec/formats/graphics.md`, "The horizon layer").
    pub grid: Option<f64>,
    /// The dimming's step, of how many.
    pub dim: (u32, u32),
    /// Seconds into the level.
    pub time: f64,
    /// Steps of the level played.
    pub steps: u32,
}

impl Viewing {
    /// Viewing from `origin` at the level's start, between pixels.
    pub fn at(origin: (f64, f64)) -> Viewing {
        Viewing {
            origin,
            ..Viewing::default()
        }
    }

    /// Whether the step just played is an odd one, counting the level's
    /// first as 0, so layers that alternate are drawn further.
    fn odd(&self) -> bool {
        self.steps > 0 && self.steps.is_multiple_of(2)
    }

    /// How far a layer with `parallax` across is scrolled. On the pixel
    /// grid that's whole pixels, rounded down, from the view's whole
    /// pixel: a layer at half speed moves a pixel for every two of the
    /// view's.
    fn scroll_x(&self, parallax: f64) -> f64 {
        match self.grid {
            Some(nudge) => (self.origin.0.round() * parallax + nudge).floor(),
            None => self.origin.0 * parallax,
        }
    }

    /// How far a layer with `parallax` up and down is scrolled.
    fn scroll_y(&self, parallax: f64) -> f64 {
        match self.grid {
            Some(_) => (self.origin.1.round() * parallax).floor(),
            None => self.origin.1 * parallax,
        }
    }
}

/// How much further than the view's whole pixel layers scroll across on
/// the pixel grid, for a view whose position across is `left`: an eighth
/// of its position past its pixel, kept to 1/256 px, as the first
/// level's horizon layer has it (`spec/formats/graphics.md`, "The
/// horizon layer"). Too little to move a layer that keeps to the view or
/// to half its speed.
pub fn nudge(left: Fixed) -> f64 {
    let f = (left.to_bits() >> (clamber_fixed::FRAC_BITS - 8)) & 0xFF;
    (f >> 3) as f64 / 256.0
}

/// A layer's image, and its normal map if it has one.
#[derive(Clone, Copy)]
struct Art<'a> {
    texture: &'a Arc<Texture>,
    normals: Option<&'a Arc<Texture>>,
}

/// Draws the part `src` of a layer's image with its top left at `at`.
fn draw_image(
    surface: &mut dyn Surface,
    Art { texture, normals }: Art,
    look: Look,
    lean: f32,
    at: (f32, f32),
    src: Rect,
    repeat: (bool, bool),
) {
    let scale = look.scale;
    let (w, h) = (src.w, src.h);
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
                opacity: look.opacity,
                lean,
                normals: normals.cloned(),
            });
        }
    }
}

fn draw_prop(
    surface: &mut dyn Surface,
    Art { texture, normals }: Art,
    look: Look,
    prop: &Prop,
    at: (f32, f32),
) {
    let (w, h) = (texture.width(), texture.height());
    if w == 0 || h == 0 {
        return;
    }
    let size = (w as f32 / look.scale, h as f32 / look.scale);
    let foot = point(prop.at);
    let lean = look.lean(foot.0, size.1);
    let left = at.0 + foot.0 as f32 - size.0 / 2.0;
    let bottom = at.1 + foot.1 as f32;
    let dst = [left, bottom - size.1, left + size.0, bottom];
    let view = surface.view();
    let reach = lean.abs();
    if dst[2] + reach < 0.0 || dst[0] - reach > view.0 as f32 || dst[3] < 0.0 {
        return;
    }
    if dst[1] > view.1 as f32 {
        return;
    }
    surface.quad(&Quad {
        texture: texture.clone(),
        src: Rect { x: 0, y: 0, w, h },
        dst,
        flip: Flip {
            x: prop.flip,
            ..Flip::default()
        },
        flash: None,
        opacity: look.opacity,
        lean,
        normals: normals.cloned(),
    });
}

fn draw_tiles(
    surface: &mut dyn Surface,
    Art { texture, normals }: Art,
    look: Look,
    set: &Tileset,
    tiles: &TileLayer,
    at: (f32, f32),
) {
    let scale = look.scale;
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
                opacity: look.opacity,
                lean: 0.0,
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
        art.draw(&mut canvas, level, depth, &Viewing::at(origin));
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
    fn on_the_pixel_grid_layers_scroll_whole_pixels_rounded_down() {
        let level = level(
            "[[layers]]\nname = \"far\"\nimage = \"far.png\"\npixel_art = true\n\
             parallax = [0.5, 0.25]\noffset = [0, -1]\nrepeat = [true, true]\n",
        );
        let mut art = LayerArt::default();
        art.insert("far.png", coords(64, 64), true);
        let shown = |origin, nudge| {
            let viewing = Viewing {
                grid: Some(nudge),
                ..Viewing::at(origin)
            };
            let mut canvas = Canvas::new(8, 6);
            art.draw(&mut canvas, &level, Depth::Back, &viewing);
            canvas.get(0, 0).unwrap()
        };
        // Pixel (0, 0) shows (⌊x / 2⌋, ⌊y / 4⌋ + 1), whatever the
        // fraction, and a nudge too small to carry changes nothing.
        for (x, y) in [(0, 0), (1, 1), (2, 3), (3, 4), (5, 7), (9, 11)] {
            let want = at(x / 2, y / 4 + 1);
            assert_eq!(shown((x as f64, y as f64), 0.0), want, "{x}, {y}");
            assert_eq!(shown((x as f64, y as f64), 31.0 / 256.0), want);
        }
    }

    fn vector(name: &str) -> Vec<Vec<i64>> {
        let path = format!(
            "{}/../../spec/vectors/graphics/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<Vec<i64>> = text
            .lines()
            .skip(1)
            .map(|l| l.split('\t').map(|n| n.parse().unwrap()).collect())
            .collect();
        assert!(!rows.is_empty(), "{name}");
        rows
    }

    /// The first level's far layer as the importer places it: parallax
    /// (1/2, 1/4), offset (0, -1), repeating 512 by 256.
    #[test]
    fn the_far_layer_s_place_matches_the_spec_vectors() {
        for row in vector("far-place.tsv") {
            let [e, t, i, j, x, y] = row[..] else {
                panic!("{row:?}");
            };
            let viewing = Viewing {
                grid: Some(0.0),
                ..Viewing::at((e as f64, t as f64))
            };
            let left = -viewing.scroll_x(0.5) as i64;
            let top = -1 - viewing.scroll_y(0.25) as i64;
            let shown = ((i - left).rem_euclid(512), (j - top).rem_euclid(256));
            assert_eq!(shown, (x, y), "{row:?}");
        }
    }

    /// Band `k` of the first level's horizon layer, at (`k` + 2)/16 of
    /// the view's speed, shifted by the nudge; its picture repeats every
    /// 32 px.
    #[test]
    fn the_horizon_shifts_match_the_spec_vectors() {
        for row in vector("horizon-shifts.tsv") {
            let (e, f) = (row[0], row[1]);
            let left = Fixed::from_bits((e << 32) + (f << 24));
            let viewing = Viewing {
                grid: Some(nudge(left)),
                ..Viewing::at((e as f64, 0.0))
            };
            let shifts: Vec<i64> = (0..4)
                .map(|k| (viewing.scroll_x((k + 2) as f64 / 16.0) as i64).rem_euclid(32))
                .collect();
            assert_eq!(shifts, row[2..], "{row:?}");
        }
    }

    #[test]
    fn bands_scroll_across_at_their_own_rates() {
        let level = level(
            "[[layers]]\nname = \"strip\"\nimage = \"strip.png\"\npixel_art = true\n\
             parallax = [0, 0]\noffset = [0, 1]\nrepeat = [true, false]\n\
             bands = [{ rows = 1, parallax = 0.125 }, { rows = 2, parallax = 0.1875 }]\n",
        );
        let mut art = LayerArt::default();
        art.insert("strip.png", coords(32, 4), true);
        let rows = |x: f64, nudge| {
            let viewing = Viewing {
                grid: Some(nudge),
                ..Viewing::at((x, 50.0))
            };
            let mut canvas = Canvas::new(8, 6);
            canvas.clear(BG);
            art.draw(&mut canvas, &level, Depth::Back, &viewing);
            (0..6)
                .map(|y| canvas.get(0, y).unwrap())
                .collect::<Vec<_>>()
        };
        // Row 0 is above the strip, and its last row isn't in a band.
        assert_eq!(rows(0.0, 0.0), [BG, at(0, 0), at(0, 1), at(0, 2), BG, BG]);
        // At x = 15, ⌊15 / 8⌋ = 1 and ⌊45 / 16⌋ = 2. At x = 5 the nudge
        // carries the second band's 15/16 on to 1.
        assert_eq!(rows(15.0, 0.0)[1..4], [at(1, 0), at(2, 1), at(2, 2)]);
        assert_eq!(rows(5.0, 0.0)[1..4], [at(0, 0), at(0, 1), at(0, 2)]);
        assert_eq!(
            rows(5.0, 16.0 / 256.0)[1..4],
            [at(0, 0), at(1, 1), at(1, 2)]
        );
        // The image repeats across.
        assert_eq!(rows(272.0, 0.0)[1..4], [at(2, 0), at(19, 1), at(19, 2)]);
    }

    #[test]
    fn alternating_layers_move_after_odd_steps() {
        let level = level(
            "[[layers]]\nname = \"rows\"\nimage = \"rows.png\"\npixel_art = true\n\
             parallax = [0, 0]\noffset = [0, -1]\nalternate = [0, 1]\n",
        );
        assert_eq!(level.layers[0].alternate, clamber_level::Vec2::ints(0, 1));
        let mut art = LayerArt::default();
        art.insert("rows.png", coords(8, 8), true);
        let top = |steps| {
            let viewing = Viewing {
                steps,
                ..Viewing::default()
            };
            let mut canvas = Canvas::new(8, 6);
            art.draw(&mut canvas, &level, Depth::Back, &viewing);
            canvas.get(0, 0).unwrap()
        };
        // After step 0 (one played), 2, 4, … as given; after 1, 3, …
        // one row lower.
        assert_eq!(
            [top(0), top(1), top(2), top(3), top(4)].map(|c| c == at(0, 1)),
            [true, true, false, true, false]
        );
        assert_eq!(top(2), at(0, 0));
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

    const PROPS: &str = "[[layers]]\nname = \"ferns\"\npixel_art = true\noffset = [1, 0]\n\
        props = [{ image = \"fern.png\", at = [2, 4] }, { image = \"fern.png\", at = [5, 6], flip = true }]\n";

    #[test]
    fn places_props_by_the_middle_of_their_bottom_edge() {
        let mut art = LayerArt::default();
        art.insert("fern.png", coords(2, 3), true);
        let canvas = draw(&art, &level(PROPS), Depth::Back, (0.0, 0.0));
        let grid: Vec<Vec<_>> = (0..6)
            .map(|y| (0..8).map(|x| canvas.get(x, y).unwrap()).collect())
            .collect();
        // From the layer's offset of 1: the first from (2, 1) to (4, 4),
        // the second mirrored from (5, 3) to (7, 6).
        #[rustfmt::skip]
        assert_eq!(grid, [
            [BG, BG, BG, BG, BG, BG, BG, BG],
            [BG, BG, at(0, 0), at(1, 0), BG, BG, BG, BG],
            [BG, BG, at(0, 1), at(1, 1), BG, BG, BG, BG],
            [BG, BG, at(0, 2), at(1, 2), BG, at(1, 0), at(0, 0), BG],
            [BG, BG, BG, BG, BG, at(1, 1), at(0, 1), BG],
            [BG, BG, BG, BG, BG, at(1, 2), at(0, 2), BG],
        ]);
    }

    #[test]
    fn draws_a_layer_at_its_opacity() {
        let level = level(
            "[[layers]]\nname = \"leaves\"\nimage = \"white.png\"\npixel_art = true\nopacity = 0.5\n",
        );
        let mut white = RgbaImage::new(8, 6);
        white.data.fill([255, 255, 255, 255]);
        let mut art = LayerArt::default();
        art.insert("white.png", white, true);
        let canvas = draw(&art, &level, Depth::Back, (0.0, 0.0));
        // Half of white over the background's 9.
        assert_eq!(canvas.get(3, 3), Some(rgb(132, 132, 132)));
    }

    #[test]
    fn sways_by_the_time_leaning_the_top_about_the_bottom() {
        let level = level(
            "[[layers]]\nname = \"reed\"\npixel_art = true\nsway = { amount = 2, period = 4 }\n\
             props = [{ image = \"reed.png\", at = [3, 5] }]\n",
        );
        let mut art = LayerArt::default();
        art.insert("reed.png", coords(2, 4), true);
        let rows = |time: f64| {
            let mut canvas = Canvas::new(8, 6);
            canvas.clear(BG);
            let viewing = Viewing {
                time,
                ..Viewing::default()
            };
            art.draw(&mut canvas, &level, Depth::Back, &viewing);
            let first = |y: i32| (0..8).find(|&x| canvas.get(x, y) != Some(BG));
            [first(1), first(4)]
        };
        // Leaning furthest right a quarter of the way through a swing and
        // furthest left at three quarters; the bottom row stays put.
        assert_eq!(rows(1.0), [Some(4), Some(2)]);
        assert_eq!(rows(3.0), [Some(0), Some(2)]);
        // The same at any time is the same picture.
        assert_eq!(rows(5.0), rows(1.0));
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
        assert_eq!(
            (&level.layers[0].dim, &level.layers[1].dim),
            (&Dim::Palette, &Dim::None)
        );
        let colour = [165, 99, 49, 255];
        let mut solid = RgbaImage::new(4, 2);
        solid.data.fill(colour);
        let mut art = LayerArt::default();
        art.insert("a.png", solid.clone(), true);
        art.insert("b.png", solid, true);
        let shown = |dim| {
            let mut canvas = Canvas::new(8, 6);
            let viewing = Viewing {
                dim,
                ..Viewing::default()
            };
            art.draw(&mut canvas, &level, Depth::Back, &viewing);
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
    #[test]
    fn layers_dim_to_their_night_image_and_darker_layers_are_darker() {
        let level = level(
            "[[layers]]\nname = \"sky\"\nimage = \"a.png\"\npixel_art = true\nrepeat = [true, true]\n\
             dim = { night = \"night.png\" }\n\
             [[layers]]\nname = \"far\"\nimage = \"b.png\"\npixel_art = true\n\
             dim = true\ndarker = true\n",
        );
        assert_eq!(level.layers[0].dim, Dim::Night("night.png".into()));
        assert!(level.layers[1].darker && !level.layers[0].darker);
        let colour = [165, 99, 49, 255];
        let mut solid = RgbaImage::new(4, 2);
        solid.data.fill(colour);
        let mut night = RgbaImage::new(1, 1);
        night.data.fill([0, 0, 25, 255]);
        let mut art = LayerArt::default();
        art.insert("a.png", solid.clone(), true);
        art.insert("b.png", solid, true);
        art.insert("night.png", night.clone(), true);
        let shown = |dim| {
            let mut canvas = Canvas::new(8, 6);
            let viewing = Viewing {
                dim,
                ..Viewing::default()
            };
            art.draw(&mut canvas, &level, Depth::Back, &viewing);
            (canvas.get(7, 5).unwrap(), canvas.get(0, 0).unwrap())
        };
        let colour_of = |[r, g, b, _]: Rgba| rgb(r, g, b);
        let darker = |c| colour_of(crate::dim::darker(c));
        assert_eq!(shown((0, 32)), (colour_of(colour), darker(colour)));
        let sky = colour_of(crate::dim::toward(colour, night.data[0], 16, 32));
        let far = darker(crate::dim::dim(colour, 16, 32));
        assert_eq!(shown((16, 32)), (sky, far));
        assert_eq!(shown((32, 32)), (colour_of(night.data[0]), far_end(colour)));

        fn far_end(colour: Rgba) -> u32 {
            let [r, g, b, _] = crate::dim::darker(crate::dim::dim(colour, 32, 32));
            rgb(r, g, b)
        }
    }
}
