//! Levels for the Clamber engine.
//!
//! A level is a TOML file (`.level`) in world units: pixels, +y down, as
//! fixed-point numbers. A grid is an editor convenience, not part of the
//! format. A level has:
//!
//! - **layers**: images, tiles or freely placed props, each behind or in
//!   front of entities, with its own parallax factor and opacity, drawn
//!   smoothly or as pixel art, at any number of texels per world unit,
//!   optionally swaying
//! - **arrivals**: named points other areas lead into, where the
//!   characters walk in, come out of an entrance, or the level begins
//!   as at its start
//! - **entities**: placed things with per-instance parameters
//! - **sections**: optionally, when entities appear: section by section,
//!   by the path the view takes ([`Section`])
//! - **collision**: solid polygons and segments at any angle, one-way
//!   platforms, thin or thick, and surface types ([`Surface`]), separate
//!   from visuals
//! - **regions**: areas with water, wind, darkness, physics overrides and
//!   camera rules
//! - **camera** rules for the whole level: limits, look-ahead and a
//!   swaying view
//! - **music**: a track to play, with optional loop points
//! - **scene**: optionally, that the area plays by itself and ends on
//!   its own, and where play goes on after it ([`Scene`])
//! - **lighting**: an ambient colour and a key light, and point lights,
//!   which shade layers and sprites that have normal maps
//!
//! Maps made in Tiled can be imported ([`tiled`]).

mod collision;
mod geom;
mod level;
pub mod tiled;

pub use clamber_fixed::Fixed;
pub use collision::{Collider, Collision, Dir, Hit, Shape, Span};
pub use geom::{Area, Rect, Vec2};
pub use level::{
    Arrival, Band, Camera, CameraRule, Depth, Entity, FORMAT, KeyLight, Layer, LayerContent, Level,
    LevelError, Light, Lighting, Prop, Region, Rgb, Scene, Section, Spot, Surface, Sway, Tile,
    TileLayer, Tileset, Track, builtin_surfaces,
};
