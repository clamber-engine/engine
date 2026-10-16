//! The colours of a level's dimming, as `spec/behavior/dimming.md`
//! gives them for each step, worked on each channel at 5 bits.
//!
//! Each colour changes on its own, from the colour it has before the
//! dimming. Over the dimming's steps, green goes in equal steps to `M`,
//! 5/8 of itself, blue in equal steps to the same `M`, and red down to
//! about 5/16 of itself, all rounded down at every step as the spec says.

use clamber_sprite::{Rgba, RgbaImage};

/// `colour` after `step` of `steps`, worked at 5 bits a channel, keeping
/// its alpha.
pub fn dim(colour: Rgba, step: u32, steps: u32) -> Rgba {
    let [r, g, b, a] = colour;
    if step == 0 || steps == 0 || a == 0 {
        return colour;
    }
    let k = i64::from(step.min(steps));
    let n = i64::from(steps);
    let [r, g, b] = [r, g, b].map(|v| (i64::from(v) * 31 + 127) / 255);
    let m = 5 * g / 8;
    // With 32 steps red goes down by ⌊11R / 2⌋ / 256 a step.
    let red = (8 * n * r - k * (11 * r / 2)).div_euclid(8 * n);
    let toward = |c: i64| c + (k * (m - c)).div_euclid(n);
    let [r, g, b] = [red, toward(g), toward(b)].map(|c| ((c * 255 + 15) / 31) as u8);
    [r, g, b, a]
}

/// `image` after `step` of `steps`.
pub fn dim_image(image: &RgbaImage, step: u32, steps: u32) -> RgbaImage {
    RgbaImage {
        width: image.width,
        height: image.height,
        data: image.data.iter().map(|&c| dim(c, step, steps)).collect(),
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
    fn steps_past_the_last_and_clear_texels_change_nothing_more() {
        let c = colour([20, 12, 6]);
        assert_eq!(dim(c, 40, 32), dim(c, 32, 32));
        assert_eq!(dim([1, 2, 3, 0], 5, 32), [1, 2, 3, 0]);
    }
}
