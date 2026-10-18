//! Importing a game the player owns, from the title screen, and the
//! games there are to play.
//!
//! The player picks the image in the system's file dialog, or by
//! dropping it on the window. Where there's no dialog, or the player is
//! using a gamepad, a [`Picker`] browses their folders for it instead,
//! using the same neutral actions as every menu. It lists only files
//! the size of a supported image, and marks those that are one. On a
//! keyboard, the player can also type or paste a path there.
//! [`Importing`] runs the importer on a thread of its own and shows its
//! progress, then what came of it. [`Games`] lists the bundled game and
//! the imported packs, each marked as a personal import.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use clamber_import::{
    Catalog, Image, ImportError, Imported, Importer, Installed, import_report, installed,
    skipped_summary,
};
use clamber_input::{Action, Frame};

use crate::camera::VIEW;
use crate::canvas::Color;
use crate::menu::{Choice, Menu};
use crate::report;
use crate::session::{FAILED, HEADING, SUBTITLE, TITLE};
use crate::surface::Surface;
use crate::text::{ADVANCE, LINE, draw_text_centred, fit, fit_end, wrap};

/// How wide a line of text on these screens can be, in view units.
const WIDTH: i32 = 336;
/// How wide a menu item can be, leaving room for the cursor.
const ITEM_WIDTH: i32 = WIDTH - 4 * ADVANCE;

/// Folder entries shown at once.
const PICKER_ROWS: usize = 11;

/// The report on the last import, written in the packs folder for the
/// player to share when something went wrong.
pub const REPORT: &str = "import-report.txt";

/// How a personal import is described, wherever one is shown.
pub const PERSONAL: &str = "Personal import: yours to play, not to share";

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

/// How a file that is a supported image is marked in the picker.
const SUPPORTED: &str = "★ ";

/// A change to the path being typed in the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Typed or pasted text. Only its first line is kept.
    Insert(String),
    Backspace,
    /// Go to the path.
    Enter,
    /// Stop typing.
    Cancel,
}

/// What a frame of input did to the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    File(PathBuf),
    /// Leave without picking.
    Back,
}

/// The files in a folder found to be supported images, by entry, and
/// whether all of them have been looked at.
#[derive(Debug, Default)]
struct Marks {
    found: Vec<usize>,
    done: bool,
}

/// Choosing a file, a folder at a time: Jump opens a folder or picks a
/// file, Left goes up a folder and Pause leaves. Or typing a path:
/// see [`Picker::edit`].
#[derive(Clone, Debug)]
pub struct Picker {
    /// The folder being shown.
    pub dir: PathBuf,
    entries: Vec<Entry>,
    pub menu: Menu<usize>,
    /// Why the folder couldn't be read, if it couldn't.
    pub problem: Option<String>,
    /// The path being typed, if one is.
    pub typed: Option<String>,
    /// There's nothing at the path typed, as found on Enter.
    pub nothing_there: bool,
    /// The images it lists.
    catalog: Arc<Catalog>,
    /// Filled in on a thread of its own, as reading the files takes a
    /// while.
    marks: Arc<Mutex<Marks>>,
    /// How many of the marks are shown.
    marked: usize,
}

impl PartialEq for Picker {
    fn eq(&self, other: &Picker) -> bool {
        self.dir == other.dir
            && self.entries == other.entries
            && self.menu == other.menu
            && self.problem == other.problem
            && self.typed == other.typed
            && self.nothing_there == other.nothing_there
            && Arc::ptr_eq(&self.marks, &other.marks)
    }
}

impl Eq for Picker {}

impl Picker {
    /// Shows `dir`'s folders, then its files that may be one of
    /// `catalog`'s images, by name. Hidden ones (starting with `.`) are
    /// left out.
    pub fn open(dir: &Path, catalog: Arc<Catalog>) -> Picker {
        Picker::open_at(dir, None, catalog)
    }

    /// The folder to start in: `remembered` if it's still there, or else
    /// the player's home folder, or else the working folder.
    pub fn start_dir(remembered: Option<&Path>) -> PathBuf {
        remembered
            .map(Path::to_path_buf)
            .into_iter()
            .chain(home())
            .find(|d| d.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Shows `dir` with the entry called `select` selected.
    fn open_at(dir: &Path, select: Option<&str>, catalog: Arc<Catalog>) -> Picker {
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
                    let path = entry.path();
                    if path.is_dir() {
                        folders.push(name);
                    } else if std::fs::metadata(&path).is_ok_and(|m| catalog.may_be(m.len())) {
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
        let width = ITEM_WIDTH;
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
        let marks = Arc::new(Mutex::new(Marks::default()));
        let files: Vec<(usize, PathBuf)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                Entry::File(name) => Some((i, dir.join(name))),
                _ => None,
            })
            .collect();
        {
            let (catalog, marks) = (catalog.clone(), marks.clone());
            std::thread::spawn(move || {
                let lock = || marks.lock().unwrap_or_else(PoisonError::into_inner);
                for (i, path) in files {
                    if catalog.identify_file(&path).is_some() {
                        lock().found.push(i);
                    }
                }
                lock().done = true;
            });
        }
        Picker {
            dir,
            entries,
            menu,
            problem,
            typed: None,
            nothing_there: false,
            catalog,
            marks,
            marked: 0,
        }
    }

    /// Marks the files found to be supported images since the last
    /// time. Returns whether all the files have been looked at.
    pub fn mark(&mut self) -> bool {
        let marks = self.marks.lock().unwrap_or_else(PoisonError::into_inner);
        for &i in &marks.found[self.marked..] {
            if let Entry::File(name) = &self.entries[i] {
                self.menu.items[i].0 = fit(&format!("{SUPPORTED}{name}"), ITEM_WIDTH);
            }
        }
        self.marked = marks.found.len();
        marks.done
    }

    fn up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            let name = self
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            *self = Picker::open_at(parent, name.as_deref(), self.catalog.clone());
        }
    }

    /// Applies one frame of input. While a path is being typed, the
    /// menu waits.
    pub fn update(&mut self, frame: &Frame) -> Option<Picked> {
        self.mark();
        if self.typed.is_some() {
            return None;
        }
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
                    *self = Picker::open(&self.dir.join(name), self.catalog.clone());
                    None
                }
                Entry::File(name) => Some(Picked::File(self.dir.join(name))),
            },
        }
    }

    /// Changes the path being typed; typing or pasting starts one. On
    /// Enter, a folder opens and a file is picked. A path that isn't
    /// absolute is taken from the folder shown, a leading `~` stands
    /// for the player's home folder, and quotes around the path are
    /// left out.
    pub fn edit(&mut self, edit: Edit) -> Option<Picked> {
        self.nothing_there = false;
        match edit {
            Edit::Insert(text) => {
                let line = text.lines().next().unwrap_or_default();
                let line: String = line.chars().filter(|c| !c.is_control()).collect();
                // A space alone doesn't start a path.
                if self.typed.is_some() || !line.trim().is_empty() {
                    self.typed.get_or_insert_default().push_str(&line);
                }
            }
            Edit::Backspace => {
                if let Some(typed) = &mut self.typed {
                    typed.pop();
                }
            }
            Edit::Cancel => self.typed = None,
            Edit::Enter => {
                let path = typed_path(self.typed.as_deref()?, &self.dir);
                let Some(path) = path else {
                    self.typed = None;
                    return None;
                };
                if path.is_dir() {
                    *self = Picker::open(&path, self.catalog.clone());
                } else if path.is_file() {
                    self.typed = None;
                    return Some(Picked::File(path));
                } else {
                    self.nothing_there = true;
                }
            }
        }
        None
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
        // The end of a long path says the most.
        let (shown, color) = match &self.typed {
            Some(typed) => (format!("{typed}_"), HEADING),
            None => (self.dir.display().to_string(), TITLE),
        };
        let tail = fit_end(&shown, WIDTH);
        draw_text_centred(canvas, centre, 12 + 2 * LINE + 6, &tail, color);
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
        let (note, color) = match (&self.typed, self.nothing_there) {
            (Some(_), true) => ("Nothing is there".to_string(), FAILED),
            (Some(_), false) => (
                "Enter: go there   Escape: stop typing".to_string(),
                SUBTITLE,
            ),
            (None, _) => (format!("{SUPPORTED}a supported game"), SUBTITLE),
        };
        draw_text_centred(canvas, centre, VIEW.1 as i32 - 12 - LINE, &note, color);
    }
}

/// The player's home folder, if it's known.
fn home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Where a path typed in the picker showing `dir` leads, if anywhere.
fn typed_path(typed: &str, dir: &Path) -> Option<PathBuf> {
    let mut typed = typed.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = typed
            .strip_prefix(quote)
            .and_then(|t| t.strip_suffix(quote))
        {
            typed = inner;
        }
    }
    if typed.is_empty() {
        return None;
    }
    let path = match typed.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            home()?.join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(typed),
    };
    Some(dir.join(path))
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
    done: Option<(Result<Imported, ImportError>, Option<PathBuf>)>,
}

/// An import of the file at `file`, running or finished.
#[derive(Clone, Debug)]
pub struct Importing {
    pub file: PathBuf,
    progress: Arc<Mutex<Progress>>,
    /// What the import made, or why it didn't, once it has finished.
    pub result: Option<Result<Imported, ImportError>>,
    /// Where the report on it was written, if it could be.
    pub report: Option<PathBuf>,
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
            report: None,
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
            // Not where there's no packs folder: nothing was imported.
            let path = packs.join(REPORT);
            let report = match packs.is_dir() {
                true => match std::fs::write(&path, import_report(&result)) {
                    Ok(()) => Some(path),
                    Err(e) => {
                        report::warn(format!("{}: {e}", path.display()));
                        None
                    }
                },
                false => None,
            };
            lock().done = Some((result, report));
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
            Some((result, report)) => {
                self.report = report;
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
        match &result {
            Ok(imported) => {
                for skipped in &imported.skipped {
                    report::warn(format!("importing {}: {skipped}", self.file.display()));
                }
            }
            Err(e) => report::warn(format!("importing {}: {e}", self.file.display())),
        }
        if let Some(path) = &self.report {
            report::note(format!("The report on the import is in {}", path.display()));
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
                    y = paragraph(canvas, centre, y + LINE, none, SUBTITLE);
                }
                for line in skipped_summary(&imported.skipped) {
                    y = paragraph(canvas, centre, y + LINE, &line, SUBTITLE);
                }
                if !imported.skipped.is_empty() {
                    self.draw_report(canvas, centre, y + LINE);
                }
            }
            Some(Err(e)) => {
                heading(canvas, "CAN'T IMPORT", FAILED);
                let y = paragraph(canvas, centre, y, &capitalized(&e.to_string()), HEADING);
                self.draw_report(canvas, centre, y + LINE);
            }
        }
        if let Some(menu) = &self.menu {
            menu.draw(canvas, centre, VIEW.1 as i32 - 20 - menu.height());
        }
    }
}

impl Importing {
    /// Where the report is, if it was written.
    fn draw_report(&self, canvas: &mut dyn Surface, centre: i32, top: i32) {
        if let Some(path) = &self.report {
            let text = format!("A report to share is in {}", path.display());
            paragraph(canvas, centre, top, &text, SUBTITLE);
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
            items.push((fit(&pack.manifest.name, ITEM_WIDTH), Pick::Pack(i)));
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
