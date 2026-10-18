//! Gamepads through gilrs, including hot-plugging.

use gilrs::{EventType, GamepadId, Gilrs};

use crate::{Axis, Event, PadId, PadInput, PadStyle};

/// The system's gamepads, reported as [`Event`]s.
pub struct Gamepads {
    gilrs: Gilrs,
    /// Pads already connected at startup, reported by the first poll.
    initial: Vec<GamepadId>,
}

impl Gamepads {
    /// Opens the platform's gamepad backend. On platforms gilrs doesn't
    /// support, this still succeeds and never reports a pad.
    pub fn new() -> Result<Gamepads, Box<gilrs::Error>> {
        let gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(gilrs::Error::NotImplemented(dummy)) => dummy,
            Err(e) => return Err(Box::new(e)),
        };
        let initial = gilrs.gamepads().map(|(id, _)| id).collect();
        Ok(Gamepads { gilrs, initial })
    }

    /// Events since the last poll. Call before each logic frame and pass
    /// every event to [`Input::handle`](crate::Input::handle).
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for id in std::mem::take(&mut self.initial) {
            events.push(Event::PadConnected(PadId(id.into())));
            events.push(self.style(id));
        }
        while let Some(gilrs::Event { id, event, .. }) = self.gilrs.next_event() {
            let pad = PadId(id.into());
            if matches!(event, EventType::Connected) {
                events.push(Event::PadConnected(pad));
                events.push(self.style(id));
                continue;
            }
            events.extend(match event {
                EventType::Disconnected => Some(Event::PadDisconnected(pad)),
                EventType::ButtonPressed(b, _) => button(b).map(|input| Event::PadButton {
                    pad,
                    input,
                    down: true,
                }),
                EventType::ButtonReleased(b, _) => button(b).map(|input| Event::PadButton {
                    pad,
                    input,
                    down: false,
                }),
                EventType::AxisChanged(a, value, _) => {
                    axis(a).map(|axis| Event::PadAxis { pad, axis, value })
                }
                _ => None,
            });
        }
        events
    }

    /// How the pad `id` marks its face buttons, judged by its maker and
    /// name.
    fn style(&self, id: GamepadId) -> Event {
        let style = self
            .gilrs
            .connected_gamepad(id)
            .map_or(PadStyle::Letters, |g| PadStyle::of(g.vendor_id(), g.name()));
        Event::PadStyle {
            pad: PadId(id.into()),
            style,
        }
    }
}

fn button(b: gilrs::Button) -> Option<PadInput> {
    use gilrs::Button as B;
    Some(match b {
        B::South => PadInput::South,
        B::East => PadInput::East,
        B::West => PadInput::West,
        B::North => PadInput::North,
        B::LeftTrigger => PadInput::LeftShoulder,
        B::RightTrigger => PadInput::RightShoulder,
        B::LeftTrigger2 => PadInput::LeftTrigger,
        B::RightTrigger2 => PadInput::RightTrigger,
        B::Select => PadInput::Select,
        B::Start => PadInput::Start,
        B::LeftThumb => PadInput::LeftStickPress,
        B::RightThumb => PadInput::RightStickPress,
        B::DPadUp => PadInput::DPadUp,
        B::DPadDown => PadInput::DPadDown,
        B::DPadLeft => PadInput::DPadLeft,
        B::DPadRight => PadInput::DPadRight,
        _ => return None,
    })
}

fn axis(a: gilrs::Axis) -> Option<Axis> {
    use gilrs::Axis as A;
    Some(match a {
        A::LeftStickX => Axis::LeftStickX,
        A::LeftStickY => Axis::LeftStickY,
        A::RightStickX => Axis::RightStickX,
        A::RightStickY => Axis::RightStickY,
        _ => return None,
    })
}
