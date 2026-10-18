//! Button prompts: keys and gamepad controls drawn as pictures of them,
//! pre-rendered by `art/prompts.py` into `content/base/ui` and built into
//! the game like the HUD's icons. Keys show their name on the cap, and
//! the face buttons follow the pad's style: letters on coloured buttons,
//! or shapes. Labels are vector text on top, so they stay sharp.

use std::sync::OnceLock;

use clamber_input::{Key, PadInput, PadStyle};

use crate::canvas::{Color, rgb};
use crate::icons::UiArt;
use crate::panel::draw_sliced;
use crate::surface::Surface;
use crate::text::{draw_label, label_width};

const SHEET: &str = include_str!("../../../content/base/ui/prompts.json");
const IMAGE: &[u8] = include_bytes!("../../../content/base/ui/prompts.png");

/// View units per world unit of the art: a key is about 8.5 units tall,
/// inside a row of text and its highlight.
const WORLD: f32 = 0.85;
/// How big labels are, against the usual text: on keys and small
/// buttons, and the letters on face buttons.
const LABEL: f32 = 0.6;
const LETTER: f32 = 0.8;
/// Which stick, on a stick's cap.
const STICK: f32 = 0.45;
/// Room either side of a key's name, in view units.
const KEY_PAD: f32 = 1.5;
/// Between prompts in a row, in view units.
pub const GAP: f32 = 1.5;

const ON_KEY: Color = rgb(0x2a, 0x2e, 0x36);
const ON_PAD: Color = rgb(0xf4, 0xf0, 0xe8);

fn art() -> &'static UiArt {
    static ART: OnceLock<UiArt> = OnceLock::new();
    ART.get_or_init(|| UiArt::load(SHEET, IMAGE, "ui/prompts.json"))
}

/// How one input is drawn: a piece of the art, stretched across to fit
/// its label or not, and the label written on it.
struct Prompt {
    piece: String,
    stretch: bool,
    label: Option<(String, f32, Color)>,
}

impl Prompt {
    fn plain(piece: &str) -> Prompt {
        Prompt {
            piece: piece.to_string(),
            stretch: false,
            label: None,
        }
    }

    fn labelled(piece: &str, stretch: bool, label: &str, scale: f32, color: Color) -> Prompt {
        Prompt {
            piece: piece.to_string(),
            stretch,
            label: Some((label.to_string(), scale, color)),
        }
    }

    /// For the input named `name` in settings files, on a pad of
    /// `style`. A name that's neither a key nor a pad input shows as a
    /// key, labelled with it.
    fn of(name: &str, style: PadStyle) -> Prompt {
        if let Some(&input) = PadInput::ALL.iter().find(|i| i.name() == name) {
            return Prompt::pad(input, style);
        }
        let key = Key::ALL.iter().find(|k| k.name() == name);
        let text = match key {
            Some(Key::ArrowUp) => "↑".to_string(),
            Some(Key::ArrowDown) => "↓".to_string(),
            Some(Key::ArrowLeft) => "←".to_string(),
            Some(Key::ArrowRight) => "→".to_string(),
            _ => label(name),
        };
        Prompt::labelled("key", true, &text, LABEL, ON_KEY)
    }

    fn pad(input: PadInput, style: PadStyle) -> Prompt {
        use PadInput as P;
        let shapes = style == PadStyle::Shapes;
        let face = |position: &str, letter: &str| {
            if shapes {
                Prompt::plain(&format!("face-shapes-{position}"))
            } else {
                let piece = format!("face-letters-{position}");
                Prompt::labelled(&piece, false, letter, LETTER, ON_PAD)
            }
        };
        let small = |piece: &str, text: &str| Prompt::labelled(piece, false, text, LABEL, ON_PAD);
        let stick = |piece: &str, text: &str| Prompt::labelled(piece, false, text, STICK, ON_PAD);
        let pill = |text: &str| Prompt::labelled("pill", true, text, LABEL, ON_PAD);
        let pick = |letters: &'static str, shapes_name: &'static str| {
            if shapes { shapes_name } else { letters }
        };
        match input {
            P::South => face("south", "A"),
            P::East => face("east", "B"),
            P::West => face("west", "X"),
            P::North => face("north", "Y"),
            P::LeftShoulder => pill(pick("LB", "L1")),
            P::RightShoulder => pill(pick("RB", "R1")),
            P::LeftTrigger => small("trigger", pick("LT", "L2")),
            P::RightTrigger => small("trigger", pick("RT", "R2")),
            P::Select => pill("SELECT"),
            P::Start => pill("START"),
            P::LeftStickPress => stick("stick-press", "L"),
            P::RightStickPress => stick("stick-press", "R"),
            P::DPadUp => Prompt::plain("dpad-up"),
            P::DPadDown => Prompt::plain("dpad-down"),
            P::DPadLeft => Prompt::plain("dpad-left"),
            P::DPadRight => Prompt::plain("dpad-right"),
            P::LeftStickUp => stick("stick-up", "L"),
            P::LeftStickDown => stick("stick-down", "L"),
            P::LeftStickLeft => stick("stick-left", "L"),
            P::LeftStickRight => stick("stick-right", "L"),
            P::RightStickUp => stick("stick-up", "R"),
            P::RightStickDown => stick("stick-down", "R"),
            P::RightStickLeft => stick("stick-left", "R"),
            P::RightStickRight => stick("stick-right", "R"),
        }
    }

    /// View units per texel of the art.
    fn texel() -> f32 {
        WORLD / art().sheet.scale as f32
    }

    fn width(&self) -> f32 {
        let frame = art()
            .frames(&self.piece)
            .next()
            .expect("a prompt has a frame");
        let drawn = frame.rect.w as f32 * Prompt::texel();
        match &self.label {
            Some((text, scale, _)) if self.stretch => {
                drawn.max(label_width(text, *scale) + 2.0 * KEY_PAD)
            }
            _ => drawn,
        }
    }

    /// Draws it with its left edge at `x`, centred down on `middle`.
    fn draw(&self, surface: &mut dyn Surface, (x, middle): (f32, f32)) {
        let art = art();
        let frame = art
            .frames(&self.piece)
            .next()
            .expect("a prompt has a frame");
        let k = Prompt::texel();
        let (w, h) = (self.width(), frame.rect.h as f32 * k);
        let dst = [x, middle - h / 2.0, x + w, middle + h / 2.0];
        if self.stretch {
            let end = frame.rect.h / 2;
            draw_sliced(
                surface,
                art,
                frame.rect,
                (end, 0),
                dst,
                (end as f32 * k, 0.0),
                255,
            );
        } else {
            art.draw(surface, frame.rect, dst, 255);
        }
        if let Some((text, scale, color)) = &self.label {
            // A key's cap sits a little above its middle.
            let lift = if self.piece == "key" { 0.4 } else { 0.0 };
            draw_label(surface, (x + w / 2.0, middle - lift), text, *color, *scale);
        }
    }
}

/// How wide the prompt for the input `name` is, in view units.
pub fn prompt_width(name: &str, style: PadStyle) -> f32 {
    Prompt::of(name, style).width()
}

/// Draws the prompt for the input named `name` in settings files, on a
/// pad of `style`, with its left edge at `x` and centred down on
/// `middle`. Returns how wide it is.
pub fn draw_prompt(surface: &mut dyn Surface, name: &str, style: PadStyle, at: (f32, f32)) -> f32 {
    let prompt = Prompt::of(name, style);
    prompt.draw(surface, at);
    prompt.width()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;

    const BG: Color = rgb(1, 2, 3);

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
    fn every_input_has_a_prompt() {
        for style in [PadStyle::Letters, PadStyle::Shapes] {
            for input in PadInput::ALL {
                assert!(prompt_width(input.name(), style) > 4.0, "{input:?}");
            }
        }
        for key in Key::ALL {
            assert!(prompt_width(key.name(), PadStyle::Letters) > 4.0, "{key:?}");
        }
    }

    #[test]
    fn keys_widen_for_long_names() {
        let z = prompt_width("KeyZ", PadStyle::Letters);
        for key in ["KeyQ", "KeyW", "KeyM", "Digit0", "ArrowLeft"] {
            assert_eq!(prompt_width(key, PadStyle::Letters), z, "a square cap");
        }
        assert!(prompt_width("Space", PadStyle::Letters) > z + 4.0);
    }

    #[test]
    fn face_buttons_follow_the_pad_style() {
        let draw = |style| {
            let mut canvas = Canvas::scaled((20, 20), 4);
            canvas.clear(BG);
            let w = draw_prompt(&mut canvas, "South", style, (2.0, 10.0));
            (canvas, w)
        };
        let (letters, w) = draw(PadStyle::Letters);
        let (shapes, _) = draw(PadStyle::Shapes);
        assert_ne!(letters, shapes);
        // Drawn within its width, and centred on the middle.
        let width = letters.width as usize;
        for (i, &p) in letters.pixels.iter().enumerate() {
            if p != BG {
                let (x, y) = ((i % width) as f32 / 4.0, (i / width) as f32 / 4.0);
                assert!((1.5..2.5 + w).contains(&x), "{x}");
                assert!((4.5..15.5).contains(&y), "{y}");
            }
        }
    }
}
