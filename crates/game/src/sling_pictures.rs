//! What a launch barrel, the smoke at a catch and the flash at a shot
//! show (`spec/behavior/launch-barrel.md`, "Pictures").
//!
//! The barrel's pictures are in the pack's [`SHEET`]. A barrel that shoots
//! left shows [`REST`] on every frame; one that shoots down shows
//! [`DOWN_REST`], opens through three pictures before the shot and is
//! [`REST`] again just before it breaks. The frames of the catch itself
//! (C) are counted from `0` as `n`. Where the spec doesn't say what a
//! barrel shows (the player covers it then), it keeps the picture it had.
//! The smoke and the flash are those of the animal crate and the thrown
//! objects; a barrel's own pictures are used only when the sheet has all
//! those of its kind.

use clamber_fixed::Fixed;
use clamber_sim::Sling;

/// The sheet the barrels' pictures come from.
pub const SHEET: &str = "barrel-pictures";

pub const REST: &str = "barrel-rest";
pub const OPEN: [&str; 3] = ["barrel-open-1", "barrel-open-2", "barrel-open-3"];
pub const DOWN_REST: &str = "barrel-down-rest";

/// The labels a barrel that shoots left needs.
pub const LEFT: [&str; 1] = [REST];

/// The labels a barrel that shoots down needs.
pub const DOWN: [&str; 5] = [REST, OPEN[0], OPEN[1], OPEN[2], DOWN_REST];

/// The frames since the catch after which nothing of it is left: the
/// flash of a barrel that shoots down is gone from C + 68.
pub const LAST: u32 = 68;

/// Whether the barrel shoots straight down.
pub fn shoots_down(sling: &Sling) -> bool {
    sling.shot.x == Fixed::ZERO && sling.shot.y > Fixed::ZERO
}

/// The label of the picture of a barrel `n` frames after a catch, or
/// before one (`None`).
pub fn picture(down: bool, n: Option<u32>) -> &'static str {
    if !down {
        return REST;
    }
    match n {
        None | Some(0..=4) => DOWN_REST,
        Some(5..=16) => DOWN_REST,
        Some(17..=20) => OPEN[2],
        Some(21..=23) => OPEN[1],
        Some(24..=26) => OPEN[0],
        Some(_) => REST,
    }
}

/// The index into the smoke's six pictures (`hutch_pictures::SMOKE`) `n`
/// frames after a catch, or `None` once it's gone (from 17).
pub fn smoke(n: u32) -> Option<usize> {
    match n {
        0..=1 => Some(0),
        2..=16 => Some(1 + (n as usize - 2) / 3),
        _ => None,
    }
}

/// The index into the flash's eight pictures `k` frames after the shot,
/// or `None` once it's gone (from 39): `flash-1` for 4 frames, then each
/// of the others for 5.
pub fn flash(k: u32) -> Option<usize> {
    match k {
        0..=3 => Some(0),
        4..=38 => Some(1 + (k as usize - 4) / 5),
        _ => None,
    }
}

/// How far the flash has moved `k` frames after the shot: 271/256 px on
/// that frame and on each after.
pub fn flash_moved(k: u32) -> f64 {
    f64::from(k + 1) * 271.0 / 256.0
}

/// The clock of a barrel after a catch: the frames since it, or `None`
/// for a barrel that hasn't caught. It counts on until the flash is gone.
pub fn next(clock: Option<u32>, caught: bool, ready: bool) -> Option<u32> {
    match clock {
        _ if ready => None,
        Some(n) => Some((n + 1).min(LAST + 1)),
        None => caught.then_some(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_down_barrel_opens_and_closes_before_it_breaks() {
        let at = |n| picture(true, Some(n));
        assert_eq!([at(4), at(17), at(20)], [DOWN_REST, OPEN[2], OPEN[2]]);
        assert_eq!([at(21), at(23), at(24)], [OPEN[1], OPEN[1], OPEN[0]]);
        assert_eq!([at(26), at(27), at(28)], [OPEN[0], REST, REST]);
        assert_eq!(picture(true, None), DOWN_REST);
        assert_eq!(picture(false, Some(30)), REST);
    }

    #[test]
    fn the_smoke_shows_each_picture_three_frames() {
        assert_eq!([smoke(0), smoke(1), smoke(2)], [Some(0), Some(0), Some(1)]);
        assert_eq!([smoke(4), smoke(5), smoke(16)], [Some(1), Some(2), Some(5)]);
        assert_eq!(smoke(17), None);
    }

    #[test]
    fn the_flash_lasts_39_frames_and_moves_271_256ths_a_frame() {
        assert_eq!(
            [flash(3), flash(4), flash(8), flash(9)],
            [Some(0), Some(1), Some(1), Some(2)]
        );
        assert_eq!([flash(38), flash(39)], [Some(7), None]);
        assert_eq!(flash_moved(0), 271.0 / 256.0);
        assert_eq!(flash_moved(3), 4.0 * 271.0 / 256.0);
    }

    #[test]
    fn the_clock_starts_on_a_catch_and_stops_once_ready_again() {
        assert_eq!(next(None, false, false), None);
        assert_eq!(next(None, true, false), Some(0));
        assert_eq!(next(Some(0), true, false), Some(1));
        assert_eq!(next(Some(LAST + 1), true, false), Some(LAST + 1));
        assert_eq!(next(Some(30), false, true), None);
    }
}
