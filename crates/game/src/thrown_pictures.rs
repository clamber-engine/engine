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
//! A broken object leaves a flash of eight pictures over 19 frames, mirrored
//! when it was moving right ([`flash`]), and four pieces that show
//! `piece-a-*` (pieces 1 and 2) or `piece-b-*` (3 and 4), never mirrored
//! ([`piece`]). Each set is shown only when the sheet has all eight.
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

/// The flash's labels, in order.
pub const FLASH: [&str; 8] = [
    "flash-1", "flash-2", "flash-3", "flash-4", "flash-5", "flash-6", "flash-7", "flash-8",
];

/// The labels of the first and second set of pieces, in order.
pub const PIECES: [[&str; 8]; 2] = [
    [
        "piece-a-1",
        "piece-a-2",
        "piece-a-3",
        "piece-a-4",
        "piece-a-5",
        "piece-a-6",
        "piece-a-7",
        "piece-a-8",
    ],
    [
        "piece-b-1",
        "piece-b-2",
        "piece-b-3",
        "piece-b-4",
        "piece-b-5",
        "piece-b-6",
        "piece-b-7",
        "piece-b-8",
    ],
];

/// How many frames each of the flash's pictures shows.
const FLASH_FRAMES: [u32; 8] = [2, 2, 3, 2, 3, 2, 3, 2];

/// The index into [`FLASH`] shown `count` frames after the object broke,
/// which is frame 0, or `None` once the flash is gone.
pub fn flash(count: u32) -> Option<usize> {
    let mut left = count;
    for (i, frames) in FLASH_FRAMES.into_iter().enumerate() {
        if left < frames {
            return Some(i);
        }
        left -= frames;
    }
    None
}

/// The index into a set of [`PIECES`] shown `count` frames after the
/// object broke, which is frame 0.
pub fn piece(count: u32) -> usize {
    ((count + 1) / 4 % 8) as usize
}

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

    /// thrown-objects.md, "Example (synthetic)": breaking on frame 540.
    #[test]
    fn flash_and_pieces_as_the_spec_s_example() {
        for (from, to, want) in [(540, 541, 0), (542, 543, 1), (544, 546, 2), (557, 558, 7)] {
            for frame in from..=to {
                assert_eq!(flash(frame - 540), Some(want), "frame {frame}");
            }
        }
        assert_eq!(flash(19), None);
        for (from, to, want) in [(540, 542, 0), (543, 546, 1), (567, 570, 7), (571, 574, 0)] {
            for frame in from..=to {
                assert_eq!(piece(frame - 540), want, "frame {frame}");
            }
        }
    }

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
