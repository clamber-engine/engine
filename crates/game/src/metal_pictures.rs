//! Which picture a metal barrel shows lying and in flight
//! (`spec/behavior/metal-barrel.md`, "Pictures").
//!
//! Lying, standing or put down, it shows `metal-lying`, not mirrored.
//! In flight it cycles through `metal-thrown-1` to `metal-thrown-8` as
//! a thrown object does ([`crate::thrown_pictures::flight`]), mirrored
//! exactly when it moves left, which changes on its turn at a wall. It
//! has no flash and no pieces.
//!
//! The pictures are in the pack's [`SHEET`]. The flight pictures are
//! shown only when the sheet has all eight ([`FLIGHT`]); otherwise the
//! engine's art turns along with the cycle. `metal-lying` stands alone.

/// The sheet a metal barrel's pictures come from.
pub const SHEET: &str = "metal-pictures";

/// The flight pictures' labels, in cycle order.
pub const FLIGHT: [&str; 8] = [
    "metal-thrown-1",
    "metal-thrown-2",
    "metal-thrown-3",
    "metal-thrown-4",
    "metal-thrown-5",
    "metal-thrown-6",
    "metal-thrown-7",
    "metal-thrown-8",
];

/// The label of the picture of a metal barrel lying on the floor.
pub const LYING: &str = "metal-lying";
