//! The screens around a level: title, pause, controls, options,
//! credits, importing a game, the games to play, level complete and game
//! over, and moving between them and play.
//!
//! Menus read the same neutral actions as play, from every player at
//! once, so a keyboard or a gamepad alone can drive them (see
//! [`Menu`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clamber_import::{Importer, Installed};

use clamber_input::{Action, Event, Frame, Input, Settings};
use clamber_sim::{InputFrame, Outcome};

use crate::camera::VIEW;
use crate::canvas::{Color, rgb};
use crate::controls::{Controls, Request};
use crate::credits::Credits;
use crate::display::Look;
use crate::feels::Feels;
use crate::game::Game;
use crate::hud::{COUNT, GEM, RELIC_ICON, relic_total};
use crate::import::{After, Games, Importing, PERSONAL, Pick, Picked, Picker};
use crate::menu::{Choice, Menu};
use crate::prefs::{Prefs, Volume};
use crate::report;
use crate::sound::{Cue, cues};
use crate::surface::Surface;
use crate::text::{ADVANCE, LINE, draw_text, draw_text_centred, text_size};
use crate::{SPRITES, content_dir, load_game};

/// Where the credits screen's lines start, and how many show at once.
const CREDITS_TOP: i32 = 34;
const CREDITS_ROWS: usize = 18;
/// Characters across a line of credits.
const CREDITS_WIDTH: usize = 60;

pub(crate) const TITLE: Color = rgb(0xf0, 0xc8, 0x40);
pub(crate) const HEADING: Color = rgb(0xf4, 0xf0, 0xe8);
pub(crate) const SUBTITLE: Color = rgb(0xa8, 0xb0, 0xbc);
pub(crate) const FAILED: Color = rgb(0xe8, 0x50, 0x60);
/// Laid over the level behind a menu.
const DIM: Color = rgb(0x08, 0x0a, 0x10);

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
    /// Choosing a game image to import, and the screen to go back to.
    Picker(Picker, Box<Screen>),
    /// An import, running or finished, and the picker it was started
    /// from.
    Importing(Importing, Box<Screen>),
    /// Choosing a game to play, and the screen to go back to.
    Games(Games, Box<Screen>),
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
    /// Turned down with Left and up with Right or Jump, which wraps
    /// around to silent.
    MusicVolume,
    EffectsVolume,
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
            Screen::Complete(_) | Screen::GameOver(_) => None,
            Screen::Controls(_, from)
            | Screen::Movement(_, from)
            | Screen::Options(_, from)
            | Screen::Credits(_, from)
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
    /// The level the session started with, to go back to from an
    /// imported one, if there's one to go back to.
    pub bundled: Option<PathBuf>,
    /// The personal import being played, if one is.
    pub pack: Option<Installed>,
    /// Where imported packs go, if anywhere.
    pub packs_dir: Option<PathBuf>,
    pub importer: Arc<Importer>,
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
            bundled: None,
            pack: None,
            packs_dir: None,
            importer: Arc::new(Importer::builtin()),
        }
    }

    /// Records a device event. A press that finishes a remap shows on
    /// the controls screen.
    pub fn handle(&mut self, event: Event) {
        let remapped = self.input.handle(event);
        if let (Some(remapped), Screen::Controls(controls, _)) = (remapped, &mut self.screen) {
            controls.remapped(remapped);
        }
    }

    /// Ends a logic frame of device input and steps with it.
    pub fn tick(&mut self) {
        let players = self.input.tick();
        self.step(&InputFrame { players });
    }

    /// Advances one logic frame: plays the level, or works the menu
    /// that's showing.
    pub fn step(&mut self, input: &InputFrame) {
        self.cues.clear();
        if self.screen == Screen::Playing {
            self.play(input);
            return;
        }
        let frame = everyone(input);
        match self.screen {
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
                    menu.items = options_items(&self.prefs);
                    self.save_prefs("the volume");
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
                // changed above
                Setting::MusicVolume | Setting::EffectsVolume => return,
                Setting::Back => unreachable!(),
            }
            menu.items = options_items(&self.prefs);
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
            Item::Start | Item::Restart => {
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
                let menu = Menu::new(options_items(&self.prefs));
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
                let dir = Picker::start_dir(self.prefs.import_folder.as_deref());
                let from = std::mem::replace(&mut self.screen, Screen::Playing);
                self.screen = Screen::Picker(Picker::open(&dir), Box::new(from));
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
            | Screen::Picker(_, from)
            | Screen::Importing(_, from)
            | Screen::Games(_, from) => from,
            other => Box::new(other),
        };
        self.screen = *from;
    }

    fn step_picker(&mut self, frame: &Frame) {
        let Screen::Picker(picker, _) = &mut self.screen else {
            return;
        };
        match picker.update(frame) {
            None => {}
            Some(Picked::Back) => self.back(),
            Some(Picked::File(file)) => {
                if self.prefs.import_folder.as_ref() != Some(&picker.dir) {
                    self.prefs.import_folder = Some(picker.dir.clone());
                    self.save_prefs("the folder to import from");
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
        self.screen = Screen::Importing(importing, picker);
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
            After::Replace => self.start_import(file, true),
            After::Another => self.back(),
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

    /// Plays the level at `path` with the movement being played, and
    /// `pack`'s sprites if it has them. Keeps the debug view setting,
    /// and turns it on for a level with nothing else to see.
    fn play_level(&mut self, path: &Path, pack: Option<Installed>) -> Result<(), String> {
        let sprites = pack
            .as_ref()
            .and_then(Installed::sprites)
            .unwrap_or_else(|| content_dir().join(SPRITES));
        let look = pack
            .as_ref()
            .map_or(Look::RENDERED, |p| Look::of(&p.manifest));
        let mut game = load_game(path, &sprites, self.game.world.feel.clone())?.with_look(look);
        game.debug |= self.game.debug;
        self.game = game;
        self.level_path = path.to_path_buf();
        self.pack = pack;
        Ok(())
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
        self.game.set_feel(feel);
        self.prefs.preset = Some(id.to_string());
        self.save_prefs("the movement choice");
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
            Outcome::Complete => self.screen = Screen::complete(),
            Outcome::GameOver => self.screen = Screen::game_over(),
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
            Screen::Playing | Screen::Paused(_) | Screen::Complete(_) | Screen::GameOver(_) => {
                self.game.look
            }
            _ => Look::RENDERED,
        }
    }

    /// Draws the game and the screen over it into `canvas`, whose view
    /// should be [`look`](Session::look)'s.
    pub fn draw(&self, canvas: &mut dyn Surface, alpha: f64) {
        let playing = self.screen == Screen::Playing;
        self.game.draw(canvas, if playing { alpha } else { 1.0 });
        if playing {
            return;
        }
        canvas.shade(DIM, 160);

        let centre = canvas.view().0 as i32 / 2;
        let (menu, top) = match &self.screen {
            Screen::Playing => return,
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
                menu.draw(canvas, centre, 80);
                return;
            }
            Screen::Credits(scroll, _) => {
                self.draw_credits(canvas, *scroll, centre);
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
                draw_text_centred(canvas, centre, 40, "CLAMBER", TITLE);
                let name = &self.game.world.level.name;
                if !name.is_empty() {
                    draw_text_centred(canvas, centre, 40 + 2 * LINE, name, SUBTITLE);
                }
                if self.pack.is_some() {
                    draw_text_centred(canvas, centre, 40 + 3 * LINE, PERSONAL, FAILED);
                }
                (menu, 40 + 5 * LINE)
            }
            Screen::Paused(menu) => {
                draw_text_centred(canvas, centre, 70, "PAUSED", HEADING);
                (menu, 100)
            }
            Screen::Complete(menu) => {
                draw_text_centred(canvas, centre, 50, "LEVEL COMPLETE", HEADING);
                self.draw_results(canvas, centre, 50 + 3 * LINE);
                (menu, 130)
            }
            Screen::GameOver(menu) => {
                draw_text_centred(canvas, centre, 70, "GAME OVER", FAILED);
                (menu, 100)
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
        let top = 30 + 4 * LINE;
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

    /// The credits from line `scroll`, and a hint when there are more.
    fn draw_credits(&self, canvas: &mut dyn Surface, scroll: usize, centre: i32) {
        draw_text_centred(canvas, centre, 14, "CREDITS", HEADING);
        let lines = self.credits.wrapped(CREDITS_WIDTH);
        let left = centre - (CREDITS_WIDTH as i32 * ADVANCE) / 2;
        for (row, line) in lines.iter().skip(scroll).take(CREDITS_ROWS).enumerate() {
            let color = if line.heading { TITLE } else { HEADING };
            let y = CREDITS_TOP + row as i32 * LINE;
            draw_text(canvas, left, y, &line.text, color);
        }
        let more_above = scroll > 0;
        let more_below = lines.len() > scroll + CREDITS_ROWS;
        let hint = match (more_above, more_below) {
            (false, false) => "Jump: back",
            _ => "Up/Down: scroll   Jump: back",
        };
        draw_text_centred(canvas, centre, VIEW.1 as i32 - 14, hint, SUBTITLE);
    }

    /// The level's time and what was collected, centred on `centre_x`.
    fn draw_results(&self, canvas: &mut dyn Surface, centre_x: i32, top: i32) {
        let world = &self.game.world;
        let time = format!("TIME {}", clock_time(world.frame));
        draw_text_centred(canvas, centre_x, top, &time, HEADING);

        let mut items = vec![('◆', GEM, world.flow.gems.to_string())];
        let relics = relic_total(world);
        if relics > 0 {
            items.push(('★', RELIC_ICON, format!("{}/{relics}", world.flow.relics)));
        }
        let gap = 2 * ADVANCE;
        let width: i32 = items
            .iter()
            .map(|(_, _, count)| ADVANCE + 1 + text_size(count).0 + gap)
            .sum::<i32>()
            - gap;
        let mut x = centre_x - width / 2;
        let y = top + 2 * LINE;
        for (icon, color, count) in items {
            draw_text(canvas, x, y, &icon.to_string(), color);
            x += ADVANCE + 1;
            draw_text(canvas, x, y, &count, COUNT);
            x += text_size(&count).0 + gap;
        }
    }
}

/// The options screen's items, showing the settings in `prefs`.
fn options_items(prefs: &Prefs) -> Vec<(String, Setting)> {
    let on = |on| if on { "On" } else { "Off" };
    let (display, volume) = (&prefs.display, &prefs.volume);
    vec![
        (
            format!("Fullscreen: {}", on(display.fullscreen)),
            Setting::Fullscreen,
        ),
        (format!("Vsync: {}", on(display.vsync)), Setting::Vsync),
        (
            format!("Music: < {}% >", volume.music),
            Setting::MusicVolume,
        ),
        (
            format!("Effects: < {}% >", volume.effects),
            Setting::EffectsVolume,
        ),
        ("Back".into(), Setting::Back),
    ]
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
        let dir = std::env::temp_dir().join(format!("clamber-prefs-{}", std::process::id()));
        let path = dir.join("prefs.toml");
        let mut s = session(LEVEL);
        s.prefs_path = Some(path.clone());
        open_movement(&mut s);
        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)));
        assert_eq!(*s.game.world.feel, generic_feel("snappy").unwrap());
        assert_eq!(s.prefs.preset.as_deref(), Some("snappy"));
        assert_eq!(Prefs::load(&path), s.prefs);
        std::fs::remove_dir_all(dir).unwrap();

        // Opening it again starts on the new preset.
        s.step(&press(Action::Jump));
        assert_eq!(movement_menu(&s).current(), 1);
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
                "Vsync: On",
                "Music: < 100% >",
                "Effects: < 100% >",
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
        assert_eq!(labels(&s)[..2], ["Fullscreen: On", "Vsync: Off"]);
        assert_eq!(options(&s).current(), Setting::Vsync, "stays put");
        assert_eq!(Prefs::load(&path), s.prefs);

        // Left turns a volume down, Right up to full, and Jump up,
        // wrapping around to silent.
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
        assert_eq!(labels(&s)[2..4], ["Music: < 0% >", "Effects: < 90% >"]);
        assert_eq!(options(&s).current(), Setting::EffectsVolume);
        assert_eq!(Prefs::load(&path), s.prefs);
        std::fs::remove_dir_all(dir).unwrap();

        s.step(&press(Action::Down));
        s.step(&press(Action::Jump));
        let Screen::Title(menu) = &s.screen else {
            panic!("back to the title")
        };
        assert_eq!(menu.current(), Item::Options);
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
        assert!(canvas.pixels.contains(&GEM));

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
        assert!(title.pixels.contains(&TITLE));
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
        let image: Vec<u8> = (0..=255u8).cycle().take(500).collect();
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
        assert_eq!(imported.dir, dir.join("packs/title-9"));
        assert!(!imported.manifest.redistribute);
        assert_eq!(importing.menu.as_ref().unwrap().current(), After::Play);
        let mut done = Game::canvas();
        s.draw(&mut done, 1.0);
        assert_ne!(canvas, done, "shows what came of it");

        s.step(&press(Action::Jump));
        let Screen::Title(_) = &s.screen else {
            panic!("plays it from the title: {:?}", s.screen);
        };
        assert_eq!(s.game.world.level.name, "Imported level");
        assert_eq!(s.level_path, dir.join("packs/title-9/levels/first.level"));
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
        let manifest = dir.join("packs/title-9/manifest.toml");
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(text.contains("pixel_art = true"), "{text}");
        std::fs::write(&manifest, format!("{text}view = [256, 224]\n")).unwrap();
        let pack = Installed::load(&dir.join("packs/title-9")).unwrap();
        s.play_pack(pack).unwrap();
        let look = Look {
            view: (256, 224),
            pixel_art: true,
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

    #[test]
    fn refuses_what_it_cant_import_and_says_why() {
        let (mut s, dir) = importing_session("refuses");
        s.importer = Arc::new(Importer::builtin());
        import_the_game(&mut s);
        let importing = finish_import(&mut s);
        let Some(Err(ImportError::Unsupported(_))) = &importing.result else {
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
        let manifest = std::fs::read_to_string(dir.join("packs/title-9/manifest.toml")).unwrap();
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

    #[test]
    fn the_picker_browses_folders() {
        let dir = scratch("picker");
        std::fs::create_dir_all(dir.join("b-folder/inner")).unwrap();
        std::fs::create_dir_all(dir.join("A-folder")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::write(dir.join("z.img"), "").unwrap();
        std::fs::write(dir.join("c.img"), "").unwrap();
        let mut picker = Picker::open(&dir);
        let labels =
            |p: &Picker| -> Vec<String> { p.menu.items.iter().map(|i| i.0.clone()).collect() };
        assert_eq!(
            labels(&picker),
            ["../", "A-folder/", "b-folder/", "c.img", "z.img"]
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

        let mut gone = Picker::open(&dir.join("not-there"));
        assert!(gone.problem.is_some());
        assert_eq!(labels(&gone), ["../"]);
        assert_eq!(gone.update(&jump), None);
        assert_eq!(gone.dir, dir);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn formats_level_time() {
        assert_eq!(clock_time(0), "0:00.00");
        assert_eq!(clock_time(90), "0:01.50");
        assert_eq!(clock_time(60 * 75 + 59), "1:15.98");
    }
}
