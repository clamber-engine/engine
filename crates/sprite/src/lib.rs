//! Sprites for the Clamber engine: PNG sheets with JSON metadata.
//!
//! Metadata is Aseprite's JSON export, read directly, plus a few optional
//! fields of Clamber's own (see [`sheet`]): exact durations in logic
//! frames, origins, hitboxes and hurtboxes, and named palettes. The ROM
//! importer will write the same format.
//!
//! Indexed PNGs stay indexed ([`Image::Indexed`]) so palettes can be
//! swapped for character variants and hit flashes ([`Palette`]). Frames
//! from many sheets are packed into texture pages with [`AtlasBuilder`].
//! [`Playback`] steps through an animation one logic frame at a time.

mod atlas;
mod image;
mod playback;
pub mod sheet;

use std::fmt;
use std::path::{Path, PathBuf};

pub use atlas::{Atlas, AtlasBuilder, AtlasError, Placement, pack};
pub use image::{
    Image, ImageError, IndexImage, Palette, Pixels, Rect, Rgba, RgbaImage, decode_png, encode_png,
};
pub use playback::Playback;
pub use sheet::{Animation, Area, AreaKind, Frame, Sheet, SheetError};

/// A sheet's metadata with its decoded image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteSheet {
    pub sheet: Sheet,
    pub image: Image,
}

impl SpriteSheet {
    /// Reads a metadata file and the image it names, relative to it.
    pub fn load(json: &Path) -> Result<SpriteSheet, LoadError> {
        let err = |path: &Path, message: String| LoadError {
            path: path.to_path_buf(),
            message,
        };
        let src = std::fs::read_to_string(json).map_err(|e| err(json, e.to_string()))?;
        let sheet = Sheet::from_json_str(&src, &json.display().to_string())
            .map_err(|e| err(json, e.message))?;
        let image_path = json.parent().unwrap_or(Path::new("")).join(&sheet.image);
        let bytes = std::fs::read(&image_path).map_err(|e| err(&image_path, e.to_string()))?;
        let image = decode_png(&bytes).map_err(|e| err(&image_path, e.0))?;
        SpriteSheet::new(sheet, image).map_err(|m| err(json, m))
    }

    /// Pairs metadata with its image, checking that every frame fits.
    pub fn new(sheet: Sheet, image: Image) -> Result<SpriteSheet, String> {
        let (w, h) = (image.width(), image.height());
        if let Some(i) = sheet.frames.iter().position(|f| !f.rect.fits(w, h)) {
            return Err(format!(
                "frame {i} is outside the {w}x{h} image `{}`",
                sheet.image
            ));
        }
        Ok(SpriteSheet { sheet, image })
    }
}

/// A sprite sheet file that couldn't be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for LoadError {}
