//! Importing maps from Tiled (mapeditor.org), saved as JSON.
//!
//! Orthogonal, fixed-size maps with tile layers in CSV format (the JSON
//! default) are supported. Tilesets may be embedded or separate JSON files
//! (`.tsj`/`.json`) and must use a single image.
//!
//! - **Tile layers** become tile layers, split in one per tileset used.
//! - **Image layers** become image layers, keeping parallax, offset and
//!   repeat.
//! - **Group layers** are flattened, combining their offsets and parallax.
//! - A layer property `depth` (`"back"` or `"front"`, default back) puts the
//!   layer behind or in front of entities.
//! - **Objects** are read by class (called type in older Tiled versions):
//!   - `solid`: rectangles and polygons are solid areas, polylines are
//!     segments. Properties `surface` (string) and `one_way` (bool).
//!   - `platform`: one-way; a rectangle becomes its top edge.
//!   - `region`: a rectangle or polygon with properties `water` (bool),
//!     `darkness` (float), `wind_x`/`wind_y` (float), `physics.<param>`
//!     (float, sets it) or `physics.<param>.scale` (float, scales it), and
//!     camera rules `camera.lock_x`, `camera.lock_y`,
//!     `camera.look_ahead_x`/`_y` (float) and `camera.limits` (bool: limit
//!     the camera to this region's bounds).
//!   - `start`: where the lead character starts (exactly one).
//!   - anything else is an entity of that type, placed at the object's
//!     position (bottom-left for tile objects, as Tiled draws them), with
//!     its name as id and its properties (plus `width` and `height`, if it
//!     has a size) as parameters.
//!   - objects without a class on a layer named `collision` are `solid`.
//! - Map properties `name` and `camera.look_ahead_x`/`_y` set the level's
//!   name and look-ahead.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value as Json;
use toml::{Table, Value};

use crate::level::{FORMAT, Level, LevelError, Tile};

#[derive(Deserialize)]
struct Map {
    orientation: String,
    #[serde(default)]
    infinite: bool,
    width: u32,
    height: u32,
    tilewidth: u32,
    tileheight: u32,
    #[serde(default)]
    layers: Vec<TLayer>,
    #[serde(default)]
    tilesets: Vec<TilesetRef>,
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize, Clone)]
struct TLayer {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    offsetx: f64,
    #[serde(default)]
    offsety: f64,
    parallaxx: Option<f64>,
    parallaxy: Option<f64>,
    // tile layers
    width: Option<u32>,
    data: Option<Json>,
    encoding: Option<String>,
    // image layers
    image: Option<String>,
    #[serde(default)]
    repeatx: bool,
    #[serde(default)]
    repeaty: bool,
    // object layers
    #[serde(default)]
    objects: Vec<Object>,
    // groups
    #[serde(default)]
    layers: Vec<TLayer>,
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize, Clone)]
struct Object {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    class: String,
    x: f64,
    y: f64,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
    #[serde(default)]
    rotation: f64,
    #[serde(default)]
    point: bool,
    #[serde(default)]
    ellipse: bool,
    polygon: Option<Vec<Point>>,
    polyline: Option<Vec<Point>>,
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize, Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Deserialize, Clone)]
struct Property {
    name: String,
    value: Json,
}

#[derive(Deserialize)]
struct TilesetRef {
    firstgid: u32,
    source: Option<String>,
    #[serde(flatten)]
    embedded: Option<TilesetData>,
}

#[derive(Deserialize, Clone)]
struct TilesetData {
    image: Option<String>,
    tilewidth: u32,
    tileheight: u32,
    #[serde(default)]
    margin: u32,
    #[serde(default)]
    spacing: u32,
    #[serde(default)]
    name: String,
}

struct Tileset {
    firstgid: u32,
    image: String,
    tile_size: [u32; 2],
    margin: u32,
    spacing: u32,
}

/// Imports a map file, reading separate tilesets relative to it. Image
/// paths in the level stay relative to the map's directory.
pub fn import_file(path: &Path) -> Result<Level, LevelError> {
    let origin = path.display().to_string();
    let dir = path.parent().unwrap_or(Path::new(""));
    let src = std::fs::read_to_string(path).map_err(|e| LevelError::new(&origin, e))?;
    import(&src, &origin, |source| {
        std::fs::read_to_string(dir.join(source)).map_err(|e| e.to_string())
    })
}

/// Imports a map from JSON text. `read_tileset` returns the text of a
/// separate tileset file, given its path relative to the map.
pub fn import(
    json: &str,
    origin: &str,
    mut read_tileset: impl FnMut(&str) -> Result<String, String>,
) -> Result<Level, LevelError> {
    let err = |m: String| LevelError::new(origin, m);
    let map: Map = serde_json::from_str(json).map_err(|e| err(e.to_string()))?;
    if map.orientation != "orthogonal" {
        return Err(err(format!(
            "{} maps aren't supported; use orthogonal",
            map.orientation
        )));
    }
    if map.infinite {
        return Err(err(
            "infinite maps aren't supported; turn off Infinite in Map Properties".into(),
        ));
    }

    let mut tilesets = Vec::new();
    for t in &map.tilesets {
        let (data, dir) = match &t.source {
            Some(source) => {
                let text =
                    read_tileset(source).map_err(|e| err(format!("tileset {source}: {e}")))?;
                let data: TilesetData = serde_json::from_str(&text)
                    .map_err(|e| err(format!("tileset {source}: {e}")))?;
                let dir = Path::new(source).parent().map(|p| p.to_path_buf());
                (data, dir)
            }
            None => match &t.embedded {
                Some(d) => (d.clone(), None),
                None => return Err(err(format!("tileset at gid {} has no data", t.firstgid))),
            },
        };
        let image = data.image.ok_or_else(|| {
            err(format!(
                "tileset `{}` is a collection of images; use a single tileset image",
                data.name
            ))
        })?;
        let image = match dir {
            Some(d) if !d.as_os_str().is_empty() => {
                format!("{}/{image}", d.to_string_lossy().replace('\\', "/"))
            }
            _ => image,
        };
        tilesets.push(Tileset {
            firstgid: t.firstgid,
            image,
            tile_size: [data.tilewidth, data.tileheight],
            margin: data.margin,
            spacing: data.spacing,
        });
    }
    tilesets.sort_by_key(|t| t.firstgid);

    let mut out = Out {
        origin,
        tilesets: &tilesets,
        layers: Vec::new(),
        entities: Vec::new(),
        collision: Vec::new(),
        regions: Vec::new(),
        start: None,
        layer_names: Vec::new(),
    };
    for layer in &map.layers {
        out.layer(layer, (0.0, 0.0), (1.0, 1.0))?;
    }

    let mut level = Table::new();
    level.insert("format".into(), Value::Integer(FORMAT.into()));
    let props = props(&map.properties);
    if let Some(Value::String(name)) = props.get("name") {
        level.insert("name".into(), Value::String(name.clone()));
    }
    let size = [
        f64::from(map.width * map.tilewidth),
        f64::from(map.height * map.tileheight),
    ];
    level.insert("size".into(), floats(&size));
    let start = out
        .start
        .ok_or_else(|| err("no `start` object; add a point with class `start`".into()))?;
    level.insert("start".into(), floats(&start));
    let mut camera = Table::new();
    if let Some(look) = pair_prop(&props, "camera.look_ahead_x", "camera.look_ahead_y") {
        camera.insert("look_ahead".into(), floats(&look));
    }
    if !camera.is_empty() {
        level.insert("camera".into(), Value::Table(camera));
    }
    for (key, items) in [
        ("layers", out.layers),
        ("entities", out.entities),
        ("collision", out.collision),
        ("regions", out.regions),
    ] {
        if !items.is_empty() {
            level.insert(
                key.into(),
                Value::Array(items.into_iter().map(Value::Table).collect()),
            );
        }
    }
    let text = toml::to_string(&level).expect("level table serializes");
    Level::from_toml_str(&text, origin)
}

struct Out<'a> {
    origin: &'a str,
    tilesets: &'a [Tileset],
    layers: Vec<Table>,
    entities: Vec<Table>,
    collision: Vec<Table>,
    regions: Vec<Table>,
    start: Option<[f64; 2]>,
    layer_names: Vec<String>,
}

impl Out<'_> {
    fn err(&self, m: String) -> LevelError {
        LevelError::new(self.origin, m)
    }

    fn unique_name(&mut self, name: &str) -> String {
        let mut candidate = name.to_string();
        let mut n = 2;
        while self.layer_names.contains(&candidate) {
            candidate = format!("{name} {n}");
            n += 1;
        }
        self.layer_names.push(candidate.clone());
        candidate
    }

    fn layer(
        &mut self,
        l: &TLayer,
        offset: (f64, f64),
        parallax: (f64, f64),
    ) -> Result<(), LevelError> {
        let offset = (offset.0 + l.offsetx, offset.1 + l.offsety);
        let parallax = (
            parallax.0 * l.parallaxx.unwrap_or(1.0),
            parallax.1 * l.parallaxy.unwrap_or(1.0),
        );
        let props = props(&l.properties);
        let depth = match props.get("depth") {
            None => "back",
            Some(Value::String(d)) if d == "back" || d == "front" => d.as_str(),
            Some(_) => {
                return Err(self.err(format!(
                    "layer `{}`: depth must be \"back\" or \"front\"",
                    l.name
                )));
            }
        };
        let base = |name: String| {
            let mut t = Table::new();
            t.insert("name".into(), Value::String(name));
            t.insert("depth".into(), Value::String(depth.into()));
            t.insert("parallax".into(), floats(&[parallax.0, parallax.1]));
            if offset != (0.0, 0.0) {
                t.insert("offset".into(), floats(&[offset.0, offset.1]));
            }
            t
        };
        match l.kind.as_str() {
            "group" => {
                for child in &l.layers {
                    self.layer(child, offset, parallax)?;
                }
            }
            "imagelayer" => {
                let Some(image) = l.image.as_ref().filter(|i| !i.is_empty()) else {
                    return Ok(()); // an image layer with no image draws nothing
                };
                let mut t = base(self.unique_name(&l.name));
                t.insert("image".into(), Value::String(image.clone()));
                let repeat = vec![Value::Boolean(l.repeatx), Value::Boolean(l.repeaty)];
                t.insert("repeat".into(), Value::Array(repeat));
                self.layers.push(t);
            }
            "tilelayer" => {
                if l.encoding.as_deref().is_some_and(|e| e != "csv") {
                    return Err(self.err(format!(
                        "layer `{}`: tile data is {}; set Tile Layer Format to CSV",
                        l.name,
                        l.encoding.as_deref().unwrap_or_default()
                    )));
                }
                let data: Vec<u32> = l
                    .data
                    .as_ref()
                    .and_then(|d| serde_json::from_value(d.clone()).ok())
                    .ok_or_else(|| self.err(format!("layer `{}`: no tile data", l.name)))?;
                let columns = l.width.unwrap_or(0);
                // one layer per tileset the layer uses
                for (n, ts) in self.tilesets.iter().enumerate() {
                    let end = self.tilesets.get(n + 1).map_or(u32::MAX, |t| t.firstgid);
                    let mut used = false;
                    let mut tiles = Vec::with_capacity(data.len());
                    for &gid in &data {
                        let flags = gid & 0xf000_0000;
                        let id = gid & 0x0fff_ffff;
                        if flags & 0x1000_0000 != 0 {
                            return Err(self.err(format!(
                                "layer `{}`: hexagonal rotation isn't supported",
                                l.name
                            )));
                        }
                        if id >= ts.firstgid && id < end {
                            used = true;
                            let flags = flips(flags);
                            tiles.push(Value::Integer(i64::from(flags | (id - ts.firstgid + 1))));
                        } else {
                            tiles.push(Value::Integer(0));
                        }
                    }
                    if !used {
                        continue;
                    }
                    let mut t = base(self.unique_name(&l.name));
                    let mut tt = Table::new();
                    tt.insert("tileset".into(), Value::String(ts.image.clone()));
                    let size = ts
                        .tile_size
                        .iter()
                        .map(|&s| Value::Integer(s.into()))
                        .collect();
                    tt.insert("tile_size".into(), Value::Array(size));
                    tt.insert("margin".into(), Value::Integer(ts.margin.into()));
                    tt.insert("spacing".into(), Value::Integer(ts.spacing.into()));
                    tt.insert("columns".into(), Value::Integer(columns.into()));
                    tt.insert("data".into(), Value::Array(tiles));
                    t.insert("tiles".into(), Value::Table(tt));
                    self.layers.push(t);
                }
                if let Some(&gid) = data.iter().find(|&&g| {
                    let id = g & 0x0fff_ffff;
                    id != 0 && self.tilesets.first().is_none_or(|t| id < t.firstgid)
                }) {
                    return Err(self.err(format!("layer `{}`: tile {gid} has no tileset", l.name)));
                }
            }
            "objectgroup" => {
                let collision_layer = l.name.eq_ignore_ascii_case("collision");
                for o in &l.objects {
                    self.object(o, offset, collision_layer)?;
                }
            }
            other => {
                return Err(self.err(format!("layer `{}`: unknown layer type {other}", l.name)));
            }
        }
        Ok(())
    }

    fn object(
        &mut self,
        o: &Object,
        offset: (f64, f64),
        collision_layer: bool,
    ) -> Result<(), LevelError> {
        let at = |m: &str| self.err(format!("object {}: {m}", o.id));
        let class = if !o.class.is_empty() {
            &o.class
        } else {
            &o.kind
        };
        let class = match class.as_str() {
            "" if collision_layer => "solid",
            "" => {
                return Err(at(
                    "has no class; set one, such as solid, region or an entity type",
                ));
            }
            c => c,
        };
        if o.rotation != 0.0 {
            return Err(at("rotated objects aren't supported"));
        }
        if o.ellipse {
            return Err(at("ellipses aren't supported; use a polygon"));
        }
        let (x, y) = (o.x + offset.0, o.y + offset.1);
        let props = props(&o.properties);
        let absolute =
            |ps: &[Point]| -> Vec<[f64; 2]> { ps.iter().map(|p| [x + p.x, y + p.y]).collect() };
        let rect = [x, y, o.width, o.height];
        let surface = |t: &mut Table| {
            if let Some(s) = props.get("surface") {
                t.insert("surface".into(), s.clone());
            }
        };

        match class {
            "solid" | "platform" => {
                let one_way = class == "platform"
                    || matches!(props.get("one_way"), Some(Value::Boolean(true)));
                let mut t = Table::new();
                if let Some(line) = &o.polyline {
                    t.insert("shape".into(), Value::String("polyline".into()));
                    t.insert("points".into(), points(&absolute(line)));
                    t.insert("one_way".into(), Value::Boolean(one_way));
                } else if class == "platform" && o.polygon.is_none() {
                    t.insert("shape".into(), Value::String("segment".into()));
                    t.insert("from".into(), floats(&[x, y]));
                    t.insert("to".into(), floats(&[x + o.width, y]));
                    t.insert("one_way".into(), Value::Boolean(true));
                } else if one_way {
                    return Err(at(
                        "one-way areas aren't supported; use a polyline or a platform",
                    ));
                } else if let Some(poly) = &o.polygon {
                    t.insert("shape".into(), Value::String("polygon".into()));
                    t.insert("points".into(), points(&absolute(poly)));
                } else if o.point {
                    return Err(at("a point can't be solid"));
                } else {
                    t.insert("shape".into(), Value::String("rect".into()));
                    t.insert("rect".into(), floats(&rect));
                }
                surface(&mut t);
                self.collision.push(t);
            }
            "region" => {
                let mut t = Table::new();
                if !o.name.is_empty() {
                    t.insert("name".into(), Value::String(o.name.clone()));
                }
                let bounds = if let Some(poly) = &o.polygon {
                    let abs = absolute(poly);
                    t.insert("points".into(), points(&abs));
                    let (xs, ys) = (abs.iter().map(|p| p[0]), abs.iter().map(|p| p[1]));
                    let (x0, x1) = (
                        xs.clone().fold(f64::MAX, f64::min),
                        xs.fold(f64::MIN, f64::max),
                    );
                    let (y0, y1) = (
                        ys.clone().fold(f64::MAX, f64::min),
                        ys.fold(f64::MIN, f64::max),
                    );
                    [x0, y0, x1 - x0, y1 - y0]
                } else if o.point || o.polyline.is_some() {
                    return Err(at("a region needs an area: a rectangle or polygon"));
                } else {
                    t.insert("rect".into(), floats(&rect));
                    rect
                };
                let mut physics = Table::new();
                let mut camera = Table::new();
                for (key, value) in &props {
                    match key.as_str() {
                        "water" => {
                            t.insert("water".into(), value.clone());
                        }
                        "darkness" => {
                            t.insert("darkness".into(), value.clone());
                        }
                        "wind_x" | "wind_y" => {}
                        "camera.lock_x" | "camera.lock_y" => {
                            camera.insert(key["camera.".len()..].into(), value.clone());
                        }
                        "camera.look_ahead_x" | "camera.look_ahead_y" => {}
                        "camera.limits" => {
                            if value == &Value::Boolean(true) {
                                camera.insert("limits".into(), floats(&bounds));
                            }
                        }
                        k if k.starts_with("physics.") => {
                            let param = &k["physics.".len()..];
                            match param.strip_suffix(".scale") {
                                Some(p) => {
                                    let mut s = Table::new();
                                    s.insert("scale".into(), value.clone());
                                    physics.insert(p.into(), Value::Table(s));
                                }
                                None => {
                                    physics.insert(param.into(), value.clone());
                                }
                            }
                        }
                        k => return Err(at(&format!("unknown region property `{k}`"))),
                    }
                }
                if let Some(w) = pair_prop(&props, "wind_x", "wind_y") {
                    t.insert("wind".into(), floats(&w));
                }
                if let Some(l) = pair_prop(&props, "camera.look_ahead_x", "camera.look_ahead_y") {
                    camera.insert("look_ahead".into(), floats(&l));
                }
                if !physics.is_empty() {
                    t.insert("physics".into(), Value::Table(physics));
                }
                if !camera.is_empty() {
                    t.insert("camera".into(), Value::Table(camera));
                }
                self.regions.push(t);
            }
            "start" => {
                if self.start.is_some() {
                    return Err(at("a second `start`; a map has exactly one"));
                }
                self.start = Some([x, y]);
            }
            kind => {
                let mut t = Table::new();
                t.insert("type".into(), Value::String(kind.into()));
                if !o.name.is_empty() {
                    t.insert("id".into(), Value::String(o.name.clone()));
                }
                t.insert("at".into(), floats(&[x, y]));
                let mut params = props.clone();
                if o.width != 0.0 || o.height != 0.0 {
                    params.insert("width".into(), Value::Float(o.width));
                    params.insert("height".into(), Value::Float(o.height));
                }
                if !params.is_empty() {
                    t.insert("params".into(), Value::Table(params));
                }
                self.entities.push(t);
            }
        }
        Ok(())
    }
}

/// A property list as a table, in order.
/// A tile's flip bits in Clamber's order. Tiled uses the same bits but
/// swaps x and y first and mirrors after; mirroring first means the two
/// mirrors trade places when the tile is also swapped.
fn flips(flags: u32) -> u32 {
    let mirrors = Tile::FLIP_X | Tile::FLIP_Y;
    let one_mirror = (flags & mirrors).count_ones() == 1;
    if flags & Tile::FLIP_DIAGONAL != 0 && one_mirror {
        flags ^ mirrors
    } else {
        flags
    }
}

fn props(list: &[Property]) -> Table {
    list.iter()
        .filter_map(|p| json_to_toml(&p.value).map(|v| (p.name.clone(), v)))
        .collect()
}

fn json_to_toml(j: &Json) -> Option<Value> {
    Some(match j {
        Json::Null => return None,
        Json::Bool(b) => Value::Boolean(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Float(n.as_f64()?),
        },
        Json::String(s) => Value::String(s.clone()),
        Json::Array(a) => Value::Array(a.iter().filter_map(json_to_toml).collect()),
        Json::Object(o) => Value::Table(
            o.iter()
                .filter_map(|(k, v)| json_to_toml(v).map(|v| (k.clone(), v)))
                .collect(),
        ),
    })
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// Two numeric properties as a pair, if either is present (the other is 0).
fn pair_prop(props: &Table, x: &str, y: &str) -> Option<[f64; 2]> {
    let (px, py) = (props.get(x), props.get(y));
    if px.is_none() && py.is_none() {
        return None;
    }
    Some([
        px.and_then(number).unwrap_or(0.0),
        py.and_then(number).unwrap_or(0.0),
    ])
}

/// Whole numbers as integers, so imported levels read naturally.
fn float(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        Value::Integer(x as i64)
    } else {
        Value::Float(x)
    }
}

fn floats(xs: &[f64]) -> Value {
    Value::Array(xs.iter().map(|&x| float(x)).collect())
}

fn points(ps: &[[f64; 2]]) -> Value {
    Value::Array(ps.iter().map(|p| floats(p)).collect())
}
