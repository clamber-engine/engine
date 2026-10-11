//! `clamber-level`: check levels and import Tiled maps.

use std::path::Path;
use std::process::ExitCode;

use clamber_level::{Level, tiled};

const USAGE: &str = "usage:
  clamber-level check <file.level>...
  clamber-level import-tiled <map.json> [-o <out.level>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check", files @ ..] if !files.is_empty() => files.iter().try_for_each(|f| {
            Level::load(Path::new(f))
                .map(|_| println!("{f}: ok"))
                .map_err(|e| e.to_string())
        }),
        ["import-tiled", map] => tiled::import_file(Path::new(map))
            .map(|l| print!("{}", l.to_toml_string()))
            .map_err(|e| e.to_string()),
        ["import-tiled", map, "-o", out] => tiled::import_file(Path::new(map))
            .map_err(|e| e.to_string())
            .and_then(|l| {
                std::fs::write(out, l.to_toml_string()).map_err(|e| format!("{out}: {e}"))
            }),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
