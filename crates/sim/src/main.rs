//! `clamber-sim`: run a level headlessly.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use clamber_level::Level;
use clamber_pack::Pack;
use clamber_sim::{Script, World, generic_feel, run};
use clamber_trace::Scenario;

const USAGE: &str = "\
usage:
  clamber-sim <file.level> [--pack <pack.toml>] [--preset <id>]
              (--scenario <file.scn> | --frames <n>)

Runs the level from its start with the scenario's inputs, or with nothing
held for n frames, and prints the final state and a digest of it. Without
--pack, a single character plays with the preset's world physics.";

struct Options<'a> {
    level: &'a str,
    pack: Option<&'a str>,
    preset: Option<&'a str>,
    scenario: Option<&'a str>,
    frames: Option<&'a str>,
}

fn parse(args: &[String]) -> Option<Options<'_>> {
    let mut level = None;
    let mut options = Options {
        level: "",
        pack: None,
        preset: None,
        scenario: None,
        frames: None,
    };
    let mut iter = args.iter().map(String::as_str);
    while let Some(arg) = iter.next() {
        let slot = match arg {
            "--pack" => &mut options.pack,
            "--preset" => &mut options.preset,
            "--scenario" => &mut options.scenario,
            "--frames" => &mut options.frames,
            _ if arg.starts_with('-') || level.is_some() => return None,
            _ => {
                level = Some(arg);
                continue;
            }
        };
        if slot.replace(iter.next()?).is_some() {
            return None;
        }
    }
    options.level = level?;
    (options.scenario.is_some() != options.frames.is_some()).then_some(options)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(options) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match simulate(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("clamber-sim: {e}");
            ExitCode::FAILURE
        }
    }
}

fn simulate(options: &Options) -> Result<(), String> {
    let level = Level::load(Path::new(options.level)).map_err(|e| e.to_string())?;
    let feel = match options.pack {
        Some(path) => {
            let src = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let pack = Pack::from_toml_str(&src, path).map_err(|e| e.to_string())?;
            let selection = pack
                .select_feel(options.preset)
                .map_err(|e| e.to_string())?;
            for warning in &selection.warnings {
                eprintln!("warning: {warning}");
            }
            selection.feel
        }
        None => {
            let preset = options.preset.unwrap_or(clamber_pack::preset::DEFAULT);
            generic_feel(preset).ok_or_else(|| format!("no preset named `{preset}`"))?
        }
    };
    let script = match (options.scenario, options.frames) {
        (Some(path), _) => {
            Script::from_scenario(&Scenario::load(Path::new(path)).map_err(|e| e.to_string())?)
        }
        (_, Some(n)) => Script::idle(
            n.parse()
                .map_err(|_| format!("`--frames {n}` isn't a frame count"))?,
        ),
        _ => unreachable!("parse requires one of them"),
    };

    let mut world = World::new(Arc::new(level), Arc::new(feel));
    run(&mut world, &script, |_| {});

    let p = &world.player;
    println!("frame   {}", world.frame);
    println!("pos     {} {}", p.pos.x, p.pos.y);
    println!("vel     {} {}", p.vel.x, p.vel.y);
    println!("ground  {}", p.grounded());
    println!("facing  {:?}", p.facing);
    println!("state   {}", p.state.name());
    println!("digest  {:016x}", fnv1a(&world.state_bytes()));
    Ok(())
}

/// A short fingerprint of the state, for comparing runs by eye.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}
