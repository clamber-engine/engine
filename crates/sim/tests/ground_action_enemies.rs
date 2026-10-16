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
    let entities: String = (0..walkers)
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
fn only_on_attack_frames_and_each_pause_lengthens_the_action() {
    // A walker heading left reaches I = -62 at the end of P + 16 (not an
    // attack frame), then sits in range for the next attack frame, P + 17.
    // A second reaches I = -62 at the end of P + 27: the second attack
    // frame, t = 25, moved 2 frames later by the first pause.
    let mut w = world(300, Facing::Right, SMALL, "left", 2);
    let rows = act(&mut w, &[D, A], &[(16, 0, 363.5), (27, 1, 363.5)], 70);
    assert_eq!(struck(&rows), [17, 27]);
    assert_eq!(rows[17].struck, [0]);
    assert_eq!(rows[27].struck, [1]);
    // E = P + 63.
    assert_eq!(acting(&rows), (0..=62).collect::<Vec<_>>());
    assert_eq!(rows[63].state, State::Idle);
    // On pause frames neither walker moves, the falling one included.
    for k in [28, 29] {
        assert_eq!(rows[k].walkers, rows[27].walkers, "frame {k}");
    }
    assert_ne!(rows[30].walkers[0], rows[29].walkers[0]);
    assert_ne!(rows[30].walkers[1], rows[29].walkers[1]);
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
