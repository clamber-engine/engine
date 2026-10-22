//! Which picture an extra-life item shows on each frame
//! (`spec/behavior/extra-lives.md`, "Pictures").
//!
//! All four kinds use the same eight pictures, labelled [`LABELS`], in
//! the pack's [`SHEET`]: a [`CYCLE`] of 56 frames up through the eight
//! and back down, counted from the frame the item appears ([`entry`]).
//! The sheet is used when it has all eight; otherwise the engine's art
//! is drawn with the same timing.

/// The sheet the pictures of an extra-life item come from.
pub const SHEET: &str = "item-pictures";

/// The labels of the eight pictures.
pub const LABELS: [&str; 8] = [
    "item-1", "item-2", "item-3", "item-4", "item-5", "item-6", "item-7", "item-8",
];

/// Frames in a cycle.
pub const CYCLE: u32 = 56;

/// Frames on each step of the cycle.
const FRAMES: u32 = 4;

/// The index into [`LABELS`] an item shows `count` frames after the one
/// it appears on, which is 0.
pub fn entry(count: u32) -> usize {
    let step = (count + 1) % CYCLE / FRAMES;
    // Up through the eight (steps 0 to 7), then back down (8 to 13).
    (if step < 8 { step } else { 14 - step }) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cycle_follows_the_spec_s_list() {
        let at = |n: u32| entry(n) + 1;
        // item-1 on a, a + 1 and a + 2; item-2 on a + 3 to a + 6.
        assert_eq!([at(0), at(2), at(3), at(6), at(7)], [1, 1, 2, 2, 3]);
        // item-8 on a + 27 to a + 30, item-7 on the way down from a + 31.
        assert_eq!([at(26), at(27), at(30), at(31)], [7, 8, 8, 7]);
        // Down to item-2 on a + 51 to a + 54, item-1 on a + 55 to a + 58.
        assert_eq!([at(50), at(51), at(54), at(55)], [3, 2, 2, 1]);
        assert_eq!([at(58), at(59), at(60)], [1, 2, 2]);
    }
}
