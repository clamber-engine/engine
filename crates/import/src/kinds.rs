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
    /// A walker 13 px taller than the small one (`riding-walkers.md`).
    MediumWalker,
    LargeWalker,
    Thrower,
    /// A ball placed in the level, not thrown.
    Ball,
    Bunch,
    /// The letter with this number, 1 to 4.
    Letter(u8),
    /// A golden figure, adding to the figure count (`figure.md`).
    Figure,
    /// An object that can be carried and holds nothing.
    Barrel,
    /// An object that can be carried and holds the missing character.
    Holder,
    EndMarker,
    /// The place the characters come out of when the level begins.
    Entrance,
    /// An exit that takes the lead out of the area (`exits.md`).
    Exit,
    /// A crate holding an animal.
    AnimalCrate,
    /// The restart barrel, which marks the level's restart point.
    RestartBarrel,
    Stinger,
    /// A tyre that throws the player up.
    Tyre,
    /// A sign: scenery that nothing touches (`signs.md`).
    Sign,
    /// A hidden object the ground action strikes, releasing a bunch.
    HiddenBunch,
    /// A hidden object a long fall sets off, releasing a barrel.
    HiddenBarrel,
    /// A hidden object a long fall sets off, releasing a metal barrel.
    HiddenMetalBarrel,
}

impl Kind {
    pub const ALL: [Kind; 24] = [
        Kind::SmallWalker,
        Kind::MediumWalker,
        Kind::LargeWalker,
        Kind::Thrower,
        Kind::Ball,
        Kind::Bunch,
        Kind::Letter(1),
        Kind::Letter(2),
        Kind::Letter(3),
        Kind::Letter(4),
        Kind::Figure,
        Kind::Barrel,
        Kind::Holder,
        Kind::EndMarker,
        Kind::Entrance,
        Kind::Exit,
        Kind::AnimalCrate,
        Kind::RestartBarrel,
        Kind::Stinger,
        Kind::Tyre,
        Kind::Sign,
        Kind::HiddenBunch,
        Kind::HiddenBarrel,
        Kind::HiddenMetalBarrel,
    ];

    /// The kind's name in the spec, which is also its entities' kind.
    pub fn name(self) -> &'static str {
        match self {
            Kind::SmallWalker => "small-walker",
            Kind::MediumWalker => "medium-walker",
            Kind::LargeWalker => "large-walker",
            Kind::Thrower => "thrower",
            Kind::Ball => "ball",
            Kind::Bunch => "bunch",
            Kind::Letter(1) => "letter-1",
            Kind::Letter(2) => "letter-2",
            Kind::Letter(3) => "letter-3",
            Kind::Letter(_) => "letter-4",
            Kind::Figure => "figure",
            Kind::Barrel => "barrel",
            Kind::Holder => "holder",
            Kind::EndMarker => "end-marker",
            Kind::Entrance => "entrance",
            Kind::Exit => "exit",
            Kind::AnimalCrate => "animal-crate",
            Kind::RestartBarrel => "restart-barrel",
            Kind::Stinger => "stinger",
            Kind::Tyre => "tyre",
            Kind::Sign => "sign",
            Kind::HiddenBunch => "hidden-bunch",
            Kind::HiddenBarrel => "hidden-barrel",
            Kind::HiddenMetalBarrel => "hidden-metal-barrel",
        }
    }

    /// How far below its entry's level y an object of this kind's
    /// position is: animal-crate.md puts a crate 1 px below.
    pub fn below_entry(self) -> i32 {
        match self {
            Kind::AnimalCrate => 1,
            _ => 0,
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
            Kind::SmallWalker | Kind::MediumWalker | Kind::LargeWalker => {
                // walker.md: full speed 1, closing an eighth of the gap
                // each frame, turning 4 frames late; bounce.md's fall.
                // The sizes and offsets are the ones the classic-1
                // preset's bounce and hurt boxes give walker-contact.md's
                // ranges with. The medium walker's ranges move 10 px
                // with its heading.
                let (size, offset) = match self {
                    Kind::SmallWalker => ([20, 12], 0.5),
                    Kind::MediumWalker => ([25, 25], 5.0),
                    _ => ([26, 38], 0.5),
                };
                set("rider", self.rider());
                set("speed", Value::Integer(1));
                set("response", Value::Float(0.125));
                set("size", Value::Array(size.map(Value::Integer).into()));
                set("offset", Value::Float(offset));
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
            // figure.md's values are the archetype's own.
            Kind::Figure => "idol",
            Kind::Barrel => "crate",
            Kind::Holder => {
                set("holds_character", Value::Boolean(true));
                "crate"
            }
            Kind::EndMarker => END,
            // level-start.md: where the characters start is the level's
            // start record's point, not this entry's.
            Kind::Entrance => ENTRANCE,
            // exits.md: placed and not drawn. Its values are the
            // preset's (`exit_*`).
            Kind::Exit => EXIT,
            // animal-crate.md's values are the archetype's own.
            Kind::AnimalCrate => "hutch",
            // stinger.md doesn't cover how a level's stingers get their
            // reaches and speeds, so they keep the archetype's: they stay
            // at their start. The first level has none.
            Kind::Stinger => "stinger",
            // restart-barrel.md's values are the archetype's own. The
            // level's restart point is its restart record's, where it has
            // one ([`crate::LevelDecoder`]), and the barrel's position
            // otherwise.
            Kind::RestartBarrel => KEG,
            // tyre.md's values are the archetype's own. The first level
            // has none.
            Kind::Tyre => "spring",
            // signs.md: it has no values of its own. How it's drawn isn't
            // covered, so both of the first level's signs look the same.
            Kind::Sign => "signpost",
            // hidden-objects.md: a hidden bunch's values are the
            // archetype's own.
            Kind::HiddenBunch => HIDDEN,
            // A long fall of 160 px sets these off, by their own ranges,
            // and the item stands where it becomes a barrel: one that
            // goes left, or a metal one that goes right.
            Kind::HiddenBarrel | Kind::HiddenMetalBarrel => {
                let (becomes, across) = match self {
                    Kind::HiddenBarrel => ("crate", -1),
                    _ => ("canister", 1),
                };
                let ints =
                    |ns: &[i64]| Value::Array(ns.iter().map(|&n| Value::Integer(n)).collect());
                set("strike_frames", ints(&[]));
                set("fall_height", Value::Integer(160));
                set(
                    "across",
                    Value::Array(vec![ints(&[-28, 42]), ints(&[-42, 28])]),
                );
                set("above", ints(&[-27, 20]));
                set("becomes", Value::String(becomes.into()));
                set("becomes_at", ints(&[0, 0]));
                set("item_speed", ints(&[across, -6]));
                HIDDEN
            }
        };
        (kind, params)
    }
}

impl Kind {
    /// A walker kind's `rider` table: riding-walkers.md's ranges, for a
    /// rider facing right (the spec's are for one facing left, so each
    /// is turned around). The spec doesn't cover how fast a thrown large
    /// walker moves across; it gives "often, but not always, 1.5 times
    /// the rider's", which is used here.
    fn rider(self) -> Value {
        let (attack, attack_above, knock, knock_above, throw) = match self {
            Kind::SmallWalker => ([[-55, 24], [-56, 23]], 25, [[-18, 26], [-19, 25]], -3, 2.0),
            Kind::MediumWalker => ([[-53, 31], [-63, 21]], 38, [[-16, 33], [-26, 23]], 10, 2.0),
            _ => ([[-58, 27], [-59, 26]], 51, [[-21, 29], [-22, 28]], 23, 1.5),
        };
        let ranges = |r: [[i64; 2]; 2]| {
            Value::Array(
                r.iter()
                    .map(|p| Value::Array(p.iter().map(|&n| Value::Integer(n)).collect()))
                    .collect(),
            )
        };
        let mut table = Table::new();
        table.insert("attack_across".into(), ranges(attack));
        table.insert("attack_above".into(), Value::Integer(attack_above));
        table.insert("knock_across".into(), ranges(knock));
        table.insert("knock_above".into(), Value::Integer(knock_above));
        table.insert("throw_speed".into(), Value::Float(throw));
        Value::Table(table)
    }
}

/// The archetype walkers are, which is the one that takes a `group`.
pub const WALKER: &str = "crawler";

/// The kind of entity that marks the end of a level, as in `clamber-sim`.
pub const END: &str = "end";

/// The archetype a level's entrance is.
pub const ENTRANCE: &str = "entrance";

/// The kind of entity an exit is.
pub const EXIT: &str = "exit";

/// The archetype restart barrels are.
pub const KEG: &str = "keg";

/// The archetype hidden objects are, as in `clamber-sim`.
pub const HIDDEN: &str = "hidden";

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
                "064bbe75bd3abfab4b38a24ad7b839fc30bdfc417023fd381ada77694e617ded",
                Kind::Stinger,
            ),
            (
                "0d36c9a5953a166b2bbfafe60fc65cbe1abd9b66392a33c650daaeb3db1b9fe8",
                Kind::Exit,
            ),
            (
                "0e1f45ccf4bc6e7a48005c73ce0003f0a0d0205fbb063ade26b4f1fe83877da0",
                Kind::Stinger,
            ),
            (
                "12ddb2c3b0a218c0ccb5c1798e7094b5a2b61d21d2361da03b6f6286fa191e0a",
                Kind::RestartBarrel,
            ),
            (
                "15ac2312a3f8d89089ee4c658cf1bee3df8b12254e7ebe4d7646b35028b932d6",
                Kind::Letter(1),
            ),
            (
                "15e5d66f8306cbb5fc446248adeed6c91374c587a91b124a466964d3c2afd580",
                Kind::Stinger,
            ),
            (
                "180898553bcc5b68269a895a68e20ed50a90d35c3a70a22caadff48695fbdd43",
                Kind::Holder,
            ),
            (
                "1dd6e0d8ec2f8e99c24863107071b385b740a4d0fe8f4c5c43f37d52952db345",
                Kind::Stinger,
            ),
            (
                "21186f287296d7b9c4bd4c75f0fbb40c006ecc509ea40745ba84abfa49e97f3b",
                Kind::Stinger,
            ),
            (
                "253004ec8ef2f867d9bacc7dcc6abeb4fa4ae6dc86d98336db33c32e11196e3a",
                Kind::Stinger,
            ),
            (
                "2c3d3e307d2fe5c18345e8308e49c7922732a9ec006ad6cd67900634117da740",
                Kind::AnimalCrate,
            ),
            (
                "334d93d686429e72e3c6a60e23570cd379765c0db2cae288375dc82355f418f7",
                Kind::Tyre,
            ),
            (
                "3b1534a2da04c2655ee410d9570846bf8da60ecabf7afa758b2e5cab13c547a0",
                Kind::Stinger,
            ),
            (
                "3e1f6c2f24fa461bc637adec8e62fe520d4f2a69e7cf8e0d950610235d2e3486",
                Kind::Letter(4),
            ),
            (
                "3f9e1b957059a405568bfd11ba7757786263483c98b2a01bb8e22cc9ae833735",
                Kind::Stinger,
            ),
            (
                "46a03d0df615a0aff54b9f2a823f9ead4e500b9ecdeb48c2fe2dfe37aa02431f",
                Kind::Letter(2),
            ),
            (
                "4c90ae328510dac24fa35424acdaa6b793fcbebfa288220cce41179f518d9978",
                Kind::Tyre,
            ),
            (
                "4cb625b24ccb2edc0d7487e5ff9aa9de2cd5409110383d64bfa5ff1a97a22170",
                Kind::LargeWalker,
            ),
            (
                "5fae0c706628b184dc3d2db99a589d70ce53d54750e685369dd03f73f71e7f06",
                Kind::Stinger,
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
                "765a7a9517d401764d141421814606b3c711a924352de8ea10d9878433fe40de",
                Kind::Stinger,
            ),
            (
                "7a0a5f16cbb9153611411aa5c6ed1aa024f8f2af6b87a221073c56f814c33e3d",
                Kind::Thrower,
            ),
            (
                "7d89c7e6b0041ab373a3e96810740ef610b1e112d06596aef72ff8fa0fe46b41",
                Kind::HiddenBunch,
            ),
            (
                "814362811b6cd4c8fc220ee7fbd04eb626f6837ef7d7e5dc0027700380948338",
                Kind::Bunch,
            ),
            (
                "82dee59bc51d643e34964b5adebf3c9ae39a4fcb4c321a22026922108e32a052",
                Kind::Figure,
            ),
            (
                "8aad3408aa49bd9ceda4b82bd21591b32c7417dd4d4971eba27e30fd47eaf42c",
                Kind::Entrance,
            ),
            (
                "9338801286daea12e1f30fd2478b61a44ef3325da9a893f865e2dff0b43c9cc7",
                Kind::HiddenBarrel,
            ),
            (
                "96815bf167d1e7ae0187cf583b5e9cd05af19646c808ad1b8abb2fe09ad30e52",
                Kind::Stinger,
            ),
            (
                "a18b370140438f25f467370655a5c802ccd40f07be7e5296c81570fd169a9add",
                Kind::MediumWalker,
            ),
            (
                "a3917b33a073e05c714b57e8b4ca93dc514015f068d5d3ae7c8e89e64d7bcf08",
                Kind::Stinger,
            ),
            (
                "a589d75d56211618a7b72120ff23715bf7dbfce7afe68a106e75170cc54be675",
                Kind::EndMarker,
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
                Kind::MediumWalker,
            ),
            (
                "ca241289eec0b66d060f41c4a59b57f87638faf55b0b2b7aa6082d5dafed37af",
                Kind::Exit,
            ),
            (
                "cc9382a3b49fbd2580834652575cf6c0fb7129761622b5cf7261081c549f25e6",
                Kind::HiddenMetalBarrel,
            ),
            (
                "d95f1da6f034e0484756a23152ad6c3ecdc3bf418860bc083e55a33ef6cf6abc",
                Kind::Stinger,
            ),
            (
                "e2e5bad069370a387793f29708000e57f265a85fa38b598be9d019dec4259cc9",
                Kind::EndMarker,
            ),
            (
                "e332f1969a9e6ec35546e8ef037006fd8d1d4bbc9933646879072cc1f3a10009",
                Kind::Sign,
            ),
            (
                "f44d17c1cc692790ebee841de128717e08b078d09eb11b1931a12c05fbe5c81c",
                Kind::Stinger,
            ),
            (
                "f4b1fad5059ecd6ac05c3a59a129e6b47adf0feabddab5c437e06aec2b6214e2",
                Kind::Tyre,
            ),
            (
                "f85e5c914a63c73583c210056c33f883bcd8fecee32b42323bb458fb6f174819",
                Kind::Ball,
            ),
            (
                "fc06b850598f866e68da84373087bd70260e200cd74a79edd39e9ee7612b91e3",
                Kind::Sign,
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
        assert_eq!(want.len(), 44);
        assert_eq!(kind_rows("title-1"), want);
        assert!(kind_rows("title-2").is_empty());
    }

    #[test]
    fn every_kind_is_a_builtin_archetype_or_a_flow_kind() {
        use clamber_level::{Entity, Vec2};
        use clamber_sim::{Archetypes, Behavior};

        assert_eq!(END, clamber_sim::END);
        assert_eq!(HIDDEN, clamber_sim::HIDDEN);
        for kind in Kind::ALL {
            let (name, params) = kind.entity();
            if kind == Kind::EndMarker {
                assert_eq!(name, END);
                continue;
            }
            if kind == Kind::Exit {
                assert_eq!(name, EXIT);
                assert!(!Archetypes::builtin().names().any(|n| n == name));
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
            let walker = matches!(
                kind,
                Kind::SmallWalker | Kind::MediumWalker | Kind::LargeWalker
            );
            assert_eq!(archetype.behavior.is_walker(), walker, "{kind}");
            assert_eq!(name == WALKER, walker, "{kind}");
            match (kind, archetype.behavior) {
                (Kind::Letter(n), Behavior::Collect { letter, .. }) => {
                    assert_eq!(letter, n, "{kind}")
                }
                (Kind::Figure, Behavior::Collect { relic, .. }) => assert!(relic),
                (Kind::Figure, _) => panic!("{kind} isn't a collectible"),
                (Kind::Holder, Behavior::Carry { holder, .. }) => assert!(holder.is_some()),
                (Kind::MediumWalker, Behavior::Walker { rider, .. }) => {
                    assert_eq!(rider.attack_above, 38);
                    assert_eq!(rider.knock_across, [(-16, 33), (-26, 23)]);
                }
                (Kind::Barrel, Behavior::Carry { holder, .. }) => assert!(holder.is_none()),
                (Kind::AnimalCrate, behavior) => {
                    assert!(matches!(behavior, Behavior::Hutch(_)), "{kind}")
                }
                (Kind::Stinger, behavior) => {
                    assert!(matches!(behavior, Behavior::Stinger(_)), "{kind}")
                }
                (Kind::RestartBarrel, behavior) => {
                    assert!(matches!(behavior, Behavior::Keg(_)), "{kind}")
                }
                (Kind::Entrance, behavior) => {
                    assert!(matches!(behavior, Behavior::Entrance(_)), "{kind}")
                }
                (Kind::Tyre, behavior) => {
                    assert!(matches!(behavior, Behavior::Spring(_)), "{kind}")
                }
                (Kind::Sign, behavior) => assert_eq!(behavior, Behavior::Scenery, "{kind}"),
                (Kind::HiddenBunch, Behavior::Hidden(h)) => {
                    assert_eq!((h.fall_height, h.becomes), (0, "cluster"))
                }
                (Kind::HiddenBarrel, Behavior::Hidden(h)) => {
                    assert_eq!((h.fall_height, h.becomes), (160, "crate"));
                    assert_eq!(h.item_speed.x, clamber_sim::Fixed::from_int(-1));
                }
                (Kind::HiddenMetalBarrel, Behavior::Hidden(h)) => {
                    assert_eq!((h.fall_height, h.becomes), (160, "canister"));
                    assert_eq!(h.across, [(-28, 42), (-42, 28)]);
                }
                (Kind::HiddenBunch | Kind::HiddenBarrel | Kind::HiddenMetalBarrel, _) => {
                    panic!("{kind} isn't hidden")
                }
                _ => {}
            }
        }
    }
}
