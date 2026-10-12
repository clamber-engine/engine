//! Levels for the Clamber engine.
//!
//! A level is a TOML file (`.level`) in world units: pixels, +y down, as
//! fixed-point numbers. A grid is an editor convenience, not part of the
//! format. A level has:
//!
//! - **layers**: images or tiles, each behind or in front of entities, with
//!   its own parallax factor
//! - **entities**: placed things with per-instance parameters
//! - **collision**: solid polygons and segments at any angle, one-way
//!   platforms, and surface types ([`Surface`]), separate from visuals
//! - **regions**: areas with water, wind, darkness, physics overrides and
//!   camera rules
//! - **camera** rules for the whole level: limits and look-ahead
//! - **music**: a track to play, with optional loop points
//!
//! Maps made in Tiled can be imported ([`tiled`]).

mod collision;
mod geom;
mod level;
pub mod tiled;

pub use clamber_fixed::Fixed;
pub use collision::{Collider, Collision, Dir, Hit, Shape};
pub use geom::{Area, Rect, Vec2};
pub use level::{
    Camera, CameraRule, Depth, Entity, FORMAT, Layer, LayerContent, Level, LevelError, Region,
    Surface, Tile, TileLayer, Tileset, Track, builtin_surfaces,
};
