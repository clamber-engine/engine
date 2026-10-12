//! Translates the window's key codes to `clamber-input` keys. Both name
//! keys by physical position after the W3C UI Events `code` values.

use clamber_input::Key;
use winit::keyboard::KeyCode;

/// The input key for a window key code, if the game has one.
pub fn key(code: KeyCode) -> Option<Key> {
    use KeyCode as C;
    Some(match code {
        C::KeyA => Key::KeyA,
        C::KeyB => Key::KeyB,
        C::KeyC => Key::KeyC,
        C::KeyD => Key::KeyD,
        C::KeyE => Key::KeyE,
        C::KeyF => Key::KeyF,
        C::KeyG => Key::KeyG,
        C::KeyH => Key::KeyH,
        C::KeyI => Key::KeyI,
        C::KeyJ => Key::KeyJ,
        C::KeyK => Key::KeyK,
        C::KeyL => Key::KeyL,
        C::KeyM => Key::KeyM,
        C::KeyN => Key::KeyN,
        C::KeyO => Key::KeyO,
        C::KeyP => Key::KeyP,
        C::KeyQ => Key::KeyQ,
        C::KeyR => Key::KeyR,
        C::KeyS => Key::KeyS,
        C::KeyT => Key::KeyT,
        C::KeyU => Key::KeyU,
        C::KeyV => Key::KeyV,
        C::KeyW => Key::KeyW,
        C::KeyX => Key::KeyX,
        C::KeyY => Key::KeyY,
        C::KeyZ => Key::KeyZ,
        C::Digit0 => Key::Digit0,
        C::Digit1 => Key::Digit1,
        C::Digit2 => Key::Digit2,
        C::Digit3 => Key::Digit3,
        C::Digit4 => Key::Digit4,
        C::Digit5 => Key::Digit5,
        C::Digit6 => Key::Digit6,
        C::Digit7 => Key::Digit7,
        C::Digit8 => Key::Digit8,
        C::Digit9 => Key::Digit9,
        C::F1 => Key::F1,
        C::F2 => Key::F2,
        C::F3 => Key::F3,
        C::F4 => Key::F4,
        C::F5 => Key::F5,
        C::F6 => Key::F6,
        C::F7 => Key::F7,
        C::F8 => Key::F8,
        C::F9 => Key::F9,
        C::F10 => Key::F10,
        C::F11 => Key::F11,
        C::F12 => Key::F12,
        C::ArrowUp => Key::ArrowUp,
        C::ArrowDown => Key::ArrowDown,
        C::ArrowLeft => Key::ArrowLeft,
        C::ArrowRight => Key::ArrowRight,
        C::Space => Key::Space,
        C::Enter => Key::Enter,
        C::Escape => Key::Escape,
        C::Tab => Key::Tab,
        C::Backspace => Key::Backspace,
        C::CapsLock => Key::CapsLock,
        C::ShiftLeft => Key::ShiftLeft,
        C::ShiftRight => Key::ShiftRight,
        C::ControlLeft => Key::ControlLeft,
        C::ControlRight => Key::ControlRight,
        C::AltLeft => Key::AltLeft,
        C::AltRight => Key::AltRight,
        C::Minus => Key::Minus,
        C::Equal => Key::Equal,
        C::BracketLeft => Key::BracketLeft,
        C::BracketRight => Key::BracketRight,
        C::Backslash => Key::Backslash,
        C::Semicolon => Key::Semicolon,
        C::Quote => Key::Quote,
        C::Backquote => Key::Backquote,
        C::Comma => Key::Comma,
        C::Period => Key::Period,
        C::Slash => Key::Slash,
        C::Insert => Key::Insert,
        C::Delete => Key::Delete,
        C::Home => Key::Home,
        C::End => Key::End,
        C::PageUp => Key::PageUp,
        C::PageDown => Key::PageDown,
        C::Numpad0 => Key::Numpad0,
        C::Numpad1 => Key::Numpad1,
        C::Numpad2 => Key::Numpad2,
        C::Numpad3 => Key::Numpad3,
        C::Numpad4 => Key::Numpad4,
        C::Numpad5 => Key::Numpad5,
        C::Numpad6 => Key::Numpad6,
        C::Numpad7 => Key::Numpad7,
        C::Numpad8 => Key::Numpad8,
        C::Numpad9 => Key::Numpad9,
        C::NumpadAdd => Key::NumpadAdd,
        C::NumpadSubtract => Key::NumpadSubtract,
        C::NumpadMultiply => Key::NumpadMultiply,
        C::NumpadDivide => Key::NumpadDivide,
        C::NumpadDecimal => Key::NumpadDecimal,
        C::NumpadEnter => Key::NumpadEnter,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every input key comes from the window key code of the same name.
    #[test]
    fn covers_every_input_key() {
        for &k in Key::ALL {
            let code = CODES.iter().find(|c| format!("{c:?}") == k.name());
            let code = code.unwrap_or_else(|| panic!("no window key code named {}", k.name()));
            assert_eq!(key(*code), Some(k));
        }
        assert_eq!(key(KeyCode::MediaPlayPause), None);
    }

    use KeyCode as C;
    #[rustfmt::skip]
    const CODES: &[KeyCode] = &[
        C::KeyA, C::KeyB, C::KeyC, C::KeyD, C::KeyE, C::KeyF, C::KeyG, C::KeyH, C::KeyI,
        C::KeyJ, C::KeyK, C::KeyL, C::KeyM, C::KeyN, C::KeyO, C::KeyP, C::KeyQ, C::KeyR,
        C::KeyS, C::KeyT, C::KeyU, C::KeyV, C::KeyW, C::KeyX, C::KeyY, C::KeyZ,
        C::Digit0, C::Digit1, C::Digit2, C::Digit3, C::Digit4, C::Digit5, C::Digit6,
        C::Digit7, C::Digit8, C::Digit9,
        C::F1, C::F2, C::F3, C::F4, C::F5, C::F6, C::F7, C::F8, C::F9, C::F10, C::F11, C::F12,
        C::ArrowUp, C::ArrowDown, C::ArrowLeft, C::ArrowRight,
        C::Space, C::Enter, C::Escape, C::Tab, C::Backspace, C::CapsLock,
        C::ShiftLeft, C::ShiftRight, C::ControlLeft, C::ControlRight, C::AltLeft, C::AltRight,
        C::Minus, C::Equal, C::BracketLeft, C::BracketRight, C::Backslash, C::Semicolon,
        C::Quote, C::Backquote, C::Comma, C::Period, C::Slash,
        C::Insert, C::Delete, C::Home, C::End, C::PageUp, C::PageDown,
        C::Numpad0, C::Numpad1, C::Numpad2, C::Numpad3, C::Numpad4, C::Numpad5, C::Numpad6,
        C::Numpad7, C::Numpad8, C::Numpad9, C::NumpadAdd, C::NumpadSubtract,
        C::NumpadMultiply, C::NumpadDivide, C::NumpadDecimal, C::NumpadEnter,
    ];
}
