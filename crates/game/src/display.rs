//! The window's size and how often it's drawn: the parts of showing the
//! game that don't need a window, so they can be tested.

use std::sync::Arc;
use std::time::Duration;

use clamber_import::{Manifest, Widescreen};
use clamber_pack::{Feel, Param};
use clamber_sim::World;

use crate::camera::VIEW;
use crate::canvas::Fit;

/// The window's size when nothing else decides it, in physical pixels.
pub const START_SIZE: (u32, u32) = (1280, 720);

/// Frames drawn a second, at most, without vsync.
pub const FRAME_CAP: u32 = 240;

/// Frames drawn a second behind a menu or while the window isn't
/// focused, when nothing moves quickly.
pub const IDLE_RATE: u32 = 30;

/// A window's size in physical pixels, as a share of the screen's, that
/// counts as a comfortable fit.
const MOST: f64 = 0.8;
const FITS: f64 = 0.9;

/// The size to open the window at on a screen of `screen` physical
/// pixels, if it's known: the size the player `saved` if it still fits,
/// or else [`START_SIZE`], or larger to fill most of a large screen, at
/// the shape of `view`.
pub fn start_size(
    saved: Option<[u32; 2]>,
    screen: Option<(u32, u32)>,
    view: (u32, u32),
) -> (u32, u32) {
    let Some(screen) = screen.filter(|&(w, h)| w > 0 && h > 0) else {
        return saved.map_or(START_SIZE, |[w, h]| (w, h));
    };
    let fits = |(w, h): (u32, u32)| {
        w as f64 <= screen.0 as f64 * FITS && h as f64 <= screen.1 as f64 * FITS
    };
    if let Some([w, h]) = saved
        && fits((w, h))
    {
        return (w, h);
    }
    let scale =
        (screen.0 as f64 * MOST / view.0 as f64).min(screen.1 as f64 * MOST / view.1 as f64);
    let most = (
        (view.0 as f64 * scale) as u32,
        (view.1 as f64 * scale) as u32,
    );
    if fits(START_SIZE) && START_SIZE.0 > most.0 {
        START_SIZE
    } else {
        most
    }
}

/// Where a `canvas` goes in a `window`: as large as fits at its own
/// shape, centred, as `(x, y, width, height)` in window pixels.
pub fn letterbox(canvas: (u32, u32), window: (u32, u32)) -> (f32, f32, f32, f32) {
    let scale = (window.0 as f32 / canvas.0 as f32).min(window.1 as f32 / canvas.1 as f32);
    let (w, h) = (canvas.0 as f32 * scale, canvas.1 as f32 * scale);
    (
        (window.0 as f32 - w) / 2.0,
        (window.1 as f32 - h) / 2.0,
        w,
        h,
    )
}

/// Where a `canvas` of pixel art goes in a `window`: scaled by the
/// largest whole number that fits, centred, as `(x, y, width, height)` in
/// window pixels. A window too small for it at all gets
/// [`letterbox`]'s fit instead.
pub fn pixel_fit(canvas: (u32, u32), window: (u32, u32)) -> (f32, f32, f32, f32) {
    if window.0 < canvas.0 || window.1 < canvas.1 {
        return letterbox(canvas, window);
    }
    let fit = Fit::new(canvas, window);
    let size = |n: u32| (n * fit.scale) as f32;
    (fit.x as f32, fit.y as f32, size(canvas.0), size(canvas.1))
}

/// How a game is drawn: how much of the world the view shows, and how
/// it's scaled to the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// The view's size in world units.
    pub view: (u32, u32),
    /// Drawn as pixel art: the view is drawn at one pixel per unit, then
    /// scaled up by a whole number with nearest-neighbour filtering and
    /// letterboxed ([`pixel_fit`]). Otherwise the view is drawn at the
    /// window's own resolution ([`letterbox`]), and each texture is
    /// filtered as it says.
    pub pixel_art: bool,
    /// How it plays when `view` is wider than its movement's own view.
    pub widescreen: Widescreen,
}

impl Look {
    /// The bundled art's look: a [`VIEW`] drawn at the window's
    /// resolution, in wide activation.
    pub const RENDERED: Look = Look {
        view: VIEW,
        pixel_art: false,
        widescreen: Widescreen::WideActivation,
    };

    /// The look an imported pack asks for in its manifest. A pack that
    /// doesn't give its view's size gets [`VIEW`], and one that doesn't
    /// say how it plays wide gets original activation.
    pub fn of(manifest: &Manifest) -> Look {
        Look {
            view: manifest.view.map_or(VIEW, |[w, h]| (w, h)),
            pixel_art: manifest.pixel_art,
            widescreen: manifest.widescreen.unwrap_or_default(),
        }
    }

    /// The look `world` is drawn with: in original framing, as wide as
    /// its movement's view, if it has one and that's narrower, and as
    /// high as this look's (#194).
    pub fn framing(self, world: &World) -> Look {
        let width = world
            .view
            .map(|_| world.feel.world[Param::ViewWidth].floor());
        match (self.widescreen, width) {
            (Widescreen::OriginalFraming, Some(width)) if width > 0 => Look {
                view: (self.view.0.min(width as u32), self.view.1),
                ..self
            },
            _ => self,
        }
    }

    /// The window things appear, act and are removed by when `feel` is
    /// played with this look ([`Feel::window`]): in wide activation, as
    /// wide as this look's view, if the movement has a view of its own
    /// and that's narrower. It's fixed by the look, so it doesn't change
    /// when the window on screen is resized.
    pub fn window(self, feel: &Feel) -> Option<u32> {
        let picture = feel.world[Param::ViewWidth].floor();
        (self.widescreen == Widescreen::WideActivation
            && picture > 0
            && i64::from(self.view.0) > picture)
            .then_some(self.view.0)
    }

    /// `feel` with the window this look plays it in.
    pub fn feel(self, feel: &Arc<Feel>) -> Arc<Feel> {
        let window = self.window(feel);
        if feel.window == window {
            feel.clone()
        } else {
            Arc::new(Feel {
                window,
                ..Feel::clone(feel)
            })
        }
    }
}

impl Default for Look {
    fn default() -> Look {
        Look::RENDERED
    }
}

/// The least time between drawing frames. With vsync during play it's
/// none, as presenting waits for the display; without, it's
/// [`FRAME_CAP`]'s. Behind a menu or out of focus (not `active`), it's
/// [`IDLE_RATE`]'s.
pub fn frame_interval(vsync: bool, active: bool) -> Duration {
    let per_second = |n: u32| Duration::from_secs(1) / n;
    match (active, vsync) {
        (false, _) => per_second(IDLE_RATE),
        (true, true) => Duration::ZERO,
        (true, false) => per_second(FRAME_CAP),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: (u32, u32) = (384, 216);

    #[test]
    fn opens_at_720p_or_most_of_a_large_screen() {
        assert_eq!(start_size(None, Some((1920, 1080)), VIEW), (1536, 864));
        assert_eq!(start_size(None, Some((1600, 900)), VIEW), (1280, 720));
        assert_eq!(start_size(None, Some((3840, 2160)), VIEW), (3072, 1728));
        assert_eq!(start_size(None, None, VIEW), START_SIZE);
    }

    #[test]
    fn shrinks_to_fit_a_small_screen() {
        assert_eq!(start_size(None, Some((1366, 768)), VIEW), (1092, 614));
        assert_eq!(start_size(None, Some((1024, 768)), VIEW), (819, 460));
    }

    #[test]
    fn keeps_the_saved_size_if_it_fits() {
        let saved = Some([1000, 700]);
        assert_eq!(start_size(saved, Some((1920, 1080)), VIEW), (1000, 700));
        assert_eq!(start_size(saved, None, VIEW), (1000, 700));
        let big = Some([3000, 1600]);
        assert_eq!(start_size(big, Some((1920, 1080)), VIEW), (1536, 864));
    }

    #[test]
    fn letterboxes_at_the_canvas_shape() {
        assert_eq!(letterbox(VIEW, (1280, 720)), (0.0, 0.0, 1280.0, 720.0));
        assert_eq!(letterbox(VIEW, (1280, 1000)), (0.0, 140.0, 1280.0, 720.0));
        assert_eq!(letterbox(VIEW, (2000, 720)), (360.0, 0.0, 1280.0, 720.0));
    }

    #[test]
    fn fits_pixel_art_by_whole_numbers() {
        // 1280 / 256 is 5, but 720 / 224 is only 3.2.
        assert_eq!(
            pixel_fit((256, 224), (1280, 720)),
            (256.0, 24.0, 768.0, 672.0)
        );
        assert_eq!(
            pixel_fit((256, 224), (1024, 896)),
            (0.0, 0.0, 1024.0, 896.0)
        );
        assert_eq!(pixel_fit((256, 224), (300, 224)), (22.0, 0.0, 256.0, 224.0));
        // Too small to show whole: shrunk to fit like everything else.
        assert_eq!(
            pixel_fit((256, 224), (128, 224)),
            letterbox((256, 224), (128, 224))
        );
    }

    #[test]
    fn draws_slowly_unless_playing() {
        assert_eq!(frame_interval(true, true), Duration::ZERO);
        assert_eq!(
            frame_interval(false, true),
            Duration::from_secs(1) / FRAME_CAP
        );
        assert_eq!(frame_interval(true, false), Duration::from_secs(1) / 30);
        assert_eq!(frame_interval(false, false), Duration::from_secs(1) / 30);
    }
}
