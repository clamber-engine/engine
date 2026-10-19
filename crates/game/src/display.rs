//! The window's size and how often it's drawn: the parts of showing the
//! game that don't need a window, so they can be tested.

use std::sync::Arc;
use std::time::Duration;

use clamber_import::{Manifest, Widescreen};
use clamber_pack::{Feel, Param};
use clamber_sim::World;
use serde::{Deserialize, Serialize};

use crate::camera::VIEW;
use crate::canvas::Fit;
use crate::prefs::FrameSync;

/// The window's size when nothing else decides it, in physical pixels.
pub const START_SIZE: (u32, u32) = (1280, 720);

/// Frames drawn a second, at most, without vsync.
pub const FRAME_CAP: u32 = 240;

/// The share of a VRR display's highest refresh rate that frames are
/// drawn at, so they never have to wait for a refresh and the display
/// keeps varying its own to match them.
const VRR_HEADROOM: f64 = 0.97;

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

/// How an imported pixel-art game's view is drawn, picked on the options
/// screen. Neither changes how it plays: only what's drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rendering {
    /// At the original's resolution, a pixel to a unit, then scaled up
    /// as one picture by a whole number ([`Look::pixel_art`]), so all of
    /// it, Clamber's own art too, is on the original's pixel grid.
    #[default]
    Original,
    /// At the window's resolution: the original's art scaled up sharp,
    /// and Clamber's own art at full detail.
    Enhanced,
}

impl Rendering {
    pub const ALL: [Rendering; 2] = [Rendering::Original, Rendering::Enhanced];

    /// Its name, as the options screen shows it.
    pub fn label(self) -> &'static str {
        match self {
            Rendering::Original => "Original",
            Rendering::Enhanced => "Enhanced",
        }
    }

    /// The other mode.
    pub fn next(self) -> Rendering {
        match self {
            Rendering::Original => Rendering::Enhanced,
            Rendering::Enhanced => Rendering::Original,
        }
    }
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

/// How frames are shown, once [`FrameSync`] is settled for the display
/// the window is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presenting {
    /// Each frame waits for the display's next refresh, and the steps
    /// follow the refreshes ([`Pace`](crate::Pace)).
    Vsync,
    /// Frames are timed by the game, just under the display's highest
    /// refresh rate ([`vrr_interval`]) or once a step, and presented
    /// without tearing; the display refreshes as each one arrives.
    Vrr,
    /// Frames don't wait for the display, and are capped at
    /// [`FRAME_CAP`] a second.
    Off,
}

impl Presenting {
    /// How `sync` presents on a display that can vary its refresh
    /// (`vrr_display`) or not: Auto is VRR on one that can, and vsync
    /// on others.
    pub fn of(sync: FrameSync, vrr_display: bool) -> Presenting {
        match sync {
            FrameSync::Auto if vrr_display => Presenting::Vrr,
            FrameSync::Auto | FrameSync::Vsync => Presenting::Vsync,
            FrameSync::Vrr => Presenting::Vrr,
            FrameSync::Off => Presenting::Off,
        }
    }
}

/// The time between frames with VRR on a display whose highest refresh
/// rate is `millihertz` / 1000 Hz: a little longer than a refresh
/// ([`VRR_HEADROOM`]), and no shorter than [`FRAME_CAP`]'s. Frames are
/// drawn between steps. `None` if the rate isn't known, or that would
/// be no faster than the steps, when instead a frame is drawn as each
/// step is due, showing that step.
pub fn vrr_interval(millihertz: Option<u32>) -> Option<Duration> {
    let rate = (millihertz? as f64 / 1000.0 * VRR_HEADROOM).min(FRAME_CAP as f64);
    (rate > clamber_sim::STEPS_PER_SECOND as f64).then(|| Duration::from_secs_f64(1.0 / rate))
}

/// The time frames are meant to be apart while playing, presented as
/// `presenting` says on a display that refreshes `millihertz` / 1000
/// times a second, if that's known.
pub fn expected_interval(presenting: Presenting, millihertz: Option<u32>) -> Option<Duration> {
    let step = Duration::from_secs(1) / clamber_sim::STEPS_PER_SECOND;
    match presenting {
        Presenting::Vsync => millihertz
            .filter(|&m| m > 0)
            .map(|m| Duration::from_secs_f64(1000.0 / m as f64)),
        Presenting::Vrr => Some(vrr_interval(millihertz).unwrap_or(step)),
        Presenting::Off => Some(Duration::from_secs(1) / FRAME_CAP),
    }
}

/// The least time between drawing frames. With vsync during play it's
/// none, as presenting waits for the display; with VRR, it's
/// [`vrr_interval`]'s, or a step's; without either, it's
/// [`FRAME_CAP`]'s. Behind a menu or out of focus (not `active`), it's
/// [`IDLE_RATE`]'s.
pub fn frame_interval(presenting: Presenting, active: bool, millihertz: Option<u32>) -> Duration {
    let per_second = |n: u32| Duration::from_secs(1) / n;
    match (active, presenting) {
        (false, _) => per_second(IDLE_RATE),
        (true, Presenting::Vsync) => Duration::ZERO,
        (true, Presenting::Vrr) => {
            vrr_interval(millihertz).unwrap_or_else(|| per_second(clamber_sim::STEPS_PER_SECOND))
        }
        (true, Presenting::Off) => per_second(FRAME_CAP),
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
        let hz = Some(60_000);
        assert_eq!(frame_interval(Presenting::Vsync, true, hz), Duration::ZERO);
        assert_eq!(
            frame_interval(Presenting::Off, true, hz),
            Duration::from_secs(1) / FRAME_CAP
        );
        for presenting in [Presenting::Vsync, Presenting::Vrr, Presenting::Off] {
            let idle = Duration::from_secs(1) / 30;
            assert_eq!(frame_interval(presenting, false, hz), idle);
        }
    }

    #[test]
    fn auto_sync_is_vrr_only_on_a_vrr_display() {
        assert_eq!(Presenting::of(FrameSync::Auto, true), Presenting::Vrr);
        assert_eq!(Presenting::of(FrameSync::Auto, false), Presenting::Vsync);
        assert_eq!(Presenting::of(FrameSync::Vsync, true), Presenting::Vsync);
        assert_eq!(Presenting::of(FrameSync::Vrr, false), Presenting::Vrr);
        assert_eq!(Presenting::of(FrameSync::Off, true), Presenting::Off);
    }

    #[test]
    fn vrr_draws_just_under_the_highest_refresh() {
        let fps = |m| 1.0 / vrr_interval(Some(m)).unwrap().as_secs_f64();
        assert!((fps(144_000) - 139.68).abs() < 0.01);
        assert!((fps(75_000) - 72.75).abs() < 0.01);
        assert_eq!(fps(360_000).round(), FRAME_CAP as f64);
        let vrr = |m| frame_interval(Presenting::Vrr, true, m);
        assert_eq!(vrr(Some(144_000)), vrr_interval(Some(144_000)).unwrap());
    }

    #[test]
    fn vrr_draws_once_a_step_unless_it_can_go_faster() {
        assert_eq!(vrr_interval(None), None);
        assert_eq!(vrr_interval(Some(60_000)), None);
        let step = Duration::from_secs(1) / 60;
        assert_eq!(frame_interval(Presenting::Vrr, true, None), step);
        assert_eq!(expected_interval(Presenting::Vrr, Some(60_000)), Some(step));
    }

    #[test]
    fn expects_frames_a_refresh_apart_with_vsync() {
        let at = |m| expected_interval(Presenting::Vsync, m);
        assert_eq!(
            at(Some(120_000)),
            Some(Duration::from_secs_f64(1.0 / 120.0))
        );
        assert_eq!(at(None), None);
        assert_eq!(
            expected_interval(Presenting::Off, None),
            Some(Duration::from_secs(1) / FRAME_CAP)
        );
    }
}
