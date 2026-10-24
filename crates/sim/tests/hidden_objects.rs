//! Hidden objects on `classic-1` (spec/behavior/hidden-objects.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Presets};
use clamber_sim::{
    Behavior, Event, Facing, Fixed, InputFrame, Phase, ROLL, State, World, generic_feel, step,
};

use Action::{Action as A, Down as D, Right as R};

/// The params the importer gives a hidden barrel (`hidden-barrel`), or a
/// metal one going right with `metal`.
fn falling_kind(metal: bool) -> String {
    let (becomes, across) = if metal {
        ("canister", 1)
    } else {
        ("crate", -1)
    };
    format!(
        r#"{{ strike_frames = [], fall_height = 160, across = [[-28, 42], [-42, 28]], above = [-27, 20], second_across = [[-38, 28], [-28, 38]], second_above = [-20, 27], becomes = "{becomes}", becomes_at = [0, 0], item_speed = [{across}, -6] }}"#
    )
}

/// Flat ground at y = 300 from x = 0 to 2000, the player at `start`, and
/// a hidden object at each of `hidden`, with `params`.
fn world(start: (f64, f64), hidden: &[(i32, i32)], params: &str) -> World {
    let entities: String = hidden
        .iter()
        .map(|(x, y)| {
            format!(
                r#"
                [[entities]]
                type = "hidden"
                at = [{x}, {y}]
                params = {params}
                "#
            )
        })
        .collect();
    let src = format!(
        r#"
        format = 1
        size = [2000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, 300, 2000, 300]

        {entities}
        "#,
        start.0, start.1
    );
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Plays `frames` frames from frame 0 with `on_0` pressed on frame 0
/// and nothing held after, calling `each` after every frame.
fn play(w: &mut World, on_0: &[Action], frames: usize, mut each: impl FnMut(usize, &World)) {
    let mut last = Actions::NONE;
    for k in 0..frames {
        let now: Actions = if k == 0 {
            on_0.iter().copied().collect()
        } else {
            Actions::NONE
        };
        step(w, &InputFrame::single(Frame::after(last, now)));
        last = now;
        each(k, w);
    }
}

/// Settles the player on the floor, facing `facing`.
fn settle(w: &mut World, facing: Facing) {
    for _ in 0..4 {
        step(w, &InputFrame::default());
    }
    assert!(w.player.grounded());
    w.player.facing = facing;
}

#[test]
fn the_example() {
    // The spec's player stands at x 100.5; I is the same at 100.
    let mut w = world((100.0, 300.0), &[(130, 300)], "{}");
    settle(&mut w, Facing::Right);
    let mut rows = Vec::new();
    play(&mut w, &[D, A], 80, |_, w| {
        let t = &w.things[0];
        rows.push((
            w.player.pos.x,
            w.player.state,
            t.kind.clone(),
            t.pos,
            t.phase,
            w.player.frozen,
        ));
    });
    for (k, row) in rows.iter().enumerate().take(25) {
        assert_eq!(row.2, "hidden", "frame {k}: not struck before K");
        assert_eq!(row.4, Phase::Ready, "frame {k}");
    }
    let item = |k: usize| {
        let (_, _, ref kind, pos, phase, _) = rows[k];
        let vy = match phase {
            Phase::Item { vy, .. } => Some(vy),
            _ => None,
        };
        assert_eq!(kind, "cluster", "frame {k}");
        (pos.x, pos.y, vy)
    };
    let table = [
        (25, 130.0, 300.0, -6.0),
        (26, 130.0, 300.0, -6.0),
        (27, 130.0, 300.0, -6.0),
        (28, 131.0, 294.25, -5.75),
        (29, 132.0, 288.75, -5.5),
        (51, 154.0, 231.0, 0.0),
        (73, 176.0, 294.25, 5.5),
    ];
    for (k, x, y, vy) in table {
        assert_eq!(item(k), (fx(x), fx(y), Some(fx(vy))), "frame {k}");
    }
    // L = K + 49: on the floor.
    assert_eq!(rows[74].4, Phase::Landed(0));
    assert_eq!((rows[74].3.x, rows[74].3.y), (fx(177.0), fx(300.0)));
    // L + 1: one more step right.
    assert_eq!(rows[75].3.x, fx(178.0));
    // L + 2: a bunch 1 px left of where the item stood.
    assert_eq!(rows[76].4, Phase::Ready);
    assert_eq!((rows[76].3.x, rows[76].3.y), (fx(177.0), fx(300.0)));
    let bunch = w.things[0].archetype.unwrap();
    assert!(matches!(
        bunch.behavior,
        Behavior::Collect { worth: 10, .. }
    ));
    // The pause: K + 1 and K + 2 hold the player still, and the action
    // ends one pause later than without the object: E = 61.
    assert_eq!(rows[25].5, 2);
    for row in &rows {
        assert_eq!(row.0, Fixed::from_int(100));
    }
    assert!((0..=60).all(|k| rows[k].1 == State::GroundAction));
    assert_eq!(rows[61].1, State::Idle);
}

/// Whether a ground action standing still, direction 0, strikes a
/// hidden bunch at `I` = `i` and `D` = `d` from a player facing
/// `facing`.
fn strikes_at(facing: Facing, i: i32, d: i32) -> bool {
    let x = 500;
    let mut w = world((f64::from(x), 300.0), &[(x - i, 300 + d)], "{}");
    settle(&mut w, facing);
    let mut struck = false;
    play(&mut w, &[D, A], 40, |_, w| {
        struck |= w.things[0].phase != Phase::Ready;
    });
    struck
}

#[test]
fn struck_by_the_ground_action_ranges() {
    for (facing, low, high) in [(Facing::Right, -53, 0), (Facing::Left, 0, 53)] {
        for (i, out) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(strikes_at(facing, i, 0), out, "facing {facing:?}, I = {i}");
        }
    }
    for (d, out) in [(-53, false), (-52, true), (8, true), (9, false)] {
        assert_eq!(strikes_at(Facing::Right, -20, d), out, "D = {d}");
    }
}

#[test]
fn only_on_the_second_and_third_attack_frames() {
    // Put in range just before t = 17, and out of it again after: no
    // strike. Kept in range, it's struck on t = 25.
    let mut w = world((500.0, 300.0), &[(1500, 300)], "{}");
    settle(&mut w, Facing::Right);
    let mut struck_on = None;
    let mut last = Actions::NONE;
    for k in 0..40 {
        w.things[0].pos.x = Fixed::from_int(if (17..=18).contains(&k) { 520 } else { 1500 });
        let now: Actions = if k == 0 {
            [D, A].into_iter().collect()
        } else {
            Actions::NONE
        };
        step(&mut w, &InputFrame::single(Frame::after(last, now)));
        last = now;
        if struck_on.is_none() && w.things[0].phase != Phase::Ready {
            struck_on = Some(k);
        }
    }
    assert_eq!(struck_on, None);

    let mut w = world((500.0, 300.0), &[(520, 300)], "{}");
    settle(&mut w, Facing::Right);
    let mut struck_on = None;
    play(&mut w, &[D, A], 40, |k, w| {
        if struck_on.is_none() && w.things[0].phase != Phase::Ready {
            struck_on = Some(k);
        }
    });
    assert_eq!(struck_on, Some(25));
}

#[test]
fn never_in_the_way_and_never_hurting() {
    // Walking right through a hidden bunch and over both falling kinds.
    let bunch = world((100.0, 300.0), &[(160, 300)], "{}");
    let barrel = world((100.0, 300.0), &[(160, 300)], &falling_kind(false));
    for mut w in [bunch, barrel] {
        settle(&mut w, Facing::Right);
        let mut last = Actions::NONE;
        for _ in 0..90 {
            let now: Actions = [R].into_iter().collect();
            step(&mut w, &InputFrame::single(Frame::after(last, now)));
            last = now;
            assert!(!w.events.contains(&Event::Hurt));
            assert!(!w.events.contains(&Event::Bounce));
            assert!(w.player.grounded());
        }
        assert!(w.player.pos.x > fx(200.0));
        assert_eq!(w.things[0].phase, Phase::Ready);
        assert_eq!(w.things[0].kind, "hidden");
    }
}

/// Lets the player fall from rest with its feet line `height` px above a
/// hidden barrel at (500, 300), `across` px right of it. Returns the
/// frame it was set off on, if it was, and the player's `vy` then.
fn fall_onto(height: f64, across: f64, metal: bool) -> Option<(usize, Fixed, World)> {
    let mut w = world(
        (500.0 + across, 300.0 - height),
        &[(500, 300)],
        &falling_kind(metal),
    );
    for k in 0..120 {
        step(&mut w, &InputFrame::default());
        if w.things[0].phase != Phase::Ready {
            let vy = w.player.vel.y;
            return Some((k, vy, w));
        }
    }
    None
}

#[test]
fn landed_on_after_a_long_fall() {
    // From rest exactly 160 px above, the feet line is less than 160 px
    // above by the end of the first frame: it passes through.
    assert!(fall_onto(160.0, 0.0, false).is_none());
    let (_, vy, mut w) = fall_onto(160.5, 0.0, false).expect("bounces from 160.5 px");
    // B is a bounce frame: vy -7.125, then two frames without moving.
    assert_eq!(vy, fx(-7.125));
    assert!(w.events.contains(&Event::Bounce));
    assert_eq!(w.player.state, State::Bounce);
    let at = w.player.pos;
    for _ in 0..2 {
        step(&mut w, &InputFrame::default());
        assert_eq!(w.player.pos, at);
    }
    step(&mut w, &InputFrame::default());
    assert_ne!(w.player.pos, at);
    // A walk-off or an ordinary jump onto it passes through.
    assert!(fall_onto(100.0, 0.0, false).is_none());
}

#[test]
fn landed_on_ranges() {
    // Facing right (the player starts so): I from -28 to 42.
    for (across, out) in [(-29.0, false), (-28.0, true), (42.0, true), (43.0, false)] {
        assert_eq!(
            fall_onto(200.0, across, false).is_some(),
            out,
            "I = {across}"
        );
    }
}

#[test]
fn a_rolling_player_passes_through() {
    let mut w = world((500.0, 100.0), &[(500, 300)], &falling_kind(false));
    for _ in 0..120 {
        w.player.state = State::Roll;
        step(&mut w, &InputFrame::default());
        if w.player.grounded() {
            break;
        }
    }
    assert_eq!(w.things[0].phase, Phase::Ready);
}

#[test]
fn the_barrel_item_goes_left_and_becomes_a_barrel() {
    let (b, _, mut w) = fall_onto(200.0, 0.0, false).unwrap();
    assert_eq!(w.things[0].kind, "crate");
    let at = w.things[0].pos;
    assert_eq!((at.x, at.y), (fx(500.0), fx(300.0)));
    // K + 51: it stands 48 px left of the object, as a barrel.
    for _ in b + 1..=b + 51 {
        step(&mut w, &InputFrame::default());
    }
    let thing = &w.things[0];
    assert_eq!(thing.phase, Phase::Ready);
    assert_eq!((thing.pos.x, thing.pos.y), (fx(452.0), fx(300.0)));
    assert!(matches!(
        thing.archetype.unwrap().behavior,
        Behavior::Carry { .. }
    ));
}

#[test]
fn the_metal_barrel_item_goes_right() {
    let (b, _, mut w) = fall_onto(200.0, 0.0, true).unwrap();
    assert_eq!(w.things[0].kind, "canister");
    for _ in b + 1..=b + 51 {
        step(&mut w, &InputFrame::default());
    }
    let thing = &w.things[0];
    assert_eq!(thing.phase, Phase::Ready);
    assert_eq!((thing.pos.x, thing.pos.y), (fx(548.0), fx(300.0)));
}

#[test]
fn an_item_from_1_px_below_the_floor_lands_a_move_sooner() {
    let mut w = world((100.0, 300.0), &[(130, 301)], "{}");
    settle(&mut w, Facing::Right);
    let mut landed = None;
    play(&mut w, &[D, A], 80, |k, w| {
        if landed.is_none() && w.things[0].phase == Phase::Landed(0) {
            landed = Some(k);
        }
    });
    // K = 25, L = K + 48, standing 47 px right once a bunch.
    assert_eq!(landed, Some(73));
    assert_eq!(w.things[0].pos.x, fx(130.0 + 47.0 - 1.0));
}

#[test]
fn landed_on_vertical_range() {
    // Fall from rest at level y 0, with the object out of the way; on a
    // frame that ends with the feet line at about 240, put the object so
    // that D at the end of it is `d`.
    let feet = |w: &World| -(-w.player.pos.y).floor();
    let mut w = world((500.0, 0.0), &[(1500, 300)], &falling_kind(false));
    loop {
        let mut next = w.clone();
        step(&mut next, &InputFrame::default());
        if feet(&next) >= 240 {
            break;
        }
        w = next;
    }
    let mut ahead = w.clone();
    step(&mut ahead, &InputFrame::default());
    let end = feet(&ahead);
    for (d, out) in [(-28, false), (-27, true), (20, true), (21, false)] {
        let mut w = w.clone();
        w.things[0].pos.x = Fixed::from_int(500);
        w.things[0].pos.y = Fixed::from_int((end + d) as i32);
        step(&mut w, &InputFrame::default());
        assert_eq!(w.things[0].phase != Phase::Ready, out, "D = {d}");
    }
}

/// The second character under control.
fn second(mut w: World) -> World {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::from([(ROLL.to_string(), BTreeMap::new())]),
    });
    w.feel = Arc::new(feel);
    w.player.character = 1;
    w
}

/// Whether the second character, falling from rest 200 px above a hidden
/// barrel at (500, 300), `across` px right of it facing `facing`, sets
/// it off.
fn second_falls(across: f64, facing: Facing) -> bool {
    let mut w = second(world(
        (500.0 + across, 100.0),
        &[(500, 300)],
        &falling_kind(true),
    ));
    w.player.facing = facing;
    for _ in 0..120 {
        step(&mut w, &InputFrame::default());
        if w.things[0].phase != Phase::Ready {
            return true;
        }
        if w.player.grounded() {
            break;
        }
    }
    false
}

/// hidden-objects.md, "The second character": a long fall sets off a
/// hidden metal barrel from I −38 to 28 facing right and −28 to 38 facing
/// left, not the first character's ranges.
#[test]
fn the_second_character_is_set_off_by_its_own_ranges() {
    for (facing, low, high) in [(Facing::Right, -38.0, 28.0), (Facing::Left, -28.0, 38.0)] {
        for (across, out) in [
            (low - 1.0, false),
            (low, true),
            (high, true),
            (high + 1.0, false),
        ] {
            assert_eq!(second_falls(across, facing), out, "{facing:?} I = {across}");
        }
    }
}

/// hidden-objects.md, "The second character": D from −20 to 27.875.
#[test]
fn the_second_character_vertical_range() {
    let feet = |w: &World| -(-w.player.pos.y).floor();
    let mut w = second(world((500.0, 0.0), &[(1500, 300)], &falling_kind(true)));
    loop {
        let mut next = w.clone();
        step(&mut next, &InputFrame::default());
        if feet(&next) >= 240 {
            break;
        }
        w = next;
    }
    let mut ahead = w.clone();
    step(&mut ahead, &InputFrame::default());
    let end = feet(&ahead);
    for (d, out) in [(-21, false), (-20, true), (27, true), (28, false)] {
        let mut w = w.clone();
        w.things[0].pos.x = Fixed::from_int(500);
        w.things[0].pos.y = Fixed::from_int((end + d) as i32);
        step(&mut w, &InputFrame::default());
        assert_eq!(w.things[0].phase != Phase::Ready, out, "D = {d}");
    }
}

/// hidden-objects.md, "The second character": no ground action, and its
/// roll doesn't strike a hidden bunch.
#[test]
fn the_second_character_never_strikes_a_hidden_bunch() {
    for keys in [&[D, A][..], &[D, A, R][..], &[D, A][..]] {
        let mut w = second(world((500.0, 300.0), &[(520, 300)], "{}"));
        settle(&mut w, Facing::Right);
        play(&mut w, keys, 60, |_, w| {
            assert_eq!(w.things[0].phase, Phase::Ready);
            assert_eq!(w.things[0].kind, "hidden");
        });
    }
}

/// The steps on which hidden objects at the offsets `at` (their `x`
/// less the player's, all on the floor) are struck by a standing ground
/// action, and the step the action ends on.
fn strikes_of(at: &[i32]) -> (Vec<Option<usize>>, usize) {
    let hidden: Vec<(i32, i32)> = at.iter().map(|d| (500 + d, 300)).collect();
    let mut w = world((500.0, 300.0), &hidden, "{}");
    settle(&mut w, Facing::Right);
    let mut struck = vec![None; at.len()];
    let mut end = 0;
    play(&mut w, &[D, A], 90, |k, w| {
        for (i, s) in struck.iter_mut().enumerate() {
            if s.is_none() && w.things[i].phase != Phase::Ready {
                *s = Some(k);
            }
        }
        if w.player.state == State::GroundAction {
            end = k + 1;
        }
    });
    (struck, end)
}

#[test]
fn one_attack_frame_strikes_one_hidden_object() {
    // Two in range on t = 25: one then, the other 8 frames later (t = 31
    // after the pause), and E = P + 63.
    let (struck, end) = strikes_of(&[20, 40]);
    let mut at: Vec<_> = struck.iter().map(|s| s.unwrap()).collect();
    at.sort_unstable();
    assert_eq!(at, [25, 33]);
    assert_eq!(end, 63);
    // One of them out of range: one strike, E = P + 61.
    let (struck, end) = strikes_of(&[80, 40]);
    assert_eq!(struck, [None, Some(25)]);
    assert_eq!(end, 61);
}
