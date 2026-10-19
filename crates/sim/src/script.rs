//! Input for the simulation: one [`InputFrame`] per step, and scripts of
//! held actions to replay.

use clamber_input::{Action, Actions, Frame, MAX_PLAYERS};
use clamber_trace::{Buttons, Scenario};

/// Every player's input for one step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputFrame {
    pub players: [Frame; MAX_PLAYERS],
}

impl InputFrame {
    /// Input for player 1 only.
    pub fn single(frame: Frame) -> InputFrame {
        let mut input = InputFrame::default();
        input.players[0] = frame;
        input
    }
}

/// Player 1's held actions, one entry per step. Pressed and released
/// follow from what was held, so a script replays exactly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    pub held: Vec<Actions>,
}

impl Script {
    /// `frames` steps with nothing held.
    pub fn idle(frames: u32) -> Script {
        Script {
            held: vec![Actions::NONE; frames as usize],
        }
    }

    /// The scenario's inputs for each of its frames.
    pub fn from_scenario(scenario: &Scenario) -> Script {
        Script::from_buttons((0..scenario.frames).map(|f| scenario.held(f)))
    }

    /// The buttons held on each frame, in order, as actions.
    pub fn from_buttons(held: impl IntoIterator<Item = Buttons>) -> Script {
        Script {
            held: held.into_iter().map(actions).collect(),
        }
    }

    /// The input for each step. Nothing is held before the first.
    pub fn frames(&self) -> impl Iterator<Item = InputFrame> + '_ {
        let previous = std::iter::once(Actions::NONE).chain(self.held.iter().copied());
        previous
            .zip(&self.held)
            .map(|(previous, &held)| InputFrame::single(Frame::after(previous, held)))
    }
}

/// Scenario buttons and input actions share their codes.
fn actions(buttons: Buttons) -> Actions {
    Action::ALL
        .into_iter()
        .filter(|a| Buttons::from_code(a.code()).is_some_and(|b| buttons.contains(b)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_inputs_become_frames() {
        let scenario = Scenario::parse(
            "# clamber-scenario v1\n\
             id: t\nsummary: s\nrequires: r\nstart: s\nframes: 4\n\
             [inputs]\n0-3 R\n1-2 JA\n",
            "test",
        )
        .unwrap();
        let script = Script::from_scenario(&scenario);
        let held: Vec<String> = script.held.iter().map(|a| a.to_string()).collect();
        assert_eq!(held, ["R", "RJA", "RJA", "R"]);

        let frames: Vec<Frame> = script.frames().map(|f| f.players[0]).collect();
        assert_eq!(frames[0].pressed, Action::Right.into());
        assert_eq!(frames[1].pressed.to_string(), "JA");
        assert!(frames[2].pressed.is_empty());
        assert_eq!(frames[3].released.to_string(), "JA");
        assert!(script.frames().all(|f| f.players[1] == Frame::default()));
    }
}
