//! The heads-up display: lives, pickups and hit points, from the world's
//! level flow, and the letter row while the world shows it. A world with a count
//! display shows the count and lives only when it says so.

use clamber_pack::Param;
use clamber_sim::{CountDisplay, LetterRow, LivesDisplay, RELIC, World};

use crate::canvas::{Color, rgb};
use crate::surface::Surface;
use crate::text::{ADVANCE, draw_text, draw_text_centred};

const MARGIN: i32 = 6;
/// Space between one item and the next icon.
const GAP: i32 = 10;

const LIFE: Color = rgb(0xe8, 0x50, 0x60);
pub(crate) const GEM: Color = rgb(0x5a, 0xd8, 0xe8);
pub(crate) const RELIC_ICON: Color = rgb(0xf0, 0xc8, 0x40);
pub(crate) const COUNT: Color = rgb(0xf4, 0xf0, 0xe8);
/// Hit points, the colour of the tonic that gives them back.
const HIT_POINT: Color = rgb(0x48, 0xd8, 0x68);

/// A letter's image in the letter row, px square.
const LETTER: i32 = 16;
/// The count display's collectible image, px square.
const IMAGE: i32 = 16;
const LETTER_BACK: Color = rgb(0xf0, 0xb0, 0x50);
const LETTER_TEXT: Color = rgb(0x30, 0x20, 0x10);

/// Draws the HUD along the top left of the canvas: lives, gems, hit
/// points when a life has more than one, and relics out of the level's
/// total when it has any. With a count
/// display, the count display and the lives display instead, when the
/// world shows them. Then the letter row, if the world shows it.
pub fn draw_hud(canvas: &mut dyn Surface, world: &World) {
    if world.feel.world[Param::CountShowFor].floor() > 0 {
        let left = picture_left(canvas, world);
        if let Some(count) = world.count_display() {
            draw_count(canvas, left, &count);
        }
        if let Some(lives) = world.lives_display() {
            draw_lives(canvas, left, &lives);
        }
    } else {
        draw_items(canvas, world);
    }
    if let Some(row) = world.letter_row() {
        draw_letter_row(canvas, world, &row);
    }
}

/// Lives, gems, hit points and relics in a row. Hit points are a pip
/// each, full for those left.
fn draw_items(canvas: &mut dyn Surface, world: &World) {
    let flow = &world.flow;
    let relics = relic_total(world);
    let mut items = vec![
        ("♥".to_string(), LIFE, flow.lives.to_string()),
        ("◆".to_string(), GEM, flow.gems.to_string()),
    ];
    if let Some((left, max)) = world.hit_points() {
        let pips = (0..max).map(|k| if k < left { '●' } else { '○' });
        items.push((pips.collect(), HIT_POINT, String::new()));
    }
    if relics > 0 {
        items.push((
            "★".to_string(),
            RELIC_ICON,
            format!("{}/{relics}", flow.relics),
        ));
    }

    let mut x = MARGIN;
    for (icon, color, count) in items {
        draw_text(canvas, x, MARGIN, &icon, color);
        x += icon.chars().count() as i32 * ADVANCE + 1;
        draw_text(canvas, x, MARGIN, &count, COUNT);
        x += count.chars().count() as i32 * ADVANCE + GAP;
    }
}

/// Where the world's picture's left edge is on the canvas: it's centred
/// across it when the world has a view narrower than it.
pub(crate) fn picture_left(canvas: &dyn Surface, world: &World) -> i32 {
    let picture = world.feel.world[Param::ViewWidth].floor();
    let width = i64::from(canvas.view().0);
    if picture > 0 && picture < width {
        ((width - picture) / 2) as i32
    } else {
        0
    }
}

/// Draws the count display with its picture's left edge at `left`. How
/// the image and digits are drawn isn't in the spec: the image is the
/// gem icon in its 16 px square, and each digit is drawn from its place.
fn draw_count(canvas: &mut dyn Surface, left: i32, count: &CountDisplay) {
    let (x, y) = count.image;
    draw_text_centred(canvas, left + x + IMAGE / 2, y + 4, "◆", GEM);
    let (x, y) = count.digits;
    for (k, digit) in count.number.to_string().chars().enumerate() {
        let x = left + x + count.gap * k as i32;
        draw_text(canvas, x, y + 4, &digit.to_string(), COUNT);
    }
}

/// Draws the lives display with its picture's left edge at `left`: the
/// life icon to the left of the number (the spec doesn't cover the
/// image), then each digit from its place.
fn draw_lives(canvas: &mut dyn Surface, left: i32, lives: &LivesDisplay) {
    let x = left + lives.x;
    draw_text(canvas, x - ADVANCE - 1, lives.y + 4, "♥", LIFE);
    for (k, digit) in lives.number.to_string().chars().enumerate() {
        let x = x + lives.gap * k as i32;
        draw_text(canvas, x, lives.y + 4, &digit.to_string(), COUNT);
    }
}

/// Draws the letters taken in their slots. The row's places are in the
/// world's picture, which is centred across the canvas when the world
/// has a view narrower than it. How the letters are drawn isn't in the
/// spec; each is its number on a plain square.
fn draw_letter_row(canvas: &mut dyn Surface, world: &World, row: &LetterRow) {
    let left = picture_left(canvas, world);
    for k in 0..row.count {
        if row.letters & (1 << k) == 0 {
            continue;
        }
        let x = left + row.x[usize::from(k)];
        canvas.fill_rect(x, row.y, LETTER, LETTER, LETTER_BACK);
        let label = (k + 1).to_string();
        draw_text_centred(canvas, x + LETTER / 2, row.y + 4, &label, LETTER_TEXT);
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
        let mut canvas = Canvas::scaled((120, 24), 4);
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
        world.flow.hits_taken = 1;
        let after = drawn(&world);
        let mut expected = Canvas::scaled((120, 24), 4);
        draw_text(&mut expected, MARGIN, MARGIN, "♥", LIFE);
        draw_text(&mut expected, MARGIN + 7, MARGIN, "2", COUNT);
        draw_text(&mut expected, MARGIN + 23, MARGIN, "◆", GEM);
        draw_text(&mut expected, MARGIN + 30, MARGIN, "10", COUNT);
        // Hit points after the gems: two left of three.
        draw_text(&mut expected, MARGIN + 52, MARGIN, "●●○", HIT_POINT);
        assert_eq!(after, expected);
    }

    #[test]
    fn shows_no_hit_points_when_every_hit_costs_a_life() {
        let level = Level::from_toml_str(
            "format = 1\nsize = [400, 300]\nstart = [100, 200]\n",
            "test",
        );
        let feel = generic_feel("classic-1").unwrap();
        let world = World::new(Arc::new(level.unwrap()), Arc::new(feel));
        assert!(!drawn(&world).pixels.contains(&HIT_POINT));
    }

    #[test]
    fn shows_the_letters_taken_while_the_row_is_shown() {
        let src = r#"
            format = 1
            size = [400, 300]
            start = [100, 200]
            [[collision]]
            shape = "rect"
            rect = [0, 200, 400, 100]
            [[entities]]
            type = "letter"
            at = [100, 190]
            params = { letter = 2 }
        "#;
        let level = Level::from_toml_str(src, "test").unwrap();
        let feel = generic_feel("classic-1").unwrap();
        let mut world = World::new(Arc::new(level), Arc::new(feel));
        let input = clamber_sim::InputFrame::single(Default::default());
        let mut canvas = Canvas::new(256, 64);
        for _ in 0..40 {
            clamber_sim::step(&mut world, &input);
        }
        draw_hud(&mut canvas, &world);
        // Letter 2's square, at x 108 and y 12 in the picture.
        assert_eq!(canvas.get(109, 13), Some(LETTER_BACK));
        assert_ne!(
            canvas.get(85, 13),
            Some(LETTER_BACK),
            "letter 1 isn't taken"
        );

        for _ in 0..100 {
            clamber_sim::step(&mut world, &input);
        }
        let mut canvas = Canvas::new(256, 64);
        draw_hud(&mut canvas, &world);
        assert!(!canvas.pixels.contains(&LETTER_BACK), "the row has gone");
    }

    #[test]
    fn a_count_display_shows_the_count_only_when_the_world_shows_it() {
        let src = r#"
            format = 1
            size = [400, 300]
            start = [100, 200]
            [[collision]]
            shape = "rect"
            rect = [0, 200, 400, 100]
        "#;
        let level = Level::from_toml_str(src, "test").unwrap();
        let feel = generic_feel("classic-1").unwrap();
        let mut world = World::new(Arc::new(level), Arc::new(feel));
        let input = clamber_sim::InputFrame::single(Default::default());
        clamber_sim::step(&mut world, &input);
        let mut canvas = Canvas::new(256, 64);
        draw_hud(&mut canvas, &world);
        assert!(
            canvas.pixels.iter().all(|&p| p == canvas.pixels[0]),
            "nothing is shown when the level begins"
        );

        // The count goes up by one: the display shows the image and 1.
        world.flow.gems = 1;
        clamber_sim::step(&mut world, &input);
        let mut canvas = Canvas::new(256, 64);
        draw_hud(&mut canvas, &world);
        assert!(canvas.pixels.contains(&GEM));
        assert!(!canvas.pixels.contains(&LIFE), "no lives display");
        let mut expected = Canvas::new(256, 64);
        draw_count(&mut expected, 0, &world.count_display().unwrap());
        assert_eq!(canvas, expected);
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
        let mut expected = Canvas::scaled((120, 24), 4);
        draw_text(&mut expected, MARGIN, MARGIN, "♥", LIFE);
        draw_text(&mut expected, MARGIN + 7, MARGIN, "3", COUNT);
        draw_text(&mut expected, MARGIN + 23, MARGIN, "◆", GEM);
        draw_text(&mut expected, MARGIN + 30, MARGIN, "0", COUNT);
        draw_text(&mut expected, MARGIN + 46, MARGIN, "●●●", HIT_POINT);
        // Relic icon after "♥3", "◆0" and three hit points.
        let x = MARGIN + 7 + 6 + GAP + 7 + 6 + GAP + 3 * 6 + 1 + GAP;
        draw_text(&mut expected, x, MARGIN, "★", RELIC_ICON);
        draw_text(&mut expected, x + 7, MARGIN, "1/2", COUNT);
        assert_eq!(canvas, expected);
        assert!(canvas.pixels.contains(&RELIC_ICON));
    }
}
