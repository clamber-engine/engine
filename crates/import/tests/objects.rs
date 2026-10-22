//! A made-up image with objects' pictures in it, imported with an
//! [`ObjectDecoder`] and read back from the pack. None of the data is any
//! real game's: it's the sprite vectors in `spec/vectors/sprites`, put
//! where a made-up frame table and palettes say. The sheets are named as
//! the game reads them, and the importer's list of sheets is checked
//! against the sheets the game reads in the game crate's tests.

use std::fs;

use clamber_import::{
    Catalog, Image, Importer, Known, Location, ObjectDecoder, ObjectPalette, ObjectSheet, installed,
};
use clamber_sprite::{Image as Pixels, Sheet, SpriteSheet, decode_png};

const BANK: u8 = 0x5A;
const TABLE: u16 = 0x8010;
const TABLE_FRAMES: usize = 39;
const SHARED_AT: u16 = 0xF000;
const THROWN_AT: u16 = 0xF020;

fn vector(name: &str) -> String {
    let path = format!(
        "{}/../../spec/vectors/sprites/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

/// The vectors' bank, with the vectors' palette as the shared palette
/// and its colours reversed as the thrown objects'.
fn image() -> Image {
    let at = Location::new(BANK, 0).position(usize::MAX).unwrap();
    let mut bytes = vec![0; at + 0x1_0000];
    for line in vector("bank.hex").lines().filter(|l| !l.trim().is_empty()) {
        let (address, rest) = line.split_once(':').unwrap();
        let p = at + usize::from(u16::from_str_radix(address.trim(), 16).unwrap());
        let line = hex(rest);
        bytes[p..p + line.len()].copy_from_slice(&line);
    }
    let shared = hex(&vector("palette.hex"));
    let thrown: Vec<u8> = shared.chunks(2).rev().flatten().copied().collect();
    for (address, palette) in [(SHARED_AT, &shared), (THROWN_AT, &thrown)] {
        let p = at + usize::from(address);
        bytes[p..p + palette.len()].copy_from_slice(palette);
    }
    Image::new(bytes)
}

fn hash(frame: u16) -> &'static str {
    let text = vector("hashes.tsv");
    let line = text
        .lines()
        .find(|l| l.split('\t').next() == Some(&frame.to_string()))
        .unwrap();
    line.rsplit('\t').next().unwrap().to_string().leak()
}

fn sheets() -> &'static [ObjectSheet] {
    Vec::leak(vec![
        ObjectSheet {
            name: "letter-pictures",
            pictures: Vec::leak(vec![
                ("letter-pic-1", hash(35), ObjectPalette::Shared),
                ("letter-pic-2", hash(36), ObjectPalette::Shared),
            ]),
        },
        ObjectSheet {
            name: "thrown-pictures",
            pictures: Vec::leak(vec![
                ("thrown-1", hash(37), ObjectPalette::Thrown),
                ("thrown-flash", hash(35), ObjectPalette::Shared),
            ]),
        },
        // Its picture isn't in the image: no sheet, and a note.
        ObjectSheet {
            name: "lost-pictures",
            pictures: Vec::leak(vec![(
                "lost",
                &*Box::leak("07".repeat(32).into_boxed_str()),
                ObjectPalette::Shared,
            )]),
        },
    ])
}

fn importer(image: &Image) -> Importer {
    let at = |a| Location::new(BANK, a);
    let objects = ObjectDecoder::custom(
        "title-9",
        at(TABLE),
        TABLE_FRAMES,
        vec![
            (ObjectPalette::Shared, at(SHARED_AT)),
            (ObjectPalette::Thrown, at(THROWN_AT)),
        ],
        sheets(),
    );
    Importer {
        catalog: Catalog {
            known: vec![Known {
                title: "title-9".into(),
                release: "made-up release".into(),
                size: image.len() as u64,
                sha256: image.sha256(),
            }],
        },
        decoders: vec![Box::new(objects)],
    }
}

#[test]
fn an_import_writes_the_sheets_the_game_loads() {
    let image = image();
    let dir = std::env::temp_dir().join(format!("clamber-objects-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let packs = dir.join("packs");
    let imported = importer(&image).import(&image, &packs, false).unwrap();
    let (found, problems) = installed(&packs);
    assert!(problems.is_empty(), "{problems:?}");
    let [pack] = &found[..] else {
        panic!("one pack, not {}", found.len());
    };
    assert_eq!(pack.dir, imported.dir);

    let load = |name: &str| {
        let sprites = pack.dir.join("sprites");
        let json = fs::read_to_string(sprites.join(format!("{name}.json"))).unwrap();
        let meta = Sheet::from_json_str(&json, name).unwrap();
        let png = decode_png(&fs::read(sprites.join(format!("{name}.png"))).unwrap()).unwrap();
        SpriteSheet::new(meta, png).unwrap()
    };
    let labels = |sheet: &SpriteSheet| -> Vec<String> {
        sheet
            .sheet
            .animations
            .iter()
            .map(|a| a.name.clone())
            .collect()
    };

    let letters = load("letter-pictures");
    assert_eq!(labels(&letters), ["letter-pic-1", "letter-pic-2"]);
    // The thrown sheet draws on two palettes, 16 colours apart.
    let thrown = load("thrown-pictures");
    assert_eq!(labels(&thrown), ["thrown-1", "thrown-flash"]);
    let Pixels::Indexed { palette, indices } = &thrown.image else {
        panic!("not indexed");
    };
    assert_eq!(palette.0.len(), 32);
    assert!(indices.data.iter().any(|&i| i > 16));

    assert!(!pack.dir.join("sprites/lost-pictures.json").exists());
    assert!(
        imported
            .notes
            .iter()
            .any(|n| n.starts_with("lost-pictures: 1 of 1 pictures not found")),
        "{:?}",
        imported.notes
    );
    let _ = fs::remove_dir_all(&dir);
}
