//! A level's ground and background in an image as a Clamber level:
//! `spec/formats/level-ground.md` for its collision and
//! `spec/formats/graphics.md` for its layers.
//!
//! Its objects come from its placement list
//! (`spec/formats/placements.md`) and its single collectibles from its
//! group list (`spec/formats/collectibles.md`), its camera rules from
//! its view-limit record ([`crate::ViewLimits`]), and its music from its song
//! ([`crate::music`]).
//!
//! No format spec says where a level's grid size and type count are
//! stored, so a [`LevelDecoder`] is given each part's location and size;
//! [`first_level`](crate::first_level) has them where the spec does.

use std::collections::BTreeMap;

use clamber_level::{
    Area, Arrival, Collision, Dir, Entity, FORMAT, Fixed, Level, Rect, Region, Scene, Section,
    Track, Vec2,
};

use crate::{
    BANK_SIZE, Background, COLUMN_BANDS, Collectibles, Decoder, END, EXIT, GridPicture, Ground,
    Image, KEG, Known, LEVELS, Layout, LevelSong, Location, Optional, PackWriter, PaletteSet,
    Pictures, Placements, ROOM_ENTRANCE, ScreenMap, Skipped, StartRecord, TileSet, VIEW_HEIGHT,
    VIEW_WIDTH, ViewLimits, WALL_EXIT, WALL_RETURN, kind_rows, object_of, settings_rows, unpack,
    unpack_nibbles,
};

/// Lines in a band.
const BAND: i64 = 32;

/// Bytes in a column of a column grid.
const COLUMN_BYTES: usize = 2 * COLUMN_BANDS as usize;

/// Bytes in a palette set, and in a cell picture.
const PALETTE_SET_BYTES: usize = 256;
const PICTURE_BYTES: usize = 32;

/// Bytes in a tile stored as it is, and in a two-plane tile.
const TILE_BYTES: usize = 32;
const TWO_PLANE_TILE_BYTES: usize = 16;

/// The surface type of an imported level's ground.
const SURFACE: &str = "normal";

/// Where the characters start, as level x, in a level without a start
/// record: on the highest surface in this column, near the left end of
/// the grid.
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
    /// Its background pictures.
    pub scenery: Scenery,
    /// The placement list, if the level has its objects.
    pub placements: Option<Location>,
    /// The group list and the shape table, if the level has its single
    /// collectibles.
    pub collectibles: Option<CollectibleParts>,
    /// The start record, if the level has one (`level-start.md`).
    pub start: Option<Location>,
    /// The way the characters walk in to the start record's point when
    /// play comes into the level from another one, at the arrival
    /// [`FROM_LEVEL`], if it does.
    pub walk_in: Option<Dir>,
    /// The restart record, if the level has a restart point.
    pub restart: Option<Location>,
    /// The scene-return record, if play comes back to the level after
    /// the area its floor exit leads to, walking in rightward.
    pub scene_return: Option<Location>,
    /// The room-return record, if play comes back to the level after the
    /// area its upper exit leads to, beginning as at its start.
    pub room_return: Option<Location>,
    /// The wall-return record, if play comes back to the level after the
    /// area its first wall exit leads to, out of its wall-return
    /// entrance.
    pub wall_return: Option<Location>,
    /// The second wall-return record, if play comes back to the level
    /// after the area its second wall exit leads to, dropping in.
    pub second_wall_return: Option<Location>,
    /// The file name, without `.level`, of the area the upper exit leads
    /// to, if that's imported too: play comes into it at [`FROM_LEVEL`].
    /// Without it the exit leads straight to [`ROOM_RETURN`].
    pub room: Option<&'static str>,
    /// The file name, without `.level`, of the level that passing an end
    /// marker goes back into, at its [`ROOM_RETURN`] arrival, instead of
    /// finishing this one, as in the first level's room
    /// (`level-end.md`, "In the first level's room").
    pub back_to: Option<&'static str>,
    /// The file name, without `.level`, of the area the floor exit leads
    /// to, if that's imported too: play comes into it at [`FROM_LEVEL`].
    /// Without it the exit leads straight to [`SCENE_RETURN`].
    pub scene: Option<&'static str>,
    /// The file name, without `.level`, of the level this area is the
    /// scene of, if it's one: it plays by itself with the timing of
    /// `exits.md` ("The scene"), and then play goes back into that level
    /// at its [`SCENE_RETURN`] arrival.
    pub scene_of: Option<&'static str>,
    /// The view-limit record, if the level has one (`view-limits.md`).
    pub view_limits: Option<Location>,
    /// The level's song, if it has its music.
    pub song: Option<LevelSong>,
}

/// Where a level's single collectibles are in an image
/// (`spec/formats/collectibles.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectibleParts {
    pub groups: Location,
    pub shapes: Location,
}

/// Where a level's background pictures are in an image
/// (`spec/formats/graphics.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenery {
    /// Layers made from the grid's cells ("The layers"): the packed tile
    /// data, which its end command ends, the palette set, and the
    /// picture table for types 0 to `pictures` − 1.
    Grid {
        tiles: Location,
        palettes: Location,
        picture_table: Location,
        pictures: usize,
    },
    /// One screen picture ("Screen pictures"): its 1 024 tiles, stored
    /// as they are, the first `split` at `tiles` and the others at
    /// `rest`; its screen map; and its palette set.
    Screen {
        tiles: Location,
        split: usize,
        rest: Location,
        map: Location,
        palettes: Location,
    },
    /// Layers made from the grid's cells, with the far picture from its
    /// first columns, and a front layer from a screen map of two-plane
    /// tiles, as in the first level's scene ("The first level's scene"):
    /// `tiles`, `palettes`, `picture_table` and `pictures` as for
    /// [`Scenery::Grid`], then the first `front_count` tiles of the
    /// front tile set and the front map.
    Scene {
        tiles: Location,
        palettes: Location,
        picture_table: Location,
        pictures: usize,
        front_tiles: Location,
        front_count: usize,
        front_map: Location,
    },
}

/// Tiles in a screen picture's tile set.
const SCREEN_TILES: usize = 1024;

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

/// The arrival play comes back to the first level at after the scene its
/// floor exit leads to.
pub const SCENE_RETURN: &str = "from-scene";

/// The arrival play comes back to the first level at after the room its
/// upper exit leads to.
pub const ROOM_RETURN: &str = "from-room";

/// The arrival play comes back to the first level at after the area its
/// first wall exit leads to.
pub const WALL_RETURN_ARRIVAL: &str = "from-wall";

/// The arrival play comes back to the first level at after the area its
/// second wall exit leads to.
pub const SECOND_WALL_RETURN_ARRIVAL: &str = "from-second-wall";

/// The arrival play comes into an area at from the level it was entered
/// from, as into the first level's room.
pub const FROM_LEVEL: &str = "from-level";

/// Says where each exit in `entities` leads, with `scene` and `room` the
/// arrivals there are of [`SCENE_RETURN`] and [`ROOM_RETURN`]: exits.md
/// has the floor exit lead to the scene and the upper one to the room.
/// Where an exit leads is a setting of its definition that no format
/// spec gives, so the lowest exit is taken to be the floor one and the
/// others the upper one.
///
/// The floor exit leads into the scene at [`FROM_LEVEL`] if the scene
/// is imported as `into.scene`, and the upper one into the room if it is
/// imported as `into.room`. Without them, each leads straight to where
/// play comes back after it.
fn lead_exits(entities: &mut [Entity], scene: bool, room: bool, into: Leads) {
    let floor = entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == EXIT)
        .max_by_key(|(_, e)| e.at.y)
        .map(|(i, _)| i);
    for (i, exit) in entities
        .iter_mut()
        .enumerate()
        .filter(|(_, e)| e.kind == EXIT)
    {
        let (area, back) = match Some(i) == floor {
            true => (into.scene, scene.then_some(SCENE_RETURN)),
            false => (into.room, room.then_some(ROOM_RETURN)),
        };
        let leads = match area {
            Some(area) => Some((Some(area), FROM_LEVEL)),
            None => back.map(|arrival| (None, arrival)),
        };
        if let Some((area, arrival)) = leads {
            lead(exit, area, arrival);
        }
    }
}

/// The areas a level's exits lead into, by file name without `.level`,
/// where they're imported.
#[derive(Clone, Copy, Debug, Default)]
struct Leads {
    scene: Option<&'static str>,
    room: Option<&'static str>,
}

/// The scene of a level, as `exits.md` has the first level's ("The
/// scene"): frame 396 is its last, and the brightness is 15 − `j` on
/// frames 365 + 2`j` and 366 + 2`j`, so 15 to frame 366 and 0 on 395 and
/// 396. Then play goes on in `level` at its [`SCENE_RETURN`] arrival.
fn scene_of(level: &str) -> Scene {
    Scene {
        last: 396,
        fade: 365,
        fade_step: 2,
        area: Some(level.into()),
        arrival: Some(SCENE_RETURN.into()),
    }
}

/// Has the wall exits in `entities`, from left to right, lead to
/// [`WALL_RETURN_ARRIVAL`] if `first` and [`SECOND_WALL_RETURN_ARRIVAL`]
/// if `second`: wall-exits.md has play come back to the level there
/// after the area behind each. Those areas aren't imported, so the wall
/// exits lead straight back.
fn lead_wall_exits(entities: &mut [Entity], first: bool, second: bool) {
    let mut walls: Vec<_> = entities
        .iter_mut()
        .filter(|e| e.kind == WALL_EXIT)
        .collect();
    walls.sort_by_key(|e| e.at.x);
    let arrivals = [
        first.then_some(WALL_RETURN_ARRIVAL),
        second.then_some(SECOND_WALL_RETURN_ARRIVAL),
    ];
    for (wall, arrival) in walls.into_iter().zip(arrivals) {
        if let Some(arrival) = arrival {
            lead(wall, None, arrival);
        }
    }
}

/// Has `entity` lead to `arrival`, in the area `area` if it names one.
fn lead(entity: &mut Entity, area: Option<&str>, arrival: &str) {
    if let Some(area) = area {
        entity
            .params
            .insert("area".into(), toml::Value::String(area.into()));
    }
    entity
        .params
        .insert("arrival".into(), toml::Value::String(arrival.into()));
}

/// Has every end marker in `entities` lead back into the level `area` at
/// its [`ROOM_RETURN`] arrival: `level-end.md` has what follows the
/// marker depend on the area it's in, not on the marker.
fn lead_back(entities: &mut [Entity], area: &str) {
    for marker in entities.iter_mut().filter(|e| e.kind == END) {
        lead(marker, Some(area), ROOM_RETURN);
    }
}

/// Makes `at` the restart point of every restart barrel in `entities`:
/// `level-start.md` says the level's restart record decides it, not
/// where the barrel is.
fn restart_at(entities: &mut [Entity], (x, y): (i64, i64)) {
    for keg in entities.iter_mut().filter(|e| e.kind == KEG) {
        let offset = [x - keg.at.x.floor(), y - keg.at.y.floor()];
        keg.params.insert(
            "restart".into(),
            toml::Value::Array(offset.map(toml::Value::Integer).into()),
        );
    }
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
    /// to it, the optional parts it goes without, and notes on how parts
    /// were converted.
    #[allow(clippy::type_complexity)]
    pub fn level(
        &self,
        image: &Image,
    ) -> Result<(Level, BTreeMap<String, Vec<u8>>, Vec<Skipped>, Vec<String>), String> {
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
        let width = ground.width();
        let height = ground.bands() * BAND;
        let limits = match p.view_limits {
            Some(at) => {
                let bytes = image.at(at).ok_or_else(|| {
                    "the level's view-limit record isn't in the image".to_string()
                })?;
                Some(ViewLimits::read(bytes)?)
            }
            None => None,
        };
        let prefix = format!("{}-", self.file);
        let grid_layers = |tiles, palettes, picture_table, pictures: usize| {
            let packed = image
                .at(tiles)
                .ok_or_else(|| "the level's packed tile data isn't in the image".to_string())?;
            let tiles = TileSet::new(&unpack(packed)?)?;
            let palettes = PaletteSet::new(read("palette set", palettes, PALETTE_SET_BYTES)?)?;
            let pictures = Pictures::new(read(
                "picture table",
                picture_table,
                pictures * PICTURE_BYTES,
            )?)?;
            let picture = GridPicture::new(grid, &pictures)?;
            Ok::<_, String>((picture, tiles, palettes))
        };
        let background = match p.scenery {
            Scenery::Grid {
                tiles,
                palettes,
                picture_table,
                pictures,
            } => {
                let (picture, tiles, palettes) =
                    grid_layers(tiles, palettes, picture_table, pictures)?;
                Background::new(&picture, &tiles, &palettes, &prefix)?
            }
            Scenery::Scene {
                tiles,
                palettes,
                picture_table,
                pictures,
                front_tiles,
                front_count,
                front_map,
            } => {
                let (picture, tiles, palettes) =
                    grid_layers(tiles, palettes, picture_table, pictures)?;
                let front_tiles = TileSet::two_plane(read(
                    "front tile set",
                    front_tiles,
                    front_count * TWO_PLANE_TILE_BYTES,
                )?)?;
                let map = ScreenMap::new(read("front map", front_map, ScreenMap::BYTES)?)?;
                // The view's top left: the left limit, and the top of
                // the first vertical limit, as the scene's are
                // (`level-ends.md`, "In the first level's scene").
                let view = limits.as_ref().map_or_else(Vec2::default, |l| {
                    let top = l.vertical.first().map_or(0, |v| v.top);
                    Vec2::ints(i32::from(l.left), i32::from(top))
                });
                Background::scene(
                    &picture,
                    &tiles,
                    &palettes,
                    (&map, &front_tiles),
                    view,
                    &prefix,
                )?
            }
            Scenery::Screen {
                tiles,
                split,
                rest,
                map,
                palettes,
            } => {
                let split = split.min(SCREEN_TILES);
                let mut bytes = read("tile set", tiles, split * TILE_BYTES)?.to_vec();
                bytes.extend(read("tile set", rest, (SCREEN_TILES - split) * TILE_BYTES)?);
                let tiles = TileSet::new(&bytes)?;
                let palettes = PaletteSet::new(read("palette set", palettes, PALETTE_SET_BYTES)?)?;
                let map = ScreenMap::new(read("screen map", map, ScreenMap::BYTES)?)?;
                Background::screen(&map, &tiles, &palettes, &prefix)?
            }
        };
        let mut files = background.images;

        let record = |what: &str, at: Location| {
            image
                .at(at)
                .and_then(StartRecord::read)
                .map(|r| (i64::from(r.x), r.level_y(height)))
                .ok_or_else(|| format!("the image ends before the level's {what} record"))
        };
        let (start_x, start_y) = match p.start {
            Some(at) => record("start", at)?,
            None => (0..ground.bands())
                .find_map(|band| ground.surface(PLACEHOLDER_START_X, band))
                .map(|surface| (PLACEHOLDER_START_X, surface.line))
                .ok_or_else(|| {
                    format!("there's no ground to start on at level x {PLACEHOLDER_START_X}")
                })?,
        };
        let src = format!(
            "format = {FORMAT}\nname = {name:?}\nsize = [{width}, {height}]\n\
             start = [{start_x}, {start_y}]\n",
            name = self.name,
        );
        let mut level = Level::from_toml_str(&src, &self.file).map_err(|e| e.to_string())?;
        level.layers = background.layers;
        level.collision = Collision::new(ground.colliders(0..width, SURFACE));
        // No format spec says how an image records an area that starts
        // the player riding, so none does.
        level.regions.extend(water(&ground, false));
        if let Some(limits) = limits {
            let regions;
            (level.camera.rule, regions) = limits.camera(width, height);
            level.regions.extend(regions);
        }
        if let Scenery::Screen { .. } = p.scenery {
            // A screen picture doesn't move: the view always shows its
            // top left.
            let px = |n: i64| Fixed::from_int(i32::try_from(n).expect("a view fits in i32"));
            level.camera.rule.limits = Some(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: px(VIEW_WIDTH),
                h: px(VIEW_HEIGHT),
            });
        }
        if let Some(list) = p.placements {
            (level.entities, level.sections) = self.objects(image, list, height)?;
        }
        if let Some(at) = p.collectibles {
            let collectibles = Collectibles::read(
                bank_of(image, at.groups),
                at.groups.address,
                bank_of(image, at.shapes),
                at.shapes.address,
            )
            .map_err(|e| format!("the level's single collectibles: {e}"))?;
            level.entities.extend(collectibles.entities());
        }
        if let Some(at) = p.restart {
            restart_at(&mut level.entities, record("restart", at)?);
        }
        if let Some(walk) = p.walk_in {
            level.arrivals.push(Arrival {
                name: FROM_LEVEL.into(),
                at: level.start,
                walk: Some(walk),
                entrance: None,
                drop: false,
            });
        }
        let returns = [
            (
                SCENE_RETURN,
                "scene-return",
                p.scene_return,
                Some(Dir::Right),
            ),
            (ROOM_RETURN, "room-return", p.room_return, None),
        ];
        for (name, what, at, walk) in returns {
            if let Some(at) = at {
                let (x, y) = record(what, at)?;
                let px = |n: i64| Fixed::from_int(i32::try_from(n).expect("a level fits in i32"));
                level.arrivals.push(Arrival {
                    name: name.into(),
                    at: Vec2::new(px(x), px(y)),
                    walk,
                    entrance: return_entrance(name, &level.entities),
                    drop: false,
                });
            }
        }
        // wall-exits.md: thrown out of the wall-return entrance from the
        // record's point, where the level has both, and dropping in at
        // the second record's point, into the barrel there.
        let px = |n: i64| Fixed::from_int(i32::try_from(n).expect("a level fits in i32"));
        let mut first = false;
        if let Some(at) = p.wall_return
            && level
                .entities
                .iter()
                .any(|e| e.id.as_deref() == Some(WALL_RETURN))
        {
            let (x, y) = record("wall-return", at)?;
            level.arrivals.push(Arrival {
                name: WALL_RETURN_ARRIVAL.into(),
                at: Vec2::new(px(x), px(y)),
                walk: None,
                entrance: Some(WALL_RETURN.into()),
                drop: false,
            });
            first = true;
        }
        if let Some(at) = p.second_wall_return {
            let (x, y) = record("second wall-return", at)?;
            level.arrivals.push(Arrival {
                name: SECOND_WALL_RETURN_ARRIVAL.into(),
                at: Vec2::new(px(x), px(y)),
                walk: None,
                entrance: None,
                drop: true,
            });
        }
        lead_wall_exits(&mut level.entities, first, p.second_wall_return.is_some());
        lead_exits(
            &mut level.entities,
            p.scene_return.is_some(),
            p.room_return.is_some(),
            Leads {
                scene: p.scene,
                room: p.room,
            },
        );
        if let Some(area) = p.back_to {
            lead_back(&mut level.entities, area);
        }
        level.scene = p.scene_of.map(scene_of);
        // Music is optional: a song that can't be converted leaves the
        // level to the game's own music.
        let (mut skipped, mut notes) = (Vec::new(), Vec::new());
        if let Some(song) = p.song {
            let read = |at| image.at(at);
            match crate::optional(|| song.music(read)) {
                Ok(music) => {
                    notes.push(format!(
                        "song {}: {}",
                        song.song,
                        match music.looping {
                            Some(_) => music.fit.describe(),
                            None => "ends",
                        }
                    ));
                    let file = format!("{}-music.wav", self.file);
                    level.music = Some(Track {
                        file: file.clone(),
                        loop_start: music.looping.map(|l| l.start as u64),
                        loop_length: music.looping.map(|l| l.length as u64),
                    });
                    files.insert(file, music.wav());
                }
                Err(reason) => skipped.push(Skipped {
                    part: Optional::Song,
                    name: format!("song {}", song.song),
                    reason,
                    diagnostics: crate::optional(|| Ok(song.diagnose(read)))
                        .unwrap_or_else(|e| vec![e]),
                }),
            }
        }
        Ok((level, files, skipped, notes))
    }

    /// The objects in the placement list at `list`, for a level whose
    /// bottom edge is at `bottom`, with their kinds where `title`'s kind
    /// fingerprint rows give them and their settings, and the list's
    /// sections if it has them.
    fn objects(
        &self,
        image: &Image,
        list: Location,
        bottom: i64,
    ) -> Result<(Vec<Entity>, Vec<Section>), String> {
        let placements = Placements::read(bank_of(image, list), list.address)
            .map_err(|e| format!("the level's placement list: {e}"))?;
        let rows = kind_rows(&self.title);
        let settings = settings_rows(&self.title);
        let kind = |reference| object_of(image.bytes(), reference, &rows, &settings);
        Ok(placements.objects(bottom, kind))
    }
}

/// The bytes of `at`'s bank in `image`, from an address to the bank's
/// end, or `None` where the image doesn't have that address.
fn bank_of<'a>(image: &'a Image, at: Location) -> impl Fn(u16) -> Option<&'a [u8]> {
    move |address| {
        let start = at.in_bank(address).position(image.len())?;
        let end = (start / BANK_SIZE + 1) * BANK_SIZE;
        image.bytes().get(start..end.min(image.len()))
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
        let (level, files, skipped, notes) = self.level(image)?;
        for skipped in skipped {
            pack.skip(skipped);
        }
        for note in notes {
            pack.note(note);
        }
        for (path, contents) in files {
            pack.write(&format!("{LEVELS}/{path}"), &contents)?;
        }
        pack.write(
            &format!("{LEVELS}/{}.level", self.file),
            level.to_toml_string().as_bytes(),
        )
    }
}

/// The entrance the characters come out of at the arrival `name`:
/// level-start.md has them thrown out of the room-return entrance coming
/// back from the room, where the level has one.
fn return_entrance(name: &str, entities: &[Entity]) -> Option<String> {
    (name == ROOM_RETURN
        && entities
            .iter()
            .any(|e| e.id.as_deref() == Some(ROOM_ENTRANCE)))
    .then(|| ROOM_ENTRANCE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fault in converting a song skips the song, not the import.
    #[test]
    fn a_converter_that_panics_skips_its_part() {
        let converted: Result<(), String> = crate::optional(|| panic!("a fault"));
        assert_eq!(
            converted,
            Err("the converter stopped unexpectedly".to_string())
        );
        assert_eq!(crate::optional(|| Ok(3)), Ok(3));
    }

    /// level-start.md, "Coming back from the room": out of the
    /// room-return entrance, where there is one, and only from the room.
    #[test]
    fn coming_back_from_the_room_is_out_of_the_room_return_entrance() {
        let entity = |id: Option<&str>| Entity {
            kind: crate::kinds::ENTRANCE.into(),
            id: id.map(Into::into),
            at: Vec2::ints(100, 300),
            params: Default::default(),
        };
        let both = [entity(None), entity(Some(ROOM_ENTRANCE))];
        assert_eq!(
            return_entrance(ROOM_RETURN, &both).as_deref(),
            Some(ROOM_ENTRANCE)
        );
        assert_eq!(return_entrance(SCENE_RETURN, &both), None);
        assert_eq!(return_entrance(ROOM_RETURN, &both[..1]), None);
    }

    /// wall-exits.md: the first wall exit is the one further left, the
    /// second the one further right.
    #[test]
    fn the_wall_exits_lead_back_to_their_returns() {
        let entity = |kind: &str, x| Entity {
            kind: kind.into(),
            id: None,
            at: Vec2::ints(x, 300),
            params: Default::default(),
        };
        let entities = [
            entity(WALL_EXIT, 900),
            entity(EXIT, 100),
            entity(WALL_EXIT, 400),
        ];
        let leads = |first, second| {
            let mut entities = entities.clone();
            lead_wall_exits(&mut entities, first, second);
            assert!(entities.iter().all(|e| !e.params.contains_key("area")));
            entities
                .iter()
                .map(|e| {
                    e.params
                        .get("arrival")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .collect::<Vec<_>>()
        };
        let (one, two) = (
            Some(WALL_RETURN_ARRIVAL.to_owned()),
            Some(SECOND_WALL_RETURN_ARRIVAL.to_owned()),
        );
        assert_eq!(leads(true, false), [None, None, one.clone()]);
        assert_eq!(leads(false, true), [two.clone(), None, None]);
        assert_eq!(leads(true, true), [two, None, one]);
    }

    #[test]
    fn the_lowest_exit_leads_to_the_scene_and_the_others_to_the_room() {
        let entity = |kind: &str, y| Entity {
            kind: kind.into(),
            id: None,
            at: Vec2::ints(100, y),
            params: Default::default(),
        };
        let mut entities = [entity(EXIT, 300), entity(KEG, 450), entity(EXIT, 400)];
        lead_exits(&mut entities, true, true, Leads::default());
        fn leads(e: &Entity) -> Option<&str> {
            e.params.get("arrival").and_then(|v| v.as_str())
        }
        assert_eq!(leads(&entities[0]), Some(ROOM_RETURN));
        assert_eq!(leads(&entities[1]), None);
        assert_eq!(leads(&entities[2]), Some(SCENE_RETURN));
        let mut entities = [entity(EXIT, 300), entity(EXIT, 400)];
        lead_exits(&mut entities, false, true, Leads::default());
        assert_eq!(leads(&entities[0]), Some(ROOM_RETURN));
        assert_eq!(leads(&entities[1]), None);
    }

    #[test]
    fn the_exits_lead_into_the_room_and_the_scene_and_the_room_s_end_markers_back() {
        let entity = |kind: &str, y| Entity {
            kind: kind.into(),
            id: None,
            at: Vec2::ints(100, y),
            params: Default::default(),
        };
        let param = |e: &Entity, key| e.params.get(key).and_then(|v| v.as_str()).map(String::from);
        let mut level = [entity(EXIT, 300), entity(EXIT, 400)];
        let into = Leads {
            scene: Some("scene"),
            room: Some("room"),
        };
        lead_exits(&mut level, true, true, into);
        assert_eq!(param(&level[0], "area").as_deref(), Some("room"));
        assert_eq!(param(&level[0], "arrival").as_deref(), Some(FROM_LEVEL));
        assert_eq!(param(&level[1], "area").as_deref(), Some("scene"));
        assert_eq!(param(&level[1], "arrival").as_deref(), Some(FROM_LEVEL));
        let mut room = [entity(END, 400), entity(KEG, 400)];
        lead_back(&mut room, "level");
        assert_eq!(param(&room[0], "area").as_deref(), Some("level"));
        assert_eq!(param(&room[0], "arrival").as_deref(), Some(ROOM_RETURN));
        assert!(room[1].params.is_empty());
    }

    #[test]
    fn the_restart_record_moves_the_kegs_restart_point() {
        let entity = |kind: &str, x, y| Entity {
            kind: kind.into(),
            id: None,
            at: Vec2::ints(x, y),
            params: Default::default(),
        };
        let mut entities = [entity(KEG, 1000, 400), entity("crate", 900, 400)];
        restart_at(&mut entities, (1040, 370));
        let restart = entities[0].params["restart"].as_array().unwrap();
        let restart: Vec<i64> = restart.iter().filter_map(|v| v.as_integer()).collect();
        assert_eq!(restart, [40, -30]);
        assert!(entities[1].params.is_empty());

        // The restart point is then where the simulation resumes.
        let keg = clamber_sim::Archetypes::builtin()
            .resolve(&entities[0], "test", "keg")
            .unwrap()
            .unwrap();
        let clamber_sim::Behavior::Keg(keg) = keg.behavior else {
            panic!("not a keg");
        };
        assert_eq!(keg.restart, Vec2::ints(40, -30));
    }
}
