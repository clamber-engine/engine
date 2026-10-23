//! The colours of a level's dimming, as `spec/behavior/dimming.md`
//! gives them for each step, worked on each channel at 5 bits.
//!
//! Each colour changes on its own, from the colour it has before the
//! dimming. Over the dimming's steps, green goes in equal steps to `M`,
//! 5/8 of itself, blue in equal steps to the same `M`, and red down to
//! about 5/16 of itself, all rounded down at every step as the spec says.
//!
//! A layer can dim another way, each texel going in equal steps to a
//! night colour ([`toward`]), and can be drawn darker ([`darker`]).

use clamber_sprite::{Rgba, RgbaImage};

/// An 8-bit channel at 5 bits.
fn narrow(v: u8) -> i64 {
    (i64::from(v) * 31 + 127) / 255
}

/// A 5-bit channel at 8 bits, as the importer writes it.
fn wide(c: i64) -> u8 {
    ((c * 255 + 15) / 31) as u8
}

/// `colour` after `step` of `steps`, worked at 5 bits a channel, keeping
/// its alpha.
pub fn dim(colour: Rgba, step: u32, steps: u32) -> Rgba {
    let [r, g, b, a] = colour;
    if step == 0 || steps == 0 || a == 0 {
        return colour;
    }
    let k = i64::from(step.min(steps));
    let n = i64::from(steps);
    let [r, g, b] = [r, g, b].map(narrow);
    let m = 5 * g / 8;
    // With 32 steps red goes down by ⌊11R / 2⌋ / 256 a step.
    let red = (8 * n * r - k * (11 * r / 2)).div_euclid(8 * n);
    let toward = |c: i64| c + (k * (m - c)).div_euclid(n);
    let [r, g, b] = [red, toward(g), toward(b)].map(wide);
    [r, g, b, a]
}

/// The steps `spec/behavior/dimming.md` gives the sprites' colours for.
pub const SPRITE_STEPS: u32 = 8;

/// A sprite colour after `step` of the dimming, worked at 5 bits a
/// channel (`spec/behavior/dimming.md`, "Sprite colours"): red loses
/// `R / 64` a step, green `⌊G / 2⌋ / 32` and blue moves towards green by
/// `(G - B) / 64`, each rounded down after the steps. Steps past
/// [`SPRITE_STEPS`] are not covered; the colour stays as it is at the last.
pub fn sprite(colour: Rgba, step: u32) -> Rgba {
    let [r, g, b, a] = colour;
    let k = i64::from(step.min(SPRITE_STEPS));
    if k == 0 || a == 0 {
        return colour;
    }
    let [r, g, b] = [r, g, b].map(narrow);
    let red = (64 * r - k * r).div_euclid(64);
    let green = (32 * g - k * (g / 2)).div_euclid(32);
    let blue = (64 * b + k * (g - b)).div_euclid(64);
    let [r, g, b] = [red, green, blue].map(wide);
    [r, g, b, a]
}

/// `colour` after `step` of `steps` going in equal steps to `night`,
/// worked at 5 bits a channel and rounded down, keeping its alpha
/// (`spec/behavior/dimming.md`, "The backdrop and the horizon layer").
pub fn toward(colour: Rgba, night: Rgba, step: u32, steps: u32) -> Rgba {
    let [r, g, b, a] = colour;
    if step == 0 || steps == 0 || a == 0 {
        return colour;
    }
    let k = i64::from(step.min(steps));
    let n = i64::from(steps);
    let channel = |c: u8, to: u8| {
        let (c, to) = (narrow(c), narrow(to));
        wide(c + (k * (to - c)).div_euclid(n))
    };
    [
        channel(r, night[0]),
        channel(g, night[1]),
        channel(b, night[2]),
        a,
    ]
}

/// `colour` drawn darker: each channel, worked at 5 bits, 4 less (at
/// least 0) and then halved, keeping its alpha (`spec/formats/graphics.md`,
/// "The far layer's colours").
pub fn darker(colour: Rgba) -> Rgba {
    let [r, g, b, a] = colour;
    let [r, g, b] = [r, g, b].map(|v| wide((narrow(v) - 4).max(0) / 2));
    [r, g, b, a]
}

/// `image` after `step` of `steps`.
pub fn dim_image(image: &RgbaImage, step: u32, steps: u32) -> RgbaImage {
    map_image(image, |_, _, c| dim(c, step, steps))
}

/// `image` after `step` of `steps` going to `night`, each texel to the
/// texel at the same place in it, whatever its size.
pub fn toward_image(image: &RgbaImage, night: &RgbaImage, step: u32, steps: u32) -> RgbaImage {
    if night.width == 0 || night.height == 0 {
        return image.clone();
    }
    let at = |x: u32, y: u32| {
        let nx = u64::from(x) * u64::from(night.width) / u64::from(image.width.max(1));
        let ny = u64::from(y) * u64::from(night.height) / u64::from(image.height.max(1));
        night.get(nx as u32, ny as u32)
    };
    map_image(image, |x, y, c| toward(c, at(x, y), step, steps))
}

/// `image` drawn darker ([`darker`]).
pub fn darker_image(image: &RgbaImage) -> RgbaImage {
    map_image(image, |_, _, c| darker(c))
}

fn map_image(image: &RgbaImage, f: impl Fn(u32, u32, Rgba) -> Rgba) -> RgbaImage {
    let width = image.width.max(1);
    RgbaImage {
        width: image.width,
        height: image.height,
        data: (0..)
            .zip(&image.data)
            .map(|(i, &c)| f(i % width, i / width, c))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A colour of 5-bit channels as 8 bits, as the importer writes it.
    fn colour([r, g, b]: [u8; 3]) -> Rgba {
        let wide = |c: u8| ((u32::from(c) * 255 + 15) / 31) as u8;
        [wide(r), wide(g), wide(b), 255]
    }

    #[test]
    fn the_sprite_colours_in_the_spec() {
        // dimming.md, "Sprite colours" and its example.
        let first = colour([20, 12, 6]);
        assert_eq!(sprite(first, 0), first);
        assert_eq!(sprite(first, 1), colour([19, 11, 6]));
        assert_eq!(sprite(first, 8), colour([17, 10, 6]));
        let white = colour([31, 31, 31]);
        assert_eq!(sprite(white, 1), colour([30, 30, 31]));
        assert_eq!(sprite(white, 8), colour([27, 27, 31]));
        assert_eq!(sprite(white, 20), sprite(white, 8), "held at the 8th");
    }

    #[test]
    fn the_made_up_colours_in_the_spec() {
        // dimming.md, "Example (made-up colours)".
        let first = colour([20, 12, 6]);
        for (step, after) in [
            (0, [20, 12, 6]),
            (1, [19, 11, 6]),
            (2, [19, 11, 6]),
            (16, [13, 9, 6]),
            (31, [6, 7, 6]),
            (32, [6, 7, 7]),
        ] {
            assert_eq!(dim(first, step, 32), colour(after), "step {step}");
        }
        let second = colour([3, 25, 28]);
        for (step, after) in [(1, [2, 24, 27]), (16, [2, 20, 21]), (32, [1, 15, 15])] {
            assert_eq!(dim(second, step, 32), colour(after), "step {step}");
        }
    }

    #[test]
    fn every_colour_ends_as_the_table_says() {
        // After step 32: red ⌊R − 32 × ⌊11R / 2⌋ / 256⌋, green and blue M.
        for r in 0..32u8 {
            for g in 0..32u8 {
                for b in [0, 9, 31] {
                    let m = 5 * g / 8;
                    let wide = u32::from(r);
                    let red = ((256 * wide - 32 * (11 * wide / 2)) / 256) as u8;
                    let after = dim(colour([r, g, b]), 32, 32);
                    assert_eq!(after, colour([red, m, m]), "{r} {g} {b}");
                }
            }
        }
    }

    #[test]
    fn list_entries_go_to_their_night_colour() {
        // dimming.md, "Example (made-up colours)": a list entry.
        let (day, night) = (colour([20, 12, 6]), colour([0, 0, 3]));
        for (step, after) in [
            (0, [20, 12, 6]),
            (1, [19, 11, 5]),
            (2, [18, 11, 5]),
            (16, [10, 6, 4]),
            (31, [0, 0, 3]),
            (32, [0, 0, 3]),
        ] {
            assert_eq!(toward(day, night, step, 32), colour(after), "step {step}");
        }
    }

    #[test]
    fn the_far_layer_dims_then_is_darker() {
        // dimming.md, "Example (made-up colours)": a far-layer pixel.
        let c = colour([20, 12, 6]);
        for (step, after) in [
            (0, [8, 4, 1]),
            (1, [7, 3, 1]),
            (16, [4, 2, 1]),
            (32, [1, 1, 1]),
        ] {
            assert_eq!(darker(dim(c, step, 32)), colour(after), "step {step}");
        }
        // graphics.md, "The far layer's colours".
        assert_eq!(darker(colour([31, 10, 3])), colour([13, 3, 0]));
    }

    #[test]
    fn night_images_of_any_size_cover_the_image() {
        let mut image = RgbaImage::new(2, 4);
        image.data.fill(colour([20, 12, 6]));
        image.data[0][3] = 0;
        let mut night = RgbaImage::new(1, 2);
        night.data = vec![colour([0, 0, 3]), colour([0, 0, 0])];
        let after = toward_image(&image, &night, 32, 32);
        assert_eq!(after.data[0], image.data[0], "clear texels stay");
        assert_eq!(after.data[1..4], [colour([0, 0, 3]); 3]);
        assert_eq!(after.data[4..], [colour([0, 0, 0]); 4]);
    }

    #[test]
    fn steps_past_the_last_and_clear_texels_change_nothing_more() {
        let c = colour([20, 12, 6]);
        assert_eq!(dim(c, 40, 32), dim(c, 32, 32));
        assert_eq!(dim([1, 2, 3, 0], 5, 32), [1, 2, 3, 0]);
    }
}
