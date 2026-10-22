//! Which picture a bunch shows on each frame
//! (`spec/behavior/bunches.md`, "Pictures").
//!
//! A collectible whose entity's `pictures` param is [`BUNCH`] spins up
//! through `bunch-1` to `bunch-6` and back down, a [`CYCLE`] of 10
//! entries, each on 6 counted frames. Each bunch keeps its own cycle,
//! counted from the frame it appears, or the frame play starts again
//! after a lost life, on which its count is 0 ([`entry`]). The frames
//! on which walkers hold still don't count (`World::still`).
//!
//! The pictures are labelled [`LABELS`] in the pack's [`SHEET`], drawn
//! unmirrored with their origin at the bunch's position, when the sheet
//! has all six; otherwise the engine's art turns along with the cycle.

/// The sheet a bunch's pictures come from.
pub const SHEET: &str = "cluster-pictures";

/// The entity `pictures` param of a collectible drawn as a bunch.
pub const BUNCH: &str = "bunch";

/// The labels of the six pictures.
pub const LABELS: [&str; 6] = [
    "bunch-1", "bunch-2", "bunch-3", "bunch-4", "bunch-5", "bunch-6",
];

/// Entries in a cycle.
pub const CYCLE: u32 = 10;

/// Counted frames on each entry.
const FRAMES: u32 = 6;

/// The entry, 0 to [`CYCLE`] − 1, a bunch shows with `count` counted
/// frames since it appeared: its first lasts a frame less.
pub fn entry(count: u32) -> u32 {
    (count + 1) / FRAMES % CYCLE
}

/// The label of `entry`'s picture: up through the six, then back down.
pub fn label(entry: u32) -> &'static str {
    let at = match entry % CYCLE {
        e @ 0..=5 => e,
        e => CYCLE - e,
    };
    LABELS[at as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// bunches.md, "Example (synthetic)": appearing on frame 100, with
    /// 121 and 122 held still by a bounce.
    #[test]
    fn pictures_as_the_spec_s_example() {
        let mut count = 0;
        let mut shown = Vec::new();
        for frame in 100..=166 {
            if frame > 100 && !(121..=122).contains(&frame) {
                count += 1;
            }
            shown.push((frame, label(entry(count))));
        }
        let at = |frame: u32| shown[(frame - 100) as usize].1;
        for (from, to, want) in [
            (100, 104, "bunch-1"),
            (105, 110, "bunch-2"),
            (111, 116, "bunch-3"),
            (117, 124, "bunch-4"),
            (125, 130, "bunch-5"),
            (131, 136, "bunch-6"),
            (137, 142, "bunch-5"),
            (143, 148, "bunch-4"),
            (149, 154, "bunch-3"),
            (155, 160, "bunch-2"),
            (161, 166, "bunch-1"),
        ] {
            for frame in from..=to {
                assert_eq!(at(frame), want, "frame {frame}");
            }
        }
    }
}
