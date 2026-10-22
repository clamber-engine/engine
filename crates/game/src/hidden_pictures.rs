//! The picture of a hidden barrel (`spec/behavior/hidden-objects.md`,
//! "Pictures").
//!
//! A `hidden-barrel` and a `hidden-metal-barrel` are drawn as one picture,
//! a patch of bare earth, with its origin at their position until they
//! are set off; the first mirrored left to right. A hidden bunch isn't
//! drawn. The picture is [`LABEL`] in the pack's [`SHEET`]; without one,
//! the engine's art of the same name is drawn.

use clamber_sim::{Behavior, Fixed, HIDDEN, Phase, Thing};

/// The sheet the patch comes from.
pub const SHEET: &str = "hidden-patch";

/// The label of the patch.
pub const LABEL: &str = "patch";

/// Whether `thing` is drawn as the patch, and if so whether mirrored:
/// a hidden barrel (whose item is a barrel that goes left) or hidden
/// metal barrel that hasn't been set off.
pub fn shown(thing: &Thing) -> Option<bool> {
    if thing.kind != HIDDEN || thing.phase != Phase::Ready {
        return None;
    }
    match thing.archetype?.behavior {
        Behavior::Hidden(h) if h.fall_height > 0 => Some(h.item_speed.x < Fixed::ZERO),
        _ => None,
    }
}
