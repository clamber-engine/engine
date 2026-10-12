//! The player's movement states, played through whole worlds.

use std::collections::BTreeMap;
use std::sync::Arc;

use std::ops::{Deref, DerefMut};

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Feel, Fixed, Param};
use clamber_sim::{Event, InputFrame, ROLL, SWAP_FRAMES, State, World, generic_feel, step};

/// Flat ground at y = 400 from x = 0 to 1600, a ledge down to y = 528
/// beyond it, and a one-way platform at y = 340 over x = 300..400.
const FIELD: &str = r#"
    format = 1
    name = "field"
    size = [4096, 600]
    start = [64, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 400, 1600, 200]

    [[collision]]
    shape = "rect"
    rect = [1600, 528, 2496, 72]

    [[collision]]
    shape = "segment"
    from = [300, 340]
    to = [400, 340]
    one_way = true
"#;

fn level() -> Arc<Level> {
    Arc::new(Level::from_toml_str(FIELD, "field").unwrap())
}

/// A world and what was held on its last step, so input carries over
/// from one [`hold`] to the next.
struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(feel: Feel) -> Play {
        Play {
            world: World::new(level(), Arc::new(feel)),
            held: Actions::NONE,
        }
    }
}

impl Deref for Play {
    type Target = World;

    fn deref(&self) -> &World {
        &self.world
    }
}

impl DerefMut for Play {
    fn deref_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

fn world() -> Play {
    Play::new(generic_feel("default").unwrap())
}

fn physics(param: Param) -> Fixed {
    generic_feel("default").unwrap().world[param]
}

fn keys(actions: &[Action]) -> Actions {
    actions.iter().copied().collect()
}

/// One step's outcome.
#[derive(Debug)]
struct Step {
    state: State,
    events: Vec<Event>,
}

/// Holds `held` for `frames` steps.
fn hold(play: &mut Play, held: &[Action], frames: usize) -> Vec<Step> {
    let held = keys(held);
    let mut steps = Vec::new();
    for _ in 0..frames {
        let input = InputFrame::single(Frame::after(play.held, held));
        play.held = held;
        step(&mut play.world, &input);
        steps.push(Step {
            state: play.player.state,
            events: play.events.clone(),
        });
    }
    steps
}

/// Index of the first step with `event`.
fn first(steps: &[Step], event: Event) -> Option<usize> {
    steps.iter().position(|s| s.events.contains(&event))
}

fn all_events(steps: &[Step]) -> Vec<Event> {
    steps
        .iter()
        .flat_map(|s| s.events.iter().copied())
        .collect()
}

/// Jump height: how far above its take-off point the player rose,
/// holding jump for `jump_frames` steps.
fn jump_height(play: &mut Play, jump_frames: usize) -> Fixed {
    let floor = play.player.pos.y;
    let mut top = floor;
    let mut steps = Vec::new();
    loop {
        let held: &[Action] = if steps.len() < jump_frames {
            &[Action::Jump]
        } else {
            &[]
        };
        steps.extend(hold(play, held, 1));
        top = top.min(play.player.pos.y);
        if play.player.grounded() {
            break;
        }
    }
    assert_eq!(first(&steps, Event::JumpStart), Some(0));
    floor - top
}

#[test]
fn starts_idle_on_the_ground() {
    let w = world();
    assert!(w.player.grounded());
    assert_eq!(w.player.state, State::Idle);
}

#[test]
fn walks_and_runs_at_their_own_speeds() {
    let mut w = world();
    let steps = hold(&mut w, &[Action::Right], 60);
    assert_eq!(steps[0].state, State::Walk);
    assert_eq!(w.player.vel.x, physics(Param::WalkSpeed));

    let steps = hold(&mut w, &[Action::Right, Action::Action], 60);
    assert!(steps.iter().all(|s| s.state == State::Run));
    assert_eq!(w.player.vel.x, physics(Param::TopSpeed));

    // letting go of action eases back down to walking
    let steps = hold(&mut w, &[Action::Right], 60);
    assert_eq!(steps[0].state, State::Walk);
    assert_eq!(w.player.vel.x, physics(Param::WalkSpeed));

    let steps = hold(&mut w, &[], 60);
    assert_eq!(steps.last().unwrap().state, State::Idle);
    assert_eq!(w.player.vel.x, Fixed::ZERO);
}

#[test]
fn turning_around() {
    let mut w = world();
    hold(&mut w, &[Action::Right], 30);
    let steps = hold(&mut w, &[Action::Left], 30);
    assert_eq!(steps[0].events, [Event::Turn]);
    assert_eq!(all_events(&steps), [Event::Turn]);
    assert!(w.player.vel.x.is_negative());
}

#[test]
fn jump_height_depends_on_how_long_jump_is_held() {
    let full = jump_height(&mut world(), 60);
    let short = jump_height(&mut world(), 1);
    let speed = physics(Param::JumpSpeed);
    let gravity = physics(Param::Gravity);
    assert!(short < full, "{short} < {full}");
    // about v² / 2g; each step adds gravity before moving
    let expected = Fixed::from_int(72);
    assert!((full - expected).abs() <= speed, "{full} vs {expected}");
    assert!(short > gravity);
}

#[test]
fn jump_goes_up_then_falls_and_lands() {
    let mut w = world();
    let mut steps = hold(&mut w, &[Action::Jump], 10);
    steps.extend(hold(&mut w, &[], 60));
    assert_eq!(steps[0].state, State::Jump);
    let release = first(&steps, Event::JumpRelease).unwrap();
    assert_eq!(release, 10);
    let fall = steps.iter().position(|s| s.state == State::Fall).unwrap();
    assert!(fall > release);
    let land = first(&steps, Event::Land).unwrap();
    assert!(land > fall);
    assert_eq!(steps[land].state, State::Idle);
    assert_eq!(
        all_events(&steps),
        [Event::JumpStart, Event::JumpRelease, Event::Land]
    );
}

#[test]
fn holding_jump_doesnt_jump_again() {
    let mut w = world();
    let steps = hold(&mut w, &[Action::Jump], 200);
    let starts = all_events(&steps)
        .into_iter()
        .filter(|&e| e == Event::JumpStart)
        .count();
    assert_eq!(starts, 1);
}

/// A world with the player falling from high up.
fn dropped() -> Play {
    let mut w = world();
    w.player.pos.y = Fixed::from_int(200);
    w.player.ground = None;
    w.player.state = State::Fall;
    w
}

/// A world with the player falling, landing on the `n`th step from now.
fn falling(n: usize) -> Play {
    let mut probe = dropped();
    let land = first(&hold(&mut probe, &[], 120), Event::Land).unwrap();
    let mut w = dropped();
    hold(&mut w, &[], land + 1 - n);
    w
}

#[test]
fn a_jump_pressed_just_before_landing_happens_on_landing() {
    let window = physics(Param::JumpBuffer).floor() as usize;

    // pressed on the step that lands, the jump comes on the next, which
    // is `window` steps after the press when pressed `window - 1` early
    let mut w = falling(window);
    let steps = hold(&mut w, &[Action::Jump], window + 1);
    assert_eq!(first(&steps, Event::Land), Some(window - 1));
    assert_eq!(first(&steps, Event::JumpStart), Some(window));

    // one step earlier is too early
    let mut w = falling(window + 1);
    let steps = hold(&mut w, &[Action::Jump], window + 10);
    assert_eq!(first(&steps, Event::Land), Some(window));
    assert_eq!(first(&steps, Event::JumpStart), None);
}

#[test]
fn a_buffered_tap_makes_a_short_hop() {
    let mut w = falling(3);
    hold(&mut w, &[Action::Jump], 1);
    let mut steps = hold(&mut w, &[], 3);
    assert_eq!(steps[2].events, [Event::JumpStart]);
    let floor = w.player.pos.y + w.player.vel.y;
    let mut top = floor;
    while !w.player.grounded() {
        top = top.min(w.player.pos.y);
        steps.extend(hold(&mut w, &[], 1));
    }
    assert_eq!(first(&steps, Event::JumpRelease), None);
    let hop = floor - top;
    let tap = jump_height(&mut world(), 1);
    assert!(hop > Fixed::ZERO && hop < tap, "{hop} vs {tap}");
}

/// A world walking right toward the ledge, `frames` steps after leaving
/// the ground.
fn walked_off(frames_after: usize) -> (Play, Vec<Step>) {
    let mut w = world();
    w.player.pos.x = Fixed::from_int(1500);
    let mut steps = Vec::new();
    while first(&steps, Event::LeaveGround).is_none() {
        steps.extend(hold(&mut w, &[Action::Right], 1));
    }
    steps.extend(hold(&mut w, &[Action::Right], frames_after));
    (w, steps)
}

#[test]
fn jumping_just_after_walking_off_a_ledge_still_works() {
    let coyote = physics(Param::CoyoteTime).floor() as usize;
    let (mut w, steps) = walked_off(coyote - 1);
    assert_eq!(steps.last().unwrap().state, State::Fall);
    let steps = hold(&mut w, &[Action::Right, Action::Jump], 1);
    assert_eq!(steps[0].events, [Event::JumpStart]);
    assert_eq!(steps[0].state, State::Jump);

    let (mut w, _) = walked_off(coyote);
    let steps = hold(&mut w, &[Action::Right, Action::Jump], 1);
    assert!(steps[0].events.is_empty(), "{:?}", steps[0].events);
}

#[test]
fn jumps_up_through_a_one_way_platform_and_lands_on_it() {
    let mut w = world();
    w.player.pos.x = Fixed::from_int(350);
    let mut steps = hold(&mut w, &[Action::Jump], 60);
    steps.extend(hold(&mut w, &[], 60));
    assert_eq!(all_events(&steps), [Event::JumpStart, Event::Land]);
    assert_eq!(w.player.pos.y, Fixed::from_int(340));

    // and walks off its end back down
    let steps = hold(&mut w, &[Action::Right], 120);
    assert_eq!(
        all_events(&steps),
        [Event::LeaveGround, Event::Land],
        "{steps:?}"
    );
    assert_eq!(w.player.pos.y, Fixed::from_int(400));
}

#[test]
fn rolls_on_down_and_action_while_moving() {
    let mut w = world();
    hold(&mut w, &[Action::Right, Action::Action], 60);
    let speed = w.player.vel.x;

    let steps = hold(&mut w, &[Action::Down, Action::Action], 1);
    assert_eq!(steps[0].events, [Event::RollStart]);
    assert_eq!(steps[0].state, State::Roll);
    assert_eq!(w.player.vel.x, speed - physics(Param::RollFriction));

    // keeps rolling on its own, holding nothing, until too slow
    let steps = hold(&mut w, &[], 600);
    let end = first(&steps, Event::RollEnd).unwrap();
    assert!(steps[..end].iter().all(|s| s.state == State::Roll));
    assert_ne!(steps[end].state, State::Roll);
    assert_eq!(w.player.vel.x, Fixed::ZERO);
    assert_eq!(steps.last().unwrap().state, State::Idle);
}

#[test]
fn pressing_against_a_roll_brakes_it_and_keeps_facing() {
    let mut w = world();
    hold(&mut w, &[Action::Right, Action::Action], 60);
    hold(&mut w, &[Action::Down, Action::Action], 1);
    let speed = w.player.vel.x;
    let steps = hold(&mut w, &[Action::Left], 1);
    assert!(steps[0].events.is_empty());
    assert_eq!(w.player.vel.x, speed - physics(Param::RollDeceleration));
    assert_eq!(w.player.facing, clamber_sim::Facing::Right);
}

#[test]
fn no_roll_when_standing_still_or_without_the_ability() {
    let mut w = world();
    let steps = hold(&mut w, &[Action::Down, Action::Action], 10);
    assert!(all_events(&steps).is_empty());

    let mut feel = generic_feel("default").unwrap();
    feel.roster[0].abilities.remove(ROLL);
    let mut w = Play::new(feel);
    hold(&mut w, &[Action::Right, Action::Action], 60);
    let steps = hold(&mut w, &[Action::Down, Action::Action], 1);
    assert!(steps[0].events.is_empty());
    assert_eq!(steps[0].state, State::Run);
}

#[test]
fn jumping_out_of_a_roll_ends_it() {
    let mut w = world();
    hold(&mut w, &[Action::Right, Action::Action], 60);
    hold(&mut w, &[Action::Down, Action::Action], 1);
    let steps = hold(&mut w, &[Action::Jump], 1);
    assert_eq!(steps[0].events, [Event::RollEnd, Event::JumpStart]);
    assert_eq!(steps[0].state, State::Jump);
}

fn two_characters() -> Feel {
    let mut feel = generic_feel("default").unwrap();
    let mut second = feel.roster[0].physics;
    second.set(Param::WalkSpeed, Fixed::from_int(1));
    feel.roster.push(CharacterFeel {
        physics: second,
        abilities: BTreeMap::new(),
    });
    feel
}

#[test]
fn swaps_to_the_next_character() {
    let mut w = Play::new(two_characters());
    let swap = SWAP_FRAMES as usize;
    let steps = hold(&mut w, &[Action::Swap], 1);
    assert_eq!(steps[0].state, State::Swap);
    assert_eq!(w.player.character, 1);

    // input is ignored until the swap is over
    let steps = hold(&mut w, &[Action::Right, Action::Jump], swap - 1);
    assert!(steps.iter().all(|s| s.state == State::Swap));
    assert!(all_events(&steps).is_empty());
    assert_eq!(w.player.vel.x, Fixed::ZERO);
    hold(&mut w, &[], 1);
    let steps = hold(&mut w, &[Action::Right, Action::Jump], 1);
    assert_eq!(steps[0].events, [Event::JumpStart]);

    hold(&mut w, &[Action::Right], 200);
    assert_eq!(w.player.vel.x, Fixed::from_int(1));

    // and back to the first
    hold(&mut w, &[], 30);
    hold(&mut w, &[Action::Swap], swap);
    assert_eq!(w.player.character, 0);
}

#[test]
fn no_swap_with_one_character_or_in_the_air() {
    let mut w = world();
    let steps = hold(&mut w, &[Action::Swap], 5);
    assert!(steps.iter().all(|s| s.state == State::Idle));

    let mut w = Play::new(two_characters());
    hold(&mut w, &[Action::Jump], 5);
    let steps = hold(&mut w, &[Action::Swap], 1);
    assert_ne!(steps[0].state, State::Swap);
    assert_eq!(w.player.character, 0);
}

#[test]
fn state_names_follow_the_trace_vocabulary() {
    let names: Vec<_> = [
        State::Idle,
        State::Walk,
        State::Run,
        State::Roll,
        State::Jump,
        State::Fall,
        State::Swap,
    ]
    .iter()
    .map(|s| s.name())
    .collect();
    assert_eq!(
        names,
        ["idle", "walk", "run", "roll", "jump", "fall", "swap"]
    );
}
