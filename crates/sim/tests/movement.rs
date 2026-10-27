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
fn a_roll_with_a_direction_held_flows_into_moving() {
    // Clamber's own presets: a roll with its direction held ends at
    // `roll_flow_speed` and carries on from that speed, running with
    // Action held and walking without.
    use Action::{Action as A, Down as D, Right as R};
    for name in ["default", "snappy", "floaty"] {
        let feel = generic_feel(name).unwrap();
        let flow = feel.world[Param::RollFlowSpeed];
        for (keys, cap, state) in [
            (vec![R, A], feel.world[Param::TopSpeed], State::Run),
            (vec![R], feel.world[Param::WalkSpeed], State::Walk),
        ] {
            let mut w = Play::new(feel.clone());
            hold(&mut w, &[R, A], 60);
            hold(&mut w, &[R, A, D], 1);
            let steps = hold(&mut w, &keys, 150);
            let end = first(&steps, Event::RollEnd).unwrap();
            assert!(steps[..end].iter().all(|s| s.state == State::Roll));
            assert!(
                steps[end..].iter().all(|s| s.state == state),
                "{name} {keys:?}"
            );
            assert_eq!(all_events(&steps), [Event::RollEnd], "{name} {keys:?}");
            assert_eq!(w.player.vel.x, cap, "{name} {keys:?}");

            // and from the speed it ended at, without stopping first
            let mut w = Play::new(feel.clone());
            hold(&mut w, &[R, A], 60);
            hold(&mut w, &[R, A, D], 1);
            hold(&mut w, &keys, end + 1);
            assert!(w.player.vel.x >= flow - feel.world[Param::RollFriction]);
            assert!(w.player.vel.x <= flow);
            // A roll can start again straight away.
            let steps = hold(&mut w, &[R, A, D], 1);
            assert_eq!(steps[0].events, [Event::RollStart], "{name} {keys:?}");
        }
    }
}

#[test]
fn classic_rolls_ignore_roll_flow_speed() {
    // A timed roll (Classic 1) ends as spec/behavior/roll-speed.md says,
    // with or without `roll_flow_speed`.
    use Action::{Action as A, Right as R};
    let run = |flow: f64| {
        let mut feel = generic_feel("classic-1").unwrap();
        feel.world
            .set(Param::RollFlowSpeed, Fixed::from_f64(flow).unwrap());
        let mut w = Play::new(feel);
        play(&mut w, 60, |f| if f == 0 { vec![A] } else { vec![A, R] })
            .iter()
            .map(|r| (r.state, r.events.clone(), r.x, r.vx))
            .collect::<Vec<_>>()
    };
    let rows = run(0.0);
    assert!(rows.iter().any(|r| r.1.contains(&Event::RollEnd)));
    assert_eq!(rows, run(2.5));
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
fn classic_walking_or_a_turn_starts_on_u_plus_10() {
    // spec/behavior/player-frames.md, "Crouching": walking right, Down
    // also held on frames 20 to 59, so U is 60. The player stands to
    // U + 9 even with a direction held, facing doesn't change, and walking
    // or a turn starts on U + 10.
    for (way, turns) in [(Action::Right, false), (Action::Left, true)] {
        let mut w = classic();
        let mut rows = Vec::new();
        for frame in 0..75 {
            let held: &[Action] = match frame {
                0..20 => &[Action::Right],
                20..40 => &[Action::Right, Action::Down],
                40..60 => &[way, Action::Down],
                _ => &[way],
            };
            hold(&mut w, held, 1);
            rows.push((w.player.gait, w.player.turn, w.player.facing));
        }
        for (frame, &(gait, turn, facing)) in rows.iter().enumerate().take(70).skip(21) {
            assert_eq!(mode(gait), ("standing", 0), "{way:?} frame {frame}");
            assert_eq!(turn, None, "{way:?} frame {frame}");
            assert_eq!(facing, Facing::Right, "{way:?} frame {frame}");
        }
        let (gait, turn, _) = rows[70];
        if turns {
            assert!(turn.is_some());
        } else {
            assert_eq!(mode(gait), ("walking", 256));
        }
    }
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
fn immediate_jump_starts_on_the_press_frame() {
    // #437: with the feel immediate, a press is answered on its own frame,
    // like a roll's; a one-frame tap included.
    let mut feel = generic_feel("classic-1").unwrap();
    feel.immediate = true;
    let mut w = Play::new(feel);
    let mut steps = hold(&mut w, &[Action::Jump], 1);
    steps.extend(hold(&mut w, &[], 2));
    assert_eq!(first(&steps, Event::JumpStart), Some(0));
    let starts = all_events(&steps)
        .into_iter()
        .filter(|&e| e == Event::JumpStart)
        .count();
    assert_eq!(starts, 1);
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
    let held_from = |p: usize, last: usize| move |f: usize| (p..=last).contains(&f);
    // spec/behavior/jump-input.md: a press on or after L starts a jump on
    // max(P + 1, E) if Jump is held up to the frame before, so a one-frame
    // tap only from E - 1 on.
    for press in LAND..LAND + 6 {
        let jump = classic_jump_after_landing(held_from(press, LAND + 10));
        assert_eq!(jump, Some((press + 1).max(EARLIEST)), "pressed on {press}");
        let tap = classic_jump_after_landing(|f| f == press);
        let expected = (press + 1 >= EARLIEST).then_some(press + 1);
        assert_eq!(tap, expected, "tapped on {press}");
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

/// A standing jump in Classic 1 in a world from `new`, pressed on frame
/// 0 and held for `held` frames, played twice: the second time with
/// Jump pressed again on the frame before landing and held. Returns the frame the jump starts on (T),
/// its approach frame, its landing frame and the next jump's start.
fn classic_standing_jump(
    new: impl Fn() -> Play,
    held: usize,
) -> (usize, usize, usize, Option<usize>) {
    let mut probe = new();
    let mut steps = hold(&mut probe, &[Action::Jump], held);
    steps.extend(hold(&mut probe, &[], 80));
    let start = first(&steps, Event::JumpStart).unwrap();
    let land = first(&steps, Event::Land).unwrap();
    let approach = start + usize::from(probe.player.approach.unwrap_or(20));

    let mut w = new();
    let mut steps = hold(&mut w, &[Action::Jump], held);
    steps.extend(hold(&mut w, &[], land - 1 - held));
    steps.extend(hold(&mut w, &[Action::Jump], 20));
    assert_eq!(first(&steps, Event::Land), Some(land));
    let again = steps
        .iter()
        .enumerate()
        .skip(land)
        .find(|(_, s)| s.events.contains(&Event::JumpStart))
        .map(|(i, _)| i);
    (start, approach, land, again)
}

#[test]
fn classic_landing_waits_for_the_switch_frame() {
    // spec/behavior/jump-input.md, "The switch frame M": standing jumps
    // on flat ground, Jump pressed on T - 1, with E = M + 2 found by
    // pressing Jump on L - 1 and holding it.
    for (held, approach, land, earliest) in [
        (1, 20, 31, 3),
        (10, 20, 36, 2),
        (14, 24, 38, 2),
        (20, 29, 41, 3),
    ] {
        let (t, a, l, e) = classic_standing_jump(classic, held);
        assert_eq!(t, 1, "held {held}");
        assert_eq!(a - t, approach, "held {held}");
        assert_eq!(l - t, land, "held {held}");
        assert_eq!(e, Some(l + earliest), "held {held}");
    }
}

/// Flat ground at y = 400, with a one-way platform 50 px above it over
/// the start.
const LOW_SHELF: &str = r#"
    format = 1
    name = "shelf"
    size = [1024, 600]
    start = [64, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 400, 1024, 200]

    [[collision]]
    shape = "segment"
    from = [0, 350]
    to = [200, 350]
    one_way = true
"#;

#[test]
fn classic_landing_before_the_approach_frame() {
    // spec/behavior/jump-input.md: a landing before the approach frame A
    // has M = A + 4, so E = A + 6 with A = T + 20.
    let shelf = || {
        let level = Arc::new(Level::from_toml_str(LOW_SHELF, "shelf").unwrap());
        Play::on(level, generic_feel("classic-1").unwrap())
    };
    let (t, a, l, e) = classic_standing_jump(shelf, 1);
    assert!(l < a, "lands on {l}, approach {a}");
    assert_eq!(a, t + 20);
    assert_eq!(e, Some(a + 6));
}

#[test]
fn classic_landing_after_a_roll_jump_waits_for_the_switch_frame() {
    // spec/behavior/jump-input.md, "The switch frame M": the first
    // character's jumps out of a roll pressed on the ground land as any
    // other jump, with E = M + 2, not L + 3.
    let rolling = || {
        let mut w = classic();
        w.player.vel.x = Fixed::from_int(3);
        w.player.state = State::Roll;
        w
    };
    for held in [1, 10, 14, 20] {
        let jump = classic_standing_jump(rolling, held);
        assert_eq!(jump, classic_standing_jump(classic, held), "held {held}");
    }
    let (_, _, l, e) = classic_standing_jump(rolling, 10);
    assert_eq!(e, Some(l + 2));
}

/// Classic 1 with the second character as the player, rolling at 3 px a
/// frame if `rolling`.
fn classic_second(rolling: bool) -> Play {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: clamber_pack::Presets::builtin()
            .slot("classic-1", 1)
            .unwrap(),
        abilities: feel.roster[0].abilities.clone(),
    });
    let mut w = Play::new(feel);
    w.player.character = 1;
    if rolling {
        w.player.vel.x = Fixed::from_int(3);
        w.player.state = State::Roll;
    }
    w
}

#[test]
fn classic_second_character_landing_has_its_switch_frame_on_landing() {
    // spec/behavior/jump-input.md, "The second character": its landings
    // after jumps from the floor or out of a roll pressed on the ground
    // have no stage frames, M = L, so E = L + 2 (the first character's
    // rule would give L + 3 for most of these).
    for rolling in [false, true] {
        for held in [1, 10, 14, 20, 40] {
            let (t, _, l, e) = classic_standing_jump(|| classic_second(rolling), held);
            assert_eq!(t, 1, "rolling {rolling}, held {held}");
            assert_eq!(e, Some(l + 2), "rolling {rolling}, held {held}");
        }
    }
}

#[test]
fn classic_second_character_mode_after_a_landing() {
    use Action::{Action as A, Right as R};
    // spec/behavior/ground-speed.md, "After a landing", "The second
    // character": running on L with Action and one direction held on
    // L, otherwise the mode on L + 17 by the input then.
    for rolling in [false, true] {
        for jump in [1, 10, 20] {
            let new = || classic_second(rolling);
            let case = format!("rolling {rolling}, jump {jump}");
            let (_, _, gaits) = classic_after_landing(new, jump, &[A, R], |_| vec![A, R]);
            assert_eq!(mode(gaits[0]), ("running", 256), "{case}");

            // Action held from take-off and Right added on L + 1: no run
            // before L + 17.
            let (_, _, gaits) =
                classic_after_landing(
                    new,
                    jump,
                    &[A],
                    |n| {
                        if n >= 1 { vec![A, R] } else { vec![A] }
                    },
                );
            assert!(
                gaits[..17].iter().all(|&g| mode(g) == ("landing", 0)),
                "{case}"
            );
            assert_eq!(mode(gaits[17]), ("running", 256), "{case}");

            let (_, _, gaits) = classic_after_landing(new, jump, &[], |_| vec![R]);
            assert!(
                gaits[..17].iter().all(|&g| mode(g) == ("landing", 0)),
                "{case}"
            );
            assert_eq!(mode(gaits[17]), ("walking", 256), "{case}");

            let (_, _, gaits) = classic_after_landing(new, jump, &[], |_| vec![]);
            assert!(
                gaits[..17].iter().all(|&g| mode(g) == ("landing", 0)),
                "{case}"
            );
            assert_eq!(mode(gaits[17]), ("standing", 0), "{case}");
        }
    }
}

/// A Classic 1 standing jump from `new`, pressed on frame 0 and held for
/// `jump` frames, then `held(n)` on the n-th frame from the landing frame
/// L and `air` before it. Returns L and the rows and gaits from L on.
fn classic_after_landing(
    new: impl Fn() -> Play,
    jump: usize,
    air: &[Action],
    held: impl Fn(usize) -> Vec<Action>,
) -> (usize, Vec<Row>, Vec<Gait>) {
    let mut probe = new();
    let mut steps = hold(&mut probe, &[Action::Jump], jump);
    steps.extend(hold(&mut probe, air, 80));
    let land = first(&steps, Event::Land).unwrap();

    let mut w = new();
    let mut rows = play(&mut w, land, |f| {
        let mut keys = if f < jump { vec![Action::Jump] } else { vec![] };
        // Action from take-off on, not with the press, so no roll starts.
        keys.extend(air.iter().filter(|&&a| f > 0 || a != Action::Action));
        keys
    });
    let mut gaits = Vec::new();
    for n in 0..30 {
        rows.extend(play(&mut w, 1, |_| held(n)));
        gaits.push(w.player.gait);
    }
    assert!(rows[land].events.contains(&Event::Land));
    (land, rows.split_off(land), gaits)
}

/// The gait as "landing", or the mode and stride counter in 1/256.
fn mode(gait: Gait) -> (&'static str, i64) {
    match gait {
        Gait::Landing(_) => ("landing", 0),
        Gait::Standing => ("standing", 0),
        Gait::Walking(s) => ("walking", grains(s.left)),
        Gait::Running(s) => ("running", grains(s.left)),
    }
}

#[test]
fn classic_after_a_landing_the_mode_waits() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/ground-speed.md, "After a landing", the example: a
    // standing jump facing right with Jump held 20 frames (M = L + 1).
    // Left held from L + 2.
    let (_, rows, gaits) =
        classic_after_landing(classic, 20, &[], |n| if n >= 2 { vec![L] } else { vec![] });
    let vx: Vec<_> = rows[..11].iter().map(|r| (r.vx * 256.0) as i64).collect();
    assert_eq!(
        vx,
        [0, 0, -64, -120, -169, -211, -248, -281, -309, -334, -356]
    );
    assert_eq!(rows[1].facing, Facing::Right);
    assert!(rows[2].events.contains(&Event::Turn));
    assert!(rows[2..11].iter().all(|r| r.facing == Facing::Left));
    assert_eq!(frames_with(&rows[..11], Event::Turn), [2]);
    let modes: Vec<_> = gaits[..11].iter().map(|&g| mode(g)).collect();
    assert!(modes[..7].iter().all(|&m| m == ("landing", 0)), "{modes:?}");
    assert_eq!(
        modes[7..],
        [
            ("walking", 256),
            ("walking", 160),
            ("walking", 64),
            ("walking", 219)
        ]
    );

    // Action held from take-off, Right added from L + 3.
    let (_, rows, gaits) =
        classic_after_landing(
            classic,
            20,
            &[A],
            |n| {
                if n >= 3 { vec![A, R] } else { vec![A] }
            },
        );
    let vx: Vec<_> = rows[..11].iter().map(|r| (r.vx * 256.0) as i64).collect();
    assert_eq!(vx, [0, 0, 0, 96, 180, 253, 317, 373, 422, 465, 502]);
    let modes: Vec<_> = gaits[..11].iter().map(|&g| mode(g)).collect();
    assert!(modes[..7].iter().all(|&m| m == ("landing", 0)), "{modes:?}");
    assert_eq!(
        modes[7..],
        [
            ("running", 256),
            ("running", 160),
            ("running", 59),
            ("running", 207)
        ]
    );

    // With Right and Action both held from take-off, running starts on M.
    let (_, _, gaits) = classic_after_landing(classic, 20, &[A, R], |_| vec![A, R]);
    assert_eq!(mode(gaits[0]), ("landing", 0));
    assert_eq!(mode(gaits[1]), ("running", 256));
}

#[test]
fn classic_after_a_landing_on_its_switch_frame() {
    use Action::{Action as A, Right as R};
    // spec/behavior/ground-speed.md, "After a landing": with M = L, the
    // mode starts on L + 9 after the seventh stage frame (Jump held 10
    // frames) and on L + 10 on it (Jump held 14 frames); running with
    // Action and a direction held starts on L itself.
    for (jump, settle) in [(10, 9), (14, 10)] {
        let (_, _, gaits) = classic_after_landing(classic, jump, &[], |_| vec![R]);
        let first = gaits.iter().position(|&g| mode(g).0 != "landing").unwrap();
        assert_eq!(mode(gaits[0]), ("landing", 0), "jump {jump}");
        assert_eq!(first, settle, "jump {jump}");
        assert_eq!(mode(gaits[first]), ("walking", 256), "jump {jump}");

        let (_, _, gaits) = classic_after_landing(classic, jump, &[A, R], |_| vec![A, R]);
        assert_eq!(mode(gaits[0]), ("running", 256), "jump {jump}");
    }
}

#[test]
fn classic_no_turn_starts_after_a_landing() {
    use Action::{Left as L, Right as R};
    // spec/behavior/ground-speed.md, "After a landing": facing follows the
    // input on every landing frame, K = L + 7 included, so the direction
    // held there is forward.
    let (_, rows, gaits) = classic_after_landing(classic, 20, &[], |n| match n {
        2 => vec![L],
        7.. => vec![R],
        _ => vec![],
    });
    assert_eq!(frames_with(&rows[..12], Event::Turn), [2, 7]);
    assert_eq!(rows[7].facing, Facing::Right);
    assert_eq!(mode(gaits[7]), ("walking", 256));

    // Standing from K, the usual rules apply: the other way held on
    // L + 8 starts an 8-frame turn, and facing changes 4 frames in.
    let (_, rows, gaits) = classic_after_landing(classic, 20, &[], |n| match n {
        2 => vec![L],
        8.. => vec![R],
        _ => vec![],
    });
    assert_eq!(mode(gaits[7]), ("standing", 0));
    assert_eq!(frames_with(&rows[..20], Event::Turn), [2, 12]);
}

/// Plays `n` frames of `w`, holding `held(frame)` on each, and returns
/// the rows and each frame's gait.
fn play_gaits(
    w: &mut Play,
    n: usize,
    held: impl Fn(usize) -> Vec<Action>,
) -> (Vec<Row>, Vec<Gait>) {
    let mut rows = Vec::new();
    let mut gaits = Vec::new();
    for f in 0..n {
        rows.extend(play(w, 1, |_| held(f)));
        gaits.push(w.player.gait);
    }
    (rows, gaits)
}

#[test]
fn classic_after_a_roll_carries_on_it_walks() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/ground-speed.md, "After a roll": a roll pressed on 0
    // (D = 29) with Action held through D carries on, and walks from
    // D + 3, the `roll_end` frame, whatever is held. With the opposite
    // direction held, a turn starts at the first turn check, the end of
    // step 1. (Steering in the carry-on isn't in the spec, so nothing is
    // held on D + 3, and Left from D + 4.)
    let mut w = classic();
    let (rows, gaits) = play_gaits(&mut w, 45, |f| match f {
        ..30 => vec![A, R],
        30..32 => vec![R],
        32 => vec![],
        _ => vec![L],
    });
    assert_eq!(frames_with(&rows, Event::RollEnd), [32]);
    assert_eq!(mode(gaits[31]), ("standing", 0));
    assert_eq!(mode(gaits[32]), ("walking", 256));
    let rate = gaits[33..]
        .iter()
        .position(|&g| !matches!(g, Gait::Walking(s) if s.step == 1))
        .unwrap();
    assert_eq!(frames_with(&rows[..34], Event::Turn), NONE);
    let turn = frames_with(&rows, Event::Turn)[0] - 4;
    assert_eq!(turn, 33 + rate);
    assert_eq!(mode(gaits[turn]), ("standing", 0));

    // With Action and a direction held, running starts at the end of
    // step 2.
    let mut w = classic();
    let (rows, gaits) = play_gaits(&mut w, 45, |_| vec![A, R]);
    assert_eq!(frames_with(&rows, Event::RollStart), [0]);
    assert_eq!(mode(gaits[32]), ("walking", 256));
    let run = gaits
        .iter()
        .position(|&g| matches!(g, Gait::Running(_)))
        .unwrap();
    assert!(matches!(gaits[run - 1], Gait::Walking(s) if s.step == 2));
}

#[test]
fn classic_after_a_roll_stops_it_stands() {
    use Action::{Action as A, Right as R};
    // spec/behavior/ground-speed.md, "After a roll": a roll pressed on 0
    // that stops (Action released, D = 29) stands on D + 17 whatever is
    // held, and with Right held walks from D + 18.
    let mut w = classic();
    let (rows, gaits) = play_gaits(&mut w, 50, |f| match f {
        0 => vec![A],
        1..29 => vec![],
        _ => vec![R],
    });
    assert_eq!(frames_with(&rows, Event::RollEnd), [29]);
    assert_eq!(mode(gaits[29 + 17]), ("standing", 0));
    assert_eq!(rows[29 + 17].vx, 0.0);
    assert_eq!(mode(gaits[29 + 18]), ("walking", 256));
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
fn immediate_classic_2_jump_takes_off_on_the_press_frame() {
    // #437: the two frames between a Classic 2 jump's start and its take-off
    // are a delay after the press, so with the feel immediate it takes off
    // on P (a tap included); without it, on P + 2.
    for (immediate, take_off) in [(true, 0), (false, 2)] {
        let mut feel = generic_feel("classic-2").unwrap();
        feel.immediate = immediate;
        let mut w = Play::new(feel);
        let mut steps = hold(&mut w, &[Action::Jump], 1);
        steps.extend(hold(&mut w, &[], 3));
        assert_eq!(
            first(&steps, Event::JumpStart),
            Some(take_off),
            "{immediate}"
        );
        let starts = all_events(&steps)
            .into_iter()
            .filter(|&e| e == Event::JumpStart);
        assert_eq!(starts.count(), 1, "{immediate}");
    }
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
fn classic_1_turns_during_a_crouch() {
    fn standing_classic_1() -> Play {
        let mut w = Play::new(generic_feel("classic-1").unwrap());
        hold(&mut w, &[], 60);
        assert!(w.player.grounded());
        w
    }

    // spec/behavior/ground-speed.md, "Turns during a crouch": standing
    // still facing right, Down held on frames 0 to 4 and Left from frame
    // 0 on. The crouch lasts to frame 12, so U is 13; facing stays right
    // to frame 26, the turn starts on 23 and facing is left on 27.
    use Action::{Down, Left};
    let mut w = standing_classic_1();
    for frame in 0..30 {
        let held: &[Action] = if frame < 5 { &[Down, Left] } else { &[Left] };
        hold(&mut w, held, 1);
        let left = w.player.facing == Facing::Left;
        assert_eq!(left, frame >= 27, "frame {frame}");
        assert!(!(w.player.turn.is_some() && frame < 23), "frame {frame}");
    }

    // Left held on frames 8 to 10 only, or 18 to 20 only: no turn at all.
    for range in [8..11, 18..21] {
        let mut w = standing_classic_1();
        for frame in 0..40 {
            let mut held = vec![];
            if frame < 5 {
                held.push(Down);
            }
            if range.contains(&frame) {
                held.push(Left);
            }
            hold(&mut w, &held, 1);
            assert_eq!(w.player.facing, Facing::Right, "{range:?} frame {frame}");
        }
    }

    // Left held only from frame 24 on: the turn starts on 24, facing is
    // left on 28.
    let mut w = standing_classic_1();
    for frame in 0..30 {
        let mut held = vec![];
        if frame < 5 {
            held.push(Down);
        }
        if frame >= 24 {
            held.push(Left);
        }
        hold(&mut w, &held, 1);
        assert_eq!(
            w.player.facing == Facing::Left,
            frame >= 28,
            "frame {frame}"
        );
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
        assert_eq!(w.player.turn, None, "U + {frame}");
    }
    hold(&mut w, &[Left], 1);
    assert_eq!(w.player.turn, Some(0), "U + 11");

    // Down without Up during U to U + 10 starts a new crouch, with its
    // own U after it.
    let mut w = classic_2();
    hold(&mut w, &[Down], 3);
    hold(&mut w, &[], 5);
    hold(&mut w, &[Down], 1);
    assert!(w.player.crouch.held > 0);
    hold(&mut w, &[Left], 11);
    assert_eq!(w.player.turn, None);
    hold(&mut w, &[Left], 1);
    assert_eq!(w.player.turn, Some(0));
}

/// How a Classic 2 test starts: a name, the start, and what to hold to
/// keep going as before.
type Start<'a> = (&'a str, fn() -> Play, &'a [Action]);

#[test]
fn classic_2_turns_on_the_ground() {
    // spec/behavior/ground-speed.md, "Turns (`title-2`)": the other way
    // held on a grounded frame starts an 8-frame turn on that frame,
    // standing, walking or running, and facing changes on T + 4. Once
    // started, the turn completes.
    use Action::{Action as Act, Left, Right};
    let starts: [Start; 3] = [
        ("standing", classic_2_clear, &[]),
        ("walking", || classic_2_after(&[Right], 2.25), &[Right]),
        (
            "running",
            || classic_2_after(&[Right, Act], 3.375),
            &[Right, Act],
        ),
    ];
    for (from, start, after) in starts {
        // Left for one frame, then nothing or the old way. Action stays
        // held, as pressing it again would roll.
        let running = after.contains(&Act);
        for then in [&[][..], after] {
            let mut w = start();
            let rows = play(&mut w, 12, |f| {
                if f > 0 {
                    then.to_vec()
                } else if running {
                    vec![Left, Act]
                } else {
                    vec![Left]
                }
            });
            assert_eq!(frames_with(&rows, Event::Turn), [4], "{from} {then:?}");
            assert_eq!(rows[3].facing, Facing::Right, "{from}");
            assert_eq!(rows[4].facing, Facing::Left, "{from}");
        }
    }

    // A turn doesn't change the speed rule: reversing from +2.25 gives
    // the values of "Worked values (`title-2`)".
    let mut w = classic_2_after(&[Right], 2.25);
    let rows = play(&mut w, 5, |_| vec![Left]);
    assert_eq!(vx_grains(&rows), [432, 306, 196, 100, 16]);
}

#[test]
fn classic_2_chained_turns() {
    // spec/behavior/ground-speed.md, "Turns (`title-2`)": if the way
    // opposite to the new facing is held on T + 8, a new turn starts
    // there. Walking right, Left for 5 frames and then Right turns left
    // on 4 and right again on 12.
    use Action::{Left, Right};
    let mut w = classic_2_after(&[Right], 2.25);
    let rows = play(&mut w, 20, |f| if f < 5 { vec![Left] } else { vec![Right] });
    assert_eq!(frames_with(&rows, Event::Turn), [4, 12]);
    // Holding the other way all along turns every 8 frames.
    let mut w = classic_2_clear();
    let rows = play(&mut w, 24, |f| {
        if f / 8 % 2 == 0 {
            vec![Left]
        } else {
            vec![Right]
        }
    });
    assert_eq!(frames_with(&rows, Event::Turn), [4, 12, 20]);
}

/// Classic 2 running at `vx` the way `ahead` points, turning back for
/// one frame with `back`, then running on with `ahead` until frame 15
/// and holding `back` from then. Returns the `turn` frames and `vx` in
/// 1/256 on frame 9 (T + 1 of the second turn).
fn classic_2_turns_twice(ahead: Action, back: Action, back_frames: usize) -> (Vec<usize>, i64) {
    use Action::Action as Act;
    let vx = if ahead == Action::Right {
        3.375
    } else {
        -3.375
    };
    let mut w = classic_2_after(&[ahead, Act], vx);
    let rows = play(&mut w, 24, |f| {
        if f < back_frames || f >= 15 {
            vec![back, Act]
        } else {
            vec![ahead, Act]
        }
    });
    (frames_with(&rows, Event::Turn), vx_grains(&rows)[9])
}

#[test]
fn classic_2_short_turns() {
    // spec/behavior/ground-speed.md, "Turns (`title-2`)", "Short turn": a
    // turn that starts on T + 8 of another while running toward the new
    // facing at 797/256 or faster on T + 1 lasts 7 frames. Facing still
    // changes on T + 4, and a new turn can start on T + 7. Here the
    // second turn starts on 8 and the other way is held from 15.
    use Action::{Left, Right};
    let (turns, vx) = classic_2_turns_twice(Right, Left, 1);
    assert_eq!(vx, 797);
    assert_eq!(turns, [4, 12, 19]);
    let (turns, vx) = classic_2_turns_twice(Left, Right, 1);
    assert_eq!(vx, -797);
    assert_eq!(turns, [4, 12, 19]);
    // Slower on T + 1, the turn lasts 8 frames and the next one starts
    // on 16.
    let (turns, vx) = classic_2_turns_twice(Right, Left, 2);
    assert!(vx <= 780, "{vx}");
    assert_eq!(turns, [4, 12, 20]);
}

#[test]
fn classic_2_crouch_during_a_turn() {
    // spec/behavior/ground-speed.md, "Turns (`title-2`)": a crouch that
    // starts during a turn ends it at once. Started on T + 1 to T + 4,
    // facing doesn't change; from T + 5 on it has changed and stays so.
    use Action::{Down, Left};
    for k in 1..=7 {
        let mut w = classic_2_clear();
        let rows = play(&mut w, 30, |f| match f {
            0 => vec![Left],
            f if (k..k + 3).contains(&f) => vec![Down],
            _ => vec![],
        });
        assert!(!rows[k].events.contains(&Event::Turn), "T + {k}");
        let facing = if k <= 4 { Facing::Right } else { Facing::Left };
        assert_eq!(rows[29].facing, facing, "crouch on T + {k}");
        assert_eq!(w.player.turn, None);
        let turns = frames_with(&rows, Event::Turn);
        assert_eq!(turns, if k <= 4 { vec![] } else { vec![4] }, "T + {k}");
    }
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

/// Classic 2 walking right from x = 1500 off the ledge at x = 1600 of
/// `level`, holding `held(n)` on frame L + n, where L is the frame it
/// leaves the ground on with Right alone held; `held` should hold Right
/// before L, which it reaches walking at +2.25. Rows from frame L to
/// L + `after`.
fn classic_2_off(level: &str, after: usize, held: impl Fn(i64) -> Vec<Action>) -> Vec<Row> {
    let new = || {
        let level = Arc::new(Level::from_toml_str(level, "ledge").unwrap());
        let mut w = Play::on(level, generic_feel("classic-2").unwrap());
        w.player.pos.x = Fixed::from_int(1500);
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
    assert_eq!(rows[l - 1].vx, 2.25, "walking at +2.25 before L");
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
        // Right held keeps it at +2.25 throughout.
        assert!(rows.iter().all(|r| r.vx == 2.25), "pressed on L + {press}");
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
    // `vx` falls under the speed rule: 1.96875 on L, 0.41015625 on U.
    assert_eq!((rows[0].vx, rows[12].vx), (1.96875, 0.41015625));
    assert_eq!(rows[13].target, 2.25);
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

fn classic_2_clear() -> Play {
    let mut w = classic_2();
    w.player.pos.x = Fixed::from_int(500);
    w
}

/// `vx` in 1/256 px per frame on each row.
fn vx_grains(rows: &[Row]) -> Vec<i64> {
    rows.iter().map(|r| (r.vx * 256.0) as i64).collect()
}

/// The first row from which `vx` stays at `target` to the end.
fn reached(rows: &[Row], target: f64) -> usize {
    rows.iter()
        .rposition(|r| r.vx != target)
        .map_or(0, |i| i + 1)
}

#[test]
fn classic_2_ground_speed_worked_values() {
    // spec/behavior/ground-speed.md, "Worked values (`title-2`)": from
    // rest, Right held from frame 0.
    let mut w = classic_2_clear();
    let start = w.player.pos.x.to_f64();
    let rows = play(&mut w, 45, |_| vec![Action::Right]);
    for (frame, vx, x) in [
        (0, 72, 0.28125),
        (1, 135, 0.80859375),
        (2, 190, 1.55078125),
        (3, 238, 2.48046875),
        (10, 441, 12.5859375),
        (20, 538, 32.28515625),
        (30, 563, 53.94921875),
        (36, 569, 67.2265625),
        (37, 576, 69.4765625),
    ] {
        let row = &rows[frame];
        assert_eq!(
            (row.vx * 256.0, row.x - start),
            (vx as f64, x),
            "frame {frame}"
        );
    }
    assert_eq!(reached(&rows, 2.25), 37);
}

/// Classic 2 after holding `keys` for 100 frames from rest at x = 500,
/// with `vx` as given. Action held from rest starts a roll that carries
/// on.
fn classic_2_after(keys: &[Action], vx: f64) -> Play {
    let mut w = classic_2_clear();
    hold(&mut w, keys, 100);
    assert_eq!(w.player.vel.x.to_f64(), vx);
    w
}

/// A row of the `title-2` speed summary: how the player starts, what is
/// held from frame 0, the first three `vx`, the target, the frame it's
/// reached on and the distance until then.
type SummaryRow<'a> = (fn() -> Play, &'a [Action], [f64; 3], f64, usize, f64);

#[test]
fn classic_2_ground_speed_summary() {
    // spec/behavior/ground-speed.md, "Worked values (`title-2`)", the
    // summary: the first three `vx`, the frame the target is reached on
    // and the distance until then, that frame included.
    use Action::{Action as Act, Left, Right};
    let rest = classic_2_clear;
    let walking = || classic_2_after(&[Right], 2.25);
    let running = || classic_2_after(&[Right, Act], 3.375);
    let action_only = || classic_2_after(&[Act], 0.0);
    let cases: [SummaryRow; 6] = [
        (
            rest,
            &[Right],
            [0.28125, 0.52734375, 0.7421875],
            2.25,
            37,
            69.4765625,
        ),
        (
            action_only,
            &[Right, Act],
            [0.28125, 0.66796875, 1.00390625],
            3.375,
            40,
            113.37890625,
        ),
        (
            walking,
            &[],
            [1.96875, 1.72265625, 1.5078125],
            0.0,
            37,
            16.0234375,
        ),
        (
            running,
            &[],
            [2.953125, 2.5859375, 2.265625],
            0.0,
            40,
            23.984375,
        ),
        (
            running,
            &[Act],
            [2.953125, 2.5859375, 2.265625],
            0.0,
            40,
            23.984375,
        ),
        (
            running,
            &[Right],
            [3.375, 3.234375, 3.11328125],
            2.25,
            33,
            85.7421875,
        ),
    ];
    for (n, (before, keys, first, target, on, distance)) in cases.into_iter().enumerate() {
        let mut w = before();
        let start = w.player.pos.x.to_f64();
        let rows = play(&mut w, 60, |_| keys.to_vec());
        let vx: Vec<f64> = rows[..3].iter().map(|r| r.vx).collect();
        assert_eq!(vx, first, "case {n}");
        assert_eq!(reached(&rows, target), on, "case {n}");
        assert_eq!(rows[on].x - start, distance, "case {n}");
    }

    // Reversing from +2.25 with Left held.
    let mut w = walking();
    let start = w.player.pos.x.to_f64();
    let rows = play(&mut w, 50, |_| vec![Left]);
    let vx: Vec<f64> = rows[..6].iter().map(|r| r.vx).collect();
    assert_eq!(
        vx,
        [1.6875, 1.1953125, 0.765625, 0.390625, 0.0625, -0.2265625]
    );
    assert_eq!(rows[4].x - start, 4.1015625);
    assert_eq!(reached(&rows, -2.25), 42);
    // From +3.375 with Left and Action held.
    let mut w = running();
    let rows = play(&mut w, 50, |_| vec![Left, Act]);
    let vx: Vec<f64> = rows[..6].iter().map(|r| r.vx).collect();
    assert_eq!(
        vx,
        [
            2.53125,
            1.79296875,
            1.1484375,
            0.5859375,
            0.09375,
            -0.33984375
        ]
    );
    assert_eq!(reached(&rows, -3.375), 45);
}

/// A Classic 2 jump with Jump held from the press G (row 0) to landing:
/// `on_g[k]` held with it on G + k, so T is row 2, and `air` from T + 1.
/// All rows up to and including the landing.
fn classic_2_air_jump(mut w: Play, on_g: [&[Action]; 3], air: &[Action]) -> Vec<Row> {
    let mut rows = Vec::new();
    while rows.len() < 200 {
        let keys = on_g.get(rows.len()).copied().unwrap_or(air);
        rows.extend(play(&mut w, 1, |_| [keys, &[Action::Jump]].concat()));
        if rows.last().unwrap().events.contains(&Event::Land) {
            assert_eq!(frames_with(&rows, Event::JumpStart), [2]);
            return rows;
        }
    }
    panic!("never landed");
}

#[test]
fn classic_2_air_control_worked_values() {
    // spec/behavior/air-control.md, "Worked values (`title-2`)": Right
    // held from rest for 45 frames, Jump and Right on G to T, then Jump
    // and Left.
    use Action::{Jump, Left, Right};
    let mut w = classic_2_clear();
    hold(&mut w, &[Right], 45);
    let mut rows = play(&mut w, 3, |_| vec![Right, Jump]);
    rows.extend(play(&mut w, 6, |_| vec![Left, Jump]));
    assert_eq!(frames_with(&rows, Event::JumpStart), [2]);
    assert!(
        rows[..3]
            .iter()
            .all(|r| r.vx == 2.25 && r.facing == Facing::Right)
    );
    assert_eq!(vx_grains(&rows[3..]), [432, 306, 196, 100, 16, -58]);
    for k in 3..rows.len() {
        assert!(!rows[k].grounded, "T + {}", k - 2);
        assert_eq!(rows[k].facing, Facing::Left, "T + {}", k - 2);
        assert_eq!(rows[k].x - rows[k - 1].x, rows[k].vx, "T + {}", k - 2);
    }
    assert_eq!(frames_with(&rows, Event::Turn), [3]);
}

/// A row of the `title-2` jump distance table: how the player starts,
/// what is held on G, G + 1 and T and from T + 1, the distance and `vx`
/// on landing.
type Distance2Row<'a> = (fn() -> Play, [&'a [Action]; 3], &'a [Action], f64, f64);

#[test]
fn classic_2_jump_distances() {
    // spec/behavior/air-control.md, "Jump distances (`title-2`)": `x` on
    // the landing frame minus `x` on T, and `vx` on landing, on T + 44.
    use Action::{Action as Act, Left, Right};
    let rest = classic_2_clear as fn() -> Play;
    let walking = (|| classic_2_after(&[Right], 2.25)) as fn() -> Play;
    // Running at 801/256 with Right and Action held on G - 1.
    let running = (|| {
        let mut w = classic_2_after(&[Right, Act], 3.375);
        w.player.vel.x = Fixed::from_ratio_pow2(792, 8);
        hold(&mut w, &[Right, Act], 1);
        assert_eq!(w.player.vel.x, Fixed::from_ratio_pow2(801, 8));
        w
    }) as fn() -> Play;
    let (r, ra): (&[Action], &[Action]) = (&[Right], &[Right, Act]);
    let none: &[Action] = &[];
    let table: [Distance2Row; 12] = [
        (rest, [none; 3], none, 0.0, 0.0),
        (rest, [none, r, r], r, 86.66796875, 2.25),
        (rest, [none, none, r], r, 84.9453125, 2.25),
        (rest, [none; 3], ra, 123.50390625, 3.375),
        (rest, [none, none, ra], ra, 126.59765625, 3.375),
        (walking, [r; 3], r, 99.0, 2.25),
        (walking, [r; 3], none, 16.0234375, 0.0),
        (walking, [r; 3], &[Left], -67.1171875, -2.25),
        (walking, [r; 3], ra, 139.2578125, 3.375),
        (running, [ra; 3], ra, 147.29296875, 3.375),
        (running, [ra; 3], r, 107.02734375, 2.25),
        (running, [ra; 3], none, 22.82421875, 0.0),
    ];
    for (n, (before, on_g, air, distance, vx)) in table.into_iter().enumerate() {
        let rows = classic_2_air_jump(before(), on_g, air);
        assert_eq!(rows.len(), 3 + 44, "row {n}");
        let landing = rows.last().unwrap();
        assert_eq!(
            (landing.x - rows[2].x, landing.vx),
            (distance, vx),
            "row {n}"
        );
    }

    // The mirror image: walking left with Left held.
    let l: &[Action] = &[Left];
    let mut w = classic_2();
    w.player.pos.x = Fixed::from_int(1000);
    hold(&mut w, l, 60);
    let rows = classic_2_air_jump(w, [l; 3], l);
    let landing = rows.last().unwrap();
    assert_eq!((landing.x - rows[2].x, landing.vx), (-99.0, -2.25));

    // Short jumps from walking with Right held: Jump on G only lands on
    // T + 37 after 83.25 px; Jump held from G to T + 2 on T + 38 after
    // 85.5 px.
    for (jump, land, distance) in [(1, 37, 83.25), (5, 38, 85.5)] {
        let mut w = walking();
        let rows = play(&mut w, 45, |f| {
            if f < jump {
                vec![Right, Action::Jump]
            } else {
                vec![Right]
            }
        });
        assert_eq!(frames_with(&rows, Event::JumpStart), [2], "{jump} frames");
        assert_eq!(frames_with(&rows, Event::Land), [2 + land], "{jump} frames");
        assert_eq!(rows[2 + land].x - rows[2].x, distance, "{jump} frames");
    }
}

#[test]
fn classic_2_facing_in_a_jump() {
    // spec/behavior/air-control.md, "Before take-off (`title-2`)" and
    // "Facing (`title-2`)".
    use Action::{Left, Right};
    use Facing::{Left as FL, Right as FR};
    let (r, l): (&[Action], &[Action]) = (&[Right], &[Left]);
    let walking = || classic_2_after(r, 2.25);
    let facing = |rows: &[Row]| -> Vec<Facing> { rows.iter().map(|r| r.facing).collect() };

    // No turn on G, G + 1 or T. Left still held on T + 1 turns there.
    let rows = classic_2_air_jump(walking(), [l; 3], l);
    assert_eq!(facing(&rows[..4]), [FR, FR, FR, FL]);
    assert_eq!(frames_with(&rows, Event::Turn), [3]);
    // Released by T + 1, facing stays.
    let rows = classic_2_air_jump(walking(), [l; 3], &[]);
    assert!(rows.iter().all(|r| r.facing == FR));
    // Speed still follows the ground rules on G to T.
    assert_eq!(vx_grains(&rows[..3]), [432, 306, 196]);

    // In the air facing follows the input on the same frame; both, or
    // neither, change nothing.
    let air: [&[Action]; 7] = [&[], l, l, &[], &[Left, Right], r, l];
    let mut w = walking();
    let mut rows = play(&mut w, 3, |_| vec![Right, Action::Jump]);
    rows.extend(play(&mut w, air.len(), |f| {
        [air[f], &[Action::Jump]].concat()
    }));
    assert!(rows[3..].iter().all(|r| !r.grounded));
    assert_eq!(facing(&rows[3..]), [FR, FL, FL, FL, FL, FR, FL]);
    // The speed is unaffected: still moving right while facing left.
    assert!(rows[4].vx > 0.0);

    // The landing frame follows its own input too.
    let mut w = walking();
    let mut rows = play(&mut w, 3, |_| vec![Right, Action::Jump]);
    while !rows.last().unwrap().grounded || rows.len() < 4 {
        let next = if rows.len() < 3 + 43 { r } else { l };
        rows.extend(play(&mut w, 1, |_| [next, &[Action::Jump]].concat()));
    }
    let landing = rows.last().unwrap();
    assert!(landing.events.contains(&Event::Land));
    assert_eq!((rows[rows.len() - 2].facing, landing.facing), (FR, FL));
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
fn classic_the_mode_goes_on_off_a_ledge_without_a_face() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/ground-speed.md, "After a fall or a bounce": walking
    // or running off a ledge without a face changes nothing at the edge,
    // in the air or on landing. The stride cycle goes on, turns start in
    // the air and facing changes 4 frames in, a turn under way at the
    // edge goes on, running starts and the player comes to stand in the
    // air, and there are no landing frames. So each frame's facing, turn
    // and mode are as on ground that goes on.
    type Inputs = Box<dyn Fn(usize) -> Vec<Action>>;
    // As `RAISED_176`, but the raised ground is a one-way ledge, so that
    // coming back under it meets no wall.
    let toml = RAISED_176.replace(
        "shape = \"rect\"\n    rect = [800, 400, 800, 176]",
        "shape = \"segment\"\n    from = [800, 400]\n    to = [1600, 400]\n    one_way = true",
    );
    assert_ne!(toml, RAISED_176);
    let level = Arc::new(Level::from_toml_str(&toml, "ledge").unwrap());
    let ledge = || {
        let mut w = Play::on(level.clone(), generic_feel("classic-1").unwrap());
        w.player.pos.x = Fixed::from_int(1300);
        w
    };
    let leave = |run: bool| {
        let held = move |_| if run { vec![A, R] } else { vec![R] };
        frames_with(&play(&mut ledge(), 300, held), Event::LeaveGround)[0]
    };
    let (walk, run) = (leave(false), leave(true));
    let then = |l: usize, before: Vec<Action>, from: isize, after: Vec<Action>| -> Inputs {
        Box::new(move |f| {
            if f as isize >= l as isize + from {
                after.clone()
            } else {
                before.clone()
            }
        })
    };
    // Segments of 1 to 20 frames of Right, Left or nothing from L + 1,
    // with Action held throughout or never: a press of it on the ground
    // would start a roll.
    let random = |l: usize, run: bool, seed: u64| -> Inputs {
        Box::new(move |f| {
            if f <= l {
                return if run { vec![A, R] } else { vec![R] };
            }
            let mut n = seed;
            let mut end = l + 1;
            loop {
                n = n
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                end += 1 + (n >> 33) as usize % 20;
                if f < end {
                    break;
                }
            }
            let mut keys = match (n >> 40) % 3 {
                0 => vec![R],
                1 => vec![L],
                _ => vec![],
            };
            if run {
                keys.push(A);
            }
            keys
        })
    };
    // Left from a frame that has a turn under way on the leave frame.
    let at_edge = (1..20)
        .find(|&before| {
            let mut w = ledge();
            let held = then(run, vec![A, R], -before, vec![A, L]);
            (0..run + 20).any(|f| {
                let row = play(&mut w, 1, |_| held(f)).pop().unwrap();
                row.events.contains(&Event::LeaveGround) && w.player.turn.is_some()
            })
        })
        .expect("a turn under way at the edge");
    let mut cases: Vec<(String, usize, Inputs)> = vec![
        (
            "run, turn at the edge".into(),
            run,
            then(run, vec![A, R], -at_edge, vec![A, L]),
        ),
        (
            "walk, turn in the air".into(),
            walk,
            then(walk, vec![R], 2, vec![L]),
        ),
        (
            "walk, stand in the air".into(),
            walk,
            then(walk, vec![R], 1, vec![]),
        ),
        (
            "run, turn in the air".into(),
            run,
            then(run, vec![A, R], 5, vec![A, L]),
        ),
    ];
    for seed in 0..12 {
        let run_off = seed % 2 == 1;
        let l = if run_off { run } else { walk };
        cases.push((format!("random {seed}"), l, random(l, run_off, seed)));
    }
    let mut air_turns = 0;
    for (name, l, held) in &cases {
        let mut off = ledge();
        let mut flat = classic_unended(1300);
        let (mut left, mut landed) = (false, false);
        for f in 0..l + 80 {
            let a = play(&mut off, 1, |_| held(f)).pop().unwrap();
            let b = play(&mut flat, 1, |_| held(f)).pop().unwrap();
            let what = format!("{name}: frame {f}");
            left |= a.events.contains(&Event::LeaveGround);
            landed |= a.events.contains(&Event::Land);
            assert_eq!(a.grounded, !left || landed, "{what}");
            assert_eq!((a.x, a.vx, a.facing), (b.x, b.vx, b.facing), "{what}");
            assert_eq!(off.player.turn, flat.player.turn, "{what}");
            assert_eq!(off.player.gait, flat.player.gait, "{what}");
            assert!(!matches!(off.player.gait, Gait::Landing(_)), "{what}");
            let turned = a.events.contains(&Event::Turn);
            assert_eq!(turned, b.events.contains(&Event::Turn), "{what}");
            if turned && !a.grounded {
                air_turns += 1;
            }
        }
        assert!(left && landed, "{name}");
    }
    assert!(air_turns >= 5, "{air_turns}");

    // Action pressed in the air: running starts at a run check there.
    let mut w = ledge();
    let held = then(walk, vec![R], 1, vec![A, R]);
    let mut gaits = Vec::new();
    for f in 0..walk + 80 {
        let row = play(&mut w, 1, |_| held(f)).pop().unwrap();
        assert!(!row.events.contains(&Event::RollStart), "frame {f}");
        gaits.push((row.grounded, w.player.gait));
    }
    let start = gaits
        .iter()
        .position(|&(_, g)| matches!(g, Gait::Running(_)))
        .unwrap();
    assert!(start > walk && !gaits[start].0, "{start}");
    assert!(matches!(gaits[start - 1].1, Gait::Walking(_)));
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

    // A new press on 56 starts a roll there, also standing still on it
    // and also lasting 30 frames.
    let rows = classic_roll_presses(&[10, 56], usize::MAX, 90);
    assert_eq!(frames_with(&rows, Event::RollStart), [10, 56]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [39, 86]);
    assert_eq!(rows[56].vx, 0.0);
}

#[test]
fn classic_roll_stop_turns_on_the_press_frame() {
    use Action::{Action as A, Left as L};
    // spec/behavior/roll-input.md, "How a roll ends": in the stop after a
    // roll (D = 39, the roll's last frame, here), a direction turns facing on the frame it is
    // pressed from D + 1 to D + 17, also while the player still slides,
    // and the slide goes on; from D + 18 it's the standing turn.
    let rolled = |press: usize, key: Action| {
        let mut w = classic();
        play(&mut w, 70, |f| match f {
            10 => vec![A],
            _ if f >= press => vec![key],
            _ => vec![],
        })
    };
    for press in 40..=56 {
        let rows = rolled(press, L);
        assert_eq!(frames_with(&rows, Event::Turn), [press], "{press}");
        assert_eq!(rows[press].facing, Facing::Left, "{press}");
    }
    let slid = rolled(100, L);
    let rows = rolled(40, L);
    assert_eq!(rows[40].vx, slid[40].vx);
    assert_eq!(rows[41].vx, slid[41].vx);
    // The direction already faced turns nothing.
    assert_eq!(frames_with(&rolled(46, Action::Right), Event::Turn), NONE);
    // From D + 18 the standing turn changes facing 4 frames later.
    let rows = rolled(57, L);
    assert_eq!(frames_with(&rows, Event::Turn), [61]);
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

/// A raised stretch of ground at y = 400 over x = 800..1600, 176 px above
/// flat floor at y = 576, with nothing at its ends.
const RAISED_176: &str = r#"
    format = 1
    name = "raised"
    size = [4096, 600]
    start = [1200, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 576, 4096, 24]

    [[collision]]
    shape = "rect"
    rect = [800, 400, 800, 176]
"#;

/// The first column past the right end of `RAISED_176`'s raised ground.
const RAISED_END: f64 = 1600.0;

/// Classic 1 on `RAISED_176`, standing at `x`.
fn classic_raised(x: i32) -> Play {
    let level = Arc::new(Level::from_toml_str(RAISED_176, "raised").unwrap());
    let mut w = Play::on(level, generic_feel("classic-1").unwrap());
    w.player.pos.x = Fixed::from_int(x);
    w
}

/// Classic 1 on `RAISED_176` walking right at +2 from x = 1300, then
/// Action and Right on frame S and Right alone after it, with S chosen
/// so that the roll leaves the ground on S + `lead`. The rows from S on,
/// `n` of them, with `adjust(frame, world)` called before each frame.
fn classic_roll_off_raised(lead: usize, n: usize, adjust: impl Fn(usize, &mut Play)) -> Vec<Row> {
    classic_roll_off_raised_jumping(lead, n, 0..0, adjust)
}

/// [`classic_roll_off_raised`], with Jump held too on the frames in
/// `jump`, counted from S.
fn classic_roll_off_raised_jumping(
    lead: usize,
    n: usize,
    jump: std::ops::Range<usize>,
    adjust: impl Fn(usize, &mut Play),
) -> Vec<Row> {
    classic_roll_off_raised_pressing(lead, n, |f| jump.contains(&f), adjust)
}

/// [`classic_roll_off_raised`], with Jump held too on the frames, counted
/// from S, for which `jump` is true.
fn classic_roll_off_raised_pressing(
    lead: usize,
    n: usize,
    jump: impl Fn(usize) -> bool,
    adjust: impl Fn(usize, &mut Play),
) -> Vec<Row> {
    let held = |f: usize, s: usize| {
        let mut keys = match f.cmp(&s) {
            std::cmp::Ordering::Equal => vec![Action::Action, Action::Right],
            _ => vec![Action::Right],
        };
        if f >= s && jump(f - s) {
            keys.push(Action::Jump);
        }
        keys
    };
    for s in 1..200 {
        let rows = play(&mut classic_raised(1300), s + lead + 1, |f| held(f, s));
        if frames_with(&rows, Event::LeaveGround).first() == Some(&(s + lead)) {
            assert_eq!(rows[s - 1].vx, 2.0, "walking at +2 before S");
            assert_eq!(frames_with(&rows, Event::RollStart), [s]);
            let mut w = classic_raised(1300);
            play(&mut w, s, |f| held(f, s));
            return (0..n)
                .map(|f| {
                    adjust(f, &mut w);
                    play(&mut w, 1, |_| held(s + f, s)).pop().unwrap()
                })
                .collect();
        }
    }
    panic!("no roll leaves the ground on S + {lead}");
}

#[test]
fn classic_roll_hovers_by_when_it_leaves_the_ground() {
    // spec/behavior/rolling-off-ledges.md, "The fall": which frames after
    // L hover depends only on L - S. A hover frame moves down 1/256 and
    // ends with `vy` 1/256; the fall goes on from there.
    let drop = 3.4375;
    let hover = 0.00390625;
    for lead in 0..=12 {
        let hovers = |after: usize| match lead {
            0 => (1..=8).contains(&after),
            1..=7 => after <= 9,
            8 => after == 0,
            _ => false,
        };
        let rows = classic_roll_off_raised(lead, lead + 16, |_, _| {});
        let mut vy = hover;
        for after in 0..16 {
            let moved = if hovers(after) {
                vy = hover;
                hover
            } else if after == 0 {
                vy = hover;
                drop
            } else {
                vy += 0.4375;
                vy
            };
            let f = lead + after;
            assert_eq!(rows[f].state, State::Roll, "lead {lead}, L + {after}");
            let before = if f == 0 { 400.0 } else { rows[f - 1].y };
            assert_eq!(
                (rows[f].y - before, rows[f].vy),
                (moved, vy),
                "lead {lead}, L + {after}"
            );
        }
    }
}

#[test]
fn classic_roll_hover_worked_example() {
    // spec/behavior/rolling-off-ledges.md, "Example of a hover": L = S + 7.
    let rows = classic_roll_off_raised(7, 60, |_, _| {});
    let below = |f: usize| rows[f].y - 400.0;
    for (after, fallen, vy) in [
        (0, 0.00390625, 0.00390625),
        (9, 0.0390625, 0.00390625),
        (10, 0.48046875, 0.44140625),
        (11, 1.359375, 0.87890625),
    ] {
        assert_eq!((below(7 + after), rows[7 + after].vy), (fallen, vy));
    }
}

#[test]
fn classic_jumping_during_a_roll_hover() {
    // spec/behavior/rolling-off-ledges.md, "Jumping during the fall", the
    // example of a press during a hover: L = S + 7, Jump pressed on
    // L + 3. P stays a hover frame and the jump ends the hover.
    let (l, p) = (7, 10);
    for (held, rows_after) in [
        (12, [(0.28515625, -7.0), (-6.71875, -6.71875)]),
        (1, [(0.44140625, -7.0), (-6.5625, -6.5625)]),
    ] {
        let rows = classic_roll_off_raised_jumping(l, 30, p..p + held, |_, _| {});
        for f in l..=p {
            let before = if f == 0 { 400.0 } else { rows[f - 1].y };
            assert_eq!((rows[f].y - before, rows[f].vy), (0.00390625, 0.00390625));
        }
        assert_eq!(frames_with(&rows, Event::RollEnd), [p], "held {held}");
        assert_eq!(frames_with(&rows, Event::JumpStart), [p + 1], "held {held}");
        for (k, (moved, vy)) in rows_after.into_iter().enumerate() {
            let f = p + 1 + k;
            assert_eq!(
                (rows[f].y - rows[f - 1].y, rows[f].vy),
                (moved, vy),
                "held {held}, T + {k}"
            );
        }
    }
}

#[test]
fn classic_landing_after_a_roll_jump_in_the_air_waits_for_the_switch_frame() {
    // spec/behavior/jump-input.md, "The switch frame M": the first
    // character's jumps out of a roll pressed in the air, on a hover
    // frame or not, take off on P + 1 and land as any other jump, with
    // E = M + 2, not L + 3. Off `RAISED_176` these land with M = L + 2
    // (`switch_steps`), as some measured ones did.
    for (lead, after) in [(7, 3), (7, 12), (0, 4), (3, 0), (10, 2)] {
        let p = lead + after;
        for held in [1, 12] {
            let rows = classic_roll_off_raised_jumping(lead, 120, p..p + held, |_, _| {});
            assert_eq!(frames_with(&rows, Event::JumpStart), [p + 1]);
            let l = frames_with(&rows, Event::Land)[0];
            let rows = classic_roll_off_raised_pressing(
                lead,
                l + 20,
                |f| (p..p + held).contains(&f) || f + 1 >= l,
                |_, _| {},
            );
            assert_eq!(frames_with(&rows, Event::Land), [l]);
            let e = frames_with(&rows, Event::JumpStart);
            assert_eq!(
                e,
                [p + 1, l + 4],
                "lead {lead}, P = L + {after}, held {held}"
            );
        }
    }
}

#[test]
fn classic_jumping_on_every_hover_frame() {
    // spec/behavior/rolling-off-ledges.md, "Jumping during the fall": a
    // press on any hover frame keeps it a hover frame and takes off from
    // there with `vy` -7 on T; a press on an L that isn't a hover frame
    // moves down 3.28125 instead of 3.4375.
    for lead in 0..=10 {
        let hovers = |after: usize| match lead {
            0 => (1..=8).contains(&after),
            1..=7 => after <= 9,
            8 => after == 0,
            _ => false,
        };
        // Jump pressed on S itself isn't covered.
        for after in usize::from(lead == 0)..=10 {
            let p = lead + after;
            let rows = classic_roll_off_raised_jumping(lead, p + 3, p..p + 12, |_, _| {});
            let before = if p == 0 { 400.0 } else { rows[p - 1].y };
            let fall_vy = rows[p - 1].vy;
            let (moved, vy) = if hovers(after) || after == 0 {
                (if hovers(after) { 0.00390625 } else { 3.28125 }, 0.00390625)
            } else {
                let vy = (fall_vy + 0.28125).min(8.0);
                (vy, vy)
            };
            let ctx = format!("lead {lead}, P = L + {after}");
            assert_eq!((rows[p].y - before, rows[p].vy), (moved, vy), "{ctx}");
            let t = (vy + 0.28125).min(8.0);
            assert_eq!(
                (rows[p + 1].y - rows[p].y, rows[p + 1].vy),
                (t, -7.0),
                "{ctx}"
            );
            assert_eq!(rows[p + 2].vy, -6.71875, "{ctx}");
        }
    }
}

#[test]
fn classic_roll_speed_off_the_end_of_the_ground() {
    // spec/behavior/rolling-off-ledges.md, "The fall": where the ground
    // simply ends, `vx` follows the roll's speed rule on L and after, as
    // the same roll does on the ground.
    let rows = classic_roll_off_raised(12, 50, |_, _| {});
    let land = frames_with(&rows, Event::Land)[0];
    let mut w = classic_clear();
    let flat = play(&mut w, 60, |_| vec![Action::Right]);
    assert_eq!(flat[59].vx, 2.0);
    let flat = play(&mut w, land, |f| match f {
        0 => vec![Action::Action, Action::Right],
        _ => vec![Action::Right],
    });
    // The flat roll's last frame is S + 28.
    let vx = |rows: &[Row]| rows[..29].iter().map(|r| r.vx).collect::<Vec<_>>();
    assert!(land > 29);
    assert_eq!(vx(&rows), vx(&flat));
}

#[test]
fn classic_roll_ends_on_landing_after_a_30_frame_fall() {
    // spec/behavior/rolling-off-ledges.md, "When the roll ends": landing
    // exactly 30 frames after leaving the ground makes the landing frame D;
    // a fall one frame shorter or longer, or one with a hover, ends at
    // G + 30.
    let ends = |lead: usize, nudge: f64| {
        let rows = classic_roll_off_raised(lead, 100, |f, w| {
            if f == lead + 5 {
                w.player.pos.y += Fixed::from_f64(nudge).unwrap();
            }
        });
        let leave = frames_with(&rows, Event::LeaveGround)[0];
        let land = frames_with(&rows, Event::Land)[0];
        assert!(rows[..land].iter().all(|r| r.state == State::Roll));
        let end = frames_with(&rows, Event::RollEnd)[0];
        (land - leave, end - land)
    };
    assert_eq!(ends(12, 0.0), (30, 0));
    assert_eq!(ends(20, 0.0), (30, 0));
    assert_eq!(ends(3, 0.0), (40, 30));
    // Moved down or up partway through the fall, by whole pixels.
    for (way, fall) in [(1.0, 29), (-1.0, 31)] {
        let nudged = (1..=16).map(|px| ends(12, way * f64::from(px)));
        assert_eq!(nudged.clone().find(|&(f, _)| f == fall), Some((fall, 30)));
        assert!(nudged.filter(|&(f, _)| f != 30).all(|(_, end)| end == 30));
    }
}

#[test]
fn classic_roll_started_on_the_first_airborne_frame_ends_on_landing_after_a_29_frame_fall() {
    // spec/behavior/rolling-off-ledges.md, "When the roll ends": when the
    // roll starts on L itself, a fall of exactly 29 frames makes the
    // landing frame D and a 30-frame fall ends at G + 30; from L = S + 1
    // on it is the 30-frame fall.
    let ends = |lead: usize, nudge: f64| {
        let rows = classic_roll_off_raised(lead, 100, |f, w| {
            if f == lead + 5 {
                w.player.pos.y += Fixed::from_f64(nudge).unwrap();
            }
        });
        let leave = frames_with(&rows, Event::LeaveGround)[0];
        let land = frames_with(&rows, Event::Land)[0];
        let end = frames_with(&rows, Event::RollEnd)[0];
        (land - leave, end - land)
    };
    let falls = |lead: usize| -> Vec<(usize, usize)> {
        (-16..=120)
            .map(|px| ends(lead, f64::from(px)))
            .filter(|&(f, _)| (28..=31).contains(&f))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    assert_eq!(falls(0), [(28, 30), (29, 0), (30, 30), (31, 30)]);
    assert_eq!(falls(1), [(28, 30), (29, 30), (30, 0), (31, 30)]);
}

#[test]
fn classic_carry_on_roll_ends_in_a_fall_when_the_ground_ends_under_it() {
    // spec/behavior/rolling-off-ledges.md, "Carrying on into running as the
    // ground ends": with Action held throughout, a roll that leaves the
    // ground on D + 1 to D + 3 still ends on D + 3, in a fall.
    let run = |s: usize, n: usize| {
        let held = move |f: usize| {
            if f >= s {
                vec![Action::Action, Action::Right]
            } else {
                vec![Action::Right]
            }
        };
        play(&mut classic_raised(1300), s + n, held)
    };
    // Far from the edge, the carry-on ends on D + 3.
    let rows = run(1, 40);
    let start = frames_with(&rows, Event::RollStart)[0];
    assert!(frames_with(&rows, Event::LeaveGround).is_empty());
    let span = frames_with(&rows, Event::RollEnd)[0] - start;
    let mut seen = std::collections::BTreeSet::new();
    for s in 1..200 {
        let rows = run(s, 120);
        let (Some(&start), Some(&leave)) = (
            frames_with(&rows, Event::RollStart).first(),
            frames_with(&rows, Event::LeaveGround).first(),
        ) else {
            continue;
        };
        let late = leave.saturating_sub(start);
        if (span - 2..=span).contains(&late) {
            seen.insert(late);
            assert_eq!(
                frames_with(&rows, Event::RollEnd)[0],
                start + span,
                "L = S + {late}"
            );
            assert!(
                frames_with(&rows, Event::Land)[0] > start + span,
                "L = S + {late}"
            );
        }
    }
    assert_eq!(seen.len(), 3, "{seen:?}");
}

#[test]
fn classic_roll_off_the_end_of_the_ground_worked_example() {
    // spec/behavior/rolling-off-ledges.md, "When the roll ends", the
    // example where the ground simply ends: standing 56 px from the edge,
    // Right held and Action tapped on S = 0. L = 25, G = 55, and the stop
    // starts on the landing frame.
    let mut w = classic_raised(1600 - 56);
    let rows = play(&mut w, 70, |f| match f {
        0 => vec![Action::Action, Action::Right],
        _ => vec![Action::Right],
    });
    assert_eq!(frames_with(&rows, Event::LeaveGround), [25]);
    assert_eq!(frames_with(&rows, Event::Land), [55]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [55]);
    assert!(rows[..55].iter().all(|r| r.state == State::Roll));
    assert!(rows[56..58].iter().all(|r| r.state != State::Roll));
}

#[test]
fn classic_roll_stands_still_before_the_end_of_the_ground() {
    // spec/behavior/rolling-off-ledges.md, "Stopping before a ledge":
    // standing with floor(x) = E - 10, Action alone on S.
    let mut w = classic_raised(1590);
    let rows = play(&mut w, 32, |f| match f {
        0 => vec![Action::Action],
        _ => vec![],
    });
    assert!(rows[..29].iter().all(|r| r.state == State::Roll));
    assert!(
        rows.iter()
            .all(|r| r.x == 1590.0 && r.vx == 0.0 && r.grounded)
    );
    assert_eq!(frames_with(&rows, Event::RollEnd), [29]);

    // Going left toward the other end, mirrored.
    let mut w = classic_raised(809);
    w.player.facing = Facing::Left;
    let rows = play(&mut w, 32, |f| match f {
        0 => vec![Action::Action],
        _ => vec![],
    });
    assert!(
        rows.iter()
            .all(|r| r.x == 809.0 && r.vx == 0.0 && r.grounded)
    );
    assert_eq!(frames_with(&rows, Event::RollEnd), [29]);
}

#[test]
fn classic_roll_stop_next_to_an_edge_has_stopped_frames() {
    // spec/behavior/rolling-off-ledges.md, "The stop after a roll, next to
    // an edge": a frame of the stop (D to D + 3) that starts with |vx| at
    // most 271/256 and no ground 15 px ahead keeps the player still with
    // vx 0. The edge is at E = 1600 here, so floor(x) + 15 >= 1600.
    let mut stopped = 0;
    for start in 1480..1590 {
        for slow_from in [20, 23] {
            let mut w = classic_raised(start);
            for frame in 0..40 {
                let (x, vx) = (w.player.pos.x, w.player.vel.x);
                let before = w.player.roll_stop;
                let held: Vec<Action> = match frame {
                    0 => vec![Action::Action, Action::Right],
                    f if f >= slow_from && f < 28 => vec![Action::Action, Action::Left],
                    f if f < 28 => vec![Action::Action, Action::Right],
                    _ => vec![Action::Right],
                };
                hold(&mut w, &held, 1);
                // The stop's frames: the frame just played is D + n.
                if before == 0 && w.player.roll_stop == 0 || w.player.roll_stop > 4 {
                    continue;
                }
                let in_stop = w.player.roll_stop > 0 || before > 0;
                if !in_stop || vx == Fixed::ZERO || vx.to_f64() < 0.0 || !w.player.grounded() {
                    continue;
                }
                let no_ground_ahead = x.floor() + 15 >= 1600;
                if vx.to_f64() <= 271.0 / 256.0 && no_ground_ahead {
                    stopped += 1;
                    assert_eq!(w.player.vel.x, Fixed::ZERO, "start {start} frame {frame}");
                    assert_eq!(w.player.pos.x, x, "start {start} frame {frame}");
                } else if vx.to_f64() <= 271.0 / 256.0 {
                    assert!(w.player.pos.x != x, "start {start} frame {frame}");
                }
            }
        }
    }
    assert!(stopped > 0);
}

#[test]
fn classic_roll_stop_next_to_a_left_edge_has_stopped_frames() {
    // The same rule at a left edge, directions mirrored (spec
    // rolling-off-ledges.md, "The stop after a roll, next to an edge"). The
    // ground's left end is column 800, so no ground 15 px ahead means
    // floor(x) - 15 < 800.
    let mut stopped = 0;
    for start in 810..920 {
        for slow_from in [20, 23] {
            let mut w = classic_raised(start);
            w.player.facing = Facing::Left;
            for frame in 0..40 {
                let (x, vx) = (w.player.pos.x, w.player.vel.x);
                let before = w.player.roll_stop;
                let held: Vec<Action> = match frame {
                    0 => vec![Action::Action, Action::Left],
                    f if f >= slow_from && f < 28 => vec![Action::Action, Action::Right],
                    f if f < 28 => vec![Action::Action, Action::Left],
                    _ => vec![Action::Left],
                };
                hold(&mut w, &held, 1);
                if before == 0 && w.player.roll_stop == 0 || w.player.roll_stop > 4 {
                    continue;
                }
                let in_stop = w.player.roll_stop > 0 || before > 0;
                if !in_stop || vx == Fixed::ZERO || vx.to_f64() > 0.0 || !w.player.grounded() {
                    continue;
                }
                let no_ground_ahead = x.floor() - 15 < 800;
                if -vx.to_f64() <= 271.0 / 256.0 && no_ground_ahead {
                    stopped += 1;
                    assert_eq!(w.player.vel.x, Fixed::ZERO, "start {start} frame {frame}");
                    assert_eq!(w.player.pos.x, x, "start {start} frame {frame}");
                } else if -vx.to_f64() <= 271.0 / 256.0 {
                    assert!(w.player.pos.x != x, "start {start} frame {frame}");
                }
            }
        }
    }
    assert!(stopped > 0);
}

#[test]
fn classic_roll_stop_pressing_toward_the_edge_while_moving_away_from_it() {
    // spec/behavior/rolling-off-ledges.md, "Pressing toward the edge while
    // moving away from it": a roll reversed away from the edge at E = 1600,
    // then Right and Action held, tests the ground 15 px toward the edge,
    // not the way the player moves.
    let mut stopped = 0;
    let mut moved = 0;
    for start in 1560..1600 {
        for (slow_from, slow_to) in
            (1..22).flat_map(|from| (from + 2..31).map(move |to| (from, to)))
        {
            let mut w = classic_raised(start);
            for frame in 0..44 {
                let (x, vx) = (w.player.pos.x, w.player.vel.x);
                let before = w.player.roll_stop;
                let held: Vec<Action> = match frame {
                    0 => vec![Action::Action, Action::Right],
                    f if (slow_from..slow_to).contains(&f) => vec![Action::Action, Action::Left],
                    f if f < 28 => vec![Action::Action, Action::Right],
                    28 => vec![Action::Right],
                    _ => vec![Action::Action, Action::Right],
                };
                hold(&mut w, &held, 1);
                if before == 0 && w.player.roll_stop == 0 || w.player.roll_stop > 4 {
                    continue;
                }
                let toward = held.contains(&Action::Right) && held.contains(&Action::Action);
                if !toward || vx.to_f64() >= 0.0 || !w.player.grounded() {
                    continue;
                }
                let slow = -vx.to_f64() <= 271.0 / 256.0;
                if slow && x.floor() + 15 >= 1600 {
                    stopped += 1;
                    assert_eq!(w.player.vel.x, Fixed::ZERO, "start {start} frame {frame}");
                    assert_eq!(w.player.pos.x, x, "start {start} frame {frame}");
                } else if slow && -vx.to_f64() > 0.05 {
                    moved += 1;
                    assert!(w.player.pos.x != x, "start {start} frame {frame}");
                }
            }
        }
    }
    assert!(stopped > 0 && moved > 0, "{stopped} {moved}");
}

#[test]
fn classic_walk_off_fall_jumps_on_l_plus_2() {
    // spec/behavior/jump-input.md, "Landings this doesn't cover": the lead
    // walking off a ledge with no face has E = L + 2: a Jump press from
    // L - 10 on, held through L + 1, starts the jump on L + 2; one on
    // L - 11 or earlier gives none; one on L + 1 or later starts it on
    // the frame after.
    let mut checked = 0;
    for press in 0..50 {
        let mut w = classic_raised(1590);
        let probe = play(&mut w, 120, |_| vec![Action::Right]);
        let leave = frames_with(&probe, Event::LeaveGround)[0];
        let land = frames_with(&probe, Event::Land)[0];
        if !(13..=42).contains(&(land - leave)) || press < leave + 1 || press > land + 3 {
            continue;
        }
        let mut w = classic_raised(1590);
        let rows = play(&mut w, land + 10, |f| match f {
            f if f == press => vec![Action::Right, Action::Jump],
            f if f > press && f < land + 3 => vec![Action::Right, Action::Jump],
            _ => vec![Action::Right],
        });
        let jumps = frames_with(&rows, Event::JumpStart);
        if press + 10 < land {
            assert!(jumps.is_empty(), "press {press} land {land}: {jumps:?}");
        } else {
            assert_eq!(
                jumps,
                [(land + 2).max(press + 1)],
                "press {press} land {land}"
            );
        }
        checked += 1;
    }
    assert!(checked > 5);
}

#[test]
fn classic_roll_pushes_off_toward_the_end_of_the_ground() {
    // spec/behavior/rolling-off-ledges.md, "Stopping before a ledge": the
    // same place, Right and Action on S and Right held after.
    let mut w = classic_raised(1590);
    let rows = play(&mut w, 8, |f| match f {
        0 => vec![Action::Action, Action::Right],
        _ => vec![Action::Right],
    });
    assert_eq!((rows[0].x, rows[0].vx), (1590.0, 0.0));
    assert_eq!((rows[1].x, rows[1].vx), (1592.0, 2.0));
    let vx: Vec<_> = rows[2..5].iter().map(|r| r.vx).collect();
    assert_eq!(vx, [2.125, 2.234375, 2.328125]);
    assert_eq!(frames_with(&rows, Event::LeaveGround), [5]);

    // Without a direction held it stays put.
    let mut w = classic_raised(1590);
    let rows = play(&mut w, 8, |f| match f {
        0 => vec![Action::Action, Action::Right],
        _ => vec![],
    });
    assert!(rows.iter().all(|r| r.x == 1590.0));
}

#[test]
fn classic_pushed_roll_goes_on_as_a_normal_roll() {
    // spec/behavior/rolling-off-ledges.md, "Pushing off": a slow roll
    // pushed off eases toward 3 from 2 with its direction held.
    let mut w = classic_raised(1590);
    let rows = play(&mut w, 4, |f| match f {
        0 => vec![Action::Action],
        _ => vec![Action::Right],
    });
    assert_eq!(w.player.roll.unwrap().size, RollSize::Slow);
    let vx: Vec<_> = rows.iter().map(|r| r.vx).collect();
    assert_eq!(vx, [0.0, 2.0, 2.125, 2.234375]);

    // With nothing held after the push it goes on at 2, whatever its size.
    for first in [vec![Action::Action], vec![Action::Action, Action::Right]] {
        let mut w = classic_raised(1590);
        let rows = play(&mut w, 4, |f| match f {
            0 => first.clone(),
            1 => vec![Action::Right],
            _ => vec![],
        });
        let vx: Vec<_> = rows.iter().map(|r| r.vx).collect();
        assert_eq!(vx, [0.0, 2.0, 2.0, 2.0]);
    }
}

#[test]
fn classic_slow_roll_stops_within_15_px_of_the_end_of_the_ground() {
    // spec/behavior/rolling-off-ledges.md, "Stopping before a ledge":
    // walking right, then Right released and Action alone, `vx`
    // 1.51953125 on S. The spec doesn't say how the walk slowed before S,
    // so the speed at the start of S is set to the one the speed rule
    // takes to 1.51953125 (1.59375). The slow roll's first frame to start
    // with floor(x) = E - 15 is S + 24, and it stays there to D.
    let walked = |x: i32| {
        let mut w = classic_raised(1300);
        play(&mut w, 60, |_| vec![Action::Right]);
        w.player.pos.x = Fixed::from_int(x);
        w.player.vel.x = Fixed::from_f64(1.59375).unwrap();
        let rows = play(&mut w, 32, |f| match f {
            0 => vec![Action::Action],
            _ => vec![],
        });
        assert_eq!(rows[0].vx, 1.51953125);
        rows
    };
    let mut found = false;
    for x in 1500..1570 {
        let rows = walked(x);
        let stop = (1..29).find(|&f| rows[f].x == rows[f - 1].x);
        let Some(stop) = stop else { continue };
        // It starts with no ground 15 px ahead, and the frame before it
        // didn't, or started faster than 271/256.
        let column = |f: usize| rows[f].x.floor();
        assert!(column(stop - 1) >= RAISED_END - 15.0, "from {x}");
        assert!(stop < 2 || column(stop - 2) < RAISED_END - 15.0 || rows[stop - 2].vx > 1.05859375);
        assert!(rows[stop..29].iter().all(|r| r.vx == 0.0));
        assert!(rows.iter().all(|r| r.grounded));
        if stop == 24 {
            assert_eq!(rows[23].vx, 1.0390625);
            found = true;
        }
    }
    assert!(found);
}

#[test]
fn classic_roll_started_in_the_stop_pushes_off_and_hovers() {
    // spec/behavior/rolling-off-ledges.md, "A roll started in the stop
    // after a roll", the first example: floor(x) = E - 10, Action alone
    // on 0, so D = 29. Action again on 34 for one frame (S' = 46) and
    // Right held from 47.
    let mut w = classic_raised(1590);
    let rows = play(&mut w, 75, |f| match f {
        0 | 34 => vec![Action::Action],
        f if f >= 47 => vec![Action::Right],
        _ => vec![],
    });
    assert_eq!(frames_with(&rows, Event::RollEnd)[0], 29);
    assert!(rows[..47].iter().all(|r| r.x == 1590.0));
    assert_eq!(rows[46].state, State::Roll);
    assert_ne!(rows[45].state, State::Roll);
    let vx: Vec<_> = rows[47..51].iter().map(|r| r.vx).collect();
    assert_eq!(vx, [2.0, 2.125, 2.234375, 2.328125]);
    assert_eq!(frames_with(&rows, Event::LeaveGround), [51]);
    assert_eq!(hover_frames(&rows, 51), (51..=60).collect::<Vec<_>>());
}

#[test]
fn classic_roll_started_in_the_stop_in_the_air() {
    // spec/behavior/rolling-off-ledges.md, "When the stop carries the
    // player off the edge", the example: floor(x) = E - 4, Action alone
    // on 0, so D = 29; Action again on 28 for one frame, Right held from
    // 29. The stop carries the player off on 31, and the remembered press
    // starts the roll on 46 in the air.
    let mut w = classic_raised(1596);
    let rows = play(&mut w, 70, |f| match f {
        0 | 28 => vec![Action::Action],
        f if f >= 29 => vec![Action::Right],
        _ => vec![],
    });
    assert_eq!(frames_with(&rows, Event::RollEnd)[0], 29);
    let vx: Vec<_> = rows[29..33].iter().map(|r| r.vx).collect();
    assert_eq!(vx, [2.0, 1.75, 1.53125, 0.0]);
    assert_eq!(frames_with(&rows, Event::LeaveGround), [31]);
    assert_eq!(rows[31].y - rows[30].y, 3.4375);
    for f in 32..46 {
        assert_eq!(rows[f].vx, 0.0, "frame {f}");
        assert_eq!(rows[f].vy, (rows[f - 1].vy + 0.4375).min(8.0), "frame {f}");
    }
    assert_ne!(rows[45].state, State::Roll);
    assert_eq!(rows[46].state, State::Roll);
    assert_eq!((rows[46].vx, rows[46].vy), (0.0, 6.56640625));
    assert_eq!(rows[47].vx, 0.375);
    assert_eq!(hover_frames(&rows, 46), (47..=55).collect::<Vec<_>>());
    assert_eq!(rows[56].vy, 0.44140625);
}

/// Classic 1 on `RAISED_176` standing at `x`: Action alone on 0, a slow
/// roll that stops on the ground (D = 29), then Action again on `press`
/// for one frame and Right held from `right`.
fn classic_roll_again(x: i32, press: usize, right: usize, n: usize) -> Vec<Row> {
    let mut w = classic_raised(x);
    let rows = play(&mut w, n, |f| {
        let mut keys = if f == 0 || f == press {
            vec![Action::Action]
        } else {
            vec![]
        };
        if f >= right {
            keys.push(Action::Right);
        }
        keys
    });
    assert_eq!(frames_with(&rows, Event::RollEnd)[0], 29, "from {x}");
    assert!(rows[..30].iter().all(|r| r.grounded), "from {x}");
    rows
}

/// The frames from `l` to `l` + 12 that move down by 1/256.
fn hover_frames(rows: &[Row], l: usize) -> Vec<usize> {
    (l..=l + 12)
        .filter(|&f| rows[f].y - rows[f - 1].y == 0.00390625)
        .collect()
}

#[test]
fn classic_roll_started_on_d_17_hovers_by_its_own_table() {
    // spec/behavior/rolling-off-ledges.md, "A roll started in the stop
    // after a roll": a roll started on D + 17 hovers on L and the 9 frames
    // after it for n = L - S' from 1 to 8, on L only for 9, and not at all
    // from 10. Remembered presses (S' = 46) and new ones on 46.
    for press in [34, 46] {
        let mut seen = std::collections::BTreeSet::new();
        for (x, right) in (1450..1598).flat_map(|x| [46, 49, 52, 55].map(|r| (x, r))) {
            let rows = classic_roll_again(x, press, right, 130);
            let leaves = frames_with(&rows, Event::LeaveGround);
            // Off the edge well before the roll ends.
            let Some(&l) = leaves.iter().find(|&&l| l <= 46 + 20) else {
                continue;
            };
            assert!(rows[45].state != State::Roll && rows[46].state == State::Roll);
            let n = l - 46;
            let hovers: Vec<usize> = match n {
                1..=8 => (l..l + 10).collect(),
                9 => vec![l],
                _ => vec![],
            };
            let ctx = format!("from {x}, press {press}, Right from {right}, n {n}");
            assert_eq!(hover_frames(&rows, l), hovers, "{ctx}");
            seen.insert(n);
        }
        assert!(
            (2..=10).all(|n| seen.contains(&n)),
            "press {press}: {seen:?}"
        );
    }
}

#[test]
fn classic_roll_started_on_d_18_hovers_as_usual() {
    // spec/behavior/rolling-off-ledges.md, "A roll started in the stop
    // after a roll": one started on D + 18 is an ordinary roll, with
    // n = L - S' in "The fall" (L and the 9 after it up to 7, L only on 8).
    let mut seen = std::collections::BTreeSet::new();
    for (x, right) in (1450..1598).flat_map(|x| [47, 50, 53, 56].map(|r| (x, r))) {
        let rows = classic_roll_again(x, 47, right, 130);
        let leaves = frames_with(&rows, Event::LeaveGround);
        let Some(&l) = leaves.iter().find(|&&l| l <= 47 + 20) else {
            continue;
        };
        let n = l - 47;
        let hovers: Vec<usize> = match n {
            0 => (l + 1..=l + 8).collect(),
            1..=7 => (l..l + 10).collect(),
            8 => vec![l],
            _ => vec![],
        };
        assert_eq!(
            hover_frames(&rows, l),
            hovers,
            "from {x}, Right from {right}, n {n}"
        );
        seen.insert(n);
    }
    assert!((2..=9).all(|n| seen.contains(&n)), "{seen:?}");
}

#[test]
fn classic_roll_started_on_d_17_lasts_30_frames() {
    // spec/behavior/roll-input.md, "Starting the next roll": a roll
    // started on D + 17 lasts 30 frames, from a remembered press or a new
    // one; on D + 18 it lasts 29.
    for (press, start, end) in [(34, 46, 76), (46, 46, 76), (47, 47, 76)] {
        let rows = classic_roll_again(1000, press, usize::MAX, 90);
        assert_eq!(
            frames_with(&rows, Event::RollStart),
            [0, start],
            "press {press}"
        );
        assert_eq!(
            frames_with(&rows, Event::RollEnd),
            [29, end],
            "press {press}"
        );
    }
}

#[test]
fn classic_jump_press_on_d_17_beats_a_remembered_roll_press() {
    // spec/behavior/roll-input.md, "Starting the next roll": a Jump press
    // on D + 17 beats a remembered roll press and a new one on D + 17; the
    // jump starts on D + 18 and no roll starts. Earlier it jumps too; on
    // D + 18 the roll has started already on D + 17.
    for press in [34, 46] {
        for jump in [44, 45, 46] {
            let mut w = classic_raised(1000);
            let rows = play(&mut w, 60, |f| {
                let mut keys = match f {
                    0 => vec![Action::Action, Action::Right],
                    f if f == press => vec![Action::Action],
                    28..34 => vec![Action::Right],
                    _ => vec![],
                };
                if (jump..jump + 4).contains(&f) {
                    keys.extend([Action::Jump, Action::Right]);
                }
                keys
            });
            assert_eq!(
                frames_with(&rows, Event::RollStart),
                [0],
                "press {press} jump {jump}"
            );
            assert_eq!(
                frames_with(&rows, Event::JumpStart),
                [jump + 1],
                "press {press} jump {jump}"
            );
        }
    }
    // On D + 18 the roll starts on D + 17 from the remembered press.
    let mut w = classic_raised(1000);
    let rows = play(&mut w, 60, |f| match f {
        0 | 34 => vec![Action::Action],
        47..=50 => vec![Action::Jump],
        _ => vec![],
    });
    assert_eq!(frames_with(&rows, Event::RollStart), [0, 46]);
}

#[test]
fn classic_new_press_on_d_17_sets_size_and_direction_then() {
    // spec/behavior/roll-speed.md, "When the roll ends": facing right
    // with Left held on D + 17 only, a new press then goes left as a
    // normal roll from D + 18.
    let mut w = classic_raised(1000);
    let rows = play(&mut w, 50, |f| match f {
        0 => vec![Action::Action],
        46 => vec![Action::Action, Action::Left],
        _ => vec![],
    });
    assert_eq!(rows[45].facing, Facing::Right);
    assert_eq!(rows[46].vx, 0.0);
    assert_eq!(w.player.roll.unwrap().size, RollSize::Normal);
    let vx: Vec<_> = rows[47..49].iter().map(|r| r.vx).collect();
    assert_eq!(vx, [-0.375, -0.703125]);
    assert!(rows[47..].iter().all(|r| r.facing == Facing::Left));
}

#[test]
fn classic_remembered_press_sets_size_and_direction_on_d_17() {
    // spec/behavior/roll-speed.md, "When the roll ends": a roll started
    // on D + 17 (46) has it as its press frame, whether the press was
    // remembered (34) or new (46). Facing right: Left on D + 17 only goes
    // left as a normal roll; Right on D + 17 and Left from D + 18 is a
    // normal roll going right that turns left on D + 18; nothing on
    // D + 17 and Right on D + 18 only is a slow roll made normal then.
    for press in [34, 46] {
        let cases = [
            (Some(Action::Left), None, RollSize::Normal, Facing::Left),
            (
                Some(Action::Right),
                Some(Action::Left),
                RollSize::Normal,
                Facing::Left,
            ),
            (None, Some(Action::Right), RollSize::Slow, Facing::Right),
        ];
        for (on_17, after, size, dir) in cases {
            let mut w = classic_raised(1000);
            let rows = play(&mut w, 48, |f| {
                let mut keys = match f {
                    0 => vec![Action::Action],
                    f if f == press => vec![Action::Action],
                    _ => vec![],
                };
                match f {
                    46 => keys.extend(on_17),
                    47.. => keys.extend(after),
                    _ => {}
                }
                keys
            });
            let ctx = format!("press {press}, {on_17:?} then {after:?}");
            assert_eq!(rows[45].facing, Facing::Right, "{ctx}");
            assert_eq!(rows[46].state, State::Roll, "{ctx}");
            assert_eq!(rows[46].vx, 0.0, "{ctx}");
            let roll = w.player.roll.unwrap();
            assert_eq!((roll.size, roll.dir), (size, dir), "{ctx}");
            let vx = if dir == Facing::Left { -0.375 } else { 0.375 };
            assert_eq!(rows[47].vx, vx, "{ctx}");
            assert_eq!(rows[47].target.abs(), 3.0, "{ctx}");
        }
    }
}

#[test]
fn classic_running_in_the_stop_does_not_make_the_next_roll_fast() {
    // spec/behavior/roll-speed.md, "Running recently": Action and a
    // direction held in the stop, up to D + 16, aren't running frames.
    let mut w = classic_raised(1000);
    play(&mut w, 47, |f| match f {
        0 => vec![Action::Action],
        30..=45 => vec![Action::Action, Action::Right],
        46 => vec![Action::Action, Action::Right],
        _ => vec![],
    });
    assert_eq!(w.player.roll.unwrap().size, RollSize::Normal);
}

#[test]
fn classic_turned_roll_before_the_end_of_the_ground() {
    // spec/behavior/rolling-off-ledges.md, "Stopping before a ledge":
    // a roll started on D + 17 within 15 px of the edge, with Left held on
    // S' + 1, turns and pushes off away from the edge at 2 on S' + 1.
    let mut w = classic_raised(1590);
    let rows = play(&mut w, 49, |f| match f {
        0 | 34 => vec![Action::Action],
        47.. => vec![Action::Left],
        _ => vec![],
    });
    assert_eq!(rows[46].x, 1590.0);
    assert_eq!((rows[47].x, rows[47].vx), (1588.0, -2.0));

    // A slow roll steered away from the edge as it reaches it moves on.
    let mut w = classic_raised(1570);
    let rows = play(&mut w, 40, |f| match f {
        0 => vec![Action::Action],
        _ => vec![],
    });
    let stop = (1..29)
        .find(|&f| rows[f].x == rows[f - 1].x)
        .expect("a stop");
    let mut w = classic_raised(1570);
    let rows = play(&mut w, 29, |f| match f {
        0 => vec![Action::Action],
        f if f >= stop => vec![Action::Left],
        _ => vec![],
    });
    assert_ne!(rows[stop].x, rows[stop - 1].x);
    assert!(rows[stop].target < 0.0);
}

#[test]
fn classic_roll_started_on_d_17_takes_its_direction_near_the_edge() {
    // spec/behavior/rolling-off-ledges.md, "A roll started in the stop
    // after a roll": facing the edge within 15 px of it (E - 10), with
    // Left held on S' (46) only, the roll goes away from the edge as a
    // normal roll. Held away on S' and toward the edge from S' + 1, S'
    // isn't a stopped frame, S' + 1 is, and the roll pushes off toward the
    // edge on S' + 2. Remembered presses (34) and new ones on 46.
    for press in [34, 46] {
        let mut w = classic_raised(1590);
        let rows = play(&mut w, 49, |f| match f {
            0 => vec![Action::Action],
            46 if press == 46 => vec![Action::Action, Action::Left],
            46 => vec![Action::Left],
            f if f == press => vec![Action::Action],
            _ => vec![],
        });
        assert_eq!(rows[46].x, 1590.0, "press {press}");
        assert_eq!(w.player.roll.unwrap().size, RollSize::Normal);
        let vx: Vec<_> = rows[47..49].iter().map(|r| r.vx).collect();
        assert_eq!(vx, [-0.375, -0.703125], "press {press}");

        let mut w = classic_raised(1590);
        let rows = play(&mut w, 50, |f| match f {
            0 => vec![Action::Action],
            46 if press == 46 => vec![Action::Action, Action::Left],
            46 => vec![Action::Left],
            f if f == press => vec![Action::Action],
            47.. => vec![Action::Right],
            _ => vec![],
        });
        assert_eq!((rows[47].x, rows[47].vx), (1590.0, 0.0), "press {press}");
        assert_eq!((rows[48].x, rows[48].vx), (1592.0, 2.0), "press {press}");
    }
}

#[test]
fn classic_roll_started_in_the_air_ends_on_landing_30_frames_after() {
    // spec/behavior/rolling-off-ledges.md, "When the stop carries the
    // player off the edge": the example's roll starts on S' = 46 in the
    // air. Moved up or down during the stop's fall so that it lands
    // G = S' + 26
    // to S' + 35, it ends on G when G = S' + 30 and on G + 30 otherwise.
    let mut seen = std::collections::BTreeSet::new();
    for lift in -60..20 {
        let mut w = classic_raised(1596);
        let keys = |f: usize| match f {
            0 | 28 => vec![Action::Action],
            f if f >= 29 => vec![Action::Right],
            _ => vec![],
        };
        let mut rows = play(&mut w, 40, keys);
        w.player.pos.y -= Fixed::from_int(lift);
        rows.extend(play(&mut w, 120, |f| keys(f + 40)));
        assert_eq!(rows[46].state, State::Roll, "lift {lift}");
        let Some(g) = (47..rows.len()).find(|&f| rows[f].grounded) else {
            continue;
        };
        let n = g - 46;
        if !(26..=35).contains(&n) {
            continue;
        }
        let end = if n == 30 { g } else { g + 30 };
        assert_eq!(
            frames_with(&rows, Event::RollEnd)[1],
            end,
            "lift {lift}, n {n}"
        );
        seen.insert(n);
    }
    assert!((26..=35).all(|n| seen.contains(&n)), "{seen:?}");
}

#[test]
fn classic_roll_started_in_the_air_lands_on_s_prime_and_hover_frames() {
    // spec/behavior/rolling-off-ledges.md, "When the stop carries the
    // player off the edge": the landing rule applies on every airborne
    // frame from S' = 46 on, hover frames included. With the height set
    // on one frame from S' - 1 to S' + 10 to 1 to 16 px above the floor
    // (576), the roll lands 0 to 16 frames after S' as any landing
    // (`vy` 0, the roll goes on, the same horizontal speed) and ends on
    // G + 30.
    let keys = |f: usize| match f {
        0 | 28 => vec![Action::Action],
        f if f >= 29 => vec![Action::Right],
        _ => vec![],
    };
    let unmoved = play(&mut classic_raised(1596), 120, keys);
    let mut seen = std::collections::BTreeSet::new();
    for at in 45..=56 {
        for h in (256..=16 * 256).step_by(13).chain(5 * 256..=5 * 256 + 12) {
            let mut w = classic_raised(1596);
            let mut rows = play(&mut w, at, keys);
            w.player.pos.y = Fixed::from_int(576) - Fixed::from_ratio_pow2(h, 8);
            rows.extend(play(&mut w, 120 - at, |f| keys(f + at)));
            let ctx = format!("height {h}/256 set before {at}");
            let Some(g) = (46..rows.len()).find(|&f| rows[f].grounded) else {
                panic!("{ctx}: no landing");
            };
            if rows[45].grounded {
                continue;
            }
            assert_eq!(rows[46].state, State::Roll, "{ctx}");
            assert!(rows[at..g].iter().all(|r| r.y <= 571.0), "{ctx}");
            assert_eq!((rows[g].y, rows[g].vy), (576.0, 0.0), "{ctx}");
            assert_eq!(rows[g].state, State::Roll, "{ctx}");
            for f in 46..=g {
                assert_eq!(rows[f].vx, unmoved[f].vx, "{ctx}, frame {f}");
            }
            assert_eq!(frames_with(&rows, Event::RollEnd)[1], g + 30, "{ctx}");
            seen.insert(g - 46);
        }
    }
    assert!((0..=10).all(|n| seen.contains(&n)), "{seen:?}");
}

/// Classic 2 standing at x = 500, playing `held(frame)` for `n` frames.
fn classic_2_rolls(n: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    let mut w = classic_2_clear();
    play(&mut w, n, held)
}

/// The roll on Classic 2's current frame, for its size.
fn classic_2_roll_size(w: &Play) -> RollSize {
    w.player.roll.expect("a roll").size
}

#[test]
fn classic_2_roll_matches_the_reference_trace() {
    // spec/traces/reference/title-2/roll-flat.trace: walking right, a
    // one-frame Action press on 30, the roll's 31 frames, the stop on
    // 61 to 75 and walking again from 76.
    let trace = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/traces/reference/title-2/roll-flat.trace"
    ))
    .unwrap();
    let lines: Vec<Vec<&str>> = trace
        .lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .map(|l| l.split('\t').collect())
        .collect();
    let inputs: Vec<Vec<Action>> = lines
        .iter()
        .map(|cols| {
            cols[1]
                .chars()
                .filter(|&c| c != '.')
                .map(|c| match c {
                    'R' => Action::Right,
                    'A' => Action::Action,
                    _ => panic!("input {c}"),
                })
                .collect()
        })
        .collect();
    let mut w = classic_2_clear();
    let x0 = w.player.pos.x.to_f64();
    let rows = play(&mut w, inputs.len(), |f| inputs[f].clone());
    for (row, cols) in rows.iter().zip(&lines) {
        let events: Vec<_> = row.events.iter().map(|e| e.name()).collect();
        let facing = match row.facing {
            Facing::Left => "L",
            Facing::Right => "R",
        };
        assert_eq!(
            (
                format!("{}", row.x - x0),
                format!("{}", row.vx),
                row.state.name(),
                facing,
                events.join(",")
            ),
            (
                cols[2].to_string(),
                cols[4].to_string(),
                cols[6],
                cols[8],
                cols.get(9).copied().unwrap_or("").to_string()
            ),
            "frame {}",
            cols[0]
        );
    }
}

/// A row of the `title-2` worked values: the size, `vx` on S, S + 1 and
/// S + 2, `vx` on S + 30, and the distances from S to S + 30 and to D.
type RollRow = (RollSize, [f64; 3], f64, f64, f64);

/// Checks a row of spec/behavior/roll-speed.md, "Worked values
/// (`title-2`)": a Classic 2 roll from `w` holding `on_s` on its first
/// frame and `then` after, ending in a stop.
fn check_classic_2_roll(mut w: Play, on_s: &[Action], then: &[Action], expected: RollRow) {
    let (size, first_vx, vx_last, to_last, to_d) = expected;
    let x0 = w.player.pos.x.to_f64();
    let rows = play(&mut w, 1, |_| on_s.to_vec());
    assert_eq!(rows[0].events, [Event::RollStart]);
    assert_eq!(classic_2_roll_size(&w), size);
    let rows: Vec<_> = rows
        .into_iter()
        .chain(play(&mut w, 32, |_| then.to_vec()))
        .collect();
    assert_eq!([rows[0].vx, rows[1].vx, rows[2].vx], first_vx);
    assert_eq!(rows[30].vx, vx_last);
    assert_eq!(rows[30].x - x0, to_last);
    assert_eq!(frames_with(&rows, Event::RollEnd), [31]);
    assert_eq!((rows[31].x - x0, rows[31].vx), (to_d, 0.0));
    assert!(rows[..31].iter().all(|r| r.state == State::Roll));
    assert_eq!(rows[31].state, State::Idle);
}

#[test]
fn classic_2_roll_worked_values() {
    use Action::{Action as A, Right as R};
    check_classic_2_roll(
        classic_2_clear(),
        &[A],
        &[],
        (
            RollSize::Slow,
            [0.140625, 0.26171875, 0.3671875],
            1.09375,
            26.78515625,
            27.8828125,
        ),
    );
    check_classic_2_roll(
        classic_2_clear(),
        &[A, R],
        &[R],
        (
            RollSize::Normal,
            [0.421875, 0.7890625, 1.109375],
            3.30859375,
            81.02734375,
            84.34375,
        ),
    );
    check_classic_2_roll(
        classic_2_after(&[R], 2.25),
        &[A, R],
        &[R],
        (
            RollSize::Normal,
            [2.390625, 2.51171875, 2.6171875],
            3.34375,
            96.53515625,
            99.8828125,
        ),
    );
    // Running right with Action held for 66 frames, then Right alone for
    // a frame. The row gives `vx` 3.15625 after that frame, which isn't
    // what running reaches in 66 frames here, so the roll starts from it.
    let mut w = classic_2_clear();
    hold(&mut w, &[A, R], 66);
    hold(&mut w, &[R], 1);
    w.player.vel.x = Fixed::from_f64(3.15625).unwrap();
    check_classic_2_roll(
        w,
        &[A, R],
        &[R],
        (
            RollSize::Fast,
            [3.32421875, 3.46875, 3.59765625],
            4.46484375,
            129.953125,
            134.421875,
        ),
    );
}

#[test]
fn classic_2_roll_steering() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/roll-speed.md, "Worked values (`title-2`)": walking
    // right at 2.25, Action and Right on S, then Left.
    let mut w = classic_2_after(&[R], 2.25);
    let x0 = w.player.pos.x.to_f64();
    let rows = play(&mut w, 32, |f| if f == 0 { vec![A, R] } else { vec![L] });
    assert_eq!((rows[0].vx, rows[0].facing), (2.390625, Facing::Right));
    assert_eq!((rows[1].vx, rows[1].facing), (1.671875, Facing::Left));
    assert_eq!(rows[2].vx, 1.04296875);
    assert_eq!(rows.iter().position(|r| r.vx < 0.0), Some(5));
    assert_eq!(rows[30].vx, -3.2578125);
    assert_eq!(rows[31].x - x0, -62.16796875);
    assert!(rows[1..31].iter().all(|r| r.state == State::Roll));

    // Holding the roll's way from S + 10 doesn't make a slow roll normal.
    let alone = classic_2_rolls(31, |f| if f == 0 { vec![A] } else { vec![] });
    let along = classic_2_rolls(31, |f| match f {
        0 => vec![A],
        1..10 => vec![],
        _ => vec![R],
    });
    assert_eq!(vx_grains(&along), vx_grains(&alone));

    // The other way reverses it on any frame, any number of times, with
    // no turn delay; the size stays.
    let rows = classic_2_rolls(31, |f| match f {
        0 => vec![A, R],
        5..8 => vec![L],
        12 => vec![R],
        _ => vec![],
    });
    assert_eq!(rows[4].facing, Facing::Right);
    assert_eq!(rows[5].facing, Facing::Left);
    assert_eq!(rows[5].target, -3.375);
    assert_eq!(rows[11].facing, Facing::Left);
    assert_eq!((rows[12].facing, rows[12].target), (Facing::Right, 3.375));
}

/// The size of a Classic 2 roll pressed with `on_s` on the frame after
/// `before`, a list of what to hold and for how long.
fn classic_2_size_after(before: &[(&[Action], usize)], on_s: &[Action]) -> RollSize {
    let mut w = classic_2_clear();
    for &(keys, n) in before {
        hold(&mut w, keys, n);
    }
    let step = hold(&mut w, on_s, 1).pop().unwrap();
    assert_eq!(step.events, [Event::RollStart], "{before:?}");
    classic_2_roll_size(&w)
}

#[test]
fn classic_2_roll_sizes() {
    // spec/behavior/roll-speed.md, "Size (`title-2`)": the table of
    // examples. A hold of Action from rest rolls, then carries on into
    // running.
    use Action::{Action as A, Right as R};
    let run: (&[Action], usize) = (&[A, R], 66);
    for gap in [1, 10, 19] {
        assert_eq!(
            classic_2_size_after(&[run, (&[R], gap)], &[A, R]),
            RollSize::Fast,
            "{gap}"
        );
    }
    assert_eq!(
        classic_2_size_after(&[run, (&[R], 20)], &[A, R]),
        RollSize::Normal
    );
    assert_eq!(
        classic_2_size_after(&[run, (&[R], 1)], &[A]),
        RollSize::Slow
    );
    // Both ways held is no direction.
    assert_eq!(
        classic_2_size_after(&[run, (&[R], 1)], &[A, R, Action::Left]),
        RollSize::Slow
    );
    // The way opposite to facing counts as a direction.
    assert_eq!(
        classic_2_size_after(&[run, (&[R], 1)], &[A, Action::Left]),
        RollSize::Fast
    );
    // Action held through a slow roll that then stops, then the stop.
    assert_eq!(
        classic_2_size_after(&[(&[A], 31), (&[], 15)], &[A, R]),
        RollSize::Normal
    );
    // Holds of 32 and 31 frames in the air, from frame 20 of a jump.
    for (hold, size) in [(32, RollSize::Fast), (31, RollSize::Normal)] {
        assert_eq!(
            classic_2_size_after(
                &[(&[Action::Jump], 2), (&[], 18), (&[A], hold), (&[], 1)],
                &[A, R]
            ),
            size,
            "{hold}"
        );
    }
    // Only the last hold counts: a tap in the air after a long hold
    // there. Without the tap, the roll is fast.
    let jump: (&[Action], usize) = (&[Action::Jump], 2);
    let long: (&[Action], usize) = (&[A], 32);
    let tap: (&[Action], usize) = (&[A], 1);
    assert_eq!(
        classic_2_size_after(&[jump, (&[], 1), long, (&[], 1), tap, (&[], 13)], &[A, R]),
        RollSize::Normal
    );
    assert_eq!(
        classic_2_size_after(&[jump, (&[], 1), long, (&[], 15)], &[A, R]),
        RollSize::Fast
    );
}

/// Classic 2 standing, with Action pressed (or held) on the frames
/// `action` gives, for `n` frames.
fn classic_2_presses(action: impl Fn(usize) -> bool, n: usize) -> Vec<Row> {
    classic_2_rolls(n, |f| {
        if action(f) {
            vec![Action::Action]
        } else {
            vec![]
        }
    })
}

#[test]
fn classic_2_roll_input_worked_example() {
    // spec/behavior/roll-input.md, "Worked example (`title-2`)": a
    // one-frame press on 10 rolls on 10 to 40, stops on 41, stands still
    // on 42 to 55 and moves again from 56.
    let rows = classic_2_presses(|f| f == 10, 70);
    assert_eq!(frames_with(&rows, Event::RollStart), [10]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [41]);
    assert!(rows[10..41].iter().all(|r| r.state == State::Roll));
    assert!(rows[41..56].iter().all(|r| r.state == State::Idle));
    assert!(rows[42..56].iter().all(|r| r.x == rows[41].x));

    // A one-frame press on 11 to 55 is ignored.
    for p in 11..56 {
        let rows = classic_2_presses(|f| f == 10 || f == p, 70);
        assert_eq!(frames_with(&rows, Event::RollStart), [10], "{p}");
    }
    // A press on 45 to 55 held through 56 starts the next roll on 56, as
    // does a press on 56; one on 44 doesn't.
    for p in 45..57 {
        let rows = classic_2_presses(|f| f == 10 || (p..=56).contains(&f), 70);
        assert_eq!(frames_with(&rows, Event::RollStart), [10, 56], "{p}");
        // It moves on its first frame.
        assert!(rows[56].vx > 0.0);
    }
    let rows = classic_2_presses(|f| f == 10 || (44..=56).contains(&f), 70);
    assert_eq!(frames_with(&rows, Event::RollStart), [10]);
    // Let go of before 56, the press is forgotten.
    let rows = classic_2_presses(|f| f == 10 || (50..=55).contains(&f), 70);
    assert_eq!(frames_with(&rows, Event::RollStart), [10]);
    // A press on 57 starts a roll there.
    let rows = classic_2_presses(|f| f == 10 || f == 57, 70);
    assert_eq!(frames_with(&rows, Event::RollStart), [10, 57]);

    // Held from 10 on, the roll carries on into running on 41: `vx`
    // keeps rising toward the slow roll's 1.125 on 41, and from 42
    // follows the ground rules toward 0 (spec/behavior/roll-speed.md,
    // "When the roll ends (`title-2`)").
    let rows = classic_2_presses(|f| f >= 10, 45);
    assert_eq!(frames_with(&rows, Event::RollEnd), [41]);
    assert_eq!(vx_grains(&rows[40..43]), [280, 281, 246]);
    // A press again on D + 2 starts the next roll.
    let rows = classic_2_presses(|f| (10..=41).contains(&f) || f == 43, 45);
    assert_eq!(frames_with(&rows, Event::RollStart), [10, 43]);
}

#[test]
fn classic_2_how_rolls_end() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/roll-input.md, "How long a roll lasts (`title-2`)":
    // Action let go of during the roll, even if pressed again, ends it in
    // a stop.
    let rows = classic_2_rolls(50, |f| match f {
        0..32 if f != 10 => vec![A, R],
        _ => vec![R],
    });
    assert_eq!(frames_with(&rows, Event::RollEnd), [31]);
    assert_eq!(rows[31].vx, 0.0);
    assert!(rows[32..46].iter().all(|r| r.vx == 0.0));
    assert!(rows[46].vx > 0.0);

    // A fast roll ends in a stop even with Action held all through it.
    let mut w = classic_2_clear();
    hold(&mut w, &[A, R], 66);
    hold(&mut w, &[R], 1);
    let mut rows = play(&mut w, 1, |_| vec![A, R]);
    assert_eq!(classic_2_roll_size(&w), RollSize::Fast);
    rows.extend(play(&mut w, 49, |_| vec![A, R]));
    assert_eq!(frames_with(&rows, Event::RollEnd), [31]);
    assert!(rows[32..46].iter().all(|r| r.vx == 0.0));

    // Facing doesn't change during a stop; a turn toward Left held then
    // starts on D + 15, facing changing 4 frames later. Facing still
    // reverses on D, as a roll frame.
    let rows = classic_2_rolls(60, |f| if f == 0 { vec![A, R] } else { vec![L] });
    assert_eq!(rows[1].facing, Facing::Left);
    let rows = classic_2_rolls(60, |f| match f {
        0 => vec![A, R],
        1..32 => vec![R],
        _ => vec![L],
    });
    assert_eq!(frames_with(&rows, Event::Turn), [50]);
    assert!(rows[32..46].iter().all(|r| r.facing == Facing::Right));
    // After a roll that carries on, the turn starts on D + 1
    // (spec/behavior/ground-speed.md, "Turns (`title-2`)").
    let rows = classic_2_rolls(45, |f| if f < 32 { vec![A, R] } else { vec![A, L] });
    assert_eq!(frames_with(&rows, Event::Turn), [36]);
}

#[test]
fn classic_2_rolls_on_the_ground() {
    use Action::{Action as A, Down, Jump as J, Right as R};
    // spec/behavior/roll-input.md, "On the ground (`title-2`)": a press
    // with Down held crouches instead, and one with Jump pressed too
    // jumps.
    let rows = classic_2_rolls(10, |f| if f == 3 { vec![A, Down] } else { vec![] });
    assert_eq!(frames_with(&rows, Event::RollStart), NONE);
    let rows = classic_2_rolls(10, |f| if f >= 3 { vec![A, J] } else { vec![] });
    assert_eq!(frames_with(&rows, Event::RollStart), NONE);
    assert_eq!(frames_with(&rows, Event::JumpStart), [5]);
    // A press on G + 1 or T of a jump doesn't roll either, and Action
    // counts as held from then on, for the target on T + 1
    // (spec/behavior/air-control.md, "Before take-off (`title-2`)").
    for p in [4, 5] {
        let rows = classic_2_rolls(12, |f| match f {
            3 => vec![J, R],
            _ if f >= p => vec![J, A, R],
            _ => vec![J, R],
        });
        assert_eq!(frames_with(&rows, Event::RollStart), NONE, "{p}");
        assert_eq!(rows[6].target, 3.375, "{p}");
    }
    // Whatever the speed and direction: from running left too.
    let mut w = classic_2_after(&[Action::Left, A], -3.375);
    hold(&mut w, &[Action::Left], 1);
    let rows = play(&mut w, 2, |_| vec![A]);
    assert_eq!(frames_with(&rows, Event::RollStart), [0]);
    assert_eq!(rows[0].target, -1.125);
}

#[test]
fn classic_2_rolls_after_landing() {
    // spec/behavior/roll-input.md, "In the air (`title-2`)": with landing
    // frame L, a press on L - 10 to L held through L + 1 rolls on L + 1.
    let rolls = |held: &dyn Fn(usize) -> bool| {
        let mut w = falling_in(classic_2, LAND + 1);
        w.player.walked_off = false;
        let rows = play(&mut w, LAND + 10, |f| {
            if held(f) {
                vec![Action::Action]
            } else {
                vec![]
            }
        });
        assert_eq!(frames_with(&rows, Event::Land), [LAND]);
        frames_with(&rows, Event::RollStart)
    };
    for p in LAND - 10..=LAND {
        assert_eq!(rolls(&|f| (p..=LAND + 1).contains(&f)), [LAND + 1], "{p}");
        // Let go of before L + 1, nothing happens.
        assert_eq!(rolls(&|f| (p..=LAND).contains(&f)), NONE, "{p}");
    }
    assert_eq!(rolls(&|f| (LAND - 11..=LAND + 1).contains(&f)), NONE);
    // A press on L + 1 or later rolls on that frame.
    assert_eq!(rolls(&|f| f == LAND + 1), [LAND + 1]);
    assert_eq!(rolls(&|f| f == LAND + 4), [LAND + 4]);
    // A jump starting on L + 1 wins.
    let mut w = falling_in(classic_2, LAND + 1);
    w.player.walked_off = false;
    let rows = play(&mut w, LAND + 10, |f| match f {
        _ if f > LAND => vec![Action::Action, Action::Jump],
        _ if f >= LAND - 3 => vec![Action::Action],
        _ => vec![],
    });
    assert_eq!(frames_with(&rows, Event::RollStart), NONE);
    assert_eq!(frames_with(&rows, Event::JumpStart), [LAND + 3]);
}

#[test]
fn classic_2_jumping_out_of_a_roll() {
    use Action::{Action as A, Jump as J, Right as R};
    // spec/behavior/roll-speed.md, "Jumping out of a roll (`title-2`)":
    // a normal roll from standing, Jump pressed on S + 10. G, G + 1 and T
    // follow the roll's rule, and the air rules from T + 1.
    let rows = classic_2_rolls(16, |f| match f {
        0 => vec![A, R],
        10.. => vec![J, R],
        _ => vec![R],
    });
    assert_eq!(frames_with(&rows, Event::JumpStart), [12]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [12]);
    assert_eq!(rows[10].vx * 256.0, 661.0);
    assert_eq!(rows[12].vx * 256.0, 708.0);
    assert_eq!(rows[13].vx * 256.0, 692.0);
    assert!(!rows[13].grounded);

    // A press on D of a roll that would stop: no stop.
    let rows = classic_2_rolls(40, |f| match f {
        0 => vec![A, R],
        31.. => vec![J, R],
        _ => vec![R],
    });
    assert_eq!(frames_with(&rows, Event::JumpStart), [33]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [33]);
    assert!(rows[31..34].iter().all(|r| r.target == 3.375 && r.vx > 3.3));
    assert!(rows[34].vx < rows[33].vx);

    // A press during the stop: `vx` stays 0 on G, G + 1 and T, then the
    // air rules apply.
    let rows = classic_2_rolls(45, |f| match f {
        0 => vec![A, R],
        40.. => vec![J, R],
        _ => vec![R],
    });
    assert_eq!(frames_with(&rows, Event::JumpStart), [42]);
    assert!(rows[31..43].iter().all(|r| r.vx == 0.0));
    assert_eq!(rows[43].vx, 0.28125);
    // On the stop's last frame too, holding the stop through T.
    let rows = classic_2_rolls(50, |f| match f {
        0 => vec![A, R],
        45.. => vec![J, R],
        _ => vec![R],
    });
    assert_eq!(frames_with(&rows, Event::JumpStart), [47]);
    assert!(rows[31..48].iter().all(|r| r.vx == 0.0));
}

fn classic_2_ledge() -> Play {
    let level = Arc::new(Level::from_toml_str(LEDGE_64, "ledge").unwrap());
    Play::on(level, generic_feel("classic-2").unwrap())
}

/// Classic 2 on `LEDGE_64` walking right at +2.25 from x = 1300, then
/// holding `held(f)` on frame S + f: the world at the start of S, and
/// S, chosen so that the roll started by Action on S leaves the ground
/// on S + `lead`.
fn classic_2_before_roll_off(lead: usize, held: &impl Fn(usize) -> Vec<Action>) -> (Play, usize) {
    let new = || {
        let mut w = classic_2_ledge();
        w.player.pos.x = Fixed::from_int(1300);
        w
    };
    let walk = |f: usize, s: usize| {
        if f < s {
            vec![Action::Right]
        } else {
            held(f - s)
        }
    };
    for s in 1..200 {
        let rows = play(&mut new(), s + lead + 1, |f| walk(f, s));
        if frames_with(&rows, Event::LeaveGround).first() == Some(&(s + lead)) {
            assert_eq!(rows[s - 1].vx, 2.25, "walking at +2.25 before S");
            assert_eq!(frames_with(&rows, Event::RollStart)[0], s);
            let mut w = new();
            play(&mut w, s, |f| walk(f, s));
            return (w, s);
        }
    }
    panic!("no roll leaves the ground on S + {lead}");
}

/// The rows of [`classic_2_before_roll_off`] from S to S + `n` - 1.
fn classic_2_roll_off(lead: usize, n: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    let (mut w, _) = classic_2_before_roll_off(lead, &held);
    play(&mut w, n, held)
}

/// Classic 2 on `LEDGE_64` standing still facing right, the nearest
/// whole pixel to the edge from which a roll started by Action and
/// Right on frame 0, with Right held after it, leaves the ground on D =
/// 31. Holds `held(f)` on frame f; the rows and how far from the edge
/// it stood.
fn classic_2_stop_off(n: usize, held: impl Fn(usize) -> Vec<Action>) -> (Vec<Row>, i32) {
    for from in (60..120).rev() {
        let level = Arc::new(Level::from_toml_str(LEDGE_64, "ledge").unwrap());
        let mut w = Play::on(level, generic_feel("classic-2").unwrap());
        w.player.pos.x = Fixed::from_int(1600 - from);
        let rows = play(&mut w, n, &held);
        if frames_with(&rows, Event::LeaveGround).first() == Some(&31) {
            return (rows, from);
        }
    }
    panic!("no roll leaves the ground on D");
}

/// Right held, with `extra` on frames in `on`.
fn right_and(extra: &[Action], on: impl Fn(usize) -> bool) -> impl Fn(usize) -> Vec<Action> {
    let extra = extra.to_vec();
    move |f| {
        let mut held = vec![Action::Right];
        if on(f) {
            held.extend(&extra);
        }
        held
    }
}

#[test]
fn classic_2_roll_hovers_off_a_ledge() {
    // spec/behavior/rolling-off-ledges.md, "Examples (`title-2`)",
    // Hover: Action pressed on S, 3 frames before the edge. Heights below
    // the ledge and `vy` after each frame.
    let rows = classic_2_roll_off(3, 80, right_and(&[Action::Action], |f| f == 0));
    let below = below_ledge(&rows);
    let check = |k: usize, height: f64, vy: f64| {
        assert_eq!((below[k], rows[k].vy), (height, vy), "S + {k}");
        assert!(!rows[k].grounded, "S + {k}");
    };
    check(3, 3.4375, 0.00390625);
    check(4, 3.87890625, 0.44140625);
    for k in 5..=12 {
        check(k, 3.87890625, 0.0);
    }
    check(13, 4.31640625, 0.4375);
    check(28, 63.37890625, 7.0);
    assert_eq!(frames_with(&rows, Event::Land), [29]);
    assert_eq!(below[29], 64.0);
    // In the air at the end of S + 28, so D = S + 59: the roll goes on
    // along the floor to S + 58, and the stop is S + 59 to S + 73.
    assert!(rows[..59].iter().all(|r| r.state == State::Roll));
    assert_eq!(frames_with(&rows, Event::RollEnd), [59]);
    assert!(rows[30..59].iter().all(|r| r.grounded && r.vx > 3.0));
    assert!(rows[59..74].iter().all(|r| r.vx == 0.0));
    assert!(rows[74].vx > 0.0);
}

#[test]
fn classic_2_long_roll_off_a_ledge() {
    // spec/behavior/rolling-off-ledges.md, "Examples (`title-2`)", Long
    // roll: Action pressed 24 frames before the edge doesn't hover, lands
    // on S + 41, rolls on along the floor to S + 58, and stops on S + 59.
    let rows = classic_2_roll_off(24, 75, right_and(&[Action::Action], |f| f == 0));
    assert_eq!(rows[25].vy, 0.44140625);
    assert_eq!(rows[26].vy, 0.87890625);
    assert_eq!(frames_with(&rows, Event::Land), [41]);
    assert!(rows[..59].iter().all(|r| r.state == State::Roll));
    assert_eq!(frames_with(&rows, Event::RollEnd), [59]);
    assert!(rows[59..74].iter().all(|r| r.vx == 0.0));
}

#[test]
fn classic_2_roll_stops_in_the_air() {
    // spec/behavior/rolling-off-ledges.md, "Examples (`title-2`)", Stop in
    // the air: standing about 84 px from the edge, Right held, Action
    // tapped on S = 0. D = 31 is also L.
    let (rows, from) = classic_2_stop_off(60, right_and(&[Action::Action], |f| f == 0));
    assert!((80..=88).contains(&from), "{from} px from the edge");
    assert_eq!(frames_with(&rows, Event::RollEnd), [31]);
    assert_eq!(rows[31].vx, 0.0);
    // D + 1 to D + 14: straight down, `vy` rising by 0.4375 from
    // 0.44140625 to 6.12890625, 49.4296875 px below the ledge.
    let below = below_ledge(&rows);
    for n in 1..=14 {
        let r = &rows[31 + n];
        assert_eq!((r.vx, r.x), (0.0, rows[31].x), "D + {n}");
        assert_eq!(r.vy, 0.00390625 + 0.4375 * n as f64, "D + {n}");
    }
    assert_eq!(rows[45].vy, 6.12890625);
    assert_eq!(below[45], 49.4296875);
    // D + 15: `vy` 1.4375, and the air rules move the player right again
    // until it lands on D + 20.
    assert_eq!(rows[46].vy, 1.4375);
    assert_eq!(rows[46].vx, 0.28125);
    assert_eq!(rows[47].vy, 1.875);
    assert_eq!(frames_with(&rows, Event::Land), [51]);
    assert!(rows[46..=51].iter().all(|r| r.vx > 0.0));
}

#[test]
fn classic_2_roll_steers_in_the_air() {
    // spec/behavior/rolling-off-ledges.md, "The roll in the air
    // (`title-2`)": the roll's speed rule and steering on every frame, L
    // included, with no air targets. The other way reverses the roll and
    // facing at once.
    use Action::{Action as A, Left, Right};
    let rows = classic_2_roll_off(3, 12, |f| match f {
        0 => vec![A, Right],
        1 | 2 | 7.. => vec![Right],
        _ => vec![Left],
    });
    assert!(rows[2].grounded && rows[3].events.contains(&Event::LeaveGround));
    let steer = |r: &Row| (r.facing, r.target);
    assert!(
        rows[3..7]
            .iter()
            .all(|r| steer(r) == (Facing::Left, -3.375))
    );
    assert!(
        rows[7..12]
            .iter()
            .all(|r| steer(r) == (Facing::Right, 3.375))
    );
    assert_eq!(frames_with(&rows, Event::Turn), [3, 7]);
    // Action, Down and Up change nothing.
    let plain = classic_2_roll_off(10, 40, |f| match f {
        0 => vec![A, Right],
        _ => vec![Right],
    });
    let busy = classic_2_roll_off(10, 40, |f| match f {
        0 => vec![A, Right],
        f if f % 3 == 0 => vec![A, Right, Action::Down],
        f if f % 3 == 1 => vec![Right, Action::Up],
        _ => vec![Right],
    });
    let path = |rows: &[Row]| rows.iter().map(|r| (r.x, r.y)).collect::<Vec<_>>();
    assert_eq!(path(&busy[..40]), path(&plain[..40]));
}

#[test]
fn classic_2_roll_lengthened_again_in_a_long_fall() {
    // spec/behavior/rolling-off-ledges.md, "How long the roll lasts
    // (`title-2`)": still in the air at the end of S + 56, the roll is
    // lengthened again, to D = S + 87. Moved up 300 px partway through
    // the fall.
    use Action::{Action as A, Right};
    let held = |f: usize| if f == 0 { vec![A, Right] } else { vec![Right] };
    let (mut w, _) = classic_2_before_roll_off(3, &held);
    let mut rows = play(&mut w, 20, held);
    w.player.pos.y -= Fixed::from_int(300);
    rows.extend(play(&mut w, 90, |f| held(f + 20)));
    let land = frames_with(&rows, Event::Land)[0];
    assert!((57..87).contains(&land), "lands on S + {land}");
    assert!(rows[..87].iter().all(|r| r.state == State::Roll));
    assert_eq!(frames_with(&rows, Event::RollEnd), [87]);

    // Landing on S + 28 or earlier leaves D = S + 31: a roll leaving on
    // S + 11 doesn't hover and lands on S + 28.
    let rows = classic_2_roll_off(11, 40, held);
    assert_eq!(frames_with(&rows, Event::Land), [28]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [31]);
}

#[test]
fn classic_2_roll_carries_on_in_the_air() {
    // spec/behavior/rolling-off-ledges.md, "A roll that ends in the air
    // (`title-2`)": with Action held from S to D, D is a roll frame and
    // `vy` becomes 1.4375 on D + 1, from where the air rules apply.
    use Action::{Action as A, Right};
    let (rows, _) = classic_2_stop_off(40, |_| vec![A, Right]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [31]);
    assert_eq!(rows[31].target, 3.375);
    assert!(rows[31].vx > 3.0);
    assert_eq!(rows[31].vy, 0.00390625);
    assert_eq!(rows[32].vy, 1.4375);
    assert_eq!(rows[33].vy, 1.875);
    assert_eq!(rows[32].state, State::Fall);
}

/// The frames a jump starts on (`jump_start`, its take-off frame T).
fn takeoffs(rows: &[Row]) -> Vec<usize> {
    frames_with(rows, Event::JumpStart)
}

#[test]
fn classic_2_jumps_on_roll_frames_in_the_air() {
    // spec/behavior/rolling-off-ledges.md, "Jumping (`title-2`)": a Jump
    // press on a roll frame in the air starts a jump there, under
    // 0.3515625 until T even with Jump tapped, and the roll's target
    // until T.
    use Action::{Action as A, Jump as J, Right};
    let rows = classic_2_roll_off(24, 40, |f| match f {
        0 => vec![A, Right],
        30 => vec![J, Right],
        _ => vec![Right],
    });
    assert_eq!(takeoffs(&rows), [32]);
    assert_eq!(frames_with(&rows, Event::RollEnd), [32]);
    for k in 30..=31 {
        assert_eq!(rows[k].vy, rows[k - 1].vy + 0.3515625, "S + {k}");
    }
    assert!(rows[30..=32].iter().all(|r| r.target == 3.375));
    assert_eq!(rows[32].y - rows[31].y, rows[31].vy + 0.3515625);
    assert_eq!(rows[32].vy, -8.21875);
    assert_eq!(rows[33].vy, -8.21875 + 0.4375);
    assert_eq!(rows[33].target, 2.25);

    // On D = L itself: the move on L is 3 + 0.3515625, and the roll's
    // target goes on after D until T, with no stop.
    let (rows, _) = classic_2_stop_off(40, |f| match f {
        0 => vec![A, Right],
        31 => vec![J, Right],
        _ => vec![Right],
    });
    assert_eq!(below_ledge(&rows)[31], 3.3515625);
    assert_eq!(takeoffs(&rows), [33]);
    assert!(
        rows[31..=33]
            .iter()
            .all(|r| r.target == 3.375 && r.vx > 3.0)
    );
}

#[test]
fn classic_2_jumps_while_hovering() {
    // spec/behavior/rolling-off-ledges.md, "Jumping (`title-2`)": during
    // a hover the player stays still on G to T, T included.
    use Action::{Action as A, Jump as J, Right};
    let rows = classic_2_roll_off(3, 14, |f| match f {
        0 => vec![A, Right],
        7 => vec![J, Right],
        _ => vec![Right],
    });
    assert_eq!(takeoffs(&rows), [9]);
    for k in 7..=9 {
        assert_eq!(below_ledge(&rows)[k], 3.87890625, "S + {k}");
    }
    assert_eq!(rows[9].vy, -8.21875);
    assert!(below_ledge(&rows)[10] < 0.0);
}

#[test]
fn classic_2_jumps_after_a_roll_ends_in_the_air() {
    // spec/behavior/rolling-off-ledges.md, "Jumping (`title-2`)": after a
    // stop in the air, Jump starts a jump on D + 1 to D + 15 but not
    // later; after a carry-on, on D + 1 only. During the stop `vx` stays
    // 0 until T, and the gravity is the lighter one only with Jump held.
    use Action::{Action as A, Jump as J, Right};
    let stop = |jump: &dyn Fn(usize) -> bool| {
        classic_2_stop_off(52, |f| {
            let mut held = vec![Right];
            if f == 0 {
                held.push(A);
            }
            if jump(f) {
                held.push(J);
            }
            held
        })
        .0
    };
    for g in 32..=46 {
        assert_eq!(takeoffs(&stop(&|f| f == g)), [g + 2], "D + {}", g - 31);
    }
    assert_eq!(takeoffs(&stop(&|f| f == 47)), NONE);
    assert_eq!(takeoffs(&stop(&|f| f >= 47)), NONE);

    let tapped = stop(&|f| f == 34);
    let held = stop(&|f| f >= 34);
    // Jump is held on G either way.
    assert_eq!(tapped[34].vy, tapped[33].vy + 0.3515625);
    assert_eq!(tapped[35].vy, tapped[34].vy + 0.4375);
    assert_eq!(held[35].vy, held[34].vy + 0.3515625);
    assert!(tapped[31..=36].iter().all(|r| r.vx == 0.0));
    assert_eq!(tapped[36].vy, -8.21875);
    assert!(tapped[37].vx > 0.0);

    let carry = |g: usize| {
        let (rows, _) = classic_2_stop_off(45, |f| {
            if f == g {
                vec![A, J, Right]
            } else {
                vec![A, Right]
            }
        });
        takeoffs(&rows)
    };
    assert_eq!(carry(32), [34]);
    assert_eq!(carry(33), NONE);
}

#[test]
fn classic_2_rolls_after_landing_from_a_stop_in_the_air() {
    // spec/behavior/rolling-off-ledges.md, "Action in the fall
    // (`title-2`)": after a stop in the air (landing on A = D + 20), a
    // press on A - 9 to A + 1 held through A + 2 rolls on A + 2.
    use Action::{Action as A, Right};
    let a = 51;
    let rolls = |action: &dyn Fn(usize) -> bool| {
        let (rows, _) = classic_2_stop_off(a + 6, |f| {
            if f == 0 || action(f) {
                vec![A, Right]
            } else {
                vec![Right]
            }
        });
        assert_eq!(frames_with(&rows, Event::Land), [a]);
        frames_with(&rows, Event::RollStart)
            .into_iter()
            .filter(|&f| f > 0)
            .collect::<Vec<_>>()
    };
    for p in a - 9..=a + 1 {
        assert_eq!(
            rolls(&|f| (p..=a + 2).contains(&f)),
            [a + 2],
            "A - {}",
            a as i64 - p as i64
        );
    }
    assert_eq!(rolls(&|f| (a - 10..=a + 2).contains(&f)), NONE);
    assert_eq!(rolls(&|f| (a..=a + 1).contains(&f)), NONE);
    assert_eq!(rolls(&|f| f == a + 1), NONE);
    assert_eq!(rolls(&|f| f == a + 2), [a + 2]);
}

/// Classic 1 standing at `x` on ground like `RAISED_176`'s raised
/// stretch, but going on across the whole level.
fn classic_unended(x: i32) -> Play {
    let toml = RAISED_176.replace("rect = [800, 400, 800, 176]", "rect = [0, 400, 4096, 176]");
    let level = Arc::new(Level::from_toml_str(&toml, "unended").unwrap());
    let mut w = Play::on(level, generic_feel("classic-1").unwrap());
    w.player.pos.x = Fixed::from_int(x);
    w
}

#[test]
fn classic_leaves_where_the_ground_ends() {
    // spec/behavior/walk-off.md, "Ledges without a face": up to the end
    // the player moves as on flat ground, and is grounded exactly while
    // their column has ground. L is the first frame ending past the end,
    // and `vx` and `x` on L follow the speed rule as before it.
    type Inputs = fn(usize) -> Vec<Action>;
    let walk: Inputs = |_| vec![Action::Right];
    let run: Inputs = |_| vec![Action::Action, Action::Right];
    let run_then_walk: Inputs = |f| match f {
        0..60 => vec![Action::Action, Action::Right],
        _ => vec![Action::Right],
    };
    let slide: Inputs = |f| match f {
        0..20 => vec![Action::Right],
        _ => vec![],
    };
    let taps: Inputs = |f| match f % 6 {
        0 => vec![Action::Right],
        _ => vec![],
    };
    let mirror = |held: Vec<Action>| -> Vec<Action> {
        held.into_iter()
            .map(|a| if a == Action::Right { Action::Left } else { a })
            .collect()
    };
    let cases: [(i32, Inputs); 7] = [
        (1300, walk),
        (1592, walk),
        (1599, walk),
        (1300, run),
        (1300, run_then_walk),
        (1590, slide),
        (1596, taps),
    ];
    for (start, held) in cases {
        for left in [false, true] {
            // Going left from the same distance before the left end.
            let x = if left { 2400 - start } else { start };
            let inputs = |f| if left { mirror(held(f)) } else { held(f) };
            let rows = play(&mut classic_raised(x), 300, inputs);
            let flat = play(&mut classic_unended(x), 300, inputs);
            let leave = frames_with(&rows, Event::LeaveGround);
            assert_eq!(leave.len(), 1, "{start} {left}");
            let l = leave[0];
            for f in 0..=l {
                let on_ground = if left {
                    rows[f].x >= 800.0
                } else {
                    rows[f].x < RAISED_END
                };
                let what = format!("{start} {left} L - {}", l - f);
                assert_eq!(rows[f].grounded, f < l, "{what}");
                assert_eq!(on_ground, f < l, "{what}");
                assert_eq!((rows[f].x, rows[f].vx), (flat[f].x, flat[f].vx), "{what}");
                let y = if f < l { 400.0 } else { 403.4375 };
                assert_eq!(
                    (rows[f].y, rows[f].vy),
                    (y, if f < l { 0.0 } else { 0.00390625 })
                );
            }
        }
    }
}

#[test]
fn classic_jumps_at_a_ledge_without_a_face() {
    // spec/behavior/walk-off.md, "Jumping at a ledge without a face": the
    // example walking right, Jump pressed on the frame given and held for
    // 20 frames. Height below the ledge from L - 1 to L + 3, and the
    // frame of `jump_start`, counted from L.
    let walked = play(&mut classic_raised(1300), 300, |_| vec![Action::Right]);
    let l = frames_with(&walked, Event::LeaveGround)[0];
    let off = |press: Option<usize>, frames: usize| {
        let rows = play(&mut classic_raised(1300), l + 5, |f| match press {
            Some(p) if (p..p + frames).contains(&f) => vec![Action::Right, Action::Jump],
            _ => vec![Action::Right],
        });
        let below: Vec<f64> = rows[l - 1..l + 4].iter().map(|r| r.y - 400.0).collect();
        let jump = frames_with(&rows, Event::JumpStart);
        let leave = frames_with(&rows, Event::LeaveGround);
        (
            rows,
            below,
            jump.first().map(|&t| t as i64 - l as i64),
            leave,
        )
    };
    for (press, below, takeoff) in [
        (
            Some(l - 2),
            [0.0, -6.71875, -13.15625, -19.3125, -25.1875],
            Some(-1),
        ),
        (
            Some(l - 1),
            [0.0, 3.28125, -3.4375, -9.875, -16.03125],
            Some(0),
        ),
        (
            Some(l),
            [0.0, 3.28125, 3.56640625, -3.15234375, -9.58984375],
            Some(1),
        ),
        (None, [0.0, 3.4375, 3.87890625, 4.7578125, 6.07421875], None),
    ] {
        let (rows, got, t, leave) = off(press, 20);
        assert_eq!(
            (got.as_slice(), t),
            (below.as_slice(), takeoff),
            "{press:?}"
        );
        if let Some(t) = t {
            let t = (l as i64 + t) as usize;
            assert_eq!(rows[t].vy, -7.0);
            assert_eq!(rows[t + 1].vy, -6.71875);
        }
        // Taking off on L, the row carries `jump_start` and no
        // `leave_ground`, and the player isn't on the ground.
        assert_eq!(leave.is_empty(), t.is_some_and(|t| t <= 0), "{press:?}");
        assert!(!rows[l].grounded);
    }
    // Presses after L start nothing in the air.
    for p in l + 1..l + 4 {
        let (_, got, t, _) = off(Some(p), 20);
        assert_eq!(t, None, "pressed on L + {}", p - l);
        assert_eq!(got[1..3], [3.4375, 3.87890625]);
    }
    // Held for 1 frame only, g is 0.4375 from P + 1 on.
    let (_, got, _, _) = off(Some(l - 1), 1);
    assert_eq!(got[1], 3.4375);
    let (_, got, _, _) = off(Some(l), 1);
    assert_eq!((got[1], got[2] - got[1]), (3.28125, 0.44140625));
}

#[test]
fn classic_down_around_a_landing() {
    // spec/behavior/air-control.md, "Action and Down around take-off and
    // landing": walking at +2, Jump pressed on frame 60 and held (T = 61,
    // L = 110), Down held from frame 62 to 115. The crouch starts on
    // L + 2.
    let (r, d) = (Action::Right, Action::Down);
    let down_to = |last: usize| {
        let mut w = classic_walking();
        play(&mut w, 70, move |f| {
            let frame = f + 60;
            let mut keys = vec![r, Action::Jump];
            if (62..=last).contains(&frame) {
                keys.push(d);
            }
            keys
        })
    };
    let rows = down_to(115);
    assert_eq!(frames_with(&rows, Event::JumpStart), [1]);
    assert_eq!(frames_with(&rows, Event::Land), [50]);
    let vx: Vec<f64> = rows[50..56].iter().map(|row| row.vx).collect();
    assert_eq!(vx, [2.0, 2.0, 2.0, 1.75, 1.53125, 1.33984375]);
    // Down released on 111: no crouch.
    let rows = down_to(111);
    assert!(rows[50..].iter().all(|row| row.vx == 2.0));
    // Down in the air changes nothing.
    let mut w = classic_walking();
    let plain = play(&mut w, 51, |_| vec![r, Action::Jump]);
    let rows = down_to(115);
    for (n, (a, b)) in plain.iter().zip(&rows).enumerate() {
        assert_eq!((a.x, a.y, a.vx, a.vy), (b.x, b.y, b.vx, b.vy), "row {n}");
        assert_eq!(a.facing, b.facing, "row {n}");
    }
}

#[test]
fn classic_action_on_the_take_off_frame() {
    // spec/behavior/air-control.md, "Jump distances": standing, Right and
    // Action held from T (an Action press on T) start no roll; the speed
    // rule starts on T.
    let (r, a) = (Action::Right, Action::Action);
    let rows = classic_air_jump(classic_clear(), &[], &[r, a], &[r, a]);
    assert_eq!(rows[1].vx, 0.375);
    assert_eq!(rows.last().unwrap().x - rows[1].x, 128.37109375);
    assert!(rows.iter().all(|row| row.state != State::Roll));
    assert!(frames_with(&rows, Event::RollStart).is_empty());
    // Action pressed on P together with Jump, walking: an ordinary jump,
    // with Down or without.
    for on_p in [&[r, a][..], &[r, a, Action::Down]] {
        let rows = classic_air_jump(classic_walking(), on_p, &[r], &[r]);
        assert!(frames_with(&rows, Event::RollStart).is_empty(), "{on_p:?}");
        assert!(
            rows.iter()
                .all(|row| !matches!(row.state, State::Roll | State::GroundAction)),
            "{on_p:?}"
        );
    }
}

/// Classic 1 walking at +2, then `held` on P, P + 1, ... with Right held
/// throughout, then Right alone.
fn classic_crouch_jump(held: &[&[Action]], n: usize) -> (Play, Vec<Row>) {
    let mut w = classic_walking();
    let held: Vec<Vec<Action>> = held.iter().map(|keys| keys.to_vec()).collect();
    let rows = play(&mut w, n, |f| {
        let mut keys = held.get(f).cloned().unwrap_or_default();
        keys.push(Action::Right);
        keys
    });
    (w, rows)
}

#[test]
fn classic_1_jump_press_in_a_crouch_turns_on_its_first_frame() {
    // spec/behavior/ground-speed.md, "Turns during a crouch": walking
    // right, Down held on frames 0 to 12 (U is 13); Left and Jump pressed
    // together on 13, Left held to 20. `jump_start` and `turn` are both on
    // 14. A press on any frame 0 to 19 with Left held from it to 20 does
    // the same; Left held on the press frame only doesn't turn.
    use Action::{Down, Jump, Left, Right};
    let run = |press: usize, left: std::ops::RangeInclusive<usize>| {
        let mut w = classic_walking();
        let rows = play(&mut w, 40, |f| {
            let mut keys = vec![Right];
            if f <= 12 {
                keys.push(Down);
            }
            if f == press || f == press + 1 {
                keys.push(Jump);
            }
            if left.contains(&f) {
                keys.retain(|k| *k != Right);
                keys.push(Left);
            }
            keys
        });
        (w, rows)
    };
    let (_, rows) = run(13, 13..=20);
    assert_eq!(frames_with(&rows, Event::JumpStart), [14]);
    assert_eq!(frames_with(&rows, Event::Turn).first(), Some(&14));
    for press in 0..20 {
        let (_, rows) = run(press, press..=20);
        assert_eq!(
            frames_with(&rows, Event::Turn).first(),
            Some(&(press + 1)),
            "{press}"
        );
    }
    let (_, rows) = run(13, 13..=13);
    assert!(frames_with(&rows, Event::Turn).is_empty());
}

#[test]
fn classic_jumping_from_a_crouch() {
    // spec/behavior/jump-input.md, "Crouching": the example. Jump, Action
    // and Down pressed on frame 30 (row 0), Action and Down released
    // after frame 31, Jump held on.
    let (j, a, d) = (Action::Jump, Action::Action, Action::Down);
    let (_, rows) = classic_crouch_jump(&[&[j, a, d], &[j, a, d], &[j], &[j], &[j]], 5);
    let vx: Vec<i64> = rows[..4]
        .iter()
        .map(|row| (row.vx * 256.0) as i64)
        .collect();
    assert_eq!(vx, [544, 572, 501, 502]);
    assert_eq!(frames_with(&rows, Event::JumpStart), [3]);
    assert!(rows[3].grounded);
    assert!(!rows[4].grounded);

    // The table, P on row 0.
    let jump_start = |held: &[&[Action]]| {
        let (w, rows) = classic_crouch_jump(held, 30);
        (frames_with(&rows, Event::JumpStart), w)
    };
    assert_eq!(jump_start(&[&[j, a], &[j, d], &[j]]).0, [3]);
    for held in [&[&[j, a][..], &[j, d]][..], &[&[j, a], &[d]]] {
        let (starts, _) = jump_start(held);
        assert!(starts.is_empty(), "{held:?}");
        // The crouch lasts at least to P + 13.
        let (mut w, _) = classic_crouch_jump(held, 14);
        assert!(w.player.crouch.held > 0, "{held:?}");
        hold(&mut w, &[Action::Right], 1);
        assert_eq!(w.player.crouch.held, 0, "{held:?}");
    }
    assert_eq!(jump_start(&[&[j, a, d], &[j]]).0, [1]);
    assert_eq!(jump_start(&[&[j, d], &[j, a, d], &[j]]).0, [1]);

    // A press on any frame of a crouch jumps on P + 1, Down held or not;
    // so does one on U to U + 9.
    let (down, press_down, press_jump) = ([d], [j, d], [j]);
    for press in [1, 5, 12, 13, 20, 22] {
        let mut held: Vec<&[Action]> = vec![&down; 3];
        held.resize(press, &[]);
        held.push(&press_down);
        held.push(&press_jump);
        let (starts, _) = jump_start(&held);
        assert_eq!(starts, [press + 1], "press on {press}");
    }
    // A press on the first frame Down is held starts no crouch.
    let (_, rows) = classic_crouch_jump(&[&[j, d], &[]], 2);
    assert_eq!(frames_with(&rows, Event::JumpStart), [1]);
}

#[test]
fn classic_a_stopped_roll_carried_on_stands_and_pushes_off() {
    // spec/behavior/ground-speed.md, "After a roll": a first-character
    // roll that had stopped (1590, 10 px from the end of the floor) and
    // carried on (D = 29) stands on D + 3 with nothing held. Right first
    // held on a frame from D to D + 3 pushes off at 2 on that frame and
    // walks on D + 3; from D + 4 walking starts as from standing.
    let run = |change: usize| {
        let mut w = classic_raised(1590);
        play(&mut w, 40, |f| {
            let mut keys = vec![];
            if f < 32 {
                keys.push(Action::Action);
            }
            if f >= change {
                keys.push(Action::Right);
            }
            keys
        })
    };
    let rows = run(40);
    assert_eq!(frames_with(&rows, Event::RollEnd), [32]);
    assert!(rows[28..].iter().all(|r| (r.x, r.vx) == (1590.0, 0.0)));
    assert_eq!(rows[32].state, State::Idle);
    for change in 29..=32 {
        let rows = run(change);
        assert_eq!((rows[change - 1].x, rows[change - 1].vx), (1590.0, 0.0));
        assert_eq!((rows[change].x, rows[change].vx), (1592.0, 2.0), "{change}");
        assert_eq!(rows[32].state, State::Walk, "{change}");
    }
    for change in [33, 34] {
        let rows = run(change);
        assert_eq!(rows[change - 1].vx, 0.0, "{change}");
        assert_eq!(rows[change].state, State::Walk, "{change}");
        assert_eq!(rows[change].vx, 0.25, "{change}");
    }
}

/// Frames (counted from the turn's first, 0) of the Turn and JumpStart
/// events, and the first frame the state is a roll, with Left held on
/// `left` and `button` pressed (and held) from `press`.
fn classic_turn_press(left: std::ops::Range<usize>, button: Action, press: usize) -> Tpress {
    let mut w = classic();
    let mut steps = Vec::new();
    for f in 0..16 {
        let mut held = Vec::new();
        if left.contains(&f) {
            held.push(Action::Left);
        }
        if f >= press {
            held.push(button);
        }
        steps.extend(hold(&mut w, &held, 1));
    }
    Tpress {
        turn: first(&steps, Event::Turn),
        jump: first(&steps, Event::JumpStart),
        roll: steps.iter().position(|s| s.state == State::Roll),
    }
}

#[derive(Debug, PartialEq)]
struct Tpress {
    turn: Option<usize>,
    jump: Option<usize>,
    roll: Option<usize>,
}

#[test]
fn classic_a_press_during_a_turn_cancels_its_facing_change() {
    // spec/behavior/ground-speed.md, "A Jump or Action press during a turn":
    // the worked rows (standing, facing right).
    let rows: [(std::ops::Range<usize>, usize, Option<usize>, usize); 7] = [
        (0..1, 2, None, 3),
        (0..1, 5, Some(4), 6),
        (0..3, 1, Some(2), 2),
        (0..3, 2, None, 3),
        (0..9, 2, Some(3), 3),
        (0..9, 4, Some(5), 5),
        (0..1, 6, Some(4), 7),
    ];
    for (left, press, turn, jump) in rows {
        let got = classic_turn_press(left.clone(), Action::Jump, press);
        assert_eq!(
            (got.turn, got.jump),
            (turn, Some(jump)),
            "{left:?} Jump {press}"
        );
    }
    // Action starts a roll on its press, facing as in the column.
    for (left, press, turn) in [(0..3, 2, None), (0..9, 3, Some(4))] {
        let got = classic_turn_press(left.clone(), Action::Action, press);
        assert_eq!(
            (got.turn, got.roll),
            (turn, Some(press)),
            "{left:?} Action {press}"
        );
    }
}

fn classic_roll_stop_jump(press: usize, right: bool) -> Vec<Row> {
    let mut w = classic();
    play(&mut w, 70, |f| {
        let mut keys = Vec::new();
        if f == 2 {
            keys.push(Action::Action);
        }
        if f < 29 || (right && f < 54) {
            keys.push(Action::Right);
        }
        if (press..54).contains(&f) {
            keys.push(Action::Jump);
        }
        keys
    })
}

#[test]
fn classic_jump_press_on_d_plus_3_of_a_roll_stop_leaves_the_speed() {
    // spec/behavior/roll-input.md, "A Jump press during the stop": the
    // roll is frames 2 to 30, D = 31.
    let d = 31;
    let rows = classic_roll_stop_jump(d + 3, true);
    assert_eq!(frames_with(&rows, Event::RollEnd), [d]);
    let vx: Vec<f64> = rows[d..d + 5].iter().map(|r| r.vx * 256.0).collect();
    assert_eq!(vx, [753.0, 659.0, 577.0, 505.0, 512.0]);
    // A press on D + 4 gives 0 there.
    let rows = classic_roll_stop_jump(d + 4, true);
    assert_eq!(rows[d + 3].vx, 0.0);
    assert_eq!(rows[d + 4].vx, 0.0);
    // With no direction held from 29, the air rule slows toward 0.
    let rows = classic_roll_stop_jump(d + 3, false);
    let vx: Vec<f64> = rows[d + 3..d + 7].iter().map(|r| r.vx * 256.0).collect();
    assert_eq!(vx, [505.0, 442.0, 387.0, 339.0]);
}

#[test]
fn classic_roll_pressed_on_a_landing_frame_goes_the_way_held() {
    use Action::{Action as A, Left as L, Right as R};
    // spec/behavior/roll-speed.md, "Size and direction on the press
    // frame": on L to K facing follows the input, so Action with the
    // opposite direction held rolls that way; from K + 1 the roll goes the
    // way the player faces, and reverses on the next frame.
    for jump in [12, 40] {
        let (_, _, gaits) = classic_after_landing(classic, jump, &[R], |_| vec![R]);
        let k = gaits
            .iter()
            .position(|g| !matches!(g, Gait::Landing(_)))
            .unwrap();
        for press in 1..=k + 3 {
            let (_, rows, _) = classic_after_landing(classic, jump, &[R], |n| {
                if n == press { vec![A, L] } else { vec![R] }
            });
            assert!(rows[press].events.contains(&Event::RollStart));
            let expected = if press <= k {
                Facing::Left
            } else {
                Facing::Right
            };
            assert_eq!(
                rows[press].facing, expected,
                "jump {jump}, press L + {press}, K = L + {k}"
            );
            assert_eq!(rows[press].events.contains(&Event::Turn), press <= k);
        }
    }
}

#[test]
fn classic_jump_release_is_logged_only_while_rising() {
    // spec/behavior/jump-arc.md, "The `jump_release` event": the row's
    // `vy` after gravity decides. Held 25 frames: -0.09375 and the event;
    // held 26: 0.1875 and none.
    for (held, event) in [(25, true), (26, false)] {
        let mut w = classic();
        let rows = play(&mut w, held + 2, |f| {
            if f < held { vec![Action::Jump] } else { vec![] }
        });
        assert_eq!(
            frames_with(&rows, Event::JumpRelease),
            if event { vec![held] } else { vec![] },
            "held {held}, vy {}",
            rows[held].vy
        );
    }
}
