//! Where each kind of data starts in a supported image
//! (`spec/formats/locators.md`), and the parts of the levels they give.

use crate::{LevelParts, LevelSong, Location, PackedGround};

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
    FrameTable,
    PlacementList,
    StartRecord,
    RestartRecord,
    MapPoints,
    MapCorners,
    PackedMap,
}

impl Data {
    pub const ALL: [Data; 17] = [
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
        Data::FrameTable,
        Data::PlacementList,
        Data::StartRecord,
        Data::RestartRecord,
        Data::MapPoints,
        Data::MapCorners,
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
            Data::FrameTable => "Sprite frames: the frame table",
            Data::PlacementList => "Placements: the first level's placement list",
            Data::StartRecord => "Level start: the first level's start record",
            Data::RestartRecord => "Level start: the first level's restart record",
            Data::MapPoints => "Level map: point tables",
            Data::MapCorners => "Level map: corner records",
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
        ("title-1", Data::FrameTable) => (0xFB, 0xCC9C),
        ("title-1", Data::PlacementList) => (0xFD, 0x95DC),
        ("title-1", Data::StartRecord) => (0xFC, 0xBF78),
        ("title-1", Data::RestartRecord) => (0xFC, 0xBFB4),
        ("title-1", Data::MapPoints) => (0xFC, 0xF47B),
        ("title-1", Data::MapCorners) => (0xC0, 0xD13C),
        ("title-2", Data::PackedMap) => (0xE3, 0xE006),
        ("title-2", Data::TypeTable) => (0xFC, 0x6154),
        _ => return None,
    };
    Some(Location::new(bank, address))
}

/// The parts of the first level of `title`, where the spec gives all of
/// them: the locations from [`locate`], and the sizes from
/// `level-ground.md` and `graphics.md` ("The first level's data").
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
        tiles: at(Data::PackedTiles)?,
        palettes: at(Data::PaletteSet)?,
        picture_table: at(Data::PictureTable)?,
        pictures,
        placements: Some(at(Data::PlacementList)?),
        // `sound.md`: "The first level plays song 0".
        song: Some(LevelSong {
            sample_table: at(Data::SampleTable)?,
            shared_samples: at(Data::SharedSampleList)?,
            sample_sets: at(Data::SampleSetTable)?,
            song_table: at(Data::SongTable)?,
            song: 0,
        }),
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
        assert_eq!(names, ["First level"]);
    }

    #[test]
    fn the_first_level_s_parts_fit_their_banks() {
        let parts = first_level("title-1").unwrap();
        let end = |at: Location, len: usize| usize::from(at.address) + len;
        // 32 bytes a column, 2 a type, 32 a picture.
        assert!(end(parts.grid, 32 * parts.columns) <= usize::from(parts.picture_table.address));
        assert!(end(parts.picture_table, 32 * parts.pictures) <= 0x1_0000);
        assert!(end(parts.type_table, 2 * parts.types) <= 0x1_0000);
    }
}
