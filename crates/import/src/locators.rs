//! Where each kind of data starts in a supported image
//! (`spec/formats/locators.md`), and the parts of the levels they give.

use clamber_level::Dir;

use crate::{
    CollectibleParts, FIRST_LEVEL, FIRST_ROOM, FIRST_SCENE, LevelParts, LevelSong, Location,
    PackedGround, Scenery, SkyParts,
};

/// The data a locator row finds. The level parts are the first level's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Data {
    SampleTable,
    SharedSampleList,
    SampleSetTable,
    SongTable,
    EffectBlock,
    Grid,
    TypeTable,
    PackedTiles,
    PaletteSet,
    PictureTable,
    BackdropColours,
    BackdropNight,
    HorizonColours,
    HorizonBlock,
    FrameTable,
    FirstLeadingPalette,
    FirstFollowingPalette,
    SecondLeadingPalette,
    SecondFollowingPalette,
    SharedPalette,
    ThrownPalette,
    CratePalette,
    AnimalPalette,
    SmallWalkerPalette,
    MediumWalkerPalette,
    LargeWalkerPalette,
    ThrowerPalette,
    CritterPalette,
    StingerPalette,
    MapPalette,
    PlacementList,
    CollectibleGroups,
    CollectibleShapes,
    StartRecord,
    RestartRecord,
    SceneReturnRecord,
    RoomReturnRecord,
    WallReturnRecord,
    SecondWallReturnRecord,
    ViewLimits,
    RoomGrid,
    RoomTypeTable,
    RoomTiles,
    RoomTilesRest,
    RoomScreenMap,
    RoomPaletteSet,
    RoomPlacementList,
    RoomStartRecord,
    SceneGrid,
    SceneTypeTable,
    ScenePackedTiles,
    ScenePaletteSet,
    ScenePictureTable,
    SceneFrontTiles,
    SceneFrontMap,
    ScenePlacementList,
    SceneStartRecord,
    SceneViewLimits,
    WallAreaPlacementList,
    WallAreaStartRecord,
    WallAreaViewLimits,
    SecondWallAreaPlacementList,
    SecondWallAreaStartRecord,
    SecondWallAreaViewLimits,
    MapPoints,
    MapCorners,
    MapPageTileMaps,
    MapPageTileSets,
    MapPagePaletteSets,
    MapFirstPalette,
    PackedMap,
}

impl Data {
    pub const ALL: [Data; 71] = [
        Data::SampleTable,
        Data::SharedSampleList,
        Data::SampleSetTable,
        Data::SongTable,
        Data::EffectBlock,
        Data::Grid,
        Data::TypeTable,
        Data::PackedTiles,
        Data::PaletteSet,
        Data::PictureTable,
        Data::BackdropColours,
        Data::BackdropNight,
        Data::HorizonColours,
        Data::HorizonBlock,
        Data::FrameTable,
        Data::FirstLeadingPalette,
        Data::FirstFollowingPalette,
        Data::SecondLeadingPalette,
        Data::SecondFollowingPalette,
        Data::SharedPalette,
        Data::ThrownPalette,
        Data::CratePalette,
        Data::AnimalPalette,
        Data::SmallWalkerPalette,
        Data::MediumWalkerPalette,
        Data::LargeWalkerPalette,
        Data::ThrowerPalette,
        Data::CritterPalette,
        Data::StingerPalette,
        Data::MapPalette,
        Data::PlacementList,
        Data::CollectibleGroups,
        Data::CollectibleShapes,
        Data::StartRecord,
        Data::RestartRecord,
        Data::SceneReturnRecord,
        Data::RoomReturnRecord,
        Data::WallReturnRecord,
        Data::SecondWallReturnRecord,
        Data::ViewLimits,
        Data::RoomGrid,
        Data::RoomTypeTable,
        Data::RoomTiles,
        Data::RoomTilesRest,
        Data::RoomScreenMap,
        Data::RoomPaletteSet,
        Data::RoomPlacementList,
        Data::RoomStartRecord,
        Data::SceneGrid,
        Data::SceneTypeTable,
        Data::ScenePackedTiles,
        Data::ScenePaletteSet,
        Data::ScenePictureTable,
        Data::SceneFrontTiles,
        Data::SceneFrontMap,
        Data::ScenePlacementList,
        Data::SceneStartRecord,
        Data::SceneViewLimits,
        Data::WallAreaPlacementList,
        Data::WallAreaStartRecord,
        Data::WallAreaViewLimits,
        Data::SecondWallAreaPlacementList,
        Data::SecondWallAreaStartRecord,
        Data::SecondWallAreaViewLimits,
        Data::MapPoints,
        Data::MapCorners,
        Data::MapPageTileMaps,
        Data::MapPageTileSets,
        Data::MapPagePaletteSets,
        Data::MapFirstPalette,
        Data::PackedMap,
    ];

    /// The row's name in the spec's table.
    pub fn spec_name(self) -> &'static str {
        match self {
            Data::SampleTable => "Sound: sample table",
            Data::SharedSampleList => "Sound: shared sample list",
            Data::SampleSetTable => "Sound: sample-set table",
            Data::SongTable => "Sound: song table",
            Data::EffectBlock => "Sound: sound-effect block",
            Data::Grid => "Level ground: the first level's grid",
            Data::TypeTable => "Level ground: the first level's type table",
            Data::PackedTiles => "Level graphics: the first level's packed tile data",
            Data::PaletteSet => "Level graphics: the first level's palette set",
            Data::PictureTable => "Level graphics: the first level's picture table",
            Data::BackdropColours => "Level graphics: the first level's backdrop row-colour list",
            Data::BackdropNight => "Level graphics: the first level's backdrop night colour table",
            Data::HorizonColours => "Level graphics: the first level's horizon row-colour list",
            Data::HorizonBlock => "Level graphics: the first level's horizon block",
            Data::FrameTable => "Sprite frames: the frame table",
            Data::FirstLeadingPalette => "Sprite palettes: the first character's leading palette",
            Data::FirstFollowingPalette => {
                "Sprite palettes: the first character's following palette"
            }
            Data::SecondLeadingPalette => "Sprite palettes: the second character's leading palette",
            Data::SecondFollowingPalette => {
                "Sprite palettes: the second character's following palette"
            }
            Data::SharedPalette => {
                "Sprite palettes: the shared palette (letters, collectibles, bunches, flashes, smoke)"
            }
            Data::ThrownPalette => "Sprite palettes: the thrown objects' palette",
            Data::CratePalette => "Sprite palettes: the crate's palette",
            Data::AnimalPalette => "Sprite palettes: the animal's palette",
            Data::SmallWalkerPalette => "Sprite palettes: the small walkers' palette",
            Data::MediumWalkerPalette => "Sprite palettes: the medium walkers' palette",
            Data::LargeWalkerPalette => "Sprite palettes: the large walkers' palette",
            Data::ThrowerPalette => "Sprite palettes: the thrower's palette",
            Data::CritterPalette => "Sprite palettes: the critters' palette",
            Data::StingerPalette => "Sprite palettes: the stinger's palette",
            Data::MapPalette => "Sprite palettes: the level map's palette",
            Data::PlacementList => "Placements: the first level's placement list",
            Data::CollectibleGroups => "Single collectibles: the first level's group list",
            Data::CollectibleShapes => "Single collectibles: shape table",
            Data::StartRecord => "Level start: the first level's start record",
            Data::RestartRecord => "Level start: the first level's restart record",
            Data::SceneReturnRecord => "Level start: the first level's scene-return record",
            Data::RoomReturnRecord => "Level start: the first level's room-return record",
            Data::WallReturnRecord => "Level start: the first level's wall-return record",
            Data::SecondWallReturnRecord => {
                "Level start: the first level's second wall-return record"
            }
            Data::ViewLimits => "View limits: the first level's record",
            Data::RoomGrid => "Level ground: the first level's room's grid",
            Data::RoomTypeTable => "Level ground: the first level's room's type table",
            Data::RoomTiles => "Level graphics: the first level's room's tile set, tiles 0 to 986",
            Data::RoomTilesRest => {
                "Level graphics: the first level's room's tile set, tiles 987 to 1 023"
            }
            Data::RoomScreenMap => "Level graphics: the first level's room's screen map",
            Data::RoomPaletteSet => "Level graphics: the first level's room's palette set",
            Data::RoomPlacementList => "Placements: the first level's room's placement list",
            Data::RoomStartRecord => "Level start: the first level's room's start record",
            Data::SceneGrid => "Level ground: the first level's scene's grid",
            Data::SceneTypeTable => "Level ground: the first level's scene's type table",
            Data::ScenePackedTiles => "Level graphics: the first level's scene's packed tile data",
            Data::ScenePaletteSet => "Level graphics: the first level's scene's palette set",
            Data::ScenePictureTable => "Level graphics: the first level's scene's picture table",
            Data::SceneFrontTiles => "Level graphics: the first level's scene's front tile set",
            Data::SceneFrontMap => "Level graphics: the first level's scene's front map",
            Data::ScenePlacementList => "Placements: the first level's scene's placement list",
            Data::SceneStartRecord => "Level start: the first level's scene's start record",
            Data::SceneViewLimits => "View limits: the first level's scene's record",
            Data::WallAreaPlacementList => {
                "Placements: the placement list of the area behind the first level's first wall exit"
            }
            Data::WallAreaStartRecord => {
                "Level start: the start record of the area behind the first level's first wall exit"
            }
            Data::WallAreaViewLimits => {
                "View limits: the record of the area behind the first level's first wall exit"
            }
            Data::SecondWallAreaPlacementList => {
                "Placements: the placement list of the area behind the first level's second wall exit"
            }
            Data::SecondWallAreaStartRecord => {
                "Level start: the start record of the area behind the first level's second wall exit"
            }
            Data::SecondWallAreaViewLimits => {
                "View limits: the record of the area behind the first level's second wall exit"
            }
            Data::MapPoints => "Level map: point tables",
            Data::MapCorners => "Level map: corner records",
            Data::MapPageTileMaps => "Level map: the page picture tile-map table",
            Data::MapPageTileSets => "Level map: the page picture tile-set table",
            Data::MapPagePaletteSets => "Level map: the page picture palette-set table",
            Data::MapFirstPalette => "Level map: the first page's palette set",
            Data::PackedMap => "Level ground: the first level's packed map",
        }
    }
}

/// Where `data` starts in the image of `title`, if the spec says.
pub fn locate(title: &str, data: Data) -> Option<Location> {
    let (bank, address) = match (title, data) {
        ("title-1", Data::SampleTable) => (0xC9, 0xFF00),
        ("title-1", Data::SharedSampleList) => (0xCA, 0xB24A),
        ("title-1", Data::SampleSetTable) => (0xCA, 0xB212),
        ("title-1", Data::SongTable) => (0xCA, 0xB15E),
        ("title-1", Data::EffectBlock) => (0xC9, 0x2D95),
        ("title-1", Data::Grid) => (0xD9, 0x0000),
        ("title-1", Data::TypeTable) => (0xD9, 0xED00),
        ("title-1", Data::PackedTiles) => (0xD5, 0x8FC0),
        ("title-1", Data::PaletteSet) => (0xF9, 0xA1DC),
        ("title-1", Data::PictureTable) => (0xD9, 0xA3C0),
        ("title-1", Data::BackdropColours) => (0xF9, 0x96E6),
        ("title-1", Data::BackdropNight) => (0xF9, 0x9658),
        ("title-1", Data::HorizonColours) => (0xF9, 0x98EA),
        ("title-1", Data::HorizonBlock) => (0xCD, 0xF337),
        ("title-1", Data::FrameTable) => (0xFB, 0xCC9C),
        ("title-1", Data::FirstLeadingPalette) => (0xFC, 0x849A),
        ("title-1", Data::FirstFollowingPalette) => (0xFC, 0x84B8),
        ("title-1", Data::SecondLeadingPalette) => (0xFC, 0x8422),
        ("title-1", Data::SecondFollowingPalette) => (0xFC, 0x8440),
        ("title-1", Data::SharedPalette) => (0xFC, 0x86F2),
        ("title-1", Data::ThrownPalette) => (0xFC, 0x872E),
        ("title-1", Data::CratePalette) => (0xFC, 0x8710),
        ("title-1", Data::AnimalPalette) => (0xFC, 0x8404),
        ("title-1", Data::SmallWalkerPalette) => (0xFC, 0x8A1C),
        ("title-1", Data::MediumWalkerPalette) => (0xFC, 0x8512),
        ("title-1", Data::LargeWalkerPalette) => (0xFC, 0x85C6),
        ("title-1", Data::ThrowerPalette) => (0xFC, 0x86D4),
        ("title-1", Data::CritterPalette) => (0xFC, 0x894A),
        ("title-1", Data::StingerPalette) => (0xFC, 0x8DBE),
        ("title-1", Data::MapPalette) => (0xF9, 0xBFC5),
        ("title-1", Data::PlacementList) => (0xFD, 0x95DC),
        ("title-1", Data::CollectibleGroups) => (0xF8, 0xC2ED),
        ("title-1", Data::CollectibleShapes) => (0xF8, 0xD75D),
        ("title-1", Data::StartRecord) => (0xFC, 0xBF78),
        ("title-1", Data::RestartRecord) => (0xFC, 0xBFB4),
        ("title-1", Data::SceneReturnRecord) => (0xFC, 0xC09C),
        ("title-1", Data::RoomReturnRecord) => (0xFC, 0xBF58),
        ("title-1", Data::WallReturnRecord) => (0xFC, 0xBF40),
        ("title-1", Data::SecondWallReturnRecord) => (0xFC, 0xC0D8),
        ("title-1", Data::ViewLimits) => (0xFC, 0x957A),
        ("title-1", Data::RoomGrid) => (0xE3, 0x0000),
        ("title-1", Data::RoomTypeTable) => (0xD6, 0x0000),
        ("title-1", Data::RoomTiles) => (0xCF, 0x84A0),
        ("title-1", Data::RoomTilesRest) => (0xCF, 0x0000),
        ("title-1", Data::RoomScreenMap) => (0xCF, 0x7DA0),
        ("title-1", Data::RoomPaletteSet) => (0xF9, 0xD683),
        ("title-1", Data::RoomPlacementList) => (0xFD, 0xD3C0),
        ("title-1", Data::RoomStartRecord) => (0xFC, 0xC090),
        ("title-1", Data::SceneGrid) => (0xDA, 0x0000),
        ("title-1", Data::SceneTypeTable) => (0xEC, 0x15F4),
        ("title-1", Data::ScenePackedTiles) => (0xDC, 0x0000),
        ("title-1", Data::ScenePaletteSet) => (0xF9, 0xAF65),
        ("title-1", Data::ScenePictureTable) => (0xDA, 0xBF00),
        ("title-1", Data::SceneFrontTiles) => (0xE7, 0xFCE9),
        ("title-1", Data::SceneFrontMap) => (0xE7, 0xDCE9),
        ("title-1", Data::ScenePlacementList) => (0xFD, 0xD068),
        ("title-1", Data::SceneStartRecord) => (0xFC, 0xC03C),
        ("title-1", Data::SceneViewLimits) => (0xFC, 0xA5F2),
        ("title-1", Data::WallAreaPlacementList) => (0xFD, 0x84D4),
        ("title-1", Data::WallAreaStartRecord) => (0xFC, 0xBF38),
        ("title-1", Data::WallAreaViewLimits) => (0xFC, 0x919E),
        ("title-1", Data::SecondWallAreaPlacementList) => (0xFD, 0xA03C),
        ("title-1", Data::SecondWallAreaStartRecord) => (0xFC, 0xBF88),
        ("title-1", Data::SecondWallAreaViewLimits) => (0xFC, 0x97EC),
        ("title-1", Data::MapPoints) => (0xFC, 0xF47B),
        ("title-1", Data::MapCorners) => (0xC0, 0xD13C),
        ("title-1", Data::MapPageTileMaps) => (0xC0, 0xE170),
        ("title-1", Data::MapPageTileSets) => (0xC0, 0xE13C),
        ("title-1", Data::MapPagePaletteSets) => (0xC0, 0xE1A4),
        ("title-1", Data::MapFirstPalette) => (0xF9, 0xBE03),
        ("title-2", Data::PackedMap) => (0xE3, 0xE006),
        ("title-2", Data::TypeTable) => (0xFC, 0x6154),
        _ => return None,
    };
    Some(Location::new(bank, address))
}

/// The parts of the first level of `title`, where the spec gives all of
/// them: the locations from [`locate`], and the sizes from
/// `level-ground.md` and `graphics.md` ("The first level's data"). Its
/// exits lead into its room and its scene.
pub fn first_level(title: &str) -> Option<LevelParts> {
    let at = |data| locate(title, data);
    let (columns, types, pictures) = match title {
        // The grid's first 184 columns, `N` = 234, and types 0 to 512.
        "title-1" => (184, 234, 513),
        _ => return None,
    };
    Some(LevelParts {
        grid: at(Data::Grid)?,
        columns,
        type_table: at(Data::TypeTable)?,
        types,
        scenery: Scenery::Grid {
            tiles: at(Data::PackedTiles)?,
            palettes: at(Data::PaletteSet)?,
            picture_table: at(Data::PictureTable)?,
            pictures,
            sky: Some(SkyParts {
                backdrop: at(Data::BackdropColours)?,
                night: at(Data::BackdropNight)?,
                night_entries: 71,
                horizon: at(Data::HorizonColours)?,
                block: at(Data::HorizonBlock)?,
            }),
        },
        placements: Some(at(Data::PlacementList)?),
        collectibles: Some(CollectibleParts {
            groups: at(Data::CollectibleGroups)?,
            shapes: at(Data::CollectibleShapes)?,
        }),
        start: Some(at(Data::StartRecord)?),
        walk_in: None,
        restart: Some(at(Data::RestartRecord)?),
        scene_return: Some(at(Data::SceneReturnRecord)?),
        room_return: Some(at(Data::RoomReturnRecord)?),
        wall_return: Some(at(Data::WallReturnRecord)?),
        second_wall_return: Some(at(Data::SecondWallReturnRecord)?),
        room: Some(FIRST_ROOM),
        back_to: None,
        scene: Some(FIRST_SCENE),
        scene_of: None,
        view_limits: Some(at(Data::ViewLimits)?),
        // `sound.md`: "The first level plays song 0".
        song: Some(LevelSong {
            sample_table: at(Data::SampleTable)?,
            shared_samples: at(Data::SharedSampleList)?,
            sample_sets: at(Data::SampleSetTable)?,
            song_table: at(Data::SongTable)?,
            song: 0,
        }),
        // `extra-lives.md`, "Late items": of its two `extra-life-3`
        // items, the right one.
        late_life_item: true,
    })
}

/// The parts of the room the first level's upper exit leads to, where
/// the spec gives all of them: the locations from [`locate`], the grid's
/// 16 columns and `N` = 17 from `level-ground.md`, and the tile set's
/// first 987 tiles in one part from `graphics.md` ("The first level's
/// room"). The characters walk in leftward (`exits.md`, "Walking in"),
/// and it has no restart, view-limit record or music.
pub fn first_room(title: &str) -> Option<LevelParts> {
    let at = |data| locate(title, data);
    let (columns, types, split) = match title {
        "title-1" => (16, 17, 987),
        _ => return None,
    };
    Some(LevelParts {
        grid: at(Data::RoomGrid)?,
        columns,
        type_table: at(Data::RoomTypeTable)?,
        types,
        scenery: Scenery::Screen {
            tiles: at(Data::RoomTiles)?,
            split,
            rest: at(Data::RoomTilesRest)?,
            map: at(Data::RoomScreenMap)?,
            palettes: at(Data::RoomPaletteSet)?,
        },
        placements: Some(at(Data::RoomPlacementList)?),
        // "The first level's room [...] ha[s] no single collectibles".
        collectibles: None,
        start: Some(at(Data::RoomStartRecord)?),
        walk_in: Some(Dir::Left),
        restart: None,
        scene_return: None,
        room_return: None,
        wall_return: None,
        second_wall_return: None,
        room: None,
        back_to: Some(FIRST_LEVEL),
        scene: None,
        scene_of: None,
        view_limits: None,
        song: None,
        late_life_item: false,
    })
}

/// The parts of the scene the first level's floor exit leads to, where
/// the spec gives all of them: the locations from [`locate`], the grid's
/// 1 528 columns and `N` = 154 from `level-ground.md`, and types 0 to
/// 435 and front tiles 0 to 43 from `graphics.md` ("The first level's
/// scene"). The lead walks in leftward, and the scene plays by itself
/// and goes back to the first level (`exits.md`, "The scene"). It has no
/// single collectibles (`collectibles.md`), restart or music.
pub fn first_scene(title: &str) -> Option<LevelParts> {
    let at = |data| locate(title, data);
    let (columns, types, pictures, front_count) = match title {
        "title-1" => (1528, 154, 436, 44),
        _ => return None,
    };
    Some(LevelParts {
        grid: at(Data::SceneGrid)?,
        columns,
        type_table: at(Data::SceneTypeTable)?,
        types,
        scenery: Scenery::Scene {
            tiles: at(Data::ScenePackedTiles)?,
            palettes: at(Data::ScenePaletteSet)?,
            picture_table: at(Data::ScenePictureTable)?,
            pictures,
            front_tiles: at(Data::SceneFrontTiles)?,
            front_count,
            front_map: at(Data::SceneFrontMap)?,
        },
        placements: Some(at(Data::ScenePlacementList)?),
        collectibles: None,
        start: Some(at(Data::SceneStartRecord)?),
        walk_in: Some(Dir::Left),
        restart: None,
        scene_return: None,
        room_return: None,
        wall_return: None,
        second_wall_return: None,
        room: None,
        back_to: None,
        scene: None,
        scene_of: Some(FIRST_LEVEL),
        view_limits: Some(at(Data::SceneViewLimits)?),
        song: None,
        late_life_item: false,
    })
}

/// Where the first level's ground of `title` is, for a title whose
/// levels' ground is in packed maps: `level-ground.md`, "The first
/// level's data (`title-2`)".
pub fn first_level_ground(title: &str) -> Option<PackedGround> {
    let (first_column, columns, types) = match title {
        // Map columns 1 to 223, `N` = 170.
        "title-2" => (1, 223, 170),
        _ => return None,
    };
    Some(PackedGround {
        map: locate(title, Data::PackedMap)?,
        first_column,
        columns,
        type_table: locate(title, Data::TypeTable)?,
        types,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_title_has_the_rows_in_the_spec() {
        let path = format!(
            "{}/../../spec/formats/locators.md",
            env!("CARGO_MANIFEST_DIR")
        );
        let spec = std::fs::read_to_string(&path).unwrap();
        for title in ["title-1", "title-2"] {
            let (_, section) = spec.split_once(&format!("## `{title}`")).unwrap();
            let section = section.split("\n## ").next().unwrap();
            let table = section.split("### ").next().unwrap();
            let rows: Vec<(String, Location)> = table
                .lines()
                .filter_map(|l| {
                    let cells: Vec<&str> =
                        l.split('|').map(|c| c.trim().trim_matches('`')).collect();
                    let bank = u8::from_str_radix(cells.get(3)?, 16).ok()?;
                    let address = u16::from_str_radix(cells.get(4)?, 16).ok()?;
                    Some((cells[1].to_string(), Location::new(bank, address)))
                })
                .collect();
            let located = Data::ALL
                .into_iter()
                .filter(|&d| locate(title, d).is_some());
            assert_eq!(rows.len(), located.count(), "{title}");
            for (name, location) in rows {
                let data = Data::ALL
                    .into_iter()
                    .find(|d| d.spec_name() == name)
                    .unwrap_or_else(|| panic!("no data for the row {name:?}"));
                assert_eq!(locate(title, data), Some(location), "{title}: {name}");
            }
        }
        assert_eq!(locate("title-2", Data::Grid), None);
        assert_eq!(locate("title-3", Data::TypeTable), None);
    }

    #[test]
    fn title_2_s_first_level_ground_is_in_a_packed_map() {
        let ground = first_level_ground("title-2").unwrap();
        assert_eq!(ground.map, Location::new(0xE3, 0xE006));
        assert_eq!(ground.type_table, Location::new(0xFC, 0x6154));
        assert_eq!(
            (ground.first_column, ground.columns, ground.types),
            (1, 223, 170)
        );
        assert!(first_level_ground("title-1").is_none());
        // 4 bytes a type.
        assert!(usize::from(ground.type_table.address) + 4 * ground.types <= 0x1_0000);
    }

    #[test]
    fn the_first_level_is_found_for_title_1_only() {
        let parts = first_level("title-1").unwrap();
        assert_eq!(parts.grid, Location::new(0xD9, 0));
        assert_eq!(parts.placements, Some(Location::new(0xFD, 0x95DC)));
        assert_eq!(parts.start, Some(Location::new(0xFC, 0xBF78)));
        assert_eq!(parts.restart, Some(Location::new(0xFC, 0xBFB4)));
        assert_eq!(
            parts.song.map(|s| (s.song_table, s.song)),
            Some((Location::new(0xCA, 0xB15E), 0))
        );
        assert!(first_level("title-2").is_none());
    }

    #[test]
    fn the_builtin_importer_converts_title_1_s_first_level() {
        let importer = crate::Importer::builtin();
        let title_1 = &importer.catalog.known[0];
        assert_eq!(title_1.title, "title-1");
        let names: Vec<&str> = importer
            .decoders
            .iter()
            .filter(|d| d.applies_to(title_1))
            .map(|d| d.name())
            .collect();
        assert_eq!(
            names,
            [
                "First level",
                "First level's room",
                "First level's scene",
                "Characters' pictures",
                "Objects' pictures",
                "Level map"
            ]
        );
    }

    #[test]
    fn the_first_level_s_parts_fit_their_banks() {
        let parts = first_level("title-1").unwrap();
        let end = |at: Location, len: usize| usize::from(at.address) + len;
        // 32 bytes a column, 2 a type, 32 a picture.
        let Scenery::Grid {
            picture_table,
            pictures,
            ..
        } = parts.scenery
        else {
            panic!("the first level's layers are made from its grid");
        };
        assert!(end(parts.grid, 32 * parts.columns) <= usize::from(picture_table.address));
        assert!(end(picture_table, 32 * pictures) <= 0x1_0000);
        assert!(end(parts.type_table, 2 * parts.types) <= 0x1_0000);
    }

    #[test]
    fn the_first_level_s_room_is_found_for_title_1_only() {
        let parts = first_room("title-1").unwrap();
        assert_eq!(parts.grid, Location::new(0xE3, 0));
        assert_eq!((parts.columns, parts.types), (16, 17));
        assert_eq!(parts.start, Some(Location::new(0xFC, 0xC090)));
        assert_eq!(parts.walk_in, Some(Dir::Left));
        assert_eq!(parts.view_limits, None);
        let Scenery::Screen {
            tiles,
            split,
            rest,
            map,
            ..
        } = parts.scenery
        else {
            panic!("the room has a screen picture");
        };
        // graphics.md: 31 584 and 1 184 bytes, 32 a tile.
        assert_eq!(32 * split, 31_584);
        assert_eq!(32 * (1024 - split), 1_184);
        let end = |at: Location, len: usize| usize::from(at.address) + len;
        assert!(end(tiles, 32 * split) <= 0x1_0000);
        assert!(end(rest, 32 * (1024 - split)) <= usize::from(map.address));
        assert!(first_room("title-2").is_none());
    }

    #[test]
    fn the_first_level_s_scene_is_found_for_title_1_only() {
        let parts = first_scene("title-1").unwrap();
        assert_eq!(parts.grid, Location::new(0xDA, 0));
        assert_eq!((parts.columns, parts.types), (1528, 154));
        assert_eq!(parts.start, Some(Location::new(0xFC, 0xC03C)));
        assert_eq!(parts.view_limits, Some(Location::new(0xFC, 0xA5F2)));
        assert_eq!(parts.walk_in, Some(Dir::Left));
        let Scenery::Scene {
            picture_table,
            pictures,
            front_tiles,
            front_count,
            front_map,
            ..
        } = parts.scenery
        else {
            panic!("the scene has a front layer");
        };
        let end = |at: Location, len: usize| usize::from(at.address) + len;
        // 32 bytes a column and a picture, 16 a two-plane tile.
        assert_eq!(end(parts.grid, 32 * parts.columns), 0xBF00);
        assert!(end(parts.grid, 32 * parts.columns) <= usize::from(picture_table.address));
        assert!(end(picture_table, 32 * pictures) <= 0x1_0000);
        assert!(end(front_map, 2048) <= usize::from(front_tiles.address));
        assert!(end(front_tiles, 16 * front_count) <= 0x1_0000);
        assert!(end(parts.type_table, 2 * parts.types) <= 0x1_0000);
        assert!(first_scene("title-2").is_none());
    }
}
