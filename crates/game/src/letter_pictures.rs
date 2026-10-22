//! Which picture a letter shows (`spec/behavior/letters.md`,
//! "Pictures", "The letter").
//!
//! Letter `n` of a level's word shows the picture labelled `letter-pic-n`
//! from the pack's [`SHEET`] on every frame it's there, unmirrored, with
//! its origin a px left of the letter's position. It shows them only if
//! the sheet has all four ([`LABELS`]); otherwise all letters show the
//! engine's own art. A letter past the fourth has no picture of its own.

use clamber_sim::{Behavior, Thing};

/// The sheet a letter's picture comes from.
pub const SHEET: &str = "letter-pictures";

/// The four letters' pictures.
pub const LABELS: [&str; 4] = [
    "letter-pic-1",
    "letter-pic-2",
    "letter-pic-3",
    "letter-pic-4",
];

/// The label of letter `letter`'s picture (1 for the first), if it has
/// one.
pub fn label(letter: u8) -> Option<&'static str> {
    LABELS.get(usize::from(letter).checked_sub(1)?).copied()
}

/// The picture label of `thing`, if it's a letter with a picture.
pub fn shown(thing: &Thing) -> Option<&'static str> {
    match thing.archetype?.behavior {
        Behavior::Collect { letter, .. } => label(letter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_of_the_first_four_letters_has_its_own_picture() {
        assert_eq!(label(0), None);
        assert_eq!(label(1), Some("letter-pic-1"));
        assert_eq!(label(4), Some("letter-pic-4"));
        assert_eq!(label(5), None);
    }
}
