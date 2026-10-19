//! Sound data (`spec/formats/sound.md`): samples, decoded into audio
//! sample points with exact loop points, which samples each song has, and
//! each song's block of data.
//!
//! A song's data is played by [`crate::Player`] (`songs.md`); the rate a
//! sample plays at is the pitch a note hands its voice, so
//! [`SampleAudio::ogg`] takes the rate from its caller.

use clamber_audio::encode::Vorbis;

use crate::Location;

/// Bytes in a block of sample data.
pub const BLOCK_SIZE: usize = 9;

/// Steps in a block, and so outputs per block.
pub const STEPS: usize = 16;

/// Ends a list of sample numbers.
pub const END_OF_LIST: u16 = 0xFFFF;

/// Loop passes decoded while looking for a run of passes that repeats.
const MAX_LOOP_PASSES: usize = 64;

/// Gives the bytes from a location to the end of what can be read there,
/// or `None` where there's nothing.
pub trait Read<'a>: Fn(Location) -> Option<&'a [u8]> {}
impl<'a, F: Fn(Location) -> Option<&'a [u8]>> Read<'a> for F {}

fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// The first byte of a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub shift: u8,
    pub filter: u8,
    pub loops: bool,
    pub end: bool,
}

impl BlockHeader {
    pub fn new(byte: u8) -> BlockHeader {
        BlockHeader {
            shift: byte >> 4,
            filter: (byte >> 2) & 3,
            loops: byte & 2 != 0,
            end: byte & 1 != 0,
        }
    }
}

/// The two outputs before the next one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Earlier {
    y1: i16,
    y2: i16,
}

impl Earlier {
    /// Decodes one step `v` (−8 to 7) of a block with `header`.
    fn step(&mut self, v: i32, header: BlockHeader) -> i16 {
        let r = u32::from(header.shift);
        let mut s = match r {
            0..=12 => (v << r) >> 1,
            _ if v < 0 => -2048,
            _ => 0,
        };
        let a = i32::from(self.y1);
        let b = i32::from(self.y2) >> 1;
        // `>>` on a signed number rounds down, as the spec's ⌊ ⌋ does.
        s += match header.filter {
            0 => 0,
            1 => (a >> 1) + (-a >> 5),
            2 => a - b + (b >> 4) + ((-3 * a) >> 6),
            _ => a - b + ((-13 * a) >> 7) + ((3 * b) >> 4),
        };
        let s = s.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        // Wraps 2s into range, as the spec's step 4 does.
        let out = (2 * s) as i16;
        self.y2 = self.y1;
        self.y1 = out;
        out
    }

    /// Decodes `blocks` in order, adding their outputs to `out`.
    pub(crate) fn decode<'b>(
        &mut self,
        blocks: impl IntoIterator<Item = &'b [u8]>,
        out: &mut Vec<i16>,
    ) {
        for block in blocks {
            let header = BlockHeader::new(block[0]);
            for &byte in &block[1..] {
                for nibble in [byte >> 4, byte & 15] {
                    // 8 to 15 are −8 to −1.
                    let v = (i32::from(nibble) ^ 8) - 8;
                    out.push(self.step(v, header));
                }
            }
        }
    }
}

/// A sample record: its loop offset and data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    /// `L`, in bytes of data.
    pub loop_offset: u16,
    pub data: Vec<u8>,
}

impl Sample {
    /// Reads the record that starts at `bytes`.
    pub fn read(bytes: &[u8]) -> Result<Sample, String> {
        if bytes.len() < 4 {
            return Err("a sample record runs past the end of the image".into());
        }
        let length = usize::from(word(bytes, 2));
        let data = bytes
            .get(4..4 + length)
            .ok_or("a sample's data runs past the end of the image")?;
        Ok(Sample {
            loop_offset: word(bytes, 0),
            data: data.to_vec(),
        })
    }

    /// The whole blocks of the data.
    pub fn blocks(&self) -> impl Iterator<Item = &[u8]> {
        self.data.as_chunks::<BLOCK_SIZE>().0.iter().map(|b| &b[..])
    }

    /// Bytes after the last whole block, which aren't part of the sample.
    pub fn left_over(&self) -> usize {
        self.data.len() % BLOCK_SIZE
    }

    /// The first block whose end flag is set.
    pub fn last_block(&self) -> Result<usize, String> {
        self.blocks()
            .position(|b| BlockHeader::new(b[0]).end)
            .ok_or_else(|| "a sample has no block with its end flag set".into())
    }

    /// The block a looping sample loops from, or `None` for a one-shot
    /// sample.
    pub fn loop_block(&self) -> Result<Option<usize>, String> {
        let last = self.last_block()?;
        let header = BlockHeader::new(self.data[last * BLOCK_SIZE]);
        if !header.loops {
            return Ok(None);
        }
        let block = usize::from(self.loop_offset) / BLOCK_SIZE;
        if block > last {
            return Err(format!(
                "a sample loops from block {block}, after its last block {last}"
            ));
        }
        Ok(Some(block))
    }

    /// The first pass, then each loop pass in turn for a looping sample.
    pub fn passes(&self) -> Result<Passes<'_>, String> {
        let last = self.last_block()?;
        Ok(Passes {
            blocks: self.blocks().take(last + 1).collect(),
            loop_block: self.loop_block()?,
            earlier: Earlier::default(),
            started: false,
        })
    }

    /// The sample as audio sample points, with a loop that plays exactly
    /// as the sample does: the first pass, then as many loop passes as it
    /// takes for a run of passes to repeat, looping over that run.
    pub fn audio(&self) -> Result<SampleAudio, String> {
        let mut passes = self.passes()?;
        let first = passes.next().unwrap_or_default();
        let Some(loop_block) = passes.loop_block else {
            return Ok(SampleAudio {
                points: first,
                looping: None,
            });
        };
        let lead_in = STEPS * loop_block;
        // The loop blocks' part of the first pass counts as pass 0.
        let mut seen = vec![first[lead_in..].to_vec()];
        let mut points = first;
        for pass in passes.take(MAX_LOOP_PASSES) {
            if let Some(i) = seen.iter().position(|p| *p == pass) {
                let start = lead_in + seen[..i].iter().map(Vec::len).sum::<usize>();
                return Ok(SampleAudio {
                    looping: Some(Loop {
                        start,
                        length: points.len() - start,
                    }),
                    points,
                });
            }
            points.extend_from_slice(&pass);
            seen.push(pass);
        }
        Err(format!(
            "a sample's loop passes don't repeat within {MAX_LOOP_PASSES} passes"
        ))
    }
}

/// A sample's passes: see [`Sample::passes`].
pub struct Passes<'a> {
    blocks: Vec<&'a [u8]>,
    loop_block: Option<usize>,
    earlier: Earlier,
    started: bool,
}

impl Iterator for Passes<'_> {
    type Item = Vec<i16>;

    fn next(&mut self) -> Option<Vec<i16>> {
        let from = if !self.started {
            self.started = true;
            0
        } else {
            self.loop_block?
        };
        let mut out = Vec::with_capacity(STEPS * (self.blocks.len() - from));
        self.earlier
            .decode(self.blocks[from..].iter().copied(), &mut out);
        Some(out)
    }
}

/// Where a sample's audio loops: from `start` to the end of its points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loop {
    pub start: usize,
    pub length: usize,
}

/// A sample as audio: one point per output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleAudio {
    pub points: Vec<i16>,
    pub looping: Option<Loop>,
}

impl SampleAudio {
    /// A mono Ogg Vorbis file of the points at `rate` points a second,
    /// at Vorbis quality `quality` (-1 to 10). Loop points aren't stored
    /// in it; they go in content metadata, and are points of the file as
    /// they are of [`points`](SampleAudio::points).
    pub fn ogg(&self, rate: u32, quality: f32) -> Result<Vec<u8>, String> {
        let vorbis = Vorbis {
            quality,
            ..Vorbis::default()
        };
        vorbis
            .encode(&self.points, 1, rate)
            .map_err(|e| e.to_string())
    }
}

/// Reads entry `n` of a table of 3-byte locations.
fn table_location<'a>(read: &impl Read<'a>, table: Location, n: u16) -> Result<Location, String> {
    let address = usize::from(table.address) + Location::STORED_SIZE * usize::from(n);
    let entry = u16::try_from(address)
        .ok()
        .and_then(|a| read(table.in_bank(a)))
        .and_then(|b| b.get(..Location::STORED_SIZE))
        .ok_or_else(|| format!("entry {n} of a table runs past the end of its bank"))?;
    Ok(Location::from_stored([entry[0], entry[1], entry[2]]))
}

/// Reads samples 0 to `count` − 1 from the sample table at `table`.
pub fn read_samples<'a>(
    read: impl Read<'a>,
    table: Location,
    count: u16,
) -> Result<Vec<Sample>, String> {
    (0..count)
        .map(|k| {
            let at = table_location(&read, table, k)?;
            let bytes = read(at).ok_or_else(|| format!("sample {k} isn't in the image"))?;
            Sample::read(bytes).map_err(|e| format!("sample {k}: {e}"))
        })
        .collect()
}

/// Reads the list of sample numbers at `at`.
pub fn read_list<'a>(read: impl Read<'a>, at: Location) -> Result<Vec<u16>, String> {
    let bytes = read(at).ok_or("a sample list isn't in the image")?;
    let mut list = Vec::new();
    for &pair in bytes.as_chunks::<2>().0 {
        match u16::from_le_bytes(pair) {
            END_OF_LIST => return Ok(list),
            n => list.push(n),
        }
    }
    Err("a sample list runs past the end of the image without ending".into())
}

/// The list of sample set `set`, from the sample-set table at `table`.
pub fn read_set<'a>(read: impl Read<'a>, table: Location, set: u16) -> Result<Vec<u16>, String> {
    let address = usize::from(table.address) + 2 * usize::from(set);
    let entry = u16::try_from(address)
        .ok()
        .and_then(|a| read(table.in_bank(a)))
        .and_then(|b| b.get(..2))
        .ok_or_else(|| format!("sample set {set} runs past the end of its bank"))?;
    read_list(read, table.in_bank(word(entry, 0))).map_err(|e| format!("sample set {set}: {e}"))
}

/// The samples there while song `song` plays: the shared list at
/// `shared`, then the song's set from the sample-set table at `sets`.
pub fn song_samples<'a>(
    read: impl Read<'a> + Copy,
    shared: Location,
    sets: Location,
    song: u16,
) -> Result<Vec<u16>, String> {
    let mut samples = read_list(read, shared)?;
    samples.extend(read_set(read, sets, song)?);
    Ok(samples)
}

/// A song's block: its data, and the address its first byte has when the
/// data refers to places in itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Song {
    pub base: u16,
    pub data: Vec<u8>,
}

impl Song {
    /// Reads the block that starts at `bytes`.
    pub fn read(bytes: &[u8]) -> Result<Song, String> {
        if bytes.len() < 4 {
            return Err("a song block runs past the end of the image".into());
        }
        let length = usize::from(word(bytes, 0));
        let data = bytes
            .get(4..4 + length)
            .ok_or("a song's data runs past the end of the image")?;
        Ok(Song {
            base: word(bytes, 2),
            data: data.to_vec(),
        })
    }

    /// The song data from address `address` on, if it's in the data.
    pub fn at(&self, address: u16) -> Option<&[u8]> {
        self.data
            .get(usize::from(address.checked_sub(self.base)?)..)
    }
}

/// Reads song `song`'s block, from the song table at `table`.
pub fn read_song<'a>(read: impl Read<'a>, table: Location, song: u16) -> Result<Song, String> {
    let at = table_location(&read, table, song)?;
    let bytes = read(at).ok_or_else(|| format!("song {song} isn't in the image"))?;
    Song::read(bytes).map_err(|e| format!("song {song}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANK: u8 = 0x5B;
    const BASE: u16 = 0x8000;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/sound/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The bank fragment's bytes, checking each line's address.
    fn fragment() -> Vec<u8> {
        let mut bytes = Vec::new();
        for line in vector("bank.hex").lines().filter(|l| !l.trim().is_empty()) {
            let mut fields = line.split_whitespace();
            let address = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
            assert_eq!(address, usize::from(BASE) + bytes.len(), "{line}");
            bytes.extend(fields.map(|b| u8::from_str_radix(b, 16).unwrap()));
        }
        bytes
    }

    fn reader(bytes: &[u8]) -> impl Read<'_> + Copy {
        move |at: Location| {
            if at.bank != BANK {
                return None;
            }
            bytes.get(usize::from(at.address.checked_sub(BASE)?)..)
        }
    }

    fn at(address: u16) -> Location {
        Location::new(BANK, address)
    }

    fn samples(bytes: &[u8]) -> Vec<Sample> {
        read_samples(reader(bytes), at(0x8000), 6).unwrap()
    }

    /// The rows of a table after its header, split at tabs.
    fn rows(name: &str) -> Vec<Vec<String>> {
        let rows: Vec<Vec<String>> = vector(name)
            .lines()
            .skip(1)
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect();
        assert!(!rows.is_empty(), "{name} is empty");
        rows
    }

    fn numbers(text: &str) -> Vec<i16> {
        text.split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect()
    }

    #[test]
    fn samples_match_the_spec_vectors() {
        let bytes = fragment();
        let samples = samples(&bytes);
        let want = rows("samples.tsv");
        assert_eq!(want.len(), samples.len());
        for (k, (sample, row)) in samples.iter().zip(&want).enumerate() {
            let loop_block = sample.loop_block().unwrap();
            let mut passes = sample.passes().unwrap();
            let first = passes.next().unwrap().len().to_string();
            let loop_len = passes.next().map_or("-".into(), |p| p.len().to_string());
            let got = [
                k.to_string(),
                sample.blocks().count().to_string(),
                sample.left_over().to_string(),
                sample.last_block().unwrap().to_string(),
                if loop_block.is_some() { "yes" } else { "no" }.to_string(),
                loop_block.map_or("-".into(), |b| b.to_string()),
                first,
                loop_len,
            ];
            assert_eq!(&got[..], &row[..], "sample {k}");
        }
    }

    #[test]
    fn decoding_matches_the_spec_vectors() {
        let bytes = fragment();
        let samples = samples(&bytes);
        let want = rows("decoded.tsv");
        for (k, sample) in samples.iter().enumerate() {
            let rows: Vec<_> = want.iter().filter(|r| r[0] == k.to_string()).collect();
            assert!(!rows.is_empty(), "no outputs for sample {k}");
            let got: Vec<_> = sample.passes().unwrap().take(rows.len() + 1).collect();
            let passes = rows.len() + usize::from(sample.loop_block().unwrap().is_some());
            assert_eq!(got.len(), passes, "passes of sample {k}");
            for (i, (pass, row)) in got.iter().zip(&rows).enumerate() {
                let name = if i == 0 {
                    "first".into()
                } else {
                    format!("loop{i}")
                };
                assert_eq!(row[1], name);
                assert_eq!(*pass, numbers(&row[2]), "sample {k}, {name}");
            }
        }
    }

    /// Plays `audio` through its loop until it has `len` points.
    fn played(audio: &SampleAudio, len: usize) -> Vec<i16> {
        let mut out = audio.points.clone();
        let Some(Loop { start, length }) = audio.looping else {
            return out;
        };
        while out.len() < len {
            out.extend_from_slice(&audio.points[start..start + length]);
        }
        out.truncate(len);
        out
    }

    #[test]
    fn audio_plays_as_the_sample_does() {
        let bytes = fragment();
        for (k, sample) in samples(&bytes).iter().enumerate() {
            let audio = sample.audio().unwrap();
            let passes = sample
                .passes()
                .unwrap()
                .take(7)
                .collect::<Vec<_>>()
                .concat();
            assert_eq!(played(&audio, passes.len()), passes, "sample {k}");
            if let Some(Loop { start, length }) = audio.looping {
                assert_eq!(start + length, audio.points.len(), "sample {k}");
            }
        }
    }

    #[test]
    fn exact_loops_start_at_the_loop_block() {
        let bytes = fragment();
        let samples = samples(&bytes);
        for (k, start) in [(2, 0), (4, 16)] {
            let audio = samples[k].audio().unwrap();
            let first = samples[k].passes().unwrap().next().unwrap();
            assert_eq!(audio.points, first, "sample {k}");
            assert_eq!(audio.looping.unwrap().start, start, "sample {k}");
        }
        assert_eq!(samples[0].audio().unwrap().looping, None);
    }

    #[test]
    fn a_loop_that_settles_later() {
        // Sample 3's passes all differ at first, so its loop starts past
        // the loop block, after some loop passes.
        let bytes = fragment();
        let audio = samples(&bytes)[3].audio().unwrap();
        let looping = audio.looping.unwrap();
        assert!(looping.start > STEPS * 2);
        assert_eq!(looping.length % 48, 0);
        // A one-shot block ends there.
        let one_shot = Sample {
            loop_offset: 0,
            data: [0x05].into_iter().chain([0x11; 8]).collect(),
        };
        assert_eq!(one_shot.passes().unwrap().count(), 1);
    }

    #[test]
    fn steps_follow_the_spec() {
        let header = |byte| BlockHeader::new(byte);
        let mut e = Earlier::default();
        // Shift 13 to 15: −2048 for negative steps, 0 otherwise.
        assert_eq!(e.step(-1, header(0xD0)), -4096);
        assert_eq!(e.step(7, header(0xF0)), 0);
        // Filter 2 takes s past 32 767; the limit makes it 32 767, which
        // comes out as −2.
        let mut e = Earlier { y1: 32766, y2: 0 };
        assert_eq!(e.step(1, header(0xC8)), -2);
        assert_eq!(
            BlockHeader::new(0xA7),
            BlockHeader {
                shift: 10,
                filter: 1,
                loops: true,
                end: true
            }
        );
    }

    #[test]
    fn song_samples_match_the_spec_vectors() {
        let bytes = fragment();
        for row in rows("song-samples.tsv") {
            let song: u16 = row[0].parse().unwrap();
            let got = song_samples(reader(&bytes), at(0x8040), at(0x8050), song).unwrap();
            let want: Vec<u16> = row[1]
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            assert_eq!(got, want, "song {song}");
        }
    }

    #[test]
    fn songs_match_the_spec_vectors() {
        let bytes = fragment();
        for row in rows("songs.tsv") {
            let n: u16 = row[0].parse().unwrap();
            let song = read_song(reader(&bytes), at(0x8060), n).unwrap();
            let data: String = song.data.iter().map(|b| format!("{b:02x}")).collect();
            let got = [
                n.to_string(),
                format!("{:04x}", song.base),
                song.data.len().to_string(),
                data,
            ];
            assert_eq!(&got[..], &row[..], "song {n}");
            assert_eq!(song.at(song.base), Some(&song.data[..]));
            assert_eq!(song.at(song.base.wrapping_sub(1)), None);
        }
    }

    #[test]
    fn reading_stops_at_the_edges() {
        let bytes = fragment();
        // A list cut before its end number.
        assert!(read_list(reader(&bytes[..0x44]), at(0x8040)).is_err());
        // A record cut inside its data.
        assert!(read_samples(reader(&bytes[..0x150]), at(0x8000), 1).is_err());
        assert!(read_samples(reader(&bytes), at(0x8000), 6).is_ok());
        assert!(read_song(reader(&bytes), at(0x7000), 0).is_err());
        let no_end = Sample {
            loop_offset: 0,
            data: vec![0; 9],
        };
        assert!(no_end.audio().is_err());
        let late_loop = Sample {
            loop_offset: 9,
            data: [0x03].into_iter().chain([0; 8]).collect(),
        };
        assert!(late_loop.loop_block().is_err());
    }

    #[test]
    fn ogg_files_hold_every_point_at_the_rate() {
        let audio = SampleAudio {
            points: (0..500).map(|i| (i * 40) as i16).collect(),
            looping: None,
        };
        let ogg = audio
            .ogg(32000, clamber_audio::encode::DEFAULT_QUALITY)
            .unwrap();
        let input = clamber_audio::Input::bytes(ogg, Some("ogg"));
        let (sound, rate) = clamber_audio::Sound::decode(&input).unwrap();
        assert_eq!((sound.channels, rate, sound.frames()), (1, 32000, 500));
    }
}
