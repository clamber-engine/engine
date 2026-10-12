//! Whole levels played from the start to the goal with recorded inputs,
//! so CI proves they stay completable. Each scenario in
//! `tests/playthrough/` names the level it plays in `requires`, relative
//! to the repository root, and must reach the goal on the default preset.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_level::Level;
use clamber_sim::{Outcome, Script, World, generic_feel, run};
use clamber_trace::Scenario;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Plays the scenario and returns the frame the goal was reached on.
fn play(file: &Path) -> Result<u32, String> {
    let scenario = Scenario::load(file).map_err(|e| e.to_string())?;
    let level = Level::load(&root().join(&scenario.requires)).map_err(|e| e.to_string())?;
    let feel = generic_feel("default").unwrap();
    let mut world = World::try_new(Arc::new(level), Arc::new(feel)).map_err(|e| e.to_string())?;
    let mut reached = None;
    run(&mut world, &Script::from_scenario(&scenario), |w| {
        if w.flow.outcome == Outcome::Complete && reached.is_none() {
            reached = Some(w.frame);
        }
    });
    reached.ok_or_else(|| {
        let p = &world.player;
        format!(
            "didn't reach the goal: ended {:?} at ({}, {}) on frame {}, {} lives left",
            world.flow.outcome, p.pos.x, p.pos.y, world.frame, world.flow.lives
        )
    })
}

#[test]
fn every_playthrough_reaches_the_goal() {
    let dir = root().join("tests/playthrough");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "scn"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no playthroughs in {}", dir.display());
    let failures: Vec<String> = files
        .iter()
        .filter_map(|f| play(f).err().map(|e| format!("{}: {e}", f.display())))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
