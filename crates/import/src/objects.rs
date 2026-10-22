//! The objects' picture sheets, which the importer can't write yet.
//!
//! The spec names the pictures of the letters, sparkles, collectibles,
//! bunches, thrown objects, animal crate, walkers, stingers, critters, tyres and thrower by hash, but
//! not which sprite palette colours each group or where it is
//! (`formats/sprites.md`, "Sprite palettes"; spec#58). Until it does, a
//! real import has no such sheets and the game draws its own art for
//! these objects. The import report says so.

/// The sheets the game reads for objects, with what they hold. The
/// importer writes none of them yet.
pub const PENDING_SHEETS: &[(&str, &str)] = &[
    ("letter-pictures", "the letters"),
    ("spark-pictures", "the sparkles"),
    ("cluster-pictures", "the bunches"),
    (
        "thrown-pictures",
        "the thrown objects, their flashes and pieces",
    ),
    (
        "crate-pictures",
        "the animal crate, its smoke and the animal",
    ),
    ("stinger-pictures", "the stingers"),
    ("critter-pictures", "the critters"),
    ("tyre-pictures", "the tyres"),
    ("stiff-tyre-pictures", "the stiff tyres"),
    ("thrower-pictures", "the thrower"),
    ("ball-pictures", "the thrower's balls"),
    ("small-walker", "the small walkers"),
    ("medium-walker", "the medium walkers"),
    ("large-walker", "the large walkers"),
];

/// Import report notes for the objects whose pictures aren't converted.
pub fn notes() -> impl Iterator<Item = String> {
    PENDING_SHEETS.iter().map(|(sheet, what)| {
        format!(
            "{what} ({sheet}): not converted, the spec doesn't yet say their colours; the \
             engine's own art stands in"
        )
    })
}
