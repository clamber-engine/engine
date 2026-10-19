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
        .filter(|o| !std::fs::read(content().join(&o.path)).is_ok_and(|b| o.is_current(&b)))
        .map(|o| o.path.display().to_string())
        .collect();
    assert!(
        stale.is_empty(),
        "out of date (run `cargo run -p clamber-art`): {stale:?}"
    );
}

/// The sound effects are Ogg Vorbis that the game decodes, with every
/// sample of their source.
#[test]
fn checked_in_sounds_decode() {
    let outputs = clamber_art::generate(&content()).unwrap();
    let mut oggs = 0;
    for o in &outputs {
        if o.path.extension().is_none_or(|e| e != "ogg") {
            continue;
        }
        oggs += 1;
        let input = clamber_audio::Input::file(content().join(&o.path));
        let (theirs, rate) = clamber_audio::Sound::decode(&input).unwrap();
        let input = clamber_audio::Input::bytes(o.bytes.clone(), Some("ogg"));
        let (ours, _) = clamber_audio::Sound::decode(&input).unwrap();
        let what = o.path.display();
        assert_eq!(
            (rate, theirs.channels),
            (clamber_art::synth::RATE, 1),
            "{what}"
        );
        assert_eq!(theirs.frames(), ours.frames(), "{what}");
    }
    assert_eq!(oggs, 11);
}
