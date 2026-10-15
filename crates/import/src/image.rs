//! Game images and recognizing them.

use std::fmt;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::ImportError;

/// The largest file read as an image. Anything bigger isn't one.
pub const MAX_IMAGE_SIZE: u64 = 64 * 1024 * 1024;

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
        Ok(Image::new(std::fs::read(path).map_err(io)?))
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

/// The images the importer can recognize.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub known: Vec<Known>,
}

impl Catalog {
    /// The images this build of Clamber supports.
    ///
    /// Empty until the spec says which releases to support and how to
    /// verify them (spec issue #15), so every image is refused for now.
    pub fn builtin() -> Catalog {
        Catalog::default()
    }

    /// Finds the image in the catalog. An image matching none of them
    /// is unsupported, or a supported one that has been changed: the two
    /// can't be told apart.
    pub fn identify(&self, image: &Image) -> Result<&Known, ImportError> {
        if image.is_empty() {
            return Err(ImportError::Unsupported("the file is empty".into()));
        }
        let size = image.len() as u64;
        let hash = image.sha256();
        if let Some(known) = self
            .known
            .iter()
            .find(|k| k.size == size && k.sha256 == hash)
        {
            return Ok(known);
        }
        let reason = if self.known.is_empty() {
            "this version of Clamber can't import any games yet".to_string()
        } else if self.known.iter().any(|k| k.size == size) {
            format!(
                "it has the size of a supported image but not its contents \
                 (SHA-256 {hash}), so it may have been modified or damaged"
            )
        } else {
            format!("it isn't a supported image ({size} bytes, SHA-256 {hash})")
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
        assert!(reason(&Catalog::builtin(), &image).contains("can't import any games yet"));
        let catalog = catalog(&image);
        assert!(reason(&catalog, &Image::new(vec![1, 2, 3, 5])).contains("modified"));
        assert!(reason(&catalog, &Image::new(vec![1, 2, 3])).contains("isn't a supported"));
        assert!(reason(&catalog, &Image::new(Vec::new())).contains("empty"));
    }

    #[test]
    fn debug_hides_the_bytes() {
        assert_eq!(format!("{:?}", Image::new(vec![0xAB; 3])), "Image(3 bytes)");
    }
}
