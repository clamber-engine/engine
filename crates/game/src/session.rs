//! The screens around a level: title, pause, controls, level complete
//! and game over, and moving between them and play.
//!
//! Menus read the same neutral actions as play, from every player at
//! once, so a keyboard or a gamepad alone can drive them (see
//! [`Menu`]).

use std::path::{Path, PathBuf};

use clamber_input::{Action, Event, Frame, Input, Settings};
use clamber_sim::{InputFrame, Outcome};

use crate::camera::VIEW;
use crate::canvas::{Canvas, Color, rgb};
use crate::controls::{Controls, Request};
use crate::game::Game;
use crate::hud::{COUNT, GEM, RELIC_ICON, relic_total};
use crate::menu::{Choice, Menu};
use crate::sound::{Cue, cues};
use crate::text::{ADVANCE, LINE, draw_text, draw_text_centred, text_size};

const TITLE: Color = rgb(0xf0, 0xc8, 0x40);
const HEADING: Color = rgb(0xf4, 0xf0, 0xe8);
const SUBTITLE: Color = rgb(0xa8, 0xb0, 0xbc);
const FAILED: Color = rgb(0xe8, 0x50, 0x60);
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
}

impl Screen {
    pub fn title() -> Screen {
        Screen::Title(Menu::new([
            ("Start", Item::Start),
            ("Controls", Item::Controls),
            ("Quit", Item::Quit),
        ]))
    }

    fn paused() -> Screen {
        Screen::Paused(Menu::new([
            ("Resume", Item::Resume),
            ("Restart level", Item::Restart),
            ("Controls", Item::Controls),
            ("Quit", Item::ToTitle),
        ]))
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
    /// Quit was picked on the title screen; the window should close.
    pub quit: bool,
    /// What made a sound during the last step, in order.
    pub cues: Vec<Cue>,
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
            quit: false,
            cues: Vec::new(),
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
            Item::ToTitle => self.screen = Screen::title(),
            Item::Quit => self.quit = true,
        }
    }

    fn play(&mut self, input: &InputFrame) {
        if everyone(input).pressed.contains(Action::Pause) {
            self.screen = Screen::paused();
            self.cues.push(Cue::Pause);
            return;
        }
        self.game.step(input);
        self.cues.extend(cues(&self.game.world));
        match self.game.world.flow.outcome {
            Outcome::Complete => self.screen = Screen::complete(),
            Outcome::GameOver => self.screen = Screen::game_over(),
            _ => {}
        }
    }

    /// Whether the level's music should be playing: during play, and
    /// on through pausing it.
    pub fn music_on(&self) -> bool {
        match &self.screen {
            Screen::Playing | Screen::Paused(_) => true,
            Screen::Controls(_, from) => matches!(**from, Screen::Paused(_)),
            Screen::Title(_) | Screen::Complete(_) | Screen::GameOver(_) => false,
        }
    }

    /// Draws the screen. During play, `alpha` is how far to draw between
    /// the last two states; behind a menu the level stands still.
    pub fn draw(&self, canvas: &mut Canvas, alpha: f64) {
        let playing = self.screen == Screen::Playing;
        self.game.draw(canvas, if playing { alpha } else { 1.0 });
        if playing {
            return;
        }
        canvas.shade(DIM, 160);

        let centre = VIEW.0 as i32 / 2;
        let (menu, top) = match &self.screen {
            Screen::Playing => return,
            Screen::Controls(controls, _) => {
                controls.draw(canvas, centre, &self.input);
                return;
            }
            Screen::Title(menu) => {
                draw_text_centred(canvas, centre, 60, "CLAMBER", TITLE);
                let name = &self.game.world.level.name;
                if !name.is_empty() {
                    draw_text_centred(canvas, centre, 60 + 2 * LINE, name, SUBTITLE);
                }
                (menu, 120)
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

    /// The level's time and what was collected, centred on `centre_x`.
    fn draw_results(&self, canvas: &mut Canvas, centre_x: i32, top: i32) {
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

    #[test]
    fn formats_level_time() {
        assert_eq!(clock_time(0), "0:00.00");
        assert_eq!(clock_time(90), "0:01.50");
        assert_eq!(clock_time(60 * 75 + 59), "1:15.98");
    }
}
