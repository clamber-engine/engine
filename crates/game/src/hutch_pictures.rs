//! What an animal crate, the smoke of its breaking and the animal show
//! (`spec/behavior/animal-crate.md`, "Pictures").
//!
//! The crate shows `crate-whole`, then, from the frame it breaks (B),
//! a broken one: `crate-broken-2` from B + 18 on, and (the spec doesn't
//! say) `crate-broken-1` before. On B a flash rises from it
//! ([`flash`], the pictures of [`crate::thrown_pictures`]) and a puff of
//! smoke appears at a point near it ([`smoke`]); both count the frames
//! on which something moves. From B + 18 the animal plays a fixed
//! sequence of 717 counted frames ([`waiting`]), and from 2 to 21 frames
//! after it's mounted it shows five pictures ([`mounting`]), mirrored
//! when the player faced left.
//!
//! The pictures are in the pack's [`SHEET`]. Each group (the crate's
//! three, the smoke's six, the animal's 22, the mounting five) is shown
//! only when the sheet has all of it; otherwise the engine's art stays.

use std::sync::OnceLock;

/// The sheet the crate's, smoke's and animal's pictures come from.
pub const SHEET: &str = "crate-pictures";

pub const CRATE: [&str; 3] = ["crate-whole", "crate-broken-1", "crate-broken-2"];

pub const SMOKE: [&str; 6] = [
    "smoke-1", "smoke-2", "smoke-3", "smoke-4", "smoke-5", "smoke-6",
];

pub const ANIMAL: [&str; 22] = [
    "animal-1",
    "animal-2",
    "animal-3",
    "animal-4",
    "animal-5",
    "animal-6",
    "animal-7",
    "animal-8",
    "animal-9",
    "animal-10",
    "animal-11",
    "animal-12",
    "animal-13",
    "animal-14",
    "animal-15",
    "animal-16",
    "animal-17",
    "animal-18",
    "animal-19",
    "animal-20",
    "animal-21",
    "animal-22",
];

/// The animal's pictures while it stands, ridden or not.
pub const STANDING: [&str; 6] = [
    "animal-1", "animal-2", "animal-3", "animal-4", "animal-5", "animal-6",
];

pub const MOUNTING: [&str; 5] = [
    "mounting-1",
    "mounting-2",
    "mounting-3",
    "mounting-4",
    "mounting-5",
];

/// Where a crate is in its pictures: the frames since it broke, the
/// counted frames since B (`c`) and since B + 18 (`a`), where its smoke
/// is, and whether the mounting pictures are mirrored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// Frames since B, or `None` for a crate that hasn't broken.
    pub frames: Option<u32>,
    pub c: u32,
    pub a: u32,
    pub dx: i32,
    pub dy: i32,
    pub flip: bool,
}

impl Clock {
    /// The clock a frame on: `still` says nothing moved on it. `seed`
    /// picks where the smoke comes if it's B.
    pub fn next(self, broken: bool, still: bool, seed: u32) -> Clock {
        if !broken {
            return Clock::default();
        }
        let Some(frames) = self.frames.map(|f| f.saturating_add(1)) else {
            let (dx, dy) = smoke_offset(seed);
            return Clock {
                frames: Some(0),
                dx,
                dy,
                ..Clock::default()
            };
        };
        let counted = u32::from(!still);
        let a = match frames {
            ..ANIMAL_FROM => 0,
            ANIMAL_FROM => 0,
            _ => self.a + counted,
        };
        Clock {
            frames: Some(frames),
            c: self.c + counted,
            a,
            ..self
        }
    }
}

/// The frame (since B) from which the animal is there, waiting.
pub const ANIMAL_FROM: u32 = 18;

/// The index into [`CRATE`] of the crate's picture, `frames` after it
/// broke, or `None` for a crate that hasn't.
pub fn crate_picture(frames: Option<u32>) -> usize {
    match frames {
        None => 0,
        Some(f) if f < ANIMAL_FROM => 1,
        Some(_) => 2,
    }
}

/// The index into [`crate::thrown_pictures::FLASH`] of the flash `c`
/// counted frames after B, or `None` once it's gone (from 39). `flash-1`
/// was never seen on screen; the engine shows it through `c` 3.
pub fn flash(c: u32) -> Option<usize> {
    match c {
        0..=3 => Some(0),
        4..=38 => Some(1 + (c as usize - 4) / 5),
        _ => None,
    }
}

/// The flash's level y: 0.75 px above the crate's position, rising 0.75
/// px per counted frame.
pub fn flash_rise(c: u32) -> f64 {
    0.75 + 0.75 * f64::from(c)
}

/// The index into [`SMOKE`] of the puff `c` counted frames after B, or
/// `None` once it's gone (from 17). `smoke-1` and `smoke-2` were never
/// seen on screen; the engine shows them through `c` 2 and 4.
pub fn smoke(c: u32) -> Option<usize> {
    match c {
        0..=2 => Some(0),
        3..=4 => Some(1),
        5..=16 => Some(2 + (c as usize - 5) / 3),
        _ => None,
    }
}

/// How far from the crate's position a puff of smoke appears, as far as
/// measured: `dx` -17 to 14 and `dy` 25 to 34. How they're chosen isn't
/// in the spec; they come from a number, one per break.
pub fn smoke_offset(seed: u32) -> (i32, i32) {
    let mut h = seed.wrapping_mul(0x9e37_79b1) ^ 0x85eb_ca6b;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    ((h % 32) as i32 - 17, 25 + ((h >> 8) % 10) as i32)
}

/// The animal's waiting sequence, as the numbers of [`ANIMAL`] (from 1),
/// one per counted frame.
fn sequence() -> &'static [u8] {
    static SEQUENCE: OnceLock<Vec<u8>> = OnceLock::new();
    SEQUENCE.get_or_init(|| {
        let mut out = Vec::new();
        let mut each = |pictures: &[u8], frames: usize| {
            for &p in pictures {
                out.extend(std::iter::repeat_n(p, frames));
            }
        };
        each(&[22, 21, 20, 19], 6);
        each(&[18], 26);
        each(&[19, 20, 21, 22], 6);
        each(&[1], 21);
        each(&(2..=13).collect::<Vec<u8>>(), 6);
        each(&[14, 15, 16, 15].repeat(5), 4);
        let mut seven = vec![16, 17, 18];
        seven.extend((11..=17).rev());
        seven.extend(12..=22);
        each(&seven, 6);
        each(&[1, 2, 3], 4);
        each(&[4, 5, 6, 5].repeat(10), 8);
        each(&[4, 3, 2], 4);
        out
    })
}

/// How many counted frames the waiting sequence lasts.
pub const SEQUENCE_FRAMES: usize = 717;

/// The index into [`ANIMAL`] of the waiting animal's picture, `a`
/// counted frames after B + 18. On B + 18 it's already on frame 1.
pub fn waiting(a: u32) -> usize {
    let frame = (a as usize + 1) % SEQUENCE_FRAMES;
    usize::from(sequence()[frame]) - 1
}

/// The index into [`MOUNTING`] of the mounting animal's picture,
/// `since` frames after M, or `None` on the frames not covered.
pub fn mounting(since: u32) -> Option<usize> {
    match since {
        2 => Some(0),
        3..=5 => Some(1),
        6..=8 => Some(2),
        9..=13 => Some(3),
        14..=21 => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waiting_sequence_is_717_frames_of_the_spec_s_parts() {
        let s = sequence();
        assert_eq!(s.len(), SEQUENCE_FRAMES);
        // Parts 1 and 2 and 3 start at 0, 24 and 50; part 4 at 74.
        assert_eq!(s[..7], [22, 22, 22, 22, 22, 22, 21]);
        assert_eq!(s[23], 19);
        assert_eq!(s[24], 18);
        assert_eq!(s[49], 18);
        assert_eq!(s[50], 19);
        assert_eq!(s[74], 1);
        assert_eq!(s[94], 1);
        assert_eq!(s[95], 2);
        // Part 5 ends at 166; part 6 is 80 long; part 7 starts at 247.
        assert_eq!(s[166], 13);
        assert_eq!(s[167], 14);
        assert_eq!(s[246], 15);
        assert_eq!(s[247], 16);
        assert_eq!(s[253], 17);
        assert_eq!(s[259], 18);
        assert_eq!(s[265], 17);
        assert_eq!(s[301], 11);
        assert_eq!(s[306], 11);
        assert_eq!(s[307], 12);
        assert_eq!(s[372], 22);
        // Part 8 at 373, part 9 at 385, part 10 at 705.
        assert_eq!(s[373], 1);
        assert_eq!(s[385], 4);
        assert_eq!(s[705], 4);
        assert_eq!(s[716], 2);
    }

    #[test]
    fn the_animal_is_on_frame_one_on_its_first_frame_and_starts_again_after_716() {
        assert_eq!(waiting(0), 21);
        assert_eq!(waiting(4), 21);
        assert_eq!(waiting(5), 20);
        assert_eq!(waiting(715), usize::from(sequence()[716]) - 1);
        assert_eq!(waiting(716), 21);
    }

    #[test]
    fn the_flash_and_smoke_step_on_counted_frames() {
        assert_eq!(flash(3), Some(0));
        assert_eq!([flash(4), flash(8), flash(9)], [Some(1), Some(1), Some(2)]);
        assert_eq!([flash(34), flash(38), flash(39)], [Some(7), Some(7), None]);
        assert_eq!(
            [smoke(4), smoke(5), smoke(7), smoke(8)],
            [Some(1), Some(2), Some(2), Some(3)]
        );
        assert_eq!([smoke(14), smoke(16), smoke(17)], [Some(5), Some(5), None]);
        assert_eq!(flash_rise(4), 3.75);
    }

    #[test]
    fn mounting_shows_five_pictures_from_two_to_twenty_one() {
        let shown: Vec<_> = (0..23).map(mounting).collect();
        assert_eq!(shown[..2], [None, None]);
        assert_eq!(shown[2], Some(0));
        assert_eq!(shown[13], Some(3));
        assert_eq!(shown[14], Some(4));
        assert_eq!(shown[21], Some(4));
        assert_eq!(shown[22], None);
    }

    #[test]
    fn smoke_stays_in_the_measured_range() {
        for seed in 0..2000 {
            let (dx, dy) = smoke_offset(seed);
            assert!(
                (-17..=14).contains(&dx) && (25..=34).contains(&dy),
                "{seed}"
            );
        }
    }
}
