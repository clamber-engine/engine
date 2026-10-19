//! Classic 1's player carrying an object among walkers: the carrying
//! pose's hurt ranges (spec/behavior/walker-contact.md, "Carrying") and
//! the carried object defeating walkers (spec/behavior/thrown-objects.md,
//! "A carried object"), also during a pick-up, a throw or a put-down,
//! with the object where carrying.md ("The object in the hands") puts
//! it.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{
    Act, Event, Facing, Fixed, FlowEvent, Hands, InputFrame, Phase, World, generic_feel, step,
};

const CLASSIC_WALKER: &str = "response = 0.125, turn_delay = 4, \
    defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8";

/// Small, medium and large walkers as in walker-contact.md, with the
/// sizes and offsets the classic-1 boxes go with.
const SMALL: &str = "size = [20, 12], offset = 0.5";
const MEDIUM: &str = "size = [25, 25], offset = 5";
const LARGE: &str = "size = [26, 38], offset = 0.5";

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat floor at y = 400, the player at `player_x` facing `facing` and
/// carrying a crate, and a walker of `kind` at `walker_x` with `speed`
/// heading `heading`. The walker is thing 0, the crate thing 1.
fn carrying(player_x: f64, facing: Facing, kind: &str, walker: (f64, f64, &str)) -> World {
    let (walker_x, speed, heading) = walker;
    let src = format!(
        r#"
        format = 1
        name = "carried"
        size = [1000, 600]
        start = [{}, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 1000, 200]

        [[entities]]
        type = "crawler"
        at = [{walker_x}, 400]
        params = {{ speed = {speed}, facing = "{heading}", {kind}, {CLASSIC_WALKER} }}

        [[entities]]
        type = "crate"
        at = [900, 400]
        "#,
        player_x.floor()
    );
    let level = Level::from_toml_str(&src, "carried").unwrap();
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    world.player.pos.x = fx(player_x);
    world.player.facing = facing;
    world.player.hands = Some(Hands {
        thing: 1,
        act: Act::Carry,
        frame: 30,
        press: None,
    });
    world.things[1].phase = Phase::Held;
    world
}

/// One step with Action held.
fn carry_step(world: &mut World) {
    let held: Actions = Action::Action.into();
    step(world, &InputFrame::single(Frame::after(held, held)));
}

/// Whether a carrying player standing `i` px from a still walker of
/// `kind` heading `heading` is hurt by it on the second frame and not the
/// first. The crate is taken out so it can't break on the walker first.
fn carrying_hurt_at(kind: &str, i: i32, facing: Facing, heading: &str) -> bool {
    let mut w = carrying(f64::from(300 + i), facing, kind, (300.0, 0.0, heading));
    w.things[1].active = false;
    carry_step(&mut w);
    assert!(!w.events.contains(&Event::Hurt), "hurt on the first frame");
    carry_step(&mut w);
    w.events.contains(&Event::Hurt)
}

#[test]
fn carrying_narrows_the_hurt_range() {
    // walker-contact.md, "Carrying": facing right the low end is 1 px
    // higher and the high end 5 px lower; facing left 5 px higher and
    // 1 px lower. Upright ranges from the table under "Being hurt".
    let table = [
        (SMALL, Facing::Right, "left", -19, 24),
        (SMALL, Facing::Right, "right", -18, 25),
        (SMALL, Facing::Left, "left", -25, 18),
        (SMALL, Facing::Left, "right", -24, 19),
        (MEDIUM, Facing::Right, "left", -26, 22),
        (MEDIUM, Facing::Right, "right", -16, 32),
        (MEDIUM, Facing::Left, "left", -32, 16),
        (MEDIUM, Facing::Left, "right", -22, 26),
        (LARGE, Facing::Right, "left", -22, 27),
        (LARGE, Facing::Right, "right", -21, 28),
        (LARGE, Facing::Left, "left", -28, 21),
        (LARGE, Facing::Left, "right", -27, 22),
    ];
    for (n, (kind, facing, heading, low, high)) in table.into_iter().enumerate() {
        let (low, high) = match facing {
            Facing::Right => (low + 1, high - 5),
            Facing::Left => (low + 5, high - 1),
        };
        for (i, hurt) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                carrying_hurt_at(kind, i, facing, heading),
                hurt,
                "row {n}: I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

#[test]
fn the_spec_examples_of_carrying_hurt_ranges() {
    // Facing right, a small walker heading left: −18 to 19. Facing left,
    // a medium walker heading right: −17 to 25; a large one heading left:
    // −23 to 20.
    for (kind, facing, heading, low, high) in [
        (SMALL, Facing::Right, "left", -18, 19),
        (MEDIUM, Facing::Left, "right", -17, 25),
        (LARGE, Facing::Left, "left", -23, 20),
    ] {
        assert!(!carrying_hurt_at(kind, low - 1, facing, heading));
        assert!(carrying_hurt_at(kind, low, facing, heading));
        assert!(carrying_hurt_at(kind, high, facing, heading));
        assert!(!carrying_hurt_at(kind, high + 1, facing, heading));
    }
}

/// Whether the crate a player carries at `object_x` breaks on a still
/// large walker at 300 facing `heading` on the first step.
fn breaks_on_large_walker(object_x: i32, facing: Facing, heading: &str) -> bool {
    let mut w = carrying(f64::from(object_x), facing, LARGE, (300.0, 0.0, heading));
    carry_step(&mut w);
    let broke = !w.things[1].active;
    assert_eq!(broke, w.player.hands.is_none());
    assert_eq!(broke, w.flow.events.contains(&FlowEvent::Defeat(0)));
    broke
}

#[test]
fn a_carried_object_breaks_on_a_large_walker_in_the_measured_ranges() {
    // thrown-objects.md, "When the walker is hit": I is the object's x
    // less the walker's.
    for (heading, facing, low, high) in [
        ("right", Facing::Right, -25, 31),
        ("right", Facing::Left, -30, 26),
        ("left", Facing::Right, -26, 30),
        ("left", Facing::Left, -31, 25),
    ] {
        let row = format!("walker {heading}, player {facing:?}");
        assert!(
            !breaks_on_large_walker(300 + low - 1, facing, heading),
            "{row}"
        );
        assert!(breaks_on_large_walker(300 + low, facing, heading), "{row}");
        assert!(breaks_on_large_walker(300 + high, facing, heading), "{row}");
        assert!(
            !breaks_on_large_walker(300 + high + 1, facing, heading),
            "{row}"
        );
    }
}

#[test]
fn a_carried_object_never_reaches_a_small_walker_on_the_players_floor() {
    for i in -40..=40 {
        let mut w = carrying(
            f64::from(300 + i),
            Facing::Right,
            SMALL,
            (300.0, 0.0, "left"),
        );
        carry_step(&mut w);
        assert!(w.things[1].active, "I = {i}");
    }
}

#[test]
fn a_carried_object_breaks_on_a_large_walker_as_in_the_example() {
    // thrown-objects.md, "A carried object", "Example", 100 px further
    // right and 400 lower: the walker heads left at full speed and is at
    // 227.5 on K − 1 (I = −27) and 226.5 on K (I = −26).
    let mut w = carrying(200.0, Facing::Right, LARGE, (237.5, 1.0, "left"));
    for frame in 1..=10 {
        carry_step(&mut w);
        assert!(w.things[1].active, "broke on frame {frame}");
    }
    assert_eq!(w.things[0].pos.x, fx(227.5));
    carry_step(&mut w);
    // K: the object breaks, the walker's vx is −4 and it heads left.
    assert!(!w.things[1].active);
    assert!(w.player.hands.is_none());
    assert_eq!(w.things[0].pos.x, fx(226.5));
    assert_eq!(w.things[0].vx, fx(-4.0));
    assert!(w.flow.events.contains(&FlowEvent::Defeat(0)));
    let rows = [
        (222.75, 400.0),
        (219.21875, 394.4375),
        (215.87890625, 389.3125),
    ];
    for (n, (x, y)) in rows.into_iter().enumerate() {
        carry_step(&mut w);
        let walker = &w.things[0];
        assert_eq!(
            (walker.pos.x, walker.pos.y),
            (fx(x), fx(y)),
            "K + {}",
            n + 1
        );
        assert!(!w.events.contains(&Event::Hurt));
    }
}

#[test]
fn the_walker_a_carried_object_broke_on_can_still_hurt_on_the_next_frame() {
    // thrown-objects.md, "What happens on a hit": at the end of K the
    // walker still counts as walking. Object and player at 300 with the
    // walker still at 300: within the hit range and the hurt range.
    let mut w = carrying(300.0, Facing::Right, LARGE, (300.0, 0.0, "left"));
    carry_step(&mut w);
    assert!(!w.things[1].active);
    assert!(!w.events.contains(&Event::Hurt));
    carry_step(&mut w);
    assert!(w.events.contains(&Event::Hurt));
}

#[test]
fn a_crate_just_picked_up_doesnt_break_on_its_first_carried_frame() {
    // "the player was carrying the object at the end of the frame
    // before": the carrying starts on G + 15. I = 29 is within the hit
    // range and outside the hurt range.
    let mut w = carrying(329.0, Facing::Right, LARGE, (300.0, 0.0, "left"));
    w.player.hands = Some(Hands {
        thing: 1,
        act: Act::PickUp,
        frame: 14,
        press: None,
    });
    carry_step(&mut w);
    assert_eq!(w.player.hands.map(|h| h.act), Some(Act::Carry));
    assert!(w.things[1].active);
    carry_step(&mut w);
    assert!(!w.things[1].active);
}

/// Whether the crate a player carries at `object_x` breaks on a still
/// medium walker at 300 facing `heading` on the first step.
fn breaks_on_medium_walker(object_x: i32, facing: Facing, heading: &str) -> bool {
    let mut w = carrying(f64::from(object_x), facing, MEDIUM, (300.0, 0.0, heading));
    carry_step(&mut w);
    !w.things[1].active
}

#[test]
fn a_carried_object_breaks_on_a_medium_walker_in_the_measured_ranges() {
    // thrown-objects.md, "When the walker is hit", the medium walker's
    // table, with the player on its floor (H of 41 and 42).
    for (heading, facing, low, high) in [
        ("right", Facing::Right, -20, 35),
        ("right", Facing::Left, -25, 30),
        ("left", Facing::Right, -30, 25),
        ("left", Facing::Left, -35, 20),
    ] {
        let row = format!("walker {heading}, player {facing:?}");
        assert!(
            !breaks_on_medium_walker(300 + low - 1, facing, heading),
            "{row}"
        );
        assert!(breaks_on_medium_walker(300 + low, facing, heading), "{row}");
        assert!(
            breaks_on_medium_walker(300 + high, facing, heading),
            "{row}"
        );
        assert!(
            !breaks_on_medium_walker(300 + high + 1, facing, heading),
            "{row}"
        );
    }
}

/// Whether a player standing 22 px right of a still small walker heading
/// left, doing `act` and on its frame `frame` at the end of the first
/// step, is hurt on the second. 22 is in the upright range (−19 to 24)
/// and not the carrying one (−18 to 19). The crate is made into one
/// that breaks on nothing, so that the action goes on as with it.
fn hurt_during(act: Act, frame: u8) -> bool {
    let mut w = carrying(322.0, Facing::Right, SMALL, (300.0, 0.0, "left"));
    w.things[1].archetype = None;
    w.player.hands = Some(Hands {
        thing: 1,
        act,
        frame: frame - 1,
        press: None,
    });
    let held: Actions = match act {
        Act::PickUp | Act::Carry => Action::Action.into(),
        Act::Throw | Act::PutDown => Actions::NONE,
    };
    step(&mut w, &InputFrame::single(Frame::after(held, held)));
    assert_eq!(w.player.hands.map(|h| (h.act, h.frame)), Some((act, frame)));
    assert!(!w.events.contains(&Event::Hurt), "hurt on the first frame");
    step(&mut w, &InputFrame::single(Frame::after(held, held)));
    w.events.contains(&Event::Hurt)
}

#[test]
fn pick_ups_throws_and_put_downs_have_the_carrying_pose() {
    // walker-contact.md, "Carrying": G to G + 14, Q to Q + 42 of a throw
    // and Q to Q + 12 of a put-down; upright from Q + 13 of a put-down.
    for frame in [1, 7, 14] {
        assert!(!hurt_during(Act::PickUp, frame), "G + {frame}");
    }
    for frame in [1, 21, 22, 42] {
        assert!(!hurt_during(Act::Throw, frame), "throw Q + {frame}");
    }
    for frame in [1, 12] {
        assert!(!hurt_during(Act::PutDown, frame), "put-down Q + {frame}");
    }
    assert!(hurt_during(Act::PutDown, 13), "put-down Q + 13");
}

/// Whether a player throwing, on frame `frame` of the throw on the step
/// it falls onto a still large walker 10 px above its floor, bounces.
/// With `frame` 0 the throw starts on that step.
fn bounces_throwing(frame: u8) -> bool {
    let mut w = carrying(300.0, Facing::Right, LARGE, (300.0, 0.0, "left"));
    w.things[1].active = false;
    w.player.ground = None;
    // After this frame's gravity it falls 1/16 px, to 10 above the floor.
    w.player.pos.y = Fixed::from_int(390) - fx(0.0625);
    w.player.vel.y = fx(0.0625 - 0.4375);
    if frame > 0 {
        w.player.hands = Some(Hands {
            thing: 1,
            act: Act::Throw,
            frame: frame - 1,
            press: None,
        });
    }
    step(
        &mut w,
        &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
    );
    assert_eq!(
        w.player.hands.map(|h| (h.act, h.frame)),
        Some((Act::Throw, frame))
    );
    w.events.contains(&Event::Bounce)
}

#[test]
fn no_bounce_while_the_object_is_still_in_the_hands_of_a_throw() {
    // walker-contact.md, "Carrying": no bounce on Q + 1 to Q + 21; a
    // bounce on Q and from Q + 22.
    assert!(bounces_throwing(0), "Q");
    for frame in [1, 10, 21] {
        assert!(!bounces_throwing(frame), "Q + {frame}");
    }
    assert!(bounces_throwing(22), "Q + 22");
}

/// carrying.md, "The object in the hands": (ahead, up) on each frame of
/// a pick-up from G + 1, and of a throw and a put-down from Q.
const PICK_UP: [(i32, i32); 14] = [
    (20, 3),
    (20, 3),
    (20, 15),
    (20, 15),
    (20, 27),
    (20, 27),
    (20, 33),
    (20, 33),
    (4, 38),
    (4, 38),
    (-2, 39),
    (-2, 39),
    (-2, 39),
    (-2, 39),
];
const THROW: [(i32, i32); 22] = [
    (2, 46),
    (-6, 47),
    (-6, 47),
    (-18, 48),
    (-18, 48),
    (-30, 44),
    (-30, 44),
    (-40, 39),
    (-40, 39),
    (-38, 39),
    (-38, 39),
    (-36, 38),
    (-36, 38),
    (-32, 39),
    (-32, 39),
    (-28, 41),
    (-28, 41),
    (-24, 45),
    (-24, 45),
    (-7, 54),
    (-7, 54),
    (0, 54),
];
const PUT_DOWN: [(i32, i32); 14] = [
    (-2, 39),
    (4, 38),
    (4, 38),
    (20, 33),
    (20, 33),
    (20, 27),
    (20, 27),
    (20, 15),
    (20, 15),
    (20, 3),
    (20, 3),
    (16, 3),
    (16, 3),
    (16, 3),
];

/// One step that leaves the player doing `act`, on frame `frame` of it,
/// from frame `frame` − 1 (from carrying for frame 0).
fn step_to(w: &mut World, act: Act, frame: u8) {
    w.player.hands = Some(match frame {
        0 => Hands {
            thing: 1,
            act: Act::Carry,
            frame: 30,
            press: None,
        },
        _ => Hands {
            thing: 1,
            act,
            frame: frame - 1,
            press: None,
        },
    });
    let action: Actions = Action::Action.into();
    let (before, held) = match act {
        Act::PickUp | Act::Carry => (action, action),
        Act::Throw => (action, Actions::NONE),
        Act::PutDown => (action, Action::Down.into()),
    };
    let before = if frame == 0 { before } else { held };
    step(w, &InputFrame::single(Frame::after(before, held)));
    // A pick-up hit on its last frame ends there.
    if act == Act::PickUp && frame == 14 && !w.things[1].active {
        assert!(w.player.hands.is_none());
        return;
    }
    assert_eq!(
        w.player.hands.map(|h| (h.act, h.frame)),
        Some((act, frame)),
        "{act:?} frame {frame}"
    );
}

/// Where the object is on frame `n` of `act` by `path` (from frame
/// `first`), for a player at `x` on the floor at 400 facing `facing`.
fn in_hands(path: &[(i32, i32)], first: u8, n: u8, x: i32, facing: Facing) -> (Fixed, Fixed) {
    let (ahead, up) = path[usize::from(n - first)];
    let ahead = if facing == Facing::Right {
        ahead
    } else {
        -ahead
    };
    (Fixed::from_int(x + ahead), Fixed::from_int(400 - up))
}

#[test]
fn the_object_moves_in_the_hands_during_pick_ups_throws_and_put_downs() {
    for facing in [Facing::Right, Facing::Left] {
        for (act, path, first) in [
            (Act::PickUp, &PICK_UP[..], 1),
            (Act::Throw, &THROW[..], 0),
            (Act::PutDown, &PUT_DOWN[..], 0),
        ] {
            for n in first..first + path.len() as u8 {
                // The walker is far away.
                let mut w = carrying(300.25, facing, SMALL, (50.0, 0.0, "left"));
                step_to(&mut w, act, n);
                let thing = &w.things[1];
                assert_eq!(
                    (thing.pos.x, thing.pos.y),
                    in_hands(path, first, n, 300, facing),
                    "{act:?} frame {n}, facing {facing:?}"
                );
            }
        }
    }
}

#[test]
fn a_put_down_object_breaks_on_a_small_walker_as_in_the_example() {
    // thrown-objects.md, "During a pick-up, a throw or a put-down",
    // "Example", 200 px further right and 400 lower: the walker is at
    // 343.5 on Q + 3, and the object breaks on Q + 5, once H is 27.
    let mut w = carrying(300.0, Facing::Right, SMALL, (347.5, 1.0, "left"));
    w.things[0].vx = fx(-1.0);
    step_to(&mut w, Act::PutDown, 0);
    for n in 1..=4 {
        step_to(&mut w, Act::PutDown, n);
        assert!(w.things[1].active, "broke on Q + {n}");
    }
    assert_eq!(w.things[0].pos.x, fx(342.5));
    step_to(&mut w, Act::PutDown, 5);
    assert!(!w.things[1].active, "Q + 5");
    assert_eq!(w.things[0].pos.x, fx(341.5));
    assert_eq!(w.things[0].vx, fx(-4.0));
    assert!(w.flow.events.contains(&FlowEvent::Defeat(0)));
    // The put-down goes on to its end, and nothing lies on the floor.
    let down: Actions = Action::Down.into();
    for n in 6..=13 {
        step(&mut w, &InputFrame::single(Frame::after(down, down)));
        assert_eq!(
            w.player.hands.map(|h| (h.act, h.frame)),
            Some((Act::PutDown, n))
        );
    }
    step(&mut w, &InputFrame::single(Frame::after(down, down)));
    assert!(w.player.hands.is_none());
    assert!(!w.things[1].active);
}

/// Whether the object breaks on a still walker of `kind` heading
/// `heading`, `i` px left of where the object is on frame `n` of `act`,
/// on that frame.
fn breaks_in_the_hands(kind: &str, heading: &str, facing: Facing, act: Act, n: u8, i: i32) -> bool {
    let (path, first) = match act {
        Act::PickUp => (&PICK_UP[..], 1),
        Act::Throw => (&THROW[..], 0),
        _ => (&PUT_DOWN[..], 0),
    };
    let (x, _) = in_hands(path, first, n.max(first), 300, facing);
    let walker_x = f64::from(x.floor() as i32 - i);
    let mut w = carrying(300.0, facing, kind, (walker_x, 0.0, heading));
    step_to(&mut w, act, n);
    let broke = !w.things[1].active;
    assert_eq!(broke, w.flow.events.contains(&FlowEvent::Defeat(0)));
    if broke {
        // Knocked back as by one thrown right.
        assert_eq!(w.things[0].vx, fx(-4.0));
        if (act, n) != (Act::PickUp, 14) {
            assert_eq!(w.player.hands.map(|h| h.act), Some(act));
        }
    }
    broke
}

#[test]
fn a_throw_breaks_the_object_in_the_hands_by_the_early_thrown_ranges() {
    // thrown-objects.md, "During a pick-up, a throw or a put-down": on
    // Q + 1 to Q + 20, for all three kinds.
    let rows = [
        (SMALL, "right", Facing::Right, -47, 29),
        (SMALL, "left", Facing::Right, -48, 28),
        (SMALL, "right", Facing::Left, -28, 48),
        (SMALL, "left", Facing::Left, -29, 47),
        (MEDIUM, "right", Facing::Right, -45, 36),
        (MEDIUM, "left", Facing::Left, -36, 45),
        (LARGE, "right", Facing::Left, -31, 51),
        (LARGE, "left", Facing::Right, -51, 31),
    ];
    for (kind, heading, facing, low, high) in rows {
        for n in [1, 8, 20] {
            let row = format!("{kind}, walker {heading}, player {facing:?}, Q + {n}");
            let breaks = |i| breaks_in_the_hands(kind, heading, facing, Act::Throw, n, i);
            assert!(!breaks(low - 1), "{row}: {}", low - 1);
            assert!(breaks(low), "{row}: {low}");
            assert!(breaks(high), "{row}: {high}");
            assert!(!breaks(high + 1), "{row}: {}", high + 1);
        }
    }
    // Not on Q, nor on Q + 21.
    for n in [0, 21] {
        assert!(!breaks_in_the_hands(
            LARGE,
            "left",
            Facing::Right,
            Act::Throw,
            n,
            0
        ));
    }
}

#[test]
fn small_walkers_are_hit_while_the_object_is_low_in_a_pick_up_or_put_down() {
    // H at most 27: G + 1 to G + 6 and Q + 5 to Q + 12. I within −23 to
    // 27 for a walker heading left and a player facing right.
    let low = |act, n| {
        let breaks = |i| breaks_in_the_hands(SMALL, "left", Facing::Right, act, n, i);
        breaks(-23) && breaks(27) && !breaks(-24) && !breaks(28)
    };
    let missed = |act, n| breaks_in_the_hands(SMALL, "left", Facing::Right, act, n, 0);
    for n in 1..=6 {
        assert!(low(Act::PickUp, n), "G + {n}");
    }
    for n in 7..=14 {
        assert!(!missed(Act::PickUp, n), "G + {n}");
    }
    for n in 0..=4 {
        assert!(!missed(Act::PutDown, n), "Q + {n}");
    }
    for n in 5..=12 {
        assert!(low(Act::PutDown, n), "Q + {n}");
    }
    // Not from Q + 13.
    assert!(!missed(Act::PutDown, 13));
}

#[test]
fn medium_and_large_walkers_are_hit_through_a_pick_up_and_a_put_down() {
    // The carried ranges: a large walker heading right, a player facing
    // left, −30 to 26; a medium one heading left, a player facing right,
    // −30 to 25.
    for (kind, heading, facing, low, high) in [
        (LARGE, "right", Facing::Left, -30, 26),
        (MEDIUM, "left", Facing::Right, -30, 25),
    ] {
        for (act, frames) in [(Act::PickUp, 1..=14), (Act::PutDown, 0..=12)] {
            for n in frames {
                let row = format!("{kind}, {act:?} frame {n}");
                let breaks = |i| breaks_in_the_hands(kind, heading, facing, act, n, i);
                assert!(breaks(low) && breaks(high), "{row}");
                assert!(!breaks(low - 1) && !breaks(high + 1), "{row}");
            }
        }
    }
}

#[test]
fn a_pick_up_that_was_hit_ends_with_nothing_in_the_hands() {
    // Hit on G + 3 with I = −25, the walker outside the hurt range; the
    // pick-up goes on to G + 14, and from G + 15 the player holds
    // nothing.
    let mut w = carrying(300.0, Facing::Right, LARGE, (345.0, 0.0, "left"));
    step_to(&mut w, Act::PickUp, 3);
    assert!(!w.things[1].active);
    w.things[0].active = false;
    for n in 4..=14 {
        carry_step(&mut w);
        assert_eq!(
            w.player.hands.map(|h| (h.act, h.frame)),
            if n < 14 { Some((Act::PickUp, n)) } else { None },
            "G + {n}"
        );
    }
}

#[test]
fn the_walker_hit_during_a_put_down_can_still_hurt_on_the_next_frame() {
    // On Q the object is 2 px behind the player: I = −12 for a walker
    // 10 px ahead of the player, which is in the hurt range.
    let mut w = carrying(300.0, Facing::Right, LARGE, (310.0, 0.0, "left"));
    step_to(&mut w, Act::PutDown, 0);
    assert!(!w.things[1].active);
    assert!(!w.events.contains(&Event::Hurt));
    let down: Actions = Action::Down.into();
    step(&mut w, &InputFrame::single(Frame::after(down, down)));
    assert!(w.events.contains(&Event::Hurt));
}
