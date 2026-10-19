//! Bunch and letter pickup ranges during the ground action, turns, the
//! frames after a roll, falls without a jump and bounces in Classic 1
//! (spec/behavior/bunches.md and spec/behavior/letters.md, "Pickup
//! ranges").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Event, Facing, FlowEvent, InputFrame, World, generic_feel, step};

use Action::{Action as Act, Down, Jump, Left, Right};

const F: i32 = 400;

const STAND: [i32; 4] = [-15, 21, -8, 40];
const ROLL: [i32; 4] = [-13, 21, -4, 38];
const AIR: [i32; 4] = [-15, 18, 13, 57];
const BOUNCE: [i32; 4] = [-17, 27, -7, 39];
const NONE: [i32; 4] = [1, 0, 1, 0];

/// Classic 1 on flat ground at `F` up to x = 700 and a floor 176 px
/// lower beyond it, the player at (`start`, `F`), and a `kind` at each
/// of `at`.
fn ledge_world(kind: &str, start: i32, at: &[(i32, i32)]) -> World {
    walker_world(kind, start, "", at)
}

/// `ledge_world`, with a crawler at each of `walkers`: `(x, facing)`.
fn walker_world(kind: &str, start: i32, walkers: &str, at: &[(i32, i32)]) -> World {
    let lower = F + 176;
    let mut src = format!(
        r#"
        format = 1
        name = "ledge"
        size = [3000, 800]
        start = [{start}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 700, 400]

        [[collision]]
        shape = "rect"
        rect = [700, {lower}, 2300, 200]
        "#
    );
    src += walkers;
    for (x, y) in at {
        src += &format!("\n[[entities]]\ntype = \"{kind}\"\nat = [{x}, {y}]\n");
    }
    let level = Arc::new(Level::from_toml_str(&src, "ledge").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

/// What a step left behind.
#[derive(Clone, Debug)]
struct Row {
    x: i32,
    feet: i32,
    facing: Facing,
    ground: bool,
    turn: Option<u8>,
    ground_action: bool,
    events: Vec<Event>,
}

/// Plays `keys` from `world`: each step's row, and the first step on
/// which something was taken.
fn play(mut world: World, keys: &[Vec<Action>]) -> (Vec<Row>, Option<usize>) {
    let mut held = Actions::NONE;
    let mut rows = Vec::new();
    let mut taken = None;
    for (n, k) in keys.iter().enumerate() {
        let now: Actions = k.iter().copied().collect();
        step(&mut world, &InputFrame::single(Frame::after(held, now)));
        held = now;
        let p = &world.player;
        rows.push(Row {
            x: p.pos.x.floor() as i32,
            feet: -((-p.pos.y).floor() as i32),
            facing: p.facing,
            ground: p.grounded(),
            turn: p.turn,
            ground_action: p.ground_action.is_some(),
            events: world.events.clone(),
        });
        let took = world
            .flow
            .events
            .iter()
            .any(|e| matches!(e, FlowEvent::Collect(_)));
        if took && taken.is_none() {
            taken = Some(n);
        }
    }
    (rows, taken)
}

fn first(rows: &[Row], event: Event) -> usize {
    rows.iter()
        .position(|r| r.events.contains(&event))
        .unwrap_or_else(|| panic!("no {event:?}"))
}

/// For a `kind` placed around the player's place on each of the steps
/// `anchors`, checks that playing `keys` from `start` takes it on the
/// first step whose ranges, by `ranges`, hold it, `I` measured the way
/// the player faces on that step.
fn check(
    kind: &str,
    start: i32,
    keys: &[Vec<Action>],
    anchors: &[usize],
    ranges: impl Fn(&[Row], usize) -> [i32; 4],
) {
    check_in(|at| ledge_world(kind, start, at), keys, anchors, ranges);
}

/// `check`, in the worlds `world` makes with the thing at each place.
fn check_in(
    world: impl Fn(&[(i32, i32)]) -> World,
    keys: &[Vec<Action>],
    anchors: &[usize],
    ranges: impl Fn(&[Row], usize) -> [i32; 4],
) {
    let (rows, _) = play(world(&[]), keys);
    for &a in anchors {
        let (xa, fa) = (rows[a].x, rows[a].feet);
        for past in -30..30 {
            for below in [-9, -8, -7, -6, -5, -4, 12, 13, 38, 39, 40, 41] {
                let at = (xa - past, fa - below);
                let want = rows.iter().enumerate().position(|(n, row)| {
                    let [i0, i1, d0, d1] = ranges(&rows, n);
                    let i = match row.facing {
                        Facing::Right => row.x - at.0,
                        Facing::Left => at.0 - row.x,
                    };
                    let d = row.feet - at.1;
                    (i0..=i1).contains(&i) && (d0..=d1).contains(&d)
                });
                let (_, got) = play(world(&[at]), keys);
                assert_eq!(
                    got, want,
                    "{past} px left and {below} px above the place on step {a}"
                );
            }
        }
    }
}

fn keys(parts: &[(&[Action], usize)]) -> Vec<Vec<Action>> {
    parts
        .iter()
        .flat_map(|&(k, n)| std::iter::repeat_n(k.to_vec(), n))
        .collect()
}

/// The ground action's steps, from its press step `G` to `G` + 58.
fn ground_action_steps(rows: &[Row]) -> (usize, usize) {
    let g = rows.iter().position(|r| r.ground_action).unwrap();
    let last = g + rows[g..].iter().take_while(|r| r.ground_action).count() - 1;
    assert_eq!(last, g + 58, "the action lasts 59 steps");
    (g, last)
}

#[test]
fn the_ground_action_has_the_standing_ranges() {
    for kind in ["cluster", "letter"] {
        // Down held through the action, which isn't crouching.
        let k = keys(&[
            (&[], 4),
            (&[Down, Act, Right], 1),
            (&[Down, Right], 58),
            (&[], 20),
        ]);
        let (rows, _) = play(ledge_world(kind, 300, &[]), &k);
        let (g, last) = ground_action_steps(&rows);
        check(kind, 300, &k, &[g, g + 20, last], |_, _| STAND);
        // Against the facing: facing right, direction left.
        let k = keys(&[(&[], 4), (&[Down, Act, Left], 1), (&[Left], 58), (&[], 20)]);
        let (rows, _) = play(ledge_world(kind, 400, &[]), &k);
        let (g, last) = ground_action_steps(&rows);
        assert!(rows[last].facing == Facing::Right && rows[last].x < rows[g].x);
        check(kind, 400, &k, &[g + 10, last], |_, _| STAND);
    }
}

#[test]
fn the_ground_action_has_the_standing_ranges_in_the_air_after_a_ledge() {
    for kind in ["cluster", "letter"] {
        let k = keys(&[(&[], 4), (&[Down, Act, Right], 1), (&[], 90)]);
        let (rows, _) = play(ledge_world(kind, 600, &[]), &k);
        let (g, last) = ground_action_steps(&rows);
        let off = first(&rows, Event::LeaveGround);
        assert!(off > g && off < last && !rows[last].ground);
        check(kind, 600, &k, &[off, off + 8, last], |_, _| STAND);
    }
}

/// Checks a turn: the standing ranges on every step, `I` in the old
/// facing on `U` to `U` + 3 and the new one from `U` + 4.
fn check_turn(kind: &str, start: i32, k: &[Vec<Action>]) {
    let (rows, _) = play(ledge_world(kind, start, &[]), k);
    let u = rows.iter().position(|r| r.turn.is_some()).unwrap();
    let old = rows[u].facing;
    assert!(rows[u..u + 4].iter().all(|r| r.facing == old));
    assert!(rows[u + 4..u + 8].iter().all(|r| r.facing != old));
    check(kind, start, k, &[u, u + 3, u + 4, u + 8], |_, _| STAND);
}

#[test]
fn a_turn_has_the_standing_ranges_in_each_steps_facing() {
    for kind in ["cluster", "letter"] {
        // From standing, walking and running (after a roll, as pressing
        // Action rolls).
        check_turn(kind, 300, &keys(&[(&[], 4), (&[Left], 30), (&[], 10)]));
        check_turn(
            kind,
            300,
            &keys(&[(&[Right], 20), (&[Left], 30), (&[], 10)]),
        );
        check_turn(
            kind,
            100,
            &keys(&[(&[Right, Act], 50), (&[Left, Act], 30), (&[], 10)]),
        );
    }
}

#[test]
fn a_new_turn_on_u_plus_8_uses_the_facing_it_starts_in() {
    for kind in ["cluster", "letter"] {
        let k = keys(&[(&[Right], 20), (&[Left], 8), (&[Right], 30), (&[], 10)]);
        let (rows, _) = play(ledge_world(kind, 300, &[]), &k);
        let u = rows.iter().position(|r| r.turn.is_some()).unwrap();
        assert_eq!(rows[u + 8].turn, Some(0), "a new turn on U + 8");
        assert!(rows[u + 8..u + 12].iter().all(|r| r.facing == Facing::Left));
        assert!(rows[u + 12].facing == Facing::Right);
        check(kind, 300, &k, &[u + 8, u + 11, u + 12], |_, _| STAND);
    }
}

#[test]
fn a_stop_after_a_roll_keeps_the_rolling_ranges_to_r_plus_8() {
    for kind in ["cluster", "letter"] {
        for k in [
            keys(&[(&[], 4), (&[Act], 1), (&[], 70)]),
            keys(&[(&[], 4), (&[Act, Right], 1), (&[Right], 1), (&[], 70)]),
            // Turning during the stop.
            keys(&[(&[], 4), (&[Act], 1), (&[], 29), (&[Left], 40)]),
        ] {
            let (rows, _) = play(ledge_world(kind, 300, &[]), &k);
            let s = first(&rows, Event::RollStart);
            let r = first(&rows, Event::RollEnd);
            assert_eq!(r, s + 29, "a stop");
            check(kind, 300, &k, &[r - 1, r + 8, r + 9, r + 14], |_, n| {
                if (s..=r + 8).contains(&n) {
                    ROLL
                } else {
                    STAND
                }
            });
        }
    }
}

#[test]
fn a_roll_running_on_keeps_the_rolling_ranges_to_r_plus_2() {
    for kind in ["cluster", "letter"] {
        let k = keys(&[(&[], 4), (&[Act, Right], 70)]);
        let (rows, _) = play(ledge_world(kind, 100, &[]), &k);
        let s = first(&rows, Event::RollStart);
        let end = first(&rows, Event::RollEnd);
        let r = end - 3;
        assert_eq!(r, s + 29, "a roll running on ends on R + 3");
        check(kind, 100, &k, &[r - 1, r + 2, r + 3, r + 6], |_, n| {
            if (s..=r + 2).contains(&n) {
                ROLL
            } else {
                STAND
            }
        });
    }
}

#[test]
fn a_fall_without_a_jump_has_the_standing_ranges() {
    for kind in ["cluster", "letter"] {
        // Walking with Right held, then let go in the air, running (on
        // from a roll), and walking then turning left in the air.
        for (start, k) in [
            (600, keys(&[(&[Right], 100)])),
            (650, keys(&[(&[Right], 40), (&[], 60)])),
            (400, keys(&[(&[Right, Act], 160)])),
            (650, keys(&[(&[Right], 40), (&[Left], 60)])),
        ] {
            let (rows, _) = play(ledge_world(kind, start, &[]), &k);
            let off = first(&rows, Event::LeaveGround);
            let land = first(&rows, Event::Land);
            // The keys change in the air.
            assert!(k[off] == k[0] && land > off + 10);
            check(
                kind,
                start,
                &k,
                &[off, off + 8, land - 2, land, land + 3],
                |_, _| STAND,
            );
        }
    }
}

/// A crawler at `x` on the floor, facing `facing`.
fn crawler(x: i32, facing: &str) -> String {
    format!(
        "\n[[entities]]\ntype = \"crawler\"\nat = [{x}, {F}]\nparams = {{ facing = \"{facing}\" }}\n"
    )
}

/// The first wait before `keys(wait)` that bounces off a walker, and the
/// rows played.
fn find_bounce(
    start: i32,
    walkers: &str,
    keys: impl Fn(usize) -> Vec<Vec<Action>>,
) -> (usize, Vec<Row>) {
    (0..100)
        .find_map(|wait| {
            let (rows, _) = play(walker_world("cluster", start, walkers, &[]), &keys(wait));
            rows.iter()
                .any(|r| r.events.contains(&Event::Bounce))
                .then_some((wait, rows))
        })
        .expect("a bounce")
}

/// Checks a bunch around a bounce off a walker with `keys` (bunches.md,
/// "Bouncing off a walker"): the jump's ranges up to `B` - 1, the
/// bounce's on `B` and from `B` + 3 until landing, none on the frozen
/// `B` + 1 and `B` + 2, and the standing ones from the landing.
fn check_bounce(start: i32, walkers: &str, keys: &[Vec<Action>]) {
    let (rows, _) = play(walker_world("cluster", start, walkers, &[]), keys);
    let t = first(&rows, Event::JumpStart);
    let b = first(&rows, Event::Bounce);
    let land = b + rows[b..]
        .iter()
        .position(|r| r.events.contains(&Event::Land))
        .expect("a landing");
    assert!(b > t + 6 && land > b + 6, "T {t}, B {b}, L {land}");
    assert!(rows[b + 1].x == rows[b].x && rows[b + 2].feet == rows[b].feet);
    check_in(
        |at| walker_world("cluster", start, walkers, at),
        keys,
        &[b - 2, b, b + 1, b + 2, b + 3, land - 1, land, land + 3],
        |_, n| match n {
            n if n < t + 6 => STAND,
            n if n < b => AIR,
            n if n == b + 1 || n == b + 2 => NONE,
            n if n < land => BOUNCE,
            _ => STAND,
        },
    );
}

#[test]
fn a_bounce_off_a_walker_has_its_own_ranges_and_none_while_frozen() {
    let jump = |held: usize| move |wait: usize| keys(&[(&[], wait), (&[Jump], held), (&[], 120)]);
    let ahead = crawler(360, "left");
    // Jump held through it, and a short jump.
    for held in [120, 3] {
        let (wait, _) = find_bounce(300, &ahead, jump(held));
        check_bounce(300, &ahead, &jump(held)(wait));
    }
    // Jump let go of on B + 3.
    let (wait, rows) = find_bounce(300, &ahead, jump(120));
    let b = first(&rows, Event::Bounce);
    let k = keys(&[(&[], wait), (&[Jump], b + 3 - wait), (&[], 120)]);
    check_bounce(300, &ahead, &k);
    // Moving right through it and landing walking, and slowing with Left
    // from B + 3 to land in a turn.
    let right = |wait: usize| keys(&[(&[Right], wait), (&[Right, Jump], 20), (&[Right], 120)]);
    let (wait, rows) = find_bounce(200, &ahead, right);
    check_bounce(200, &ahead, &right(wait));
    let b = first(&rows, Event::Bounce);
    let k = keys(&[
        (&[Right], wait),
        (&[Right, Jump], b + 3 - wait),
        (&[Left], 120),
    ]);
    check_bounce(200, &ahead, &k);
    // Facing left, onto a walker coming from the left.
    let behind = crawler(240, "right");
    let left = |wait: usize| keys(&[(&[Left], 1), (&[], wait), (&[Jump], 20), (&[], 120)]);
    let (wait, rows) = find_bounce(300, &behind, left);
    let b = first(&rows, Event::Bounce);
    assert!(rows[b - 2..].iter().all(|r| r.facing == Facing::Left));
    check_bounce(300, &behind, &left(wait));
}
