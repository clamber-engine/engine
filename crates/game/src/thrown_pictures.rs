//! Which picture a thrown object shows in flight and lying
//! (`spec/behavior/thrown-objects.md`, "Pictures").
//!
//! In flight, from the frame it leaves the player's hands, it cycles
//! through `thrown-1` to `thrown-8`: its first picture on 2 frames, then
//! each on 3, and round again, unchanged through bounces and rolling
//! ([`flight`]). It is mirrored exactly when it moves left. Put down,
//! it shows `lying`, not mirrored, from the frame the player is free
//! again ([`lying`]).
//!
//! The pictures are in the pack's [`SHEET`]. The flight pictures are
//! shown only when the sheet has all eight ([`FLIGHT`]); otherwise the
//! engine's art turns along with the cycle. `lying` stands alone.

/// The sheet a thrown object's pictures come from.
pub const SHEET: &str = "thrown-pictures";

/// The flight pictures' labels, in cycle order.
pub const FLIGHT: [&str; 8] = [
    "thrown-1", "thrown-2", "thrown-3", "thrown-4", "thrown-5", "thrown-6", "thrown-7", "thrown-8",
];

/// The label of the picture of an object lying where it was put down.
pub const LYING: &str = "lying";

/// The index into [`FLIGHT`] shown `count` frames after the object left
/// the hands, which is frame 0.
pub fn flight(count: u32) -> usize {
    ((count + 1) / 3 % FLIGHT.len() as u32) as usize
}

/// Frames since an object was put down that it shows `lying` from: not
/// on the frame it was set down, which is still the put-down's.
pub const LYING_FROM: u32 = 1;

/// Whether an object put down `count` frames ago shows `lying`.
pub fn lying(count: u32) -> bool {
    count >= LYING_FROM
}

#[cfg(test)]
mod tests {
    use super::*;

    /// thrown-objects.md, "Example (synthetic)": leaving the hands on
    /// frame 500.
    #[test]
    fn flight_as_the_spec_s_example() {
        for (from, to, want) in [
            (500, 501, 0),
            (502, 504, 1),
            (505, 507, 2),
            (520, 522, 7),
            (523, 525, 0),
            (535, 537, 4),
            (538, 539, 5),
        ] {
            for frame in from..=to {
                assert_eq!(flight(frame - 500), want, "frame {frame}");
            }
        }
    }
}
