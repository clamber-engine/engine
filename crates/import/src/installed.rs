//! The packs that have been imported, and keeping them from being
//! shared.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{LEVELS, MANIFEST, Manifest, NotShareable, SPRITES};

/// An imported pack in the packs folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The pack's folder.
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Installed {
    /// Reads the pack in `dir`, whose name must be the pack's id.
    pub fn load(dir: &Path) -> Result<Installed, String> {
        let manifest = Manifest::load(dir)?;
        if dir.file_name() != Some(manifest.id.as_ref()) {
            return Err(format!(
                "{}: the folder isn't named after the pack's id, `{}`",
                dir.display(),
                manifest.id
            ));
        }
        Ok(Installed {
            dir: dir.to_path_buf(),
            manifest,
        })
    }

    /// The pack's `.level` files, by name. None if it has no levels
    /// folder.
    pub fn levels(&self) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(self.dir.join(LEVELS)) else {
            return Vec::new();
        };
        let mut levels: Vec<PathBuf> = entries
            .filter_map(|e| Some(e.ok()?.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "level"))
            .collect();
        levels.sort();
        levels
    }

    /// The pack's sprites folder, if it has one.
    pub fn sprites(&self) -> Option<PathBuf> {
        Some(self.dir.join(SPRITES)).filter(|d| d.is_dir())
    }
}

/// The packs in `packs_dir`, by id, and why any other folders there
/// couldn't be read as packs. There are none if the folder doesn't
/// exist. Folders starting with `.` (imports in progress) are skipped.
pub fn installed(packs_dir: &Path) -> (Vec<Installed>, Vec<String>) {
    let entries = match fs::read_dir(packs_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), Vec::new()),
        Err(e) => return (Vec::new(), vec![format!("{}: {e}", packs_dir.display())]),
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.is_dir())
        .filter(|p| {
            !p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with('.')
        })
        .collect();
    dirs.sort();
    let (mut packs, mut problems) = (Vec::new(), Vec::new());
    for dir in dirs {
        match Installed::load(&dir) {
            Ok(pack) => packs.push(pack),
            Err(e) => problems.push(e),
        }
    }
    (packs, problems)
}

/// Refuses `path` if it's a personal import or anything inside one: a
/// folder whose manifest doesn't say `redistribute = true`. Anything
/// that exports or shares files, or pictures of them, calls this first.
pub fn check_shareable(path: &Path) -> Result<(), NotShareable> {
    let path = fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    for dir in path.ancestors() {
        let manifest = dir.join(MANIFEST);
        if !manifest.is_file() {
            continue;
        }
        let name = || {
            dir.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        // A manifest that can't be read might be a personal import's.
        let Ok(src) = fs::read_to_string(&manifest) else {
            return Err(NotShareable(name()));
        };
        let Ok(table) = src.parse::<toml::Table>() else {
            return Err(NotShareable(name()));
        };
        let Some(redistribute) = table.get("redistribute") else {
            // Some other kind of manifest.
            continue;
        };
        if redistribute.as_bool() != Some(true) {
            let id = table.get("id").and_then(|v| v.as_str()).map(String::from);
            return Err(NotShareable(id.unwrap_or_else(name)));
        }
    }
    Ok(())
}
