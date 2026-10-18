//! The HUD's icons: a life, a gem, a relic, and a hit point full and
//! lost, pre-rendered by `art/hud.py` into `content/base/ui` and built
//! into the game, like its font. They're drawn unlit and filtered
//! smoothly, sharp at any window size.

use std::sync::{Arc, OnceLock};

use clamber_sprite::{Sheet, SpriteSheet, decode_png};

use crate::surface::{Flip, Quad, Surface, Texture};

const SHEET: &str = include_str!("../../../content/base/ui/hud.json");
const IMAGE: &[u8] = include_bytes!("../../../content/base/ui/hud.png");

/// Each icon's square frame, in the sheet's world units, its origin in
/// the middle (`FRAME` in `art/hud.py`).
const FRAME: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Life,
    Gem,
    Relic,
    HitPoint,
    HitPointLost,
}

impl Icon {
    /// Its animation in the sheet.
    fn name(self) -> &'static str {
        match self {
            Icon::Life => "life",
            Icon::Gem => "gem",
            Icon::Relic => "relic",
            Icon::HitPoint => "hit-point",
            Icon::HitPointLost => "hit-point-lost",
        }
    }
}

struct Icons {
    sheet: Sheet,
    texture: Arc<Texture>,
}

fn icons() -> &'static Icons {
    static ICONS: OnceLock<Icons> = OnceLock::new();
    ICONS.get_or_init(|| {
        let sheet = Sheet::from_json_str(SHEET, "ui/hud.json").expect("the HUD sheet reads");
        let image = decode_png(IMAGE).expect("the HUD image decodes");
        let sprite = SpriteSheet::new(sheet, image).expect("the HUD image fits its sheet");
        Icons {
            texture: Texture::new(sprite.image.to_rgba(), sprite.sheet.pixel_art),
            sheet: sprite.sheet,
        }
    })
}

/// Draws `icon` to fit the square `size` units across with its top left
/// at `(x, y)`, centred in it as it was rendered.
pub fn draw_icon(surface: &mut dyn Surface, icon: Icon, (x, y): (f32, f32), size: f32) {
    let icons = icons();
    let animation = icons
        .sheet
        .animation(icon.name())
        .expect("every icon is in the HUD sheet");
    let frame = &icons.sheet.frames[animation.frames[0]];
    // View units per texel.
    let k = size / (FRAME * icons.sheet.scale as f32);
    let left = x + size / 2.0 + frame.offset.0 as f32 * k;
    let top = y + size / 2.0 + frame.offset.1 as f32 * k;
    surface.quad(&Quad {
        texture: icons.texture.clone(),
        src: frame.rect,
        dst: [
            left,
            top,
            left + frame.rect.w as f32 * k,
            top + frame.rect.h as f32 * k,
        ],
        flip: Flip::default(),
        flash: None,
        opacity: 255,
        normals: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Canvas, rgb};

    const ALL: [Icon; 5] = [
        Icon::Life,
        Icon::Gem,
        Icon::Relic,
        Icon::HitPoint,
        Icon::HitPointLost,
    ];

    #[test]
    fn each_icon_draws_inside_its_square() {
        let bg = rgb(1, 2, 3);
        for icon in ALL {
            let mut canvas = Canvas::scaled((40, 40), 4);
            canvas.clear(bg);
            draw_icon(&mut canvas, icon, (10.0, 10.0), 20.0);
            let w = canvas.width as usize;
            let mut drawn = 0;
            for (i, &p) in canvas.pixels.iter().enumerate() {
                if p == bg {
                    continue;
                }
                drawn += 1;
                let (px, py) = (i % w, i / w);
                assert!(
                    (40..120).contains(&px) && (40..120).contains(&py),
                    "{icon:?} at {px}, {py}"
                );
            }
            assert!(drawn > 40 * 40, "{icon:?} draws {drawn} pixels");
        }
    }

    #[test]
    fn icons_differ() {
        let draw = |icon| {
            let mut canvas = Canvas::scaled((10, 10), 4);
            draw_icon(&mut canvas, icon, (0.0, 0.0), 10.0);
            canvas
        };
        assert_ne!(draw(Icon::HitPoint), draw(Icon::HitPointLost));
        assert_ne!(draw(Icon::Life), draw(Icon::Gem));
    }
}
