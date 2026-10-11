//! World maps for the Clamber engine.
//!
//! A world map (`.map`, TOML) is a graph of [`Node`]s (level entrances,
//! warps to other maps, save points, content-defined events such as shops,
//! and plain waypoints) joined by [`Path`]s drawn as curves through
//! waypoints. Nodes and paths can carry unlock [`Condition`]s on the
//! player's [`Progress`]; hidden ones stay invisible until unlocked, and
//! [`WorldMap::reveal`] reports each reveal once so it can be animated.
//!
//! Maps nest through warp nodes: an overworld warps into a regional map
//! and back. A map can instead be free-roaming ([`Mode::Free`]): the player
//! moves over the terrain of the map's scene level, optionally in a
//! vehicle, and enters nodes by touching them. Animated elements are
//! [`Decoration`]s and node sprites.
//!
//! [`Progress`] is what the map needs from save data, and serializes.

mod curve;
mod map;
mod progress;

pub use curve::{Walker, cosine, distance, length, smooth};
pub use map::{
    Condition, Decoration, Exit, FORMAT, Free, MapError, Mode, Node, NodeKind, Path, Vehicle,
    WorldMap, check_warps,
};
pub use progress::{Position, Progress};
