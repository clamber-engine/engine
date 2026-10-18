//! Player input for the Clamber engine.
//!
//! Devices report [`Event`]s; [`Input`] turns them into each player's
//! neutral [`Action`]s once per logic frame. The simulation only ever sees
//! [`Frame`]s, which depend on held actions alone, so recorded input
//! replays exactly. Its [`JumpBuffer`] works from those frames too.
//!
//! Menus have fixed controls of their own ([`Input::menu`]), apart from
//! the game's bindings.
//!
//! Bindings are per player and remappable ([`Input::begin_remap`]); they
//! are stored as TOML ([`Settings`]), with the defaults in
//! `content/base/input.toml`.
//!
//! Gamepads come through gilrs with the `gamepad` feature ([`Gamepads`]).
//! Keyboard events come from whatever owns the window, which translates
//! its key codes to [`Key`].

mod action;
mod buffer;
mod device;
#[cfg(feature = "gamepad")]
mod gamepad;
mod router;
mod settings;

pub use action::{Action, Actions, Frame};
pub use buffer::JumpBuffer;
pub use device::{Axis, Device, Event, Key, PadId, PadInput, PadStyle};
#[cfg(feature = "gamepad")]
pub use gamepad::Gamepads;
pub use router::{Input, Remap, Remapped, menu_action_for_key, menu_action_for_pad, menu_inputs};
pub use settings::{MAX_PLAYERS, PlayerBindings, Settings, SettingsError};
