//! Warnings and errors for the player. They go to stderr and to a log
//! file, because a release build on Windows runs without a console.
//! There, fatal errors also show in a message box when there's no
//! console to read them in.

use std::fmt::Display;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The log file, once [`start_log`] has opened it, and its path.
static LOG: Mutex<Option<(PathBuf, File)>> = Mutex::new(None);

fn log() -> MutexGuard<'static, Option<(PathBuf, File)>> {
    LOG.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts a new log at `path`, replacing the one from the last run.
/// If it can't be created, that's a warning and only stderr is used.
pub fn start_log(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match File::create(path) {
        Ok(file) => *log() = Some((path.to_path_buf(), file)),
        Err(e) => warn(format!("{}: {e}; not keeping a log", path.display())),
    }
}

/// Writes `line` to stderr and the log.
pub fn note(line: impl Display) {
    let line = line.to_string();
    eprintln!("{line}");
    if let Some((_, file)) = log().as_mut() {
        let _ = writeln!(file, "{line}");
    }
}

/// Notes a warning: something's missing or wrong, but the game goes on.
pub fn warn(warning: impl Display) {
    note(format_args!("warning: {warning}"));
}

/// Notes an error the game can't go on from, and shows it in a message
/// box if there's no console.
pub fn fatal(error: impl Display) {
    let error = error.to_string();
    note(&error);
    #[cfg(windows)]
    if !windows::has_console() {
        let log = log().as_ref().map(|(path, _)| path.clone());
        let text = match log {
            Some(path) => format!("{error}\n\nThe log is in {}", path.display()),
            None => error,
        };
        windows::message_box("Clamber", &text);
    }
}

/// Shows a console for stderr: the one the game was started from, or
/// a new one. Only Windows release builds start without one.
pub fn open_console() {
    #[cfg(windows)]
    windows::open_console();
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, GetConsoleWindow,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    pub fn has_console() -> bool {
        // SAFETY: takes no arguments; returns null when there's no console.
        !unsafe { GetConsoleWindow() }.is_null()
    }

    pub fn open_console() {
        // SAFETY: plain calls with no pointers. If attaching fails (no
        // parent console), a new console is made instead. Rust's stderr
        // looks up the standard handle on each write, so it uses the
        // console from then on.
        unsafe {
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                AllocConsole();
            }
        }
    }

    pub fn message_box(caption: &str, text: &str) {
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (caption, text) = (wide(caption), wide(text));
        // SAFETY: both strings are NUL-terminated UTF-16 that outlive
        // the call, and a null owner window is allowed.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}
