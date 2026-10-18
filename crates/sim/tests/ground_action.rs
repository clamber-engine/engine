//! The ground action on `classic-1` (spec/behavior/ground-action.md) and
//! the view's shake during it (spec/behavior/view-vertical.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Presets};
use clamber_sim::{Event, Facing, Fixed, InputFrame, State, World, generic_feel, step};

use Action::{Action as A, Down as D, Jump as J, Left as L, Right as R};

/// Flat ground at y = 600 from x = 0 to 4000.
const FLAT: &str = r#"
    format = 1
    size = [4400, 800]
    start = [1000, 600]

    [[collision]]
    shape = "rect"
    rect = [0, 600, 4000, 200]
"#;

fn world() -> World {
    let level = Level::from_toml_str(FLAT, "flat").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One frame of play and where it left the player.
#[derive(Debug)]
struct Row {
    state: State,
    events: Vec<Event>,
    x: f64,
    vx: f64,
    vy: f64,
    facing: Facing,
    grounded: bool,
}

/// Plays `n` frames, holding `held(frame)` on each, carrying on from what
/// `last` says was held before.
fn play(
    w: &mut World,
    last: &mut Actions,
    n: usize,
    held: impl Fn(usize) -> Vec<Action>,
) -> Vec<Row> {
    (0..n)
        .map(|frame| {
            let now: Actions = held(frame).into_iter().collect();
            step(w, &InputFrame::single(Frame::after(*last, now)));
            *last = now;
            let p = &w.player;
            Row {
                state: p.state,
                events: w.events.clone(),
                x: p.pos.x.to_f64(),
                vx: p.vel.x.to_f64(),
                vy: p.vel.y.to_f64(),
                facing: p.facing,
                grounded: p.grounded(),
            }
        })
        .collect()
}

/// The frames of `rows` in the ground action.
fn acting(rows: &[Row]) -> Vec<usize> {
    (0..rows.len())
        .filter(|&i| rows[i].state == State::GroundAction)
        .collect()
}

/// Plays `before` frames holding `lead`, then the ground action from
/// frame P = `before` holding `on_p` on P and `then(k)` on P + k. Returns
/// the rows from P on and `x` on P - 1.
fn from(
    lead: &[Action],
    before: usize,
    on_p: &[Action],
    then: impl Fn(usize) -> Vec<Action>,
    n: usize,
) -> (Vec<Row>, f64) {
    from_after(&[(lead, before)], on_p, then, n)
}

/// [`from`], holding each of `lead`'s keys for its number of frames in
/// turn before P.
fn from_after(
    lead: &[(&[Action], usize)],
    on_p: &[Action],
    then: impl Fn(usize) -> Vec<Action>,
    n: usize,
) -> (Vec<Row>, f64) {
    let mut w = world();
    let mut last = Actions::NONE;
    for &(keys, before) in lead {
        play(&mut w, &mut last, before, |_| keys.to_vec());
    }
    let x0 = w.player.pos.x.to_f64();
    let rows = play(&mut w, &mut last, n, |k| {
        if k == 0 { on_p.to_vec() } else { then(k) }
    });
    (rows, x0)
}

#[test]
fn speed_and_position_going_right() {
    // "Speed and position", the example: the same from any starting
    // speed.
    let standing: &[(&[Action], usize)] = &[(&[], 10)];
    let walking: &[(&[Action], usize)] = &[(&[R], 40)];
    // Action is let go for a frame, so that it's pressed on P.
    let running: &[(&[Action], usize)] = &[(&[A, R], 100), (&[R], 1)];
    for lead in [standing, walking, running] {
        let (rows, x0) = from_after(lead, &[D, A, R], |_| vec![], 62);
        for (k, vx, dx) in [
            (0, 0.375, 0.375),
            (1, 0.703125, 1.078125),
            (2, 0.98828125, 2.06640625),
            (3, 1.23828125, 3.3046875),
            (4, 1.45703125, 4.76171875),
            (9, 2.203125, 14.48046875),
            (19, 2.78125, 40.3125),
            (29, 2.93359375, 69.1328125),
            (38, 2.97265625, 95.74609375),
            (39, 3.0, 98.74609375),
            (58, 3.0, 155.74609375),
            (59, 3.0, 158.74609375),
        ] {
            assert_eq!((rows[k].vx, rows[k].x - x0), (vx, dx), "{lead:?} P + {k}");
        }
        assert_eq!(acting(&rows), (0..59).collect::<Vec<_>>(), "{lead:?}");
        assert!(rows[..60].iter().all(|r| r.facing == Facing::Right));
        assert!(rows[..60].iter().all(|r| r.grounded && r.vy == 0.0));
        assert!(
            rows[..60]
                .iter()
                .all(|r| !r.events.contains(&Event::RollStart))
        );
    }
}

#[test]
fn direction_comes_only_from_what_is_held_on_the_press_frame() {
    // Walking right and letting go of Right on P: direction 0.
    let (rows, x0) = from(&[R], 40, &[D, A], |_| vec![R], 60);
    assert!(rows.iter().all(|r| r.vx == 0.0 && r.x == x0));
    // Both held: direction 0.
    let (rows, x0) = from(&[R], 40, &[D, A, L, R], |_| vec![], 60);
    assert!(rows.iter().all(|r| r.vx == 0.0 && r.x == x0));
    // Left held on P while facing right: it goes left, still facing
    // right, whatever is held after.
    let (rows, _) = from(&[], 10, &[D, A, L], |_| vec![R], 60);
    assert_eq!(rows[0].vx, -0.375);
    assert_eq!(rows[59].vx, -3.0);
    assert!(rows.iter().all(|r| r.facing == Facing::Right));
}

#[test]
fn how_it_ends() {
    // "How it ends", the examples: direction +1, held from E on.
    for (held, e1, e2) in [
        (&[R][..], 2.875, 2.765625),
        (&[R, D][..], 2.875, 2.515625),
        (&[L][..], 2.375, 1.828125),
        (&[][..], 2.625, 2.296875),
    ] {
        let (rows, _) = from(
            &[],
            10,
            &[D, A, R],
            |k| {
                if k < 59 { vec![] } else { held.to_vec() }
            },
            62,
        );
        assert_eq!(rows[59].vx, 3.0, "{held:?}");
        assert_eq!((rows[60].vx, rows[61].vx), (e1, e2), "{held:?}");
    }
}

#[test]
fn trace_rows_example() {
    // "Trace rows": walking right from x = 0 with Right held, Down and
    // Action pressed on frame 30 and held to 33.
    let mut w = world();
    let x0 = w.player.pos.x.to_f64();
    let mut last = Actions::NONE;
    let rows = play(&mut w, &mut last, 91, |f| {
        if (30..=33).contains(&f) {
            vec![R, D, A]
        } else {
            vec![R]
        }
    });
    for (f, x, vx, state) in [
        (29, 45.99609375, 1.94921875, State::Walk),
        (30, 46.37109375, 0.375, State::GroundAction),
        (31, 47.07421875, 0.703125, State::GroundAction),
        (88, 201.7421875, 3.0, State::GroundAction),
        (89, 204.7421875, 3.0, State::Walk),
        (90, 207.6171875, 2.875, State::Walk),
    ] {
        let r = &rows[f];
        assert_eq!((r.x - x0, r.vx, r.state), (x, vx, state), "frame {f}");
        assert!(r.events.is_empty(), "frame {f}");
    }
}

#[test]
fn which_presses_start_it() {
    // Down pressed together with Action, or held from before, including
    // while crouching.
    for lead in [&[][..], &[D][..]] {
        let (rows, _) = from(lead, 30, &[D, A], |_| vec![], 2);
        assert_eq!(rows[0].state, State::GroundAction, "{lead:?}");
    }
    // Down pressed after Action: a roll, which Down doesn't change.
    let (rows, _) = from(&[], 10, &[A], |_| vec![A, D], 3);
    assert_eq!(rows[0].events, [Event::RollStart]);
    assert!(rows.iter().all(|r| r.state == State::Roll));
    // Action held, Down pressed later (here after a roll that carried on
    // into running): no ground action.
    let (rows, _) = from(&[A, R], 80, &[A, D], |_| vec![A, D], 30);
    assert!(acting(&rows).is_empty());
    assert!(rows.iter().all(|r| r.events.is_empty()));
}

#[test]
fn presses_in_the_air() {
    // Jumping from standing with jump held; the landing frame L.
    let mut w = world();
    let mut last = Actions::NONE;
    let jump = play(
        &mut w,
        &mut last,
        80,
        |f| if f < 40 { vec![J] } else { vec![] },
    );
    let land = jump
        .iter()
        .position(|r| r.events.contains(&Event::Land))
        .unwrap();
    let start = jump
        .iter()
        .position(|r| r.events.contains(&Event::JumpStart))
        .unwrap();
    let air = land - start;
    let try_press = |press: usize, down: bool| -> Vec<Row> {
        let mut w = world();
        let mut last = Actions::NONE;
        play(&mut w, &mut last, land + 10, |f| {
            let mut keys = if f < 40 { vec![J] } else { vec![] };
            if f >= press && f <= land + 2 {
                keys.push(A);
            }
            if down && f > start + 1 {
                keys.push(D);
            }
            keys
        })
    };
    assert!(air > 6);
    // A press remembered through landing rolls on L + 1, Down or not.
    let rows = try_press(land - 3, true);
    assert_eq!(rows[land + 1].events, [Event::RollStart]);
    assert!(acting(&rows).is_empty());
    // A press on L + 1 with Down held starts the ground action.
    let rows = try_press(land + 1, true);
    assert_eq!(acting(&rows)[0], land + 1);
}

#[test]
fn the_second_character_rolls() {
    // With the second character under control, Down makes no difference.
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: feel.roster[0].abilities.clone(),
    });
    let level = Level::from_toml_str(FLAT, "flat").unwrap();
    let mut w = World::new(Arc::new(level), Arc::new(feel));
    w.player.character = 1;
    let mut last = Actions::NONE;
    play(&mut w, &mut last, 10, |_| vec![]);
    let rows = play(&mut w, &mut last, 2, |_| vec![D, A]);
    assert_eq!(rows[0].events, [Event::RollStart]);
    assert_eq!(rows[1].state, State::Roll);
}

#[test]
fn inputs_during_it() {
    // Action presses, with or without Down, do nothing, on E too. Up,
    // Down and directions do nothing either.
    let (rows, _) = from(
        &[],
        10,
        &[D, A, R],
        |k| match k {
            k if k % 2 == 0 && k < 59 => vec![D, A, L],
            59 => vec![A, D],
            60 => vec![A, D],
            _ => vec![Action::Up],
        },
        62,
    );
    assert_eq!(acting(&rows), (0..59).collect::<Vec<_>>());
    assert!(rows[..60].iter().all(|r| r.facing == Facing::Right));
    assert_eq!(rows[59].vx, 3.0);
    assert!(rows.iter().all(|r| r.events.is_empty()));

    // A new press on E + 1 starts a new one.
    let (rows, _) = from(
        &[],
        10,
        &[D, A, R],
        |k| if k == 60 { vec![D, A] } else { vec![] },
        62,
    );
    assert_eq!(rows[59].state, State::Walk);
    assert_eq!(rows[60].state, State::GroundAction);
    assert_eq!(rows[60].vx, 0.0);
    // Without Down, a roll.
    let (rows, _) = from(
        &[],
        10,
        &[D, A, R],
        |k| if k == 60 { vec![A] } else { vec![] },
        62,
    );
    assert_eq!(rows[60].events, [Event::RollStart]);
}

#[test]
fn a_jump_ends_it() {
    // The example: walking right, the press on P with Right held, Jump
    // pressed on P + 20 and Right held throughout.
    let (rows, _) = from(
        &[R],
        40,
        &[D, A, R],
        |k| if k == 20 { vec![J, R] } else { vec![R] },
        23,
    );
    assert_eq!(acting(&rows), (0..20).collect::<Vec<_>>());
    assert_eq!((rows[20].vx, rows[20].grounded), (2.80859375, true));
    assert!(rows[20].events.is_empty());
    assert_eq!(rows[21].events, [Event::JumpStart]);
    assert_eq!(rows[21].vx, 2.7109375);
    assert!(!rows[22].grounded);

    // A press on E jumps too.
    let (rows, _) = from(
        &[],
        10,
        &[D, A, R],
        |k| if k == 59 { vec![J] } else { vec![] },
        61,
    );
    assert_eq!(rows[60].events, [Event::JumpStart]);
}

#[test]
fn going_off_a_ledge_doesnt_end_it() {
    // "At a ledge without a face": it goes on in the air.
    let mut w = world();
    w.player.pos.x = Fixed::from_int(3950);
    assert!(w.player.grounded());
    let mut last = Actions::NONE;
    let rows = play(&mut w, &mut last, 59, |_| vec![D, A, R]);
    let off = rows.iter().position(|r| !r.grounded).unwrap();
    assert!(off < 58);
    assert_eq!(acting(&rows), (0..59).collect::<Vec<_>>());
}

/// The view's top and smoothed gap on each frame of a ground action
/// in place, from the press frame on.
fn shaking(frames: usize) -> Vec<(Fixed, Fixed)> {
    let mut w = world();
    let mut last = Actions::NONE;
    play(&mut w, &mut last, 10, |_| vec![]);
    (0..frames)
        .map(|f| {
            play(&mut w, &mut last, 1, |_| {
                if f == 0 { vec![D, A] } else { vec![] }
            });
            let view = w.view.unwrap();
            (view.top, view.gap)
        })
        .collect()
}

#[test]
fn the_view_shakes() {
    // spec/behavior/view-vertical.md, "Following the target" and "The
    // ground action's shake", for a player standing still on the ground
    // with the view settled: the target doesn't move, and the offsets
    // (H = P + 26 here) are added after step 5 without changing Q.
    let rows = shaking(140);
    let target = rows[0].0;
    assert_eq!(rows[..26], vec![(target, Fixed::ZERO); 26]);
    let (mut v, mut q) = (target.to_bits() >> 24, 0i64);
    let mut offsets = Vec::new();
    for (k, &(top, gap)) in rows.iter().enumerate() {
        let mut d = (target.to_bits() >> 24) - v;
        if (0..=31).contains(&d) {
            d = 0;
        }
        q += (d - q).div_euclid(16);
        v += q.div_euclid(64);
        if k >= 26 && (k - 26) % 4 == 0 && (k - 26) / 4 < 22 {
            let n = (k - 26) / 4;
            let size = 1024 >> (n / 2);
            let offset = if n % 2 == 0 { size } else { -size };
            v += offset;
            offsets.push(offset);
        }
        assert_eq!(
            (top.to_bits() >> 24, gap.to_bits() >> 24),
            (v, q),
            "P + {k}"
        );
    }
    assert_eq!(offsets.len(), 22);
    assert_eq!(&offsets[..6], [1024, -1024, 512, -512, 256, -256]);
    assert_eq!(&offsets[20..], [1, -1]);
}

#[test]
fn no_shake_without_a_ground_action() {
    let mut w = world();
    let mut last = Actions::NONE;
    let settled = w.view.unwrap();
    play(&mut w, &mut last, 120, |f| {
        if f == 10 { vec![A] } else { vec![] }
    });
    assert_eq!(w.view.unwrap().top, settled.top);
}
