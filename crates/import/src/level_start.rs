//! Start records (`spec/formats/level-start.md`): where the characters
//! are when a level begins, and a level's restart point.

/// Bytes in a start record.
pub const START_RECORD_SIZE: usize = 4;

/// One point of a level: level x, and height above the level's bottom
/// edge, both whole px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartRecord {
    pub x: u16,
    pub h: u16,
}

impl StartRecord {
    /// The record at the start of `bytes`, or `None` if they're too few.
    pub fn read(bytes: &[u8]) -> Option<StartRecord> {
        let bytes = bytes.get(..START_RECORD_SIZE)?;
        let word = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        Some(StartRecord {
            x: word(0),
            h: word(2),
        })
    }

    /// The level y of the point, for a level whose bottom edge is at
    /// `bottom`.
    pub fn level_y(self, bottom: i64) -> i64 {
        bottom - i64::from(self.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/level-start/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The bank fragment's bytes from its first address, checking each
    /// line's address.
    fn fragment() -> (usize, Vec<u8>) {
        let mut base = None;
        let mut bytes = Vec::new();
        for line in vector("bank.hex").lines().filter(|l| !l.trim().is_empty()) {
            let (address, data) = line.split_once(':').unwrap();
            let address = usize::from_str_radix(address.trim(), 16).unwrap();
            assert_eq!(
                address,
                *base.get_or_insert(address) + bytes.len(),
                "{line}"
            );
            bytes.extend(
                data.split_whitespace()
                    .map(|b| u8::from_str_radix(b, 16).unwrap()),
            );
        }
        (base.unwrap(), bytes)
    }

    #[test]
    fn records_match_the_spec_vectors() {
        // vectors/level-start/README.md: `Yb` = 512.
        const BOTTOM: i64 = 512;
        let (base, bytes) = fragment();
        let rows: Vec<String> = vector("points.tsv")
            .lines()
            .skip(1)
            .map(String::from)
            .collect();
        assert_eq!(rows.len(), 5);
        for row in rows {
            let cells: Vec<&str> = row.split('\t').collect();
            let address = usize::from_str_radix(cells[0], 16).unwrap();
            let record = StartRecord::read(&bytes[address - base..]).unwrap();
            let got = [
                cells[0].to_string(),
                record.x.to_string(),
                record.h.to_string(),
                record.x.to_string(),
                record.level_y(BOTTOM).to_string(),
            ];
            assert_eq!(got, cells[..], "{row}");
        }
    }

    #[test]
    fn a_record_needs_four_bytes() {
        assert_eq!(StartRecord::read(&[1, 0, 2]), None);
        assert_eq!(
            StartRecord::read(&[1, 0, 2, 0, 9]),
            Some(StartRecord { x: 1, h: 2 })
        );
    }
}
