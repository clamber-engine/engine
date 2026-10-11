//! Game packs for the Clamber engine.
//!
//! A pack defines how its world plays; the engine knows nothing about
//! specific games. Physics comes in layers, each changing only what it
//! needs to:
//!
//! 1. the chosen [feel preset](preset)'s world physics, built on its bases
//!    down to `default` (the engine's generic defaults)
//! 2. the character's own movement profile from the pack's roster
//! 3. the preset's adjustments for that character's roster slot
//! 4. the level region the character is in, such as water or low gravity
//!    ([`Physics::patched`])
//!
//! Content files are TOML. Numbers are stored as [`Fixed`] so the
//! simulation stays deterministic.

mod load;
mod pack;
pub mod physics;
pub mod preset;

pub use clamber_fixed::Fixed;
pub use load::LoadError;
pub use pack::{
    AbilityParams, Character, CharacterFeel, Feel, FeelWarning, Pack, Selection, UnknownPreset,
};
pub use physics::{Adjust, Param, Patch, Physics, PhysicsPatch};
pub use preset::{Preset, Presets};

/// Reads a table of movement-parameter adjustments, such as a level
/// region's physics override. `path` names the table in error messages.
pub fn read_physics_patch(
    table: &toml::Table,
    origin: &str,
    path: &str,
) -> Result<PhysicsPatch, LoadError> {
    load::Reader { origin }.physics_patch(path, table)
}
