//! A level's ground and background in an image as a Clamber level:
//! `spec/formats/level-ground.md` for its collision and
//! `spec/formats/graphics.md` for its layers.
//!
//! Its objects come from its placement list
//! (`spec/formats/placements.md`), and its music from its song
//! ([`crate::music`]).
//!
//! No format spec says where a level's grid size and type count are
//! stored, so a [`LevelDecoder`] is given each part's location and size;
//! [`first_level`](crate::first_level) has them where the spec does.

use std::collections::BTreeMap;

use clamber_level::{Area, Collision, Entity, FORMAT, Fixed, Level, Rect, Region, Section, Track};

use crate::{
    BANK_SIZE, Background, COLUMN_BANDS, Decoder, GridPicture, Ground, Image, Known, LEVELS,
    Layout, LevelSong, Location, PackWriter, PaletteSet, Pictures, Placements, TileSet, kind_of,
    kind_rows, unpack, unpack_nibbles,
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
    /// The placement list, if the level has its objects.
    pub placements: Option<Location>,
    /// The level's song, if it has its music.
    pub song: Option<LevelSong>,
}

/// The water over a whole area with `ground`, if the player swims there:
/// `spec/behavior/swimming.md`, "Where the player swims". An area swims
/// all over exactly when its ground is a ceiling grid and it doesn't
/// start the player `riding`. The region runs from the left edge to the
/// right, and from the top of the grid to the bottom.
pub fn water(ground: &Ground, riding: bool) -> Option<Region> {
    if ground.layout() != Layout::Ceilings || riding {
        return None;
    }
    let px = |n: i64| Fixed::from_int(i32::try_from(n).expect("a level fits in i32"));
    Some(Region {
        name: Some("water".into()),
        area: Area::Rect(Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: px(ground.width()),
            h: px(ground.bands() * BAND),
        }),
        water: true,
        wind: None,
        darkness: None,
        physics: None,
        camera: None,
    })
}

/// Where a level's ground is in an image whose maps are packed, as in
/// `title-2`: the level's grid is `columns` map columns from map column
/// `first_column` of the packed map, with types 0 to `types` − 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedGround {
    pub map: Location,
    pub first_column: usize,
    pub columns: usize,
    pub type_table: Location,
    pub types: usize,
}

impl PackedGround {
    /// Unpacks the map and reads the level's ground from it.
    pub fn read(&self, image: &Image) -> Result<Ground, String> {
        let packed = image
            .at(self.map)
            .ok_or_else(|| "the level's packed map isn't in the image".to_string())?;
        let map = unpack_nibbles(packed).map_err(|e| format!("the level's map: {e}"))?;
        let types = image
            .at(self.type_table)
            .and_then(|bytes| bytes.get(..4 * self.types))
            .ok_or_else(|| "the image ends before the level's type table".to_string())?;
        Ground::title_2(&map, self.first_column, self.columns, types)
    }
}

/// Writes one level of a title as `levels/<file>.level`, with the images
/// its layers draw and its music next to it, named starting `<file>-`.
pub struct LevelDecoder {
    pub title: String,
    /// The level file's name, without `.level`.
    pub file: String,
    /// The level's name, shown to the player.
    pub name: String,
    pub parts: LevelParts,
}

impl LevelDecoder {
    /// The level, with its layers' images and its music by path relative
    /// to it.
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
        let mut files = background.images;

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
        // No format spec says how an image records an area that starts
        // the player riding, so none does.
        level.regions.extend(water(&ground, false));
        if let Some(list) = p.placements {
            (level.entities, level.sections) = self.objects(image, list, height)?;
        }
        if let Some(song) = p.song {
            let music = song.music(|at| image.at(at))?;
            let file = format!("{}-music.wav", self.file);
            level.music = Some(Track {
                file: file.clone(),
                loop_start: music.looping.map(|l| l.start as u64),
                loop_length: music.looping.map(|l| l.length as u64),
            });
            files.insert(file, music.wav());
        }
        Ok((level, files))
    }

    /// The objects in the placement list at `list`, for a level whose
    /// bottom edge is at `bottom`, with their kinds where `title`'s kind
    /// fingerprint rows give them, and the list's sections if it has them.
    fn objects(
        &self,
        image: &Image,
        list: Location,
        bottom: i64,
    ) -> Result<(Vec<Entity>, Vec<Section>), String> {
        let bank = |address| {
            let start = list.in_bank(address).position(image.len())?;
            let end = (start / BANK_SIZE + 1) * BANK_SIZE;
            image.bytes().get(start..end.min(image.len()))
        };
        let placements = Placements::read(bank, list.address)
            .map_err(|e| format!("the level's placement list: {e}"))?;
        let rows = kind_rows(&self.title);
        let kind = |reference| kind_of(image.bytes(), reference, &rows).map(|(_, kind)| kind);
        Ok(placements.objects(bottom, kind))
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
        let (level, files) = self.level(image)?;
        for (path, contents) in files {
            pack.write(&format!("{LEVELS}/{path}"), &contents)?;
        }
        pack.write(
            &format!("{LEVELS}/{}.level", self.file),
            level.to_toml_string().as_bytes(),
        )
    }
}
