//! Thrown objects in Classic 1 bouncing, rolling, breaking, hitting
//! walkers and passing through the characters
//! (spec/behavior/thrown-objects.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Feel, Fixed, Presets};
use clamber_sim::{
    Event, Facing, Flight, FlowEvent, InputFrame, Phase, View, World, generic_feel, step,
};

const A: Action = Action::Action;

/// The values walkers in walker.md have, with walker-contact.md's small
/// walker's box.
const SMALL_WALKER: &str = "speed = 1, facing = \"left\", size = [20, 12], offset = 0.5, \
    response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, \
    defeat_max_fall = 8";

/// The player at x = 200 on a floor at y = 400, a crate in reach to its
/// right, then `more` (collision and entities).
fn level(more: &str) -> String {
    format!(
        r#"
        format = 1
        name = "thrown"
        size = [3000, 800]
        start = [200, 400]

        [[entities]]
        type = "crate"
        at = [220, 400]
        {more}
        "#
    )
}

/// Flat floor at y = 400 from x = 0 to 3000.
const FLAT: &str = "[[collision]]\nshape = \"rect\"\nrect = [0, 400, 3000, 200]";

/// The flat floor of thrown-objects.md's "Example" to x = 400, where a
/// face rising to the right starts: its first column's floor is 1 px
/// higher than the flat floor, and each one after it 4 px higher again.
fn face() -> String {
    face_of("")
}

/// [`face`], with `more` added to its collider.
fn face_of(more: &str) -> String {
    let mut points = vec![[0, 600], [0, 400], [400, 400]];
    for k in 0..10 {
        let y = 399 - 4 * k;
        points.extend([[400 + k, y], [401 + k, y]]);
    }
    points.extend([[600, 363], [600, 600]]);
    format!("[[collision]]\nshape = \"polygon\"\npoints = {points:?}\n{more}")
}

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// 1/256 px per frame.
fn speed(n: i32) -> Fixed {
    Fixed::from_ratio_pow2(n, 8)
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// Picks up the crate standing still and carries it, ready to let
    /// go of action on the next step, Q.
    fn new(src: &str) -> Play {
        Play::with_feel(src, generic_feel("classic-1").unwrap())
    }

    /// [`Play::new`] with both characters, the other following.
    fn with_other(src: &str) -> Play {
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::new(),
        });
        let play = Play::with_feel(src, feel);
        assert!(play.world.other.is_some());
        play
    }

    fn with_feel(src: &str, feel: Feel) -> Play {
        let level = Arc::new(Level::from_toml_str(src, "thrown").unwrap());
        let world = World::new(level, Arc::new(feel));
        let mut play = Play {
            world,
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        play.hold(&[A], 21);
        assert!(play.world.player.hands.is_some());
        play
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    fn hold(&mut self, keys: &[Action], frames: usize) {
        for _ in 0..frames {
            self.step(keys);
        }
    }

    /// Lets go and steps through Q + `n`.
    fn throw_to(&mut self, n: usize) {
        self.hold(&[], n + 1);
    }

    fn object(&self) -> Flight {
        match self.world.things[0].phase {
            Phase::Thrown(flight) => flight,
            other => panic!("{other:?}"),
        }
    }
}

/// thrown-objects.md, "Example": two bounces, rolling, and breaking
/// against a face. Positions from the object's on Q + 21.
#[test]
fn bounces_rolls_and_breaks_against_a_face() {
    let mut play = Play::new(&level(&face()));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(start, Vec2::new(f(200.0), f(346.0)));
    let mut q = 21;
    for (n, x, y, vx, vy) in [
        (22, 14.4375, 1.4375, 3696, Some(368)),
        (30, 92.71875, 28.6875, 1860, Some(1264)),
        (33, 111.9296875, 46.125, 1543, Some(1600)),
        (34, 117.64453125, 54.0, 1463, Some(-800)),
        (35, 123.0859375, 51.3125, 1393, Some(-688)),
        (41, 151.60546875, 44.375, 1121, Some(-16)),
        (42, 155.875, 44.75, 1093, Some(96)),
        (45, 168.1640625, 48.5, 1029, Some(432)),
        (46, 172.12109375, 54.0, 1013, Some(-216)),
        (47, 176.0234375, 53.59375, 999, Some(-104)),
        (48, 179.87890625, 54.0, 987, None),
        (49, 183.69140625, 54.0, 976, None),
        (53, 198.61328125, 54.0, 945, None),
    ] {
        play.hold(&[], n - q);
        q = n;
        let thing = &play.world.things[0];
        let flight = play.object();
        assert!(thing.active, "Q + {n}");
        assert_eq!(thing.pos.x - start.x, f(x), "Q + {n}");
        assert_eq!(thing.pos.y - start.y, f(y), "Q + {n}");
        assert_eq!(flight.vx, speed(vx), "Q + {n}");
        assert_eq!(flight.rolling, vy.is_none(), "Q + {n}");
        if let Some(vy) = vy {
            assert_eq!(flight.vy, speed(vy), "Q + {n}");
        }
    }
    // The move would take it into a face column higher than it.
    assert!(play.world.bursts.is_empty());
    play.step(&[]);
    assert!(!play.world.things[0].active);
    // A flash and four pieces are left where it was, the flash mirrored
    // as it was moving right, and last as long as the flash does.
    let [burst] = play.world.bursts[..] else {
        panic!("{:?}", play.world.bursts);
    };
    assert_eq!((burst.thing, burst.object, burst.frame), (0, Some(true), 0));
    assert_eq!(burst.flash, play.world.things[0].pos);
    for _ in 0..18 {
        play.step(&[]);
    }
    assert_eq!(play.world.bursts[0].frame, 18);
    play.step(&[]);
    assert!(play.world.bursts.is_empty());
}

/// A wall breaks it too, though the spec only covers faces.
#[test]
fn breaks_against_a_wall() {
    let wall = "[[collision]]\nshape = \"rect\"\nrect = [300, 200, 20, 200]";
    let mut play = Play::new(&level(&format!("{FLAT}\n{wall}")));
    play.throw_to(21);
    let mut frames = 0;
    while play.world.things[0].active {
        assert!(play.world.things[0].pos.x < f(300.0));
        play.step(&[]);
        frames += 1;
        assert!(frames < 60);
    }
}

/// metal-barrel.md, "Turning back at a wall": a metal barrel that meets a
/// wall doesn't break. It stays where it is across, whole pixels only,
/// goes away from it at 3.5 px per frame, and rolls on.
#[test]
fn a_metal_barrel_turns_back_at_a_wall() {
    let wall = "[[collision]]\nshape = \"rect\"\nrect = [300, 200, 20, 200]";
    let src = level(&format!("{FLAT}\n{wall}")).replace("\"crate\"", "\"canister\"");
    let mut play = Play::new(&src);
    play.throw_to(21);
    let mut turned = None;
    for n in 0..80 {
        let before = play.world.things[0].pos;
        play.step(&[]);
        let thing = &play.world.things[0];
        assert!(thing.active, "{n}");
        assert!(thing.pos.x < f(300.0), "{n}");
        if turned.is_none() && play.object().vx.is_negative() {
            turned = Some(n);
            assert_eq!(thing.pos.x, Fixed::from_int(before.x.floor() as i32));
            assert_eq!(play.object().vx, speed(-896));
        }
    }
    assert!(turned.is_some());
    assert!(play.world.things[0].pos.x < f(300.0) - f(100.0));
    assert!(play.object().rolling);
    assert_eq!(play.object().vx, speed(-896));
}

/// thrown-objects.md, "Rolling": it rolls up and down slopes of 1 px per
/// 4 columns on their floor, without breaking.
#[test]
fn rolls_over_gentle_slopes() {
    // Flat to 400, past where it starts rolling, up 1 px every 4
    // columns for 40 px, flat, then down again, all before it's far
    // enough from the view to be removed.
    let mut points = vec![[0, 600], [0, 400], [400, 400]];
    for k in 0..10 {
        points.extend([[400 + 4 * k, 399 - k], [404 + 4 * k, 399 - k]]);
    }
    for k in 0..10 {
        points.extend([[450 + 4 * k, 391 + k], [454 + 4 * k, 391 + k]]);
    }
    points.extend([[3000, 400], [3000, 600]]);
    let src = level(&format!(
        "[[collision]]\nshape = \"polygon\"\npoints = {points:?}"
    ));
    let ground = Level::from_toml_str(&src, "thrown").unwrap().collision;
    let mut play = Play::new(&src);
    play.throw_to(48);
    let mut x = Fixed::ZERO;
    while play.world.things[0].active {
        let thing = &play.world.things[0];
        assert!(play.object().rolling);
        x = thing.pos.x;
        let floor = ground.spans(Fixed::from_int(x.floor() as i32) + f(0.5))[0].top;
        assert_eq!(thing.pos.y, floor, "{:?}", thing.pos);
        play.step(&[]);
    }
    assert!(x > f(500.0), "{x:?}");
}

/// thrown-objects.md, "Leaving the picture": removed once its x rounded
/// down is 385 or more right of the view's left edge.
#[test]
fn is_removed_far_right_of_the_view() {
    let mut play = Play::new(&level(FLAT));
    play.throw_to(21);
    let mut last = 0;
    while play.world.things[0].active {
        let view = play.world.view.unwrap().x();
        last = play.world.things[0].pos.x.floor() - view;
        assert!(last < 385, "{last}");
        play.step(&[]);
    }
    let view = play.world.view.unwrap().x();
    let x = match play.world.things[0].phase {
        Phase::Thrown(_) => play.world.things[0].pos.x.floor(),
        other => panic!("{other:?}"),
    };
    assert!(x - view >= 385, "{last} then {}", x - view);
}

/// thrown-objects.md, "Leaving the picture": removed once its x rounded
/// down is 128 or more left of the view's left edge as it was at the
/// start of the frame, and kept at 127.
#[test]
fn is_removed_far_left_of_the_view() {
    let mut play = Play::new(&level(FLAT));
    play.world.player.facing = Facing::Left;
    play.throw_to(21);
    let with_view = |play: &Play, left: i64| {
        let mut next = Play {
            world: play.world.clone(),
            held: play.held,
        };
        next.world.view = Some(View {
            left: Fixed::from_int(left as i32),
            ..play.world.view.unwrap()
        });
        next.step(&[]);
        next
    };
    // Until it nears the level's left end, past which it falls out.
    for _ in 0..30 {
        assert!(play.world.things[0].active);
        // Where it ends this step, with the view clear of it.
        let x = with_view(&play, play.world.things[0].pos.x.floor())
            .world
            .things[0]
            .pos
            .x
            .floor();
        assert!((0..200).contains(&x), "thrown left: {x}");
        assert!(!with_view(&play, x + 128).world.things[0].active, "{x}");
        play = with_view(&play, x + 127);
    }
}

/// The walker's `(x, y, vx, vy)`, `vy` 0 while it walks.
fn walker(play: &Play) -> (Fixed, Fixed, Fixed, Fixed) {
    let w = &play.world.things[1];
    let vy = match w.phase {
        Phase::Knocked { vy } => vy.unwrap_or(Fixed::ZERO),
        _ => Fixed::ZERO,
    };
    (w.pos.x, w.pos.y, w.vx, vy)
}

/// thrown-objects.md, "Hitting walkers", "Example": a small walker
/// heading left at full speed, `x` = 160.25 from the object's start on
/// Q + 21, is hit on Q + 32 and knocked back.
#[test]
fn hits_a_walker_in_its_first_frames_out_of_the_hands() {
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(play.world.things[1].vx, speed(-256));
    play.world.things[1].pos.x = start.x + f(160.25);
    let mut q = 21;
    for (n, x, y, vx, vy) in [
        (30, 151.25, 54.0, -256, 0),
        (31, 150.25, 54.0, -256, 0),
        (32, 149.25, 54.0, -1024, 0),
        (33, 145.5, 54.0, -960, -1536),
        (34, 141.96875, 48.4375, -904, -1424),
        (35, 138.62890625, 43.3125, -855, -1312),
    ] {
        play.hold(&[], n - q);
        q = n;
        let hit = play.world.flow.events.contains(&FlowEvent::Defeat(1));
        assert_eq!(hit, n == 32, "Q + {n}");
        let got = walker(&play);
        let want = (start.x + f(x), start.y + f(y), speed(vx), speed(vy));
        assert_eq!(got, want, "Q + {n}");
    }
    // The object goes on as it would have: it bounces on Q + 34.
    assert_eq!(play.world.things[0].pos.y - start.y, f(51.3125));
    assert_eq!(play.object().vy, speed(-688));
    // It falls through the floor and out of the level.
    play.hold(&[], 200);
    assert!(!play.world.things[1].active);
}

/// metal-barrel.md, "Walkers": a metal barrel thrown at a walker on the
/// floor defeats it as a barrel does, with the same knock-off and removal,
/// and goes on as it was, for each walker size, heading and throw side.
#[test]
fn a_metal_barrel_defeats_walkers_as_a_barrel_does() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let large = SMALL_WALKER.replace("size = [20, 12]", "size = [26, 38]");
    let run = |kind: &str, walker_params: &str, facing: &str, distance: f64| {
        let params = walker_params.replace("facing = \"left\"", &format!("facing = \"{facing}\""));
        let entity =
            format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {params} }}");
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", kind);
        let mut play = Play::new(&src);
        play.throw_to(21);
        let start = play.world.things[0].pos;
        play.world.things[1].pos.x = start.x + f(distance);
        let mut rows = Vec::new();
        let mut defeats = 0;
        for _ in 0..60 {
            play.hold(&[], 1);
            defeats += play.world.flow.events.contains(&FlowEvent::Defeat(1)) as u32;
            let (o, w) = (&play.world.things[0], &play.world.things[1]);
            rows.push((o.pos, o.active, walker(&play), w.active));
        }
        (rows, defeats)
    };
    for params in [SMALL_WALKER, medium.as_str(), large.as_str()] {
        for facing in ["left", "right"] {
            let (barrel, hits) = run("\"crate\"", params, facing, 150.0);
            let (metal, metal_hits) = run("\"canister\"", params, facing, 150.0);
            assert_eq!(hits, 1, "{params} {facing}");
            assert_eq!(metal_hits, 1, "{params} {facing}");
            assert_eq!(barrel, metal, "{params} {facing}");
        }
    }
}

/// metal-barrel.md, "Rolling right on the flat floor": a metal barrel
/// rolling right long after it left the hands defeats walkers by the
/// ranges of the "t of 17 or more" column, knocking them left at 4 px a
/// frame, and isn't slowed, lifted or removed by the hit.
#[test]
fn a_rolling_metal_barrel_defeats_walkers_by_the_late_ranges() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let large = SMALL_WALKER.replace("size = [20, 12]", "size = [26, 38]");
    // (walker, facing) -> the range of barrel x minus walker x.
    let kinds = [
        (SMALL_WALKER, [(-23, 27), (-22, 28)]),
        (medium.as_str(), [(-30, 25), (-20, 35)]),
        (large.as_str(), [(-26, 30), (-25, 31)]),
    ];
    for (params, ranges) in kinds {
        for (facing, (low, high)) in ["left", "right"].into_iter().zip(ranges) {
            for offset in (-20..=118).step_by(3) {
                let params = params
                    .replace("facing = \"left\"", &format!("facing = \"{facing}\""))
                    .replace("speed = 1", "speed = 0");
                let entity = format!(
                    "[[entities]]\ntype = \"crawler\"\nat = [100, 400]\nparams = {{ {params} }}"
                );
                let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
                let mut play = Play::new(&src);
                play.throw_to(21 + 20);
                let at = play.world.player.pos;
                place(&mut play, at, 0, 0, false, true);
                play.world.things[0].pos.x = f(300.0);
                play.world.things[1].pos.x = f(300.0 + offset as f64);
                let floor = play.world.things[0].pos.y;
                let mut hit = None;
                for n in 0..60 {
                    let before =
                        play.world.things[0].pos.x.floor() - play.world.things[1].pos.x.floor();
                    play.hold(&[], 1);
                    let diff =
                        play.world.things[0].pos.x.floor() - play.world.things[1].pos.x.floor();
                    if !play.world.things[0].active {
                        break;
                    }
                    assert_eq!(play.object().vx, speed(896));
                    assert_eq!(play.world.things[0].pos.y, floor);
                    if hit.is_none() && play.world.flow.events.contains(&FlowEvent::Defeat(1)) {
                        hit = Some((n, diff));
                        assert!(n == 0 || before < low, "{params} {facing} {offset}: late");
                        assert_eq!(play.world.things[1].vx, f(-4.0), "{facing} {offset}");
                    }
                }
                match hit {
                    Some((_, diff)) => assert!(
                        (low..=high).contains(&diff),
                        "{params} {facing} {offset}: hit at {diff}"
                    ),
                    None => panic!("{params} {facing} {offset}: no hit"),
                }
            }
        }
    }
}

/// metal-barrel.md, "Rolling right on the flat floor", walkers: after the
/// hit the walker's frames are those of thrown-objects.md, "The walker",
/// knocked left: -4 px a frame on K, -960/256 on K + 1, then the same
/// steps as a walker hit by an ordinary barrel.
#[test]
fn a_walker_hit_by_a_rolling_metal_barrel_is_knocked_off_as_by_a_barrel() {
    let run = |kind: &str| {
        let entity = format!(
            "[[entities]]\ntype = \"crawler\"\nat = [100, 400]\nparams = {{ {SMALL_WALKER} }}"
        );
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", kind);
        let mut play = Play::new(&src);
        play.throw_to(21 + 20);
        let at = play.world.player.pos;
        place(&mut play, at, 0, 0, false, true);
        play.world.things[0].pos.x = f(300.0);
        play.world.things[1].pos.x = f(300.0);
        let mut rows = Vec::new();
        for _ in 0..30 {
            play.hold(&[], 1);
            if play.world.flow.events.contains(&FlowEvent::Defeat(1)) || !rows.is_empty() {
                rows.push(walker(&play));
            }
        }
        rows
    };
    let metal = run("\"canister\"");
    assert_eq!(metal[0].2, f(-4.0));
    assert_eq!(metal[1].2, f(-3.75));
    assert!(metal.len() > 10);
    // The walker keeps being knocked off as by a barrel thrown the same way.
    let barrel = run("\"crate\"");
    let n = metal.len().min(barrel.len());
    assert_eq!(metal[..2.min(n)], barrel[..2.min(n)]);
}

/// metal-barrel.md, "Rolling left on the flat floor": a metal barrel
/// rolling left long after it left the hands defeats walkers by the
/// ranges of the "t of 17 or more" column, knocking them right at 4 px a
/// frame, and isn't slowed, lifted or removed by the hit.
#[test]
fn a_metal_barrel_rolling_left_defeats_walkers_by_the_late_ranges() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let large = SMALL_WALKER.replace("size = [20, 12]", "size = [26, 38]");
    // (walker, facing) -> the range of barrel x minus walker x.
    let kinds = [
        (SMALL_WALKER, [(-28, 22), (-27, 23)]),
        (medium.as_str(), [(-35, 20), (-25, 30)]),
        (large.as_str(), [(-31, 25), (-30, 26)]),
    ];
    for (params, ranges) in kinds {
        for (facing, (low, high)) in ["left", "right"].into_iter().zip(ranges) {
            for offset in (-118..=20).step_by(3) {
                let params = params
                    .replace("facing = \"left\"", &format!("facing = \"{facing}\""))
                    .replace("speed = 1", "speed = 0");
                let entity = format!(
                    "[[entities]]\ntype = \"crawler\"\nat = [100, 400]\nparams = {{ {params} }}"
                );
                let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
                let mut play = Play::new(&src);
                play.throw_to(21 + 20);
                let at = play.world.player.pos;
                place(&mut play, at, -1, 0, false, true);
                play.world.things[0].pos.x = f(300.0);
                play.world.things[1].pos.x = f(300.0 + offset as f64);
                let floor = play.world.things[0].pos.y;
                let mut hit = None;
                for n in 0..60 {
                    let before =
                        play.world.things[0].pos.x.floor() - play.world.things[1].pos.x.floor();
                    play.hold(&[], 1);
                    let diff =
                        play.world.things[0].pos.x.floor() - play.world.things[1].pos.x.floor();
                    if !play.world.things[0].active {
                        break;
                    }
                    assert_eq!(play.object().vx, speed(-896));
                    assert_eq!(play.world.things[0].pos.y, floor);
                    if hit.is_none() && play.world.flow.events.contains(&FlowEvent::Defeat(1)) {
                        hit = Some((n, diff));
                        assert!(n == 0 || before > high, "{params} {facing} {offset}: late");
                        assert_eq!(play.world.things[1].vx, f(4.0), "{facing} {offset}");
                    }
                }
                match hit {
                    Some((_, diff)) => assert!(
                        (low..=high).contains(&diff),
                        "{params} {facing} {offset}: hit at {diff}"
                    ),
                    None => panic!("{params} {facing} {offset}: no hit"),
                }
            }
        }
    }
}

/// metal-barrel.md, "Rolling left, thrown by the second character,
/// against walkers": the late ranges' lower ends, a walker not hit on the
/// first frame followed, its sideways speed 4 px per frame to the right on
/// the hit, and the barrel unaffected.
#[test]
fn a_metal_barrel_thrown_by_the_second_character_rolling_left_defeats_walkers() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let large = SMALL_WALKER.replace("size = [20, 12]", "size = [26, 38]");
    let kinds = [
        (SMALL_WALKER, [-28, -27]),
        (medium.as_str(), [-35, -25]),
        (large.as_str(), [-31, -30]),
    ];
    for (params, lows) in kinds {
        for (facing, low) in ["left", "right"].into_iter().zip(lows) {
            for diff in -50..=16 {
                let params = params
                    .replace("facing = \"left\"", &format!("facing = \"{facing}\""))
                    .replace("speed = 1", "speed = 0");
                let entity = format!(
                    "[[entities]]\ntype = \"crawler\"\nat = [100, 400]\nparams = {{ {params} }}"
                );
                let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
                let mut play = second_play(&src);
                play.throw_to(18 + 20);
                let at = play.world.player.pos;
                place(&mut play, at, -1, 0, false, true);
                play.world.things[0].pos.x = f(300.0);
                play.world.things[1].pos.x = f(300.0 - diff as f64);
                let floor = play.world.things[0].pos.y;
                let mut hit = None;
                for n in 0..60 {
                    play.hold(&[], 1);
                    assert!(play.world.things[0].active);
                    assert_eq!(play.object().vx, speed(-896));
                    assert_eq!(play.world.things[0].pos.y, floor);
                    if hit.is_none() && play.world.flow.events.contains(&FlowEvent::Defeat(1)) {
                        let at =
                            play.world.things[0].pos.x.floor() - play.world.things[1].pos.x.floor();
                        hit = Some((n, at));
                        assert_eq!(play.world.things[1].vx, f(4.0), "{facing} {diff}");
                    }
                }
                // Only the lower end is covered: a walker behind it is not hit.
                if diff < low {
                    assert_eq!(hit, None, "{facing} {diff}");
                } else if diff >= low + 4 {
                    let (n, at) = hit.unwrap_or_else(|| panic!("{facing} {diff}: no hit"));
                    assert!(at >= low, "{facing} {diff}: hit at {at}");
                    // In range when it is placed (the first frame followed):
                    // hit on the next one, the first step.
                    assert_eq!(n, 0, "{facing} {diff}: late");
                }
            }
        }
    }
}

/// metal-barrel.md, "Rolling left on the flat floor", walkers: the
/// walker's frames after the hit are those of thrown-objects.md, "The
/// walker", knocked right (d = -1): +4 px per frame on K, +960/256 on
/// K + 1, then as a walker hit by an ordinary barrel rolling left.
#[test]
fn a_walker_hit_by_a_metal_barrel_rolling_left_is_knocked_off_to_the_right() {
    let run = |kind: &str, facing: &str, offset: i32| {
        let params = SMALL_WALKER
            .replace("facing = \"left\"", &format!("facing = \"{facing}\""))
            .replace("speed = 1", "speed = 0");
        let entity =
            format!("[[entities]]\ntype = \"crawler\"\nat = [100, 400]\nparams = {{ {params} }}");
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", kind);
        let mut play = Play::new(&src);
        play.throw_to(21 + 20);
        let at = play.world.player.pos;
        place(&mut play, at, -1, 0, false, true);
        play.world.things[0].pos.x = f(440.0);
        play.world.things[1].pos.x = f(440.0 + offset as f64);
        let mut rows = Vec::new();
        for _ in 0..80 {
            play.hold(&[], 1);
            if play.world.flow.events.contains(&FlowEvent::Defeat(1)) || !rows.is_empty() {
                rows.push(walker(&play));
            }
        }
        rows
    };
    for facing in ["left", "right"] {
        for offset in (-100..=-10).step_by(3) {
            let metal = run("\"canister\"", facing, offset);
            assert_eq!(metal[0].2, f(4.0), "{facing} {offset}");
            assert_eq!(metal[1].2, speed(960), "{facing} {offset}");
            assert!(metal.len() > 10, "{facing} {offset}");
            // A walker hit by an ordinary barrel the same way follows the
            // same rule.
            let barrel = run("\"crate\"", facing, offset);
            let n = metal.len().min(barrel.len());
            assert_eq!(metal[..n], barrel[..n], "{facing} {offset}");
        }
    }
}

/// metal-barrel.md, "Rolling right, thrown by the second character": a
/// barrel thrown right, rolling on the flat floor 37 frames after it left
/// the hands, defeats a walker on the first frame its offset is within the
/// range's lower end (the table's "t of 17 or more" column, a large
/// walker facing right one pixel earlier in some runs), knocking it left
/// at 4 px per frame; the barrel is unaffected.
#[test]
fn a_metal_barrel_thrown_right_by_the_second_character_defeats_walkers() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let large = SMALL_WALKER.replace("size = [20, 12]", "size = [26, 38]");
    let kinds = [
        (SMALL_WALKER, [-23, -22]),
        (medium.as_str(), [-30, -20]),
        (large.as_str(), [-26, -25]),
    ];
    let run = |params: &str, facing: &str, diff: i32| {
        let params = params
            .replace("facing = \"left\"", &format!("facing = \"{facing}\""))
            .replace("speed = 1", "speed = 0");
        let entity =
            format!("[[entities]]\ntype = \"crawler\"\nat = [100, 400]\nparams = {{ {params} }}");
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
        let mut play = second_play(&src);
        play.throw_to(18 + 36);
        let barrel = play.world.things[0].pos.x;
        play.world.things[1].pos.x = barrel - f(diff as f64);
        let mut rows = Vec::new();
        let mut hit = None;
        let mut before = i64::from(diff);
        for n in 0..14 {
            play.hold(&[], 1);
            let thing = &play.world.things[0];
            rows.push((thing.pos, play.object().vx));
            if hit.is_none() && play.world.flow.events.contains(&FlowEvent::Defeat(1)) {
                hit = Some((n, before));
                assert_eq!(play.world.things[1].vx, f(-4.0), "{params} {diff}");
            }
            before = thing.pos.x.floor() - play.world.things[1].pos.x.floor();
        }
        (hit, rows)
    };
    for (params, lows) in kinds {
        for (facing, low) in ["left", "right"].into_iter().zip(lows) {
            // The second character is level with nothing on the way; the
            // barrel's own frames are the same with the walker far ahead.
            let (_, free) = run(params, facing, -300);
            for diff in -40..=3 {
                let (hit, rows) = run(params, facing, diff);
                assert_eq!(rows, free, "{params} {facing} {diff}: barrel changed");
                let (n, at) = hit.unwrap_or_else(|| panic!("{params} {facing} {diff}: no hit"));
                // `at` is the difference on the frame before the hit: the
                // first one in range, except that a large walker facing
                // right was hit a pixel early in some runs.
                assert!(n == 0 || at < low, "{params} {facing} {diff}: late");
            }
        }
    }
}

/// metal-barrel.md, "Stingers": a metal barrel rolling right defeats a
/// stinger that doesn't move on the frame after the end of the frame its
/// offsets (`x` rounded up less the stinger's rounded down, and the
/// stinger's height above the barrel, both `y` rounded up) are within
/// -28 to 32 and -28 to 19, and goes on unaffected. The stinger holds
/// still on the hit frame K, its speed across becoming -4, and on K + 1,
/// its speed up becoming -6, then moves by 3.75 px left and 5.5625 px up
/// on K + 2.
#[test]
fn a_rolling_metal_barrel_defeats_a_stinger_that_does_not_move() {
    for above in [-30, -29, -28, -21, -20, -5, 0, 10, 19, 20, 25] {
        for offset in (-20..=70).step_by(7) {
            let entity = format!(
                "[[entities]]\ntype = \"stinger\"\nat = [{}, {}]",
                300 + offset,
                400 - above
            );
            let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
            let mut play = Play::new(&src);
            play.throw_to(21 + 20);
            let at = play.world.player.pos;
            place(&mut play, at, 0, 0, false, true);
            play.world.things[0].pos.x = f(300.0);
            let mut hit_on = None;
            for n in 0..40 {
                play.hold(&[], 1);
                assert_eq!(play.object().vx, speed(896), "{above} {offset}");
                let stinger = &play.world.things[1];
                let ceil = |v: Fixed| -(-v).floor();
                let across = ceil(play.world.things[0].pos.x) - stinger.pos.x.floor();
                let up = ceil(play.world.things[0].pos.y) - ceil(stinger.pos.y);
                let inside = (-28..=32).contains(&across) && (-28..=19).contains(&up);
                if hit_on.is_none() {
                    assert_eq!(
                        stinger.phase == Phase::Struck(0),
                        inside,
                        "{above} {offset}: frame {n}"
                    );
                    if inside {
                        hit_on = Some(n);
                    }
                    continue;
                }
                // What a hit does to a stinger more than 20 px below isn't specified.
                if above < -20 {
                    break;
                }
                let start = stinger.pos;
                let k = n - hit_on.unwrap();
                match k {
                    1 => {
                        assert_eq!((stinger.vx, stinger.pos), (f(-4.0), start));
                        assert_eq!(stinger.phase, Phase::Struck(1));
                    }
                    2 => assert_eq!(stinger.phase, Phase::Knocked { vy: Some(f(-6.0)) }),
                    _ => {}
                }
                if k == 3 {
                    assert_eq!(stinger.vx, f(-960.0 / 256.0));
                    assert_eq!(
                        stinger.phase,
                        Phase::Knocked {
                            vy: Some(f(-5.5625))
                        }
                    );
                }
            }
            assert_eq!(
                hit_on.is_some(),
                (-28..=19).contains(&above),
                "{above} {offset}"
            );
        }
    }
}

/// metal-barrel.md, "A stinger that sways": a metal barrel rolling right
/// defeats a stinger that flies by the same ranges, with the barrel as it
/// was at the end of the frame before the hit frame K and the stinger as
/// it is at the end of K, after its own move. On K its speed across
/// becomes -4, on K + 1 it holds still and its speed up becomes -6.
#[test]
fn a_rolling_metal_barrel_defeats_a_stinger_that_sways() {
    let mut hits = 0;
    for params in [
        "reach = [20, 20, 0, 0], velocity = [-1.5, 0]",
        "reach = [10, 30, 10, 10], velocity = [1.0, 0.5]",
    ] {
        for above in [-10, 0, 10, 19] {
            for offset in (0..=90).step_by(11) {
                let entity = format!(
                    "[[entities]]\ntype = \"stinger\"\nat = [{}, {}]\nparams = {{ {params} }}",
                    300 + offset,
                    400 - above
                );
                let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
                let mut play = Play::new(&src);
                play.throw_to(21 + 20);
                let at = play.world.player.pos;
                place(&mut play, at, 0, 0, false, true);
                play.world.things[0].pos.x = f(300.0);
                let ceil = |v: Fixed| -(-v).floor();
                let mut hit_on = None;
                let mut barrel = play.world.things[0].pos;
                for n in 0..40 {
                    play.hold(&[], 1);
                    let stinger = &play.world.things[1];
                    let across = ceil(barrel.x) - stinger.pos.x.floor();
                    let up = ceil(barrel.y) - ceil(stinger.pos.y);
                    let inside = (-28..=32).contains(&across) && (-28..=19).contains(&up);
                    barrel = play.world.things[0].pos;
                    if let Some((k, held)) = hit_on {
                        let stinger = &play.world.things[1];
                        if n == k + 1 {
                            assert_eq!(
                                (stinger.phase, stinger.pos),
                                (Phase::Knocked { vy: Some(f(-6.0)) }, held),
                                "{params}"
                            );
                        }
                        continue;
                    }
                    let stinger = &play.world.things[1];
                    assert_eq!(
                        matches!(stinger.phase, Phase::Struck(_)),
                        inside,
                        "{params} {above} {offset}: frame {n}"
                    );
                    if inside {
                        assert_eq!(stinger.vx, f(-4.0));
                        hit_on = Some((n, stinger.pos));
                        hits += 1;
                    }
                }
            }
        }
    }
    assert!(hits > 10, "{hits}");
}

/// metal-barrel.md, "Stingers": a stinger knocked off still hurts a
/// standing player facing left on the floor, 5 px below it, on the frame
/// after the hit, for I from -27 to 26, as one that was not hit would.
#[test]
fn a_stinger_knocked_off_still_hurts_a_standing_player() {
    for i in [-29, -28, -27, -26, 0, 25, 26, 27, 28] {
        let entity = "[[entities]]\ntype = \"stinger\"\nat = [320, 395]";
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
        let mut play = Play::new(&src);
        play.throw_to(21 + 20);
        let at = play.world.player.pos;
        place(&mut play, at, 0, 0, false, true);
        play.world.things[0].pos.x = f(300.0);
        play.world.player.pos.x = f(f64::from(320 + i));
        play.world.player.facing = Facing::Left;
        let mut hit_on = None;
        let mut hurt_on = None;
        for n in 0..8 {
            play.hold(&[], 1);
            if hit_on.is_none() && play.world.things[1].phase == Phase::Struck(0) {
                hit_on = Some(n);
            }
            if hurt_on.is_none() && play.world.events.contains(&Event::Hurt) {
                hurt_on = Some(n);
            }
        }
        assert!(hit_on.is_some(), "{i}");
        let hurts = (-27..=26).contains(&i);
        assert_eq!(hurt_on, hurts.then(|| hit_on.unwrap() + 2), "{i}");
    }
}

/// metal-barrel.md, "Rolling left": as for rolling right, with the
/// barrel's `x` rounded to the nearest (halves up), the hit is at offsets
/// of -25 to 34 and -28 to 19, and the knock-off is mirrored: the
/// stinger's speed across becomes +4 and it moves 3.75 px right on
/// K + 2.
#[test]
fn a_metal_barrel_rolling_left_defeats_a_stinger_that_does_not_move() {
    for above in [-30, -29, -28, -21, -20, -5, 0, 10, 19, 20, 25] {
        for offset in (-50..=10).step_by(4) {
            let entity = format!(
                "[[entities]]\ntype = \"stinger\"\nat = [{}, {}]",
                330 + offset,
                400 - above
            );
            let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
            let mut play = Play::new(&src);
            play.throw_to(21 + 20);
            let at = play.world.player.pos;
            place(&mut play, at, -1, 0, false, true);
            play.world.things[0].pos.x = f(330.0);
            let mut hit_on = None;
            for n in 0..40 {
                play.hold(&[], 1);
                assert_eq!(play.object().vx, speed(-896), "{above} {offset}");
                let stinger = &play.world.things[1];
                let ceil = |v: Fixed| -(-v).floor();
                let near = (play.world.things[0].pos.x + f(0.5)).floor();
                let across = near - stinger.pos.x.floor();
                let up = ceil(play.world.things[0].pos.y) - ceil(stinger.pos.y);
                let inside = (-25..=34).contains(&across) && (-28..=19).contains(&up);
                if hit_on.is_none() {
                    assert_eq!(
                        stinger.phase == Phase::Struck(0),
                        inside,
                        "{above} {offset}: frame {n}"
                    );
                    if inside {
                        hit_on = Some(n);
                    }
                    continue;
                }
                // What a hit does to a stinger more than 20 px below isn't specified.
                if above < -20 {
                    break;
                }
                let start = stinger.pos;
                match n - hit_on.unwrap() {
                    1 => {
                        assert_eq!((stinger.vx, stinger.pos), (f(4.0), start));
                        assert_eq!(stinger.phase, Phase::Struck(1));
                    }
                    2 => assert_eq!(stinger.phase, Phase::Knocked { vy: Some(f(-6.0)) }),
                    3 => {
                        assert_eq!(stinger.vx, f(960.0 / 256.0));
                        assert_eq!(
                            stinger.phase,
                            Phase::Knocked {
                                vy: Some(f(-5.5625))
                            }
                        );
                    }
                    _ => {}
                }
            }
            assert_eq!(
                hit_on.is_some(),
                (-28..=19).contains(&above),
                "{above} {offset}"
            );
        }
    }
}

/// metal-barrel.md, "Rolling left, a stinger that sways": a metal barrel
/// rolling left defeats a stinger that flies with the barrel as it was at
/// the end of the frame before the hit frame K (`x` rounded to the
/// nearest) and the stinger as it is at the end of K: across -26 to 34,
/// above -28 to 19. On K its speed across becomes +4, on K + 1 it holds
/// still and its speed up becomes -6.
#[test]
fn a_metal_barrel_rolling_left_defeats_a_stinger_that_sways() {
    let mut hits = 0;
    for params in [
        "reach = [20, 20, 0, 0], velocity = [-1.5, 0]",
        "reach = [10, 30, 10, 10], velocity = [1.0, 0.5]",
    ] {
        for above in [-10, 0, 10, 19] {
            for offset in (-50..=10).step_by(5) {
                let entity = format!(
                    "[[entities]]\ntype = \"stinger\"\nat = [{}, {}]\nparams = {{ {params} }}",
                    330 + offset,
                    400 - above
                );
                let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
                let mut play = Play::new(&src);
                play.throw_to(21 + 20);
                let at = play.world.player.pos;
                place(&mut play, at, -1, 0, false, true);
                play.world.things[0].pos.x = f(330.0);
                let ceil = |v: Fixed| -(-v).floor();
                let mut hit_on = None;
                let mut barrel = play.world.things[0].pos;
                for n in 0..40 {
                    play.hold(&[], 1);
                    let stinger = &play.world.things[1];
                    let across = (barrel.x + f(0.5)).floor() - stinger.pos.x.floor();
                    let up = ceil(barrel.y) - ceil(stinger.pos.y);
                    let inside = (-26..=34).contains(&across) && (-28..=19).contains(&up);
                    barrel = play.world.things[0].pos;
                    if let Some((k, held)) = hit_on {
                        let stinger = &play.world.things[1];
                        if n == k + 1 {
                            assert_eq!(
                                (stinger.phase, stinger.pos),
                                (Phase::Knocked { vy: Some(f(-6.0)) }, held),
                                "{params}"
                            );
                        }
                        continue;
                    }
                    let stinger = &play.world.things[1];
                    assert_eq!(
                        matches!(stinger.phase, Phase::Struck(_)),
                        inside,
                        "{params} {above} {offset}: frame {n}"
                    );
                    if inside {
                        assert_eq!(stinger.vx, f(4.0));
                        hit_on = Some((n, stinger.pos));
                        hits += 1;
                    }
                }
            }
        }
    }
    assert!(hits > 10, "{hits}");
}

/// metal-barrel.md, "Rolling left, thrown by the second character":
/// as for the first character's barrel, but a stinger up to 28 px below
/// it is hit, not 20.
#[test]
fn a_metal_barrel_thrown_by_the_second_character_rolling_left_defeats_a_stinger_that_does_not_move()
{
    for above in [-31, -29, -28, -21, -20, -5, 0, 10, 19, 20, 25] {
        for offset in (-50..=10).step_by(4) {
            let entity = format!(
                "[[entities]]\ntype = \"stinger\"\nat = [{}, {}]",
                330 + offset,
                400 - above
            );
            let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
            let mut play = second_play(&src);
            play.throw_to(18);
            let at = play.world.player.pos;
            place(&mut play, at, -1, 0, false, true);
            play.world.things[0].pos.x = f(330.0);
            let mut hit_on = None;
            for n in 0..40 {
                play.hold(&[], 1);
                assert_eq!(play.object().vx, speed(-896), "{above} {offset}");
                let stinger = &play.world.things[1];
                let ceil = |v: Fixed| -(-v).floor();
                let near = (play.world.things[0].pos.x + f(0.5)).floor();
                let across = near - stinger.pos.x.floor();
                let up = ceil(play.world.things[0].pos.y) - ceil(stinger.pos.y);
                let inside = (-25..=34).contains(&across) && (-28..=19).contains(&up);
                if hit_on.is_none() {
                    assert_eq!(
                        stinger.phase == Phase::Struck(0),
                        inside,
                        "{above} {offset}: frame {n}"
                    );
                    if inside {
                        hit_on = Some(n);
                    }
                    continue;
                }
                let start = stinger.pos;
                match n - hit_on.unwrap() {
                    1 => {
                        assert_eq!((stinger.vx, stinger.pos), (f(4.0), start));
                        assert_eq!(stinger.phase, Phase::Struck(1));
                    }
                    2 => assert_eq!(stinger.phase, Phase::Knocked { vy: Some(f(-6.0)) }),
                    3 => {
                        assert_eq!(stinger.vx, f(960.0 / 256.0));
                        assert_eq!(
                            stinger.phase,
                            Phase::Knocked {
                                vy: Some(f(-5.5625))
                            }
                        );
                    }
                    _ => {}
                }
            }
            assert_eq!(
                hit_on.is_some(),
                (-28..=19).contains(&above),
                "{above} {offset}"
            );
        }
    }
}

/// metal-barrel.md, "Stingers": a stinger the second character's barrel
/// rolling left knocked off is removed on K + 2 when its `x` rounded
/// down, after that frame's move, is below the view's left edge less 31
/// (walker-view.md, "Defeated walkers falling out"), and not before.
#[test]
fn a_stinger_knocked_off_by_the_second_character_s_barrel_leaves_by_the_window() {
    // Where the stinger is after K + 2's move (3.75 px right of its
    // start): offset -10 is 323, which the view at 355 puts 32 left.
    for (offset, gone) in [
        (-30, true),
        (-12, true),
        (-10, true),
        (-9, false),
        (0, false),
    ] {
        let entity = format!(
            "[[entities]]\ntype = \"stinger\"\nat = [{}, 400]",
            330 + offset
        );
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
        let mut play = second_play(&src);
        play.throw_to(18);
        let at = play.world.player.pos;
        place(&mut play, at, -1, 0, false, true);
        play.world.things[0].pos.x = f(330.0);
        let mut hit_on = None;
        for n in 0..8 {
            play.world.view = Some(View {
                left: f(355.0),
                top: f(300.0),
                ..View::default()
            });
            play.hold(&[], 1);
            let stinger = &play.world.things[1];
            if hit_on.is_none() && stinger.phase == Phase::Struck(0) {
                hit_on = Some(n);
            }
            // The frame it is first struck is one before the spec's K,
            // on which its speed changes.
            let Some(k) = hit_on.map(|h| h + 1) else {
                continue;
            };
            assert_eq!(
                stinger.active,
                n < k + 2 || !gone,
                "{offset}: frame {n}, K at {k}",
            );
        }
        assert!(hit_on.is_some(), "{offset}");
    }
}

/// The same example's second case: 40 px further right, it's hit after
/// the object's first bounce, on Q + 43, by the smaller area.
#[test]
fn hits_a_walker_later_by_the_smaller_area() {
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    play.world.things[1].pos.x = start.x + f(200.25);
    for n in 22..=43 {
        play.step(&[]);
        let hit = play.world.things[1].phase != Phase::Ready;
        assert_eq!(hit, n == 43, "Q + {n}");
        if n == 43 {
            let object = play.world.things[0].pos;
            let w = play.world.things[1].pos;
            assert_eq!(object.x.floor() - w.x.floor(), -18);
            assert_eq!(w.y - object.y, f(8.4375));
        }
    }
}

/// thrown-objects.md, "When a walker is hit" and "The walker": a medium
/// walker facing left is hit by the object moving right on Q + 37 (t =
/// 16) with I = −51, by the larger area (−55 to 26), out of it on the
/// frame before. On flat floor it's there for the last time on K + 40,
/// 95 px below its floor.
#[test]
fn hits_a_medium_walker_which_falls_out_like_a_small_one() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {medium} }}");
    let mut play = Play::new(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    play.world.things[1].pos.x = start.x + f(200.25);
    let mut k = None;
    let mut i = 0;
    for n in 22..=100 {
        play.step(&[]);
        let w = &play.world.things[1];
        let before = i;
        i = play.world.things[0].pos.x.floor() - w.pos.x.floor();
        if k.is_none() && w.phase != Phase::Ready {
            assert_eq!((n, i), (37, -51));
            assert!(before < -55, "{before}");
            k = Some(n);
        }
        let Some(k) = k else { continue };
        let there = play.world.things[1].active;
        assert_eq!(there, n <= k + 40, "K + {}", n - k);
        if n == k + 40 {
            assert_eq!(play.world.things[1].pos.y.floor(), 495);
        }
    }
    assert!(k.is_some());
}

/// thrown-objects.md, "When a walker is hit", "Two walkers in range on
/// the same frame": only one of two large walkers is hit on a frame, the
/// same one whatever the offset between them, the other on a later frame
/// if still in range. Since spec-v0.320.0 also with the object moving
/// left, and for two medium walkers. Since spec-v0.323.0 also for a
/// large and a medium walker: the engine's walkers differ only in their
/// size parameters, and the rule here doesn't look at them. The same
/// holds since spec-v0.324.0 for a large or a medium walker with a small one.
#[test]
fn hits_one_of_two_walkers_in_range_on_a_frame() {
    for right in [true, false] {
        one_of_two_walkers_in_range(right);
    }
}

fn one_of_two_walkers_in_range(right: bool) {
    let large = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let x0 = if right { 300 } else { 100 };
    for offset in [-20.0, -8.0, 0.0, 8.0, 20.0] {
        let entities: String = (0..2)
            .map(|_| {
                format!(
                    "[[entities]]\ntype = \"crawler\"\nat = [{x0}, 400]\nparams = {{ {large} }}\n"
                )
            })
            .collect();
        let mut play = Play::new(&level(&format!("{FLAT}\n{entities}")));
        if !right {
            play.world.player.facing = Facing::Left;
        }
        play.throw_to(21);
        let start = play.world.things[0].pos;
        let ahead = if right { 200.25 } else { -200.25 };
        play.world.things[1].pos.x = start.x + f(ahead);
        play.world.things[2].pos.x = start.x + f(ahead + offset);
        let mut first = None;
        for _ in 22..=60 {
            play.step(&[]);
            let hit: Vec<usize> = (1..=2)
                .filter(|&i| play.world.flow.events.contains(&FlowEvent::Defeat(i)))
                .collect();
            assert!(hit.len() <= 1, "offset {offset}: {hit:?}");
            if let (None, [i]) = (first, hit.as_slice()) {
                first = Some(*i);
            }
        }
        assert!(first.is_some(), "right {right}, offset {offset}");
    }
}

/// A thrower at `(x, y)` that throws nothing for a long while.
fn thrower(x: i32, y: i32) -> String {
    format!(
        "[[entities]]\ntype = \"thrower\"\nat = [{x}, {y}]\n\
         params = {{ period = 5000, throws = [[4999, 0, -7]] }}"
    )
}

/// The thrower's `y` and whether it's been knocked out.
fn knocked_thrower(play: &Play) -> (Fixed, bool) {
    let t = &play.world.things[1];
    (t.pos.y, matches!(t.phase, Phase::Falling { .. }))
}

/// thrown-objects.md, "Hitting the thrower", "Example": the object
/// thrown right knocks out a thrower 115 px right of and 20 px below its
/// start on Q + 26, and goes on as without it; the thrower holds still
/// on K + 1 and falls from K + 2.
#[test]
fn knocks_out_the_thrower_and_goes_on() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(315, 366))));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(start, Vec2::new(f(200.0), f(346.0)));
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    for n in 22..=30 {
        play.step(&[]);
        control.step(&[]);
        let (y, knocked) = knocked_thrower(&play);
        assert_eq!(knocked, n >= 26, "Q + {n}");
        let want = match n {
            ..=27 => 20.0,
            28 => 15.4375,
            29 => 11.3125,
            _ => 7.625,
        };
        assert_eq!(y - start.y, f(want), "Q + {n}");
        assert!(play.world.things[0].active, "Q + {n}");
        assert_eq!(play.world.things[0].phase, control.world.things[0].phase);
        assert_eq!(play.world.things[0].pos, control.world.things[0].pos);
    }
    assert!(
        play.world
            .flow
            .events
            .iter()
            .all(|e| *e != FlowEvent::Defeat(1))
    );
}

/// thrower-contact.md, "The knocked-out thrower": after a hit by an
/// object, on each frame from K + 2 to K + 44 the thrower neither hurts
/// the player nor gives a bounce, wherever the player is around it.
#[test]
fn the_thrower_an_object_knocked_out_touches_nothing() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(315, 366))));
    play.throw_to(21);
    for n in 22..=70 {
        let thrower = play.world.things[1].pos;
        if n >= 28 {
            for (dx, dy, vy) in [(0, 0, 2.0), (-20, 10, 2.0), (14, -6, -1.0), (30, 30, 2.0)] {
                let mut probe = Play {
                    world: play.world.clone(),
                    held: play.held,
                };
                let player = &mut probe.world.player;
                player.pos = Vec2::new(thrower.x + f(dx as f64), thrower.y + f(dy as f64));
                player.vel = Vec2::new(Fixed::ZERO, f(vy));
                probe.step(&[]);
                let events = &probe.world.events;
                assert!(!events.contains(&Event::Hurt), "Q + {n} {dx} {dy}");
                assert!(!events.contains(&Event::Bounce), "Q + {n} {dx} {dy}");
            }
        }
        play.step(&[]);
    }
}

/// Throws the crate, right or left, and on Q + `n` (from 22) puts the
/// thrower `i` px left of where the object ends the frame (I, both `x`
/// rounded down) and `h` px below it (H, both `y` rounded up). Returns
/// whether the thrower was knocked out on that frame.
fn thrower_hit(right: bool, n: usize, i: i64, h: i64) -> bool {
    let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(300, 100))));
    if !right {
        play.world.player.facing = Facing::Left;
    }
    play.throw_to(n - 1);
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.step(&[]);
    let object = control.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    let x = object.x.floor() - i;
    let y = up(object.y) + h;
    play.world.things[1].pos = Vec2::new(
        Fixed::from_int(x as i32) + f(0.75),
        Fixed::from_int(y as i32) - f(0.25),
    );
    play.step(&[]);
    assert_eq!(play.world.things[0].pos, object);
    knocked_thrower(&play).1
}

/// thrown-objects.md, "When the thrower is hit": the ranges of I and H,
/// ends included, early (t up to 16) and late, moving right and left.
#[test]
fn hits_the_thrower_in_the_measured_ranges() {
    for (right, n, (i_from, i_to), (h_from, h_to)) in [
        (true, 22, (-55, 37), (-47, 65)),
        (true, 37, (-55, 37), (-47, 65)),
        (true, 38, (-30, 36), (-23, 32)),
        (false, 22, (-36, 56), (-47, 65)),
        (false, 38, (-35, 31), (-23, 32)),
    ] {
        for (i, h, want) in [
            (i_from, 0, true),
            (i_to, 0, true),
            (i_from - 1, 0, false),
            (i_to + 1, 0, false),
            (0, h_from, true),
            (0, h_to, true),
            (0, h_from - 1, false),
            (0, h_to + 1, false),
        ] {
            let got = thrower_hit(right, n, i, h);
            assert_eq!(got, want, "right {right} Q + {n} I {i} H {h}");
        }
    }
}

/// thrown-objects.md, "What happens on a hit": still in the hands during
/// a throw, from Q + 1 to Q + 20, it breaks on a thrower by the early
/// ranges, and the throw goes on as without it. Not on Q or Q + 21.
#[test]
fn breaks_on_the_thrower_in_the_hands() {
    for n in 0..=21 {
        let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(300, 100))));
        let mut control = Play {
            world: play.world.clone(),
            held: play.held,
        };
        control.world.things[1].active = false;
        if n > 0 {
            play.throw_to(n - 1);
            control.throw_to(n - 1);
        }
        // The thrower 55 px right of where the object ends the step on
        // the player's head, the early range's end.
        let mut ahead = Play {
            world: control.world.clone(),
            held: control.held,
        };
        ahead.step(&[]);
        let head = ahead.world.things[0].pos;
        play.world.things[1].pos = Vec2::new(head.x + f(55.0), head.y);
        play.step(&[]);
        control.step(&[]);
        let hit = (1..=20).contains(&n);
        assert_eq!(knocked_thrower(&play).1, hit, "Q + {n}");
        assert_eq!(play.world.things[0].active, !hit, "Q + {n}");
        if hit {
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.hands, control.world.player.hands);
            // The rest of the throw goes on and throws nothing.
            play.throw_to(30);
            control.throw_to(30);
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.state, control.world.player.state);
            assert!(!play.world.things[0].active);
        }
    }
}

/// thrown-objects.md, "What happens on a hit": carried, it breaks on a
/// thrower by the late ranges, the way the player faces. The player
/// goes on as without it, carrying nothing, and letting go of action
/// throws nothing.
#[test]
fn breaks_on_the_thrower_while_carried() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{}", thrower(300, 100))));
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    // A jump, carrying.
    while play.world.player.grounded() {
        play.step(&[A, Action::Jump]);
        control.step(&[A, Action::Jump]);
    }
    play.world.player.facing = Facing::Left;
    control.world.player.facing = Facing::Left;
    // Where the object ends the next step, from the control.
    let mut ahead = Play {
        world: control.world.clone(),
        held: control.held,
    };
    ahead.step(&[A]);
    let head = ahead.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    // I of 32 isn't hit; 31, the late range's end facing left, is.
    let at = |i: i64| {
        Vec2::new(
            Fixed::from_int((head.x.floor() - i) as i32),
            Fixed::from_int(up(head.y) as i32),
        )
    };
    let mut miss = Play {
        world: play.world.clone(),
        held: play.held,
    };
    miss.world.things[1].pos = at(32);
    miss.step(&[A]);
    assert!(!knocked_thrower(&miss).1);
    assert!(miss.world.things[0].active);
    play.world.things[1].pos = at(31);
    play.step(&[A]);
    control.step(&[A]);
    assert!(knocked_thrower(&play).1);
    assert!(!play.world.things[0].active);
    assert!(play.world.player.hands.is_none());
    for _ in 0..40 {
        assert_eq!(play.world.player.pos, control.world.player.pos);
        assert_eq!(play.world.player.vel, control.world.player.vel);
        play.step(&[]);
        control.step(&[]);
    }
    assert!(play.world.player.hands.is_none());
    assert!(!play.world.things[0].active);
}

/// A one-way face breaks it as a solid one does (thrown-objects.md,
/// "Breaking against a wall"): the "Example" throw breaks on the same
/// frame.
#[test]
fn breaks_against_a_one_way_face() {
    let mut play = Play::new(&level(&face_of("one_way = true")));
    play.throw_to(53);
    assert!(play.world.things[0].active);
    play.step(&[]);
    assert!(!play.world.things[0].active);
}

/// Puts the thrown object at `(x, y)` with no speed across and `vy`
/// (before this step's gravity), and steps once. Returns its `y` and
/// `vy`.
fn from(play: &mut Play, x: f64, y: f64, vy: f64) -> (Fixed, Fixed) {
    let mut flight = play.object();
    (flight.vx, flight.target, flight.vy, flight.rolling) =
        (Fixed::ZERO, Fixed::ZERO, f(vy), false);
    play.world.things[0].pos = Vec2::new(f(x), f(y));
    play.world.things[0].phase = Phase::Thrown(flight);
    play.step(&[]);
    (play.world.things[0].pos.y, play.object().vy)
}

/// thrown-objects.md, "Ground passable from below": rising from below
/// into ground that isn't one-way, it keeps its whole px and stops; a
/// one-way platform it rises into.
#[test]
fn is_stopped_rising_into_solid_ground_from_below() {
    for (one_way, want) in [(false, (325.6875, 0.0)), (true, (319.6875, -5.5625))] {
        let ceiling = format!(
            "[[collision]]\nshape = \"rect\"\nrect = [180, 300, 100, 20]\none_way = {one_way}"
        );
        let mut play = Play::new(&level(&format!("{FLAT}\n{ceiling}")));
        play.throw_to(21);
        assert_eq!(
            from(&mut play, 200.5, 325.25, -6.0),
            (f(want.0), f(want.1)),
            "{one_way}"
        );
        // It falls from there.
        play.step(&[]);
        assert_eq!(play.object().vy, f(want.1 + 0.4375));
    }
}

/// thrown-objects.md, "In the air" and "The floor of a column": in a
/// platform's layer up to a step below its top and not moving down, it's
/// lifted by as much as the top is above its `y` rounded up; lower in the
/// layer it moves on as if there were no floor.
#[test]
fn is_lifted_onto_a_platform_a_step_above_it() {
    let platform = "[[collision]]\nshape = \"rect\"\nrect = [180, 300, 100, 16]\none_way = true";
    let src = level(&format!("{FLAT}\n{platform}"));
    for (y, vy, want) in [
        // New y 303.6875, lifted by 306 - 300.
        (305.25, -2.0, (297.6875, -1.5625)),
        // Not moving: lifted by 305 - 300.
        (304.5625, -0.4375, (299.5625, 0.0)),
        // More than a step below the top: no floor.
        (310.0, -2.0, (308.4375, -1.5625)),
        // Below the underside, it rises into the layer.
        (317.0, -2.0, (315.4375, -1.5625)),
    ] {
        let mut play = Play::new(&src);
        play.throw_to(21);
        assert_eq!(from(&mut play, 200.5, y, vy), (f(want.0), f(want.1)), "{y}");
    }
    // Moving down, it touches the top instead and bounces.
    let mut play = Play::new(&src);
    play.throw_to(21);
    let (y, vy) = from(&mut play, 200.5, 305.25, 2.0);
    assert_eq!(y, f(300.0));
    assert!(vy < Fixed::ZERO);
}

/// A ball placed out of the way, which tests put where they want with
/// [`put_ball`].
const BALL: &str = "[[entities]]\ntype = \"ball\"\nat = [1000, 100]";

/// Puts the ball (`things[1]`) at `pos`, falling at `vy` (before the
/// next step's gravity), after `steps` steps of its turn.
fn put_ball(play: &mut Play, pos: Vec2, vy: Fixed, steps: u32) {
    let ball = &mut play.world.things[1];
    ball.active = true;
    ball.pos = pos;
    ball.vx = Fixed::ZERO;
    ball.clock = steps;
    ball.phase = Phase::Flying {
        vy,
        ignoring: None,
        kept: false,
    };
}

/// thrown-objects.md, "Hitting a ball", "Example": a ball falling
/// straight down 74 px right of the object's start is hit on Q + 24,
/// not before, and the object goes on as without it.
#[test]
fn removes_a_ball_and_goes_on() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
    play.throw_to(21);
    let start = play.world.things[0].pos;
    assert_eq!(start, Vec2::new(f(200.0), f(346.0)));
    // At the end of Q + 21, in the fourth part of its turn.
    put_ball(
        &mut play,
        Vec2::new(start.x + f(74.0), start.y - f(30.0)),
        f(3.0625),
        22,
    );
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    for (n, ball_y) in [(22, -26.5), (23, -22.5625), (24, 0.0), (25, 0.0)] {
        play.step(&[]);
        control.step(&[]);
        let ball = &play.world.things[1];
        assert_eq!(ball.active, n < 24, "Q + {n}");
        if ball.active {
            assert_eq!(ball.pos.y - start.y, f(ball_y), "Q + {n}");
        }
        assert!(play.world.things[0].active, "Q + {n}");
        assert_eq!(play.world.things[0].pos, control.world.things[0].pos);
        assert_eq!(play.world.things[0].phase, control.world.things[0].phase);
    }
    assert_eq!(play.world.things[0].pos.x - start.x, f(50.2109375));
}

/// Throws the crate, right or left, and before Q + `n` (from 22) puts
/// the ball, `steps` steps into its turn, `i` px left of where the
/// object ends the frame (I, both `x` rounded down) and `h` px below it
/// (H, both `y` rounded up). Returns whether the ball was hit on that
/// frame. Without a view, the ball isn't removed for leaving the
/// picture.
fn ball_hit(right: bool, n: usize, steps: u32, i: i64, h: i64) -> bool {
    let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
    if !right {
        play.world.player.facing = Facing::Left;
    }
    play.throw_to(n - 1);
    play.world.view = None;
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    control.step(&[]);
    let object = control.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    let x = object.x.floor() - i;
    let y = up(object.y) + h;
    put_ball(
        &mut play,
        Vec2::new(
            Fixed::from_int(x as i32) + f(0.75),
            Fixed::from_int(y as i32) - f(0.25),
        ),
        Fixed::ZERO,
        steps,
    );
    // Without the object, nothing else removes the ball.
    let mut alone = Play {
        world: play.world.clone(),
        held: play.held,
    };
    alone.world.things[0].active = false;
    alone.step(&[]);
    assert!(
        alone.world.things[1].active,
        "alone: right {right} Q + {n} steps {steps} I {i} H {h}"
    );
    play.step(&[]);
    assert_eq!(play.world.things[0].pos, object);
    assert!(play.world.things[0].active);
    !play.world.things[1].active
}

/// thrown-objects.md, "When a ball is hit": the ranges of I and H, ends
/// included, early (t up to 16) and late, moving right and left, in
/// each part of the ball's turn.
#[test]
fn hits_a_ball_in_the_measured_ranges() {
    for (right, n, (i_from, i_to), (h_from, h_to)) in [
        (true, 22, (-47, 27), (-54, 48)),
        (true, 37, (-47, 27), (-54, 48)),
        (true, 38, (-22, 26), (-30, 15)),
        (false, 22, (-28, 46), (-54, 48)),
        (false, 38, (-27, 21), (-30, 15)),
    ] {
        // Steps into the first to fourth parts, and the shifts of I
        // and H there.
        for (steps, di, dh) in [(0, 0, 0), (9, 0, -3), (15, 1, -3), (21, 1, 0)] {
            let (i_from, i_to, h_from, h_to) = (i_from + di, i_to + di, h_from + dh, h_to + dh);
            for (i, h, want) in [
                (i_from, 0, true),
                (i_to, 0, true),
                (i_from - 1, 0, false),
                (i_to + 1, 0, false),
                (0, h_from, true),
                (0, h_to, true),
                (0, h_from - 1, false),
                (0, h_to + 1, false),
            ] {
                let got = ball_hit(right, n, steps, i, h);
                assert_eq!(got, want, "right {right} Q + {n} steps {steps} I {i} H {h}");
            }
        }
    }
}

/// thrown-objects.md, "What happens on a hit": in the hands during a
/// throw, from Q + 1 to Q + 20, it breaks on a ball by the early
/// ranges and removes it, and the throw goes on as without it. Not on
/// Q or Q + 21.
#[test]
fn breaks_on_a_ball_in_the_hands() {
    for n in 0..=21 {
        let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
        if n > 0 {
            play.throw_to(n - 1);
        }
        let mut control = Play {
            world: play.world.clone(),
            held: play.held,
        };
        control.world.things[1].active = false;
        let mut ahead = Play {
            world: control.world.clone(),
            held: control.held,
        };
        ahead.step(&[]);
        let head = ahead.world.things[0].pos;
        // 47 px right of where the object ends the step, the early
        // range's end.
        put_ball(
            &mut play,
            Vec2::new(head.x + f(47.0), head.y),
            Fixed::ZERO,
            0,
        );
        play.step(&[]);
        control.step(&[]);
        let hit = (1..=20).contains(&n);
        assert_eq!(play.world.things[1].active, !hit, "Q + {n}");
        assert_eq!(play.world.things[0].active, !hit, "Q + {n}");
        if hit {
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.hands, control.world.player.hands);
            play.throw_to(30);
            control.throw_to(30);
            assert_eq!(play.world.player.pos, control.world.player.pos);
            assert_eq!(play.world.player.state, control.world.player.state);
            assert!(!play.world.things[0].active);
        }
    }
}

/// thrown-objects.md, "What happens on a hit": carried, it breaks on a
/// ball by the late ranges, the way the player faces, and the player
/// carries nothing from then on.
#[test]
fn breaks_on_a_ball_while_carried() {
    let mut play = Play::new(&level(&format!("{FLAT}\n{BALL}")));
    play.world.player.facing = Facing::Left;
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    control.step(&[A]);
    let head = control.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    // Facing left, I of 22 isn't hit; 21, the late range's end, is.
    let at = |i: i64| {
        Vec2::new(
            Fixed::from_int((head.x.floor() - i) as i32),
            Fixed::from_int(up(head.y) as i32),
        )
    };
    let mut miss = Play {
        world: play.world.clone(),
        held: play.held,
    };
    put_ball(&mut miss, at(22), Fixed::ZERO, 0);
    miss.step(&[A]);
    assert!(miss.world.things[1].active);
    assert!(miss.world.things[0].active);
    put_ball(&mut play, at(21), Fixed::ZERO, 0);
    play.step(&[A]);
    assert!(!play.world.things[1].active);
    assert!(!play.world.things[0].active);
    assert!(play.world.player.hands.is_none());
    for _ in 0..30 {
        assert_eq!(play.world.player.pos, control.world.player.pos);
        play.step(&[]);
        control.step(&[]);
    }
    assert!(play.world.player.hands.is_none());
}

/// thrown-objects.md, "Hitting a ball", "The ball placed in the level":
/// a ball placed with speeds (thrower.md, "Balls placed in a level") is
/// hit by the same ranges as the thrower's balls: carried facing left, it
/// breaks the object at 21 px and not at 22.
#[test]
fn breaks_on_a_placed_ball_while_carried() {
    let placed = "[[entities]]\ntype = \"ball\"\nat = [1000, 100]\n\
        params = { velocity = [-1.171875, -3] }";
    let mut play = Play::new(&level(&format!("{FLAT}\n{placed}")));
    play.world.player.facing = Facing::Left;
    play.world.view = None;
    let mut control = Play {
        world: play.world.clone(),
        held: play.held,
    };
    control.world.things[1].active = false;
    control.step(&[A]);
    let head = control.world.things[0].pos;
    let up = |v: Fixed| -(-v).floor();
    let at = |i: i64| {
        Vec2::new(
            Fixed::from_int((head.x.floor() - i) as i32),
            Fixed::from_int(up(head.y) as i32),
        )
    };
    for (i, hit) in [(22, false), (21, true)] {
        let mut trial = Play {
            world: play.world.clone(),
            held: play.held,
        };
        put_ball(&mut trial, at(i), Fixed::ZERO, 0);
        trial.step(&[A]);
        assert_eq!(trial.world.things[0].active, !hit, "I {i}");
        assert_eq!(trial.world.things[1].active, !hit, "I {i}");
    }
}

/// A 64 px vertical step up to the left at x = 100: floor at y = 336
/// left of it, the upper side, and at y = 400 from it on, the lower side.
const STEP_UP: &str = "[[collision]]\nshape = \"polygon\"\n\
    points = [[0, 600], [0, 336], [100, 336], [100, 400], [3000, 400], [3000, 600]]";

/// Puts the thrown object at `(x, y)` moving across at `vx` and down at
/// `vy` (before this step's gravity), and steps once. Returns whether
/// it's still there, and its `x`.
fn toward(play: &mut Play, x: f64, y: f64, vx: f64, vy: f64) -> (bool, Fixed) {
    let mut flight = play.object();
    (flight.vx, flight.target, flight.vy, flight.rolling) = (f(vx), f(vx), f(vy), false);
    play.world.things[0].pos = Vec2::new(f(x), f(y));
    play.world.things[0].phase = Phase::Thrown(flight);
    play.step(&[]);
    let thing = &play.world.things[0];
    (thing.active, thing.pos.x)
}

/// thrown-objects.md, "Breaking at a vertical step": moving into the
/// upper side, it breaks without moving if its `y` before the frame is
/// more than 8 px below the upper floor, whatever its `vy`; otherwise it
/// goes on into the column.
#[test]
fn breaks_at_a_vertical_step_more_than_8_px_below_its_top() {
    for vy in [-4.0, -1.0, 0.0, 1.0, 4.0] {
        for (y, breaks) in [
            (344.0625, true),
            (360.0, true),
            (399.0, true),
            (344.0, false),
        ] {
            let mut play = Play::new(&level(STEP_UP));
            play.throw_to(21);
            let (active, x) = toward(&mut play, 100.5, y, -1.0, vy);
            assert_eq!(active, !breaks, "y {y}, vy {vy}");
            if !breaks {
                assert_eq!(x, f(99.5), "y {y}, vy {vy}");
            }
        }
    }
    // Well above the upper floor, it flies on over it.
    let mut play = Play::new(&level(STEP_UP));
    play.throw_to(21);
    assert_eq!(toward(&mut play, 100.5, 300.0, -1.0, 0.0), (true, f(99.5)));
}

/// thrown-objects.md, "Breaking at a vertical step": it breaks on the
/// frame whose move would take `x` into the upper side's first column,
/// not on one ending on the left edge of the lower side's last column.
#[test]
fn breaks_at_a_vertical_step_only_once_inside_the_upper_side() {
    let mut play = Play::new(&level(STEP_UP));
    play.throw_to(21);
    assert_eq!(toward(&mut play, 100.5, 360.0, -0.5, 0.0), (true, f(100.0)));
    let mut play = Play::new(&level(STEP_UP));
    play.throw_to(21);
    assert!(!toward(&mut play, 100.5, 360.0, -0.5625, 0.0).0);
}

/// thrown-objects.md, "Breaking at a vertical step": an object in the
/// hands isn't broken by the step. Walking into it, the player is
/// stopped there; jumping, it gets onto the upper side.
#[test]
fn a_carried_object_isnt_broken_at_a_vertical_step() {
    const L: Action = Action::Left;
    const J: Action = Action::Jump;
    let mut play = Play::new(&level(STEP_UP));
    play.hold(&[A, L], 180);
    // Stopped at the left edge of its column (steep-faces.md, "Stopped
    // by a rise"): moving this fast, the one before the step's.
    assert_eq!(play.world.player.pos.x, f(101.0));
    assert_eq!(play.world.player.pos.y, f(400.0));
    assert!(play.world.player.hands.is_some());
    assert!(play.world.things[0].active);
    for _ in 0..4 {
        play.hold(&[A, L, J], 30);
        play.hold(&[A, L], 10);
    }
    assert!(play.world.player.pos.x < f(100.0));
    assert_eq!(play.world.player.pos.y, f(336.0));
    assert!(play.world.player.hands.is_some());
    assert!(play.world.things[0].active);
}

/// What a character's motion and its hurts are made of: the lead, the
/// other character, and the lives and hit points.
fn characters(play: &Play) -> String {
    let w = &play.world;
    format!("{:?} {:?} {:?} {:?}", w.player, w.other, w.flow, w.loss)
}

/// Steps 12 frames from `play` three ways: as it is, with the object gone,
/// and with `moved` (a character moved 200 px away) applied. Each
/// character moves as without the object, the object as without the
/// character, and no one is hurt.
fn passes_through(play: &Play, moved: impl Fn(&mut World), what: &str) {
    let mut with = Play {
        world: play.world.clone(),
        held: play.held,
    };
    let mut without = Play {
        world: play.world.clone(),
        held: play.held,
    };
    without.world.things[0].active = false;
    let mut away = Play {
        world: play.world.clone(),
        held: play.held,
    };
    moved(&mut away.world);
    for n in 1..=12 {
        with.step(&[]);
        without.step(&[]);
        away.step(&[]);
        assert_eq!(characters(&with), characters(&without), "{what}, frame {n}");
        let object = &with.world.things[0];
        assert_eq!(
            format!("{object:?}"),
            format!("{:?}", away.world.things[0]),
            "{what}, frame {n}"
        );
        assert!(with.world.hit_by.is_none(), "{what}, frame {n}");
    }
}

/// Puts the object `across` px from `at` and `up` px above it, moving
/// 3.5 px per frame toward `at` or away from it, in the air or rolling.
fn place(play: &mut Play, at: Vec2, across: i32, up: i32, toward: bool, rolling: bool) {
    let thing = &mut play.world.things[0];
    let Phase::Thrown(mut flight) = thing.phase else {
        panic!("{:?}", thing.phase);
    };
    thing.pos = Vec2::new(at.x + Fixed::from_int(across), at.y - Fixed::from_int(up));
    let right = (across < 0) == toward;
    flight.vx = if right { speed(896) } else { speed(-896) };
    flight.target = flight.vx;
    flight.rolling = rolling;
    if rolling {
        flight.vy = Fixed::ZERO;
    }
    thing.phase = Phase::Thrown(flight);
}

/// thrown-objects.md, "Meeting a character": a thrown object passes
/// through the lead, from t = 1 (Q + 22) while it's still in its throw,
/// standing facing either way or in a jump.
#[test]
fn passes_through_the_lead() {
    for t in [1, 9, 28, 40, 60] {
        for jump in [false, true] {
            for turn in [false, true] {
                let mut base = Play::new(&level(FLAT));
                base.throw_to(21 + t);
                if turn {
                    base.hold(&[Action::Left], 1);
                }
                if jump {
                    base.hold(&[Action::Jump], 6);
                }
                for across in (-24..=24).step_by(8) {
                    let ups: Vec<i32> = if jump {
                        (-48..=16).step_by(16).collect()
                    } else {
                        (0..=64).step_by(16).collect()
                    };
                    for up in ups {
                        for toward in [false, true] {
                            for rolling in [false, true] {
                                if rolling && (jump || up != 0) {
                                    continue;
                                }
                                let mut play = Play {
                                    world: base.world.clone(),
                                    held: base.held,
                                };
                                let at = play.world.player.pos;
                                place(&mut play, at, across, up, toward, rolling);
                                passes_through(
                                    &play,
                                    |w| w.player.pos.x += Fixed::from_int(200),
                                    &format!(
                                        "t = {t}, jump {jump}, turn {turn}, \
                                         {across} across, {up} up, toward {toward}, \
                                         rolling {rolling}"
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// thrown-objects.md, "Meeting a character": it passes through the
/// other character too, with the lead there, from t = 1 on.
#[test]
fn passes_through_the_other_character() {
    for t in [1, 9, 28, 40, 60] {
        let mut base = Play::with_other(&level(FLAT));
        base.throw_to(21 + t);
        for across in (-24..=24).step_by(8) {
            for up in (0..=64).step_by(16) {
                for toward in [false, true] {
                    for rolling in [false, true] {
                        if rolling && up != 0 {
                            continue;
                        }
                        let mut play = Play {
                            world: base.world.clone(),
                            held: base.held,
                        };
                        let at = play.world.other.as_ref().unwrap().pos;
                        place(&mut play, at, across, up, toward, rolling);
                        passes_through(
                            &play,
                            |w| w.other.as_mut().unwrap().pos.x += Fixed::from_int(200),
                            &format!(
                                "t = {t}, {across} across, {up} up, toward {toward}, \
                                 rolling {rolling}"
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// [`Play::new`] with the second character under control.
fn second_play(src: &str) -> Play {
    let level = Arc::new(Level::from_toml_str(src, "thrown").unwrap());
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: feel.roster[0].abilities.clone(),
    });
    let mut world = World::new(level, Arc::new(feel));
    world.player.character = 1;
    let mut play = Play {
        world,
        held: Actions::NONE,
    };
    play.hold(&[], 2);
    play.hold(&[A], 21);
    assert!(play.world.player.hands.is_some());
    play
}

/// thrown-objects.md, "The second character": the object leaves on Q +
/// 18 and t = 1 is Q + 19, so the larger area holds through Q + 34 and
/// the smaller one from Q + 35.
#[test]
fn hits_a_walker_by_the_second_character_s_object_three_frames_sooner() {
    let medium = SMALL_WALKER.replace(
        "size = [20, 12], offset = 0.5",
        "size = [25, 25], offset = 5.0",
    );
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {medium} }}");
    let mut play = second_play(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(18);
    let start = play.world.things[0].pos;
    play.world.things[1].pos.x = start.x + f(200.25);
    let mut k = None;
    let mut i = 0;
    for n in 19..=60 {
        play.step(&[]);
        let w = &play.world.things[1];
        let before = i;
        i = play.world.things[0].pos.x.floor() - w.pos.x.floor();
        if k.is_none() && w.phase != Phase::Ready {
            assert_eq!((n, i), (34, -51));
            assert!(before < -55, "{before}");
            k = Some(n);
        }
    }
    assert!(k.is_some());
}

/// The same, further away: a small walker is hit only by the smaller
/// area, from Q + 35 on.
#[test]
fn hits_a_walker_by_the_second_character_s_object_later_by_the_smaller_area() {
    let entity =
        format!("[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}");
    let mut play = second_play(&level(&format!("{FLAT}\n{entity}")));
    play.throw_to(18);
    let start = play.world.things[0].pos;
    play.world.things[1].pos.x = start.x + f(200.25);
    let mut hit = None;
    for n in 19..=60 {
        play.step(&[]);
        if hit.is_none() && play.world.things[1].phase != Phase::Ready {
            hit = Some(n);
        }
    }
    assert!(hit.is_some_and(|n| n >= 35), "{hit:?}");
}

/// metal-barrel.md, "The thrower and its balls": carried, a metal barrel
/// that meets a ball or a thrower doesn't break. It moves up at 5 px per
/// frame, then falls by 0.4375 a frame to 8, straight down through the
/// floor, and is removed far below. The player carries nothing.
#[test]
fn a_metal_barrel_in_the_hands_falls_through_the_floor_on_a_ball_or_a_thrower() {
    for with_ball in [true, false] {
        let more = if with_ball {
            BALL.to_string()
        } else {
            thrower(300, 100)
        };
        let src = level(&format!("{FLAT}\n{more}")).replace("\"crate\"", "\"canister\"");
        let mut play = Play::new(&src);
        play.world.player.facing = Facing::Left;
        let mut control = Play {
            world: play.world.clone(),
            held: play.held,
        };
        control.world.things[1].active = false;
        control.step(&[A]);
        let head = control.world.things[0].pos;
        let up = |v: Fixed| -(-v).floor();
        let at = Vec2::new(
            Fixed::from_int(head.x.floor() as i32 - 20),
            Fixed::from_int(up(head.y) as i32),
        );
        if with_ball {
            put_ball(&mut play, at, Fixed::ZERO, 0);
        } else {
            play.world.things[1].pos = at;
        }
        play.step(&[A]);
        assert!(play.world.player.hands.is_none());
        let thing = &play.world.things[0];
        assert!(thing.active, "ball: {with_ball}");
        let x = thing.pos.x;
        let Phase::LetGo { vy, .. } = thing.phase else {
            panic!("{:?}", thing.phase);
        };
        assert_eq!(vy, f(-5.0));
        let mut last = vy;
        let mut steps = 0;
        while play.world.things[0].active {
            play.step(&[]);
            let thing = &play.world.things[0];
            if let Phase::LetGo { vy, .. } = thing.phase {
                assert_eq!(vy, (last + f(0.4375)).min(f(8.0)));
                last = vy;
                assert_eq!(thing.pos.x, x);
            }
            steps += 1;
            assert!(steps < 200);
        }
        assert!(last == f(8.0), "reached its top speed");
        assert!(play.world.things[0].pos.y > f(400.0), "through the floor");
    }
}

/// metal-barrel.md, "The thrower and its balls": a metal barrel thrown
/// either way hits the thrower on the frames a barrel does, and carried
/// in a jump it does too, then falls through the floor like one in the
/// hands; the thrower and the player go on as with a barrel.
#[test]
fn a_metal_barrel_meets_the_thrower_as_a_barrel_does() {
    let plain = level(&format!("{FLAT}\n{}", thrower(300, 100)));
    let metal = plain.replace("\"crate\"", "\"canister\"");
    let mut hits = 0;
    for right in [true, false] {
        for n in [22, 30, 37, 38, 45] {
            for i in [-60, -40, -20, 0, 20, 40] {
                let hit = |src: &str| {
                    let mut play = Play::new(src);
                    if !right {
                        play.world.player.facing = Facing::Left;
                    }
                    play.throw_to(n - 1);
                    let mut ahead = Play {
                        world: play.world.clone(),
                        held: play.held,
                    };
                    ahead.step(&[]);
                    let o = ahead.world.things[0].pos;
                    let x = o.x.floor() - i;
                    play.world.things[1].pos = Vec2::new(
                        Fixed::from_int(x as i32) + f(0.75),
                        Fixed::from_int((-(-o.y).floor()) as i32) - f(0.25),
                    );
                    play.step(&[]);
                    (knocked_thrower(&play).1, play.world.player.pos)
                };
                let got = hit(&metal);
                assert_eq!(hit(&plain), got, "right {right} Q + {n} I {i}");
                hits += usize::from(got.0);
            }
        }
    }
    assert!(hits > 10, "{hits}");
    // Carried in a jump.
    let mut play = Play::new(&metal);
    let mut control = Play::new(&plain);
    control.world.things[1].active = false;
    play.world.things[1].active = false;
    while play.world.player.grounded() {
        play.step(&[A, Action::Jump]);
        control.step(&[A, Action::Jump]);
    }
    play.world.player.facing = Facing::Left;
    control.world.player.facing = Facing::Left;
    let mut ahead = Play {
        world: control.world.clone(),
        held: control.held,
    };
    ahead.step(&[A]);
    let head = ahead.world.things[0].pos;
    let at = Vec2::new(
        Fixed::from_int((head.x.floor() - 31) as i32),
        Fixed::from_int((-(-head.y).floor()) as i32),
    );
    play.world.things[1].active = true;
    play.world.things[1].pos = at;
    play.step(&[A]);
    control.step(&[A]);
    assert!(knocked_thrower(&play).1);
    assert!(play.world.player.hands.is_none());
    assert!(matches!(
        play.world.things[0].phase,
        Phase::LetGo { vy, .. } if vy == f(-5.0)
    ));
    let mut steps = 0;
    while play.world.things[0].active {
        assert_eq!(play.world.player.pos, control.world.player.pos);
        play.step(&[]);
        control.step(&[]);
        steps += 1;
        assert!(steps < 100);
    }
    assert!((39..=49).contains(&steps), "{steps}");
}

/// metal-barrel.md, "The second character throwing it", the ledge
/// thrower: a metal barrel thrown either way by the second character
/// knocks out the thrower on the frame a barrel does, with the same rows
/// for the thrower and the character.
#[test]
fn a_metal_barrel_thrown_by_the_second_character_meets_the_thrower_as_a_barrel_does() {
    let plain = level(&format!("{FLAT}\n{}", thrower(300, 100)));
    let metal = plain.replace("\"crate\"", "\"canister\"");
    let mut hits = 0;
    for right in [true, false] {
        for n in [22, 30, 37, 45] {
            for i in [-60, -20, 0, 20, 40] {
                let hit = |src: &str| {
                    let mut play = second_play(src);
                    if !right {
                        play.world.player.facing = Facing::Left;
                    }
                    play.throw_to(n - 1);
                    let mut ahead = Play {
                        world: play.world.clone(),
                        held: play.held,
                    };
                    ahead.step(&[]);
                    let o = ahead.world.things[0].pos;
                    let x = o.x.floor() - i;
                    play.world.things[1].pos = Vec2::new(
                        Fixed::from_int(x as i32) + f(0.75),
                        Fixed::from_int((-(-o.y).floor()) as i32) - f(0.25),
                    );
                    let mut rows = Vec::new();
                    for _ in 0..10 {
                        play.step(&[]);
                        rows.push((knocked_thrower(&play), play.world.player.pos));
                    }
                    rows
                };
                let got = hit(&metal);
                assert_eq!(hit(&plain), got, "right {right} Q + {n} I {i}");
                hits += usize::from(got.iter().any(|r| r.0.1));
            }
        }
    }
    assert!(hits > 5, "{hits}");
}

/// The same, with the metal barrel still in the second character's hands
/// when it reaches the thrower: it isn't broken but moves up at 5 px per
/// frame, then falls through the floor and is gone some frames later (40 or
/// 41 on the ledge measured; the count follows how far below the picture
/// the floor is, so the flat test floor gives fewer).
#[test]
fn a_metal_barrel_in_the_second_character_s_hands_meets_the_thrower() {
    let src = level(&format!("{FLAT}\n{}", thrower(300, 100))).replace("\"crate\"", "\"canister\"");
    let mut play = second_play(&src);
    play.world.player.facing = Facing::Left;
    let head = play.world.things[0].pos;
    play.world.things[1].pos = Vec2::new(
        Fixed::from_int(head.x.floor() as i32 - 20),
        Fixed::from_int((-(-head.y).floor()) as i32),
    );
    play.step(&[A]);
    assert!(play.world.player.hands.is_none());
    assert!(matches!(
        play.world.things[0].phase,
        Phase::LetGo { vy, .. } if vy == f(-5.0)
    ));
    let mut steps = 0;
    while play.world.things[0].active {
        play.step(&[]);
        steps += 1;
        assert!(steps < 100);
    }
    assert!((30..=49).contains(&steps), "{steps}");
}

/// metal-barrel.md, "The second character throwing it", the thrower's
/// balls: a ball is removed by the second character's metal barrel in
/// the same positions as by a plain object, thrown either way, and the
/// thrown barrel goes on as the plain object does.
#[test]
fn a_metal_barrel_thrown_by_the_second_character_removes_a_ball_as_a_barrel_does() {
    let plain = level(&format!("{FLAT}\n{BALL}"));
    let metal = plain.replace("\"crate\"", "\"canister\"");
    let mut hits = 0;
    for right in [true, false] {
        for n in [22, 30, 37] {
            for i in [-40, -20, 0, 20, 40] {
                for h in [-20, 0, 20] {
                    let run = |src: &str| {
                        let mut play = second_play(src);
                        if !right {
                            play.world.player.facing = Facing::Left;
                        }
                        play.throw_to(n - 1);
                        play.world.view = None;
                        let mut ahead = Play {
                            world: play.world.clone(),
                            held: play.held,
                        };
                        ahead.step(&[]);
                        let o = ahead.world.things[0].pos;
                        let x = o.x.floor() - i;
                        let y = -(-o.y).floor() + h;
                        put_ball(
                            &mut play,
                            Vec2::new(
                                Fixed::from_int(x as i32) + f(0.75),
                                Fixed::from_int(y as i32) - f(0.25),
                            ),
                            Fixed::ZERO,
                            1,
                        );
                        play.step(&[]);
                        let t = &play.world.things[0];
                        (play.world.things[1].active, t.pos, t.vx, t.active)
                    };
                    let got = run(&metal);
                    assert_eq!(run(&plain), got, "right {right} Q + {n} I {i} H {h}");
                    hits += usize::from(!got.0);
                }
            }
        }
    }
    assert!(hits > 5, "{hits}");
}

/// The same with the metal barrel still in the second character's hands:
/// it isn't broken but moves up at 5 px per frame and falls through the
/// floor.
#[test]
fn a_metal_barrel_in_the_second_character_s_hands_is_kept_on_a_ball() {
    let src = level(&format!("{FLAT}\n{BALL}")).replace("\"crate\"", "\"canister\"");
    let mut play = second_play(&src);
    play.world.player.facing = Facing::Left;
    let head = play.world.things[0].pos;
    put_ball(
        &mut play,
        Vec2::new(
            Fixed::from_int(head.x.floor() as i32 - 20),
            Fixed::from_int((-(-head.y).floor()) as i32),
        ),
        Fixed::ZERO,
        0,
    );
    play.step(&[A]);
    assert!(play.world.player.hands.is_none());
    assert!(matches!(
        play.world.things[0].phase,
        Phase::LetGo { vy, .. } if vy == f(-5.0)
    ));
    assert_eq!(play.world.things[0].vx, Fixed::ZERO);
    let mut steps = 0;
    while play.world.things[0].active {
        play.step(&[]);
        steps += 1;
        assert!(steps < 100);
    }
}

/// metal-barrel.md, "The second character throwing it": thrown by the
/// second character, a metal barrel turns back at a wall as it does for
/// the first, and before that moves as a barrel does.
#[test]
fn a_metal_barrel_thrown_by_the_second_character_turns_back_at_a_wall() {
    let wall = "[[collision]]\nshape = \"rect\"\nrect = [300, 200, 20, 200]";
    let src = level(&format!("{FLAT}\n{wall}"));
    let mut barrel = second_play(&src);
    let mut metal = second_play(&src.replace("\"crate\"", "\"canister\""));
    barrel.throw_to(18);
    metal.throw_to(18);
    let mut turned = false;
    for n in 0..80 {
        let before = metal.world.things[0].pos;
        barrel.step(&[]);
        metal.step(&[]);
        let thing = &metal.world.things[0];
        assert!(thing.active, "{n}");
        if !turned && metal.object().vx.is_negative() {
            turned = true;
            assert_eq!(thing.pos.x, Fixed::from_int(before.x.floor() as i32));
            assert_eq!(metal.object().vx, speed(-896));
        }
        if !turned {
            assert_eq!(barrel.world.things[0].pos, thing.pos, "{n}");
            assert_eq!(barrel.world.things[0].active, thing.active, "{n}");
        }
    }
    assert!(turned);
}

/// The same, at a walker on the floor: the walker, the character and the
/// object move as with a barrel, and when the walker reaches the
/// character with the object still in the hands it isn't broken but moves
/// up at 5 px per frame.
#[test]
fn a_metal_barrel_thrown_by_the_second_character_defeats_walkers() {
    let run = |kind: &str, distance: f64| {
        let entity = format!(
            "[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}"
        );
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", kind);
        let mut play = second_play(&src);
        play.throw_to(18);
        let start = play.world.things[0].pos;
        play.world.things[1].pos.x = start.x + f(distance);
        let mut rows = Vec::new();
        for _ in 0..60 {
            play.hold(&[], 1);
            let (o, w) = (&play.world.things[0], &play.world.things[1]);
            rows.push((
                o.pos,
                o.active,
                walker(&play),
                w.active,
                play.world.player.pos,
            ));
        }
        rows
    };
    assert_eq!(run("\"crate\"", 150.0), run("\"canister\"", 150.0));
}

/// When the walker reaches the second character with the metal barrel
/// still in the hands, the barrel isn't broken: it moves up at 5 px per
/// frame and is gone later; a barrel is removed.
#[test]
fn a_metal_barrel_in_the_second_character_s_hands_rises_when_a_walker_reaches_it() {
    let run = |kind: &str| {
        let entity = format!(
            "[[entities]]\ntype = \"crawler\"\nat = [300, 400]\nparams = {{ {SMALL_WALKER} }}"
        );
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", kind);
        let mut play = second_play(&src);
        play.world.things[1].pos.x = play.world.things[0].pos.x + f(50.0);
        let mut rows = Vec::new();
        for _ in 0..60 {
            play.step(&[]);
            let t = &play.world.things[0];
            rows.push((t.active, t.phase, t.pos));
        }
        rows
    };
    let barrel = run("\"crate\"");
    let broke = barrel
        .iter()
        .position(|r| !r.0)
        .expect("a barrel is removed");
    let metal = run("\"canister\"");
    let at = metal
        .iter()
        .position(|r| matches!(r.1, Phase::LetGo { .. }))
        .expect("a metal barrel is let go");
    assert_eq!(at, broke);
    let Phase::LetGo { vy, .. } = metal[at].1 else {
        unreachable!()
    };
    assert_eq!(vy, f(-5.0));
    assert!(metal[at].0 && metal[at + 1].2.y < metal[at].2.y);
    assert!(metal.iter().any(|r| !r.0), "gone later");
}

/// metal-barrel.md, "Critters": a metal barrel rolling right passes a
/// critter as if it weren't there. The barrel's rows equal those of a run
/// with the critter far above its path, and the critter's rows equal
/// those of a run with no barrel in the picture.
#[test]
fn a_rolling_metal_barrel_passes_a_critter() {
    let run = |critter_y: i32, barrel: bool| {
        let entity = format!(
            "[[entities]]\ntype = \"critter\"\nat = [{}, {critter_y}]",
            300 + 60
        );
        let src = level(&format!("{FLAT}\n{entity}")).replace("\"crate\"", "\"canister\"");
        let mut play = Play::new(&src);
        play.throw_to(21 + 20);
        let at = play.world.player.pos;
        place(&mut play, at, 0, 0, false, true);
        play.world.things[0].pos.x = f(300.0);
        if !barrel {
            play.world.things[0].active = false;
        }
        let mut rows = Vec::new();
        for _ in 0..60 {
            play.world.view = Some(View {
                left: f(200.0),
                top: f(200.0),
                ..View::default()
            });
            play.hold(&[], 1);
            let (barrel, critter) = (&play.world.things[0], &play.world.things[1]);
            rows.push((
                (barrel.active, barrel.pos, barrel.vx),
                (critter.active, critter.pos),
            ));
        }
        rows
    };
    for height in [0, 16, 40, 64] {
        let near = run(400 - height, true);
        let far = run(400 - height - 250, true);
        let none = run(400 - height, false);
        assert!(near.iter().any(|r| r.1.0), "the critter appears");
        for n in 0..60 {
            assert_eq!(near[n].0, far[n].0, "{height}: barrel, frame {n}");
            assert_eq!(near[n].1, none[n].1, "{height}: critter, frame {n}");
        }
    }
}
