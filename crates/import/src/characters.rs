//! The characters' pictures for the frames they're held still
//! (`spec/behavior/player-frames.md`), coloured with their own palettes
//! (`spec/formats/sprites.md`, "The characters' palettes"), as a sprite
//! sheet for each character.
//!
//! The pictures are named by their picture hashes, so the importer looks
//! through every full frame for them ([`find_pictures`]). They come in
//! sets that are shown together: a set goes into the pack only if every
//! one of its pictures is found, and the engine shows its own art where a
//! set is missing. A character with no set found has no sheet, and an
//! image whose frames or palettes can't be read gives a pack without any,
//! noted in the import report.
//!
//! [`CHARACTER_SHEET`] followed by the character's roster slot names the
//! sheet (`character-0` for the first character). Each picture is an
//! animation of one sheet frame, named after its label in
//! `player-frames.md`. The sheet's image is in the character's leading
//! palette, and [`FOLLOWING`] names its following palette.

use std::collections::{BTreeMap, HashSet};

use crate::{
    Data, Decoder, FrameTable, Image, Known, Location, Mirror, PackWriter, SPRITES, Sha256Hash,
    SpritePalette, TABLE_ENTRY_SIZE, find_pictures, locate, read_frames, sprite_sheet,
};

/// What a character's sheet is named, before its roster slot.
pub const CHARACTER_SHEET: &str = "character-";

/// The sheet's palette for when the other character is the player.
pub const FOLLOWING: &str = "following";

/// The sheet of the characters' pictures on the level map, drawn with
/// the map's own palette (`spec/formats/sprites.md`, "Sprite palettes").
pub const MAP_SHEET: &str = "map-characters";

/// The name of the picture set that goes in [`MAP_SHEET`].
const MAP_SET: &str = "map";

/// Pictures that are shown together, by label and picture hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureSet {
    /// The set's name: `stop`, `standing`, `jump`, `bounce`, `roll`,
    /// `walking`, `crouch`, `carry`, `scene` or `map`.
    pub name: &'static str,
    pub pictures: &'static [(&'static str, &'static str)],
}

/// A character's picture sets, and where its palettes are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterPictures {
    /// Its roster slot: 0 for the first character, 1 for the second.
    pub slot: usize,
    pub sets: &'static [PictureSet],
    pub leading: Location,
    pub following: Location,
}

/// The first character's sets ("The stop after a roll", "Standing
/// still", "A jump and its landing", "A bounce", "A roll",
/// "Walking, running and turning", "Crouching", "Carrying").
const FIRST: &[PictureSet] = &[
    PictureSet {
        name: "stop",
        pictures: &[
            (
                "roll-end",
                "bfa91eacc23022b5cf106533b4a0aa3baebcb414263d38c2dc0c774d8e1d1171",
            ),
            (
                "stop",
                "3c6fdd12d20a9444c3256170405e94b7d6cf88b1aed7902e3e3aee6c0debf603",
            ),
            (
                "stand-1",
                "004fe43c667649b0d90ef3dea9d56e193e49afdec8f6f6a6568fb56515b744d6",
            ),
        ],
    },
    PictureSet {
        name: "standing",
        pictures: &[
            (
                "stand-1",
                "004fe43c667649b0d90ef3dea9d56e193e49afdec8f6f6a6568fb56515b744d6",
            ),
            (
                "stand-2",
                "a7baafc7dd4f75b6e05a488e2f470b1d3306027968a76a15d4d3bf0dab53b6ca",
            ),
            (
                "stand-3",
                "7554e0b375e04bd09455c65158b43249d99c5b42fccb05b5a8451f67c3f4629f",
            ),
            (
                "stand-4",
                "b4f328ec176347345a78df23786d682cfc3ee69591f781a1ecb5d48ee6d6642b",
            ),
            (
                "stand-5",
                "f84cb3cd83cf8306840277badc6689d7df0f7f19f899406fadc77263067ff1ac",
            ),
            (
                "stand-6",
                "c5d56630ccac5228f720b5ee2c1e45d189980524ca24189b0bfe251cbb3fcd85",
            ),
            (
                "stand-7",
                "4462f15a57c31e2699a69d27711752dc610205591e2aba601295cf23c52d9f02",
            ),
            (
                "stand-8",
                "37b432ca85ca8a36a8d1cbf3874610981be57f5bba5fabbf0d75412d2d8ec4ab",
            ),
            (
                "stand-9",
                "b24d5ec4033de4bd48f1cf21f22c83d6c83f7d49e4bfc5d8c1cfb657cd779c18",
            ),
            (
                "stand-10",
                "6db11fb56e4b21ec660bca54140e50038126555b0c4761b2a8e1998bd0154d22",
            ),
            (
                "stand-11",
                "753d2ddb1a7487755e95562a7fe7551db0be7043e51daa82bee9eb37e7922614",
            ),
            (
                "wait-1",
                "6a4c936d6f0b2dd8bd7f522f1b4b3d892ebdb2e9d27232c1d2ddabf6152b8bb4",
            ),
            (
                "wait-2",
                "c96b75957ba44a2bacebc293a8910bf5b91eadac5bf4d599de9f80e04cc1955e",
            ),
            (
                "wait-3",
                "0fccbe0b757d6f99c7445b227084301d067dae122f652369ef868b8c0df255f5",
            ),
            (
                "wait-4",
                "26d4c63fef34ebd9ecd5a947b8d1a66ce138236a18c2972631bddc99e3dbfc3c",
            ),
            (
                "wait-5",
                "b1cf9fd1c9b5624ef0655fa18a3fec9d05579693f7c98d34b03e99575846a522",
            ),
            (
                "wait-6",
                "0f82a2987d2546d2aeec4395ae16c6f2dfaa75cc0efd396b4e5244dd5e32d17e",
            ),
            (
                "wait-7",
                "079ec658914c590c58b7d4fd0bcecc3e18f037cfcb23df4daca09bbe151db053",
            ),
            (
                "wait-8",
                "05a84fce5d78854fd460cb1049cd1b0f761b42f4001c84f80fd0cdba17dcf897",
            ),
            (
                "wait-9",
                "8997712a38008de9c9dae9fcf12121c116cd77ede7bfa0bbb8068ed6a9a35fb8",
            ),
            (
                "wait-10",
                "13f1d9d779179d7337d0df0c217a5da63cc80bac66f71602b11924e7d3de11ec",
            ),
            (
                "wait-11",
                "92591b89d1769e00446b6ab4a507c72d57a41f7f0da984859f3be2f12066f091",
            ),
            (
                "wait-12",
                "35bfa42e2d7520717c40e65ba62b1991b8942e67427b092933719a89998bf30a",
            ),
            (
                "wait-13",
                "6fdc3af7ece4a2ccaf831827568f9ffe49f40b4f5fad7034e4acf9efaac1e53d",
            ),
            (
                "wait-14",
                "c0f4bcd65b6b55ca2bf39043ecdf0daaf4683dd383b6a70ea90dcd182b5f45d5",
            ),
            (
                "wait-15",
                "950fe6d51e500a6688632c61c3413713645262f1e28bb5ca5af501f05481831e",
            ),
            (
                "wait-16",
                "1bf3eddf1f37effc2e1eccdd9926b989d8eb31fc91345965cfcea6ff9467b709",
            ),
            (
                "wait-17",
                "c918abe2a2ca8e70cd88d9bb5ea9338763d9b33e56383fbcb9e8c72d1079806d",
            ),
            (
                "wait-18",
                "bc6281921a826fc035ae5abe4e9e29c18850c06c7e17b18e98e8d4cb5f2405cf",
            ),
            (
                "wait-19",
                "723e95a3fe870c41bb0c777b32d85797ffd7e6556f16e6e2bcf63ebdc2fa54c7",
            ),
            (
                "wait-20",
                "f08612123c94528360253f036c68e9dbaf0a5b995c4f513b8f411de024d3f7f8",
            ),
            (
                "wait-21",
                "675c9fafafc4b9fa21fcc23b8fe80ba05247c86e46ee4e86f250cf688d10621e",
            ),
            (
                "wait-22",
                "5c0a21e0c5ac0fb9027bea52ca588f3ccb4fbc32d93a1c863ed17d05caf5376f",
            ),
            (
                "wait-23",
                "f82346f38857a53e086ae562a4790bf64d11e739ca87383e120f949d09f3c500",
            ),
            (
                "wait-24",
                "e775bc10f75f931b95cb9795e97b95694edab14d1e5682f4925bf7cfde437ca5",
            ),
            (
                "wait-25",
                "3d930c272c6f949de5d49f07ff92ebeb5cf8eacb7eda0086b3cf06e3e12132b9",
            ),
            (
                "wait-26",
                "513f122bdb17242b473512676d8173dbbe938dc2e74483ec2d7cf37c86de29b0",
            ),
            (
                "wait-27",
                "62e3b8497901dc16150660e926b08961756ac8dcaa55c419d83785a1451c2756",
            ),
            (
                "wait-28",
                "7e73bc97a9700533ab821a1bebd48498511c09e47a617267e7f7d37a479fbd30",
            ),
            (
                "wait-29",
                "3b9b519e510df8cd5cf247bb9e1fad058c8204363fa26e5cd211b65299dc2eb9",
            ),
            (
                "wait-30",
                "acc9897e6cbcc3a7bc2290d4db8e6279972a10d1de9b60fa8d1e4b75a515dcec",
            ),
            (
                "wait-31",
                "ee01f14de3eb2f010b99a3d805c421aa2519dc532fec06b4c3f69d56319364a1",
            ),
            (
                "wait-32",
                "6c17407c7dbb62a0a63132e2c15bd1110aba212074245b2f082d5bdf8ed27a47",
            ),
            (
                "wait-33",
                "dc6e810340ca71af76ef4e3f09b47493cacedab2c22308e1511ec3296ed4fe8f",
            ),
            (
                "wait-34",
                "1545bc738cca010bd6a140d3ed96eb2e4b9ffd9f22175ee3419d5a2180dbe879",
            ),
        ],
    },
    PictureSet {
        name: "teeter",
        pictures: &[
            (
                "teeter-1",
                "f0a9e5b4444f484f6b1dabece744f17cadbb953c28d71d50d4df118a7ee15ada",
            ),
            (
                "teeter-2",
                "ea6ac0e78db695cb54a868d7e6c2e818740c5fa7cd16e9e2116f2aac15694367",
            ),
            (
                "teeter-3",
                "b73c5d039ce5c7ce193a3767d7df073f9b4be0797b310bc0c8e2dae8edcc1875",
            ),
            (
                "teeter-4",
                "de123a4c994344a0d6fcf89da634ad717a562cbaa72fd79c357979c03e6b4df4",
            ),
            (
                "teeter-5",
                "3c7f4bf50621a5ee548699dfada5d3fd05ff405ff1dc64bb2862927274cb736c",
            ),
            (
                "teeter-6",
                "3344a98d306a079af0cd36913a7156cae176766e1a64a66a5eddcc925b698c49",
            ),
            (
                "teeter-7",
                "08d6383bc9c3413313a022882bc1b74859a58ab4418b81aa65b1c0c09fa8eb71",
            ),
            (
                "teeter-8",
                "adbca35b173e87bb0b5b2e047801e64723bbb361c3599e77ec3730bedb402eb9",
            ),
            (
                "teeter-9",
                "064296e774795741ee2840d0adaf2cdd0e7ab5ea912d25e319eb7b342d895c61",
            ),
            (
                "teeter-10",
                "8e8835456b88f58022d08b79fa9197a44651d85ba490351e8feb9cf3b01d4b93",
            ),
            (
                "teeter-11",
                "d204dc85ff4e0e2584683392af1da86d190cd04295b828d6f010beaca5782991",
            ),
            (
                "teeter-12",
                "fe68e744a0d4e5782ab8d2ee25e84ad353fc3a16e86e3bc1b1b0ab9c1c78f7a4",
            ),
            (
                "teeter-13",
                "c470ea00d76066797b045ead44656f361cfd6b1d0201341ce113e16c8836f6f1",
            ),
            (
                "teeter-14",
                "e161fa426d03cd3b5813f27c24a0fb6a84556cfb817670eefb2b38677728c32a",
            ),
            (
                "teeter-15",
                "a6ea8df836706b6719a9a694a1b5863ebcd116ff48988a2f7078bdea8e1f30fd",
            ),
            (
                "teeter-16",
                "3530a447fb28546faaf4d46ae952cbc45bd6b5533f362bdb546a19e8b753125d",
            ),
            (
                "teeter-17",
                "3c71b79018311dcedf4a8b4e86cb366a67ab1a742615d9f5d5b595e2fe013ae4",
            ),
            (
                "teeter-18",
                "c2293a48aaed2ff048ee921d9db866ab44e060ebbf9c5dc3a47671d2fa5c42d5",
            ),
            (
                "teeter-19",
                "f38e2f144cb7018f3ec8e934ce51325a14d3624246c3986cbb9a468b68719f2f",
            ),
            (
                "teeter-20",
                "e4605c757d920f6a924d21b2a4a4f709c2db5f06022f444693bd66d45e3a2401",
            ),
            (
                "teeter-21",
                "97edf1ddd8632c5c838afca3eec8ed44d09a398402e6a2d45676c1c22aedd8fb",
            ),
            (
                "teeter-22",
                "d16004e55ed68d58e337e1a607c0213d9acf19328c5a17585edc9cff17b1291f",
            ),
        ],
    },
    PictureSet {
        name: "jump",
        pictures: &[
            (
                "jump-1",
                "f986d24b9b9e34ad53119824008e8ec440beb5cd0fe8293e4d90c2ec620bb330",
            ),
            (
                "jump-2",
                "5c50e91f1e888be1db77135973e61e8da620353da147d580cd38bdeb1610b054",
            ),
            (
                "rise-1",
                "1b977a6e7cd96246b6482be6acd436314c6e97461ef6f78cbccbfcb47d278959",
            ),
            (
                "rise-2",
                "93fbe24fe835247cd8e2c00435f900087778037d87c73f59097e964fb3be4757",
            ),
            (
                "rise-3",
                "688db3b9e151c4cb15204a24e07faaa8413120619ac344154921e4f0f4bb6649",
            ),
            (
                "rise-4",
                "7bcd4889e9436b91ef51f357d493f865eac62b2dcb89f30a6c646a6051d9bc6a",
            ),
            (
                "rise-5",
                "550a12cbb427b7bee7549b910890d8835e79cf197dcfe13bdff906a7ba9ba614",
            ),
            (
                "rise-6",
                "6ca478f0e8bec5b89623619a0e47cf5591c65780810e21e74811975ae1197abc",
            ),
            (
                "rise-7",
                "58aab5afa82d057d52e938bf3dc92e741e3ddac12cede4e084e1e1e75c377563",
            ),
            (
                "rise-8",
                "76774bd65418cce9ca170556bb817f0cb2edf43d0a32d49ba06bea42f66e87c6",
            ),
            (
                "air-0",
                "4e852e62aefbe279476a27e9c8be69d3004cb0eb510750d6d184f12533e93f15",
            ),
            (
                "air-1",
                "c8717062eeea830c1833e6dbcc40e5e7885e44587159bb87f70821ca99818389",
            ),
            (
                "air-2",
                "f34102792ae5842ff241f6be255feb85e6664152fb0cb5c4e1d65f281ecf973a",
            ),
            (
                "air-3",
                "c21fcae90c91c92d62ac4e0b541d664bb7fed0af97333337aa9eb65ef0bad4c4",
            ),
            (
                "air-4",
                "8bd607b6c7b20ae0ddad9e350af61e8ef7fc3ee6fa4d58abe70eef30f28c0cbe",
            ),
            (
                "air-5",
                "4e50247e9c7daaef441de409c20eb3e3c5b5307899ca3a3d5f3ee27ed6b07bf4",
            ),
            (
                "air-6",
                "3c55e75329625f75afb94b157251a0c1f8ed35aa585e15d70cf20d0450f92798",
            ),
            (
                "land-1",
                "071bb10e9146db4cf64a46de46fbb9e125ad18025f32b16d278a81c53ed48859",
            ),
            (
                "land-2",
                "e9c6583be091b93204aa1f68e4d5bee697d22b659432590e5fbf99b18de835f4",
            ),
            (
                "land-3",
                "ffe44f8dc8abd3bbcb0bbc2c093ff9cb575c76f8f961e555c08918784e7c262a",
            ),
        ],
    },
    PictureSet {
        name: "bounce",
        pictures: &[
            (
                "bounce-1",
                "f7f3875a277149592416208d80a1d24bc68fc3ceea2413bed49103ba6e95aaad",
            ),
            (
                "bounce-2",
                "d7ab095272be3883968fbd0f50465575c56b5fa07c75cdda6498fc6afb23a819",
            ),
            (
                "bounce-3",
                "82b10e9e5284a5d7da3511ece46a3a1052c767a488f058eda5b50b5161f264d6",
            ),
            (
                "bounce-4",
                "2a9654187a97444654379b13ffd68a1dce8360fd8d853e5ae7650c76e250b45e",
            ),
            (
                "bounce-5",
                "573acc69a0425780cb567326d0d78d01790f8028d81d7dab95b80dfb29cc53e8",
            ),
            (
                "bounce-6",
                "dfd4fd3b8ca07a92e673c34cc0a2241d9673315d001f701a5203fab2acb493c8",
            ),
            (
                "bounce-7",
                "a163baa8a900201b09e2822617500831372addfd2ec17c7f2c1fcd18cb8a63c9",
            ),
            (
                "bounce-8",
                "47fdd0574f55acec29b9f0a0375f116fa75c763827ad42409d431e5bfcd900dd",
            ),
            (
                "bounce-9",
                "c0544693ffe6ae05c34743f5b32e5c6cc9af49e19a83f5d3cae03af76e83d29e",
            ),
            (
                "bounce-10",
                "63fae7d24294916456ed1789a492597fe524071d723f2219653efc48855450fe",
            ),
            (
                "bounce-11",
                "0d51a9f292934cb73ad19bd0d3048caf1cbc6bf73f873be6a24f8a76dd1c7026",
            ),
            (
                "bounce-12",
                "7cd131cc14f39d102ab4aea666665020317e54b9994347c3b480fc71c0352055",
            ),
            (
                "bounce-13",
                "a23e743d7f0f325ccc808e58c4531cc5ee498f9448916ac4c4925eecff86d798",
            ),
            (
                "bounce-14",
                "ae93828761146d17e8742278e97029f2bac74e6e486b6081509e8d78afa3856a",
            ),
            (
                "bounce-15",
                "91bff11041ab77dcd73ec276ff502050ffac922ef4cda780eaad4585c4b5473c",
            ),
            (
                "bounce-16",
                "e94fccbe55b920f8bb70ac1f6fab29a70e19de81c274b5c67e265f1718766519",
            ),
        ],
    },
    PictureSet {
        name: "roll",
        pictures: &[
            (
                "roll-1",
                "d1293eefe586b6d7c5a8206301e9732a4b325de17de865693bb031da337f0ad3",
            ),
            (
                "roll-2",
                "22aee199cd0021b2d3aaf0d9a5c94a8ce9b7e2c0cf673a9260f0e57aa82c8507",
            ),
            (
                "roll-3",
                "440441b302e863b5196d6ea694ecbe51741e6cfd1e05b955b858f3cd8b54cf90",
            ),
            (
                "roll-4",
                "2047f847707e77d5cc284d43a5df149c615a53bec99cc9340321a287da1e5352",
            ),
            (
                "roll-5",
                "e0c36ff720d8dcd46f0b558d54e84c3f7e5a1ad038df0a548fba39a54cd7cda8",
            ),
            (
                "roll-6",
                "c1cbbc6557606bbf7e94d694a9b3ed51902ddae2e58c4e53df3de6ae40bc2f30",
            ),
            (
                "roll-7",
                "3ae1e0344b600798af6eb10e4e1a86ddb4a28fd55e3912f8a81b16cf99cd670f",
            ),
            (
                "roll-8",
                "2d7740e2363b34945b5c1a3b8ef8dbde1a8154799757a37a37f63adb0560d0d9",
            ),
            (
                "roll-9",
                "17f2a4a92f37caf7ff88e5412f36496d92b3699ef92f4dac1c9d39d148db4ca1",
            ),
            (
                "roll-end",
                "bfa91eacc23022b5cf106533b4a0aa3baebcb414263d38c2dc0c774d8e1d1171",
            ),
            (
                "stop",
                "3c6fdd12d20a9444c3256170405e94b7d6cf88b1aed7902e3e3aee6c0debf603",
            ),
        ],
    },
    PictureSet {
        name: "walking",
        pictures: &[
            (
                "walk-1",
                "f2a4a8ea5c197c39fe642e16da3e12c7dcfa80b953f8deee62050f06b02aeba1",
            ),
            (
                "walk-2",
                "ead13c4d2f7d70e2ac569b89777ca09fc0d96b86b23e92d69c94658ef6422594",
            ),
            (
                "walk-3",
                "b779e9d3935e5ba3d7b394119f9f745dcfa7fd02ee1fa3d9b10f13619a5c5144",
            ),
            (
                "walk-4",
                "b2d756f5f773372931a834bc87ad462de78bfe7c15e7789978faa9e9976b6321",
            ),
            (
                "walk-5",
                "028edbcc0fbdbfa9c3852bcb514ea55f9c7b19f0bc65de37c01fa29f7fae548f",
            ),
            (
                "walk-6",
                "d52593a0df652f58feeb50582fdb4d59a79264b1b02c32682a01b2f0129692ee",
            ),
            (
                "walk-7",
                "18264b178b8cb8fafef9105eab655b6870e1d8bb162d701f4d7082b1558e607b",
            ),
            (
                "walk-8",
                "d2bdfeebc70818c20983d7c5ce69ad37418ac3811ca09525ca15ba6146954338",
            ),
            (
                "walk-9",
                "226119ba16a39e678c3e604efc6457e672b2758f9f121fd0efc50f9334096e2d",
            ),
            (
                "walk-10",
                "7e855c5bc18ce4d143d64f9d2e79a74b6fba09b439cdebfe8fdb6fd94cee89df",
            ),
            (
                "walk-11",
                "0b5255e8eab4572f97923d3f31a149333dc9090387e099b07fb368a77192cfd4",
            ),
            (
                "walk-12",
                "c68690b3924217273e47d031decb95b0dce199f1668d70afd4814a1a8326d868",
            ),
            (
                "walk-13",
                "df323a8b3ac8ad68b84260e7101f8035de61a5946aec230dcb0941ba4a6517ff",
            ),
            (
                "walk-14",
                "04107531454b0c00fc1b21e5b9ce75530f53f201a1aa8a9c4efa1f75fdfbbf39",
            ),
            (
                "walk-15",
                "b641cb7b596e5329df8d54db4f9f058b0956502579bdb2610a347b46ea5a9399",
            ),
            (
                "walk-16",
                "ad7057061f07a2de77269eefbc01fcd1642a65b52373902d510b074392f06837",
            ),
            (
                "walk-17",
                "5750201a322642bf0325f7c68151971ed2a686e1a4c4ca1cf617b34056535e11",
            ),
            (
                "walk-18",
                "7a50e074c9589e17dd0ce4c18300b7aa30f5cd07b1aee36adec9a1d2f2c907c3",
            ),
            (
                "walk-19",
                "f1c4cfcf5e3b292c4269d1a9b543059ddbdeb5cc0c507ff3bb8ae23823938e35",
            ),
            (
                "walk-20",
                "f9f29b934568b969e6f1cc0a96cac48183701cb32fbb4edb58fc6c84a5032308",
            ),
            (
                "run-1",
                "b6f865d897d7da52ff20a929b661ba160f1e5f85ce7c9bfc560e53f1d71679ba",
            ),
            (
                "run-2",
                "77764f275b81969aa03223b84fa5e6f3f51380bee41a4dc52aa761b0407b9951",
            ),
            (
                "run-3",
                "3033b61264d87a900be073693e5a8b9d29d69b1b46d01b1b699ea1e004ce4105",
            ),
            (
                "run-4",
                "7e6bc8f655b42d82772e41834b58e54f7215578e4249e9f8b4c4cbc389b918b6",
            ),
            (
                "run-5",
                "3dbe3765da062ab5a8cc4393799066a189fff9ba9dd82e825c63ee55f662a4f7",
            ),
            (
                "run-6",
                "33a7cb8129e0abcfcf5e8a27d8f0abe7b58bd6a6e59dd863b8d40c8663861d08",
            ),
            (
                "run-7",
                "b6fa595089aa760cc0d62cd45316b85ad3c0b7669cc485666fd5363246c51cfb",
            ),
            (
                "run-8",
                "7b9bb24e9ecad1da5ec38d882bbfcc21fbb205f2bddb8237fc40ab63582980d0",
            ),
            (
                "run-9",
                "1ad7d04ec267b66d2f0410a66041766cf1258aacdcd858843444d29f8e66b020",
            ),
            (
                "run-10",
                "7ee72d77abb36e1e7aba2a5fbc334204a3b0fa3dc6d7f9f04befe4116b612e66",
            ),
            (
                "run-11",
                "7f7526cf1b159a12e4813cbb8e1b73471d20edab8d1f7a0ccf3b34f98ffc7011",
            ),
            (
                "run-12",
                "98c7c64d18dee901a4f8f7d1520f9d7636a0b644acfbed393f6887c3dbb6e408",
            ),
            (
                "run-13",
                "785c92acb236559c7775f7ddd6bf23a28fa7b5b5720892980bba117a9c516a32",
            ),
            (
                "run-14",
                "84887ec3e2c94a8868e1857af4873ef024fd493cc73ac8ee247da3ae2f6d3fbd",
            ),
            (
                "run-15",
                "a0775ce709377dbd41ce05ceb6d8a953238b07283e0caf475922a7b51b026130",
            ),
            (
                "run-16",
                "53ebca1e61285c486cae0b697a7b090dc15d7ec707a4d3f651e092384dc553eb",
            ),
            (
                "run-17",
                "9929100a5c57b8f3199f338b085e75b7113cb99c8a6f8fcb701b57036c1c0865",
            ),
            (
                "run-18",
                "5a0d9375b560dee536d02908e10a4da1481e75fe7b6bed231312226f5b49931e",
            ),
            (
                "run-19",
                "02074bd88a36837ea9beefa49e9bafc435ac89e4c80c8ca55e7c58831b377224",
            ),
            (
                "run-20",
                "3d52847919d709b7e0e53b155f1334f4f9c132278d820671b92ae0fcab0308b7",
            ),
            (
                "turn-a",
                "2717571d7a62db0b56f4d6b4174d066e8fea0b600304053c62626dbf5df34fcd",
            ),
            (
                "turn-b",
                "8f355022b2be6e6b2bdf5f652729186f57ef82a24c1f4e711a3b55ddc173a9a1",
            ),
        ],
    },
    PictureSet {
        name: "crouch",
        pictures: &[
            (
                "crouch-1",
                "b463402516b70beec1ed9bac32bc635bbe5dfb59af596a8ee95a551d64e4a477",
            ),
            (
                "crouch-2",
                "dd9fc793823c9485409150278ba18cb45fbfca5ae5375ce20d6bc94668252c11",
            ),
            (
                "crouch-3",
                "305ed19c8006d4875b30caf5381cfaac9e5fd1775c2d8fc2d0d8239f6a38155c",
            ),
            (
                "crouch-4",
                "263d6caca9f1d52d6d721e6e8d1125bc5fb5586020f7b3923d73969d06b9b501",
            ),
            (
                "crouch-5",
                "f1a90c2fd870e2df22a68a9eadab38bc4f45b508421185e1ac87eb997b70785d",
            ),
            (
                "crouch-6",
                "9b81f129ce11f64c9f8bdf91faf18b07ae1fd31a6cb39a78be23e323fe47c32d",
            ),
            (
                "crouch-7",
                "9808295f722fd3e78a92be553ece24c47306cfadb29abbf9a741e423221ab659",
            ),
            (
                "crouch-8",
                "d56d6ab0d6e2c7ba4862ba0679422b7998ea6c7aa5fc6fba9bc78c78f76dab9b",
            ),
            (
                "crouch-9",
                "dc4ddf6f773ae8bc87ab64ce4c3fd534d1b8cb733d47a7841a149cdfebe0e698",
            ),
            (
                "crouch-10",
                "c11e24ae51bfec8191aacef63cc6480c8ea379469cd14392bc4ebb99413517ea",
            ),
            (
                "crouch-11",
                "6a737c37bed5006001b8e3d04363329836fea5cea3c5186b2112e367b2e55178",
            ),
            (
                "crouch-12",
                "edc3ea0d338ccb294e4bb8f6fcb7b4671243afbdc1834a2163b0cbcb4f1ef423",
            ),
            (
                "crouch-13",
                "bd8255b5b1b7ccc26e74fd9bf6c1cfc932b4cf0ac7c6abb1fbfe845ad8e56e62",
            ),
            (
                "crouch-14",
                "c34244765304407992061cf5e2e8e0ac7267be840919d600dd7e65fea10ffef5",
            ),
            (
                "crouch-15",
                "e619c72afce8eed5b72087c36f2eba7470d6dddb62605c3725478545df5ebe57",
            ),
            (
                "crouch-16",
                "03a6d3c2abea7edec903acff098570d6997f657e6845065fa97275e26c736721",
            ),
            (
                "crouch-17",
                "19c51ee14af6fa8590358a29c6e15a46d8bf034667274ee910a463e599540743",
            ),
            (
                "crouch-18",
                "e32079c0a52b8fdc34ae74f14cdc719ae12fc08a5ed2687f4fdc9bc753a14e26",
            ),
            (
                "crouch-19",
                "ff4032fc956cf073be57253fed3aa0b325d435869b8ce71ab3aa1175ed8e3ee4",
            ),
            (
                "crouch-20",
                "dec760a482539afb058b936ece72755321b0c23380684e6d4846e5f71b2a892a",
            ),
            (
                "crouch-21",
                "3b814f5a1805cec116cd678ea1fdf2142677c8e6101765866f055d846fe8ba12",
            ),
            (
                "crouch-22",
                "4790466d2a9262931a7444ad7bbcc04ca4035231e399c41cf4adbae63be71383",
            ),
        ],
    },
    PictureSet {
        name: "carry",
        pictures: &[
            (
                "pick-1",
                "cd27f80530f4cb2a5f226bdaac29c57c6a3c74d5112484665c45313938034e1a",
            ),
            (
                "pick-2",
                "d8daa68b6eafc09b0ad90b121b7d3bb059d99a01e48f129ccfd438ae77b97c9e",
            ),
            (
                "pick-3",
                "715efe520a06bdd4adbdc72993c53f73fb98c46df7d80829d9b69e4404c944c2",
            ),
            (
                "pick-4",
                "27278d40b1f231d7306a02543c01d76959d837ecddef98c33e3b2a1a2da0f86a",
            ),
            (
                "pick-5",
                "74cee9f18305ab40df2ace8db45d385ebbd998bbbdf949d5cbbf028d66597601",
            ),
            (
                "pick-6",
                "09ee11cfc8d5193e779fa514a493c9f9b0a7b20844d7d2a4dfb0142a3a126a5a",
            ),
            (
                "pick-7",
                "77b9e4abb255a03282f7ed188b7b0fb61bded08e7422959193e9ba92c770d437",
            ),
            (
                "carry-turn",
                "7ef77cd4e18ba3f55158b9a8242578ed88f057bb0f9ece4afc95a0cd8756db97",
            ),
            (
                "carry-1",
                "b0ed653acac73f77abf68f65ba96c414d967add679c6462238a3506bba25ebc7",
            ),
            (
                "carry-2",
                "2f1eab5ffb597ea4d901ee3870508b4d27ab211e402f3a47cc606b1c5cec391d",
            ),
            (
                "carry-3",
                "cddbf5b7729345e608173755ee8d25509f6a47bd70f28b1c0bdb1d0c05a797bb",
            ),
            (
                "carry-4",
                "905c1777489891decc9af766455670c34c548ee05648e1df0bfa087d07a92ff7",
            ),
            (
                "carry-5",
                "90069cd46001e6a8122de975b4ae4e7a2e9143079a9d8a2272188f67b49474b3",
            ),
            (
                "carry-6",
                "86067ee3150323dd68b69e9312b1a821c3340021350b4d7ac1852d3fb0fe9d26",
            ),
            (
                "carry-7",
                "80a07441a675d3a82488b5283b54d03cd6e7ff166c9c513feb407a302b0d046d",
            ),
            (
                "carry-8",
                "9a72ff96a4ba4bb19873e4168944f0c1fcea52c4154ca5b5145ac17025c07d76",
            ),
            (
                "carry-9",
                "bc8bbe6d54c736057e52518d75c6efc64e02c18bf7ec3ff24a547909ee992a4e",
            ),
            (
                "carry-10",
                "6a80a172a692edb12dc0d08ad26884bb6d32f1df8a9b1a26530f12dc7e1d6703",
            ),
            (
                "throw-1",
                "683a6ac73fb20ab4bc40dd752f9a1a294742b25a2224f55fa5097badd56fb056",
            ),
            (
                "throw-2",
                "551d8134096f7aab059c3e8394dcc700e03de14d91794fbfa141c8bd785d173e",
            ),
            (
                "throw-3",
                "e2e906bb65cf8e30a1b8bab7173d4dc608b2903a1a2793f5c11f895ad1399eed",
            ),
            (
                "throw-4",
                "3bb70a22f2be75ffed750932468a623fefe2c50c76c90014d0bcb5b3c75e6e57",
            ),
            (
                "throw-5",
                "32093e5d1adfc093e86cab6d17c0e7c2e1ed99551e28dd22fb111b20bec347ba",
            ),
            (
                "throw-6",
                "833d9fc67a0d1b29fe68f42d7a30b63b72f61540e6340fb0376deb651a77a664",
            ),
            (
                "throw-7",
                "3d593bbd817afa6d7b48e90f12a1d91370dbb8987aa869341bbe787dadbd8162",
            ),
            (
                "throw-8",
                "5eda4649ba2bb4200d748596e1a277973eb62f6ef1dda3cdef4819f880cd197f",
            ),
            (
                "throw-9",
                "911c9f254332aa63ed8d0df3021686237161bd6c19375ef89cfad185a3f00165",
            ),
            (
                "throw-10",
                "344fa8a91e8c60262f0ff62b90b7d1dca66abe5871c4a935a69688e8e8f0c1cd",
            ),
            (
                "throw-11",
                "bfe2dc6f685dea79cb58fcea94ee7911b31f182861b9c58256d9031a90262685",
            ),
            (
                "throw-12",
                "ca65cafde72a81f7426c8da2884031194c89c0ee29624733b1b3aa7b23bc7deb",
            ),
            (
                "throw-13",
                "34e2db05b574eb3bfff8ca143e24798ec0a757ec96bdeccd534b9755e96cc24c",
            ),
            (
                "throw-14",
                "14c7268a74357a90ca09876ae4f24e805b40676058c6b7dc05e3278f7c8694d7",
            ),
            (
                "throw-15",
                "578b20e8aafb2fd83873b99c89b1a0e110adcf7a087ac96e0a5fa5426f17666d",
            ),
            (
                "throw-16",
                "1690a4352a09c402ca8cd7718e3a18e846a5e12a3bf2c424882ff6abaa76004f",
            ),
            (
                "throw-17",
                "515ebb484ab1b1e1faaa1ada5e34625a542ec589029620cf26621aa7fa899e2d",
            ),
            (
                "throw-18",
                "be1e3911c20269277dbf326f7923b6005d1e6831f0d6b7567444ba2083fca3f6",
            ),
            (
                "throw-19",
                "74df26f3b2a7995f73ddc96e3484fcd89d5cbd7f2bec2a649d927dc7511c8058",
            ),
            (
                "hold-1",
                "d12fe447715353e8cd79508c2f6ae1d86ba7d2fb28106ee256f60659ca58d276",
            ),
            (
                "hold-2",
                "5ec88544a334ec3ee36eb874423029899fade4fe1700072f591c6af14b823afc",
            ),
            (
                "hold-3",
                "81a692ff1cc89e7078fe4e10d8e04ac9fa544f81d19d9e7d28ea3adf6f4d8dbc",
            ),
        ],
    },
    PictureSet {
        name: "hurt",
        pictures: &[
            (
                "hurt-1",
                "f9c765babbbeadf91ee7c01124b36732dd8e34ed359655f94103e4827ae1b005",
            ),
            (
                "hurt-2",
                "3ae60c86090c5c24af663cafb59d951fbbf2e24ddcbd2c72eb0ba6ceb8b426df",
            ),
            (
                "hurt-3",
                "654c7e700b6978d64e0f822680b1432e8003ad9d205ec444983ec3d4f1a10d46",
            ),
            (
                "hurt-4",
                "1c5aa20cafd2c2313f6c65017f531009b2e5effffee31752e7a3db100636023d",
            ),
            (
                "hurt-5",
                "b2e475276312ecd4b7f3e27065379800efda8ec60e107f75554085347d907029",
            ),
            (
                "hurt-6",
                "623adc7b31ef8427fae1581f7a7f510d94bbabf69089492c790d5f6925f95c26",
            ),
            (
                "hurt-7",
                "244349b60a9af4c471a3dff43073e0d0c8daf3d0049102b73c563f5c2d8c6f76",
            ),
            (
                "hurt-8",
                "50d55398b60ee70096325834335fbb9ea084cbd7f55e3bc3fe588d42acdc6b77",
            ),
            (
                "hurt-9",
                "ef9f40cca69e3c3c8ea219855450d17e0cf5ef087a357ac519b84c73fcfac389",
            ),
            (
                "hurt-10",
                "0cfadd58e7e41582af05237392289c9d15d0b37234351807f89d962b54bba135",
            ),
            (
                "hurt-11",
                "77fc7aa9409369dcb77b6a898edb08641bfc8e761900595605d1eb94b1bbf8d9",
            ),
            (
                "hurt-12",
                "5fede1d92e38b24c8d1ed0f29d24deb5c9d33cdd270abbd205363ce1ea5d15dd",
            ),
            (
                "hurt-13",
                "8b93d93a5896a716e02612c0bda55d234b31ce30a8bbea4c3f385e603d8ccfae",
            ),
            (
                "hurt-14",
                "f9e863914357fcc098884a4b1a6f0d88697173842c7640ebd341a1b9cfa20d38",
            ),
            (
                "lie-1",
                "a43fd6517bb09992476aacd904bb5c078686b1d0672ca5cf12d73107dbad9b05",
            ),
            (
                "lie-2",
                "b6d0fec95941e8035ee23749dc0f7773848c659d8dcd74ab2f0bdb227dfa95cb",
            ),
            (
                "lie-3",
                "bddc8e378bec7650c1b3c175203d2decd9d90a7c960eb1dcdecdb8677cbb86a9",
            ),
            (
                "lie-4",
                "36094f217f2f4349af7d5bd9db27b6d097a42c8a94b397c8735b0f0fe4cf557f",
            ),
        ],
    },
    PictureSet {
        name: "act",
        pictures: &[
            (
                "act-1",
                "132ea2d7f9b9bd1f4c392f47cac3d16ad7612ad0550428f031786a505e728dc2",
            ),
            (
                "act-2",
                "a875319d5cc745c697c239014a6cbd282baae31462e1e1be6b6f4917b71b071d",
            ),
            (
                "act-3",
                "c680aeb5206be54685df651b27dad33c75725f966c732bd8cdfc3070da8f1826",
            ),
            (
                "act-4",
                "d901c34c11d903e483f2c74c93e0267fa8684ec6dfcfc79b6d3841c48555c430",
            ),
            (
                "act-5",
                "8ee034f7f435df285f79ba1bfd3840b2ece5822b9c119ec97f7489ca65d0f028",
            ),
            (
                "act-6",
                "50d637da95d9f39468b0c95c84407a607771bcdc0e22bf293e567627df50475c",
            ),
            (
                "act-7",
                "3c31027864e9711c1a9e5e49276fc09ac0ca6e4ec8d0af88fd8ae963351ac8d9",
            ),
            (
                "act-8",
                "d9a50b8dc78369ce5a596e11c39a6dd684c2f9e959894004f0a245f959ac75b4",
            ),
            (
                "act-9",
                "99bacbdb0d686d34247ea9bfed5060c6a3aa3da7663cd5b6418cd26e16de442c",
            ),
            (
                "act-10",
                "5699aa8cdc737c58d003d98467297ff237725206d478dfcecfbb39fd647f9477",
            ),
            (
                "act-11",
                "e950cf16398e1aceaaedc05ece83b79c4dcc6598c90f008d75e421caf2cbb21c",
            ),
            (
                "act-12",
                "8f243b832b1ea541b922a77bf1d81665e58ca172dea5d60a36054370722d06aa",
            ),
            (
                "act-13",
                "9ad913b14df817b9e2d72768388e0b513a95272c26ee79c9d4856058841a7180",
            ),
            (
                "act-14",
                "130b187b53acaf5726ddf165b0437836f7fc1caed3bb87054195ca3eca8b07d1",
            ),
            (
                "act-15",
                "794618c16078f0c322467d4cd573be822526c146dc2232ba2d693d01a703527b",
            ),
            (
                "act-16",
                "e5d5feca522686f5d30584ad8b0d9c24153525cc8b1c9c8f65f894fb28a4995c",
            ),
            (
                "act-17",
                "69f26e266a303ddef066abaf49cfa52b9de76e823ac1c132556dc2d256c7ac31",
            ),
            (
                "act-18",
                "a89e778c790ac82ac2596e9bba19eaf31fea4b9163a6c155d39a04401876a38d",
            ),
            (
                "act-19",
                "d331915f0c80065f5eee3835852f9788b904d20799c038ccfc76f85a7feca9ee",
            ),
            (
                "act-20",
                "24e159ccb25614386c42982523e5236b4024460a200631cc77c074161ab2cfdf",
            ),
            (
                "act-21",
                "34b348f6ea95f652be57bff355b57fcf3e106dcea1c6ab2367902cbe965cd313",
            ),
            (
                "act-22",
                "dd42331e9bfaa090fce986338c09fd371fe4c23e9b47fe856d14a57a31816e7e",
            ),
            (
                "act-23",
                "028b1f7ebc9eaf86e2f659a38c32724d9a12a21ee20b7e1ca8367d58f7d65cdf",
            ),
            (
                "act-24",
                "cf811c45f4a366fbb72bffe99cda7325a78e9964f5bce375ecf33a000203ac99",
            ),
            (
                "act-25",
                "27eb5c3bb0719614258ef9bf75d8c9d295920907ada594f2dc2df733f33f4da6",
            ),
            (
                "act-26",
                "740df8ba5ca48771ace9a2810688f59e3b040c55dd86fbd0268bc3d8849a3ca5",
            ),
            (
                "act-27",
                "e9719a5ed08993a779be5f4fe342000db1e6bff665daa80dbcfc325ffc645ba4",
            ),
            (
                "act-28",
                "ba9bea1f5047f21798f2c20573962be944187dc47e76679fd1f888acdaa7bb9a",
            ),
            (
                "act-29",
                "d08a157ead1ed7b6ed59b59a93001246efe85cf626b0a0ddff3ecac23833e081",
            ),
            (
                "act-30",
                "2b14322db8bf117b41203b9aa9e7b6aaf2da0efab979a510740d0fa723a96c40",
            ),
        ],
    },
    PictureSet {
        name: "rope",
        pictures: &[
            (
                "rope-hang-1",
                "e457b7014286f0d3f1c31d240636cf54a53ff1e6d42e1a2c9f453da9f09d4616",
            ),
            (
                "rope-hang-2",
                "33992e6fdb8a542f7240a0a4bac26204723c4b2a7193c813d98ecb38bcca1f13",
            ),
            (
                "rope-hang-3",
                "23b55e719fde7d9b1e20d28165455d16b6335d523622e2e510f000d4c3b8ae0a",
            ),
            (
                "rope-hang-4",
                "64fbcdc96f2654a1e2761e765f9150ec2253f861bde8d1f59ee81844c99ff6c0",
            ),
            (
                "rope-turn-1",
                "d5717ff11b38aa1ed7eef6fb4cc3eb15b13b7f67ce6a924f62ade0145c78135a",
            ),
            (
                "rope-turn-2",
                "b955311812452b4847963507e303711464dc15ef697208f629c46bf08efdd7ed",
            ),
            (
                "rope-climb-1",
                "d5aff97c81fdb87aeb09a946b7a4cd7b16c929427599ec278fdeca71040f79eb",
            ),
            (
                "rope-climb-2",
                "248f0bf0e34abd243ba5033fb20842f9d40fa1259fb3ad678a90ff0ece05d5df",
            ),
            (
                "rope-climb-3",
                "4b91728e6a111955aa0c85aeccc7bf4236dfb900b0394aff495839850d9e3a1f",
            ),
            (
                "rope-climb-4",
                "852e32b33b3fbd4545d1f2f2c3ba52bff717ed4e1ca27dd71e0ae81809d794ed",
            ),
            (
                "rope-climb-5",
                "4b029f2b1dd49e52c9e98e2af7db63af41e602416fa4b877976a92e8cd7dfdfe",
            ),
            (
                "rope-climb-6",
                "fd60685a4f8e2e9cc5fe66699b137f67ac963bc41e4440f1ffaf271702d5a730",
            ),
        ],
    },
    PictureSet {
        name: "scene",
        pictures: &[
            (
                "scene-1",
                "09ef39b8c01b7fe26d276f168402f4167cc9d38db2d473e35004ecf729206e6c",
            ),
            (
                "scene-2",
                "61324a3c3597f2c033b2f41be8ea4322e7334de361bf3156ed6c9d194d2a6714",
            ),
            (
                "scene-3",
                "9de7aa4fc0df89ca063f4b8ae1744d1b70c6be7098c34cba0dfa30ce781d7bf5",
            ),
            (
                "scene-4",
                "c3200e506b8cd41f10ef020a17cf1d9730046ef5f467c674f48234401311c875",
            ),
            (
                "scene-5",
                "c8aff6b3a5ec7727eddf266e3685f5fc79720ed8cf2ec0aaf36e7af9564b7be5",
            ),
            (
                "scene-6",
                "75b72d7f6857b7a546a5fa3168a1b676edc56713608520fc1f67bfe3c1eadc36",
            ),
            (
                "scene-7",
                "3001aefe4304aec30aa3c3603518be0012b7a1593c8131afe7c19548d83b18ad",
            ),
            (
                "scene-8",
                "251acf233ea18fc5c5be4e9c5b3bb0ce2693f40727b9cd037fe23a7411f17bc0",
            ),
            (
                "scene-9",
                "d617e9e7a23f21e47565568e4c910058aae349a7b3ce26910c4dad8cd016fbd8",
            ),
            (
                "scene-10",
                "77ea4b322124a10d212498db7bb0febe00147c417185aea1f08a84987d77cc72",
            ),
            (
                "scene-11",
                "dc5beb6737e0662f8c885247509935d4a5423981cc3eb61c5c4868453d4d3446",
            ),
            (
                "scene-12",
                "29ca7a808f7f46d3d4948256b1a1916cb0ddcd068faf2f42776425e0561edee5",
            ),
            (
                "scene-13",
                "c1b35d450fcfa8afa40be8f43cbc7af596ad0a5ba690a878bc92f5c1caed1285",
            ),
        ],
    },
    PictureSet {
        name: "map",
        pictures: &[
            (
                "map-lead-1",
                "d04c96c11202572daf17c9452bd108a3e913c1e7804a7d32e7465411e003ee5c",
            ),
            (
                "map-lead-2",
                "624ce46eb019ebc2e0480359824a6c368de59a3b7f94ed322fccbd14e26ea43a",
            ),
            (
                "map-lead-3",
                "1f44c8d65217e4e2dc61d784c748632ff32319e838f12245c8c02e5c31f96f14",
            ),
            (
                "map-lead-4",
                "3be7370c1bb673ad00129964f14522db3e48859051c2011bcd4132f1cd51977a",
            ),
            (
                "map-lead-5",
                "f3be04419a891ddd876982c456dd0ee7b324193099c1fd9ff18be3046e327a72",
            ),
            (
                "map-lead-6",
                "d2f845667dfcb290cfeb5b52220b5252f3c4e27c81fd5756b0b786335e5457d8",
            ),
            (
                "map-lead-7",
                "2d56620e6b88a64c10dca3394bd2f261cd5b102f0e4787fdebb0e813ec7bb1cb",
            ),
            (
                "map-lead-8",
                "f3fda78649ddb25d4ecdd77c7eedd9409a76b78b1c35d009ad4828b7f6d62b4f",
            ),
            (
                "map-lead-9",
                "90042a3a02f9905068dcf7d91449bc6b0fe88d72bed7f21328d32ab3edcb0c7e",
            ),
            (
                "map-lead-10",
                "9f4d8a0c16a19596a82ce47e11cd7a0e9953e25efcc28a7048813905e521b0f2",
            ),
            (
                "map-other-1",
                "70c4929658e8baba52ca2b02fc36903d1d0a61aa8f13e7bd5141a3b1e6d8e9cd",
            ),
            (
                "map-other-2",
                "e4c158c7d2ae49f1cbe9ce97f0ee3a43c56afcf3831f9dfb570564f2bd6c404c",
            ),
            (
                "map-other-3",
                "4a9176f66de94895fff1139c9cf21c12f8b28d4c87a2648f73799284880ab2c0",
            ),
            (
                "map-other-4",
                "fb91e71d2e990b4e82fa5473a44c9fdc95f0098a051daaae0c031de291082b5f",
            ),
            (
                "map-other-5",
                "feeff0479a169ea6a17b8cc582e2e405169b7fd3ae83121b1a71f91f2e4b5604",
            ),
            (
                "map-other-6",
                "2350ffc0ea6ca2713f5e30229bf5f8564d9f91d0e8b4e6befc95c0919adc2153",
            ),
            (
                "map-other-7",
                "612f58cf9802d54c9ffad81af186fe3a442f6c5bd1fbd00b6b27ebe79fae9e24",
            ),
            (
                "map-other-8",
                "b266d2713e5a9834051e67e350f8ad988be5354c3cdc94a3ea5862cd51374f48",
            ),
        ],
    },
];

/// The second character's sets, as [`FIRST`].
const SECOND: &[PictureSet] = &[
    PictureSet {
        name: "stop",
        pictures: &[
            (
                "s-stop-1",
                "43027c100fbbd80d664ca0b03465cab3d20cb68bf20648981920e94fbb4a2cb3",
            ),
            (
                "s-stop-2",
                "313a44cf5203b99b98ec2030abbd1deea906d15109ad6736dc01c34e9cf38b9c",
            ),
            (
                "s-stop-3",
                "61ec973d83cb594930dbecbd50d3ea924bc198149e28dbae44e03303ea485cb8",
            ),
            (
                "s-stop-4",
                "183c7d39b26859c048f13e80381738893ab18b8bb73c6c446a42d238f0d2b5cd",
            ),
            (
                "s-stop-5",
                "96d312d91fd5b4aabea2271d5ad6cf667142dbb37228ef558e72f8381d0af7ba",
            ),
        ],
    },
    PictureSet {
        name: "standing",
        pictures: &[
            (
                "s-stand-1",
                "c96e37978ef9f66368de70e708e0cb10cc1347e24f43e9222b783defa0221c11",
            ),
            (
                "s-stand-2",
                "3d2ed98a07e2e4f545cb2cb3dc9ea12f273c14b437474e0abddb7916175ba056",
            ),
            (
                "s-stand-3",
                "ecebcd67df650e80459a118f4788661fa6af6fbe6e1ae2e21753f1b1e710b8c6",
            ),
            (
                "s-stand-4",
                "23b2d1a607536da95170e443cf43fbdefaaefe5da16dd4f5d914c33259c88893",
            ),
            (
                "s-stand-5",
                "e31b57b1a769f04a51fa7b459fb26c23795467f7522ed7052a398317cef0dbdd",
            ),
            (
                "s-stand-6",
                "b57ab76627040021aa7d9da22da43942e2803dab699afb20cd232bb504beda78",
            ),
            (
                "s-stand-7",
                "e07eef5810187a69bdf62a7dff419f578f384a125943bb8c1a58f94ee63cb151",
            ),
            (
                "s-stand-8",
                "89dcca59242ac99c8ee9639edd8994b0f49e1d9bc4a8cb9a3819a599f6143b57",
            ),
            (
                "s-stand-9",
                "9dc1f0e63cfaa3e753b425c2015ac28821906b2cb458d49497f21cc288ec96a7",
            ),
            (
                "s-stand-10",
                "90fabea4853a8c10af4388bcaffc8442f78fd40e207313d280b6021ccc937739",
            ),
            (
                "s-stand-11",
                "208ca7b1f8366a5af562a92fddc3f83fae93fd4d2b5c998c326ceb8893d35fd8",
            ),
            (
                "s-stand-12",
                "2c816925a2e8977d623d53f778eca84b6e425c05bb72bc671e0f1bbde33af44e",
            ),
            (
                "s-stand-13",
                "cc722562ca626acba64c949499f797dd3de18f9ddffc90760c0f11f9f1197cfd",
            ),
            (
                "s-stand-14",
                "302ddcaccf6a4eb54278499a299a5af2a76ac2a88f2f0144a715925828753fa6",
            ),
            (
                "s-stand-15",
                "19468a3a669273a3ada59d5c59d18859b690faa7d0476b1ea438cd78fb751c29",
            ),
            (
                "s-stand-16",
                "7f41b1efa5d6ea1b1a77532cd8cd956545cb7a431451ecf28507869210a6686e",
            ),
            (
                "s-stand-17",
                "ce85ec4cef60ea055374bdaff52e08261fd34461a5b00dd896776e284bf43ebb",
            ),
            (
                "s-stand-18",
                "925aa10f752319a35e2d7b5a6a54dc54ebeb72c9700dc0f69902bad7e466754d",
            ),
            (
                "s-stand-19",
                "ee2b0485363cad8b65e77d31ff6fa39c7a3736b7ad4c9e4a208ab4c022d29232",
            ),
            (
                "s-stand-20",
                "34c5eba75562a143b03c9810a966dc01807793f02185a6c0982ee4b5c7a998a6",
            ),
            (
                "s-stand-21",
                "ab40a6fd3cc26b930626ff5795e116506e2d94d467519d620adac877f006813b",
            ),
            (
                "s-stand-22",
                "37e30fd07b8739a541e4e12bea5d619dfa5e500db7ed271015b7d791d7abe51c",
            ),
            (
                "s-wait-1",
                "48a6627ac32d1ee73208a070e65b9b92626530d28723142a66ceb71cc4772035",
            ),
            (
                "s-wait-2",
                "35584fa26c80dc162e31ad8c9504a814160c90e6195c0684f3e2daee79ca949c",
            ),
            (
                "s-wait-3",
                "060e6c9a88e26683575e11517b8c51276d6cf339b76afe37e03fa2145422dbe5",
            ),
            (
                "s-wait-4",
                "bf05928020da5cc65e755a4d52fec8bc19c10acb9f8e19a21bf8ca99dedacbca",
            ),
            (
                "s-wait-5",
                "1f4cb794c518185c5ba8f018356a743bf5b294c6e712618aaf24dd3fc1637b53",
            ),
            (
                "s-wait-6",
                "19f8686513d370e9337ac3e9c0b70ea635b973a325a4aa2c656a16d5e982cc74",
            ),
            (
                "s-wait-7",
                "debd7dcfc47faaae3a9b02129360ff9024b1af26059de7571a530651970425ed",
            ),
            (
                "s-wait-8",
                "7485918dc7eb6de2e6daad362eda80a5b2d8fb721ae11bcc7d749a80724f2e69",
            ),
            (
                "s-wait-9",
                "335ee0f913c171cb7a71b2d25244f6691133c11e887c2641628adb8c1644fc54",
            ),
            (
                "s-wait-10",
                "47ef7c2578a6f5975aedd84e4205187b2f264b0145e1a39c4ee0be3db5fb735f",
            ),
            (
                "s-wait-11",
                "6359e45e83aa7fa3509d7d89ef65ab70b5e6fccecec4c1c58120c046d8aae370",
            ),
            (
                "s-wait-12",
                "72c18aeeee446c94535dd77067dfffe5f86fa286db32ca37705c7f6254d0b7c6",
            ),
            (
                "s-wait-13",
                "f5709cfb4fd3588c53c69576d496dbbde6182e327253d1e94c6e1ad71593669e",
            ),
            (
                "s-wait-14",
                "1577e86bdab5d2114fe29fd014242e96a1e33397fc469478df86615a17ce3568",
            ),
            (
                "s-wait-15",
                "4bf0e08ce812c80380f1310d70efb160a0642c7759000626cf85adee8f6cdf4e",
            ),
            (
                "s-wait-16",
                "34117412cb7d59beb1fbf774773656fd9c55d39046775453495150c4deaa0bfe",
            ),
            (
                "s-wait-17",
                "86db93a8d220f3fe0d76c82b05dec865f25464e89ed3286d6e3a4daaa86f0500",
            ),
        ],
    },
    PictureSet {
        name: "teeter",
        pictures: &[
            (
                "s-teeter-1",
                "12b2719b4d79af53ef25e2c08fe054ec8bb383202c28581ab123ecbf865c126e",
            ),
            (
                "s-teeter-2",
                "3b438c9c3b195d6733275867a6ed37b0b7bd81bf9cb1ef3c5506f6b16d5b1ed4",
            ),
            (
                "s-teeter-3",
                "e70b432390539e6a9874a788f693214cb2ccc6e86d429958af5c20d986ea7717",
            ),
            (
                "s-teeter-4",
                "97bb7f2ea92d398a3f5ebc19d353e7879320b079bde394e71afea751ef84fe9e",
            ),
            (
                "s-teeter-5",
                "745164196edb736a3118a1e0f8551d07225a7b250e14be2785e42be3be9450c9",
            ),
            (
                "s-teeter-6",
                "51e38b017af0b21fe35f4a0a143df671f3a831b771b2ca145c402053bbc5c024",
            ),
            (
                "s-teeter-7",
                "ea8ba818498e81fe0cde176be8bbdb94108c184496d9f1f11b2ea25ec099a3e2",
            ),
            (
                "s-teeter-8",
                "e8f31d23c2f4561a2e008cd86192975edcc6e3f4476da4f9ef0ecef78174a3ce",
            ),
            (
                "s-teeter-9",
                "10d657718f5db5ab50c657caff097e396935ac0cfb2c337639be256c19d1ada0",
            ),
            (
                "s-teeter-10",
                "6829656acc73516fbcbc495cfcdb073d1dd50938bbc46ec88551fe3f44aeef8b",
            ),
            (
                "s-teeter-11",
                "46b7b2e590a8ac2df4a03ae3b2a4316dfdc890ede948c767072d1433136124bf",
            ),
            (
                "s-teeter-12",
                "554d18eb09324c7f67f28d50947279ad9169c88c58c070e68c4b9391e1d7c6a2",
            ),
            (
                "s-teeter-13",
                "ae758e47bdfdc334a9b96240e49f198c6c65350dcb95e376a73a803781845d0e",
            ),
            (
                "s-teeter-14",
                "0c503c3715b0934fc1aade749b0a0e8094127232e339c4b604ffd52604309c3d",
            ),
            (
                "s-teeter-15",
                "50a61303104a441a8f361ab9d0ffcd1cdf35671e5a5f4ab2a0f928d2953b0821",
            ),
            (
                "s-teeter-16",
                "93d30b22e16050874939be740423c4477fa56d6e7d84e4cc1d5df0156e15a7ec",
            ),
            (
                "s-teeter-17",
                "b898ca126279367d0f00492160436c993763f853b3735cd3a54990071ab8d666",
            ),
            (
                "s-teeter-18",
                "c94d6535ec7f6d7fdaac09775f4f96145e78d57a9db3f8da3e2625b140f444d5",
            ),
        ],
    },
    PictureSet {
        name: "jump",
        pictures: &[
            (
                "s-jump-1",
                "171a596201c949c0f31ee4dc50eb5c9f31a63dc03a74224a47c5fdf6df13cfe3",
            ),
            (
                "s-jump-2",
                "7a25d69f9869fcaaa12debb0752478d080e0433bf380e198a429800d45c5c8da",
            ),
            (
                "s-rise-1",
                "dfda6a14e311f0ef690d363d68ab366ed3d6e06ab1d5dc29fcfc82df7ee11b32",
            ),
            (
                "s-rise-2",
                "163a5ca10d16c528365e64081499817f0628afba467800b2ac07047d1c1bbeb7",
            ),
            (
                "s-rise-3",
                "cb869e313c211297ec218d7f090e78ff7c3d40ec933f99b2f20192e02af06aaf",
            ),
            (
                "s-rise-4",
                "afc7ac1eaccb359928fdbb8d64101a4c572acc3660b4d3a316bd42b089760eeb",
            ),
            (
                "s-rise-5",
                "7b44b7fdc164bb573ae0473e49cccca49c10318daf51bb5dcd811800879a9fff",
            ),
            (
                "s-rise-6",
                "24cb46f729c07967c0359c8fa7cca68651fbd4d80cf3e22199e30318fcb3ba7f",
            ),
            (
                "s-rise-7",
                "30bc2c112829d653907735ffe1e3ffa6ad936a88bb12178d3a04cf54bc202da8",
            ),
            (
                "s-air-1",
                "02b3162f1894ae8dca8296da5109156441253b05c0594fbc8fe740595fccdc92",
            ),
            (
                "s-air-2",
                "b3d4cd1928d232c98fd674a3a1d41282cc23a163672bb08884c893f4ff28ffb7",
            ),
            (
                "s-air-3",
                "002c1abdc0178ce980992efc81ee5d5eb7530b5b98ef529cb7cbb7f03b32d3db",
            ),
            (
                "s-air-4",
                "3bcc5357806ef9872c6b29a57fdb29e65cb0ab4b82d795e4ae081305a2a449b5",
            ),
            (
                "s-air-5",
                "63ee2b65b5f5d674d9c585ee691d1c97a47346f11951b1d37d6d9c8460d2f346",
            ),
            (
                "s-air-6",
                "d9fc3b7253494e0351c948629104eeb530646fab813995883fbab71f00c8d5d5",
            ),
            (
                "s-land-1",
                "53ff1b65a28a9c26fe13480d5ff8c2bea60910ae79f53b9b1a70c88e1f8c3868",
            ),
            (
                "s-land-2",
                "6013f3677c1393724c1f882d78d014da69f25e336448303b10211f4ece31f3a2",
            ),
            (
                "s-land-3",
                "c1c24dc0b4caa4481e47d0ae942398cf4be1664146a69e34760023f666895f89",
            ),
            (
                "s-land-4",
                "a5850548dbd60c824d2bfc752768b90da7669043e03cb97efdedfb2abf985e97",
            ),
        ],
    },
    PictureSet {
        name: "bounce",
        pictures: &[
            (
                "s-bounce-1",
                "aa5af774ce0f14bdff10b1c89725c4107a0c2fa5dffc10d1c4e9c91bfb0520b1",
            ),
            (
                "s-bounce-2",
                "b40d5b5deb112862c4fbd80476cf07c51a7a34b8c2ecdab4b975905b3ce73b7b",
            ),
            (
                "s-bounce-3",
                "6d00935beacd8d1aa9132aeefcbb5fe1212d862ff399d1c42565d681391d4c1e",
            ),
            (
                "s-bounce-4",
                "337e5a0cb19407f73b9ce127df4d26b57b3dd0af94f8643237799239287a56c2",
            ),
            (
                "s-bounce-5",
                "8aceee657d8fc0c0bb9ab430babf1e1b93572e5d1d2d86fee519da833882fe28",
            ),
            (
                "s-bounce-6",
                "f43c08340af52289063e212bafbea81dab2897593605b90e11f24c25883a0c81",
            ),
            (
                "s-bounce-7",
                "fcb4b92464b38a724435e380537c6d082949c4cc6d113bb25f6f347190491cb1",
            ),
            (
                "s-bounce-8",
                "0aace791815d4b0cbb14e91f535d3514a035a0fa4a388fdc36d0225b637847dd",
            ),
            (
                "s-bounce-9",
                "0aa65508b13d245ae3f29bb0eb15600bbf47ef4c3a423ac6118044838ce87a44",
            ),
            (
                "s-bounce-10",
                "bea4c83e3be641088b82bbf4ce44b6bc5c508bdb8090fe9f50acce4d90d3d434",
            ),
            (
                "s-bounce-11",
                "86bceffe08e6d18e6aba9d84bf2baa893324af2eeb4fcd438390ef711b5fb726",
            ),
            (
                "s-bounce-12",
                "a2a53de5711e1a842dd8e08b87c6b3ba72cd8ad36ae1ececf5c97ebab5c8e5d4",
            ),
            (
                "s-bounce-13",
                "22eddc0614067e72edc56d44e6f033d8b12b7843145de6605610e1dacd0c2617",
            ),
            (
                "s-bounce-14",
                "75e2e9bdc8f8f9382ea757d02926bd639bd04ed2276f71d832af5efc844deb21",
            ),
            (
                "s-bounce-15",
                "bdb86eca5e54140127e549ba52052970ba2a632de159305d8acaf108aa511280",
            ),
            (
                "s-bounce-16",
                "0c1aef55a7181b1fa3480ea66052e869ddffe751ed47d796cde4fff3b6e91dc4",
            ),
        ],
    },
    PictureSet {
        name: "walking",
        pictures: &[
            (
                "s-walk-1",
                "1b8b325c0f381b276f9ea2f4609878873b66ea5cbedebf2df1829d08a9bc182f",
            ),
            (
                "s-walk-2",
                "96499fdf7c9e8c6b7d6447dd26fc48be53c915a9a47dd8905af0b7ed0dcf480b",
            ),
            (
                "s-walk-3",
                "0cfc6a1c8d877df03ad2b0b284a356c38d73ff936cfc8987c0ecb18e454bcc0a",
            ),
            (
                "s-walk-4",
                "4142d614856b976990d6d55504e2d0075f0633aafe02a4038d7007717d540790",
            ),
            (
                "s-walk-5",
                "830a443f8e7acb7663f1588d54cfdf84e916cde1f7ca08ada8641267af56c4dc",
            ),
            (
                "s-walk-6",
                "8b99f61450196b95a51d04d6cb01849ed6093e50af4b647389d3e8b09806a248",
            ),
            (
                "s-walk-7",
                "fd96adb4432b7c65efe6abf3acd7f55e6a112f6d1de00e8856ed88ae646af26d",
            ),
            (
                "s-walk-8",
                "8e74b9c570040aa2736231af7c5b859884c164a5c6eb87a958a76bdd9cb8e9b1",
            ),
            (
                "s-walk-9",
                "9e554e85898e5d3e1480881ba895f62cbcc9348ea5860dc4dfc5e0651acdef6a",
            ),
            (
                "s-walk-10",
                "2a23302690015e9f3f6ee2423c14e68726590c161101e4d66e13bbe5cf7138f0",
            ),
            (
                "s-walk-11",
                "95d19e026176ae813422e5522d29bd9df28cceb01dae47bd7117e5e65983a39e",
            ),
            (
                "s-walk-12",
                "562c505a9c20e984b90b0b864735435cb37dd92d28bdd90c2f5e14401d384b5f",
            ),
            (
                "s-walk-13",
                "98d3b72615e095bcee2f827841e47dacc78696c787483b7cf3b68fc12ab6c9a1",
            ),
            (
                "s-walk-14",
                "70514667bbcd4fe1c45de80ba93ae13569d4b3610141bc28391c5f3a14ffc611",
            ),
            (
                "s-walk-15",
                "c275da1810f8d4a45c9b3e074e2246db5bf4ff1a9d6738c3a7046580c5ddc050",
            ),
            (
                "s-walk-16",
                "946683f941c1a111cadbdba0987302735476fb90de896cb558e02560f0fee762",
            ),
            (
                "s-run-1",
                "71eeb1b06c7fb41bade0de1d98419a7eacd827e9c600def1818b0025819ce7fb",
            ),
            (
                "s-run-2",
                "7b34599e9e7d96a94ff766022a44cd776e1f8b35e35f9a4f6cad5b1c17c62232",
            ),
            (
                "s-run-3",
                "543fd6e8b127538c5f6e11f5e1b1a174f87d55324d39d29974890aad307ec84d",
            ),
            (
                "s-run-4",
                "2e90b7ca3a1986fe229383e666347bab5eb6f16504d60a474b7dffb39049f0e5",
            ),
            (
                "s-run-5",
                "33e2700832b35f66439a755029c2cce7d011b4a240ce05770ba548b08193c33b",
            ),
            (
                "s-run-6",
                "9a0ac4e329c1ee97efbda34c10c51eb293c8a5ef78356878b6d4163a7ba68732",
            ),
            (
                "s-run-7",
                "42ec7c1d5f506eb2a608ace2bb6c2d102d472bd3d15fac3a46ab465beb2593c1",
            ),
            (
                "s-run-8",
                "d69e3f0325eb5391477402e363e362895d04243ff0dabeba96766bf0cd786cc3",
            ),
            (
                "s-run-9",
                "bd65f6cc03350830cb9c6820b225147abfc4b434d91eb3529e3b07dcf58c42e1",
            ),
            (
                "s-run-10",
                "8f26170857c83e362ef6373ca9cdfd66570b7b2e8851f58e7f0271b7b99d4e63",
            ),
            (
                "s-run-11",
                "32900816039c9988318bdf15d69924f1b915d155e491f1b71f41485de2f1accf",
            ),
            (
                "s-run-12",
                "590976c6ca84686564856f717543173e80e2f6edc9fb7f8a8cec2a74d081de2b",
            ),
            (
                "s-run-13",
                "a67e8517d7a9814202331a4ecd5a132abcf56aad81a70ab30d61bb348cb79d35",
            ),
            (
                "s-stand-5",
                "e31b57b1a769f04a51fa7b459fb26c23795467f7522ed7052a398317cef0dbdd",
            ),
            (
                "s-stand-7",
                "e07eef5810187a69bdf62a7dff419f578f384a125943bb8c1a58f94ee63cb151",
            ),
        ],
    },
    PictureSet {
        name: "roll",
        pictures: &[
            (
                "s-roll-1",
                "e260be4d643b60d179a121d8ff3c1a35544d1e8702187f7045b02ce161de83a0",
            ),
            (
                "s-roll-2",
                "31b1f1a114521437f19b56f634e49bb1ac7259a92c5c76f8082bb503e251ef6d",
            ),
            (
                "s-roll-3",
                "ebc797fcd18aa1296403e90e2757811cc25dee8462ec551733622f27882b473d",
            ),
            (
                "s-roll-4",
                "59f4f382ebc48159f5a6754cac558d074ef7519aa292428bc62b0c3e18a35a1c",
            ),
            (
                "s-roll-5",
                "1dd1870acb5e3f688c51ac1a61d60de5c7ade3e87c0be0f6c5e5d564f7e644d3",
            ),
            (
                "s-roll-6",
                "61ba547c495768606592c53d3f3ea0c34856a0e5ff345cc6a02dbbf4d547ea2b",
            ),
            (
                "s-roll-7",
                "d8ee99f24af7f6b97be549ad1f1da3c52c4f9f7b1d435e3113dbc99d93efb846",
            ),
            (
                "s-roll-8",
                "e0a00910dc3eee70bf1b4338a0dbf062adcb39cc3831230505f1869d3490f9ce",
            ),
            (
                "s-roll-9",
                "63238f2f6c22296738e176547600218be84ac62a5ac014431dc5c8d7863fbc22",
            ),
            (
                "s-roll-10",
                "3703eeedf0cfa4d90863a4e946ac30f89993ed703d7452fa24864ecf1e3df207",
            ),
            (
                "s-roll-11",
                "e9dc4951bf49354c3ffd9893d2c1f714ddd55d1a18c8412086f6bdeb2175a9ab",
            ),
            (
                "s-roll-12",
                "14ae06e25a39a6d3a2b9a3ff492d8ca5924125d02f5bca6575c9eaf1cc4fe3b7",
            ),
            (
                "s-stop-3",
                "61ec973d83cb594930dbecbd50d3ea924bc198149e28dbae44e03303ea485cb8",
            ),
            (
                "s-stop-4",
                "183c7d39b26859c048f13e80381738893ab18b8bb73c6c446a42d238f0d2b5cd",
            ),
            (
                "s-stop-5",
                "96d312d91fd5b4aabea2271d5ad6cf667142dbb37228ef558e72f8381d0af7ba",
            ),
        ],
    },
    PictureSet {
        name: "crouch",
        pictures: &[
            (
                "s-crouch-1",
                "015cd44b0d26d2c593b09b89dc9389e9910e83f8739f250865ec4f7af3c06f90",
            ),
            (
                "s-crouch-2",
                "f0dc9d7c4e987f6290929d3d278ff656b5bb4514948b4deda634c676428996d7",
            ),
            (
                "s-crouch-3",
                "fe025be976d293a9eb2bfebf3ffb9bc886dffe91e053bdcb1570484168c8a7e9",
            ),
            (
                "s-crouch-4",
                "96d87ffc70a84e64b8e4c276d0bb1471ec1cf5f1c052589cb231634b20440a6b",
            ),
            (
                "s-crouch-5",
                "2fe8cad8b01a384ee72af2c9a625ee684a347c1d2ea305bdbb1cea2823b44084",
            ),
            (
                "s-crouch-6",
                "2ea0c9b8a6ada522d9b2075768385bf6a6ce3dc1034c2b40b90c0c2a4afe9516",
            ),
            (
                "s-crouch-7",
                "97116a40c5f856dbdc4341db86eaee740df1b8e61eb8bd7a26fec7f5a89d0000",
            ),
            (
                "s-crouch-8",
                "a1881316d4fe00b3f3edd84aa30b47826696797549c4a1c54553929930f46dae",
            ),
            (
                "s-crouch-9",
                "3464348957e5cf55aa68ac5049ab6ebf475bb1637971880f3711ebc4bf55f743",
            ),
            (
                "s-crouch-10",
                "183cadaecfc238b527c59b57023a41998493f540b71b32d923eea148db7d93b4",
            ),
            (
                "s-crouch-11",
                "5a2003778acbb5a8111f7705e062d940a8450351c24fff37d59a1d63d0441289",
            ),
            (
                "s-crouch-12",
                "ee75de33f4f61d734c86b2733e4532b253cb0611f40346e36f0f5db631f7d383",
            ),
            (
                "s-crouch-13",
                "a3b36705e81cc557c92fb22389203ff905864249fc4ce1794317b98c00719483",
            ),
        ],
    },
    PictureSet {
        name: "carry",
        pictures: &[
            (
                "s-pick-1",
                "3e3ea2ea2f2ce9b0d6bbf88d0354e67836821c465cbe302eff5dde44178c8794",
            ),
            (
                "s-pick-2",
                "7a7f965eeacdb588a0562033c7ddc88ec3e4e6b0243e536d8efc3c8f57e4d43b",
            ),
            (
                "s-pick-3",
                "47b9bfd4b5d878aa1e884d93be73cf49533440e8b3d6bfb5d9e9e6e998e57257",
            ),
            (
                "s-pick-4",
                "f5b62a0eee7933f28c2afe9611ff164c03297f08b59fc7a8d9d005c732e0b1c6",
            ),
            (
                "s-pick-5",
                "ee17b50e55e4641777085b2d8a2ce2b20f96a5942f71917a8b1805de5ee2be34",
            ),
            (
                "s-hold",
                "dad35cff5d4ba2801b223f2a8836e52b7d97e4c3224ecd5f35f00e4025ca1a9a",
            ),
            (
                "s-carry-1",
                "296f16e88cfb1ad09637f6f8e2336a2439610706eb4c0ea567cbad0d573f03aa",
            ),
            (
                "s-carry-2",
                "9f95797148b2827ee6c9bc86b759a0e6766ef04b8aa0227732a546de6f0cc8a7",
            ),
            (
                "s-carry-3",
                "54724e042a57fe430ee34a3a0df416fccfbf4a2e3e4fbb2926ef804300e735df",
            ),
            (
                "s-carry-4",
                "7ba653bde91158d51c94960bc318fff4d37346058616538d418863efdb4ae507",
            ),
            (
                "s-carry-5",
                "8337e9e6abdbfdebc26d847fc411fb1d09c2c3ee7f51b11790fe6835b00ffb47",
            ),
            (
                "s-carry-6",
                "624a08d8f433d29022860257bcfd08d5b7f4f5e80c0c1f553b189c618195c1a8",
            ),
            (
                "s-carry-7",
                "aeea425e6afbd63d6c61bd7c53d7478a395821e8cdf93508275f2b02627262fa",
            ),
            (
                "s-carry-8",
                "251f21118a708eeada86c5ffa3efae22ad1d1715ddce40ea3623b5ed498baaae",
            ),
            (
                "s-throw-1",
                "518142440da24a1a2d402e956d5de019fd11ba6fef81651cd5abe6ef590c3fce",
            ),
            (
                "s-throw-2",
                "b3d9d1cfe37e269609dcad15ed63506358806d7fce490b0c5b894153ee8b0ed0",
            ),
            (
                "s-throw-3",
                "cb901e46196b751499528e6a10b2824fa24ceccdfa150af49c7824e4fa67c660",
            ),
            (
                "s-throw-4",
                "50e9645e8ad7521700e883cd8625bf265a1fc9704cfebe47a2386281f0df28b7",
            ),
            (
                "s-throw-5",
                "56ef5b78c1cc373c5ae0ffb04ef28da75d067c43df0339abc77e67f5a3f730cc",
            ),
            (
                "s-throw-6",
                "cb12f91b487770048d70ba8dab279da224abce3a47661a6fa17fbc38c36fc5bd",
            ),
            (
                "s-throw-7",
                "eef0523797e51e64fba20ed7066df1d9742dd383ad221375cd2fbd468bf76b8d",
            ),
            (
                "s-throw-8",
                "21b6416c2efbff71cebdf5fb4581aeb02aa2bfed3951a9075e875e6452057e5f",
            ),
            (
                "s-throw-9",
                "9a4cde408b12459a92d34c1b1821e8545f2df729e4b35b624298acb03c7314b5",
            ),
            (
                "s-throw-10",
                "f864beb7dbadef34540497929df6050a2b6668892aefaeeb6e08c6fc1a786415",
            ),
            (
                "s-throw-11",
                "0f8226638dfd549643098515af51f9189e0c63b36fc87cd78a118184a4eb41cf",
            ),
            (
                "s-throw-12",
                "1cc91338b63b82eec99cb34aa291db5d68ce6197d55081dc32340995b57fcdf8",
            ),
            (
                "s-throw-13",
                "0a4b4911968230c159011f604890fd5572973685f185ae75af3c7e238f536f08",
            ),
            (
                "s-throw-14",
                "b88e458bb725281246beb5c629711075d6a5402b1fec6a872e80ca8dcd6d87b2",
            ),
        ],
    },
    PictureSet {
        name: "hurt",
        pictures: &[
            (
                "s-hurt-1",
                "02768e5122d8334ce66a9336f489e63f7a1b7f17fd55e3a34a35838b75979fba",
            ),
            (
                "s-hurt-2",
                "f98af65913f01832d72b1e6605a376ebbcf061e4e0c19ed31ac26e1e16dfe81b",
            ),
            (
                "s-hurt-3",
                "149edf52239dc253865760f3589cc66622e2f2de92830a0875baf15b5a9f8f21",
            ),
            (
                "s-hurt-4",
                "1a36049f91fe73a48a9061a51887be39c2d86b26df30193466cc22a8b0d77958",
            ),
            (
                "s-hurt-5",
                "b57966f119a06fcd5ebdb15f93291a443ac6134f15faec91f55f599c4d0cae4c",
            ),
            (
                "s-hurt-6",
                "7c8947659c11aeec64ef7cc034bc2cfaab6cdb4bce532c15678edd68e857ff97",
            ),
            (
                "s-hurt-7",
                "a878b4cb6d4088d77f101078d618f8666998f60b0ac56a6e2ff528a14f1d4187",
            ),
            (
                "s-hurt-8",
                "5e68049d22f8722f8cbb52a3bc1f0977b10316d0d2f36750ef9b41ddf3c84a63",
            ),
            (
                "s-hurt-9",
                "d26b52afb6a80c736cb4dd94f58286cd8886a43264e97e8e169217085d57240f",
            ),
            (
                "s-hurt-10",
                "4dcf9fc1a61567982945ca4b8a9bd3510513e058703e39c6a161b18611de54d4",
            ),
            (
                "s-hurt-11",
                "03fa0c32d6e3199b313226284ae1049e96fea60468372fa5cb2189bd6013e3e3",
            ),
            (
                "s-hurt-12",
                "3c000f013be921c729690ef9a9bc238b69f5d64ec170c9f3f905b0a2c1fb7118",
            ),
            (
                "s-hurt-13",
                "23620782d47a66ff45f65784d7366bbeb4d805a518bdf47b9387b60e9f47fe88",
            ),
            (
                "s-hurt-14",
                "87b42ceafd2a9302c6618b003273cd4997f754c211d22580d3e29c5f1cc21c5f",
            ),
            (
                "s-lie-1",
                "bd832dd5ccbcea92f8ae7406ae56156adfb8342ffbdfea21cbdcb526519d0ee8",
            ),
            (
                "s-lie-2",
                "c4b01efbfd6dce4dcde953441d1c053c1eae4b97a25a97cac9244caaf5741eac",
            ),
            (
                "s-lie-3",
                "60c1c19928c11ffa024bba5871026bb5581dcb5169fdd058fbce68fc36850bfa",
            ),
        ],
    },
    PictureSet {
        name: "scene",
        pictures: &[
            (
                "s-scene-1",
                "49bc050262c27962098bf9f20a8e202d9adb7feaec6a727376436fb61499ef5f",
            ),
            (
                "s-scene-2",
                "d30f00b2a403413af0c3fb335d6b5c19681bd45ea6cd8f38dc0d485ba23860b8",
            ),
            (
                "s-scene-3",
                "6aaaa3f1e08fb1d76affa2007adea12f62c65ec5e578cd6361995e0c7adc226b",
            ),
            (
                "s-scene-4",
                "69023e0cb6ce49e511e5ef182e76773501b4a6ac7f372729f020c9c5e1e01669",
            ),
            (
                "s-scene-5",
                "c617366fc1f4edc54c9f8d780d592c1fd0baa3ef2e1752da2e510b530385c166",
            ),
            (
                "s-scene-6",
                "a85a2454996ff759e0cade447e51188e4bdfa1cedc1a6509731ccc5d1c0311ff",
            ),
            (
                "s-scene-7",
                "80dde04726f742dbb9b0d2fe68af598590372318ad9723abe8c8236eaf100466",
            ),
            (
                "s-scene-8",
                "412e31d60ca329b3697518bb820c1ecda7227379eff1381950f9acc13439fcd1",
            ),
            (
                "s-scene-9",
                "039e43f1462c8f6827ecd3bead24f77108aca4392bfc0bae18029d8e597dde9c",
            ),
            (
                "s-scene-10",
                "14e89b4001c978a96b639dab2e2563e3dddff5edeac844384d79d6a7a2e26088",
            ),
            (
                "s-scene-11",
                "63d05cb4952fa76e26af9aad7b42aa113d3b7a1901d2571d209272a9cfb70d8c",
            ),
            (
                "s-scene-12",
                "47b02752138552e3245077ce6eac48505eb9870a07185229f2cb50681611ce1a",
            ),
            (
                "s-scene-13",
                "f109257406a02a93facd69a051b008850e7dec4d9570df16293ae14dc7f4121c",
            ),
            (
                "s-scene-14",
                "2877ae035ca8163ee1164c7d9043006b92bcc8d1aaef9e1a06ab2c207155a562",
            ),
            (
                "s-scene-15",
                "f2593b8d3a52ffb872bba3115c96825774aa6851fd6ab1964a46ef567ad793eb",
            ),
            (
                "s-scene-16",
                "825a3174a15be4d6c0fef323e5ce6467b1d3bcc93eff410dce76dfb7b9673edf",
            ),
            (
                "s-scene-17",
                "9a47bb5fc16f3417efcb07ff10560bf8929b48794dfc5b5554d47e8b04d9fe63",
            ),
        ],
    },
];

/// Files by path in the pack, with their contents.
pub type Files = Vec<(String, Vec<u8>)>;

/// A sheet's two files, by path in the pack.
fn sheet_files(name: &str, sheet: crate::SheetFiles) -> [(String, Vec<u8>); 2] {
    [
        (format!("{SPRITES}/{name}.json"), sheet.json.into_bytes()),
        (format!("{SPRITES}/{name}.png"), sheet.png),
    ]
}

/// Writes each character's sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterDecoder {
    pub title: String,
    pub table: Location,
    /// Entries in the frame table.
    pub entries: usize,
    pub characters: Vec<CharacterPictures>,
    /// Where the level map's palette is.
    pub map_palette: Location,
}

impl CharacterDecoder {
    /// The characters' pictures of `title`, where the spec names them and
    /// says where their frames and palettes are. `title-1`'s frame table
    /// has frames 0 to 2 748 (`formats/sprites.md`, "The frame table").
    pub fn of(title: &str) -> Option<CharacterDecoder> {
        let at = |data| locate(title, data);
        let (entries, characters) = match title {
            "title-1" => (
                2_749,
                vec![
                    CharacterPictures {
                        slot: 0,
                        sets: FIRST,
                        leading: at(Data::FirstLeadingPalette)?,
                        following: at(Data::FirstFollowingPalette)?,
                    },
                    CharacterPictures {
                        slot: 1,
                        sets: SECOND,
                        leading: at(Data::SecondLeadingPalette)?,
                        following: at(Data::SecondFollowingPalette)?,
                    },
                ],
            ),
            _ => return None,
        };
        Some(CharacterDecoder {
            title: title.to_string(),
            table: at(Data::FrameTable)?,
            entries,
            characters,
            map_palette: at(Data::MapPalette)?,
        })
    }

    /// The sheets' files, by path in the pack, and notes for the import
    /// report.
    pub fn sheets(&self, image: &Image) -> Result<(Files, Vec<String>), String> {
        let characters = &self.characters;
        let bytes = image
            .at(self.table)
            .and_then(|b| b.get(..self.entries * TABLE_ENTRY_SIZE))
            .ok_or("the frame table is outside the image")?;
        let table = FrameTable::new(bytes)?;
        let wanted: HashSet<Sha256Hash> = characters
            .iter()
            .flat_map(|c| c.sets.iter().flat_map(|s| s.pictures))
            .filter_map(|&(_, hash)| Sha256Hash::from_hex(hash))
            .collect();
        let found = find_pictures(&table, &wanted, |l| image.at(l));
        let mut files = Vec::new();
        let mut notes = Vec::new();
        let mut map_sheet = Vec::new();
        let palette = |at| {
            let bytes = image
                .at(at)
                .and_then(|b| b.get(..SpritePalette::SIZE))
                .ok_or("a palette is outside the image")?;
            SpritePalette::new(bytes)
        };
        let map_palette = palette(self.map_palette)?;
        for character in characters {
            let leading = palette(character.leading)?;
            let following = palette(character.following)?;
            // By label, so a picture in two sets is in the sheet once.
            let mut frames = BTreeMap::new();
            let mut map_frames = BTreeMap::new();
            let mut missing = Vec::new();
            for set in character.sets {
                let numbers: Option<Vec<(&str, u16)>> = set
                    .pictures
                    .iter()
                    .map(|&(label, hash)| {
                        let hash = Sha256Hash::from_hex(hash)?;
                        Some((label, *found.get(&hash)?))
                    })
                    .collect();
                match numbers {
                    Some(numbers) if set.name == MAP_SET => map_frames.extend(numbers),
                    Some(numbers) => frames.extend(numbers),
                    None => missing.push(set.name),
                }
            }
            if !missing.is_empty() {
                notes.push(format!(
                    "character {}: not every picture found for: {}; the engine's own art \
                     stands in",
                    character.slot,
                    missing.join(", ")
                ));
            }
            let read_pictures = |frames: &BTreeMap<&str, u16>| {
                let read = read_frames(&table, frames.values().copied(), |l| image.at(l))?;
                frames
                    .iter()
                    .map(|(&label, number)| {
                        let picture = read[number].picture(None, Mirror::default())?;
                        Ok((label.to_string(), picture))
                    })
                    .collect::<Result<Vec<_>, String>>()
            };
            if !map_frames.is_empty() {
                let sheet = sprite_sheet(
                    &read_pictures(&map_frames)?,
                    &map_palette,
                    &[],
                    &format!("{MAP_SHEET}.png"),
                )?;
                map_sheet.extend(sheet_files(MAP_SHEET, sheet));
            }
            if frames.is_empty() {
                continue;
            }
            let name = format!("{CHARACTER_SHEET}{}", character.slot);
            let sheet = sprite_sheet(
                &read_pictures(&frames)?,
                &leading,
                &[(FOLLOWING, &following)],
                &format!("{name}.png"),
            )?;
            files.extend(sheet_files(&name, sheet));
        }
        files.extend(map_sheet);
        Ok((files, notes))
    }
}

impl Decoder for CharacterDecoder {
    fn name(&self) -> &str {
        "Characters' pictures"
    }

    fn applies_to(&self, image: &Known) -> bool {
        image.title == self.title
    }

    /// The pictures are optional: if they can't be read, the pack goes
    /// without them and the engine shows its own art.
    fn decode(&self, image: &Image, pack: &mut PackWriter) -> Result<(), String> {
        match crate::optional(|| self.sheets(image)) {
            Ok((files, notes)) => {
                for (path, contents) in files {
                    pack.write(&path, &contents)?;
                }
                for note in notes {
                    pack.note(note);
                }
                pack.note(
                    "the characters' pictures for poses the spec doesn't cover (such as a fall \
                     off a ledge with a face, or teetering at a floor's edge): the nearest \
                     converted picture of the same character stands in"
                        .to_string(),
                );
            }
            Err(reason) => pack.note(format!(
                "the characters' pictures couldn't be converted: {reason}; the engine's own \
                 art stands in"
            )),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clamber_sprite::{SpriteSheet, decode_png};

    use super::*;

    /// Where the made-up bank of `spec/vectors/sprites/` has its frame
    /// table, and how many entries it has.
    const BANK: u8 = 0x5A;
    const TABLE: u16 = 0x8010;
    const TABLE_FRAMES: usize = 39;
    /// Made-up places for the palettes, past the vectors' bytes.
    const LEADING: u16 = 0xF000;
    const FOLLOWING_AT: u16 = 0xF020;

    fn vector(name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/sprites/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn hex(text: &str) -> Vec<u8> {
        text.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    /// The vectors' bank, with the palette from `palette.hex` as the
    /// leading palette and its colours reversed as the following one.
    fn image() -> Image {
        let at = Location::new(BANK, 0).position(usize::MAX).unwrap();
        let mut bytes = vec![0; at + 0x1_0000];
        for line in vector("bank.hex").lines().filter(|l| !l.trim().is_empty()) {
            let (address, rest) = line.split_once(':').unwrap();
            let p = at + usize::from(u16::from_str_radix(address.trim(), 16).unwrap());
            let line = hex(rest);
            bytes[p..p + line.len()].copy_from_slice(&line);
        }
        let leading = hex(&vector("palette.hex"));
        let following: Vec<u8> = leading.chunks(2).rev().flatten().copied().collect();
        for (address, palette) in [(LEADING, &leading), (FOLLOWING_AT, &following)] {
            let p = at + usize::from(address);
            bytes[p..p + palette.len()].copy_from_slice(palette);
        }
        Image::new(bytes)
    }

    /// The picture hash of frame `n` in `hashes.tsv`.
    fn hash(n: u16) -> &'static str {
        let line = vector("hashes.tsv")
            .lines()
            .find(|l| l.split('\t').next() == Some(&n.to_string()))
            .unwrap()
            .to_string();
        line.rsplit('\t').next().unwrap().to_string().leak()
    }

    fn decoder(sets: Vec<PictureSet>) -> CharacterDecoder {
        CharacterDecoder {
            title: "made-up".into(),
            table: Location::new(BANK, TABLE),
            entries: TABLE_FRAMES,
            characters: vec![CharacterPictures {
                slot: 1,
                sets: sets.leak(),
                leading: Location::new(BANK, LEADING),
                following: Location::new(BANK, FOLLOWING_AT),
            }],
            // Made up: the following palette's place stands in for it.
            map_palette: Location::new(BANK, FOLLOWING_AT),
        }
    }

    fn set(name: &'static str, pictures: Vec<(&'static str, &'static str)>) -> PictureSet {
        PictureSet {
            name,
            pictures: pictures.leak(),
        }
    }

    #[test]
    fn the_hashes_are_the_specs() {
        let path = format!(
            "{}/../../spec/behavior/player-frames.md",
            env!("CARGO_MANIFEST_DIR")
        );
        let spec = std::fs::read_to_string(&path).unwrap();
        let tables: Vec<Vec<(String, String)>> = ["| First character |", "| Second character |"]
            .iter()
            .map(|heading| {
                let (_, rest) = spec.split_once(heading).unwrap();
                rest.lines()
                    .skip(2)
                    .take_while(|l| l.starts_with('|'))
                    .map(|l| {
                        let cells: Vec<&str> =
                            l.split('|').map(|c| c.trim().trim_matches('`')).collect();
                        (cells[1].to_string(), cells[2].to_string())
                    })
                    .collect()
            })
            .collect();
        // The being-hurt pictures have a table of their own.
        let (_, hurt_all) = spec.split_once("## Being hurt").unwrap();
        let hurt = hurt_all;
        let hurt = hurt.lines().skip_while(|l| !l.starts_with("| `hurt-1`"));
        let hurt: Vec<(String, String)> = hurt
            .take_while(|l| l.starts_with('|'))
            .map(|l| {
                let cells: Vec<&str> = l.split('|').map(|c| c.trim().trim_matches('`')).collect();
                (cells[1].to_string(), cells[2].to_string())
            })
            .collect();
        let second_hurt = hurt_all
            .lines()
            .skip_while(|l| !l.starts_with("| `s-hurt-1`"));
        let second_hurt: Vec<(String, String)> = second_hurt
            .take_while(|l| l.starts_with('|'))
            .map(|l| {
                let cells: Vec<&str> = l.split('|').map(|c| c.trim().trim_matches('`')).collect();
                (cells[1].to_string(), cells[2].to_string())
            })
            .collect();
        let mut tables = tables;
        tables[0].extend(hurt);
        tables[1].extend(second_hurt);
        // The scene's pictures are in `exits.md`.
        let exits = std::fs::read_to_string(format!(
            "{}/../../spec/behavior/exits.md",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let (_, scene) = exits.split_once("### Pictures in the scene").unwrap();
        let (scene, _) = scene.split_once("**When a hash").unwrap();
        for l in scene.lines().filter(|l| l.starts_with("| `")) {
            let cells: Vec<&str> = l.split('|').map(|c| c.trim().trim_matches('`')).collect();
            let row = (cells[1].to_string(), cells[2].to_string());
            tables[usize::from(row.0.starts_with("s-"))].push(row);
        }
        // The map's pictures are in `level-map.md`; they go in the first
        // character's sheet.
        let map = std::fs::read_to_string(format!(
            "{}/../../spec/behavior/level-map.md",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let (_, pictures) = map.split_once("\n## Pictures").unwrap();
        let (pictures, _) = pictures.split_once("**Where.**").unwrap();
        for l in pictures.lines().filter(|l| l.starts_with("| `map-")) {
            let cells: Vec<&str> = l.split('|').map(|c| c.trim().trim_matches('`')).collect();
            tables[0].push((cells[1].to_string(), cells[2].to_string()));
        }
        for (sets, table) in [FIRST, SECOND].iter().zip(&tables) {
            let ours: BTreeMap<&str, &str> = sets
                .iter()
                .flat_map(|s| s.pictures.iter().copied())
                .collect();
            let spec: BTreeMap<&str, &str> = table
                .iter()
                .map(|(l, h)| (l.as_str(), h.as_str()))
                .collect();
            assert_eq!(ours, spec);
            // A label always names the same picture.
            for &(label, hash) in sets.iter().flat_map(|s| s.pictures) {
                assert_eq!(spec[label], hash, "{label}");
            }
        }
        let title = CharacterDecoder::of("title-1").unwrap();
        assert_eq!(title.characters.len(), 2);
        assert!(CharacterDecoder::of("title-2").is_none());
    }

    #[test]
    fn found_sets_go_into_the_sheet_in_the_leading_palette() {
        let decoder = decoder(vec![
            set("one", vec![("a", hash(35)), ("b", hash(37))]),
            set("two", vec![("b", hash(37))]),
        ]);
        let (files, notes) = decoder.sheets(&image()).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            ["sprites/character-1.json", "sprites/character-1.png"]
        );
        let json = std::str::from_utf8(&files[0].1).unwrap();
        let meta = clamber_sprite::Sheet::from_json_str(json, "character-1.json").unwrap();
        let names: Vec<&str> = meta.animations.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        let leading = SpritePalette::new(&hex(&vector("palette.hex"))).unwrap();
        let following = &meta.palettes[FOLLOWING];
        assert_eq!(following.0[1], leading.palette().0[15]);
        let image = decode_png(&files[1].1).unwrap();
        let sheet = SpriteSheet::new(meta, image).unwrap();
        let clamber_sprite::Image::Indexed { palette, .. } = &sheet.image else {
            panic!("not indexed");
        };
        assert_eq!(palette, &leading.palette());
    }

    #[test]
    fn the_map_set_goes_into_its_own_sheet_in_the_map_palette() {
        let decoder = decoder(vec![
            set("one", vec![("a", hash(35))]),
            set("map", vec![("m", hash(37))]),
        ]);
        let (files, notes) = decoder.sheets(&image()).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "sprites/character-1.json",
                "sprites/character-1.png",
                "sprites/map-characters.json",
                "sprites/map-characters.png",
            ]
        );
        let names = |json: &[u8]| {
            let meta =
                clamber_sprite::Sheet::from_json_str(std::str::from_utf8(json).unwrap(), "s.json")
                    .unwrap();
            meta.animations
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&files[0].1), ["a"]);
        assert_eq!(names(&files[2].1), ["m"]);
        let map = decode_png(&files[3].1).unwrap();
        let clamber_sprite::Image::Indexed { palette, .. } = &map else {
            panic!("not indexed");
        };
        let expected = SpritePalette::new(&hex(&vector("palette.hex")))
            .unwrap()
            .palette();
        // The map's palette is the one at its own place, not the leading one.
        assert_ne!(palette, &expected);
    }

    #[test]
    fn a_set_with_a_picture_missing_is_left_out() {
        let missing = "07".repeat(32).leak();
        let decoder = decoder(vec![
            set("one", vec![("a", hash(35))]),
            set("two", vec![("b", hash(37)), ("c", missing)]),
        ]);
        let (files, notes) = decoder.sheets(&image()).unwrap();
        let meta = std::str::from_utf8(&files[0].1).unwrap();
        let meta = clamber_sprite::Sheet::from_json_str(meta, "c.json").unwrap();
        let names: Vec<&str> = meta.animations.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["a"]);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("two"), "{}", notes[0]);
    }

    #[test]
    fn a_character_with_nothing_found_has_no_sheet() {
        let missing = "07".repeat(32).leak();
        let decoder = decoder(vec![set("one", vec![("a", missing)])]);
        let (files, notes) = decoder.sheets(&image()).unwrap();
        assert!(files.is_empty());
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn an_image_without_the_frames_gives_a_pack_without_pictures() {
        let decoder = decoder(vec![set("one", vec![("a", hash(35))])]);
        assert!(decoder.sheets(&Image::new(vec![0; 1024])).is_err());
        let dir = std::env::temp_dir().join(format!("characters-{}", std::process::id()));
        let mut pack = PackWriter {
            dir: dir.clone(),
            written: Default::default(),
            skipped: Vec::new(),
            notes: Vec::new(),
        };
        decoder
            .decode(&Image::new(vec![0; 1024]), &mut pack)
            .unwrap();
        assert_eq!(pack.files().count(), 0);
        assert!(pack.notes[0].starts_with("the characters' pictures couldn't be converted"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
