//! The window's size and how often it's drawn: the parts of showing the
//! game that don't need a window, so they can be tested.

use std::time::Duration;

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
