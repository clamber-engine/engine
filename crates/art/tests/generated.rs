//! The generated art and sound checked in under `content/base`.

use std::path::{Path, PathBuf};

use clamber_sprite::SpriteSheet;

fn content() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/base")
}

#[test]
fn checked_in_files_match_their_sources() {
    let outputs = clamber_art::generate(&content()).unwrap();
    let stale: Vec<_> = outputs
        .iter()
        .filter(|o| std::fs::read(content().join(&o.path)).ok().as_ref() != Some(&o.bytes))
        .map(|o| o.path.display().to_string())
        .collect();
    assert!(
        stale.is_empty(),
        "out of date (run `cargo run -p clamber-art`): {stale:?}"
    );
}

#[test]
fn every_sheet_loads_with_its_animations() {
    let outputs = clamber_art::generate(&content()).unwrap();
    let sheets: Vec<_> = outputs
        .iter()
        .filter(|o| o.path.extension().is_some_and(|e| e == "json"))
        .collect();
    assert!(!sheets.is_empty());
    for out in sheets {
        let sprite = SpriteSheet::load(&content().join(&out.path)).unwrap();
        assert!(
            !sprite.sheet.animations.is_empty(),
            "{}",
            out.path.display()
        );
    }
}
