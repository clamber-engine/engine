//! `clamber-import`: import a game image the player owns as a personal
//! content pack.

use std::path::PathBuf;
use std::process::ExitCode;

use clamber_import::{Image, Imported, Importer, import_report, packs_dir, skipped_summary};

const USAGE: &str = "\
usage: clamber-import <image> [--into <dir>] [--replace] [--import-report <file>]
                      [--audio-quality <q>]

Reads your own copy of a supported game and writes a personal content
pack for Clamber to play. The pack is yours alone: it can't be exported
or shared.

--into <dir>            put the pack in <dir> instead of your packs folder
--replace               overwrite an earlier import of the same game
--import-report <file>  write a report on the import to <file>, to share
                        when something went wrong; it has nothing from
                        the game in it
--audio-quality <q>     encode music at Ogg Vorbis quality <q>, from -1
                        (smallest) to 10 (best); 6 by default
";

fn main() -> ExitCode {
    let mut image = None;
    let mut into = None;
    let mut replace = false;
    let mut report = None;
    let mut quality = clamber_audio::encode::DEFAULT_QUALITY;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--replace" => replace = true,
            "--import-report" => match args.next() {
                Some(file) => report = Some(PathBuf::from(file)),
                None => return usage("`--import-report` needs a file"),
            },
            "--audio-quality" => match args.next().map(|q| q.parse::<f32>()) {
                Some(Ok(q)) if (-1.0..=10.0).contains(&q) => quality = q,
                _ => return usage("`--audio-quality` needs a number from -1 to 10"),
            },
            "--into" => match args.next() {
                Some(dir) => into = Some(PathBuf::from(dir)),
                None => return usage("`--into` needs a folder"),
            },
            flag if flag.starts_with("--") => return usage(&format!("unknown option `{flag}`")),
            _ if image.is_some() => return usage("give one image"),
            _ => image = Some(PathBuf::from(arg)),
        }
    }
    let Some(image) = image else {
        return usage("missing the image to import");
    };
    let Some(packs) = into.or_else(packs_dir) else {
        eprintln!("clamber-import: can't tell where your packs folder is; use --into <dir>");
        return ExitCode::from(2);
    };
    let image = match Image::read(&image) {
        Ok(image) => image,
        Err(e) => {
            eprintln!("clamber-import: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = Importer::with_audio_quality(quality).import(&image, &packs, replace);
    if let Some(file) = report {
        match std::fs::write(&file, import_report(&result)) {
            Ok(()) => eprintln!("clamber-import: wrote the report to {}", file.display()),
            Err(e) => eprintln!("clamber-import: {}: {e}", file.display()),
        }
    }
    match result {
        Ok(imported) => {
            show(&imported);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("clamber-import: {e}");
            ExitCode::FAILURE
        }
    }
}

fn show(imported: &Imported) {
    for skipped in &imported.skipped {
        eprintln!("clamber-import: warning: {skipped}");
    }
    let m = &imported.manifest;
    println!(
        "Imported {} ({}) to {}",
        m.title,
        m.release,
        imported.dir.display()
    );
    for decoder in &imported.decoded {
        println!("  converted the {decoder}");
    }
    for line in skipped_summary(&imported.skipped) {
        println!("{line}");
    }
    println!("This is a personal import: it's yours to play, and can't be shared.");
}

fn usage(message: &str) -> ExitCode {
    eprintln!("clamber-import: {message}\n\n{USAGE}");
    ExitCode::from(2)
}
