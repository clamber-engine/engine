//! Classic 2: walkers (spec/behavior/walker.md, "In `title-2`"), when
//! they touch the player (walker-contact.md, "In `title-2`") and bouncing
//! off them (bounce.md, "In `title-2`").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    Behavior, Event, Facing, Fixed, InputFrame, Phase, Stance, State, World, generic_feel, stance,
    step,
};

/// The values `title-2` walkers have, with the contact ranges of
/// walker-contact.md, "In `title-2`". The size only draws them.
const WALKER: &str = "size = [20, 18], offset = 0, response = [0.0625, 0.015625], \
    move_first = true, columns = true, turn_delay = 0, bounced_speed = 1, \
    defeat_jump = 8, defeat_gravity = 0.4375, defeat_max_fall = 12, \
    contact = { stand = [-23, 30, -36, 20], crouch = [-23, 30, -22, 22], \
    walk = [-25, 28, -36, 20], run = [-28, 29, -32, 17], air = [-26, 29, -28, 21], \
    turn = 7, land_hold = [17, 8], bounce_rise = 1.25, bounce_above = -10 }";

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
