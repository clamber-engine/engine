//! A made-up image with a level's ground and background in it, imported
//! with a [`LevelDecoder`], loaded from the pack and played. None of the
//! data is any real game's: it's made up from the format specs,
//! `spec/formats/level-ground.md` and `spec/formats/graphics.md`.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clamber_import::{
    BANK_SIZE, Below, Catalog, Ground, Image, Importer, Known, Layout, LevelDecoder, LevelParts,
    Location, installed,
};
use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
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

/// Where each part goes in the made-up image.
fn parts() -> LevelParts {
    LevelParts {
        grid: Location::new(BANK, 0x1000),
        columns: COLUMNS,
        type_table: Location::new(BANK, 0x3000),
        types: types().len() / 2,
        tiles: Location::new(BANK, 0x4000),
        palettes: Location::new(BANK, 0x5000),
        picture_table: Location::new(BANK, 0x6000),
        pictures: 5,
    }
}

/// Two banks of a repeating pattern, with the level's parts written in.
fn image() -> Image {
    let mut bytes: Vec<u8> = (0..=250u8).cycle().take(2 * BANK_SIZE).collect();
    let p = parts();
    for (at, data) in [
        (p.grid, grid()),
        (p.type_table, types()),
        (p.tiles, packed_tiles()),
        (p.palettes, palettes()),
        (p.picture_table, pictures()),
    ] {
        let start = at.position(bytes.len()).unwrap();
        bytes[start..start + data.len()].copy_from_slice(&data);
    }
    Image::new(bytes)
}

fn importer() -> Importer {
    let image = image();
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
    for image in ["first-far.png", "first-tiles.png"] {
        assert!(dir.join(image).is_file(), "no {image}");
    }
    assert_eq!(level.name, "made-up level");
    assert_eq!(level.size, Vec2::ints(32 * COLUMNS as i32, 512));
    assert_eq!(level.start, Vec2::ints(16, 481));
    let names: Vec<&str> = level.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["far", "ground"]);
    assert!(level.surfaces.contains_key("normal"));
    assert!(!level.collision.colliders.is_empty());
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
