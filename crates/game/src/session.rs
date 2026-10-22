//! The screens around a level: title, pause, controls, options,
//! credits, importing a game, the games to play, level complete and game
//! over, the level map between levels for a pack that has one
//! ([`MapPlay`]), and moving between them and play.
//!
//! Menus read the same neutral actions as play, from every player at
//! once, so a keyboard or a gamepad alone can drive them (see
//! [`Menu`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_import::{Importer, Installed, Replacements, Widescreen};

use clamber_input::{Action, Actions, Device, Event, Frame, Input, Settings};
use clamber_level::Arrival;
use clamber_sim::{InputFrame, Outcome, World};

use crate::canvas::{Color, rgb};
use crate::controls::{Controls, Request};
use crate::credits::Credits;
use crate::display::{Look, Rendering};
use crate::feels::Feels;
use crate::game::Game;
use crate::hud::{counter_width, draw_counter, relic_total};
use crate::icons::Icon;
use crate::import::{After, Edit, Games, Importing, PERSONAL, Pick, Picked, Picker};
use crate::map::{MapPlay, MapStep};
use crate::menu::{Choice, Menu};
use crate::panel::draw_panel;
use crate::prefs::{BUNDLED, Display, Prefs, Volume};
use crate::prompts::{Hint, draw_hints};
use crate::report;
use crate::sound::{Cue, cues};
use crate::surface::Surface;
use crate::text::{ADVANCE, LINE, draw_text, draw_text_centred};
use crate::title::{draw_logo, draw_scene};
use crate::{SPRITES, content_dir, load_game_with};

/// Where the credits screen's lines start, and how many show at once.
const CREDITS_TOP: i32 = 34;
const CREDITS_ROWS: usize = 18;
/// How wide a line of credits can be, in view units.
const CREDITS_WIDTH: i32 = 360;

pub(crate) const TITLE: Color = rgb(0xf0, 0xc8, 0x40);
pub(crate) const HEADING: Color = rgb(0xf4, 0xf0, 0xe8);
pub(crate) const SUBTITLE: Color = rgb(0xa8, 0xb0, 0xbc);
pub(crate) const FAILED: Color = rgb(0xe8, 0x50, 0x60);
/// Laid over the level behind a menu.
const DIM: Color = rgb(0x08, 0x0a, 0x10);

/// Where the menus' first items are, down the view.
pub(crate) const TITLE_MENU: i32 = 44 + 5 * LINE;
pub(crate) const PAUSED_MENU: i32 = 100;
pub(crate) const COMPLETE_MENU: i32 = 130;
pub(crate) const GAME_OVER_MENU: i32 = 100;
pub(crate) const MOVEMENT_MENU: i32 = 30 + 4 * LINE;
pub(crate) const OPTIONS_MENU: i32 = 80;

/// Logic frames per second, for showing a level's time.
const FPS: u32 = 60;

/// Something a menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Start,
    Resume,
    /// Start the level over.
    Restart,
    /// The controls screen.
    Controls,
    /// The movement screen.
    Movement,
    /// The options screen.
    Options,
    /// The credits screen.
    Credits,
    /// The games screen: the bundled game and the imported ones.
    Games,
    /// Import a game the player owns.
    Import,
    /// Back to the title screen.
    ToTitle,
    /// Close the game.
    Quit,
}

/// Which screen is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    Title(Menu<Item>),
    Playing,
    Paused(Menu<Item>),
    Complete(Menu<Item>),
    GameOver(Menu<Item>),
    /// The controls screen, and the screen to go back to.
    Controls(Controls, Box<Screen>),
    /// Choosing a movement preset, by index in the session's
    /// [`Feels::presets`], and the screen to go back to.
    Movement(Menu<usize>, Box<Screen>),
    /// The display and sound options, and the screen to go back to.
    Options(Menu<Setting>, Box<Screen>),
    /// The credits, scrolled down this many lines, and the screen to go
    /// back to.
    Credits(usize, Box<Screen>),
    /// Waiting for the player to choose a game image to import in the
    /// system's file dialog ([`Session::file_request`]), and the screen
    /// to go back to.
    Choosing(Box<Screen>),
    /// Choosing a game image to import in the game's own browser, and
    /// the screen to go back to.
    Picker(Picker, Box<Screen>),
    /// An import, running or finished, and the picker it was started
    /// from.
    Importing(Box<Importing>, Box<Screen>),
    /// Choosing a game to play, and the screen to go back to.
    Games(Games, Box<Screen>),
    /// The session's level map ([`Session::map`]).
    Map,
}

/// The music a screen plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tune {
    Title,
    /// The level's own.
    Level,
}

/// An item on the options screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Fullscreen,
    Vsync,
    Vrr,
    /// Turned down with Left and up with Right or Jump, which wraps
    /// around to silent.
    MusicVolume,
    EffectsVolume,
    /// The widescreen mode of the game being played, changed with Left,
    /// and Right or Jump, which go round the modes.
    Widescreen,
    /// The rendering mode of the imported pixel-art game being played,
    /// changed with Left, Right or Jump. Not shown for other games.
    Rendering,
    Back,
}

impl Screen {
    pub fn title() -> Screen {
        Screen::Title(Menu::new([
            ("Start", Item::Start),
            ("Games", Item::Games),
            ("Import a game…", Item::Import),
            ("Movement", Item::Movement),
            ("Controls", Item::Controls),
            ("Options", Item::Options),
            ("Credits", Item::Credits),
            ("Quit", Item::Quit),
        ]))
    }

    fn paused() -> Screen {
        Screen::Paused(Menu::new([
            ("Resume", Item::Resume),
            ("Restart level", Item::Restart),
            ("Controls", Item::Controls),
            ("Options", Item::Options),
            ("Quit", Item::ToTitle),
        ]))
    }

    /// The music this screen plays: the title's on the title screen and
    /// those opened from it, the level's in play and through pausing it,
    /// and none once the level is over.
    pub fn tune(&self) -> Option<Tune> {
        match self {
            Screen::Title(_) => Some(Tune::Title),
            Screen::Playing | Screen::Paused(_) => Some(Tune::Level),
            Screen::Complete(_) | Screen::GameOver(_) | Screen::Map => None,
            Screen::Controls(_, from)
            | Screen::Movement(_, from)
            | Screen::Options(_, from)
            | Screen::Credits(_, from)
            | Screen::Choosing(from)
            | Screen::Picker(_, from)
            | Screen::Importing(_, from)
            | Screen::Games(_, from) => from.tune(),
        }
    }

    fn complete() -> Screen {
        Screen::Complete(Menu::new([
            ("Play again", Item::Restart),
            ("Quit", Item::ToTitle),
        ]))
    }

    fn game_over() -> Screen {
        Screen::GameOver(Menu::new([
            ("Try again", Item::Restart),
            ("Quit", Item::ToTitle),
        ]))
    }
}

/// The game from its title screen on.
pub struct Session {
    pub game: Game,
    pub screen: Screen,
    /// The players' devices and bindings, which the controls screen
    /// changes.
    pub input: Input,
    /// Where the controls screen saves the input settings, if anywhere.
    pub settings_path: Option<PathBuf>,
    /// The presets the movement screen offers, and how to play each.
    pub feels: Feels,
    /// What the credits screen shows.
    pub credits: Credits,
    /// Menu choices to remember, and where to save them, if anywhere.
    pub prefs: Prefs,
    pub prefs_path: Option<PathBuf>,
    /// Quit was picked on the title screen; the window should close.
    pub quit: bool,
    /// What made a sound during the last step, in order.
    pub cues: Vec<Cue>,
    /// The level being played, if it came from a file.
    pub level_path: PathBuf,
    /// The areas play went on from into the one being played, with the
    /// files they came from, oldest first: coming back to one of them,
    /// play goes on in it as it was left.
    pub left: Vec<(PathBuf, Game)>,
    /// The level the session started with, to go back to from an
    /// imported one, if there's one to go back to.
    pub bundled: Option<PathBuf>,
    /// The personal import being played, if one is.
    pub pack: Option<Installed>,
    /// Where imported packs go, if anywhere.
    pub packs_dir: Option<PathBuf>,
    /// Where replacement art packs for imported games are, if anywhere
    /// ([`clamber_import::Replacements`]).
    pub art_dir: Option<PathBuf>,
    pub importer: Arc<Importer>,
    /// The level map of the pack being played, if it has one: a new game
    /// begins on it, and finishing a level entered from it goes back
    /// there.
    pub map: Option<MapPlay>,
    /// The platform has a file dialog to choose a game image in.
    pub file_dialog: bool,
    /// The folder to open the file dialog in, when the game wants the
    /// player to choose a file there. The platform takes it, shows the
    /// dialog, and passes what was chosen to [`Session::file_chosen`].
    pub file_request: Option<PathBuf>,
    /// The kind of device the last button was pressed on.
    pub device: Device,
    /// Frames the title screen has shown for, all told: its logo comes
    /// in the first time only.
    pub title_frames: u32,
    /// The mouse pointer's place in the view, once it has moved over it.
    pub pointer: Option<(f32, f32)>,
    /// Presses the mouse adds to the next menu step.
    pub(crate) clicks: Actions,
    /// The volume being dragged with the mouse, if one is.
    pub(crate) dragging: Option<Setting>,
}

impl Session {
    /// Starts at the title screen, with `game`'s level behind it and the
    /// default bindings.
    pub fn new(game: Game) -> Session {
        Session {
            game,
            screen: Screen::title(),
            input: Input::new(Settings::builtin()),
            settings_path: None,
            feels: Feels::generic(),
            credits: Credits::default(),
            prefs: Prefs::default(),
            prefs_path: None,
            quit: false,
            cues: Vec::new(),
            level_path: PathBuf::new(),
            left: Vec::new(),
            bundled: None,
            pack: None,
            packs_dir: None,
            art_dir: None,
            importer: Arc::new(Importer::builtin()),
            map: None,
            file_dialog: false,
            file_request: None,
            device: Device::Keyboard,
            title_frames: 0,
            pointer: None,
            clicks: Actions::NONE,
            dragging: None,
        }
    }

    /// Records a device event. A press that finishes a remap shows on
    /// the controls screen.
    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Key { down: true, .. } => self.device = Device::Keyboard,
            Event::PadButton { down: true, .. } => self.device = Device::Pad,
            _ => {}
        }
        let remapped = self.input.handle(event);
        if let (Some(remapped), Screen::Controls(controls, _)) = (remapped, &mut self.screen) {
            controls.remapped(remapped);
        }
    }

    /// Ends a logic frame of device input and steps with it: with the
    /// players' game controls in play and on the level map, and with the
    /// menu controls on the other screens.
    pub fn tick(&mut self) {
        let players = self.input.tick();
        if matches!(self.screen, Screen::Playing | Screen::Map) {
            self.step(&InputFrame { players });
        } else {
            let mut menu = self.input.menu();
            let clicks = std::mem::replace(&mut self.clicks, Actions::NONE);
            menu.pressed = menu.pressed | clicks;
            self.step(&InputFrame::single(menu));
        }
    }

    /// Advances one logic frame: plays the level, or works the menu
    /// that's showing. A menu takes its input as neutral actions: the
    /// directions move, Jump confirms and Pause goes back (see
    /// [`Input::menu`]).
    pub fn step(&mut self, input: &InputFrame) {
        self.cues.clear();
        if self.screen == Screen::Playing {
            self.play(input);
            return;
        }
        let frame = everyone(input);
        if let Screen::Title(_) = self.screen {
            self.title_frames = self.title_frames.saturating_add(1);
        }
        match self.screen {
            Screen::Map => return self.step_map(&frame),
            Screen::Choosing(..) => return self.step_choosing(&frame),
            Screen::Picker(..) => return self.step_picker(&frame),
            Screen::Importing(..) => return self.step_importing(&frame),
            Screen::Games(..) => return self.step_games(&frame),
            _ => {}
        }
        if let Screen::Controls(controls, _) = &mut self.screen {
            match controls.update(&frame, &mut self.input) {
                Some(Request::Save) => {
                    let result = match &self.settings_path {
                        Some(path) => save(self.input.settings(), path),
                        None => Err("no settings file".to_string()),
                    };
                    controls.saved(result);
                }
                Some(Request::Back) => {
                    let Screen::Controls(_, from) =
                        std::mem::replace(&mut self.screen, Screen::Playing)
                    else {
                        unreachable!()
                    };
                    self.screen = *from;
                }
                None => {}
            }
            return;
        }
        if let Screen::Movement(menu, _) = &mut self.screen {
            match menu.update(&frame) {
                Some(Choice::Picked(index)) => {
                    let id = self.feels.presets().iter().nth(index).map(|p| p.id.clone());
                    if let Some(id) = id {
                        self.choose_preset(&id);
                    }
                }
                Some(Choice::Back) => {}
                None => return,
            }
            let Screen::Movement(_, from) = std::mem::replace(&mut self.screen, Screen::Playing)
            else {
                unreachable!()
            };
            self.screen = *from;
            return;
        }
        if let Screen::Credits(scroll, _) = &mut self.screen {
            let pressed = frame.pressed;
            if pressed.contains(Action::Jump) || pressed.contains(Action::Pause) {
                let Screen::Credits(_, from) = std::mem::replace(&mut self.screen, Screen::Playing)
                else {
                    unreachable!()
                };
                self.screen = *from;
                return;
            }
            let lines = self.credits.wrapped(CREDITS_WIDTH).len();
            let last = lines.saturating_sub(CREDITS_ROWS);
            if pressed.contains(Action::Down) {
                *scroll = (*scroll + 1).min(last);
            } else if pressed.contains(Action::Up) {
                *scroll = scroll.saturating_sub(1);
            }
            return;
        }
        let (widescreen, rendering) = (self.widescreen(), self.rendering());
        if let Screen::Options(menu, _) = &mut self.screen {
            let pressed = frame.pressed;
            let volume = &mut self.prefs.volume;
            let level = match menu.current() {
                Setting::MusicVolume => Some(&mut volume.music),
                Setting::EffectsVolume => Some(&mut volume.effects),
                _ => None,
            };
            if let Some(level) = level {
                let step = Volume::STEP;
                let changed = if pressed.contains(Action::Left) {
                    *level = level.saturating_sub(step);
                    true
                } else if pressed.contains(Action::Right) {
                    *level = (*level + step).min(100);
                    true
                } else if pressed.contains(Action::Jump) {
                    *level = if *level >= 100 { 0 } else { *level + step };
                    true
                } else {
                    false
                };
                if changed {
                    menu.items = options_items(&self.prefs, widescreen, rendering);
                    self.save_prefs("the volume");
                    return;
                }
            }
            if menu.current() == Setting::Widescreen {
                let back = pressed.contains(Action::Left);
                if back || pressed.contains(Action::Right) || pressed.contains(Action::Jump) {
                    let mode = self.widescreen().next(back);
                    let key = self.game_key().to_string();
                    self.prefs.widescreen.insert(key, mode);
                    self.apply_widescreen();
                    let Screen::Options(menu, _) = &mut self.screen else {
                        unreachable!()
                    };
                    menu.items = options_items(&self.prefs, mode, rendering);
                    self.save_prefs("the widescreen mode");
                    return;
                }
            }
            if let (Setting::Rendering, Some(mode)) = (menu.current(), rendering) {
                let change = [Action::Left, Action::Right, Action::Jump];
                if change.iter().any(|&a| pressed.contains(a)) {
                    let mode = mode.next();
                    let key = self.game_key().to_string();
                    self.prefs.rendering.insert(key, mode);
                    self.apply_rendering();
                    let Screen::Options(menu, _) = &mut self.screen else {
                        unreachable!()
                    };
                    menu.items = options_items(&self.prefs, widescreen, Some(mode));
                    self.save_prefs("the rendering mode");
                    return;
                }
            }
            let setting = match menu.update(&frame) {
                Some(Choice::Picked(Setting::Back) | Choice::Back) => {
                    let Screen::Options(_, from) =
                        std::mem::replace(&mut self.screen, Screen::Playing)
                    else {
                        unreachable!()
                    };
                    self.screen = *from;
                    return;
                }
                Some(Choice::Picked(setting)) => setting,
                None => return,
            };
            let display = &mut self.prefs.display;
            match setting {
                Setting::Fullscreen => display.fullscreen = !display.fullscreen,
                Setting::Vsync => display.vsync = !display.vsync,
                Setting::Vrr => display.vrr = !display.vrr,
                // changed above
                Setting::MusicVolume
                | Setting::EffectsVolume
                | Setting::Widescreen
                | Setting::Rendering => return,
                Setting::Back => unreachable!(),
            }
            menu.items = options_items(&self.prefs, widescreen, rendering);
            self.save_prefs("the display options");
            return;
        }
        let (Screen::Title(menu)
        | Screen::Paused(menu)
        | Screen::Complete(menu)
        | Screen::GameOver(menu)) = &mut self.screen
        else {
            return;
        };
        let item = match menu.update(&frame) {
            Some(Choice::Picked(item)) => item,
            // Pause leaves the pause menu; other screens need a choice.
            Some(Choice::Back) if matches!(self.screen, Screen::Paused(_)) => Item::Resume,
            _ => return,
        };
        match item {
            Item::Resume => self.screen = Screen::Playing,
            Item::Start if self.map.is_some() => {
                if let Some(map) = &mut self.map {
                    map.new_game(self.game.world.other.is_some());
                }
                self.screen = Screen::Map;
            }
            Item::Start | Item::Restart => {
                self.first_area();
                self.game.restart();
                self.screen = Screen::Playing;
            }
            Item::Controls => {
                let from = std::mem::replace(&mut self.screen, Screen::Playing);
                self.screen = Screen::Controls(Controls::new(), Box::new(from));
            }
            Item::Movement => {
                let from = std::mem::replace(&mut self.screen, Screen::Playing);
                self.screen = Screen::Movement(self.movement_menu(), Box::new(from));
            }
            Item::Options => {
                let from = std::mem::replace(&mut self.screen, Screen::Playing);
                let menu = Menu::new(options_items(
                    &self.prefs,
                    self.widescreen(),
                    self.rendering(),
                ));
                self.screen = Screen::Options(menu, Box::new(from));
            }
            Item::Credits => {
                let from = std::mem::replace(&mut self.screen, Screen::Playing);
                self.screen = Screen::Credits(0, Box::new(from));
            }
            Item::Games => {
                let from = std::mem::replace(&mut self.screen, Screen::Playing);
                self.screen = Screen::Games(self.games(), Box::new(from));
            }
            Item::Import => {
                let from = std::mem::replace(&mut self.screen, Screen::Playing);
                self.choose_file(from);
            }
            Item::ToTitle => self.screen = Screen::title(),
            Item::Quit => self.quit = true,
        }
    }

    /// Goes back from the screen showing to the one it was opened from.
    fn back(&mut self) {
        let from = match std::mem::replace(&mut self.screen, Screen::Playing) {
            Screen::Controls(_, from)
            | Screen::Movement(_, from)
            | Screen::Options(_, from)
            | Screen::Credits(_, from)
            | Screen::Choosing(from)
            | Screen::Picker(_, from)
            | Screen::Importing(_, from)
            | Screen::Games(_, from) => from,
            other => Box::new(other),
        };
        self.screen = *from;
    }

    /// Has the player choose a game image to import, in the folder they
    /// last imported from: in the system's file dialog if there is one
    /// and they're using the keyboard, or else in the game's browser.
    fn choose_file(&mut self, from: Screen) {
        let dir = Picker::start_dir(self.prefs.import_folder.as_deref());
        if self.file_dialog && self.device == Device::Keyboard {
            self.file_request = Some(dir);
            self.screen = Screen::Choosing(Box::new(from));
        } else {
            self.browse(dir, from);
        }
    }

    /// The game's own browser for choosing a game image, in `dir`.
    fn browse(&mut self, dir: PathBuf, from: Screen) {
        let catalog = Arc::new(self.importer.catalog.clone());
        self.screen = Screen::Picker(Picker::open(&dir, catalog), Box::new(from));
    }

    /// Goes from waiting on the file dialog to the game's browser.
    fn browse_instead(&mut self) {
        if let Screen::Choosing(from) = std::mem::replace(&mut self.screen, Screen::Playing) {
            let dir = Picker::start_dir(self.prefs.import_folder.as_deref());
            self.browse(dir, *from);
        }
    }

    /// While the file dialog is open, Pause browses in the game instead.
    fn step_choosing(&mut self, frame: &Frame) {
        if frame.pressed.contains(Action::Pause) {
            self.browse_instead();
        }
    }

    /// What the player chose in the file dialog: a file to import, or
    /// nothing. If the dialog `failed` to show, the game's browser takes
    /// its place from then on.
    pub fn file_chosen(&mut self, file: Option<PathBuf>, failed: bool) {
        if !matches!(self.screen, Screen::Choosing(_)) {
            // The player went on to the game's browser.
            return;
        }
        match file {
            Some(file) => {
                if let Some(dir) = file.parent() {
                    self.remember_import_folder(dir);
                }
                self.start_import(file, false);
            }
            None if failed => {
                report::warn("the file dialog didn't open; browsing in the game instead");
                self.file_dialog = false;
                self.browse_instead();
            }
            None => self.back(),
        }
    }

    /// Imports a file dropped on the window, unless an import is running.
    /// During play, the game pauses first.
    pub fn dropped(&mut self, file: PathBuf) {
        match &self.screen {
            Screen::Importing(importing, _) if importing.result.is_none() => {
                report::warn(format!("{}: an import is running already", file.display()));
                return;
            }
            // A finished import is replaced by this one.
            Screen::Importing(..) => return self.start_import(file, false),
            Screen::Playing => self.screen = Screen::paused(),
            _ => {}
        }
        let from = std::mem::replace(&mut self.screen, Screen::Playing);
        self.screen = Screen::Choosing(Box::new(from));
        self.file_chosen(Some(file), false);
    }

    fn remember_import_folder(&mut self, dir: &Path) {
        if self.prefs.import_folder.as_deref() != Some(dir) {
            self.prefs.import_folder = Some(dir.to_path_buf());
            self.save_prefs("the folder to import from");
        }
    }

    fn step_picker(&mut self, frame: &Frame) {
        let Screen::Picker(picker, _) = &mut self.screen else {
            return;
        };
        let picked = picker.update(frame);
        self.picked(picked);
    }

    /// Whether the screen showing takes typed text: the game's browser
    /// takes a path.
    pub fn takes_text(&self) -> bool {
        matches!(self.screen, Screen::Picker(..))
    }

    /// Whether a path is being typed, so that Enter, Escape and
    /// Backspace edit it rather than work the menu.
    pub fn typing(&self) -> bool {
        matches!(&self.screen, Screen::Picker(picker, _) if picker.typed.is_some())
    }

    /// Changes the path being typed in the game's browser.
    pub fn edit(&mut self, edit: Edit) {
        let Screen::Picker(picker, _) = &mut self.screen else {
            return;
        };
        let picked = picker.edit(edit);
        self.picked(picked);
    }

    fn picked(&mut self, picked: Option<Picked>) {
        match picked {
            None => {}
            Some(Picked::Back) => self.back(),
            Some(Picked::File(file)) => {
                if let Some(dir) = file.parent() {
                    self.remember_import_folder(dir);
                }
                self.start_import(file, false);
            }
        }
    }

    /// Imports `file` from the picker that's showing, or again from the
    /// import that's showing.
    fn start_import(&mut self, file: PathBuf, replace: bool) {
        let picker = match std::mem::replace(&mut self.screen, Screen::Playing) {
            Screen::Importing(_, picker) => picker,
            picker => Box::new(picker),
        };
        let packs = self.packs_dir.clone();
        let importing = Importing::start(self.importer.clone(), file, packs, replace);
        self.screen = Screen::Importing(Box::new(importing), picker);
    }

    fn step_importing(&mut self, frame: &Frame) {
        let Screen::Importing(importing, _) = &mut self.screen else {
            return;
        };
        let Some(after) = importing.update(frame) else {
            return;
        };
        let file = importing.file.clone();
        let made = match &importing.result {
            Some(Ok(imported)) => Some(imported.dir.clone()),
            _ => None,
        };
        match after {
            After::Replace | After::Retry => self.start_import(file, true),
            After::OpenFolder => {
                if let Some(folder) = importing.blocked_folder() {
                    crate::import::open_folder(folder);
                }
            }
            After::Another => {
                self.back();
                if let Screen::Choosing(_) = self.screen {
                    // The dialog again.
                    self.back();
                    let from = std::mem::replace(&mut self.screen, Screen::Playing);
                    self.choose_file(from);
                }
            }
            After::Done => {
                self.back();
                self.back();
            }
            After::Play => {
                let played = made
                    .ok_or_else(|| "nothing was imported".to_string())
                    .and_then(|dir| Installed::load(&dir))
                    .and_then(|pack| self.play_pack(pack));
                match played {
                    Ok(()) => self.screen = Screen::title(),
                    Err(e) => self.show_games_problem(e),
                }
            }
        }
    }

    /// The games screen, with the game being played selected.
    fn games(&self) -> Games {
        let playing = self.pack.as_ref().map(|p| p.manifest.id.as_str());
        Games::list(self.packs_dir.as_deref(), playing, self.bundled.is_some())
    }

    /// Shows the games screen, opened from the title, saying why a game
    /// couldn't be played.
    fn show_games_problem(&mut self, problem: String) {
        report::warn(&problem);
        let mut games = self.games();
        games.problem = Some(problem);
        self.screen = Screen::Games(games, Box::new(Screen::title()));
    }

    fn step_games(&mut self, frame: &Frame) {
        let Screen::Games(games, _) = &mut self.screen else {
            return;
        };
        let played = match games.update(frame) {
            None => return,
            Some(Pick::Back) => return self.back(),
            Some(Pick::Bundled) => match self.bundled.clone() {
                Some(path) => self.play_level(&path, None),
                None => return,
            },
            Some(Pick::Pack(i)) => {
                let pack = games.packs[i].clone();
                self.play_pack(pack)
            }
        };
        match played {
            Ok(()) => self.screen = Screen::title(),
            Err(e) => self.show_games_problem(e),
        }
    }

    /// Plays the first of `pack`'s levels, from the title screen.
    fn play_pack(&mut self, pack: Installed) -> Result<(), String> {
        let Some(level) = pack.levels().into_iter().next() else {
            return Err(format!("{} has nothing to play yet", pack.manifest.name));
        };
        self.play_level(&level, Some(pack))
    }

    /// Plays `pack`'s level at `path` straight away, without the title
    /// screen, from its start or from its arrival called `arrival`: for
    /// headless replays.
    pub fn play_installed(
        &mut self,
        path: &Path,
        pack: Installed,
        arrival: Option<&str>,
    ) -> Result<(), String> {
        self.play_level(path, Some(pack))?;
        if let Some(name) = arrival {
            let level = self.game.world.level.clone();
            let at = level
                .arrival(name)
                .cloned()
                .ok_or_else(|| format!("{} has no arrival {name}", path.display()))?;
            let world = World::try_arriving(level, self.game.world.feel.clone(), &at)
                .map_err(|e| e.to_string())?;
            self.game.replace_world(world);
        }
        self.screen = Screen::Playing;
        Ok(())
    }

    /// Plays the level at `path` with the movement being played, and
    /// `pack`'s sprites if it has them. Keeps the debug view setting,
    /// and turns it on for a level with nothing else to see.
    fn play_level(&mut self, path: &Path, pack: Option<Installed>) -> Result<(), String> {
        self.game = self.load(path, pack.as_ref())?;
        self.level_path = path.to_path_buf();
        self.left.clear();
        self.map = match pack.as_ref().map(|p| MapPlay::load(&p.dir)) {
            Some(Ok(map)) => map,
            Some(Err(e)) => {
                report::warn(format!("the level map can't be played: {e}"));
                None
            }
            None => None,
        };
        self.pack = pack;
        Ok(())
    }

    /// Runs a frame of the level map, beginning a level entered there.
    fn step_map(&mut self, frame: &Frame) {
        let Some(map) = &mut self.map else {
            self.screen = Screen::title();
            return;
        };
        let MapStep::Begin(path) = map.step(frame.held) else {
            return;
        };
        match self.load(&path, self.pack.as_ref()) {
            Ok(mut game) => {
                // Finishing a level carries its characters into the next
                // one (following.md, "When a level begins again").
                if self.game.world.flow.outcome == Outcome::Complete {
                    let world = game.world.clone().with_characters_of(&self.game.world);
                    game.replace_world(world);
                }
                self.game = game;
                self.level_path = path;
                self.left.clear();
                self.screen = Screen::Playing;
            }
            Err(e) => {
                report::warn(format!("can't play {}: {e}", path.display()));
                self.screen = Screen::title();
            }
        }
    }

    /// The level at `path` with the movement being played, and `pack`'s
    /// sprites (over the game's own) and look if it has them, at its start. Keeps the debug
    /// view setting, and turns it on for a level with nothing else to
    /// see. A pixel-art pack's pictures that the replacement art packs
    /// in the [`Session::art_dir`] have replacements for are drawn with
    /// them in Enhanced mode.
    fn load(&self, path: &Path, pack: Option<&Installed>) -> Result<Game, String> {
        let own = content_dir().join(SPRITES);
        let packed = pack.and_then(Installed::sprites);
        let mut sprites = vec![own.as_path()];
        sprites.extend(packed.as_deref());
        let mut look = Look {
            widescreen: self.widescreen_for(pack),
            ..pack.map_or(Look::RENDERED, |p| Look::of(&p.manifest))
        };
        look.pixel_art &= self.rendering_for(pack) != Some(Rendering::Enhanced);
        let feel = look.feel(&self.game.world.feel);
        let mut problems = Vec::new();
        let replacements = match (pack.filter(|p| p.manifest.pixel_art), &self.art_dir) {
            (Some(_), Some(dir)) => {
                let (replacements, troubles) = Replacements::load(dir);
                problems.extend(troubles);
                replacements
            }
            _ => Replacements::default(),
        };
        let game = load_game_with(path, &sprites, feel, &replacements, &mut problems);
        for problem in problems {
            report::warn(format!("replacement art: {problem}"));
        }
        let mut game = game?.with_look(look);
        game.debug |= self.game.debug;
        Ok(game)
    }

    /// Goes on where the level's entity `exit` leads, once play has left
    /// through it: into the area of the pack its `area` param names, a
    /// level in the same folder, at the arrival its `arrival` param names
    /// there, or that level's start without one. Coming back to an area
    /// play went on from, it goes on as it was left, and the areas left
    /// from it since are forgotten. Play carries the count, the lives and
    /// the letters from area to area ([`World::carrying`]). An exit that
    /// names no area, or one that can't be played, leads where
    /// [`Game::leave_by`] has it.
    ///
    /// Engine choices where the spec says nothing: an area that isn't
    /// one play went on from begins afresh each time, as the first
    /// level's room does after coming back from it.
    fn leave_by(&mut self, exit: usize) {
        let level = self.game.world.level.clone();
        let Some(entity) = level.entities.get(exit) else {
            return self.game.leave_by(exit);
        };
        let Some(area) = clamber_sim::area(entity) else {
            return self.game.leave_by(exit);
        };
        let arrival = entity.params.get("arrival").and_then(|v| v.as_str());
        if let Err(e) = self.go_to(area, arrival) {
            report::warn(format!("can't go on to {area}: {e}"));
            self.game.leave_by(exit);
        }
    }

    /// Goes on where the area's scene leads once it's over, as
    /// [`leave_by`](Session::leave_by) does where an exit leads: the
    /// scene's `area` and `arrival` in place of the exit's params.
    fn end_scene(&mut self) {
        let level = self.game.world.level.clone();
        let Some(scene) = &level.scene else {
            return;
        };
        let arrival = scene.arrival.as_deref();
        let Some(area) = &scene.area else {
            return self.game.come_back(arrival);
        };
        if let Err(e) = self.go_to(area, arrival) {
            report::warn(format!("can't go on to {area}: {e}"));
            self.game.come_back(arrival);
        }
    }

    /// Goes on in the area `area` at the arrival called `arrival`, as in
    /// [`leave_by`](Session::leave_by).
    fn go_to(&mut self, area: &str, arrival: Option<&str>) -> Result<(), String> {
        let path = self.level_path.with_file_name(format!("{area}.level"));
        let back = self.left.iter().position(|(p, _)| *p == path);
        let mut game = match back {
            Some(i) => self.left.drain(i..).next().expect("found").1,
            None => self.load(&path, self.pack.as_ref())?,
        };
        let level = game.world.level.clone();
        let at = match arrival {
            Some(name) => level
                .arrival(name)
                .cloned()
                .ok_or_else(|| format!("{} has no arrival {name}", path.display()))?,
            None => Arrival {
                name: String::new(),
                at: level.start,
                walk: None,
                entrance: None,
                drop: false,
            },
        };
        let world = match back {
            Some(_) => game.world.coming_back_from(&at, &self.game.world),
            None => World::try_arriving(level, game.world.feel.clone(), &at)
                .map_err(|e| e.to_string())?
                .carrying(&self.game.world),
        };
        game.replace_world(world);
        let from = std::mem::replace(&mut self.game, game);
        if back.is_none() {
            self.left.push((self.level_path.clone(), from));
        }
        self.level_path = path;
        Ok(())
    }

    /// Goes back to the first of the areas play went on from, if it went
    /// on from any, to begin again there.
    fn first_area(&mut self) {
        if self.left.is_empty() {
            return;
        }
        let (path, game) = self.left.swap_remove(0);
        self.left.clear();
        self.game = game;
        self.level_path = path;
    }

    /// The presets to choose from, with the one being played selected.
    /// Placeholders are shown, but can't be picked.
    fn movement_menu(&self) -> Menu<usize> {
        let presets = self.feels.presets();
        let playing = &self.game.world.feel.preset;
        let menu = Menu::new(
            presets
                .iter()
                .enumerate()
                .map(|(i, p)| (p.label.clone(), i)),
        )
        .with_unavailable(
            presets
                .iter()
                .enumerate()
                .filter(|(_, p)| p.placeholder)
                .map(|(i, _)| i),
        );
        match presets.iter().position(|p| &p.id == playing) {
            Some(i) => menu.with_selected(i),
            None => menu,
        }
    }

    /// Plays preset `id` from the start of the level, and remembers it.
    fn choose_preset(&mut self, id: &str) {
        if self.game.world.feel.preset == id {
            return;
        }
        let feel = match self.feels.feel(id) {
            Ok((feel, warnings)) => {
                warnings.iter().for_each(report::warn);
                feel
            }
            Err(e) => {
                report::warn(e);
                return;
            }
        };
        self.first_area();
        self.game.set_feel(feel);
        self.prefs.preset = Some(id.to_string());
        self.save_prefs("the movement choice");
    }

    /// What [`Prefs::widescreen`] calls the game being played.
    fn game_key(&self) -> &str {
        self.pack.as_ref().map_or(BUNDLED, |p| &p.manifest.id)
    }

    /// The widescreen mode `pack`, or the bundled game without one,
    /// plays in: the one picked for it, or else its own default.
    fn widescreen_for(&self, pack: Option<&Installed>) -> Widescreen {
        let key = pack.map_or(BUNDLED, |p| &p.manifest.id);
        let default = pack.map_or(Look::RENDERED, |p| Look::of(&p.manifest));
        self.prefs
            .widescreen
            .get(key)
            .copied()
            .unwrap_or(default.widescreen)
    }

    /// The widescreen mode of the game being played.
    pub fn widescreen(&self) -> Widescreen {
        self.widescreen_for(self.pack.as_ref())
    }

    /// Plays the game being played, and the areas play went on from, in
    /// the widescreen mode the prefs give it.
    pub fn apply_widescreen(&mut self) {
        let mode = self.widescreen();
        self.game.set_widescreen(mode);
        for (_, game) in &mut self.left {
            game.set_widescreen(mode);
        }
    }

    /// The rendering mode `pack` is drawn in, if it's pixel art: the one
    /// picked for it, or else [`Rendering::Original`]. Other games, and
    /// the bundled one, are always drawn at the window's resolution.
    fn rendering_for(&self, pack: Option<&Installed>) -> Option<Rendering> {
        let pack = pack.filter(|p| p.manifest.pixel_art)?;
        let picked = self.prefs.rendering.get(&pack.manifest.id).copied();
        Some(picked.unwrap_or_default())
    }

    /// The rendering mode of the game being played, if it has a choice.
    pub fn rendering(&self) -> Option<Rendering> {
        self.rendering_for(self.pack.as_ref())
    }

    /// Draws the game being played, and the areas play went on from, in
    /// the rendering mode the prefs give it.
    pub fn apply_rendering(&mut self) {
        let Some(mode) = self.rendering() else {
            return;
        };
        let pixel_art = mode == Rendering::Original;
        self.game.look.pixel_art = pixel_art;
        for (_, game) in &mut self.left {
            game.look.pixel_art = pixel_art;
        }
    }

    /// Saves the prefs, if there's somewhere to. On failure, warns that
    /// `what` won't be remembered.
    pub fn save_prefs(&self, what: &str) {
        if let Some(path) = &self.prefs_path
            && let Err(e) = self.prefs.save(path)
        {
            report::warn(format!("{e}; {what} won't be remembered"));
        }
    }

    /// Pauses the level if it's being played, as when the window loses
    /// focus.
    pub fn pause(&mut self) {
        if self.screen == Screen::Playing {
            self.screen = Screen::paused();
        }
    }

    fn play(&mut self, input: &InputFrame) {
        if everyone(input).pressed.contains(Action::Pause) {
            self.screen = Screen::paused();
            self.cues.push(Cue::Pause);
            return;
        }
        if !self.game.step(input) {
            return;
        }
        self.cues.extend(cues(&self.game.world));
        match self.game.world.flow.outcome {
            Outcome::Complete => match &mut self.map {
                Some(map) if map.playing() => {
                    map.finished(self.game.world.other.is_some());
                    self.screen = Screen::Map;
                }
                _ => self.screen = Screen::complete(),
            },
            Outcome::GameOver => self.screen = Screen::game_over(),
            Outcome::Exited(exit) => self.leave_by(exit),
            Outcome::Ended => self.end_scene(),
            _ => {}
        }
    }

    /// The music that should be playing.
    pub fn tune(&self) -> Option<Tune> {
        self.screen.tune()
    }

    /// Draws the screen. During play, `alpha` is how far to draw between
    /// the last two states; behind a menu the level stands still.
    /// How the next frame is drawn: the game's look in play and on the
    /// small screens over it, and the bundled look for the other menus,
    /// which are laid out for its view.
    pub fn look(&self) -> Look {
        match self.screen {
            Screen::Playing
            | Screen::Paused(_)
            | Screen::Complete(_)
            | Screen::GameOver(_)
            | Screen::Map => self.game.shown(),
            _ => Look::RENDERED,
        }
    }

    /// Draws the game and the screen over it into `canvas`, whose view
    /// should be [`look`](Session::look)'s.
    pub fn draw(&self, canvas: &mut dyn Surface, alpha: f64) {
        if let (Screen::Map, Some(map)) = (&self.screen, &self.map) {
            map.draw(canvas, self.game.map_cast());
            return;
        }
        let playing = self.screen == Screen::Playing;
        self.game.draw(canvas, if playing { alpha } else { 1.0 });
        if playing {
            return;
        }
        canvas.over_view();
        canvas.shade(DIM, 160);

        let centre = canvas.view().0 as i32 / 2;
        self.draw_screen(canvas, centre);
        let hints = self.hints();
        let style = self.input.pad_style(0);
        draw_hints(canvas, centre, self.device, style, &hints);
    }

    /// What the menu controls do on the screen showing, for the row of
    /// hints along the bottom.
    fn hints(&self) -> Vec<Hint<'static>> {
        const SELECT: Hint = (&[Action::Jump], "select");
        const BACK: Hint = (&[Action::Pause], "back");
        match &self.screen {
            Screen::Playing | Screen::Map => vec![],
            Screen::Controls(..) if self.input.remapping().is_some() => vec![],
            Screen::Controls(..) | Screen::Movement(..) | Screen::Options(..) => {
                vec![SELECT, BACK]
            }
            Screen::Games(..) => vec![SELECT, BACK],
            Screen::Credits(scroll, _) => {
                let lines = self.credits.wrapped(CREDITS_WIDTH).len();
                let more = *scroll > 0 || lines > scroll + CREDITS_ROWS;
                let scroll = (&[Action::Up, Action::Down][..], "scroll");
                if more { vec![scroll, BACK] } else { vec![BACK] }
            }
            Screen::Choosing(_) => vec![(&[Action::Pause], "browse in the game instead")],
            Screen::Picker(picker, _) if picker.typed.is_some() => vec![],
            Screen::Picker(..) => vec![
                (&[Action::Jump], "open"),
                (&[Action::Left], "up a folder"),
                BACK,
            ],
            Screen::Importing(importing, _) if importing.menu.is_some() => vec![SELECT],
            Screen::Importing(..) => vec![],
            Screen::Paused(_) => vec![SELECT, (&[Action::Pause], "resume")],
            Screen::Title(_) | Screen::Complete(_) | Screen::GameOver(_) => vec![SELECT],
        }
    }

    /// Draws the screen showing over the game, but for its hints.
    fn draw_screen(&self, canvas: &mut dyn Surface, centre: i32) {
        let (menu, top) = match &self.screen {
            Screen::Playing | Screen::Map => return,
            Screen::Controls(controls, _) => {
                controls.draw(canvas, centre, &self.input);
                return;
            }
            Screen::Movement(menu, _) => {
                self.draw_movement(canvas, menu, centre);
                return;
            }
            Screen::Options(menu, _) => {
                draw_text_centred(canvas, centre, 50, "OPTIONS", HEADING);
                menu.draw(canvas, centre, OPTIONS_MENU);
                let y = OPTIONS_MENU + menu.height() + 2 * LINE;
                let about = match menu.current() {
                    Setting::Vsync => Some(about_vsync(&self.prefs.display)),
                    Setting::Vrr => Some(about_vrr(self.prefs.display.vrr)),
                    Setting::Widescreen => Some(about_widescreen(self.widescreen())),
                    Setting::Rendering => self.rendering().map(about_rendering),
                    _ => None,
                };
                if let Some(about) = about {
                    draw_text_centred(canvas, centre, y, about, SUBTITLE);
                }
                return;
            }
            Screen::Credits(scroll, _) => {
                self.draw_credits(canvas, *scroll, centre);
                return;
            }
            Screen::Choosing(_) => {
                draw_choosing(canvas, centre);
                return;
            }
            Screen::Picker(picker, _) => {
                picker.draw(canvas, centre);
                return;
            }
            Screen::Importing(importing, _) => {
                importing.draw(canvas, centre);
                return;
            }
            Screen::Games(games, _) => {
                games.draw(canvas, centre);
                return;
            }
            Screen::Title(menu) => {
                draw_logo(canvas, centre as f32, self.title_frames);
                draw_scene(canvas, menu.age);
                // Under the logo, which is under the HUD's top row.
                let top = 44;
                let name = &self.game.world.level.name;
                if !name.is_empty() {
                    draw_text_centred(canvas, centre, top + 2 * LINE, name, SUBTITLE);
                }
                if self.pack.is_some() {
                    draw_text_centred(canvas, centre, top + 3 * LINE, PERSONAL, FAILED);
                }
                (menu, TITLE_MENU)
            }
            Screen::Paused(menu) => {
                draw_text_centred(canvas, centre, 70, "PAUSED", HEADING);
                (menu, PAUSED_MENU)
            }
            Screen::Complete(menu) => {
                draw_text_centred(canvas, centre, 50, "LEVEL COMPLETE", HEADING);
                self.draw_results(canvas, centre, 50 + 3 * LINE);
                (menu, COMPLETE_MENU)
            }
            Screen::GameOver(menu) => {
                draw_text_centred(canvas, centre, 70, "GAME OVER", FAILED);
                (menu, GAME_OVER_MENU)
            }
        };
        menu.draw(canvas, centre, top);
    }

    /// The movement screen: the presets, the one being played, and what
    /// the selected one is like.
    fn draw_movement(&self, canvas: &mut dyn Surface, menu: &Menu<usize>, centre: i32) {
        let presets = self.feels.presets();
        draw_text_centred(canvas, centre, 30, "MOVEMENT", HEADING);
        let playing = &self.game.world.feel.preset;
        let label = presets.get(playing).map_or(playing.as_str(), |p| &p.label);
        draw_text_centred(
            canvas,
            centre,
            30 + 2 * LINE,
            &format!("NOW: {label}"),
            SUBTITLE,
        );
        let top = MOVEMENT_MENU;
        menu.draw(canvas, centre, top);
        let about = match presets.iter().nth(menu.current()) {
            Some(p) if p.placeholder => "Not available yet",
            Some(p) => p.description.as_deref().unwrap_or(""),
            None => "",
        };
        draw_text_centred(
            canvas,
            centre,
            top + menu.height() + 2 * LINE,
            about,
            SUBTITLE,
        );
    }

    /// The credits from line `scroll`.
    fn draw_credits(&self, canvas: &mut dyn Surface, scroll: usize, centre: i32) {
        draw_text_centred(canvas, centre, 14, "CREDITS", HEADING);
        let lines = self.credits.wrapped(CREDITS_WIDTH);
        let left = centre - CREDITS_WIDTH / 2;
        let bottom = CREDITS_TOP + CREDITS_ROWS as i32 * LINE;
        let (l, r) = (left - 8, centre + (centre - left) + 8);
        let panel = [l, CREDITS_TOP - 7, r, bottom + 3];
        draw_panel(canvas, panel.map(|e| e as f32), 255);
        for (row, line) in lines.iter().skip(scroll).take(CREDITS_ROWS).enumerate() {
            let color = if line.heading { TITLE } else { HEADING };
            let y = CREDITS_TOP + row as i32 * LINE;
            draw_text(canvas, left, y, &line.text, color);
        }
    }

    /// The level's time and what was collected, centred on `centre_x`.
    fn draw_results(&self, canvas: &mut dyn Surface, centre_x: i32, top: i32) {
        let world = &self.game.world;
        let time = format!("TIME {}", clock_time(world.frame));
        draw_text_centred(canvas, centre_x, top, &time, HEADING);

        let mut items = vec![(Icon::Gem, world.flow.gems.to_string())];
        let relics = relic_total(world);
        if relics > 0 {
            items.push((Icon::Relic, format!("{}/{relics}", world.flow.relics)));
        }
        let gap = 2 * ADVANCE;
        let width: i32 = items
            .iter()
            .map(|(_, count)| counter_width(count) + gap)
            .sum::<i32>()
            - gap;
        let mut x = centre_x - width / 2;
        let y = top + 2 * LINE;
        for (icon, count) in items {
            x = draw_counter(canvas, (x, y), icon, &count) + gap;
        }
    }
}

/// While the player chooses a file in the system's file dialog.
fn draw_choosing(canvas: &mut dyn Surface, centre: i32) {
    draw_text_centred(canvas, centre, 40, "IMPORT A GAME", HEADING);
    let lines = [
        "Choose your own copy of a supported game",
        "in the window that opened",
    ];
    for (i, line) in lines.into_iter().enumerate() {
        draw_text_centred(canvas, centre, 40 + (2 + i as i32) * LINE, line, SUBTITLE);
    }
}

/// The options screen's items, showing the settings in `prefs`, the
/// game's widescreen mode, `widescreen`, and its rendering mode,
/// `rendering`, if it has one.
pub(crate) fn options_items(
    prefs: &Prefs,
    widescreen: Widescreen,
    rendering: Option<Rendering>,
) -> Vec<(String, Setting)> {
    let on = |on| if on { "On" } else { "Off" };
    let (display, volume) = (&prefs.display, &prefs.volume);
    let mut items = vec![
        (
            format!("Fullscreen: {}", on(display.fullscreen)),
            Setting::Fullscreen,
        ),
        (format!("VSync: {}", on(display.vsync)), Setting::Vsync),
        (format!("VRR: {}", on(display.vrr)), Setting::Vrr),
        (
            format!("Music: < {}% >", volume.music),
            Setting::MusicVolume,
        ),
        (
            format!("Effects: < {}% >", volume.effects),
            Setting::EffectsVolume,
        ),
        (
            format!("Widescreen: < {} >", widescreen.label()),
            Setting::Widescreen,
        ),
    ];
    if let Some(rendering) = rendering {
        items.push((
            format!("Rendering: < {} >", rendering.label()),
            Setting::Rendering,
        ));
    }
    items.push(("Back".into(), Setting::Back));
    items
}

/// What the VSync setting in `display` does, in a line.
fn about_vsync(display: &Display) -> &'static str {
    if display.vrr {
        "Not used while VRR is on, which doesn't tear either way"
    } else if display.vsync {
        "Waits for each refresh of the display, without tearing"
    } else {
        "Up to 240 frames a second; the picture may tear"
    }
}

/// What the VRR setting does, in a line, when it's `on` or off.
fn about_vrr(on: bool) -> &'static str {
    if on {
        "The display refreshes as each frame is ready, without tearing"
    } else {
        "For G-Sync, FreeSync and HDMI VRR displays; judders on others"
    }
}

/// What widescreen mode `mode` does, in a line.
fn about_widescreen(mode: Widescreen) -> &'static str {
    match mode {
        Widescreen::OriginalActivation => "Shows more; things act as in the original view",
        Widescreen::WideActivation => "Things appear and act across the whole window",
        Widescreen::OriginalFraming => "The original view, with borders at the sides",
    }
}

/// What rendering mode `mode` does, in a line.
fn about_rendering(mode: Rendering) -> &'static str {
    match mode {
        Rendering::Original => "On the original's pixel grid, scaled up",
        Rendering::Enhanced => "At the window's full resolution",
    }
}

/// Writes the input settings to `path`, creating its folder. Returns
/// where it went, for showing.
fn save(settings: &Settings, path: &Path) -> Result<String, String> {
    let shown = path.display().to_string();
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{shown}: {e}"))?;
    }
    std::fs::write(path, settings.to_toml_string()).map_err(|e| format!("{shown}: {e}"))?;
    Ok(shown)
}

/// Every player's input as one, for menus anyone can drive.
fn everyone(input: &InputFrame) -> Frame {
    input.players.iter().fold(Frame::default(), |all, f| Frame {
        held: all.held | f.held,
        pressed: all.pressed | f.pressed,
        released: all.released | f.released,
    })
}

/// Logic frames as `minutes:seconds.hundredths`.
pub fn clock_time(frames: u32) -> String {
    let seconds = frames / FPS;
    let hundredths = frames % FPS * 100 / FPS;
    format!("{}:{:02}.{:02}", seconds / 60, seconds % 60, hundredths)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_import::ImportError;
    use clamber_input::{Actions, Key};
    use clamber_level::Level;
    use clamber_sim::{World, generic_feel};

    use crate::camera::VIEW;
    use crate::canvas::Canvas;
    use crate::map::{LEVEL_MAP, MapPhase};
    use crate::mouse::Mouse;

    use super::*;

    const LEVEL: &str = r#"
        format = 1
        name = "Test level"
        size = [1000, 400]
        start = [500, 300]

        [[collision]]
        shape = "rect"
        rect = [0, 300, 1000, 100]

        [[entities]]
        type = "goal"
        at = [600, 300]
    "#;

    fn session(src: &str) -> Session {
        let level = Level::from_toml_str(src, "test").unwrap();
        Session::new(Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("default").unwrap()),
        )))
    }

    /// Input where `held` is held now and `before` the frame before.
    fn input(before: Actions, held: Actions) -> InputFrame {
        InputFrame::single(Frame::after(before, held))
    }

    fn press(action: Action) -> InputFrame {
        input(Actions::NONE, action.into())
    }

    fn idle() -> InputFrame {
        InputFrame::default()
    }

    #[test]
    fn starts_and_quits_from_the_title() {
        let mut s = session(LEVEL);
        assert!(matches!(s.screen, Screen::Title(_)));
        s.step(&press(Action::Pause));
        assert!(
            matches!(s.screen, Screen::Title(_)),
            "nothing to go back to"
        );
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing);

        let mut s = session(LEVEL);
        s.step(&press(Action::Up));
        s.step(&press(Action::Jump));
        assert!(s.quit);
    }

    /// Presses and lets go of `key` over two ticks.
    fn tap(s: &mut Session, key: Key) {
        s.handle(Event::Key { key, down: true });
        s.tick();
        s.handle(Event::Key { key, down: false });
        s.tick();
    }

    fn tap_pad(s: &mut Session, input: clamber_input::PadInput) {
        let pad = clamber_input::PadId(0);
        s.handle(Event::PadButton {
            pad,
            input,
            down: true,
        });
        s.tick();
        s.handle(Event::PadButton {
            pad,
            input,
            down: false,
        });
        s.tick();
    }

    #[test]
    fn menus_take_enter_and_escape_and_not_space() {
        let mut s = session(LEVEL);
        tap(&mut s, Key::Space);
        tap(&mut s, Key::KeyZ);
        assert!(matches!(s.screen, Screen::Title(_)), "jump keys do nothing");
        tap(&mut s, Key::Enter);
        assert_eq!(s.screen, Screen::Playing);
        tap(&mut s, Key::Enter);
        assert_eq!(s.screen, Screen::Playing, "Enter doesn't pause");
        tap(&mut s, Key::Escape);
        assert!(matches!(s.screen, Screen::Paused(_)));
        tap(&mut s, Key::Space);
        assert!(matches!(s.screen, Screen::Paused(_)));
        tap(&mut s, Key::Escape);
        assert_eq!(s.screen, Screen::Playing, "Escape closes it again");

        // Pause, then Options and back with Escape, then resume with Enter.
        tap(&mut s, Key::Escape);
        while !matches!(&s.screen, Screen::Paused(m) if m.current() == Item::Options) {
            tap(&mut s, Key::ArrowDown);
        }
        tap(&mut s, Key::NumpadEnter);
        assert!(matches!(s.screen, Screen::Options(..)));
        tap(&mut s, Key::Escape);
        assert!(matches!(s.screen, Screen::Paused(_)));
        while !matches!(&s.screen, Screen::Paused(m) if m.current() == Item::Resume) {
            tap(&mut s, Key::ArrowUp);
        }
        tap(&mut s, Key::Enter);
        assert_eq!(s.screen, Screen::Playing);
    }

    #[test]
    fn menus_take_the_pads_bottom_and_right_buttons_and_start() {
        use clamber_input::PadInput as P;
        let mut s = session(LEVEL);
        s.handle(Event::PadConnected(clamber_input::PadId(0)));
        tap_pad(&mut s, P::South);
        assert_eq!(s.screen, Screen::Playing);
        tap_pad(&mut s, P::East);
        assert_eq!(
            s.screen,
            Screen::Playing,
            "the right button is tandem in play"
        );
        tap_pad(&mut s, P::Start);
        assert!(matches!(s.screen, Screen::Paused(_)));
        tap_pad(&mut s, P::Start);
        assert_eq!(s.screen, Screen::Playing, "Start closes it again");
        tap_pad(&mut s, P::Start);
        tap_pad(&mut s, P::East);
        assert_eq!(s.screen, Screen::Playing, "the right button goes back");
    }

    #[test]
    fn remapping_jump_leaves_the_menus_alone() {
        let mut s = session(LEVEL);
        while !matches!(&s.screen, Screen::Title(m) if m.current() == Item::Controls) {
            tap(&mut s, Key::ArrowDown);
        }
        tap(&mut s, Key::Enter);
        assert!(matches!(s.screen, Screen::Controls(..)));
        // Jump's row, then remap it to Enter.
        while !matches!(&s.screen, Screen::Controls(c, _) if c.row() == crate::controls::Row::Action(Action::Jump))
        {
            tap(&mut s, Key::ArrowDown);
        }
        tap(&mut s, Key::Enter);
        assert!(s.input.remapping().is_some());
        tap(&mut s, Key::Enter);
        assert!(s.input.remapping().is_none());
        let bindings = &s.input.settings().players[0];
        assert_eq!(bindings.key_action(Key::Enter), Some(Action::Jump));
        assert_eq!(bindings.key_action(Key::Space), None);
        // Enter still confirms and Space still does nothing in menus.
        tap(&mut s, Key::Space);
        assert!(s.input.remapping().is_none());
        tap(&mut s, Key::Escape);
        assert!(matches!(s.screen, Screen::Title(_)));
    }

    #[test]
    fn hints_show_the_menu_controls_in_use() {
        let mut s = session(LEVEL);
        let draw = |s: &Session| {
            let mut canvas = Canvas::new(VIEW.0, VIEW.1);
            s.draw(&mut canvas, 1.0);
            canvas
        };
        let keys = draw(&s);
        s.handle(Event::PadButton {
            pad: clamber_input::PadId(0),
            input: clamber_input::PadInput::North,
            down: true,
        });
        assert_ne!(draw(&s), keys, "pad buttons instead of keys");
        assert_eq!(s.hints().len(), 1);
        tap(&mut s, Key::Escape);
        tap_pad(&mut s, clamber_input::PadInput::South);
        assert!(s.hints().is_empty(), "none in play");
    }

    /// Goes from the selected item of a menu to the controls screen.
    fn open_controls(s: &mut Session) {
        while !matches!(&s.screen, Screen::Title(m) | Screen::Paused(m) if m.current() == Item::Controls)
        {
            s.step(&press(Action::Down));
        }
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Controls(..)));
    }

    #[test]
    fn remaps_from_the_title_and_goes_back() {
        let mut s = session(LEVEL);
        open_controls(&mut s);
        // Remap left on the keyboard: the screen starts there.
        s.step(&press(Action::Jump));
        s.handle(Event::Key {
            key: Key::KeyQ,
            down: true,
        });
        let Screen::Controls(controls, _) = &s.screen else {
            panic!()
        };
        assert_eq!(controls.status(), Some("left: Q"));
        let bindings = &s.input.settings().players[0];
        assert_eq!(bindings.key_action(Key::KeyQ), Some(Action::Left));
        s.tick();
        assert!(matches!(s.screen, Screen::Controls(..)), "Q was swallowed");

        s.step(&press(Action::Pause));
        let Screen::Title(menu) = &s.screen else {
            panic!("back to the title")
        };
        assert_eq!(menu.current(), Item::Controls);
    }

    #[test]
    fn opens_controls_from_pause_and_returns_there() {
        let mut s = session(LEVEL);
        s.step(&press(Action::Jump));
        s.step(&press(Action::Pause));
        open_controls(&mut s);
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        assert!(canvas.pixels.contains(&HEADING));
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Paused(_)));
        s.step(&press(Action::Pause));
        assert_eq!(s.screen, Screen::Playing);
    }

    #[test]
    fn saves_the_settings_file() {
        let dir = std::env::temp_dir().join(format!("clamber-session-{}", std::process::id()));
        let path = dir.join("nested").join("input.toml");
        let mut s = session(LEVEL);
        s.settings_path = Some(path.clone());
        open_controls(&mut s);
        s.step(&press(Action::Jump));
        s.handle(Event::Key {
            key: Key::KeyQ,
            down: true,
        });
        // From left, up past the player row to Back, then Save.
        for _ in 0..3 {
            s.step(&press(Action::Up));
        }
        s.step(&press(Action::Jump));
        let saved = std::fs::read_to_string(&path).expect("saved");
        let read = Settings::from_toml_str(&saved, "saved").unwrap();
        assert_eq!(&read, s.input.settings());
        assert_eq!(read.players[0].key_action(Key::KeyQ), Some(Action::Left));
        std::fs::remove_dir_all(dir).unwrap();

        s.settings_path = None;
        s.step(&press(Action::Jump));
        let Screen::Controls(controls, _) = &s.screen else {
            panic!()
        };
        assert_eq!(controls.status(), Some("Can't save: no settings file"));
    }

    /// Goes from the title to the movement screen.
    /// Picks `item` on the title screen.
    fn pick(s: &mut Session, item: Item) {
        for _ in 0..20 {
            if matches!(&s.screen, Screen::Title(m) if m.current() == item) {
                break;
            }
            s.step(&press(Action::Down));
        }
        s.step(&press(Action::Jump));
    }

    fn open_movement(s: &mut Session) {
        pick(s, Item::Movement);
        assert!(matches!(s.screen, Screen::Movement(..)));
    }

    fn movement_menu(s: &Session) -> &Menu<usize> {
        let Screen::Movement(menu, _) = &s.screen else {
            panic!("not on the movement screen")
        };
        menu
    }

    #[test]
    fn the_movement_screen_lists_presets_and_shows_placeholders_unavailable() {
        let mut s = session(LEVEL);
        open_movement(&mut s);
        let menu = movement_menu(&s);
        let labels: Vec<_> = menu.items.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Default",
                "Snappy",
                "Floaty",
                "Classic 1",
                "Classic 2",
                "Classic 3"
            ]
        );
        assert_eq!(menu.unavailable, [4, 5]);
        assert_eq!(menu.current(), 0, "starts on the preset being played");

        // Up wraps to Classic 3, which can't be picked.
        s.step(&press(Action::Up));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Movement(..)));
        assert_eq!(s.game.world.feel.preset, "default");

        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);

        s.step(&press(Action::Pause));
        let Screen::Title(menu) = &s.screen else {
            panic!("back to the title")
        };
        assert_eq!(menu.current(), Item::Movement);
        assert_eq!(s.game.world.feel.preset, "default");
    }

    #[test]
    fn picking_a_preset_plays_it_and_remembers_it() {
        let dir =
            std::env::temp_dir().join(format!("clamber-session-prefs-{}", std::process::id()));
        let path = dir.join("prefs.toml");
        let mut s = session(LEVEL);
        s.prefs_path = Some(path.clone());
        open_movement(&mut s);
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)));
        assert_eq!(s.game.world.feel.preset, "snappy");
        assert_eq!(
            s.game.world.feel.world,
            generic_feel("snappy").unwrap().world
        );
        assert_eq!(s.prefs.preset.as_deref(), Some("snappy"));
        assert_eq!(Prefs::load(&path), s.prefs);
        std::fs::remove_dir_all(dir).unwrap();

        // Opening it again starts on the new preset.
        s.step(&press(Action::Jump));
        assert_eq!(movement_menu(&s).current(), 1);
    }

    /// The middle of row `i` of a menu that starts at `top`.
    fn row_at(top: i32, i: usize) -> (f32, f32) {
        (
            VIEW.0 as f32 / 2.0,
            (top + i as i32 * (LINE + 3) + LINE / 2) as f32,
        )
    }

    fn point(s: &mut Session, (x, y): (f32, f32)) {
        s.mouse(Mouse::Moved(x, y));
    }

    fn click(s: &mut Session) {
        s.mouse(Mouse::Left { down: true });
        s.tick();
        s.mouse(Mouse::Left { down: false });
        s.tick();
    }

    fn selected(s: &Session) -> usize {
        match &s.screen {
            Screen::Title(m) | Screen::Paused(m) => m.selected,
            Screen::Options(m, _) => m.selected,
            _ => panic!("not a menu"),
        }
    }

    fn open_options(s: &mut Session) {
        pick(s, Item::Options);
        assert!(matches!(s.screen, Screen::Options(..)));
    }

    #[test]
    fn hovering_selects_until_a_key_is_pressed() {
        let mut s = session(LEVEL);
        point(&mut s, row_at(TITLE_MENU, 2));
        assert_eq!(selected(&s), 2);
        tap(&mut s, Key::ArrowDown);
        assert_eq!(selected(&s), 3, "keys go on from the hovered item");
        s.tick();
        assert_eq!(selected(&s), 3, "the pointer standing still doesn't fight");
        point(&mut s, row_at(TITLE_MENU, 0));
        assert_eq!(selected(&s), 0);
        point(&mut s, (2.0, 2.0));
        assert_eq!(selected(&s), 0, "off the items nothing changes");
    }

    #[test]
    fn clicking_picks_and_a_right_click_goes_back() {
        let mut s = session(LEVEL);
        let Screen::Title(menu) = &s.screen else {
            panic!()
        };
        let options = menu
            .items
            .iter()
            .position(|i| i.1 == Item::Options)
            .unwrap();
        point(&mut s, row_at(TITLE_MENU, options));
        click(&mut s);
        assert!(matches!(s.screen, Screen::Options(..)));
        s.mouse(Mouse::Right { down: true });
        s.tick();
        assert!(matches!(s.screen, Screen::Title(_)));
        point(&mut s, row_at(TITLE_MENU, 0));
        click(&mut s);
        assert_eq!(s.screen, Screen::Playing);
    }

    #[test]
    fn the_wheel_moves_the_selection() {
        let mut s = session(LEVEL);
        s.mouse(Mouse::Wheel(-1.0));
        s.tick();
        assert_eq!(selected(&s), 1);
        s.mouse(Mouse::Wheel(1.0));
        s.tick();
        assert_eq!(selected(&s), 0);
    }

    #[test]
    fn a_volume_is_clicked_and_dragged_and_toggles_change_on_click() {
        let dir = std::env::temp_dir().join(format!("clamber-mouse-{}", std::process::id()));
        let path = dir.join("prefs.toml");
        let mut s = session(LEVEL);
        s.prefs_path = Some(path.clone());
        open_options(&mut s);
        let music = 3;
        let (centre, y) = row_at(OPTIONS_MENU, music);
        point(&mut s, (centre, y));
        s.mouse(Mouse::Left { down: true });
        assert_eq!(s.prefs.volume.music, 50, "the middle is half");
        assert_eq!(selected(&s), music);
        point(&mut s, (centre + 1000.0, y));
        assert_eq!(s.prefs.volume.music, 100);
        point(&mut s, (-1000.0, y + 40.0));
        assert_eq!(s.prefs.volume.music, 0, "dragging goes on off the row");
        assert!(!path.exists(), "saved when let go");
        s.mouse(Mouse::Left { down: false });
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("music = 0"), "{saved}");
        let Screen::Options(menu, _) = &s.screen else {
            panic!()
        };
        assert_eq!(menu.items[music].0, "Music: < 0% >");

        point(&mut s, row_at(OPTIONS_MENU, 0));
        click(&mut s);
        assert!(s.prefs.display.fullscreen);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_mouse_does_nothing_in_play() {
        let mut s = session(LEVEL);
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing);
        s.mouse(Mouse::Right { down: true });
        s.mouse(Mouse::Wheel(1.0));
        s.tick();
        assert_eq!(s.screen, Screen::Playing);
    }

    #[test]
    fn options_toggle_the_display_settings_and_remember_them() {
        let dir = std::env::temp_dir().join(format!("clamber-options-{}", std::process::id()));
        let path = dir.join("prefs.toml");
        let mut s = session(LEVEL);
        s.prefs_path = Some(path.clone());
        while !matches!(&s.screen, Screen::Title(m) if m.current() == Item::Options) {
            s.step(&press(Action::Down));
        }
        s.step(&press(Action::Jump));
        let options = |s: &Session| {
            let Screen::Options(menu, _) = &s.screen else {
                panic!("not on the options screen")
            };
            menu.clone()
        };
        let labels =
            |s: &Session| -> Vec<String> { options(s).items.into_iter().map(|(l, _)| l).collect() };
        assert_eq!(
            labels(&s),
            [
                "Fullscreen: Off",
                "VSync: On",
                "VRR: Off",
                "Music: < 80% >",
                "Effects: < 80% >",
                "Widescreen: < Wide activation >",
                "Back"
            ]
        );
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        assert!(canvas.pixels.contains(&HEADING));

        s.step(&press(Action::Jump));
        assert!(s.prefs.display.fullscreen);
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(!s.prefs.display.vsync);
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(s.prefs.display.vrr);
        assert_eq!(labels(&s)[..3], ["Fullscreen: On", "VSync: Off", "VRR: On"]);
        assert_eq!(options(&s).current(), Setting::Vrr, "stays put");
        assert_eq!(Prefs::load(&path), s.prefs);

        // Left turns a volume down, Right up to full, and Jump up,
        // wrapping around to silent.
        s.prefs.volume = Volume {
            music: 100,
            effects: 100,
        };
        s.step(&press(Action::Down));
        s.step(&press(Action::Left));
        assert_eq!(s.prefs.volume.music, 90);
        s.step(&press(Action::Right));
        s.step(&press(Action::Right));
        assert_eq!(s.prefs.volume.music, 100);
        s.step(&press(Action::Jump));
        assert_eq!(s.prefs.volume.music, 0);
        s.step(&press(Action::Left));
        assert_eq!(s.prefs.volume.music, 0);
        s.step(&press(Action::Down));
        s.step(&press(Action::Left));
        assert_eq!(labels(&s)[3..5], ["Music: < 0% >", "Effects: < 90% >"]);
        assert_eq!(options(&s).current(), Setting::EffectsVolume);
        assert_eq!(Prefs::load(&path), s.prefs);
        std::fs::remove_dir_all(dir).unwrap();

        s.step(&press(Action::Down));
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        let Screen::Title(menu) = &s.screen else {
            panic!("back to the title")
        };
        assert_eq!(menu.current(), Item::Options);
    }

    #[test]
    fn a_game_with_a_view_of_its_own_is_framed_to_it() {
        let mut s = session(LEVEL);
        s.screen = Screen::Playing;
        assert_eq!(s.look(), Look::RENDERED, "no view of its own");
        s.game.set_widescreen(Widescreen::OriginalFraming);
        assert_eq!(s.look().view, VIEW, "still no view of its own");
        s.game.set_feel(generic_feel("classic-1").unwrap());
        let framed = Look {
            view: (256, VIEW.1),
            widescreen: Widescreen::OriginalFraming,
            ..Look::RENDERED
        };
        assert_eq!(s.look(), framed);
        assert_eq!(s.game.camera.view, framed.view);
        let mut canvas = crate::Canvas::new(framed.view.0, framed.view.1);
        s.draw(&mut canvas, 1.0);
        assert_eq!(
            s.game.world.view_middle().map(|x| x.to_f64()),
            Some(s.game.camera.top_left(1.0).0 + 128.0),
            "the picture is the view's"
        );
        s.game.restart();
        assert_eq!(s.game.camera.view, framed.view, "kept on a restart");
    }

    #[test]
    fn options_pick_each_games_widescreen_mode_and_remember_it() {
        let dir = std::env::temp_dir().join(format!("clamber-widescreen-{}", std::process::id()));
        let path = dir.join("prefs.toml");
        let mut s = session(LEVEL);
        s.prefs_path = Some(path.clone());
        s.game.set_feel(generic_feel("classic-1").unwrap());
        s.step(&press(Action::Pause));
        while !matches!(&s.screen, Screen::Title(m) if m.current() == Item::Options) {
            s.step(&press(Action::Down));
        }
        s.step(&press(Action::Jump));
        while !matches!(&s.screen, Screen::Options(m, _) if m.current() == Setting::Widescreen) {
            s.step(&press(Action::Down));
        }
        let label = |s: &Session| {
            let Screen::Options(menu, _) = &s.screen else {
                panic!("not on the options screen")
            };
            menu.items[5].0.clone()
        };
        assert_eq!(
            s.widescreen(),
            Widescreen::WideActivation,
            "the bundled default"
        );
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        assert!(canvas.pixels.contains(&SUBTITLE), "says what it does");

        s.step(&press(Action::Right));
        assert_eq!(s.widescreen(), Widescreen::OriginalFraming);
        assert_eq!(s.game.look.widescreen, Widescreen::OriginalFraming);
        assert_eq!(label(&s), "Widescreen: < Original framing >");
        assert_eq!(
            s.prefs.widescreen.get(BUNDLED),
            Some(&Widescreen::OriginalFraming)
        );
        assert_eq!(Prefs::load(&path), s.prefs);
        s.step(&press(Action::Jump));
        assert_eq!(s.widescreen(), Widescreen::OriginalActivation);
        assert_eq!(s.game.shown().view, VIEW, "the whole window");
        s.step(&press(Action::Left));
        s.step(&press(Action::Left));
        assert_eq!(s.widescreen(), Widescreen::WideActivation);
        assert_eq!(Prefs::load(&path), s.prefs);
        assert_eq!(s.game.shown().view, VIEW, "the whole window");
        assert_eq!(s.game.world.feel.window, Some(VIEW.0), "things go by it");
        std::fs::remove_dir_all(dir).unwrap();

        // Another game has its own.
        s.prefs.widescreen.clear();
        s.prefs
            .widescreen
            .insert("title-1".into(), Widescreen::OriginalFraming);
        s.apply_widescreen();
        assert_eq!(s.game.look.widescreen, Widescreen::WideActivation);
    }

    #[test]
    fn credits_scroll_and_go_back_to_the_title() {
        let mut s = session(LEVEL);
        s.credits = Credits::from_markdown(&"A line of credits.\n\n".repeat(30));
        while !matches!(&s.screen, Screen::Title(m) if m.current() == Item::Credits) {
            s.step(&press(Action::Down));
        }
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Credits(0, _)));
        assert_eq!(s.tune(), Some(Tune::Title));
        let mut top = Game::canvas();
        s.draw(&mut top, 1.0);
        assert!(top.pixels.contains(&HEADING) && top.pixels.contains(&SUBTITLE));

        s.step(&press(Action::Up));
        assert!(
            matches!(s.screen, Screen::Credits(0, _)),
            "already at the top"
        );
        for _ in 0..100 {
            s.step(&press(Action::Down));
        }
        let last = s.credits.wrapped(CREDITS_WIDTH).len() - CREDITS_ROWS;
        assert!(matches!(s.screen, Screen::Credits(n, _) if n == last));
        let mut bottom = Game::canvas();
        s.draw(&mut bottom, 1.0);
        assert_ne!(top, bottom);

        s.step(&press(Action::Jump));
        let Screen::Title(menu) = &s.screen else {
            panic!("back to the title")
        };
        assert_eq!(menu.current(), Item::Credits);
    }

    #[test]
    fn the_title_and_play_have_music_and_the_ends_of_the_level_dont() {
        let mut s = session(LEVEL);
        assert_eq!(s.tune(), Some(Tune::Title));
        s.step(&press(Action::Jump));
        assert_eq!(s.tune(), Some(Tune::Level));
        for screen in [Screen::complete(), Screen::game_over()] {
            s.screen = screen;
            assert_eq!(s.tune(), None);
        }
    }

    #[test]
    fn options_from_pause_go_back_there_with_the_music_on() {
        let mut s = session(LEVEL);
        s.step(&press(Action::Jump));
        s.step(&press(Action::Pause));
        while !matches!(&s.screen, Screen::Paused(m) if m.current() == Item::Options) {
            s.step(&press(Action::Down));
        }
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Options(..)));
        assert_eq!(s.tune(), Some(Tune::Level));
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Paused(_)));
    }

    #[test]
    fn pausing_from_outside_only_pauses_play() {
        let mut s = session(LEVEL);
        s.pause();
        assert!(matches!(s.screen, Screen::Title(_)));
        s.step(&press(Action::Jump));
        s.pause();
        assert!(matches!(s.screen, Screen::Paused(_)));
    }

    #[test]
    fn the_title_waits_for_the_player() {
        let mut s = session(LEVEL);
        for _ in 0..10 {
            s.step(&idle());
        }
        assert_eq!(s.game.world.frame, 0);
    }

    #[test]
    fn pauses_resumes_and_restarts() {
        let mut s = session(LEVEL);
        s.step(&press(Action::Jump));
        for _ in 0..5 {
            s.step(&idle());
        }
        assert_eq!(s.game.world.frame, 5);

        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Paused(_)));
        s.step(&idle());
        assert_eq!(s.game.world.frame, 5, "the level stands still");
        s.step(&press(Action::Pause));
        assert_eq!(s.screen, Screen::Playing, "pause again resumes");

        s.step(&press(Action::Pause));
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing, "Resume");
        assert_eq!(s.game.world.frame, 5);

        s.step(&press(Action::Pause));
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing, "Restart level");
        assert_eq!(s.game.world.frame, 0);

        s.step(&press(Action::Pause));
        s.step(&press(Action::Up));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)), "Quit");
        assert!(!s.quit, "only back to the title");
    }

    #[test]
    fn shows_the_complete_screen_at_the_goal() {
        let mut s = session(LEVEL);
        s.step(&press(Action::Jump));
        let right = Actions::from(Action::Right);
        let mut before = Actions::NONE;
        while s.screen == Screen::Playing {
            s.step(&input(before, right));
            before = right;
            assert!(s.game.world.frame < 600, "never reached the goal");
        }
        assert!(matches!(s.screen, Screen::Complete(_)));
        let frame = s.game.world.frame;
        s.step(&idle());
        assert_eq!(s.game.world.frame, frame);

        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 0.5);
        // The gem icon's bright green-blue, over the dimmed level.
        let gem = |&c: &u32| {
            let [r, g, b] = [c >> 16, c >> 8 & 0xff, c & 0xff];
            r < 0x80 && g > 0xc0 && b > 0x90
        };
        assert!(canvas.pixels.iter().any(gem), "the gems collected");

        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing, "Play again");
        assert_eq!(s.game.world.frame, 0);
    }

    #[test]
    fn game_over_then_the_level_restarts() {
        // Nothing to stand on, so every life falls out of the level.
        let src = LEVEL.replace("rect = [0, 300, 1000, 100]", "rect = [0, 0, 10, 10]");
        let mut s = session(&src);
        s.step(&press(Action::Jump));
        while s.screen == Screen::Playing {
            s.step(&idle());
        }
        assert!(matches!(s.screen, Screen::GameOver(_)));
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::GameOver(_)), "needs a choice");
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing, "Try again");
        assert_eq!(s.game.world.flow.lives, clamber_sim::START_LIVES);
    }

    #[test]
    fn any_player_drives_the_menus() {
        let mut s = session(LEVEL);
        let mut input = idle();
        input.players[1] = Frame::after(Actions::NONE, Action::Jump.into());
        s.step(&input);
        assert_eq!(s.screen, Screen::Playing);
    }

    #[test]
    fn menus_draw_over_the_dimmed_level() {
        let mut s = session(LEVEL);
        let mut playing = Game::canvas();
        s.game.draw(&mut playing, 1.0);
        let mut title = Game::canvas();
        s.draw(&mut title, 1.0);
        assert_ne!(title.get(192, 35), playing.get(192, 35), "the logo");
        assert!(title.pixels.contains(&SUBTITLE), "the level's name");
        assert_ne!(title.get(0, 100), playing.get(0, 100));

        s.step(&press(Action::Jump));
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        assert_eq!(canvas, playing);
    }

    /// A folder of its own for one test, outside the source tree.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("clamber-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes [`LEVEL`], renamed, as the pack's one level.
    struct LevelDecoder;

    impl clamber_import::Decoder for LevelDecoder {
        fn name(&self) -> &str {
            "level"
        }
        fn applies_to(&self, _: &clamber_import::Known) -> bool {
            true
        }
        fn decode(
            &self,
            _: &clamber_import::Image,
            pack: &mut clamber_import::PackWriter,
        ) -> Result<(), String> {
            let src = LEVEL.replace("Test level", "Imported level");
            pack.write("levels/first.level", src.as_bytes())
        }
    }

    /// A session that imports a made-up image, `game.img` in a folder of
    /// its own, into a packs folder of its own, and starts with a level
    /// file to go back to.
    fn importing_session(name: &str) -> (Session, PathBuf) {
        let dir = scratch(name);
        let image: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
        let games = dir.join("games");
        std::fs::create_dir_all(&games).unwrap();
        std::fs::write(games.join("game.img"), &image).unwrap();
        std::fs::write(dir.join("bundled.level"), LEVEL).unwrap();

        let mut s = session(LEVEL);
        s.level_path = dir.join("bundled.level");
        s.bundled = Some(s.level_path.clone());
        s.packs_dir = Some(dir.join("packs"));
        s.prefs.import_folder = Some(games);
        let image = clamber_import::Image::new(image);
        s.importer = Arc::new(Importer {
            catalog: clamber_import::Catalog {
                known: vec![clamber_import::Known {
                    title: "title-9".into(),
                    release: "made-up release".into(),
                    size: image.len() as u64,
                    sha256: image.sha256(),
                }],
            },
            decoders: vec![Box::new(LevelDecoder)],
        });
        (s, dir)
    }

    /// Steps until the import that's showing has finished.
    fn finish_import(s: &mut Session) -> &Importing {
        for _ in 0..500 {
            if let Screen::Importing(importing, _) = &s.screen
                && importing.result.is_some()
            {
                break;
            }
            s.step(&idle());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        match &s.screen {
            Screen::Importing(importing, _) if importing.result.is_some() => importing,
            other => panic!("not a finished import: {other:?}"),
        }
    }

    /// From the title, picks `game.img` in the picker.
    fn import_the_game(s: &mut Session) {
        pick(s, Item::Import);
        let Screen::Picker(picker, _) = &s.screen else {
            panic!("the picker opens: {:?}", s.screen);
        };
        assert!(picker.dir.ends_with("games"), "{:?}", picker.dir);
        let labels: Vec<_> = picker.menu.items.iter().map(|i| i.0.as_str()).collect();
        assert_eq!(labels, ["../", "game.img"]);
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Importing(..)), "{:?}", s.screen);
    }

    #[test]
    fn imports_a_game_from_the_title_and_plays_it() {
        let (mut s, dir) = importing_session("import");
        import_the_game(&mut s);
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        let importing = finish_import(&mut s);
        let imported = importing.result.clone().unwrap().unwrap();
        assert_eq!(imported.dir, dir.join("packs/title-9/import-1"));
        assert!(!imported.manifest.redistribute);
        assert_eq!(importing.menu.as_ref().unwrap().current(), After::Play);
        let report = dir.join("packs/import-report.txt");
        assert_eq!(importing.report.as_ref(), Some(&report));
        let report = std::fs::read_to_string(report).unwrap();
        assert!(
            report.contains("Imported title-9 (made-up release)"),
            "{report}"
        );
        assert!(report.contains("Nothing was skipped."), "{report}");
        let mut done = Game::canvas();
        s.draw(&mut done, 1.0);
        assert_ne!(canvas, done, "shows what came of it");

        s.step(&press(Action::Jump));
        let Screen::Title(_) = &s.screen else {
            panic!("plays it from the title: {:?}", s.screen);
        };
        assert_eq!(s.game.world.level.name, "Imported level");
        assert_eq!(
            s.level_path,
            dir.join("packs/title-9/import-1/levels/first.level")
        );
        let pack = s.pack.as_ref().expect("a personal import");
        assert_eq!(pack.manifest.id, "title-9");
        let mut title = Game::canvas();
        s.draw(&mut title, 1.0);
        assert!(
            title.pixels.contains(&FAILED),
            "marked as a personal import"
        );
        assert_eq!(s.prefs.import_folder, Some(dir.join("games")));

        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plays_a_pixel_art_pack_at_its_own_view() {
        let (mut s, dir) = importing_session("look");
        import_the_game(&mut s);
        finish_import(&mut s);
        s.step(&press(Action::Pause));
        let manifest = dir.join("packs/title-9/import-1/manifest.toml");
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(text.contains("pixel_art = true"), "{text}");
        std::fs::write(&manifest, format!("{text}view = [256, 224]\n")).unwrap();
        let pack = Installed::load(&dir.join("packs/title-9")).unwrap();
        s.play_pack(pack).unwrap();
        let look = Look {
            view: (256, 224),
            pixel_art: true,
            widescreen: Widescreen::OriginalActivation,
        };
        assert_eq!(s.game.look, look);
        assert_eq!(s.game.camera.view, look.view);
        // The menus are laid out for the bundled view.
        s.screen = Screen::title();
        assert_eq!(s.look(), Look::RENDERED);

        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing);
        assert_eq!(s.look(), look);
        let mut canvas = crate::Canvas::new(256, 224);
        s.draw(&mut canvas, 1.0);
        s.game.restart();
        assert_eq!(s.game.camera.view, look.view, "kept on a restart");
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Paused(_)), "{:?}", s.screen);
        assert_eq!(s.look(), look, "pausing doesn't change the view");
        s.draw(&mut canvas, 1.0);

        // The bundled game has the bundled look.
        let bundled = s.bundled.clone().unwrap();
        s.play_level(&bundled, None).unwrap();
        assert_eq!(s.game.look, Look::RENDERED);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn options_pick_a_pixel_art_packs_rendering_mode() {
        let (mut s, dir) = importing_session("rendering");
        import_the_game(&mut s);
        finish_import(&mut s);
        let pack = Installed::load(&dir.join("packs/title-9")).unwrap();
        s.play_pack(pack.clone()).unwrap();
        assert_eq!(s.rendering(), Some(Rendering::Original), "the default");
        assert!(s.game.look.pixel_art);
        s.screen = Screen::Playing;
        s.step(&press(Action::Pause));
        while !matches!(&s.screen, Screen::Paused(m) if m.current() == Item::Options) {
            s.step(&press(Action::Down));
        }
        s.step(&press(Action::Jump));
        while !matches!(&s.screen, Screen::Options(m, _) if m.current() == Setting::Rendering) {
            s.step(&press(Action::Down));
        }
        let label = |s: &Session| {
            let Screen::Options(menu, _) = &s.screen else {
                panic!("not on the options screen")
            };
            menu.items[menu.selected].0.clone()
        };
        assert_eq!(label(&s), "Rendering: < Original >");
        s.step(&press(Action::Right));
        assert_eq!(label(&s), "Rendering: < Enhanced >");
        assert_eq!(s.prefs.rendering.get("title-9"), Some(&Rendering::Enhanced));
        assert!(!s.game.look.pixel_art, "drawn at the window's resolution");
        s.play_pack(pack).unwrap();
        assert!(!s.game.look.pixel_art, "remembered for the game");
        s.prefs.rendering.clear();
        s.apply_rendering();
        assert!(s.game.look.pixel_art);

        // Menus over the game view are marked to be drawn apart from it.
        s.screen = Screen::paused();
        let mut list = crate::DrawList::new(s.look().view);
        s.draw(&mut list, 1.0);
        let over = list.over.expect("menus over the view");
        assert!(over > 0 && over < list.items.len());
        s.screen = Screen::Playing;
        s.draw(&mut list, 1.0);
        assert_eq!(list.over, None, "nothing over the view in play");

        // Other games have no choice.
        let bundled = s.bundled.clone().unwrap();
        s.play_level(&bundled, None).unwrap();
        assert_eq!(s.rendering(), None);
        let items = options_items(&s.prefs, s.widescreen(), s.rendering());
        assert!(items.iter().all(|(_, s)| *s != Setting::Rendering));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn importing_again_offers_to_replace() {
        let (mut s, dir) = importing_session("again");
        import_the_game(&mut s);
        finish_import(&mut s);
        // Done: back to the title.
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Title(_)), "{:?}", s.screen);
        assert!(s.pack.is_none());

        import_the_game(&mut s);
        let importing = finish_import(&mut s);
        let result = importing.result.clone().unwrap();
        assert_eq!(
            result,
            Err(ImportError::AlreadyImported(dir.join("packs/title-9")))
        );
        assert_eq!(importing.menu.as_ref().unwrap().current(), After::Replace);
        s.step(&press(Action::Jump));
        let importing = finish_import(&mut s);
        assert!(importing.result.as_ref().unwrap().is_ok());
        s.step(&press(Action::Pause));

        // Choose another file, from the failure: back to the picker.
        import_the_game(&mut s);
        finish_import(&mut s);
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Picker(..)), "{:?}", s.screen);
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Title(_)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Where the system refuses to change the pack's folder, as Windows
    /// does while something holds it open, the player can retry once
    /// it's closed.
    #[cfg(unix)]
    #[test]
    fn a_blocked_import_can_be_retried() {
        use std::os::unix::fs::PermissionsExt;
        let (mut s, dir) = importing_session("blocked");
        import_the_game(&mut s);
        finish_import(&mut s);
        s.step(&press(Action::Pause));
        let top = dir.join("packs/title-9");
        let mode = |m| std::fs::set_permissions(&top, std::fs::Permissions::from_mode(m));
        mode(0o555).unwrap();

        import_the_game(&mut s);
        finish_import(&mut s);
        s.step(&press(Action::Jump));
        let importing = finish_import(&mut s);
        assert_eq!(importing.blocked_folder(), Some(top.as_path()));
        let menu = importing.menu.as_ref().unwrap();
        let labels: Vec<_> = menu.items.iter().map(|i| i.0.as_str()).collect();
        assert_eq!(
            labels,
            ["Retry", "Open folder", "Choose another file", "Done"]
        );
        let report = std::fs::read_to_string(dir.join("packs/import-report.txt")).unwrap();
        assert!(report.contains("replace the old copy"), "{report}");
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);

        mode(0o755).unwrap();
        s.step(&press(Action::Jump));
        let importing = finish_import(&mut s);
        let imported = importing.result.clone().unwrap().unwrap();
        assert_eq!(imported.dir, top.join("import-2"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_a_game_chosen_in_the_file_dialog() {
        let (mut s, dir) = importing_session("dialog");
        s.file_dialog = true;
        pick(&mut s, Item::Import);
        assert!(matches!(s.screen, Screen::Choosing(_)), "{:?}", s.screen);
        let games = s.file_request.take().expect("the dialog is asked for");
        assert!(games.ends_with("games"), "{games:?}");
        s.step(&idle());
        assert!(matches!(s.screen, Screen::Choosing(_)), "it waits");

        // A file somewhere else: that folder is remembered.
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::copy(games.join("game.img"), elsewhere.join("mine.img")).unwrap();
        s.file_chosen(Some(elsewhere.join("mine.img")), false);
        assert!(finish_import(&mut s).result.as_ref().unwrap().is_ok());
        assert_eq!(s.prefs.import_folder.as_ref(), Some(&elsewhere));

        // Done goes back to the title, past the dialog.
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)), "{:?}", s.screen);

        // Choose another file opens the dialog again.
        pick(&mut s, Item::Import);
        assert_eq!(s.file_request.take(), Some(elsewhere.clone()));
        s.file_chosen(Some(elsewhere.join("mine.img")), false);
        finish_import(&mut s);
        let Screen::Importing(importing, _) = &s.screen else {
            unreachable!()
        };
        assert!(matches!(
            importing.result,
            Some(Err(ImportError::AlreadyImported(_)))
        ));
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Choosing(_)), "{:?}", s.screen);
        assert_eq!(s.file_request.take(), Some(elsewhere));

        // Nothing chosen: back to the title.
        s.file_chosen(None, false);
        assert!(matches!(s.screen, Screen::Title(_)), "{:?}", s.screen);
        // What a dialog chooses after the player has gone on is ignored.
        s.file_chosen(Some(games.join("game.img")), false);
        assert!(matches!(s.screen, Screen::Title(_)), "{:?}", s.screen);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn browses_in_the_game_without_a_file_dialog() {
        let (mut s, dir) = importing_session("no-dialog");
        s.file_dialog = true;

        // Pause while the dialog is open.
        pick(&mut s, Item::Import);
        assert!(s.file_request.take().is_some());
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Picker(..)), "{:?}", s.screen);
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Title(_)), "{:?}", s.screen);

        // With a gamepad.
        s.handle(Event::PadButton {
            pad: clamber_input::PadId(0),
            input: clamber_input::PadInput::South,
            down: true,
        });
        assert_eq!(s.device, Device::Pad);
        pick(&mut s, Item::Import);
        assert!(matches!(s.screen, Screen::Picker(..)), "{:?}", s.screen);
        assert_eq!(s.file_request, None);
        s.step(&press(Action::Pause));

        // A dialog that doesn't open: the browser from then on.
        s.handle(Event::Key {
            key: clamber_input::Key::Enter,
            down: true,
        });
        assert_eq!(s.device, Device::Keyboard);
        pick(&mut s, Item::Import);
        assert!(s.file_request.take().is_some());
        s.file_chosen(None, true);
        assert!(matches!(s.screen, Screen::Picker(..)), "{:?}", s.screen);
        assert!(!s.file_dialog);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_a_dropped_file() {
        let (mut s, dir) = importing_session("dropped");
        let file = dir.join("games/game.img");
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing);
        s.dropped(file.clone());
        assert!(matches!(s.screen, Screen::Importing(..)), "{:?}", s.screen);
        // Not while it runs.
        s.dropped(file.clone());
        let importing = finish_import(&mut s);
        assert!(importing.result.as_ref().unwrap().is_ok());
        assert_eq!(s.prefs.import_folder, Some(dir.join("games")));

        // Over a finished import: it's done again, from where that was.
        s.dropped(file);
        let importing = finish_import(&mut s);
        assert!(matches!(
            importing.result,
            Some(Err(ImportError::AlreadyImported(_)))
        ));
        // Done: back to the game, paused.
        s.step(&press(Action::Up));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Paused(_)), "{:?}", s.screen);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_what_it_cant_import_and_says_why() {
        let (mut s, dir) = importing_session("refuses");
        // The picker lists the file, but the importer doesn't know it.
        pick(&mut s, Item::Import);
        s.importer = Arc::new(Importer::builtin());
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        let importing = finish_import(&mut s);
        let Some(Err(ImportError::NotAGame(_))) = &importing.result else {
            panic!("{:?}", importing.result);
        };
        let menu = importing.menu.as_ref().unwrap();
        assert_eq!(menu.current(), After::Another);
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        assert!(canvas.pixels.contains(&FAILED));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Picker(..)));
        assert!(!dir.join("packs").exists());

        // Still on the same file.
        s.packs_dir = None;
        s.step(&press(Action::Jump));
        let Screen::Importing(importing, _) = &s.screen else {
            panic!("{:?}", s.screen);
        };
        assert!(matches!(
            importing.result,
            Some(Err(ImportError::Destination(_)))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_games_screen_lists_imports_and_plays_them() {
        let (mut s, dir) = importing_session("games");
        pick(&mut s, Item::Games);
        let Screen::Games(games, _) = &s.screen else {
            panic!("{:?}", s.screen);
        };
        assert!(games.packs.is_empty());
        let labels: Vec<_> = games.menu.items.iter().map(|i| i.0.as_str()).collect();
        assert_eq!(labels, ["Clamber", "Back"]);
        s.step(&press(Action::Pause));
        assert!(matches!(s.screen, Screen::Title(_)));

        import_the_game(&mut s);
        finish_import(&mut s);
        s.step(&press(Action::Pause));
        // A pack with nothing to play is listed, but can't be picked.
        std::fs::create_dir_all(dir.join("packs/title-8")).unwrap();
        let manifest =
            std::fs::read_to_string(dir.join("packs/title-9/import-1/manifest.toml")).unwrap();
        let manifest = manifest.replace("title-9", "title-8");
        std::fs::write(dir.join("packs/title-8/manifest.toml"), manifest).unwrap();

        pick(&mut s, Item::Games);
        let Screen::Games(games, _) = &s.screen else {
            panic!("{:?}", s.screen);
        };
        let labels: Vec<_> = games.menu.items.iter().map(|i| i.0.as_str()).collect();
        assert_eq!(
            labels,
            ["Clamber", "Imported title-8", "Imported title-9", "Back"]
        );
        assert!(!games.menu.available(1));
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Games(..)), "can't play title-8");
        s.step(&press(Action::Down));
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)));
        assert_eq!(s.pack.as_ref().unwrap().manifest.id, "title-9");

        pick(&mut s, Item::Games);
        let Screen::Games(games, _) = &s.screen else {
            panic!("{:?}", s.screen);
        };
        assert_eq!(games.menu.current(), Pick::Pack(1), "the one playing");
        s.step(&press(Action::Up));
        s.step(&press(Action::Up));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)));
        assert!(s.pack.is_none());
        assert_eq!(s.level_path, dir.join("bundled.level"));
        assert_eq!(s.game.world.level.name, "Test level");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A catalog that knows one image, `[1; 2048]`.
    fn catalog() -> Arc<clamber_import::Catalog> {
        let image = clamber_import::Image::new(vec![1; 2048]);
        Arc::new(clamber_import::Catalog {
            known: vec![clamber_import::Known {
                title: "title-9".into(),
                release: "made-up release".into(),
                size: image.len() as u64,
                sha256: image.sha256(),
            }],
        })
    }

    #[test]
    fn the_picker_browses_folders() {
        let dir = scratch("picker");
        std::fs::create_dir_all(dir.join("b-folder/inner")).unwrap();
        std::fs::create_dir_all(dir.join("A-folder")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        // The size of the image with a copier header, but not the image.
        std::fs::write(dir.join("z.img"), [2; 2048 + 512]).unwrap();
        std::fs::write(dir.join("c.img"), [1; 2048]).unwrap();
        // Not the size of an image the catalog knows.
        std::fs::write(dir.join("notes.txt"), "notes").unwrap();
        std::fs::write(dir.join("big.img"), [1; 4096]).unwrap();
        let mut picker = Picker::open(&dir, catalog());
        let labels =
            |p: &Picker| -> Vec<String> { p.menu.items.iter().map(|i| i.0.clone()).collect() };
        assert_eq!(
            labels(&picker),
            ["../", "A-folder/", "b-folder/", "c.img", "z.img"]
        );
        // The image is marked once it has been read.
        for _ in 0..500 {
            if picker.mark() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(picker.mark());
        assert_eq!(
            labels(&picker),
            ["../", "A-folder/", "b-folder/", "★ c.img", "z.img"]
        );
        let down = Frame::after(Actions::NONE, Action::Down.into());
        let jump = Frame::after(Actions::NONE, Action::Jump.into());
        let left = Frame::after(Actions::NONE, Action::Left.into());
        picker.update(&down);
        picker.update(&down);
        assert_eq!(picker.update(&jump), None);
        assert_eq!(picker.dir, dir.join("b-folder"));
        assert_eq!(labels(&picker), ["../", "inner/"]);
        picker.update(&left);
        assert_eq!(picker.dir, dir);
        assert_eq!(picker.menu.current(), 2, "the folder it came from");
        picker.update(&down);
        assert_eq!(picker.update(&jump), Some(Picked::File(dir.join("c.img"))));

        let mut gone = Picker::open(&dir.join("not-there"), catalog());
        assert!(gone.problem.is_some());
        assert_eq!(labels(&gone), ["../"]);
        assert_eq!(gone.update(&jump), None);
        assert_eq!(gone.dir, dir);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_picker_takes_a_typed_path() {
        let dir = scratch("picker-typed");
        std::fs::create_dir_all(dir.join("my games/inner")).unwrap();
        std::fs::write(dir.join("my games/c.img"), [1; 2048]).unwrap();
        let mut picker = Picker::open(&dir, catalog());
        let insert = |t: &str| Edit::Insert(t.into());

        // A space alone doesn't start a path, nor does Backspace.
        picker.edit(insert(" "));
        picker.edit(Edit::Backspace);
        assert_eq!(picker.typed, None);

        // Relative to the folder shown; the menu waits meanwhile.
        picker.edit(insert("my"));
        picker.edit(insert(" gamez"));
        picker.edit(Edit::Backspace);
        assert_eq!(picker.typed.as_deref(), Some("my game"));
        let down = Frame::after(Actions::NONE, Action::Down.into());
        let pause = Frame::after(Actions::NONE, Action::Pause.into());
        assert_eq!(picker.update(&pause), None);
        picker.update(&down);
        assert_eq!(picker.menu.current(), 0);
        assert_eq!(picker.edit(Edit::Enter), None);
        assert!(picker.nothing_there);
        assert_eq!(picker.typed.as_deref(), Some("my game"), "kept to fix");
        picker.edit(insert("s"));
        assert!(!picker.nothing_there);
        assert_eq!(picker.edit(Edit::Enter), None);
        assert_eq!(picker.dir, dir.join("my games"));
        assert_eq!(picker.typed, None);

        // Pasted, in quotes, over more than one line.
        let file = dir.join("my games/c.img");
        let pasted = format!("\"{}\"\nmore", file.display());
        assert_eq!(picker.edit(insert(&pasted)), None);
        assert_eq!(picker.edit(Edit::Enter), Some(Picked::File(file)));
        assert_eq!(picker.typed, None);

        // Escape stops typing; then the menu works again.
        picker.edit(insert("inn"));
        picker.edit(Edit::Cancel);
        assert_eq!(picker.typed, None);
        assert_eq!(picker.edit(Edit::Enter), None);
        picker.update(&down);
        assert_eq!(picker.menu.current(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_a_typed_path() {
        let (mut s, dir) = importing_session("typed");
        assert!(!s.takes_text());
        pick(&mut s, Item::Import);
        assert!(matches!(s.screen, Screen::Picker(..)), "{:?}", s.screen);
        assert!(s.takes_text());
        let file = dir.join("games/game.img");
        s.edit(Edit::Insert(file.display().to_string()));
        assert!(s.typing());
        assert!(s.hints().is_empty());
        s.edit(Edit::Enter);
        let importing = finish_import(&mut s);
        assert!(importing.result.as_ref().unwrap().is_ok());
        assert_eq!(s.prefs.import_folder, Some(dir.join("games")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A level with an exit near its start that leads into `room`, and
    /// the room, whose end marker leads back to the level's `back`.
    fn areas(name: &str) -> PathBuf {
        let dir = scratch(name);
        let level = r#"
            format = 1
            name = "level"
            size = [1000, 400]
            start = [200, 300]

            [[collision]]
            shape = "rect"
            rect = [0, 300, 1000, 100]

            [[entities]]
            type = "exit"
            at = [100, 300]
            params = { area = "room", arrival = "in" }

            [[arrivals]]
            name = "back"
            at = [600, 300]
        "#;
        let room = r#"
            format = 1
            name = "room"
            size = [256, 400]
            start = [200, 300]

            [[collision]]
            shape = "rect"
            rect = [0, 300, 256, 100]

            [[entities]]
            type = "end"
            at = [210, 300]
            params = { area = "level", arrival = "back" }

            [[arrivals]]
            name = "in"
            at = [200, 300]
            walk = "left"
        "#;
        std::fs::write(dir.join("level.level"), level).unwrap();
        std::fs::write(dir.join("room.level"), room).unwrap();
        dir
    }

    /// Steps holding `keys` until play is in the level at `path`, at
    /// most `most` frames.
    fn until_in(s: &mut Session, keys: Actions, path: &Path, most: usize) {
        for _ in 0..most {
            s.step(&input(keys, keys));
            if s.level_path == path {
                return;
            }
        }
        panic!("not in {} after {most} frames", path.display());
    }

    #[test]
    fn an_exit_leads_into_another_area_and_its_end_marker_back() {
        let dir = areas("areas");
        let (level, room) = (dir.join("level.level"), dir.join("room.level"));
        let mut s = session(LEVEL);
        s.game.world.feel = Arc::new(generic_feel("classic-1").unwrap());
        s.play_level(&level, None).unwrap();
        s.screen = Screen::Playing;
        s.game.world.flow.lives = 7;
        until_in(&mut s, Action::Left.into(), &room, 300);
        assert_eq!(s.game.world.level.name, "room");
        assert_eq!(s.game.world.frame, 0);
        assert!(s.game.world.walking_in.is_some());
        assert_eq!(s.game.world.flow.lives, 7, "carried into the room");
        assert_eq!(s.left.len(), 1);
        s.game.world.flow.lives = 9;
        until_in(&mut s, Action::Right.into(), &level, 300);
        assert_eq!(s.game.world.level.name, "level");
        assert_eq!(s.game.world.frame, 0);
        assert_eq!(s.game.world.player.pos, clamber_level::Vec2::ints(600, 300));
        assert_eq!(s.game.world.flow.lives, 9, "carried back");
        assert!(s.left.is_empty());
        assert_eq!(s.screen, Screen::Playing);
    }

    #[test]
    fn restarting_in_another_area_begins_again_in_the_first() {
        let dir = areas("restart-areas");
        let (level, room) = (dir.join("level.level"), dir.join("room.level"));
        let mut s = session(LEVEL);
        s.game.world.feel = Arc::new(generic_feel("classic-1").unwrap());
        s.play_level(&level, None).unwrap();
        s.screen = Screen::Playing;
        until_in(&mut s, Action::Left.into(), &room, 300);
        s.first_area();
        s.game.restart();
        assert_eq!(s.level_path, level);
        assert_eq!(s.game.world.level.name, "level");
        assert!(s.left.is_empty());
    }

    #[test]
    fn an_area_that_isnt_there_leads_where_the_level_has_it() {
        let dir = areas("missing-area");
        let level = dir.join("level.level");
        std::fs::remove_file(dir.join("room.level")).unwrap();
        let mut s = session(LEVEL);
        s.game.world.feel = Arc::new(generic_feel("classic-1").unwrap());
        s.play_level(&level, None).unwrap();
        s.screen = Screen::Playing;
        for _ in 0..300 {
            s.step(&input(Action::Left.into(), Action::Left.into()));
        }
        assert_eq!(s.level_path, level);
        assert!(s.left.is_empty());
        assert!(!s.game.world.flow.outcome.is_over());
    }

    #[test]
    fn a_scene_goes_back_where_it_leads_once_it_s_over() {
        let dir = areas("scene");
        let (level, scene) = (dir.join("level.level"), dir.join("room.level"));
        let room = std::fs::read_to_string(&scene).unwrap();
        let played = room.replace(
            "[[arrivals]]",
            "[scene]\nlast = 60\nfade = 40\nfade_step = 2\narea = \"level\"\narrival = \"back\"\n\n[[arrivals]]",
        );
        std::fs::write(&scene, played).unwrap();
        let mut s = session(LEVEL);
        s.game.world.feel = Arc::new(generic_feel("classic-1").unwrap());
        s.play_level(&level, None).unwrap();
        s.screen = Screen::Playing;
        until_in(&mut s, Action::Left.into(), &scene, 300);
        // Right would pass the end marker, which leads back too, if the
        // scene let it.
        let right = input(Action::Right.into(), Action::Right.into());
        for _ in 0..60 {
            s.step(&right);
        }
        assert_eq!(s.level_path, scene, "still in the scene");
        assert_eq!(s.game.world.brightness(), (1, 10));
        s.step(&right);
        assert_eq!(s.level_path, level);
        assert_eq!(s.game.world.frame, 0);
        assert_eq!(s.game.world.player.pos, clamber_level::Vec2::ints(600, 300));
        assert!(s.left.is_empty());
        assert_eq!(s.screen, Screen::Playing);
    }

    /// A pack folder with [`LEVEL`] as `levels/level-1.level` and a level
    /// map: a new game walks from `s` to the warp `k`, which leads to the
    /// level node `f`; finishing `f` opens the path down to `n`.
    fn map_pack(name: &str) -> PathBuf {
        let dir = scratch(name);
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("levels/level-1.level"), LEVEL).unwrap();
        let map = r#"
            format = 1
            id = "level-map"
            size = [512, 256]
            mode = "legs"
            start = "s"

            [legs]
            leg_frames = 64
            remember = 31
            page_width = 256
            page_wait = 64
            appear_offset = [8, 0]
            appear_wait = 64
            y_up = true
            fade_step = 2

            [legs.finish.first]
            black = 49
            still = [163, 50]
            gate = 242

            [legs.finish.again]
            black = 52
            gate = 64

            [legs.new_game]
            walk = 64
            toward = "right"

            [[nodes]]
            id = "s"
            kind = "waypoint"
            at = [100, 40]

            [[nodes]]
            id = "k"
            kind = "warp"
            map = "level-map"
            node = "f"
            at = [172, 40]

            [[nodes]]
            id = "f"
            kind = "level"
            level = "levels/level-1.level"
            exits = ["normal"]
            at = [60, 200]
            onward = "down"

            [[nodes]]
            id = "n"
            kind = "waypoint"
            at = [60, 150]

            [[paths]]
            from = "s"
            to = "k"
            leave = ["right"]
            back = ["left"]
            back_unlock = { flag = "never" }

            [[paths]]
            from = "f"
            to = "n"
            leave = ["down"]
            back = ["up"]
            unlock = { cleared = "f" }
        "#;
        std::fs::write(dir.join(LEVEL_MAP), map).unwrap();
        dir
    }

    /// Runs `frames` frames with `held(frame)` held, from `before` held,
    /// returning what was held last.
    fn hold(
        s: &mut Session,
        before: Actions,
        frames: u32,
        held: impl Fn(u32) -> Actions,
    ) -> Actions {
        let mut before = before;
        for f in 0..frames {
            let now = held(f);
            s.step(&input(before, now));
            before = now;
        }
        before
    }

    fn map_of(s: &Session) -> &MapPlay {
        s.map.as_ref().expect("a level map")
    }

    /// The node the lead stands at or last stood at, and whether the map
    /// shows.
    fn lead(s: &Session) -> (String, bool) {
        let map = map_of(s);
        match &map.phase {
            MapPhase::Shown(screen) => (map.map.nodes[screen.lead.node].id.clone(), true),
            _ => (String::new(), false),
        }
    }

    /// Plays the level until it's over, holding Right.
    fn finish_level(s: &mut Session) {
        let right = Actions::from(Action::Right);
        let mut before = Actions::NONE;
        while s.screen == Screen::Playing {
            s.step(&input(before, right));
            before = right;
            assert!(s.game.world.frame < 600, "never reached the goal");
        }
    }

    /// spec/behavior/level-map.md, "A new game", its example: the map
    /// before the first level, which begins on the frame after the black
    /// ones.
    #[test]
    fn a_new_game_begins_on_the_level_map() {
        let dir = map_pack("map-new-game");
        let mut s = session(LEVEL);
        s.map = MapPlay::load(&dir).unwrap();
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Map, "Start shows the map");
        assert_eq!(s.tune(), None);
        let jump = Actions::from(Action::Jump);
        let down = Actions::from(Action::Down);
        let at = |f: u32| match f {
            100 | 140 | 230 | 240 => jump,
            180..=200 => down,
            _ => Actions::NONE,
        };
        let mut before = Actions::NONE;
        let mut canvas = Game::canvas();
        s.draw(&mut canvas, 1.0);
        assert!(
            canvas.pixels.iter().all(|&p| p == 0),
            "black before frame 0"
        );
        for f in 0..272 {
            before = hold(&mut s, before, 1, |_| at(f));
            let (node, shown) = lead(&s);
            match f {
                126 => assert_eq!(node, "s", "still walking on {f}"),
                127 | 171 => assert_eq!(node, "k", "at the warp on {f}"),
                172 | 271 => assert_eq!(node, "f", "at the level on {f}"),
                _ => {}
            }
            assert!(shown, "the map on {f}");
            assert_eq!(s.screen, Screen::Map, "frame {f}");
        }
        let mut lit = Game::canvas();
        s.step(&idle());
        assert_eq!(s.screen, Screen::Playing, "the level begins on P + 32");
        assert_eq!(s.level_path, dir.join("levels/level-1.level"));
        assert_eq!(s.game.world.frame, 0);
        assert!(map_of(&s).playing());

        // the map at full brightness
        let map = s.map.as_mut().unwrap();
        map.new_game(false);
        for _ in 0..40 {
            map.step(Actions::NONE);
        }
        map.draw(&mut lit, s.game.map_cast());
        assert!(lit.pixels.iter().any(|&p| p != 0), "a stand-in picture");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// spec/behavior/level-map.md, "Pictures": the lead's picture changes
    /// every 4 counted frames through 10, it faces the way its `x` last
    /// went, and none is drawn on the frames around a page change.
    #[test]
    fn the_lead_on_the_map_cycles_faces_and_hides_at_a_page_change() {
        let dir = map_pack("map-pictures");
        let path = dir.join(LEVEL_MAP);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("page_width = 256", "page_width = 64")).unwrap();
        let mut map = MapPlay::load(&dir).unwrap().unwrap();
        let mut pictures = Vec::new();
        for _ in 0..140 {
            map.step(Actions::NONE);
            pictures.push(map.lead_picture());
        }
        assert_eq!(pictures[0], Some((0, false)));
        assert_eq!(pictures[3], Some((0, false)));
        assert_eq!(pictures[4], Some((1, false)));
        let hidden: Vec<usize> = (0..140).filter(|&f| pictures[f].is_none()).collect();
        assert!(!hidden.is_empty(), "a page changes in the walk");
        let MapPhase::Shown(screen) = &map.phase else {
            panic!("shown");
        };
        assert!(screen.lead.frame() > 100);
        // frames P - 1 to P + 1 and the lag frame, which isn't counted
        assert_eq!(hidden.len(), 4, "{hidden:?}");
        assert_eq!(hidden[3] - hidden[0], 3);
        // the cycle goes on through them: the counted frame decides
        let after = hidden[3] + 1;
        let counted = after - 1; // one lag frame before it
        assert_eq!(pictures[after].unwrap().0, counted / 4 % 10);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// spec/behavior/level-map.md, "The other character": a two-character
    /// game begins the map with it 16 px left of the lead; one character
    /// has none.
    #[test]
    fn a_new_two_character_game_has_the_other_beside_the_lead() {
        let dir = map_pack("map-other");
        let mut map = MapPlay::load(&dir).unwrap().unwrap();
        let positions = |map: &MapPlay| {
            let MapPhase::Shown(screen) = &map.phase else {
                panic!("shown");
            };
            (screen.lead.position(), screen.lead.other_position())
        };
        map.new_game(false);
        assert!(positions(&map).1.is_none());
        map.new_game(true);
        let (lead, other) = positions(&map);
        assert_eq!(
            other.expect("the other character").x.floor(),
            lead.x.floor() - 16
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The map's first frame on the step `black` after the level's last.
    fn until_shown(s: &mut Session) -> u32 {
        let mut steps = 0;
        while !lead(s).1 {
            s.step(&idle());
            steps += 1;
            assert!(steps < 100, "the map never appears");
        }
        steps
    }

    /// spec/behavior/level-map.md, "After finishing a level": the map
    /// after the black frames, the level recorded and Down given at the
    /// gate; a press at a node that isn't a level does nothing; and
    /// entering the level again and finishing it, the shorter times.
    #[test]
    fn finishing_a_level_goes_back_to_the_map() {
        let dir = map_pack("map-finish");
        let mut s = session(LEVEL);
        s.map = MapPlay::load(&dir).unwrap();
        s.step(&press(Action::Jump));
        let jump = Actions::from(Action::Jump);
        hold(&mut s, Actions::NONE, 273, |f| match f {
            140 | 240 => jump,
            _ => Actions::NONE,
        });
        assert_eq!(s.screen, Screen::Playing);

        finish_level(&mut s);
        assert_eq!(s.screen, Screen::Map, "no complete screen");
        assert!(map_of(&s).progress.cleared_by("level-map:f", "normal"));
        assert_eq!(until_shown(&mut s), 49);
        // frame 0 was the last step; the walk down starts on the gate
        hold(&mut s, Actions::NONE, 241, |_| Actions::NONE);
        let walking = |s: &Session| match &map_of(s).phase {
            MapPhase::Shown(screen) => screen.lead.walking(),
            _ => false,
        };
        assert!(!walking(&s), "not before the gate");
        s.step(&idle());
        assert!(walking(&s), "Down given on the gate");
        hold(&mut s, Actions::NONE, 80, |_| Actions::NONE);
        assert_eq!(lead(&s).0, "n");

        // a waypoint can't be entered
        let before = hold(&mut s, Actions::NONE, 1, |_| jump);
        hold(&mut s, before, 60, |_| Actions::NONE);
        assert_eq!(s.screen, Screen::Map);
        assert_eq!(lead(&s), ("n".to_string(), true));

        // back up to the level, into it and out again
        let up = Actions::from(Action::Up);
        let before = hold(&mut s, Actions::NONE, 1, |_| up);
        let before = hold(&mut s, before, 80, |_| Actions::NONE);
        assert_eq!(lead(&s).0, "f");
        hold(&mut s, before, 1, |_| jump);
        hold(&mut s, jump, 32, |_| Actions::NONE);
        assert_eq!(s.screen, Screen::Playing);
        finish_level(&mut s);
        assert_eq!(until_shown(&mut s), 52, "finished before");
        hold(&mut s, Actions::NONE, 63, |_| Actions::NONE);
        assert!(!walking(&s));
        s.step(&idle());
        assert!(walking(&s), "the gate is 64 when finished before");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// spec/behavior/following.md, "When a level begins again", first
    /// row: finishing a level with both characters, the next one begins
    /// with both, the same one leading.
    #[test]
    fn the_next_level_from_the_map_begins_with_both_characters() {
        let dir = map_pack("map-both");
        let mut s = session(LEVEL);
        s.map = MapPlay::load(&dir).unwrap();
        s.step(&press(Action::Jump));
        let jump = Actions::from(Action::Jump);
        hold(&mut s, Actions::NONE, 273, |f| match f {
            140 | 240 => jump,
            _ => Actions::NONE,
        });
        assert_eq!(s.screen, Screen::Playing);
        // Both characters, the second leading.
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(feel.roster[0].clone());
        let mut world = World::new(s.game.world.level.clone(), Arc::new(feel));
        world.player.character = 1;
        world.other.as_mut().expect("both start").character = 0;
        s.game.replace_world(world);

        finish_level(&mut s);
        until_shown(&mut s);
        hold(&mut s, Actions::NONE, 242, |_| Actions::NONE);
        hold(&mut s, Actions::NONE, 80, |_| Actions::NONE);
        let up = Actions::from(Action::Up);
        let before = hold(&mut s, Actions::NONE, 1, |_| up);
        let before = hold(&mut s, before, 80, |_| Actions::NONE);
        hold(&mut s, before, 1, |_| jump);
        hold(&mut s, jump, 32, |_| Actions::NONE);
        assert_eq!(s.screen, Screen::Playing);
        assert_eq!(s.game.world.frame, 0, "a new level");
        assert_eq!(s.game.world.player.character, 1);
        assert_eq!(s.game.world.other.as_ref().map(|o| o.character), Some(0));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_new_game_without_a_level_map_plays_the_level() {
        let mut s = session(LEVEL);
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing);
        assert!(s.map.is_none());
    }

    #[test]
    fn formats_level_time() {
        assert_eq!(clock_time(0), "0:00.00");
        assert_eq!(clock_time(90), "0:01.50");
        assert_eq!(clock_time(60 * 75 + 59), "1:15.98");
    }
}
