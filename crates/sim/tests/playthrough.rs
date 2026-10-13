//! Whole levels played from the start to the goal with recorded inputs,
//! so CI proves they stay completable. Each scenario names the level it
//! plays in `requires`, relative to the repository root. Scenarios in
//! `tests/playthrough/` must reach the goal on the `default` preset, and
//! those in `tests/playthrough/<preset>/` on that preset: a run found for
//! one feel rarely works on another.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_level::Level;
use clamber_sim::{Outcome, Script, World, generic_feel, run};
use clamber_trace::Scenario;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Plays the scenario on `preset` and returns the frame the goal was
/// reached on.
fn play(file: &Path, preset: &str) -> Result<u32, String> {
    let scenario = Scenario::load(file).map_err(|e| e.to_string())?;
    let level = Level::load(&root().join(&scenario.requires)).map_err(|e| e.to_string())?;
    let feel = generic_feel(preset).ok_or_else(|| format!("no preset named `{preset}`"))?;
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

/// The scenarios in `dir`, sorted.
fn scenarios(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "scn"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_playthrough_reaches_the_goal() {
    let dir = root().join("tests/playthrough");
    let mut runs: Vec<(PathBuf, String)> = scenarios(&dir)
        .into_iter()
        .map(|f| (f, "default".to_string()))
        .collect();
    assert!(!runs.is_empty(), "no playthroughs in {}", dir.display());
    let mut presets: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    presets.sort();
    for sub in presets {
        let preset = sub.file_name().unwrap().to_string_lossy().into_owned();
        runs.extend(scenarios(&sub).into_iter().map(|f| (f, preset.clone())));
    }
    let failures: Vec<String> = runs
        .iter()
        .filter_map(|(f, preset)| {
            play(f, preset)
                .err()
                .map(|e| format!("{} on `{preset}`: {e}", f.display()))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The bundled game starts on `classic-1`, so its first level must be
/// completable there too.
#[test]
fn the_first_level_has_a_classic_1_playthrough() {
    let path = root().join("tests/playthrough/classic-1/first-level.scn");
    assert!(path.exists(), "{}", path.display());
}
