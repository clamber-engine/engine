//! Importing a game the player owns, from the title screen, and the
//! games there are to play.
//!
//! A [`Picker`] browses the player's folders for the image, using the
//! same neutral actions as every menu, so it works from a gamepad too.
//! [`Importing`] runs the importer on a thread of its own and shows its
//! progress, then what came of it. [`Games`] lists the bundled game and
//! the imported packs, each marked as a personal import.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use clamber_import::{Image, ImportError, Imported, Importer, Installed, installed};
use clamber_input::{Action, Frame};

use crate::camera::VIEW;
use crate::canvas::Color;
use crate::menu::{Choice, Menu};
use crate::report;
use crate::session::{FAILED, HEADING, SUBTITLE, TITLE};
use crate::surface::Surface;
use crate::text::{LINE, draw_text_centred, wrap};

/// Characters across a line of text on these screens.
const WIDTH: usize = 56;

/// Folder entries shown at once.
const PICKER_ROWS: usize = 11;

/// How a personal import is described, wherever one is shown.
pub const PERSONAL: &str = "Personal import: yours to play, not to share";

/// `text` cut to `width` characters, with `…` where it was cut.
fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Draws `text` wrapped to [`WIDTH`], centred, from `top` down. Returns
/// where the next line would go.
fn paragraph(canvas: &mut dyn Surface, centre: i32, top: i32, text: &str, color: Color) -> i32 {
    let mut y = top;
    for line in wrap(text, WIDTH) {
        draw_text_centred(canvas, centre, y, &line, color);
        y += LINE;
    }
    y
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    /// The folder above.
    Up,
    Folder(String),
    File(String),
}

/// What a frame of input did to the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    File(PathBuf),
    /// Leave without picking.
    Back,
}

/// Choosing a file, a folder at a time: Jump opens a folder or picks a
/// file, Left goes up a folder and Pause leaves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picker {
    /// The folder being shown.
    pub dir: PathBuf,
    entries: Vec<Entry>,
    pub menu: Menu<usize>,
    /// Why the folder couldn't be read, if it couldn't.
    pub problem: Option<String>,
}

impl Picker {
    /// Shows `dir`'s folders, then its files, by name. Hidden ones
    /// (starting with `.`) are left out.
    pub fn open(dir: &Path) -> Picker {
        Picker::open_at(dir, None)
    }

    /// The folder to start in: `remembered` if it's still there, or else
    /// the player's home folder, or else the working folder.
    pub fn start_dir(remembered: Option<&Path>) -> PathBuf {
        let home = ["HOME", "USERPROFILE"]
            .into_iter()
            .filter_map(std::env::var_os)
            .find(|v| !v.is_empty())
            .map(PathBuf::from);
        remembered
            .map(Path::to_path_buf)
            .into_iter()
            .chain(home)
            .find(|d| d.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Shows `dir` with the entry called `select` selected.
    fn open_at(dir: &Path, select: Option<&str>) -> Picker {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        let mut entries = Vec::new();
        if dir.parent().is_some() {
            entries.push(Entry::Up);
        }
        let mut problem = None;
        match std::fs::read_dir(&dir) {
            Ok(read) => {
                let (mut folders, mut files) = (Vec::new(), Vec::new());
                for entry in read.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    // Follows links, so a linked folder opens like one.
                    if entry.path().is_dir() {
                        folders.push(name);
                    } else {
                        files.push(name);
                    }
                }
                folders.sort_by_key(|n| n.to_lowercase());
                files.sort_by_key(|n| n.to_lowercase());
                entries.extend(folders.into_iter().map(Entry::Folder));
                entries.extend(files.into_iter().map(Entry::File));
            }
            Err(e) => problem = Some(format!("Can't read this folder: {e}")),
        }
        let width = WIDTH - 4;
        let labels = entries.iter().enumerate().map(|(i, e)| {
            let label = match e {
                Entry::Up => "../".to_string(),
                Entry::Folder(name) => fit(&format!("{name}/"), width),
                Entry::File(name) => fit(name, width),
            };
            (label, i)
        });
        let mut menu = Menu::new(labels);
        let selected = select.and_then(|name| {
            entries
                .iter()
                .position(|e| matches!(e, Entry::Folder(n) if n == name))
        });
        if let Some(i) = selected {
            menu = menu.with_selected(i);
        }
        Picker {
            dir,
            entries,
            menu,
            problem,
        }
    }

    fn up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            let name = self
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            *self = Picker::open_at(parent, name.as_deref());
        }
    }

    /// Applies one frame of input.
    pub fn update(&mut self, frame: &Frame) -> Option<Picked> {
        if self.entries.is_empty() {
            // Nothing to choose; only leaving.
            let pressed = frame.pressed;
            let leave = pressed.contains(Action::Pause) || pressed.contains(Action::Jump);
            return leave.then_some(Picked::Back);
        }
        if frame.pressed.contains(Action::Left) {
            self.up();
            return None;
        }
        match self.menu.update(frame)? {
            Choice::Back => Some(Picked::Back),
            Choice::Picked(i) => match self.entries[i].clone() {
                Entry::Up => {
                    self.up();
                    None
                }
                Entry::Folder(name) => {
                    *self = Picker::open(&self.dir.join(name));
                    None
                }
                Entry::File(name) => Some(Picked::File(self.dir.join(name))),
            },
        }
    }

    pub fn draw(&self, canvas: &mut dyn Surface, centre: i32) {
        draw_text_centred(canvas, centre, 12, "IMPORT A GAME", HEADING);
        draw_text_centred(
            canvas,
            centre,
            12 + LINE + 3,
            "Choose your own copy of a supported game",
            SUBTITLE,
        );
        let shown = self.dir.display().to_string();
        let tail: String = {
            let chars: Vec<char> = shown.chars().collect();
            if chars.len() <= WIDTH {
                shown
            } else {
                // The end of a long path says the most.
                let mut tail = String::from("…");
                tail.extend(&chars[chars.len() - (WIDTH - 1)..]);
                tail
            }
        };
        draw_text_centred(canvas, centre, 12 + 2 * LINE + 6, &tail, TITLE);
        let top = 12 + 4 * LINE;
        if let Some(problem) = &self.problem {
            paragraph(
                canvas,
                centre,
                top + Menu::<usize>::rows_height(2),
                problem,
                FAILED,
            );
        }
        if !self.entries.is_empty() {
            self.menu.draw_rows(canvas, centre, top, PICKER_ROWS);
        }
        let hint = "Jump: open   Left: up a folder   Pause: back";
        draw_text_centred(canvas, centre, VIEW.1 as i32 - 12, hint, SUBTITLE);
    }
}

/// What to do once an import has finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    /// Play the imported pack.
    Play,
    /// Import it again over the earlier import.
    Replace,
    /// Back to the picker.
    Another,
    /// Back to where the picker was opened from.
    Done,
}

#[derive(Debug, Default)]
struct Progress {
    /// The decoders that have started, in order.
    started: Vec<String>,
    done: Option<Result<Imported, ImportError>>,
}

/// An import of the file at `file`, running or finished.
#[derive(Clone, Debug)]
pub struct Importing {
    pub file: PathBuf,
    progress: Arc<Mutex<Progress>>,
    /// What the import made, or why it didn't, once it has finished.
    pub result: Option<Result<Imported, ImportError>>,
    /// The levels in the pack, once it's made.
    pub levels: usize,
    /// The choices once it's finished.
    pub menu: Option<Menu<After>>,
}

impl PartialEq for Importing {
    fn eq(&self, other: &Importing) -> bool {
        Arc::ptr_eq(&self.progress, &other.progress)
            && self.result == other.result
            && self.menu == other.menu
    }
}

impl Eq for Importing {}

impl Importing {
    /// Starts importing `file` into `packs` (replacing an earlier import
    /// if `replace`) on a thread of its own. Without a packs folder, it
    /// has failed already.
    pub fn start(
        importer: Arc<Importer>,
        file: PathBuf,
        packs: Option<PathBuf>,
        replace: bool,
    ) -> Importing {
        let progress = Arc::new(Mutex::new(Progress::default()));
        let mut importing = Importing {
            file: file.clone(),
            progress: progress.clone(),
            result: None,
            levels: 0,
            menu: None,
        };
        let Some(packs) = packs else {
            importing.finish(Err(ImportError::Destination(
                "can't tell where your packs folder is".into(),
            )));
            return importing;
        };
        std::thread::spawn(move || {
            let lock = || progress.lock().unwrap_or_else(PoisonError::into_inner);
            let run = std::panic::AssertUnwindSafe(|| {
                let image = Image::read(&file)?;
                importer.import_with_progress(&image, &packs, replace, &mut |name| {
                    lock().started.push(name.to_string())
                })
            });
            let result = std::panic::catch_unwind(run).unwrap_or_else(|_| {
                // Not the panic's message, which could say anything.
                let decoder = lock().started.last().cloned();
                Err(ImportError::Decode {
                    decoder: decoder.unwrap_or_else(|| "game".into()),
                    message: "the importer stopped unexpectedly".into(),
                })
            });
            lock().done = Some(result);
        });
        importing
    }

    /// Takes the result once the import has finished. Returns whether
    /// it has.
    pub fn poll(&mut self) -> bool {
        if self.result.is_some() {
            return true;
        }
        let done = self
            .progress
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .done
            .take();
        match done {
            Some(result) => {
                self.finish(result);
                true
            }
            None => false,
        }
    }

    fn finish(&mut self, result: Result<Imported, ImportError>) {
        let menu = match &result {
            Ok(imported) => {
                self.levels = Installed::load(&imported.dir)
                    .map(|pack| pack.levels().len())
                    .unwrap_or(0);
                let menu = Menu::new([("Play it", After::Play), ("Done", After::Done)]);
                if self.levels == 0 {
                    menu.with_unavailable([0]).with_selected(1)
                } else {
                    menu
                }
            }
            Err(ImportError::AlreadyImported(_)) => Menu::new([
                ("Replace it", After::Replace),
                ("Choose another file", After::Another),
                ("Done", After::Done),
            ]),
            Err(_) => Menu::new([
                ("Choose another file", After::Another),
                ("Done", After::Done),
            ]),
        };
        if let Err(e) = &result {
            report::warn(format!("importing {}: {e}", self.file.display()));
        }
        self.result = Some(result);
        self.menu = Some(menu);
    }

    /// Applies one frame of input: none while it's running, and then the
    /// choice of what to do next. Pause is the last choice.
    pub fn update(&mut self, frame: &Frame) -> Option<After> {
        if !self.poll() {
            return None;
        }
        let menu = self.menu.as_mut()?;
        match menu.update(frame)? {
            Choice::Picked(after) => Some(after),
            Choice::Back => menu.items.last().map(|(_, after)| *after),
        }
    }

    pub fn draw(&self, canvas: &mut dyn Surface, centre: i32) {
        let name = self.file.file_name().unwrap_or_default().to_string_lossy();
        let name = fit(&name, WIDTH);
        let mut y = 30;
        let heading = |canvas: &mut dyn Surface, text, color| {
            draw_text_centred(canvas, centre, 30, text, color);
        };
        y += 2 * LINE;
        draw_text_centred(canvas, centre, y, &name, TITLE);
        y += 2 * LINE;
        match &self.result {
            None => {
                heading(canvas, "IMPORTING…", HEADING);
                let started = self
                    .progress
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .started
                    .clone();
                draw_text_centred(canvas, centre, y, "Checking the file", SUBTITLE);
                for name in started.iter().rev().take(6).rev() {
                    y += LINE;
                    let line = fit(&format!("Converting the {name}"), WIDTH);
                    draw_text_centred(canvas, centre, y, &line, SUBTITLE);
                }
            }
            Some(Ok(imported)) => {
                heading(canvas, "IMPORTED", HEADING);
                let m = &imported.manifest;
                let line = fit(&format!("{} ({})", m.name, m.release), WIDTH);
                draw_text_centred(canvas, centre, y, &line, HEADING);
                y = paragraph(canvas, centre, y + LINE, PERSONAL, SUBTITLE);
                if self.levels == 0 {
                    let none = "Nothing to play yet: this version of Clamber \
                                can't convert its levels.";
                    paragraph(canvas, centre, y + LINE, none, SUBTITLE);
                }
            }
            Some(Err(e)) => {
                heading(canvas, "CAN'T IMPORT", FAILED);
                paragraph(canvas, centre, y, &capitalized(&e.to_string()), HEADING);
            }
        }
        if let Some(menu) = &self.menu {
            menu.draw(canvas, centre, VIEW.1 as i32 - 20 - menu.height());
        }
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// An item on the games screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// The game the session started with.
    Bundled,
    /// An imported pack, by index in [`Games::packs`].
    Pack(usize),
    Back,
}

/// The games there are to play: the bundled one, then the imported
/// packs. Packs with nothing to play yet are shown, but can't be picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Games {
    pub menu: Menu<Pick>,
    pub packs: Vec<Installed>,
    /// Why the last game picked couldn't be played.
    pub problem: Option<String>,
}

impl Games {
    /// The packs in `packs_dir`, with the one called `playing` selected,
    /// or the bundled game if none is. Folders there that aren't packs
    /// are warned about. Without a bundled game to go back to, its item
    /// can't be picked.
    pub fn list(packs_dir: Option<&Path>, playing: Option<&str>, bundled: bool) -> Games {
        let (packs, problems) = packs_dir.map(installed).unwrap_or_default();
        problems.iter().for_each(report::warn);
        let mut items = vec![("Clamber".to_string(), Pick::Bundled)];
        let mut unavailable = Vec::new();
        if !bundled {
            unavailable.push(0);
        }
        for (i, pack) in packs.iter().enumerate() {
            if pack.levels().is_empty() {
                unavailable.push(items.len());
            }
            items.push((fit(&pack.manifest.name, WIDTH - 4), Pick::Pack(i)));
        }
        items.push(("Back".to_string(), Pick::Back));
        let selected = playing
            .and_then(|id| packs.iter().position(|p| p.manifest.id == id))
            .map_or(0, |i| i + 1);
        let menu = Menu::new(items)
            .with_unavailable(unavailable)
            .with_selected(selected);
        Games {
            menu,
            packs,
            problem: None,
        }
    }

    /// Applies one frame of input. Pause is Back.
    pub fn update(&mut self, frame: &Frame) -> Option<Pick> {
        match self.menu.update(frame)? {
            Choice::Picked(pick) => Some(pick),
            Choice::Back => Some(Pick::Back),
        }
    }

    pub fn draw(&self, canvas: &mut dyn Surface, centre: i32) {
        draw_text_centred(canvas, centre, 20, "GAMES", HEADING);
        let top = 20 + 2 * LINE;
        let rows = 8;
        self.menu.draw_rows(canvas, centre, top, rows);
        let rows = rows.min(self.menu.items.len());
        let mut y = top + Menu::<Pick>::rows_height(rows) + 2 * LINE;
        if let Some(problem) = &self.problem {
            paragraph(canvas, centre, y, problem, FAILED);
            return;
        }
        match self.menu.current() {
            Pick::Bundled => {
                let about = "The game Clamber comes with";
                draw_text_centred(canvas, centre, y, about, SUBTITLE);
                y += 2 * LINE;
            }
            Pick::Pack(i) => {
                let pack = &self.packs[i];
                let m = &pack.manifest;
                let line = fit(&format!("Imported from {} ({})", m.title, m.release), WIDTH);
                draw_text_centred(canvas, centre, y, &line, SUBTITLE);
                y = paragraph(canvas, centre, y + LINE, PERSONAL, SUBTITLE);
                if pack.levels().is_empty() {
                    draw_text_centred(canvas, centre, y, "Nothing to play yet", SUBTITLE);
                }
            }
            Pick::Back => {}
        }
        if self.packs.is_empty() {
            let none = "No imported games yet: pick Import a game… on the title screen";
            paragraph(canvas, centre, y, none, SUBTITLE);
        }
    }
}
