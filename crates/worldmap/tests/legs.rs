use clamber_level::{Fixed, Vec2};
use clamber_worldmap::{Buttons, Dir, LegWalker, Legs, Mode, Progress, Step, WorldMap};

const B: Buttons = Buttons::other(0);

/// A legs map with the given `[legs]` overrides, nodes and paths.
fn map_with(legs: &str, rest: &str) -> WorldMap {
    let src = format!(
        "format = 1\nid = \"m\"\nsize = [1024, 256]\nmode = \"legs\"\nstart = \"a\"\n\
         [legs]\nleg_frames = 64\nremember = 31\npage_width = 256\npage_wait = 64\n\
         y_up = true\n{legs}\n{rest}"
    );
    WorldMap::from_toml_str(&src, "m.map").unwrap()
}

fn legs(map: &WorldMap) -> &Legs {
    let Mode::Legs(legs) = &map.mode else {
        panic!("not a legs map")
    };
    legs
}

/// x + n/256.
fn px(x: i32, n: i32) -> Fixed {
    Fixed::from_int(x) + Fixed::from_ratio_pow2(n, 8)
}

fn at(x: (i32, i32), y: (i32, i32)) -> Vec2 {
    Vec2::new(px(x.0, x.1), px(y.0, y.1))
}

fn map_err(legs: &str, rest: &str) -> String {
    let src = format!(
        "format = 1\nid = \"m\"\nsize = [100, 100]\nmode = \"legs\"\nstart = \"a\"\n{legs}\n\
         [[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [50, 0]\n\
         [[nodes]]\nid = \"c\"\nkind = \"waypoint\"\nat = [0, 50]\n{rest}"
    );
    WorldMap::from_toml_str(&src, "m.map")
        .unwrap_err()
        .to_string()
}

const LEGS: &str = "[legs]\nleg_frames = 64\nremember = 31\npage_width = 256\n\
                    page_wait = 64\nappear_offset = [8, 0]\nappear_wait = 64";

/// The worked example in spec/behavior/level-map.md, "Walking a path".
#[test]
fn walks_the_worked_example() {
    let map = map_with(
        "appear_wait = 0\nappear_offset = [8, 0]",
        "[[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [92, 40]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [126, 20]\n\
         [[paths]]\nfrom = \"a\"\nto = \"b\"\nvia = [[130, 52]]\nleave = [\"right\"]",
    );
    let legs = legs(&map);
    let p = Progress::default();
    let mut lead = LegWalker::appear(&map, legs, 0);
    assert_eq!(lead.position(), Vec2::ints(100, 40));
    lead.set_position(at((100, 128), (40, 192)));

    let expected = [
        (0, at((100, 248), (40, 240))),
        (1, at((101, 112), (41, 32))),
        (58, at((128, 40), (51, 208))),
        (59, at((128, 160), (52, 0))),
        (60, at((129, 24), (52, 0))),
        (61, at((129, 144), (52, 0))),
        (62, at((130, 8), (52, 0))),
        (63, at((129, 248), (51, 128))),
        (64, at((129, 232), (51, 0))),
        (126, at((126, 8), (20, 0))),
        (127, at((126, 248), (20, 128))),
    ];
    let mut steps = Vec::new();
    for frame in 0..=127 {
        let held = if frame == 0 {
            Buttons::RIGHT
        } else {
            Buttons::NONE
        };
        let step = lead.step(&map, legs, &p, held);
        steps.push(step);
        if let Some((_, want)) = expected.iter().find(|(f, _)| *f == frame) {
            assert_eq!(lead.position(), *want, "frame {frame}");
        }
    }
    assert_eq!(steps[0].started, Some(0));
    assert!(steps[..127].iter().all(|s| s.arrived.is_none()));
    assert_eq!(steps[127].arrived, Some(1));
    assert_eq!(lead.node, 1);
    assert!(!lead.walking());
}

/// Two stops 64 px apart, so a leg takes 64 frames (0 to 63).
fn line() -> WorldMap {
    map_with(
        "appear_wait = 0\nappear_offset = [0, 0]",
        "[[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [64, 0]\n\
         [[nodes]]\nid = \"c\"\nkind = \"waypoint\"\nat = [128, 0]\n\
         [[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"right\"]\nback = [\"left\"]\n\
         [[paths]]\nfrom = \"b\"\nto = \"c\"\nleave = [\"down\"]",
    )
}

/// Steps `frames` frames with `held(frame)` and returns the frames on
/// which a walk started.
fn run(
    map: &WorldMap,
    lead: &mut LegWalker,
    frames: u64,
    held: impl Fn(u64) -> Buttons,
) -> Vec<(u64, Step)> {
    let legs = legs(map);
    let p = Progress::default();
    let mut out = Vec::new();
    while lead.frame() < frames {
        let frame = lead.frame();
        let step = lead.step(map, legs, &p, held(frame));
        if step.started.is_some() || step.arrived.is_some() {
            out.push((frame, step));
        }
    }
    out
}

#[test]
fn a_press_during_a_walk_is_remembered_for_31_frames() {
    let map = line();
    // Right from a: the walk's last frame L is 63. Down pressed on
    // L - 29 starts the next walk on L + 1.
    for (press, starts) in [(34, true), (33, false)] {
        let mut lead = LegWalker::appear(&map, legs(&map), 0);
        let events = run(&map, &mut lead, 200, |f| match f {
            0 => Buttons::RIGHT,
            f if f == press => Buttons::DOWN,
            _ => Buttons::NONE,
        });
        assert_eq!(events[1].0, 63);
        assert_eq!(events[1].1.arrived, Some(1));
        if starts {
            assert_eq!(events[2].0, 64, "press on {press}");
            assert_eq!(events[2].1.started, Some(1));
        } else {
            assert_eq!(events.len(), 2, "press on {press}");
        }
    }
}

#[test]
fn a_held_direction_starts_a_walk_and_presses_dont_change_it() {
    let map = line();
    let mut lead = LegWalker::appear(&map, legs(&map), 0);
    // Left pressed during the walk doesn't turn it; held Down starts the
    // next on L + 1.
    let events = run(&map, &mut lead, 200, |f| match f {
        0 => Buttons::RIGHT,
        10 => Buttons::LEFT,
        f if f >= 40 => Buttons::DOWN,
        _ => Buttons::NONE,
    });
    assert_eq!(events[1].1.arrived, Some(1));
    assert_eq!((events[2].0, events[2].1.started), (64, Some(1)));
}

#[test]
fn the_first_direction_counts_even_without_a_path() {
    let map = line();
    let mut lead = LegWalker::appear(&map, legs(&map), 0);
    run(&map, &mut lead, 64, |f| {
        if f == 0 {
            Buttons::RIGHT
        } else {
            Buttons::NONE
        }
    });
    assert_eq!(lead.node, 1);
    // At b, Right has no path. Held from 70 with Down remembered from 72,
    // nothing happens until Right is released on 90.
    let events = run(&map, &mut lead, 200, |f| match f {
        72 => Buttons::RIGHT | Buttons::DOWN,
        70..90 => Buttons::RIGHT,
        _ => Buttons::NONE,
    });
    assert_eq!((events[0].0, events[0].1.started), (90, Some(1)));
}

#[test]
fn a_new_press_of_another_button_replaces_the_remembered_press() {
    let map = line();
    let mut lead = LegWalker::appear(&map, legs(&map), 0);
    let events = run(&map, &mut lead, 200, |f| match f {
        0 => Buttons::RIGHT,
        50 => Buttons::DOWN,
        55 => B,
        _ => Buttons::NONE,
    });
    assert_eq!(events.len(), 2);
    assert_eq!(lead.node, 1);
}

#[test]
fn a_kept_held_button_is_never_remembered() {
    let map = line();
    let mut lead = LegWalker::appear(&map, legs(&map), 0);
    // Down held from before the walk and still held at its end starts the
    // next walk; released at L, it is not remembered.
    let events = run(&map, &mut lead, 200, |f| match f {
        0 => Buttons::RIGHT | Buttons::DOWN,
        1..63 => Buttons::DOWN,
        _ => Buttons::NONE,
    });
    assert_eq!(events.len(), 2);
}

#[test]
fn walking_back_takes_the_corners_in_reverse() {
    let map = map_with(
        "appear_wait = 0\nappear_offset = [0, 0]",
        "[[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [64, 64]\n\
         [[nodes]]\nid = \"c\"\nkind = \"waypoint\"\nat = [0, 64]\n\
         [[paths]]\nfrom = \"a\"\nto = \"b\"\nvia = [[64, 0]]\n\
         leave = [\"right\"]\nback = [\"left\", \"up\"]\n\
         [[paths]]\nfrom = \"a\"\nto = \"c\"\nvia = [[32, 0], [32, 64]]\n\
         back_via = [[0, 32]]\nleave = [\"down\"]\nback = [\"right\"]",
    );
    let p = Progress::default();
    let route = |node, dir| map.leg_route(node, dir, &p).map(|(_, to, pts)| (to, pts));
    assert_eq!(
        route(0, Dir::Right),
        Some((1, vec![Vec2::ints(64, 0), Vec2::ints(64, 64)]))
    );
    assert_eq!(route(1, Dir::Up), route(1, Dir::Left));
    assert_eq!(
        route(1, Dir::Left),
        Some((0, vec![Vec2::ints(64, 0), Vec2::ints(0, 0)]))
    );
    assert_eq!(
        route(2, Dir::Right),
        Some((0, vec![Vec2::ints(0, 32), Vec2::ints(0, 0)]))
    );
    assert_eq!(route(1, Dir::Right), None);
}

#[test]
fn closed_paths_arent_walked() {
    let map = map_with(
        "appear_wait = 0\nappear_offset = [0, 0]",
        "[[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [64, 0]\n\
         [[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"right\"]\nunlock = { flag = \"open\" }",
    );
    let mut p = Progress::default();
    assert_eq!(map.leg_route(0, Dir::Right, &p), None);
    p.flags.insert("open".into());
    assert!(map.leg_route(0, Dir::Right, &p).is_some());
}

#[test]
fn no_walk_starts_soon_after_the_map_appears() {
    let map = map_with(
        "appear_wait = 64\nappear_offset = [8, 0]",
        "[[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [64, 0]\n\
         [[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"right\"]",
    );
    let mut lead = LegWalker::appear(&map, legs(&map), 0);
    assert_eq!(lead.position(), Vec2::ints(8, 0));
    let events = run(&map, &mut lead, 100, |_| Buttons::RIGHT);
    assert_eq!((events[0].0, events[0].1.started), (64, Some(0)));
    // A press before then is remembered into it.
    let mut lead = LegWalker::appear(&map, legs(&map), 0);
    let events = run(&map, &mut lead, 100, |f| {
        if f == 40 {
            Buttons::RIGHT
        } else {
            Buttons::NONE
        }
    });
    assert_eq!(events[0].0, 64);
}

#[test]
fn a_new_page_costs_a_lag_frame_and_holds_the_next_walk() {
    // From x 192 to 320 in one leg of 64 frames, 2 px a frame: the whole
    // pixel reaches 256 on frame 31, then 320 on frame 63.
    let map = map_with(
        "appear_wait = 0\nappear_offset = [0, 0]",
        "[[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [192, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [320, 0]\n\
         [[nodes]]\nid = \"c\"\nkind = \"waypoint\"\nat = [320, 64]\n\
         [[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"right\"]\n\
         [[paths]]\nfrom = \"b\"\nto = \"c\"\nleave = [\"up\"]",
    );
    let legs = legs(&map);
    let p = Progress::default();
    let mut lead = LegWalker::appear(&map, legs, 0);
    assert_eq!(lead.page(legs), 0);
    let mut log = Vec::new();
    for _ in 0..200 {
        let frame = lead.frame();
        let held = if frame == 0 || frame >= 40 {
            Buttons::RIGHT | Buttons::UP
        } else {
            Buttons::NONE
        };
        log.push((frame, lead.step(&map, legs, &p, held)));
    }
    let page = log.iter().position(|(_, s)| s.page.is_some()).unwrap();
    assert_eq!(log[page].0, 31);
    assert_eq!(log[page].1.page, Some(1));
    assert!(log[page + 1].1.lag);
    assert_eq!(log[page + 1].0, 32, "a lag frame isn't counted");
    assert_eq!(log[page + 2].0, 32);
    assert!(!log[page + 2].1.lag);
    let arrived = log.iter().find(|(_, s)| s.arrived.is_some()).unwrap();
    assert_eq!(arrived.0, 63);
    // Right, held with Up, has no path at b and blocks it.
    let started: Vec<u64> = log
        .iter()
        .filter(|(_, s)| s.started.is_some())
        .map(|(f, _)| *f)
        .collect();
    assert_eq!(started, [0]);
}

#[test]
fn a_held_direction_waits_out_a_page_change() {
    let map = map_with(
        "appear_wait = 0\nappear_offset = [0, 0]",
        "[[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [192, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [320, 0]\n\
         [[nodes]]\nid = \"c\"\nkind = \"waypoint\"\nat = [320, 64]\n\
         [[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"right\"]\n\
         [[paths]]\nfrom = \"b\"\nto = \"c\"\nleave = [\"up\"]",
    );
    let mut lead = LegWalker::appear(&map, legs(&map), 0);
    let events = run(&map, &mut lead, 200, |f| {
        if f == 0 { Buttons::RIGHT } else { Buttons::UP }
    });
    assert_eq!(events[1].0, 63);
    assert_eq!((events[2].0, events[2].1.started), (95, Some(1)));
}

#[test]
fn legs_maps_are_checked() {
    let path = "[[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"right\"]";
    assert!(map_err("", path).contains("a legs map needs a [legs] table"));
    assert!(
        map_err(&LEGS.replace("leg_frames = 64", "leg_frames = 60"), path)
            .contains("legs.leg_frames")
    );
    assert!(
        map_err(LEGS, "[[paths]]\nfrom = \"a\"\nto = \"b\"")
            .contains("needs leave or back directions")
    );
    assert!(
        map_err(
            LEGS,
            "[[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"sideways\"]"
        )
        .contains("`sideways`; use right, left, up or down")
    );
    assert!(
        map_err(
            LEGS,
            &format!("{path}\n[[paths]]\nfrom = \"a\"\nto = \"c\"\nleave = [\"right\"]")
        )
        .contains("right from `a` already takes another path")
    );
    assert!(
        map_err(
            LEGS,
            "[[paths]]\nfrom = \"a\"\nto = \"b\"\nleave = [\"right\"]\nback = [\"left\"]\none_way = true"
        )
        .contains("a one-way path can't have back")
    );
    let paths_mode = "format = 1\nid = \"m\"\nsize = [100, 100]\nstart = \"a\"\n\
         [[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [[nodes]]\nid = \"b\"\nkind = \"waypoint\"\nat = [50, 0]\n";
    let err = WorldMap::from_toml_str(&format!("{paths_mode}{path}"), "m.map")
        .unwrap_err()
        .to_string();
    assert!(err.contains("only for maps with mode = \"legs\""), "{err}");
    let err = WorldMap::from_toml_str(&format!("{paths_mode}{LEGS}"), "m.map")
        .unwrap_err()
        .to_string();
    assert!(err.contains("legs: only for maps with mode"), "{err}");
}
