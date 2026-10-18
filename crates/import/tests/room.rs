//! A made-up image with an area whose background is a screen picture,
//! like the first level's room, imported with a [`LevelDecoder`], loaded
//! from the pack and walked into. None of the data is any real game's:
//! it's made up from `spec/formats/graphics.md` ("Screen pictures"),
//! `spec/formats/level-ground.md`, `spec/formats/placements.md` and
//! `spec/formats/level-start.md`, with the room's view from
//! `spec/behavior/level-ends.md` ("In the first level's room").

use std::fs;
use std::sync::Arc;

use clamber_import::{
    BANK_SIZE, Catalog, FROM_LEVEL, Image, Importer, Known, LevelDecoder, LevelParts, Location,
    Scenery, installed, rgb,
};
use clamber_input::{Action, Actions, Frame};
use clamber_level::{Dir, Fixed, LayerContent, Level, Vec2};
use clamber_sim::{InputFrame, World, generic_feel, step};
use clamber_sprite::{Image as Picture, decode_png};

const BANK: u8 = 0xC1;

/// Tiles in the first part of the tile set; the other 24 are in the
/// second.
const SPLIT: usize = 1000;

/// Where each part goes in the made-up image.
fn parts() -> LevelParts {
    LevelParts {
        grid: Location::new(BANK, 0xB000),
        columns: 16,
        type_table: Location::new(BANK, 0xB400),
        types: 2,
        scenery: Scenery::Screen {
            tiles: Location::new(BANK, 0x0000),
            split: SPLIT,
            rest: Location::new(BANK, 0x8000),
            map: Location::new(BANK, 0x9000),
            palettes: Location::new(BANK, 0xA000),
        },
        placements: Some(Location::new(BANK, 0xB500)),
        collectibles: None,
        start: Some(Location::new(BANK, 0xB600)),
        walk_in: Some(Dir::Left),
        restart: None,
        scene_return: None,
        room_return: None,
        wall_return: None,
        second_wall_return: None,
        room: None,
        back_to: None,
        scene: None,
        scene_of: None,
        view_limits: None,
        song: None,
        late_life_item: false,
    }
}

/// A floor on the bottom band all the way along, at line 481: type 1
/// flat at rise 31 (shape 61).
fn grid() -> Vec<u8> {
    let mut words = [0u16; 16 * 16];
    for column in 0..16 {
        words[column * 16 + 15] = 1;
    }
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn types() -> Vec<u8> {
    vec![0, 0, 61, 61]
}

/// Tile 0 blank, every other tile all colour 15.
fn tiles() -> Vec<u8> {
    let mut bytes = vec![0xFF; 1024 * 32];
    bytes[..32].fill(0);
    bytes
}

/// The backdrop, colour 0 of row 0, and colour 15 of each row, are
/// made-up colours.
fn palettes() -> Vec<u8> {
    let mut words = [0u16; 128];
    words[0] = 0x2D6B;
    for row in 0..8 {
        words[row * 16 + 15] = (row as u16 * 3) | (31 - row as u16 * 2) << 5 | 9 << 10;
    }
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// The top row of squares is tile 1 in palette row 2, and square (5, 5)
/// tile 1 010, from the tile set's second part, in row 3. The rest is
/// tile 0.
fn screen_map() -> Vec<u8> {
    let mut entries = [0u16; 32 * 32];
    entries[..32].fill(1 | 2 << 10);
    entries[5 * 32 + 5] = 1010 | 3 << 10;
    entries.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// One object at x 100, 31 above the bottom edge, then the end.
fn placement_list() -> Vec<u8> {
    [1u16, 100, 31, 0x7100, 0, 0, 0, 0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
}

/// A start record: level x 200, 31 above the bottom edge, on the floor.
fn start_record() -> Vec<u8> {
    [200u16, 31].iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Two banks of a repeating pattern, with the room's parts written in.
fn image() -> Image {
    let mut bytes: Vec<u8> = (0..=250u8).cycle().take(2 * BANK_SIZE).collect();
    let p = parts();
    let all = tiles();
    let (first, rest) = all.split_at(SPLIT * 32);
    for (address, data) in [
        (0x0000, first.to_vec()),
        (0x8000, rest.to_vec()),
        (0x9000, screen_map()),
        (0xA000, palettes()),
        (0xB000, grid()),
        (0xB400, types()),
        (0xB500, placement_list()),
        (0xB600, start_record()),
    ] {
        let start = Location::new(BANK, address).position(bytes.len()).unwrap();
        bytes[start..start + data.len()].copy_from_slice(&data);
    }
    assert_eq!(p.grid, Location::new(BANK, 0xB000));
    Image::new(bytes)
}

/// Imports the made-up image and loads the room back from the pack, with
/// the pack's levels folder.
fn imported_room(name: &str) -> (Level, std::path::PathBuf) {
    let image = image();
    let importer = Importer {
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
            file: "room".into(),
            name: "made-up room".into(),
            parts: parts(),
            kinds: vec![],
            settings: vec![],
        })],
    };
    let dir = std::env::temp_dir().join(format!("clamber-room-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let packs = dir.join("packs");
    importer.import(&image, &packs, false).unwrap();
    let (packs, problems) = installed(&packs);
    assert!(problems.is_empty(), "{problems:?}");
    let levels = packs[0].levels();
    assert_eq!(levels, [packs[0].dir.join("levels/room.level")]);
    (
        Level::load(&levels[0]).unwrap(),
        packs[0].dir.join("levels"),
    )
}

/// The screen picture is one image fixed to level coordinates, with the
/// backdrop where it's transparent.
#[test]
fn writes_the_screen_picture() {
    let (level, dir) = imported_room("picture");
    assert_eq!(level.name, "made-up room");
    assert_eq!(level.size, Vec2::ints(512, 512));
    let [layer] = &level.layers[..] else {
        panic!("one layer, not {}", level.layers.len());
    };
    assert_eq!(
        (layer.parallax, layer.offset),
        (Vec2::ints(1, 1), Vec2::default())
    );
    let LayerContent::Image {
        path,
        repeat_x: false,
        repeat_y: false,
        ..
    } = &layer.content
    else {
        panic!("the screen layer isn't one image");
    };
    assert_eq!(path, "room-screen.png");
    let Picture::Rgba(image) = decode_png(&fs::read(dir.join(path)).unwrap()).unwrap() else {
        panic!("not RGBA");
    };
    assert_eq!((image.width, image.height), (256, 256));
    let colour = |row: usize, number: usize| {
        let at = 2 * (16 * row + number);
        let [r, g, b] = rgb(u16::from_le_bytes([palettes()[at], palettes()[at + 1]]));
        [r, g, b, 255]
    };
    assert_eq!(image.get(0, 0), colour(2, 15));
    assert_eq!(image.get(255, 7), colour(2, 15));
    assert_eq!(image.get(44, 44), colour(3, 15));
    for (x, y) in [(0, 8), (100, 100), (255, 255)] {
        assert_eq!(image.get(x, y), colour(0, 0), "{x},{y}");
    }
}

/// The ground, the objects and where play comes in.
#[test]
fn places_the_room_s_ground_objects_and_arrival() {
    let (level, _) = imported_room("arrival");
    assert!(!level.collision.colliders.is_empty());
    let placed: Vec<(&str, Vec2)> = level
        .entities
        .iter()
        .map(|e| (e.kind.as_str(), e.at))
        .collect();
    assert_eq!(placed, [("placed-7100", Vec2::ints(100, 481))]);
    assert_eq!(level.start, Vec2::ints(200, 481));
    let arrival = level.arrival(FROM_LEVEL).unwrap();
    assert_eq!((arrival.at, arrival.walk), (level.start, Some(Dir::Left)));
}

/// level-ends.md, "In the first level's room": the picture always shows
/// level x 0 to 255 with its top row at level y 0, and the player is
/// stopped at the ends as with `A` = `B` = 0.
#[test]
fn the_view_never_moves_and_the_ends_hold() {
    let (level, _) = imported_room("view");
    let px = |n| Fixed::from_int(n);
    let limits = level.camera.rule.limits.unwrap();
    assert_eq!(
        (limits.x, limits.y, limits.w, limits.h),
        (px(0), px(0), px(256), px(224))
    );
    let arrival = level.arrival(FROM_LEVEL).unwrap().clone();
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let mut world = World::arriving(Arc::new(level), feel, &arrival);
    let mut held = Actions::NONE;
    let mut xs = Vec::new();
    for (keys, frames) in [([Action::Left], 300), ([Action::Right], 300)] {
        let now: Actions = keys.into_iter().collect();
        for _ in 0..frames {
            let x0 = world.player.pos.x.floor();
            step(&mut world, &InputFrame::single(Frame::after(held, now)));
            held = now;
            let view = world.view.unwrap();
            assert_eq!((view.x(), view.y()), (0, 0));
            let x = world.player.pos.x.floor();
            // Stopped on a frame that starts at the end.
            if keys == [Action::Left] && x0 < 18 || keys == [Action::Right] && x0 >= 238 {
                assert_eq!(x, x0, "from {x0}");
            }
            // The walk-in starts 80 px right of the start.
            if world.frame > 28 {
                xs.push(x);
            }
        }
    }
    let (least, most) = (xs.iter().min().unwrap(), xs.iter().max().unwrap());
    assert!((14..18).contains(least), "{least}");
    assert!((238..242).contains(most), "{most}");
}
