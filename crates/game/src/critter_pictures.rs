//! Which picture a critter shows on each frame
//! (`spec/behavior/critter.md`, "Pictures").
//!
//! Flying, its six wing pictures repeat every 10 flying frames, counted
//! from the frame it appeared and again from the one it flies on after a
//! perch; frames it holds still on aren't counted. Perched, it shows a
//! fixed list over [`PERCH`] frames. It is mirrored on the frames it
//! flies leftwards, and keeps its mirroring while held still or perched.
//!
//! The pictures are labelled [`LABELS`] in the pack's [`SHEET`]. When the
//! sheet lacks one of the six, the engine's art turns along with the same
//! lists, mirrored the same way.

use clamber_sim::Thing;

/// The sheet a critter's pictures come from.
pub const SHEET: &str = "critter-pictures";

/// The labels of the six pictures.
pub const LABELS: [&str; 6] = ["wing-1", "wing-2", "wing-3", "wing-4", "wing-5", "wing-6"];

/// Where a critter is in its pictures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// Counted flying frames since it appeared or last perched.
    pub flying: u32,
    /// Frames since the perch began, while it perches.
    pub perched: Option<u32>,
    /// Drawn mirrored left to right.
    pub mirrored: bool,
}

impl Clock {
    /// The clock of a critter on the frame it appears.
    pub fn appearing(thing: &Thing) -> Clock {
        Clock {
            flying: 0,
            perched: None,
            mirrored: thing.vx.is_negative(),
        }
    }

    /// The clock a frame on, after `thing` took its step. `perch` is the
    /// archetype's perch length: a step that leaves `perch - 1` frames of
    /// standing still has begun a perch.
    pub fn next(self, thing: &Thing, perch: u32) -> Clock {
        if thing.still {
            return self;
        }
        let standing = thing.clock > 0;
        match self.perched {
            // The perch's last frame leaves no frames to stand, as the
            // frame it flies again on.
            Some(a) if a + 1 < perch => Clock {
                perched: Some(a + 1),
                ..self
            },
            Some(_) => Clock {
                flying: 0,
                perched: None,
                mirrored: thing.vx.is_negative(),
            },
            None if standing && thing.clock + 1 == perch => Clock {
                perched: Some(0),
                ..self
            },
            // Standing at the world's start, before it first flies.
            None if standing => self,
            None => Clock {
                flying: self.flying + 1,
                perched: None,
                mirrored: thing.vx.is_negative(),
            },
        }
    }

    /// The index into [`LABELS`] of the picture to show.
    pub fn picture(self) -> usize {
        match self.perched {
            None => FLYING[(self.flying % 10) as usize].into(),
            Some(a) => perched(a),
        }
    }
}

const FLYING: [u8; 10] = [1, 2, 3, 4, 5, 4, 3, 2, 1, 0];

/// The picture `a` frames into a perch.
fn perched(a: u32) -> usize {
    let rise = |a: u32| 1 + (a / 6) as usize;
    let fall = |a: u32| 4 - (a / 6) as usize;
    match a {
        0..=79 => 0,
        80..=103 => rise(a - 80),
        104..=154 => 5,
        155..=178 => fall(a - 155),
        179..=259 => 0,
        260..=283 => rise(a - 260),
        284..=334 => 5,
        335..=358 => fall(a - 335),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flying_repeats_every_ten_frames() {
        let shown: Vec<usize> = (0..12)
            .map(|flying| {
                Clock {
                    flying,
                    ..Clock::default()
                }
                .picture()
            })
            .collect();
        assert_eq!(shown, [1, 2, 3, 4, 5, 4, 3, 2, 1, 0, 1, 2]);
    }

    #[test]
    fn a_perch_follows_the_spec_s_list() {
        let list: Vec<usize> = (0..360).map(perched).collect();
        assert_eq!(list.len(), 360);
        assert_eq!(list[79], 0);
        assert_eq!(
            &list[80..104].iter().step_by(6).copied().collect::<Vec<_>>(),
            &[1, 2, 3, 4]
        );
        assert_eq!(list[103], 4);
        assert_eq!((list[104], list[154]), (5, 5));
        assert_eq!(
            &list[155..179]
                .iter()
                .step_by(6)
                .copied()
                .collect::<Vec<_>>(),
            &[4, 3, 2, 1]
        );
        assert_eq!((list[178], list[179], list[259]), (1, 0, 0));
        assert_eq!((list[260], list[283], list[284]), (1, 4, 5));
        assert_eq!((list[334], list[335], list[358], list[359]), (5, 4, 1, 0));
    }
}
