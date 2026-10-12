//! Sprites for the Clamber engine: PNG sheets with JSON metadata.
//!
//! Metadata is Aseprite's JSON export, read directly, plus a few optional
//! fields of Clamber's own (see [`sheet`]): exact durations in logic
//! frames, origins, hitboxes and hurtboxes, and named palettes. The ROM
//! importer will write the same format.
//!
//! Indexed PNGs stay indexed ([`Image::Indexed`]) so palettes can be
//! swapped for character variants and hit flashes ([`Palette`]); other
//! sheets can name whole alternative images instead. Frames
//! from many sheets are packed into texture pages with [`AtlasBuilder`].
//! [`Playback`] steps through an animation one logic frame at a time.

mod atlas;
mod image;
mod playback;
pub mod sheet;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub use atlas::{Atlas, AtlasBuilder, AtlasError, Placement, pack};
pub use image::{
    Image, ImageError, IndexImage, Palette, Pixels, Rect, Rgba, RgbaImage, decode_png,
    encode_indexed_png, encode_png,
};
pub use playback::Playback;
pub use sheet::{Animation, Area, AreaKind, Frame, Sheet, SheetError};

/// A sheet's metadata with its decoded images.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteSheet {
    pub sheet: Sheet,
    pub image: Image,
    /// The sheet's named variants, each the size of `image`.
    pub variants: BTreeMap<String, Image>,
}

impl SpriteSheet {
    /// Reads a metadata file and the images it names, relative to it.
    pub fn load(json: &Path) -> Result<SpriteSheet, LoadError> {
        let err = |path: &Path, message: String| LoadError {
            path: path.to_path_buf(),
            message,
        };
        let src = std::fs::read_to_string(json).map_err(|e| err(json, e.to_string()))?;
        let sheet = Sheet::from_json_str(&src, &json.display().to_string())
            .map_err(|e| err(json, e.message))?;
        let dir = json.parent().unwrap_or(Path::new(""));
        let read = |name: &str| {
            let path = dir.join(name);
            let bytes = std::fs::read(&path).map_err(|e| err(&path, e.to_string()))?;
            decode_png(&bytes).map_err(|e| err(&path, e.0))
        };
        let image = read(&sheet.image)?;
        let mut variants = BTreeMap::new();
        for (name, file) in &sheet.variants {
            variants.insert(name.clone(), read(file)?);
        }
        SpriteSheet::new(sheet, image)
            .and_then(|s| s.with_variants(variants))
            .map_err(|m| err(json, m))
    }

    /// Pairs metadata with its image, checking that every frame fits.
    /// It has no variant images yet.
    pub fn new(sheet: Sheet, image: Image) -> Result<SpriteSheet, String> {
        let (w, h) = (image.width(), image.height());
        if let Some(i) = sheet.frames.iter().position(|f| !f.rect.fits(w, h)) {
            return Err(format!(
                "frame {i} is outside the {w}x{h} image `{}`",
                sheet.image
            ));
        }
        Ok(SpriteSheet {
            sheet,
            image,
            variants: BTreeMap::new(),
        })
    }

    /// Adds the images for the sheet's variants, checking that each is
    /// named by the sheet and is the size of its image.
    pub fn with_variants(
        mut self,
        variants: BTreeMap<String, Image>,
    ) -> Result<SpriteSheet, String> {
        let size = (self.image.width(), self.image.height());
        for (name, image) in &variants {
            if !self.sheet.variants.contains_key(name) {
                return Err(format!("variant `{name}` isn't in the metadata"));
            }
            let (w, h) = (image.width(), image.height());
            if (w, h) != size {
                return Err(format!(
                    "variant `{name}` is {w}x{h}, but the sheet image is {}x{}",
                    size.0, size.1
                ));
            }
        }
        self.variants = variants;
        Ok(self)
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
