//! Nibble packing (`spec/formats/nibble-packing.md`): a 39-byte header of
//! run bytes, single bytes and stored pairs, then a stream of 4-bit
//! commands that build the unpacked bytes. `title-2`'s level maps are
//! packed this way.

/// Bytes in a packed block's header, before its nibble stream.
pub const NIBBLE_HEADER: usize = 39;

/// Reads a packed block's nibble stream, high nibble of each byte first.
struct Nibbles<'a> {
    bytes: &'a [u8],
    /// Nibbles read so far.
    at: usize,
}

impl Nibbles<'_> {
    fn nibble(&mut self) -> Result<usize, String> {
        let byte = self
            .bytes
            .get(self.at / 2)
            .ok_or("a packed block ends before its end command")?;
        let nibble = if self.at.is_multiple_of(2) {
            byte >> 4
        } else {
            byte & 0x0F
        };
        self.at += 1;
        Ok(usize::from(nibble))
    }

    /// A number of `count` nibbles, the first the most significant.
    fn number(&mut self, count: usize) -> Result<usize, String> {
        (0..count).try_fold(0, |n, _| Ok(16 * n + self.nibble()?))
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.number(2)? as u8)
    }
}

/// Unpacks a nibble-packed block. Bytes after its end command are
/// ignored.
pub fn unpack_nibbles(block: &[u8]) -> Result<Vec<u8>, String> {
    let header = block
        .get(..NIBBLE_HEADER)
        .ok_or("a packed block ends inside its header")?;
    let (run_a, run_b, single_a, single_b) = (header[1], header[2], header[3], header[4]);
    let pair = &header[5..7];
    let stored = |i: usize| &header[7 + 2 * i..9 + 2 * i];
    let mut stream = Nibbles {
        bytes: &block[NIBBLE_HEADER..],
        at: 0,
    };
    let mut out: Vec<u8> = Vec::new();
    // A copy of `k` bytes from `d` back, which can repeat bytes it has
    // just appended.
    let copy = |out: &mut Vec<u8>, k: usize, d: usize| {
        if d == 0 || d > out.len() {
            return Err(format!(
                "a packed block copies from {d} bytes back after {} bytes",
                out.len()
            ));
        }
        for _ in 0..k {
            out.push(out[out.len() - d]);
        }
        Ok(())
    };
    loop {
        match stream.nibble()? {
            0 => match stream.nibble()? {
                0 => return Ok(out),
                n => {
                    for _ in 0..n {
                        out.push(stream.byte()?);
                    }
                }
            },
            1 => out.push(stream.byte()?),
            2 => out.extend([stream.byte()?, stream.byte()?]),
            3 => {
                let n = stream.nibble()?;
                let v = stream.byte()?;
                out.extend(std::iter::repeat_n(v, n + 3));
            }
            4 => out.extend(std::iter::repeat_n(run_a, stream.nibble()? + 3)),
            5 => out.extend(std::iter::repeat_n(run_b, stream.nibble()? + 3)),
            6 => out.extend_from_slice(pair),
            7 => out.push(single_a),
            8 => out.push(single_b),
            9 => {
                let n = stream.nibble()?;
                copy(&mut out, 2, n + 2)?;
            }
            10 => {
                let n = stream.nibble()?;
                let d = usize::from(stream.byte()?);
                copy(&mut out, n + 3, n + 3 + d)?;
            }
            11 => {
                let n = stream.nibble()?;
                let e = stream.number(3)?;
                copy(&mut out, n + 3, e + 259)?;
            }
            12 => {
                let n = stream.nibble()?;
                let d = stream.number(4)?;
                copy(&mut out, n + 3, d)?;
            }
            13 => copy(&mut out, 1, 1)?,
            14 => copy(&mut out, 2, 2)?,
            _ => out.extend_from_slice(stored(stream.nibble()?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        digits
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn vector(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../../spec/vectors/nibble-packing/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        hex(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}")))
    }

    #[test]
    fn unpacks_the_spec_vectors() {
        for case in [
            "each-command",
            "copies",
            "runs-and-pairs",
            "random-a",
            "random-b",
            "random-c",
            "ends-mid-byte",
        ] {
            let packed = vector(&format!("{case}.packed.hex"));
            let unpacked = vector(&format!("{case}.unpacked.hex"));
            assert!(!unpacked.is_empty(), "{case}");
            assert_eq!(unpack_nibbles(&packed), Ok(unpacked), "{case}");
        }
    }

    #[test]
    fn the_synthetic_example_in_the_format() {
        let mut block = vec![0; NIBBLE_HEADER];
        block[3] = 0x11;
        block[5..7].copy_from_slice(&[0x22, 0x33]);
        block[13..15].copy_from_slice(&[0x44, 0x55]);
        block.extend(hex("1A B7 42 6F 3D 9A 2F 10 30 1C 00 00"));
        assert_eq!(
            unpack_nibbles(&block),
            Ok(hex("AB 11 00 00 00 00 00 22 33 44 55 55 AB 11 F1 03 C0"))
        );
        // Bytes after the end are ignored.
        block.extend([0xFF; 4]);
        assert_eq!(unpack_nibbles(&block).map(|o| o.len()), Ok(17));
    }

    #[test]
    fn refuses_blocks_it_cant_unpack() {
        assert!(unpack_nibbles(&[0; NIBBLE_HEADER - 1]).is_err());
        // No end command.
        assert!(unpack_nibbles(&[0; NIBBLE_HEADER]).is_err());
        let with = |stream: &[u8]| {
            let mut block = vec![0; NIBBLE_HEADER];
            block.extend_from_slice(stream);
            unpack_nibbles(&block)
        };
        assert!(with(&[0x1A]).is_err());
        // Copies from before the start, and from distance 0.
        assert!(with(&[0xD0, 0x00]).is_err());
        assert!(with(&[0x1A, 0xBC, 0x00, 0x00, 0x00, 0x00]).is_err());
        assert_eq!(with(&[0x1A, 0xBD, 0x00]), Ok(vec![0xAB, 0xAB]));
    }
}
