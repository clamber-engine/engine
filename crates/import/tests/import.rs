//! Importing made-up images with test catalogs and decoders, and the
//! `clamber-import` command.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clamber_import::{
    Catalog, Decoder, Image, ImportError, Importer, Known, MANIFEST, Manifest, PackWriter,
};

/// A fresh folder outside the source tree.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("clamber-import-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A made-up image: not any real game's.
fn image() -> Image {
    Image::new((0..=255u8).cycle().take(1000).collect())
}

/// Copies the image's length into a file, or fails.
struct Probe {
    fail: bool,
}

impl Decoder for Probe {
    fn name(&self) -> &str {
        "probe"
    }

    fn applies_to(&self, image: &Known) -> bool {
        image.title == "title-9"
    }

    fn decode(&self, image: &Image, pack: &mut PackWriter) -> Result<(), String> {
        if self.fail {
            return Err("made-up failure".into());
        }
        pack.write("levels/size.txt", image.len().to_string().as_bytes())
    }
}

fn importer(fail: bool) -> Importer {
    let image = image();
    Importer {
        catalog: Catalog {
            known: vec![Known {
                title: "title-9".into(),
                release: "made-up release".into(),
                size: image.len() as u64,
                sha256: image.sha256(),
            }],
        },
        decoders: vec![Box::new(Probe { fail })],
    }
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn writes_a_personal_pack() {
    let packs = scratch("writes").join("packs");
    let imported = importer(false).import(&image(), &packs, false).unwrap();
    assert_eq!(imported.dir, packs.join("title-9"));
    assert_eq!(imported.decoded, ["probe"]);
    assert_eq!(
        fs::read_to_string(imported.dir.join("levels/size.txt")).unwrap(),
        "1000"
    );
    let manifest = Manifest::load(&imported.dir).unwrap();
    assert_eq!(manifest, imported.manifest);
    assert!(!manifest.redistribute);
    assert!(manifest.ensure_shareable().is_err());
    assert_eq!(entries(&packs), ["title-9"]);
}

#[test]
fn keeps_an_earlier_import_unless_replacing() {
    let packs = scratch("replace");
    let importer = importer(false);
    importer.import(&image(), &packs, false).unwrap();
    fs::write(packs.join("title-9/extra"), "old").unwrap();
    assert_eq!(
        importer.import(&image(), &packs, false),
        Err(ImportError::AlreadyImported(packs.join("title-9")))
    );
    assert!(packs.join("title-9/extra").exists());

    importer.import(&image(), &packs, true).unwrap();
    assert!(!packs.join("title-9/extra").exists());
    assert!(packs.join("title-9").join(MANIFEST).exists());
    assert_eq!(entries(&packs), ["title-9"]);
}

#[test]
fn a_failed_import_leaves_nothing_behind() {
    let packs = scratch("fails");
    importer(false).import(&image(), &packs, false).unwrap();
    let err = importer(true).import(&image(), &packs, true).unwrap_err();
    assert_eq!(
        err.to_string(),
        "couldn't convert the probe: made-up failure"
    );
    assert_eq!(entries(&packs), ["title-9"]);
    assert_eq!(
        fs::read_to_string(packs.join("title-9/levels/size.txt")).unwrap(),
        "1000"
    );
}

#[test]
fn refuses_unknown_images_before_writing() {
    let packs = scratch("unknown").join("packs");
    let err = importer(false)
        .import(&Image::new(vec![9; 1000]), &packs, false)
        .unwrap_err();
    assert!(matches!(err, ImportError::Unsupported(_)), "{err}");
    assert!(err.to_string().contains("modified"), "{err}");
    assert!(!packs.exists());
}

#[test]
fn refuses_the_source_tree() {
    let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("packs-test");
    let err = importer(false)
        .import(&image(), &inside, false)
        .unwrap_err();
    assert!(matches!(err, ImportError::Destination(_)), "{err}");
    assert!(!inside.exists());
}

#[test]
fn decoders_write_only_plain_paths() {
    struct Bad(&'static str);
    impl Decoder for Bad {
        fn name(&self) -> &str {
            "bad"
        }
        fn applies_to(&self, _: &Known) -> bool {
            true
        }
        fn decode(&self, _: &Image, pack: &mut PackWriter) -> Result<(), String> {
            pack.write(self.0, b"x")
        }
    }
    for path in ["../escape", "/abs", "", MANIFEST, "a/../b", "a\\b"] {
        let packs = scratch("paths");
        let mut importer = importer(false);
        importer.decoders = vec![Box::new(Bad(path))];
        let err = importer.import(&image(), &packs, false).unwrap_err();
        assert!(matches!(err, ImportError::Decode { .. }), "{path}: {err}");
        assert!(entries(&packs).is_empty(), "{path}");
    }
}

#[test]
fn the_command_refuses_images_it_doesnt_know() {
    let dir = scratch("cli");
    let file = dir.join("game.img");
    fs::write(&file, image().bytes()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_clamber-import"))
        .arg(&file)
        .arg("--into")
        .arg(dir.join("packs"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("can't import any games yet"), "{stderr}");
    assert!(out.stdout.is_empty());
    assert!(!dir.join("packs").exists());

    let out = Command::new(env!("CARGO_BIN_EXE_clamber-import"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
