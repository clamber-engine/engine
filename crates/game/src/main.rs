//! `clamber-game`: play a level in a window.

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use clamber_game::{Canvas, Clock, Game, Session, VIEW, content_dir, keys, present, rgb};
use clamber_input::{Event, Key, Settings};
use clamber_level::Level;
use clamber_pack::Pack;
use clamber_sim::{World, generic_feel};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

const USAGE: &str = "\
usage:
  clamber-game [<file.level>] [--pack <pack.toml>] [--preset <id>]
               [--settings <input.toml>]

Plays the level, or the bundled one if none is given, from the title
screen. Without --pack, a single character plays with the preset's
world physics. Controls are read from, and saved by the controls screen
to, --settings or the per-user settings file.

  Z, Space       jump; pick in menus
  Enter, Escape  pause; back in menus
  F1             collision and region view
  F11            fullscreen";

/// Around the scaled view when the window's shape doesn't match it.
const BORDER: u32 = rgb(0, 0, 0);

/// The window starts at this multiple of the view.
const START_SCALE: u32 = 3;

struct Options {
    level: Option<PathBuf>,
    pack: Option<String>,
    preset: Option<String>,
    settings: Option<String>,
}

fn parse(args: &[String]) -> Option<Options> {
    let mut options = Options {
        level: None,
        pack: None,
        preset: None,
        settings: None,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let slot = match arg.as_str() {
            "--pack" => &mut options.pack,
            "--preset" => &mut options.preset,
            "--settings" => &mut options.settings,
            _ if arg.starts_with('-') || options.level.is_some() => return None,
            _ => {
                options.level = Some(PathBuf::from(arg));
                continue;
            }
        };
        if slot.replace(iter.next()?.clone()).is_some() {
            return None;
        }
    }
    Some(options)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(options) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match play(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("clamber-game: {e}");
            ExitCode::FAILURE
        }
    }
}

fn play(options: Options) -> Result<(), String> {
    let level_path = options
        .level
        .unwrap_or_else(|| content_dir().join(clamber_game::DEFAULT_LEVEL));
    let level = Level::load(&level_path).map_err(|e| e.to_string())?;
    let feel = match &options.pack {
        Some(path) => {
            let src = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let pack = Pack::from_toml_str(&src, path).map_err(|e| e.to_string())?;
            let selection = pack
                .select_feel(options.preset.as_deref())
                .map_err(|e| e.to_string())?;
            for warning in &selection.warnings {
                eprintln!("warning: {warning}");
            }
            selection.feel
        }
        None => {
            let preset = options
                .preset
                .as_deref()
                .unwrap_or(clamber_pack::preset::DEFAULT);
            generic_feel(preset).ok_or_else(|| format!("no preset named `{preset}`"))?
        }
    };
    let world = World::new(Arc::new(level), Arc::new(feel));
    let mut session = Session::new(Game::new(world));
    session.settings_path = options
        .settings
        .map(PathBuf::from)
        .or_else(clamber_game::settings_path);
    if let Some(path) = &session.settings_path {
        session.input.set_settings(load_settings(path));
    }

    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App {
        title: format!("Clamber: {}", title(&session.game.world.level, &level_path)),
        session,
        canvas: Game::canvas(),
        #[cfg(feature = "gamepad")]
        gamepads: clamber_input::Gamepads::new()
            .map_err(|e| eprintln!("warning: no gamepads: {e}"))
            .ok(),
        held: BTreeSet::new(),
        clock: Clock::new(),
        last: Instant::now(),
        window: None,
        error: None,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.error.map_or(Ok(()), Err)
}

/// The settings in `path`, or the defaults when there's no such file or
/// it can't be read.
fn load_settings(path: &Path) -> Settings {
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::builtin(),
        Err(e) => {
            eprintln!(
                "warning: {}: {e}; using the default controls",
                path.display()
            );
            return Settings::builtin();
        }
    };
    Settings::from_toml_str(&src, &path.display().to_string()).unwrap_or_else(|e| {
        eprintln!("warning: {e}; using the default controls");
        Settings::builtin()
    })
}

fn title(level: &Level, path: &Path) -> String {
    if level.name.is_empty() {
        path.display().to_string()
    } else {
        level.name.clone()
    }
}

struct Screen {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
}

struct App {
    title: String,
    session: Session,
    canvas: Canvas,
    #[cfg(feature = "gamepad")]
    gamepads: Option<clamber_input::Gamepads>,
    /// Keys down now, to release them all when the window loses focus.
    held: BTreeSet<Key>,
    clock: Clock,
    last: Instant,
    window: Option<Screen>,
    error: Option<String>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }

    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<Screen, String> {
        let attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(PhysicalSize::new(
                VIEW.0 * START_SCALE,
                VIEW.1 * START_SCALE,
            ))
            .with_min_inner_size(PhysicalSize::new(VIEW.0, VIEW.1));
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| e.to_string())?,
        );
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface =
            softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
        Ok(Screen { window, surface })
    }

    fn key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let down = event.state == ElementState::Pressed;
        if down && !event.repeat {
            match code {
                KeyCode::F1 => self.session.game.debug = !self.session.game.debug,
                KeyCode::F11 => {
                    if let Some(screen) = &self.window {
                        let full = screen.window.fullscreen().is_some();
                        screen
                            .window
                            .set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
                    }
                }
                _ => {}
            }
        }
        if let Some(key) = keys::key(code) {
            if down {
                self.held.insert(key);
            } else {
                self.held.remove(&key);
            }
            self.session.handle(Event::Key { key, down });
        }
    }

    /// Runs the steps that are due, then draws.
    fn frame(&mut self) -> Result<(), String> {
        let now = Instant::now();
        let steps = self.clock.advance(now - self.last);
        self.last = now;
        for _ in 0..steps {
            #[cfg(feature = "gamepad")]
            if let Some(pads) = &mut self.gamepads {
                for event in pads.poll() {
                    self.session.handle(event);
                }
            }
            self.session.tick();
            if self.session.quit {
                return Ok(());
            }
        }
        self.session.draw(&mut self.canvas, self.clock.alpha());

        let Some(screen) = &mut self.window else {
            return Ok(());
        };
        let size = screen.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return Ok(());
        };
        screen.surface.resize(w, h).map_err(|e| e.to_string())?;
        let mut buffer = screen.surface.buffer_mut().map_err(|e| e.to_string())?;
        present(&self.canvas, &mut buffer, size.width, size.height, BORDER);
        buffer.present().map_err(|e| e.to_string())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match self.open(event_loop) {
            Ok(screen) => {
                self.window = Some(screen);
                self.last = Instant::now();
            }
            Err(e) => self.fail(event_loop, format!("can't open a window: {e}")),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => self.key(event),
            WindowEvent::Focused(false) => {
                for key in std::mem::take(&mut self.held) {
                    self.session.handle(Event::Key { key, down: false });
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.frame() {
                    self.fail(event_loop, e);
                } else if self.session.quit {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let due = self.last + self.clock.until_next();
        if Instant::now() >= due {
            if let Some(screen) = &self.window {
                screen.window.request_redraw();
            }
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(due));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_arguments() {
        let o = parse(&args("a.level --preset fast")).unwrap();
        assert_eq!(o.level, Some(PathBuf::from("a.level")));
        assert_eq!(o.preset.as_deref(), Some("fast"));
        let o = parse(&args("--settings my.toml")).unwrap();
        assert_eq!(o.settings.as_deref(), Some("my.toml"));
        assert!(parse(&args("")).unwrap().level.is_none());
        assert!(parse(&args("a.level b.level")).is_none());
        assert!(parse(&args("--pack")).is_none());
        assert!(parse(&args("--bogus")).is_none());
    }
}
