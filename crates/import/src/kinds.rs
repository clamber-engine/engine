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
    /// An exit hidden in a wall, opened by a thrown object
    /// (`wall-exits.md`).
    WallExit,
    /// A crate holding an animal.
    AnimalCrate,
    /// The restart barrel, which marks the level's restart point.
    RestartBarrel,
    /// The place the characters come out of when play comes back to the
    /// level from its room (`level-start.md`).
    RoomReturnEntrance,
    /// The place the lead comes out of when play comes back to the level
    /// from the area behind its first wall exit (`wall-exits.md`).
    WallReturnEntrance,
    /// A barrel that catches the player and shoots them to the left
    /// (`launch-barrel.md`).
    LeftLaunchBarrel,
    /// A barrel that catches the player, shoots them straight down and
    /// breaks.
    DownLaunchBarrel,
    Stinger,
    /// A tyre that throws the player up.
    Tyre,
    /// A tyre that throws the player up and never squashes (`tyre.md`,
    /// "The stiff tyre").
    StiffTyre,
    /// A sign: scenery that nothing touches (`signs.md`).
    Sign,
    /// A small flying creature: scenery that nothing touches
    /// (`critter.md`).
    Critter,
    /// An extra-life item giving this many lives, 1 or 3, that stays
    /// where it's placed (`extra-lives.md`).
    ExtraLife(u8),
    /// An extra-life item giving this many lives, 1 or 2, that rises
    /// once it appears.
    RisingExtraLife(u8),
    /// A hidden object the ground action strikes, releasing a bunch.
    HiddenBunch,
    /// A hidden object a long fall sets off, releasing a barrel.
    HiddenBarrel,
    /// A hidden object a long fall sets off, releasing a metal barrel.
    HiddenMetalBarrel,
}

impl Kind {
    pub const ALL: [Kind; 35] = [
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
        Kind::WallExit,
        Kind::AnimalCrate,
        Kind::RestartBarrel,
        Kind::RoomReturnEntrance,
        Kind::WallReturnEntrance,
        Kind::LeftLaunchBarrel,
        Kind::DownLaunchBarrel,
        Kind::Stinger,
        Kind::Tyre,
        Kind::StiffTyre,
        Kind::Sign,
        Kind::Critter,
        Kind::ExtraLife(1),
        Kind::ExtraLife(3),
        Kind::RisingExtraLife(1),
        Kind::RisingExtraLife(2),
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
            Kind::WallExit => "wall-exit",
            Kind::AnimalCrate => "animal-crate",
            Kind::RestartBarrel => "restart-barrel",
            Kind::RoomReturnEntrance => "room-return-entrance",
            Kind::WallReturnEntrance => "wall-return-entrance",
            Kind::LeftLaunchBarrel => "left-launch-barrel",
            Kind::DownLaunchBarrel => "down-launch-barrel",
            Kind::Stinger => "stinger",
            Kind::Tyre => "tyre",
            Kind::StiffTyre => "stiff-tyre",
            Kind::Sign => "sign",
            Kind::Critter => "critter",
            Kind::ExtraLife(1) => "extra-life-1",
            Kind::ExtraLife(_) => "extra-life-3",
            Kind::RisingExtraLife(1) => "rising-extra-life-1",
            Kind::RisingExtraLife(_) => "rising-extra-life-2",
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

    /// The id the first entity of this kind gets, if it's one that
    /// something else in the level refers to.
    pub fn id(self) -> Option<&'static str> {
        match self {
            Kind::WallReturnEntrance => Some(WALL_RETURN),
            _ => None,
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// [`Kind::entity_with`] with every setting's default.
    pub fn entity(self) -> (&'static str, Table) {
        self.entity_with(&KindSettings::default())
    }

    /// The type and params of the entity that places an object of this
    /// kind with `settings`: the built-in archetype
    /// (`content/base/archetypes.toml`) with the behavior the kind's
    /// behavior file describes, with title-1's values from
    /// `spec/behavior/` where the archetype's own differ, and the
    /// settings (`placements.md`, "Settings") the kind has, each one not
    /// given taking its default.
    pub fn entity_with(self, settings: &KindSettings) -> (&'static str, Table) {
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
                let heading = settings.heading.unwrap_or(Heading::Left);
                set("facing", Value::String(heading.name().into()));
                let range = match settings.patrol.unwrap_or(Patrol::None) {
                    Patrol::None => vec![],
                    Patrol::Between(a, b) => {
                        vec![Value::Integer(a.into()), Value::Integer(b.into())]
                    }
                };
                set("range", Value::Array(range));
                // The walker waits that many frames, counting the one it
                // appears on, before it moves.
                set(
                    "appear_wait",
                    Value::Integer(settings.first_step.unwrap_or(1).into()),
                );
                WALKER
            }
            Kind::Thrower => {
                let throws = settings.throws.as_deref().unwrap_or(&DEFAULT_THROWS);
                let (period, timetable) = timetable(throws);
                let timetable = timetable
                    .into_iter()
                    .map(|(frame, vx)| {
                        Value::Array(vec![
                            Value::Integer(frame.into()),
                            Value::Float(f64::from(vx) / 256.0),
                            Value::Integer(THROW_VY),
                        ])
                    })
                    .collect();
                set("period", Value::Integer(period.into()));
                set("throws", Value::Array(timetable));
                "thrower"
            }
            Kind::Ball => {
                let speed = |given: Option<i32>, default| {
                    Value::Float(f64::from(given.unwrap_or(default)) / 256.0)
                };
                set(
                    "velocity",
                    Value::Array(vec![
                        speed(settings.start_vx, DEFAULT_START_VX),
                        speed(settings.start_vy, DEFAULT_START_VY),
                    ]),
                );
                "ball"
            }
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
            // wall-exits.md: placed and not drawn, and closed until a
            // thrown barrel opens it. Its values are the preset's
            // (`wall_exit_*`, and `exit_*` for the rest).
            Kind::WallExit => WALL_EXIT,
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
            // wall-exits.md, "Coming back from the first wall exit's
            // area": an entrance the characters come out of only when
            // play comes back to the level at its arrival, thrown at
            // (3.75, 2) for 6 frames, then falling as out of the level's
            // entrance, steered from the seventh frame. Entering from the
            // level map it's drawn and does nothing.
            Kind::WallReturnEntrance => {
                set(
                    "throw_speed",
                    Value::Array(vec![Value::Float(3.75), Value::Integer(2)]),
                );
                set("rise", Value::Integer(6));
                set("steer_from", Value::Integer(7));
                ENTRANCE
            }
            // level-start.md, "Coming back from the room": coming in from
            // the level map it does nothing and isn't drawn. The engine
            // doesn't come back from the room through it yet (#217).
            Kind::RoomReturnEntrance => ROOM_ENTRANCE,
            // launch-barrel.md: the one that shoots left has the
            // archetype's own values. The one that shoots down holds
            // for 28 frames, shoots at the same speed down for a frame
            // before the flight, breaks, and is never ready again.
            Kind::LeftLaunchBarrel => SLING,
            Kind::DownLaunchBarrel => {
                set("hold", Value::Integer(28));
                set(
                    "shot",
                    Value::Array(vec![Value::Integer(0), Value::Float(2167.0 / 256.0)]),
                );
                set("glide", Value::Integer(1));
                set("breaks", Value::Boolean(true));
                SLING
            }
            // tyre.md's values are the archetype's own. The first level
            // has none.
            Kind::Tyre => "spring",
            // tyre.md, "The stiff tyre": its top stays 15 up through the
            // catch and after a throw, over runs as long as the tyre's,
            // with its own touch and throw ranges.
            Kind::StiffTyre => {
                let ints =
                    |ns: &[i64]| Value::Array(ns.iter().map(|&n| Value::Integer(n)).collect());
                set("top", Value::Integer(15));
                set("squash", Value::Array(vec![ints(&[15, 14])]));
                set("wobble", Value::Array(vec![ints(&[15, 32])]));
                set(
                    "across",
                    Value::Array(vec![ints(&[-24, 24]), ints(&[-26, 22])]),
                );
                set("throw_up", ints(&[-51, 18]));
                "spring"
            }
            // signs.md: it has no values of its own. How it's drawn isn't
            // covered, so both of the first level's signs look the same.
            Kind::Sign => "signpost",
            // critter.md: it does nothing to play, and how it flies and
            // where it appears aren't covered, so it's placed as an
            // entity that nothing acts on and that isn't drawn.
            Kind::Critter => CRITTER,
            // extra-lives.md: the ranges are the archetype's own. The
            // rising kinds speed up by 1/256 px a frame each frame.
            Kind::ExtraLife(lives) => {
                set("lives", Value::Integer(lives.into()));
                "heart"
            }
            Kind::RisingExtraLife(lives) => {
                set("lives", Value::Integer(lives.into()));
                set("rise", Value::Float(1.0 / 256.0));
                "heart"
            }
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

/// The kind of entity a wall exit is: no archetype's.
pub const WALL_EXIT: &str = "wall-exit";

/// The id of the first wall-return entrance's entity, which the arrival
/// coming back from the area behind the first wall exit names.
pub const WALL_RETURN: &str = "wall-return";

/// The kind of entity a room-return entrance is: no archetype's.
pub const ROOM_ENTRANCE: &str = "room-entrance";

/// The archetype launch barrels are.
pub const SLING: &str = "sling";

/// The kind of entity a critter is: no archetype's.
pub const CRITTER: &str = "critter";

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
                "10c0e2b03ca2e4bd4b19e904df4a206746f1a8068c58b18485b70c2ffbee8a86",
                Kind::Thrower,
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
                "26aa21923dc163f0796ba72d954e879c23f92cf083e3cb766cb697fa3a8ada1e",
                Kind::Critter,
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
                "33ba1408651608e5bba77ae5a3232bc7fe0ea4a63c9950bb493bb07bed985c21",
                Kind::RisingExtraLife(2),
            ),
            (
                "3b1534a2da04c2655ee410d9570846bf8da60ecabf7afa758b2e5cab13c547a0",
                Kind::Stinger,
            ),
            (
                "3bd63880d0c8f6e81a4975e1549c730655b1f1e7957454c0ce6ea980044a1718",
                Kind::DownLaunchBarrel,
            ),
            (
                "3d6edece3440402e518d12b9b530167bc00a8ea4c35137f0ffb9414dcbce3c2c",
                Kind::Thrower,
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
                "406919fff90e945a33f5cc43d8ac30037ff4c393cfa80eaba828d5955f523f63",
                Kind::Thrower,
            ),
            (
                "46a03d0df615a0aff54b9f2a823f9ead4e500b9ecdeb48c2fe2dfe37aa02431f",
                Kind::Letter(2),
            ),
            (
                "4799c125f8ce77f8cbcd9562a1c126036b80c0930f94ffe5c5b273a5ee6af2cc",
                Kind::Thrower,
            ),
            (
                "47a9fc70cf6be73792670830d89523401fe75b8ad5ade692f5fe8d534962429c",
                Kind::Sign,
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
                "558635a3657029cd26bc9e52c8c3a2d79fe81ebbad040fff194828098afe61a5",
                Kind::LeftLaunchBarrel,
            ),
            (
                "5624aa1b5f08527d0ddab7f1d5fc72f4fc0559b083ce1cd80be4baf05eac1997",
                Kind::WallReturnEntrance,
            ),
            (
                "5894d96cb527c55642142ea263c695df5028dc704583a4d0b29e20a2f3c6d895",
                Kind::Thrower,
            ),
            (
                "5d9b10becfc0595ae047545480de275256a32d63feb76eee0f71343b59ba4acf",
                Kind::Thrower,
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
                "72bc1c2e265a914fcfce4b9fd28df1243aa0c404bc95d3e185e030072236a427",
                Kind::Thrower,
            ),
            (
                "765a7a9517d401764d141421814606b3c711a924352de8ea10d9878433fe40de",
                Kind::Stinger,
            ),
            (
                "774c17b7d70e72c9f259f4b1cf4bac552f0a9ee7ed3fa62276229e5537da3b43",
                Kind::Thrower,
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
                "858bd5ab1c8e2b25e0d23f36da439e91cc09f4578483506115836f355c507396",
                Kind::WallExit,
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
                "9448b2e77aec028a49942a6a03cc58479acad0fdb0ee19ca26d46eabe8512402",
                Kind::Thrower,
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
                "c3f159c77315eec82d09dda32bd1ff7a989fa181b9e5c3fbdf127b031f6ef15a",
                Kind::RisingExtraLife(1),
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
                "d3c74b9f836d76f4cf056376f253b9e2810e44323ff3fabbf0415ef3b99cbfc2",
                Kind::WallExit,
            ),
            (
                "d95f1da6f034e0484756a23152ad6c3ecdc3bf418860bc083e55a33ef6cf6abc",
                Kind::Stinger,
            ),
            (
                "e03617c2b72b0aa7a602ab803484eb8c7a2a2b68340d669cb12de259701c5c19",
                Kind::ExtraLife(1),
            ),
            (
                "e19428b6ac97b12161446b28e017a417298eee371dcc9f412c79cfafdcdebd98",
                Kind::RoomReturnEntrance,
            ),
            (
                "e28fb5597b640eb4e7a70f79a382cf2c62c20eb202c2b56e903119bb6a1c05a3",
                Kind::ExtraLife(3),
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
                "ef9f527e7d99c063327beb2e5ab910b66859e3251df85554202bade9aa887b16",
                Kind::StiffTyre,
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

/// A walker's heading when it appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    Left,
    Right,
}

impl Heading {
    pub fn name(self) -> &'static str {
        match self {
            Heading::Left => "left",
            Heading::Right => "right",
        }
    }
}

/// A walker's patrol range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Patrol {
    /// It turns only where something else turns it.
    None,
    /// It turns at its start plus the first, and its start plus the
    /// second (px, the first no more than 0, the second no less).
    Between(i16, i16),
}

/// One of a thrower's throws: a ball with this `vx`, in 1/256 px per
/// frame, then `wait` frames, 1 or more, to the next throw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Throw {
    pub vx: i32,
    pub wait: u32,
}

/// The settings a kind fingerprint's settings row gives
/// (`placements.md`, "Settings"). A setting that's `None` takes its
/// default, and one the kind doesn't have is never given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KindSettings {
    /// A walker's heading when it appears. Default left.
    pub heading: Option<Heading>,
    /// A walker's patrol range. Default none.
    pub patrol: Option<Patrol>,
    /// The frame of a walker's first step, counted from the one it
    /// appears on: 0 or 1. Default 1.
    pub first_step: Option<u8>,
    /// A placed ball's speeds before its first frame of moves, in 1/256
    /// px per frame. Defaults −300 and −768.
    pub start_vx: Option<i32>,
    pub start_vy: Option<i32>,
    /// A thrower's throws, in order, repeating. Default
    /// [`DEFAULT_THROWS`].
    pub throws: Option<Vec<Throw>>,
}

const DEFAULT_START_VX: i32 = -300;
const DEFAULT_START_VY: i32 = -768;

/// The default throws, the first level's thrower's.
pub const DEFAULT_THROWS: [Throw; 3] = [
    Throw { vx: 0, wait: 31 },
    Throw { vx: 800, wait: 111 },
    Throw { vx: -800, wait: 31 },
];

/// The `vy` every thrown ball starts with (`thrower.md`, "A thrown
/// ball").
const THROW_VY: i64 = -7;

/// The frame of a thrower's first throw, counted from the one it
/// appears on.
const FIRST_THROW: u32 = 2;

/// The engine's timetable for `throws`: its period, the sum of the
/// waits, and the frame of each throw in it, counted from the one the
/// thrower appears on, with its `vx`. A timetable's frames are within its
/// period, so with a period of 2 or less the first throw comes early;
/// the spec's are all longer.
pub fn timetable(throws: &[Throw]) -> (u32, Vec<(u32, i32)>) {
    let period: u32 = throws.iter().map(|t| t.wait).sum();
    let mut frame = FIRST_THROW;
    let frames = throws
        .iter()
        .map(|t| {
            let at = (frame % period, t.vx);
            frame += t.wait;
            at
        })
        .collect();
    (period, frames)
}

/// A settings row: a kind fingerprint and the settings its objects start
/// with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsRow {
    pub fingerprint: Sha256Hash,
    pub settings: KindSettings,
}

/// The settings rows for placement references in the image of `title`,
/// from `spec/formats/locators.md`.
pub fn settings_rows(title: &str) -> Vec<SettingsRow> {
    let row = |hex: &str, settings| SettingsRow {
        fingerprint: Sha256Hash::from_hex(hex).unwrap(),
        settings,
    };
    let walker = |hex, heading, patrol, first_step| {
        row(
            hex,
            KindSettings {
                heading: Some(heading),
                patrol: Some(patrol),
                first_step: Some(first_step),
                ..KindSettings::default()
            },
        )
    };
    let thrower = |hex, throws: &[(i32, u32)]| {
        let throws = throws
            .iter()
            .map(|&(vx, wait)| Throw { vx, wait })
            .collect();
        row(
            hex,
            KindSettings {
                throws: Some(throws),
                ..KindSettings::default()
            },
        )
    };
    use Heading::{Left, Right};
    match title {
        "title-1" => vec![
            walker(
                "4cb625b24ccb2edc0d7487e5ff9aa9de2cd5409110383d64bfa5ff1a97a22170",
                Left,
                Patrol::Between(-128, 128),
                1,
            ),
            walker(
                "a18b370140438f25f467370655a5c802ccd40f07be7e5296c81570fd169a9add",
                Left,
                Patrol::None,
                0,
            ),
            walker(
                "b0b7540b3c061784c8c319b69b0c7db7ed9c67a84d094cefd7b05fa52dcbe7a4",
                Left,
                Patrol::None,
                0,
            ),
            walker(
                "bc061647de9b607ad9eb002282563c85bb821236cf557781087f987a0ba862aa",
                Left,
                Patrol::Between(-144, 144),
                1,
            ),
            walker(
                "c8337d0771ac9e0feb76d90747123b32fba93ad3fe8fc5db12cd8a1a647b5910",
                Right,
                Patrol::Between(-12, 240),
                1,
            ),
            row(
                "f85e5c914a63c73583c210056c33f883bcd8fecee32b42323bb458fb6f174819",
                KindSettings {
                    start_vx: Some(-300),
                    start_vy: Some(-768),
                    ..KindSettings::default()
                },
            ),
            thrower(
                "10c0e2b03ca2e4bd4b19e904df4a206746f1a8068c58b18485b70c2ffbee8a86",
                &[(0, 79)],
            ),
            thrower(
                "3d6edece3440402e518d12b9b530167bc00a8ea4c35137f0ffb9414dcbce3c2c",
                &[(-304, 31), (0, 31), (304, 31), (800, 111), (-800, 31)],
            ),
            thrower(
                "406919fff90e945a33f5cc43d8ac30037ff4c393cfa80eaba828d5955f523f63",
                &[(0, 47)],
            ),
            thrower(
                "4799c125f8ce77f8cbcd9562a1c126036b80c0930f94ffe5c5b273a5ee6af2cc",
                &[(0, 111)],
            ),
            thrower(
                "5894d96cb527c55642142ea263c695df5028dc704583a4d0b29e20a2f3c6d895",
                &[(640, 31), (-640, 31), (0, 31)],
            ),
            thrower(
                "5d9b10becfc0595ae047545480de275256a32d63feb76eee0f71343b59ba4acf",
                &[(800, 111)],
            ),
            thrower(
                "72bc1c2e265a914fcfce4b9fd28df1243aa0c404bc95d3e185e030072236a427",
                &[(0, 31), (640, 79), (-640, 31)],
            ),
            thrower(
                "774c17b7d70e72c9f259f4b1cf4bac552f0a9ee7ed3fa62276229e5537da3b43",
                &[(0, 31), (544, 79), (-544, 31)],
            ),
            thrower(
                "7a0a5f16cbb9153611411aa5c6ed1aa024f8f2af6b87a221073c56f814c33e3d",
                &[(0, 31), (800, 111), (-800, 31)],
            ),
            thrower(
                "9448b2e77aec028a49942a6a03cc58479acad0fdb0ee19ca26d46eabe8512402",
                &[(-800, 31), (0, 31)],
            ),
        ],
        _ => vec![],
    }
}

/// The kind `reference` places in `image`, by `rows`, with the settings
/// its fingerprint's row in `settings` gives, if it has one.
pub fn object_of(
    image: &[u8],
    reference: u16,
    rows: &[KindRow],
    settings: &[SettingsRow],
) -> Option<(Kind, KindSettings)> {
    let (block, kind) = kind_of(image, reference, rows)?;
    let fingerprint = candidate(image, reference, block)?;
    let settings = settings
        .iter()
        .find(|r| r.fingerprint == fingerprint)
        .map(|r| r.settings.clone())
        .unwrap_or_default();
    Some((kind, settings))
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
        let (table, _) = table.split_once("\n### ").unwrap();
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
        assert_eq!(want.len(), 66);
        assert_eq!(kind_rows("title-1"), want);
        assert!(kind_rows("title-2").is_empty());
    }

    /// A vector settings row's fingerprint and kind.
    fn settings_row(row: &[String], settings: KindSettings) -> SettingsRow {
        assert!(Kind::from_name(&row[1]).is_some(), "{}", row[1]);
        SettingsRow {
            fingerprint: Sha256Hash::from_hex(&row[0]).unwrap(),
            settings,
        }
    }

    fn given<T>(cell: &str, parse: impl Fn(&str) -> T) -> Option<T> {
        (cell != "-").then(|| parse(cell))
    }

    fn heading(cell: &str) -> Heading {
        match cell {
            "left" => Heading::Left,
            "right" => Heading::Right,
            _ => panic!("heading {cell}"),
        }
    }

    /// `none` or `a to b`, with `−` or `-` for minus.
    fn patrol(cell: &str) -> Patrol {
        if cell == "none" {
            return Patrol::None;
        }
        let n = |s: &str| s.trim().replace('−', "-").parse().unwrap();
        let (a, b) = cell.split_once(" to ").unwrap();
        Patrol::Between(n(a), n(b))
    }

    /// Throws written `vx:wait`, separated by commas.
    fn throws(cell: &str) -> Vec<Throw> {
        cell.split(',')
            .map(|t| {
                let (vx, wait) = t.split_once(':').unwrap();
                Throw {
                    vx: vx.parse().unwrap(),
                    wait: wait.parse().unwrap(),
                }
            })
            .collect()
    }

    /// A row of `kinds-settings.tsv`, with the kind its reference places
    /// and the params of the entity that places it.
    type VectorObject = (Vec<String>, Option<(Kind, Table)>);

    /// The objects of the vectors' references.
    fn vector_objects(settings: &[SettingsRow]) -> Vec<VectorObject> {
        let image = image();
        let kind_rows = vector_rows();
        rows("kinds-settings.tsv")
            .into_iter()
            .map(|row| {
                let object = object_of(&image, reference(&row[0]), &kind_rows, settings)
                    .map(|(kind, settings)| (kind, kind.entity_with(&settings).1));
                (row, object)
            })
            .collect()
    }

    fn integers(value: &Value) -> Vec<i64> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_integer().unwrap())
            .collect()
    }

    /// A number param, in 1/256 px.
    fn in_256ths(value: &Value) -> i64 {
        let n = value
            .as_float()
            .or(value.as_integer().map(|n| n as f64))
            .unwrap();
        let n = n * 256.0;
        assert_eq!(n.fract(), 0.0);
        n as i64
    }

    #[test]
    fn walker_settings_match_the_spec_vectors() {
        let settings: Vec<SettingsRow> = rows("kinds-settings-rows.tsv")
            .iter()
            .map(|r| {
                settings_row(
                    r,
                    KindSettings {
                        heading: given(&r[2], heading),
                        patrol: given(&r[3], patrol),
                        first_step: given(&r[4], |c| c.parse().unwrap()),
                        ..KindSettings::default()
                    },
                )
            })
            .collect();
        for (row, object) in vector_objects(&settings) {
            let got = match object {
                Some((kind, params)) if params.contains_key("range") => {
                    assert_eq!(kind.name(), row[1]);
                    let range = integers(&params["range"]);
                    [
                        params["facing"].as_str().unwrap().to_string(),
                        match range[..] {
                            [] => "none".to_string(),
                            [a, b] => format!("{a} to {b}"),
                            _ => panic!("{range:?}"),
                        },
                        params["appear_wait"].as_integer().unwrap().to_string(),
                    ]
                }
                _ => ["-", "-", "-"].map(String::from),
            };
            assert_eq!(&got[..], &row[2..], "{}", row[0]);
        }
    }

    #[test]
    fn ball_settings_match_the_spec_vectors() {
        let settings: Vec<SettingsRow> = rows("kinds-ball-settings-rows.tsv")
            .iter()
            .map(|r| {
                settings_row(
                    r,
                    KindSettings {
                        start_vx: given(&r[2], |c| c.parse().unwrap()),
                        start_vy: given(&r[3], |c| c.parse().unwrap()),
                        ..KindSettings::default()
                    },
                )
            })
            .collect();
        let want = rows("kinds-ball-settings.tsv");
        for ((row, object), want) in vector_objects(&settings).into_iter().zip(want) {
            assert_eq!(row[0], want[0]);
            let got = match object {
                Some((Kind::Ball, params)) => {
                    let v = params["velocity"].as_array().unwrap();
                    [in_256ths(&v[0]), in_256ths(&v[1])].map(|n| n.to_string())
                }
                _ => ["-", "-"].map(String::from),
            };
            assert_eq!(&got[..], &want[2..], "{}", want[0]);
        }
    }

    #[test]
    fn thrower_settings_match_the_spec_vectors() {
        let settings: Vec<SettingsRow> = rows("kinds-thrower-settings-rows.tsv")
            .iter()
            .map(|r| {
                settings_row(
                    r,
                    KindSettings {
                        throws: given(&r[2], throws),
                        ..KindSettings::default()
                    },
                )
            })
            .collect();
        let image = image();
        let kind_rows = vector_rows();
        for row in rows("kinds-thrower-settings.tsv") {
            let got = match object_of(&image, reference(&row[0]), &kind_rows, &settings) {
                Some((Kind::Thrower, s)) => {
                    let list = s.throws.as_deref().unwrap_or(&DEFAULT_THROWS);
                    let list: Vec<String> = list
                        .iter()
                        .map(|t| format!("{}:{}", t.vx, t.wait))
                        .collect();
                    // The first seven throws by the engine's timetable.
                    let params = Kind::Thrower.entity_with(&s).1;
                    let period = params["period"].as_integer().unwrap();
                    let timetable = params["throws"].as_array().unwrap();
                    let mut first = Vec::new();
                    for frame in 0.. {
                        for throw in timetable {
                            let throw = throw.as_array().unwrap();
                            if throw[0].as_integer().unwrap() == frame % period {
                                assert_eq!(throw[2].as_integer(), Some(THROW_VY));
                                first.push(format!("{frame}:{}", in_256ths(&throw[1])));
                            }
                        }
                        if first.len() >= 7 {
                            break;
                        }
                    }
                    [list.join(","), first.join(" ")]
                }
                _ => ["-", "-"].map(String::from),
            };
            assert_eq!(&got[..], &row[2..], "{}", row[0]);
        }
    }

    /// The settings tables in title-1's section of `locators.md`.
    #[test]
    fn title_1_has_the_settings_in_the_spec() {
        let path = format!(
            "{}/../../spec/formats/locators.md",
            env!("CARGO_MANIFEST_DIR")
        );
        let spec = std::fs::read_to_string(&path).unwrap();
        let (_, title) = spec.split_once("## `title-1`").unwrap();
        let (title, _) = title.split_once("\n## ").unwrap();
        let (_, tables) = title
            .split_once("### Settings: placement references")
            .unwrap();
        let want: Vec<SettingsRow> = tables
            .lines()
            .filter_map(|l| {
                let cells: Vec<&str> = l.split('|').map(|c| c.trim().trim_matches('`')).collect();
                let fingerprint = Sha256Hash::from_hex(cells.get(1)?)?;
                let kind = Kind::from_name(cells[2]).unwrap();
                let speed = |cell: &str| {
                    let (_, n) = cell.split_once('(').unwrap();
                    let (n, _) = n.split_once("/256").unwrap();
                    n.replace('−', "-").parse().unwrap()
                };
                let settings = match kind {
                    Kind::SmallWalker | Kind::MediumWalker | Kind::LargeWalker => KindSettings {
                        heading: Some(heading(cells[3])),
                        patrol: Some(patrol(&cells[4].replace('`', ""))),
                        first_step: Some(cells[5].parse().unwrap()),
                        ..KindSettings::default()
                    },
                    Kind::Ball => KindSettings {
                        start_vx: Some(speed(cells[3])),
                        start_vy: Some(speed(cells[4])),
                        ..KindSettings::default()
                    },
                    Kind::Thrower => {
                        let list = cells[3]
                            .replace('−', "-")
                            .replace('+', "")
                            .replace(", ", ":")
                            .replace("; ", ",");
                        KindSettings {
                            throws: Some(throws(&list)),
                            ..KindSettings::default()
                        }
                    }
                    _ => panic!("{kind} has no settings"),
                };
                Some(SettingsRow {
                    fingerprint,
                    settings,
                })
            })
            .collect();
        assert_eq!(want.len(), 16);
        assert_eq!(settings_rows("title-1"), want);
        assert!(settings_rows("title-2").is_empty());
        // Every row's fingerprint is a kind row's.
        let kinds = kind_rows("title-1");
        assert!(
            want.iter()
                .all(|s| kinds.iter().any(|k| k.fingerprint == s.fingerprint))
        );
    }

    /// The default throws are the thrower archetype's own timetable.
    #[test]
    fn the_default_throws_are_the_archetypes() {
        let params = Kind::Thrower.entity().1;
        assert_eq!(params["period"].as_integer(), Some(173));
        let frames: Vec<(i64, i64)> = params["throws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let t = t.as_array().unwrap();
                (t[0].as_integer().unwrap(), in_256ths(&t[1]))
            })
            .collect();
        assert_eq!(frames, [(2, 0), (33, 800), (144, -800)]);
    }

    #[test]
    fn every_kind_is_a_builtin_archetype_or_a_flow_kind() {
        use clamber_level::{Entity, Vec2};
        use clamber_sim::{Archetypes, Behavior};

        assert_eq!(END, clamber_sim::END);
        assert_eq!(HIDDEN, clamber_sim::HIDDEN);
        assert_eq!(EXIT, clamber_sim::EXIT);
        assert_eq!(WALL_EXIT, clamber_sim::WALL_EXIT);
        for kind in Kind::ALL {
            let (name, params) = kind.entity();
            if kind == Kind::EndMarker {
                assert_eq!(name, END);
                continue;
            }
            if [
                Kind::Exit,
                Kind::WallExit,
                Kind::RoomReturnEntrance,
                Kind::Critter,
            ]
            .contains(&kind)
            {
                assert!(
                    [EXIT, WALL_EXIT, ROOM_ENTRANCE, CRITTER].contains(&name),
                    "{kind}"
                );
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
                (
                    Kind::ExtraLife(n) | Kind::RisingExtraLife(n),
                    Behavior::Collect {
                        lives, rise, once, ..
                    },
                ) => {
                    assert_eq!(lives, n, "{kind}");
                    assert!(once, "{kind}");
                    let rising = matches!(kind, Kind::RisingExtraLife(_));
                    assert_eq!(rise > clamber_sim::Fixed::ZERO, rising, "{kind}");
                }
                (Kind::ExtraLife(_) | Kind::RisingExtraLife(_), _) => {
                    panic!("{kind} isn't a collectible")
                }
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
                (Kind::StiffTyre, Behavior::Spring(s)) => {
                    assert_eq!((s.top, s.across), (15, [(-24, 24), (-26, 22)]));
                    assert_eq!(s.squash_frames(), 14);
                }
                (Kind::StiffTyre, _) => panic!("{kind} isn't a spring"),
                (Kind::LeftLaunchBarrel, Behavior::Sling(s)) => {
                    assert_eq!((s.hold, s.glide, s.breaks), (15, 23, false));
                    assert_eq!(s.shot.x, -clamber_sim::Fixed::from_bits((2167 << 32) / 256));
                }
                (Kind::DownLaunchBarrel, Behavior::Sling(s)) => {
                    assert_eq!((s.hold, s.glide, s.breaks), (28, 1, true));
                    assert_eq!(
                        s.shot,
                        Vec2::new(
                            clamber_sim::Fixed::ZERO,
                            clamber_sim::Fixed::from_bits((2167 << 32) / 256)
                        )
                    );
                }
                (Kind::LeftLaunchBarrel | Kind::DownLaunchBarrel, _) => {
                    panic!("{kind} isn't a sling")
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
