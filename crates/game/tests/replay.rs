//! Headless replays of a made-up imported pack: the trace, the pictures
//! and the parts they're made of, from the library and from
//! `clamber-replay`.

use std::path::{Path, PathBuf};
use std::process::Command;

use clamber_game::replay::{INPUTS_HEADER, Replay, load_inputs, parse_inputs, preset_for};
use clamber_import::{FORMAT, Installed, MANIFEST};
use clamber_sim::Phase;
use clamber_sprite::{RgbaImage, decode_png, encode_png};
use clamber_trace::{Buttons, Decimal, Trace};

const GROUND: [u8; 4] = [40, 120, 60, 255];
const HAZE: [u8; 4] = [250, 250, 250, 255];

/// A pack of `title-1` in a folder of its own, with a 256x224 view, as the title's own and one
/// level: flat ground, an opaque layer that moves with the world, a
/// half see-through one fixed to the view, and two restart points out of
/// view, a checkpoint and then a keg.
fn made_up_pack(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("clamber-replay-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let pack = dir.join("title-1");
    std::fs::create_dir_all(pack.join("levels")).unwrap();
    std::fs::write(
        pack.join(MANIFEST),
        format!(
            "format = {FORMAT}\nid = \"title-1\"\nname = \"Made up\"\ntitle = \"title-1\"\n\
             release = \"none\"\nimporter = \"0\"\nredistribute = false\npixel_art = true\n\
             view = [256, 224]\nwidescreen = \"original-activation\"\n"
        ),
    )
    .unwrap();
    let solid = |w, h, colour| {
        let mut image = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                image.set(x, y, colour);
            }
        }
        encode_png(&image)
    };
    std::fs::write(
        pack.join("levels/first-ground.png"),
        solid(512, 256, GROUND),
    )
    .unwrap();
    std::fs::write(pack.join("levels/first-haze.png"), solid(8, 8, HAZE)).unwrap();
    std::fs::write(
        pack.join("levels/first.level"),
        r#"
format = 1
name = "Made up"
size = [512, 256]
start = [32, 200]

[[layers]]
name = "ground"
image = "first-ground.png"
pixel_art = true

[[layers]]
name = "haze two"
image = "first-haze.png"
pixel_art = true
parallax = [0, 0]
offset = [4, 4]
opacity = 0.5

[[collision]]
shape = "rect"
rect = [0, 200, 512, 56]

[[entities]]
type = "checkpoint"
at = [300, 200]

[[entities]]
type = "keg"
id = "barrel"
at = [400, 191]
"#,
    )
    .unwrap();
    pack
}

/// `frames` frames of walking right.
fn walk(frames: usize) -> Vec<Buttons> {
    vec![Buttons::parse_trace("R").unwrap(); frames]
}

fn over(under: [u8; 4], top: [u8; 4]) -> [u8; 4] {
    let a = u32::from(top[3]);
    let mix = |u: u8, t: u8| ((u32::from(t) * a + u32::from(u) * (255 - a) + 127) / 255) as u8;
    [
        mix(under[0], top[0]),
        mix(under[1], top[1]),
        mix(under[2], top[2]),
        255,
    ]
}

#[test]
fn a_title_s_pack_plays_on_its_preset() {
    assert_eq!(preset_for("title-1"), "classic-1");
    assert_eq!(preset_for("title-9"), "classic-1", "the starting preset");
}

#[test]
fn replays_are_the_same_every_time() {
    let pack = made_up_pack("same");
    let run = || {
        let mut replay = Replay::begin(Installed::load(&pack).unwrap(), None, None, None).unwrap();
        let rows: Vec<_> = walk(30).into_iter().map(|b| replay.step(b)).collect();
        (rows, replay.picture())
    };
    let (rows, picture) = run();
    assert_eq!(run(), (rows.clone(), picture.clone()));
    assert_eq!(rows.len(), 30);
    assert_eq!(rows[0].frame, 0);
    assert!(rows[29].x > rows[0].x, "walked right");
    assert_eq!(
        (picture.width, picture.height),
        (256, 224),
        "the pack's view"
    );
    std::fs::remove_dir_all(pack.parent().unwrap()).unwrap();
}

#[test]
fn the_parts_make_up_the_picture() {
    let pack = made_up_pack("parts");
    let mut replay =
        Replay::begin(Installed::load(&pack).unwrap(), Some("first"), None, None).unwrap();
    for buttons in walk(10) {
        replay.step(buttons);
    }
    let parts = replay.parts();
    let names: Vec<_> = parts.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "backdrop",
            "layer-0-ground",
            "layer-1-haze_two",
            "sprites",
            "hud"
        ]
    );
    let [backdrop, ground, haze, sprites, hud] = [0, 1, 2, 3, 4].map(|i| &parts[i].1);
    assert!(backdrop.data.iter().all(|p| p[3] == 255));
    assert!(ground.data.iter().all(|&p| p == GROUND));
    // The haze is an 8x8 square at (4, 4), half see-through.
    assert_eq!(haze.get(0, 0), [0; 4]);
    let [r, g, b, a] = haze.get(4, 4);
    assert!((126..=129).contains(&a), "{a}");
    assert!([r, g, b].iter().all(|&c| c >= 248), "{:?}", [r, g, b]);
    assert_eq!(haze.get(12, 12), [0; 4]);
    assert!(sprites.data.iter().any(|p| p[3] == 255), "the player");
    assert!(sprites.data.iter().any(|p| p[3] == 0));

    let picture = replay.picture();
    for y in 0..224 {
        for x in 0..256 {
            let mut p = [0, 0, 0, 255];
            for part in [backdrop, ground, haze, sprites, hud] {
                p = over(p, part.get(x, y));
            }
            // The player's own sheet is lit, which can make its
            // see-through edges brighter than a part can hold.
            if !matches!(sprites.get(x, y)[3], 0 | 255) {
                continue;
            }
            let c = picture.pixels[(y * 256 + x) as usize];
            let want = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
            for i in 0..3 {
                assert!(p[i].abs_diff(want[i]) <= 2, "{x},{y}: {p:?} isn't {want:?}");
            }
        }
    }
    std::fs::remove_dir_all(pack.parent().unwrap()).unwrap();
}

#[test]
fn a_level_or_arrival_that_isn_t_there_is_an_error() {
    let pack = made_up_pack("missing");
    let installed = || Installed::load(&pack).unwrap();
    let err = |r: Result<Replay, String>| r.err().expect("an error");
    assert!(err(Replay::begin(installed(), Some("second"), None, None)).contains("no level"));
    assert!(err(Replay::begin(installed(), None, None, Some("back"))).contains("no arrival"));
    assert!(err(Replay::begin(installed(), None, Some("bogus"), None)).contains("no preset"));
    std::fs::remove_dir_all(pack.parent().unwrap()).unwrap();
}

#[test]
fn a_replay_can_come_back_at_a_restart_point() {
    let pack = made_up_pack("restart");
    let begin = || Replay::begin(Installed::load(&pack).unwrap(), None, None, None).unwrap();
    let mut replay = begin();
    replay.come_back_at("barrel").unwrap();
    let row = replay.step(Buttons::NONE);
    let world = &replay.session.game.world;
    let at = world.restart_point(1);
    assert_eq!(world.player.pos, at, "back at the keg's restart point");
    assert!(world.resume.is_some(), "inside the keg");
    assert_eq!(world.things[1].phase, Phase::Broken);
    assert_eq!((row.frame, row.x, row.y), (0, Decimal::ZERO, Decimal::ZERO));

    let mut by_number = begin();
    by_number.come_back_at("1").unwrap();
    assert_eq!(by_number.step(Buttons::NONE), row);
    let mut checkpoint = begin();
    checkpoint.come_back_at("0").unwrap();
    checkpoint.step(Buttons::NONE);
    let world = &checkpoint.session.game.world;
    assert_eq!(world.player.pos, world.level.entities[0].at);
    assert!(world.resume.is_none());

    assert!(begin().come_back_at("2").unwrap_err().contains("has 2"));
    assert!(begin().come_back_at("nowhere").is_err());
    assert!(replay.come_back_at("barrel").is_err(), "play has begun");
    std::fs::remove_dir_all(pack.parent().unwrap()).unwrap();
}

#[test]
fn inputs_are_read_a_line_a_frame() {
    let text = format!("{INPUTS_HEADER}\n# walk, then jump\nR\n\nRJ\n.\n");
    let held = parse_inputs(&text, "test").unwrap();
    let codes: Vec<_> = held.iter().map(|b| b.to_string()).collect();
    assert_eq!(codes, ["R", "RJ", "."]);
    assert!(
        parse_inputs("R\n", "test")
            .unwrap_err()
            .contains("first line")
    );
    let bad = parse_inputs(&format!("{INPUTS_HEADER}\nR\nQ\n"), "test").unwrap_err();
    assert!(bad.starts_with("test:3:"), "{bad}");
}

#[test]
fn clamber_replay_writes_the_trace_and_the_pictures() {
    let pack = made_up_pack("cli");
    let dir = pack.parent().unwrap();
    let inputs = dir.join("walk.inputs");
    std::fs::write(&inputs, format!("{INPUTS_HEADER}\n{}", "R\n".repeat(8))).unwrap();
    assert_eq!(load_inputs(&inputs).unwrap(), walk(8));
    let out = dir.join("out");
    let run = Command::new(env!("CARGO_BIN_EXE_clamber-replay"))
        .arg("--pack")
        .arg(&pack)
        .arg("--inputs")
        .arg(&inputs)
        .arg("--out")
        .arg(&out)
        .args([
            "--level", "first", "--frames", "2..7", "--every", "2", "--layers",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");

    let trace = std::fs::read_to_string(out.join("replay.trace")).unwrap();
    let trace = Trace::parse(&trace, "replay.trace").unwrap();
    assert_eq!(trace.scenario, "walk");
    assert_eq!(trace.rows.len(), 7, "every frame up to the last picture");
    let header = |key: &str| {
        trace
            .extra
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(header("title"), Some("title-1"));
    assert_eq!(header("preset"), Some("classic-1"));

    let mut files: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    let mut want = vec!["replay.trace".to_string()];
    for frame in ["000002", "000004", "000006"] {
        want.push(format!("{frame}.png"));
        for part in [
            "backdrop",
            "hud",
            "layer-0-ground",
            "layer-1-haze_two",
            "sprites",
        ] {
            want.push(format!("{frame}-{part}.png"));
        }
    }
    want.sort();
    assert_eq!(files, want);
    let size = |path: &Path| {
        let image = decode_png(&std::fs::read(path).unwrap()).unwrap().to_rgba();
        (image.width, image.height)
    };
    assert_eq!(size(&out.join("000004.png")), (256, 224));
    assert_eq!(size(&out.join("000004-sprites.png")), (256, 224));
    std::fs::remove_dir_all(dir).unwrap();
}
