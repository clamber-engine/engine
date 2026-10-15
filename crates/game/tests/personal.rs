//! Personal imports can't be captured for sharing.

use std::process::Command;

#[test]
fn captures_refuse_levels_in_a_personal_import() {
    let dir = std::env::temp_dir().join(format!("clamber-personal-{}", std::process::id()));
    let levels = dir.join("title-9/levels");
    std::fs::create_dir_all(&levels).unwrap();
    std::fs::write(
        dir.join("title-9/manifest.toml"),
        "id = \"title-9\"\nredistribute = false\n",
    )
    .unwrap();
    let level = levels.join("first.level");
    std::fs::write(&level, "").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_clamber-game"))
        .arg("--capture")
        .arg(&level)
        .args(["--inputs", "none.scn", "--out"])
        .arg(dir.join("media"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("can't capture"), "{stderr}");
    assert!(stderr.contains("can't be exported or shared"), "{stderr}");
    assert!(!dir.join("media").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
