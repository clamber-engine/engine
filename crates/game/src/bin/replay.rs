//! `clamber-replay`: plays an input recording on a level of an imported
//! pack headlessly, and writes its trace and the picture of its frames
//! (TESTING.md, "Tools").

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clamber_game::encode_png;
use clamber_game::replay::{Replay, load_inputs};
use clamber_import::Installed;
use clamber_trace::Trace;
use clamber_trace::trace::Source;

const USAGE: &str = "\
usage:
  clamber-replay --pack <dir> --inputs <file> --out <dir> [--level <id>]
                 [--preset <id>] [--arrival <name>] [--frames <from>..<to>]
                 [--every <n>] [--layers]

Plays the inputs on a level of the imported pack in <dir> from its start
(or from its arrival <name>), with no window, and writes into --out:

  replay.trace          the trace of every frame played, from frame 0
  <frame>.png           the picture of each frame in --frames (default:
                        all of them), every <n>th (default 1), at the
                        pack's own resolution, as Original rendering shows it
  <frame>-<part>.png    with --layers, each part of the game's picture on
                        its own: backdrop, layer-<i>-<name>, sprites, hud

<frame> is the frame number, six digits; a picture shows the state after
the frame's step. Inputs are a scenario (.scn), the input column of a
trace (.trace), or a `# clamber-inputs v1` file with the actions held on
each frame, one line per frame (`.` for none). The level is a file name
in the pack's levels folder without `.level` (default: the first), and
the preset defaults to the one holding the pack's title's values.
";

struct Options {
    pack: PathBuf,
    inputs: PathBuf,
    out: PathBuf,
    level: Option<String>,
    preset: Option<String>,
    arrival: Option<String>,
    frames: Option<Range<u32>>,
    every: u32,
    layers: bool,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let options = match parse(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("clamber-replay: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("clamber-replay: {message}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String]) -> Result<Options, String> {
    let (mut pack, mut inputs, mut out) = (None, None, None);
    let (mut level, mut preset, mut arrival) = (None, None, None);
    let (mut frames, mut every, mut layers) = (None, None, false);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--layers" {
            layers = true;
            continue;
        }
        let slot = match arg.as_str() {
            "--pack" => &mut pack,
            "--inputs" => &mut inputs,
            "--out" => &mut out,
            "--level" => &mut level,
            "--preset" => &mut preset,
            "--arrival" => &mut arrival,
            "--frames" => &mut frames,
            "--every" => &mut every,
            other => return Err(format!("unknown argument `{other}`")),
        };
        let value = iter
            .next()
            .ok_or_else(|| format!("`{arg}` needs a value"))?;
        if slot.replace(value.clone()).is_some() {
            return Err(format!("`{arg}` is given twice"));
        }
    }
    let needed =
        |value: Option<String>, name: &str| value.ok_or_else(|| format!("`--{name}` is needed"));
    let every = match every {
        Some(n) => n
            .parse()
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| format!("`--every {n}` isn't a whole number above 0"))?,
        None => 1,
    };
    Ok(Options {
        pack: needed(pack, "pack")?.into(),
        inputs: needed(inputs, "inputs")?.into(),
        out: needed(out, "out")?.into(),
        level,
        preset,
        arrival,
        frames: frames.as_deref().map(range).transpose()?,
        every,
        layers,
    })
}

/// `<from>..<to>`, frames `from` up to but not including `to`; either
/// may be left out.
fn range(text: &str) -> Result<Range<u32>, String> {
    let bad = || format!("`--frames {text}` isn't <from>..<to>");
    let (from, to) = text.split_once("..").ok_or_else(bad)?;
    let number = |s: &str, empty: u32| match s {
        "" => Ok(empty),
        s => s.parse().map_err(|_| bad()),
    };
    Ok(number(from, 0)?..number(to, u32::MAX)?)
}

fn run(options: &Options) -> Result<(), String> {
    let held = load_inputs(&options.inputs)?;
    let pack = Installed::load(&options.pack)?;
    let id = pack.manifest.id.clone();
    let mut replay = Replay::begin(
        pack,
        options.level.as_deref(),
        options.preset.as_deref(),
        options.arrival.as_deref(),
    )?;
    let out = &options.out;
    std::fs::create_dir_all(out).map_err(|e| format!("can't make {}: {e}", out.display()))?;
    let frames = options.frames.clone().unwrap_or(0..u32::MAX);
    let mut rows = Vec::new();
    let mut pictures = 0;
    for &buttons in held.iter().take(frames.end as usize) {
        let row = replay.step(buttons);
        let frame = row.frame;
        rows.push(row);
        if !frames.contains(&frame) || !(frame - frames.start).is_multiple_of(options.every) {
            continue;
        }
        let picture = replay.picture();
        write(
            &out.join(format!("{frame:06}.png")),
            &encode_png(&picture, 1)?,
        )?;
        if options.layers {
            for (name, image) in replay.parts() {
                let path = out.join(format!("{frame:06}-{name}.png"));
                write(&path, &clamber_sprite::encode_png(&image))?;
            }
        }
        pictures += 1;
    }
    let world = &replay.session.game.world;
    let mut extra = vec![
        ("preset".to_string(), world.feel.preset.clone()),
        ("pack".to_string(), id),
    ];
    if let Some(title) = &world.feel.title {
        extra.push(("title".to_string(), title.clone()));
    }
    let trace = Trace {
        scenario: stem(&options.inputs),
        source: Source::Engine,
        extra,
        rows,
    };
    write(&out.join("replay.trace"), trace.to_string().as_bytes())?;
    let (width, height) = replay.size();
    println!(
        "replayed {} frames into {}: replay.trace and {pictures} pictures at {width}x{height}",
        replay.frame(),
        out.display()
    );
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("can't write {}: {e}", path.display()))
}

/// The inputs file's name without its extension, as the trace's
/// scenario.
fn stem(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str());
    stem.unwrap_or("replay").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_the_command_line() {
        let o = parse(&args("--pack p --inputs i.scn --out o")).unwrap();
        assert_eq!(
            (o.pack, o.inputs, o.out),
            ("p".into(), "i.scn".into(), "o".into())
        );
        assert_eq!((o.frames, o.every, o.layers), (None, 1, false));
        let o = parse(&args(
            "--pack p --inputs i --out o --level room --preset classic-1 --arrival back \
             --frames 60..120 --every 4 --layers",
        ))
        .unwrap();
        assert_eq!(o.level.as_deref(), Some("room"));
        assert_eq!(o.preset.as_deref(), Some("classic-1"));
        assert_eq!(o.arrival.as_deref(), Some("back"));
        assert_eq!((o.frames, o.every, o.layers), (Some(60..120), 4, true));
        assert_eq!(range("..10").unwrap(), 0..10);
        assert_eq!(range("5..").unwrap(), 5..u32::MAX);
        for bad in [
            "--inputs i --out o",
            "--pack p --inputs i --out o --every 0",
            "--pack p --inputs i --out o --frames 5",
            "--pack p --pack q --inputs i --out o",
            "--pack p --inputs i --out o --bogus",
            "--pack p --inputs i --out",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad}");
        }
    }
}
