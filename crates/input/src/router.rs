//! Turning device events into per-player actions, once per logic frame.

use std::collections::{BTreeMap, BTreeSet};

use crate::settings::{MAX_PLAYERS, PlayerBindings};
use crate::{Action, Actions, Device, Event, Frame, Key, PadId, PadInput, PadStyle, Settings};

/// Collects device events between logic frames and turns them into each
/// player's [`Frame`].
///
/// The keyboard is shared: every active player's key bindings apply to it.
/// Gamepads are assigned to player slots in the order they connect; with
/// one active player, every connected pad controls player 1.
///
/// An input pressed and released between two logic frames still counts as
/// held for one frame, so quick taps aren't lost.
#[derive(Clone, Debug)]
pub struct Input {
    settings: Settings,
    active_players: usize,
    keys: Latch<Key>,
    pads: BTreeMap<PadId, Latch<PadInput>>,
    owners: [Option<PadId>; MAX_PLAYERS],
    previous: [Actions; MAX_PLAYERS],
    /// The menu input of the last frame.
    menu: Frame,
    remap: Option<Remap>,
    styles: BTreeMap<PadId, PadStyle>,
    /// The pad pressed last.
    last_pad: Option<PadId>,
}

/// A remap waiting for the player to press the new input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remap {
    pub player: usize,
    pub action: Action,
    pub device: Device,
}

/// A finished remap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remapped {
    pub remap: Remap,
    /// The input's settings-file name.
    pub input: &'static str,
    /// When the input belonged to another action that had no other input
    /// on this device, that action now has `remap.action`'s old inputs
    /// instead: the two were swapped.
    pub swapped_with: Option<Action>,
}

/// Down state for one kind of input, plus presses since the last frame.
#[derive(Clone, Debug)]
struct Latch<T> {
    down: BTreeSet<T>,
    pressed: BTreeSet<T>,
    /// Down, but used up by a remap: ignored until released.
    swallowed: BTreeSet<T>,
}

impl<T> Default for Latch<T> {
    fn default() -> Self {
        Latch {
            down: BTreeSet::new(),
            pressed: BTreeSet::new(),
            swallowed: BTreeSet::new(),
        }
    }
}

impl<T: Copy + Ord> Latch<T> {
    /// Records a change; returns true for a new press.
    fn set(&mut self, input: T, down: bool) -> bool {
        if down {
            let new = self.down.insert(input);
            if new {
                self.pressed.insert(input);
            }
            new
        } else {
            self.down.remove(&input);
            self.swallowed.remove(&input);
            false
        }
    }

    fn swallow(&mut self, input: T) {
        self.pressed.remove(&input);
        self.swallowed.insert(input);
    }

    /// Inputs that count as held this frame.
    fn active(&self) -> impl Iterator<Item = T> + '_ {
        self.down
            .union(&self.pressed)
            .filter(|i| !self.swallowed.contains(i))
            .copied()
    }

    fn end_frame(&mut self) {
        self.pressed.clear();
    }
}

impl Input {
    pub fn new(settings: Settings) -> Input {
        Input {
            settings,
            active_players: 1,
            keys: Latch::default(),
            pads: BTreeMap::new(),
            owners: [None; MAX_PLAYERS],
            previous: [Actions::NONE; MAX_PLAYERS],
            menu: Frame::default(),
            remap: None,
            styles: BTreeMap::new(),
            last_pad: None,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Replaces the settings, for example after loading a settings file.
    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    /// Sets how many players are playing, from 1 to [`MAX_PLAYERS`].
    /// Inactive players' frames are always empty.
    pub fn set_active_players(&mut self, players: usize) {
        assert!((1..=MAX_PLAYERS).contains(&players));
        self.active_players = players;
    }

    pub fn active_players(&self) -> usize {
        self.active_players
    }

    /// The gamepad assigned to a player slot.
    pub fn pad_owner(&self, player: usize) -> Option<PadId> {
        self.owners[player]
    }

    /// How the pad `player` uses marks its face buttons: the pad that
    /// drives them, or with one player, the pad used last, or any
    /// connected pad.
    pub fn pad_style(&self, player: usize) -> PadStyle {
        let pad = if self.active_players == 1 {
            self.last_pad
                .filter(|p| self.pads.contains_key(p))
                .or_else(|| self.pads.keys().next().copied())
        } else {
            self.owners[player]
        };
        pad.and_then(|p| self.styles.get(&p).copied())
            .unwrap_or_default()
    }

    /// Records a device event. Returns the finished remap if this event
    /// completed one; that press is then not seen as game input.
    pub fn handle(&mut self, event: Event) -> Option<Remapped> {
        match event {
            Event::Key { key, down } => {
                if self.keys.set(key, down) {
                    return self.try_remap_key(key);
                }
            }
            Event::PadConnected(pad) => {
                self.pads.entry(pad).or_default();
                self.claim_slot(pad);
            }
            Event::PadStyle { pad, style } => {
                self.styles.insert(pad, style);
            }
            Event::PadDisconnected(pad) => {
                self.pads.remove(&pad);
                self.styles.remove(&pad);
                for owner in &mut self.owners {
                    if *owner == Some(pad) {
                        *owner = None;
                    }
                }
            }
            Event::PadButton { pad, input, down } => return self.pad_input(pad, input, down),
            Event::PadAxis { pad, axis, value } => {
                let dz = self.settings.stick_dead_zone;
                let (negative, positive) = axis.directions();
                let a = self.pad_input(pad, negative, value <= -dz);
                let b = self.pad_input(pad, positive, value >= dz);
                return a.or(b);
            }
        }
        None
    }

    fn pad_input(&mut self, pad: PadId, input: PadInput, down: bool) -> Option<Remapped> {
        let pressed = self.pads.entry(pad).or_default().set(input, down);
        if !pressed {
            return None;
        }
        self.last_pad = Some(pad);
        // A pad that connected while every slot was taken claims the first
        // slot that frees up when it's used.
        if !self.owners.contains(&Some(pad)) {
            self.claim_slot(pad);
        }
        self.try_remap_pad(pad, input)
    }

    fn claim_slot(&mut self, pad: PadId) {
        if self.owners.contains(&Some(pad)) {
            return;
        }
        if let Some(slot) = self.owners.iter_mut().find(|o| o.is_none()) {
            *slot = Some(pad);
        }
    }

    /// Whether `pad` drives `player` right now.
    fn controls(&self, pad: PadId, player: usize) -> bool {
        if self.active_players == 1 {
            player == 0
        } else {
            self.owners[player] == Some(pad)
        }
    }

    /// Ends a logic frame: returns each player's input for it and starts
    /// collecting the next.
    pub fn tick(&mut self) -> [Frame; MAX_PLAYERS] {
        let mut frames = [Frame::default(); MAX_PLAYERS];
        for (player, frame) in frames.iter_mut().enumerate() {
            let held = if player < self.active_players {
                self.held(player)
            } else {
                Actions::NONE
            };
            *frame = Frame::after(self.previous[player], held);
            self.previous[player] = held;
        }
        self.menu = Frame::after(self.menu.held, self.menu_held());
        self.keys.end_frame();
        for pad in self.pads.values_mut() {
            pad.end_frame();
        }
        frames
    }

    /// The menu input of the frame [`tick`](Input::tick) last ended.
    ///
    /// Menus have controls of their own, the same for everyone and not
    /// remappable, so changing the game's bindings never changes how
    /// menus work. They come as neutral actions: the directions move,
    /// Jump confirms and Pause goes back. See [`menu_action_for_key`]
    /// and [`menu_action_for_pad`].
    pub fn menu(&self) -> Frame {
        self.menu
    }

    fn menu_held(&self) -> Actions {
        let from_keys = self.keys.active().filter_map(menu_action_for_key);
        let from_pads = self
            .pads
            .values()
            .flat_map(|pad| pad.active())
            .filter_map(menu_action_for_pad);
        from_keys.chain(from_pads).collect()
    }

    fn held(&self, player: usize) -> Actions {
        let bindings = &self.settings.players[player];
        let from_keys = self.keys.active().filter_map(|k| bindings.key_action(k));
        let from_pads = self
            .pads
            .iter()
            .filter(|&(&id, _)| self.controls(id, player))
            .flat_map(|(_, pad)| pad.active())
            .filter_map(|i| bindings.pad_action(i));
        from_keys.chain(from_pads).collect()
    }

    /// Waits for the next press on `device` and binds it to `action` for
    /// `player`, replacing that action's inputs on the device. Replaces any
    /// remap already waiting.
    pub fn begin_remap(&mut self, player: usize, action: Action, device: Device) {
        assert!(player < MAX_PLAYERS);
        self.remap = Some(Remap {
            player,
            action,
            device,
        });
    }

    pub fn cancel_remap(&mut self) {
        self.remap = None;
    }

    pub fn remapping(&self) -> Option<Remap> {
        self.remap
    }

    fn try_remap_key(&mut self, key: Key) -> Option<Remapped> {
        let remap = self.remap.filter(|r| r.device == Device::Keyboard)?;
        self.remap = None;
        self.keys.swallow(key);
        let bindings = &mut self.settings.players[remap.player];
        let swapped_with = bind(&mut bindings.keys, remap.action, key);
        Some(Remapped {
            remap,
            input: key.name(),
            swapped_with,
        })
    }

    fn try_remap_pad(&mut self, pad: PadId, input: PadInput) -> Option<Remapped> {
        let remap = self.remap.filter(|r| r.device == Device::Pad)?;
        // Another player's pad can't remap this player's controls.
        let owner = self.owners.iter().position(|&o| o == Some(pad));
        if owner.is_some_and(|o| o != remap.player) && self.active_players > 1 {
            return None;
        }
        self.remap = None;
        if let Some(p) = self.pads.get_mut(&pad) {
            p.swallow(input);
        }
        let bindings: &mut PlayerBindings = &mut self.settings.players[remap.player];
        let swapped_with = bind(&mut bindings.pad, remap.action, input);
        Some(Remapped {
            remap,
            input: input.name(),
            swapped_with,
        })
    }
}

/// What a key does in menus: the arrow keys move, Enter confirms (as
/// Jump) and Escape goes back (as Pause). Space and every other key do
/// nothing there.
pub fn menu_action_for_key(key: Key) -> Option<Action> {
    Some(match key {
        Key::ArrowLeft => Action::Left,
        Key::ArrowRight => Action::Right,
        Key::ArrowUp => Action::Up,
        Key::ArrowDown => Action::Down,
        Key::Enter | Key::NumpadEnter => Action::Jump,
        Key::Escape => Action::Pause,
        _ => return None,
    })
}

/// What a gamepad control does in menus: the D-pad and left stick move,
/// the bottom face button confirms (as Jump), and the right face button
/// and Start go back (as Pause), so Start closes the pause menu it
/// opened.
pub fn menu_action_for_pad(input: PadInput) -> Option<Action> {
    use PadInput as P;
    Some(match input {
        P::DPadLeft | P::LeftStickLeft => Action::Left,
        P::DPadRight | P::LeftStickRight => Action::Right,
        P::DPadUp | P::LeftStickUp => Action::Up,
        P::DPadDown | P::LeftStickDown => Action::Down,
        P::South => Action::Jump,
        P::East | P::Start => Action::Pause,
        _ => return None,
    })
}

/// The inputs that do `action` in menus on `device`, by settings-file
/// name, for showing them.
pub fn menu_inputs(action: Action, device: Device) -> Vec<&'static str> {
    match device {
        Device::Keyboard => Key::ALL
            .iter()
            .filter(|&&k| menu_action_for_key(k) == Some(action))
            .map(|k| k.name())
            .collect(),
        Device::Pad => PadInput::ALL
            .iter()
            .filter(|&&i| menu_action_for_pad(i) == Some(action))
            .map(|i| i.name())
            .collect(),
    }
}

/// Makes `input` the only input for `action`. If it belonged to an action
/// left with nothing, that action gets `action`'s old inputs; returns it.
fn bind<T: Copy + PartialEq>(
    map: &mut BTreeMap<Action, Vec<T>>,
    action: Action,
    input: T,
) -> Option<Action> {
    let previous = map.insert(action, vec![input]).unwrap_or_default();
    let mut swapped = None;
    for (&other, inputs) in map.iter_mut() {
        if other == action || !inputs.contains(&input) {
            continue;
        }
        inputs.retain(|&i| i != input);
        if inputs.is_empty() {
            *inputs = previous.iter().copied().filter(|&i| i != input).collect();
            swapped = Some(other);
        }
    }
    swapped
}
