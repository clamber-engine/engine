//! Physical inputs: keyboard keys and gamepad controls, by position
//! rather than by label, so bindings survive keyboard layouts and pad
//! brands.

use serde::{Deserialize, Serialize};

/// Declares an enum of unit variants named the same in code and in
/// settings files, plus a list of every variant.
macro_rules! named {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum $name { $($variant),* }

        impl $name {
            pub const ALL: &[$name] = &[$($name::$variant),*];

            /// The name used in settings files.
            pub fn name(self) -> &'static str {
                match self { $($name::$variant => stringify!($variant)),* }
            }
        }
    };
}

named! {
    /// A keyboard key by physical position, named as in the W3C UI Events
    /// `code` values (`KeyZ` is the key where QWERTY has Z).
    Key {
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
        KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
        Space, Enter, Escape, Tab, Backspace, CapsLock,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight,
        Minus, Equal, BracketLeft, BracketRight, Backslash, Semicolon, Quote,
        Backquote, Comma, Period, Slash,
        Insert, Delete, Home, End, PageUp, PageDown,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7,
        Numpad8, Numpad9, NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide,
        NumpadDecimal, NumpadEnter,
    }
}

named! {
    /// A gamepad control. Face buttons are named by position (`South` is
    /// the bottom one). Stick directions act as buttons, held while the
    /// stick is pushed past the dead zone.
    PadInput {
        South, East, West, North,
        LeftShoulder, RightShoulder, LeftTrigger, RightTrigger,
        Select, Start, LeftStickPress, RightStickPress,
        DPadUp, DPadDown, DPadLeft, DPadRight,
        LeftStickUp, LeftStickDown, LeftStickLeft, LeftStickRight,
        RightStickUp, RightStickDown, RightStickLeft, RightStickRight,
    }
}

/// An analog stick axis. Positive is right or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
}

impl Axis {
    /// The stick directions this axis drives: (negative, positive).
    pub(crate) fn directions(self) -> (PadInput, PadInput) {
        match self {
            Axis::LeftStickX => (PadInput::LeftStickLeft, PadInput::LeftStickRight),
            Axis::LeftStickY => (PadInput::LeftStickDown, PadInput::LeftStickUp),
            Axis::RightStickX => (PadInput::RightStickLeft, PadInput::RightStickRight),
            Axis::RightStickY => (PadInput::RightStickDown, PadInput::RightStickUp),
        }
    }
}

/// A connected gamepad, identified by the backend. Ids are not reused
/// while the program runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PadId(pub usize);

/// Which kind of device a binding or a remap is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Device {
    Keyboard,
    Pad,
}

/// Something that happened on a device, as reported by the platform layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Key {
        key: Key,
        down: bool,
    },
    PadConnected(PadId),
    PadDisconnected(PadId),
    /// A digital control. Stick directions come from [`Event::PadAxis`].
    PadButton {
        pad: PadId,
        input: PadInput,
        down: bool,
    },
    PadAxis {
        pad: PadId,
        axis: Axis,
        /// From -1.0 to 1.0.
        value: f32,
    },
}
