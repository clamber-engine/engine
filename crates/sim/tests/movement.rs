//! The player's movement states, played through whole worlds.

use std::collections::BTreeMap;
use std::sync::Arc;

use std::ops::{Deref, DerefMut};

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Feel, Fixed, Param};
use clamber_sim::{
    Event, Facing, Gait, InputFrame, ROLL, RollSize, SWAP_FRAMES, State, World, generic_feel, step,
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
        Play::on(level(), feel)
    }

    fn on(level: Arc<Level>, feel: Feel) -> Play {
        Play {
            world: World::new(level, Arc::new(feel)),
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

/// Horizontal speeds in Classic 1 over `frames` frames, walking right
/// from standing with Right held throughout and Down held on the frames
/// in `down`.
fn classic_crouch_speeds(frames: usize, down: &[usize]) -> Vec<i64> {
    let mut w = classic();
    let mut vx = Vec::new();
    for frame in 0..frames {
        let held: &[Action] = if down.contains(&frame) {
            &[Action::Right, Action::Down]
        } else {
            &[Action::Right]
        };
        hold(&mut w, held, 1);
        vx.push(grains(w.player.vel.x));
    }
    vx
}

#[test]
fn classic_crouch_lasts_at_least_thirteen_frames() {
    // spec/behavior/ground-speed.md, "Crouching", the short press: Down
    // held on frame 30 only. The crouch lasts to frame 42 and the target
    // stays 0 to frame 52.
    let vx = classic_crouch_speeds(60, &[30]);
    assert_eq!(vx[30], 500);
    assert_eq!(vx[31], 438);
    assert_eq!(vx[32], 384);
    assert_eq!(vx[42], 104);
    for frame in 31..=52 {
        assert!(vx[frame] < vx[frame - 1] || vx[frame] == 0, "frame {frame}");
    }
    assert!(vx[53] > vx[52]);
    // A press of 5 frames gives exactly the same speeds.
    assert_eq!(classic_crouch_speeds(60, &[30, 31, 32, 33, 34]), vx);
}

#[test]
fn classic_crouch_again_while_getting_up() {
    // spec/behavior/ground-speed.md, "Crouching": after the short press
    // on frame 30, U is frame 43. Down again on frame 45 starts a new
    // crouch with target 0 on that frame too; it lasts to frame 57, and
    // the target stays 0 to frame 67.
    let vx = classic_crouch_speeds(80, &[30, 45]);
    assert_eq!(vx[..45], classic_crouch_speeds(45, &[30]));
    for frame in 31..=67 {
        assert!(vx[frame] < vx[frame - 1] || vx[frame] == 0, "frame {frame}");
    }
    assert!(vx[68] > vx[67]);
    // The same press after U + 9 is a fresh crouch: the target is normal
    // on its first frame.
    let vx = classic_crouch_speeds(80, &[30, 53]);
    assert!(vx[53] > vx[52]);
    assert!(vx[54] < vx[53]);
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

/// The frame a turn starts on in Classic 1, holding `before` for `from`
/// frames from standing still and then `after`.
fn classic_turn_start(before: &[Action], from: usize, after: &[Action]) -> usize {
    let mut w = classic();
    // Action held from before the start, so it doesn't start a roll.
    w.held = keys(&[Action::Action]);
    let mut steps = hold(&mut w, before, from);
    steps.extend(hold(&mut w, after, 40));
    let facing = first(&steps, Event::Turn).expect("a turn");
    facing - 4
}

#[test]
fn classic_turns_while_walking_wait_for_a_turn_check() {
    // spec/behavior/ground-speed.md, "Standing, walking and running",
    // "Example": the stride counter while walking from standing still.
    let mut w = classic();
    let mut counter = Vec::new();
    for _ in 0..9 {
        hold(&mut w, &[Action::Right], 1);
        let Gait::Walking(stride) = w.player.gait else {
            panic!("not walking: {:?}", w.player.gait);
        };
        counter.push(grains(stride.left));
    }
    assert_eq!(counter, [256, 160, 64, 224, 128, 32, 192, 96, 251]);

    // Left held from frame 3 turns on frame 3; from frame 4, on frame 8;
    // from frame 9, on frame 16.
    let right = [Action::Right];
    let left = [Action::Left];
    for (from, turn) in [(3, 3), (4, 8), (9, 16)] {
        assert_eq!(classic_turn_start(&right, from, &left), turn, "{from}");
    }
}

#[test]
fn classic_turns_start_within_a_stride_check() {
    // spec/behavior/ground-speed.md, "Facing and turns": a turn starts 0
    // to 7 frames after the other way is first held while walking, and 0
    // to 12 while running.
    let walk_delays: Vec<_> = (1..=129)
        .map(|from| classic_turn_start(&[Action::Right], from, &[Action::Left]) - from)
        .collect();
    assert_eq!(walk_delays.iter().max(), Some(&7));
    assert_eq!(walk_delays.iter().min(), Some(&0));

    // With action held, running starts when step 2 ends, on frame 6.
    let run = [Action::Right, Action::Action];
    let back = [Action::Left, Action::Action];
    let run_delays: Vec<_> = (7..=129)
        .map(|from| classic_turn_start(&run, from, &back) - from)
        .collect();
    assert!(run_delays.iter().all(|&d| d <= 12), "{run_delays:?}");
    assert!(run_delays.iter().any(|&d| d > 7), "{run_delays:?}");
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

/// No `jump_start` frames.
const NONE: [usize; 0] = [];

fn classic_2() -> Play {
    Play::new(generic_feel("classic-2").unwrap())
}

/// The `jump_start` frames (take-offs, T) of Classic 2, falling to land
/// on frame `LAND` after a jump and holding jump on the frames `held`
/// gives.
fn classic_2_jumps_after_landing(held: impl Fn(usize) -> bool) -> Vec<usize> {
    let mut w = falling_in(classic_2, LAND + 1);
    // The fall comes from a jump, as in jump-input.md.
    w.player.walked_off = false;
    let mut steps = Vec::new();
    for frame in 0..LAND + 20 {
        let keys: &[Action] = if held(frame) { &[Action::Jump] } else { &[] };
        steps.extend(hold(&mut w, keys, 1));
    }
    assert_eq!(first(&steps, Event::Land), Some(LAND));
    (0..steps.len())
        .filter(|&i| steps[i].events.contains(&Event::JumpStart))
        .collect()
}

#[test]
fn classic_2_jump_takes_off_two_frames_after_the_press() {
    // spec/behavior/jump-input.md, "When a jump starts (`title-2`)": a
    // press on the ground starts a jump on P, even a one-frame tap, and
    // jump-arc.md, "Take-off frame (`title-2`)": T = P + 2, with nothing
    // visible before it and the player still on the ground on T.
    let mut w = classic_2();
    let y = w.player.pos.y;
    let mut steps = Vec::new();
    for frame in 0..4 {
        let keys: &[Action] = if frame == 0 { &[Action::Jump] } else { &[] };
        steps.extend(hold(&mut w, keys, 1));
        if frame < 3 {
            assert!(w.player.grounded(), "frame {frame}");
            assert_eq!(w.player.pos.y, y, "frame {frame}");
        }
        if frame < 2 {
            assert_eq!(w.player.vel.y, Fixed::ZERO, "frame {frame}");
            assert_eq!(steps[frame].state, State::Idle, "frame {frame}");
        }
    }
    assert_eq!(first(&steps, Event::JumpStart), Some(2));
    assert!(w.player.vel.y.is_negative());
    assert!(!w.player.grounded());
    assert_eq!(all_events(&steps).len(), 1);

    // and on every frame of a roll
    let mut w = classic_2();
    w.player.vel.x = Fixed::from_int(3);
    w.player.state = State::Roll;
    let mut steps = hold(&mut w, &[Action::Jump], 1);
    steps.extend(hold(&mut w, &[], 2));
    assert_eq!(steps[1].state, State::Roll);
    assert_eq!(steps[2].events, [Event::RollEnd, Event::JumpStart]);
}

#[test]
fn classic_2_presses_around_landing() {
    // spec/behavior/jump-input.md, "Landing (`title-2`)": every row of
    // the table, with the landing frame L = LAND. The earliest start is
    // L + 1, so the earliest take-off L + 3.
    let held_from = |p: usize, last: usize| move |f: usize| (p..=last).contains(&f);

    // pressed L - 14 to L, held through L + 1
    for press in LAND - 14..=LAND {
        let jumps = classic_2_jumps_after_landing(held_from(press, LAND + 1));
        assert_eq!(jumps, [LAND + 3], "pressed on {press}");
    }
    // pressed L - 15 or earlier, held through L + 1 and on
    for press in [LAND - 15, LAND - 17] {
        for last in [LAND + 1, LAND + 10] {
            let jumps = classic_2_jumps_after_landing(held_from(press, last));
            assert_eq!(jumps, NONE, "pressed on {press}, held until {last}");
        }
    }
    // pressed in the air (or on L), released on or before L
    for press in [LAND - 5, LAND] {
        for last in [LAND - 1, LAND].into_iter().filter(|&l| l >= press) {
            let jumps = classic_2_jumps_after_landing(held_from(press, last));
            assert_eq!(jumps, NONE, "pressed on {press}, held until {last}");
        }
    }
    // pressed on L + 1 or later: a jump starts on the press frame, even a
    // one-frame tap
    for press in LAND + 1..LAND + 6 {
        let jumps = classic_2_jumps_after_landing(|f| f == press);
        assert_eq!(jumps, [press + 2], "pressed on {press}");
    }

    // held continuously since the jump that took off
    let mut w = classic_2();
    let steps = hold(&mut w, &[Action::Jump], 150);
    assert!(first(&steps, Event::Land).is_some());
    let jumps = all_events(&steps)
        .into_iter()
        .filter(|&e| e == Event::JumpStart)
        .count();
    assert_eq!(jumps, 1);
}

/// Classic 2 walking right off the ledge at x = 1600, holding jump on the
/// frames `jump` gives: the frames of the first `leave_ground` and
/// `land`, and of every `jump_start`.
fn classic_2_walk_off(jump: impl Fn(usize) -> bool) -> (usize, Option<usize>, Vec<usize>) {
    let mut w = classic_2();
    w.player.pos.x = Fixed::from_int(1560);
    let mut steps = Vec::new();
    for frame in 0..90 {
        let keys: &[Action] = if jump(frame) {
            &[Action::Right, Action::Jump]
        } else {
            &[Action::Right]
        };
        steps.extend(hold(&mut w, keys, 1));
    }
    let jumps = (0..steps.len())
        .filter(|&i| steps[i].events.contains(&Event::JumpStart))
        .collect();
    let leave = first(&steps, Event::LeaveGround).unwrap_or(usize::MAX);
    (leave, first(&steps, Event::Land), jumps)
}

/// The leave frame L and landing frame A of Classic 2's walk off the
/// ledge without jumping.
fn classic_2_fall() -> (usize, usize) {
    let (leave, land, jumps) = classic_2_walk_off(|_| false);
    assert_eq!(jumps, NONE);
    (leave, land.unwrap())
}

#[test]
fn classic_2_jumps_near_the_edge() {
    // spec/behavior/walk-off.md, "Jumping near the edge (`title-2`)":
    // every row of the table, with one-frame taps.
    let (l, _) = classic_2_fall();
    assert!(l > 5);
    let tap = |p: usize| classic_2_walk_off(move |f| f == p);

    // L - 2 or earlier: an ordinary jump from the ground; on L - 2, T
    // falls on L and the player takes off from the ground.
    for press in [l - 5, l - 2] {
        let (leave, _, jumps) = tap(press);
        assert_eq!(jumps[0], press + 2, "pressed on {press}");
        assert!(leave > press + 2, "pressed on {press}");
    }
    // L - 1: the jump starts there, the player still leaves the ground on
    // L, and takes off on L + 1 in the air.
    let (leave, _, jumps) = tap(l - 1);
    assert_eq!((leave, jumps[0]), (l, l + 1));
    // L or L + 1: the jump starts on P and takes off on P + 2, in the air.
    for press in [l, l + 1] {
        let (leave, _, jumps) = tap(press);
        assert_eq!((leave, jumps[0]), (l, press + 2), "pressed on {press}");
    }
    // L + 2 or later: no jump in the air.
    for press in [l + 2, l + 3, l + 8] {
        let (leave, _, jumps) = tap(press);
        assert_eq!(leave, l);
        assert_eq!(jumps, NONE, "pressed on {press}");
    }
}

#[test]
fn classic_2_jumps_on_landing_after_a_fall() {
    // spec/behavior/walk-off.md, "Jumping on landing after a fall
    // (`title-2`)": every row of the table, with the landing frame A. The
    // earliest start is A + 2, so the earliest take-off A + 4.
    let (l, a) = classic_2_fall();
    assert!(a - 14 > l + 1, "the fall is long enough");
    let held_from = |p: usize, last: usize| {
        let (_, land, jumps) = classic_2_walk_off(move |f| (p..=last).contains(&f));
        assert_eq!(land, Some(a));
        jumps
    };

    // pressed on A - 13 or later (after L + 1), held through A + 2
    for press in [a - 13, a - 5, a - 1] {
        assert_eq!(held_from(press, a + 2), [a + 4], "pressed on {press}");
    }
    // pressed on A - 14 or earlier, held through A + 2
    for press in [a - 14, a - 16] {
        assert_eq!(held_from(press, a + 2), NONE, "pressed on {press}");
    }
    // pressed in the air, released before A + 2
    for last in [a - 1, a, a + 1] {
        assert_eq!(held_from(a - 5, last), NONE, "held until {last}");
    }
    // pressed on A, A + 1 or A + 2 (held through A + 2)
    for press in [a, a + 1, a + 2] {
        assert_eq!(held_from(press, a + 2), [a + 4], "pressed on {press}");
    }
    // pressed on A + 3 or later: a jump starts on the press frame
    for press in [a + 3, a + 4, a + 8] {
        assert_eq!(held_from(press, press), [press + 2], "pressed on {press}");
    }
}

#[test]
fn classic_2_crouch_goes_on_with_down_or_up_alone() {
    // spec/behavior/ground-speed.md, "Crouching (`title-2`)": a crouch
    // starts on Down without Up, goes on while exactly one of Down and Up
    // is held, and ends on the first frame U with neither or both. The
    // target is 0 from the first frame to U, and normal from U + 1.
    use Action::{Down, Right, Up};
    let walking = |w: &mut Play| {
        hold(w, &[Right], 30);
        assert!(w.player.target > Fixed::ZERO);
    };
    let targets = |w: &mut Play, inputs: &[&[Action]]| -> Vec<Fixed> {
        inputs
            .iter()
            .map(|&keys| {
                hold(w, keys, 1);
                w.player.target
            })
            .collect()
    };
    let walk = generic_feel("classic-2").unwrap().world[Param::WalkSpeed];
    let zero = Fixed::ZERO;

    // Down, then Up alone, then neither: U is the frame with neither.
    let mut w = classic_2();
    walking(&mut w);
    let t = targets(
        &mut w,
        &[
            &[Right, Down],
            &[Right, Down],
            &[Right, Up],
            &[Right, Up],
            &[Right],
            &[Right],
        ],
    );
    assert_eq!(t, [zero, zero, zero, zero, zero, walk]);

    // Both held ends it as well.
    let mut w = classic_2();
    walking(&mut w);
    let t = targets(&mut w, &[&[Right, Down], &[Right, Down, Up], &[Right]]);
    assert_eq!(t, [zero, zero, walk]);
    assert_eq!(w.player.crouch.held, 0);

    // Up alone doesn't start one.
    let mut w = classic_2();
    walking(&mut w);
    let t = targets(&mut w, &[&[Right, Up], &[Right, Up]]);
    assert_eq!(t, [walk, walk]);

    // Down and Up together stop the player without a crouch, so there's
    // no frame U after them.
    let mut w = classic_2();
    walking(&mut w);
    let t = targets(&mut w, &[&[Right, Down, Up], &[Right, Down, Up]]);
    assert_eq!(t, [zero, zero]);
    assert_eq!(w.player.crouch.held, 0);
    let t = targets(&mut w, &[&[Right], &[Right, Up, Down], &[Right, Down]]);
    assert_eq!(t, [walk, zero, zero]);
    assert!(w.player.crouch.held > 0, "Down without Up starts one");

    // Default, Snappy and Floaty don't crouch.
    for preset in ["default", "snappy", "floaty"] {
        let mut w = Play::new(generic_feel(preset).unwrap());
        walking(&mut w);
        let t = targets(&mut w, &[&[Right, Down], &[Right, Up], &[Right]]);
        assert!(t.iter().all(|&t| t > Fixed::ZERO), "{preset}");
    }
}

#[test]
fn classic_2_no_turn_while_crouching_or_just_after() {
    // spec/behavior/ground-speed.md, "Crouching (`title-2`)": from U to
    // U + 10 no turn can start, even with the other way held; one can
    // on U + 11. Nor can one while crouching.
    use Action::{Down, Left};
    let mut w = classic_2();
    assert_eq!(w.player.facing, Facing::Right);
    hold(&mut w, &[Down], 3);
    hold(&mut w, &[Down, Left], 3);
    assert_eq!(w.player.facing, Facing::Right);
    // U is the frame Down is let go of.
    for frame in 0..=10 {
        hold(&mut w, &[Left], 1);
        assert_eq!(w.player.facing, Facing::Right, "U + {frame}");
    }
    hold(&mut w, &[Left], 1);
    assert_eq!(w.player.facing, Facing::Left, "U + 11");

    // Down without Up during U to U + 10 starts a new crouch, with its
    // own U after it.
    let mut w = classic_2();
    hold(&mut w, &[Down], 3);
    hold(&mut w, &[], 5);
    hold(&mut w, &[Down], 1);
    assert!(w.player.crouch.held > 0);
    hold(&mut w, &[Left], 11);
    assert_eq!(w.player.facing, Facing::Right);
    hold(&mut w, &[Left], 1);
    assert_eq!(w.player.facing, Facing::Left);
}

/// The `jump_start` frames of Classic 2 after crouching for 10 frames
/// and then holding `inputs`, one a frame, from G = 10.
fn classic_2_crouch_jumps(inputs: &[&[Action]]) -> (Play, Vec<usize>) {
    let mut w = classic_2();
    let mut steps = hold(&mut w, &[Action::Down], 10);
    for &keys in inputs {
        steps.extend(hold(&mut w, keys, 1));
    }
    steps.extend(hold(&mut w, &[], 20));
    let jumps = (0..steps.len())
        .filter(|&i| steps[i].events.contains(&Event::JumpStart))
        .collect();
    (w, jumps)
}

#[test]
fn classic_2_jumps_from_a_crouch() {
    // spec/behavior/jump-input.md, "Crouching (`title-2`)": every row of
    // the table. G is the frame jump is pressed.
    use Action::{Down, Jump, Up};
    const G: usize = 10;
    let jumps = |inputs: &[&[Action]]| classic_2_crouch_jumps(inputs).1;

    // Down + jump, then Down, Down: take-off on G + 2.
    assert_eq!(jumps(&[&[Down, Jump], &[Down], &[Down]]), [G + 2]);
    // Down + jump, then Up, Up: take-off on G + 2.
    assert_eq!(jumps(&[&[Down, Jump], &[Up], &[Up]]), [G + 2]);
    // Down + jump, then nothing: no jump; getting up starts on G + 1.
    let mut w = classic_2();
    hold(&mut w, &[Down], G);
    hold(&mut w, &[Down, Jump], 1);
    assert!(w.player.crouch.held > 0);
    hold(&mut w, &[], 1);
    assert_eq!(w.player.crouch.held, 0);
    assert_eq!(jumps(&[&[Down, Jump]]), NONE);
    // Down + jump, then Down + jump, then nothing: no jump; getting up
    // starts on G + 2.
    let mut w = classic_2();
    hold(&mut w, &[Down], G);
    hold(&mut w, &[Down, Jump], 2);
    assert!(w.player.crouch.held > 0);
    hold(&mut w, &[], 1);
    assert_eq!(w.player.crouch.held, 0);
    assert_eq!(jumps(&[&[Down, Jump], &[Down, Jump]]), NONE);
    // Down + jump, then jump held: the jump starts over on G + 1 and
    // takes off on G + 3.
    assert_eq!(jumps(&[&[Down, Jump], &[Jump], &[Jump], &[Jump]]), [G + 3]);
    // Jump alone (Down released on G): take-off on G + 2.
    assert_eq!(jumps(&[&[Jump]]), [G + 2]);
    assert_eq!(jumps(&[&[Jump], &[Down]]), [G + 2]);
}

#[test]
fn classic_2_jump_held_from_take_off_to_landing() {
    // spec/behavior/jump-arc.md, "Held from take-off to landing
    // (`title-2`)". G is step 0 and T step 2.
    let steps = jump_in(classic_2, usize::MAX);
    assert!(steps[2].0.events.contains(&Event::JumpStart));
    for (n, y, vy) in [
        (0, 0.0, -8.21875),
        (1, -7.8671875, -7.8671875),
        (2, -15.3828125, -7.515625),
        (5, -35.8203125, -6.4609375),
        (10, -62.8515625, -4.703125),
        (20, -90.546875, -1.1875),
        (23, -92.0, -0.1328125),
        (24, -91.78125, 0.21875),
        (25, -91.125, 0.65625),
        (30, -81.28125, 2.84375),
        (40, -28.78125, 7.21875),
        (41, -21.125, 7.65625),
        (42, -13.125, 8.0),
        (43, -5.125, 8.0),
    ] {
        let (_, sy, svy) = &steps[2 + n];
        assert_eq!((*sy, *svy), (y, vy), "T + {n}");
    }
    // T + 44 lands, placed on the floor.
    assert_eq!(steps.len(), 2 + 45);
    assert!(steps[2 + 44].0.events.contains(&Event::Land));
    assert_eq!(steps[2 + 44].1, 0.0);
}

#[test]
fn classic_2_jump_height_and_airtime_by_hold() {
    // spec/behavior/jump-arc.md, "How long jump is held (`title-2`)", for
    // H frames held from G, and "Example (`title-2`)" (H = 20 lands on
    // G + 45).
    for (held, apex, airtime) in [
        (1, 73.125, 37),
        (2, 73.125, 37),
        (3, 73.125, 37),
        (4, 74.671875, 37),
        (5, 76.2109375, 38),
        (6, 77.671875, 38),
        (8, 80.3359375, 39),
        (10, 82.7265625, 40),
        (12, 84.875, 40),
        (15, 87.515625, 41),
        (18, 89.578125, 42),
        (20, 90.578125, 43),
        (22, 91.3515625, 43),
        (25, 91.9140625, 44),
        (26, 92.0, 44),
        (40, 92.0, 44),
        (usize::MAX, 92.0, 44),
    ] {
        let steps = jump_in(classic_2, held);
        let top = steps.iter().map(|s| -s.1).fold(0.0, f64::max);
        assert_eq!(top, apex, "held {held}");
        // the jump takes off on step 2 and lands on the last
        assert!(steps[2].0.events.contains(&Event::JumpStart));
        assert_eq!(steps.len() - 3, airtime, "held {held}");
    }
}

/// Flat ground at y = 400 from x = 0 to 1600, and floor 64 px lower
/// beyond it.
const LEDGE_64: &str = r#"
    format = 1
    name = "ledge"
    size = [4096, 600]
    start = [64, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 400, 1600, 200]

    [[collision]]
    shape = "rect"
    rect = [1600, 464, 2496, 136]
"#;

/// Classic 2 walking right from x = 1560 off the ledge at x = 1600 of
/// `level`, holding `held(n)` on frame L + n, where L is the frame it
/// leaves the ground on with Right alone held; `held` should hold Right
/// before L. Rows from frame L to L + `after`.
fn classic_2_off(level: &str, after: usize, held: impl Fn(i64) -> Vec<Action>) -> Vec<Row> {
    let new = || {
        let level = Arc::new(Level::from_toml_str(level, "ledge").unwrap());
        let mut w = Play::on(level, generic_feel("classic-2").unwrap());
        w.player.pos.x = Fixed::from_int(1560);
        w
    };
    let probe = play(&mut new(), 60, |_| vec![Action::Right]);
    let l = frames_with(&probe, Event::LeaveGround)[0];
    let mut w = new();
    let rows = play(&mut w, l + after + 1, |f| held(f as i64 - l as i64));
    assert_eq!(
        frames_with(&rows, Event::LeaveGround),
        [l],
        "the leave frame"
    );
    rows.into_iter().skip(l).collect()
}

/// `y` after each frame, measured down from the ledge at y = 400.
fn below_ledge(rows: &[Row]) -> Vec<f64> {
    rows.iter().map(|r| r.y - 400.0).collect()
}

#[test]
fn classic_2_walking_off_a_ledge() {
    // spec/behavior/walk-off.md, "The leave frame (`title-2`)" and
    // "Falling (`title-2`)": `vy` after the frame and the total fallen
    // below the ledge, by frames after L.
    let rows = classic_2_off(FIELD, 22, |_| vec![Action::Right]);
    let fallen = below_ledge(&rows);
    for (n, vy, total) in [
        (0, 0.00390625, 3.4375),
        (1, 1.4375, 4.875),
        (2, 1.875, 6.75),
        (5, 3.1875, 15.0),
        (10, 5.375, 37.5),
        (15, 7.5625, 70.9375),
        (16, 8.0, 78.9375),
        (20, 8.0, 110.9375),
    ] {
        assert_eq!((rows[n].vy, fallen[n]), (vy, total), "L + {n}");
    }
    assert!(rows.iter().all(|r| !r.grounded));
}

#[test]
fn classic_2_lands_on_reaching_the_floor() {
    // spec/behavior/walk-off.md, "Falling (`title-2`)", the example off a
    // 64 px ledge: 60.5625 px above the floor after L, 0.625 px after
    // L + 14, which stays airborne, and it lands on L + 15.
    let rows = classic_2_off(LEDGE_64, 16, |_| vec![Action::Right]);
    let above = |n: usize| 464.0 - rows[n].y;
    assert_eq!(above(0), 60.5625);
    assert_eq!(above(14), 0.625);
    assert!(!rows[14].grounded);
    assert_eq!(frames_with(&rows, Event::Land), [15]);
    assert_eq!(above(15), 0.0);
    assert!(rows[15].grounded);
}

#[test]
fn classic_2_facing_waits_two_frames_after_walking_off() {
    // spec/behavior/walk-off.md, "Falling (`title-2`)": facing doesn't
    // change on L or L + 1; holding the other way from L on turns the
    // player on L + 2.
    use Action::{Left, Right};
    let rows = classic_2_off(FIELD, 4, |n| vec![if n < 0 { Right } else { Left }]);
    let facing: Vec<Facing> = rows.iter().map(|r| r.facing).collect();
    use Facing::Left as FL;
    use Facing::Right as FR;
    assert_eq!(facing, [FR, FR, FL, FL, FL]);
    assert_eq!(frames_with(&rows, Event::Turn), [2]);
}

#[test]
fn classic_2_jumps_near_the_edge_fall_until_take_off() {
    // spec/behavior/walk-off.md, "Jumping near the edge (`title-2`)",
    // the example: Jump pressed on L - 1, L or L + 1 and held for 20
    // frames, height below the ledge after frames L to L + 4. `vy` is
    // -8.21875 on T and -7.8671875 on T + 1.
    use Action::{Jump, Right};
    for (press, heights) in [
        (
            -1,
            [
                3.3515625,
                3.70703125,
                -4.16015625,
                -11.67578125,
                -18.83984375,
            ],
        ),
        (0, [3.3515625, 3.70703125, 4.4140625, -3.453125, -10.96875]),
        (1, [3.4375, 3.79296875, 4.5, 5.55859375, -2.30859375]),
    ] {
        let rows = classic_2_off(FIELD, 4, |n| {
            if (press..press + 20).contains(&n) {
                vec![Right, Jump]
            } else {
                vec![Right]
            }
        });
        assert_eq!(below_ledge(&rows), heights, "pressed on L + {press}");
        let t = (press + 2) as usize;
        assert_eq!(frames_with(&rows, Event::JumpStart), [t]);
        assert_eq!((rows[t].vy, rows[t + 1].vy), (-8.21875, -7.8671875));
    }

    // A tap on L - 1 isn't held on L, so L moves 3.4375 px and L + 1
    // (T) falls under 0.4375, as does the jump after it.
    let rows = classic_2_off(FIELD, 2, |n| {
        if n == -1 {
            vec![Right, Jump]
        } else {
            vec![Right]
        }
    });
    assert_eq!(below_ledge(&rows), [3.4375, 3.87890625, -3.90234375]);
    assert_eq!((rows[1].vy, rows[2].vy), (-8.21875, -7.78125));

    // Facing doesn't change from L to T, and follows the input from
    // T + 1.
    let rows = classic_2_off(FIELD, 5, |n| match n {
        ..=-1 => vec![Right],
        1 => vec![Action::Left, Jump],
        _ => vec![Action::Left],
    });
    let facing: Vec<Facing> = rows.iter().map(|r| r.facing).collect();
    use Facing::Left as FL;
    use Facing::Right as FR;
    assert_eq!(facing, [FR, FR, FR, FR, FL, FL]);
}

#[test]
fn classic_2_crouching_in_a_fall() {
    // spec/behavior/walk-off.md, "Crouching on the leave frame
    // (`title-2`)", the example: Down pressed on L and held until
    // L + 11, released with Right held (U = L + 12). The target is 0 to
    // U, `vy` grows by 0.4375 from 1/256 to U, and is 1.4375 on U + 1.
    use Action::{Down, Left, Right};
    let rows = classic_2_off(FIELD, 14, |n| match n {
        0..=11 => vec![Right, Down],
        _ => vec![Right],
    });
    for (n, row) in rows.iter().enumerate().take(13) {
        assert_eq!(row.target, 0.0, "L + {n}");
        assert_eq!(row.vy, 1.0 / 256.0 + 0.4375 * n as f64, "L + {n}");
    }
    assert_eq!(rows[12].vy, 5.25390625);
    assert_eq!(rows[13].vy, 1.4375);
    assert!(rows[13].target > 0.0);
    assert!(rows.iter().all(|r| !r.grounded));

    // Up alone keeps it going in the air too.
    let rows = classic_2_off(FIELD, 6, |n| match n {
        0..=1 => vec![Right, Down],
        2..=4 => vec![Right, Action::Up],
        _ => vec![Right],
    });
    assert!(rows[..6].iter().all(|r| r.target == 0.0));
    assert_eq!(
        (rows[5].vy, rows[6].vy),
        (1.0 / 256.0 + 0.4375 * 5.0, 1.4375)
    );

    // Facing doesn't change while crouched, on U or on U + 1, and follows
    // the input from U + 2.
    let rows = classic_2_off(FIELD, 14, |n| match n {
        ..0 => vec![Right],
        0..=11 => vec![Left, Down],
        _ => vec![Left],
    });
    assert!(rows[..14].iter().all(|r| r.facing == Facing::Right));
    assert_eq!(rows[14].facing, Facing::Left);

    // A crouch held through the landing lands crouched.
    let rows = classic_2_off(FIELD, 40, |n| {
        if n < 0 {
            vec![Right]
        } else {
            vec![Right, Down]
        }
    });
    let land = frames_with(&rows, Event::Land)[0];
    assert!(rows[land..].iter().all(|r| r.target == 0.0));
    // and keeps falling from 1/256 without a step to 1.4375
    assert_eq!(rows[land - 1].vy, 8.0);
    assert_eq!(rows[16].vy, 1.0 / 256.0 + 0.4375 * 16.0);
}

#[test]
fn classic_2_jumping_while_crouched_in_the_air() {
    // spec/behavior/walk-off.md, "Jumping while crouched in the air
    // (`title-2`)".
    use Action::{Down, Jump, Right};
    let crouched_then = |inputs: Vec<(i64, Vec<Action>)>| {
        classic_2_off(FIELD, 8, move |n| match n {
            ..0 => vec![Right],
            _ => inputs
                .iter()
                .find(|(m, _)| *m == n)
                .map_or(vec![Down], |(_, keys)| keys.clone()),
        })
    };

    // The example: Down from L, Down and Jump on L + 3, then Down alone.
    // G = L + 3, T = L + 5, with 0.3515625 from G to T.
    let rows = crouched_then(vec![(3, vec![Down, Jump])]);
    assert_eq!(frames_with(&rows, Event::JumpStart), [5]);
    let vy: Vec<f64> = rows[2..5].iter().map(|r| r.vy).collect();
    assert_eq!(vy, [0.87890625, 1.23046875, 1.58203125]);
    assert_eq!(rows[5].y - rows[4].y, 1.93359375);
    assert_eq!(rows[5].vy, -8.21875);

    // However long the crouch has lasted.
    let rows = crouched_then(vec![(6, vec![Down, Jump])]);
    assert_eq!(frames_with(&rows, Event::JumpStart), [8]);

    // A press on U (Jump as Down is let go) is a normal jump, with the
    // target 0 on G; on U + 1 it starts a jump too, but not on U + 2.
    let neither = |from: i64, press: i64| {
        crouched_then(
            (from..9)
                .map(|n| (n, if n == press { vec![Jump] } else { vec![] }))
                .collect(),
        )
    };
    let rows = neither(3, 3);
    assert_eq!(frames_with(&rows, Event::JumpStart), [5]);
    assert_eq!(rows[3].target, 0.0);
    // jump is let go on G + 1, so the 0.4375 there and on T
    assert_eq!(rows[3].vy, 1.23046875);
    assert_eq!(rows[4].vy, 1.66796875);
    assert_eq!(frames_with(&neither(3, 4), Event::JumpStart), [6]);
    assert_eq!(frames_with(&neither(3, 5), Event::JumpStart), NONE);
    // without that press, U + 1 is the 1.4375 frame
    assert_eq!(neither(3, 99)[4].vy, 1.4375);

    // The crouch ending on G + 1 cancels the jump. U is still under
    // 0.3515625 and U + 1 is the 1.4375 frame.
    let rows = crouched_then(vec![(3, vec![Down, Jump]), (4, vec![]), (5, vec![])]);
    assert_eq!(frames_with(&rows, Event::JumpStart), NONE);
    assert_eq!((rows[4].vy, rows[5].vy), (1.58203125, 1.4375));
    // With Jump held on U, a new jump starts there: G = U = L + 4.
    let rows = crouched_then(vec![
        (3, vec![Down, Jump]),
        (4, vec![Jump]),
        (5, vec![Jump]),
        (6, vec![Jump]),
    ]);
    assert_eq!(frames_with(&rows, Event::JumpStart), [6]);
    assert_eq!(rows[4].target, 0.0);
    assert_eq!(rows[5].vy, 1.58203125 + 0.3515625);

    // No jump in the air without a crouch.
    let rows = classic_2_off(FIELD, 8, |n| {
        if n == 4 {
            vec![Right, Jump]
        } else {
            vec![Right]
        }
    });
    assert_eq!(frames_with(&rows, Event::JumpStart), NONE);
}

/// A Classic 1 jump from standing, pressed on step 0 and held for `held`
/// steps: `y` relative to the start and `vy` after each step, up to and
/// including the landing.
fn classic_jump(held: usize) -> Vec<(Step, f64, f64)> {
    jump_in(classic, held)
}

/// As [`classic_jump`], in the world `new` makes.
fn jump_in(new: fn() -> Play, held: usize) -> Vec<(Step, f64, f64)> {
    let mut w = new();
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

/// Classic 1 standing at x = 500, clear of the one-way platform.
fn classic_clear() -> Play {
    let mut w = classic();
    w.player.pos.x = Fixed::from_int(500);
    w
}

/// Classic 1 walking right at +2, from x = 500.
fn classic_walking() -> Play {
    let mut w = classic_clear();
    hold(&mut w, &[Action::Right], 60);
    assert_eq!(w.player.vel.x, Fixed::from_int(2));
    w
}

/// A Classic 1 jump with Jump held from the press P (row 0) to landing:
/// `on_p` held with it on P, `on_t` on the take-off frame T (row 1) and
/// `air` from T + 1. All rows up to and including the landing.
fn classic_air_jump(mut w: Play, on_p: &[Action], on_t: &[Action], air: &[Action]) -> Vec<Row> {
    let with_jump = |keys: &[Action]| [keys, &[Action::Jump]].concat();
    let mut rows = Vec::new();
    while rows.len() < 200 {
        let keys = match rows.len() {
            0 => on_p,
            1 => on_t,
            _ => air,
        };
        rows.extend(play(&mut w, 1, |_| with_jump(keys)));
        if rows.last().unwrap().events.contains(&Event::Land) {
            assert_eq!(frames_with(&rows, Event::JumpStart), [1]);
            return rows;
        }
    }
    panic!("never landed");
}

#[test]
fn classic_air_control_worked_values() {
    // spec/behavior/air-control.md, "Worked values": P is frame 60, so
    // frame 80 is row 20.
    let (r, l) = (Action::Right, Action::Left);
    let mut w = classic_walking();
    let rows = play(&mut w, 27, |f| {
        let dir = if f <= 20 { r } else { l };
        vec![dir, Action::Jump]
    });
    for (frame, vx) in [
        (80, 512),
        (81, 384),
        (82, 272),
        (83, 174),
        (84, 89),
        (85, 14),
        (86, -51),
    ] {
        let (row, before) = (&rows[frame - 60], &rows[frame - 61]);
        assert!(!row.grounded, "frame {frame}");
        assert_eq!(row.vx * 256.0, vx as f64, "frame {frame}");
        assert_eq!(row.x - before.x, row.vx, "frame {frame}");
    }

    // From rest, Right held from T on.
    let rows = classic_air_jump(classic_clear(), &[], &[r], &[r]);
    let vx: Vec<f64> = rows[1..4].iter().map(|row| row.vx).collect();
    assert_eq!(vx, [0.25, 0.46875, 0.66015625]);
}

/// A row of the jump distance table: how the player starts, what is held
/// on P, on T and in the air, the distance and `vx` on landing.
type DistanceRow<'a> = (
    fn() -> Play,
    &'a [Action],
    &'a [Action],
    &'a [Action],
    f64,
    f64,
);

#[test]
fn classic_jump_distances() {
    // spec/behavior/air-control.md, "Jump distances": `x` on landing
    // minus `x` on T, and `vx` on landing. Landing is on T + 49.
    let (r, l, a) = (Action::Right, Action::Left, Action::Action);
    let standing = classic_clear as fn() -> Play;
    let walking = classic_walking as fn() -> Play;
    let running = || {
        let mut w = classic_clear();
        hold(&mut w, &[Action::Action, Action::Right], 100);
        assert_eq!(w.player.vel.x, Fixed::from_int(3));
        w
    };
    let running = running as fn() -> Play;
    let table: [DistanceRow; 10] = [
        (standing, &[], &[], &[], 0.0, 0.0),
        (standing, &[], &[r], &[r], 85.5234375, 2.0),
        (standing, &[], &[], &[r, a], 125.74609375, 3.0),
        (walking, &[r], &[r], &[r], 98.0, 2.0),
        (walking, &[r], &[], &[], 12.4765625, 0.0),
        (walking, &[r], &[l], &[l], -73.19140625, -2.0),
        (running, &[r, a], &[r, a], &[r, a], 147.0, 3.0),
        (running, &[r, a], &[r], &[r], 104.37109375, 2.0),
        (running, &[r, a], &[], &[], 18.62890625, 0.0),
        (running, &[r, a], &[l, a], &[l, a], -109.9375, -3.0),
    ];
    for (n, (before, on_p, on_t, air, distance, vx)) in table.into_iter().enumerate() {
        let rows = classic_air_jump(before(), on_p, on_t, air);
        assert_eq!(rows.len(), 1 + 50, "row {n}");
        let landing = rows.last().unwrap();
        assert_eq!(
            (landing.x - rows[1].x, landing.vx),
            (distance, vx),
            "row {n}"
        );
    }

    // The mirror image: walking left with Left held.
    let mut w = classic();
    w.player.pos.x = Fixed::from_int(1000);
    hold(&mut w, &[l], 60);
    let rows = classic_air_jump(w, &[l], &[l], &[l]);
    let landing = rows.last().unwrap();
    assert_eq!((landing.x - rows[1].x, landing.vx), (-98.0, -2.0));

    // Jump held for 5 frames from P, walking with Right held: lands on
    // T + 33 after 66 px.
    let mut w = classic_walking();
    let rows = play(&mut w, 40, |f| {
        if f < 5 {
            vec![r, Action::Jump]
        } else {
            vec![r]
        }
    });
    assert_eq!(frames_with(&rows, Event::JumpStart), [1]);
    assert_eq!(frames_with(&rows, Event::Land), [34]);
    assert_eq!(rows[34].x - rows[1].x, 66.0);
}

#[test]
fn classic_facing_in_a_jump_follows_the_input() {
    // spec/behavior/air-control.md, "Facing": no 8-frame turn in the air;
    // facing changes on the first frame a direction is held, with `turn`.
    let (r, l) = (Action::Right, Action::Left);
    let air: Vec<Vec<Action>> = vec![
        vec![],
        vec![l],
        vec![l],
        vec![],
        vec![l, r],
        vec![r],
        vec![r],
        vec![l],
    ];
    let mut w = classic_walking();
    let mut rows = play(&mut w, 2, |_| vec![r, Action::Jump]);
    let n = air.len();
    rows.extend(play(&mut w, n, |f| [&air[f][..], &[Action::Jump]].concat()));
    let facings: Vec<Facing> = rows[2..].iter().map(|row| row.facing).collect();
    use Facing::{Left as FL, Right as FR};
    assert_eq!(facings, [FR, FL, FL, FL, FL, FR, FR, FL]);
    let turns: Vec<bool> = rows[2..]
        .iter()
        .map(|row| row.events.contains(&Event::Turn))
        .collect();
    assert_eq!(turns, [false, true, false, false, false, true, false, true]);
    assert!(rows[2..].iter().all(|row| !row.grounded));
    // The speed is unaffected: still moving right while facing left.
    assert!(rows[3].vx > 0.0);

    // Walking right, Jump and Left pressed together on P: facing is
    // still right on P and becomes left on T.
    let rows = classic_air_jump(classic_walking(), &[l], &[l], &[l]);
    assert_eq!((rows[0].facing, rows[1].facing), (FR, FL));
    assert!(rows[1].events.contains(&Event::Turn));
}

#[test]
fn classic_facing_doesnt_change_in_a_walk_off_fall() {
    // spec/behavior/air-control.md, "Facing": falling after walking off
    // a ledge, facing stays the way the player walked off; the speed
    // still follows the rule from L + 1 on.
    let mut w = classic();
    w.player.pos.x = Fixed::from_int(1500);
    let mut steps = Vec::new();
    while first(&steps, Event::LeaveGround).is_none() {
        steps.extend(hold(&mut w, &[Action::Right], 1));
    }
    let mut prev = grains(w.player.vel.x);
    // 10 frames: before landing, and before holding Left takes the
    // player back into the wall below the ledge.
    let rows = play(&mut w, 10, |_| vec![Action::Left]);
    for (k, row) in rows.iter().enumerate() {
        let n = k + 1;
        assert!(!row.grounded, "L + {n}");
        assert_eq!(row.facing, Facing::Right, "L + {n}");
        assert!(!row.events.contains(&Event::Turn), "L + {n}");
        let step = (-512 - prev) / 8;
        let expected = if step == 0 { -512 } else { prev + step };
        let got = grains(Fixed::from_f64(row.vx).unwrap());
        assert_eq!(got, expected, "L + {n}");
        prev = got;
    }
    assert!(prev < 0);
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
    target: f64,
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
                target: p.target.to_f64(),
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
