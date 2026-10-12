//! `clamber-art [--check] [content folder]`: regenerates the placeholder
//! art and sound listed in `art/art.toml`, by default in `content/base`.
//! With `--check`, writes nothing and fails if any file is out of date.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut check = false;
    let mut content = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--check" => check = true,
            "-h" | "--help" => {
                println!("usage: clamber-art [--check] [content folder]");
                return ExitCode::SUCCESS;
            }
            _ if content.is_none() => content = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("error: unexpected argument `{arg}`");
                return ExitCode::FAILURE;
            }
        }
    }
    let content = content
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/base"));

    let outputs = match clamber_art::generate(&content) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut stale = 0;
    for out in &outputs {
        let path = content.join(&out.path);
        if std::fs::read(&path).is_ok_and(|b| b == out.bytes) {
            continue;
        }
        if check {
            eprintln!("out of date: {}", out.path.display());
            stale += 1;
            continue;
        }
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, &out.bytes));
        if let Err(e) = written {
            eprintln!("error: {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", out.path.display());
    }
    if stale > 0 {
        eprintln!("run `cargo run -p clamber-art` to regenerate");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
