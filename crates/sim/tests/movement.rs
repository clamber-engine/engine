//! The player's movement states, played through whole worlds.

use std::collections::BTreeMap;
use std::sync::Arc;

use std::ops::{Deref, DerefMut};

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Feel, Fixed, Param};
use clamber_sim::{
    Event, Facing, InputFrame, ROLL, RollSize, SWAP_FRAMES, State, World, generic_feel, step,
};

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

/// `w` with the player falling from high up.
fn dropped_in(mut w: Play) -> Play {
    w.player.pos.y = Fixed::from_int(200);
    w.player.ground = None;
    w.player.state = State::Fall;
    w
}

/// A world with the player falling, landing on the `n`th step from now.
fn falling(n: usize) -> Play {
    falling_in(world, n)
}

fn falling_in(new: fn() -> Play, n: usize) -> Play {
    let mut probe = dropped_in(new());
    let land = first(&hold(&mut probe, &[], 120), Event::Land).unwrap();
    let mut w = dropped_in(new());
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

fn classic() -> Play {
    Play::new(generic_feel("classic-1").unwrap())
}

/// Horizontal speed in 1/256 px per frame.
fn grains(v: Fixed) -> i64 {
    (v.to_f64() * 256.0) as i64
}

#[test]
fn classic_speed_eases_toward_walking_speed() {
    // spec/behavior/ground-speed.md, "Worked values".
    let mut w = classic();
    let start = w.player.pos.x;
    let mut vx = Vec::new();
    for _ in 0..37 {
        hold(&mut w, &[Action::Right], 1);
        vx.push(grains(w.player.vel.x));
    }
    for (frame, expected) in [
        (0, 64),
        (1, 120),
        (2, 169),
        (3, 211),
        (10, 392),
        (20, 478),
        (29, 499),
        (30, 500),
        (35, 505),
        (36, 512),
    ] {
        assert_eq!(vx[frame], expected, "frame {frame}");
    }
    assert_eq!(w.player.pos.x - start, Fixed::from_f64(59.7734375).unwrap());
}

#[test]
fn classic_crouch_stops_the_player() {
    // spec/behavior/ground-speed.md, "Crouching": walking at +2, Down
    // held on frames 40 to 69, Right held throughout.
    let mut w = classic();
    let mut vx = Vec::new();
    for frame in 0..81 {
        let held: &[Action] = if (40..70).contains(&frame) {
            &[Action::Right, Action::Down]
        } else {
            &[Action::Right]
        };
        hold(&mut w, held, 1);
        vx.push(w.player.vel.x.to_f64());
    }
    assert_eq!(vx[39], 2.0);
    assert_eq!(vx[40], 2.0);
    assert_eq!(vx[41], 1.75);
    assert_eq!(vx[42], 1.53125);
    assert_eq!(vx[76], 0.02734375);
    assert_eq!(vx[77..80], [0.0; 3]);
    assert_eq!(vx[80], 0.25);
}

#[test]
fn classic_turns_take_eight_frames() {
    // spec/behavior/ground-speed.md, "Facing and turns": from standing, one
    // frame of Left is enough, and facing changes 4 frames in.
    let mut w = classic();
    let mut steps = hold(&mut w, &[Action::Left], 1);
    steps.extend(hold(&mut w, &[], 10));
    let turns: Vec<_> = (0..steps.len())
        .filter(|&i| steps[i].events.contains(&Event::Turn))
        .collect();
    assert_eq!(turns, [4]);
    assert_eq!(w.player.facing, Facing::Left);

    // Left for 5 frames, then Right: the second turn starts as the first
    // ends, 8 frames in, and turns back 4 frames after that.
    let mut w = classic();
    let mut steps = hold(&mut w, &[Action::Left], 5);
    steps.extend(hold(&mut w, &[Action::Right], 15));
    let turns: Vec<_> = (0..steps.len())
        .filter(|&i| steps[i].events.contains(&Event::Turn))
        .collect();
    assert_eq!(turns, [4, 12]);
    assert_eq!(w.player.facing, Facing::Right);
}

/// The frame of the first jump in Classic 1, falling to land on frame
/// `LAND` and holding jump on the frames `held` gives.
fn classic_jump_after_landing(held: impl Fn(usize) -> bool) -> Option<usize> {
    let mut w = falling_in(classic, LAND + 1);
    let mut steps = Vec::new();
    for frame in 0..LAND + 20 {
        let keys: &[Action] = if held(frame) { &[Action::Jump] } else { &[] };
        steps.extend(hold(&mut w, keys, 1));
    }
    assert_eq!(first(&steps, Event::Land), Some(LAND));
    first(&steps, Event::JumpStart)
}

const LAND: usize = 20;

/// The earliest frame a jump can start after landing on `LAND`.
const EARLIEST: usize = LAND + 3;

#[test]
fn classic_jump_starts_the_frame_after_the_press() {
    // spec/behavior/jump-input.md, "On the ground": even a one-frame tap.
    let mut w = classic();
    let mut steps = hold(&mut w, &[Action::Jump], 1);
    steps.extend(hold(&mut w, &[], 2));
    assert_eq!(first(&steps, Event::JumpStart), Some(1));
    assert_eq!(all_events(&steps).len(), 1);

    // and on every frame of a roll
    let mut w = classic();
    w.player.vel.x = Fixed::from_int(3);
    w.player.state = State::Roll;
    let mut steps = hold(&mut w, &[Action::Jump], 1);
    assert_eq!(steps[0].state, State::Roll);
    steps.extend(hold(&mut w, &[Action::Jump], 1));
    assert_eq!(steps[1].events, [Event::RollEnd, Event::JumpStart]);
}

#[test]
fn classic_presses_before_landing() {
    // spec/behavior/jump-input.md, "Pressing before landing": every row of
    // the table, with the landing frame L = LAND and E = L + 3.
    let held_from = |p: usize, last: usize| move |f: usize| (p..=last).contains(&f);

    // pressed 1 to 9 frames before L, held through E - 1
    for before in 1..=9 {
        let jump = classic_jump_after_landing(held_from(LAND - before, EARLIEST - 1));
        assert_eq!(
            jump,
            Some(EARLIEST),
            "pressed {before} frames before landing"
        );
    }
    // pressed exactly at E - 12, held through E - 1
    let jump = classic_jump_after_landing(held_from(EARLIEST - 12, EARLIEST - 1));
    assert_eq!(jump, Some(EARLIEST));
    // pressed at E - 13 or earlier, held through landing
    for press in [EARLIEST - 13, EARLIEST - 15] {
        let jump = classic_jump_after_landing(held_from(press, LAND + 10));
        assert_eq!(jump, None, "pressed on {press}");
    }
    // pressed in the air, released before E - 1
    for last in [LAND - 1, LAND, EARLIEST - 2] {
        let jump = classic_jump_after_landing(held_from(LAND - 5, last));
        assert_eq!(jump, None, "held until {last}");
    }

    // held continuously since the jump that took off
    let mut w = classic();
    let steps = hold(&mut w, &[Action::Jump], 150);
    assert!(first(&steps, Event::Land).is_some());
    let jumps = all_events(&steps)
        .into_iter()
        .filter(|&e| e == Event::JumpStart)
        .count();
    assert_eq!(jumps, 1);
}

#[test]
fn classic_presses_on_or_after_landing() {
    // spec/behavior/jump-input.md: a press on or after L starts a jump on
    // max(P + 1, E), even a one-frame tap.
    for press in LAND..LAND + 6 {
        let jump = classic_jump_after_landing(|f| f == press);
        assert_eq!(jump, Some((press + 1).max(EARLIEST)), "pressed on {press}");
    }
}

#[test]
fn classic_jump_input_worked_example() {
    // spec/behavior/jump-input.md, "Worked example", moved from L = 40 to
    // L = LAND: pressing 9 frames before landing and holding jumps on
    // L + 3; pressing 10 frames before doesn't jump.
    let jump = classic_jump_after_landing(|f| f >= LAND - 9);
    assert_eq!(jump, Some(LAND + 3));
    let jump = classic_jump_after_landing(|f| f >= LAND - 10);
    assert_eq!(jump, None);
}

#[test]
fn classic_has_no_grace_after_walking_off() {
    // spec/behavior/walk-off.md, "Jumping near the edge": a press on the
    // frame before leaving the ground still jumps; on L or later it
    // doesn't.
    let walk_to_edge = || {
        let mut w = classic();
        w.player.pos.x = Fixed::from_int(1500);
        let mut probe = classic();
        probe.player.pos.x = Fixed::from_int(1500);
        let leave = first(&hold(&mut probe, &[Action::Right], 120), Event::LeaveGround).unwrap();
        hold(&mut w, &[Action::Right], leave - 1);
        w
    };
    let mut w = walk_to_edge();
    let steps = hold(&mut w, &[Action::Right, Action::Jump], 2);
    assert_eq!(steps[1].events, [Event::JumpStart]);

    for after in 0..3 {
        let mut w = walk_to_edge();
        let mut steps = hold(&mut w, &[Action::Right], 1 + after);
        steps.extend(hold(&mut w, &[Action::Right, Action::Jump], 3));
        assert_eq!(first(&steps, Event::LeaveGround), Some(1));
        assert_eq!(
            first(&steps, Event::JumpStart),
            None,
            "pressed {after} after leaving"
        );
    }
}

/// A Classic 1 jump from standing, pressed on step 0 and held for `held`
/// steps: `y` relative to the start and `vy` after each step, up to and
/// including the landing.
fn classic_jump(held: usize) -> Vec<(Step, f64, f64)> {
    let mut w = classic();
    let floor = w.player.pos.y;
    let mut steps = Vec::new();
    while steps.len() < 200 {
        let keys: &[Action] = if steps.len() < held {
            &[Action::Jump]
        } else {
            &[]
        };
        let step = hold(&mut w, keys, 1).remove(0);
        let landed = step.events.contains(&Event::Land);
        let y = (w.player.pos.y - floor).to_f64();
        steps.push((step, y, w.player.vel.y.to_f64()));
        if landed {
            return steps;
        }
    }
    panic!("never landed");
}

#[test]
fn classic_jump_takes_off_the_frame_after_it_starts() {
    // spec/behavior/jump-arc.md, "Take-off frame": on T, `vy` is -7 but
    // the player is still on the ground.
    let mut w = classic();
    let mut steps = hold(&mut w, &[Action::Jump], 2);
    assert_eq!(steps[1].events, [Event::JumpStart]);
    assert_ne!(steps[1].state, State::Jump);
    assert!(w.player.grounded());
    assert_eq!(w.player.pos.y, Fixed::from_int(400));
    assert_eq!(w.player.vel.y, Fixed::from_int(-7));
    steps.extend(hold(&mut w, &[Action::Jump], 1));
    assert_eq!(steps[2].state, State::Jump);
    assert!(!w.player.grounded());
    assert!(steps[2].events.is_empty());
}

#[test]
fn classic_jump_held_from_take_off_to_landing() {
    // spec/behavior/jump-arc.md, "Held from take-off to landing". T is
    // step 1.
    let steps = classic_jump(usize::MAX);
    for (n, y, vy) in [
        (0, 0.0, -7.0),
        (1, -6.71875, -6.71875),
        (2, -13.15625, -6.4375),
        (5, -30.78125, -5.59375),
        (10, -54.53125, -4.1875),
        (20, -80.9375, -1.375),
        (24, -83.625, -0.25),
        (25, -83.59375, 0.03125),
        (30, -79.21875, 1.4375),
        (40, -49.375, 4.25),
        (48, -5.25, 6.5),
        (49, 0.0, 0.0),
    ] {
        let (_, sy, svy) = &steps[1 + n];
        assert_eq!((*sy, *svy), (y, vy), "T + {n}");
    }
    assert_eq!(steps.len(), 1 + 50);
    assert!(steps[50].0.events.contains(&Event::Land));
}

#[test]
fn classic_jump_height_and_airtime_by_hold() {
    // spec/behavior/jump-arc.md, "How long jump is held": `apex_height`
    // and `airtime` as in spec/traces/METRICS.md, for H frames held from
    // the press.
    for (held, apex, airtime) in [
        (1, 52.5, 31),
        (2, 52.5, 31),
        (3, 55.0, 31),
        (4, 57.34375, 32),
        (5, 59.5625, 33),
        (8, 65.71875, 35),
        (10, 69.3125, 36),
        (12, 72.53125, 37),
        (15, 76.5625, 39),
        (20, 81.28125, 41),
        (25, 83.46875, 43),
        (26, 83.625, 44),
        (30, 83.625, 45),
        (usize::MAX, 83.625, 49),
    ] {
        let steps = classic_jump(held);
        let top = steps.iter().map(|s| -s.1).fold(0.0, f64::max);
        assert_eq!(top, apex, "held {held}");
        // the jump starts on step 1 and lands on the last
        assert!(steps[1].0.events.contains(&Event::JumpStart));
        assert_eq!(steps.len() - 2, airtime, "held {held}");
    }
}

#[test]
fn classic_pressing_again_in_the_air_keeps_the_heavier_gravity() {
    // spec/behavior/jump-arc.md: once let go, gravity is 0.4375 for the
    // rest of the jump.
    let mut w = classic();
    hold(&mut w, &[Action::Jump], 3);
    hold(&mut w, &[], 1);
    let vy = w.player.vel.y;
    hold(&mut w, &[Action::Jump], 1);
    assert_eq!(w.player.vel.y - vy, Fixed::from_f64(0.4375).unwrap());
}

#[test]
fn classic_falls_are_capped_at_8() {
    let mut w = dropped_in(classic());
    w.player.pos.y = Fixed::ZERO;
    let steps = hold(&mut w, &[], 30);
    assert_eq!(first(&steps, Event::Land), None);
    assert_eq!(w.player.vel.y, Fixed::from_int(8));
}

#[test]
fn classic_lands_from_less_than_5_px_above() {
    // spec/behavior/landing.md, "Examples": height above the floor before
    // the frame, the frame's `vy`, and whether it lands.
    for (height, vy, lands) in [
        (9.84375, 8.125, true),
        (11.25, 6.40625, true),
        (10.4375, 5.4375, false),
        (12.0625, 6.8125, false),
        (1.0, -0.0625, false),
    ] {
        let mut w = dropped_in(classic());
        let floor = Fixed::from_int(400);
        w.player.pos.y = floor - Fixed::from_f64(height).unwrap();
        w.player.vel.y = Fixed::from_f64(vy - 0.4375).unwrap();
        let steps = hold(&mut w, &[], 1);
        assert_eq!(
            steps[0].events.contains(&Event::Land),
            lands,
            "{height}, {vy}"
        );
        if lands {
            assert_eq!(w.player.pos.y, floor);
            assert_eq!(w.player.vel.y, Fixed::ZERO);
            assert!(w.player.grounded());
        } else {
            let after = Fixed::from_f64(height - vy).unwrap();
            assert_eq!(floor - w.player.pos.y, after, "{height}, {vy}");
        }
    }
}

#[test]
fn classic_walking_off_a_ledge() {
    // spec/behavior/walk-off.md, "Falling": `vy` after the frame and the
    // total fallen below the ledge, by frames after L.
    let mut w = classic();
    w.player.pos.x = Fixed::from_int(1500);
    let ledge = w.player.pos.y;
    let mut steps = Vec::new();
    while first(&steps, Event::LeaveGround).is_none() {
        steps.extend(hold(&mut w, &[Action::Right], 1));
    }
    let mut fall = vec![(w.player.vel.y, w.player.pos.y - ledge)];
    for _ in 0..19 {
        hold(&mut w, &[Action::Right], 1);
        fall.push((w.player.vel.y, w.player.pos.y - ledge));
    }
    for (n, vy, fallen) in [
        (0, 0.00390625, 3.4375),
        (1, 0.44140625, 3.87890625),
        (2, 0.87890625, 4.7578125),
        (10, 4.37890625, 27.5390625),
        (18, 7.87890625, 78.3203125),
        (19, 8.0, 86.3203125),
    ] {
        let expected = (
            Fixed::from_f64(vy).unwrap(),
            Fixed::from_f64(fallen).unwrap(),
        );
        assert_eq!(fall[n], expected, "L + {n}");
    }
}

/// One frame of play and where it left the player.
#[derive(Debug)]
struct Row {
    state: State,
    events: Vec<Event>,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    facing: Facing,
    grounded: bool,
}

/// Plays `n` frames, holding `held(frame)` on each.
fn play(w: &mut Play, n: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    (0..n)
        .map(|frame| {
            let step = hold(w, &held(frame), 1).pop().unwrap();
            let p = &w.player;
            Row {
                state: step.state,
                events: step.events,
                x: p.pos.x.to_f64(),
                y: p.pos.y.to_f64(),
                vx: p.vel.x.to_f64(),
                vy: p.vel.y.to_f64(),
                facing: p.facing,
                grounded: p.grounded(),
            }
        })
        .collect()
}

/// The frames with `event`.
fn frames_with(rows: &[Row], event: Event) -> Vec<usize> {
    (0..rows.len())
        .filter(|&i| rows[i].events.contains(&event))
        .collect()
}

/// Classic 1 running right at full speed, out of a roll that carried on.
fn classic_running() -> Play {
    let mut w = classic();
    hold(&mut w, &[Action::Action, Action::Right], 100);
    assert_eq!(w.player.vel.x, Fixed::from_int(3));
    assert_eq!(w.player.state, State::Run);
    w
}

/// Checks a row of spec/behavior/roll-speed.md, "Worked values": a roll
/// holding `on_s` on its first frame and `then` after.
fn check_classic_roll(
    mut w: Play,
    on_s: &[Action],
    then: &[Action],
    first_vx: [f64; 3],
    vx_on_d: f64,
    to_d: f64,
    to_stop: f64,
) {
    let x0 = w.player.pos.x.to_f64();
    let rows = play(&mut w, 33, |f| if f == 0 { on_s } else { then }.to_vec());
    assert_eq!(rows[0].events, [Event::RollStart]);
    assert_eq!([rows[0].vx, rows[1].vx, rows[2].vx], first_vx);
    assert_eq!(frames_with(&rows, Event::RollEnd), [29]);
    assert_eq!(rows[29].vx, vx_on_d);
    assert_eq!(rows[29].x - x0, to_d);
    assert_eq!(rows[32].x - x0, to_stop);
    assert_eq!(rows[32].vx, 0.0);
}

#[test]
fn classic_roll_worked_values() {
    use Action::{Action as A, Right as R};
    check_classic_roll(
        classic(),
        &[A],
        &[],
        [0.125, 0.234375, 0.328125],
        0.96875,
        22.78125,
        25.0234375,
    );
    check_classic_roll(
        classic(),
        &[A, R],
        &[R],
        [0.375, 0.703125, 0.98828125],
        2.93359375,
        69.1328125,
        75.921875,
    );
    let mut walking = classic();
    hold(&mut walking, &[R], 40);
    assert_eq!(walking.player.vel.x, Fixed::from_int(2));
    check_classic_roll(
        walking,
        &[A, R],
        &[R],
        [2.125, 2.234375, 2.328125],
        2.96875,
        82.78125,
        89.64453125,
    );
    let mut running = classic_running();
    hold(&mut running, &[R], 1);
    assert_eq!(running.player.vel.x.to_f64(), 2.875);
    check_classic_roll(
        running,
        &[A, R],
        &[R],
        [3.015625, 3.13671875, 3.2421875],
        3.96484375,
        111.94140625,
        121.11328125,
    );
}

#[test]
fn classic_roll_steering() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/roll-speed.md, "Worked values": reversing.
    let mut w = classic();
    hold(&mut w, &[R], 40);
    let x0 = w.player.pos.x.to_f64();
    let rows = play(&mut w, 30, |f| if f == 0 { vec![A, R] } else { vec![L] });
    assert_eq!((rows[0].vx, rows[0].facing), (2.125, Facing::Right));
    assert_eq!((rows[1].vx, rows[1].facing), (1.484375, Facing::Left));
    assert_eq!(frames_with(&rows, Event::Turn), [1]);
    assert_eq!(rows[2].vx, 0.92578125);
    assert_eq!(rows.iter().position(|r| r.vx < 0.0), Some(5));
    assert_eq!(rows[29].vx, -2.8828125);
    assert_eq!(rows[29].x - x0, -49.47265625);

    // Steering a slow roll: Right from S + 10 makes it normal.
    let mut w = classic();
    let rows = play(&mut w, 11, |f| match f {
        0 => vec![A],
        1..10 => vec![],
        _ => vec![R],
    });
    assert_eq!(rows[9].vx, 0.7265625);
    assert_eq!(rows[10].vx, 1.0078125);
}

/// Classic 1 from standing, pressing action alone on `presses` and
/// holding Right from `right`.
fn classic_roll_presses(presses: &[usize], right: usize, n: usize) -> Vec<Row> {
    let mut w = classic();
    play(&mut w, n, |f| {
        let mut keys = Vec::new();
        if presses.contains(&f) {
            keys.push(Action::Action);
        }
        if f >= right {
            keys.push(Action::Right);
        }
        keys
    })
}

#[test]
fn classic_roll_input_worked_example() {
    // spec/behavior/roll-input.md, "Worked example".
    let rows = classic_roll_presses(&[10], usize::MAX, 60);
    assert_eq!(frames_with(&rows, Event::RollStart), [10]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [39]);
    assert!(rows[10..39].iter().all(|r| r.state == State::Roll));
    assert_eq!(
        [rows[39].state, rows[40].state, rows[41].state],
        [State::Walk; 3]
    );
    assert!(rows[42..57].iter().all(|r| r.state == State::Idle));

    // Right held during the stop moves the player from 57.
    let rows = classic_roll_presses(&[10], 50, 60);
    assert!(rows[42..57].iter().all(|r| r.vx == 0.0));
    assert_eq!(rows[57].vx, 0.25);

    // A press on 37 is remembered to 56, and that roll lasts 30 frames
    // and doesn't move on its first.
    let rows = classic_roll_presses(&[10, 37], usize::MAX, 90);
    assert_eq!(frames_with(&rows, Event::RollStart), [10, 56]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [39, 86]);
    assert_eq!((rows[56].vx, rows[56].x), (0.0, rows[55].x));
    assert!(rows[57].vx > 0.0);

    // One on 36 is ignored.
    let rows = classic_roll_presses(&[10, 36], usize::MAX, 90);
    assert_eq!(frames_with(&rows, Event::RollStart), [10]);

    // A new press on 56 starts a roll there, also standing still on it.
    let rows = classic_roll_presses(&[10, 56], usize::MAX, 90);
    assert_eq!(frames_with(&rows, Event::RollStart), [10, 56]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [39, 85]);
    assert_eq!(rows[56].vx, 0.0);
}

#[test]
fn classic_roll_carries_on_into_running() {
    use Action::{Action as A, Right as R};
    // spec/behavior/roll-input.md, "How a roll ends" and "Starting the
    // next roll": action held through D = 29, released on D + 1 and
    // pressed again on D + 2.
    let mut w = classic();
    let rows = play(&mut w, 40, |f| if f == 30 { vec![R] } else { vec![A, R] });
    assert_eq!(frames_with(&rows, Event::RollStart), [0, 33]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [32]);
    assert!(rows[..32].iter().all(|r| r.state == State::Roll));
    assert_eq!(rows[32].state, State::Run);
    assert!(rows[32].vx > 2.0);
}

/// The size of a Classic 1 roll pressed with `on_s` held, after running
/// and then holding `between` for `k` frames, the last of them without
/// action.
fn classic_roll_size(between: &[Action], k: usize, on_s: &[Action]) -> RollSize {
    let mut w = classic_running();
    let last: Vec<_> = between
        .iter()
        .copied()
        .filter(|&a| a != Action::Action)
        .collect();
    hold(&mut w, between, k - 1);
    hold(&mut w, &last, 1);
    let steps = hold(&mut w, on_s, 1);
    assert_eq!(steps[0].events, [Event::RollStart]);
    w.player.roll.unwrap().size
}

#[test]
fn classic_rolls_are_fast_soon_after_running() {
    use Action::{Action as A, Right as R};
    // spec/behavior/roll-speed.md, "Running recently", the table.
    for (between, on_s, last_fast, otherwise) in [
        (&[R][..], &[A, R][..], 14, RollSize::Normal),
        (&[], &[A, R], 14, RollSize::Normal),
        (&[], &[A], 13, RollSize::Slow),
        (&[A], &[A], 13, RollSize::Slow),
    ] {
        for k in 1..=last_fast {
            let size = classic_roll_size(between, k, on_s);
            assert_eq!(size, RollSize::Fast, "{between:?} {k}");
        }
        let size = classic_roll_size(between, last_fast + 1, on_s);
        assert_eq!(size, otherwise, "{between:?}");
    }
}

/// Classic 1 standing still with action held and no roll after landing,
/// then running for `run` frames from standing, holding `after` for
/// `k` frames, and pressing action alone. Returns the roll's size.
fn classic_size_after_standing_run(run: usize, after: &[Action], k: usize) -> RollSize {
    use Action::{Action as A, Jump as J, Right as R};
    let mut w = classic();
    hold(&mut w, &[J], 1);
    hold(&mut w, &[], 2);
    // Pressed too long before landing to roll on it.
    let steps = hold(&mut w, &[A], 60);
    assert!(first(&steps, Event::Land).is_some());
    assert_eq!(first(&steps, Event::RollStart), None);
    assert_eq!(w.player.vel.x, Fixed::ZERO);
    hold(&mut w, &[A, R], run);
    hold(&mut w, after, k);
    hold(&mut w, &[], 1);
    let steps = hold(&mut w, &[A], 1);
    assert_eq!(steps[0].events, [Event::RollStart]);
    w.player.roll.unwrap().size
}

#[test]
fn classic_a_single_run_from_standing_doesnt_make_a_fast_roll() {
    use Action::Right as R;
    // spec/behavior/roll-speed.md, "Running recently", the exception.
    for k in 0..13 {
        assert_eq!(classic_size_after_standing_run(1, &[], k), RollSize::Slow);
    }
    // A direction held after it, or two running frames, make it fast.
    assert_eq!(classic_size_after_standing_run(1, &[R], 1), RollSize::Fast);
    assert_eq!(classic_size_after_standing_run(2, &[], 1), RollSize::Fast);
}

/// The frame of the first roll in Classic 1, falling to land on `LAND`
/// and holding action on the frames in `held`.
fn classic_roll_after_landing(held: std::ops::Range<usize>) -> Option<usize> {
    let mut w = falling_in(classic, LAND + 1);
    let rows = play(&mut w, LAND + 10, |f| {
        if held.contains(&f) {
            vec![Action::Action]
        } else {
            vec![]
        }
    });
    assert_eq!(frames_with(&rows, Event::Land), [LAND]);
    frames_with(&rows, Event::RollStart).first().copied()
}

#[test]
fn classic_roll_presses_around_landing() {
    // spec/behavior/roll-input.md, "In the air".
    for early in 0..=6 {
        let press = LAND - early;
        assert_eq!(classic_roll_after_landing(press..LAND + 2), Some(LAND + 1));
        assert_eq!(classic_roll_after_landing(press..LAND + 1), None);
    }
    assert_eq!(classic_roll_after_landing(LAND - 7..LAND + 2), None);
    assert_eq!(
        classic_roll_after_landing(LAND + 1..LAND + 2),
        Some(LAND + 1)
    );
    assert_eq!(
        classic_roll_after_landing(LAND + 3..LAND + 4),
        Some(LAND + 3)
    );
}

/// Classic 1 rolling right off the ledge from 30 px before it, holding
/// action for `action` frames and Right throughout.
fn classic_roll_off_ledge(action: usize, jump: std::ops::Range<usize>) -> Vec<Row> {
    let mut w = classic();
    w.player.pos.x = Fixed::from_int(1570);
    play(&mut w, 100, |f| {
        let mut keys = vec![Action::Right];
        if f < action {
            keys.push(Action::Action);
        }
        if jump.contains(&f) {
            keys.push(Action::Jump);
        }
        keys
    })
}

#[test]
fn classic_rolling_off_a_ledge() {
    // spec/behavior/rolling-off-ledges.md, "When the roll ends": the roll
    // lasts to 29 frames after landing, and carries on if action is held.
    let rows = classic_roll_off_ledge(100, 0..0);
    let leave = frames_with(&rows, Event::LeaveGround)[0];
    let land = frames_with(&rows, Event::Land)[0];
    assert!(rows[leave..land].iter().all(|r| !r.grounded));
    assert!(rows[..land + 30].iter().all(|r| r.state == State::Roll));
    assert_eq!(frames_with(&rows, Event::RollEnd), [land + 33]);
    assert!(rows[leave + 1..land].iter().all(|r| r.vx <= 3.0));

    let rows = classic_roll_off_ledge(1, 0..0);
    let land = frames_with(&rows, Event::Land)[0];
    assert_eq!(frames_with(&rows, Event::RollEnd), [land + 30]);
}

#[test]
fn classic_jumping_during_a_roll_fall() {
    // spec/behavior/rolling-off-ledges.md, "Jumping during the fall": `vy`
    // is 1.75390625 at the end of L + 4, so P = L + 5.
    let rows = classic_roll_off_ledge(1, 0..0);
    let leave = frames_with(&rows, Event::LeaveGround)[0];
    assert_eq!(rows[leave + 4].vy, 1.75390625);
    let p = leave + 5;
    let rows = classic_roll_off_ledge(1, p..p + 12);
    assert_eq!(frames_with(&rows, Event::RollEnd), [p]);
    assert_eq!(rows[p].state, State::Fall);
    assert_eq!(frames_with(&rows, Event::JumpStart), [p + 1]);
    assert_eq!(rows[p + 1].state, State::Jump);
    for (frame, moved, vy) in [
        (p, 2.03515625, 2.03515625),
        (p + 1, 2.31640625, -7.0),
        (p + 2, -6.71875, -6.71875),
        (p + 3, -6.4375, -6.4375),
        (p + 12, -3.75, -3.75),
    ] {
        let row = &rows[frame];
        assert_eq!(
            (row.y - rows[frame - 1].y, row.vy),
            (moved, vy),
            "P + {}",
            frame - p
        );
    }
}

#[test]
fn classic_jumping_out_of_a_fast_roll() {
    use Action::{Action as A, Jump as J, Right as R};
    // spec/behavior/roll-speed.md, "Jumping out of a roll": `vx` 946/256 on
    // P - 1, then Right alone (and jump on P).
    let mut w = classic_running();
    hold(&mut w, &[R], 1);
    hold(&mut w, &[A, R], 1);
    assert_eq!(w.player.roll.unwrap().size, RollSize::Fast);
    let mut tries = 0;
    while grains(w.player.vel.x) != 946 {
        hold(&mut w, &[R], 1);
        tries += 1;
        assert!(tries < 28);
    }
    let rows = play(&mut w, 3, |f| if f == 0 { vec![J, R] } else { vec![R] });
    assert_eq!(frames_with(&rows, Event::RollEnd), [0]);
    assert_eq!(frames_with(&rows, Event::JumpStart), [1]);
    let vx: Vec<_> = rows.iter().map(|r| (r.vx * 256.0) as i64).collect();
    assert_eq!(vx, [955, 900, 852]);
}
