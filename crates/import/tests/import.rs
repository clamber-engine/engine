//! Importing made-up images with test catalogs and decoders, and the
//! `clamber-import` command.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clamber_import::{
    CURRENT, Catalog, Decoder, Image, ImportError, Importer, Installed, Known, MANIFEST, Manifest,
    NotShareable, PackWriter, check_shareable, installed, tidy,
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
    Image::new((0..=255u8).cycle().take(1024).collect())
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
    assert_eq!(imported.dir, packs.join("title-9/import-1"));
    assert_eq!(entries(&packs.join("title-9")), [CURRENT, "import-1"]);
    assert_eq!(
        fs::read_to_string(packs.join("title-9").join(CURRENT)).unwrap(),
        "import-1\n"
    );
    assert_eq!(imported.decoded, ["probe"]);
    assert_eq!(
        fs::read_to_string(imported.dir.join("levels/size.txt")).unwrap(),
        "1024"
    );
    let manifest = Manifest::load(&imported.dir).unwrap();
    assert_eq!(manifest, imported.manifest);
    assert!(!manifest.redistribute);
    assert!(manifest.ensure_shareable().is_err());
    // Imports are pixel art; this made-up title has no view size.
    assert!(manifest.pixel_art);
    assert_eq!(manifest.view, None);
    assert_eq!(entries(&packs), ["title-9"]);
}

#[test]
fn knows_the_view_sizes_the_spec_gives() {
    assert_eq!(clamber_import::view_of("title-1"), Some([256, 224]));
    assert_eq!(clamber_import::view_of("title-9"), None);
}

#[test]
fn keeps_an_earlier_import_unless_replacing() {
    let packs = scratch("replace");
    let importer = importer(false);
    importer.import(&image(), &packs, false).unwrap();
    fs::write(packs.join("title-9/import-1/extra"), "old").unwrap();
    assert_eq!(
        importer.import(&image(), &packs, false),
        Err(ImportError::AlreadyImported(packs.join("title-9")))
    );
    assert!(packs.join("title-9/import-1/extra").exists());

    let imported = importer.import(&image(), &packs, true).unwrap();
    assert_eq!(imported.dir, packs.join("title-9/import-2"));
    assert!(!imported.dir.join("extra").exists());
    assert_eq!(
        Installed::load(&packs.join("title-9")).unwrap().dir,
        imported.dir
    );
    assert_eq!(entries(&packs), ["title-9"]);
    // The earlier import goes once nothing could be using it.
    assert_eq!(
        entries(&packs.join("title-9")),
        [CURRENT, "import-1", "import-2"]
    );
    tidy(&packs);
    assert_eq!(entries(&packs.join("title-9")), [CURRENT, "import-2"]);
}

/// Holds `path` open, as a file manager window or a program reading a
/// file does. On Windows that stops it being renamed or removed.
fn hold_open(path: &Path) -> fs::File {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Folders open only with FILE_FLAG_BACKUP_SEMANTICS. Sharing
        // reads and writes but not deleting, like a file manager.
        options.custom_flags(0x0200_0000).share_mode(0x1 | 0x2);
    }
    options.open(path).unwrap()
}

#[test]
fn replaces_an_import_while_its_folder_and_files_are_open() {
    let packs = scratch("held");
    let importer = importer(false);
    importer.import(&image(), &packs, false).unwrap();
    let held = [
        hold_open(&packs.join("title-9")),
        hold_open(&packs.join("title-9/import-1")),
        hold_open(&packs.join("title-9/import-1/levels")),
        hold_open(&packs.join("title-9/import-1/levels/size.txt")),
    ];

    let imported = importer.import(&image(), &packs, true).unwrap();
    assert_eq!(imported.dir, packs.join("title-9/import-2"));
    let pack = Installed::load(&packs.join("title-9")).unwrap();
    assert_eq!(pack.dir, imported.dir);
    // Tidying leaves what's held for later, and doesn't touch the import
    // in use.
    tidy(&packs);
    assert_eq!(
        fs::read_to_string(imported.dir.join("levels/size.txt")).unwrap(),
        "1024"
    );

    drop(held);
    tidy(&packs);
    assert_eq!(entries(&packs.join("title-9")), [CURRENT, "import-2"]);
    // And once more over that.
    let imported = importer.import(&image(), &packs, true).unwrap();
    assert_eq!(imported.dir, packs.join("title-9/import-3"));
}

#[test]
fn replaces_a_pack_imported_before_imports_had_folders() {
    let packs = scratch("unversioned");
    let importer = importer(false);
    let imported = importer.import(&image(), &packs, false).unwrap();
    // Laid out the old way: the pack's files right in its folder.
    let top = packs.join("title-9");
    fs::remove_file(top.join(CURRENT)).unwrap();
    for name in [MANIFEST, "levels"] {
        fs::rename(imported.dir.join(name), top.join(name)).unwrap();
    }
    fs::remove_dir(&imported.dir).unwrap();
    assert_eq!(Installed::load(&top).unwrap().dir, top);
    tidy(&packs);
    assert_eq!(entries(&top), ["levels", MANIFEST]);

    let _held = hold_open(&top);
    let imported = importer.import(&image(), &packs, true).unwrap();
    assert_eq!(imported.dir, top.join("import-1"));
    assert_eq!(Installed::load(&top).unwrap().dir, imported.dir);
    tidy(&packs);
    assert_eq!(entries(&top), [CURRENT, "import-1"]);
}

/// Where the system refuses to change the pack's folder, as Windows
/// does while something holds it open, the earlier import stays.
#[cfg(unix)]
#[test]
fn a_blocked_replacement_keeps_the_earlier_import() {
    use std::os::unix::fs::PermissionsExt;
    let packs = scratch("blocked");
    let importer = importer(false);
    importer.import(&image(), &packs, false).unwrap();
    let top = packs.join("title-9");
    let mode = |m| fs::set_permissions(&top, fs::Permissions::from_mode(m));
    mode(0o555).unwrap();
    let err = importer.import(&image(), &packs, true).unwrap_err();
    mode(0o755).unwrap();
    let ImportError::Blocked { folder, detail } = err else {
        panic!("{err}");
    };
    assert_eq!(folder, top);
    assert!(detail.contains("import-2"), "{detail}");
    assert_eq!(entries(&packs), ["title-9"]);
    assert_eq!(entries(&top), [CURRENT, "import-1"]);
    assert_eq!(Installed::load(&top).unwrap().dir, top.join("import-1"));
    let imported = importer.import(&image(), &packs, true).unwrap();
    assert_eq!(imported.dir, top.join("import-2"));
}

#[test]
fn says_plainly_when_something_blocks_replacing() {
    let folder = PathBuf::from("packs").join("title-9");
    let blocked = ImportError::Blocked {
        folder: folder.clone(),
        detail: "made-up detail".into(),
    };
    let message = blocked.to_string();
    assert!(message.contains("replace the old copy"), "{message}");
    assert!(message.contains(&folder.display().to_string()), "{message}");
    assert!(message.ends_with("Close it and try again."), "{message}");
    assert!(!message.contains("made-up detail"), "{message}");
    let report = clamber_import::import_report(&Err(blocked));
    assert!(report.contains("made-up detail"), "{report}");
}

#[test]
fn a_pointer_to_somewhere_else_is_refused() {
    let packs = scratch("pointer");
    importer(false).import(&image(), &packs, false).unwrap();
    let top = packs.join("title-9");
    for bad in ["../title-9", ".hidden", "a/b", ""] {
        fs::write(top.join(CURRENT), bad).unwrap();
        assert!(Installed::load(&top).is_err(), "{bad:?}");
        assert!(check_shareable(&top).is_err(), "{bad:?}");
    }
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
    assert_eq!(entries(&packs.join("title-9")), [CURRENT, "import-1"]);
    assert_eq!(
        fs::read_to_string(packs.join("title-9/import-1/levels/size.txt")).unwrap(),
        "1024"
    );
}

#[test]
fn refuses_unknown_images_before_writing() {
    let packs = scratch("unknown").join("packs");
    let err = importer(false)
        .import(&Image::new(vec![9; 1024]), &packs, false)
        .unwrap_err();
    assert!(matches!(err, ImportError::NotAGame(_)), "{err}");
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
fn reports_each_decoder_as_it_starts() {
    let packs = scratch("progress");
    let mut started = Vec::new();
    importer(false)
        .import_with_progress(&image(), &packs, false, &mut |name| {
            started.push(name.to_string())
        })
        .unwrap();
    assert_eq!(started, ["probe"]);
}

#[test]
fn lists_installed_packs_and_their_levels() {
    let packs = scratch("installed");
    assert_eq!(installed(&packs.join("none")), (Vec::new(), Vec::new()));
    importer(false).import(&image(), &packs, false).unwrap();
    let levels = packs.join("title-9/import-1/levels");
    fs::write(levels.join("b.level"), "").unwrap();
    fs::write(levels.join("a.level"), "").unwrap();
    fs::create_dir(packs.join(".importing-title-9-1")).unwrap();
    fs::create_dir(packs.join("stray")).unwrap();

    let (found, problems) = installed(&packs);
    assert_eq!(found.len(), 1);
    let pack = &found[0];
    assert_eq!(pack.manifest.id, "title-9");
    assert_eq!(
        pack.levels(),
        [levels.join("a.level"), levels.join("b.level")]
    );
    assert_eq!(pack.sprites(), None);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("stray"), "{problems:?}");

    fs::rename(packs.join("title-9"), packs.join("renamed")).unwrap();
    let (found, problems) = installed(&packs);
    assert!(found.is_empty());
    assert!(
        problems.iter().any(|p| p.contains("named after")),
        "{problems:?}"
    );
}

#[test]
fn nothing_in_a_personal_import_is_shareable() {
    let packs = scratch("shareable");
    let imported = importer(false).import(&image(), &packs, false).unwrap();
    let refused = Err(NotShareable("title-9".into()));
    assert_eq!(check_shareable(&imported.dir), refused);
    assert_eq!(
        check_shareable(&imported.dir.join("levels/size.txt")),
        refused
    );
    assert_eq!(check_shareable(&imported.dir.join("not/there")), refused);
    assert_eq!(check_shareable(&packs.join("title-9")), refused);
    assert_eq!(check_shareable(&packs), Ok(()));

    let other = scratch("shareable-other");
    fs::write(other.join(MANIFEST), "name = \"not a pack\"\n").unwrap();
    assert_eq!(check_shareable(&other.join("file")), Ok(()));
    fs::write(other.join(MANIFEST), "redistribute = true\n").unwrap();
    assert_eq!(check_shareable(&other.join("file")), Ok(()));
    fs::write(other.join(MANIFEST), "redistribute = 'yes'\n").unwrap();
    assert!(check_shareable(&other.join("file")).is_err());
}

#[test]
fn reads_images_with_and_without_copier_headers() {
    let dir = scratch("headers");
    let mut with_header = vec![0xAA; clamber_import::COPIER_HEADER];
    with_header.extend(image().bytes());
    for (name, bytes) in [
        ("plain.img", image().bytes().to_vec()),
        ("header.img", with_header),
    ] {
        fs::write(dir.join(name), bytes).unwrap();
        let read = Image::read(&dir.join(name)).unwrap();
        assert_eq!(read.sha256(), image().sha256(), "{name}");
    }
    fs::write(dir.join("odd.img"), [0; 1000]).unwrap();
    let err = Image::read(&dir.join("odd.img")).unwrap_err();
    assert!(err.to_string().contains("no image has its size"), "{err}");
}

#[test]
fn refuses_known_images_it_cant_convert() {
    let packs = scratch("no-decoder").join("packs");
    let mut importer = importer(false);
    importer.decoders.clear();
    let err = importer.import(&image(), &packs, false).unwrap_err();
    assert!(matches!(err, ImportError::Unsupported(_)), "{err}");
    assert!(err.to_string().contains("can't convert it yet"), "{err}");
    assert!(!packs.exists());
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
    assert!(stderr.contains("no supported image is"), "{stderr}");
    assert!(out.stdout.is_empty());
    assert!(!dir.join("packs").exists());

    let out = Command::new(env!("CARGO_BIN_EXE_clamber-import"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
