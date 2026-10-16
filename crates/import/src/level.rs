//! A level's ground and background in an image as a Clamber level:
//! `spec/formats/level-ground.md` for its collision and
//! `spec/formats/graphics.md` for its layers.
//!
//! `spec/formats/locators.md` doesn't say where these parts are yet, nor
//! does any format spec say where a level's grid size and type count are
//! stored, so a [`LevelDecoder`] is given each part's location and size.

use std::collections::BTreeMap;

use clamber_level::{Collision, FORMAT, Level};

use crate::{
    Background, COLUMN_BANDS, Decoder, GridPicture, Ground, Image, Known, LEVELS, Layout, Location,
    PackWriter, PaletteSet, Pictures, TileSet, unpack,
};

/// Lines in a band.
const BAND: i64 = 32;

/// Bytes in a column of a column grid.
const COLUMN_BYTES: usize = 2 * COLUMN_BANDS as usize;

/// Bytes in a palette set, and in a cell picture.
const PALETTE_SET_BYTES: usize = 256;
const PICTURE_BYTES: usize = 32;

/// The surface type of an imported level's ground.
const SURFACE: &str = "normal";

/// Where the characters start, as level x: the spec doesn't give a
/// level's start yet (#96), so they start on the highest surface in this
/// column, near the left end of the grid.
const PLACEHOLDER_START_X: i64 = 16;

/// Where each part of a level is in an image, and how big it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelParts {
    /// The ground grid, a column grid of `columns` columns.
    pub grid: Location,
    pub columns: usize,
    /// The type table, for types 0 to `types` − 1.
    pub type_table: Location,
    pub types: usize,
    /// The packed tile data; its end command ends it.
    pub tiles: Location,
    pub palettes: Location,
    /// The picture table, for types 0 to `pictures` − 1.
    pub picture_table: Location,
    pub pictures: usize,
}

/// Writes one level of a title as `levels/<file>.level`, with the images
/// its layers draw next to it, named starting `<file>-`.
pub struct LevelDecoder {
    pub title: String,
    /// The level file's name, without `.level`.
    pub file: String,
    /// The level's name, shown to the player.
    pub name: String,
    pub parts: LevelParts,
}

impl LevelDecoder {
    /// The level, with its layers' images by path relative to it.
    pub fn level(&self, image: &Image) -> Result<(Level, BTreeMap<String, Vec<u8>>), String> {
        let p = &self.parts;
        let read = |what: &str, at: Location, len: usize| {
            image
                .at(at)
                .and_then(|bytes| bytes.get(..len))
                .ok_or_else(|| format!("the image ends before the level's {what}"))
        };
        let grid = read("grid", p.grid, p.columns * COLUMN_BYTES)?;
        let ground = Ground::new(
            Layout::Columns,
            grid,
            read("type table", p.type_table, 2 * p.types)?,
        )?;
        let packed = image
            .at(p.tiles)
            .ok_or_else(|| "the level's packed tile data isn't in the image".to_string())?;
        let tiles = TileSet::new(&unpack(packed)?)?;
        let palettes = PaletteSet::new(read("palette set", p.palettes, PALETTE_SET_BYTES)?)?;
        let pictures = Pictures::new(read(
            "picture table",
            p.picture_table,
            p.pictures * PICTURE_BYTES,
        )?)?;
        let picture = GridPicture::new(grid, &pictures)?;
        let background = Background::new(&picture, &tiles, &palettes, &format!("{}-", self.file))?;

        let width = ground.width();
        let height = ground.bands() * BAND;
        let start_y = (0..ground.bands())
            .find_map(|band| ground.surface(PLACEHOLDER_START_X, band))
            .map(|surface| surface.line)
            .ok_or_else(|| {
                format!("there's no ground to start on at level x {PLACEHOLDER_START_X}")
            })?;
        let src = format!(
            "format = {FORMAT}\nname = {name:?}\nsize = [{width}, {height}]\n\
             start = [{PLACEHOLDER_START_X}, {start_y}]\n",
            name = self.name,
        );
        let mut level = Level::from_toml_str(&src, &self.file).map_err(|e| e.to_string())?;
        level.layers = background.layers;
        level.collision = Collision::new(ground.colliders(0..width, SURFACE));
        Ok((level, background.images))
    }
}

impl Decoder for LevelDecoder {
    fn name(&self) -> &str {
        &self.name
    }

    fn applies_to(&self, image: &Known) -> bool {
        image.title == self.title
    }

    fn decode(&self, image: &Image, pack: &mut PackWriter) -> Result<(), String> {
        let (level, images) = self.level(image)?;
        for (path, png) in images {
            pack.write(&format!("{LEVELS}/{path}"), &png)?;
        }
        pack.write(
            &format!("{LEVELS}/{}.level", self.file),
            level.to_toml_string().as_bytes(),
        )
    }
}
