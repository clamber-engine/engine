//! Which kind of object a placement reference places, told by
//! fingerprint (`spec/formats/placements.md`, "Kinds"): the SHA-256 of
//! the 64 image bytes at the reference's position in a bank block, looked
//! up in the image's kind fingerprint rows (`spec/formats/locators.md`).

use std::fmt;

use sha2::{Digest, Sha256};
use toml::{Table, Value};

use crate::{BANK_SIZE, Sha256Hash};

/// Bytes a fingerprint is taken over.
pub const FINGERPRINT_SIZE: usize = 64;

/// The kinds of object a reference can place (`placements.md`, "Kind
/// names").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    SmallWalker,
    LargeWalker,
    Thrower,
    /// A ball placed in the level, not thrown.
    Ball,
    Bunch,
    /// The letter with this number, 1 to 4.
    Letter(u8),
    /// An object that can be carried and holds nothing.
    Barrel,
    /// An object that can be carried and holds the missing character.
    Holder,
    EndMarker,
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::SmallWalker,
        Kind::LargeWalker,
        Kind::Thrower,
        Kind::Ball,
        Kind::Bunch,
        Kind::Letter(1),
        Kind::Letter(2),
        Kind::Letter(3),
        Kind::Letter(4),
        Kind::Barrel,
        Kind::Holder,
        Kind::EndMarker,
    ];

    /// The kind's name in the spec, which is also its entities' kind.
    pub fn name(self) -> &'static str {
        match self {
            Kind::SmallWalker => "small-walker",
            Kind::LargeWalker => "large-walker",
            Kind::Thrower => "thrower",
            Kind::Ball => "ball",
            Kind::Bunch => "bunch",
            Kind::Letter(1) => "letter-1",
            Kind::Letter(2) => "letter-2",
            Kind::Letter(3) => "letter-3",
            Kind::Letter(_) => "letter-4",
            Kind::Barrel => "barrel",
            Kind::Holder => "holder",
            Kind::EndMarker => "end-marker",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The type and params of the entity that places an object of this
    /// kind: the built-in archetype (`content/base/archetypes.toml`) with
    /// the behavior the kind's behavior file describes, with title-1's
    /// values from `spec/behavior/` where the archetype's own differ. What
    /// the spec leaves to a definition (a walker's heading and its wait
    /// before its first step, a placed ball's speeds) keeps the
    /// archetype's values.
    pub fn entity(self) -> (&'static str, Table) {
        let mut params = Table::new();
        let mut set = |key: &str, value: Value| {
            params.insert(key.into(), value);
        };
        let kind = match self {
            Kind::SmallWalker | Kind::LargeWalker => {
                // walker.md: full speed 1, closing an eighth of the gap
                // each frame, turning 4 frames late; bounce.md's fall.
                // The sizes are the ones the classic-1 preset's bounce
                // and hurt boxes give walker-contact.md's ranges with.
                let size = match self {
                    Kind::SmallWalker => [20, 12],
                    _ => [26, 38],
                };
                set("speed", Value::Integer(1));
                set("response", Value::Float(0.125));
                set("size", Value::Array(size.map(Value::Integer).into()));
                set("offset", Value::Float(0.5));
                set("turn_delay", Value::Integer(4));
                set("defeat_jump", Value::Integer(6));
                set("defeat_gravity", Value::Float(0.4375));
                set("defeat_max_fall", Value::Integer(8));
                WALKER
            }
            Kind::Thrower => "thrower",
            Kind::Ball => "ball",
            Kind::Bunch => "cluster",
            Kind::Letter(n) => {
                set("letter", Value::Integer(n.into()));
                "letter"
            }
            Kind::Barrel => "crate",
            Kind::Holder => {
                set("holds_character", Value::Boolean(true));
                "crate"
            }
            Kind::EndMarker => END,
        };
        (kind, params)
    }
}

/// The archetype walkers are, which is the one that takes a `group`.
pub const WALKER: &str = "crawler";

/// The kind of entity that marks the end of a level, as in `clamber-sim`.
pub const END: &str = "end";

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A kind fingerprint row: a fingerprint and the kind it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindRow {
    pub fingerprint: Sha256Hash,
    pub kind: Kind,
}

/// The kind fingerprint rows for placement references in the image of
/// `title`, from `spec/formats/locators.md`.
pub fn kind_rows(title: &str) -> Vec<KindRow> {
    let rows: &[(&str, Kind)] = match title {
        "title-1" => &[
            (
                "15ac2312a3f8d89089ee4c658cf1bee3df8b12254e7ebe4d7646b35028b932d6",
                Kind::Letter(1),
            ),
            (
                "180898553bcc5b68269a895a68e20ed50a90d35c3a70a22caadff48695fbdd43",
                Kind::Holder,
            ),
            (
                "3e1f6c2f24fa461bc637adec8e62fe520d4f2a69e7cf8e0d950610235d2e3486",
                Kind::Letter(4),
            ),
            (
                "46a03d0df615a0aff54b9f2a823f9ead4e500b9ecdeb48c2fe2dfe37aa02431f",
                Kind::Letter(2),
            ),
            (
                "4cb625b24ccb2edc0d7487e5ff9aa9de2cd5409110383d64bfa5ff1a97a22170",
                Kind::LargeWalker,
            ),
            (
                "68ec177ef637f9d79142cd826f4de8d5b14836aa0067cee144d2c606eeaf9c52",
                Kind::Letter(3),
            ),
            (
                "705fd77a10893864d58f32885b437d1e741c3b935279dd4648a7c2282548d067",
                Kind::Barrel,
            ),
            (
                "7a0a5f16cbb9153611411aa5c6ed1aa024f8f2af6b87a221073c56f814c33e3d",
                Kind::Thrower,
            ),
            (
                "814362811b6cd4c8fc220ee7fbd04eb626f6837ef7d7e5dc0027700380948338",
                Kind::Bunch,
            ),
            (
                "a18b370140438f25f467370655a5c802ccd40f07be7e5296c81570fd169a9add",
                Kind::SmallWalker,
            ),
            (
                "b0b7540b3c061784c8c319b69b0c7db7ed9c67a84d094cefd7b05fa52dcbe7a4",
                Kind::SmallWalker,
            ),
            (
                "bc061647de9b607ad9eb002282563c85bb821236cf557781087f987a0ba862aa",
                Kind::SmallWalker,
            ),
            (
                "c8337d0771ac9e0feb76d90747123b32fba93ad3fe8fc5db12cd8a1a647b5910",
                Kind::SmallWalker,
            ),
            (
                "e2e5bad069370a387793f29708000e57f265a85fa38b598be9d019dec4259cc9",
                Kind::EndMarker,
            ),
            (
                "f85e5c914a63c73583c210056c33f883bcd8fecee32b42323bb458fb6f174819",
                Kind::Ball,
            ),
        ],
        _ => &[],
    };
    rows.iter()
        .map(|&(hex, kind)| KindRow {
            fingerprint: Sha256Hash::from_hex(hex).unwrap(),
            kind,
        })
        .collect()
}

/// The candidate fingerprint of `reference` for bank block `block` of
/// `image`, or `None` if its 64 bytes run past the image's end.
pub fn candidate(image: &[u8], reference: u16, block: usize) -> Option<Sha256Hash> {
    let start = block * BANK_SIZE + usize::from(reference);
    let bytes = image.get(start..start + FINGERPRINT_SIZE)?;
    Some(Sha256Hash(Sha256::digest(bytes).into()))
}

/// The kind `reference` places in `image`, with the bank block whose
/// candidate matched one of `rows`, or `None` if none does.
pub fn kind_of(image: &[u8], reference: u16, rows: &[KindRow]) -> Option<(usize, Kind)> {
    let blocks = image.len().div_ceil(BANK_SIZE);
    (0..blocks).find_map(|block| {
        let fingerprint = candidate(image, reference, block)?;
        let row = rows.iter().find(|r| r.fingerprint == fingerprint)?;
        Some((block, row.kind))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/placements/{name}",
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

    /// The vectors' made-up image: 3 bank blocks.
    fn image() -> Vec<u8> {
        let image: Vec<u8> = (0..3 * BANK_SIZE)
            .map(|p| ((37 * p + p / 2048) % 256) as u8)
            .collect();
        assert_eq!(
            Sha256Hash(Sha256::digest(&image).into()).to_string(),
            "a6ba56c90978aa11c57f493b12e84636df5e08f726485164e53537cc69aaa433"
        );
        image
    }

    fn vector_rows() -> Vec<KindRow> {
        rows("kinds-rows.tsv")
            .iter()
            .map(|r| KindRow {
                fingerprint: Sha256Hash::from_hex(&r[0]).unwrap(),
                kind: Kind::from_name(&r[1]).unwrap(),
            })
            .collect()
    }

    fn reference(hex: &str) -> u16 {
        u16::from_str_radix(hex, 16).unwrap()
    }

    #[test]
    fn candidates_match_the_spec_vectors() {
        let image = image();
        for row in rows("kinds-candidates.tsv") {
            let block = row[1].parse().unwrap();
            let got = candidate(&image, reference(&row[0]), block)
                .map_or("none".to_string(), |h| h.to_string());
            assert_eq!(got, row[2], "{} in block {block}", row[0]);
        }
    }

    #[test]
    fn kinds_match_the_spec_vectors() {
        let image = image();
        let kind_rows = vector_rows();
        for row in rows("kinds.tsv") {
            let got = match kind_of(&image, reference(&row[0]), &kind_rows) {
                Some((block, kind)) => [block.to_string(), kind.to_string()],
                None => ["-".to_string(), "none".to_string()],
            };
            assert_eq!(&got[..], &row[1..], "{}", row[0]);
        }
    }

    #[test]
    fn names_go_both_ways() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(Kind::from_name("letter-5"), None);
    }

    #[test]
    fn title_1_has_the_rows_in_the_spec() {
        let path = format!(
            "{}/../../spec/formats/locators.md",
            env!("CARGO_MANIFEST_DIR")
        );
        let spec = std::fs::read_to_string(&path).unwrap();
        let (_, title) = spec.split_once("## `title-1`").unwrap();
        let (_, table) = title
            .split_once("### Kind fingerprints: placement references")
            .unwrap();
        let want: Vec<KindRow> = table
            .lines()
            .filter_map(|l| {
                let cells: Vec<&str> = l.split('|').map(|c| c.trim().trim_matches('`')).collect();
                Some(KindRow {
                    fingerprint: Sha256Hash::from_hex(cells.get(1)?)?,
                    kind: Kind::from_name(cells.get(2)?).unwrap(),
                })
            })
            .collect();
        assert_eq!(want.len(), 15);
        assert_eq!(kind_rows("title-1"), want);
        assert!(kind_rows("title-2").is_empty());
    }

    #[test]
    fn every_kind_is_a_builtin_archetype_or_the_end_marker() {
        use clamber_level::{Entity, Vec2};
        use clamber_sim::{Archetypes, Behavior};

        assert_eq!(END, clamber_sim::END);
        for kind in Kind::ALL {
            let (name, params) = kind.entity();
            if kind == Kind::EndMarker {
                assert_eq!(name, END);
                continue;
            }
            let entity = Entity {
                kind: name.to_string(),
                id: None,
                at: Vec2::ints(0, 0),
                params,
            };
            let archetype = Archetypes::builtin()
                .resolve(&entity, "test", &format!("{kind}"))
                .unwrap_or_else(|e| panic!("{kind}: {e}"))
                .unwrap_or_else(|| panic!("{kind}: {name} isn't an archetype"));
            let walker = matches!(kind, Kind::SmallWalker | Kind::LargeWalker);
            assert_eq!(archetype.behavior.is_walker(), walker, "{kind}");
            assert_eq!(name == WALKER, walker, "{kind}");
            match (kind, archetype.behavior) {
                (Kind::Letter(n), Behavior::Collect { letter, .. }) => {
                    assert_eq!(letter, n, "{kind}")
                }
                (Kind::Holder, Behavior::Carry { holder, .. }) => assert!(holder.is_some()),
                (Kind::Barrel, Behavior::Carry { holder, .. }) => assert!(holder.is_none()),
                _ => {}
            }
        }
    }
}
