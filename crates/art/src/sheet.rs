//! Sprite sheets from rendered SVGs.
//!
//! The SVG is a grid of equal frames, numbered left to right and top to
//! bottom. Its PNG is written as it is (indexed if asked), with Aseprite
//! "array" JSON beside it: a frame tag per animation, the sheet's origin
//! in `meta.clamber.origin`, and palette swaps in `meta.clamber.palettes`.
//! The placeholder art is pixel art, so sheets say `meta.clamber.pixelArt`.

use std::collections::BTreeMap;
use std::path::Path;

use clamber_sprite::{Palette, Pixels, Rgba, RgbaImage, encode_indexed_png, encode_png};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Output;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetSpec {
    /// The SVG, relative to the manifest.
    pub source: String,
    /// The JSON metadata, relative to the content folder; the PNG goes
    /// beside it with the same name.
    pub out: String,
    /// Width and height of each frame.
    pub frame: [u32; 2],
    /// The origin (pivot) within each frame, such as the feet.
    pub origin: [i64; 2],
    /// Store palette indices, so the sheet's palettes can be swapped.
    #[serde(default)]
    pub indexed: bool,
    /// Palette swaps by name: each maps colors of the sheet's own palette,
    /// `#rrggbb`, to replacements. Needs `indexed`.
    #[serde(default)]
    pub palettes: BTreeMap<String, BTreeMap<String, String>>,
    pub animations: Vec<AnimationSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationSpec {
    pub name: String,
    /// The first and last frame, inclusive.
    pub frames: [usize; 2],
    /// How long each frame shows, in 60 Hz logic frames.
    pub ticks: u32,
    /// Aseprite's: `forward` (the default), `reverse`, `pingpong` or
    /// `pingpong_reverse`.
    pub direction: Option<String>,
    /// Passes to play; loops forever without one.
    pub repeat: Option<u32>,
}

/// How long frames that no animation uses show.
const DEFAULT_TICKS: u32 = 6;

/// The PNG and JSON for a sheet whose SVG rendered to `image`.
pub fn build(spec: &SheetSpec, image: &RgbaImage) -> Result<Vec<Output>, String> {
    let [fw, fh] = spec.frame;
    if fw == 0 || fh == 0 || !image.width.is_multiple_of(fw) || !image.height.is_multiple_of(fh) {
        return Err(format!(
            "the {}x{} image isn't a whole grid of {fw}x{fh} frames",
            image.width, image.height
        ));
    }
    let (columns, rows) = (image.width / fw, image.height / fh);
    let count = (columns * rows) as usize;

    let mut ticks: Vec<Option<u32>> = vec![None; count];
    for anim in &spec.animations {
        let [from, to] = anim.frames;
        if from > to || to >= count {
            return Err(format!(
                "animation {}: frames {from} to {to} aren't within the sheet's {count} frames",
                anim.name
            ));
        }
        if anim.ticks == 0 {
            return Err(format!("animation {}: ticks must be at least 1", anim.name));
        }
        for t in &mut ticks[from..=to] {
            match *t {
                Some(other) if other != anim.ticks => {
                    return Err(format!(
                        "animation {}: shares frames with an animation of {other} ticks",
                        anim.name
                    ));
                }
                _ => *t = Some(anim.ticks),
            }
        }
    }

    let json_path = Path::new(&spec.out);
    let png_path = json_path.with_extension("png");
    let png_name = png_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("out `{}` has no file name", spec.out))?
        .to_string();
    let stem = json_path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("sheet");

    let frames: Vec<Value> = (0..count as u32)
        .map(|i| {
            let (x, y) = (i % columns * fw, i / columns * fh);
            let ticks = ticks[i as usize].unwrap_or(DEFAULT_TICKS);
            json!({
                "filename": format!("{stem} {i}"),
                "frame": { "x": x, "y": y, "w": fw, "h": fh },
                "rotated": false,
                "trimmed": false,
                "spriteSourceSize": { "x": 0, "y": 0, "w": fw, "h": fh },
                "sourceSize": { "w": fw, "h": fh },
                "duration": (ticks * 1000 + 30) / 60,
                "ticks": ticks,
            })
        })
        .collect();

    let tags: Vec<Value> = spec
        .animations
        .iter()
        .map(|a| {
            let mut tag = json!({
                "name": a.name,
                "from": a.frames[0],
                "to": a.frames[1],
                "direction": a.direction.as_deref().unwrap_or("forward"),
            });
            if let Some(n) = a.repeat {
                tag["repeat"] = json!(n.to_string());
            }
            tag
        })
        .collect();

    let (png, palettes) = if spec.indexed {
        let (indices, palette) = index(image)?;
        let palettes = swaps(&palette, &spec.palettes)?;
        (encode_indexed_png(&indices, &palette), palettes)
    } else {
        if !spec.palettes.is_empty() {
            return Err("palettes need `indexed = true`".into());
        }
        (encode_png(image), BTreeMap::new())
    };

    let mut clamber = json!({
        "origin": { "x": spec.origin[0], "y": spec.origin[1] },
        "pixelArt": true,
    });
    if !palettes.is_empty() {
        clamber["palettes"] = json!(palettes);
    }
    let metadata = json!({
        "frames": frames,
        "meta": {
            "app": "clamber-art",
            "image": png_name,
            "format": if spec.indexed { "I8" } else { "RGBA8888" },
            "size": { "w": image.width, "h": image.height },
            "scale": "1",
            "frameTags": tags,
            "clamber": clamber,
        },
    });
    let mut text = serde_json::to_string_pretty(&metadata).expect("JSON");
    text.push('\n');

    Ok(vec![
        Output {
            path: png_path,
            bytes: png,
        },
        Output {
            path: json_path.to_path_buf(),
            bytes: text.into_bytes(),
        },
    ])
}

/// The image as palette indices. Index 0 is transparent; the other colors
/// are numbered in the order they first appear, row by row.
pub fn index(image: &RgbaImage) -> Result<(Pixels<u8>, Palette), String> {
    let mut palette: Vec<Rgba> = vec![[0, 0, 0, 0]];
    let mut indices = Pixels::new(image.width, image.height);
    for (i, &c) in image.data.iter().enumerate() {
        let c = if c[3] == 0 { [0, 0, 0, 0] } else { c };
        let n = match palette.iter().position(|&p| p == c) {
            Some(n) => n,
            None => {
                palette.push(c);
                palette.len() - 1
            }
        };
        if n > 255 {
            return Err(
                "more than 256 colors; set shape-rendering=\"crispEdges\" and avoid gradients"
                    .into(),
            );
        }
        indices.data[i] = n as u8;
    }
    Ok((indices, Palette(palette)))
}

/// Each palette swap, written in index order.
fn swaps(
    base: &Palette,
    specs: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let mut out = BTreeMap::new();
    for (name, changes) in specs {
        let mut colors = base.0.clone();
        for (from, to) in changes {
            let at = |m: String| format!("palettes.{name}: {m}");
            let parse =
                |s: &str| Palette::parse_color(s).ok_or_else(|| at(format!("bad color `{s}`")));
            let (from_c, to_c) = (parse(from)?, parse(to)?);
            let i = colors
                .iter()
                .skip(1)
                .position(|&c| c == from_c)
                .ok_or_else(|| at(format!("`{from}` isn't one of the sheet's colors")))?;
            colors[i + 1] = to_c;
        }
        out.insert(name.clone(), colors.iter().map(hex).collect());
    }
    Ok(out)
}

fn hex(c: &Rgba) -> String {
    let [r, g, b, a] = *c;
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(toml_src: &str) -> SheetSpec {
        toml::from_str(toml_src).unwrap()
    }

    #[test]
    fn indexes_in_order_of_appearance() {
        let mut img: RgbaImage = Pixels::new(3, 1);
        img.data = vec![[9, 9, 9, 255], [0, 0, 0, 0], [9, 9, 9, 255]];
        let (indices, palette) = index(&img).unwrap();
        assert_eq!(indices.data, [1, 0, 1]);
        assert_eq!(palette.0, [[0, 0, 0, 0], [9, 9, 9, 255]]);
    }

    #[test]
    fn rejects_frames_that_disagree_on_ticks() {
        let s = spec(
            "source = \"a.svg\"\nout = \"a.json\"\nframe = [2, 2]\norigin = [1, 2]\n\
             [[animations]]\nname = \"a\"\nframes = [0, 1]\nticks = 4\n\
             [[animations]]\nname = \"b\"\nframes = [1, 1]\nticks = 5\n",
        );
        let err = build(&s, &Pixels::new(4, 2)).unwrap_err();
        assert!(err.contains("shares frames"), "{err}");
    }

    #[test]
    fn swaps_must_name_sheet_colors() {
        let base = Palette(vec![[0, 0, 0, 0], [1, 2, 3, 255]]);
        let mut changes = BTreeMap::new();
        changes.insert(
            "alt".to_string(),
            BTreeMap::from([("#010203".to_string(), "#ff0000".to_string())]),
        );
        assert_eq!(
            swaps(&base, &changes).unwrap()["alt"],
            ["#00000000", "#ff0000"]
        );
        changes
            .get_mut("alt")
            .unwrap()
            .insert("#ffffff".into(), "#000000".into());
        assert!(swaps(&base, &changes).is_err());
    }
}
