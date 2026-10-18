//! Coming back to the first level after an exit on `classic-1`
//! (spec/behavior/exits.md, "Coming back to the level").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Actions, Frame};
use clamber_level::{Arrival, Level, Vec2};
use clamber_pack::{CharacterFeel, Feel, Presets};
use clamber_sim::{InputFrame, Phase, World, generic_feel, step};

/// The player stands at x = 200 on flat ground at y = 400.
const X: i32 = 200;
const F: i32 = 400;

/// Where the things are, in entity order.
const SINGLE: usize = 0;
const BUNCH: usize = 1;
const LETTER: usize = 2;
const BARREL: usize = 3;
const WALKER: usize = 4;

/// A level with a single collectible, a bunch, the first letter and a
/// restart barrel all where the player stands, and a small walker off
/// to the right, near where play comes back in most tests.
fn level() -> Arc<Level> {
    let src = format!(
        r#"
        format = 1
        name = "coming-back"
        size = [3000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 3000, 200]

        [[entities]]
        type = "spark"
        at = [{X}, {}]

        [[entities]]
        type = "cluster"
        at = [{X}, {}]

        [[entities]]
        type = "letter"
        at = [{X}, {}]
        params = {{ letter = 1 }}

        [[entities]]
        type = "keg"
        at = [{X}, {F}]

        [[entities]]
        type = "crawler"
        at = [1300, {F}]
        "#,
        F - 16,
        F - 16,
        F - 16,
    );
    Arc::new(Level::from_toml_str(&src, "coming-back").unwrap())
}

fn feel(characters: usize) -> Arc<Feel> {
    let mut feel = generic_feel("classic-1").unwrap();
    for slot in 1..characters {
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", slot).unwrap(),
            abilities: BTreeMap::new(),
        });
    }
    Arc::new(feel)
}

fn idle(world: &mut World, steps: u32) {
    for _ in 0..steps {
        step(
            world,
            &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
        );
    }
}

/// Coming back at `at`, beginning there as the level does.
fn back_at(world: &World, at: i32) -> World {
    let arrival = Arrival {
        name: "back".into(),
        at: Vec2::ints(at, F),
        walk: None,
        entrance: None,
    };
    world.coming_back(&arrival)
}

/// The level after standing among its things long enough for all of
/// them to be taken and counted.
fn taken() -> World {
    let mut w = World::new(level(), feel(1));
    idle(&mut w, 120);
    for i in [SINGLE, BUNCH, LETTER, BARREL] {
        assert!(!w.things[i].active, "thing {i} taken");
    }
    assert_eq!(w.flow.gems, 11);
    assert_eq!(w.flow.letters, 1);
    assert_eq!(w.flow.checkpoints, [BARREL]);
    w
}

#[test]
fn the_count_lives_letters_and_restart_point_are_kept() {
    let mut before = taken();
    before.flow.lives = 7;
    let after = back_at(&before, 1200);
    assert_eq!(after.flow.gems, 11);
    assert_eq!(after.flow.shown, before.flow.shown);
    assert_eq!(after.flow.lives, 7);
    assert_eq!(after.flow.letters, 1);
    assert_eq!(after.flow.checkpoints, [BARREL]);
    assert_eq!(after.restart_point(BARREL), before.restart_point(BARREL));
}

#[test]
fn bunches_letters_and_the_barrel_stay_gone_and_singles_are_there_again() {
    let before = taken();
    let after = back_at(&before, 1200);
    for i in [BUNCH, LETTER] {
        assert!(!after.things[i].active, "thing {i}");
        assert_eq!(after.things[i].phase, Phase::Gone, "thing {i}");
    }
    assert!(!after.things[BARREL].active);
    assert_eq!(after.things[BARREL].phase, Phase::Broken);
    assert!(after.things[SINGLE].active);
    assert_eq!(after.things[SINGLE].phase, Phase::Ready);
}

#[test]
fn back_over_their_places_only_the_single_is_taken_again() {
    let before = taken();
    let mut after = back_at(&before, X);
    idle(&mut after, 120);
    assert!(!after.things[SINGLE].active);
    assert_eq!(after.flow.gems, 12, "one more, for the single");
    assert_eq!(after.flow.letters, 1);
    assert_eq!(after.flow.checkpoints, [BARREL]);
}

#[test]
fn a_bounced_walker_is_there_again() {
    let mut before = World::new(level(), feel(1));
    before.things[WALKER].active = false;
    let after = back_at(&before, 1200);
    assert!(after.things[WALKER].active);
    assert_eq!(after.things[WALKER].phase, Phase::Ready);
}

#[test]
fn single_collectibles_taken_without_coming_back_stay_taken() {
    // The control: going on in the same visit, nothing comes back.
    let mut w = taken();
    idle(&mut w, 60);
    assert!(!w.things[SINGLE].active);
    assert_eq!(w.flow.gems, 11);
}

#[test]
fn the_lead_stays_the_same_and_a_lost_character_stays_lost() {
    let mut both = World::new(level(), feel(2));
    let lead = both.player.character;
    let other = both.other.as_ref().unwrap().character;
    let back = back_at(&both, 1200);
    assert_eq!(back.player.character, lead);
    assert_eq!(back.other.as_ref().unwrap().character, other);

    both.player.character = other;
    both.other.as_mut().unwrap().character = lead;
    let back = back_at(&both, 1200);
    assert_eq!(back.player.character, other);
    assert_eq!(back.other.as_ref().unwrap().character, lead);

    both.other = None;
    assert!(back_at(&both, 1200).other.is_none());
}

/// A room of its own with nothing in it, as the first level's upper exit
/// leads to.
fn room() -> Arc<Level> {
    let src = format!(
        r#"
        format = 1
        name = "room"
        size = [256, 800]
        start = [100, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 256, 200]
        "#
    );
    Arc::new(Level::from_toml_str(&src, "room").unwrap())
}

#[test]
fn another_area_carries_the_count_lives_and_letters_but_not_restart_points() {
    let mut before = taken();
    before.flow.lives = 7;
    let room = World::new(room(), feel(1)).carrying(&before);
    assert_eq!(room.flow.gems, 11);
    assert_eq!(room.flow.shown, before.flow.shown);
    assert_eq!(room.flow.lives, 7);
    assert_eq!(room.flow.letters, 1);
    assert!(room.flow.checkpoints.is_empty());
}

#[test]
fn back_from_another_area_the_level_is_as_it_was_left_with_what_play_carried() {
    let before = taken();
    let mut room = World::new(room(), feel(1)).carrying(&before);
    room.flow.lives = 9;
    room.flow.gems = 12;
    let arrival = Arrival {
        name: "back".into(),
        at: Vec2::ints(1200, F),
        walk: None,
        entrance: None,
    };
    let after = before.coming_back_from(&arrival, &room);
    assert_eq!(after.level.name, "coming-back");
    assert_eq!((after.flow.lives, after.flow.gems), (9, 12));
    assert_eq!(after.flow.checkpoints, [BARREL]);
    assert_eq!(after.things[BUNCH].phase, Phase::Gone);
    assert_eq!(after.things[BARREL].phase, Phase::Broken);
    assert!(after.things[SINGLE].active);
}
