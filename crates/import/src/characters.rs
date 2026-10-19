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

/// Pictures that are shown together, by label and picture hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureSet {
    /// The set's name: `stop`, `standing`, `jump`, `bounce`, `roll` or
    /// `walking`.
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
/// "Walking, running and turning").
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
        ],
    },
];

/// Files by path in the pack, with their contents.
pub type Files = Vec<(String, Vec<u8>)>;

/// Writes each character's sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterDecoder {
    pub title: String,
    pub table: Location,
    /// Entries in the frame table.
    pub entries: usize,
    pub characters: Vec<CharacterPictures>,
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
        for character in characters {
            let palette = |at| {
                let bytes = image
                    .at(at)
                    .and_then(|b| b.get(..SpritePalette::SIZE))
                    .ok_or("a palette is outside the image")?;
                SpritePalette::new(bytes)
            };
            let leading = palette(character.leading)?;
            let following = palette(character.following)?;
            // By label, so a picture in two sets is in the sheet once.
            let mut frames = BTreeMap::new();
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
            if frames.is_empty() {
                continue;
            }
            let read = read_frames(&table, frames.values().copied(), |l| image.at(l))?;
            let pictures = frames
                .iter()
                .map(|(&label, number)| {
                    let picture = read[number].picture(None, Mirror::default())?;
                    Ok((label.to_string(), picture))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let name = format!("{CHARACTER_SHEET}{}", character.slot);
            let sheet = sprite_sheet(
                &pictures,
                &leading,
                &[(FOLLOWING, &following)],
                &format!("{name}.png"),
            )?;
            files.push((format!("{SPRITES}/{name}.json"), sheet.json.into_bytes()));
            files.push((format!("{SPRITES}/{name}.png"), sheet.png));
        }
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
        assert_eq!(pack.notes.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
