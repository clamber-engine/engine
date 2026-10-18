use clamber_input::{
    Action, Actions, Axis, Device, Event, Input, Key, PadId, PadInput, PadStyle, Settings,
};

fn key(key: Key, down: bool) -> Event {
    Event::Key { key, down }
}

fn button(pad: usize, input: PadInput, down: bool) -> Event {
    Event::PadButton {
        pad: PadId(pad),
        input,
        down,
    }
}

fn acts(list: &[Action]) -> Actions {
    list.iter().copied().collect()
}

fn settings_err(src: &str) -> String {
    Settings::from_toml_str(src, "input.toml")
        .unwrap_err()
        .to_string()
}

#[test]
fn builtin_binds_every_action_for_both_players() {
    let s = Settings::builtin();
    assert_eq!(s.players.len(), 2);
    for p in &s.players {
        for a in Action::ALL {
            assert!(!p.names(a, Device::Keyboard).is_empty(), "{a} key");
            assert!(!p.names(a, Device::Pad).is_empty(), "{a} pad");
        }
    }
    assert_eq!(s.players[0].key_action(Key::KeyZ), Some(Action::Jump));
}

#[test]
fn builtin_pad_layout_is_the_classic_titles() {
    // spec/behavior/controls.md: the same layout in both titles.
    let layout = [
        (PadInput::DPadLeft, Some(Action::Left)),
        (PadInput::DPadRight, Some(Action::Right)),
        (PadInput::DPadUp, Some(Action::Up)),
        (PadInput::DPadDown, Some(Action::Down)),
        (PadInput::South, Some(Action::Jump)),
        (PadInput::West, Some(Action::Action)),
        (PadInput::East, Some(Action::Tandem)),
        (PadInput::North, None),
        (PadInput::LeftShoulder, None),
        (PadInput::RightShoulder, None),
        (PadInput::Start, Some(Action::Pause)),
        (PadInput::Select, Some(Action::Swap)),
    ];
    for p in &Settings::builtin().players {
        for (input, action) in layout {
            assert_eq!(p.pad_action(input), action, "{input:?}");
        }
    }
}

#[test]
fn settings_file_overrides_only_what_it_lists() {
    let s = Settings::from_toml_str(
        "jump_buffer_frames = 0\n[[player]]\n[player.keys]\njump = [\"KeyK\"]\n",
        "input.toml",
    )
    .unwrap();
    let builtin = Settings::builtin();
    assert_eq!(s.jump_buffer_frames, 0);
    assert_eq!(s.stick_dead_zone, builtin.stick_dead_zone);
    // the keys table replaces player 1's keys; the pad table is untouched
    assert_eq!(s.players[0].key_action(Key::KeyK), Some(Action::Jump));
    assert_eq!(s.players[0].key_action(Key::ArrowLeft), None);
    assert_eq!(s.players[0].pad, builtin.players[0].pad);
    assert_eq!(s.players[1], builtin.players[1]);
}

#[test]
fn settings_round_trip() {
    let mut s = Settings::builtin();
    s.jump_buffer_frames = 7;
    s.players[1].pad.remove(&Action::Swap);
    let text = s.to_toml_string();
    assert_eq!(Settings::from_toml_str(&text, "saved").unwrap(), s);
}

#[test]
fn settings_errors_name_the_problem() {
    assert!(settings_err("[[player]]\n[player.keys]\nfly = [\"KeyF\"]\n").contains("fly"));
    assert!(settings_err("[[player]]\n[player.keys]\njump = [\"KeyQQ\"]\n").contains("KeyQQ"));
    assert!(settings_err("unknown = 1\n").contains("unknown"));
    assert_eq!(
        settings_err("[[player]]\n[player.pad]\njump = [\"South\"]\naction = [\"South\"]\n"),
        "input.toml: player[0].pad: `South` is bound to both jump and action"
    );
    assert_eq!(
        settings_err("[[player]]\n[[player]]\n[[player]]\n"),
        "input.toml: player: 3 players listed; at most 2 are supported"
    );
    assert_eq!(
        settings_err("stick_dead_zone = 1.5\n"),
        "input.toml: stick_dead_zone: 1.5 is not between 0 and 1"
    );
}

#[test]
fn keyboard_held_pressed_released() {
    let mut input = Input::new(Settings::builtin());
    input.handle(key(Key::ArrowRight, true));
    let [f, _] = input.tick();
    assert_eq!(f.held, Action::Right.into());
    assert_eq!(f.pressed, Action::Right.into());

    input.handle(key(Key::KeyZ, true));
    let [f, _] = input.tick();
    assert_eq!(f.held, acts(&[Action::Right, Action::Jump]));
    assert_eq!(f.pressed, Action::Jump.into());

    input.handle(key(Key::ArrowRight, false));
    let [f, _] = input.tick();
    assert_eq!(f.held, Action::Jump.into());
    assert_eq!(f.released, Action::Right.into());
}

#[test]
fn tap_between_frames_counts_for_one_frame() {
    let mut input = Input::new(Settings::builtin());
    input.handle(key(Key::KeyZ, true));
    input.handle(key(Key::KeyZ, false));
    let [f, _] = input.tick();
    assert_eq!(f.pressed, Action::Jump.into());
    let [f, _] = input.tick();
    assert!(f.held.is_empty());
    assert_eq!(f.released, Action::Jump.into());
}

#[test]
fn second_player_only_when_active() {
    let mut input = Input::new(Settings::builtin());
    input.handle(key(Key::KeyD, true));
    assert!(input.tick()[1].held.is_empty());
    input.set_active_players(2);
    assert_eq!(input.tick()[1].held, Action::Right.into());
    assert!(input.tick()[0].held.is_empty());
}

#[test]
fn pads_assigned_in_connection_order() {
    let mut input = Input::new(Settings::builtin());
    input.set_active_players(2);
    input.handle(Event::PadConnected(PadId(7)));
    input.handle(Event::PadConnected(PadId(3)));
    assert_eq!(input.pad_owner(0), Some(PadId(7)));
    assert_eq!(input.pad_owner(1), Some(PadId(3)));

    input.handle(button(3, PadInput::South, true));
    let [p1, p2] = input.tick();
    assert!(p1.held.is_empty());
    assert_eq!(p2.held, Action::Jump.into());
}

#[test]
fn one_player_uses_every_pad() {
    let mut input = Input::new(Settings::builtin());
    input.handle(Event::PadConnected(PadId(0)));
    input.handle(Event::PadConnected(PadId(1)));
    input.handle(button(1, PadInput::West, true));
    assert_eq!(input.tick()[0].held, Action::Action.into());
}

#[test]
fn hot_plug_frees_and_reclaims_slots() {
    let mut input = Input::new(Settings::builtin());
    input.set_active_players(2);
    for id in [0, 1, 2] {
        input.handle(Event::PadConnected(PadId(id)));
    }
    input.handle(button(0, PadInput::DPadRight, true));
    assert_eq!(input.tick()[0].held, Action::Right.into());

    input.handle(Event::PadDisconnected(PadId(0)));
    assert_eq!(input.pad_owner(0), None);
    // the unplugged pad's held input is gone
    assert_eq!(input.tick()[0].released, Action::Right.into());

    // the waiting third pad takes the free slot when it's used
    input.handle(button(2, PadInput::South, true));
    assert_eq!(input.pad_owner(0), Some(PadId(2)));
    assert_eq!(input.tick()[0].held, Action::Jump.into());

    // a pad that reports input without a connect event is still seen
    let mut input = Input::new(Settings::builtin());
    input.handle(button(9, PadInput::Start, true));
    assert_eq!(input.pad_owner(0), Some(PadId(9)));
    assert_eq!(input.tick()[0].held, Action::Pause.into());
}

#[test]
fn stick_directions_use_dead_zone() {
    let mut input = Input::new(Settings::builtin());
    let axis = |value| Event::PadAxis {
        pad: PadId(0),
        axis: Axis::LeftStickX,
        value,
    };
    input.handle(axis(-0.3));
    assert!(input.tick()[0].held.is_empty());
    input.handle(axis(-0.8));
    assert_eq!(input.tick()[0].held, Action::Left.into());
    input.handle(axis(0.9));
    let [f, _] = input.tick();
    assert_eq!(f.held, Action::Right.into());
    assert_eq!(f.released, Action::Left.into());
    input.handle(Event::PadAxis {
        pad: PadId(0),
        axis: Axis::LeftStickY,
        value: 1.0,
    });
    assert_eq!(input.tick()[0].held, acts(&[Action::Right, Action::Up]));
}

#[test]
fn remap_key_swaps_with_conflicting_action() {
    let mut input = Input::new(Settings::builtin());
    input.begin_remap(0, Action::Jump, Device::Keyboard);
    // the press that completes a remap isn't game input
    let done = input.handle(key(Key::KeyX, true)).unwrap();
    assert_eq!(done.input, "KeyX");
    assert_eq!(done.swapped_with, Some(Action::Action));
    assert!(input.remapping().is_none());
    assert!(input.tick()[0].held.is_empty());

    let p = &input.settings().players[0];
    assert_eq!(p.names(Action::Jump, Device::Keyboard), ["KeyX"]);
    assert_eq!(p.names(Action::Action, Device::Keyboard), ["KeyZ", "Space"]);

    // after release the new binding works
    input.handle(key(Key::KeyX, false));
    input.tick();
    input.handle(key(Key::KeyX, true));
    assert_eq!(input.tick()[0].held, Action::Jump.into());
}

#[test]
fn remap_takes_input_from_action_with_others() {
    let mut input = Input::new(Settings::builtin());
    input.begin_remap(0, Action::Swap, Device::Keyboard);
    let done = input.handle(key(Key::Space, true)).unwrap();
    assert_eq!(done.swapped_with, None);
    let p = &input.settings().players[0];
    assert_eq!(p.names(Action::Swap, Device::Keyboard), ["Space"]);
    assert_eq!(p.names(Action::Jump, Device::Keyboard), ["KeyZ"]);
    // the old swap key is now unbound
    assert_eq!(p.key_action(Key::KeyC), None);
}

#[test]
fn remap_pad_waits_for_that_device() {
    let mut input = Input::new(Settings::builtin());
    input.set_active_players(2);
    input.handle(Event::PadConnected(PadId(0)));
    input.handle(Event::PadConnected(PadId(1)));
    input.begin_remap(1, Action::Swap, Device::Pad);

    // keys and the other player's pad don't complete it
    assert!(input.handle(key(Key::KeyH, true)).is_none());
    assert!(
        input
            .handle(button(0, PadInput::RightTrigger, true))
            .is_none()
    );
    let done = input
        .handle(button(1, PadInput::RightTrigger, true))
        .unwrap();
    assert_eq!(done.remap.player, 1);
    assert_eq!(
        input.settings().players[1].pad_action(PadInput::RightTrigger),
        Some(Action::Swap)
    );
    assert_eq!(
        input.settings().players[0].pad_action(PadInput::RightTrigger),
        None
    );
}

#[test]
fn cancelled_remap_changes_nothing() {
    let mut input = Input::new(Settings::builtin());
    input.begin_remap(0, Action::Jump, Device::Keyboard);
    input.cancel_remap();
    assert!(input.handle(key(Key::KeyX, true)).is_none());
    assert_eq!(input.settings(), &Settings::builtin());
    assert_eq!(input.tick()[0].held, Action::Action.into());
}

#[test]
fn pad_style_follows_the_pad_in_use() {
    let mut input = Input::new(Settings::builtin());
    assert_eq!(input.pad_style(0), PadStyle::Letters, "no pad");
    input.handle(Event::PadConnected(PadId(0)));
    input.handle(Event::PadStyle {
        pad: PadId(0),
        style: PadStyle::Shapes,
    });
    input.handle(Event::PadConnected(PadId(1)));
    assert_eq!(input.pad_style(0), PadStyle::Shapes, "the first connected");
    input.handle(Event::PadButton {
        pad: PadId(1),
        input: PadInput::South,
        down: true,
    });
    assert_eq!(input.pad_style(0), PadStyle::Letters, "the one used last");

    input.set_active_players(2);
    assert_eq!(input.pad_style(0), PadStyle::Shapes, "each player's own");
    assert_eq!(input.pad_style(1), PadStyle::Letters);
    input.handle(Event::PadDisconnected(PadId(0)));
    assert_eq!(input.pad_style(0), PadStyle::Letters);
}
