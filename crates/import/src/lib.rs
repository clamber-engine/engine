//! The importer: turns a player's own game image into a personal content
//! pack.
//!
//! An [`Image`] (a file without its copier header) is recognized against
//! a [`Catalog`] of supported images, then each [`Decoder`] for its title
//! finds its data by [`Location`] and converts part of it to Clamber's
//! own formats, written through a [`PackWriter`]. The pack goes into the
//! player's [`packs_dir`], never into the game's folder or a source
//! tree, with a [`Manifest`] marking it as a personal import that can't
//! be shared.
//!
//! The image's bytes never leave memory: errors, logs and `Debug` output
//! describe an image by its size and hash only. Decoders keep to that
//! too.
//!
//! A pack's levels are in its [`LEVELS`] folder, in Clamber's level
//! format, and its sprite sheets in [`SPRITES`]. [`installed`] lists the
//! packs in a folder, and [`check_shareable`] refuses to let anything
//! inside a personal import be exported or shared.
//!
//! The `clamber-import` command and the game's import menu both call
//! [`Importer::import`].

mod ground;
mod image;
mod installed;
mod location;
mod manifest;

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub use ground::{Below, COLUMN_BANDS, Ground, Layout, Surface};
pub use image::{COPIER_HEADER, Catalog, Image, Known, MAX_IMAGE_SIZE, Outcome, Sha256Hash};
pub use installed::{Installed, check_shareable, installed};
pub use location::{BANK_SIZE, Location};
pub use manifest::{FORMAT, MANIFEST, MAX_VIEW, Manifest, NotShareable};

/// The folder in a pack that holds its levels.
pub const LEVELS: &str = "levels";

/// The folder in a pack that holds its sprite sheets.
pub const SPRITES: &str = "sprites";

/// Converts one part of a supported image (graphics, a level, …) to
/// Clamber's formats. Imports can run on a thread of their own, so
/// decoders can be shared between threads.
pub trait Decoder: Send + Sync {
    /// What it decodes, for progress and error messages.
    fn name(&self) -> &str;
    /// Whether it has anything to decode in this image.
    fn applies_to(&self, image: &Known) -> bool;
    /// Writes its files into the pack. Error messages must not quote the
    /// image's bytes.
    fn decode(&self, image: &Image, pack: &mut PackWriter) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The image file couldn't be read.
    Read(String),
    /// The image isn't one the importer supports.
    Unsupported(String),
    /// The pack can't go there.
    Destination(String),
    /// The title has been imported already, and replacing wasn't asked for.
    AlreadyImported(PathBuf),
    Decode {
        decoder: String,
        message: String,
    },
    Write(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Read(message) => write!(f, "can't read the image: {message}"),
            ImportError::Unsupported(reason) => write!(f, "can't import this file: {reason}"),
            ImportError::Destination(message) => write!(f, "can't put the pack there: {message}"),
            ImportError::AlreadyImported(dir) => write!(
                f,
                "this game has already been imported, to {}; import again with \
                 replacing turned on to overwrite it",
                dir.display()
            ),
            ImportError::Decode { decoder, message } => {
                write!(f, "couldn't convert the {decoder}: {message}")
            }
            ImportError::Write(message) => write!(f, "can't write the pack: {message}"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Collects a pack's files while it's being imported, in a folder of its
/// own that only becomes the pack once everything is written.
pub struct PackWriter {
    dir: PathBuf,
    written: BTreeSet<PathBuf>,
}

impl PackWriter {
    /// Writes a file at `path`, relative to the pack's folder, using `/`
    /// between folders.
    pub fn write(&mut self, path: &str, contents: &[u8]) -> Result<(), String> {
        let rel = Path::new(path);
        let plain = rel.components().all(|c| matches!(c, Component::Normal(_)));
        if path.is_empty() || !plain || path.contains('\\') {
            return Err(format!("`{path}` isn't a plain relative path"));
        }
        if rel == Path::new(MANIFEST) {
            return Err(format!("`{MANIFEST}` is written by the importer"));
        }
        if !self.written.insert(rel.to_path_buf()) {
            return Err(format!("`{path}` was written twice"));
        }
        let full = self.dir.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&full, contents).map_err(|e| format!("{}: {e}", full.display()))
    }

    /// The files written so far, relative to the pack's folder.
    pub fn files(&self) -> impl Iterator<Item = &Path> {
        self.written.iter().map(PathBuf::as_path)
    }
}

/// A finished import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    pub manifest: Manifest,
    /// The pack's folder.
    pub dir: PathBuf,
    /// The decoders that ran, in order.
    pub decoded: Vec<String>,
}

pub struct Importer {
    pub catalog: Catalog,
    pub decoders: Vec<Box<dyn Decoder>>,
}

impl Importer {
    /// The images and decoders this build supports.
    pub fn builtin() -> Importer {
        Importer {
            catalog: Catalog::builtin(),
            decoders: Vec::new(),
        }
    }

    /// Imports `image` as a pack in `packs_dir`, named after its title.
    /// An existing import of the same title is kept unless `replace`. If
    /// anything fails, nothing is left behind and an earlier import is
    /// untouched.
    pub fn import(
        &self,
        image: &Image,
        packs_dir: &Path,
        replace: bool,
    ) -> Result<Imported, ImportError> {
        self.import_with_progress(image, packs_dir, replace, &mut |_| {})
    }

    /// [`import`](Importer::import), passing each decoder's name to
    /// `progress` as it starts.
    pub fn import_with_progress(
        &self,
        image: &Image,
        packs_dir: &Path,
        replace: bool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Imported, ImportError> {
        let known = self.catalog.identify(image)?;
        if !self.decoders.iter().any(|d| d.applies_to(known)) {
            return Err(ImportError::Unsupported(format!(
                "it's {}, but this version of Clamber can't convert it yet",
                known.title
            )));
        }
        check_destination(packs_dir)?;
        let write_err =
            |path: &Path, e: std::io::Error| ImportError::Write(format!("{}: {e}", path.display()));

        let id = known.title.clone();
        let dest = packs_dir.join(&id);
        if dest.exists() && !replace {
            return Err(ImportError::AlreadyImported(dest));
        }
        fs::create_dir_all(packs_dir).map_err(|e| write_err(packs_dir, e))?;
        let staging = packs_dir.join(format!(".importing-{id}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir(&staging).map_err(|e| write_err(&staging, e))?;

        let result = self
            .fill(image, known, &staging, progress)
            .and_then(|imported| {
                let old = packs_dir.join(format!(".replaced-{id}-{}", std::process::id()));
                let had_old = dest.exists();
                if had_old {
                    fs::rename(&dest, &old).map_err(|e| write_err(&dest, e))?;
                }
                if let Err(e) = fs::rename(&staging, &dest) {
                    if had_old {
                        let _ = fs::rename(&old, &dest);
                    }
                    return Err(write_err(&dest, e));
                }
                if had_old {
                    let _ = fs::remove_dir_all(&old);
                }
                Ok(imported)
            });
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        let (manifest, decoded) = result?;
        Ok(Imported {
            manifest,
            dir: dest,
            decoded,
        })
    }

    /// Runs the decoders into `staging`, then writes the manifest.
    fn fill(
        &self,
        image: &Image,
        known: &Known,
        staging: &Path,
        progress: &mut dyn FnMut(&str),
    ) -> Result<(Manifest, Vec<String>), ImportError> {
        let mut pack = PackWriter {
            dir: staging.to_path_buf(),
            written: BTreeSet::new(),
        };
        let mut decoded = Vec::new();
        for decoder in self.decoders.iter().filter(|d| d.applies_to(known)) {
            progress(decoder.name());
            decoder
                .decode(image, &mut pack)
                .map_err(|message| ImportError::Decode {
                    decoder: decoder.name().to_string(),
                    message,
                })?;
            decoded.push(decoder.name().to_string());
        }
        let manifest = Manifest {
            format: FORMAT,
            id: known.title.clone(),
            name: format!("Imported {}", known.title),
            title: known.title.clone(),
            release: known.release.clone(),
            importer: env!("CARGO_PKG_VERSION").to_string(),
            redistribute: false,
            pixel_art: true,
            view: view_of(&known.title),
        };
        let path = staging.join(MANIFEST);
        fs::write(&path, manifest.to_toml())
            .map_err(|e| ImportError::Write(format!("{}: {e}", path.display())))?;
        Ok((manifest, decoded))
    }
}

/// How much of a level each title shows at once, `[width, height]` in
/// pixels, where the spec gives it: `behavior/view.md` and
/// `behavior/view-vertical.md` for `title-1`.
pub fn view_of(title: &str) -> Option<[u32; 2]> {
    match title {
        "title-1" => Some([256, 224]),
        _ => None,
    }
}

/// Refuses folders inside a source tree or the game's own folder, where
/// a personal pack could end up shared along with them.
fn check_destination(dir: &Path) -> Result<(), ImportError> {
    let absolute = std::path::absolute(dir)
        .map_err(|e| ImportError::Destination(format!("{}: {e}", dir.display())))?;
    // Resolve links through the part that exists.
    let mut existing = absolute.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        let Some(parent) = existing.parent() else {
            break;
        };
        rest.push(existing.file_name().unwrap_or_default().to_owned());
        existing = parent;
    }
    let mut resolved = fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    resolved.extend(rest.iter().rev());

    let game_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| fs::canonicalize(exe.parent()?).ok());
    for ancestor in resolved.ancestors() {
        if ancestor.join(".git").exists() {
            return Err(ImportError::Destination(format!(
                "{} is inside the source tree at {}",
                dir.display(),
                ancestor.display()
            )));
        }
        if game_dir.as_deref() == Some(ancestor) {
            return Err(ImportError::Destination(format!(
                "{} is inside the game's folder",
                dir.display()
            )));
        }
    }
    Ok(())
}

/// Where imported packs go: a `packs` folder in the platform's per-user
/// data folder (`%LOCALAPPDATA%\Clamber\packs` on Windows). `None` when
/// the environment doesn't say where that is.
pub fn packs_dir() -> Option<PathBuf> {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let data = if cfg!(windows) {
        var("LOCALAPPDATA")?.join("Clamber")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/Clamber")
    } else {
        var("XDG_DATA_HOME")
            .or_else(|| Some(var("HOME")?.join(".local/share")))?
            .join("clamber")
    };
    Some(data.join("packs"))
}
