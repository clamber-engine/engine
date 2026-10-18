//! Neutral actions: what the simulation sees, whatever the device.

use std::fmt;
use std::ops::{BitAnd, BitOr, Not};

use serde::{Deserialize, Serialize};

/// One neutral action. Codes and order match the button codes in
/// `spec/scenarios/FORMAT.md`, so recorded input converts to and from
/// scenarios and traces bit for bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    Jump,
    /// Run while held, roll on press.
    Action,
    /// Swap the lead character.
    Swap,
    Pause,
    /// Act on the pair of characters; what it does depends on the movement.
    Tandem,
}

impl Action {
    pub const ALL: [Action; 9] = [
        Action::Left,
        Action::Right,
        Action::Up,
        Action::Down,
        Action::Jump,
        Action::Action,
        Action::Swap,
        Action::Pause,
        Action::Tandem,
    ];

    /// The scenario and trace code: one of `LRUDJASPT`.
    pub fn code(self) -> char {
        b"LRUDJASPT"[self as usize] as char
    }

    /// The name used in settings files.
    pub fn name(self) -> &'static str {
        [
            "left", "right", "up", "down", "jump", "action", "swap", "pause", "tandem",
        ][self as usize]
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A set of actions, one bit per action in [`Action::ALL`] order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Actions(u16);

impl Actions {
    pub const NONE: Actions = Actions(0);

    /// Bit `i` is `Action::ALL[i]`, the same layout as trace buttons.
    pub fn from_bits(bits: u16) -> Actions {
        Actions(bits)
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    pub fn contains(self, action: Action) -> bool {
        self.0 & (1 << action as u16) != 0
    }

    pub fn insert(&mut self, action: Action) {
        self.0 |= 1 << action as u16;
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Action> {
        Action::ALL.into_iter().filter(move |&a| self.contains(a))
    }
}

impl From<Action> for Actions {
    fn from(action: Action) -> Actions {
        Actions(1 << action as u16)
    }
}

impl FromIterator<Action> for Actions {
    fn from_iter<I: IntoIterator<Item = Action>>(iter: I) -> Actions {
        let mut set = Actions::NONE;
        for a in iter {
            set.insert(a);
        }
        set
    }
}

impl BitOr for Actions {
    type Output = Actions;
    fn bitor(self, rhs: Actions) -> Actions {
        Actions(self.0 | rhs.0)
    }
}

impl BitAnd for Actions {
    type Output = Actions;
    fn bitand(self, rhs: Actions) -> Actions {
        Actions(self.0 & rhs.0)
    }
}

impl Not for Actions {
    type Output = Actions;
    fn not(self) -> Actions {
        Actions(!self.0)
    }
}

/// Trace notation: codes in `LRUDJASPT` order, or `.` for nothing.
impl fmt::Display for Actions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str(".");
        }
        self.iter().try_for_each(|a| write!(f, "{}", a.code()))
    }
}

/// One player's input for one logic frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub held: Actions,
    /// Held this frame but not the one before.
    pub pressed: Actions,
    /// Held the frame before but not this one.
    pub released: Actions,
}

impl Frame {
    /// The frame that follows one where `previous` was held. Pressed and
    /// released depend only on held actions, so a recorded stream of held
    /// actions replays exactly.
    pub fn after(previous: Actions, held: Actions) -> Frame {
        Frame {
            held,
            pressed: held & !previous,
            released: previous & !held,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_follow_scenario_order() {
        let codes: String = Action::ALL.iter().map(|a| a.code()).collect();
        assert_eq!(codes, "LRUDJASPT");
        for (i, a) in Action::ALL.iter().enumerate() {
            assert_eq!(*a as usize, i);
        }
    }

    #[test]
    fn displays_like_a_trace() {
        let set: Actions = [Action::Jump, Action::Right].into_iter().collect();
        assert_eq!(set.to_string(), "RJ");
        assert_eq!(set.bits(), 0b10010);
        assert_eq!(Actions::NONE.to_string(), ".");
    }

    #[test]
    fn edges_from_held() {
        let r = Actions::from(Action::Right);
        let rj = r | Action::Jump.into();
        let f = Frame::after(r, rj);
        assert_eq!(f.pressed, Action::Jump.into());
        assert!(f.released.is_empty());
        let f = Frame::after(rj, r);
        assert_eq!(f.released, Action::Jump.into());
        assert!(f.pressed.is_empty());
    }
}
