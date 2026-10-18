//! Hit points on Clamber's own presets (`hit_points`): a life takes three
//! hits, each but the last knocking the player back and protecting it
//! for a while, and a tonic gives one back.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{Param, Presets};
use clamber_sim::{
    Event, Fixed, FlowEvent, InputFrame, Outcome, START_LIVES, World, generic_feel, step,
};

/// Ground at y = 400 from x = 0 to 1000, the player at `start`, and
/// `extra` appended.
fn world(preset: &str, start: i32, extra: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "hits"
        size = [1000, 600]
        start = [{start}, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 1000, 200]

        {extra}
        "#
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(Arc::new(level), Arc::new(generic_feel(preset).unwrap()))
}

/// A hazard strip on the ground at x = 300..332.
const HAZARD: &str = r#"
    [[collision]]
    shape = "rect"
    rect = [300, 396, 32, 4]
    surface = "hazard"
"#;

/// One step with `held`, after `previous`.
fn press(world: &mut World, previous: Actions, held: Actions) {
    step(world, &InputFrame::single(Frame::after(previous, held)));
}

/// Steps with `held` until the player is hurt, at most `frames` steps.
/// Returns whether it was.
fn until_hurt(world: &mut World, held: Actions, frames: usize) -> bool {
    let mut previous = Actions::NONE;
    for _ in 0..frames {
        press(world, previous, held);
        previous = held;
        if world.events.contains(&Event::Hurt) {
            return true;
        }
    }
    false
}

fn physics(world: &World, param: Param) -> u32 {
    world.feel.world[param].floor() as u32
}

#[test]
fn clamber_presets_take_three_hits_and_classic_ones_one() {
    let presets = Presets::builtin();
    for (preset, points) in [
        ("default", 3),
        ("snappy", 3),
        ("floaty", 3),
        ("classic-1", 1),
        ("classic-2", 1),
        ("classic-3", 1),
    ] {
        let world = presets.world(preset).unwrap();
        assert_eq!(world[Param::HitPoints], Fixed::from_int(points), "{preset}");
    }
    assert_eq!(world("default", 64, "").hit_points(), Some((3, 3)));
    assert_eq!(world("classic-1", 64, "").hit_points(), None);
}

#[test]
fn a_hit_knocks_back_blocks_input_and_protects_for_a_while() {
    let mut w = world("default", 260, HAZARD);
    let right: Actions = Action::Right.into();
    assert!(until_hurt(&mut w, right, 200));
    assert_eq!(w.flow.outcome, Outcome::Playing);
    assert_eq!(w.flow.lives, START_LIVES);
    assert_eq!(w.hit_points(), Some((2, 3)));
    // Knocked back up and away, the way it doesn't face.
    assert!(w.player.vel.x < Fixed::ZERO && w.player.vel.y < Fixed::ZERO);
    assert!(!w.player.grounded());

    // Input is ignored while control comes back: holding right doesn't
    // stop the flight left.
    let wait = physics(&w, Param::HitControlWait);
    let protect = physics(&w, Param::HitProtectTime);
    let period = physics(&w, Param::HitFlashPeriod);
    assert!(wait > 1 && protect > wait && period > 1);
    let mut drawn = vec![w.player_drawn()];
    for _ in 1..wait {
        press(&mut w, right, right);
        assert!(w.player.vel.x < Fixed::ZERO, "frame {}", w.frame);
        drawn.push(w.player_drawn());
    }
    // Then it walks right again, over the hazard, without being hurt
    // while it's protected, blinking.
    let mut hurt = false;
    for _ in wait..protect {
        press(&mut w, right, right);
        hurt |= w.events.contains(&Event::Hurt);
        drawn.push(w.player_drawn());
    }
    assert!(!hurt);
    assert!(w.player.pos.x > Fixed::from_int(340), "walked over it");
    let expected: Vec<bool> = (0..protect)
        .map(|n| n % period < period.div_ceil(2))
        .collect();
    assert_eq!(drawn, expected);
    press(&mut w, right, right);
    assert!(w.player_drawn());
    assert_eq!(w.flow.recovery, None);
}

#[test]
fn the_last_hit_point_costs_the_life_and_coming_back_restores_them() {
    // Standing still on the hazard, it's hurt again each time protection
    // ends, once knocked back onto it.
    let mut w = world(
        "default",
        316,
        HAZARD.replace("300, 396, 32", "200, 396, 232").as_str(),
    );
    let mut seen = Vec::new();
    let mut flow: Vec<FlowEvent> = Vec::new();
    for _ in 0..1000 {
        press(&mut w, Actions::NONE, Actions::NONE);
        if w.events.contains(&Event::Hurt) {
            seen.push(w.hit_points().unwrap().0);
        }
        flow.extend(&w.flow.events);
        if w.flow.events.contains(&FlowEvent::Die) {
            break;
        }
    }
    assert_eq!(seen, [2, 1, 1]);
    assert_eq!(flow, [FlowEvent::Die]);
    assert_eq!(w.flow.lives, START_LIVES - 1);
    while !w.flow.events.contains(&FlowEvent::Respawn) {
        press(&mut w, Actions::NONE, Actions::NONE);
    }
    assert_eq!(w.hit_points(), Some((3, 3)));
}

#[test]
fn a_tonic_gives_back_a_hit_point_and_is_left_at_full_health() {
    let tonic = r#"
        [[entities]]
        type = "tonic"
        at = [200, 390]
    "#;
    let right: Actions = Action::Right.into();
    let left: Actions = Action::Left.into();

    // At full health it stays where it is.
    let mut w = world("default", 64, tonic);
    let mut previous = Actions::NONE;
    for _ in 0..120 {
        press(&mut w, previous, right);
        previous = right;
        assert!(w.flow.events.is_empty(), "{:?}", w.flow.events);
    }
    assert!(w.player.pos.x > Fixed::from_int(240));
    assert!(w.things[0].active);

    // Hurt, walking back over it takes it.
    let mut w = world("default", 64, &format!("{tonic}{HAZARD}"));
    assert!(until_hurt(&mut w, right, 300));
    assert_eq!(w.hit_points(), Some((2, 3)));
    let mut taken: Vec<FlowEvent> = Vec::new();
    let mut previous = Actions::NONE;
    for _ in 0..80 {
        press(&mut w, previous, left);
        previous = left;
        taken.extend(&w.flow.events);
    }
    assert_eq!(taken, [FlowEvent::Collect(0)]);
    assert!(!w.things[0].active);
    assert_eq!(w.hit_points(), Some((3, 3)));
}

#[test]
fn an_enemy_knocks_the_player_away_from_it() {
    // The player faces left, with a walker coming up behind it.
    let mut w = world(
        "default",
        200,
        r#"
        [[entities]]
        type = "crawler"
        at = [300, 400]
        params = { facing = "left" }
        "#,
    );
    press(&mut w, Actions::NONE, Action::Left.into());
    assert!(until_hurt(&mut w, Actions::NONE, 300));
    assert_eq!(w.hit_points(), Some((2, 3)));
    assert!(w.things[0].pos.x > w.player.pos.x);
    assert!(w.player.vel.x < Fixed::ZERO, "away from the walker");
}
