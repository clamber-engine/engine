//! Game images and recognizing them.

use std::fmt;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::ImportError;

/// The largest file read as an image. Anything bigger isn't one.
pub const MAX_IMAGE_SIZE: u64 = 64 * 1024 * 1024;

/// Bytes of a copier header, which some files have in front of the
/// image (`spec/formats/rom-image.md`, "Files and images").
pub const COPIER_HEADER: usize = 512;

/// The bytes of a player's game image.
///
/// They stay in memory: nothing here prints, logs or reports them, and
/// `Debug` shows only the size.
pub struct Image {
    bytes: Vec<u8>,
}

impl Image {
    pub fn new(bytes: Vec<u8>) -> Image {
        Image { bytes }
    }

    /// The image in a file: the file without its copier header, if it
    /// has one. Refuses a file whose size isn't whole KiB, with or
    /// without a header.
    pub fn from_file(mut bytes: Vec<u8>) -> Result<Image, ImportError> {
        match bytes.len() % 1024 {
            0 => {}
            COPIER_HEADER => {
                bytes.drain(..COPIER_HEADER);
            }
            _ => {
                return Err(ImportError::Unsupported(format!(
                    "it isn't a game image ({} bytes)",
                    bytes.len()
                )));
            }
        }
        Ok(Image::new(bytes))
    }

    /// Reads an image file, refusing anything too big to be one.
    pub fn read(path: &Path) -> Result<Image, ImportError> {
        let io = |e: std::io::Error| ImportError::Read(format!("{}: {e}", path.display()));
        let size = std::fs::metadata(path).map_err(io)?.len();
        if size > MAX_IMAGE_SIZE {
            return Err(ImportError::Read(format!(
                "{}: {size} bytes is too big to be a game image",
                path.display()
            )));
        }
        Image::from_file(std::fs::read(path).map_err(io)?)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The SHA-256 of the whole image, which identifies it.
    pub fn sha256(&self) -> Sha256Hash {
        Sha256Hash(Sha256::digest(&self.bytes).into())
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({} bytes)", self.bytes.len())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sha256Hash(pub [u8; 32]);

impl Sha256Hash {
    /// Reads 64 hex digits.
    pub fn from_hex(hex: &str) -> Option<Sha256Hash> {
        if hex.len() != 64 || !hex.is_ascii() {
            return None;
        }
        let mut out = [0; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Sha256Hash(out))
    }
}

impl fmt::Display for Sha256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

/// An image the importer supports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    /// The title it is, as a title id (`title-1`, …). Imported packs are
    /// named after it.
    pub title: String,
    /// Which release of the title, in words, to show the player.
    pub release: String,
    pub size: u64,
    pub sha256: Sha256Hash,
}

/// What an image is, from `spec/formats/rom-image.md`, "Identifying an
/// image".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// No supported image has its size.
    WrongSize,
    /// It has a supported image's size but none's hash: another release,
    /// or a changed or incomplete image.
    Unsupported,
    Known(&'a Known),
}

/// The images the importer can recognize.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub known: Vec<Known>,
}

impl Catalog {
    /// The images this build of Clamber supports: the releases in
    /// `spec/formats/rom-image.md`, "Supported releases".
    pub fn builtin() -> Catalog {
        let known = |title: &str, size, sha256| Known {
            title: title.into(),
            release: "the supported release".into(),
            size,
            sha256: Sha256Hash::from_hex(sha256).unwrap(),
        };
        Catalog {
            known: vec![
                known(
                    "title-1",
                    4 * 1024 * 1024,
                    "628147468c3539283197f58f03b94df49758a332831857481ea9cc31645f0527",
                ),
                known(
                    "title-2",
                    4 * 1024 * 1024,
                    "b79c2bb86f6fc76e1fc61c62fc16d51c664c381e58bc2933be643bbc4d8b610c",
                ),
            ],
        }
    }

    /// What `image` is. The hash is only taken for an image of a
    /// supported size.
    pub fn outcome(&self, image: &Image) -> Outcome<'_> {
        let size = image.len() as u64;
        if !self.known.iter().any(|k| k.size == size) {
            return Outcome::WrongSize;
        }
        let hash = image.sha256();
        match self
            .known
            .iter()
            .find(|k| k.size == size && k.sha256 == hash)
        {
            Some(known) => Outcome::Known(known),
            None => Outcome::Unsupported,
        }
    }

    /// Finds the image in the catalog, or says why it isn't there.
    pub fn identify(&self, image: &Image) -> Result<&Known, ImportError> {
        if image.is_empty() {
            return Err(ImportError::Unsupported("the file is empty".into()));
        }
        let size = image.len();
        let reason = match self.outcome(image) {
            Outcome::Known(known) => return Ok(known),
            _ if self.known.is_empty() => {
                "this version of Clamber can't import any games yet".to_string()
            }
            Outcome::WrongSize => {
                format!("it isn't a supported image: none is {size} bytes")
            }
            Outcome::Unsupported => format!(
                "it has the size of a supported image but not its contents \
                 (SHA-256 {}), so it's another release of the game, or it \
                 has been modified or damaged",
                image.sha256()
            ),
        };
        Err(ImportError::Unsupported(reason))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(image: &Image) -> Catalog {
        Catalog {
            known: vec![Known {
                title: "title-9".into(),
                release: "test release".into(),
                size: image.len() as u64,
                sha256: image.sha256(),
            }],
        }
    }

    #[test]
    fn hashes() {
        assert_eq!(
            Image::new(b"abc".to_vec()).sha256().to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let hash = Image::new(vec![7; 100]).sha256();
        assert_eq!(Sha256Hash::from_hex(&hash.to_string()), Some(hash));
        assert_eq!(Sha256Hash::from_hex("ab"), None);
        assert_eq!(Sha256Hash::from_hex(&"g".repeat(64)), None);
    }

    #[test]
    fn identifies_known_images() {
        let image = Image::new(vec![1, 2, 3, 4]);
        let catalog = catalog(&image);
        assert_eq!(catalog.identify(&image).unwrap().title, "title-9");
    }

    #[test]
    fn refuses_others_with_a_reason() {
        let image = Image::new(vec![1, 2, 3, 4]);
        let reason = |catalog: &Catalog, image: &Image| match catalog.identify(image) {
            Err(ImportError::Unsupported(reason)) => reason,
            other => panic!("{other:?}"),
        };
        assert!(reason(&Catalog::default(), &image).contains("can't import any games yet"));
        let catalog = catalog(&image);
        assert!(reason(&catalog, &Image::new(vec![1, 2, 3, 5])).contains("modified"));
        assert!(reason(&catalog, &Image::new(vec![1, 2, 3])).contains("isn't a supported"));
        assert!(reason(&catalog, &Image::new(Vec::new())).contains("empty"));
    }

    fn vectors(name: &str) -> Vec<Vec<String>> {
        let path = format!(
            "{}/../../spec/vectors/rom-image/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let rows: Vec<Vec<String>> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .skip(1)
            .map(|line| line.split('\t').map(String::from).collect())
            .collect();
        assert!(!rows.is_empty());
        rows
    }

    #[test]
    fn finds_images_in_files_as_the_spec_vectors_do() {
        let builtin = Catalog::builtin();
        for row in vectors("sizes.tsv") {
            let [file_size, header, image_size, outcome] = &row[..] else {
                panic!("bad row {row:?}");
            };
            let file_size: usize = file_size.parse().unwrap();
            let image = Image::from_file(vec![0; file_size]);
            let image = match (outcome.as_str(), image) {
                ("not-an-image", Err(ImportError::Unsupported(_))) => continue,
                (_, Ok(image)) => image,
                (_, other) => panic!("{row:?}: {other:?}"),
            };
            assert_eq!(image.len().to_string(), *image_size, "{row:?}");
            assert_eq!(header == "yes", image.len() != file_size, "{row:?}");
            let expected = if outcome == "wrong-size" {
                Outcome::WrongSize
            } else {
                // Zeros aren't any game's image.
                Outcome::Unsupported
            };
            assert_eq!(builtin.outcome(&image), expected, "{row:?}");
        }
    }

    #[test]
    fn hashes_images_as_the_spec_vectors_do() {
        let pattern = |k: usize| -> Vec<u8> {
            (0..4 * 1024 * 1024)
                .map(|i| ((7 * i + k) % 256) as u8)
                .collect()
        };
        let with_header = |fill: u8| {
            let mut file = vec![fill; COPIER_HEADER];
            file.extend(pattern(3));
            file
        };
        let builtin = Catalog::builtin();
        for row in vectors("hashes.tsv") {
            let [case, sha256, outcome] = &row[..] else {
                panic!("bad row {row:?}");
            };
            let file = match case.as_str() {
                "A" => pattern(3),
                "A-copier-zero" => with_header(0),
                "A-copier-ff" => with_header(0xFF),
                "B" => {
                    let mut file = pattern(3);
                    file[2_097_152] = 0;
                    file
                }
                "C" => pattern(4),
                _ => panic!("unknown case {case}"),
            };
            let image = Image::from_file(file).unwrap();
            assert_eq!(image.sha256().to_string(), *sha256, "{case}");
            assert_eq!(outcome, "unsupported");
            assert_eq!(builtin.outcome(&image), Outcome::Unsupported, "{case}");
        }
    }

    #[test]
    fn supports_the_specs_release() {
        let builtin = Catalog::builtin();
        let titles: Vec<_> = builtin.known.iter().map(|k| k.title.as_str()).collect();
        assert_eq!(titles, ["title-1", "title-2"]);
        assert!(
            builtin
                .identify(&Image::new(vec![0; 4 * 1024 * 1024]))
                .unwrap_err()
                .to_string()
                .contains("another release")
        );
    }

    #[test]
    fn debug_hides_the_bytes() {
        assert_eq!(format!("{:?}", Image::new(vec![0xAB; 3])), "Image(3 bytes)");
    }
}
