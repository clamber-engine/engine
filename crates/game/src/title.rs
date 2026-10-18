//! The title screen's rendered art: the carved "CLAMBER" logo, and the
//! climber waving from a mossy boulder beside the menu. Both are
//! pre-rendered by `art/title.py` into `content/base/ui` and built into
//! the game like the HUD's icons, drawn unlit and filtered smoothly,
//! sharp at any window size.

use std::f32::consts::TAU;
use std::sync::OnceLock;

use crate::icons::UiArt;
use crate::surface::Surface;

const LOGO_SHEET: &str = include_str!("../../../content/base/ui/logo.json");
const LOGO_IMAGE: &[u8] = include_bytes!("../../../content/base/ui/logo.png");
const SCENE_SHEET: &str = include_str!("../../../content/base/ui/title.json");
const SCENE_IMAGE: &[u8] = include_bytes!("../../../content/base/ui/title.png");

/// How wide the logo is drawn, and its top, in view units: clear of
/// the HUD's top row, which shows over the level behind the title.
pub const LOGO_WIDTH: f32 = 170.0;
pub const LOGO_TOP: f32 = 16.0;
/// Frames the logo takes to drop into place the first time the title
/// shows, and how far above its place it starts, in view units.
const INTRO: u32 = 30;
const DROP: f32 = 16.0;
/// Frames the logo takes to bob gently up and down, and how far.
const BOB: u32 = 240;
const BOB_HEIGHT: f32 = 0.8;

/// View units per scene texel: the scene is drawn half as big again as
/// the climber in play (`SCENE_SCALE` in `art/title.py` is 15).
const SCENE_SCALE: f32 = 0.1;
/// Where the climber's feet are, in view units from the view's right
/// edge and top: beside the title's menu, the boulder clear of the
/// bottom.
const SCENE_FROM_RIGHT: f32 = 62.0;
const SCENE_FEET: f32 = 174.0;

fn logo() -> &'static UiArt {
    static ART: OnceLock<UiArt> = OnceLock::new();
    ART.get_or_init(|| UiArt::load(LOGO_SHEET, LOGO_IMAGE, "ui/logo.json"))
}

fn scene() -> &'static UiArt {
    static ART: OnceLock<UiArt> = OnceLock::new();
    ART.get_or_init(|| UiArt::load(SCENE_SHEET, SCENE_IMAGE, "ui/title.json"))
}

/// Draws the logo centred on `centre_x`, `age` frames after the title
/// first showed: dropping in and fading in at first, then bobbing.
pub fn draw_logo(surface: &mut dyn Surface, centre_x: f32, age: u32) {
    let art = logo();
    let frame = art.frame_at("logo", 0);
    let t = (age.min(INTRO) as f32 / INTRO as f32).min(1.0);
    // Eased out, overshooting a little before it settles.
    let s = 1.70158;
    let u = t - 1.0;
    let eased = 1.0 + (s + 1.0) * u * u * u + s * u * u;
    let bob = BOB_HEIGHT * (age as f32 * TAU / BOB as f32).sin();
    let top = LOGO_TOP - DROP * (1.0 - eased) + bob;
    let k = LOGO_WIDTH / frame.rect.w as f32;
    let (w, h) = (frame.rect.w as f32 * k, frame.rect.h as f32 * k);
    let left = centre_x - w / 2.0;
    let opacity = (255.0 * (t * 2.5).min(1.0)) as u8;
    art.draw(surface, frame.rect, [left, top, left + w, top + h], opacity);
}

/// Draws the climber waving from the boulder, beside the menu, `age`
/// frames into the wave.
pub fn draw_scene(surface: &mut dyn Surface, age: u32) {
    let art = scene();
    let feet = (surface.view().0 as f32 - SCENE_FROM_RIGHT, SCENE_FEET);
    for frame in [art.frame_at("rock", 0), art.frame_at("wave", age)] {
        let k = SCENE_SCALE;
        let left = feet.0 + frame.offset.0 as f32 * k;
        let top = feet.1 + frame.offset.1 as f32 * k;
        let (w, h) = (frame.rect.w as f32 * k, frame.rect.h as f32 * k);
        art.draw(surface, frame.rect, [left, top, left + w, top + h], 255);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::VIEW;
    use crate::canvas::{Canvas, rgb};

    const BG: crate::canvas::Color = rgb(1, 2, 3);

    /// The part of `canvas` drawn over, in view units: left, top, right
    /// and bottom.
    fn drawn(canvas: &Canvas, scale: usize) -> [usize; 4] {
        let w = canvas.width as usize;
        let mut edges: Option<[usize; 4]> = None;
        for (i, &p) in canvas.pixels.iter().enumerate() {
            if p == BG {
                continue;
            }
            let (x, y) = (i % w / scale, i / w / scale);
            let e = edges.get_or_insert([x, y, x, y]);
            *e = [e[0].min(x), e[1].min(y), e[2].max(x), e[3].max(y)];
        }
        edges.expect("draws")
    }

    fn logo_at(age: u32) -> Canvas {
        let mut canvas = Canvas::scaled(VIEW, 2);
        canvas.clear(BG);
        draw_logo(&mut canvas, VIEW.0 as f32 / 2.0, age);
        canvas
    }

    #[test]
    fn the_logo_settles_at_the_top_middle() {
        let [l, t, r, b] = drawn(&logo_at(INTRO), 2);
        let middle = (l + r) / 2;
        assert!((middle as i32 - VIEW.0 as i32 / 2).abs() <= 1, "{l}..{r}");
        assert!((r - l) as f32 >= LOGO_WIDTH - 2.0, "{l}..{r}");
        assert!(t >= 14 && b <= 56, "{t}..{b}");
    }

    #[test]
    fn the_logo_drops_in_once() {
        let start = logo_at(1);
        let [_, t, _, _] = drawn(&start, 2);
        let [_, settled, _, _] = drawn(&logo_at(INTRO), 2);
        assert!(t < settled, "starts above: {t} vs {settled}");
        // Long after, it only bobs a little.
        let [_, later, _, _] = drawn(&logo_at(INTRO + 5000), 2);
        assert!(later.abs_diff(settled) <= 1);
    }

    #[test]
    fn the_scene_waves_beside_the_menu_inside_the_view() {
        let draw = |age| {
            let mut canvas = Canvas::scaled(VIEW, 2);
            canvas.clear(BG);
            draw_scene(&mut canvas, age);
            canvas
        };
        let first = draw(0);
        let [l, t, r, b] = drawn(&first, 2);
        assert!(l > 260 && r < VIEW.0 as usize, "{l}..{r}");
        assert!(t > 100 && b < VIEW.1 as usize, "{t}..{b}");
        assert_ne!(draw(8), first, "waves");
        assert_eq!(draw(32), first, "and round again");
    }
}
