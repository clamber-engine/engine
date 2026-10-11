//! Sprite sheet metadata: Aseprite's JSON export, plus a few optional
//! fields of Clamber's own.
//!
//! Aseprite's "hash" and "array" frame layouts are both read. On top of
//! what Aseprite writes, a sheet may have:
//!
//! - `"ticks"` on a frame: its duration in 60 Hz logic frames, used
//!   instead of the millisecond `"duration"`.
//! - `meta.clamber.origin`: `{"x", "y"}`, the origin for frames that have
//!   no `origin` slice. Defaults to the top left of the frame.
//! - `meta.clamber.palettes`: named palettes for palette swaps, each a list
//!   of `#rrggbb` or `#rrggbbaa` colors in index order.
//!
//! Slices carry per-frame geometry. A slice key applies from its frame
//! until the slice's next key; a key with zero width or height means the
//! slice is absent from that frame on. Slices are read by name:
//!
//! - `origin`: the frame's origin (pivot): the slice's pivot if it has one,
//!   otherwise its top-left corner.
//! - `hitbox…` / `hurtbox…` (for example `hitbox`, `hurtbox-head`): areas
//!   that deal and take damage.
//! - anything else: a named area, such as a spawn point for projectiles.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde_json::Value;

use crate::image::{Palette, Rect};

/// Logic frames per second, for converting Aseprite's milliseconds.
const TICKS_PER_SECOND: u32 = 60;

/// A sprite sheet's frames and animations, ready for the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    /// The sheet image, relative to the metadata file.
    pub image: String,
    pub frames: Vec<Frame>,
    /// By name, in the order the file lists them.
    pub animations: Vec<Animation>,
    pub palettes: BTreeMap<String, Palette>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Where the frame's pixels are in the sheet image.
    pub rect: Rect,
    /// Where those pixels go in the untrimmed frame, relative to the
    /// origin: draw `rect` at the sprite's position plus this.
    pub offset: (i32, i32),
    /// Duration in logic frames, at least 1.
    pub ticks: u32,
    /// Areas present on this frame, relative to the origin.
    pub boxes: Vec<Area>,
}

/// A named area of a frame, relative to the frame's origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Area {
    pub name: String,
    pub kind: AreaKind,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AreaKind {
    /// Deals damage.
    Hitbox,
    /// Takes damage.
    Hurtbox,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animation {
    pub name: String,
    /// Frame indices for one pass, with the direction already applied
    /// (a ping-pong over 0..=3 is `[0, 1, 2, 3, 2, 1]`).
    pub frames: Vec<usize>,
    /// How many passes to play; `None` loops forever.
    pub repeat: Option<u32>,
}

impl Sheet {
    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.iter().find(|a| a.name == name)
    }

    /// Reads sheet metadata. `origin` names the source in error messages.
    pub fn from_json_str(src: &str, origin: &str) -> Result<Sheet, SheetError> {
        let err = |message: String| SheetError {
            origin: origin.to_string(),
            message,
        };
        let file: File = serde_json::from_str(src).map_err(|e| err(e.to_string()))?;
        parse(file).map_err(err)
    }
}

/// Sheet metadata that couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetError {
    pub origin: String,
    pub message: String,
}

impl fmt::Display for SheetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for SheetError {}

#[derive(Deserialize)]
struct File {
    frames: Value,
    meta: Meta,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    image: String,
    size: Option<Size>,
    scale: Option<String>,
    #[serde(default)]
    frame_tags: Vec<Tag>,
    #[serde(default)]
    slices: Vec<Slice>,
    clamber: Option<Extensions>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extensions {
    origin: Option<Point>,
    #[serde(default)]
    palettes: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFrame {
    frame: JsonRect,
    #[serde(default)]
    rotated: bool,
    sprite_source_size: Option<JsonRect>,
    duration: Option<u32>,
    ticks: Option<u32>,
}

#[derive(Deserialize)]
struct Tag {
    name: String,
    from: usize,
    to: usize,
    #[serde(default = "forward")]
    direction: String,
    repeat: Option<String>,
}

fn forward() -> String {
    "forward".into()
}

#[derive(Deserialize)]
struct Slice {
    name: String,
    keys: Vec<SliceKey>,
}

#[derive(Deserialize)]
struct SliceKey {
    frame: usize,
    bounds: JsonRect,
    pivot: Option<Point>,
}

#[derive(Clone, Copy, Deserialize)]
struct JsonRect {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

#[derive(Clone, Copy, Deserialize)]
struct Point {
    x: i64,
    y: i64,
}

#[derive(Clone, Copy, Deserialize)]
struct Size {
    w: u32,
    h: u32,
}

fn parse(file: File) -> Result<Sheet, String> {
    let meta = file.meta;
    if let Some(scale) = meta.scale.as_deref()
        && scale != "1"
    {
        return Err(format!(
            "meta.scale: exported at scale {scale}; export at 1"
        ));
    }

    let raw: Vec<RawFrame> = match file.frames {
        // "hash" layout: frames keyed by name, in file order
        Value::Object(map) => map
            .into_iter()
            .map(|(name, v)| serde_json::from_value(v).map_err(|e| format!("frames.{name}: {e}")))
            .collect::<Result<_, _>>()?,
        // "array" layout
        Value::Array(list) => list
            .into_iter()
            .enumerate()
            .map(|(i, v)| serde_json::from_value(v).map_err(|e| format!("frames[{i}]: {e}")))
            .collect::<Result<_, _>>()?,
        _ => return Err("frames: expected an object or an array".into()),
    };
    if raw.is_empty() {
        return Err("frames: no frames".into());
    }

    let ext = meta.clamber;
    let default_origin = ext
        .as_ref()
        .and_then(|e| e.origin)
        .map_or((0, 0), |p| (p.x, p.y));
    let origins = slice_track(&meta.slices, "origin", raw.len())?;

    let mut frames = Vec::with_capacity(raw.len());
    let mut frame_origins = Vec::with_capacity(raw.len());
    for (i, f) in raw.iter().enumerate() {
        let at = |m: &str| format!("frame {i}: {m}");
        if f.rotated {
            return Err(at(
                "rotated frames aren't supported; export without rotation",
            ));
        }
        let rect = to_rect(f.frame).ok_or_else(|| at("frame rectangle is negative"))?;
        if let Some(size) = meta.size
            && !rect.fits(size.w, size.h)
        {
            return Err(at("frame rectangle is outside the sheet image"));
        }
        let ticks = match (f.ticks, f.duration) {
            (Some(0), _) => return Err(at("ticks must be at least 1")),
            (Some(t), _) => t,
            // nearest whole logic frame, at least one
            (None, Some(ms)) => ((ms * TICKS_PER_SECOND + 500) / 1000).max(1),
            (None, None) => return Err(at("no duration or ticks")),
        };
        let origin = match origins[i] {
            Some((key, bounds)) => match key.pivot {
                Some(p) => (bounds.x + p.x, bounds.y + p.y),
                None => (bounds.x, bounds.y),
            },
            None => default_origin,
        };
        let trim = f.sprite_source_size.map_or((0, 0), |s| (s.x, s.y));
        let offset =
            relative(trim.0, trim.1, origin).ok_or_else(|| at("coordinate out of range"))?;
        frames.push(Frame {
            rect,
            offset,
            ticks,
            boxes: Vec::new(),
        });
        frame_origins.push(origin);
    }

    for slice in &meta.slices {
        if slice.name == "origin" {
            continue;
        }
        let kind = if slice.name.starts_with("hitbox") {
            AreaKind::Hitbox
        } else if slice.name.starts_with("hurtbox") {
            AreaKind::Hurtbox
        } else {
            AreaKind::Other
        };
        let track = slice_track(&meta.slices, &slice.name, frames.len())?;
        for (i, key) in track.into_iter().enumerate() {
            let Some((_, b)) = key else { continue };
            let (x, y) = relative(b.x, b.y, frame_origins[i])
                .ok_or_else(|| format!("slice {}: coordinate out of range", slice.name))?;
            frames[i].boxes.push(Area {
                name: slice.name.clone(),
                kind,
                x,
                y,
                w: b.w as u32,
                h: b.h as u32,
            });
        }
    }

    let mut animations: Vec<Animation> = Vec::new();
    for tag in meta.frame_tags {
        let at = |m: String| format!("frameTags.{}: {m}", tag.name);
        if animations.iter().any(|a| a.name == tag.name) {
            return Err(at("listed twice".into()));
        }
        if tag.from > tag.to || tag.to >= frames.len() {
            return Err(at(format!(
                "frames {} to {} aren't within the sheet's {} frames",
                tag.from,
                tag.to,
                frames.len()
            )));
        }
        let up: Vec<usize> = (tag.from..=tag.to).collect();
        let down: Vec<usize> = up.iter().rev().copied().collect();
        // the far ends of a ping-pong play once per pass
        let inner = |v: &[usize]| v[1..v.len().saturating_sub(1).max(1)].to_vec();
        let frames = match tag.direction.as_str() {
            "forward" => up,
            "reverse" => down,
            "pingpong" => [up.clone(), inner(&down)].concat(),
            "pingpong_reverse" => [down.clone(), inner(&up)].concat(),
            d => return Err(at(format!("unknown direction `{d}`"))),
        };
        let repeat = match tag.repeat.as_deref() {
            None | Some("") | Some("0") => None,
            Some(n) => Some(
                n.parse::<u32>()
                    .map_err(|_| at(format!("repeat `{n}` isn't a whole number")))?,
            ),
        };
        animations.push(Animation {
            name: tag.name,
            frames,
            repeat,
        });
    }

    let mut palettes = BTreeMap::new();
    for (name, colors) in ext.map(|e| e.palettes).unwrap_or_default() {
        let parsed = colors
            .iter()
            .map(|c| {
                Palette::parse_color(c)
                    .ok_or_else(|| format!("meta.clamber.palettes.{name}: bad color `{c}`"))
            })
            .collect::<Result<_, _>>()?;
        palettes.insert(name, Palette(parsed));
    }

    Ok(Sheet {
        image: meta.image,
        frames,
        animations,
        palettes,
    })
}

/// A point relative to `origin`.
fn relative(x: i64, y: i64, origin: (i64, i64)) -> Option<(i32, i32)> {
    Some((
        i32::try_from(x - origin.0).ok()?,
        i32::try_from(y - origin.1).ok()?,
    ))
}

fn to_rect(r: JsonRect) -> Option<Rect> {
    Some(Rect {
        x: u32::try_from(r.x).ok()?,
        y: u32::try_from(r.y).ok()?,
        w: u32::try_from(r.w).ok()?,
        h: u32::try_from(r.h).ok()?,
    })
}

/// For each frame, the key of the slice named `name` that applies to it,
/// if any.
fn slice_track<'a>(
    slices: &'a [Slice],
    name: &str,
    frames: usize,
) -> Result<Vec<Option<(&'a SliceKey, JsonRect)>>, String> {
    let mut found = slices.iter().filter(|s| s.name == name);
    let mut track = vec![None; frames];
    let Some(slice) = found.next() else {
        return Ok(track);
    };
    if found.next().is_some() {
        return Err(format!("slice {name}: listed twice"));
    }
    let mut keys: Vec<&SliceKey> = slice.keys.iter().collect();
    keys.sort_by_key(|k| k.frame);
    for (n, key) in keys.iter().enumerate() {
        if key.frame >= frames {
            return Err(format!(
                "slice {name}: key on frame {}, but the sheet has {frames} frames",
                key.frame
            ));
        }
        if n > 0 && keys[n - 1].frame == key.frame {
            return Err(format!("slice {name}: two keys on frame {}", key.frame));
        }
        let b = key.bounds;
        if b.w < 0 || b.h < 0 {
            return Err(format!(
                "slice {name}: negative size on frame {}",
                key.frame
            ));
        }
        let end = keys.get(n + 1).map_or(frames, |k| k.frame);
        let value = (b.w > 0 && b.h > 0).then_some((*key, b));
        track[key.frame..end].fill(value);
    }
    Ok(track)
}
