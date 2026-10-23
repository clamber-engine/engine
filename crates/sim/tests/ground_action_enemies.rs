//! The ground action knocking out walkers on `classic-1`
//! (spec/behavior/ground-action-enemies.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{
    Event, Facing, Fixed, FlowEvent, InputFrame, Phase, State, World, generic_feel, step,
};

use Action::{Action as A, Down as D, Right as R};

/// Walkers as walker-contact.md's, with the sizes the classic-1 boxes go
/// with.
const CLASSIC_WALKER: &str = "response = 0.125, turn_delay = 4, \
    defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8";
const SMALL: &str = "size = [20, 12], offset = 0.5";
const LARGE: &str = "size = [26, 38], offset = 0.5";

/// Flat ground at y = 400 from x = 0 to 2000, the player standing at
/// `x` facing `facing`, and a walker of `size` heading `heading` at full
/// speed for each of `walkers`, out of reach to start with.
fn world(x: i32, facing: Facing, size: &str, heading: &str, walkers: usize) -> World {
    let walkers: String = (0..walkers)
        .map(|n| {
            format!(
                r#"
                [[entities]]
                type = "crawler"
                at = [{}, 400]
                params = {{ speed = 1, facing = "{heading}", {size}, {CLASSIC_WALKER} }}
                "#,
                x + 100 + 30 * n as i32
            )
        })
        .collect();
    world_with(x, facing, &walkers)
}

/// Flat ground at y = 400 from x = 0 to 2000 with `entities`, and the
/// player standing at `x` facing `facing`.
fn world_with(x: i32, facing: Facing, entities: &str) -> World {
    let src = format!(
        r#"
        format = 1
        size = [2000, 600]
        start = [{x}, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 2000, 200]

        {entities}
        "#
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    let mut w = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    w.player.facing = facing;
    w
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// One frame of play.
#[derive(Debug)]
struct Row {
    x: Fixed,
    vx: Fixed,
    state: State,
    hurt: bool,
    /// The walkers knocked out on this frame.
    struck: Vec<usize>,
    /// Each walker's `x`, `y` and its `vy` if it's falling.
    walkers: Vec<(Fixed, Fixed, Option<Fixed>)>,
    /// Whether each walker is still there.
    present: Vec<bool>,
    view: Option<i64>,
}

/// Plays the ground action from frame P = 0, pressed with `on_p` held
/// and nothing held after. Before frame `k` of each `(k, n, x)` in
/// `put`, walker `n` is put at `x`, so `x` is where it is at the end of
/// frame `k - 1`. Returns `frames` rows from P.
fn act(w: &mut World, on_p: &[Action], put: &[(usize, usize, f64)], frames: usize) -> Vec<Row> {
    // Settle on the ground first.
    for _ in 0..4 {
        step(w, &InputFrame::default());
    }
    assert!(w.player.grounded());
    let mut last = Actions::NONE;
    (0..frames)
        .map(|k| {
            for &(at, n, x) in put {
                if at == k {
                    w.things[n].pos.x = fx(x);
                }
            }
            let now: Actions = if k == 0 {
                on_p.iter().copied().collect()
            } else {
                Actions::NONE
            };
            step(w, &InputFrame::single(Frame::after(last, now)));
            last = now;
            Row {
                x: w.player.pos.x,
                vx: w.player.vel.x,
                state: w.player.state,
                hurt: w.events.contains(&Event::Hurt),
                struck: w
                    .flow
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        FlowEvent::Defeat(n) => Some(*n),
                        _ => None,
                    })
                    .collect(),
                walkers: w
                    .things
                    .iter()
                    .map(|t| {
                        let vy = match t.phase {
                            Phase::Falling { vy, .. } => Some(vy),
                            _ => None,
                        };
                        (t.pos.x, t.pos.y, vy)
                    })
                    .collect(),
                present: w.things.iter().map(|t| t.active).collect(),
                view: w.view.map(|v| v.x()),
            }
        })
        .collect()
}

/// The frames of `rows` on which something was knocked out.
fn struck(rows: &[Row]) -> Vec<usize> {
    (0..rows.len())
        .filter(|&k| !rows[k].struck.is_empty())
        .collect()
}

/// The frames of `rows` in the ground action.
fn acting(rows: &[Row]) -> Vec<usize> {
    (0..rows.len())
        .filter(|&k| rows[k].state == State::GroundAction)
        .collect()
}

#[test]
fn the_example() {
    let mut w = world(100, Facing::Right, SMALL, "left", 1);
    let rows = act(&mut w, &[D, A], &[(0, 0, 180.25)], 64);
    assert_eq!(struck(&rows), [17]);
    let floor = Fixed::from_int(400);
    let walker = |k: usize| {
        let (x, y, vy) = rows[k].walkers[0];
        (x, y - floor, vy.unwrap_or(Fixed::ZERO))
    };
    let table = [
        (0, 179.25, 0.0, 0.0),
        (16, 163.25, 0.0, 0.0),
        (17, 162.25, 0.0, -6.0),
        (18, 162.25, 0.0, -6.0),
        (19, 162.25, 0.0, -6.0),
        (20, 161.25, -5.5625, -5.5625),
        (21, 160.25, -10.6875, -5.125),
        (27, 154.25, -32.25, -2.5),
        (33, 148.25, -38.0625, 0.125),
    ];
    for (k, x, y, vy) in table {
        assert_eq!(walker(k), (fx(x), fx(y), fx(vy)), "frame {k}");
    }
    for row in &rows {
        assert_eq!(row.x, Fixed::from_int(100));
        assert!(!row.hurt);
    }
    // E = P + 61, two frames later than without the walker.
    assert_eq!(acting(&rows), (0..=60).collect::<Vec<_>>());
    assert_eq!(rows[61].state, State::Idle);
}

/// Whether a walker of `size` heading `heading` at full speed is knocked
/// out on the first attack frame, P + 17, by a player standing facing
/// `facing` with direction 0, when I is `i` at the end of that frame.
fn knocks_out_at(size: &str, facing: Facing, heading: &str, i: i32) -> bool {
    let p = 300;
    let mut w = world(p, facing, size, heading, 1);
    let end = f64::from(p - i) + 0.5;
    let before = match heading {
        "left" => end + 1.0,
        _ => end - 1.0,
    };
    let rows = act(&mut w, &[D, A], &[(17, 0, before)], 18);
    assert_eq!(rows[17].walkers[0].0, fx(end), "the walker moves 1 px");
    assert!(struck(&rows[..17]).is_empty());
    !rows[17].struck.is_empty()
}

#[test]
fn knock_out_ranges() {
    use Facing::{Left, Right};
    let table = [
        (SMALL, Right, "left", -62, 10),
        (SMALL, Right, "right", -63, 9),
        (SMALL, Left, "left", -9, 63),
        (SMALL, Left, "right", -10, 62),
        (LARGE, Right, "left", -65, 13),
        (LARGE, Right, "right", -66, 12),
        (LARGE, Left, "left", -12, 66),
        (LARGE, Left, "right", -13, 65),
    ];
    for (n, (size, facing, heading, low, high)) in table.into_iter().enumerate() {
        for (i, out) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                knocks_out_at(size, facing, heading, i),
                out,
                "row {n}: I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

#[test]
fn only_one_walker_an_action_and_one_pause() {
    // A walker heading left reaches I = -62 at the end of P + 16 (not an
    // attack frame), then sits in range for the next attack frame, P + 17.
    // A second reaches I = -62 at the end of P + 27: the second attack
    // frame, t = 25, moved 2 frames later by the pause. It isn't knocked
    // out, and there is no second pause.
    let mut w = world(300, Facing::Right, SMALL, "left", 2);
    let rows = act(&mut w, &[D, A], &[(16, 0, 363.5), (27, 1, 363.5)], 70);
    assert_eq!(struck(&rows), [17]);
    assert_eq!(rows[17].struck, [0]);
    // On pause frames neither walker moves, the falling one included.
    for k in [18, 19] {
        assert_eq!(rows[k].walkers, rows[17].walkers, "frame {k}");
    }
    assert_ne!(rows[20].walkers[0], rows[19].walkers[0]);
    for k in 28..=29 {
        assert_ne!(rows[k].walkers[1], rows[k - 1].walkers[1], "frame {k}");
    }
}

#[test]
fn the_third_attack_frame() {
    let mut w = world(300, Facing::Right, SMALL, "left", 1);
    // I = -63 at the end of P + 30, -62 at the end of P + 31.
    let rows = act(&mut w, &[D, A], &[(30, 0, 364.5)], 64);
    assert_eq!(struck(&rows), [31]);
    assert_eq!(acting(&rows), (0..=60).collect::<Vec<_>>());
}

#[test]
fn the_pause_holds_x_and_vx_moving() {
    let mut w = world(300, Facing::Right, SMALL, "left", 1);
    // Going right, the player has moved less than 50 px by the end of
    // P + 17, so I is within the range then.
    let rows = act(&mut w, &[D, A, R], &[(17, 0, 361.5)], 64);
    assert!(rows[17].x > Fixed::from_int(310) && rows[17].x < Fixed::from_int(350));
    assert_eq!(struck(&rows), [17]);
    for k in [18, 19] {
        assert_eq!((rows[k].x, rows[k].vx), (rows[17].x, rows[17].vx));
        assert_eq!(rows[k].state, State::GroundAction);
    }
    assert_ne!(rows[20].x, rows[17].x);
    // The view holds on K and K + 1 and goes on from K + 2.
    assert_eq!(rows[17].view, rows[16].view);
    assert_eq!(rows[18].view, rows[17].view);
    assert_ne!(rows[19].view, rows[18].view);
}

#[test]
fn hurt_outside_the_range_on_an_attack_frame() {
    let mut w = world(300, Facing::Right, SMALL, "left", 1);
    // I = 15 at the end of P + 17: in the hurt range, not the knock-out
    // range, so hurt on P + 18.
    let rows = act(&mut w, &[D, A], &[(17, 0, 286.5)], 20);
    assert!(struck(&rows).is_empty());
    let hurts: Vec<_> = (0..rows.len()).filter(|&k| rows[k].hurt).collect();
    assert_eq!(hurts, [18]);
}

#[test]
fn not_hurt_until_the_fourth_frame_after_a_knock_out() {
    // Walker 0 is knocked out on P + 17. Walker 1, heading left away from
    // the player, is in the hurt range from the end of P + 17 on (I = 15,
    // then 16 after the pause): the hurt comes on K + 4 = P + 21.
    let mut w = world(300, Facing::Right, SMALL, "left", 2);
    let rows = act(&mut w, &[D, A], &[(17, 0, 351.5), (17, 1, 286.5)], 24);
    assert_eq!(struck(&rows), [17]);
    let hurts: Vec<_> = (0..rows.len()).filter(|&k| rows[k].hurt).collect();
    assert_eq!(hurts, [21]);
}

#[test]
fn the_thrower_is_knocked_out_from_its_ledge() {
    // A thrower 1 px above the ground, as on its ledge in thrower.md.
    let thrower = "[[entities]]\ntype = \"thrower\"\nat = [400, 399]\n";
    for i in [-51, -44, -37] {
        let mut w = world_with(400 + i, Facing::Right, thrower);
        let rows = act(&mut w, &[D, A], &[], 24);
        assert_eq!(struck(&rows), [17], "I = {i}");
        assert!(!rows.iter().any(|r| r.hurt));
        // It doesn't move on K, K + 1 and K + 2, then rises at 5 px per
        // frame, slowing by 7/16.
        let y = |k: usize| rows[k].walkers[0].1 - Fixed::from_int(399);
        for k in 16..=19 {
            assert_eq!(y(k), Fixed::ZERO, "I = {i}, frame {k}");
        }
        assert_eq!(y(20), fx(-4.5625), "I = {i}");
        assert_eq!(y(21), fx(-8.6875), "I = {i}");
        assert_eq!(rows[20].walkers[0].0, Fixed::from_int(400));
        assert_eq!(acting(&rows).len(), 24);
    }
}

#[test]
fn a_small_walker_on_flat_floor_is_there_for_the_last_time_on_k_plus_41() {
    let mut w = world(100, Facing::Right, SMALL, "left", 1);
    let rows = act(&mut w, &[D, A], &[(0, 0, 180.25)], 64);
    assert_eq!(struck(&rows), [17]);
    assert!(rows.iter().all(|r| r.view == rows[0].view));
    let there: Vec<_> = (0..rows.len()).filter(|&k| rows[k].present[0]).collect();
    assert_eq!(there, (0..=58).collect::<Vec<_>>());
    // 95 px below the floor it walked on.
    assert_eq!(rows[58].walkers[0].1.floor(), 495);
}

#[test]
fn a_walker_put_outside_the_window_on_k_to_k_plus_2_is_removed_on_k_plus_3() {
    for k in 17..=19 {
        let mut w = world(100, Facing::Right, SMALL, "left", 1);
        // Far right of the window from the end of frame `k` on.
        let rows = act(&mut w, &[D, A], &[(0, 0, 180.25), (k + 1, 0, 1500.0)], 24);
        assert_eq!(struck(&rows), [17]);
        let there: Vec<_> = (0..rows.len()).filter(|&f| rows[f].present[0]).collect();
        assert_eq!(there, (0..=19).collect::<Vec<_>>(), "put on {k}");
    }
}

/// The player's state after each of `frames` frames of the ground action
/// from P = 0 knocking out the example's walker on K = P + 17, then
/// `keys` held from K + 1, with `tap` pressed every `every` frames. With
/// `on_player`, the walker is put on the player's position before every
/// frame from K + 1.
fn played(
    keys: &[Action],
    (tap, every): (Action, usize),
    on_player: bool,
    frames: usize,
) -> Vec<String> {
    let mut w = world(100, Facing::Right, SMALL, "left", 1);
    for _ in 0..4 {
        step(&mut w, &InputFrame::default());
    }
    w.things[0].pos.x = fx(180.25);
    let mut last = Actions::NONE;
    (0..frames)
        .map(|k| {
            if on_player && k > 17 && w.things[0].active {
                w.things[0].pos = w.player.pos;
            }
            let mut now: Actions = if k == 0 {
                [D, A].into_iter().collect()
            } else if k > 17 {
                keys.iter().copied().collect()
            } else {
                Actions::NONE
            };
            if k > 17 && every > 0 && k % every == 0 {
                now = now | tap.into();
            }
            step(&mut w, &InputFrame::single(Frame::after(last, now)));
            last = now;
            if k == 17 {
                assert_eq!(
                    w.things[0].phase,
                    Phase::Falling {
                        vy: fx(-6.0),
                        hold: 0,
                        glide: None
                    }
                );
            }
            format!(
                "{k}: {:?} {:?} {:?} {:?} {:?}",
                w.player.state, w.player.pos, w.player.vel, w.player.knockout, w.events
            )
        })
        .collect()
}

#[test]
fn a_knocked_out_walker_never_touches_the_player() {
    use Action::{Jump as J, Left as L};
    // Nothing held, walking, running, rolling and jumping.
    let inputs: [(&[Action], (Action, usize)); 5] = [
        (&[], (J, 0)),
        (&[R], (J, 0)),
        (&[L, A], (J, 0)),
        (&[R], (A, 5)),
        (&[L], (J, 7)),
    ];
    for (keys, tap) in inputs {
        let left = played(keys, tap, false, 59);
        let put = played(keys, tap, true, 59);
        assert_eq!(left, put, "{keys:?}, {tap:?}");
    }
    // As a control, a walker still walking put on the player the same
    // way hurts it.
    let mut w = world(100, Facing::Right, SMALL, "left", 1);
    let rows = act(&mut w, &[D, A], &[(0, 0, 180.25)], 18);
    assert_eq!(struck(&rows), [17]);
    w.things[0].phase = Phase::Ready;
    let hurt = (0..8).any(|_| {
        w.things[0].pos = w.player.pos;
        step(&mut w, &InputFrame::default());
        w.events.contains(&Event::Hurt)
    });
    assert!(hurt);
}

#[test]
fn two_walkers_in_reach_knock_out_one_whichever_way_they_and_the_player_face() {
    // "Two things in reach": at most one of two small walkers, always the
    // same one of the pair, even exchanged, facing either way and heading
    // either way.
    for (facing, heading) in [
        (Facing::Left, "left"),
        (Facing::Left, "right"),
        (Facing::Right, "left"),
        (Facing::Right, "right"),
    ] {
        let sign = if facing == Facing::Left { -1.0 } else { 1.0 };
        let mut both = 0;
        for a in (10..=60_i32).step_by(10) {
            for b in (10..=60).step_by(10) {
                let mut chosen = Vec::new();
                for (first, second) in [(a, b), (b, a)] {
                    let mut w = world(300, facing, SMALL, heading, 2);
                    let put = [
                        (0, 0, 300.0 + sign * f64::from(first)),
                        (0, 1, 300.0 + sign * f64::from(second)),
                    ];
                    let rows = act(&mut w, &[D, A], &put, 70);
                    let hit: Vec<usize> = rows.iter().flat_map(|r| r.struck.clone()).collect();
                    assert!(hit.len() <= 1, "{facing:?} {heading} {first} {second}");
                    chosen.push(hit);
                }
                // Level with each other they are in range together: the same
                // one is struck, always the first placed.
                if a == b && chosen[0].len() == 1 && chosen[1].len() == 1 {
                    both += 1;
                    assert_eq!(chosen[0], chosen[1], "{facing:?} {heading} {a} {b}");
                }
            }
        }
        assert!(both > 0, "{facing:?} {heading}: nothing in reach");
    }
}
