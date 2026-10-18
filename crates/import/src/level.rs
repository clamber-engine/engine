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
    Area, Arrival, Collision, Dir, Entity, FORMAT, Fixed, Level, Rect, Region, Section, Track, Vec2,
};

use crate::{
    BANK_SIZE, Background, COLUMN_BANDS, Collectibles, Decoder, END, EXIT, GridPicture, Ground,
    Image, KEG, Known, LEVELS, Layout, LevelSong, Location, PackWriter, PaletteSet, Pictures,
    Placements, ScreenMap, StartRecord, TileSet, VIEW_HEIGHT, VIEW_WIDTH, ViewLimits, WALL_EXIT,
    WALL_RETURN, kind_rows, object_of, settings_rows, unpack, unpack_nibbles,
};

/// Lines in a band.
const BAND: i64 = 32;

/// Bytes in a column of a column grid.
const COLUMN_BYTES: usize = 2 * COLUMN_BANDS as usize;

/// Bytes in a palette set, and in a cell picture.
const PALETTE_SET_BYTES: usize = 256;
const PICTURE_BYTES: usize = 32;

/// Bytes in a tile stored as it is.
const TILE_BYTES: usize = 32;

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
    /// The file name, without `.level`, of the area the upper exit leads
    /// to, if that's imported too: play comes into it at [`FROM_LEVEL`].
    /// Without it the exit leads straight to [`ROOM_RETURN`].
    pub room: Option<&'static str>,
    /// The file name, without `.level`, of the level that passing an end
    /// marker goes back into, at its [`ROOM_RETURN`] arrival, instead of
    /// finishing this one, as in the first level's room
    /// (`level-end.md`, "In the first level's room").
    pub back_to: Option<&'static str>,
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
/// The upper exit leads into the room at [`FROM_LEVEL`] if the room is
/// imported as `into_room`. The scene isn't imported yet, so the floor
/// exit leads straight to where play comes back after it, and so does the
/// upper one without the room.
fn lead_exits(entities: &mut [Entity], scene: bool, room: bool, into_room: Option<&str>) {
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
        let leads = match (Some(i) == floor, into_room) {
            (true, _) => scene.then_some((None, SCENE_RETURN)),
            (false, Some(area)) => Some((Some(area), FROM_LEVEL)),
            (false, None) => room.then_some((None, ROOM_RETURN)),
        };
        if let Some((area, arrival)) = leads {
            lead(exit, area, arrival);
        }
    }
}

/// Has the first wall exit in `entities`, the one furthest left, lead to
/// [`WALL_RETURN_ARRIVAL`]: wall-exits.md has play come back to the level
/// there after the area behind it. That area isn't imported, so the wall
/// exit leads straight back.
fn lead_wall_exit(entities: &mut [Entity]) {
    let first = entities
        .iter_mut()
        .filter(|e| e.kind == WALL_EXIT)
        .min_by_key(|e| e.at.x);
    if let Some(wall) = first {
        lead(wall, None, WALL_RETURN_ARRIVAL);
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
        let prefix = format!("{}-", self.file);
        let background = match p.scenery {
            Scenery::Grid {
                tiles,
                palettes,
                picture_table,
                pictures,
            } => {
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
                Background::new(&picture, &tiles, &palettes, &prefix)?
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

        let width = ground.width();
        let height = ground.bands() * BAND;
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
        if let Some(at) = p.view_limits {
            let bytes = image
                .at(at)
                .ok_or_else(|| "the level's view-limit record isn't in the image".to_string())?;
            let limits = ViewLimits::read(bytes)?;
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
                    entrance: None,
                });
            }
        }
        // wall-exits.md: thrown out of the wall-return entrance from the
        // record's point, where the level has both.
        if let Some(at) = p.wall_return
            && level
                .entities
                .iter()
                .any(|e| e.id.as_deref() == Some(WALL_RETURN))
        {
            let (x, y) = record("wall-return", at)?;
            let px = |n: i64| Fixed::from_int(i32::try_from(n).expect("a level fits in i32"));
            level.arrivals.push(Arrival {
                name: WALL_RETURN_ARRIVAL.into(),
                at: Vec2::new(px(x), px(y)),
                walk: None,
                entrance: Some(WALL_RETURN.into()),
            });
            lead_wall_exit(&mut level.entities);
        }
        lead_exits(
            &mut level.entities,
            p.scene_return.is_some(),
            p.room_return.is_some(),
            p.room,
        );
        if let Some(area) = p.back_to {
            lead_back(&mut level.entities, area);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// wall-exits.md: the first wall exit is the one further left.
    #[test]
    fn the_first_wall_exit_leads_back_to_the_wall_return() {
        let entity = |kind: &str, x| Entity {
            kind: kind.into(),
            id: None,
            at: Vec2::ints(x, 300),
            params: Default::default(),
        };
        let mut entities = [
            entity(WALL_EXIT, 900),
            entity(EXIT, 100),
            entity(WALL_EXIT, 400),
        ];
        lead_wall_exit(&mut entities);
        let leads: Vec<_> = entities
            .iter()
            .map(|e| e.params.get("arrival").and_then(|v| v.as_str()))
            .collect();
        assert_eq!(leads, [None, None, Some(WALL_RETURN_ARRIVAL)]);
        assert!(entities.iter().all(|e| !e.params.contains_key("area")));
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
        lead_exits(&mut entities, true, true, None);
        fn leads(e: &Entity) -> Option<&str> {
            e.params.get("arrival").and_then(|v| v.as_str())
        }
        assert_eq!(leads(&entities[0]), Some(ROOM_RETURN));
        assert_eq!(leads(&entities[1]), None);
        assert_eq!(leads(&entities[2]), Some(SCENE_RETURN));
        let mut entities = [entity(EXIT, 300), entity(EXIT, 400)];
        lead_exits(&mut entities, false, true, None);
        assert_eq!(leads(&entities[0]), Some(ROOM_RETURN));
        assert_eq!(leads(&entities[1]), None);
    }

    #[test]
    fn the_upper_exit_leads_into_the_room_and_the_room_s_end_markers_back() {
        let entity = |kind: &str, y| Entity {
            kind: kind.into(),
            id: None,
            at: Vec2::ints(100, y),
            params: Default::default(),
        };
        let param = |e: &Entity, key| e.params.get(key).and_then(|v| v.as_str()).map(String::from);
        let mut level = [entity(EXIT, 300), entity(EXIT, 400)];
        lead_exits(&mut level, true, true, Some("room"));
        assert_eq!(param(&level[0], "area").as_deref(), Some("room"));
        assert_eq!(param(&level[0], "arrival").as_deref(), Some(FROM_LEVEL));
        assert_eq!(param(&level[1], "area"), None);
        assert_eq!(param(&level[1], "arrival").as_deref(), Some(SCENE_RETURN));
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
