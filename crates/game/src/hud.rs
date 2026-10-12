//! The heads-up display: lives and pickups, from the world's level flow.

use clamber_sim::{RELIC, World};

use crate::canvas::{Color, rgb};
use crate::surface::Surface;
use crate::text::{ADVANCE, draw_text};

const MARGIN: i32 = 6;
/// Space between one item and the next icon.
const GAP: i32 = 10;

const LIFE: Color = rgb(0xe8, 0x50, 0x60);
pub(crate) const GEM: Color = rgb(0x5a, 0xd8, 0xe8);
pub(crate) const RELIC_ICON: Color = rgb(0xf0, 0xc8, 0x40);
pub(crate) const COUNT: Color = rgb(0xf4, 0xf0, 0xe8);

/// Draws the HUD along the top left of the canvas: lives, gems, and
/// relics out of the level's total when it has any.
pub fn draw_hud(canvas: &mut dyn Surface, world: &World) {
    let flow = &world.flow;
    let relics = relic_total(world);
    let mut items = vec![
        ('♥', LIFE, flow.lives.to_string()),
        ('◆', GEM, flow.gems.to_string()),
    ];
    if relics > 0 {
        items.push(('★', RELIC_ICON, format!("{}/{relics}", flow.relics)));
    }

    let mut x = MARGIN;
    for (icon, color, count) in items {
        draw_text(canvas, x, MARGIN, &icon.to_string(), color);
        x += ADVANCE + 1;
        draw_text(canvas, x, MARGIN, &count, COUNT);
        x += count.chars().count() as i32 * ADVANCE + GAP;
    }
}

/// How many relics the level has to find.
pub(crate) fn relic_total(world: &World) -> usize {
    world
        .level
        .entities
        .iter()
        .filter(|e| e.kind == RELIC)
        .count()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_level::Level;
    use clamber_sim::generic_feel;

    use super::*;
    use crate::canvas::Canvas;

    fn world(entities: &str) -> World {
        let src = format!(
            r#"
            format = 1
            size = [400, 300]
            start = [100, 200]
            {entities}
            "#
        );
        let level = Level::from_toml_str(&src, "test").unwrap();
        World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
    }

    fn drawn(world: &World) -> Canvas {
        let mut canvas = Canvas::new(120, 24);
        draw_hud(&mut canvas, world);
        canvas
    }

    #[test]
    fn shows_lives_and_gems() {
        let mut world = world("");
        let before = drawn(&world);
        assert!(before.pixels.contains(&LIFE));
        assert!(before.pixels.contains(&GEM));
        assert!(!before.pixels.contains(&RELIC_ICON), "no relics to find");

        world.flow.lives -= 1;
        world.flow.gems += 10;
        let after = drawn(&world);
        let mut expected = Canvas::new(120, 24);
        draw_text(&mut expected, MARGIN, MARGIN, "♥", LIFE);
        draw_text(&mut expected, MARGIN + 7, MARGIN, "2", COUNT);
        draw_text(&mut expected, MARGIN + 23, MARGIN, "◆", GEM);
        draw_text(&mut expected, MARGIN + 30, MARGIN, "10", COUNT);
        assert_eq!(after, expected);
    }

    #[test]
    fn shows_relics_out_of_the_total() {
        let mut world = world(
            r#"
            [[entities]]
            type = "relic"
            at = [200, 200]
            [[entities]]
            type = "relic"
            at = [300, 200]
            "#,
        );
        world.flow.relics = 1;
        let canvas = drawn(&world);
        let mut expected = canvas.clone();
        // Relic icon after "♥3" and "◆0".
        let x = MARGIN + 7 + 6 + GAP + 7 + 6 + GAP;
        draw_text(&mut expected, x, MARGIN, "★", RELIC_ICON);
        draw_text(&mut expected, x + 7, MARGIN, "1/2", COUNT);
        assert_eq!(canvas, expected);
        assert!(canvas.pixels.contains(&RELIC_ICON));
    }
}
