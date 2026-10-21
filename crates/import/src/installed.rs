//! The packs that have been imported, and keeping them from being
//! shared.
//!
//! Each import of a title goes into a folder of its own inside the
//! pack's folder, and the pack's [`CURRENT`] file names the one in use.
//! Replacing an import writes a new one and switches that file, so it
//! never has to rename or delete the pack's folder, which a file manager
//! window open on it would block. [`tidy`] removes the imports no longer
//! in use, when nothing holds them open.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::{LEVELS, MANIFEST, Manifest, NotShareable, SPRITES};

/// The file in a pack's folder naming the folder, inside it, of the
/// import in use.
pub const CURRENT: &str = "current";

/// The folder holding the import in use in the pack folder `top`: the
/// one its [`CURRENT`] file names, or `top` itself for a pack without
/// one (made by hand, or imported before imports had folders of their
/// own). `None` if it has no [`CURRENT`] file.
pub fn current(top: &Path) -> Result<Option<PathBuf>, String> {
    let path = top.join(CURRENT);
    let name = match fs::read_to_string(&path) {
        Ok(name) => name,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let name = name.trim();
    let mut parts = Path::new(name).components();
    let plain = matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none();
    if !plain || name.starts_with('.') || name.contains(['/', '\\']) {
        return Err(format!(
            "{}: `{name}` isn't the name of a folder in the pack",
            path.display()
        ));
    }
    Ok(Some(top.join(name)))
}

/// An imported pack in the packs folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The pack's folder.
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Installed {
    /// Reads the pack in `dir`, whose name must be the pack's id: the
    /// import its [`CURRENT`] file names, if it has one. `dir` can also
    /// be the folder of the import in use itself.
    pub fn load(dir: &Path) -> Result<Installed, String> {
        let (top, dir) = match current(dir)? {
            Some(version) => (dir, version),
            None => match dir.parent() {
                Some(parent) if current(parent).ok().flatten().as_deref() == Some(dir) => {
                    (parent, dir.to_path_buf())
                }
                _ => (dir, dir.to_path_buf()),
            },
        };
        let manifest = Manifest::load(&dir)?;
        if top.file_name() != Some(manifest.id.as_ref()) {
            return Err(format!(
                "{}: the folder isn't named after the pack's id, `{}`",
                top.display(),
                manifest.id
            ));
        }
        Ok(Installed { dir, manifest })
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

/// Removes what's no longer in use in `packs_dir`: in each pack folder
/// with a [`CURRENT`] file, everything but that file and the import it
/// names (earlier imports), and folders left behind by replacing a pack
/// the way earlier versions did. Anything that can't be removed yet,
/// because something has it open, is left for next time. Nothing that's
/// being played should be removed, so the game does this as it starts.
pub fn tidy(packs_dir: &Path) {
    let Ok(entries) = fs::read_dir(packs_dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(".replaced-") {
            let _ = fs::remove_dir_all(&path);
            continue;
        }
        let Ok(Some(version)) = current(&path) else {
            continue;
        };
        if !version.join(MANIFEST).is_file() {
            continue;
        }
        let Ok(inside) = fs::read_dir(&path) else {
            continue;
        };
        for old in inside.filter_map(Result::ok) {
            let old = old.path();
            if old == version || old.file_name() == Some(CURRENT.as_ref()) {
                continue;
            }
            let _ = match old.is_dir() {
                true => fs::remove_dir_all(&old),
                false => fs::remove_file(&old),
            };
        }
    }
}

/// Refuses `path` if it's a personal import or anything inside one: a
/// folder whose manifest doesn't say `redistribute = true`. Anything
/// that exports or shares files, or pictures of them, calls this first.
pub fn check_shareable(path: &Path) -> Result<(), NotShareable> {
    let path = fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    for dir in path.ancestors() {
        let mut manifest = dir.join(MANIFEST);
        if !manifest.is_file() {
            if !dir.is_dir() {
                continue;
            }
            // A pack folder holding imports.
            match current(dir) {
                Ok(None) => continue,
                Ok(Some(version)) => manifest = version.join(MANIFEST),
                Err(_) => return Err(NotShareable(name_of(dir))),
            }
        }
        let name = || name_of(dir);
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

fn name_of(dir: &Path) -> String {
    dir.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
