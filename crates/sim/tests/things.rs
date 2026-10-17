//! Enemies and interactive objects played through whole worlds.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    Air, Event, Facing, Fixed, FlowEvent, Gait, InputFrame, LAUNCH_REST, Outcome, Phase, Roll,
    RollSize, START_LIVES, State, World, generic_feel, step,
};

/// Ground at y = 400 from x = 0 to 600, with a wall from x = 100 to 120
/// standing on it, and the player at `start`. `entities` is appended.
fn world(start: [i32; 2], entities: &str) -> World {
    world_on("default", start, entities)
}

/// [`world`] on the given feel preset.
fn world_on(preset: &str, start: [i32; 2], entities: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "arena"
        size = [1000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 600, 200]

        [[collision]]
        shape = "rect"
        rect = [100, 300, 20, 100]

        {entities}
        "#,
        start[0], start[1]
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(Arc::new(level), Arc::new(generic_feel(preset).unwrap()))
}

/// What happened while holding `held` for `frames` steps.
#[derive(Default)]
struct Seen {
    events: Vec<Event>,
    flow: Vec<FlowEvent>,
}

fn hold(world: &mut World, held: Actions, frames: usize) -> Seen {
    let mut seen = Seen::default();
    let mut previous = Actions::NONE;
    for _ in 0..frames {
        step(world, &InputFrame::single(Frame::after(previous, held)));
        previous = held;
        seen.events.extend(&world.events);
        seen.flow.extend(&world.flow.events);
    }
    seen
}

#[test]
fn walkers_turn_at_ledges_and_walls() {
    // The player waits on top of the wall, out of the way.
    let mut w = world(
        [110, 300],
        r#"
        [[entities]]
        type = "crawler"
        at = [560, 400]
        params = { facing = "right" }
        "#,
    );
    let mut xs = Vec::new();
    for _ in 0..1600 {
        hold(&mut w, Actions::NONE, 1);
        xs.push(w.things[0].pos.x);
        assert_eq!(w.things[0].pos.y, Fixed::from_int(400));
    }
    let max = *xs.iter().max().unwrap();
    let min = *xs.iter().min().unwrap();
    // It turns with its front edge at the ledge, and against the wall.
    assert_eq!(max, Fixed::from_int(590));
    assert_eq!(min, Fixed::from_int(130));
    assert!(w.things[0].active);
}

#[test]
fn touching_an_enemy_costs_a_life() {
    let mut w = world(
        [200, 400],
        r#"
        [[entities]]
        type = "crawler"
        at = [260, 400]
        "#,
    );
    let seen = hold(&mut w, Action::Right.into(), 60);
    assert!(seen.events.contains(&Event::Hurt));
    assert_eq!(seen.flow.first(), Some(&FlowEvent::Die));
    assert_eq!(w.flow.lives, START_LIVES - 1);
    assert_eq!(w.player.state, State::Hurt);
    assert!(matches!(w.flow.outcome, Outcome::Dying(_)));
    assert_eq!(w.things[0].phase, Phase::Ready);
}

#[test]
fn landing_on_an_enemy_defeats_it_and_bounces() {
    let mut w = world(
        [300, 300],
        r#"
        [[entities]]
        type = "crawler"
        at = [300, 400]
        params = { speed = 0 }
        "#,
    );
    let mut bounced = None;
    for _ in 0..60 {
        let seen = hold(&mut w, Actions::NONE, 1);
        if seen.events.contains(&Event::Bounce) {
            assert_eq!(seen.flow, [FlowEvent::Defeat(0)]);
            bounced = Some(w.player.vel.y);
            break;
        }
    }
    assert_eq!(bounced, Some(-Fixed::from_int(6)));
    assert_eq!(w.player.state, State::Bounce);
    assert_eq!(
        w.things[0].phase,
        Phase::Falling {
            vy: -Fixed::from_int(5),
            hold: 0
        }
    );

    // It jumps, then falls through the floor and is gone once it's below
    // the level; the player is bouncing until it lands.
    hold(&mut w, Actions::NONE, 19);
    assert!(w.things[0].active);
    assert!(w.things[0].pos.y < Fixed::from_int(400));
    assert_eq!(w.player.state, State::Bounce);
    let seen = hold(&mut w, Actions::NONE, 60);
    assert!(!w.things[0].active);
    assert!(seen.events.contains(&Event::Land));
    assert!(!seen.events.contains(&Event::Hurt));
    assert_eq!(w.flow.lives, START_LIVES);
}

#[test]
fn rolling_into_an_enemy_defeats_it() {
    let mut w = world(
        [140, 400],
        r#"
        [[entities]]
        type = "crawler"
        at = [380, 400]
        params = { speed = 0 }
        "#,
    );
    hold(
        &mut w,
        Actions::from(Action::Right) | Action::Action.into(),
        40,
    );
    let seen = hold(
        &mut w,
        Actions::from(Action::Down) | Action::Action.into(),
        120,
    );
    assert!(seen.events.contains(&Event::RollStart));
    assert!(seen.flow.contains(&FlowEvent::Defeat(0)), "{:?}", w.player);
    assert!(!seen.events.contains(&Event::Hurt));
    assert!(!seen.events.contains(&Event::Bounce));
    assert_eq!(w.flow.lives, START_LIVES);
}

#[test]
fn patrols_fly_their_path_and_back() {
    let mut w = world(
        [300, 400],
        r#"
        [[entities]]
        type = "flutter"
        at = [500, 200]
        params = { path = [-60, -40], period = 120 }
        "#,
    );
    hold(&mut w, Actions::NONE, 30);
    assert_eq!(w.things[0].pos, Vec2::ints(470, 180));
    assert_eq!(w.things[0].facing, Facing::Left);
    hold(&mut w, Actions::NONE, 30);
    assert_eq!(w.things[0].pos, Vec2::ints(440, 160));
    hold(&mut w, Actions::NONE, 1);
    assert_eq!(w.things[0].facing, Facing::Right);
    hold(&mut w, Actions::NONE, 59);
    assert_eq!(w.things[0].pos, Vec2::ints(500, 200));
}

#[test]
fn launchers_fire_the_player_then_rest() {
    let mut w = world(
        [200, 400],
        r#"
        [[entities]]
        type = "launcher"
        at = [260, 392]
        "#,
    );
    let mut fired = false;
    for _ in 0..60 {
        let seen = hold(&mut w, Action::Right.into(), 1);
        if seen.flow.contains(&FlowEvent::Launch(0)) {
            fired = true;
            break;
        }
    }
    assert!(fired);
    assert_eq!(w.player.vel.y, -Fixed::from_ratio_pow2(27, 1));
    assert!(!w.player.grounded());
    assert_eq!(w.things[0].phase, Phase::Fired(0));

    let seen = hold(&mut w, Actions::NONE, 1);
    assert!(w.player.vel.y.is_negative());
    assert_eq!(w.player.state, State::Fall);
    assert!(seen.flow.is_empty());
    hold(&mut w, Actions::NONE, LAUNCH_REST as usize - 1);
    assert_eq!(w.things[0].phase, Phase::Ready);
    assert!(w.player.pos.y < Fixed::from_int(250));
}

#[test]
fn entity_params_are_checked_against_their_archetype() {
    let src = r#"
        format = 1
        size = [100, 100]
        start = [10, 10]

        [[entities]]
        type = "crawler"
        at = [50, 50]
        params = { velocity = [1, 2] }
    "#;
    let level = Level::from_toml_str(src, "test").unwrap();
    let feel = Arc::new(generic_feel("default").unwrap());
    let err = World::try_new(Arc::new(level), feel).unwrap_err();
    assert!(err.message.contains("entities[0].params.velocity"), "{err}");
}

// Classic 1: bounces off walkers (spec/behavior/bounce.md,
// walker-contact.md) and how walkers move (walker.md).

/// The values walkers in walker.md have, besides their speed and facing.
const CLASSIC_WALKER: &str = "response = 0.125, turn_delay = 4, \
    defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8";

/// A walker as in walker-contact.md's small walker, with the sizes the
/// classic-1 bounce box goes with.
fn small_walker(x: &str, speed: &str, facing: &str) -> String {
    format!(
        r#"
        [[entities]]
        type = "crawler"
        at = [{x}, 400]
        params = {{ speed = {speed}, facing = "{facing}", size = [20, 12], offset = 0.5, {CLASSIC_WALKER} }}
        "#
    )
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Steps `world` holding jump on the frames `jump` says, by frame count
/// from 0, and returns the player's `(y, vy)` and events after each.
fn play(
    world: &mut World,
    frames: usize,
    jump: impl Fn(usize) -> bool,
) -> Vec<(Fixed, Fixed, Vec<Event>)> {
    let mut previous = Actions::NONE;
    (0..frames)
        .map(|n| {
            let held = if jump(n) {
                Action::Jump.into()
            } else {
                Actions::NONE
            };
            step(world, &InputFrame::single(Frame::after(previous, held)));
            previous = held;
            (world.player.pos.y, world.player.vel.y, world.events.clone())
        })
        .collect()
}

/// A classic-1 player falling from 100 px up onto a still walker under
/// it: the world and the frame it bounces on.
fn classic_bounce() -> (World, usize) {
    let mut w = world_on("classic-1", [300, 300], &small_walker("300", "0", "left"));
    let steps = play(&mut w, 60, |_| false);
    let b = steps
        .iter()
        .position(|s| s.2.contains(&Event::Bounce))
        .expect("bounces");
    (
        world_on("classic-1", [300, 300], &small_walker("300", "0", "left")),
        b,
    )
}

#[test]
fn classic_bounce_sets_vy_then_holds_still_for_two_frames() {
    let (mut w, b) = classic_bounce();
    // Direction input in the frozen frames is ignored.
    let mut previous = Actions::NONE;
    let mut rows = Vec::new();
    for n in 0..=b + 3 {
        let held = if n > b {
            Action::Right.into()
        } else {
            Actions::NONE
        };
        step(&mut w, &InputFrame::single(Frame::after(previous, held)));
        previous = held;
        rows.push((w.player.pos, w.player.vel, w.player.state));
    }
    let (at_b, vel_b, state_b) = rows[b];
    assert_eq!(vel_b.y, fx(-7.125));
    assert_eq!(state_b, State::Bounce);
    assert_eq!(rows[b + 1], (at_b, vel_b, State::Bounce));
    assert_eq!(rows[b + 2], (at_b, vel_b, State::Bounce));
    // B + 3 moves again, under the heavier gravity: jump isn't held.
    let (at, vel, _) = rows[b + 3];
    assert_eq!(vel.y, fx(-7.125 + 0.4375));
    assert_eq!(at.y, at_b.y + vel.y);
    assert!(vel.x > Fixed::ZERO);
    // So does the walker it bounced off.
    assert_eq!(
        w.things[0].phase,
        Phase::Falling {
            vy: fx(-6.0 + 0.4375),
            hold: 0
        }
    );
}

#[test]
fn classic_bounce_height_by_how_long_jump_is_held_from_b_plus_3() {
    let (_, b) = classic_bounce();
    let table = [
        (0, 54.5),
        (1, 57.0),
        (2, 59.34375),
        (3, 61.6875),
        (4, 63.875),
        (5, 65.9375),
        (8, 71.625),
        (10, 74.90625),
        (15, 81.375),
        (20, 85.3125),
        (25, 86.71875),
        (40, 86.71875),
    ];
    for (k, height) in table {
        let (mut w, _) = classic_bounce();
        let steps = play(&mut w, b + 80, |n| n >= b + 3 && n < b + 3 + k);
        let y_b = steps[b].0;
        let landed = steps
            .iter()
            .position(|s| s.2.contains(&Event::Land))
            .unwrap();
        let top = steps[b..landed].iter().map(|s| s.0).min().unwrap();
        assert_eq!(y_b - top, fx(height), "K = {k}");
    }
}

#[test]
fn classic_bounce_hold_counts_only_from_b_plus_3() {
    let (_, b) = classic_bounce();
    let height = |jump: &dyn Fn(usize) -> bool| {
        let (mut w, _) = classic_bounce();
        let steps = play(&mut w, b + 40, jump);
        let top = steps[b..].iter().map(|s| s.0).min().unwrap();
        steps[b].0 - top
    };
    // Held through the bounce but let go on B + 3: no help.
    assert_eq!(height(&|n| n < b + 3), fx(54.5));
    // Held from before the bounce on: as a hold from B + 3.
    assert_eq!(height(&|_| true), fx(86.71875));
    // Pressed on B + 4 or later: never the lighter gravity.
    assert_eq!(height(&|n| n >= b + 4), fx(54.5));
    // A one-frame break gives the heavier gravity for the rest.
    assert_eq!(height(&|n| n >= b + 3 && n != b + 4), fx(57.0));
}

#[test]
fn classic_bounce_arc_follows_the_formula() {
    let (mut w, b) = classic_bounce();
    let steps = play(&mut w, b + 40, |_| false);
    let y_b = steps[b].0;
    let g = 0.4375;
    for n in 1..=15 {
        let (y, vy, _) = &steps[b + 2 + n];
        let n = n as f64;
        assert_eq!(*vy, fx(-7.125 + g * n), "n = {n}");
        assert_eq!(
            *y - y_b,
            fx(-7.125 * n + g * n * (n + 1.0) / 2.0),
            "n = {n}"
        );
    }
}

/// One frame after a bounce: `vx` (1/256), facing, gait, and whether a
/// turn is under way.
type BounceRow = (i64, Facing, Gait, bool);

/// A [`classic_bounce`] with nothing held up to B + 2, `air` from B + 3
/// and `on_g` from the landing frame G on, and `vx` set to `before_g` (if
/// given) at the end of G - 1. The rows from G - 1 to G + 20.
fn classic_bounce_landing(air: Actions, on_g: Actions, before_g: Option<Fixed>) -> Vec<BounceRow> {
    let (mut probe, b) = classic_bounce();
    let input = |n: usize, g: usize| match n {
        _ if n >= g => on_g,
        _ if n >= b + 3 => air,
        _ => Actions::NONE,
    };
    let mut previous = Actions::NONE;
    let mut g = usize::MAX;
    for n in 0.. {
        let held = input(n, g);
        step(
            &mut probe,
            &InputFrame::single(Frame::after(previous, held)),
        );
        previous = held;
        if n > b && probe.events.contains(&Event::Land) {
            g = n;
            break;
        }
    }
    let (mut w, _) = classic_bounce();
    let mut previous = Actions::NONE;
    let mut rows: Vec<BounceRow> = Vec::new();
    for n in 0..=g + 20 {
        if n == g
            && let Some(vx) = before_g
        {
            w.player.vel.x = vx;
            rows.last_mut().unwrap().0 = grains(vx);
        }
        let held = input(n, g);
        step(&mut w, &InputFrame::single(Frame::after(previous, held)));
        previous = held;
        assert_eq!(w.events.contains(&Event::Land), n == g, "frame {n}");
        let p = &w.player;
        rows.push((grains(p.vel.x), p.facing, p.gait, p.turn.is_some()));
        if n > b && n < g {
            assert_eq!(p.facing, Facing::Right, "B + {}", n - b);
        }
    }
    rows.split_off(g - 1)
}

/// 1/256 px per frame.
fn grains(v: Fixed) -> i64 {
    v.to_bits() >> 24
}

#[test]
fn classic_after_a_bounce_a_mode_starts_on_landing() {
    use Action::{Action as A, Left as L, Right as R};
    let keys = |a: &[Action]| a.iter().copied().collect::<Actions>();
    let walking = |g: Gait| matches!(g, Gait::Walking(s) if s.step == 1);
    let running = |g: Gait| matches!(g, Gait::Running(s) if s.step == 1);
    // spec/behavior/ground-speed.md, "After a fall or a bounce", the
    // example: Right and Action held in the air, then Left and Action
    // from G. Rows are from G - 1.
    let rows = classic_bounce_landing(keys(&[A, R]), keys(&[A, L]), Some(Fixed::from_int(3)));
    assert_eq!((rows[0].0, rows[0].1), (768, Facing::Right));
    let vx: Vec<_> = [1, 2, 3, 4, 8, 12, 16].iter().map(|&i| rows[i].0).collect();
    assert_eq!(vx, [576, 408, 261, 133, -238, -456, -584]);
    assert!(running(rows[1].2) && !rows[1].3, "running starts on G");
    assert!(rows[2..4].iter().all(|r| r.1 == Facing::Right && !r.3));
    assert!(rows[4].3, "the turn starts on G + 3");
    assert_eq!((rows[7].1, rows[8].1), (Facing::Right, Facing::Left));
    assert!(!rows[12].3 && walking(rows[12].2), "walking on G + 11");
    assert!(matches!(rows[15].2, Gait::Walking(s) if s.step == 2));
    assert!(running(rows[16].2), "running on G + 15");

    // Facing doesn't change from the bounce to G. On G: the other way
    // held starts a turn, forward starts walking, nothing standing, and
    // Action with a direction running.
    for air in [&[][..], &[L], &[A, L], &[R]] {
        let rows = classic_bounce_landing(keys(air), keys(&[L]), None);
        assert_eq!(
            (rows[1].1, rows[1].2, rows[1].3),
            (Facing::Right, Gait::Standing, true)
        );
        assert_eq!((rows[4].1, rows[5].1), (Facing::Right, Facing::Left));
        let rows = classic_bounce_landing(keys(air), keys(&[R]), None);
        assert!(walking(rows[1].2) && !rows[1].3, "{air:?}");
        let rows = classic_bounce_landing(keys(air), Actions::NONE, None);
        assert_eq!((rows[1].2, rows[1].3), (Gait::Standing, false));
    }
}

/// The frames of a [`classic_bounce`] that the bounce lands on and that
/// a jump starts on, if one does by 20 frames after the landing, with
/// only Jump held, on the frames `jump` gives counted from the landing
/// `l` of the same bounce with nothing held.
fn classic_jump_after_bounce(jump: impl Fn(i64) -> bool) -> (i64, Option<i64>) {
    let (mut probe, b) = classic_bounce();
    let steps = play(&mut probe, b + 120, |_| false);
    let l = (b + 1..steps.len())
        .find(|&n| steps[n].2.contains(&Event::Land))
        .expect("lands") as i64;
    let (mut w, _) = classic_bounce();
    let steps = play(&mut w, l as usize + 21, |n| jump(n as i64 - l));
    let at = |e: Event| {
        (b + 1..steps.len())
            .find(|&n| steps[n].2.contains(&e))
            .map(|n| n as i64 - l)
    };
    (at(Event::Land).expect("lands"), at(Event::JumpStart))
}

#[test]
fn classic_a_jump_can_start_two_frames_after_a_bounce_lands() {
    // spec/behavior/jump-input.md, "After a bounce": M = L, so E = L + 2,
    // and the press window counts back from E.
    for p in -10..=1 {
        // Pressed L - 10 to L + 1, held through L + 1: jumps on L + 2.
        assert_eq!(
            classic_jump_after_bounce(|n| n >= p),
            (0, Some(2)),
            "held from L + {p}"
        );
        assert_eq!(
            classic_jump_after_bounce(|n| (p..=1).contains(&n)),
            (0, Some(2)),
            "pressed on L + {p}, let go on L + 2"
        );
    }
    for p in [-14, -12, -11] {
        // Pressed L - 11 or earlier, held through landing: no jump.
        assert_eq!(
            classic_jump_after_bounce(|n| n >= p),
            (0, None),
            "held from L + {p}"
        );
    }
    for p in [-10, -5, -1] {
        // Pressed in the air, let go on L or earlier: no jump.
        for end in p + 1..=0 {
            assert_eq!(
                classic_jump_after_bounce(|n| (p..end).contains(&n)),
                (0, None),
                "pressed on L + {p}, let go on L + {end}"
            );
        }
    }
    for p in 2..=4 {
        // Pressed on L + 2 or later: jumps on the next frame.
        assert_eq!(
            classic_jump_after_bounce(|n| n >= p),
            (0, Some(p + 1)),
            "pressed on L + {p}"
        );
    }
}

#[test]
fn classic_jump_held_since_the_bounce_does_not_jump_on_landing() {
    // spec/behavior/jump-input.md, "After a bounce": held without a break
    // since B + 3, no jump. The held bounce lands later.
    let (_, b) = classic_bounce();
    let (mut w, _) = classic_bounce();
    let steps = play(&mut w, b + 140, |n| n >= b + 3);
    let l = (b + 1..steps.len())
        .find(|&n| steps[n].2.contains(&Event::Land))
        .expect("lands");
    assert!(l + 20 < steps.len());
    assert!(steps[b..].iter().all(|s| !s.2.contains(&Event::JumpStart)));
}

/// Whether a classic-1 player facing `facing`, falling straight down
/// `i` px from a still walker made by `walker` heading `heading`, bounces
/// off it.
fn classic_bounces_at(
    walker: fn(&str, &str, &str) -> String,
    i: i32,
    facing: Facing,
    heading: &str,
) -> bool {
    let mut w = world_on("classic-1", [300 + i, 340], &walker("300", "0", heading));
    w.player.facing = facing;
    let steps = play(&mut w, 40, |_| false);
    steps.iter().any(|s| s.2.contains(&Event::Bounce))
}

#[test]
fn classic_bounce_ranges_for_the_small_and_medium_walkers() {
    type Walker = fn(&str, &str, &str) -> String;
    let table: [(Walker, Facing, &str, i32, i32); 8] = [
        (small_walker, Facing::Right, "left", -21, 34),
        (small_walker, Facing::Right, "right", -20, 35),
        (small_walker, Facing::Left, "left", -35, 20),
        (small_walker, Facing::Left, "right", -34, 21),
        (medium_walker, Facing::Right, "left", -28, 32),
        (medium_walker, Facing::Right, "right", -18, 42),
        (medium_walker, Facing::Left, "left", -42, 18),
        (medium_walker, Facing::Left, "right", -32, 28),
    ];
    for (walker, facing, heading, low, high) in table {
        for (i, bounces) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                classic_bounces_at(walker, i, facing, heading),
                bounces,
                "I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

/// Whether a classic-1 player falling slowly onto a still walker made
/// by `walker`, `u` px above its floor at the end of the frame, bounces
/// on that frame.
fn classic_bounces_from(walker: fn(&str, &str, &str) -> String, u: i32) -> bool {
    let mut w = world_on("classic-1", [300, 0], &walker("300", "0", "left"));
    // After this frame's gravity it falls 1/16 px, to `u` above the floor.
    w.player.pos.y = Fixed::from_int(400 - u) - fx(0.0625);
    w.player.vel.y = fx(0.0625 - 0.4375);
    let steps = play(&mut w, 1, |_| false);
    steps[0].2.contains(&Event::Bounce)
}

#[test]
fn classic_bounce_height_limits_by_walker() {
    // walker-contact.md: U at most 24 (small), 37 (medium), 50 (large).
    type Walker = fn(&str, &str, &str) -> String;
    let table: [(Walker, i32); 3] = [(small_walker, 24), (medium_walker, 37), (large_walker, 50)];
    for (n, (walker, top)) in table.into_iter().enumerate() {
        assert!(classic_bounces_from(walker, top), "row {n}");
        assert!(!classic_bounces_from(walker, top + 1), "row {n}");
    }
}

#[test]
fn classic_bounce_needs_the_player_not_to_be_rising_fast() {
    // vy after this frame's gravity: -1.1875 bounces, -1.25 doesn't.
    for (vy, bounces) in [(-1.1875, true), (-1.25, false)] {
        let mut w = world_on("classic-1", [300, 380], &small_walker("300", "0", "left"));
        w.player.vel.y = fx(vy - 0.4375);
        let steps = play(&mut w, 1, |_| false);
        assert_eq!(steps[0].1 == fx(-7.125), bounces, "vy {vy}");
        assert_eq!(steps[0].2.contains(&Event::Bounce), bounces, "vy {vy}");
    }
}

#[test]
fn classic_walker_eases_its_speed() {
    // Heading left into the wall, it stops there, then speeds up to the
    // right: full speed on the 32nd frame (walker.md).
    let mut w = world_on("classic-1", [110, 300], &small_walker("160", "1", "left"));
    let mut speeds = Vec::new();
    for _ in 0..120 {
        hold(&mut w, Actions::NONE, 1);
        speeds.push(w.things[0].vx);
    }
    assert!(speeds[..20].iter().all(|&v| v == -Fixed::ONE));
    let stop = speeds.iter().position(|&v| v == Fixed::ZERO).unwrap();
    let grains: Vec<i64> = speeds[stop + 1..stop + 33]
        .iter()
        .map(|v| v.to_bits() * 256 / Fixed::ONE.to_bits())
        .collect();
    assert_eq!(
        grains,
        [
            32, 60, 84, 105, 123, 139, 153, 165, 176, 186, 194, 201, 207, 213, 218, 222, 226, 229,
            232, 235, 237, 239, 241, 242, 243, 244, 245, 246, 247, 248, 249, 256
        ]
    );
    assert_eq!(w.things[0].facing, Facing::Right);
}

/// Steps `world` with nothing held and returns the first walker's `x`,
/// `vx` and facing after each step.
fn walker_rows(world: &mut World, frames: usize) -> Vec<(Fixed, Fixed, Facing)> {
    (0..frames)
        .map(|_| {
            hold(world, Actions::NONE, 1);
            let t = &world.things[0];
            (t.pos.x, t.vx, t.facing)
        })
        .collect()
}

#[test]
fn classic_walker_turns_softly_at_the_end_of_its_range() {
    // walker.md's soft turn: heading left at full speed, its range from
    // L = 200. Placed at 328.5, it's at 200.5 after 128 steps: the
    // example's frame 1.
    let walker = format!(
        r#"
        [[entities]]
        type = "crawler"
        at = [328.5, 400]
        params = {{ speed = 1, facing = "left", size = [20, 12], offset = 0.5, range = [-128.5, 127.5], {CLASSIC_WALKER} }}
        "#
    );
    let mut w = world_on("classic-1", [120, 300], &walker);
    let rows = walker_rows(&mut w, 127 + 39);
    let frame = |f: usize| rows[126 + f];
    let (l, r) = (Facing::Left, Facing::Right);
    let want = [
        (1, 200.5, -1.0, l),
        (2, 199.5, -1.0, l),
        (3, 198.75, -0.75, l),
        (4, 198.21875, -0.53125, l),
        (5, 197.87890625, -0.33984375, l),
        (6, 197.703125, -0.17578125, l),
        (7, 197.671875, -0.03125, r),
        (8, 197.76953125, 0.09765625, r),
        (9, 197.9765625, 0.20703125, r),
        (38, 221.2734375, 0.97265625, r),
        (39, 222.2734375, 1.0, r),
    ];
    for (f, x, vx, facing) in want {
        assert_eq!(frame(f), (fx(x), fx(vx), facing), "frame {f}");
    }
}

#[test]
fn classic_walker_patrols_its_range_in_laps() {
    // A range 256 px wide, centred on its start: each lap takes 544
    // frames, the same every time (walker.md). That's 256 whole pixels
    // from L to R, both ends included.
    let walker = format!(
        r#"
        [[entities]]
        type = "crawler"
        at = [300, 400]
        params = {{ speed = 1, facing = "left", size = [20, 12], offset = 0.5, range = [-128, 127], {CLASSIC_WALKER} }}
        "#
    );
    let mut w = world_on("classic-1", [110, 300], &walker);
    let rows = walker_rows(&mut w, 2000);
    for n in 600..1400 {
        assert_eq!(rows[n], rows[n + 544], "frame {n}");
    }
    let xs = rows.iter().map(|r| r.0);
    assert!(xs.clone().min().unwrap() < fx(172.0));
    assert!(xs.max().unwrap() > fx(428.0));
    let turns = rows
        .windows(2)
        .filter(|p| p[0].1 >= Fixed::ZERO && p[1].1 < Fixed::ZERO);
    assert!(turns.count() >= 3);
}

#[test]
fn classic_walker_stops_at_a_wall_and_turns_after_five_frames() {
    // walker.md's barrier example, against the arena's wall: the walker
    // can't have x below 130, where its 20-px box touches the wall at
    // 120. Its x is the example's minus 70; placed at 150.5, it's at
    // 130.5 after 20 steps: the example's frame 3.
    let mut w = world_on("classic-1", [110, 300], &small_walker("150.5", "1", "left"));
    let rows = walker_rows(&mut w, 17 + 36);
    let frame = |f: usize| rows[16 + f];
    let (l, r) = (Facing::Left, Facing::Right);
    let want = [
        (3, 200.5, -1.0, l),
        (4, 200.0, 0.0, l),
        (5, 200.125, 0.125, l),
        (6, 200.359375, 0.234375, l),
        (7, 200.6875, 0.328125, l),
        (8, 201.09765625, 0.41015625, l),
        (9, 201.578125, 0.48046875, r),
        (36, 224.75390625, 1.0, r),
    ];
    for (f, x, vx, facing) in want {
        assert_eq!(frame(f), (fx(x - 70.0), fx(vx), facing), "frame {f}");
    }
}

#[test]
fn classic_bounced_walker_jumps_holds_still_and_falls_through_the_floor() {
    // A classic-1 player falls onto a walker walking left at full speed.
    // Find a start for the walker that gives a bounce.
    let walker = |x: i32| small_walker(&x.to_string(), "1", "left");
    let bounce_frame = |x: i32| {
        let mut w = world_on("classic-1", [300, 300], &walker(x));
        play(&mut w, 60, |_| false)
            .iter()
            .position(|s| s.2.contains(&Event::Bounce))
    };
    let (x, b) = (300..360)
        .find_map(|x| bounce_frame(x).map(|b| (x, b)))
        .expect("bounces on some walker");
    let mut w = world_on("classic-1", [300, 300], &walker(x));
    let mut rows = Vec::new();
    for _ in 0..b + 60 {
        hold(&mut w, Actions::NONE, 1);
        let t = &w.things[0];
        let vy = match t.phase {
            Phase::Falling { vy, .. } => vy,
            _ => Fixed::ZERO,
        };
        rows.push((t.pos, vy, t.facing, t.active));
    }
    // walker.md's example, relative to the end of B - 1.
    let at = rows[b - 1].0;
    let want = [
        (0, -1.0, 0.0, -6.0),
        (1, -1.0, 0.0, -6.0),
        (2, -1.0, 0.0, -6.0),
        (3, -2.0, -5.5625, -5.5625),
        (4, -3.0, -10.6875, -5.125),
        (15, -14.0, -38.1875, -0.3125),
        (16, -15.0, -38.0625, 0.125),
        (29, -28.0, 3.375, 5.8125),
    ];
    for (n, dx, dy, vy) in want {
        let (pos, got_vy, facing, active) = rows[b + n];
        assert_eq!(
            (pos.x - at.x, pos.y - at.y, got_vy),
            (fx(dx), fx(dy), fx(vy)),
            "B + {n}"
        );
        assert_eq!(facing, Facing::Left);
        assert!(active);
    }
    // It falls no faster than 8 px a frame, and is gone once below the
    // level, which ends 200 px under the floor.
    let falls = rows
        .windows(2)
        .filter(|p| p[1].3)
        .map(|p| p[1].0.y - p[0].0.y);
    assert!(falls.clone().all(|dy| dy <= fx(8.0)));
    assert!(falls.clone().any(|dy| dy == fx(8.0)));
    let gone = rows.iter().position(|r| !r.3).expect("gone");
    assert!(rows[gone - 1].0.y - fx(12.0) <= fx(600.0));
    assert!(rows[gone].0.y - fx(12.0) > fx(600.0));
    // A defeated walker never comes back, even after the player loses a
    // life.
    hold(&mut w, Actions::NONE, 600);
    assert!(!w.things[0].active);
}

// Classic 1: getting hurt by walkers (walker-contact.md, "Being hurt"),
// knock-out (knock-out.md) and walkers while the player is knocked out
// (walker.md).

/// A walker as in walker-contact.md's large walker, with the sizes the
/// classic-1 bounce and hurt boxes go with.
fn large_walker(x: &str, speed: &str, facing: &str) -> String {
    format!(
        r#"
        [[entities]]
        type = "crawler"
        at = [{x}, 400]
        params = {{ speed = {speed}, facing = "{facing}", size = [26, 38], offset = 0.5, {CLASSIC_WALKER} }}
        "#
    )
}

/// A walker as in walker-contact.md's medium walker, with the size and
/// offset the classic-1 bounce and hurt boxes go with.
fn medium_walker(x: &str, speed: &str, facing: &str) -> String {
    format!(
        r#"
        [[entities]]
        type = "crawler"
        at = [{x}, 400]
        params = {{ speed = {speed}, facing = "{facing}", size = [25, 25], offset = 5, {CLASSIC_WALKER} }}
        "#
    )
}

/// Whether a classic-1 player standing `i` px from a still walker, made
/// by `walker`, is hurt by it on the second frame and not the first.
fn classic_hurt_at(
    walker: fn(&str, &str, &str) -> String,
    i: i32,
    facing: Facing,
    heading: &str,
) -> bool {
    let mut w = world_on("classic-1", [300 + i, 400], &walker("300", "0", heading));
    w.player.facing = facing;
    let steps = play(&mut w, 2, |_| false);
    assert!(!steps[0].2.contains(&Event::Hurt));
    steps[1].2.contains(&Event::Hurt)
}

#[test]
fn classic_hurt_ranges_by_walker_facing_and_heading() {
    type Walker = fn(&str, &str, &str) -> String;
    let table: [(Walker, Facing, &str, i32, i32); 12] = [
        (small_walker, Facing::Right, "left", -19, 24),
        (small_walker, Facing::Right, "right", -18, 25),
        (small_walker, Facing::Left, "left", -25, 18),
        (small_walker, Facing::Left, "right", -24, 19),
        (medium_walker, Facing::Right, "left", -26, 22),
        (medium_walker, Facing::Right, "right", -16, 32),
        (medium_walker, Facing::Left, "left", -32, 16),
        (medium_walker, Facing::Left, "right", -22, 26),
        (large_walker, Facing::Right, "left", -22, 27),
        (large_walker, Facing::Right, "right", -21, 28),
        (large_walker, Facing::Left, "left", -28, 21),
        (large_walker, Facing::Left, "right", -27, 22),
    ];
    for (n, (walker, facing, heading, low, high)) in table.into_iter().enumerate() {
        for (i, hurt) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                classic_hurt_at(walker, i, facing, heading),
                hurt,
                "row {n}: I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

/// Whether a classic-1 player rising fast (too fast to bounce) right
/// over a still walker, `u` px above its floor and `age` frames into a
/// jump at the end of the frame, is about to be hurt.
fn classic_hurt_in_the_air(walker: fn(&str, &str, &str) -> String, u: i32, age: u8) -> bool {
    let mut w = world_on("classic-1", [300, 0], &walker("300", "0", "left"));
    // After this frame's gravity it rises 2 px, to `u` above the floor.
    w.player.pos.y = Fixed::from_int(402 - u);
    w.player.vel.y = fx(-2.0 - 0.4375);
    w.player.jump_age = Some(age - 1);
    w.player.state = State::Jump;
    step(&mut w, &InputFrame::default());
    assert_eq!(w.player.pos.y, Fixed::from_int(400 - u));
    assert_eq!(w.player.jump_age, Some(age));
    w.player.hurt_ahead.is_some()
}

#[test]
fn classic_hurt_reach_early_and_later_in_a_jump() {
    // Up to T + 5: at most 11 px (small) or 37 px (large) above.
    assert!(classic_hurt_in_the_air(small_walker, 11, 5));
    assert!(!classic_hurt_in_the_air(small_walker, 12, 5));
    assert!(classic_hurt_in_the_air(large_walker, 37, 5));
    assert!(!classic_hurt_in_the_air(large_walker, 38, 5));
    // From T + 6: at most 16 px above a large walker, and a small one
    // doesn't hurt at all.
    assert!(classic_hurt_in_the_air(large_walker, 16, 6));
    assert!(!classic_hurt_in_the_air(large_walker, 17, 6));
    for u in [0, 4, 11, 16] {
        assert!(!classic_hurt_in_the_air(small_walker, u, 6), "U = {u}");
    }
}

#[test]
fn classic_medium_walker_hurt_reach_early_and_later_in_a_jump() {
    // Up to T + 5: at most 24 px above; from T + 6, in the airborne
    // pose, at most 3.
    assert!(classic_hurt_in_the_air(medium_walker, 24, 5));
    assert!(!classic_hurt_in_the_air(medium_walker, 25, 5));
    assert!(classic_hurt_in_the_air(medium_walker, 3, 6));
    for u in [4, 16, 24] {
        assert!(!classic_hurt_in_the_air(medium_walker, u, 6), "U = {u}");
    }
}

/// Whether a classic-1 player standing `i` px from a still walker, made
/// by `walker`, in the fidget pose at the end of the frame (its
/// `standing`th frame of standing), is about to be hurt.
fn classic_fidget_hurt_at(
    walker: fn(&str, &str, &str) -> String,
    i: i32,
    facing: Facing,
    heading: &str,
    standing: u32,
) -> bool {
    let mut w = world_on("classic-1", [300 + i, 400], &walker("300", "0", heading));
    w.player.facing = facing;
    w.player.pose.standing = Some(standing - 1);
    step(&mut w, &InputFrame::default());
    assert_eq!(w.player.pose.standing, Some(standing));
    w.player.hurt_ahead.is_some()
}

#[test]
fn classic_fidget_pose_shifts_the_hurt_range() {
    // walker-contact.md: facing right, the low end 10 px higher and the
    // high end 3 px higher; facing left, 3 and 10 px lower.
    type Walker = fn(&str, &str, &str) -> String;
    let table: [(Walker, Facing, &str, i32, i32); 12] = [
        (small_walker, Facing::Right, "left", -9, 27),
        (small_walker, Facing::Right, "right", -8, 28),
        (small_walker, Facing::Left, "left", -28, 8),
        (small_walker, Facing::Left, "right", -27, 9),
        (medium_walker, Facing::Right, "left", -16, 25),
        (medium_walker, Facing::Right, "right", -6, 35),
        (medium_walker, Facing::Left, "left", -35, 6),
        (medium_walker, Facing::Left, "right", -25, 16),
        (large_walker, Facing::Right, "left", -12, 30),
        (large_walker, Facing::Right, "right", -11, 31),
        (large_walker, Facing::Left, "left", -31, 11),
        (large_walker, Facing::Left, "right", -30, 12),
    ];
    for (n, (walker, facing, heading, low, high)) in table.into_iter().enumerate() {
        for (i, hurt) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                classic_fidget_hurt_at(walker, i, facing, heading, 239),
                hurt,
                "row {n}: I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

#[test]
fn classic_fidget_pose_is_on_frames_239_to_418_and_696_to_875() {
    // I = 26 is inside the fidget range only (−9 to 27, not −19 to 24).
    for (standing, fidget) in [
        (238, false),
        (239, true),
        (418, true),
        (419, false),
        (695, false),
        (696, true),
        (875, true),
        (876, false),
    ] {
        assert_eq!(
            classic_fidget_hurt_at(small_walker, 26, Facing::Right, "left", standing),
            fidget,
            "standing frame {standing}"
        );
    }
}

#[test]
fn classic_medium_walker_from_behind_follows_the_worked_example() {
    // A player facing right at x 120.25 and a medium walker heading right
    // at full speed, at 86.5 at the end of frame 1; here 200 px further
    // right, clear of the arena's wall.
    for (fidget, hurt_on) in [(false, 4), (true, 2)] {
        let mut w = world_on(
            "classic-1",
            [320, 400],
            &medium_walker("285.5", "1", "right"),
        );
        w.player.pos.x = fx(320.25);
        w.player.facing = Facing::Right;
        if fidget {
            w.player.pose.standing = Some(300);
        }
        let mut xs = Vec::new();
        let mut hurt = None;
        for frame in 1..=4 {
            step(&mut w, &InputFrame::default());
            xs.push(w.things[0].pos.x);
            if hurt.is_none() && w.events.contains(&Event::Hurt) {
                hurt = Some(frame);
            }
        }
        assert_eq!(
            xs[..3],
            [fx(286.5), fx(287.5), fx(288.5)],
            "fidget {fidget}"
        );
        assert_eq!(hurt, Some(hurt_on), "fidget {fidget}");
    }
}

/// Sets a classic-1 player on the ground up to land on the next step
/// after going into the air by `air`, with Jump held for `held` frames.
fn about_to_land(w: &mut World, air: Air, held: u8) {
    w.player.pose.since_land = None;
    w.player.pose.air = air;
    w.player.pose.jump_held = held;
}

/// Whether a classic-1 player standing `i` px from a still walker, made
/// by `walker`, `n` frames after landing from a jump at the end of the
/// frame, is about to be hurt.
fn classic_landed_hurt_at(
    walker: fn(&str, &str, &str) -> String,
    i: i32,
    facing: Facing,
    heading: &str,
    n: u8,
) -> bool {
    let mut w = world_on("classic-1", [300 + i, 400], &walker("300", "0", heading));
    w.player.facing = facing;
    about_to_land(&mut w, Air::Moving, 1);
    w.player.pose.since_land = n.checked_sub(1);
    step(&mut w, &InputFrame::default());
    assert_eq!(w.player.pose.since_land, Some(n));
    w.player.hurt_ahead.is_some()
}

#[test]
fn classic_airborne_pose_moves_the_end_behind_the_player_in() {
    // walker-contact.md: facing right the high end is 3 px lower, facing
    // left the low end 3 px higher. On L + 1 to L + 3 after a jump the
    // player shows the landing pictures, airborne and not lifted.
    type Walker = fn(&str, &str, &str) -> String;
    let table: [(Walker, Facing, &str, i32, i32); 12] = [
        (small_walker, Facing::Right, "left", -19, 21),
        (small_walker, Facing::Right, "right", -18, 22),
        (small_walker, Facing::Left, "left", -22, 18),
        (small_walker, Facing::Left, "right", -21, 19),
        (medium_walker, Facing::Right, "left", -26, 19),
        (medium_walker, Facing::Right, "right", -16, 29),
        (medium_walker, Facing::Left, "left", -29, 16),
        (medium_walker, Facing::Left, "right", -19, 26),
        (large_walker, Facing::Right, "left", -22, 24),
        (large_walker, Facing::Right, "right", -21, 25),
        (large_walker, Facing::Left, "left", -25, 21),
        (large_walker, Facing::Left, "right", -24, 22),
    ];
    for (n, (walker, facing, heading, low, high)) in table.into_iter().enumerate() {
        for (i, hurt) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            for landed in 1..=3 {
                assert_eq!(
                    classic_landed_hurt_at(walker, i, facing, heading, landed),
                    hurt,
                    "row {n}: I = {i}, facing {facing:?}, heading {heading}, L + {landed}"
                );
            }
        }
        // Upright again from L + 4.
        let back = if facing == Facing::Right {
            high + 1
        } else {
            low - 1
        };
        assert!(
            classic_landed_hurt_at(walker, back, facing, heading, 4),
            "row {n}"
        );
    }
}

/// Whether a classic-1 player rising fast right over a still large walker,
/// `i` px from it and 10 px above its floor, 6 frames into a jump, is
/// about to be hurt.
fn classic_airborne_hurt_at(i: i32, facing: Facing, heading: &str) -> bool {
    let mut w = world_on(
        "classic-1",
        [300 + i, 0],
        &large_walker("300", "0", heading),
    );
    w.player.facing = facing;
    w.player.pos.y = Fixed::from_int(392);
    w.player.vel.y = fx(-2.0 - 0.4375);
    w.player.jump_age = Some(5);
    w.player.state = State::Jump;
    step(&mut w, &InputFrame::default());
    assert_eq!(w.player.jump_age, Some(6));
    w.player.hurt_ahead.is_some()
}

#[test]
fn classic_airborne_pose_ranges_in_the_air() {
    // walker-contact.md's example: facing right, a large walker heading
    // right hurts from I −21 to 25.
    for (facing, heading, low, high) in [
        (Facing::Right, "right", -21, 25),
        (Facing::Right, "left", -22, 24),
        (Facing::Left, "left", -25, 21),
        (Facing::Left, "right", -24, 22),
    ] {
        for (i, hurt) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                classic_airborne_hurt_at(i, facing, heading),
                hurt,
                "I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

#[test]
fn classic_small_walker_misses_the_closing_pictures() {
    // A small walker at I = −10, heading left, to a player facing right
    // that lands on the first step (L). After most jumps L still shows a
    // closing picture and L + 1 a landing one: hurt on L + 2. After a
    // jump with Jump held 8 to 15 frames the landing pictures show from
    // L: hurt on L + 1. After a fall from a ledge with a face, closing
    // pictures show on L to L + 5: hurt on L + 7.
    for (air, held, hurt_on) in [
        (Air::Moving, 1, 2),
        (Air::Still, 40, 2),
        (Air::Moving, 8, 1),
        (Air::Still, 15, 1),
        (Air::Moving, 16, 2),
        (Air::Fall, 0, 7),
    ] {
        let mut w = world_on("classic-1", [290, 400], &small_walker("300", "0", "left"));
        w.player.facing = Facing::Right;
        about_to_land(&mut w, air, held);
        let mut hurt = None;
        for frame in 0..12 {
            step(&mut w, &InputFrame::default());
            if w.events.contains(&Event::Hurt) {
                hurt = Some(frame);
                break;
            }
        }
        assert_eq!(hurt, Some(hurt_on), "{air:?}, Jump held {held}");
    }
}

#[test]
fn classic_fall_from_a_face_keeps_the_airborne_pose_to_l_plus_8() {
    // Upright from L + 9: I = 25 is in the large walker's upright range
    // facing right, heading left (−22 to 27), but not the airborne one.
    for (n, hurt) in [(8, false), (9, true)] {
        let mut w = world_on("classic-1", [325, 400], &large_walker("300", "0", "left"));
        w.player.facing = Facing::Right;
        about_to_land(&mut w, Air::Fall, 0);
        w.player.pose.since_land = Some(n - 1);
        step(&mut w, &InputFrame::default());
        assert_eq!(w.player.hurt_ahead.is_some(), hurt, "L + {n}");
    }
}

#[test]
fn classic_medium_walker_after_a_landing_follows_the_worked_example() {
    // walker-contact.md: a player facing right at x 120.25 just after
    // landing, airborne up to L + 3, and a medium walker heading right
    // at 89.5 at the end of L + 2; here 200 px further right. Not hurt
    // at I = 31 or 30 in the airborne pose, but at 29 upright on L + 4,
    // so hurt on L + 5.
    let mut w = world_on(
        "classic-1",
        [320, 400],
        &medium_walker("288.5", "1", "right"),
    );
    w.player.pos.x = fx(320.25);
    w.player.facing = Facing::Right;
    w.player.pose.since_land = Some(1);
    w.player.pose.air = Air::Moving;
    w.player.pose.jump_held = 1;
    let mut rows = Vec::new();
    for _ in 0..4 {
        step(&mut w, &InputFrame::default());
        rows.push((
            w.player.pose.since_land,
            w.things[0].pos.x,
            w.events.contains(&Event::Hurt),
        ));
    }
    assert_eq!(
        rows,
        [
            (Some(2), fx(289.5), false),
            (Some(3), fx(290.5), false),
            (Some(4), fx(291.5), false),
            (Some(5), fx(292.5), true),
        ]
    );
}

/// Whether a classic-1 player that walked off a ledge, from a steep face
/// if `face`, rising fast right over a still large walker 30 px above
/// its floor, is about to be hurt.
fn classic_walk_off_hurt_at(face: bool) -> bool {
    let mut w = world_on("classic-1", [300, 0], &large_walker("300", "0", "left"));
    w.player.pos.y = Fixed::from_int(372);
    w.player.vel.y = fx(-2.0 - 0.4375);
    w.player.walked_off = true;
    w.player.walked_off_face = face;
    w.player.state = State::Fall;
    step(&mut w, &InputFrame::default());
    assert_eq!(w.player.pos.y, Fixed::from_int(370));
    w.player.hurt_ahead.is_some()
}

#[test]
fn classic_walk_off_without_a_face_keeps_the_upright_reach() {
    // walker-contact.md, "Falls without a jump": off a ledge without a
    // face the upright reach (37 for a large walker) applies in the air;
    // off one with a face the airborne one (16).
    assert!(classic_walk_off_hurt_at(false));
    assert!(!classic_walk_off_hurt_at(true));
}

#[test]
fn classic_jump_age_counts_from_the_take_off_frame() {
    let mut w = world_on("classic-1", [300, 400], "");
    let mut ages = Vec::new();
    let mut takeoff = None;
    for n in 0..60 {
        let held = if n < 3 {
            Action::Jump.into()
        } else {
            Actions::NONE
        };
        let previous = if n == 0 || n >= 4 {
            Actions::NONE
        } else {
            Action::Jump.into()
        };
        step(&mut w, &InputFrame::single(Frame::after(previous, held)));
        if w.events.contains(&Event::JumpStart) {
            takeoff = Some(n);
        }
        ages.push((w.player.jump_age, w.events.contains(&Event::Land)));
    }
    let t = takeoff.unwrap();
    for k in 0..6 {
        assert_eq!(ages[t + k].0, Some(k as u8));
    }
    let land = ages.iter().position(|a| a.1).unwrap();
    assert_eq!(ages[land - 1].0, Some((land - 1 - t) as u8));
    assert_eq!(ages[land].0, None);
}

/// A classic-1 player standing facing right at x = 300, and a small
/// walker heading left at full speed toward it, followed by `more`
/// entities. Returns the world and the hit frame H, counted from 0.
fn classic_walked_into(more: &str) -> (World, usize) {
    let src = small_walker("322.5", "1", "left") + more;
    let mut w = world_on("classic-1", [300, 400], &src);
    let steps = play(&mut w, 10, |_| false);
    let h = steps
        .iter()
        .position(|s| s.2.contains(&Event::Hurt))
        .expect("hurt");
    (world_on("classic-1", [300, 400], &src), h)
}

#[test]
fn classic_hurt_is_decided_a_frame_ahead_and_the_player_stays_put() {
    let (mut w, h) = classic_walked_into("");
    let mut rows = Vec::new();
    for _ in 0..=h {
        step(&mut w, &InputFrame::default());
        rows.push((w.things[0].pos.x, w.player.hurt_ahead, w.events.clone()));
    }
    // I = 300 − 319 = −19 at the end of frame 2 (counting from 0): hurt
    // on frame 3, where the walker still moves.
    assert_eq!(h, 3);
    assert_eq!(rows[1].0, fx(320.5));
    assert_eq!(rows[1].1, None);
    assert_eq!(rows[2].0, fx(319.5));
    assert_eq!(rows[2].1, Some(0));
    assert_eq!(rows[3].0, fx(318.5));
    assert_eq!(rows[3].2, [Event::Hurt]);
    assert_eq!(w.player.pos, Vec2::ints(300, 400));
    assert_eq!(w.player.state, State::Hurt);
    assert_eq!(w.flow.lives, START_LIVES - 1);
}

#[test]
fn classic_hurt_comes_before_a_bounce() {
    let mut w = world_on("classic-1", [300, 0], &small_walker("300", "0", "left"));
    // At the end of the first frame the player is 5 px up, rising at
    // 1.3125: too fast to bounce, and about to be hurt. On the next it
    // would bounce.
    w.player.pos.y = fx(396.3125);
    w.player.vel.y = fx(-1.75);
    w.player.jump_age = Some(0);
    w.player.state = State::Jump;
    let steps = play(&mut w, 2, |_| false);
    assert!(steps[0].2.is_empty(), "{:?}", steps[0].2);
    assert_eq!(steps[1].2, [Event::Hurt]);
    assert_eq!(w.things[0].phase, Phase::Ready);
}

#[test]
fn classic_knock_out_follows_the_worked_values() {
    let (mut w, h) = classic_walked_into("");
    let mut rows = Vec::new();
    for _ in 0..h + 227 {
        step(&mut w, &InputFrame::default());
        rows.push((
            w.player.pos,
            w.player.vel,
            w.player.grounded(),
            w.player.state,
            w.events.clone(),
            w.flow.events.clone(),
            w.pause,
        ));
    }
    let at = rows[h - 1].0;
    let row = |k: usize| {
        let r = &rows[h + k];
        (r.0.x - at.x, r.0.y - at.y, r.1.y)
    };
    for k in 0..=10 {
        assert_eq!(row(k), (Fixed::ZERO, Fixed::ZERO, Fixed::ZERO), "H + {k}");
        assert_eq!(rows[h + k].1.x, Fixed::ZERO, "H + {k}");
        assert!(rows[h + k].2, "H + {k}");
    }
    // The 32-frame pause comes between H + 7 and H + 8.
    for (k, r) in rows.iter().enumerate() {
        assert_eq!(r.6, if k == h + 7 { 32 } else { 0 }, "frame {k}");
    }
    let table = [
        (11, -0.78125, -7.5625, -7.5625),
        (12, -1.5625, -14.6875, -7.125),
        (28, -14.0625, -69.1875, -0.125),
        (45, -27.34375, -7.0, 6.0),
        (46, -28.125, 0.0, -5.0),
        (47, -28.90625, -4.5625, -4.5625),
        (66, -43.75, -8.125, 3.75),
        (67, -44.53125, 0.0, -2.0),
        (71, -47.65625, -3.625, -0.25),
        (72, -48.4375, 0.0, 0.0),
        (225, -48.4375, 0.0, 0.0),
    ];
    for (k, x, y, vy) in table {
        assert_eq!(row(k), (fx(x), fx(y), fx(vy)), "H + {k}");
    }
    // Rebounds are airborne with no event; the third landing stops.
    for k in [46, 67] {
        assert!(!rows[h + k].2 && rows[h + k].4.is_empty(), "H + {k}");
    }
    assert!(rows[h + 72].2);
    assert_eq!(rows[h + 72].4, [Event::Land]);
    assert_eq!(rows[h + 72].1.x, Fixed::ZERO);
    for k in 0..=225 {
        assert_eq!(rows[h + k].3, State::Hurt, "H + {k}");
        assert!(rows[h + k].5.iter().all(|e| *e != FlowEvent::Respawn));
    }
    assert!(rows[h + 73..=h + 225].iter().all(|r| r.4.is_empty()));
    // The life is lost after the last frame, H + 225.
    assert_eq!(rows[h + 226].5, [FlowEvent::Respawn]);
    assert_eq!(w.flow.outcome, Outcome::Playing);
    assert_eq!(w.player.knockout, None);
}

#[test]
fn classic_knock_out_flies_away_from_the_way_the_player_faces() {
    let (mut w, _) = classic_walked_into("");
    w.player.facing = Facing::Left;
    let mut rows = Vec::new();
    for _ in 0..30 {
        step(&mut w, &InputFrame::default());
        rows.push((w.player.pos.x, w.events.contains(&Event::Hurt)));
    }
    let h = rows.iter().position(|r| r.1).unwrap();
    assert_eq!(rows[h + 10].0, rows[h].0);
    assert_eq!(rows[h + 11].0 - rows[h].0, fx(0.78125));
}

#[test]
fn classic_walkers_hold_still_while_the_player_does_after_a_hit() {
    let (mut w, h) = classic_walked_into(&small_walker("500", "1", "right"));
    let mut xs = Vec::new();
    for _ in 0..=h + 12 {
        step(&mut w, &InputFrame::default());
        xs.push((w.things[0].pos.x, w.things[1].pos.x));
    }
    // The walker that hurt the player walks on; the other moves on H,
    // holds still on H + 1 to H + 10 and moves again from H + 11.
    for k in h..=h + 12 {
        assert_eq!(xs[k].0 - xs[k - 1].0, -Fixed::ONE, "frame {k}");
        let moved = !(h + 1..=h + 10).contains(&k);
        assert_eq!(xs[k].1 - xs[k - 1].1 == Fixed::ONE, moved, "frame {k}");
    }
}

// Classic 1: rolling into walkers (rolling-into-walkers.md).

/// Steps `world` for `frames` frames holding what `held` says for each
/// frame, counted from 0, and returns every frame's row.
fn roll_rows(world: &mut World, frames: usize, held: impl Fn(usize) -> Actions) -> Vec<RollRow> {
    let mut previous = Actions::NONE;
    (0..frames)
        .map(|n| {
            let now = held(n);
            step(world, &InputFrame::single(Frame::after(previous, now)));
            previous = now;
            RollRow {
                x: world.player.pos.x,
                vx: world.player.vel.x,
                state: world.player.state,
                events: world.events.clone(),
                defeats: world
                    .flow
                    .events
                    .iter()
                    .filter(|e| matches!(e, FlowEvent::Defeat(_)))
                    .count(),
            }
        })
        .collect()
}

struct RollRow {
    x: Fixed,
    vx: Fixed,
    state: State,
    events: Vec<Event>,
    defeats: usize,
}

/// Whether a classic-1 player rolling at 3 px per frame the way `facing`
/// says, starting at x = 300, defeats a walker made by `walker` heading
/// the way `heading` says at full speed, when I is `i` at the end of the
/// frame.
fn classic_roll_defeats_at(
    walker: fn(&str, &str, &str) -> String,
    i: i32,
    facing: Facing,
    heading: &str,
) -> bool {
    let p = match facing {
        Facing::Left => 297,
        Facing::Right => 303,
    };
    let w = p - i;
    let start = match heading {
        "left" => w + 1,
        _ => w - 1,
    };
    let mut world = world_on(
        "classic-1",
        [300, 400],
        &walker(&format!("{start}.5"), "1", heading),
    );
    let speed = Fixed::from_int(3);
    world.player.facing = facing;
    world.player.state = State::Roll;
    world.player.vel.x = if facing == Facing::Left {
        -speed
    } else {
        speed
    };
    world.player.roll = Some(Roll {
        left: 20,
        dir: facing,
        size: RollSize::Normal,
        speed,
        boosts: 0,
        defeated: false,
        held: false,
        carry: false,
        still: false,
        since_press: 0,
        air: 0,
        frame: 0,
    });
    let rows = roll_rows(&mut world, 1, |_| Actions::NONE);
    assert_eq!(rows[0].x, Fixed::from_int(p));
    assert_eq!(
        world.things[0].pos.x.floor(),
        i64::from(w),
        "the walker moves 1 px"
    );
    assert!(!rows[0].events.contains(&Event::Hurt));
    rows[0].defeats == 1
}

#[test]
fn classic_roll_defeat_ranges_by_walker_facing_and_heading() {
    type Walker = fn(&str, &str, &str) -> String;
    let table: [(Walker, Facing, &str, i32, i32); 6] = [
        (small_walker, Facing::Right, "left", -16, 25),
        (small_walker, Facing::Right, "right", -17, 24),
        (small_walker, Facing::Left, "left", -24, 17),
        (small_walker, Facing::Left, "right", -25, 16),
        (large_walker, Facing::Right, "left", -19, 28),
        (large_walker, Facing::Right, "right", -20, 27),
    ];
    for (n, (walker, facing, heading, low, high)) in table.into_iter().enumerate() {
        for (i, defeated) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                classic_roll_defeats_at(walker, i, facing, heading),
                defeated,
                "row {n}: I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

/// rolling-into-walkers.md's examples: a classic-1 player walking right
/// at 2 px per frame from x = 200, who presses Action and Right on frame
/// 0, toward a small walker heading left. `action` says on which frames
/// Action is held; Right is held throughout.
fn classic_roll_example(action: impl Fn(usize) -> bool) -> (World, Vec<RollRow>) {
    let mut w = world_on("classic-1", [200, 400], &small_walker("260.5", "1", "left"));
    w.player.vel.x = Fixed::from_int(2);
    w.player.state = State::Walk;
    // Right was already held before frame 0.
    let mut previous: Actions = Action::Right.into();
    let mut rows = Vec::new();
    for n in 0..50 {
        let mut held: Actions = Action::Right.into();
        if action(n) {
            held.insert(Action::Action);
        }
        step(&mut w, &InputFrame::single(Frame::after(previous, held)));
        previous = held;
        rows.push(RollRow {
            x: w.player.pos.x - Fixed::from_int(200),
            vx: w.player.vel.x,
            state: w.player.state,
            events: w.events.clone(),
            defeats: w.flow.events.len(),
        });
    }
    (w, rows)
}

#[test]
fn classic_roll_example_with_action_held_boosts_the_roll() {
    let (w, rows) = classic_roll_example(|_| true);
    assert_eq!(rows[0].events, [Event::RollStart]);
    let expect = [
        (10, 27.53515625, 2.7578125, State::Roll),
        (11, 30.3203125, 2.78515625, State::Roll),
        (12, 33.12890625, 2.80859375, State::Roll),
        (13, 33.12890625, 2.80859375, State::Roll),
        (14, 33.12890625, 2.80859375, State::Roll),
        (15, 36.0859375, 2.95703125, State::Roll),
        (16, 39.171875, 3.0859375, State::Roll),
        (43, 140.6953125, 3.9609375, State::Roll),
        (46, 152.6015625, 3.97265625, State::Roll),
        (47, 156.6015625, 4.0, State::Run),
        (48, 160.4765625, 3.875, State::Run),
    ];
    for (frame, x, vx, state) in expect {
        let row = &rows[frame];
        assert_eq!(
            (row.x, row.vx, row.state),
            (fx(x), fx(vx), state),
            "frame {frame}"
        );
    }
    let defeats: Vec<_> = (0..50).filter(|&n| rows[n].defeats > 0).collect();
    assert_eq!(defeats, [12]);
    // No event marks the defeat or the boost; roll_end is on P + 32.
    let events: Vec<_> = (1..50).filter(|&n| !rows[n].events.is_empty()).collect();
    assert_eq!(events, [47]);
    assert_eq!(rows[47].events, [Event::RollEnd]);
    assert!(!w.events.contains(&Event::Hurt));
}

#[test]
fn classic_roll_example_without_action_carries_on_into_walking() {
    let (_, rows) = classic_roll_example(|n| n == 0);
    let expect = [
        (12, 33.12890625, 2.80859375, State::Roll),
        (14, 33.12890625, 2.80859375, State::Roll),
        (15, 35.9609375, 2.83203125, State::Roll),
        (30, 79.8125, 2.96484375, State::Roll),
        (31, 82.78125, 2.96875, State::Roll),
        (33, 88.75390625, 3.0, State::Roll),
        (34, 91.75390625, 3.0, State::Walk),
        (35, 94.62890625, 2.875, State::Walk),
    ];
    for (frame, x, vx, state) in expect {
        let row = &rows[frame];
        assert_eq!(
            (row.x, row.vx, row.state),
            (fx(x), fx(vx), state),
            "frame {frame}"
        );
    }
    let ends: Vec<_> = (0..50)
        .filter(|&n| rows[n].events.contains(&Event::RollEnd))
        .collect();
    assert_eq!(ends, [34]);
}

#[test]
fn classic_roll_inputs_in_the_pause_change_nothing() {
    // Action only on frame 0, then a jump pressed on K + 1 and left and
    // down on K + 2: the same as with none.
    let (_, plain) = classic_roll_example(|n| n == 0);
    let mut w = world_on("classic-1", [200, 400], &small_walker("260.5", "1", "left"));
    w.player.vel.x = Fixed::from_int(2);
    w.player.state = State::Walk;
    let rows = roll_rows(&mut w, 40, |n| match n {
        0 => Actions::from_iter([Action::Right, Action::Action]),
        13 => Actions::from_iter([Action::Jump, Action::Action]),
        14 => Actions::from_iter([Action::Left, Action::Down]),
        _ => Action::Right.into(),
    });
    for n in 0..40 {
        assert_eq!(rows[n].x - Fixed::from_int(200), plain[n].x, "frame {n}");
        assert_eq!(rows[n].state, plain[n].state, "frame {n}");
    }
}

#[test]
fn classic_roll_second_defeat_boosts_again() {
    // A second walker further on: after it the normal roll is size 5.
    let walkers = small_walker("260.5", "1", "left") + &small_walker("340.5", "1", "left");
    let world = || {
        let mut w = world_on("classic-1", [200, 400], &walkers);
        w.player.vel.x = Fixed::from_int(2);
        w.player.state = State::Walk;
        w
    };
    let held = |_| Actions::from_iter([Action::Right, Action::Action]);
    let rows = roll_rows(&mut world(), 80, held);
    let defeats: Vec<_> = (0..80).filter(|&n| rows[n].defeats > 0).collect();
    assert_eq!(defeats.len(), 2, "{defeats:?}");
    let k = defeats[1];
    assert_eq!(rows[k + 2].x, rows[k].x);
    assert_eq!(rows[k + 2].vx, rows[k].vx);
    assert!(rows[k + 2].vx < rows[k + 3].vx);
    assert_eq!(rows[k + 3 + 32].events, [Event::RollEnd]);
    let mut w = world();
    roll_rows(&mut w, k + 4, held);
    let roll = w.player.roll.unwrap();
    assert_eq!((roll.speed, roll.boosts), (Fixed::from_int(5), 2));
}

/// A classic-1 player standing at x = 300 facing right presses Action
/// alone on frame 0, a slow roll with D = 29, and holds Right from
/// D + 1; a small walker heading left starts at `walker`. Action is
/// held again on the frames `action` says.
fn classic_roll_stop(walker: &str, action: impl Fn(usize) -> bool) -> Vec<RollRow> {
    let mut w = world_on("classic-1", [300, 400], &small_walker(walker, "1", "left"));
    roll_rows(&mut w, 70, |n| {
        let mut held = Actions::NONE;
        if n == 0 || action(n) {
            held.insert(Action::Action);
        }
        if n > D {
            held.insert(Action::Right);
        }
        held
    })
}

const D: usize = 29;

/// Where the player stands in the stop after [`classic_roll_stop`]'s
/// roll, with no walker in the way.
fn classic_stop_x() -> Fixed {
    let rows = classic_roll_stop("900.5", |_| false);
    assert_eq!(rows[D].events, [Event::RollEnd]);
    assert_eq!(rows[D + 3].x, rows[D + 17].x);
    assert_eq!(rows[D + 17].vx, Fixed::ZERO);
    assert_ne!(rows[D + 18].vx, Fixed::ZERO);
    rows[D + 3].x
}

/// Where a walker heading left has to start for I to be `i` at the end
/// of frame `frame` against a player standing at `x`.
fn walker_start_for(x: Fixed, i: i64, frame: usize) -> String {
    format!("{}.5", x.floor() - i + frame as i64 + 1)
}

#[test]
fn classic_roll_stop_defeats_in_its_first_8_frames_and_lengthens() {
    let x = classic_stop_x();
    // I = −16 at the end of D + 7: defeated there.
    let rows = classic_roll_stop(&walker_start_for(x, -16, D + 7), |_| false);
    let defeats: Vec<_> = (0..70).filter(|&n| rows[n].defeats > 0).collect();
    assert_eq!(defeats, [D + 7]);
    assert!(!rows.iter().any(|r| r.events.contains(&Event::Hurt)));
    // The stop goes on 2 frames later: the player first moves on D + 20.
    assert_eq!(rows[D + 19].vx, Fixed::ZERO);
    assert_ne!(rows[D + 20].vx, Fixed::ZERO);
    assert_eq!(rows[D + 20].state, State::Walk);
}

#[test]
fn classic_roll_stop_hurts_from_d_plus_12() {
    let x = classic_stop_x();
    // I = −16 at the end of D + 8: past the attack frames, inside the hurt
    // range from D + 5, but the hurt is decided at the end of D + 11 at
    // the earliest.
    let rows = classic_roll_stop(&walker_start_for(x, -16, D + 8), |_| false);
    assert!(rows.iter().all(|r| r.defeats == 0));
    let hurt = rows.iter().position(|r| r.events.contains(&Event::Hurt));
    assert_eq!(hurt, Some(D + 12));
}

#[test]
fn classic_roll_stop_defeat_boosts_into_a_new_roll() {
    let x = classic_stop_x();
    // Defeated on D + 7; Action held from D + 12 starts a slow roll boosted
    // to size 2 there, which always carries on into running.
    let rows = classic_roll_stop(&walker_start_for(x, -16, D + 7), |n| n >= D + 12);
    let p = D + 12;
    assert_eq!(rows[p].events, [Event::RollStart]);
    assert_eq!(rows[p].state, State::Roll);
    let ends: Vec<_> = (p..70)
        .filter(|&n| rows[n].events.contains(&Event::RollEnd))
        .collect();
    assert!(ends.is_empty() || ends == [p + 32], "{ends:?}");
    // Facing right with Right held: the roll's own direction from P + 1
    // makes it normal.
    assert!(rows[p + 20].vx > fx(2.5));
}

#[test]
fn classic_roll_stop_boost_keeps_size_2_without_a_direction() {
    let x = classic_stop_x();
    let mut w = world_on(
        "classic-1",
        [300, 400],
        &small_walker(&walker_start_for(x, -16, D + 7), "1", "left"),
    );
    let rows = roll_rows(&mut w, D + 50, |n| {
        if n == 0 || n >= D + 12 {
            Action::Action.into()
        } else {
            Actions::NONE
        }
    });
    let p = D + 12;
    assert_eq!(rows[p].events, [Event::RollStart]);
    assert_eq!(rows[p + 31].state, State::Roll);
    assert_eq!(rows[p + 32].events, [Event::RollEnd]);
    // Eased toward 2 from the stop's standstill: never past it.
    assert!(rows[p..=p + 31].iter().all(|r| r.vx <= Fixed::from_int(2)));
    assert!(rows[p + 31].vx > fx(1.9));
}

#[test]
fn classic_hurt_before_a_roll_comes_first() {
    // Hurt on H as the walker walks in: an Action press on H doesn't roll.
    let (mut w, h) = classic_walked_into("");
    let rows = roll_rows(&mut w, h + 1, |n| {
        if n == h {
            Action::Action.into()
        } else {
            Actions::NONE
        }
    });
    assert_eq!(rows[h].events, [Event::Hurt]);
    assert_eq!(w.things[0].phase, Phase::Ready);
}
