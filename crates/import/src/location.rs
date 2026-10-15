//! Locations: where format specs say data is in an image
//! (`spec/formats/rom-image.md`, "Banks and locations").

use crate::Image;

/// Bytes in a bank block of an image.
pub const BANK_SIZE: usize = 0x1_0000;

/// A bank number and an address, which name a position in an image, or
/// none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Location {
    pub bank: u8,
    pub address: u16,
}

impl Location {
    /// Bytes a location takes when stored in data.
    pub const STORED_SIZE: usize = 3;

    pub fn new(bank: u8, address: u16) -> Location {
        Location { bank, address }
    }

    /// Reads a stored location: the address, low byte first, then the
    /// bank number.
    pub fn from_stored(bytes: [u8; 3]) -> Location {
        Location::new(bytes[2], u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// `address` in the same bank as this location.
    pub fn in_bank(self, address: u16) -> Location {
        Location::new(self.bank, address)
    }

    /// Where this is in an image of `image_len` bytes. `None` for a bank
    /// number and address that don't name the image, or a position past
    /// its end.
    pub fn position(self, image_len: usize) -> Option<usize> {
        let upper_half = self.address >= 0x8000;
        let named = match self.bank {
            64..=125 | 192..=255 => true,
            0..=63 | 128..=191 => upper_half,
            126 | 127 => false,
        };
        let position = usize::from(self.bank % 64) * BANK_SIZE + usize::from(self.address);
        (named && position < image_len).then_some(position)
    }
}

impl Image {
    /// The image's bytes from `location` to its end, or `None` if the
    /// location isn't in it.
    pub fn at(&self, location: Location) -> Option<&[u8]> {
        Some(&self.bytes()[location.position(self.len())?..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_spec_vectors() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../spec/vectors/rom-image/locations.tsv"
        );
        let vectors = std::fs::read_to_string(path).unwrap();
        let mut rows = 0;
        for line in vectors.lines().skip(1) {
            let [bank, address, position] = line.split('\t').collect::<Vec<_>>()[..] else {
                panic!("bad row {line:?}");
            };
            let location = Location::new(
                u8::from_str_radix(bank, 16).unwrap(),
                u16::from_str_radix(address, 16).unwrap(),
            );
            let expected = match position {
                "none" => None,
                hex => Some(usize::from_str_radix(hex, 16).unwrap()),
            };
            assert_eq!(location.position(0x40_0000), expected, "{line}");
            rows += 1;
        }
        assert!(rows > 0);
    }

    #[test]
    fn reads_stored_locations() {
        let location = Location::from_stored([0x34, 0x92, 0xC5]);
        assert_eq!(location, Location::new(0xC5, 0x9234));
        assert_eq!(location.in_bank(0x8000), Location::new(0xC5, 0x8000));
    }

    #[test]
    fn stops_at_the_end_of_the_image() {
        assert_eq!(
            Location::new(0xC1, 0).position(BANK_SIZE + 1),
            Some(BANK_SIZE)
        );
        assert_eq!(Location::new(0xC1, 1).position(BANK_SIZE + 1), None);
        let image = Image::new((0..=255u8).cycle().take(2 * BANK_SIZE).collect());
        assert_eq!(
            image.at(Location::new(0x41, 0xFFFE)),
            Some(&[0xFE, 0xFF][..])
        );
        assert_eq!(image.at(Location::new(0x42, 0)), None);
        assert_eq!(image.at(Location::new(0x01, 0x10)), None);
    }
}
