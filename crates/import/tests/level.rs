//! A made-up image with a level's ground and background in it, imported
//! with a [`LevelDecoder`], loaded from the pack and played. None of the
//! data is any real game's: it's made up from the format specs,
//! `spec/formats/level-ground.md`, `spec/formats/graphics.md`,
//! `spec/formats/placements.md`, `spec/formats/collectibles.md`,
//! `spec/formats/level-start.md`,
//! `spec/formats/view-limits.md`, `spec/formats/sound.md` and `spec/formats/songs.md`.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clamber_import::{
    BANK_SIZE, Below, Catalog, CollectibleParts, Ground, Image, ImportError, Importer, Known,
    Layout, LevelDecoder, LevelParts, LevelSong, Location, NIBBLE_HEADER, Optional, PackedGround,
    SECOND_WALL_RETURN_ARRIVAL, Scenery, import_report, installed, skipped_summary,
};
use clamber_input::{Action, Actions, Frame};
use clamber_level::{Fixed, Level, Vec2};
use clamber_sim::{InputFrame, World, generic_feel, step};

const COLUMNS: usize = 20;
const BANK: u8 = 0xC1;

/// A made-up level, column by column from the left:
///
/// - a floor on the bottom band all the way along, at line 481;
/// - a platform passable from below in band 13 of columns 6 and 7, at
///   line 433;
/// - a 1-in-4 slope in band 14 of column 12, up from line 480 to 473;
/// - and from column 13, a floor in band 14 at line 473.
fn grid() -> Vec<u8> {
    let mut words = vec![0u16; COLUMNS * 16];
    for column in 0..COLUMNS {
        let mut cell = |band: usize, word: u16| words[column * 16 + band] = word;
        cell(15, 1);
        match column {
            6 | 7 => cell(13, 2),
            12 => cell(14, 3),
            13.. => cell(14, 4),
            _ => {}
        }
    }
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Types 0 to 4: empty; flat at rise 31 (shape 61); flat at rise 15 and
/// passable from below (shape 62); a 1-in-4 slope rising 0 to 7 (shapes
/// 9 and 10); flat at rise 7 (shape 7).
fn types() -> Vec<u8> {
    const PASSABLE: u8 = 1 << 6;
    vec![0, 0, 61, 61, 62 | PASSABLE, 62 | PASSABLE, 9, 10, 7, 7]
}

/// Tile 0 blank, tile 1 all colour 15, packed as two fills.
fn packed_tiles() -> Vec<u8> {
    let mut packed = vec![0; 128];
    packed.extend([0x40 | 32, 0x00, 0x40 | 32, 0xFF, 0x00]);
    packed
}

/// Colour 15 of each palette row is a different made-up colour.
fn palettes() -> Vec<u8> {
    let mut words = [0u16; 128];
    for row in 0..8 {
        words[row * 16 + 15] = (row as u16 * 3) | (31 - row as u16 * 2) << 5 | 9 << 10;
    }
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Type 0's picture is tile 0; each other type's is tile 1 in the
/// palette row of its own number.
fn pictures() -> Vec<u8> {
    (0..5u16)
        .flat_map(|t| {
            let entry = if t == 0 { 0 } else { 1 | t << 10 };
            std::iter::repeat_n(entry, 16).flat_map(u16::to_le_bytes)
        })
        .collect()
}

/// Bytes of placement entries, from (class, x, h, reference) words.
fn entries(words: &[[u16; 4]]) -> Vec<u8> {
    words
        .iter()
        .flatten()
        .flat_map(|w| w.to_le_bytes())
        .collect()
}

/// A placement list: an object at x 200, 40 above the bottom edge, then
/// a group at x 300 whose members are listed at 0x7208, then the end.
fn placement_list() -> Vec<u8> {
    entries(&[[1, 200, 40, 0x7100], [5, 300, 0, 0x7200], [0; 4]])
}

/// The group's one member, an object at x 320, 64 above the bottom edge.
fn group_members() -> Vec<u8> {
    entries(&[[2, 320, 64, 0x7180], [0; 4]])
}

/// A group list of one group: shape 0 (bits 1 to 9 all 0) with its left edge at level x
/// 104 and its top at level y 456, then the end.
fn group_list() -> Vec<u8> {
    let group: u32 = (456 / 8) << 10 | (104 / 8) << 21;
    let mut bytes = group.to_le_bytes().to_vec();
    bytes.extend([0; 4]);
    bytes
}

/// A shape table whose shape 0, right after its one offset, is two
/// columns of one row, both holding an item.
fn shape_table() -> Vec<u8> {
    vec![2, 0, 2, 1, 0b11, 0]
}

/// A start record: level x 24, 31 above the bottom edge, which is on the
/// floor.
fn start_record() -> Vec<u8> {
    [24u16, 31].iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// A view-limit record: the picture's left edge from 16 to 300; the
/// view's top from 0 to 288 at the usual rates, and from x 400, from 100
/// to 200 at the quick ones.
fn view_limits() -> Vec<u8> {
    [16u16, 300, 0, 0, 288, 0, 400, 100, 200, 1, 0xFFFF]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
}

/// Where each part goes in the made-up image.
fn parts() -> LevelParts {
    LevelParts {
        grid: Location::new(BANK, 0x1000),
        columns: COLUMNS,
        type_table: Location::new(BANK, 0x3000),
        types: types().len() / 2,
        scenery: Scenery::Grid {
            tiles: Location::new(BANK, 0x4000),
            palettes: Location::new(BANK, 0x5000),
            picture_table: Location::new(BANK, 0x6000),
            pictures: 5,
        },
        placements: Some(Location::new(BANK, 0x7000)),
        collectibles: Some(CollectibleParts {
            groups: Location::new(BANK, 0x7600),
            shapes: Location::new(BANK, 0x7700),
        }),
        start: Some(Location::new(BANK, 0x7400)),
        walk_in: None,
        restart: None,
        scene_return: None,
        room_return: None,
        wall_return: None,
        second_wall_return: None,
        room: None,
        back_to: None,
        scene: None,
        scene_of: None,
        view_limits: Some(Location::new(BANK, 0x7500)),
        song: Some(LevelSong {
            sample_table: Location::new(BANK, 0x8000),
            shared_samples: Location::new(BANK, 0x8010),
            sample_sets: Location::new(BANK, 0x8020),
            song_table: Location::new(BANK, 0x8040),
            song: 0,
        }),
    }
}

/// Where the sample record and the song block go.
const SAMPLE: u16 = 0x8100;
const SONG: u16 = 0x8200;

/// The song's base address.
const SONG_BASE: u16 = 0x3000;

/// The sound tables: sample 0 at [`SAMPLE`], shared; sample set 0 empty,
/// its list at 0x8030; song 0 at [`SONG`].
fn sound_tables() -> Vec<(u16, Vec<u8>)> {
    let [lo, hi] = SAMPLE.to_le_bytes();
    let [song_lo, song_hi] = SONG.to_le_bytes();
    vec![
        (0x8000, vec![lo, hi, BANK]),
        (0x8010, vec![0, 0, 0xFF, 0xFF]),
        (0x8020, vec![0x30, 0x80]),
        (0x8030, vec![0xFF, 0xFF]),
        (0x8040, vec![song_lo, song_hi, BANK]),
    ]
}

/// A looping sample record of one block whose points are all 4 096.
fn sample_record() -> Vec<u8> {
    vec![
        0, 0, 9, 0, 0xC3, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
    ]
}

/// A song block: channel 0 sets itself up, plays a note of 4 beats,
/// then goes round a note and a rest of 2 beats each; the other
/// channels end at once.
fn song_block() -> Vec<u8> {
    let mut data: Vec<u8> = (0..8)
        .flat_map(|c| (SONG_BASE + if c == 0 { 19 } else { 18 }).to_le_bytes())
        .collect();
    data.extend([128, 0, 0]);
    data.extend([42, 100, 1, 0, 2, 127, 127, 16, 143, 224, 165, 4]);
    let back = SONG_BASE + data.len() as u16;
    data.extend([165, 2, 128, 2, 3]);
    data.extend(back.to_le_bytes());
    let mut block = (data.len() as u16).to_le_bytes().to_vec();
    block.extend(SONG_BASE.to_le_bytes());
    block.extend(data);
    block
}

/// Two banks of a repeating pattern, with the level's parts written in.
fn image() -> Image {
    image_with_song(song_block())
}

/// [`image`], with `song` as its song block.
fn image_with_song(song: Vec<u8>) -> Image {
    let mut bytes: Vec<u8> = (0..=250u8).cycle().take(2 * BANK_SIZE).collect();
    let p = parts();
    let Scenery::Grid {
        tiles,
        palettes: palette_set,
        picture_table,
        ..
    } = p.scenery
    else {
        unreachable!()
    };
    for (at, data) in [
        (p.grid, grid()),
        (p.type_table, types()),
        (tiles, packed_tiles()),
        (palette_set, palettes()),
        (picture_table, pictures()),
        (p.placements.unwrap(), placement_list()),
        (Location::new(BANK, 0x7208), group_members()),
        (p.collectibles.unwrap().groups, group_list()),
        (p.collectibles.unwrap().shapes, shape_table()),
        (p.start.unwrap(), start_record()),
        (p.view_limits.unwrap(), view_limits()),
        (Location::new(BANK, SAMPLE), sample_record()),
        (Location::new(BANK, SONG), song),
    ]
    .into_iter()
    .chain(
        sound_tables()
            .into_iter()
            .map(|(address, data)| (Location::new(BANK, address), data)),
    ) {
        let start = at.position(bytes.len()).unwrap();
        bytes[start..start + data.len()].copy_from_slice(&data);
    }
    Image::new(bytes)
}

fn importer() -> Importer {
    importer_for(&image())
}

/// An importer that knows `image` as `title-9` and has the level's
/// decoder.
fn importer_for(image: &Image) -> Importer {
    Importer {
        catalog: Catalog {
            known: vec![Known {
                title: "title-9".into(),
                release: "made-up release".into(),
                size: image.len() as u64,
                sha256: image.sha256(),
            }],
        },
        decoders: vec![Box::new(LevelDecoder {
            title: "title-9".into(),
            file: "first".into(),
            name: "made-up level".into(),
            parts: parts(),
        })],
    }
}

/// A fresh folder outside the source tree.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("clamber-level-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Imports the made-up image and loads its level back from the pack,
/// with the pack's levels folder.
fn imported_level(name: &str) -> (Level, PathBuf) {
    let packs = scratch(name).join("packs");
    let imported = importer().import(&image(), &packs, false).unwrap();
    assert_eq!(imported.decoded, ["made-up level"]);
    let (packs, problems) = installed(&packs);
    assert!(problems.is_empty(), "{problems:?}");
    let [pack] = &packs[..] else {
        panic!("one pack, not {}", packs.len());
    };
    let levels = pack.levels();
    assert_eq!(levels, [pack.dir.join("levels/first.level")]);
    (Level::load(&levels[0]).unwrap(), pack.dir.join("levels"))
}

#[test]
fn writes_the_level_and_its_images() {
    let (level, dir) = imported_level("writes");
    for file in ["first-far.png", "first-tiles.png", "first-music.wav"] {
        assert!(dir.join(file).is_file(), "no {file}");
    }
    assert_eq!(level.name, "made-up level");
    assert_eq!(level.size, Vec2::ints(32 * COLUMNS as i32, 512));
    assert_eq!(level.start, Vec2::ints(24, 481));
    let names: Vec<&str> = level.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["far", "ground"]);
    assert!(level.surfaces.contains_key("normal"));
    assert!(!level.collision.colliders.is_empty());
}

/// The song goes round a note and a rest of 2 beats each, on every other
/// step at 400 outputs a step, so it loops every 3 200 outputs; the
/// music is played through to the end of its second time round.
#[test]
fn writes_the_level_s_music_with_its_loop() {
    let (level, dir) = imported_level("music");
    let music = level.music.unwrap();
    assert_eq!(music.file, "first-music.wav");
    assert_eq!(music.loop_length, Some(3200));
    let wav = fs::read(dir.join(&music.file)).unwrap();
    let frames = (wav.len() as u64 - 44) / 4;
    let start = music.loop_start.unwrap();
    assert_eq!(start + 3200, frames);
    // The game plays it, and what leads into the loop's start is what
    // its end leaves sounding.
    let input = clamber_audio::Input::bytes(wav, Some("wav"));
    let seam = clamber_audio::seam::check(&input, start, start + 3200).unwrap();
    assert!(seam.mismatch < -60.0, "{seam:?}");
    assert!(seam.after < -60.0, "{seam:?}");
}

/// The import report says how each converted song loops.
#[test]
fn the_report_says_how_the_song_loops() {
    let packs = scratch("loops").join("packs");
    let result = importer().import(&image(), &packs, false);
    assert_eq!(result.as_ref().unwrap().notes, ["song 0: loops exactly"]);
    let report = import_report(&result);
    assert!(report.contains("\n  song 0: loops exactly\n"), "{report}");
    assert!(report.contains("Nothing was skipped."), "{report}");
}

/// A song that can't be converted doesn't stop the import: the level
/// goes without music of its own, and the import says so, with
/// diagnostics that don't quote the image.
#[test]
fn a_song_that_cant_be_converted_is_skipped() {
    let mut song = song_block();
    // Channel 0's first command (after the block header, the song header
    // and the ended track's byte) becomes an item the spec doesn't cover.
    song[4 + 19] = 60;
    let image = image_with_song(song);
    let packs = scratch("skipped").join("packs");
    let result = importer_for(&image).import(&image, &packs, false);
    let imported = result.as_ref().unwrap();
    assert_eq!(imported.decoded, ["made-up level"]);
    let [skipped] = &imported.skipped[..] else {
        panic!("{:?}", imported.skipped);
    };
    assert_eq!(skipped.part, Optional::Song);
    assert_eq!(skipped.name, "song 0");
    assert!(
        skipped.reason.contains("song item 60"),
        "{}",
        skipped.reason
    );
    assert_eq!(
        skipped_summary(&imported.skipped),
        ["1 song couldn't be converted; using Clamber's music instead."]
    );
    let report = import_report(&result);
    assert!(report.contains("Couldn't convert song 0: "), "{report}");
    assert!(report.contains("  exactly: stops: "), "{report}");

    let level = Level::load(&imported.dir.join("levels/first.level")).unwrap();
    assert_eq!(level.music, None);
    assert!(!imported.dir.join("levels/first-music.wav").exists());
}

/// Objects without a kind fingerprint row are named by their reference;
/// a group's members carry the group. The single collectibles follow.
#[test]
fn places_the_objects_in_the_placement_list() {
    let (level, _) = imported_level("places");
    let placed: Vec<(&str, Vec2)> = level
        .entities
        .iter()
        .map(|e| (e.kind.as_str(), e.at))
        .collect();
    assert_eq!(
        placed,
        [
            ("placed-7100", Vec2::ints(200, 472)),
            ("placed-7180", Vec2::ints(320, 448)),
            ("spark", Vec2::ints(112, 465)),
            ("spark", Vec2::ints(120, 465)),
        ]
    );
    let member = &level.entities[1].params;
    assert_eq!(member["group"].as_integer(), Some(2));
}

/// The view-limit record gives the level-wide camera limits across, and
/// the vertical limits by x.
#[test]
fn keeps_the_view_within_the_view_limit_record() {
    let (level, _) = imported_level("view-limits");
    let px = |n| Fixed::from_int(n);
    let limits = level.camera.rule.limits.unwrap();
    assert_eq!((limits.x, limits.w), (px(16), px(300 + 256 - 16)));
    let rule = |x, y| level.camera_at(Vec2::ints(x, y));
    for (x, top, bottom, quick) in [
        (0, 0, 288, false),
        (399, 0, 288, false),
        (400, 100, 200, true),
    ] {
        for y in [-40, 300] {
            let r = rule(x, y);
            let l = r.limits.unwrap();
            assert_eq!((l.x, l.w), (limits.x, limits.w), "at {x}, {y}");
            assert_eq!(
                (l.y, l.y + l.h - px(224)),
                (px(top), px(bottom)),
                "at {x}, {y}"
            );
            assert_eq!(r.quick, Some(quick), "at {x}, {y}");
        }
    }
}

#[test]
fn refuses_parts_past_the_end_of_the_image() {
    let mut decoder = LevelDecoder {
        title: "title-9".into(),
        file: "first".into(),
        name: "made-up level".into(),
        parts: parts(),
    };
    decoder.parts.grid = Location::new(BANK, 0xFF00);
    let err = decoder.level(&image()).unwrap_err();
    assert!(err.contains("grid"), "{err}");
}

/// Without a start record, the characters start on the highest surface
/// near the left end.
#[test]
fn without_a_start_record_the_start_is_on_the_ground_at_the_left() {
    let mut decoder = LevelDecoder {
        title: "title-9".into(),
        file: "first".into(),
        name: "made-up level".into(),
        parts: parts(),
    };
    decoder.parts.start = None;
    let (level, ..) = decoder.level(&image()).unwrap();
    assert_eq!(level.start, Vec2::ints(16, 481));
    decoder.parts.start = Some(Location::new(BANK, 0xFFFE));
    let err = decoder.level(&image()).unwrap_err();
    assert!(err.contains("start record"), "{err}");
}

/// wall-exits.md: coming back from the area behind the second wall exit,
/// the lead drops in at the second wall-return record's point (here the
/// start record's).
#[test]
fn the_second_wall_return_record_is_an_arrival_dropping_in() {
    let mut decoder = LevelDecoder {
        title: "title-9".into(),
        file: "first".into(),
        name: "made-up level".into(),
        parts: parts(),
    };
    let (level, ..) = decoder.level(&image()).unwrap();
    assert!(level.arrivals.iter().all(|a| !a.drop));
    decoder.parts.second_wall_return = decoder.parts.start;
    let (level, ..) = decoder.level(&image()).unwrap();
    let arrival = level.arrival(SECOND_WALL_RETURN_ARRIVAL).unwrap();
    assert_eq!(arrival.at, Vec2::ints(24, 481));
    assert!(arrival.drop && arrival.walk.is_none() && arrival.entrance.is_none());
    let again = Level::from_toml_str(&level.to_toml_string(), "again").unwrap();
    assert_eq!(again.arrival(SECOND_WALL_RETURN_ARRIVAL), Some(arrival));
}

/// Walking right across the whole level on `classic-1`, the player is
/// on the ground the spec says is below them on every frame: the floor,
/// under the platform, up the slope and along the higher floor.
#[test]
fn the_player_walks_on_the_imported_ground() {
    let (level, _) = imported_level("walks");
    let ground = Ground::new(Layout::Columns, &grid(), &types()).unwrap();
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    let right: Actions = [Action::Right].into_iter().collect();
    let mut lines = Vec::new();
    for frame in 0..600 {
        step(&mut world, &InputFrame::single(Frame::after(right, right)));
        let p = &world.player;
        let (x, y) = (p.pos.x.floor(), p.pos.y.floor());
        if x >= 32 * COLUMNS as i64 - 16 {
            break;
        }
        assert!(p.grounded(), "frame {frame}: in the air at ({x}, {y})");
        assert_eq!(
            ground.below(x, y),
            Below::Line(y),
            "frame {frame}: feet at ({x}, {y})"
        );
        if lines.last() != Some(&y) {
            lines.push(y);
        }
    }
    assert_eq!(lines.first(), Some(&481));
    assert_eq!(lines.last(), Some(&473), "{lines:?}");
}

/// Walking right along the floor on `classic-1`, the player takes both
/// single collectibles.
#[test]
fn the_player_takes_the_imported_collectibles() {
    let (level, _) = imported_level("takes");
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    let right: Actions = [Action::Right].into_iter().collect();
    for _ in 0..120 {
        step(&mut world, &InputFrame::single(Frame::after(right, right)));
    }
    assert!(world.player.pos.x.floor() > 140);
    assert_eq!(world.flow.gems, 2);
}

/// Packs `data` as literals, 15 bytes to a command
/// (`spec/formats/nibble-packing.md`), behind a header of zeros.
fn nibble_packed(data: &[u8]) -> Vec<u8> {
    let mut nibbles = Vec::new();
    for chunk in data.chunks(15) {
        nibbles.extend([0, chunk.len() as u8]);
        nibbles.extend(chunk.iter().flat_map(|b| [b >> 4, b & 0x0F]));
    }
    nibbles.extend([0, 0]);
    let mut block = vec![0; NIBBLE_HEADER];
    block.extend(
        nibbles
            .chunks(2)
            .map(|n| n[0] << 4 | n.get(1).unwrap_or(&0)),
    );
    block
}

#[test]
fn reads_ground_from_a_packed_map() {
    // Three map columns full of type 1, but band 15 of map column 1,
    // the level's only column, is type 2.
    let mut words = [1u16; 3 * 16];
    words[16 + 15] = 2;
    let map: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    // Type 1 flat at rise 31 (shape 61), type 2 flat at rise 15 (shape
    // 62), passable from below.
    let types = [0, 0, 0, 0, 61, 0, 61, 0, 62, 0x40, 62, 0x40];
    let mut bytes = vec![0; 2 * BANK_SIZE];
    let packed = nibble_packed(&map);
    bytes[BANK_SIZE..BANK_SIZE + packed.len()].copy_from_slice(&packed);
    bytes[BANK_SIZE + 0x8000..BANK_SIZE + 0x8000 + types.len()].copy_from_slice(&types);
    let image = Image::new(bytes);
    let parts = PackedGround {
        map: Location::new(0x41, 0),
        first_column: 1,
        columns: 1,
        type_table: Location::new(0x41, 0x8000),
        types: 3,
    };
    let ground = parts.read(&image).unwrap();
    assert_eq!(ground.width(), 32);
    let surface = ground.surface(5, 15).unwrap();
    assert_eq!((surface.line, surface.passable_from_below), (497, true));
    assert_eq!(ground.surface(5, 14).map(|s| s.line), Some(449));
    assert_eq!(ground.surface(32, 14), None);

    let past = PackedGround {
        columns: 3,
        ..parts
    };
    assert!(past.read(&image).is_err());
    let no_end = PackedGround {
        map: Location::new(0x41, 0xFFF0),
        ..parts
    };
    assert!(no_end.read(&image).is_err());
}

#[test]
fn title_2_images_are_recognised_but_not_imported_yet() {
    let image = Image::new(vec![7; 4096]);
    let importer = Importer {
        catalog: Catalog {
            known: vec![Known {
                title: "title-2".into(),
                release: "made-up release".into(),
                size: image.len() as u64,
                sha256: image.sha256(),
            }],
        },
        decoders: Importer::builtin().decoders,
    };
    let err = importer
        .import(&image, &scratch("title-2").join("packs"), false)
        .unwrap_err();
    assert!(matches!(err, ImportError::Unsupported(_)), "{err:?}");
    assert!(err.to_string().contains("title-2"), "{err}");
    assert!(err.to_string().contains("can't convert it yet"), "{err}");
}
