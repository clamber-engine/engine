//! Text in a vector font, drawn as textured quads so it stays sharp at
//! any window size, with a dark outline that falls down and right like a
//! shadow so it reads over any background.
//!
//! The font is DejaVu Sans Condensed Bold (`content/base/fonts`). Its
//! glyphs are rasterised once, large, into an atlas with mipmaps: the
//! GPU draws them at the window's resolution and the canvas at its own.
//! Capitals are [`GLYPH`]`.1` units tall, the top of a line of text is
//! the top of its capitals, and characters without a glyph draw as `?`.
//! A few symbols are sized to fit [`ADVANCE`], to use as icons in text:
//! `♥`, `◆`, `★`, `●` and `○`. The HUD's own icons are pre-rendered (see
//! `icons`).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use clamber_sprite::{Pixels, Rect};

use crate::canvas::{Color, rgb};
use crate::surface::{Flip, Quad, Surface, Texture};

/// A typical character's size, in view units: digits are this wide and
/// capitals this tall.
pub const GLYPH: (i32, i32) = (5, 7);
/// A typical character's width, for laying out by character counts.
/// Digits and the HUD icons are exactly this wide.
pub const ADVANCE: i32 = 6;
/// From one line's top to the next.
pub const LINE: i32 = 9;

/// The outline under text.
pub const SHADOW: Color = rgb(0x10, 0x10, 0x18);

const FONT: &[u8] = include_bytes!("../../../content/base/fonts/DejaVuSansCondensed-Bold.ttf");

/// Atlas texels per view unit: enough for 4K, where a unit is 10 pixels.
const TEXELS: f32 = 10.0;
/// How far the outline reaches past a glyph's edge, in view units.
const OUTLINE: f32 = 0.7;
/// How far down and right the outline is drawn, in view units, so it
/// also reads as a shadow.
const SHADOW_OFFSET: f32 = 0.4;
/// Space around each atlas cell, in texels, so smaller mipmaps of one
/// glyph don't pick up its neighbours.
const PAD: u32 = 16;
const ATLAS_WIDTH: u32 = 2048;

/// Drawn smaller than the font has them, to fit [`ADVANCE`].
const ICONS: [char; 5] = ['♥', '◆', '★', '●', '○'];

/// One character of the font.
struct Glyph {
    /// From this character's pen position to the next, in view units.
    advance: f32,
    /// Where it is in the atlas, unless it draws nothing (a space).
    cell: Option<Cell>,
}

/// A glyph's two images in the atlas, both the same size: its shape, and
/// its outline.
struct Cell {
    fill: Rect,
    outline: Rect,
    /// The images' left and top edges from the pen position and the top
    /// of the capitals, and their size, in view units.
    left: f32,
    top: f32,
    size: (f32, f32),
}

struct Font {
    atlas: Arc<Texture>,
    glyphs: HashMap<char, Glyph>,
}

fn font() -> &'static Font {
    static FONT_ATLAS: OnceLock<Font> = OnceLock::new();
    FONT_ATLAS.get_or_init(Font::build)
}

impl Font {
    fn build() -> Font {
        let font = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default())
            .expect("the bundled font loads");
        // The size that makes capitals GLYPH.1 units tall.
        let reference = 100.0;
        let cap = font.metrics('H', reference).height as f32;
        let px = reference * GLYPH.1 as f32 * TEXELS / cap;
        let cap = font.metrics('H', px).height as f32;

        let reach = (OUTLINE * TEXELS).ceil() as usize;
        let mut images = Vec::new();
        let mut glyphs = HashMap::new();
        for c in charset().filter(|&c| font.has_glyph(c) || c == ' ') {
            let mut size = px;
            if ICONS.contains(&c) {
                size *= ADVANCE as f32 * TEXELS / font.metrics(c, px).advance_width;
            }
            let (m, coverage) = font.rasterize(c, size);
            let advance = match ICONS.contains(&c) {
                true => ADVANCE as f32,
                false => m.advance_width / TEXELS,
            };
            if m.width == 0 || m.height == 0 {
                glyphs.insert(
                    c,
                    Glyph {
                        advance,
                        cell: None,
                    },
                );
                continue;
            }
            let (w, h) = (m.width + 2 * reach, m.height + 2 * reach);
            let mut fill = vec![0u8; w * h];
            for y in 0..m.height {
                let row = (y + reach) * w + reach;
                fill[row..row + m.width].copy_from_slice(&coverage[y * m.width..][..m.width]);
            }
            let outline = dilate(&fill, w, h, OUTLINE * TEXELS);
            // Icons sit in the middle of the capitals' height.
            let bottom = match ICONS.contains(&c) {
                true => (cap + m.height as f32) / 2.0,
                false => cap - m.ymin as f32,
            };
            let top = bottom - m.height as f32 - reach as f32;
            let left = m.xmin as f32 - reach as f32;
            images.push((c, (w, h), fill, outline, (left, top)));
            glyphs.insert(
                c,
                Glyph {
                    advance,
                    cell: None,
                },
            );
        }

        // Shelves of cells, each starting on a multiple of PAD.
        let slot = |n: usize| (n as u32).div_ceil(PAD) * PAD + PAD;
        let (mut x, mut y, mut shelf) = (0, 0, 0);
        let mut places = Vec::new();
        for (_, (w, h), ..) in &images {
            for _ in 0..2 {
                if x + slot(*w) > ATLAS_WIDTH {
                    (x, y, shelf) = (0, y + shelf, 0);
                }
                places.push((x, y));
                x += slot(*w);
                shelf = shelf.max(slot(*h));
            }
        }
        let height = (y + shelf).div_ceil(256) * 256;
        let mut atlas = Pixels::new(ATLAS_WIDTH, height);
        for (k, (c, (w, h), fill, outline, (left, top))) in images.into_iter().enumerate() {
            let mut rects =
                [fill, outline]
                    .into_iter()
                    .zip(&places[2 * k..])
                    .map(|(image, &(x, y))| {
                        for (i, &a) in image.iter().enumerate() {
                            atlas.set(x + (i % w) as u32, y + (i / w) as u32, [255, 255, 255, a]);
                        }
                        Rect {
                            x,
                            y,
                            w: w as u32,
                            h: h as u32,
                        }
                    });
            let (fill, outline) = (rects.next().unwrap(), rects.next().unwrap());
            glyphs.get_mut(&c).unwrap().cell = Some(Cell {
                fill,
                outline,
                left: left / TEXELS,
                top: top / TEXELS,
                size: (w as f32 / TEXELS, h as f32 / TEXELS),
            });
        }
        Font {
            atlas: Texture::new(atlas, false),
            glyphs,
        }
    }

    fn glyph(&self, c: char) -> &Glyph {
        self.glyphs
            .get(&c)
            .or_else(|| self.glyphs.get(&'?'))
            .expect("the font has '?'")
    }

    /// How wide `line` is, in view units.
    fn width(&self, line: &str) -> f32 {
        line.chars().map(|c| self.glyph(c).advance).sum()
    }
}

/// The characters put in the atlas: printable ASCII, Latin-1, some
/// punctuation, arrows and the HUD icons.
fn charset() -> impl Iterator<Item = char> {
    (' '..='~')
        .chain('\u{a1}'..='\u{ff}')
        .chain("…–—‘’“”•←↑→↓".chars())
        .chain(ICONS)
}

/// `coverage`, `w` by `h`, grown by `radius` texels: each texel covered
/// as far as it is within `radius` of a texel at least half covered,
/// smoothed at the edge, and never less than it was.
fn dilate(coverage: &[u8], w: usize, h: usize, radius: f32) -> Vec<u8> {
    // Distance to the nearest half-covered texel, by two chamfer passes.
    let far = f32::MAX / 2.0;
    let mut distance: Vec<f32> = coverage
        .iter()
        .map(|&a| if a >= 128 { 0.0 } else { far })
        .collect();
    let diagonal = std::f32::consts::SQRT_2;
    let forward = [
        (-1, -1, diagonal),
        (0, -1, 1.0),
        (1, -1, diagonal),
        (-1, 0, 1.0),
    ];
    let mut pass = |order: &mut dyn Iterator<Item = usize>, sign: isize| {
        for i in order {
            let (x, y) = ((i % w) as isize, (i / w) as isize);
            for &(dx, dy, cost) in &forward {
                let (nx, ny) = (x + sign * dx, y + sign * dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                    let near = distance[ny as usize * w + nx as usize] + cost;
                    if near < distance[i] {
                        distance[i] = near;
                    }
                }
            }
        }
    };
    pass(&mut (0..w * h), 1);
    pass(&mut (0..w * h).rev(), -1);
    distance
        .iter()
        .zip(coverage)
        .map(|(&d, &a)| {
            let grown = ((radius + 0.5 - d).clamp(0.0, 1.0) * 255.0).round() as u8;
            grown.max(a)
        })
        .collect()
}

/// The size of `text` drawn without its outline: the widest line, and
/// every line down to the foot of the last line's capitals.
pub fn text_size(text: &str) -> (i32, i32) {
    let font = font();
    let width = text
        .lines()
        .map(|line| font.width(line).round() as i32)
        .max()
        .unwrap_or(0);
    let lines = text.lines().count() as i32;
    let height = if lines == 0 {
        0
    } else {
        (lines - 1) * LINE + GLYPH.1
    };
    (width, height)
}

/// `text` in lines of at most `width` characters, broken between words
/// where it can be.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        while !word.is_empty() {
            let used = line.chars().count();
            let space = usize::from(used > 0);
            if used + space + word.len() <= width {
                if space > 0 {
                    line.push(' ');
                }
                line.extend(word.drain(..));
            } else if used == 0 {
                // Longer than a line: break it.
                line.extend(word.drain(..width));
                lines.push(std::mem::take(&mut line));
            } else {
                lines.push(std::mem::take(&mut line));
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Draws `text` with its top left at `(x, y)` and its outline under it.
/// `\n` starts a new line.
pub fn draw_text(canvas: &mut dyn Surface, x: i32, y: i32, text: &str, color: Color) {
    let (x, y) = (x as f32, y as f32);
    draw_layer(
        canvas,
        (x + SHADOW_OFFSET, y + SHADOW_OFFSET),
        text,
        SHADOW,
        true,
    );
    draw_layer(canvas, (x, y), text, color, false);
}

/// Draws `text` centred on `centre_x`, its top at `y`.
pub fn draw_text_centred(
    canvas: &mut dyn Surface,
    centre_x: i32,
    y: i32,
    text: &str,
    color: Color,
) {
    draw_text(canvas, centre_x - text_size(text).0 / 2, y, text, color);
}

/// Draws every glyph's shape, or every glyph's outline, in `color`.
fn draw_layer(
    canvas: &mut dyn Surface,
    (x, y): (f32, f32),
    text: &str,
    color: Color,
    outline: bool,
) {
    let font = font();
    let flash = [(color >> 16) as u8, (color >> 8) as u8, color as u8, 255];
    for (row, line) in text.lines().enumerate() {
        let top = y + (row as i32 * LINE) as f32;
        let mut pen = x;
        for c in line.chars() {
            let glyph = font.glyph(c);
            if let Some(cell) = &glyph.cell {
                let (l, t) = (pen + cell.left, top + cell.top);
                canvas.quad(&Quad {
                    texture: font.atlas.clone(),
                    src: if outline { cell.outline } else { cell.fill },
                    dst: [l, t, l + cell.size.0, t + cell.size.1],
                    flip: Flip::default(),
                    flash: Some(flash),
                    opacity: 255,
                    lean: 0.0,
                    normals: None,
                });
            }
            pen += glyph.advance;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;

    const WHITE: Color = rgb(255, 255, 255);

    /// The smallest box holding every pixel that is exactly `color`, as
    /// left, top, right and bottom (exclusive).
    fn bounds(canvas: &Canvas, color: Color) -> Option<[i32; 4]> {
        let w = canvas.width as usize;
        let mut found: Option<[i32; 4]> = None;
        for (i, _) in canvas.pixels.iter().enumerate().filter(|p| *p.1 == color) {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            let b = found.get_or_insert([x, y, x + 1, y + 1]);
            *b = [b[0].min(x), b[1].min(y), b[2].max(x + 1), b[3].max(y + 1)];
        }
        found
    }

    #[test]
    fn capitals_are_glyph_tall_from_the_top() {
        let mut canvas = Canvas::scaled((20, 20), 8);
        draw_text(&mut canvas, 2, 3, "H", WHITE);
        let [_, top, _, bottom] = bounds(&canvas, WHITE).unwrap();
        assert!((top - 3 * 8).abs() <= 1, "{top}");
        assert!((bottom - top - GLYPH.1 * 8).abs() <= 2, "{}", bottom - top);
    }

    #[test]
    fn descenders_hang_below_the_capitals() {
        let mut canvas = Canvas::scaled((20, 20), 8);
        draw_text(&mut canvas, 2, 3, "Hp", WHITE);
        let [_, top, _, bottom] = bounds(&canvas, WHITE).unwrap();
        assert!((top - 3 * 8).abs() <= 1, "{top}");
        assert!(bottom - top > (GLYPH.1 + 1) * 8, "{}", bottom - top);
    }

    #[test]
    fn the_outline_falls_down_and_right() {
        let mut canvas = Canvas::scaled((20, 20), 8);
        draw_text(&mut canvas, 2, 3, "H", WHITE);
        let fill = bounds(&canvas, WHITE).unwrap();
        let shadow = bounds(&canvas, SHADOW).unwrap();
        assert!(shadow[0] < fill[0] && shadow[1] < fill[1]);
        assert!(shadow[2] - fill[2] > fill[0] - shadow[0]);
        assert!(shadow[3] - fill[3] > fill[1] - shadow[1]);
    }

    #[test]
    fn icons_sit_in_the_middle_of_the_capitals() {
        let mut canvas = Canvas::scaled((20, 20), 8);
        draw_text(&mut canvas, 2, 3, "◆", WHITE);
        let [_, top, _, bottom] = bounds(&canvas, WHITE).unwrap();
        let middle = (3 * 8) as f32 + GLYPH.1 as f32 * 8.0 / 2.0;
        assert!(((top + bottom) as f32 / 2.0 - middle).abs() <= 2.0);
    }

    #[test]
    fn digits_and_icons_are_an_advance_wide() {
        let font = font();
        for c in ('0'..='9').chain(ICONS) {
            let advance = font.glyph(c).advance;
            assert!((advance - ADVANCE as f32).abs() < 0.5, "{c:?}: {advance}");
        }
    }

    #[test]
    fn has_both_cases_and_falls_back_to_a_question_mark() {
        let font = font();
        for c in (' '..='~').chain(ICONS) {
            assert!(font.glyphs.contains_key(&c), "{c:?}");
        }
        assert!(font.width("a") < font.width("A"));
        assert!(std::ptr::eq(font.glyph('\u{4e00}'), font.glyph('?')));
    }

    #[test]
    fn measures_lines() {
        assert_eq!(text_size(""), (0, 0));
        assert_eq!(text_size("A").1, GLYPH.1);
        assert!(text_size("AB").0 > text_size("A").0);
        assert_eq!(text_size("AB\nCDE"), (text_size("CDE").0, LINE + GLYPH.1));
        assert_eq!(text_size("10").0, 2 * ADVANCE);
    }

    #[test]
    fn wraps_between_words() {
        assert_eq!(wrap("", 10), Vec::<String>::new());
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("  spaced   out ", 20), ["spaced out"]);
        assert_eq!(wrap("abcdefghij k", 4), ["abcd", "efgh", "ij k"]);
    }

    #[test]
    fn clips_at_the_edges() {
        let mut canvas = Canvas::new(4, 4);
        draw_text(&mut canvas, -3, -3, "W\nW", WHITE);
        draw_text_centred(&mut canvas, 2, 2, "WWWW", WHITE);
        assert!(canvas.pixels.iter().any(|&p| p > SHADOW));
    }
}
