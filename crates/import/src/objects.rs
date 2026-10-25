//! The objects' picture sheets (`spec/formats/sprites.md`, "The other
//! objects' palettes"): the pictures the spec names by hash for the
//! letters, single collectibles, bunches, thrown objects with their
//! flashes and pieces, the animal crate with its smoke and animal, the
//! walkers, the thrower and its balls, the critters, the stingers and the
//! extra-life items,
//! each coloured with the palette the spec gives.
//!
//! Pictures are found as the characters' are ([`find_pictures`]); a
//! picture not found is left out of its sheet, and the game keeps its own
//! art for what the sheet lacks. A sheet's pictures can use several
//! palettes (the thrown objects' flashes use the shared one), so a
//! sheet's image has all of them, the n-th at colour numbers 16n + 1 to
//! 16n + 15. The tyres, signs and launch barrels have
//! no palette in the spec yet, so [`PENDING_SHEETS`] lists their sheets
//! and the import report says they are not converted.
//!
//! `crate-broken-1` is coloured with the crate's palette as the engine's
//! choice: the spec gives a palette for the crate's other pictures but
//! never saw this one.

use std::collections::HashSet;

use clamber_sprite::Palette;

use crate::characters::Files;
use crate::{
    Data, Decoder, FrameTable, Image, Known, Location, Mirror, PackWriter, SPRITES, Sha256Hash,
    SpritePalette, TABLE_ENTRY_SIZE, find_pictures, locate, read_frames, sprite_sheet_in,
    with_dim_colours,
};

/// The sprite palettes that colour objects' pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectPalette {
    Shared,
    Thrown,
    Crate,
    Animal,
    SmallWalker,
    MediumWalker,
    LargeWalker,
    Thrower,
    Critter,
    Stinger,
    OneLife,
    /// The first character's leading palette.
    FirstLeading,
    /// The second character's leading palette.
    SecondLeading,
}

/// One sheet: its name and its pictures by label, picture hash and palette.
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectSheet {
    pub name: &'static str,
    pub pictures: &'static [(&'static str, &'static str, ObjectPalette)],
}

impl ObjectPalette {
    fn data(self) -> Data {
        match self {
            ObjectPalette::Shared => Data::SharedPalette,
            ObjectPalette::Thrown => Data::ThrownPalette,
            ObjectPalette::Crate => Data::CratePalette,
            ObjectPalette::Animal => Data::AnimalPalette,
            ObjectPalette::SmallWalker => Data::SmallWalkerPalette,
            ObjectPalette::MediumWalker => Data::MediumWalkerPalette,
            ObjectPalette::LargeWalker => Data::LargeWalkerPalette,
            ObjectPalette::Thrower => Data::ThrowerPalette,
            ObjectPalette::Critter => Data::CritterPalette,
            ObjectPalette::Stinger => Data::StingerPalette,
            ObjectPalette::OneLife => Data::OneLifePalette,
            ObjectPalette::FirstLeading => Data::FirstLeadingPalette,
            ObjectPalette::SecondLeading => Data::SecondLeadingPalette,
        }
    }
}

/// The sheets the game reads for objects whose pictures the importer
/// can't write yet, with what they hold.
pub const PENDING_SHEETS: &[(&str, &str)] = &[
    ("sign-pictures", "the signs"),
    ("hidden-patch", "the hidden barrels' patch"),
    ("barrel-pictures", "the launch barrels"),
    ("tyre-pictures", "the tyres"),
    ("stiff-tyre-pictures", "the stiff tyres"),
];

/// The names of the sheets the importer writes for objects.
pub fn written_sheets() -> impl Iterator<Item = &'static str> {
    crate::object_hashes::SHEETS.iter().map(|s| s.name)
}

/// Writes the objects' sheets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectDecoder {
    pub title: String,
    table: Location,
    /// Entries in the frame table.
    entries: usize,
    palettes: Vec<(ObjectPalette, Location)>,
    /// The sheets to write.
    sheets: &'static [ObjectSheet],
}

impl ObjectDecoder {
    /// A decoder for `title` with its frame table (of `entries`),
    /// palettes and sheets given, for images the spec doesn't describe
    /// (made-up ones in tests).
    pub fn custom(
        title: &str,
        table: Location,
        entries: usize,
        palettes: Vec<(ObjectPalette, Location)>,
        sheets: &'static [ObjectSheet],
    ) -> ObjectDecoder {
        ObjectDecoder {
            title: title.to_string(),
            table,
            entries,
            palettes,
            sheets,
        }
    }

    /// The objects' pictures of `title`, where the spec says where their
    /// frames and palettes are.
    pub fn of(title: &str) -> Option<ObjectDecoder> {
        let at = |data| locate(title, data);
        let entries = match title {
            "title-1" => 2_749,
            _ => return None,
        };
        let all = [
            ObjectPalette::Shared,
            ObjectPalette::Thrown,
            ObjectPalette::Crate,
            ObjectPalette::Animal,
            ObjectPalette::SmallWalker,
            ObjectPalette::MediumWalker,
            ObjectPalette::LargeWalker,
            ObjectPalette::Thrower,
            ObjectPalette::Critter,
            ObjectPalette::Stinger,
            ObjectPalette::OneLife,
            ObjectPalette::FirstLeading,
            ObjectPalette::SecondLeading,
        ];
        let palettes = all
            .into_iter()
            .map(|p| Some((p, at(p.data())?)))
            .collect::<Option<Vec<_>>>()?;
        Some(ObjectDecoder {
            title: title.to_string(),
            table: at(Data::FrameTable)?,
            entries,
            palettes,
            sheets: crate::object_hashes::SHEETS,
        })
    }

    /// The sheets' files, by path in the pack, and notes for the import
    /// report.
    pub fn sheets(&self, image: &Image) -> Result<(Files, Vec<String>), String> {
        self.sheets_of(image, self.sheets)
    }

    fn sheets_of(
        &self,
        image: &Image,
        sheets: &[ObjectSheet],
    ) -> Result<(Files, Vec<String>), String> {
        let bytes = image
            .at(self.table)
            .and_then(|b| b.get(..self.entries * TABLE_ENTRY_SIZE))
            .ok_or("the frame table is outside the image")?;
        let table = FrameTable::new(bytes)?;
        let wanted: HashSet<Sha256Hash> = sheets
            .iter()
            .flat_map(|s| s.pictures)
            .filter_map(|&(_, hash, _)| Sha256Hash::from_hex(hash))
            .collect();
        let found = find_pictures(&table, &wanted, |l| image.at(l));
        let mut files = Vec::new();
        let mut notes = Vec::new();
        for sheet in sheets {
            let mut frames = Vec::new();
            let mut missing = 0;
            for &(label, hash, palette) in sheet.pictures {
                match Sha256Hash::from_hex(hash).and_then(|h| found.get(&h)) {
                    Some(&number) => frames.push((label, number, palette)),
                    None => missing += 1,
                }
            }
            if missing > 0 {
                notes.push(format!(
                    "{}: {missing} of {} pictures not found; the engine's own art stands in \
                     for what the sheet lacks",
                    sheet.name,
                    sheet.pictures.len()
                ));
            }
            if frames.is_empty() {
                continue;
            }
            let read = read_frames(&table, frames.iter().map(|f| f.1), |l| image.at(l))?;
            // The sheet's palettes, in order of first use.
            let mut used: Vec<ObjectPalette> = Vec::new();
            for &(_, _, palette) in &frames {
                if !used.contains(&palette) {
                    used.push(palette);
                }
            }
            let mut colours = Palette(vec![[0; 4]]);
            for &palette in &used {
                let at = self
                    .palettes
                    .iter()
                    .find(|p| p.0 == palette)
                    .map(|p| p.1)
                    .ok_or("a palette has no place")?;
                let bytes = image
                    .at(at)
                    .and_then(|b| b.get(..SpritePalette::SIZE))
                    .ok_or("a palette is outside the image")?;
                let mut group = SpritePalette::new(bytes)?.palette().0;
                if colours.0.len() > 1 {
                    group[0] = [0; 4];
                } else {
                    colours.0.clear();
                }
                colours.0.extend(group);
            }
            let mut pictures = Vec::new();
            for &(label, number, palette) in &frames {
                let mut picture = read[&number].picture(None, Mirror::default())?;
                let group = used.iter().position(|&p| p == palette).unwrap_or(0);
                let shift = u8::try_from(16 * group).map_err(|_| "too many palettes")?;
                for v in &mut picture.pixels.data {
                    if *v != 0 {
                        *v += shift;
                    }
                }
                pictures.push((label.to_string(), picture));
            }
            let sheet_image = format!("{}.png", sheet.name);
            // The shared palette's colours 1 to 15 and the first
            // character's leading palette's 1 to 14 dim
            // (`spec/behavior/dimming.md`, "Sprite colours").
            let dims: Vec<(u8, u8)> = used
                .iter()
                .enumerate()
                .filter_map(|(group, palette)| {
                    let first = u8::try_from(16 * group).ok()? + 1;
                    match palette {
                        ObjectPalette::Shared => Some((first, first + 14)),
                        ObjectPalette::FirstLeading => Some((first, first + 13)),
                        _ => None,
                    }
                })
                .collect();
            let files_for = with_dim_colours(
                sprite_sheet_in(&pictures, &colours, &[], &sheet_image)?,
                &dims,
            )?;
            files.push((
                format!("{SPRITES}/{}.json", sheet.name),
                files_for.json.into_bytes(),
            ));
            files.push((format!("{SPRITES}/{sheet_image}"), files_for.png));
        }
        Ok((files, notes))
    }
}

impl Decoder for ObjectDecoder {
    fn name(&self) -> &str {
        "Objects' pictures"
    }

    fn applies_to(&self, image: &Known) -> bool {
        image.title == self.title
    }

    /// The pictures are optional: if they can't be read, the pack goes
    /// without them and the engine shows its own art.
    fn decode(&self, image: &Image, pack: &mut PackWriter) -> Result<(), String> {
        match crate::optional(|| self.sheets(image)) {
            Ok((files, notes)) => {
                for (path, contents) in files {
                    pack.write(&path, &contents)?;
                }
                for note in notes {
                    pack.note(note);
                }
            }
            Err(reason) => pack.note(format!(
                "the objects' pictures couldn't be converted: {reason}; the engine's own art \
                 stands in"
            )),
        }
        for note in notes() {
            pack.note(note);
        }
        Ok(())
    }
}

/// Import report notes for the objects whose pictures aren't converted.
pub fn notes() -> impl Iterator<Item = String> {
    PENDING_SHEETS.iter().map(|(sheet, what)| {
        format!(
            "{what} ({sheet}): not converted, the spec doesn't yet say their palettes; the \
             engine's own art stands in"
        )
    })
}

#[cfg(test)]
mod tests {
    use clamber_sprite::{SpriteSheet, decode_png};

    use super::ObjectPalette::*;
    use super::*;

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
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn hex(text: &str) -> Vec<u8> {
        text.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    /// The vectors' bank with `palette.hex` as the shared palette and
    /// its colours reversed as the thrown objects'.
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

    fn hash(n: u16) -> &'static str {
        let line = vector("hashes.tsv")
            .lines()
            .find(|l| l.split('\t').next() == Some(&n.to_string()))
            .unwrap()
            .to_string();
        line.rsplit('\t').next().unwrap().to_string().leak()
    }

    fn decoder() -> ObjectDecoder {
        let at = |a| Location::new(BANK, a);
        ObjectDecoder {
            title: "made-up".into(),
            table: at(TABLE),
            entries: TABLE_FRAMES,
            palettes: vec![(Shared, at(SHARED_AT)), (Thrown, at(THROWN_AT))],
            sheets: &[],
        }
    }

    #[test]
    fn a_sheet_with_two_palettes_has_both_in_its_image() {
        let sheet = ObjectSheet {
            name: "mixed",
            pictures: vec![
                ("a", hash(35), Shared),
                ("b", hash(37), Thrown),
                ("c", &"07".repeat(32).leak()[..], Thrown),
            ]
            .leak(),
        };
        let (files, notes) = decoder().sheets_of(&image(), &[sheet]).unwrap();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].starts_with("mixed: 1 of 3"), "{}", notes[0]);
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["sprites/mixed.json", "sprites/mixed.png"]);
        let json = std::str::from_utf8(&files[0].1).unwrap();
        let meta = clamber_sprite::Sheet::from_json_str(json, "mixed.json").unwrap();
        let names: Vec<&str> = meta.animations.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        // Only the shared palette's colours 1 to 15 dim.
        assert_eq!(meta.dim_colours, [(1, 15)]);
        let shared = SpritePalette::new(&hex(&vector("palette.hex"))).unwrap();
        let image = decode_png(&files[1].1).unwrap();
        let sheet = SpriteSheet::new(meta, image).unwrap();
        let clamber_sprite::Image::Indexed { palette, indices } = &sheet.image else {
            panic!("not indexed");
        };
        assert_eq!(palette.0.len(), 32);
        assert_eq!(&palette.0[..16], &shared.palette().0[..]);
        // The second palette's colour 1 is the first's colour 15.
        assert_eq!(palette.0[17], shared.palette().0[15]);
        assert!(indices.data.iter().any(|&i| i > 16));
        assert!(indices.data.iter().all(|&i| i % 16 != 0 || i == 0));
    }

    #[test]
    fn nothing_found_writes_no_sheet() {
        let sheet = ObjectSheet {
            name: "none",
            pictures: vec![("a", &"07".repeat(32).leak()[..], Shared)].leak(),
        };
        let (files, notes) = decoder().sheets_of(&image(), &[sheet]).unwrap();
        assert!(files.is_empty());
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn the_hashes_are_the_specs() {
        let docs = [
            "letters",
            "collectibles",
            "bunches",
            "thrown-objects",
            "animal-crate",
            "riding",
            "walker",
            "thrower",
            "critter",
            "stinger",
            "extra-lives",
            "metal-barrel",
        ];
        let mut spec = Vec::new();
        for doc in docs {
            let path = format!(
                "{}/../../spec/behavior/{doc}.md",
                env!("CARGO_MANIFEST_DIR")
            );
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text.lines() {
                let cells: Vec<&str> = line
                    .split('|')
                    .map(|c| c.trim().trim_matches('`'))
                    .collect();
                if cells.len() >= 3
                    && cells[2].len() == 64
                    && cells[2].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    spec.push((doc, cells[1].to_string(), cells[2].to_string()));
                }
            }
        }
        for sheet in crate::object_hashes::SHEETS {
            for &(label, hash, _) in sheet.pictures {
                assert!(
                    spec.iter().any(|(_, l, h)| l == label && h == hash),
                    "{} {label}",
                    sheet.name
                );
            }
        }
        // Every written sheet is in the table, once.
        let names: Vec<&str> = written_sheets().collect();
        assert_eq!(names.len(), 14);
    }

    #[test]
    fn a_title_without_locators_has_no_decoder() {
        assert!(ObjectDecoder::of("title-1").is_some());
        assert!(ObjectDecoder::of("title-2").is_none());
    }
}
