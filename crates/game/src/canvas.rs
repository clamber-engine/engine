//! A software framebuffer at the game's internal resolution, and integer
//! scaling of it onto the window for when there's no GPU to draw it.

/// A colour as `0x00RRGGBB`, the layout the window surface takes.
pub type Color = u32;

/// Packs a colour from its channels.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// An image in screen pixels, drawn into each frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Row by row from the top left.
    pub pixels: Vec<Color>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Canvas {
        Canvas {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        }
    }

    pub fn clear(&mut self, color: Color) {
        self.pixels.fill(color);
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Color> {
        self.index(x, y).map(|i| self.pixels[i])
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let inside = (0..self.width as i32).contains(&x) && (0..self.height as i32).contains(&y);
        inside.then(|| (y as u32 * self.width + x as u32) as usize)
    }

    /// Sets one pixel; off-canvas pixels are ignored.
    pub fn plot(&mut self, x: i32, y: i32, color: Color) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = color;
        }
    }

    /// Mixes `color` into one pixel, `alpha` from 0 (no change) to 255.
    pub fn blend(&mut self, x: i32, y: i32, color: Color, alpha: u8) {
        if let Some(i) = self.index(x, y) {
            self.pixels[i] = mix(self.pixels[i], color, alpha);
        }
    }

    /// Mixes `color` into every pixel at `alpha`, to dim what's under
    /// a menu.
    pub fn shade(&mut self, color: Color, alpha: u8) {
        for pixel in &mut self.pixels {
            *pixel = mix(*pixel, color, alpha);
        }
    }

    /// Fills the rectangle from `(x, y)`, `w` by `h`, clipped to the
    /// canvas.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        let (x0, x1) = (x.max(0), (x + w).min(self.width as i32));
        let (y0, y1) = (y.max(0), (y + h).min(self.height as i32));
        if x0 >= x1 {
            return;
        }
        for row in y0..y1 {
            let start = (row as u32 * self.width) as usize;
            self.pixels[start + x0 as usize..start + x1 as usize].fill(color);
        }
    }

    /// The outline of a rectangle, one pixel thick, inside its bounds.
    pub fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.fill_rect(x, y, w, 1, color);
        self.fill_rect(x, y + h - 1, w, 1, color);
        self.fill_rect(x, y, 1, h, color);
        self.fill_rect(x + w - 1, y, 1, h, color);
    }

    /// A one-pixel line between two points in screen coordinates, clipped
    /// to the canvas first so far-off geometry costs nothing.
    pub fn line(&mut self, a: (f64, f64), b: (f64, f64), color: Color) {
        let Some((a, b)) = clip(a, b, (self.width as f64, self.height as f64)) else {
            return;
        };
        let (x0, y0) = (a.0.floor() as i32, a.1.floor() as i32);
        let (x1, y1) = (b.0.floor() as i32, b.1.floor() as i32);
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = ((x1 - x0).signum(), (y1 - y0).signum());
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.plot(x, y, color);
            if (x, y) == (x1, y1) {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Fills a simple polygon given in screen coordinates (any winding),
    /// mixing `color` in at `alpha`. A pixel is inside when its centre is.
    pub fn fill_polygon(&mut self, points: &[(f64, f64)], color: Color, alpha: u8) {
        if points.len() < 3 {
            return;
        }
        let top = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let bottom = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        let y0 = (top - 0.5).ceil().max(0.0) as i32;
        let y1 = ((bottom - 0.5).floor() as i32).min(self.height as i32 - 1);
        let mut crossings = Vec::new();
        for y in y0..=y1 {
            let cy = y as f64 + 0.5;
            crossings.clear();
            for (i, &(ax, ay)) in points.iter().enumerate() {
                let (bx, by) = points[(i + 1) % points.len()];
                if (ay <= cy) != (by <= cy) {
                    crossings.push(ax + (cy - ay) / (by - ay) * (bx - ax));
                }
            }
            crossings.sort_by(f64::total_cmp);
            for &[left, right] in crossings.as_chunks::<2>().0 {
                let x0 = (left - 0.5).ceil().max(0.0) as i32;
                let x1 = ((right - 0.5).floor() as i32).min(self.width as i32 - 1);
                for x in x0..=x1 {
                    self.blend(x, y, color, alpha);
                }
            }
        }
    }
}

/// The part of the segment `a`–`b` inside a `size` box with a pixel of
/// margin (Liang–Barsky), or `None` if it misses.
fn clip(a: (f64, f64), b: (f64, f64), size: (f64, f64)) -> Option<((f64, f64), (f64, f64))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a.0 + 1.0),
        (dx, size.0 + 1.0 - a.0),
        (-dy, a.1 + 1.0),
        (dy, size.1 + 1.0 - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            t0 = t0.max(q / p);
        } else {
            t1 = t1.min(q / p);
        }
    }
    let at = |t: f64| (a.0 + t * dx, a.1 + t * dy);
    (t0 <= t1).then(|| (at(t0), at(t1)))
}

fn mix(under: Color, over: Color, alpha: u8) -> Color {
    let a = alpha as u32;
    let channel = |shift: u32| {
        let (u, o) = ((under >> shift) & 0xff, (over >> shift) & 0xff);
        ((o * a + u * (255 - a) + 127) / 255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// Where the canvas goes in a window: the largest whole-number scale that
/// fits (at least 1), centred, with the rest left as borders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub scale: u32,
    pub x: u32,
    pub y: u32,
}

impl Fit {
    pub fn new(canvas: (u32, u32), window: (u32, u32)) -> Fit {
        let scale = (window.0 / canvas.0).min(window.1 / canvas.1).max(1);
        Fit {
            scale,
            x: window.0.saturating_sub(canvas.0 * scale) / 2,
            y: window.1.saturating_sub(canvas.1 * scale) / 2,
        }
    }
}

/// Scales `canvas` by whole pixels into a `width` by `height` window
/// buffer, nearest neighbour, with `border` around it.
pub fn present(canvas: &Canvas, out: &mut [Color], width: u32, height: u32, border: Color) {
    let fit = Fit::new((canvas.width, canvas.height), (width, height));
    let (w, h) = (width as usize, height as usize);
    let shown_w = (canvas.width * fit.scale).min(width) as usize;
    let shown_h = (canvas.height * fit.scale).min(height) as usize;
    let (left, top) = (fit.x as usize, fit.y as usize);
    let scale = fit.scale as usize;
    for (y, row) in out.chunks_exact_mut(w).take(h).enumerate() {
        if y < top || y >= top + shown_h {
            row.fill(border);
            continue;
        }
        let src = &canvas.pixels[(y - top) / scale * canvas.width as usize..];
        row[..left].fill(border);
        for (x, px) in row[left..left + shown_w].iter_mut().enumerate() {
            *px = src[x / scale];
        }
        row[left + shown_w..].fill(border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = rgb(255, 0, 0);

    #[test]
    fn fits_by_whole_pixels_and_centres() {
        assert_eq!(
            Fit::new((384, 216), (1920, 1080)),
            Fit {
                scale: 5,
                x: 0,
                y: 0
            }
        );
        assert_eq!(
            Fit::new((384, 216), (1280, 1024)),
            Fit {
                scale: 3,
                x: 64,
                y: 188
            }
        );
        assert_eq!(Fit::new((384, 216), (100, 100)).scale, 1);
    }

    #[test]
    fn presents_scaled_with_borders() {
        let mut canvas = Canvas::new(2, 2);
        canvas.plot(1, 0, RED);
        let mut out = vec![7; 6 * 5];
        present(&canvas, &mut out, 6, 5, 9);
        #[rustfmt::skip]
        assert_eq!(out, [
            9, 0, 0, RED, RED, 9,
            9, 0, 0, RED, RED, 9,
            9, 0, 0, 0, 0, 9,
            9, 0, 0, 0, 0, 9,
            9, 9, 9, 9, 9, 9,
        ]);
    }

    #[test]
    fn presents_into_a_window_smaller_than_the_canvas() {
        let canvas = Canvas::new(4, 4);
        let mut out = vec![7; 3 * 2];
        present(&canvas, &mut out, 3, 2, 9);
        assert_eq!(out, [0; 6]);
    }

    #[test]
    fn clips_rectangles() {
        let mut canvas = Canvas::new(4, 3);
        canvas.fill_rect(-2, 1, 4, 10, RED);
        canvas.fill_rect(10, 0, 4, 4, RED);
        let red: Vec<_> = (0..12).map(|i| canvas.pixels[i] == RED).collect();
        #[rustfmt::skip]
        assert_eq!(red, [
            false, false, false, false,
            true, true, false, false,
            true, true, false, false,
        ]);
    }

    #[test]
    fn draws_lines_including_both_ends() {
        let mut canvas = Canvas::new(5, 5);
        canvas.line((0.5, 0.5), (4.5, 2.5), RED);
        let lit: Vec<_> = (0..25)
            .filter(|&i| canvas.pixels[i] == RED)
            .map(|i| (i % 5, i / 5))
            .collect();
        assert_eq!(lit, [(0, 0), (1, 1), (2, 1), (3, 2), (4, 2)]);
        // Far off the canvas, and crossing it from far away.
        canvas.line((-1e9, -5.0), (-9e8, 1e9), RED);
        canvas.line((-1e9, 4.5), (1e9, 4.5), RED);
        assert!((0..5).all(|x| canvas.get(x, 4) == Some(RED)));
    }

    #[test]
    fn fills_polygons_by_pixel_centres() {
        let mut canvas = Canvas::new(4, 4);
        canvas.fill_polygon(&[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)], RED, 255);
        let lit: Vec<_> = (0..16)
            .filter(|&i| canvas.pixels[i] == RED)
            .map(|i| (i % 4, i / 4))
            .collect();
        assert_eq!(lit, [(1, 1), (2, 1), (1, 2), (2, 2)]);
    }

    #[test]
    fn blends() {
        assert_eq!(mix(rgb(0, 0, 0), rgb(255, 100, 2), 255), rgb(255, 100, 2));
        assert_eq!(mix(rgb(0, 0, 0), rgb(255, 100, 2), 0), 0);
        assert_eq!(mix(rgb(0, 0, 0), rgb(255, 100, 2), 128), rgb(128, 50, 1));
    }
}
