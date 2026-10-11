//! The level format: TOML, in world units. See `content/test/` for
//! examples and [`crate`] for an overview.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use clamber_fixed::Fixed;
use clamber_pack::{Adjust, PhysicsPatch};
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use toml::{Table, Value};

use crate::collision::{Collider, Collision, Shape};
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
    /// Drawn in order; [`Depth`] puts each behind or in front of entities.
    pub layers: Vec<Layer>,
    pub entities: Vec<Entity>,
    pub collision: Collision,
    /// Applied in order, so a later region overrides an earlier one.
    pub regions: Vec<Region>,
    pub camera: Camera,
    /// Built-in surfaces with the level's own on top.
    pub surfaces: BTreeMap<String, Surface>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub depth: Depth,
    /// How far the layer scrolls per pixel of camera movement: 1 moves
    /// with the world, 0 stays fixed on screen.
    pub parallax: Vec2,
    pub offset: Vec2,
    pub content: LayerContent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    /// Behind entities.
    Back,
    /// In front of entities.
    Front,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerContent {
    Image {
        path: String,
        repeat_x: bool,
        repeat_y: bool,
    },
    Tiles(TileLayer),
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
}

impl CameraRule {
    /// `self` with what `other` sets replacing it.
    pub fn then(self, other: &CameraRule) -> CameraRule {
        CameraRule {
            limits: other.limits.or(self.limits),
            lock_x: other.lock_x.or(self.lock_x),
            lock_y: other.lock_y.or(self.lock_y),
            look_ahead: other.look_ahead.or(self.look_ahead),
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
    #[serde(default)]
    repeat: [bool; 2],
    tiles: Option<TilesFile>,
}

fn back() -> Depth {
    Depth::Back
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TilesFile {
    tileset: String,
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
        surface: Option<String>,
    },
    Polygon {
        points: Vec<[N; 2]>,
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
        let entities = f
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

        Ok(Level {
            name: f.name,
            size,
            start,
            layers,
            entities,
            collision: Collision::new(colliders),
            regions,
            camera: Camera { rule },
            surfaces,
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
        let content = match (l.image, l.tiles) {
            (Some(path), None) => LayerContent::Image {
                path,
                repeat_x: l.repeat[0],
                repeat_y: l.repeat[1],
            },
            (None, Some(t)) => {
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
                        tile_width: t.tile_size[0],
                        tile_height: t.tile_size[1],
                        margin: t.margin,
                        spacing: t.spacing,
                    },
                    columns: t.columns,
                    tiles: t.data.into_iter().map(Tile).collect(),
                })
            }
            _ => return Err(self.err(at, "give either `image` or `tiles`")),
        };
        Ok(Layer {
            name: l.name,
            depth: l.depth,
            parallax: l.parallax.map_or(Vec2::ints(1, 1), v2),
            offset: l.offset.map_or(Vec2::default(), v2),
            content,
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
            ColliderFile::Rect { rect: r, surface } => {
                let r = rect(r);
                if r.w <= Fixed::ZERO || r.h <= Fixed::ZERO {
                    return Err(
                        self.err(&format!("{at}.rect"), "width and height must be positive")
                    );
                }
                out.push(Collider {
                    shape: Shape::Solid(r.corners()),
                    surface: check_surface(surface)?,
                });
            }
            ColliderFile::Polygon { points, surface } => {
                let points = self.polygon(&format!("{at}.points"), points)?;
                out.push(Collider {
                    shape: Shape::Solid(points),
                    surface: check_surface(surface)?,
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
        })
    }
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
            };
            lt.insert("depth".into(), Value::String(depth.into()));
            lt.insert("parallax".into(), pair(l.parallax));
            if l.offset != Vec2::default() {
                lt.insert("offset".into(), pair(l.offset));
            }
            match &l.content {
                LayerContent::Image {
                    path,
                    repeat_x,
                    repeat_y,
                } => {
                    lt.insert("image".into(), Value::String(path.clone()));
                    let repeat = vec![Value::Boolean(*repeat_x), Value::Boolean(*repeat_y)];
                    lt.insert("repeat".into(), Value::Array(repeat));
                }
                LayerContent::Tiles(tl) => {
                    let mut tt = Table::new();
                    let ts = &tl.tileset;
                    tt.insert("tileset".into(), Value::String(ts.image.clone()));
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
                Shape::Solid(ps) => {
                    ct.insert("shape".into(), Value::String("polygon".into()));
                    ct.insert("points".into(), points(ps));
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
