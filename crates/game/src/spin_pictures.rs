//! Which picture a single collectible shows on each frame
//! (`spec/behavior/collectibles.md`, "Pictures").
//!
//! A collectible whose entity's `pictures` param is [`SPIN`] spins: it
//! goes through 8 steps, 4 frames each, all collectibles together but
//! each some steps ahead by its cell on the level's 8 px grid
//! ([`step`]). The frames are counted from the latest frame 0, the
//! level's start or play starting again after a lost life, so a pause
//! doesn't change them.
//!
//! Steps 1 to 5 show the pictures labelled [`LABELS`] in the pack's
//! [`SHEET`], mirrored left to right with their origin at the
//! collectible's position, when the sheet has all five. Steps 6 to 8
//! have no pictures of their own, so they show the engine's art, as all
//! steps do without the five; the engine's art spins along with the
//! steps, unmirrored.

use clamber_level::Vec2;

/// The sheet a single collectible's pictures come from.
pub const SHEET: &str = "spark-pictures";

/// The entity `pictures` param of a collectible that spins.
pub const SPIN: &str = "spin";

/// The labels of steps 1 to 5.
pub const LABELS: [&str; 5] = ["spin-1", "spin-2", "spin-3", "spin-4", "spin-5"];

/// Steps in a turn.
pub const STEPS: u32 = 8;

/// Frames on each step.
const FRAMES: u32 = 4;

/// The step, 1 to [`STEPS`], of a collectible at `at` on the frame
/// with count `clock`, the frame's number plus one.
pub fn step(clock: u32, at: Vec2) -> u8 {
    let column = (at.x.floor() - 8).div_euclid(8);
    let row = (at.y.floor() - 9).div_euclid(8);
    let ahead = (column + row).rem_euclid(i64::from(STEPS)) as u32;
    ((clock / FRAMES + ahead) % STEPS + 1) as u8
}

/// The label of `step`'s picture, if it has one.
pub fn label(step: u8) -> Option<&'static str> {
    LABELS.get(usize::from(step).checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// collectibles.md, "Example (synthetic)".
    #[test]
    fn steps_as_the_spec_s_example() {
        let at = Vec2::ints(112, 65);
        let steps: Vec<u8> = (0..19).map(|frame| step(frame + 1, at)).collect();
        let mut expected = vec![5; 3];
        for k in [6, 7, 8, 1] {
            expected.extend([k; 4]);
        }
        assert_eq!(steps, expected);
        let next = Vec2::ints(120, 65);
        assert_eq!(step(1, next), 6);
        assert_eq!(step(3, next), 6);
        assert_eq!(step(4, next), 7);
        assert_eq!(step(7, next), 7);
        // A cell down is a step ahead too, and the turn wraps.
        assert_eq!(step(1, Vec2::ints(112, 73)), 6);
        assert_eq!(step(13, Vec2::ints(112, 73)), 1);
    }

    #[test]
    fn only_the_first_five_steps_are_labelled() {
        assert_eq!(label(1), Some("spin-1"));
        assert_eq!(label(5), Some("spin-5"));
        assert_eq!(label(6), None);
        assert_eq!(label(0), None);
    }
}
