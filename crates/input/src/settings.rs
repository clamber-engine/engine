//! Input settings: per-player bindings and a few tuning values, stored as
//! TOML. The built-in defaults are `content/base/input.toml`; a player's
//! settings file overrides only what it lists.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Action, Device, Key, PadInput};

/// Local players the engine supports at once.
pub const MAX_PLAYERS: usize = 2;

const BUILTIN: &str = include_str!("../../../content/base/input.toml");

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Settings {
    /// Frames a jump press stays buffered after the frame it happened on
    /// (see [`JumpBuffer`](crate::JumpBuffer)). 0 means no buffering.
    pub jump_buffer_frames: u8,
    /// How far a stick must be pushed, from 0 to 1, before its direction
    /// counts as held.
    pub stick_dead_zone: f32,
    /// One entry per player slot, [`MAX_PLAYERS`] of them.
    #[serde(rename = "player")]
    pub players: Vec<PlayerBindings>,
}

/// One player's bindings. An action can have several inputs on each
/// device; an input belongs to at most one action.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerBindings {
    pub keys: BTreeMap<Action, Vec<Key>>,
    pub pad: BTreeMap<Action, Vec<PadInput>>,
}

impl PlayerBindings {
    pub fn key_action(&self, key: Key) -> Option<Action> {
        find(&self.keys, key)
    }

    pub fn pad_action(&self, input: PadInput) -> Option<Action> {
        find(&self.pad, input)
    }

    /// Inputs bound to `action` on `device`, by settings-file name.
    pub fn names(&self, action: Action, device: Device) -> Vec<&'static str> {
        match device {
            Device::Keyboard => names(&self.keys, action, Key::name),
            Device::Pad => names(&self.pad, action, PadInput::name),
        }
    }
}

fn find<T: PartialEq>(map: &BTreeMap<Action, Vec<T>>, input: T) -> Option<Action> {
    map.iter()
        .find(|(_, inputs)| inputs.contains(&input))
        .map(|(&a, _)| a)
}

fn names<T: Copy>(
    map: &BTreeMap<Action, Vec<T>>,
    action: Action,
    name: fn(T) -> &'static str,
) -> Vec<&'static str> {
    map.get(&action)
        .map_or_else(Vec::new, |v| v.iter().map(|&i| name(i)).collect())
}

/// A settings file as written: everything optional.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    jump_buffer_frames: Option<u8>,
    stick_dead_zone: Option<f32>,
    player: Option<Vec<PlayerFile>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerFile {
    keys: Option<BTreeMap<Action, Vec<Key>>>,
    pad: Option<BTreeMap<Action, Vec<PadInput>>>,
}

impl Settings {
    /// The engine's defaults.
    pub fn builtin() -> Settings {
        let empty = Settings {
            jump_buffer_frames: 0,
            stick_dead_zone: 0.5,
            players: vec![PlayerBindings::default(); MAX_PLAYERS],
        };
        empty
            .overlay(BUILTIN, "content/base/input.toml")
            .expect("built-in input settings are valid")
    }

    /// Reads a settings file on top of the built-in defaults.
    pub fn from_toml_str(src: &str, origin: &str) -> Result<Settings, SettingsError> {
        Settings::builtin().overlay(src, origin)
    }

    /// These settings with a file's values applied. A player's `keys` or
    /// `pad` table, when present, replaces that device's bindings entirely;
    /// an action it leaves out is unbound.
    pub fn overlay(&self, src: &str, origin: &str) -> Result<Settings, SettingsError> {
        let err = |message: String| SettingsError {
            origin: origin.to_string(),
            message,
        };
        let file: File = toml::from_str(src).map_err(|e| err(e.to_string().trim_end().into()))?;

        let mut out = self.clone();
        if let Some(frames) = file.jump_buffer_frames {
            out.jump_buffer_frames = frames;
        }
        if let Some(dz) = file.stick_dead_zone {
            if !(dz > 0.0 && dz < 1.0) {
                return Err(err(format!("stick_dead_zone: {dz} is not between 0 and 1")));
            }
            out.stick_dead_zone = dz;
        }
        let players = file.player.unwrap_or_default();
        if players.len() > MAX_PLAYERS {
            return Err(err(format!(
                "player: {} players listed; at most {MAX_PLAYERS} are supported",
                players.len()
            )));
        }
        for (i, p) in players.into_iter().enumerate() {
            let slot = &mut out.players[i];
            if let Some(keys) = p.keys {
                check_unique(&keys, Key::name)
                    .map_err(|m| err(format!("player[{i}].keys: {m}")))?;
                slot.keys = keys;
            }
            if let Some(pad) = p.pad {
                check_unique(&pad, PadInput::name)
                    .map_err(|m| err(format!("player[{i}].pad: {m}")))?;
                slot.pad = pad;
            }
        }
        Ok(out)
    }

    /// The full settings as a file that [`Settings::from_toml_str`] reads
    /// back unchanged.
    pub fn to_toml_string(&self) -> String {
        toml::to_string(self).expect("input settings serialize")
    }
}

fn check_unique<T: Copy + PartialEq>(
    map: &BTreeMap<Action, Vec<T>>,
    name: fn(T) -> &'static str,
) -> Result<(), String> {
    let mut seen: Vec<(T, Action)> = Vec::new();
    for (&action, inputs) in map {
        for &input in inputs {
            if let Some(&(_, other)) = seen.iter().find(|(i, _)| *i == input) {
                return Err(if other == action {
                    format!("`{}` is listed twice for {action}", name(input))
                } else {
                    format!("`{}` is bound to both {other} and {action}", name(input))
                });
            }
            seen.push((input, action));
        }
    }
    Ok(())
}

/// A settings file that couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsError {
    /// Where the text came from: a file path, or a name given by the caller.
    pub origin: String,
    pub message: String,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.origin, self.message)
    }
}

impl std::error::Error for SettingsError {}
