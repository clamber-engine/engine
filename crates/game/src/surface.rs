//! What the game draws on, in view units: the camera's view is always
//! [`VIEW`](crate::VIEW) units across, whatever size it's shown at.
//!
//! A [`Surface`] takes solid shapes and textured [`Quad`]s. The
//! [`Canvas`](crate::Canvas) draws them straight into pixels at a whole
//! number of pixels per unit, for tests, captures and windows without a
//! GPU; a [`DrawList`] records them for the GPU to draw at the window's
//! own resolution.
//!
//! [`Texture`]s are drawn smoothly, filtered from mipmaps, unless they're
//! pixel art, which keeps sharp texel edges at any size.
//!
//! A quad with a normal map is lit: its colour is multiplied by the light
//! falling on each texel's normal under the surface's [`Lighting`], an
//! ambient colour, a key light and point lights. Both surfaces shade the
//! same way, so captures look like the game.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use clamber_sprite::{Pixels, Rect, Rgba, RgbaImage};

use crate::canvas::Color;

/// Something to draw on, in view units.
pub trait Surface {
    /// The visible area, in view units.
    fn view(&self) -> (u32, u32);

    /// Fills everything with `color`.
    fn clear(&mut self, color: Color);

    /// Fills the rectangle from `(x, y)`, `w` by `h`.
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color);

    /// Mixes `color` in over everything at `alpha`, from 0 (no change) to
    /// 255, to dim what's under a menu.
    fn shade(&mut self, color: Color, alpha: u8);

    /// A line one unit thick between two points.
    fn line(&mut self, a: (f64, f64), b: (f64, f64), color: Color);

    /// Fills a simple polygon (any winding), mixing `color` in at `alpha`.
    fn fill_polygon(&mut self, points: &[(f64, f64)], color: Color, alpha: u8);

    /// Draws part of a texture.
    fn quad(&mut self, quad: &Quad);

    /// Lights the quads with normal maps drawn from now on.
    fn light(&mut self, lighting: &Lighting);

    /// The outline of a rectangle, one unit thick, inside its bounds.
    fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.fill_rect(x, y, w, 1, color);
        self.fill_rect(x, y + h - 1, w, 1, color);
        self.fill_rect(x, y, 1, h, color);
        self.fill_rect(x + w - 1, y, 1, h, color);
    }

    /// Fills one unit square.
    fn plot(&mut self, x: i32, y: i32, color: Color) {
        self.fill_rect(x, y, 1, 1, color);
    }
}

/// How an image is turned as it's drawn: mirrored left to right, then
/// top to bottom, then across its leading diagonal (swapping x and y).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flip {
    pub x: bool,
    pub y: bool,
    pub diagonal: bool,
}

impl Flip {
    /// The point of the source image, from `0` to its size, that shows at
    /// `(x, y)` of the drawn one, for a source `w` by `h`.
    pub fn source(self, (x, y): (f32, f32), (w, h): (f32, f32)) -> (f32, f32) {
        let (x, y) = if self.diagonal { (y, x) } else { (x, y) };
        (
            if self.x { w - x } else { x },
            if self.y { h - y } else { y },
        )
    }

    /// The drawn size of a `w` by `h` source.
    pub fn size<T>(self, (w, h): (T, T)) -> (T, T) {
        if self.diagonal { (h, w) } else { (w, h) }
    }
}

/// An image ready to draw: its pixels, smaller copies of them for drawing
/// it small, and how to filter it.
#[derive(Debug)]
pub struct Texture {
    id: u64,
    /// Full size first, then each mipmap at half the size of the last,
    /// down to one texel. Pixel art has only the first.
    pub levels: Vec<RgbaImage>,
    /// Drawn with sharp texel edges, rather than filtered smoothly.
    pub pixel_art: bool,
}

impl Texture {
    pub fn new(image: RgbaImage, pixel_art: bool) -> Arc<Texture> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut levels = vec![image];
        if !pixel_art {
            while let Some(last) = levels.last().filter(|l| l.width > 1 || l.height > 1) {
                levels.push(half(last));
            }
        }
        Arc::new(Texture {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            levels,
            pixel_art,
        })
    }

    /// Different for every texture made, for caching what's made from it.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.levels[0].width
    }

    pub fn height(&self) -> u32 {
        self.levels[0].height
    }
}

/// `image` at half its size (rounded down, at least 1), each texel the
/// average of up to four, weighted by alpha so transparent texels don't
/// darken edges.
fn half(image: &RgbaImage) -> RgbaImage {
    let (w, h) = ((image.width / 2).max(1), (image.height / 2).max(1));
    let mut out = Pixels::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (2 * x + dx).min(image.width - 1);
                let sy = (2 * y + dy).min(image.height - 1);
                let [r, g, b, a] = image.get(sx, sy);
                let a32 = a as u32;
                sum[0] += r as u32 * a32;
                sum[1] += g as u32 * a32;
                sum[2] += b as u32 * a32;
                sum[3] += a32;
            }
            let texel = match sum[3] {
                0 => [0; 4],
                total => {
                    let c = |i: usize| ((sum[i] + total / 2) / total) as u8;
                    [c(0), c(1), c(2), ((total + 2) / 4) as u8]
                }
            };
            out.set(x, y, texel);
        }
    }
    out
}

/// Part of a texture drawn into a rectangle.
#[derive(Clone, Debug)]
pub struct Quad {
    pub texture: Arc<Texture>,
    /// The part of the texture to draw, in texels.
    pub src: Rect,
    /// Where it goes, as left, top, right and bottom edges in view units.
    /// Quads that share an edge's value meet without a gap.
    pub dst: [f32; 4],
    pub flip: Flip,
    /// Draw every visible texel in this colour, keeping its alpha.
    pub flash: Option<Rgba>,
    /// A normal map laid out like `texture`, to light it by. Flashes
    /// aren't lit.
    pub normals: Option<Arc<Texture>>,
}

/// The most point lights that shade a frame.
pub const MAX_LIGHTS: usize = 16;

/// How far in front of what they light point lights are, in view units,
/// so a surface facing the viewer is lit even right under one.
pub const LIGHT_HEIGHT: f32 = 24.0;

/// The light that quads with normal maps are drawn in, in view units.
/// Colours are from 0 to 1 for each of red, green and blue, already
/// multiplied by their intensity, so they can be more than 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    pub ambient: [f32; 3],
    /// Towards the key light, of length 1: x right, y down and z towards
    /// the viewer.
    pub key_direction: [f32; 3],
    pub key: [f32; 3],
    /// At most [`MAX_LIGHTS`]; any more are left out.
    pub lights: Vec<PointLight>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointLight {
    pub at: (f32, f32),
    /// Where its light has faded to nothing.
    pub radius: f32,
    pub color: [f32; 3],
}

impl Default for Lighting {
    /// The default level lighting, with no point lights.
    fn default() -> Lighting {
        Lighting::new(&clamber_level::Lighting::default(), Vec::new())
    }
}

impl Lighting {
    /// A level's ambient and key lights, with point lights already in
    /// view units.
    pub fn new(level: &clamber_level::Lighting, mut lights: Vec<PointLight>) -> Lighting {
        let color = |c: [u8; 3], times: f64| c.map(|v| (v as f64 / 255.0 * times) as f32);
        let [x, y, z] = level.key.direction;
        let length = (x * x + y * y + z * z).sqrt().max(f64::MIN_POSITIVE);
        lights.truncate(MAX_LIGHTS);
        Lighting {
            ambient: color(level.ambient, 1.0),
            key_direction: [x, y, z].map(|v| (v / length) as f32),
            key: color(level.key.color, level.key.intensity),
            lights,
        }
    }

    /// The light falling at `at` on a surface facing `normal`, a unit
    /// vector in view axes: what to multiply its colour by.
    pub fn at(&self, normal: [f32; 3], at: (f32, f32)) -> [f32; 3] {
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let key = dot(normal, self.key_direction).max(0.0);
        let mut light = std::array::from_fn(|i| self.ambient[i] + self.key[i] * key);
        for l in &self.lights {
            let d = [l.at.0 - at.0, l.at.1 - at.1, LIGHT_HEIGHT];
            let reach = (1.0 - d[0].hypot(d[1]) / l.radius).clamp(0.0, 1.0);
            if reach == 0.0 {
                continue;
            }
            let facing = (dot(normal, d) / dot(d, d).sqrt()).max(0.0);
            for (out, c) in light.iter_mut().zip(l.color) {
                *out += c * reach * reach * facing;
            }
        }
        light
    }
}

/// The normal in view axes of a texel of a normal map, from its colour
/// premultiplied by alpha with channels from 0 to 1, drawn with `flip`:
/// `None` where it's transparent.
pub fn normal(encoded: [f32; 4], flip: Flip) -> Option<[f32; 3]> {
    let [r, g, b, a] = encoded;
    if a <= 0.0 {
        return None;
    }
    // Stored with y up; the view's y is down.
    let decode = |c: f32| c / a * 2.0 - 1.0;
    let (mut x, mut y, z) = (decode(r), -decode(g), decode(b));
    if flip.x {
        x = -x;
    }
    if flip.y {
        y = -y;
    }
    if flip.diagonal {
        (x, y) = (y, x);
    }
    let length = (x * x + y * y + z * z).sqrt();
    (length > 0.0).then(|| [x / length, y / length, z / length])
}

/// Something recorded in a [`DrawList`], in view units.
#[derive(Clone, Debug)]
pub enum Drawn {
    /// A rectangle as left, top, right and bottom edges, mixed in at
    /// `alpha`.
    Rect {
        edges: [f32; 4],
        color: Color,
        alpha: u8,
    },
    /// A line one unit thick from `a` to `b`, covering the unit squares
    /// whose top left corners it passes through.
    Line {
        a: (f32, f32),
        b: (f32, f32),
        color: Color,
    },
    Polygon {
        points: Vec<(f32, f32)>,
        color: Color,
        alpha: u8,
    },
    Quad(Quad),
}

/// A frame's drawing, recorded for the GPU.
#[derive(Clone, Debug)]
pub struct DrawList {
    pub view: (u32, u32),
    /// Drawn at one pixel per view unit, then scaled up by a whole number
    /// with nearest-neighbour filtering ([`Look::pixel_art`](crate::Look)).
    pub pixel_art: bool,
    /// What the frame starts as.
    pub background: Color,
    /// In drawing order.
    pub items: Vec<Drawn>,
    /// The lighting the frame's lit quads are drawn in.
    pub lighting: Lighting,
}

impl DrawList {
    pub fn new(view: (u32, u32)) -> DrawList {
        DrawList {
            view,
            pixel_art: false,
            background: 0,
            items: Vec::new(),
            lighting: Lighting::default(),
        }
    }
}

impl Surface for DrawList {
    fn view(&self) -> (u32, u32) {
        self.view
    }

    fn clear(&mut self, color: Color) {
        self.background = color;
        self.items.clear();
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if w > 0 && h > 0 {
            let edges = [x, y, x + w, y + h].map(|v| v as f32);
            self.items.push(Drawn::Rect {
                edges,
                color,
                alpha: 255,
            });
        }
    }

    fn shade(&mut self, color: Color, alpha: u8) {
        let edges = [0.0, 0.0, self.view.0 as f32, self.view.1 as f32];
        self.items.push(Drawn::Rect {
            edges,
            color,
            alpha,
        });
    }

    fn line(&mut self, a: (f64, f64), b: (f64, f64), color: Color) {
        let f = |p: (f64, f64)| (p.0 as f32, p.1 as f32);
        self.items.push(Drawn::Line {
            a: f(a),
            b: f(b),
            color,
        });
    }

    fn fill_polygon(&mut self, points: &[(f64, f64)], color: Color, alpha: u8) {
        if points.len() >= 3 {
            let points = points.iter().map(|p| (p.0 as f32, p.1 as f32)).collect();
            self.items.push(Drawn::Polygon {
                points,
                color,
                alpha,
            });
        }
    }

    fn quad(&mut self, quad: &Quad) {
        self.items.push(Drawn::Quad(quad.clone()));
    }

    /// One lighting for the whole frame: the last given.
    fn light(&mut self, lighting: &Lighting) {
        self.lighting = lighting.clone();
    }
}

/// Splits a simple polygon (any winding) into triangles, by corner
/// indices, cutting off one convex corner with nothing inside it at a
/// time.
pub fn triangulate(points: &[(f32, f32)]) -> Vec<[usize; 3]> {
    let area: f32 = (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    let turn = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)) * area.signum()
    };
    let mut left: Vec<usize> = (0..points.len()).collect();
    let mut out = Vec::with_capacity(points.len().saturating_sub(2));
    while left.len() > 3 {
        let n = left.len();
        let ear = (0..n).find(|&i| {
            let [a, b, c] = [left[(i + n - 1) % n], left[i], left[(i + 1) % n]];
            let (pa, pb, pc) = (points[a], points[b], points[c]);
            turn(pa, pb, pc) > 0.0
                && left.iter().all(|&j| {
                    j == a
                        || j == b
                        || j == c
                        || turn(pa, pb, points[j]) < 0.0
                        || turn(pb, pc, points[j]) < 0.0
                        || turn(pc, pa, points[j]) < 0.0
                })
        });
        // Without an ear the polygon isn't simple; cut anywhere so it
        // still ends.
        let i = ear.unwrap_or(0);
        out.push([left[(i + n - 1) % n], left[i], left[(i + 1) % n]]);
        left.remove(i);
    }
    if left.len() == 3 {
        out.push([left[0], left[1], left[2]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, texels: &[Rgba]) -> RgbaImage {
        Pixels {
            width: w,
            height: h,
            data: texels.to_vec(),
        }
    }

    #[test]
    fn mipmaps_halve_down_to_one_texel() {
        let t = Texture::new(Pixels::new(8, 3), false);
        let sizes: Vec<_> = t.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(8, 3), (4, 1), (2, 1), (1, 1)]);
        let pixel_art = Texture::new(Pixels::new(8, 3), true);
        assert_eq!(pixel_art.levels.len(), 1);
        assert_ne!(t.id(), pixel_art.id());
    }

    #[test]
    fn mipmaps_ignore_the_colour_of_transparent_texels() {
        let red = [255, 0, 0, 255];
        let level = half(&image(2, 2, &[red, [0, 0, 255, 0], red, [0; 4]]));
        assert_eq!(level.data, [[255, 0, 0, 128]]);
        assert_eq!(half(&image(2, 1, &[[0; 4]; 2])).data, [[0; 4]]);
    }

    #[test]
    fn flips_map_back_to_the_source() {
        let size = (4.0, 2.0);
        assert_eq!(Flip::default().source((1.0, 0.5), size), (1.0, 0.5));
        let x = Flip {
            x: true,
            ..Flip::default()
        };
        assert_eq!(x.source((1.0, 0.5), size), (3.0, 0.5));
        let diagonal = Flip {
            diagonal: true,
            ..Flip::default()
        };
        assert_eq!(diagonal.size((4, 2)), (2, 4));
        assert_eq!(diagonal.source((0.5, 3.0), size), (3.0, 0.5));
    }

    fn area(points: &[(f32, f32)], triangles: &[[usize; 3]]) -> f32 {
        triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (points[a], points[b], points[c]);
                ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn triangulates_concave_polygons_either_way_round() {
        // An L shape, area 3.
        let mut l = vec![
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 1.0),
            (1.0, 1.0),
            (1.0, 2.0),
            (0.0, 2.0),
        ];
        assert_eq!(triangulate(&l).len(), 4);
        assert_eq!(area(&l, &triangulate(&l)), 3.0);
        l.reverse();
        assert_eq!(area(&l, &triangulate(&l)), 3.0);
        assert_eq!(triangulate(&l[..3]).len(), 1);
    }

    #[test]
    fn lights_by_the_key_ambient_and_nearby_lights() {
        let level = clamber_level::Lighting::default();
        let mut lighting = Lighting::new(&level, Vec::new());
        // Facing the viewer under the default lights: about as it is.
        let front = lighting.at([0.0, 0.0, 1.0], (0.0, 0.0));
        assert!(
            front.iter().all(|&c| (0.98..1.02).contains(&c)),
            "{front:?}"
        );
        // Facing away from the key light: ambient only.
        let away = lighting.at([1.0, 0.0, 0.0], (0.0, 0.0));
        assert_eq!(away, lighting.ambient);

        lighting.lights.push(PointLight {
            at: (100.0, 0.0),
            radius: 50.0,
            color: [2.0, 0.0, 0.0],
        });
        assert_eq!(
            lighting.at([1.0, 0.0, 0.0], (0.0, 0.0)),
            away,
            "out of reach"
        );
        let near = lighting.at([1.0, 0.0, 0.0], (75.0, 0.0));
        // Halfway in, with the light 25 across and 24 out.
        let expected = 2.0 * 0.5 * 0.5 * 25.0 / (25.0f32 * 25.0 + 24.0 * 24.0).sqrt();
        assert!((near[0] - away[0] - expected).abs() < 1e-5, "{near:?}");
        assert_eq!(near[1..], away[1..]);
        let many = Lighting::new(&level, vec![lighting.lights[0].clone(); 20]);
        assert_eq!(many.lights.len(), MAX_LIGHTS);
    }

    #[test]
    fn decodes_normals_into_view_axes_and_flips_them() {
        // Up and to the right, stored y up, at half alpha.
        let encoded = [0.5, 0.5, 0.25, 0.5];
        let n = normal(encoded, Flip::default()).unwrap();
        let s = 0.5f32.sqrt();
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-6);
        assert!(close(n, [s, -s, 0.0]), "{n:?}");
        let x = Flip {
            x: true,
            ..Flip::default()
        };
        assert!(close(normal(encoded, x).unwrap(), [-s, -s, 0.0]));
        let swapped = Flip {
            diagonal: true,
            ..Flip::default()
        };
        assert!(close(normal(encoded, swapped).unwrap(), [-s, s, 0.0]));
        let y = Flip {
            y: true,
            diagonal: true,
            ..Flip::default()
        };
        assert!(close(normal(encoded, y).unwrap(), [s, s, 0.0]));
        assert_eq!(normal([0.0; 4], Flip::default()), None);
    }

    #[test]
    fn records_in_view_units() {
        let mut list = DrawList::new((10, 5));
        list.fill_rect(0, 0, 3, 3, 1);
        list.clear(7);
        list.fill_rect(1, 2, 3, 0, 1);
        list.stroke_rect(1, 1, 2, 2, 2);
        list.shade(3, 128);
        assert_eq!(list.background, 7);
        assert_eq!(list.items.len(), 5);
        let Drawn::Rect { edges, alpha, .. } = list.items[4] else {
            panic!("expected the shade");
        };
        assert_eq!((edges, alpha), ([0.0, 0.0, 10.0, 5.0], 128));
    }
}
