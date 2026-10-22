//! The level format: TOML, in world units. See `content/test/` for
//! examples and [`crate`] for an overview.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use clamber_fixed::Fixed;
use clamber_pack::{Adjust, PhysicsPatch};
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use toml::{Table, Value};

use crate::collision::{Collider, Collision, Dir, Shape};
use crate::geom::{self, Area, Rect, Vec2};

/// The format version this engine reads and writes.
pub const FORMAT: u32 = 1;

/// Coordinates are limited to ±2^30 pixels so geometry can't overflow.
const MAX_COORD: f64 = (1u64 << 30) as f64;

const BUILTIN_SURFACES: &str = include_str!("../../../content/base/surfaces.toml");

#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub name: String,
    /// The level spans from (0, 0) to `size`.
    pub size: Vec2,
    /// Where the lead character starts, at its feet.
    pub start: Vec2,
    /// The other ways play comes into the level, from other areas.
    pub arrivals: Vec<Arrival>,
    /// Drawn in order; [`Depth`] puts each behind or in front of entities.
    pub layers: Vec<Layer>,
    pub entities: Vec<Entity>,
    pub collision: Collision,
    /// Applied in order, so a later region overrides an earlier one.
    pub regions: Vec<Region>,
    pub camera: Camera,
    /// Built-in surfaces with the level's own on top.
    pub surfaces: BTreeMap<String, Surface>,
    /// What plays during the level.
    pub music: Option<Track>,
    /// How art with normal maps is shaded.
    pub lighting: Lighting,
    /// Point lights placed in the level.
    pub lights: Vec<Light>,
    /// When entities appear, if they appear section by section as the
    /// view goes; empty if they don't.
    pub sections: Vec<Section>,
    /// How the area plays by itself, if it's a scene.
    pub scene: Option<Scene>,
}

/// A way play comes into a level from another area: where the lead
/// character is, at its feet, and whether the characters walk in, come
/// out of an entrance or drop in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrival {
    /// What exits that lead here call it, unique in the level.
    pub name: String,
    pub at: Vec2,
    /// The way the characters walk in towards `at`, [`Dir::Left`] or
    /// [`Dir::Right`]; `None` if the level begins at `at` as it does at
    /// its start.
    pub walk: Option<Dir>,
    /// The `id` of the entity the characters come out of, as at the
    /// level's start out of its entrance, if they don't come out of that
    /// one. An entrance an arrival names isn't the level's own.
    pub entrance: Option<String>,
    /// The lead comes in in the air at `at` and falls from there, out of
    /// no entrance; it can't walk in or name one.
    pub drop: bool,
}

impl Level {
    /// The arrival called `name`, if there is one.
    pub fn arrival(&self, name: &str) -> Option<&Arrival> {
        self.arrivals.iter().find(|a| a.name == name)
    }
}

/// A run of entities that appear section by section: which sections are
/// open depends on the path the view has taken, and the spots of an open
/// section appear in their order as they come near the view.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub spots: Vec<Spot>,
    /// Where it meets the next section; `None` for the last.
    pub meets: Option<Vec2>,
}

/// Entities in a section that appear together.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    /// Indices into the level's entities: one, or a group's.
    pub entities: Vec<usize>,
    /// Where it counts as being, if not where its first entity is.
    pub at: Option<Vec2>,
    /// Whether one entity also has to be within the view's rows to
    /// appear.
    pub rows: bool,
    /// Whether its entities also leave once they're outside the view's
    /// rows, even while the spot is near the view.
    pub leave_rows: bool,
    /// Whether each of its entities takes one of the preset's `pictures`
    /// while it's there.
    pub picture: bool,
}

/// A colour as red, green and blue, written `#rrggbb`.
pub type Rgb = [u8; 3];

/// The light every lit thing in a level gets, whatever its place. Art
/// that has no normal map is drawn as it is, unlit.
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    /// Light from everywhere, so nothing is darker than this.
    pub ambient: Rgb,
    /// A distant light, like the sun.
    pub key: KeyLight,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyLight {
    /// Towards the light: x right, y down, z out of the screen towards
    /// the viewer. Not all zero; its length doesn't matter.
    pub direction: [f64; 3],
    pub color: Rgb,
    /// Multiplies `color`, at least 0.
    pub intensity: f64,
}

impl Default for Lighting {
    /// Light from above left and in front, with enough ambient that a
    /// surface facing the viewer is shown at about its own colour.
    fn default() -> Lighting {
        Lighting {
            ambient: [0xbf; 3],
            key: KeyLight {
                direction: [-1.0, -1.0, 2.0],
                color: [0xff; 3],
                intensity: 0.3125,
            },
        }
    }
}

/// A light at a point in the level, fading out to nothing at `radius`.
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub at: Vec2,
    pub color: Rgb,
    /// How far it reaches, in world units, more than 0.
    pub radius: Fixed,
    /// Multiplies `color` at the light's centre, at least 0.
    pub intensity: f64,
}

/// An area that plays by itself, as a scene between two parts of a
/// level. Play comes in at an arrival, and the characters walk in if it
/// has them walk; from then on the lead stands still where it is, the
/// other character too, nothing held or pressed has any effect and
/// nothing else moves. The picture fades out to black, and the area ends
/// on its own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Scene {
    /// The area's last frame, counting its frames from 0.
    pub last: u32,
    /// The frame the fade-out starts from, at full brightness, before
    /// `last`.
    pub fade: u32,
    /// How many frames each step of the fade-out lasts, at least 1: the
    /// picture is black once the steps from `fade` reach `last`.
    pub fade_step: u32,
    /// How many frames shorter the scene is when a character other than
    /// the first leads: `fade` and `last` both come that many frames
    /// earlier. 0 if the lead makes no difference.
    pub shorter: u32,
    /// The other area of the pack play goes on in after the last frame,
    /// the file name of its level without `.level`; this level without
    /// one.
    pub area: Option<String>,
    /// The arrival play comes in at there; without one, the area begins
    /// as at its start.
    pub arrival: Option<String>,
}

/// A music track: an Ogg Vorbis, MP3, FLAC or WAV file. Loop points are
/// for formats without loop tags; without them the file's
/// `LOOPSTART`/`LOOPLENGTH` tags apply, or the whole track loops.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Track {
    /// Relative to the level file.
    pub file: String,
    /// Where the loop starts, in frames at the file's sample rate.
    pub loop_start: Option<u64>,
    /// The loop's length in frames; without it the loop runs to the end.
    pub loop_length: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub depth: Depth,
    /// How far the layer scrolls per pixel of camera movement: 1 moves
    /// with the world, 0 stays fixed on screen.
    pub parallax: Vec2,
    /// Where the layer's top left is when the view's top left is at the
    /// level's origin. With a parallax of 1 that's its place in the world.
    pub offset: Vec2,
    pub content: LayerContent,
    /// Texels of the layer's image or tileset per world unit, at least 1:
    /// art drawn at a higher resolution covers the same part of the
    /// world. Tile sizes, margins and spacing stay in texels.
    pub scale: u32,
    /// Draw the image with sharp texel edges at any size, rather than
    /// filtered smoothly with mipmaps.
    pub pixel_art: bool,
    /// How the layer's colours change with the level's dimming, when a
    /// preset has one.
    pub dim: Dim,
    /// The layer is drawn darker than its images: each channel, worked at
    /// 5 bits, 4 less (at least 0) and then halved, after any dimming.
    pub darker: bool,
    /// How opaque the layer is drawn, from 0 (not at all) to 1 (as its
    /// images are), so leaves in front of the player don't hide what's
    /// behind them.
    pub opacity: f64,
    /// Each of the layer's images leaning from side to side, if it sways.
    pub sway: Option<Sway>,
    /// Bands of an image layer's rows, from its top, that each scroll
    /// across at their own parallax instead of the layer's. Rows below
    /// the last band aren't drawn. Empty for a layer drawn whole.
    pub bands: Vec<Band>,
    /// How much further the layer is drawn on every other step: on the
    /// pictures after the level's odd steps, counting its first as 0.
    pub alternate: Vec2,
}

/// How a layer's colours change with the level's dimming.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Dim {
    /// They don't.
    #[default]
    None,
    /// Each colour changes as the preset's dimming says.
    Palette,
    /// Each texel goes in equal steps, one a step of the dimming, to the
    /// colour of the same place in this image, relative to the level
    /// file. Rounded down at 5 bits a channel, it shows that colour
    /// exactly after the last step.
    Night(String),
}

/// Rows of an image layer that scroll across together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    /// How many of the image's rows, after the bands above.
    pub rows: u32,
    /// How far it scrolls across per pixel of camera movement.
    pub parallax: Fixed,
}

/// Images leaning from side to side about their bottom edge, as plants
/// and leaves do in a breeze. It's only drawn: nothing in the world moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sway {
    /// How far an image's top moves to either side, in world units.
    pub amount: f64,
    /// Seconds for one swing there and back.
    pub period: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    /// Behind entities.
    Back,
    /// In front of entities.
    Front,
    /// Behind the characters, except where they are drawn behind it:
    /// in the walk-outs ([`World::behind_scenery`] in the sim), where
    /// its opaque pixels hide them.
    Raised,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerContent {
    /// An image, optionally repeated without end along either axis. The
    /// path is relative to the level file, as are tileset images.
    Image {
        path: String,
        /// A normal map laid out like the image, to shade it by the
        /// level's lights.
        normals: Option<String>,
        repeat_x: bool,
        repeat_y: bool,
    },
    Tiles(TileLayer),
    /// Images placed anywhere, such as trees, logs and ferns, in order.
    Props(Vec<Prop>),
}

/// An image placed in a props layer. Like all layers it has no effect on
/// collision.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    /// Relative to the level file.
    pub image: String,
    /// A normal map laid out like the image, to shade it by the level's
    /// lights.
    pub normals: Option<String>,
    /// Where the middle of the image's bottom edge goes, from the layer's
    /// top left: where a plant meets the ground.
    pub at: Vec2,
    /// Drawn mirrored left to right.
    pub flip: bool,
}

/// Tiles: a drawing convenience. They have no effect on collision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileLayer {
    pub tileset: Tileset,
    /// Tiles per row; rows follow from the number of tiles.
    pub columns: u32,
    /// Row by row from the top left.
    pub tiles: Vec<Tile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tileset {
    pub image: String,
    /// A normal map laid out like the image, to shade the tiles by the
    /// level's lights.
    pub normals: Option<String>,
    pub tile_width: u32,
    pub tile_height: u32,
    /// Pixels around the outside of the image, and between tiles.
    pub margin: u32,
    pub spacing: u32,
}

/// A tile: 0 is empty, otherwise the low 29 bits are the tile's index in
/// the tileset plus one, and the top three bits flip it horizontally,
/// vertically, and diagonally (swapping x and y), applied in that order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tile(pub u32);

impl Tile {
    pub const FLIP_X: u32 = 1 << 31;
    pub const FLIP_Y: u32 = 1 << 30;
    pub const FLIP_DIAGONAL: u32 = 1 << 29;
    const INDEX: u32 = (1 << 29) - 1;

    /// The tile's index in the tileset, or `None` for an empty tile.
    pub fn index(self) -> Option<u32> {
        (self.0 & Tile::INDEX).checked_sub(1)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    /// What it is, such as `spring` or `walker`; entity definitions give
    /// the meaning.
    pub kind: String,
    /// Unique within the level, for triggers to refer to.
    pub id: Option<String>,
    pub at: Vec2,
    /// Per-instance parameters, as written.
    pub params: Table,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub name: Option<String>,
    pub area: Area,
    /// Characters inside are swimming.
    pub water: bool,
    /// A push in px/frame² added to anything inside.
    pub wind: Option<Vec2>,
    /// 0 is normal lighting, 1 is fully dark.
    pub darkness: Option<Fixed>,
    /// Movement changes inside, applied on top of the character's physics.
    pub physics: Option<PhysicsPatch>,
    pub camera: Option<CameraRule>,
}

/// Camera rules for the whole level, or inside a region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CameraRule {
    /// The camera's view stays inside this rectangle.
    pub limits: Option<Rect>,
    /// Fixes the centre of the view on this x or y. A region with `lock_x`
    /// and limits taller than the screen is a vertical section.
    pub lock_x: Option<Fixed>,
    pub lock_y: Option<Fixed>,
    /// How far ahead of the character's facing and motion to look.
    pub look_ahead: Option<Vec2>,
    /// The simulated view of a preset with `view_height` above 0
    /// (`clamber_sim::View`) follows vertically at its quick rates, not
    /// its usual ones. It's also kept inside `limits` vertically, or the
    /// level's bounds without them.
    pub quick: Option<bool>,
    /// The simulated view of a preset with `view_sway` above 0 sways
    /// across (`clamber_sim::View`).
    pub sway: Option<bool>,
}

impl CameraRule {
    /// `self` with what `other` sets replacing it.
    pub fn then(self, other: &CameraRule) -> CameraRule {
        CameraRule {
            limits: other.limits.or(self.limits),
            lock_x: other.lock_x.or(self.lock_x),
            lock_y: other.lock_y.or(self.lock_y),
            look_ahead: other.look_ahead.or(self.look_ahead),
            quick: other.quick.or(self.quick),
            sway: other.sway.or(self.sway),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Camera {
    /// The level-wide rule; limits default to the level's bounds.
    pub rule: CameraRule,
}

/// What a surface type does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    /// Movement changes while standing on it.
    pub physics: PhysicsPatch,
    /// Touching it hurts.
    pub hazard: bool,
}

impl Level {
    /// Regions containing `p`, in order.
    pub fn regions_at(&self, p: Vec2) -> impl Iterator<Item = &Region> {
        self.regions.iter().filter(move |r| r.area.contains(p))
    }

    /// The camera rule in effect with the camera's target at `p`.
    pub fn camera_at(&self, p: Vec2) -> CameraRule {
        self.regions_at(p)
            .filter_map(|r| r.camera.as_ref())
            .fold(self.camera.rule, |rule, r| rule.then(r))
    }

    pub fn surface(&self, collider: usize) -> &Surface {
        &self.surfaces[&self.collision.colliders[collider].surface]
    }

    pub fn load(path: &std::path::Path) -> Result<Level, LevelError> {
        let origin = path.display().to_string();
        let src = std::fs::read_to_string(path).map_err(|e| LevelError::new(&origin, e))?;
        Level::from_toml_str(&src, &origin)
    }

    /// Reads a level. `origin` names the source in error messages.
    pub fn from_toml_str(src: &str, origin: &str) -> Result<Level, LevelError> {
        let file: File =
            toml::from_str(src).map_err(|e| LevelError::new(origin, e.to_string().trim_end()))?;
        Reader { origin }.level(file)
    }
}

/// A level that couldn't be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelError {
    pub origin: String,
    pub message: String,
}

impl LevelError {
    pub(crate) fn new(origin: &str, message: impl fmt::Display) -> LevelError {
        LevelError {
            origin: origin.to_string(),
            message: message.to_string(),
        }
    }
}

impl fmt::Display for LevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for LevelError {}

/// A number in a level file: an integer or a float, within ±2^30.
#[derive(Clone, Copy, Debug)]
struct N(Fixed);

impl<'de> Deserialize<'de> for N {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<N, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = N;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a number")
            }
            fn visit_i64<E: de::Error>(self, n: i64) -> Result<N, E> {
                self.visit_f64(n as f64)
            }
            fn visit_u64<E: de::Error>(self, n: u64) -> Result<N, E> {
                self.visit_f64(n as f64)
            }
            fn visit_f64<E: de::Error>(self, x: f64) -> Result<N, E> {
                if x.is_nan() || x.abs() > MAX_COORD {
                    return Err(E::custom(format!("{x} is out of range (±2^30)")));
                }
                Ok(N(Fixed::from_f64(x).expect("within range")))
            }
        }
        d.deserialize_any(V)
    }
}

fn v2([x, y]: [N; 2]) -> Vec2 {
    Vec2::new(x.0, y.0)
}

fn rect([x, y, w, h]: [N; 4]) -> Rect {
    Rect {
        x: x.0,
        y: y.0,
        w: w.0,
        h: h.0,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    format: u32,
    #[serde(default)]
    name: String,
    size: [N; 2],
    start: [N; 2],
    #[serde(default)]
    arrivals: Vec<ArrivalFile>,
    #[serde(default)]
    layers: Vec<LayerFile>,
    #[serde(default)]
    entities: Vec<EntityFile>,
    #[serde(default)]
    collision: Vec<ColliderFile>,
    #[serde(default)]
    regions: Vec<RegionFile>,
    camera: Option<CameraFile>,
    #[serde(default)]
    surfaces: BTreeMap<String, SurfaceFile>,
    music: Option<TrackFile>,
    lighting: Option<LightingFile>,
    #[serde(default)]
    lights: Vec<LightFile>,
    #[serde(default)]
    sections: Vec<SectionFile>,
    scene: Option<SceneFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneFile {
    last: u32,
    fade: u32,
    fade_step: u32,
    #[serde(default)]
    shorter: u32,
    area: Option<String>,
    arrival: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArrivalFile {
    name: String,
    at: [N; 2],
    walk: Option<String>,
    entrance: Option<String>,
    #[serde(default)]
    drop: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionFile {
    spots: Vec<SpotFile>,
    meets: Option<[N; 2]>,
}

/// An entity's index, or a table.
#[derive(Deserialize)]
#[serde(untagged)]
enum SpotFile {
    One(usize),
    Table {
        entities: Vec<usize>,
        at: Option<[N; 2]>,
        #[serde(default)]
        rows: bool,
        #[serde(default)]
        leave_rows: bool,
        #[serde(default)]
        picture: bool,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightingFile {
    ambient: Option<String>,
    key: Option<KeyLightFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyLightFile {
    direction: Option<[N; 3]>,
    color: Option<String>,
    intensity: Option<N>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightFile {
    at: [N; 2],
    #[serde(default = "white")]
    color: String,
    radius: N,
    intensity: Option<N>,
}

fn white() -> String {
    "#ffffff".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackFile {
    file: String,
    loop_start: Option<u64>,
    loop_length: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerFile {
    name: String,
    #[serde(default = "back")]
    depth: Depth,
    parallax: Option<[N; 2]>,
    offset: Option<[N; 2]>,
    image: Option<String>,
    normals: Option<String>,
    #[serde(default)]
    repeat: [bool; 2],
    tiles: Option<TilesFile>,
    props: Option<Vec<PropFile>>,
    #[serde(default = "one")]
    scale: u32,
    #[serde(default)]
    pixel_art: bool,
    #[serde(default)]
    dim: DimFile,
    #[serde(default)]
    darker: bool,
    opacity: Option<N>,
    sway: Option<SwayFile>,
    #[serde(default)]
    bands: Vec<BandFile>,
    alternate: Option<[N; 2]>,
}

/// `dim = true`, or `dim = { night = "image.png" }`.
#[derive(Default, Deserialize)]
#[serde(untagged, expecting = "`true`, `false` or `{ night = \"image.png\" }`")]
enum DimFile {
    #[default]
    None,
    Palette(bool),
    Night(NightFile),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NightFile {
    night: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BandFile {
    rows: u32,
    parallax: N,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PropFile {
    image: String,
    normals: Option<String>,
    at: [N; 2],
    #[serde(default)]
    flip: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SwayFile {
    amount: N,
    period: N,
}

fn one() -> u32 {
    1
}

fn back() -> Depth {
    Depth::Back
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TilesFile {
    tileset: String,
    normals: Option<String>,
    tile_size: [u32; 2],
    #[serde(default)]
    margin: u32,
    #[serde(default)]
    spacing: u32,
    columns: u32,
    data: Vec<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityFile {
    #[serde(rename = "type")]
    kind: String,
    id: Option<String>,
    at: [N; 2],
    #[serde(default)]
    params: Table,
}

#[derive(Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase", deny_unknown_fields)]
enum ColliderFile {
    Rect {
        rect: [N; 4],
        #[serde(default)]
        one_way: bool,
        #[serde(default)]
        hangs: bool,
        surface: Option<String>,
    },
    Polygon {
        points: Vec<[N; 2]>,
        #[serde(default)]
        one_way: bool,
        #[serde(default)]
        hangs: bool,
        surface: Option<String>,
    },
    Segment {
        from: [N; 2],
        to: [N; 2],
        #[serde(default)]
        one_way: bool,
        surface: Option<String>,
    },
    /// Consecutive segments through the points.
    Polyline {
        points: Vec<[N; 2]>,
        #[serde(default)]
        one_way: bool,
        surface: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegionFile {
    name: Option<String>,
    rect: Option<[N; 4]>,
    points: Option<Vec<[N; 2]>>,
    #[serde(default)]
    water: bool,
    wind: Option<[N; 2]>,
    darkness: Option<N>,
    physics: Option<Table>,
    camera: Option<CameraFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraFile {
    limits: Option<[N; 4]>,
    lock_x: Option<N>,
    lock_y: Option<N>,
    look_ahead: Option<[N; 2]>,
    quick: Option<bool>,
    sway: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceFile {
    physics: Option<Table>,
    #[serde(default)]
    hazard: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfacesFile {
    surfaces: BTreeMap<String, SurfaceFile>,
}

struct Reader<'a> {
    origin: &'a str,
}

impl Reader<'_> {
    fn err(&self, path: &str, message: impl fmt::Display) -> LevelError {
        LevelError::new(self.origin, format!("{path}: {message}"))
    }

    fn level(&self, f: File) -> Result<Level, LevelError> {
        if f.format != FORMAT {
            return Err(self.err(
                "format",
                format!(
                    "version {} isn't supported; this engine reads {FORMAT}",
                    f.format
                ),
            ));
        }
        let size = v2(f.size);
        if size.x <= Fixed::ZERO || size.y <= Fixed::ZERO {
            return Err(self.err("size", "must be positive"));
        }
        let bounds = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: size.x,
            h: size.y,
        };
        let start = v2(f.start);
        if !bounds.contains(start) {
            return Err(self.err("start", "is outside the level"));
        }
        let mut arrival_names = BTreeSet::new();
        let arrivals: Vec<Arrival> = f
            .arrivals
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                let path = format!("arrivals[{i}]");
                if !is_name(&a.name) {
                    return Err(self.err(
                        &format!("{path}.name"),
                        "use lowercase letters, digits, - and _",
                    ));
                }
                if !arrival_names.insert(a.name.clone()) {
                    return Err(self.err(
                        &format!("{path}.name"),
                        format!("`{}` is used twice", a.name),
                    ));
                }
                let at = v2(a.at);
                if !bounds.contains(at) {
                    return Err(self.err(&format!("{path}.at"), "is outside the level"));
                }
                let walk = match a.walk.as_deref() {
                    None => None,
                    Some("left") => Some(Dir::Left),
                    Some("right") => Some(Dir::Right),
                    Some(other) => {
                        return Err(self.err(
                            &format!("{path}.walk"),
                            format!("`{other}` isn't `left` or `right`"),
                        ));
                    }
                };
                if walk.is_some() && a.entrance.is_some() {
                    return Err(self.err(
                        &format!("{path}.entrance"),
                        "the characters walk in, so they can't come out of an entrance",
                    ));
                }
                if a.drop && (walk.is_some() || a.entrance.is_some()) {
                    return Err(self.err(
                        &format!("{path}.drop"),
                        "the lead drops in, so it can't walk in or come out of an entrance",
                    ));
                }
                Ok(Arrival {
                    name: a.name,
                    at,
                    walk,
                    entrance: a.entrance,
                    drop: a.drop,
                })
            })
            .collect::<Result<_, _>>()?;

        let mut surfaces = builtin_surfaces();
        for (name, s) in f.surfaces {
            let surface = self.surface(&format!("surfaces.{name}"), s)?;
            surfaces.insert(name, surface);
        }

        let mut layer_names = BTreeSet::new();
        let layers = f
            .layers
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                let at = format!("layers[{i}]");
                if !layer_names.insert(l.name.clone()) {
                    return Err(self.err(&at, format!("a layer named `{}` already exists", l.name)));
                }
                self.layer(&at, l)
            })
            .collect::<Result<_, _>>()?;

        let mut ids = BTreeSet::new();
        let entities: Vec<Entity> = f
            .entities
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                let at = format!("entities[{i}]");
                if !is_name(&e.kind) {
                    return Err(self.err(
                        &format!("{at}.type"),
                        "use lowercase letters, digits, - and _",
                    ));
                }
                if let Some(id) = &e.id
                    && !ids.insert(id.clone())
                {
                    return Err(self.err(&format!("{at}.id"), format!("`{id}` is used twice")));
                }
                Ok(Entity {
                    kind: e.kind,
                    id: e.id,
                    at: v2(e.at),
                    params: e.params,
                })
            })
            .collect::<Result<_, _>>()?;
        for (i, a) in arrivals.iter().enumerate() {
            if let Some(id) = &a.entrance
                && !ids.contains(id)
            {
                return Err(self.err(
                    &format!("arrivals[{i}].entrance"),
                    format!("no entity has the id `{id}`"),
                ));
            }
        }

        let mut colliders = Vec::new();
        for (i, c) in f.collision.into_iter().enumerate() {
            self.collider(&format!("collision[{i}]"), c, &surfaces, &mut colliders)?;
        }

        let regions = f
            .regions
            .into_iter()
            .enumerate()
            .map(|(i, r)| self.region(&format!("regions[{i}]"), r))
            .collect::<Result<_, _>>()?;

        let mut rule = match f.camera {
            Some(c) => self.camera("camera", c)?,
            None => CameraRule::default(),
        };
        rule.limits.get_or_insert(bounds);

        let music = f.music.map(|m| self.track("music", m)).transpose()?;
        let lighting = match f.lighting {
            Some(l) => self.lighting("lighting", l)?,
            None => Lighting::default(),
        };
        let lights = f
            .lights
            .into_iter()
            .enumerate()
            .map(|(i, l)| self.light(&format!("lights[{i}]"), l))
            .collect::<Result<_, _>>()?;
        let sections = self.sections(f.sections, entities.len())?;
        let scene = f.scene.map(|s| self.scene("scene", s)).transpose()?;

        Ok(Level {
            name: f.name,
            size,
            start,
            arrivals,
            layers,
            entities,
            collision: Collision::new(colliders),
            regions,
            camera: Camera { rule },
            surfaces,
            music,
            lighting,
            lights,
            sections,
            scene,
        })
    }

    fn scene(&self, at: &str, s: SceneFile) -> Result<Scene, LevelError> {
        if s.fade_step == 0 {
            return Err(self.err(&format!("{at}.fade_step"), "must be positive"));
        }
        if s.fade >= s.last || (s.last - s.fade) / s.fade_step == 0 {
            return Err(self.err(
                &format!("{at}.fade"),
                "must leave a step of the fade before `last`",
            ));
        }
        if s.shorter > s.fade {
            return Err(self.err(&format!("{at}.shorter"), "must not be more than `fade`"));
        }
        for (key, name) in [("area", &s.area), ("arrival", &s.arrival)] {
            if let Some(name) = name
                && !is_name(name)
            {
                return Err(self.err(
                    &format!("{at}.{key}"),
                    "use lowercase letters, digits, - and _",
                ));
            }
        }
        Ok(Scene {
            last: s.last,
            fade: s.fade,
            fade_step: s.fade_step,
            shorter: s.shorter,
            area: s.area,
            arrival: s.arrival,
        })
    }

    fn sections(&self, f: Vec<SectionFile>, entities: usize) -> Result<Vec<Section>, LevelError> {
        let last = f.len().saturating_sub(1);
        let mut placed = BTreeSet::new();
        f.into_iter()
            .enumerate()
            .map(|(i, s)| {
                let at = format!("sections[{i}]");
                if (i < last) != s.meets.is_some() {
                    let message = match s.meets {
                        Some(_) => "the last section doesn't meet another",
                        None => "missing `meets`, where it meets the next section",
                    };
                    return Err(self.err(&at, message));
                }
                let spots = s
                    .spots
                    .into_iter()
                    .enumerate()
                    .map(|(j, spot)| {
                        let at = format!("{at}.spots[{j}]");
                        let spot = match spot {
                            SpotFile::One(e) => Spot {
                                entities: vec![e],
                                at: None,
                                rows: false,
                                leave_rows: false,
                                picture: false,
                            },
                            SpotFile::Table {
                                entities,
                                at,
                                rows,
                                leave_rows,
                                picture,
                            } => Spot {
                                entities,
                                at: at.map(v2),
                                rows,
                                leave_rows,
                                picture,
                            },
                        };
                        if spot.entities.is_empty() {
                            return Err(self.err(&at, "needs an entity"));
                        }
                        if spot.rows && spot.entities.len() > 1 {
                            return Err(self.err(&at, "`rows` is for one entity"));
                        }
                        for &e in &spot.entities {
                            if e >= entities {
                                return Err(self.err(&at, format!("there's no entity {e}")));
                            }
                            if !placed.insert(e) {
                                return Err(self.err(&at, format!("entity {e} is in two spots")));
                            }
                        }
                        Ok(spot)
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Section {
                    spots,
                    meets: s.meets.map(v2),
                })
            })
            .collect()
    }

    fn color(&self, at: &str, c: &str) -> Result<Rgb, LevelError> {
        parse_rgb(c).ok_or_else(|| self.err(at, format!("`{c}` isn't a `#rrggbb` colour")))
    }

    fn intensity(&self, at: &str, n: Option<N>, default: f64) -> Result<f64, LevelError> {
        match n {
            Some(n) if n.0 < Fixed::ZERO => Err(self.err(at, "can't be negative")),
            Some(n) => Ok(n.0.to_f64()),
            None => Ok(default),
        }
    }

    fn lighting(&self, at: &str, l: LightingFile) -> Result<Lighting, LevelError> {
        let mut lighting = Lighting::default();
        if let Some(c) = &l.ambient {
            lighting.ambient = self.color(&format!("{at}.ambient"), c)?;
        }
        if let Some(k) = l.key {
            let key = &mut lighting.key;
            if let Some(d) = k.direction {
                if d.iter().all(|n| n.0 == Fixed::ZERO) {
                    return Err(self.err(&format!("{at}.key.direction"), "can't be all zero"));
                }
                key.direction = d.map(|n| n.0.to_f64());
            }
            if let Some(c) = &k.color {
                key.color = self.color(&format!("{at}.key.color"), c)?;
            }
            key.intensity =
                self.intensity(&format!("{at}.key.intensity"), k.intensity, key.intensity)?;
        }
        Ok(lighting)
    }

    fn light(&self, at: &str, l: LightFile) -> Result<Light, LevelError> {
        if l.radius.0 <= Fixed::ZERO {
            return Err(self.err(&format!("{at}.radius"), "must be positive"));
        }
        Ok(Light {
            at: v2(l.at),
            color: self.color(&format!("{at}.color"), &l.color)?,
            radius: l.radius.0,
            intensity: self.intensity(&format!("{at}.intensity"), l.intensity, 1.0)?,
        })
    }

    fn track(&self, at: &str, m: TrackFile) -> Result<Track, LevelError> {
        if m.file.is_empty() {
            return Err(self.err(&format!("{at}.file"), "is empty"));
        }
        if m.loop_length.is_some() && m.loop_start.is_none() {
            return Err(self.err(&format!("{at}.loop_length"), "needs `loop_start`"));
        }
        if m.loop_length == Some(0) {
            return Err(self.err(&format!("{at}.loop_length"), "must be positive"));
        }
        Ok(Track {
            file: m.file,
            loop_start: m.loop_start,
            loop_length: m.loop_length,
        })
    }

    fn surface(&self, at: &str, s: SurfaceFile) -> Result<Surface, LevelError> {
        let physics = match s.physics {
            Some(t) => self.physics(&format!("{at}.physics"), &t)?,
            None => PhysicsPatch::default(),
        };
        Ok(Surface {
            physics,
            hazard: s.hazard,
        })
    }

    fn physics(&self, at: &str, table: &Table) -> Result<PhysicsPatch, LevelError> {
        clamber_pack::read_physics_patch(table, self.origin, at)
            .map_err(|e| LevelError::new(self.origin, e.message))
    }

    fn layer(&self, at: &str, l: LayerFile) -> Result<Layer, LevelError> {
        if l.scale == 0 {
            return Err(self.err(&format!("{at}.scale"), "must be at least 1"));
        }
        let content = match (l.image, l.tiles, l.props) {
            (Some(path), None, None) => LayerContent::Image {
                path,
                normals: l.normals,
                repeat_x: l.repeat[0],
                repeat_y: l.repeat[1],
            },
            (None, Some(t), None) => {
                if l.normals.is_some() {
                    return Err(self.err(
                        &format!("{at}.normals"),
                        "is for image layers; give tiles' normals in `tiles`",
                    ));
                }
                let at = format!("{at}.tiles");
                if t.tile_size[0] == 0 || t.tile_size[1] == 0 {
                    return Err(self.err(&format!("{at}.tile_size"), "must be positive"));
                }
                if t.columns == 0 || t.data.len() % t.columns as usize != 0 {
                    return Err(self.err(
                        &format!("{at}.data"),
                        format!(
                            "{} tiles don't fill rows of {} columns",
                            t.data.len(),
                            t.columns
                        ),
                    ));
                }
                LayerContent::Tiles(TileLayer {
                    tileset: Tileset {
                        image: t.tileset,
                        normals: t.normals,
                        tile_width: t.tile_size[0],
                        tile_height: t.tile_size[1],
                        margin: t.margin,
                        spacing: t.spacing,
                    },
                    columns: t.columns,
                    tiles: t.data.into_iter().map(Tile).collect(),
                })
            }
            (None, None, Some(props)) => {
                if l.normals.is_some() {
                    return Err(self.err(
                        &format!("{at}.normals"),
                        "is for image layers; give props' normals with each prop",
                    ));
                }
                LayerContent::Props(
                    props
                        .into_iter()
                        .map(|p| Prop {
                            image: p.image,
                            normals: p.normals,
                            at: v2(p.at),
                            flip: p.flip,
                        })
                        .collect(),
                )
            }
            _ => return Err(self.err(at, "give one of `image`, `tiles` or `props`")),
        };
        let opacity = match l.opacity {
            Some(n) if n.0 < Fixed::ZERO || n.0 > Fixed::ONE => {
                return Err(self.err(&format!("{at}.opacity"), "must be from 0 to 1"));
            }
            Some(n) => n.0.to_f64(),
            None => 1.0,
        };
        let sway = match l.sway {
            Some(_) if matches!(content, LayerContent::Tiles(_)) => {
                return Err(self.err(
                    &format!("{at}.sway"),
                    "is for image and props layers; tiles would come apart",
                ));
            }
            Some(s) if s.period.0 <= Fixed::ZERO => {
                return Err(self.err(&format!("{at}.sway.period"), "must be positive"));
            }
            Some(s) => Some(Sway {
                amount: s.amount.0.to_f64(),
                period: s.period.0.to_f64(),
            }),
            None => None,
        };
        if !l.bands.is_empty() && !matches!(content, LayerContent::Image { .. }) {
            return Err(self.err(&format!("{at}.bands"), "are for image layers"));
        }
        if l.bands.iter().any(|b| b.rows == 0) {
            return Err(self.err(&format!("{at}.bands"), "each needs at least one row"));
        }
        let bands = l
            .bands
            .into_iter()
            .map(|b| Band {
                rows: b.rows,
                parallax: b.parallax.0,
            })
            .collect();
        Ok(Layer {
            name: l.name,
            depth: l.depth,
            parallax: l.parallax.map_or(Vec2::ints(1, 1), v2),
            offset: l.offset.map_or(Vec2::default(), v2),
            content,
            scale: l.scale,
            pixel_art: l.pixel_art,
            dim: match l.dim {
                DimFile::None | DimFile::Palette(false) => Dim::None,
                DimFile::Palette(true) => Dim::Palette,
                DimFile::Night(n) => Dim::Night(n.night),
            },
            darker: l.darker,
            opacity,
            sway,
            bands,
            alternate: l.alternate.map_or(Vec2::default(), v2),
        })
    }

    fn collider(
        &self,
        at: &str,
        c: ColliderFile,
        surfaces: &BTreeMap<String, Surface>,
        out: &mut Vec<Collider>,
    ) -> Result<(), LevelError> {
        let check_surface = |s: Option<String>| -> Result<String, LevelError> {
            let s = s.unwrap_or_else(|| "normal".into());
            if !surfaces.contains_key(&s) {
                return Err(self.err(&format!("{at}.surface"), format!("unknown surface `{s}`")));
            }
            Ok(s)
        };
        let segment = |a: Vec2, b: Vec2| -> Result<(), LevelError> {
            if a == b {
                return Err(self.err(at, "segment has zero length"));
            }
            Ok(())
        };
        match c {
            ColliderFile::Rect {
                rect: r,
                one_way,
                hangs,
                surface,
            } => {
                let r = rect(r);
                if r.w <= Fixed::ZERO || r.h <= Fixed::ZERO {
                    return Err(
                        self.err(&format!("{at}.rect"), "width and height must be positive")
                    );
                }
                out.push(Collider {
                    shape: Shape::Solid {
                        points: r.corners(),
                        one_way,
                    },
                    surface: check_surface(surface)?,
                    hangs,
                });
            }
            ColliderFile::Polygon {
                points,
                one_way,
                hangs,
                surface,
            } => {
                let points = self.polygon(&format!("{at}.points"), points)?;
                out.push(Collider {
                    shape: Shape::Solid { points, one_way },
                    surface: check_surface(surface)?,
                    hangs,
                });
            }
            ColliderFile::Segment {
                from,
                to,
                one_way,
                surface,
            } => {
                let (a, b) = (v2(from), v2(to));
                segment(a, b)?;
                out.push(Collider {
                    shape: Shape::Segment { a, b, one_way },
                    surface: check_surface(surface)?,
                    hangs: false,
                });
            }
            ColliderFile::Polyline {
                points,
                one_way,
                surface,
            } => {
                if points.len() < 2 {
                    return Err(self.err(&format!("{at}.points"), "needs at least 2 points"));
                }
                let surface = check_surface(surface)?;
                let points: Vec<Vec2> = points.into_iter().map(v2).collect();
                for pair in points.windows(2) {
                    segment(pair[0], pair[1])?;
                    out.push(Collider {
                        shape: Shape::Segment {
                            a: pair[0],
                            b: pair[1],
                            one_way,
                        },
                        surface: surface.clone(),
                        hangs: false,
                    });
                }
            }
        }
        Ok(())
    }

    fn polygon(&self, at: &str, points: Vec<[N; 2]>) -> Result<Vec<Vec2>, LevelError> {
        let points: Vec<Vec2> = points.into_iter().map(v2).collect();
        match geom::polygon_problem(&points) {
            Some(problem) => Err(self.err(at, format!("polygon {problem}"))),
            None => Ok(points),
        }
    }

    fn region(&self, at: &str, r: RegionFile) -> Result<Region, LevelError> {
        let area = match (r.rect, r.points) {
            (Some(rc), None) => {
                let rc = rect(rc);
                if rc.w <= Fixed::ZERO || rc.h <= Fixed::ZERO {
                    return Err(
                        self.err(&format!("{at}.rect"), "width and height must be positive")
                    );
                }
                Area::Rect(rc)
            }
            (None, Some(points)) => Area::Polygon(self.polygon(&format!("{at}.points"), points)?),
            _ => return Err(self.err(at, "give either `rect` or `points`")),
        };
        if let Some(d) = r.darkness
            && (d.0 < Fixed::ZERO || d.0 > Fixed::ONE)
        {
            return Err(self.err(&format!("{at}.darkness"), "must be between 0 and 1"));
        }
        Ok(Region {
            name: r.name,
            area,
            water: r.water,
            wind: r.wind.map(v2),
            darkness: r.darkness.map(|d| d.0),
            physics: r
                .physics
                .map(|t| self.physics(&format!("{at}.physics"), &t))
                .transpose()?,
            camera: r
                .camera
                .map(|c| self.camera(&format!("{at}.camera"), c))
                .transpose()?,
        })
    }

    fn camera(&self, at: &str, c: CameraFile) -> Result<CameraRule, LevelError> {
        let limits = c.limits.map(rect);
        if let Some(l) = limits
            && (l.w <= Fixed::ZERO || l.h <= Fixed::ZERO)
        {
            return Err(self.err(&format!("{at}.limits"), "width and height must be positive"));
        }
        Ok(CameraRule {
            limits,
            lock_x: c.lock_x.map(|n| n.0),
            lock_y: c.lock_y.map(|n| n.0),
            look_ahead: c.look_ahead.map(v2),
            quick: c.quick,
            sway: c.sway,
        })
    }
}

/// A `#rrggbb` colour.
fn parse_rgb(s: &str) -> Option<Rgb> {
    let hex = s.strip_prefix('#').filter(|h| h.len() == 6)?;
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn rgb_value([r, g, b]: Rgb) -> Value {
    Value::String(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn float(x: f64) -> Value {
    num(Fixed::from_f64(x).unwrap_or(Fixed::ZERO))
}

fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The engine's surface types, from `content/base/surfaces.toml`.
pub fn builtin_surfaces() -> BTreeMap<String, Surface> {
    let origin = "content/base/surfaces.toml";
    let file: SurfacesFile = toml::from_str(BUILTIN_SURFACES).expect("built-in surfaces parse");
    let r = Reader { origin };
    file.surfaces
        .into_iter()
        .map(|(name, s)| {
            let surface = r
                .surface(&format!("surfaces.{name}"), s)
                .expect("built-in surfaces are valid");
            (name, surface)
        })
        .collect()
}

// ---- writing ----

fn num(f: Fixed) -> Value {
    let bits = f.to_bits();
    if bits & ((1 << clamber_fixed::FRAC_BITS) - 1) == 0 {
        Value::Integer(bits >> clamber_fixed::FRAC_BITS)
    } else {
        Value::Float(bits as f64 / (1u64 << clamber_fixed::FRAC_BITS) as f64)
    }
}

fn pair(p: Vec2) -> Value {
    Value::Array(vec![num(p.x), num(p.y)])
}

fn quad(r: Rect) -> Value {
    Value::Array(vec![num(r.x), num(r.y), num(r.w), num(r.h)])
}

fn points(ps: &[Vec2]) -> Value {
    Value::Array(ps.iter().map(|&p| pair(p)).collect())
}

fn patch(p: &PhysicsPatch) -> Value {
    let mut t = Table::new();
    for (param, adjust) in &p.0 {
        let v = match adjust {
            Adjust::Set(v) if param.is_step_set() => Value::Array(
                clamber_pack::physics::steps(*v)
                    .map(|n| Value::Integer(n.into()))
                    .collect(),
            ),
            Adjust::Set(v) => num(*v),
            Adjust::Scale(s) => {
                let mut scale = Table::new();
                scale.insert("scale".into(), num(*s));
                Value::Table(scale)
            }
        };
        t.insert(param.name().into(), v);
    }
    Value::Table(t)
}

fn camera_table(c: &CameraRule) -> Table {
    let mut t = Table::new();
    if let Some(l) = c.limits {
        t.insert("limits".into(), quad(l));
    }
    if let Some(x) = c.lock_x {
        t.insert("lock_x".into(), num(x));
    }
    if let Some(y) = c.lock_y {
        t.insert("lock_y".into(), num(y));
    }
    if let Some(l) = c.look_ahead {
        t.insert("look_ahead".into(), pair(l));
    }
    if let Some(q) = c.quick {
        t.insert("quick".into(), Value::Boolean(q));
    }
    if let Some(s) = c.sway {
        t.insert("sway".into(), Value::Boolean(s));
    }
    t
}

impl Level {
    /// The level as a file that [`Level::from_toml_str`] reads back as the
    /// same level. Built-in surfaces aren't written unless changed.
    pub fn to_toml_string(&self) -> String {
        let mut t = Table::new();
        t.insert("format".into(), Value::Integer(FORMAT.into()));
        t.insert("name".into(), Value::String(self.name.clone()));
        t.insert("size".into(), pair(self.size));
        t.insert("start".into(), pair(self.start));
        let arrivals = self.arrivals.iter().map(|a| {
            let mut at = Table::new();
            at.insert("name".into(), Value::String(a.name.clone()));
            at.insert("at".into(), pair(a.at));
            let walk = match a.walk {
                Some(Dir::Left) => Some("left"),
                Some(Dir::Right) => Some("right"),
                _ => None,
            };
            if let Some(walk) = walk {
                at.insert("walk".into(), Value::String(walk.into()));
            }
            if let Some(id) = &a.entrance {
                at.insert("entrance".into(), Value::String(id.clone()));
            }
            if a.drop {
                at.insert("drop".into(), Value::Boolean(true));
            }
            Value::Table(at)
        });
        insert_array(&mut t, "arrivals", arrivals);

        let mut camera = self.camera.rule;
        let bounds = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: self.size.x,
            h: self.size.y,
        };
        if camera.limits == Some(bounds) {
            camera.limits = None;
        }
        let camera = camera_table(&camera);
        if !camera.is_empty() {
            t.insert("camera".into(), Value::Table(camera));
        }

        if let Some(m) = &self.music {
            let mut mt = Table::new();
            mt.insert("file".into(), Value::String(m.file.clone()));
            for (key, n) in [("loop_start", m.loop_start), ("loop_length", m.loop_length)] {
                if let Some(n) = n {
                    let n = i64::try_from(n).unwrap_or(i64::MAX);
                    mt.insert(key.into(), Value::Integer(n));
                }
            }
            t.insert("music".into(), Value::Table(mt));
        }

        if let Some(s) = &self.scene {
            let mut st = Table::new();
            for (key, n) in [
                ("last", s.last),
                ("fade", s.fade),
                ("fade_step", s.fade_step),
                ("shorter", s.shorter),
            ] {
                if key != "shorter" || n > 0 {
                    st.insert(key.into(), Value::Integer(n.into()));
                }
            }
            for (key, name) in [("area", &s.area), ("arrival", &s.arrival)] {
                if let Some(name) = name {
                    st.insert(key.into(), Value::String(name.clone()));
                }
            }
            t.insert("scene".into(), Value::Table(st));
        }

        if self.lighting != Lighting::default() {
            let (l, key) = (&self.lighting, &self.lighting.key);
            let mut kt = Table::new();
            kt.insert(
                "direction".into(),
                Value::Array(key.direction.iter().map(|&d| float(d)).collect()),
            );
            kt.insert("color".into(), rgb_value(key.color));
            kt.insert("intensity".into(), float(key.intensity));
            let mut lt = Table::new();
            lt.insert("ambient".into(), rgb_value(l.ambient));
            lt.insert("key".into(), Value::Table(kt));
            t.insert("lighting".into(), Value::Table(lt));
        }
        let lights = self.lights.iter().map(|l| {
            let mut lt = Table::new();
            lt.insert("at".into(), pair(l.at));
            lt.insert("color".into(), rgb_value(l.color));
            lt.insert("radius".into(), num(l.radius));
            lt.insert("intensity".into(), float(l.intensity));
            Value::Table(lt)
        });
        insert_array(&mut t, "lights", lights);

        let builtin = builtin_surfaces();
        let mut surfaces = Table::new();
        for (name, s) in &self.surfaces {
            if builtin.get(name) == Some(s) {
                continue;
            }
            let mut st = Table::new();
            if !s.physics.is_empty() {
                st.insert("physics".into(), patch(&s.physics));
            }
            if s.hazard {
                st.insert("hazard".into(), Value::Boolean(true));
            }
            surfaces.insert(name.clone(), Value::Table(st));
        }
        if !surfaces.is_empty() {
            t.insert("surfaces".into(), Value::Table(surfaces));
        }

        let layers = self.layers.iter().map(|l| {
            let mut lt = Table::new();
            lt.insert("name".into(), Value::String(l.name.clone()));
            let depth = match l.depth {
                Depth::Back => "back",
                Depth::Front => "front",
                Depth::Raised => "raised",
            };
            lt.insert("depth".into(), Value::String(depth.into()));
            lt.insert("parallax".into(), pair(l.parallax));
            if l.offset != Vec2::default() {
                lt.insert("offset".into(), pair(l.offset));
            }
            if l.scale != 1 {
                lt.insert("scale".into(), l.scale.into());
            }
            if l.pixel_art {
                lt.insert("pixel_art".into(), Value::Boolean(true));
            }
            match &l.dim {
                Dim::None => {}
                Dim::Palette => {
                    lt.insert("dim".into(), Value::Boolean(true));
                }
                Dim::Night(night) => {
                    let mut dt = Table::new();
                    dt.insert("night".into(), Value::String(night.clone()));
                    lt.insert("dim".into(), Value::Table(dt));
                }
            }
            if l.darker {
                lt.insert("darker".into(), Value::Boolean(true));
            }
            if l.opacity != 1.0 {
                lt.insert("opacity".into(), float(l.opacity));
            }
            if let Some(sway) = &l.sway {
                let mut st = Table::new();
                st.insert("amount".into(), float(sway.amount));
                st.insert("period".into(), float(sway.period));
                lt.insert("sway".into(), Value::Table(st));
            }
            if !l.bands.is_empty() {
                let bands = l.bands.iter().map(|b| {
                    let mut bt = Table::new();
                    bt.insert("rows".into(), b.rows.into());
                    bt.insert("parallax".into(), num(b.parallax));
                    Value::Table(bt)
                });
                lt.insert("bands".into(), Value::Array(bands.collect()));
            }
            if l.alternate != Vec2::default() {
                lt.insert("alternate".into(), pair(l.alternate));
            }
            match &l.content {
                LayerContent::Image {
                    path,
                    normals,
                    repeat_x,
                    repeat_y,
                } => {
                    lt.insert("image".into(), Value::String(path.clone()));
                    if let Some(n) = normals {
                        lt.insert("normals".into(), Value::String(n.clone()));
                    }
                    let repeat = vec![Value::Boolean(*repeat_x), Value::Boolean(*repeat_y)];
                    lt.insert("repeat".into(), Value::Array(repeat));
                }
                LayerContent::Tiles(tl) => {
                    let mut tt = Table::new();
                    let ts = &tl.tileset;
                    tt.insert("tileset".into(), Value::String(ts.image.clone()));
                    if let Some(n) = &ts.normals {
                        tt.insert("normals".into(), Value::String(n.clone()));
                    }
                    let size = vec![ts.tile_width.into(), ts.tile_height.into()];
                    tt.insert("tile_size".into(), Value::Array(size));
                    tt.insert("margin".into(), ts.margin.into());
                    tt.insert("spacing".into(), ts.spacing.into());
                    tt.insert("columns".into(), tl.columns.into());
                    let data = tl
                        .tiles
                        .iter()
                        .map(|t| Value::Integer(t.0.into()))
                        .collect();
                    tt.insert("data".into(), Value::Array(data));
                    lt.insert("tiles".into(), Value::Table(tt));
                }
                LayerContent::Props(props) => {
                    let props = props.iter().map(|p| {
                        let mut pt = Table::new();
                        pt.insert("image".into(), Value::String(p.image.clone()));
                        if let Some(n) = &p.normals {
                            pt.insert("normals".into(), Value::String(n.clone()));
                        }
                        pt.insert("at".into(), pair(p.at));
                        if p.flip {
                            pt.insert("flip".into(), Value::Boolean(true));
                        }
                        Value::Table(pt)
                    });
                    lt.insert("props".into(), Value::Array(props.collect()));
                }
            }
            Value::Table(lt)
        });
        insert_array(&mut t, "layers", layers);

        let entities = self.entities.iter().map(|e| {
            let mut et = Table::new();
            et.insert("type".into(), Value::String(e.kind.clone()));
            if let Some(id) = &e.id {
                et.insert("id".into(), Value::String(id.clone()));
            }
            et.insert("at".into(), pair(e.at));
            if !e.params.is_empty() {
                et.insert("params".into(), Value::Table(e.params.clone()));
            }
            Value::Table(et)
        });
        insert_array(&mut t, "entities", entities);

        let colliders = self.collision.colliders.iter().map(|c| {
            let mut ct = Table::new();
            match &c.shape {
                Shape::Solid {
                    points: ps,
                    one_way,
                } => {
                    ct.insert("shape".into(), Value::String("polygon".into()));
                    ct.insert("points".into(), points(ps));
                    if *one_way {
                        ct.insert("one_way".into(), Value::Boolean(true));
                    }
                }
                Shape::Segment { a, b, one_way } => {
                    ct.insert("shape".into(), Value::String("segment".into()));
                    ct.insert("from".into(), pair(*a));
                    ct.insert("to".into(), pair(*b));
                    if *one_way {
                        ct.insert("one_way".into(), Value::Boolean(true));
                    }
                }
            }
            if c.hangs {
                ct.insert("hangs".into(), Value::Boolean(true));
            }
            if c.surface != "normal" {
                ct.insert("surface".into(), Value::String(c.surface.clone()));
            }
            Value::Table(ct)
        });
        insert_array(&mut t, "collision", colliders);

        let regions = self.regions.iter().map(|r| {
            let mut rt = Table::new();
            if let Some(n) = &r.name {
                rt.insert("name".into(), Value::String(n.clone()));
            }
            match &r.area {
                Area::Rect(rc) => rt.insert("rect".into(), quad(*rc)),
                Area::Polygon(ps) => rt.insert("points".into(), points(ps)),
            };
            if r.water {
                rt.insert("water".into(), Value::Boolean(true));
            }
            if let Some(w) = r.wind {
                rt.insert("wind".into(), pair(w));
            }
            if let Some(d) = r.darkness {
                rt.insert("darkness".into(), num(d));
            }
            if let Some(p) = &r.physics {
                rt.insert("physics".into(), patch(p));
            }
            if let Some(c) = &r.camera {
                rt.insert("camera".into(), Value::Table(camera_table(c)));
            }
            Value::Table(rt)
        });
        insert_array(&mut t, "regions", regions);

        let sections = self.sections.iter().map(|s| {
            let spots = s.spots.iter().map(|spot| {
                let index = |&e: &usize| Value::Integer(e as i64);
                match spot.entities.as_slice() {
                    [e] if spot.at.is_none() && !spot.rows && !spot.leave_rows && !spot.picture => {
                        index(e)
                    }
                    entities => {
                        let mut pt = Table::new();
                        let entities = entities.iter().map(index).collect();
                        pt.insert("entities".into(), Value::Array(entities));
                        if let Some(at) = spot.at {
                            pt.insert("at".into(), pair(at));
                        }
                        if spot.rows {
                            pt.insert("rows".into(), Value::Boolean(true));
                        }
                        if spot.leave_rows {
                            pt.insert("leave_rows".into(), Value::Boolean(true));
                        }
                        if spot.picture {
                            pt.insert("picture".into(), Value::Boolean(true));
                        }
                        Value::Table(pt)
                    }
                }
            });
            let mut st = Table::new();
            st.insert("spots".into(), Value::Array(spots.collect()));
            if let Some(meets) = s.meets {
                st.insert("meets".into(), pair(meets));
            }
            Value::Table(st)
        });
        insert_array(&mut t, "sections", sections);

        let mut out = format!("# clamber level v{FORMAT}\n");
        out.push_str(&toml::to_string(&t).expect("level serializes"));
        out
    }
}

fn insert_array(t: &mut Table, key: &str, items: impl Iterator<Item = Value>) {
    let items: Vec<Value> = items.collect();
    if !items.is_empty() {
        t.insert(key.into(), Value::Array(items));
    }
}
