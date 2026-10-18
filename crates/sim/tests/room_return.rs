//! Coming back to the first level from its room on `classic-1`
//! (spec/behavior/level-start.md, "Coming back from the room"): the
//! room-return entrance bursts and throws the characters as the level's
//! entrance does from the level map, and each entrance acts only for its
//! own way into the level. One that does nothing isn't drawn.

use std::sync::Arc;

use clamber_input::Frame;
use clamber_level::Level;
use clamber_sim::{InputFrame, Phase, World, generic_feel, step};

/// The room-return record's point, and the floor 80 px below it.
const XR: f64 = 400.0;
const YR: f64 = 300.0;

/// The level's entrance at its start (entity 0), the room-return
/// entrance at the room-return point (entity 1) and the arrival
/// `from-room` out of it.
fn level() -> Arc<Level> {
    let src = format!(
        r#"
        format = 1
        name = "room-return"
        size = [3000, 700]
        start = [100, 300]

        [[arrivals]]
        name = "from-room"
        at = [{XR}, {YR}]
        entrance = "room-return"

        [[collision]]
        shape = "rect"
        rect = [0, 380, 3000, 100]

        [[entities]]
        type = "entrance"
        at = [100, 300]

        [[entities]]
        type = "entrance"
        id = "room-return"
        at = [{XR}, {YR}]
        "#
    );
    Arc::new(Level::from_toml_str(&src, "room-return").unwrap())
}

fn back() -> World {
    let level = level();
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let arrival = level.arrival("from-room").unwrap().clone();
    World::arriving(level, feel, &arrival)
}

fn from_the_map() -> World {
    World::new(level(), Arc::new(generic_feel("classic-1").unwrap()))
}

fn at(world: &World) -> (f64, f64) {
    let p = world.player.pos;
    (p.x.to_f64(), p.y.to_f64())
}

/// Steps `world` with nothing held up to and including frame `last`.
fn until(world: &mut World, last: u32) {
    while world.frame <= last {
        step(world, &InputFrame::single(Frame::default()));
    }
}

/// Coming back: hidden at the point up to frame 50, the room-return
/// entrance's burst from frame 41, and on T = 51 it's gone and the lead
/// is thrown at (2.375, −2.375). The level's entrance isn't drawn.
#[test]
fn thrown_out_of_the_room_return_entrance() {
    let mut world = back();
    assert_eq!(world.entering.map(|e| e.entrance), Some(1));
    assert!(!world.things[0].active);
    for frame in 0..=50 {
        step(&mut world, &InputFrame::single(Frame::default()));
        assert_eq!(at(&world), (XR, YR), "frame {frame}");
        assert!(!world.player_drawn(), "frame {frame}");
        let burst: Vec<_> = world.bursts.iter().map(|b| b.thing).collect();
        let want: &[usize] = if frame < 41 { &[] } else { &[1] };
        assert_eq!(burst, want, "frame {frame}");
        assert!(world.things[1].active, "frame {frame}");
        assert!(!world.things[0].active, "frame {frame}");
    }
    step(&mut world, &InputFrame::single(Frame::default()));
    assert!(world.player_drawn());
    let v = world.player.vel;
    assert_eq!((v.x.to_f64(), v.y.to_f64()), (2.375, -2.375));
    assert!(!world.things[1].active);
    assert_eq!(world.things[1].phase, Phase::Broken);
    assert_ne!(world.things[0].phase, Phase::Broken);
}

/// Coming back, the lead is where it is from the level map, moved by
/// the room-return point less the start point, on every frame to 89.
#[test]
fn coming_back_is_the_level_map_start_moved() {
    let (mut back, mut map) = (back(), from_the_map());
    for frame in 0..90 {
        step(&mut back, &InputFrame::single(Frame::default()));
        step(&mut map, &InputFrame::single(Frame::default()));
        let (b, m) = (at(&back), at(&map));
        assert_eq!((b.0 - m.0, b.1 - m.1), (XR - 100.0, 0.0), "frame {frame}");
        assert_eq!(back.player.vel, map.player.vel, "frame {frame}");
        assert_eq!(back.player.facing, map.player.facing, "frame {frame}");
    }
}

/// From the level map the level's entrance acts, and the room-return
/// entrance is there but does nothing and isn't drawn.
#[test]
fn from_the_level_map_it_does_nothing() {
    let mut world = from_the_map();
    assert_eq!(world.entering.map(|e| e.entrance), Some(0));
    for frame in 0..120 {
        step(&mut world, &InputFrame::single(Frame::default()));
        assert!(world.bursts.iter().all(|b| b.thing == 0), "frame {frame}");
        assert!(!world.things[1].active, "frame {frame}");
    }
    until(&mut world, 200);
    assert!(world.player.grounded());
    assert_ne!(world.things[1].phase, Phase::Broken);
}
