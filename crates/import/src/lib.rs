//! The importer: turns a player's own game image into a personal content
//! pack.
//!
//! An [`Image`] (a file without its copier header) is recognized against
//! a [`Catalog`] of supported images, then each [`Decoder`] for its title
//! finds its data by [`Location`] (from [`locate`]) and converts part of it to Clamber's
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
//! format, written by a [`LevelDecoder`], and its sprite sheets in
//! [`SPRITES`]. [`installed`] lists the
//! packs in a folder, and [`check_shareable`] refuses to let anything
//! inside a personal import be exported or shared.
//!
//! The `clamber-import` command and the game's import menu both call
//! [`Importer::import`].

mod characters;
mod collectibles;
mod echo;
mod graphics;
mod ground;
mod image;
mod installed;
mod kinds;
mod level;
mod level_map;
mod level_start;
mod location;
mod locators;
mod manifest;
mod music;
mod nibbles;
mod object_hashes;
mod objects;
mod placements;
mod replacements;
mod songs;
mod sound;
mod sprites;
mod view_limits;
mod voice_output;
mod voices;

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use clamber_audio::encode::DEFAULT_QUALITY;

pub use characters::{
    CHARACTER_SHEET, CharacterDecoder, CharacterPictures, FOLLOWING, MAP_SHEET, PictureSet,
};
pub use collectibles::{Collectibles, GROUP_SIZE, Group, Item, SPARK, Shape};
pub use graphics::{
    Background, GridPicture, HorizonBlock, NightColours, PaletteSet, Pictures, RowColours,
    ScreenMap, Sky, TILE, TileEntry, TileSet, channels, far_colour, rgb, unpack,
};
pub use ground::{Below, COLUMN_BANDS, Ground, Half, Layout, Surface, TITLE_2_ORIGIN};
pub use image::{
    COPIER_HEADER, Catalog, IMAGE_EXTENSIONS, Image, Known, MAX_IMAGE_SIZE, Outcome, Sha256Hash,
};
pub use installed::{CURRENT, Installed, check_shareable, current, installed, tidy};
pub use kinds::{
    CRITTER, DEFAULT_THROWS, END, EXIT, FINGERPRINT_SIZE, Heading, KEG, Kind, KindRow,
    KindSettings, Patrol, ROOM_ENTRANCE, SLING, SettingsRow, Throw, WALKER, WALL_EXIT, WALL_RETURN,
    candidate, kind_of, kind_rows, object_of, settings_rows, timetable,
};
pub use level::{
    CollectibleParts, FROM_LEVEL, LevelDecoder, LevelParts, PackedGround, ROOM_RETURN,
    SCENE_RETURN, SECOND_WALL_RETURN_ARRIVAL, Scenery, SkyParts, WALL_RETURN_ARRIVAL, water,
};
pub use level_map::{
    APPEARANCE_OFFSET, CORNER_SIZE, Direction, FIRST_CORNER, LAST_CORNER, LEVEL_MAP, LevelMap,
    LevelMapDecoder, Link, MAPS, PAGES, PageLocations, Point, Rules, StopKind, TABLE_SIZE,
    corner_count, game_over_picture, game_over_picture_path, is_corner, page_locations,
    page_picture_path, page_pictures,
};
pub use level_start::{START_RECORD_SIZE, StartRecord};
pub use location::{BANK_SIZE, Location};
pub use locators::{Data, first_level, first_level_ground, first_room, first_scene, locate};
pub use manifest::{FORMAT, MANIFEST, MAX_VIEW, Manifest, NotShareable, Widescreen};
pub use music::{Fit, LevelSong, Music, RATE, render, silent_frames};
pub use nibbles::{NIBBLE_HEADER, unpack_nibbles};
pub use objects::{ObjectDecoder, ObjectPalette, ObjectSheet, PENDING_SHEETS, written_sheets};
pub use placements::{Class, ENTRY_SIZE, Entry, Placement, Placements, RECORD_SIZE, Record};
pub use replacements::{
    ART_FORMAT, ART_MANIFEST, ArtManifest, DUMP_INDEX, MAX_FACTOR, PICTURES, Replacements, art_dir,
    dump_pictures, frame_hash, image_hash, replace_image, replace_sheet, replace_tileset,
};
pub use songs::{
    CHANNELS, Effects, GAIN, HEADER_SIZE, OUTPUTS_PER_PERIOD, Player, START_PERIOD, Settings,
    SettingsSet, Step, UNIT_PITCH, Voice, VoiceStep, read_effects,
};
pub use sound::{
    BLOCK_SIZE, BlockHeader, END_OF_LIST, Loop, Passes, Read, STEPS, Sample, SampleAudio, Song,
    read_list, read_samples, read_set, read_song, song_samples,
};
pub use sprites::{
    Form, Frame, FrameTable, HASHED_OPAQUE, Header, LAST_SHORT, Mirror, Picture, Piece, Runs,
    SheetFiles, Size, SpritePalette, TABLE_ENTRY_SIZE, TableEntry, find_pictures, read_frames,
    sprite_sheet, sprite_sheet_in, with_dim_colours,
};

pub use view_limits::{VIEW_HEIGHT, VIEW_WIDTH, VerticalLimit, ViewLimits};
pub use voice_output::{Mixer, VoiceOutput};
pub use voices::{Envelope, MAX_LEVEL, Part, due, period};

/// The folder in a pack that holds its levels.
pub const LEVELS: &str = "levels";

/// The file name, without `.level`, of an imported title's first level.
pub const FIRST_LEVEL: &str = "level-1";

/// The file name, without `.level`, of the room the first level's upper
/// exit leads to. It sorts after [`FIRST_LEVEL`], so the first level is
/// still a pack's first.
pub const FIRST_ROOM: &str = "room-1";

/// The file name, without `.level`, of the scene the first level's floor
/// exit leads to. It sorts after [`FIRST_LEVEL`] too.
pub const FIRST_SCENE: &str = "scene-1";

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
    /// The file isn't a supported game image (`spec/formats/rom-image.md`,
    /// "Identifying an image").
    NotAGame(String),
    /// The image is a supported game's, but this build can't convert it.
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
    /// Something, as a file manager window, held the pack's folder or a
    /// file in it open so that the new import couldn't take the old
    /// one's place. Trying again once it's closed will work.
    Blocked {
        /// The pack's folder.
        folder: PathBuf,
        /// What the system said, for the report.
        detail: String,
    },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Read(message) => write!(f, "can't read the image: {message}"),
            ImportError::NotAGame(reason) => {
                write!(f, "this doesn't look like a supported game image: {reason}")
            }
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
            ImportError::Blocked { folder, .. } => write!(
                f,
                "{} wouldn't let Clamber replace the old copy, in {}. It's probably \
                 open in {} or another program. Close it and try again.",
                if cfg!(windows) {
                    "Windows"
                } else {
                    "The system"
                },
                folder.display(),
                if cfg!(windows) {
                    "File Explorer"
                } else {
                    "a file manager"
                },
            ),
        }
    }
}

impl std::error::Error for ImportError {}

/// An optional part of an image that couldn't be converted. The pack
/// goes without it, and the game uses its own in its place: Clamber's
/// music for a song.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub part: Optional,
    /// Which one, for the player: "song 0".
    pub name: String,
    pub reason: String,
    /// Counts, step numbers and states for finding out why, one line
    /// each. Like error messages, they never quote the image.
    pub diagnostics: Vec<String>,
}

/// The kinds of part an import can go without.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Optional {
    Song,
}

impl Optional {
    /// What the player hears instead, after "using".
    fn instead(self) -> &'static str {
        match self {
            Optional::Song => "Clamber's music",
        }
    }

    fn noun(self, count: usize) -> &'static str {
        match (self, count) {
            (Optional::Song, 1) => "song",
            (Optional::Song, _) => "songs",
        }
    }
}

impl fmt::Display for Skipped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "couldn't convert {}: {}; using {} instead",
            self.name,
            self.reason,
            self.part.instead()
        )
    }
}

/// A short note for the player on what an import went without, a line
/// for each kind of part: "3 songs couldn't be converted; using
/// Clamber's music instead." The same part skipped twice (as a song two
/// levels share) counts once.
pub fn skipped_summary(skipped: &[Skipped]) -> Vec<String> {
    let parts: BTreeSet<(Optional, &str)> =
        skipped.iter().map(|s| (s.part, s.name.as_str())).collect();
    let kinds: BTreeSet<Optional> = parts.iter().map(|&(part, _)| part).collect();
    kinds
        .into_iter()
        .map(|kind| {
            let count = parts.iter().filter(|&&(part, _)| part == kind).count();
            format!(
                "{count} {} couldn't be converted; using {} instead.",
                kind.noun(count),
                kind.instead()
            )
        })
        .collect()
}

/// Runs the conversion of an optional part, so that a fault in it (a
/// panic) only skips the part rather than stopping the import. The
/// reason given doesn't quote the panic's message, which could say
/// anything.
pub(crate) fn optional<T>(convert: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(convert))
        .unwrap_or_else(|_| Err("the converter stopped unexpectedly".into()))
}

/// A text report on an import, for the player to share when something
/// went wrong: what was imported or why it wasn't, which optional parts
/// were skipped, and their diagnostics. It never quotes the image.
pub fn import_report(result: &Result<Imported, ImportError>) -> String {
    let mut out = format!(
        "Clamber import report (importer {})\n",
        env!("CARGO_PKG_VERSION")
    );
    match result {
        Ok(imported) => {
            let m = &imported.manifest;
            out += &format!("Imported {} ({})\n", m.title, m.release);
            for decoder in &imported.decoded {
                out += &format!("  converted the {decoder}\n");
            }
            for note in &imported.notes {
                out += &format!("  {note}\n");
            }
            if imported.skipped.is_empty() {
                out += "Nothing was skipped.\n";
            }
            for skipped in &imported.skipped {
                out += &format!("\n{}\n", capitalize(&skipped.to_string()));
                for line in &skipped.diagnostics {
                    out += &format!("  {line}\n");
                }
            }
        }
        Err(e) => {
            out += &format!("Didn't import: {e}\n");
            if let ImportError::Blocked { detail, .. } = e {
                out += &format!("  {detail}\n");
            }
        }
    }
    out
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Collects a pack's files while it's being imported, in a folder of its
/// own that only becomes the pack once everything is written.
pub struct PackWriter {
    dir: PathBuf,
    written: BTreeSet<PathBuf>,
    skipped: Vec<Skipped>,
    notes: Vec<String>,
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

    /// Notes that an optional part couldn't be converted, and goes on
    /// without it.
    pub fn skip(&mut self, skipped: Skipped) {
        self.skipped.push(skipped);
    }

    /// Notes something about how a part was converted for the import
    /// report: counts and states, never quoting the image.
    pub fn note(&mut self, note: String) {
        self.notes.push(note);
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
    /// The import's folder, inside the pack's folder.
    pub dir: PathBuf,
    /// The decoders that ran, in order.
    pub decoded: Vec<String>,
    /// The optional parts that couldn't be converted, in the order they
    /// were met.
    pub skipped: Vec<Skipped>,
    /// Notes on how parts were converted, for the import report.
    pub notes: Vec<String>,
}

pub struct Importer {
    pub catalog: Catalog,
    pub decoders: Vec<Box<dyn Decoder>>,
}

impl Importer {
    /// The images and decoders this build supports: each supported
    /// title's first level, the room its upper exit leads to, the scene
    /// its floor exit leads to, and its level map, where the spec says where their parts are.
    pub fn builtin() -> Importer {
        Importer::with_audio_quality(DEFAULT_QUALITY)
    }

    /// [`builtin`](Importer::builtin), encoding music at Vorbis quality
    /// `audio_quality` (-1 to 10) rather than [`DEFAULT_QUALITY`].
    pub fn with_audio_quality(audio_quality: f32) -> Importer {
        let catalog = Catalog::builtin();
        let mut decoders: Vec<Box<dyn Decoder>> = Vec::new();
        for known in &catalog.known {
            let areas = [
                (FIRST_LEVEL, "First level", first_level(&known.title)),
                (FIRST_ROOM, "First level's room", first_room(&known.title)),
                (
                    FIRST_SCENE,
                    "First level's scene",
                    first_scene(&known.title),
                ),
            ];
            for (file, name, parts) in areas {
                if let Some(parts) = parts {
                    decoders.push(Box::new(LevelDecoder {
                        title: known.title.clone(),
                        file: file.to_string(),
                        name: name.to_string(),
                        parts,
                        kinds: kind_rows(&known.title),
                        settings: settings_rows(&known.title),
                        audio_quality,
                    }));
                }
            }
            if let Some(characters) = CharacterDecoder::of(&known.title) {
                decoders.push(Box::new(characters));
            }
            if let Some(objects) = ObjectDecoder::of(&known.title) {
                decoders.push(Box::new(objects));
            }
            if Rules::of(&known.title).is_some() && locate(&known.title, Data::MapPoints).is_some()
            {
                decoders.push(Box::new(LevelMapDecoder {
                    title: known.title.clone(),
                    audio_quality,
                }));
            }
        }
        Importer { catalog, decoders }
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
        let top = packs_dir.join(&id);
        if top.exists() && !replace {
            return Err(ImportError::AlreadyImported(top));
        }
        fs::create_dir_all(packs_dir).map_err(|e| write_err(packs_dir, e))?;
        let staging = packs_dir.join(format!(".importing-{id}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir(&staging).map_err(|e| write_err(&staging, e))?;

        let result = self
            .fill(image, known, &staging, progress)
            .and_then(|imported| Ok((imported, install(&staging, &top)?)));
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        let ((manifest, decoded, pack), dir) = result?;
        Ok(Imported {
            manifest,
            dir,
            decoded,
            skipped: pack.skipped,
            notes: pack.notes,
        })
    }

    /// Runs the decoders into `staging`, then writes the manifest.
    fn fill(
        &self,
        image: &Image,
        known: &Known,
        staging: &Path,
        progress: &mut dyn FnMut(&str),
    ) -> Result<(Manifest, Vec<String>, PackWriter), ImportError> {
        let mut pack = PackWriter {
            dir: staging.to_path_buf(),
            written: BTreeSet::new(),
            skipped: Vec::new(),
            notes: Vec::new(),
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
            widescreen: Some(Widescreen::OriginalActivation),
        };
        let path = staging.join(MANIFEST);
        fs::write(&path, manifest.to_toml())
            .map_err(|e| ImportError::Write(format!("{}: {e}", path.display())))?;
        Ok((manifest, decoded, pack))
    }
}

/// Moves the finished import in `staging` into the pack folder `top`,
/// in a folder of its own, and makes it the one in use (the
/// [`installed`] module). Returns its folder. Neither `top` nor an
/// earlier import in it is renamed or removed, so a window open on
/// them doesn't stop it; an earlier import is left for [`tidy`].
fn install(staging: &Path, top: &Path) -> Result<PathBuf, ImportError> {
    let blocked = |e: std::io::Error, path: &Path| match e.kind() {
        std::io::ErrorKind::PermissionDenied => ImportError::Blocked {
            folder: top.to_path_buf(),
            detail: format!("{}: {e}", path.display()),
        },
        _ => ImportError::Write(format!("{}: {e}", path.display())),
    };
    retrying(|| fs::create_dir_all(top)).map_err(|e| blocked(e, top))?;
    // After the imports there already, whichever is in use.
    let number = fs::read_dir(top)
        .map_err(|e| blocked(e, top))?
        .filter_map(|e| {
            let name = e.ok()?.file_name();
            name.to_str()?.strip_prefix("import-")?.parse::<u64>().ok()
        })
        .max()
        .unwrap_or(0)
        + 1;
    let name = format!("import-{number}");
    let dir = top.join(&name);
    retrying(|| fs::rename(staging, &dir)).map_err(|e| blocked(e, &dir))?;

    // Written aside, then moved over the old one, so that it's always
    // whole.
    let pointer = top.join(CURRENT);
    let next = top.join(format!(".{CURRENT}-{}", std::process::id()));
    let switched = fs::write(&next, format!("{name}\n"))
        .map_err(|e| blocked(e, &next))
        .and_then(|()| retrying(|| fs::rename(&next, &pointer)).map_err(|e| blocked(e, &pointer)));
    if let Err(e) = switched {
        let _ = fs::remove_file(&next);
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(dir)
}

/// Runs `op` until it succeeds, or for about two seconds while the
/// system refuses it: what's in the way (a virus scanner reading the
/// files just written, say) is often gone by then.
fn retrying<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut wait = std::time::Duration::from_millis(20);
    let mut waited = std::time::Duration::ZERO;
    loop {
        match op() {
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    && waited < std::time::Duration::from_secs(2) =>
            {
                std::thread::sleep(wait);
                waited += wait;
                wait = (wait * 2).min(std::time::Duration::from_millis(500));
            }
            result => return result,
        }
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
