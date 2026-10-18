//! A made-up image with an area made like the first level's scene: a
//! long grid of which the view shows 8 columns, a far picture from its
//! first columns and a front layer of two-plane tiles, imported with a
//! [`LevelDecoder`], loaded from the pack and walked into. None of the
//! data is any real game's: it's made up from `spec/formats/graphics.md`
//! ("The first level's scene"), `spec/formats/level-ground.md`,
//! `spec/formats/placements.md`, `spec/formats/level-start.md` and
//! `spec/formats/view-limits.md`, with the scene's view from
//! `spec/behavior/level-ends.md` ("In the first level's scene").

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

/// Columns in the grid: more than 32 768 px, as the scene's.
const COLUMNS: usize = 1100;

/// The view's left edge and top row, from the view-limit record.
const LEFT: u16 = 33_000;
const TOP: u16 = 260;

/// Where each part goes in the made-up image.
fn parts() -> LevelParts {
    LevelParts {
        grid: Location::new(BANK, 0x0000),
        columns: COLUMNS,
        type_table: Location::new(BANK, 0x9100),
        types: 2,
        scenery: Scenery::Scene {
            tiles: Location::new(BANK, 0x9400),
            palettes: Location::new(BANK, 0x9200),
            picture_table: Location::new(BANK, 0x9000),
            pictures: 3,
            front_tiles: Location::new(BANK, 0x9600),
            front_count: 2,
            front_map: Location::new(BANK, 0x9800),
        },
        placements: Some(Location::new(BANK, 0xA100)),
        collectibles: None,
        start: Some(Location::new(BANK, 0xA200)),
        walk_in: Some(Dir::Left),
        restart: None,
        scene_return: None,
        room_return: None,
        wall_return: None,
        room: None,
        back_to: None,
        view_limits: Some(Location::new(BANK, 0xA300)),
        song: None,
    }
}

fn words(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// A floor on the bottom band all the way along, at line 481: type 1
/// flat at rise 31 (shape 61). Band 0 of column 1 is type 2, which has
/// a picture and no ground, as the scene's types past `N` have.
fn grid() -> Vec<u8> {
    let mut cells = vec![0u16; COLUMNS * 16];
    for column in 0..COLUMNS {
        cells[column * 16 + 15] = 1;
    }
    cells[16] = 2;
    words(&cells)
}

fn types() -> Vec<u8> {
    vec![0, 0, 61, 61]
}

/// Type 0 empty, type 1 tile 1 in palette row 2, type 2 tile 1 in row 3.
fn pictures() -> Vec<u8> {
    let mut entries = vec![0u16; 3 * 16];
    entries[16..32].fill(1 | 2 << 10);
    entries[32..].fill(1 | 3 << 10);
    words(&entries)
}

/// Packed tile data: tile 0 blank, tile 1 all colour 15.
fn packed_tiles() -> Vec<u8> {
    let mut bytes = vec![0; 128];
    bytes.extend([0x60, 0x00, 0x60, 0xFF, 0x00]);
    bytes
}

/// Colour 15 of each row, and colour 3 of row 1, are made-up colours.
fn palettes() -> Vec<u8> {
    let mut colours = [0u16; 128];
    for row in 0..8 {
        colours[row * 16 + 15] = (row as u16 * 3) | (31 - row as u16 * 2) << 5 | 9 << 10;
    }
    colours[16 + 3] = 0x1234;
    words(&colours)
}

/// Two-plane tiles: tile 0 blank, tile 1 all colour 3.
fn front_tiles() -> Vec<u8> {
    [vec![0; 16], vec![0xFF; 16]].concat()
}

/// The front map's top row is tile 1 in palette row 4, raised, and its
/// second row the same not raised. The rest is tile 0.
fn front_map() -> Vec<u8> {
    let mut entries = [0u16; 32 * 32];
    entries[..32].fill(1 | 4 << 10 | 1 << 13);
    entries[32..64].fill(1 | 4 << 10);
    words(&entries)
}

/// One object at x 33 100, 31 above the bottom edge, then the end.
fn placement_list() -> Vec<u8> {
    words(&[1, 33_100, 31, 0x7100, 0, 0, 0, 0])
}

/// A start record: level x 33 200, 31 above the bottom edge, on the
/// floor.
fn start_record() -> Vec<u8> {
    words(&[33_200, 31])
}

/// `A` = `B`, and one vertical limit entry with `top` = `bottom`.
fn view_limits() -> Vec<u8> {
    words(&[LEFT, LEFT, LEFT, TOP, TOP, 0, 0xFFFF])
}

/// Two banks of a repeating pattern, with the scene's parts written in.
fn image() -> Image {
    let mut bytes: Vec<u8> = (0..=250u8).cycle().take(2 * BANK_SIZE).collect();
    for (address, data) in [
        (0x0000, grid()),
        (0x9000, pictures()),
        (0x9100, types()),
        (0x9200, palettes()),
        (0x9400, packed_tiles()),
        (0x9600, front_tiles()),
        (0x9800, front_map()),
        (0xA100, placement_list()),
        (0xA200, start_record()),
        (0xA300, view_limits()),
    ] {
        let start = Location::new(BANK, address).position(bytes.len()).unwrap();
        bytes[start..start + data.len()].copy_from_slice(&data);
    }
    Image::new(bytes)
}

/// Imports the made-up image and loads the scene back from the pack,
/// with the pack's levels folder.
fn imported_scene(name: &str) -> (Level, std::path::PathBuf) {
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
            file: "scene".into(),
            name: "made-up scene".into(),
            parts: parts(),
        })],
    };
    let dir = std::env::temp_dir().join(format!("clamber-scene-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let packs = dir.join("packs");
    importer.import(&image, &packs, false).unwrap();
    let (packs, problems) = installed(&packs);
    assert!(problems.is_empty(), "{problems:?}");
    let levels = packs[0].levels();
    assert_eq!(levels, [packs[0].dir.join("levels/scene.level")]);
    (
        Level::load(&levels[0]).unwrap(),
        packs[0].dir.join("levels"),
    )
}

fn png(path: std::path::PathBuf) -> clamber_sprite::RgbaImage {
    let Picture::Rgba(image) = decode_png(&fs::read(path).unwrap()).unwrap() else {
        panic!("not RGBA");
    };
    image
}

fn colour(row: usize, number: usize) -> [u8; 4] {
    let at = 2 * (16 * row + number);
    let [r, g, b] = rgb(u16::from_le_bytes([palettes()[at], palettes()[at + 1]]));
    [r, g, b, 255]
}

/// The far layer from the grid's first columns, the ground layer, and
/// the front layer's raised tiles fixed on screen.
#[test]
fn writes_the_scene_s_three_layers() {
    let (level, dir) = imported_scene("layers");
    assert_eq!(level.name, "made-up scene");
    assert_eq!(level.size, Vec2::ints(32 * COLUMNS as i32, 512));
    let names: Vec<&str> = level.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["far", "ground", "front"]);

    let far = &level.layers[0];
    let LayerContent::Image { path, .. } = &far.content else {
        panic!("the far layer isn't an image");
    };
    let image = png(dir.join(path));
    assert_eq!((image.width, image.height), (256, 256));
    // Column 1's band 0, type 2.
    assert_eq!(image.get(32, 0), colour(3, 15));
    assert_eq!(image.get(63, 31), colour(3, 15));
    assert_eq!(image.get(0, 0), [0; 4]);
    assert_eq!(image.get(64, 0), [0; 4]);
    // On screen with the view at its place, the far picture's row 5 is
    // at the top, and its left column at the left.
    let half = Fixed::from_ratio_pow2(1, 1);
    let (left, top) = (Fixed::from_int(LEFT.into()), Fixed::from_int(TOP.into()));
    assert_eq!(far.parallax, Vec2::new(half, half));
    assert_eq!(far.offset.y - top * half, Fixed::from_int(-5));
    assert_eq!((far.offset.x - left * half).floor().rem_euclid(256), 0);

    let front = &level.layers[2];
    assert_eq!(
        (front.parallax, front.offset),
        (Vec2::default(), Vec2::ints(0, -1))
    );
    let LayerContent::Image {
        path,
        repeat_x: false,
        repeat_y: false,
        ..
    } = &front.content
    else {
        panic!("the front layer isn't one image");
    };
    let image = png(dir.join(path));
    // Palette row 4, colour 3: colour 19 of the set, row 1's colour 3.
    assert_eq!(image.get(0, 0), colour(1, 3));
    assert_eq!(image.get(255, 7), colour(1, 3));
    // Not raised.
    assert_eq!(image.get(0, 8), [0; 4]);
}

/// The ground, the objects and where play comes in.
#[test]
fn places_the_scene_s_ground_objects_and_arrival() {
    let (level, _) = imported_scene("arrival");
    assert!(!level.collision.colliders.is_empty());
    let placed: Vec<(&str, Vec2)> = level
        .entities
        .iter()
        .map(|e| (e.kind.as_str(), e.at))
        .collect();
    assert_eq!(placed, [("placed-7100", Vec2::ints(33_100, 481))]);
    assert_eq!(level.start, Vec2::ints(33_200, 481));
    let arrival = level.arrival(FROM_LEVEL).unwrap();
    assert_eq!((arrival.at, arrival.walk), (level.start, Some(Dir::Left)));
}

/// level-ends.md, "In the first level's scene": the picture's left edge
/// is always `A` and its top row always `top`.
#[test]
fn the_view_never_moves() {
    let (level, _) = imported_scene("view");
    let px = |n: u16| Fixed::from_int(n.into());
    let limits = level.camera.rule.limits.unwrap();
    assert_eq!(
        (limits.x, limits.y, limits.w, limits.h),
        (px(LEFT), px(TOP), px(256), px(224))
    );
    let arrival = level.arrival(FROM_LEVEL).unwrap().clone();
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let mut world = World::arriving(Arc::new(level), feel, &arrival);
    let mut held = Actions::NONE;
    for (keys, frames) in [([Action::Left], 200), ([Action::Right], 200)] {
        let now: Actions = keys.into_iter().collect();
        for _ in 0..frames {
            step(&mut world, &InputFrame::single(Frame::after(held, now)));
            held = now;
            let view = world.view.unwrap();
            assert_eq!(
                (view.x(), view.y()),
                (LEFT.into(), TOP.into()),
                "frame {}",
                world.frame
            );
        }
    }
}
