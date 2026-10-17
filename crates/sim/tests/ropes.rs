//! Ropes on `classic-1` (spec/behavior/ropes.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Event, Facing, Fixed, InputFrame, State, World, generic_feel, record, step};
use clamber_trace::{Decimal, Scenario};

use Action::{Action as A, Down as D, Jump as J, Left as L, Right as R, Up as U};

/// The floor's top.
const FLOOR: i32 = 300;

/// A rope's bottom end in these tests: the feet line reaches it 24 px
/// lower, at 274, a few frames into a jump from the floor.
const END: i32 = 250;

/// Flat ground at y = 300 from x = 0 to 2000, the player at `start`, and
/// a rope at each of `ropes`, with its bottom end at [`END`].
fn level(start: (f64, f64), ropes: &[i32]) -> Level {
    let mut src = format!(
        r#"
        format = 1
        size = [2000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, {FLOOR}, 2000, 300]
        "#,
        start.0, start.1
    );
    for x in ropes {
        src.push_str(&format!(
            "\n[[entities]]\ntype = \"rope\"\nat = [{x}, {END}]\n"
        ));
    }
    Level::from_toml_str(&src, "test").unwrap()
}

/// A world on [`level`].
fn world(start: (f64, f64), ropes: &[i32]) -> World {
    World::new(
        Arc::new(level(start, ropes)),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// `n`/256 px.
fn px(n: i32) -> Fixed {
    Fixed::from_ratio_pow2(n, 8)
}

/// What a test reads after each frame.
#[derive(Clone, Debug)]
struct Row {
    x: Fixed,
    y: Fixed,
    vx: Fixed,
    vy: Fixed,
    state: State,
    facing: Facing,
    /// The rope held, by its thing.
    rope: Option<usize>,
    /// Each rope's `x`.
    ropes: Vec<Fixed>,
    /// The view's top, rounded down.
    view: i64,
    events: Vec<Event>,
}

impl Row {
    /// The feet line: `y` rounded up.
    fn feet(&self) -> i64 {
        -(-self.y).floor()
    }
}

fn row(w: &World) -> Row {
    let p = &w.player;
    Row {
        x: p.pos.x,
        y: p.pos.y,
        vx: p.vel.x,
        vy: p.vel.y,
        state: p.state,
        facing: p.facing,
        rope: p.rope.map(|on| on.thing),
        ropes: w.things.iter().map(|t| t.pos.x).collect(),
        view: w.view.map_or(0, |v| v.y()),
        events: w.events.clone(),
    }
}

/// Plays `frames` frames holding what `held` says on each, and returns a
/// row for each.
fn play(w: &mut World, frames: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    let mut last = Actions::NONE;
    (0..frames)
        .map(|k| {
            let now: Actions = held(k).into_iter().collect();
            step(w, &InputFrame::single(Frame::after(last, now)));
            last = now;
            row(w)
        })
        .collect()
}

/// The first frame holding a rope, or that a rope was caught on, at or
/// after `from`.
fn caught(rows: &[Row], from: usize) -> Option<usize> {
    (from..rows.len()).find(|&k| rows[k].rope.is_some())
}

/// Plays as [`play`] does, but stops after the first frame from `from`
/// on that the player holds a rope or caught one.
fn play_until_caught(w: &mut World, from: usize, held: impl Fn(usize) -> Vec<Action>) -> Vec<Row> {
    let mut last = Actions::NONE;
    let mut rows = Vec::new();
    for k in 0..200 {
        let now: Actions = held(k).into_iter().collect();
        step(w, &InputFrame::single(Frame::after(last, now)));
        last = now;
        rows.push(row(w));
        if k >= from && w.player.rope.is_some() {
            return rows;
        }
    }
    panic!("not caught");
}

/// A world with a rope at 500 and the player standing under it at 505,
/// which jumps with Jump held and catches the rope. Returns the world and
/// the catch frame's row.
fn holding() -> (World, Row) {
    let mut w = world((505.0, 300.0), &[500]);
    let rows = play_until_caught(&mut w, 0, |_| vec![J]);
    (w, rows[rows.len() - 1].clone())
}

/// The speed down after a frame holding a rope at `vy`, climbing toward
/// `target` (1/256 px a frame, up positive).
fn climbed(vy: Fixed, target: i32) -> Fixed {
    let u = (-vy.to_f64() * 256.0) as i32;
    let step = (target - u) / 8;
    -px(if step == 0 { target } else { u + step })
}

#[test]
fn a_jump_catches_a_rope_with_the_feet_line_within_24_px_of_its_end() {
    let mut w = world((505.0, 300.0), &[500]);
    let rows = play(&mut w, 30, |_| vec![J]);
    let c = caught(&rows, 0).expect("caught");
    let before = &rows[c - 1];
    assert!(before.feet() <= i64::from(END) + 24, "in reach after c - 1");
    assert!(rows[c - 2].feet() > i64::from(END) + 24, "not after c - 2");
    let r = &rows[c];
    assert_eq!(r.state, State::Hold);
    assert_eq!((r.vx, r.vy), (Fixed::ZERO, Fixed::ZERO), "stopped");
    // It kept the position it moved to on the catch frame.
    assert_eq!(r.y, before.y + before.vy + fx(0.28125));
    assert_eq!(r.x, before.x);
}

#[test]
fn walking_off_a_ledge_never_catches_a_rope() {
    // A ledge at 290 ending at x = 500 and a rope just past it whose end
    // is level with the ledge.
    let src = format!(
        r#"
        format = 1
        size = [2000, 600]
        start = [470, 290]

        [[collision]]
        shape = "rect"
        rect = [0, 290, 500, 10]

        [[collision]]
        shape = "rect"
        rect = [0, {FLOOR}, 2000, 300]

        [[entities]]
        type = "rope"
        at = [506, 280]
        "#
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    let mut w = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    let rows = play(&mut w, 60, |_| vec![R]);
    assert!(rows.iter().any(|r| r.events.contains(&Event::LeaveGround)));
    assert_eq!(caught(&rows, 0), None);
}

#[test]
fn reach_across_is_12_px_slow_and_4_px_at_a_quarter_px_a_frame() {
    // Standing jumps from either side of the rope, near the limit.
    let mut seen = [false; 2];
    for dx in [
        9.0, 10.0, 11.0, 12.0, 13.0, 14.0, -10.0, -11.0, -12.0, -13.0,
    ] {
        let mut w = world((500.5 + dx, 300.0), &[500]);
        // The rope sways too: read the gap where it counts.
        let rows = play(&mut w, 12, |_| vec![J]);
        let k = (1..rows.len())
            .find(|&k| rows[k - 1].feet() <= i64::from(END) + 24)
            .unwrap();
        let gap = rows[k - 1].x.floor() - rows[k - 1].ropes[0].floor();
        let catches = gap.abs() < 12;
        assert_eq!(rows[k].rope.is_some(), catches, "{dx}: gap {gap}");
        seen[usize::from(catches)] = true;
    }
    assert_eq!(seen, [true, true]);
}

#[test]
fn a_running_jump_catches_only_within_4_px() {
    // Running right at 3 px a frame into a rope, jumping on the way:
    // whatever frame it's caught on, the gap after the frame before was
    // under 4, and on every earlier frame in the air it wasn't.
    for start in 300..312 {
        let mut w = world((f64::from(start), 300.0), &[500]);
        let rows = play(&mut w, 120, |k| {
            if k >= 50 { vec![R, A, J] } else { vec![R, A] }
        });
        let Some(c) = caught(&rows, 50) else {
            continue;
        };
        let gap = |k: usize| (rows[k].x.floor() - rows[k].ropes[0].floor()).abs();
        assert!(rows[c - 1].vx.abs() >= fx(0.25));
        assert!(gap(c - 1) < 4, "{start}: gap {}", gap(c - 1));
        for k in 51..c - 1 {
            let reach = rows[k].feet() <= i64::from(END) + 24 && gap(k) < 4;
            assert!(
                !reach || rows[k + 1].events.contains(&Event::Land),
                "{start}: {k}"
            );
        }
    }
}

#[test]
fn holding_follows_the_ropes_whole_px_and_keeps_the_fraction() {
    let (mut w, at) = holding();
    let fraction = at.x - Fixed::from_int(at.x.floor() as i32);
    let mut last_rope = at.ropes[0];
    for r in play(&mut w, 60, |_| vec![]) {
        assert_eq!(r.state, State::Hold);
        assert_eq!(
            r.x.floor(),
            last_rope.floor(),
            "the rope after the frame before"
        );
        assert_eq!(r.x - Fixed::from_int(r.x.floor() as i32), fraction);
        assert_eq!(r.vx, Fixed::ZERO);
        last_rope = r.ropes[0];
    }
}

/// Speeds up from standing still toward `target` (1/256 px a frame), by
/// an eighth of the gap rounded toward zero.
fn climb(target: i32, frames: usize) -> Vec<Fixed> {
    let mut u = 0;
    (0..frames)
        .map(|_| {
            let step = (target - u) / 8;
            u = if step == 0 { target } else { u + step };
            -px(u)
        })
        .collect()
}

#[test]
fn climbing_speeds() {
    for (held, target) in [
        (vec![D], -640),
        (vec![D, A], -896),
        (vec![U], 384),
        (vec![U, A], 640),
        (vec![J], 384),
        (vec![J, A], 640),
        (vec![D, J], -640),
        (vec![D, U], -640),
        (vec![A], 0),
    ] {
        let (mut w, at) = holding();
        // Let go of Jump first, so that holding it counts as no press.
        let frames = 6;
        let rows = play(
            &mut w,
            frames,
            |k| if k == 0 { vec![] } else { held.clone() },
        );
        let want = climb(target, frames - 1);
        assert_eq!(rows[0].y, at.y, "nothing held: still");
        for (k, r) in rows[1..].iter().enumerate() {
            assert_eq!(r.vy, want[k], "{held:?}, frame {k}");
            assert_eq!(r.y, rows[k].y + r.vy, "moved by vy");
            assert_eq!(r.state, State::Hold);
        }
    }
}

#[test]
fn climbing_up_stops_41_px_below_the_view() {
    let (mut w, _) = holding();
    let rows = play(&mut w, 200, |_| vec![U]);
    for k in 1..rows.len() {
        assert_eq!(rows[k].state, State::Hold);
        assert!(
            rows[k].feet() >= rows[k - 1].view + 41 - 2,
            "{k}: feet {} under a view at {}",
            rows[k].feet(),
            rows[k - 1].view
        );
    }
    // It ends up held there, and it has climbed well above the ground.
    let last = &rows[rows.len() - 1];
    assert!((39..=40).contains(&(last.feet() - last.view)), "{last:?}");
    assert!(last.feet() < 200);
}

#[test]
fn holding_the_way_faced_turns_round_the_rope() {
    let (mut w, at) = holding();
    assert_eq!(at.facing, Facing::Right);
    // Right from frame 1 (frame 0 lets go of Jump): a turn on frames 1 to
    // 8, facing left from frame 5. Right then is the other way; Left
    // held from frame 7 starts the next turn on 9, facing right on 13.
    let rows = play(&mut w, 20, |k| match k {
        0 => vec![],
        1..=6 => vec![R],
        _ => vec![L],
    });
    let facing: Vec<Facing> = rows.iter().map(|r| r.facing).collect();
    assert!(
        facing[..5].iter().all(|&f| f == Facing::Right),
        "{facing:?}"
    );
    assert!(
        facing[5..13].iter().all(|&f| f == Facing::Left),
        "{facing:?}"
    );
    assert_eq!(facing[13], Facing::Right, "{facing:?}");
    assert!(rows[5].events.contains(&Event::Turn));
    assert!(rows.iter().all(|r| r.state == State::Hold));
}

#[test]
fn jumping_off_example() {
    // ropes.md, "Jumping off", "Example", mirrored: facing right with vy
    // 0, Jump and Right pressed on P and held.
    let (mut w, at) = holding();
    let rows = play(&mut w, 8, |k| if k < 4 { vec![] } else { vec![J, R] });
    let p = 4;
    let rope_x = rows[p - 1].ropes[0].floor();
    assert_eq!(rows[p - 1].vy, Fixed::ZERO);
    assert!(rows[p].events.contains(&Event::JumpStart));
    assert_eq!(rows[p].x.floor(), rope_x, "x is the rope's on P");
    assert_eq!((rows[p].vx, rows[p].vy), (Fixed::ZERO, fx(-6.125)));
    assert_eq!(rows[p].y, rows[p - 1].y - fx(6.125));
    assert_eq!(rows[p].rope, None);
    for (k, (vy, vx)) in [
        (fx(-5.84375), fx(0.375)),
        (fx(-5.5625), fx(0.703125)),
        (fx(-5.28125), fx(0.98828125)),
    ]
    .into_iter()
    .enumerate()
    {
        let r = &rows[p + 1 + k];
        assert_eq!((r.vy, r.vx), (vy, vx), "P + {}", k + 1);
        assert_eq!(r.y, rows[p + k].y + vy);
        assert_eq!(r.x, rows[p + k].x + vx);
    }
    assert_eq!(at.facing, Facing::Right);
}

#[test]
fn jump_off_speeds_depend_on_up_down_and_action_only() {
    for (held, vy) in [
        (vec![], -6.125),
        (vec![U], -6.3125),
        (vec![U, A], -6.4375),
        (vec![D], -5.8125),
        (vec![D, A], -5.6875),
    ] {
        // Climbing up fast first doesn't count.
        let (mut w, _) = holding();
        let rows = play(&mut w, 6, |k| match k {
            0 => vec![],
            1..=4 => vec![U, A],
            _ => [vec![J, L], held.clone()].concat(),
        });
        assert!(rows[4].vy < Fixed::ZERO);
        assert_eq!(rows[5].vy, fx(vy), "{held:?}");
        assert_eq!(rows[5].facing, Facing::Left, "facing the way held");
        assert_eq!(rows[5].y, rows[4].y + fx(vy));
    }
}

#[test]
fn after_jumping_off_the_air_targets_are_actions() {
    // Without Action, the target is still 3 px a frame.
    let (mut w, _) = holding();
    let rows = play(&mut w, 80, |k| match k {
        0..=3 => vec![],
        4 => vec![J, L],
        _ => vec![L],
    });
    let land = rows
        .iter()
        .position(|r| r.events.contains(&Event::Land))
        .unwrap();
    // Past Left's 2 px a frame, on the way to 3.
    assert_eq!(rows[5].vx, fx(-0.375));
    assert!(rows[land - 1].vx < fx(-2.9), "{:?}", rows[land - 1]);
    // Back on the ground it's forgotten.
    let rows = play(&mut w, 30, |_| vec![L]);
    assert_eq!(rows[29].vx, fx(-2.0));
}

#[test]
fn sliding_off_the_end_falls_facing_the_same_way() {
    let (mut w, _) = holding();
    let rows = play(&mut w, 60, |k| if k == 0 { vec![] } else { vec![D, L] });
    let off = rows.iter().position(|r| r.rope.is_none()).expect("let go");
    let end = i64::from(END) + 24;
    assert!(
        rows[off - 1].feet() > end,
        "past the end after the frame before"
    );
    assert!(rows[off - 2].feet() <= end);
    // The let-go frame still climbs down.
    assert_eq!(rows[off].vy, climbed(rows[off - 1].vy, -640));
    assert_eq!(rows[off].y, rows[off - 1].y + rows[off].vy);
    assert_eq!(rows[off].state, State::Fall);
    assert_eq!(rows[off].vx, Fixed::ZERO);
    let land = (off..rows.len())
        .find(|&k| rows[k].events.contains(&Event::Land))
        .expect("landed");
    for k in off + 1..land {
        let want = (rows[k - 1].vy + fx(0.4375)).min(fx(8.0));
        if !rows[k].events.contains(&Event::Land) {
            assert_eq!(rows[k].vy, want, "{k}");
        }
        assert_eq!(rows[k].facing, Facing::Right, "facing kept: {k}");
        assert_eq!(rows[k].rope, None, "a fall catches no rope");
    }
    // Left held at 2 px a frame, from P + 1.
    assert_eq!(rows[off + 1].vx, fx(-0.25));
}

#[test]
fn the_rope_last_held_is_caught_again_only_after_a_frame_out_of_reach() {
    // Jump off to the left and keep going left: it passes in reach of
    // the rope it left without catching it.
    let (mut w, _) = holding();
    let rows = play(&mut w, 12, |k| match k {
        0..=3 => vec![],
        4 => vec![J, L],
        _ => vec![J, L],
    });
    let gap = |r: &Row| (r.x.floor() - r.ropes[0].floor()).abs();
    assert!(gap(&rows[5]) < 4, "in reach after P + 1");
    assert!(rows[5..].iter().all(|r| r.rope.is_none()));
    // Then come back right while still up: caught again by the same rope.
    let rows = play(&mut w, 30, |_| vec![J, R]);
    let c = caught(&rows, 0).expect("caught again");
    assert_eq!(rows[c].rope, Some(0));
}

#[test]
fn another_rope_is_caught_at_once_and_a_turn_starts_facing_the_way_held() {
    // Ropes at 500 and 530: jump off the first to the right, holding
    // Right, and catch the second still facing right.
    let mut w = world((505.0, 300.0), &[500, 530]);
    let rows = play_until_caught(&mut w, 0, |_| vec![J]);
    assert_eq!(rows[rows.len() - 1].rope, Some(0));
    let rows = play_until_caught(&mut w, 5, |k| match k {
        0..=3 => vec![],
        _ => vec![J, R],
    });
    let c = &rows[rows.len() - 1];
    assert_eq!(c.rope, Some(1));
    assert_eq!(c.facing, Facing::Right);
    // The turn started on the catch frame: facing changes 4 frames later
    // with nothing held.
    let rows = play(&mut w, 6, |_| vec![]);
    let facing: Vec<Facing> = rows.iter().map(|r| r.facing).collect();
    assert_eq!(
        facing,
        [
            Facing::Right,
            Facing::Right,
            Facing::Right,
            Facing::Left,
            Facing::Left,
            Facing::Left
        ]
    );
}

#[test]
fn ropes_sway_about_12_px_in_all() {
    let mut w = world((100.0, 300.0), &[500]);
    let rows = play(&mut w, 600, |_| vec![]);
    let xs: Vec<f64> = rows.iter().map(|r| r.ropes[0].to_f64()).collect();
    let (lo, hi) = xs[200..]
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    assert!((494.0..495.0).contains(&lo), "{lo}");
    assert!((506.0..507.0).contains(&hi), "{hi}");
}

#[test]
fn in_a_trace_example() {
    // ropes.md, "In a trace", "Example": caught on C facing right with
    // nothing held, Up on C + 1 and C + 2, Jump pressed with Left on
    // C + 3, nothing on C + 4.
    let c = {
        let mut w = world((505.0, 300.0), &[500]);
        play_until_caught(&mut w, 0, |_| vec![J]).len() as u32 - 1
    };
    let scenario = Scenario::parse(
        &format!(
            "# clamber-scenario v1\n\
             id: t\nsummary: s\nrequires: r\nstart: s\nframes: {}\n\
             [inputs]\n0-{} J\n{}-{} U\n{} L\n{} J\n",
            c + 5,
            c - 1,
            c + 1,
            c + 2,
            c + 3,
            c + 3,
        ),
        "test",
    )
    .unwrap();
    let trace = record(
        Arc::new(level((505.0, 300.0), &[500])),
        Arc::new(generic_feel("classic-1").unwrap()),
        &scenario,
    )
    .unwrap();
    let rows = &trace.rows[c as usize..];
    use clamber_trace::trace::Facing::{Left, Right};
    let d = |v: f64| Decimal::parse(&v.to_string()).unwrap();
    let want = [
        (0.0, 0.0, "rope", Right, &["rope_catch"][..]),
        (-0.1875, -0.1875, "rope", Right, &[]),
        (-0.5390625, -0.3515625, "rope", Right, &[]),
        (-6.6640625, -6.125, "jump", Left, &["jump_start", "turn"]),
        (-12.3515625, -5.6875, "jump", Left, &["jump_release"]),
    ];
    for (k, (y, vy, state, facing, events)) in want.into_iter().enumerate() {
        let r = &rows[k];
        let at = format!("C + {k}: {r:?}");
        assert_eq!(r.y - rows[0].y, d(y), "{at}");
        assert_eq!((r.vx, r.vy), (d(0.0), d(vy)), "{at}");
        assert_eq!(r.state.name(), state, "{at}");
        assert!(!r.ground, "{at}");
        assert_eq!(r.facing, facing, "{at}");
        let mut names: Vec<&str> = r.events.iter().map(|e| e.name()).collect();
        names.sort();
        assert_eq!(names, events, "{at}");
    }
}

#[test]
fn sliding_off_the_end_in_a_trace() {
    // ropes.md, "In a trace": the let-go frame is a `fall` row with
    // `rope_slide_off`, the rows up to landing `fall` with no events.
    let (mut w, at) = holding();
    assert_eq!(at.state.name(), "rope");
    assert_eq!(at.events, [Event::RopeCatch]);
    let rows = play(&mut w, 60, |k| if k == 0 { vec![] } else { vec![D] });
    let off = rows.iter().position(|r| r.rope.is_none()).expect("let go");
    for r in &rows[..off] {
        assert_eq!((r.state.name(), &r.events[..]), ("rope", &[][..]));
    }
    assert_eq!(rows[off].state.name(), "fall");
    assert_eq!(rows[off].events, [Event::RopeSlideOff]);
    let land = (off..rows.len())
        .find(|&k| rows[k].events.contains(&Event::Land))
        .expect("landed");
    for r in &rows[off + 1..land] {
        assert_eq!((r.state.name(), &r.events[..]), ("fall", &[][..]));
    }
}
