//! The heads-up display: lives, pickups and hit points, from the world's
//! level flow, and the letter row while the world shows it. A world with a count
//! display shows the count and lives only when it says so.

use clamber_pack::Param;
use clamber_sim::{CountDisplay, LetterRow, LivesDisplay, LivesStart, RELIC, World};

use crate::canvas::{Color, rgb};
use crate::icons::{Icon, draw_icon};
use crate::surface::Surface;
use crate::text::{GLYPH, draw_text, draw_text_centred, text_size};

/// From the top left of the view to the HUD, in view units, so it stays
/// clear of the edges at any window shape.
const MARGIN: i32 = 8;
/// Space between one item and the next icon.
const GAP: i32 = 10;
/// An icon's square, a little taller than the capitals of the count
/// beside it.
const ICON: i32 = 11;
/// From one hit point's flask to the next: they're narrower than their
/// squares.
const HIT_POINT_STEP: i32 = 8;

pub(crate) const COUNT: Color = rgb(0xf4, 0xf0, 0xe8);

/// A letter's image in the letter row, px square.
const LETTER: i32 = 16;
/// The count display's collectible image, px square.
const IMAGE: i32 = 16;
const LETTER_BACK: Color = rgb(0xf0, 0xb0, 0x50);
const LETTER_TEXT: Color = rgb(0x30, 0x20, 0x10);

/// Draws the count display's image given the canvas and its top-left
/// corner on it, and says whether it did.
pub type CountImage<'a> = dyn Fn(&mut dyn Surface, (i32, i32)) -> bool + 'a;

/// Draws the HUD along the top left of the canvas: lives, gems, hit
/// points when a life has more than one, and relics out of the level's
/// total when it has any. With a count
/// display, the count display and the lives display instead, when the
/// world shows them. Then the letter row, if the world shows it.
pub fn draw_hud(canvas: &mut dyn Surface, world: &World) {
    draw_hud_with(canvas, world, &|_, _| false);
}

/// [`draw_hud`], with the count display's image drawn by `image` when
/// it can, given the canvas and the image's top-left corner on it, and
/// says whether it did; otherwise it's the engine's icon.
pub fn draw_hud_with(canvas: &mut dyn Surface, world: &World, image: &CountImage) {
    if world.feel.world[Param::CountShowFor].floor() > 0 {
        let left = picture_left(canvas, world);
        if let Some(count) = world.count_display() {
            draw_count(canvas, left, &count, image);
        }
        if let Some(lives) = world.lives_display() {
            draw_lives(canvas, left, &lives);
        }
    } else {
        draw_items(canvas, world);
    }
    for row in world.letter_rows() {
        draw_letter_row(canvas, world, &row);
    }
}

/// Lives, gems, hit points and relics in a row. Hit points are a flask
/// each, full for those left.
fn draw_items(canvas: &mut dyn Surface, world: &World) {
    let flow = &world.flow;
    let at = |x| (x, MARGIN);
    let mut x = draw_counter(canvas, at(MARGIN), Icon::Life, &flow.lives.to_string()) + GAP;
    x = draw_counter(canvas, at(x), Icon::Gem, &flow.gems.to_string()) + GAP;
    if let Some((left, max)) = world.hit_points() {
        for k in 0..max {
            let icon = match k < left {
                true => Icon::HitPoint,
                false => Icon::HitPointLost,
            };
            draw_icon_on_line(canvas, icon, at(x));
            x += HIT_POINT_STEP;
        }
        x += ICON - HIT_POINT_STEP + GAP;
    }
    let relics = relic_total(world);
    if relics > 0 {
        let count = format!("{}/{relics}", flow.relics);
        draw_counter(canvas, at(x), Icon::Relic, &count);
    }
}

/// Draws `icon` from `x`, then `count` after it, on a line of text whose
/// top is `y`: where the counter ends.
pub(crate) fn draw_counter(
    canvas: &mut dyn Surface,
    (x, y): (i32, i32),
    icon: Icon,
    count: &str,
) -> i32 {
    draw_icon_on_line(canvas, icon, (x, y));
    draw_text(canvas, x + ICON + 1, y, count, COUNT);
    x + counter_width(count)
}

/// How wide [`draw_counter`] draws `count` and its icon.
pub(crate) fn counter_width(count: &str) -> i32 {
    ICON + 1 + text_size(count).0
}

/// Draws `icon` in its square from `x`, centred on the capitals of a
/// line of text whose top is `y`.
fn draw_icon_on_line(canvas: &mut dyn Surface, icon: Icon, (x, y): (i32, i32)) {
    let top = y as f32 + (GLYPH.1 - ICON) as f32 / 2.0;
    draw_icon(canvas, icon, (x as f32, top), ICON as f32);
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
/// gem icon filling its 16 px square, and each digit is drawn from its
/// place.
fn draw_count(canvas: &mut dyn Surface, left: i32, count: &CountDisplay, image: &CountImage) {
    let (x, y) = (count.image.0 + left, count.image.1);
    if !image(canvas, (x, y)) {
        draw_icon(canvas, Icon::Gem, (x as f32, y as f32), IMAGE as f32);
    }
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
    if lives.image {
        let grow = LIVES_GROW[usize::from(lives_picture(lives.since, lives.start)) - 1];
        let size = (ICON + grow) as f32;
        let top = lives.y as f32 + 4.0 + (GLYPH.1 as f32 - size) / 2.0;
        let left = (x - ICON - 1) as f32 - grow as f32 / 2.0;
        draw_icon(canvas, Icon::Life, (left, top), size);
    }
    for (k, digit) in lives.number.to_string().chars().enumerate() {
        let x = x + lives.gap * k as i32;
        draw_text(canvas, x, lives.y + 4, &digit.to_string(), COUNT);
    }
}

/// The lives image's cycle: the picture, 1 to 8, of each of its 14 steps
/// of 3 frames (count-display.md, "The lives image").
const LIVES_CYCLE: [u8; 14] = [1, 2, 3, 2, 1, 4, 5, 6, 7, 8, 7, 6, 5, 4];

/// How much bigger than its square the engine's art for each of the
/// lives pictures 1 to 10 is, px: it swells and shrinks through the
/// cycle. The spec has no hashes for them.
const LIVES_GROW: [i32; 10] = [0, 1, 2, 3, 4, 3, 2, 1, -1, -2];

/// Which of the lives pictures, 1 to 10, the lives image shows `since`
/// frames after the number was first drawn (`D`), when the display is
/// for play starting after `start` or not. The frames before `D` + 4
/// show what `D` + 4 does.
fn lives_picture(since: u32, start: Option<LivesStart>) -> u8 {
    let from_cycle = since.saturating_sub(7);
    let step = (from_cycle / 3 % LIVES_CYCLE.len() as u32) as usize;
    match (since, start) {
        (..=6, Some(LivesStart::Level | LivesStart::Knockout)) => 9,
        (..=6, _) => 4,
        (7..=9, Some(LivesStart::Fallout)) => 10,
        _ => LIVES_CYCLE[step],
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

    #[test]
    fn the_lives_image_follows_the_cycle_of_three_frame_steps() {
        let at = |since| lives_picture(since, None);
        assert_eq!([at(4), at(6)], [4, 4]);
        let steps: Vec<u8> = (0..14).map(|k| at(7 + 3 * k)).collect();
        assert_eq!(steps, LIVES_CYCLE);
        assert_eq!([at(7), at(9), at(10)], [1, 1, 2]);
        // Step 1 follows step 14.
        assert_eq!([at(7 + 42), at(7 + 42 + 3)], [1, 2]);
    }

    #[test]
    fn play_starting_has_its_own_first_pictures() {
        let level = |since| lives_picture(since, Some(LivesStart::Level));
        assert_eq!([level(4), level(6), level(7)], [9, 9, 1]);
        let knocked = |since| lives_picture(since, Some(LivesStart::Knockout));
        assert_eq!([knocked(5), knocked(7)], [9, 1]);
        let fell = |since| lives_picture(since, Some(LivesStart::Fallout));
        assert_eq!([fell(4), fell(7), fell(9), fell(10)], [4, 10, 10, 2]);
    }
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

    /// The lives and gems counters as the HUD draws them: where the
    /// next item goes.
    fn lives_and_gems(canvas: &mut Canvas, lives: &str, gems: &str) -> i32 {
        let x = draw_counter(canvas, (MARGIN, MARGIN), Icon::Life, lives) + GAP;
        draw_counter(canvas, (x, MARGIN), Icon::Gem, gems) + GAP
    }

    #[test]
    fn shows_lives_and_gems() {
        let mut world = world("");
        world.flow.lives -= 1;
        world.flow.gems += 10;
        world.flow.hits_taken = 1;
        let mut expected = Canvas::scaled((120, 24), 4);
        let x = lives_and_gems(&mut expected, "2", "10");
        // Hit points after the gems: two left of three.
        draw_icon_on_line(&mut expected, Icon::HitPoint, (x, MARGIN));
        draw_icon_on_line(&mut expected, Icon::HitPoint, (x + HIT_POINT_STEP, MARGIN));
        let lost = (x + 2 * HIT_POINT_STEP, MARGIN);
        draw_icon_on_line(&mut expected, Icon::HitPointLost, lost);
        assert_eq!(drawn(&world), expected);
    }

    #[test]
    fn shows_no_hit_points_when_every_hit_costs_a_life() {
        let mut world = world("");
        let mut feel = (*world.feel).clone();
        feel.world.set(Param::HitPoints, clamber_fixed::Fixed::ONE);
        world.feel = Arc::new(feel);
        let mut expected = Canvas::scaled((120, 24), 4);
        lives_and_gems(&mut expected, &world.flow.lives.to_string(), "0");
        assert_eq!(drawn(&world), expected);
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
        assert!(world.lives_display().is_none(), "no lives display");
        let mut expected = Canvas::new(256, 64);
        draw_count(
            &mut expected,
            0,
            &world.count_display().unwrap(),
            &|_, _| false,
        );
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
        let mut expected = Canvas::scaled((120, 24), 4);
        let mut x = lives_and_gems(&mut expected, "3", "0");
        for _ in 0..3 {
            draw_icon_on_line(&mut expected, Icon::HitPoint, (x, MARGIN));
            x += HIT_POINT_STEP;
        }
        // The relic counter after three hit points.
        let x = x + ICON - HIT_POINT_STEP + GAP;
        draw_counter(&mut expected, (x, MARGIN), Icon::Relic, "1/2");
        assert_eq!(drawn(&world), expected);
    }

    #[test]
    fn stays_clear_of_the_edges() {
        let world = world("");
        let mut canvas = Canvas::scaled((120, 24), 4);
        let bg = canvas.pixels[0];
        draw_hud(&mut canvas, &world);
        let w = canvas.width as usize;
        let edge = 4 * 4;
        for (i, &p) in canvas.pixels.iter().enumerate() {
            let (x, y) = (i % w, i / w);
            if p != bg {
                assert!(x >= edge && y >= edge, "drawn at {x}, {y}");
            }
        }
    }
}
