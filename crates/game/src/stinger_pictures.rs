//! Which picture a stinger shows on each frame
//! (`spec/behavior/stinger.md`, "Pictures").
//!
//! A stinger is always drawn mirrored left to right. Its four wing
//! pictures repeat every [`CYCLE`] counted frames from the frame it
//! appeared, which are all its frames but the ones it holds still on in
//! a knock-out of the player. One that was removed and comes back starts
//! again.
//!
//! The pictures are labelled [`LABELS`] in the pack's [`SHEET`]. When the
//! sheet lacks one of the four, the engine's art turns along with the
//! same cycle, still mirrored.

/// The sheet a stinger's pictures come from.
pub const SHEET: &str = "stinger-pictures";

/// The labels of the four pictures.
pub const LABELS: [&str; 4] = ["wing-1", "wing-2", "wing-3", "wing-4"];

/// Counted frames in the repeating part of the cycle.
pub const CYCLE: u32 = 14;

/// The index into [`LABELS`] of the picture with `count` counted frames
/// since the stinger appeared (0 on the frame it did).
pub fn picture(count: u32) -> usize {
    // wing-1 twice, then repeating: 2, 2 of wing-2 and wing-3 up to
    // 3 of wing-4, and back down to 3 of wing-1.
    const REPEAT: [u8; CYCLE as usize] = [1, 1, 2, 2, 3, 3, 3, 2, 2, 1, 1, 0, 0, 0];
    match count.checked_sub(2) {
        None => 0,
        Some(n) => usize::from(REPEAT[(n % CYCLE) as usize]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cycle_is_the_spec_s() {
        let shown: Vec<usize> = (0..18).map(picture).collect();
        assert_eq!(
            shown,
            [0, 0, 1, 1, 2, 2, 3, 3, 3, 2, 2, 1, 1, 0, 0, 0, 1, 1]
        );
        // From the third counted frame it repeats every 14.
        for n in 2..60 {
            assert_eq!(picture(n), picture(n + CYCLE), "{n}");
        }
    }
}
