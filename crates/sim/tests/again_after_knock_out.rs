//! What is there again after a knock-out on `classic-1`
//! (spec/behavior/restart-barrel.md, "What is there again after a
//! knock-out").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{FlowEvent, InputFrame, Phase, World, generic_feel, step};

/// The player stands at x = 200 on flat ground at y = 400.
const X: i32 = 200;
const F: i32 = 400;

/// Where the things are, in entity order.
const BUNCH: usize = 0;
const LETTER: usize = 1;
const FIGURE: usize = 2;
const BARREL: usize = 3;
const SECOND_LETTER: usize = 4;
const WALKER: usize = 5;

/// A level with a bunch, the first letter and a figure where the player
/// stands, a restart barrel at `barrel` with the second letter just
/// right of it, and a still walker at 350.
fn world(barrel: i32) -> World {
    let src = format!(
        r#"
        format = 1
        name = "again"
        size = [3000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 3000, 200]

        [[entities]]
        type = "cluster"
        at = [{X}, {y}]

        [[entities]]
        type = "letter"
        at = [{X}, {y}]
        params = {{ letter = 1 }}

        [[entities]]
        type = "idol"
        at = [{X}, {y}]

        [[entities]]
        type = "keg"
        at = [{barrel}, {}]

        [[entities]]
        type = "letter"
        at = [{}, {y}]
        params = {{ letter = 2 }}

        [[entities]]
        type = "crawler"
        at = [350, {F}]
        params = {{ speed = 0 }}
        "#,
        F - 9,
        barrel + 20,
        y = F - 16,
    );
    let level = Arc::new(Level::from_toml_str(&src, "again").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

fn hold(world: &mut World, keys: &[Action], steps: u32) -> Vec<FlowEvent> {
    let held: Actions = keys.iter().copied().collect();
    let mut events = Vec::new();
    for _ in 0..steps {
        step(world, &InputFrame::single(Frame::after(held, held)));
        events.extend(world.flow.events.iter().copied());
    }
    events
}

/// Takes what is where the player stands, walks right into the walker,
/// and waits for play to come back after the knock-out. Returns the
/// world as it was just before the hit.
fn knocked_out(world: &mut World) -> World {
    knocked_out_waiting(world, 40)
}

/// [`knocked_out`], waiting `wait` steps after taking the second letter
/// on the way.
fn knocked_out_waiting(world: &mut World, wait: u32) -> World {
    hold(world, &[], 60);
    let flow = &world.flow;
    assert_eq!(flow.relics, 1);
    assert_eq!(flow.letters & 1, 1);
    for i in [BUNCH, LETTER, FIGURE] {
        assert!(!world.things[i].active, "thing {i} taken");
    }
    assert!(world.things[WALKER].active);
    let mut before = world.clone();
    let mut waited = false;
    for _ in 0..200 {
        if !waited && !world.things[SECOND_LETTER].active {
            waited = true;
            hold(world, &[], wait);
        }
        before = world.clone();
        if hold(world, &[Action::Right], 1).contains(&FlowEvent::Die) {
            break;
        }
    }
    assert!(world.player.knockout.is_some(), "knocked out");
    for _ in 0..600 {
        if hold(world, &[], 1).contains(&FlowEvent::Respawn) {
            return before;
        }
    }
    panic!("never came back");
}

#[test]
fn at_the_level_s_start_everything_is_there_again_and_no_letters_are_taken() {
    let mut w = world(2000);
    let before = knocked_out(&mut w);
    assert_eq!(w.player.pos, w.level.start);
    let flow = &w.flow;
    // The counts are kept, the letters taken aren't.
    assert_eq!(flow.lives, before.flow.lives - 1);
    assert_eq!(flow.gems, before.flow.gems);
    assert!(flow.gems > 0);
    assert_eq!(flow.relics, 1);
    assert_eq!(flow.letters, 0);
    for i in [BUNCH, LETTER, FIGURE, WALKER] {
        let thing = &w.things[i];
        assert!(thing.active, "thing {i} there again");
        assert_eq!(thing.phase, Phase::Ready, "thing {i}");
        assert_eq!(thing.pos, w.level.entities[i].at, "thing {i}");
    }
    // Taking them again gives what they gave the first time.
    hold(&mut w, &[], 60);
    assert_eq!(w.flow.relics, 2);
    assert_eq!(w.flow.letters, 1);
    assert!(w.flow.gems > before.flow.gems);
}

#[test]
fn at_the_restart_point_the_letters_taken_stay_gone() {
    let mut w = world(300);
    let before = knocked_out(&mut w);
    assert_eq!(before.flow.checkpoints, [BARREL]);
    assert_eq!(before.flow.letters, 0b11, "both letters taken");
    assert_eq!(w.player.pos, Vec2::ints(300, F - 9));
    let flow = &w.flow;
    assert_eq!(flow.letters, 0b11);
    assert_eq!(flow.relics, 1);
    assert_eq!(flow.checkpoints, [BARREL]);
    for i in [LETTER, SECOND_LETTER] {
        assert!(!w.things[i].active, "letter {i} still gone");
    }
    // The barrel stays broken; the rest is there again, left of the
    // restart point too.
    assert_eq!(w.things[BARREL].phase, Phase::Broken);
    for i in [BUNCH, FIGURE, WALKER] {
        let thing = &w.things[i];
        assert!(thing.active, "thing {i} there again");
        assert_eq!(thing.phase, Phase::Ready, "thing {i}");
    }
}

#[test]
fn a_letter_taken_just_before_the_hit_joins_during_the_knock_out_and_stays_gone() {
    let mut w = world(300);
    let before = knocked_out_waiting(&mut w, 0);
    assert_eq!(before.flow.letters, 0b01);
    assert!(!before.things[SECOND_LETTER].active);
    assert_eq!(w.flow.letters, 0b11);
    assert!(!w.things[LETTER].active);
    assert!(!w.things[SECOND_LETTER].active);
}

#[test]
fn other_presets_keep_things_as_they_were() {
    let src = format!(
        r#"
        format = 1
        name = "again"
        size = [3000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 3000, 200]

        [[entities]]
        type = "gem"
        at = [{X}, {F}]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "again").unwrap());
    let mut w = World::new(level, Arc::new(generic_feel("default").unwrap()));
    hold(&mut w, &[], 5);
    assert!(!w.things[0].active);
    w.player.pos.y = clamber_sim::Fixed::from_int(F + 900);
    w.player.ground = None;
    assert!(hold(&mut w, &[], 300).contains(&FlowEvent::Respawn));
    assert!(!w.things[0].active, "still taken");
}
