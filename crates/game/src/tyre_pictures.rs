//! Which picture a tyre shows on each frame
//! (`spec/behavior/tyre.md`, "Pictures").
//!
//! The picture depends only on where the tyre is in its squash and
//! wobble: the frame of the catch counted from 0, or the wobble count.
//! The tyre and the stiff tyre have seven pictures each, labelled
//! [`LABELS`], in the pack's [`TYRE`] and [`STIFF`] sheets. When a sheet
//! lacks one of the seven, the engine's art is drawn with the same timing.

use clamber_sim::Phase;

/// The sheet the pictures of a tyre come from.
pub const TYRE: &str = "tyre-pictures";

/// The sheet the pictures of a stiff tyre come from.
pub const STIFF: &str = "stiff-tyre-pictures";

/// The labels of the seven pictures, from the tallest to the shortest
/// below it: `rest` is the middle one.
pub const LABELS: [&str; 7] = [
    "rest", "flat-1", "flat-2", "flat-3", "tall-1", "tall-2", "tall-3",
];

const REST: usize = 0;
const FLAT_1: usize = 1;
const FLAT_2: usize = 2;
const FLAT_3: usize = 3;
const TALL_1: usize = 4;
const TALL_2: usize = 5;
const TALL_3: usize = 6;

/// The index into [`LABELS`] of the picture a tyre in `phase` shows.
pub fn picture(phase: Phase) -> usize {
    match phase {
        // `since` frames after the catch, C being 0.
        Phase::Squashed { since, .. } => match since {
            0..=2 => FLAT_1,
            3..=5 => FLAT_2,
            6..=11 => FLAT_3,
            12 => FLAT_2,
            13 => REST,
            _ => TALL_2,
        },
        Phase::Wobbling { m, .. } => match m {
            0..=3 => TALL_3,
            4 | 5 => TALL_2,
            6 | 7 => TALL_1,
            10 | 11 => FLAT_1,
            12 | 13 => FLAT_2,
            14 | 15 => FLAT_1,
            18 | 19 => TALL_1,
            20 | 21 => TALL_2,
            22 | 23 => TALL_1,
            26 | 27 => FLAT_1,
            30 | 31 => TALL_1,
            // 8, 9, 16, 17, 24, 25, 28, 29 and from 32 on.
            _ => REST,
        },
        _ => REST,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squashed(since: u8) -> usize {
        picture(Phase::Squashed {
            since,
            from_air: false,
        })
    }

    fn wobbling(m: u8) -> usize {
        picture(Phase::Wobbling { m, thrown: None })
    }

    #[test]
    fn the_squash_follows_the_spec_s_list() {
        let list: Vec<usize> = (0..=14).map(squashed).collect();
        assert_eq!(list, [1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 3, 3, 2, 0, 5],);
    }

    #[test]
    fn the_wobble_follows_the_spec_s_list() {
        let list: Vec<usize> = (0..34).map(wobbling).collect();
        let want = [
            6, 6, 6, 6, 5, 5, 4, 4, 0, 0, 1, 1, 2, 2, 1, 1, 0, 0, 4, 4, 5, 5, 4, 4, 0, 0, 1, 1, 0,
            0, 4, 4, 0, 0,
        ];
        assert_eq!(list, want);
        assert_eq!(picture(Phase::Ready), 0);
    }
}
