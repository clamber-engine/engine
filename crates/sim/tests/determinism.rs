//! Determinism (TESTING.md layer 3): every scenario, run twice, gives
//! byte-identical state after every step, under every built-in preset.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_level::{Level, Vec2};
use clamber_pack::Presets;
use clamber_sim::{Script, World, generic_feel, run};
use clamber_trace::Scenario;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every spec scenario with its test level.
fn scenarios() -> Vec<(Scenario, Arc<Level>)> {
    let dir = root().join("spec/scenarios");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "scn"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no scenarios in {}", dir.display());
    paths
        .iter()
        .map(|path| {
            let scenario = Scenario::load(path).unwrap();
            let level_path = root().join(format!("content/test/{}.level", scenario.id));
            let level = Level::load(&level_path).unwrap();
            (scenario, Arc::new(level))
        })
        .collect()
}

/// The state after each step.
fn record(level: &Arc<Level>, preset: &str, script: &Script) -> Vec<Vec<u8>> {
    let feel = Arc::new(generic_feel(preset).unwrap());
    let mut world = World::new(level.clone(), feel);
    let mut states = vec![world.state_bytes()];
    run(&mut world, script, |w| states.push(w.state_bytes()));
    states
}

#[test]
fn same_inputs_same_state() {
    for (scenario, level) in scenarios() {
        let script = Script::from_scenario(&scenario);
        for preset in Presets::builtin().iter() {
            let first = record(&level, &preset.id, &script);
            let second = record(&level, &preset.id, &script);
            assert_eq!(first.len(), scenario.frames as usize + 1);
            if let Some(step) = (0..first.len()).find(|&i| first[i] != second[i]) {
                panic!(
                    "{} under `{}` diverged after step {step}",
                    scenario.id, preset.id
                );
            }
        }
    }
}

#[test]
fn scenarios_move_the_player() {
    // A determinism test is only worth something if the runs do things.
    for (scenario, level) in scenarios() {
        let feel = Arc::new(generic_feel("default").unwrap());
        let mut world = World::new(level.clone(), feel);
        assert!(world.player.grounded(), "{} starts in the air", scenario.id);
        run(&mut world, &Script::from_scenario(&scenario), |_| {});
        assert_eq!(world.frame, scenario.frames);
        assert!(world.player.pos.x > level.start.x, "{}", scenario.id);
        assert!(world.player.grounded(), "{} ends in the air", scenario.id);
    }
}

#[test]
fn walking_off_a_ledge_lands_below() {
    let (scenario, level) = scenarios()
        .into_iter()
        .find(|(s, _)| s.id == "walk-off-ledge")
        .unwrap();
    let feel = Arc::new(generic_feel("default").unwrap());
    let mut world = World::new(level.clone(), feel);
    let mut events = Vec::new();
    run(&mut world, &Script::from_scenario(&scenario), |w| {
        events.extend(w.events.iter().map(|&e| (w.frame, e)))
    });
    use clamber_sim::Event::{Land, LeaveGround};
    let kinds: Vec<_> = events.iter().map(|&(_, e)| e).collect();
    assert_eq!(kinds, [LeaveGround, Land]);
    assert_eq!(world.player.pos.y, Vec2::ints(0, 400).y);
}
