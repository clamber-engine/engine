//! View-limit records (`spec/formats/view-limits.md`): the range the
//! picture can show from left to right, and the vertical limits the view
//! follows (`spec/behavior/view-vertical.md`, "The level's limits"), as a
//! level's camera rules.

use clamber_level::{Area, CameraRule, Fixed, Rect, Region};

/// The view's size in px (`view.md`, `view-vertical.md`): the picture's
/// limits are kept by its left edge and the view's top, and a camera
/// rule's by the whole view.
pub const VIEW_WIDTH: i64 = 256;
pub const VIEW_HEIGHT: i64 = 224;

/// Bytes in a vertical limit entry.
const ENTRY_SIZE: usize = 8;

/// The word that ends the list of entries.
const END: u16 = 0xFFFF;

/// Past any entry's start: where the last entry's region ends, unless
/// the level is wider.
const LAST_END: i64 = 1 << 16;

/// Values from here up aren't covered by the spec.
const UNCOVERED: u16 = 0x8000;

/// One vertical limit: from `start` (level x, whole px) to the next
/// entry's, the view's top is kept from `top` to `bottom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerticalLimit {
    pub start: u16,
    pub top: u16,
    pub bottom: u16,
    /// Bit 0 of `rate`: the view follows at its quick rates.
    pub quick: bool,
}

/// A view-limit record: the picture's left edge is kept from `left` to
/// `right`, and `vertical` has at least one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewLimits {
    pub left: u16,
    pub right: u16,
    pub vertical: Vec<VerticalLimit>,
}

impl ViewLimits {
    /// The record at the start of `bytes`.
    pub fn read(bytes: &[u8]) -> Result<ViewLimits, String> {
        let word = |i: usize| {
            bytes
                .get(i..i + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .ok_or_else(|| "the view-limit record runs past the image's end".to_string())
        };
        let (left, right) = (word(0)?, word(2)?);
        let mut vertical = Vec::new();
        for at in (4..).step_by(ENTRY_SIZE) {
            let start = word(at)?;
            if start == END {
                break;
            }
            vertical.push(VerticalLimit {
                start,
                top: word(at + 2)?,
                bottom: word(at + 4)?,
                quick: word(at + 6)? & 1 == 1,
            });
        }
        let limits = ViewLimits {
            left,
            right,
            vertical,
        };
        limits.check()?;
        Ok(limits)
    }

    /// The format spec's rules, and the values it covers.
    fn check(&self) -> Result<(), String> {
        let values = self.vertical.iter().flat_map(|v| [v.top, v.bottom]);
        if [self.left, self.right]
            .into_iter()
            .chain(values)
            .any(|n| n >= UNCOVERED)
        {
            return Err("a view limit is 32768 or more, which no spec covers".into());
        }
        if self.left >= self.right {
            return Err("the view-limit record's left limit isn't left of its right".into());
        }
        if self.vertical.is_empty() {
            return Err("the view-limit record has no vertical limits".into());
        }
        if self.vertical.windows(2).any(|w| w[0].start >= w[1].start) {
            return Err("the view-limit record's vertical limits aren't in order".into());
        }
        if self.vertical.iter().any(|v| v.top > v.bottom) {
            return Err("a vertical limit's top is below its bottom".into());
        }
        Ok(())
    }

    /// The number of the entry in use for a player at `x` (whole px): the
    /// last whose start is at most `x`, or the first if none is.
    pub fn in_use(&self, x: i64) -> usize {
        self.vertical
            .iter()
            .rposition(|v| i64::from(v.start) <= x)
            .unwrap_or(0)
    }

    /// The level-wide camera rule, for a level `height` px tall, and a
    /// region for each vertical limit after the first. The rule keeps the
    /// view across from `left` to `right` plus its width, and vertically
    /// as the first entry does, which is also in use left of its start.
    /// Each region runs from its entry's start to the next's, or for the
    /// last, on past the level's right end and any start, and the whole
    /// way up and down: the spec's list goes by `x` alone.
    pub fn camera(&self, width: i64, height: i64) -> (CameraRule, Vec<Region>) {
        let rule = |v: &VerticalLimit| CameraRule {
            limits: Some(rect(
                i64::from(self.left),
                i64::from(v.top),
                i64::from(self.right) + VIEW_WIDTH,
                i64::from(v.bottom) + VIEW_HEIGHT,
            )),
            quick: Some(v.quick),
            ..CameraRule::default()
        };
        let ends = self
            .vertical
            .iter()
            .skip(1)
            .map(|v| i64::from(v.start))
            .chain([width.max(LAST_END)]);
        let regions = self
            .vertical
            .iter()
            .zip(ends)
            .skip(1)
            .enumerate()
            .map(|(n, (v, end))| {
                let start = i64::from(v.start);
                Region {
                    name: Some(format!("view-{}", n + 1)),
                    area: Area::Rect(rect(start, -height, end, 2 * height)),
                    water: false,
                    wind: None,
                    darkness: None,
                    physics: None,
                    camera: Some(rule(v)),
                }
            })
            .collect();
        (rule(&self.vertical[0]), regions)
    }
}

/// The rectangle from (`left`, `top`) to (`right`, `bottom`).
fn rect(left: i64, top: i64, right: i64, bottom: i64) -> Rect {
    let px = |n: i64| Fixed::from_int(i32::try_from(n).expect("a level fits in i32"));
    Rect {
        x: px(left),
        y: px(top),
        w: px(right - left),
        h: px(bottom - top),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use clamber_level::{Level, Vec2};

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/view-limits/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The rows of a table after its header, split at tabs.
    fn rows(name: &str) -> Vec<Vec<String>> {
        let rows: Vec<Vec<String>> = vector(name)
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect();
        assert!(!rows.is_empty(), "{name} is empty");
        rows
    }

    /// The record at `address` (hex) in the bank fragment, checking each
    /// line's address.
    fn record(address: &str) -> ViewLimits {
        let mut base = None;
        let mut bytes = Vec::new();
        for line in vector("bank.hex").lines().filter(|l| !l.trim().is_empty()) {
            let (at, data) = line.split_once(':').unwrap();
            let at = usize::from_str_radix(at.trim(), 16).unwrap();
            assert_eq!(at, *base.get_or_insert(at) + bytes.len(), "{line}");
            bytes.extend(
                data.split_whitespace()
                    .map(|b| u8::from_str_radix(b, 16).unwrap()),
            );
        }
        let at = usize::from_str_radix(address, 16).unwrap() - base.unwrap();
        ViewLimits::read(&bytes[at..]).unwrap_or_else(|e| panic!("{address}: {e}"))
    }

    /// A level 6000 by 2000 px with `limits`' camera rules.
    fn level(limits: &ViewLimits) -> Level {
        let src = "format = 1\nsize = [6000, 2000]\nstart = [0, 0]\n";
        let mut level = Level::from_toml_str(src, "t").unwrap();
        (level.camera.rule, level.regions) = limits.camera(6000, 2000);
        level
    }

    #[test]
    fn records_match_the_spec_vectors() {
        for row in rows("records.tsv") {
            let r = record(&row[0]);
            let got = [
                r.left.to_string(),
                r.right.to_string(),
                r.vertical.len().to_string(),
            ];
            assert_eq!(got, row[1..], "{}", row[0]);
        }
    }

    #[test]
    fn entries_match_the_spec_vectors() {
        let all = rows("entries.tsv");
        for row in &all {
            let r = record(&row[0]);
            let v = r.vertical[row[1].parse::<usize>().unwrap()];
            let rate = if v.quick { "quick" } else { "usual" };
            let got = [
                v.start.to_string(),
                v.top.to_string(),
                v.bottom.to_string(),
                rate.into(),
            ];
            assert_eq!(got, row[2..], "{} {}", row[0], row[1]);
        }
        for address in ["4000", "4020", "4030", "4058"] {
            let listed = all.iter().filter(|r| r[0] == address).count();
            assert_eq!(record(address).vertical.len(), listed, "{address}");
        }
    }

    #[test]
    fn the_entry_in_use_matches_the_spec_vectors() {
        for row in rows("in-use.tsv") {
            let r = record(&row[0]);
            let x: i64 = row[1].parse().unwrap();
            let want: usize = row[2].parse().unwrap();
            assert_eq!(r.in_use(x), want, "{} at {x}", row[0]);
            // The level's camera rule there is that entry's, chosen by x
            // alone, above and below the level too.
            let v = r.vertical[want];
            let level = level(&r);
            for y in [-500, 0, 1000, 2500] {
                let rule = level.camera_at(Vec2::ints(x as i32, y));
                let limits = rule.limits.unwrap();
                assert_eq!(limits.y, Fixed::from_int(v.top.into()), "{} at {x}", row[0]);
                assert_eq!(
                    limits.y + limits.h - Fixed::from_int(VIEW_HEIGHT as i32),
                    Fixed::from_int(v.bottom.into()),
                    "{} at {x}",
                    row[0]
                );
                assert_eq!(rule.quick, Some(v.quick), "{} at {x}", row[0]);
            }
        }
    }

    #[test]
    fn the_edge_range_matches_the_spec_vectors() {
        // view.rs keeps the picture's left edge from the level-wide
        // limits' left to their right less the view's width.
        for row in rows("edge.tsv") {
            let r = record(&row[0]);
            let limits = level(&r).camera.rule.limits.unwrap();
            let left = limits.x.floor();
            let right = (limits.x + limits.w).floor() - VIEW_WIDTH;
            let v = Fixed::from_f64(row[1].parse().unwrap()).unwrap();
            let edge = v.floor().clamp(left, right);
            assert_eq!(edge.to_string(), row[2], "{} at {}", row[0], row[1]);
        }
    }

    #[test]
    fn the_spec_example() {
        let bytes = [
            0x40, 0x00, 0xc0, 0x12, //
            0x10, 0x00, 0x20, 0x00, 0xa0, 0x00, 0x00, 0x00, //
            0x00, 0x03, 0x30, 0x00, 0xb0, 0x00, 0x01, 0x00, //
            0xc8, 0x08, 0x00, 0x00, 0x08, 0x01, 0x00, 0x00, //
            0xff, 0xff,
        ];
        let r = ViewLimits::read(&bytes).unwrap();
        assert_eq!((r.left, r.right), (64, 4800));
        assert_eq!(r.vertical.len(), 3);
        assert_eq!(r.in_use(800), 1);
        assert!(r.vertical[1].quick);
        // Without the end word, the record runs out.
        assert!(ViewLimits::read(&bytes[..28]).is_err());
    }

    #[test]
    fn records_outside_the_spec_are_errors() {
        let record = |words: &[u16]| {
            let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
            ViewLimits::read(&bytes)
        };
        assert!(record(&[0, 10, 0, 0, 5, 0, END]).is_ok());
        assert!(record(&[10, 10, 0, 0, 5, 0, END]).is_err(), "A = B");
        assert!(record(&[0, 10, END]).is_err(), "no entries");
        assert!(
            record(&[0, 10, 5, 0, 5, 0, 5, 0, 5, 0, END]).is_err(),
            "same start"
        );
        assert!(
            record(&[0, 10, 0, 6, 5, 0, END]).is_err(),
            "top below bottom"
        );
        assert!(record(&[0, 0x8000, 0, 0, 5, 0, END]).is_err(), "B too big");
    }
}
