//! Which sound device to play on: the one the player picked, else the
//! system's default, except when a game controller takes the default over.
//!
//! Many controllers are sound devices too, and the system often makes one
//! the default when it's plugged in, which would move the game's sound to
//! a headset jack nobody is wearing. The policy is pure: the output feeds
//! it what it sees and plays on the device it names.

use std::collections::BTreeSet;

/// How long after a gamepad appears a switch of the default to a
/// controller's sound device counts as the controller's doing, in
/// milliseconds.
const PAD_GRACE: u64 = 5000;

/// Words in a device's name that mark it as a game controller's.
const CONTROLLER_WORDS: [&str; 7] = [
    "controller",
    "gamepad",
    "xbox",
    "dualshock",
    "dualsense",
    "joy-con",
    "wireless controller",
];

/// Whether a sound device's name is that of a game controller's.
pub fn is_controller_device(name: &str) -> bool {
    let name = name.to_lowercase();
    CONTROLLER_WORDS.iter().any(|w| name.contains(w))
}

/// What the policy decided on a look at the devices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decision {
    /// The device to play on, or none while there's none to play on.
    pub device: Option<String>,
    /// Why, when it's news: the device chosen or a switch skipped.
    pub note: Option<String>,
}

#[derive(Debug, Default)]
pub struct Policy {
    /// The device the player picked, if they picked one.
    preferred: Option<String>,
    /// The device playing, and whether it's there because it was picked.
    current: Option<String>,
    picked: bool,
    /// What the last look saw.
    default: Option<String>,
    present: BTreeSet<String>,
    /// The last time a gamepad appeared, in milliseconds.
    pad_at: Option<u64>,
    /// The default a switch to was last skipped for, so it's said once.
    skipped: Option<String>,
}

impl Policy {
    /// Plays on `preferred` when it's there, else the default.
    pub fn new(preferred: Option<String>) -> Policy {
        Policy {
            preferred,
            ..Policy::default()
        }
    }

    /// The player picked `device`, or the system default with none.
    pub fn prefer(&mut self, device: Option<String>) {
        self.preferred = device;
    }

    /// A gamepad appeared at `now` milliseconds.
    pub fn gamepad_connected(&mut self, now: u64) {
        self.pad_at = Some(now);
    }

    /// The device to play on given the system's `default` and the output
    /// devices `present` at `now` milliseconds.
    pub fn decide(&mut self, now: u64, default: Option<&str>, present: &[String]) -> Decision {
        let appeared = |name: &str| !self.present.contains(name);
        let pad_recent = self
            .pad_at
            .is_some_and(|t| now.saturating_sub(t) <= PAD_GRACE);
        let here = |name: &str| present.iter().any(|p| p == name);
        let default_changed = default != self.default.as_deref();
        let mut note = None;
        let target: Option<String>;
        let mut picked = false;

        let preferred = self.preferred.clone().filter(|p| here(p));
        if let Some(p) = preferred {
            picked = true;
            target = Some(p);
        } else {
            let current = self.current.clone().filter(|c| here(c));
            target = match (current, default) {
                (None, d) => d.map(str::to_string),
                (Some(c), None) => Some(c),
                (Some(c), Some(d)) if c == d => Some(c),
                // the player just turned a picked device off
                (Some(_), Some(d)) if self.picked => Some(d.to_string()),
                // an earlier decision on this default stands
                (Some(c), Some(_)) if !default_changed => Some(c),
                (Some(c), Some(d)) => {
                    let automatic = appeared(d) || pad_recent;
                    if is_controller_device(d) && automatic {
                        if self.skipped.as_deref() != Some(d) {
                            note = Some(format!(
                                "staying on {c:?}: the default moved to {d:?}, a game controller's"
                            ));
                            self.skipped = Some(d.to_string());
                        }
                        Some(c)
                    } else {
                        Some(d.to_string())
                    }
                }
            };
        }
        if target != self.current
            && let Some(t) = &target
        {
            {
                let why = if picked {
                    "picked"
                } else if self.preferred.is_some() {
                    "the picked device isn't there; the default"
                } else {
                    "the default"
                };
                note = Some(format!("playing on {t:?} ({why})"));
            }
        }
        if default.is_none_or(|d| target.as_deref() == Some(d)) {
            self.skipped = None;
        }
        self.current = target.clone();
        self.picked = picked;
        self.default = default.map(str::to_string);
        self.present = present.iter().cloned().collect();
        Decision {
            device: target,
            note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEAKERS: &str = "Speakers";
    const HEADSET: &str = "Headphones";
    const PAD: &str = "Xbox Wireless Controller";

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn device(d: &Decision) -> Option<&str> {
        d.device.as_deref()
    }

    #[test]
    fn controller_names_are_recognised() {
        assert!(is_controller_device("Headset (Xbox Wireless Controller)"));
        assert!(is_controller_device("Wireless Controller"));
        assert!(is_controller_device("DualSense Speaker"));
        assert!(!is_controller_device("Speakers (Realtek Audio)"));
    }

    #[test]
    fn started_on_the_default_even_a_controllers() {
        let mut p = Policy::new(None);
        let d = p.decide(0, Some(PAD), &names(&[PAD, SPEAKERS]));
        assert_eq!(device(&d), Some(PAD));
        let mut p = Policy::new(None);
        let d = p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS]));
        assert_eq!(device(&d), Some(SPEAKERS));
    }

    #[test]
    fn nothing_to_play_on_names_no_device() {
        let mut p = Policy::new(None);
        assert_eq!(p.decide(0, None, &[]).device, None);
        let d = p.decide(500, Some(SPEAKERS), &names(&[SPEAKERS]));
        assert_eq!(device(&d), Some(SPEAKERS));
    }

    #[test]
    fn a_controller_appearing_and_taking_the_default_is_ignored() {
        let mut p = Policy::new(None);
        p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS]));
        // the device appears together with the default change
        let d = p.decide(500, Some(PAD), &names(&[SPEAKERS, PAD]));
        assert_eq!(device(&d), Some(SPEAKERS));
        assert!(d.note.unwrap().contains("game controller"));
        // and later looks keep to the decision, without saying it again
        let d = p.decide(20_000, Some(PAD), &names(&[SPEAKERS, PAD]));
        assert_eq!(device(&d), Some(SPEAKERS));
        assert_eq!(d.note, None);
    }

    #[test]
    fn a_gamepad_appearing_marks_the_switch_as_automatic() {
        let mut p = Policy::new(None);
        // the controller's sound device was there all along
        p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS, PAD]));
        p.gamepad_connected(1000);
        let d = p.decide(1500, Some(PAD), &names(&[SPEAKERS, PAD]));
        assert_eq!(device(&d), Some(SPEAKERS));
    }

    #[test]
    fn the_player_switching_to_an_existing_controller_device_is_followed() {
        let mut p = Policy::new(None);
        p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS, PAD]));
        let d = p.decide(60_000, Some(PAD), &names(&[SPEAKERS, PAD]));
        assert_eq!(device(&d), Some(PAD));
    }

    #[test]
    fn other_default_changes_are_followed() {
        let mut p = Policy::new(None);
        p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS]));
        let d = p.decide(500, Some(HEADSET), &names(&[SPEAKERS, HEADSET]));
        assert_eq!(device(&d), Some(HEADSET));
        let d = p.decide(1000, Some(SPEAKERS), &names(&[SPEAKERS]));
        assert_eq!(device(&d), Some(SPEAKERS));
    }

    #[test]
    fn a_controller_default_is_followed_when_the_current_device_is_gone() {
        let mut p = Policy::new(None);
        p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS]));
        let d = p.decide(500, Some(PAD), &names(&[PAD]));
        assert_eq!(device(&d), Some(PAD));
    }

    #[test]
    fn a_picked_device_is_used_even_a_controllers() {
        let mut p = Policy::new(Some(PAD.into()));
        let d = p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS, PAD]));
        assert_eq!(device(&d), Some(PAD));
        p.gamepad_connected(100);
        let d = p.decide(500, Some(SPEAKERS), &names(&[SPEAKERS, PAD]));
        assert_eq!(device(&d), Some(PAD));
        assert_eq!(d.note, None);
    }

    #[test]
    fn a_picked_device_missing_then_returning() {
        let mut p = Policy::new(Some(HEADSET.into()));
        // missing at startup: the default
        let d = p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS]));
        assert_eq!(device(&d), Some(SPEAKERS));
        // it returns: back to it
        let d = p.decide(500, Some(SPEAKERS), &names(&[SPEAKERS, HEADSET]));
        assert_eq!(device(&d), Some(HEADSET));
        // goes away while playing: the default
        let d = p.decide(1000, Some(SPEAKERS), &names(&[SPEAKERS]));
        assert_eq!(device(&d), Some(SPEAKERS));
        // and returns again
        let d = p.decide(1500, Some(SPEAKERS), &names(&[SPEAKERS, HEADSET]));
        assert_eq!(device(&d), Some(HEADSET));
    }

    #[test]
    fn a_missing_picked_device_does_not_let_a_controller_take_over() {
        let mut p = Policy::new(Some(HEADSET.into()));
        p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS]));
        let d = p.decide(500, Some(PAD), &names(&[SPEAKERS, PAD]));
        assert_eq!(device(&d), Some(SPEAKERS));
    }

    #[test]
    fn picking_the_system_default_again_follows_the_default() {
        let mut p = Policy::new(Some(HEADSET.into()));
        p.decide(0, Some(SPEAKERS), &names(&[SPEAKERS, HEADSET]));
        p.prefer(None);
        let d = p.decide(500, Some(SPEAKERS), &names(&[SPEAKERS, HEADSET]));
        assert_eq!(device(&d), Some(SPEAKERS));
        assert!(d.note.unwrap().contains("default"));
    }
}
