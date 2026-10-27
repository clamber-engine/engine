//! What a hit on a rider costs in Classic 1 (spec/behavior/riding-hit.md):
//! the animal and nothing else, and 128 frames of protection from the
//! knock-off.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Param, Presets};
use clamber_sim::{
    Behavior, Event, Facing, Fixed, InputFrame, Other, Phase, Riding, World, generic_feel, step,
};

const FLOOR: i32 = 300;
const X0: i32 = 1000;
const STINGER: usize = 1;

/// Flat ground, an animal crate, and a stinger well above the rider.
fn play(both: bool) -> World {
    let src = format!(
        "format = 1\nname = \"riding\"\nsize = [3000, 600]\nstart = [{X0}, {FLOOR}]\n\
         [[collision]]\nshape = \"rect\"\nrect = [0, {FLOOR}, 3000, 280]\n\
         [[entities]]\ntype = \"hutch\"\nat = [40, {FLOOR}]\n\
         [[entities]]\ntype = \"stinger\"\nat = [{X0}, 0]\n",
    );
    let level = Level::from_toml_str(&src, "riding").unwrap();
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    feel.world.set(Param::FollowTrail, Fixed::ZERO);
    let mut world = World::new(Arc::new(level), Arc::new(feel));
    let Some(Behavior::Hutch(hutch)) = world.things[0].archetype.map(|a| a.behavior) else {
        panic!("no animal crate");
    };
    world.things[0].phase = Phase::Ridden;
    let at = Vec2::new(Fixed::from_int(X0), Fixed::from_int(FLOOR));
    world.player.pos = at;
    world.player.facing = Facing::Right;
    world.other = None;
    if both {
        let behind = Vec2::new(Fixed::from_int(X0 - 40), Fixed::from_int(FLOOR));
        world.other = Some(Other::new(1, behind, Facing::Right, at));
    }
    world.player.riding = Some(Riding {
        on_floor: true,
        was_on_floor: true,
        mount: None,
        ..Riding::new(hutch.ride, Fixed::ZERO)
    });
    world
}

fn tick(world: &mut World) {
    let input = InputFrame::single(Frame::after(Actions::NONE, Actions::NONE));
    step(world, &input);
}

/// Puts the stinger on the player and steps; returns the frame the
/// player is first hurt on, counted from the knock-off (H = 0), if it is
/// within `frames`.
fn hurt_after(world: &mut World, frames: u32) -> Option<u32> {
    let mut knocked = None;
    for n in 0..frames {
        world.things[STINGER].pos = world.player.pos;
        tick(world);
        if knocked.is_none() && world.player.riding.is_none() {
            knocked = Some(n);
        }
        if world.events.contains(&Event::Hurt) {
            return Some(n - knocked.expect("hurt while riding"));
        }
    }
    None
}

#[test]
fn a_stinger_on_the_rider_costs_only_the_animal() {
    for both in [false, true] {
        let mut world = play(both);
        let lives = world.flow.lives;
        tick(&mut world);
        assert!(world.things[STINGER].active);
        world.things[STINGER].pos = world.player.pos;
        tick(&mut world);
        assert!(world.player.riding.is_some(), "touch frame");
        tick(&mut world);
        assert!(world.player.riding.is_none(), "knocked off the next frame");
        assert!(!world.events.contains(&Event::Hurt));
        assert_eq!(world.flow.lives, lives);
        assert_eq!(world.other.is_some(), both);
        assert!(!world.losing());
        assert!(world.things[0].active);
    }
}

#[test]
fn nothing_hurts_the_player_for_128_frames_from_the_knock_off() {
    for both in [false, true] {
        let mut world = play(both);
        let at = hurt_after(&mut world, 400);
        assert_eq!(at, Some(128), "both: {both}");
    }
}

#[test]
fn the_hit_after_the_protection_is_an_ordinary_one() {
    let mut world = play(true);
    let lives = world.flow.lives;
    hurt_after(&mut world, 400).unwrap();
    assert!(world.loss.is_some() || world.flow.lives < lives);
}
