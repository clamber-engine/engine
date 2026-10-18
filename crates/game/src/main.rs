//! `clamber-game`: play a level in a window.

// Release builds on Windows open no console window; warnings and errors
// go to the log instead (see `report`), and `--console` opens one.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use clamber_game::{
    Canvas, Clock, Credits, DEFAULT_CREDITS, DrawList, Feels, Fit, GIF_DELAY, Game, Gpu, Pace,
    Prefs, SPRITES, Screen, Session, Surface, VIEW, content_dir, display, encode_gif, encode_png,
    keys, load_game, present, report, rgb,
};
use clamber_input::{Event, Key, Settings};
use clamber_level::Level;
use clamber_pack::Pack;
use clamber_sim::Script;
use clamber_trace::Scenario;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, Modifiers, WindowEvent};
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
world physics. The movement screen's choice of preset, the options
screen's display settings and the window's size are remembered in
prefs.toml in the per-user settings folder; --preset overrides the
preset.
Controls are read from, and saved by the controls screen to, --settings
or the per-user settings file. --debug starts with the
collision and region view on. Warnings and errors are written to
clamber.log in the per-user settings folder, and to stderr; on Windows,
--console shows them in a console window. With sound (the `audio`
feature), music plays on the title screen and in the level, with loops
and volumes from the content folder's base/music.toml, and effects from
its base/sounds.toml.

  Z, Space       jump; pick in menus
  Enter, Escape  pause; back in menus
  F1             collision and region view
  F11, Alt+Enter fullscreen

With --capture, plays the scenario's inputs from the title screen
headlessly, without a window or display, and writes latest.gif and
screenshots shot-1.png to shot-4.png to the folder.";

/// Around the scaled view when the window's shape doesn't match it,
/// drawing without a GPU.
const BORDER: u32 = rgb(0, 0, 0);

/// The capture's GIF is the first that fits in [`GIF_LIMIT`] bytes, or
/// else the last, of: each of these pixels per view unit, largest first,
/// keeping every captured frame, then every second, then every third.
/// Smooth art changes most pixels every frame, so it gives up frame rate
/// before resolution.
const GIF_SCALES: [u32; 2] = [2, 1];
const GIF_STEPS: [u16; 3] = [1, 2, 3];
const GIF_LIMIT: usize = 5_000_000;

/// Pixels per view unit in screenshots: 1920 by 1080, the density the
/// bundled art is rendered at.
const SHOT_SCALE: u32 = 5;

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
    session.apply_widescreen();
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
        level_path,
        session,
        canvas: Game::canvas(),
        list: DrawList::new(VIEW),
        #[cfg(feature = "gamepad")]
        gamepads: clamber_input::Gamepads::new()
            .map_err(|e| report::warn(format!("no gamepads: {e}")))
            .ok(),
        #[cfg(feature = "audio")]
        audio,
        held: BTreeSet::new(),
        modifiers: Modifiers::default(),
        focused: true,
        occluded: false,
        clock: Clock::new(),
        pace: None,
        last: Instant::now(),
        drawn: Instant::now(),
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
    // Captures are made to be shared, so not of a personal import.
    if let Some(level) = &options.level {
        clamber_import::check_shareable(level)
            .map_err(|e| format!("can't capture {}: {e}", level.display()))?;
    }
    let scenario = Scenario::load(&capture.inputs).map_err(|e| e.to_string())?;
    let script = Script::from_scenario(&scenario);

    let out = &capture.out;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let write = |name: &str, bytes: &[u8]| {
        let path = out.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    let mut gif = Vec::new();
    let mut run = None;
    'fit: for scale in GIF_SCALES {
        let (mut session, _) = load(&options, None)?;
        let captured = run.insert(clamber_game::capture(&mut session, &script, scale));
        for step in GIF_STEPS {
            let frames: Vec<_> = captured
                .frames
                .iter()
                .step_by(step.into())
                .cloned()
                .collect();
            gif = encode_gif(&frames, 1, GIF_DELAY * step)?;
            if gif.len() <= GIF_LIMIT {
                break 'fit;
            }
        }
    }
    let Some(run) = run else {
        return Err("no GIF scales to try".into());
    };
    write("latest.gif", &gif)?;

    let (mut session, _) = load(&options, None)?;
    let shots = clamber_game::redraw(&mut session, &script, &run.shots, SHOT_SCALE);
    for (n, shot) in shots.iter().enumerate() {
        write(&format!("shot-{}.png", n + 1), &encode_png(shot, 1)?)?;
    }
    eprintln!(
        "captured {} frames to {}: latest.gif ({} KB, {}x{}), {} screenshots",
        run.frames.len(),
        out.display(),
        gif.len() / 1000,
        run.frames[0].width,
        run.frames[0].height,
        shots.len()
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
    let sprites = content_dir().join(SPRITES);
    let game = load_game(&level_path, &sprites, Arc::new(feel))?;
    let mut session = Session::new(game);
    session.feels = feels;
    session.level_path = level_path.clone();
    session.bundled = Some(level_path.clone());
    session.packs_dir = clamber_import::packs_dir();
    session.credits = Credits::load(&content_dir().join(DEFAULT_CREDITS)).unwrap_or_else(|e| {
        report::warn(e);
        Credits::default()
    });
    session.game.debug = options.debug;
    Ok((session, level_path))
}

/// Sound output, playing the bundled title music, `level`'s music
/// (relative to `dir`) and the bundled sound effects. Without a sound
/// device, or with sounds that can't be read, the game plays on with
/// what there is.
#[cfg(feature = "audio")]
fn open_audio(level: &Level, dir: &Path) -> Option<(clamber_audio::Output, clamber_game::Audio)> {
    use clamber_game::{Audio, DEFAULT_MUSIC, DEFAULT_SOUNDS, MusicList, SoundList};

    let warn = |w: String| report::warn(w);
    let (handle, mixer) = clamber_audio::mixer(MAX_VOICES);
    let output = clamber_audio::Output::open(mixer)
        .map_err(|e| warn(format!("{e}; playing without sound")))
        .ok()?;
    let sounds = SoundList::load(&content_dir().join(DEFAULT_SOUNDS)).unwrap_or_else(|e| {
        warn(format!("{e}; playing without sound effects"));
        SoundList::default()
    });
    let music = MusicList::load(&content_dir().join(DEFAULT_MUSIC)).unwrap_or_else(|e| {
        warn(format!("{e}; music plays without loops or volumes"));
        MusicList::default()
    });
    let mut audio = Audio::new(handle, output.rate(), &sounds, warn);
    audio.set_title_music(&music);
    audio.set_music(level.music.as_ref(), dir, &music);
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

struct Shown {
    window: Arc<Window>,
    output: Output,
}

/// How frames get to the window.
enum Output {
    Gpu(Box<Gpu>),
    /// Without a GPU that can draw to the window: no vsync, and only
    /// whole-number scales, drawn at that scale.
    Software(softbuffer::Surface<Arc<Window>, Arc<Window>>),
}

struct App {
    title: String,
    /// The level the title is for, to change it when another is played.
    level_path: PathBuf,
    session: Session,
    /// What's drawn without a GPU, at the scale it's shown at.
    canvas: Canvas,
    /// What's drawn with one.
    list: DrawList,
    #[cfg(feature = "gamepad")]
    gamepads: Option<clamber_input::Gamepads>,
    /// The output, kept open while the game runs, and what plays on it.
    #[cfg(feature = "audio")]
    audio: Option<(clamber_audio::Output, clamber_game::Audio)>,
    /// Keys down now, to release them all when the window loses focus.
    held: BTreeSet<Key>,
    modifiers: Modifiers,
    focused: bool,
    /// The window is hidden behind others or minimized.
    occluded: bool,
    clock: Clock,
    /// How frames drawn with vsync pace the clock, on the display the
    /// window is on, if its refresh rate is known.
    pace: Option<Pace>,
    /// When the clock was last advanced.
    last: Instant,
    /// When the last frame was drawn.
    drawn: Instant,
    window: Option<Shown>,
    error: Option<String>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }

    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<Shown, String> {
        let prefs = &self.session.prefs.display;
        let screen = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next())
            .map(|m| (m.size().width, m.size().height));
        let (width, height) = display::start_size(prefs.window, screen, VIEW);
        let attributes = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(PhysicalSize::new(width, height))
            .with_min_inner_size(PhysicalSize::new(VIEW.0, VIEW.1))
            .with_fullscreen(prefs.fullscreen.then_some(Fullscreen::Borderless(None)));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| e.to_string())?,
        );
        let gpu = Gpu::new(
            window.clone(),
            event_loop.owned_display_handle(),
            prefs.vsync,
        );
        let output = match gpu {
            Ok(gpu) => Output::Gpu(Box::new(gpu)),
            Err(e) => {
                report::warn(format!(
                    "can't draw with the GPU ({e}); drawing without vsync"
                ));
                let context =
                    softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
                Output::Software(
                    softbuffer::Surface::new(&context, window.clone())
                        .map_err(|e| e.to_string())?,
                )
            }
        };
        Ok(Shown { window, output })
    }

    fn key(&mut self, event: KeyEvent) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let down = event.state == ElementState::Pressed;
        let alt_enter = self.modifiers.state().alt_key()
            && matches!(code, KeyCode::Enter | KeyCode::NumpadEnter);
        if down && !event.repeat {
            match code {
                KeyCode::F1 => self.session.game.debug = !self.session.game.debug,
                KeyCode::F11 => self.toggle_fullscreen(),
                _ if alt_enter => self.toggle_fullscreen(),
                _ => {}
            }
        }
        // Alt+Enter is only for fullscreen, not also pause.
        if alt_enter && down {
            return;
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

    fn toggle_fullscreen(&mut self) {
        let display = &mut self.session.prefs.display;
        display.fullscreen = !display.fullscreen;
        self.session.save_prefs("fullscreen");
        self.apply_display();
    }

    /// Makes the window match the display prefs, which the options
    /// screen may have changed.
    fn apply_display(&mut self) {
        let Some(shown) = &mut self.window else {
            return;
        };
        let display = &self.session.prefs.display;
        if display.fullscreen != shown.window.fullscreen().is_some() {
            shown
                .window
                .set_fullscreen(display.fullscreen.then_some(Fullscreen::Borderless(None)));
        }
        if let Output::Gpu(gpu) = &mut shown.output {
            gpu.set_vsync(display.vsync);
        }
    }

    /// Names the window after the level, once another is played.
    fn retitle(&mut self) {
        if self.session.level_path == self.level_path {
            return;
        }
        self.level_path = self.session.level_path.clone();
        let level = &self.session.game.world.level;
        self.title = format!("Clamber: {}", title(level, &self.level_path));
        if let Some(shown) = &self.window {
            shown.window.set_title(&self.title);
        }
    }

    /// Notes the window's size, to open at it next time, unless it's
    /// fullscreen, maximized or minimized.
    fn resized(&mut self, size: PhysicalSize<u32>) {
        let Some(shown) = &self.window else {
            return;
        };
        let window = &shown.window;
        let windowed = !self.session.prefs.display.fullscreen
            && window.fullscreen().is_none()
            && !window.is_maximized()
            && window.is_minimized() != Some(true);
        if windowed && size.width > 0 && size.height > 0 {
            self.session.prefs.display.window = Some([size.width, size.height]);
        }
        window.request_redraw();
    }

    /// Whether drawing would show anything.
    fn visible(&self) -> bool {
        self.window.as_ref().is_some_and(|shown| {
            let size = shown.window.inner_size();
            !self.occluded
                && shown.window.is_minimized() != Some(true)
                && size.width > 0
                && size.height > 0
        })
    }

    /// The least time between frames now: none during play or on the
    /// level map with vsync, as presenting waits for the display, and
    /// less often behind menus and out of focus.
    fn frame_interval(&self) -> std::time::Duration {
        let vsync = matches!(
            self.window.as_ref().map(|s| &s.output),
            Some(Output::Gpu(gpu)) if gpu.vsync()
        );
        let active = self.focused && matches!(self.session.screen, Screen::Playing | Screen::Map);
        display::frame_interval(vsync, active)
    }

    /// Whether the steps follow the frames: drawn with vsync during
    /// play, on a display whose refresh rate is known. Otherwise they
    /// follow the clock, between frames too.
    fn paced(&self) -> bool {
        self.pace.is_some() && self.visible() && self.frame_interval().is_zero()
    }

    /// Notes the refresh rate of the display the window is on.
    fn find_pace(&mut self) {
        let Some(shown) = &self.window else {
            return;
        };
        let millihertz = shown
            .window
            .current_monitor()
            .and_then(|m| m.refresh_rate_millihertz());
        self.pace = millihertz.and_then(Pace::new);
    }

    /// Runs the logic steps that are due.
    fn update(&mut self) {
        let now = Instant::now();
        let elapsed = now - self.last;
        let paced = self.paced();
        let steps = match &mut self.pace {
            Some(pace) if paced => self.clock.advance_paced(elapsed, pace),
            _ => self.clock.advance(elapsed),
        };
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
                return;
            }
        }
        if steps > 0 {
            self.apply_display();
            self.retitle();
        }
    }

    /// Runs the steps that are due, then draws between the last two.
    fn frame(&mut self) -> Result<(), String> {
        self.update();
        if self.session.quit {
            return Ok(());
        }
        self.drawn = Instant::now();
        let alpha = self.clock.alpha();

        let Some(shown) = &mut self.window else {
            return Ok(());
        };
        match &mut shown.output {
            Output::Gpu(gpu) => {
                let look = self.session.look();
                self.list.view = look.view;
                self.list.pixel_art = look.pixel_art;
                self.session.draw(&mut self.list, alpha);
                gpu.present(&self.list)
            }
            Output::Software(surface) => {
                let size = shown.window.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return Ok(());
                };
                // Pixel art is drawn a pixel to a unit and scaled up as
                // it's presented; the rest is drawn at the window's size.
                let look = self.session.look();
                let scale = match look.pixel_art {
                    true => 1,
                    false => Fit::new(look.view, (size.width, size.height)).scale,
                };
                if self.canvas.scale != scale || self.canvas.view() != look.view {
                    self.canvas = Canvas::scaled(look.view, scale);
                }
                self.session.draw(&mut self.canvas, alpha);
                surface.resize(w, h).map_err(|e| e.to_string())?;
                let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
                present(&self.canvas, &mut buffer, size.width, size.height, BORDER);
                buffer.present().map_err(|e| e.to_string())
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match self.open(event_loop) {
            Ok(shown) => {
                self.window = Some(shown);
                self.find_pace();
                self.last = Instant::now();
            }
            Err(e) => self.fail(event_loop, format!("can't open a window: {e}")),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => self.key(event),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers,
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    for key in std::mem::take(&mut self.held) {
                        self.session.handle(Event::Key { key, down: false });
                    }
                    self.modifiers = Modifiers::default();
                    self.session.pause();
                }
            }
            WindowEvent::Occluded(occluded) => self.occluded = occluded,
            WindowEvent::Resized(size) => {
                self.find_pace();
                self.resized(size);
            }
            WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. } => self.find_pace(),
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
        // Paced steps are only run as frames are drawn.
        if !self.paced() {
            self.update();
        }
        if self.session.quit {
            event_loop.exit();
            return;
        }
        let mut wake = self.last + self.clock.until_next();
        if self.visible() {
            let next_frame = self.drawn + self.frame_interval();
            if Instant::now() >= next_frame {
                if let Some(shown) = &self.window {
                    shown.window.request_redraw();
                }
            } else {
                wake = wake.min(next_frame);
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
    }

    /// Remembers the window's size for next time.
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(path) = &self.session.prefs_path {
            let saved = Prefs::load(path);
            if saved.display.window != self.session.prefs.display.window {
                self.session.save_prefs("the window's size");
            }
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
