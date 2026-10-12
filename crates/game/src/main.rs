//! `clamber-game`: play a level in a window.

// Release builds on Windows open no console window; warnings and errors
// go to the log instead (see `report`), and `--console` opens one.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use clamber_game::{
    Canvas, Cast, Clock, Feels, Game, LayerArt, Prefs, SPRITES, Session, VIEW, content_dir,
    encode_gif, encode_png, keys, present, report, rgb,
};
use clamber_input::{Event, Key, Settings};
use clamber_level::Level;
use clamber_pack::Pack;
use clamber_sim::{Script, World};
use clamber_trace::Scenario;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

const USAGE: &str = "\
usage:
  clamber-game [<file.level>] [--pack <pack.toml>] [--preset <id>]
               [--settings <input.toml>] [--debug] [--console]
  clamber-game --capture <file.level> --inputs <file.scn> --out <dir>
               [--pack <pack.toml>] [--preset <id>] [--debug]

Plays the level, or the bundled one if none is given, from the title
screen. Without --pack, a single character plays with the preset's
world physics. The movement screen's choice of preset is remembered in
prefs.toml in the per-user settings folder; --preset overrides it.
Controls are read from, and saved by the controls screen to, --settings
or the per-user settings file. --debug starts with the
collision and region view on. Warnings and errors are written to
clamber.log in the per-user settings folder, and to stderr; on Windows,
--console shows them in a console window. With sound (the `audio`
feature), the level's music plays, and effects from the content folder's
base/sounds.toml.

  Z, Space       jump; pick in menus
  Enter, Escape  pause; back in menus
  F1             collision and region view
  F11            fullscreen

With --capture, plays the scenario's inputs from the title screen
headlessly, without a window or display, and writes latest.gif and
screenshots shot-1.png to shot-4.png to the folder.";

/// Around the scaled view when the window's shape doesn't match it.
const BORDER: u32 = rgb(0, 0, 0);

/// The window starts at this multiple of the view.
const START_SCALE: u32 = 3;

/// Captures are scaled up by this, or drawn at 1:1 if that makes the
/// GIF larger than [`GIF_LIMIT`] bytes.
const CAPTURE_SCALE: u32 = 2;
const GIF_LIMIT: usize = 5_000_000;

/// Sound effects playing at once, at most.
#[cfg(feature = "audio")]
const MAX_VOICES: usize = 16;

struct Options {
    level: Option<PathBuf>,
    pack: Option<String>,
    preset: Option<String>,
    settings: Option<String>,
    capture: Option<Capture>,
    debug: bool,
    console: bool,
}

/// Where a headless capture's inputs come from and its output goes.
struct Capture {
    inputs: PathBuf,
    out: PathBuf,
}

fn parse(args: &[String]) -> Option<Options> {
    let mut options = Options {
        level: None,
        pack: None,
        preset: None,
        settings: None,
        capture: None,
        debug: false,
        console: false,
    };
    let (mut capture, mut inputs, mut out) = (None, None, None);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let slot = match arg.as_str() {
            "--pack" => &mut options.pack,
            "--preset" => &mut options.preset,
            "--settings" => &mut options.settings,
            "--capture" => &mut capture,
            "--inputs" => &mut inputs,
            "--out" => &mut out,
            "--debug" if !options.debug => {
                options.debug = true;
                continue;
            }
            "--console" if !options.console => {
                options.console = true;
                continue;
            }
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
    match (capture, inputs, out) {
        (Some(level), Some(inputs), Some(out)) if options.level.is_none() => {
            options.level = Some(PathBuf::from(level));
            options.capture = Some(Capture {
                inputs: PathBuf::from(inputs),
                out: PathBuf::from(out),
            });
        }
        (None, None, None) => {}
        _ => return None,
    }
    Some(options)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(mut options) = parse(&args) else {
        report::fatal(USAGE);
        return ExitCode::from(2);
    };
    if options.console {
        report::open_console();
    }
    let result = match options.capture.take() {
        Some(capture) => record(options, &capture),
        None => {
            if let Some(path) = clamber_game::log_path() {
                report::start_log(&path);
            }
            play(options)
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            report::fatal(format!("clamber-game: {e}"));
            ExitCode::FAILURE
        }
    }
}

fn play(options: Options) -> Result<(), String> {
    let prefs_path = clamber_game::prefs_path();
    let prefs = prefs_path.as_deref().map(Prefs::load).unwrap_or_default();
    let (mut session, level_path) = load(&options, prefs.preset.as_deref())?;
    session.prefs = prefs;
    session.prefs_path = prefs_path;
    #[cfg(feature = "audio")]
    let audio = open_audio(
        &session.game.world.level,
        level_path.parent().unwrap_or(Path::new("")),
    );
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
            .map_err(|e| report::warn(format!("no gamepads: {e}")))
            .ok(),
        #[cfg(feature = "audio")]
        audio,
        held: BTreeSet::new(),
        clock: Clock::new(),
        last: Instant::now(),
        window: None,
        error: None,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.error.map_or(Ok(()), Err)
}

/// Plays the capture's inputs from the title screen, with the default
/// bindings, no remembered choices and no sound, and writes the GIF and
/// screenshots.
fn record(options: Options, capture: &Capture) -> Result<(), String> {
    let (mut session, _) = load(&options, None)?;
    let scenario = Scenario::load(&capture.inputs).map_err(|e| e.to_string())?;
    let run = clamber_game::capture(&mut session, &Script::from_scenario(&scenario));

    let out = &capture.out;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let write = |name: &str, bytes: &[u8]| {
        let path = out.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    let mut gif = encode_gif(&run.frames, CAPTURE_SCALE)?;
    if gif.len() > GIF_LIMIT {
        gif = encode_gif(&run.frames, 1)?;
    }
    write("latest.gif", &gif)?;
    for (n, &shot) in run.shots.iter().enumerate() {
        write(
            &format!("shot-{}.png", n + 1),
            &encode_png(&run.frames[shot], CAPTURE_SCALE)?,
        )?;
    }
    eprintln!(
        "captured {} frames to {}: latest.gif ({} KB), {} screenshots",
        run.frames.len(),
        out.display(),
        gif.len() / 1000,
        run.shots.len()
    );
    Ok(())
}

/// The level and feel the options ask for, as a session at its title
/// screen, and the level's path. The preset is --preset's, or else
/// `saved` if there's a preset by that name, or else the one the pack
/// was designed for.
fn load(options: &Options, saved: Option<&str>) -> Result<(Session, PathBuf), String> {
    if !content_dir().is_dir() {
        return Err("can't find the `content` folder; \
            keep it in the same folder as clamber-game"
            .into());
    }
    let level_path = options
        .level
        .clone()
        .unwrap_or_else(|| content_dir().join(clamber_game::DEFAULT_LEVEL));
    let level = Level::load(&level_path).map_err(|e| e.to_string())?;
    let art = LayerArt::load(&level, level_path.parent().unwrap_or(Path::new("")))?;
    let cast = Cast::load(&content_dir().join(SPRITES))?;
    let feels = match &options.pack {
        Some(path) => {
            let src = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            Feels::pack(Pack::from_toml_str(&src, path).map_err(|e| e.to_string())?)
        }
        None => Feels::generic(),
    };
    let saved = saved.filter(|id| {
        let known = feels.presets().get(id).is_some();
        if !known {
            report::warn(format!(
                "no preset named `{id}` any more; playing the default"
            ));
        }
        known
    });
    let preset = options
        .preset
        .as_deref()
        .or(saved)
        .unwrap_or(feels.designed_for());
    let (feel, warnings) = feels.feel(preset)?;
    for warning in &warnings {
        report::warn(warning);
    }
    let world = World::try_new(Arc::new(level), Arc::new(feel)).map_err(|e| e.to_string())?;
    let mut session = Session::new(Game::new(world).with_art(art).with_cast(cast));
    session.feels = feels;
    session.game.debug = options.debug;
    Ok((session, level_path))
}

/// Sound output, playing `level`'s music (relative to `dir`) and the
/// bundled sound effects. Without a sound device, or with sounds that
/// can't be read, the game plays on with what there is.
#[cfg(feature = "audio")]
fn open_audio(level: &Level, dir: &Path) -> Option<(clamber_audio::Output, clamber_game::Audio)> {
    use clamber_game::{Audio, DEFAULT_SOUNDS, SoundList};

    let warn = |w: String| report::warn(w);
    let (handle, mixer) = clamber_audio::mixer(MAX_VOICES);
    let output = clamber_audio::Output::open(mixer)
        .map_err(|e| warn(format!("{e}; playing without sound")))
        .ok()?;
    let sounds = SoundList::load(&content_dir().join(DEFAULT_SOUNDS)).unwrap_or_else(|e| {
        warn(format!("{e}; playing without sound effects"));
        SoundList::default()
    });
    let mut audio = Audio::new(handle, output.rate(), &sounds, warn);
    audio.set_music(level.music.as_ref(), dir);
    Some((output, audio))
}

/// The settings in `path`, or the defaults when there's no such file or
/// it can't be read.
fn load_settings(path: &Path) -> Settings {
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::builtin(),
        Err(e) => {
            report::warn(format!(
                "{}: {e}; using the default controls",
                path.display()
            ));
            return Settings::builtin();
        }
    };
    Settings::from_toml_str(&src, &path.display().to_string()).unwrap_or_else(|e| {
        report::warn(format!("{e}; using the default controls"));
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
    /// The output, kept open while the game runs, and what plays on it.
    #[cfg(feature = "audio")]
    audio: Option<(clamber_audio::Output, clamber_game::Audio)>,
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
            #[cfg(feature = "audio")]
            if let Some((_, audio)) = &mut self.audio {
                audio.update(&self.session, report::warn);
            }
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
        assert!(!o.debug);
        assert!(parse(&args("--debug")).unwrap().debug);
        assert!(parse(&args("--debug --debug")).is_none());
        assert!(parse(&args("--console")).unwrap().console);
        assert!(!parse(&args("--debug")).unwrap().console);
        let o = parse(&args("--settings my.toml")).unwrap();
        assert_eq!(o.settings.as_deref(), Some("my.toml"));
        assert!(parse(&args("")).unwrap().level.is_none());
        assert!(parse(&args("a.level b.level")).is_none());
        assert!(parse(&args("--pack")).is_none());
        assert!(parse(&args("--bogus")).is_none());

        let o = parse(&args("--capture a.level --inputs a.scn --out media")).unwrap();
        assert_eq!(o.level, Some(PathBuf::from("a.level")));
        let capture = o.capture.unwrap();
        assert_eq!(capture.inputs, PathBuf::from("a.scn"));
        assert_eq!(capture.out, PathBuf::from("media"));
        assert!(parse(&args("--capture a.level --inputs a.scn")).is_none());
        assert!(parse(&args("b.level --capture a.level --inputs a.scn --out m")).is_none());
        assert!(parse(&args("--inputs a.scn --out m")).is_none());
    }
}
