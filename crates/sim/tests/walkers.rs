//! Classic 2: walkers (spec/behavior/walker.md, "In `title-2`"), when
//! they touch the player (walker-contact.md, "In `title-2`") and bouncing
//! off them (bounce.md, "In `title-2`"), being knocked out by them
//! (knock-out.md, "In `title-2`"), and rolling into them
//! (rolling-into-walkers.md, "In `title-2`").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    Behavior, Event, Facing, Fixed, FlowEvent, InputFrame, Outcome, Phase, Stance, State, World,
    generic_feel, stance, step,
};

/// The values `title-2` walkers have, with the contact ranges of
/// walker-contact.md, "In `title-2`", and the defeat range of
/// rolling-into-walkers.md, "In `title-2`", as `roll`. The spec measured
/// rolls on flat floor only; the roll's `above` range, the standing one,
/// is the engine's choice. The size only draws them.
const WALKER: &str = "size = [20, 18], offset = 0, response = [0.0625, 0.015625], \
    move_first = true, columns = true, turn_delay = 0, bounced_speed = 1, \
    defeat_jump = 8, defeat_gravity = 0.4375, defeat_max_fall = 12, \
    contact = { stand = [-23, 30, -36, 20], crouch = [-23, 30, -22, 22], \
    walk = [-25, 28, -36, 20], run = [-28, 29, -32, 17], air = [-26, 29, -28, 21], \
    roll = [-25, 27, -36, 20], turn = 7, land_hold = [17, 8], bounce_rise = 1.25, \
    bounce_above = -10 }";

/// A `title-2` walker at `x` on the floor, at `speed` heading `facing`.
fn walker(x: i32, speed: &str, facing: &str) -> String {
    format!(
        r#"
        [[entities]]
        type = "crawler"
        at = [{x}, 400]
        params = {{ speed = {speed}, facing = "{facing}", {WALKER} }}
        "#
    )
}

/// Flat ground at y = 400 from x = 0 to 1000 on classic-2, the player at
/// `start`, and `entities`.
fn world(start: [i32; 2], entities: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "arena"
        size = [1000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 1000, 200]

        {entities}
        "#,
        start[0], start[1]
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-2").unwrap()),
    )
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn speed(bits: i64) -> Fixed {
    Fixed::from_f64(bits as f64 / 256.0).unwrap()
}

/// Steps `world` once holding `held`, after `previous`.
fn once(world: &mut World, previous: Actions, held: Actions) {
    step(world, &InputFrame::single(Frame::after(previous, held)));
}

/// Steps `world` `frames` times with nothing held.
fn idle(world: &mut World, frames: usize) {
    for _ in 0..frames {
        once(world, Actions::NONE, Actions::NONE);
    }
}

/// The walker `i`'s fall speed while it falls, 0 otherwise.
fn fall_speed(world: &World, i: usize) -> Fixed {
    match world.things[i].phase {
        Phase::Falling { vy, .. } => vy,
        _ => Fixed::ZERO,
    }
}

#[test]
fn classic_2_walker_speeds_up_from_rest() {
    // walker.md, "Walking (`title-2`)": the speed at the end of each frame
    // from rest, and the example from x = 100.
    let mut w = world([800, 400], &walker(100, "1", "right"));
    w.things[0].vx = Fixed::ZERO;
    let want = [
        20, 37, 53, 68, 81, 93, 105, 116, 126, 136, 144, 152, 159, 166, 172, 178, 183, 188, 193,
        196, 199, 202, 205, 208, 211, 213, 215, 217, 219, 221, 223, 225, 226, 227, 228, 229, 230,
        231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 256,
    ];
    let mut x = Vec::new();
    for (n, &v) in want.iter().enumerate() {
        idle(&mut w, 1);
        assert_eq!(w.things[0].vx, speed(v), "frame {}", n + 1);
        assert_eq!(w.things[0].pos.y, fx(400.0));
        x.push(w.things[0].pos.x);
    }
    let table = [100.0, 100.078125, 100.22265625, 100.4296875, 100.6953125];
    for (n, &want) in table.iter().enumerate() {
        assert_eq!(x[n], fx(want), "frame {}", n + 1);
    }
    // Full speed is first used for the move on the 50th frame.
    idle(&mut w, 1);
    assert_eq!(w.things[0].pos.x - x[48], Fixed::ONE);
}

/// A classic-2 player falling from 100 px up at x = 300 onto walkers
/// made by `walkers`, facing `facing`: a fresh world and the frame it
/// bounces on, by frame count from 0, with nothing held, if it does
/// within a second.
fn try_bounce(walkers: &str, facing: Facing) -> Option<(World, usize)> {
    let fresh = || {
        let mut w = world([300, 300], walkers);
        w.player.facing = facing;
        w
    };
    let mut w = fresh();
    let b = (0..60).find(|_| {
        idle(&mut w, 1);
        w.events.contains(&Event::Bounce)
    })?;
    Some((fresh(), b))
}

/// [`try_bounce`], which must bounce.
fn bounce_on(walkers: &str, facing: Facing) -> (World, usize) {
    try_bounce(walkers, facing).expect("bounces")
}

#[test]
fn classic_2_bounced_walker_holds_still_then_falls_through_the_floor() {
    // walker.md, "Example (`title-2`)": a walker heading left at full
    // speed, relative to the end of B - 1. A second walker far off holds
    // still on B + 1 and B + 2 only.
    let rows = [
        (0, 0.0, 0.0, -8.0),
        (1, 0.0, 0.0, -8.0),
        (2, 0.0, 0.0, -8.0),
        (3, -1.0, -8.0, -7.5625),
        (4, -2.0, -15.5625, -7.125),
        (5, -3.0, -22.6875, -6.6875),
        (20, -18.0, -77.0625, -0.125),
        (21, -19.0, -77.1875, 0.3125),
        (22, -20.0, -76.875, 0.75),
        (39, -37.0, -4.625, 8.1875),
        (40, -38.0, 3.5625, 8.625),
        (48, -46.0, 84.8125, 12.0),
        (49, -47.0, 96.8125, 12.0),
    ];
    for facing in [Facing::Right, Facing::Left] {
        // Find a start for the walker that gives a bounce.
        let entities = |x: i32| format!("{}{}", walker(x, "1", "left"), walker(900, "1", "left"));
        let (mut w, b) = (300..360)
            .find_map(|x| try_bounce(&entities(x), facing))
            .expect("bounces on some walker");
        let mut seen = Vec::new();
        for _ in 0..b + 50 {
            idle(&mut w, 1);
            let t = &w.things[0];
            seen.push((t.pos, fall_speed(&w, 0), t.facing, w.things[1].pos.x));
        }
        let at = seen[b - 1].0;
        let way = if facing == Facing::Right { 1.0 } else { -1.0 };
        let away = if facing == Facing::Right {
            Facing::Left
        } else {
            Facing::Right
        };
        for (n, dx, dy, vy) in rows {
            let (pos, got_vy, got_facing, _) = seen[b + n];
            assert_eq!(
                (pos.x - at.x, pos.y - at.y, got_vy),
                (fx(dx * way), fx(dy), fx(vy)),
                "{facing:?}, B + {n}"
            );
            assert_eq!(got_facing, away, "{facing:?}, B + {n}");
        }
        // The other walker moves on B, holds still on B + 1 and B + 2, and
        // goes on at full speed from B + 3.
        let other: Vec<_> = (b - 1..b + 5).map(|n| seen[n + 1].3 - seen[n].3).collect();
        assert_eq!(
            other,
            [-1.0, 0.0, 0.0, -1.0, -1.0, -1.0].map(fx),
            "{facing:?}"
        );
    }
}

#[test]
fn classic_2_bounce_holds_still_for_two_frames_then_rises() {
    // bounce.md, "Example (`title-2`)", relative to B: jump not held.
    let (mut w, b) = bounce_on(&walker(300, "0", "left"), Facing::Right);
    let mut rows = Vec::new();
    for _ in 0..b + 41 {
        idle(&mut w, 1);
        rows.push((w.player.pos, w.player.vel.y, w.player.state));
    }
    let at = rows[b].0;
    assert_eq!(rows[b].1, fx(-8.21875));
    assert_eq!(rows[b].2, State::Bounce);
    let want = [
        (1, 0.0, -8.21875),
        (2, 0.0, -8.21875),
        (3, -7.78125, -7.78125),
        (4, -15.125, -7.34375),
        (5, -22.03125, -6.90625),
        (20, -73.125, -0.34375),
        (21, -73.03125, 0.09375),
        (22, -72.5, 0.53125),
        (40, 11.46875, 8.0),
    ];
    for (n, dy, vy) in want {
        let (pos, got_vy, state) = rows[b + n];
        assert_eq!((pos.y - at.y, got_vy), (fx(dy), fx(vy)), "B + {n}");
        assert_eq!(pos.x, at.x);
        assert_eq!(state, State::Bounce, "B + {n}");
    }
}

#[test]
fn classic_2_bounce_height_by_how_long_jump_is_held_from_b_plus_3() {
    // bounce.md, "How long jump is held (`title-2`)".
    let table = [
        (0, 73.125),
        (1, 74.671875),
        (2, 76.2109375),
        (3, 77.671875),
        (4, 79.046875),
        (5, 80.3359375),
        (7, 82.7265625),
        (9, 84.875),
        (12, 87.515625),
        (15, 89.578125),
        (17, 90.578125),
        (20, 91.609375),
        (22, 91.9140625),
        (23, 92.0),
        (30, 92.0),
    ];
    let (_, b) = bounce_on(&walker(300, "0", "left"), Facing::Right);
    for (k, height) in table {
        let (mut w, _) = bounce_on(&walker(300, "0", "left"), Facing::Right);
        idle(&mut w, b + 1);
        let at = w.player.pos.y;
        let mut previous = Actions::NONE;
        let mut top = at;
        for n in 1..40 {
            // Jump is held from B + 3 for `k` frames.
            let held = if n >= 3 && n < 3 + k {
                Actions::from(Action::Jump)
            } else {
                Actions::NONE
            };
            once(&mut w, previous, held);
            previous = held;
            top = top.min(w.player.pos.y);
        }
        assert_eq!(at - top, fx(height), "K = {k}");
    }
}

#[test]
fn classic_2_hold_before_the_bounce_counts_only_if_still_held_on_b_plus_3() {
    // bounce.md: a press first made on B + 1 counts as a hold carried
    // over; input on B + 1 and B + 2 is otherwise ignored.
    let (_, b) = bounce_on(&walker(300, "0", "left"), Facing::Right);
    let height = |first: usize| {
        let (mut w, _) = bounce_on(&walker(300, "0", "left"), Facing::Right);
        idle(&mut w, b + 1);
        let at = w.player.pos.y;
        let mut previous = Actions::NONE;
        let mut top = at;
        for n in 1..40 {
            let held = if n >= first && n < 6 {
                Actions::from(Action::Jump)
            } else {
                Actions::NONE
            };
            once(&mut w, previous, held);
            previous = held;
            top = top.min(w.player.pos.y);
        }
        at - top
    };
    // Held through B + 5, from B + 1, B + 2 or B + 3: K = 3.
    for first in 1..=3 {
        assert_eq!(height(first), fx(77.671875), "from B + {first}");
    }
    // From B + 4: K = 0.
    assert_eq!(height(4), fx(73.125));
}

/// A classic-2 world with the player standing still long since at
/// `player` and a walker made by `walker`, held at `at` and heading left
/// at `vx`, before any frame of an example.
fn standing(player: f64, walker: &str, at: (f64, f64), vx: f64) -> World {
    let mut w = world([500, 400], walker);
    idle(&mut w, 40);
    assert!(w.player.grounded());
    w.player.pos.x = fx(player);
    w.things[0].pos = Vec2::new(fx(at.0), fx(at.1));
    w.things[0].vx = fx(vx);
    w
}

#[test]
fn classic_2_a_walker_hurts_a_standing_player_on_the_frame_it_reaches() {
    // walker-contact.md, "Examples (`title-2`)": the walker is at 100.875
    // at the end of frame 1, so at 101.875 before it.
    let mut w = standing(76.5, &walker(900, "1", "left"), (101.875, 400.0), -1.0);
    for frame in 1..=3 {
        idle(&mut w, 1);
        assert_eq!(w.things[0].pos.x, fx(101.875 - frame as f64));
        assert_eq!(w.events.contains(&Event::Hurt), frame == 3, "frame {frame}");
    }
}

/// Whether a classic-2 player standing still facing `facing`, `i` px
/// right of a still walker on the same floor (`dy` 0) or `dy` px higher,
/// is hurt by it on the next frame, holding `held`.
fn hurt_at(i: i32, dy: i32, facing: Facing, held: Actions) -> bool {
    let mut w = standing(
        f64::from(400 + i),
        &walker(900, "0", "left"),
        (400.0, f64::from(400 - dy)),
        0.0,
    );
    w.player.facing = facing;
    if held != Actions::NONE {
        // Settle into the stance first, out of reach.
        let at = w.things[0].pos;
        w.things[0].pos.x = fx(100.0);
        let mut previous = Actions::NONE;
        for _ in 0..30 {
            once(&mut w, previous, held);
            previous = held;
        }
        w.player.pos.x = fx(f64::from(400 + i));
        w.things[0].pos = at;
        once(&mut w, previous, held);
    } else {
        idle(&mut w, 1);
    }
    w.events.contains(&Event::Hurt)
}

#[test]
fn classic_2_standing_and_crouching_ranges() {
    // walker-contact.md, "Ranges (`title-2`)": standing, -23 to 30 both
    // ways, and a walker 30 px above (U = -30) hurts a standing player
    // but not a crouching one (U at least -22).
    for facing in [Facing::Right, Facing::Left] {
        let hurt: Vec<_> = (-30..40)
            .filter(|&i| hurt_at(i, 0, facing, Actions::NONE))
            .collect();
        assert_eq!(hurt, (-23..=30).collect::<Vec<_>>(), "{facing:?}");
    }
    assert!(hurt_at(0, 30, Facing::Right, Actions::NONE));
    assert!(hurt_at(0, 36, Facing::Right, Actions::NONE));
    assert!(!hurt_at(0, 37, Facing::Right, Actions::NONE));
    let down = Actions::from(Action::Down);
    assert!(!hurt_at(0, 30, Facing::Right, down));
    assert!(hurt_at(0, 22, Facing::Right, down));
    assert!(!hurt_at(0, 23, Facing::Right, down));
    assert!(hurt_at(0, -22, Facing::Right, down));
    assert!(!hurt_at(0, -23, Facing::Right, down));
}

/// A classic-2 player in the air at the end of a frame E - 1 at `pos`
/// moving at `vy`, holding jump since its take-off if `held`, with a
/// still walker at `walker` (its position at the end of E - 1). Returns
/// what happens on E: `Some(true)` for a bounce, `Some(false)` for a
/// hurt, `None` for neither.
fn in_air(pos: (f64, f64), vy: Fixed, held: bool, walker_at: (f64, f64)) -> Option<bool> {
    let mut w = standing(500.0, &walker(900, "0", "left"), walker_at, 0.0);
    w.player.pos = Vec2::new(fx(pos.0), fx(pos.1));
    w.player.vel = Vec2::new(Fixed::ZERO, vy);
    w.player.ground = None;
    w.player.facing = Facing::Right;
    w.player.state = if held { State::Jump } else { State::Fall };
    w.player.jump_hold = held;
    let jump = if held {
        Actions::from(Action::Jump)
    } else {
        Actions::NONE
    };
    once(&mut w, jump, jump);
    if w.events.contains(&Event::Bounce) {
        Some(true)
    } else if w.events.contains(&Event::Hurt) {
        Some(false)
    } else {
        None
    }
}

#[test]
fn classic_2_bounce_or_hurt_in_the_air() {
    // walker-contact.md, "Examples (`title-2`)": falling at 2 after a
    // jump let go of, the walker at (110.875, 400) before its move; the
    // player ends the frame at (130.5, 381.75), U = 19: a bounce.
    let fell = in_air((130.5, 379.3125), fx(2.0), false, (110.875, 400.0));
    assert_eq!(fell, Some(true));
    // Rising with jump held: -410/256 becomes -1.25 and bounces; -411/256
    // becomes -321/256 and hurts.
    assert_eq!(
        in_air((400.0, 386.25), speed(-410), true, (400.0, 400.0)),
        Some(true)
    );
    assert_eq!(
        in_air((400.0, 386.25), speed(-411), true, (400.0, 400.0)),
        Some(false)
    );
    // U = -10 bounces, -11 hurts: with the walker 30 px above the floor,
    // the player ends the frame at y 380 or 381 after falling 2.4375.
    assert_eq!(
        in_air((400.0, 377.5625), fx(2.0), false, (400.0, 370.0)),
        Some(true)
    );
    assert_eq!(
        in_air((400.0, 378.5625), fx(2.0), false, (400.0, 370.0)),
        Some(false)
    );
    // The air ranges: across -26 to 29 facing right, above -28 to 21.
    assert_eq!(
        in_air((374.0, 377.5625), fx(2.0), false, (400.0, 400.0)),
        Some(true)
    );
    assert_eq!(
        in_air((373.0, 377.5625), fx(2.0), false, (400.0, 400.0)),
        None
    );
    assert_eq!(
        in_air((429.0, 377.5625), fx(2.0), false, (400.0, 400.0)),
        Some(true)
    );
    assert_eq!(
        in_air((430.0, 377.5625), fx(2.0), false, (400.0, 400.0)),
        None
    );
    assert_eq!(
        in_air((400.0, 376.5625), fx(2.0), false, (400.0, 400.0)),
        Some(true)
    );
    assert_eq!(
        in_air((400.0, 375.5625), fx(2.0), false, (400.0, 400.0)),
        None
    );
}

/// The player's stance for the walker in `world`.
fn stance_of(world: &World) -> Option<Stance> {
    let Some(Behavior::Walker {
        contact: Some(contact),
        ..
    }) = world.things[0].archetype.map(|a| a.behavior)
    else {
        panic!("a walker with a contact table");
    };
    stance(&world.player, &contact, &world.player_physics())
}

/// Plays `world` holding `held` for `frames` frames from `previous`, and
/// returns the stance after each.
fn stances(
    world: &mut World,
    previous: Actions,
    held: Actions,
    frames: usize,
) -> Vec<Option<Stance>> {
    let mut previous = previous;
    (0..frames)
        .map(|_| {
            once(world, previous, held);
            previous = held;
            stance_of(world)
        })
        .collect()
}

#[test]
fn classic_2_stances_switch_inside_the_measured_frames() {
    // walker-contact.md, "Poses (`title-2`)": slowing down, the standing
    // ranges take over 18 to 34 frames after the direction is let go of,
    // and running with Action let go of, the walking ones 8 to 21 frames
    // after.
    let first = |s: &[Option<Stance>], want: Stance| s.iter().position(|&x| x == Some(want));
    let right = Actions::from(Action::Right);
    let run = right | Actions::from(Action::Action);

    let mut w = world([100, 400], &walker(900, "0", "left"));
    idle(&mut w, 30);
    assert_eq!(stance_of(&w), Some(Stance::Standing));
    let walking = stances(&mut w, Actions::NONE, right, 60);
    assert_eq!(walking[59], Some(Stance::Walking));
    let after = stances(&mut w, right, Actions::NONE, 60);
    let stand = first(&after, Stance::Standing).unwrap() + 1;
    assert!((18..=34).contains(&stand), "stands {stand} frames on");
    assert!(
        after[..stand - 1]
            .iter()
            .all(|&s| s == Some(Stance::Walking))
    );

    // A roll with Action held all through carries on into running.
    let running = || {
        let mut w = world([100, 400], &walker(900, "0", "left"));
        idle(&mut w, 30);
        stances(&mut w, Actions::NONE, right, 20);
        let s = stances(&mut w, right, run, 80);
        assert_eq!(s[79], Some(Stance::Running));
        w
    };
    let mut w = running();
    let after = stances(&mut w, run, right, 40);
    let walk = first(&after, Stance::Walking).unwrap() + 1;
    assert!((8..=21).contains(&walk), "walks {walk} frames on");
    let mut w = running();
    let after = stances(&mut w, run, Actions::NONE, 60);
    let stand = first(&after, Stance::Standing).unwrap() + 1;
    assert!((18..=34).contains(&stand), "stands {stand} frames on");
    assert!(
        after[..stand - 1]
            .iter()
            .all(|&s| s == Some(Stance::Running))
    );

    // A jump is in the air from the frame it starts on, still on the
    // ground, and up to 17 frames after its landing frame.
    let mut w = world([100, 400], &walker(900, "0", "left"));
    idle(&mut w, 30);
    let jump = Actions::from(Action::Jump);
    let mut previous = Actions::NONE;
    let mut rows = Vec::new();
    for n in 0..80 {
        let held = if n == 0 { jump } else { Actions::NONE };
        once(&mut w, previous, held);
        previous = held;
        rows.push((
            stance_of(&w),
            w.player.grounded(),
            w.events.contains(&Event::Land),
        ));
    }
    assert_eq!(rows[0], (Some(Stance::Air), true, false));
    let l = rows.iter().position(|r| r.2).expect("lands");
    assert!(rows[..l + 18].iter().all(|r| r.0 == Some(Stance::Air)));
    assert_eq!(rows[l + 18].0, Some(Stance::Standing));
}

/// One row of a knock-out: the player's position, speed, whether it's
/// on the ground, its facing and state, the step's events and flow
/// events, the pause after it, and the walker's `x`.
type Row = (
    Vec2,
    Vec2,
    bool,
    Facing,
    State,
    Vec<Event>,
    Vec<FlowEvent>,
    u8,
    Fixed,
);

/// Steps `w` `frames` times with `held` held throughout, recording rows.
fn record(w: &mut World, frames: usize, held: Actions) -> Vec<Row> {
    (0..frames)
        .map(|_| {
            once(w, held, held);
            (
                w.player.pos,
                w.player.vel,
                w.player.grounded(),
                w.player.facing,
                w.player.state,
                w.events.clone(),
                w.flow.events.clone(),
                w.pause,
                w.things[0].pos.x,
            )
        })
        .collect()
}

/// A classic-2 player standing on flat floor facing `facing`, hurt by a
/// walker heading left on the third frame (walker-contact.md, "Examples
/// (`title-2`)"), then `frames` more frames holding `held`. Returns the
/// rows and H's index.
fn knocked_out(facing: Facing, frames: usize, held: Actions) -> (World, Vec<Row>, usize) {
    let mut w = standing(76.5, &walker(900, "1", "left"), (101.875, 400.0), -1.0);
    w.player.facing = facing;
    let rows = record(&mut w, 3 + frames, held);
    assert_eq!(rows[2].5.first(), Some(&Event::Hurt));
    (w, rows, 2)
}

#[test]
fn classic_2_knock_out_follows_the_example() {
    let (w, rows, h) = knocked_out(Facing::Right, 191, Actions::NONE);
    let at = rows[h].0;
    let row = |k: usize| {
        let r = &rows[h + k];
        (r.0.x - at.x, r.0.y - at.y, r.1.x, r.1.y)
    };
    // On H the player stands where it stood, with the flight's speeds.
    assert_eq!(at, Vec2::new(fx(76.5), fx(400.0)));
    let table = [
        (0, 0.0, 0.0, -1.0, -5.5),
        (1, -1.0, -5.265625, -1.0, -5.265625),
        (2, -2.0, -10.296875, -1.0, -5.03125),
        (23, -23.0, -61.8125, -1.0, -0.109375),
        (46, -46.0, 0.0, -1.0, 0.0),
        (47, -47.0, 0.0, -1.0, 0.0),
        (48, -48.0, 0.0, -1.0, -2.5),
        (49, -48.875, -2.265625, -0.875, -2.265625),
        (50, -49.640625, -4.296875, -0.765625, -2.03125),
        (58, -53.22265625, -12.109375, -0.2734375, -0.15625),
        (69, -54.79296875, 0.0, -0.07421875, 0.0),
        (79, -55.24609375, 0.0, -0.02734375, 0.0),
        (80, -55.24609375, 0.0, 0.0, 0.0),
        (189, -55.24609375, 0.0, 0.0, 0.0),
    ];
    for (k, x, y, vx, vy) in table {
        assert_eq!(row(k), (fx(x), fx(y), fx(vx), fx(vy)), "H + {k}");
    }
    // In the air from H + 1 to H + 45 and from the hop to H + 68; on the
    // ground otherwise, landing on H + 46 and H + 69.
    for k in 0..=189 {
        let air = (1..=45).contains(&k) || (49..=68).contains(&k);
        assert_eq!(rows[h + k].2, !air, "H + {k}");
        assert_eq!(rows[h + k].3, Facing::Right, "H + {k}");
        assert_eq!(rows[h + k].4, State::Hurt, "H + {k}");
        let events: &[Event] = match k {
            0 => &[Event::Hurt],
            46 | 69 => &[Event::Land],
            _ => &[],
        };
        assert_eq!(rows[h + k].5, events, "H + {k}");
        assert!(rows[h + k].6.iter().all(|e| *e != FlowEvent::Respawn));
    }
    // vx is -7/256 on P + 31 and 0 from P + 32.
    assert_eq!(row(48 + 31).2, speed(-7));
    // The 6-frame pause comes between H + 1 and H + 2.
    for (k, r) in rows.iter().enumerate() {
        assert_eq!(r.7, if k == h + 1 { 6 } else { 0 }, "frame {k}");
    }
    // H + 189 is the last frame; the life is lost after it.
    assert_eq!(rows[h + 190].6, [FlowEvent::Respawn]);
    assert_eq!(w.flow.outcome, Outcome::Playing);
    assert_eq!(w.player.knockout, None);
}

#[test]
fn classic_2_walkers_hold_on_h_plus_1_and_catch_up() {
    let (_, rows, h) = knocked_out(Facing::Right, 4, Actions::NONE);
    let x = |k: usize| rows[h + k].8;
    // It moves on H, not on H + 1, and by H + 2 it's where it would have
    // been with no stop; then 1 px a frame.
    assert_eq!(x(0), rows[h - 1].8 - fx(1.0));
    assert_eq!(x(1), x(0));
    assert_eq!(x(2), x(0) - fx(2.0));
    assert_eq!(x(3), x(0) - fx(3.0));
}

#[test]
fn classic_2_knock_out_faces_right_and_flies_left_from_either_facing() {
    let (_, right, h) = knocked_out(Facing::Right, 60, Actions::NONE);
    let (_, left, _) = knocked_out(Facing::Left, 60, Actions::NONE);
    // Facing left on H - 1, H is a turn; otherwise the same.
    assert_eq!(left[h].5, [Event::Hurt, Event::Turn]);
    assert_eq!(left[h].3, Facing::Right);
    for k in 0..=60 {
        assert_eq!(left[h + k].0, right[h + k].0, "H + {k}");
        assert_eq!(left[h + k].1, right[h + k].1, "H + {k}");
    }
}

#[test]
fn classic_2_knock_out_ignores_input() {
    let (_, still, h) = knocked_out(Facing::Right, 191, Actions::NONE);
    for held in [
        Actions::from(Action::Right),
        Actions::from(Action::Left) | Actions::from(Action::Jump),
        Actions::from(Action::Action) | Actions::from(Action::Down),
    ] {
        let mut w = standing(76.5, &walker(900, "1", "left"), (101.875, 400.0), -1.0);
        let mut rows = record(&mut w, 3, Actions::NONE);
        rows.extend(record(&mut w, 191, held));
        for k in 0..=190 {
            assert_eq!(rows[h + k].0, still[h + k].0, "H + {k}, {held:?}");
            assert_eq!(rows[h + k].5, still[h + k].5, "H + {k}, {held:?}");
        }
    }
}

#[test]
fn classic_2_a_late_landing_hops_on_landing_and_lasts_to_p_plus_140() {
    // knock-out.md, "The hop (`title-2`)": still in the air on H + 48,
    // the hop comes on the landing frame, and the last frame is P + 140.
    let (mut w, mut rows, h) = knocked_out(Facing::Right, 20, Actions::NONE);
    w.player.pos.y -= fx(96.0);
    rows.extend(record(&mut w, 300, Actions::NONE));
    let p = (h + 48..rows.len()).find(|&i| rows[i].2).expect("lands");
    assert!(p > h + 50, "P = H + {}", p - h);
    assert_eq!(rows[p].5, [Event::Land]);
    assert_eq!(rows[p].1.y, fx(-2.5));
    assert!(!rows[p + 1].2);
    assert_eq!(rows[p + 1].1.x, fx(-0.875));
    let respawn = rows
        .iter()
        .position(|r| r.6.contains(&FlowEvent::Respawn))
        .expect("comes back");
    assert_eq!(respawn, p + 141);
}

/// The actions in `list`, held together.
fn hold(list: &[Action]) -> Actions {
    list.iter()
        .fold(Actions::NONE, |all, &a| all | Actions::from(a))
}

/// One frame of a roll into a walker: the player's `x`, `vx`, state and
/// events, whether a walker was defeated, walker 0's position and fall
/// speed, and walker 1's `x`.
#[derive(Clone, Debug)]
struct RollRow {
    x: Fixed,
    vx: Fixed,
    state: State,
    events: Vec<Event>,
    defeat: bool,
    walker: Vec2,
    vy: Fixed,
    other: Fixed,
}

/// The [`RollRow`] for the frame `world` has just stepped.
fn roll_row(world: &World) -> RollRow {
    RollRow {
        x: world.player.pos.x,
        vx: world.player.vel.x,
        state: world.player.state,
        events: world.events.clone(),
        defeat: world
            .flow
            .events
            .iter()
            .any(|e| matches!(e, FlowEvent::Defeat(_))),
        walker: world.things[0].pos,
        vy: fall_speed(world, 0),
        other: world.things[1].pos.x,
    }
}

/// Steps `world` once for each of `inputs`, from nothing held.
fn play(world: &mut World, inputs: &[Actions]) -> Vec<RollRow> {
    let mut previous = Actions::NONE;
    inputs
        .iter()
        .map(|&held| {
            once(world, previous, held);
            previous = held;
            roll_row(world)
        })
        .collect()
}

/// rolling-into-walkers.md, "Examples (`title-2`)": the player standing
/// facing right at x 100, a walker heading left at full speed at x
/// 160.875 at the end of frame -1, and another far off.
fn roll_example() -> World {
    let walkers = format!("{}{}", walker(161, "1", "left"), walker(900, "1", "left"));
    let mut w = world([100, 400], &walkers);
    w.things[0].pos.x = fx(160.875);
    w
}

/// Right and Action on frame 0, then `rest` for `frames - 1` frames.
fn roll_inputs(frames: usize, rest: Actions) -> Vec<Actions> {
    let mut inputs = vec![rest; frames];
    inputs[0] = hold(&[Action::Right, Action::Action]);
    inputs
}

#[test]
fn classic_2_rolls_into_a_walker_without_action() {
    // "Action on frame 0 only": K = 12, the roll stops on S + 33.
    let mut w = roll_example();
    let rows = play(&mut w, &roll_inputs(50, hold(&[Action::Right])));
    let want = [
        (0, 100.421875, 0.421875, State::Roll),
        (11, 121.5234375, 2.6796875, State::Roll),
        (12, 124.2890625, 2.765625, State::Roll),
        (13, 124.2890625, 2.765625, State::Roll),
        (14, 124.2890625, 2.765625, State::Roll),
        (15, 127.12890625, 2.83984375, State::Roll),
        (32, 181.02734375, 3.30859375, State::Roll),
        (33, 184.34375, 0.0, State::Idle),
    ];
    for (n, x, vx, state) in want {
        let r = &rows[n];
        assert_eq!((r.x, r.vx, r.state), (fx(x), fx(vx), state), "frame {n}");
    }
    assert_eq!(rows[0].events, [Event::RollStart]);
    assert_eq!(rows[33].events, [Event::RollEnd]);
    let defeats: Vec<_> = (0..50).filter(|&n| rows[n].defeat).collect();
    assert_eq!(defeats, [12]);
    assert!(rows.iter().all(|r| !r.events.contains(&Event::Hurt)));
    // The stop lasts to D + 14, as without a defeat.
    assert!((33..=47).all(|n| rows[n].x == fx(184.34375)));
    assert!(rows[48].x > rows[47].x);
}

#[test]
fn classic_2_rolls_into_a_walker_with_action_held() {
    // "Action held from frame 0": the roll grows to 4.375 from K + 3 and
    // carries on into running on K + 34.
    let mut w = roll_example();
    let ra = hold(&[Action::Right, Action::Action]);
    let rows = play(&mut w, &roll_inputs(50, ra));
    let want = [
        (12, 124.2890625, 2.765625, State::Roll),
        (13, 124.2890625, 2.765625, State::Roll),
        (14, 124.2890625, 2.765625, State::Roll),
        (15, 127.25390625, 2.96484375, State::Roll),
        (16, 130.39453125, 3.140625, State::Roll),
        (45, 248.4921875, 4.3359375, State::Roll),
        (46, 252.83203125, 4.33984375, State::Run),
    ];
    for (n, x, vx, state) in want {
        let r = &rows[n];
        assert_eq!((r.x, r.vx, r.state), (fx(x), fx(vx), state), "frame {n}");
    }
    assert!((1..46).all(|n| rows[n].events.is_empty()), "{rows:?}");
    assert_eq!(rows[46].events, [Event::RollEnd]);
    // From D + 1 it runs as usual, easing toward 3.375.
    assert!(rows[47].vx < rows[46].vx);
}

#[test]
fn classic_2_walker_rolled_into_flies_off_and_every_walker_holds_still() {
    // "The walker (`title-2`)": the defeated walker doesn't move on K,
    // nor on K + 1 and K + 2, then leaves at -1 across and -8 up, falling
    // by 0.4375 a frame. The other walker holds still on K + 1 and K + 2.
    let mut w = roll_example();
    let rows = play(&mut w, &roll_inputs(20, hold(&[Action::Right])));
    let k = 12;
    let at = rows[k - 1].walker;
    for (after, r) in rows[k..=k + 2].iter().enumerate() {
        assert_eq!((r.walker, r.vy), (at, fx(-8.0)), "K + {after}");
    }
    let path = [(3, -1.0, -8.0, -7.5625), (4, -2.0, -15.5625, -7.125)];
    for (after, dx, dy, vy) in path {
        let r = &rows[k + after];
        assert_eq!(
            (r.walker.x - at.x, r.walker.y - at.y, r.vy),
            (fx(dx), fx(dy), fx(vy)),
            "K + {after}"
        );
    }
    let other: Vec<_> = (k - 1..k + 4)
        .map(|n| rows[n + 1].other - rows[n].other)
        .collect();
    assert_eq!(other, [-1.0, 0.0, 0.0, -1.0, -1.0].map(fx));
}

#[test]
fn classic_2_inputs_on_the_frozen_frames_after_a_roll_defeat_are_lost() {
    // "The pause (`title-2`)": Left, Jump or Action only on K + 1 and
    // K + 2, or pressed then and held on, change nothing; Left still held
    // on K + 3 turns the roll from K + 3.
    let r = hold(&[Action::Right]);
    let plain = play(&mut roll_example(), &roll_inputs(40, r));
    for extra in [Action::Left, Action::Jump, Action::Action] {
        let mut inputs = roll_inputs(40, r);
        inputs[13] = hold(&[extra]);
        inputs[14] = hold(&[extra]);
        let rows = play(&mut roll_example(), &inputs);
        assert_eq!(
            rows.iter().map(|r| (r.x, r.vx)).collect::<Vec<_>>(),
            plain.iter().map(|r| (r.x, r.vx)).collect::<Vec<_>>(),
            "{extra:?} on K + 1 and K + 2"
        );
        if extra != Action::Left {
            // Pressed on K + 1 and held from then on.
            for input in &mut inputs[15..] {
                *input = hold(&[Action::Right, extra]);
            }
            let rows = play(&mut roll_example(), &inputs);
            assert_eq!(
                rows.iter().map(|r| (r.x, r.vx)).collect::<Vec<_>>(),
                plain.iter().map(|r| (r.x, r.vx)).collect::<Vec<_>>(),
                "{extra:?} pressed on K + 1 and held on"
            );
        }
    }
    let mut inputs = roll_inputs(20, r);
    for input in &mut inputs[13..=15] {
        *input = hold(&[Action::Left]);
    }
    let rows = play(&mut roll_example(), &inputs);
    assert_eq!(rows[14].vx, rows[12].vx);
    assert!(rows[15].vx < rows[14].vx);
}

#[test]
fn classic_2_second_walker_rolled_into_boosts_again() {
    // "How the roll goes on (`title-2`)": a second defeat with Action held
    // grows the roll to 5.375 and starts 31 more roll frames.
    let walkers = format!("{}{}", walker(161, "1", "left"), walker(300, "1", "left"));
    let mut w = world([100, 400], &walkers);
    w.things[0].pos.x = fx(160.875);
    let ra = hold(&[Action::Right, Action::Action]);
    let rows = play(&mut w, &roll_inputs(120, ra));
    let defeats: Vec<_> = (0..120).filter(|&n| rows[n].defeat).collect();
    assert_eq!(defeats.len(), 2, "{defeats:?}");
    assert_eq!(defeats[0], 12);
    let k = defeats[1];
    assert_eq!(rows[k + 1].x, rows[k].x);
    assert_eq!(rows[k + 2].x, rows[k].x);
    assert!((k + 3..k + 34).all(|n| rows[n].state == State::Roll));
    assert_eq!(rows[k + 34].events, [Event::RollEnd]);
    assert_eq!(rows[k + 34].state, State::Run);
    // Easing toward 5.375 from 4.375's neighbourhood.
    assert!(rows[k + 33].vx > fx(5.0), "{:?}", rows[k + 33].vx);
    assert!(rows[k + 33].vx < fx(5.375));
}

/// A classic-2 player at x 500 facing `facing`, a walker far off, and the
/// frames of a normal roll its way (S = 0) from standing, with `then`
/// held after frame 0.
fn lone_roll(facing: Facing, then: &[Action]) -> (World, Vec<Actions>) {
    let walkers = format!("{}{}", walker(950, "1", "left"), walker(990, "1", "left"));
    let mut w = world([500, 400], &walkers);
    w.player.facing = facing;
    let way = if facing == Facing::Right {
        Action::Right
    } else {
        Action::Left
    };
    let mut inputs = vec![hold(&[&[way], then].concat()); 60];
    inputs[0] = hold(&[way, Action::Action]);
    (w, inputs)
}

/// Plays `inputs` on `world` up to frame `n`, after putting walker 0 at
/// the end of frame `n - 1` so that I on frame `n` is `i`, given the
/// player's `x` on frame `n` without it, `x`. Returns frame `n`.
fn with_walker_at(mut world: World, inputs: &[Actions], n: usize, x: Fixed, i: i64) -> RollRow {
    play(&mut world, &inputs[..n]);
    world.things[0].pos.x = Fixed::from_int((x.floor() - i) as i32) + fx(0.5);
    let previous = if n > 0 { inputs[n - 1] } else { Actions::NONE };
    once(&mut world, previous, inputs[n]);
    roll_row(&world)
}

#[test]
fn classic_2_rolls_defeat_walkers_in_the_measured_ranges() {
    // "When a walker is defeated (`title-2`)": -25 to 27 facing right,
    // -20 to 32 facing left, on every roll frame, S and S + 30 included,
    // and on no frame after.
    for (facing, from, to) in [(Facing::Right, -25, 27), (Facing::Left, -20, 32)] {
        let (w, inputs) = lone_roll(facing, &[]);
        let plain = play(&mut w.clone(), &inputs);
        assert_eq!(plain[31].events, [Event::RollEnd]);
        for n in [0, 1, 15, 30] {
            let x = plain[n].x;
            let at = |i| with_walker_at(w.clone(), &inputs, n, x, i);
            for (i, defeated) in [(from - 1, false), (from, true), (to, true), (to + 1, false)] {
                let r = at(i);
                assert_eq!(r.defeat, defeated, "{facing:?}, frame {n}, I = {i}");
                assert!(!r.events.contains(&Event::Hurt), "{facing:?} {n} {i}");
            }
        }
    }
}

#[test]
fn classic_2_walkers_hurt_in_the_stop_after_a_roll_by_the_roll_range() {
    // "Being hurt around a roll (`title-2`)": D is no attack frame; on D
    // to D + 13 a walker within the defeat range hurts, and on D + 14 the
    // standing range applies.
    let (w, inputs) = lone_roll(Facing::Right, &[]);
    let plain = play(&mut w.clone(), &inputs);
    let d = 31;
    let x = plain[d].x;
    for n in [d, d + 7, d + 13] {
        for (i, hurt) in [(-26, false), (-25, true), (27, true), (28, false)] {
            let r = with_walker_at(w.clone(), &inputs, n, x, i);
            assert!(!r.defeat, "frame {n}, I = {i}");
            assert_eq!(r.events.contains(&Event::Hurt), hurt, "frame {n}, I = {i}");
        }
    }
    for (i, hurt) in [(-24, false), (-23, true), (30, true), (31, false)] {
        let r = with_walker_at(w.clone(), &inputs, d + 14, x, i);
        assert_eq!(r.events.contains(&Event::Hurt), hurt, "D + 14, I = {i}");
    }
}

#[test]
fn classic_2_walkers_use_the_running_ranges_once_a_roll_carries_on() {
    // A roll with Action held all through carries on into running on D,
    // which isn't an attack frame: the walking or running ranges apply.
    let (w, inputs) = lone_roll(Facing::Right, &[Action::Action]);
    let plain = play(&mut w.clone(), &inputs);
    let d = 31;
    assert_eq!(plain[d].events, [Event::RollEnd]);
    assert_eq!(plain[d].state, State::Run);
    let x = plain[d].x;
    for (i, hurt) in [(-29, false), (-28, true), (29, true), (30, false)] {
        let r = with_walker_at(w.clone(), &inputs, d, x, i);
        assert!(!r.defeat, "I = {i}");
        assert_eq!(r.events.contains(&Event::Hurt), hurt, "I = {i}");
    }
}
