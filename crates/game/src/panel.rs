//! The menus' rendered pieces: a panel of slate framed in wooden beams, a
//! wooden highlight behind the selected item, and a turning gem beside
//! it. They're pre-rendered by `art/menu.py` into `content/base/ui` and
//! built into the game like the HUD's icons, drawn unlit and filtered
//! smoothly, sharp at any window size.

use std::sync::OnceLock;

use clamber_sprite::Rect;

use crate::icons::UiArt;
use crate::surface::Surface;

const SHEET: &str = include_str!("../../../content/base/ui/menu.json");
const IMAGE: &[u8] = include_bytes!("../../../content/base/ui/menu.png");

/// The panel's corners, in texels: as much of each edge as is kept whole,
/// the beams and the moss over the corners (`BEAM` in `art/menu.py`, and
/// more).
const BORDER: u32 = 80;
/// View units per panel texel: a world unit of the art is a view unit.
const PANEL_SCALE: f32 = 0.1;
/// View units per cursor texel: the gem is about 7 units across.
const CURSOR_SCALE: f32 = 0.12;

/// From a panel's edge to what's in it, in view units, clear of the
/// beams.
pub const INSET: f32 = 6.0;

fn art() -> &'static UiArt {
    static ART: OnceLock<UiArt> = OnceLock::new();
    ART.get_or_init(|| UiArt::load(SHEET, IMAGE, "ui/menu.json"))
}

/// Draws a panel filling `dst` (left, top, right and bottom, in view
/// units): its corners as rendered, its edges and middle stretched.
pub fn draw_panel(surface: &mut dyn Surface, dst: [f32; 4], opacity: u8) {
    let art = art();
    let frame = art.frames("panel").next().expect("the panel has a frame");
    let border = BORDER as f32 * PANEL_SCALE;
    draw_sliced(
        surface,
        art,
        frame.rect,
        (BORDER, BORDER),
        dst,
        (border, border),
        opacity,
    );
}

/// Draws the highlight filling `dst`: its round ends as rendered, at
/// the height asked for, and its middle stretched between them.
pub fn draw_highlight(surface: &mut dyn Surface, dst: [f32; 4], opacity: u8) {
    let art = art();
    let src = art
        .frames("highlight")
        .next()
        .expect("the highlight has a frame")
        .rect;
    let end = src.h / 2;
    let k = (dst[3] - dst[1]) / src.h as f32;
    draw_sliced(
        surface,
        art,
        src,
        (end, 0),
        dst,
        (end as f32 * k, 0.0),
        opacity,
    );
}

/// Draws the cursor centred on `(x, y)`, turned as it is `age` frames
/// after it first showed.
pub fn draw_cursor(surface: &mut dyn Surface, (x, y): (f32, f32), age: u32) {
    let art = art();
    let frame = art.frame_at("cursor", age);
    let k = CURSOR_SCALE;
    let left = x + frame.offset.0 as f32 * k;
    let top = y + frame.offset.1 as f32 * k;
    let (w, h) = (frame.rect.w as f32 * k, frame.rect.h as f32 * k);
    art.draw(surface, frame.rect, [left, top, left + w, top + h], 255);
}

/// Draws `src` in nine pieces to fill `dst`: `border` texels on each
/// side (across and down) drawn `units` view units wide, as rendered,
/// and the rest stretched. A border of 0 slices nothing off that way.
/// Borders shrink to fit a `dst` too small for them.
fn draw_sliced(
    surface: &mut dyn Surface,
    art: &UiArt,
    src: Rect,
    border: (u32, u32),
    dst: [f32; 4],
    units: (f32, f32),
    opacity: u8,
) {
    let [left, top, right, bottom] = dst;
    let ux = units.0.min((right - left) / 2.0);
    let uy = units.1.min((bottom - top) / 2.0);
    let xs = [
        src.x,
        src.x + border.0,
        src.x + src.w - border.0,
        src.x + src.w,
    ];
    let ys = [
        src.y,
        src.y + border.1,
        src.y + src.h - border.1,
        src.y + src.h,
    ];
    let dxs = [left, left + ux, right - ux, right];
    let dys = [top, top + uy, bottom - uy, bottom];
    for row in 0..3 {
        for col in 0..3 {
            let (w, h) = (xs[col + 1] - xs[col], ys[row + 1] - ys[row]);
            if w == 0 || h == 0 || dxs[col + 1] <= dxs[col] || dys[row + 1] <= dys[row] {
                continue;
            }
            let piece = Rect {
                x: xs[col],
                y: ys[row],
                w,
                h,
            };
            let to = [dxs[col], dys[row], dxs[col + 1], dys[row + 1]];
            art.draw(surface, piece, to, opacity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Canvas, rgb};

    const BG: crate::canvas::Color = rgb(1, 2, 3);

    /// The part of `canvas` drawn over, as left, top, right and bottom
    /// pixels.
    fn drawn(canvas: &Canvas) -> Option<[usize; 4]> {
        let w = canvas.width as usize;
        let mut edges: Option<[usize; 4]> = None;
        for (i, &p) in canvas.pixels.iter().enumerate() {
            if p == BG {
                continue;
            }
            let (x, y) = (i % w, i / w);
            let e = edges.get_or_insert([x, y, x, y]);
            *e = [e[0].min(x), e[1].min(y), e[2].max(x), e[3].max(y)];
        }
        edges
    }

    #[test]
    fn panels_fill_what_they_are_given_at_any_size() {
        for (w, h) in [(40.0, 30.0), (200.0, 120.0), (10.0, 6.0)] {
            let mut canvas = Canvas::scaled((220, 140), 2);
            canvas.clear(BG);
            draw_panel(&mut canvas, [5.0, 5.0, 5.0 + w, 5.0 + h], 255);
            let [l, t, r, b] = drawn(&canvas).expect("draws");
            let near = |a: usize, b: f32| (a as f32 - b * 2.0).abs() <= 2.0;
            assert!(near(l, 5.0) && near(t, 5.0), "{w}x{h}: {l}, {t}");
            assert!(
                near(r + 1, 5.0 + w) && near(b + 1, 5.0 + h),
                "{w}x{h}: {r}, {b}"
            );
            // The middle is slate, filled in.
            let middle = canvas.pixels[(5 + h as usize) * canvas.width as usize + 5 + w as usize];
            assert_ne!(middle, BG, "{w}x{h}");
        }
    }

    #[test]
    fn highlights_keep_their_ends_round() {
        let mut canvas = Canvas::scaled((100, 20), 4);
        canvas.clear(BG);
        draw_highlight(&mut canvas, [10.0, 5.0, 90.0, 15.0], 255);
        let [l, t, r, b] = drawn(&canvas).expect("draws");
        assert!(l <= 42 && r >= 357 && t <= 22 && b >= 57, "{l} {t} {r} {b}");
        let w = canvas.width as usize;
        // Round ends: the corners of the box are left alone.
        assert_eq!(canvas.pixels[20 * w + 40], BG);
        assert_ne!(canvas.pixels[40 * w + 200], BG);
    }

    #[test]
    fn the_cursor_turns() {
        let draw = |age| {
            let mut canvas = Canvas::scaled((20, 20), 4);
            canvas.clear(BG);
            draw_cursor(&mut canvas, (10.0, 10.0), age);
            canvas
        };
        let first = draw(0);
        let [l, t, r, b] = drawn(&first).expect("draws");
        assert!(l >= 4 * 5 && r <= 4 * 15 && t >= 4 * 5 && b <= 4 * 15);
        assert_ne!(draw(12), first, "turned");
        assert_eq!(draw(16 * 3), first, "all the way round");
    }
}
