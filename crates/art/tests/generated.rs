//! The generated art and sound checked in under `content/base`.

use std::path::{Path, PathBuf};

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
