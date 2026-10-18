//! The controls screen: each player's bindings, remapped one action and
//! device at a time.
//!
//! It works from a keyboard or a gamepad alone. Up and Down pick a row,
//! Left and Right pick the player (on the top row) or the device column,
//! Jump starts a remap, resets or saves, and Pause goes back. While a
//! remap waits for its press, Pause on the other device cancels it, and
//! so does waiting [`TIMEOUT`] frames, since every key on the device
//! being remapped binds.

use clamber_input::{Action, Device, Frame, Input, MAX_PLAYERS, Remapped, Settings};

use crate::canvas::{Color, rgb};
use crate::surface::Surface;
use crate::text::{ADVANCE, LINE, draw_text, draw_text_centred, text_size};

const HEADING: Color = rgb(0xf4, 0xf0, 0xe8);
const LABEL: Color = rgb(0xa8, 0xb0, 0xbc);
const ITEM: Color = rgb(0xa8, 0xb0, 0xbc);
const SELECTED: Color = rgb(0xf4, 0xf0, 0xe8);
const MARKER: Color = rgb(0xf0, 0xc8, 0x40);
const WAITING: Color = rgb(0xf0, 0xc8, 0x40);
const DONE: Color = rgb(0x6c, 0xd4, 0x8a);
const FAILED: Color = rgb(0xe8, 0x50, 0x60);

/// Frames a remap waits for its press before it's cancelled.
pub const TIMEOUT: u32 = 5 * 60;

/// From one row's top to the next.
const PITCH: i32 = LINE + 2;
/// Left edges of the action, keyboard and gamepad columns.
const ACTION_X: i32 = 20;
const KEYS_X: i32 = 74;
const PAD_X: i32 = 182;
/// Characters that fit in the keyboard and gamepad columns.
const KEYS_CHARS: usize = 16;
const PAD_CHARS: usize = 32;
const TOP: i32 = 8;

/// A row on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// Whose bindings are shown; Left and Right change it.
    Player,
    Action(Action),
    /// Put the player's bindings back to the engine's defaults.
    Reset,
    /// Write the settings to the player's settings file.
    Save,
    Back,
}

const ROWS: [Row; 13] = [
    Row::Player,
    Row::Action(Action::Left),
    Row::Action(Action::Right),
    Row::Action(Action::Up),
    Row::Action(Action::Down),
    Row::Action(Action::Jump),
    Row::Action(Action::Action),
    Row::Action(Action::Swap),
    Row::Action(Action::Pause),
    Row::Action(Action::Tandem),
    Row::Reset,
    Row::Save,
    Row::Back,
];

/// What a frame of input asks of whoever shows the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Write the input settings to the settings file, then report how it
    /// went with [`Controls::saved`].
    Save,
    /// Leave the screen.
    Back,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Controls {
    /// The player whose bindings are shown, from 0.
    pub player: usize,
    /// Index into the rows.
    row: usize,
    /// The device column picked on action rows.
    pub device: Device,
    /// Frames the current remap has waited.
    waited: u32,
    /// The last thing that happened, shown at the bottom.
    status: Option<(String, Color)>,
}

impl Default for Controls {
    fn default() -> Self {
        Controls::new()
    }
}

impl Controls {
    /// Player 1's bindings, with their first action and the keyboard
    /// picked.
    pub fn new() -> Controls {
        Controls {
            player: 0,
            row: 1,
            device: Device::Keyboard,
            waited: 0,
            status: None,
        }
    }

    pub fn row(&self) -> Row {
        ROWS[self.row]
    }

    /// The status line's text, if there is one.
    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().map(|s| s.0.as_str())
    }

    /// Applies one frame of input, starting and cancelling remaps on
    /// `input`.
    pub fn update(&mut self, frame: &Frame, input: &mut Input) -> Option<Request> {
        let pressed = frame.pressed;
        if let Some(remap) = input.remapping() {
            self.waited += 1;
            if pressed.contains(Action::Pause) || self.waited >= TIMEOUT {
                input.cancel_remap();
                self.status = Some((format!("{}: unchanged", remap.action), LABEL));
            }
            return None;
        }

        if pressed.contains(Action::Pause) {
            return Some(Request::Back);
        }
        if pressed.contains(Action::Jump) {
            return self.pick(input);
        }
        let n = ROWS.len();
        if pressed.contains(Action::Up) && !pressed.contains(Action::Down) {
            self.row = (self.row + n - 1) % n;
        } else if pressed.contains(Action::Down) && !pressed.contains(Action::Up) {
            self.row = (self.row + 1) % n;
        }
        let left = pressed.contains(Action::Left) && !pressed.contains(Action::Right);
        let right = pressed.contains(Action::Right) && !pressed.contains(Action::Left);
        if left || right {
            match self.row() {
                Row::Player => {
                    self.player = if right {
                        (self.player + 1) % MAX_PLAYERS
                    } else {
                        (self.player + MAX_PLAYERS - 1) % MAX_PLAYERS
                    };
                    self.status = None;
                }
                Row::Action(_) => {
                    self.device = match self.device {
                        Device::Keyboard => Device::Pad,
                        Device::Pad => Device::Keyboard,
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn pick(&mut self, input: &mut Input) -> Option<Request> {
        match self.row() {
            Row::Player => None,
            Row::Action(action) => {
                input.begin_remap(self.player, action, self.device);
                self.waited = 0;
                self.status = None;
                None
            }
            Row::Reset => {
                let mut settings = input.settings().clone();
                settings.players[self.player] = Settings::builtin().players[self.player].clone();
                input.set_settings(settings);
                let message = format!("Player {} reset to the defaults", self.player + 1);
                self.status = Some((message, DONE));
                None
            }
            Row::Save => Some(Request::Save),
            Row::Back => Some(Request::Back),
        }
    }

    /// Shows a finished remap.
    pub fn remapped(&mut self, remapped: Remapped) {
        let mut message = format!("{}: {}", remapped.remap.action, label(remapped.input));
        if let Some(other) = remapped.swapped_with {
            message += &format!(", swapped with {other}");
        }
        self.status = Some((message, DONE));
    }

    /// Shows how saving went: where to, or what went wrong.
    pub fn saved(&mut self, result: Result<String, String>) {
        self.status = Some(match result {
            Ok(path) => (format!("Saved to {}", tail(&path, 50)), DONE),
            Err(e) => (format!("Can't save: {e}"), FAILED),
        });
    }

    /// Draws the screen across the canvas's width, centred on
    /// `centre_x`, with `input`'s bindings.
    pub fn draw(&self, canvas: &mut dyn Surface, centre_x: i32, input: &Input) {
        draw_text_centred(canvas, centre_x, TOP, "CONTROLS", HEADING);
        let bindings = &input.settings().players[self.player];
        let remap = input.remapping();
        let mut y = TOP + 2 * LINE;
        for (i, &row) in ROWS.iter().enumerate() {
            let selected = i == self.row;
            let color = if selected { SELECTED } else { ITEM };
            match row {
                Row::Player => {
                    // The arrows say Left and Right change it.
                    let text = format!("< PLAYER {} >", self.player + 1);
                    draw_text_centred(canvas, centre_x, y, &text, color);
                    if selected {
                        let left = centre_x - text_size(&text).0 / 2;
                        draw_text(canvas, left - 2 * ADVANCE, y, ">", MARKER);
                    }
                    y += PITCH;
                    draw_text(canvas, KEYS_X, y, "KEYBOARD", LABEL);
                    draw_text(canvas, PAD_X, y, "GAMEPAD", LABEL);
                }
                Row::Action(action) => {
                    draw_text(canvas, ACTION_X, y, action.name(), color);
                    for (device, x, chars) in [
                        (Device::Keyboard, KEYS_X, KEYS_CHARS),
                        (Device::Pad, PAD_X, PAD_CHARS),
                    ] {
                        let waiting = remap.is_some_and(|r| {
                            r.player == self.player && r.action == action && r.device == device
                        });
                        let (text, cell) = if waiting {
                            ("...".to_string(), WAITING)
                        } else {
                            let picked = selected && device == self.device;
                            let names = bindings.names(action, device);
                            (fit(&names, chars), if picked { SELECTED } else { ITEM })
                        };
                        if selected && device == self.device {
                            draw_text(canvas, x - 2 * ADVANCE, y, ">", MARKER);
                        }
                        draw_text(canvas, x, y, &text, cell);
                    }
                }
                Row::Reset => draw_text(canvas, ACTION_X, y, "Reset to defaults", color),
                Row::Save => draw_text(canvas, ACTION_X, y, "Save", color),
                Row::Back => draw_text(canvas, ACTION_X, y, "Back", color),
            }
            if selected && matches!(row, Row::Reset | Row::Save | Row::Back) {
                draw_text(canvas, ACTION_X - 2 * ADVANCE, y, ">", MARKER);
            }
            y += PITCH;
        }

        y += LINE / 2;
        if let Some(remap) = remap {
            let what = match remap.device {
                Device::Keyboard => "a key",
                Device::Pad => "a gamepad button",
            };
            let other = match remap.device {
                Device::Keyboard => "the gamepad",
                Device::Pad => "the keyboard",
            };
            let text = format!("Press {what} for {}", remap.action);
            draw_text_centred(canvas, centre_x, y, &text, WAITING);
            let left = TIMEOUT.saturating_sub(self.waited).div_ceil(60);
            let text = format!("Cancels in {left}s, or press pause on {other}");
            draw_text_centred(canvas, centre_x, y + LINE, &text, LABEL);
        } else if let Some((text, color)) = &self.status {
            draw_text_centred(canvas, centre_x, y, text, *color);
        }
    }
}

/// A short, readable label for an input's settings-file name:
/// `KeyZ` is `Z`, `DPadLeft` is `DPAD LEFT`, `LeftStickUp` is
/// `L STICK UP`.
pub fn label(name: &str) -> String {
    let name = ["Key", "Digit"]
        .iter()
        .find_map(|p| name.strip_prefix(p).filter(|rest| !rest.is_empty()))
        .unwrap_or(name);
    let name = name.replace("DPad", "Dpad");
    let mut words: Vec<String> = Vec::new();
    for c in name.chars() {
        match words.last_mut() {
            Some(word) if !c.is_ascii_uppercase() => word.push(c),
            _ => words.push(c.to_string()),
        }
    }
    if words.len() > 1 {
        for side in ["Left", "Right"] {
            if words[0] == side {
                words[0] = side[..1].to_string();
            }
        }
    }
    words.join(" ").to_uppercase()
}

/// The end of `text`, at most `chars` characters, marked with `...`
/// when cut.
fn tail(text: &str, chars: usize) -> String {
    let count = text.chars().count();
    if count <= chars {
        return text.to_string();
    }
    let rest: String = text.chars().skip(count - chars + 3).collect();
    format!("...{rest}")
}

/// As many of the inputs as fit in `chars` characters, then how many
/// more there are.
fn fit(names: &[&str], chars: usize) -> String {
    if names.is_empty() {
        return "-".to_string();
    }
    let labels: Vec<String> = names.iter().map(|n| label(n)).collect();
    let mut text = String::new();
    for (i, l) in labels.iter().enumerate() {
        let joined = if text.is_empty() {
            l.clone()
        } else {
            format!("{text}, {l}")
        };
        let rest = labels.len() - i - 1;
        let more = if rest > 0 {
            format!(" +{rest}")
        } else {
            String::new()
        };
        if joined.chars().count() + more.chars().count() > chars && !text.is_empty() {
            return format!("{text} +{}", labels.len() - i);
        }
        text = joined;
    }
    text
}

#[cfg(test)]
mod tests {
    use clamber_input::{Actions, Event, Key, PadId, PadInput};

    use super::*;
    use crate::canvas::Canvas;

    fn press(action: Action) -> Frame {
        Frame::after(Actions::NONE, action.into())
    }

    fn idle() -> Frame {
        Frame::default()
    }

    fn input() -> Input {
        Input::new(Settings::builtin())
    }

    fn key(input: &mut Input, key: Key) -> Option<Remapped> {
        let r = input.handle(Event::Key { key, down: true });
        input.handle(Event::Key { key, down: false });
        r
    }

    #[test]
    fn labels_inputs() {
        assert_eq!(label("KeyZ"), "Z");
        assert_eq!(label("Digit4"), "4");
        assert_eq!(label("Space"), "SPACE");
        assert_eq!(label("ArrowLeft"), "ARROW LEFT");
        assert_eq!(label("ShiftLeft"), "SHIFT LEFT");
        assert_eq!(label("F11"), "F11");
        assert_eq!(label("DPadLeft"), "DPAD LEFT");
        assert_eq!(label("LeftStickLeft"), "L STICK LEFT");
        assert_eq!(label("RightShoulder"), "R SHOULDER");
        assert_eq!(label("South"), "SOUTH");
    }

    #[test]
    fn cuts_long_paths_from_the_front() {
        assert_eq!(tail("input.toml", 10), "input.toml");
        assert_eq!(tail("/a/input.toml", 10), "...ut.toml");
    }

    #[test]
    fn fits_bindings_in_a_column() {
        assert_eq!(fit(&[], 10), "-");
        assert_eq!(fit(&["KeyZ", "Space"], 16), "Z, SPACE");
        assert_eq!(fit(&["KeyZ", "Space"], 6), "Z +1");
        assert_eq!(
            fit(&["LeftStickLeft"], 4),
            "L STICK LEFT",
            "one always shows"
        );
        assert_eq!(
            fit(&["North", "LeftShoulder", "RightShoulder"], 32),
            "NORTH, L SHOULDER, R SHOULDER"
        );
        assert_eq!(
            fit(&["North", "LeftShoulder", "RightShoulder"], 20),
            "NORTH, L SHOULDER +1"
        );
    }

    #[test]
    fn remaps_a_key_and_reports_the_swap() {
        let mut input = input();
        let mut controls = Controls::new();
        assert_eq!(controls.row(), Row::Action(Action::Left));
        controls.update(&press(Action::Down), &mut input);
        controls.update(&press(Action::Down), &mut input);
        controls.update(&press(Action::Down), &mut input);
        assert_eq!(controls.row(), Row::Action(Action::Down));
        controls.update(&press(Action::Jump), &mut input);
        assert!(input.remapping().is_some());

        let r = key(&mut input, Key::KeyX).expect("binds");
        controls.remapped(r);
        assert_eq!(
            input.settings().players[0].names(Action::Down, Device::Keyboard),
            ["KeyX"]
        );
        assert_eq!(controls.status(), Some("down: X, swapped with action"));
        assert_eq!(
            input.settings().players[0].names(Action::Action, Device::Keyboard),
            ["ArrowDown"]
        );
    }

    #[test]
    fn picks_the_player_and_device() {
        let mut input = input();
        let mut controls = Controls::new();
        controls.update(&press(Action::Right), &mut input);
        assert_eq!(controls.device, Device::Pad);
        controls.update(&press(Action::Up), &mut input);
        assert_eq!(controls.row(), Row::Player);
        controls.update(&press(Action::Left), &mut input);
        assert_eq!(controls.player, 1, "wraps");
        controls.update(&press(Action::Down), &mut input);
        controls.update(&press(Action::Jump), &mut input);
        let remap = input.remapping().unwrap();
        assert_eq!(
            (remap.player, remap.action, remap.device),
            (1, Action::Left, Device::Pad)
        );

        let pad = PadId(0);
        input.handle(Event::PadConnected(pad));
        let r = input.handle(Event::PadButton {
            pad,
            input: PadInput::LeftShoulder,
            down: true,
        });
        controls.remapped(r.unwrap());
        assert_eq!(controls.status(), Some("left: L SHOULDER"));
        assert_eq!(
            input.settings().players[1].names(Action::Left, Device::Pad),
            ["LeftShoulder"]
        );
        assert_eq!(
            input.settings().players[0].names(Action::Left, Device::Pad),
            ["DPadLeft", "LeftStickLeft"],
            "player 1 is untouched"
        );
    }

    #[test]
    fn pause_on_the_other_device_cancels() {
        let mut input = input();
        let mut controls = Controls::new();
        controls.update(&press(Action::Jump), &mut input);
        assert!(input.remapping().is_some());
        // Jump and Pause are ignored while waiting, except to cancel.
        assert_eq!(controls.update(&press(Action::Jump), &mut input), None);
        assert!(input.remapping().is_some());
        assert_eq!(controls.update(&press(Action::Pause), &mut input), None);
        assert_eq!(input.remapping(), None);
        assert_eq!(controls.status(), Some("left: unchanged"));
        assert_eq!(input.settings(), &Settings::builtin());
    }

    #[test]
    fn waiting_too_long_cancels() {
        let mut input = input();
        let mut controls = Controls::new();
        controls.update(&press(Action::Jump), &mut input);
        for _ in 1..TIMEOUT {
            controls.update(&idle(), &mut input);
        }
        assert!(input.remapping().is_some());
        controls.update(&idle(), &mut input);
        assert_eq!(input.remapping(), None);
        assert_eq!(key(&mut input, Key::KeyQ), None, "no longer waiting");
    }

    #[test]
    fn resets_saves_and_goes_back() {
        let mut input = input();
        let mut controls = Controls::new();
        controls.update(&press(Action::Jump), &mut input);
        key(&mut input, Key::KeyQ).unwrap();
        assert_ne!(input.settings(), &Settings::builtin());

        controls.update(&press(Action::Up), &mut input);
        controls.update(&press(Action::Up), &mut input);
        controls.update(&press(Action::Up), &mut input);
        assert_eq!(controls.row(), Row::Save);
        assert_eq!(
            controls.update(&press(Action::Jump), &mut input),
            Some(Request::Save)
        );
        controls.saved(Err("disk full".into()));
        assert_eq!(controls.status(), Some("Can't save: disk full"));

        controls.update(&press(Action::Up), &mut input);
        assert_eq!(controls.row(), Row::Reset);
        assert_eq!(controls.update(&press(Action::Jump), &mut input), None);
        assert_eq!(input.settings(), &Settings::builtin());

        controls.update(&press(Action::Down), &mut input);
        controls.update(&press(Action::Down), &mut input);
        assert_eq!(
            controls.update(&press(Action::Jump), &mut input),
            Some(Request::Back)
        );
        assert_eq!(
            controls.update(&press(Action::Pause), &mut input),
            Some(Request::Back)
        );
    }

    #[test]
    fn draws_bindings_and_the_wait() {
        let mut input = input();
        let mut controls = Controls::new();
        let mut canvas = Canvas::scaled((384, 216), 4);
        controls.draw(&mut canvas, 192, &input);
        assert!(canvas.pixels.contains(&MARKER));
        let before = canvas.clone();

        controls.update(&press(Action::Jump), &mut input);
        controls.update(&idle(), &mut input);
        let mut canvas = Canvas::scaled((384, 216), 4);
        controls.draw(&mut canvas, 192, &input);
        assert_ne!(canvas, before);

        let mut expected = Canvas::scaled((384, 216), 4);
        let prompt = "Press a key for left";
        let y = TOP + 2 * LINE + (ROWS.len() as i32 + 1) * PITCH + LINE / 2;
        draw_text_centred(&mut expected, 192, y, prompt, WAITING);
        let rows = |y: i32| (y * 4 * 384 * 4) as usize;
        // Short of the next line, whose outline reaches up into this one.
        let row = |c: &Canvas| c.pixels[rows(y)..rows(y + LINE - 1)].to_vec();
        assert_eq!(row(&canvas), row(&expected));
    }
}
