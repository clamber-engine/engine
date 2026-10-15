//! `clamber-import`: import a game image the player owns as a personal
//! content pack.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clamber_import::{Image, Importer, packs_dir};

const USAGE: &str = "\
usage: clamber-import <image> [--into <dir>] [--replace]

Reads your own copy of a supported game and writes a personal content
pack for Clamber to play. The pack is yours alone: it can't be exported
or shared.

--into     put the pack in <dir> instead of your packs folder
--replace  overwrite an earlier import of the same game
";

fn main() -> ExitCode {
    let mut image = None;
    let mut into = None;
    let mut replace = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--replace" => replace = true,
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
    match import(&image, &packs, replace) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("clamber-import: {message}");
            ExitCode::FAILURE
        }
    }
}

fn import(image: &Path, packs: &Path, replace: bool) -> Result<(), String> {
    let image = Image::read(image).map_err(|e| e.to_string())?;
    let imported = Importer::builtin()
        .import(&image, packs, replace)
        .map_err(|e| e.to_string())?;
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
    println!("This is a personal import: it's yours to play, and can't be shared.");
    Ok(())
}

fn usage(message: &str) -> ExitCode {
    eprintln!("clamber-import: {message}\n\n{USAGE}");
    ExitCode::from(2)
}
