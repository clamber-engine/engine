//! Which picture a sign shows (`spec/behavior/signs.md`, "Pictures").
//!
//! A sign has the same picture on every frame, never mirrored; which of
//! three it is depends on its placement reference. The pictures are
//! labelled [`LABELS`] in the pack's [`SHEET`], and a sign's `picture`
//! (its archetype's value) is an index into them. The sheet is used when
//! it has all three; otherwise the engine's art is drawn.

/// The sheet the pictures of the signs come from.
pub const SHEET: &str = "sign-pictures";

/// The labels of the three pictures: the arrow, the word and the
/// scene's board.
pub const LABELS: [&str; 3] = ["sign-arrow", "sign-word", "sign-scene"];
