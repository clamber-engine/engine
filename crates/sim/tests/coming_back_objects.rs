//! What stays gone and what is there again on coming back to the first
//! level after an exit on `classic-1`: hidden objects, the animal crate,
//! extra-life items, holders, barrels, walkers, the thrower and its ball
//! (spec/behavior/exits.md, "Coming back to the level").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_fixed::Fixed;
use clamber_input::{Action, Actions, Frame};
use clamber_level::{Arrival, Level, Vec2};
use clamber_pack::{CharacterFeel, Feel, Presets};
use clamber_sim::{Flight, InputFrame, Phase, World, generic_feel, step};

/// The player stands at x = 200 on flat ground at y = 400.
const F: i32 = 400;

/// Where the things are, in entity order.
const HIDDEN_BUNCH: usize = 0;
const HIDDEN_BARREL: usize = 1;
const HOLDER: usize = 2;
const HUTCH: usize = 3;
const HEART: usize = 4;
const RISING_HEART: usize = 5;
const BARREL: usize = 6;
const WALKER: usize = 7;
const THROWER: usize = 8;
const BALL: usize = 9;

/// The params the importer gives a hidden barrel (`hidden-barrel`).
const HIDDEN_BARREL_PARAMS: &str = r#"{ strike_frames = [], fall_height = 160, across = [[-28, 42], [-42, 28]], above = [-27, 20], becomes = "crate", becomes_at = [0, 0], item_speed = [-1, -6] }"#;

/// A level with a hidden bunch in reach of the ground action, a holder
/// further right, and the rest further still.
fn level() -> Arc<Level> {
    let src = format!(
        r#"
        format = 1
        name = "coming-back"
        size = [3000, 800]
        start = [200, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 3000, 200]

        [[entities]]
        type = "hidden"
        at = [230, {F}]

        [[entities]]
        type = "hidden"
        at = [600, {F}]
        params = {HIDDEN_BARREL_PARAMS}

        [[entities]]
        type = "crate"
        at = [420, {F}]
        params = {{ holds_character = true }}

        [[entities]]
        type = "hutch"
        at = [800, {F}]

        [[entities]]
        type = "heart"
        at = [900, {}]
        params = {{ lives = 3 }}

        [[entities]]
        type = "heart"
        at = [950, {}]
        params = {{ rise = 0.00390625 }}

        [[entities]]
        type = "crate"
        at = [1000, {F}]

        [[entities]]
        type = "crawler"
        at = [1100, {F}]

        [[entities]]
        type = "thrower"
        at = [1200, {F}]
        params = {{ period = 5000, throws = [[4000, 0, -7]] }}

        [[entities]]
        type = "ball"
        at = [1250, {}]
        "#,
        F - 30,
        F - 63,
        F - 40,
    );
    Arc::new(Level::from_toml_str(&src, "coming-back").unwrap())
}

/// Classic 1 with both characters in the roster.
fn feel() -> Arc<Feel> {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    Arc::new(feel)
}

/// The level at its start, the lead alone: the second character is the
/// one the holder holds.
fn alone() -> World {
    let mut w = World::new(level(), feel());
    w.other = None;
    w
}

/// Holds `keys` for `steps` steps, pressing them on the first.
fn play(world: &mut World, keys: &[Action], steps: u32) {
    let held: Actions = keys.iter().copied().collect();
    for k in 0..steps {
        let last = if k == 0 { Actions::NONE } else { held };
        step(world, &InputFrame::single(Frame::after(last, held)));
    }
}

fn press(world: &mut World, keys: &[Action]) {
    let now: Actions = keys.iter().copied().collect();
    step(world, &InputFrame::single(Frame::after(Actions::NONE, now)));
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

/// Thing `i` after coming back at `at`, and as it is coming back to a
/// level where nothing happened.
fn compare(world: &World, i: usize, at: i32) -> (clamber_sim::Thing, clamber_sim::Thing) {
    let after = back_at(world, at).things[i].clone();
    let fresh = back_at(&alone(), at).things[i].clone();
    (after, fresh)
}

fn assert_there_again(world: &World, i: usize) {
    for at in [200, 1100] {
        let (after, fresh) = compare(world, i, at);
        assert_eq!(after, fresh, "thing {i} back at {at}");
    }
}

fn assert_gone(world: &World, i: usize) {
    for at in [200, 1100] {
        let after = &back_at(world, at).things[i];
        assert!(!after.active, "thing {i} back at {at}");
        assert_ne!(after.phase, Phase::Ready, "thing {i} back at {at}");
    }
}

#[test]
fn a_struck_hidden_bunch_stays_gone_with_its_bunch() {
    let mut w = alone();
    play(&mut w, &[], 4);
    w.player.facing = clamber_sim::Facing::Right;
    press(&mut w, &[Action::Down, Action::Action]);
    play(&mut w, &[], 90);
    let bunch = &w.things[HIDDEN_BUNCH];
    assert_eq!(bunch.kind, "cluster", "struck, its item became a bunch");
    assert!(bunch.active);
    assert_gone(&w, HIDDEN_BUNCH);

    // Nothing is there to strike again.
    let mut after = back_at(&w, 200);
    let gems = after.flow.gems;
    play(&mut after, &[], 4);
    after.player.facing = clamber_sim::Facing::Right;
    press(&mut after, &[Action::Down, Action::Action]);
    play(&mut after, &[], 120);
    assert!(!after.things[HIDDEN_BUNCH].active);
    assert_eq!(after.flow.gems, gems);
}

#[test]
fn a_hidden_bunch_not_struck_is_there_again() {
    let mut w = alone();
    w.things[HIDDEN_BUNCH].active = false;
    assert_there_again(&w, HIDDEN_BUNCH);
}

#[test]
fn a_hidden_barrel_set_off_stays_gone_wherever_its_barrel_is() {
    for phase in [
        Phase::Item {
            vy: Fixed::ZERO,
            wait: 0,
        },
        Phase::Ready,
        Phase::Held,
        Phase::Broken,
    ] {
        let mut w = alone();
        let barrel = &mut w.things[HIDDEN_BARREL];
        barrel.kind = "crate".into();
        barrel.pos = Vec2::ints(300, F);
        barrel.phase = phase;
        assert_gone(&w, HIDDEN_BARREL);
    }
}

/// Picks up the holder, standing just left of it, and throws it onto the
/// floor, where it breaks and frees the second character.
fn holder_broken() -> World {
    let mut w = alone();
    w.player.pos = Vec2::ints(400, F);
    play(&mut w, &[], 2);
    play(&mut w, &[Action::Action], 21);
    assert!(w.player.hands.is_some(), "picked up");
    play(&mut w, &[], 80);
    assert!(!w.things[HOLDER].active, "broken");
    assert!(w.other.is_some(), "freed");
    w
}

#[test]
fn a_broken_holder_stays_gone_and_its_character_follows() {
    let w = holder_broken();
    assert_gone(&w, HOLDER);
    // Far from the walker and the thrower.
    let mut after = back_at(&w, 2000);
    let other = after.other.as_ref().expect("still there");
    assert_eq!(other.character, 1);
    let lead = after.player.pos;
    play(&mut after, &[Action::Right], 60);
    assert!(after.player.pos.x > lead.x);
    let other = after.other.as_ref().unwrap();
    assert!(other.pos.x > lead.x, "follows the lead");
}

#[test]
fn a_holder_put_down_or_carried_is_at_its_place_still_holding() {
    let mut put_down = alone();
    put_down.things[HOLDER].pos = Vec2::ints(500, F);
    let mut carried = alone();
    carried.things[HOLDER].phase = Phase::Held;
    for w in [put_down, carried] {
        assert_there_again(&w, HOLDER);
        assert!(back_at(&w, 200).other.is_none(), "still missing");
    }
}

#[test]
fn a_broken_animal_crate_stays_gone_with_its_animal() {
    for (active, phase) in [
        (true, Phase::Opened(3)),
        (
            true,
            Phase::Bolting {
                vx: Fixed::from_int(2),
            },
        ),
        (
            false,
            Phase::Bolting {
                vx: Fixed::from_int(2),
            },
        ),
    ] {
        let mut w = alone();
        w.things[HUTCH].active = active;
        w.things[HUTCH].phase = phase;
        assert_gone(&w, HUTCH);
    }
    let mut w = alone();
    w.things[HUTCH].active = false;
    assert_there_again(&w, HUTCH);
}

#[test]
fn an_extra_life_item_taken_stays_gone_and_one_not_taken_is_there_again() {
    for i in [HEART, RISING_HEART] {
        let mut taken = alone();
        taken.things[i].active = false;
        taken.things[i].phase = Phase::Gone;
        assert_gone(&taken, i);

        let mut removed = alone();
        removed.things[i].active = false;
        removed.things[i].phase = Phase::Broken;
        assert_there_again(&removed, i);
    }
    let mut w = alone();
    let lives = w.flow.lives;
    let mut back = back_at(&w, 900);
    play(&mut back, &[], 2);
    assert_eq!(back.flow.lives, lives + 3, "taken again");
    w.things[HEART].active = false;
    w.things[HEART].phase = Phase::Gone;
    let mut back = back_at(&w, 900);
    play(&mut back, &[], 2);
    assert_eq!(back.flow.lives, lives);
}

/// A thrown flight, for a barrel thrown and broken.
fn thrown() -> Phase {
    Phase::Thrown(Flight {
        vx: Fixed::from_int(4),
        vy: Fixed::ZERO,
        target: Fixed::from_int(4),
        response: Fixed::ZERO,
        gravity: Fixed::ZERO,
        max_fall: Fixed::ZERO,
        thrown: true,
        margin: Fixed::ZERO,
        bounce_min: Fixed::ZERO,
        bounce: Fixed::ZERO,
        rolling: false,
        age: 30,
    })
}

#[test]
fn barrels_walkers_the_thrower_and_its_ball_are_there_again() {
    let mut broken = alone();
    broken.things[BARREL].active = false;
    broken.things[BARREL].phase = thrown();
    let mut put_down = alone();
    put_down.things[BARREL].pos = Vec2::ints(700, F);
    let mut carried = alone();
    carried.things[BARREL].phase = Phase::Held;
    for w in [broken, put_down, carried] {
        assert_there_again(&w, BARREL);
    }

    let mut w = alone();
    w.things[WALKER].active = false;
    w.things[WALKER].phase = Phase::Falling {
        vy: Fixed::from_int(8),
        hold: 0,
    };
    w.things[THROWER].active = false;
    w.things[THROWER].phase = Phase::Falling {
        vy: Fixed::from_int(8),
        hold: 0,
    };
    w.things[BALL].active = false;
    w.things[BALL].phase = Phase::Flying {
        vy: Fixed::ZERO,
        ignoring: None,
        kept: false,
    };
    for i in [WALKER, THROWER, BALL] {
        assert_there_again(&w, i);
    }
}

#[test]
fn a_thrown_plain_barrel_is_not_taken_for_a_holder() {
    // Only an object holding the missing character stays gone broken.
    let mut w = holder_broken();
    w.things[BARREL].active = false;
    w.things[BARREL].phase = thrown();
    assert_gone(&w, HOLDER);
    assert_there_again(&w, BARREL);
}
