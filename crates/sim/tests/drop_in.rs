//! Coming back to the first level from the area behind its second wall
//! exit on `classic-1` (spec/behavior/wall-exits.md, "Coming back from
//! the second wall exit's area"): dropping in at an arrival, into the
//! barrel that shoots down.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Event, Facing, InputFrame, Phase, World, generic_feel, step};

/// The values of the barrel that shoots down, as the importer gives them.
const DOWN: &str = "hold = 28, shot = [0, 8.46484375], glide = 1, breaks = true";

/// The barrel's position, at the top edge of the level, with the floor
/// 400 px below it.
const XB: i32 = 600;
const YB: i32 = 1;

/// The level's own entrance at its start (entity 0), the barrel that
/// shoots down (entity 1) and the arrival `from-above` at `(x, y)`.
fn level(x: f64, y: f64) -> Arc<Level> {
    let src = format!(
        r#"
        format = 1
        name = "drop-in"
        size = [3000, 700]
        start = [100, 300]

        [[arrivals]]
        name = "from-above"
        at = [{x}, {y}]
        drop = true

        [[collision]]
        shape = "rect"
        rect = [0, 401, 3000, 100]

        [[entities]]
        type = "entrance"
        at = [100, 300]

        [[entities]]
        type = "sling"
        at = [{XB}, {YB}]
        params = {{ {DOWN} }}
        "#
    );
    Arc::new(Level::from_toml_str(&src, "drop-in").unwrap())
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// Coming back at `from-above`, at `(x, y)`.
    fn at_point(x: f64, y: f64) -> Play {
        let level = level(x, y);
        let feel = Arc::new(generic_feel("classic-1").unwrap());
        let arrival = level.arrival("from-above").unwrap().clone();
        Play {
            world: World::arriving(level, feel, &arrival),
            held: Actions::NONE,
        }
    }

    /// Coming back with the point at the barrel.
    fn back() -> Play {
        Play::at_point(f64::from(XB), f64::from(YB))
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    fn at(&self) -> (f64, f64) {
        let p = self.world.player.pos;
        (p.x.to_f64(), p.y.to_f64())
    }

    fn speed(&self) -> (f64, f64) {
        let v = self.world.player.vel;
        (v.x.to_f64(), v.y.to_f64())
    }

    fn caught(&self) -> bool {
        self.world.player.sling.is_some_and(|s| s.since == 0)
    }
}

/// The table: still at the point on frame 0, falling from frame 1 with
/// the barrel there, caught on 2, held to 30, shot down on 31, and with
/// nothing held landing on 81 400 px below the point.
#[test]
fn drops_into_the_barrel_and_is_shot_down() {
    let (xb, yb) = (f64::from(XB), f64::from(YB));
    let mut play = Play::back();
    assert!(play.world.entering.is_none());
    play.step(&[Action::Right, Action::Jump]);
    assert_eq!(play.at(), (xb, yb));
    assert_eq!(play.speed(), (0.0, 0.0));
    assert!(!play.world.player.grounded());
    assert_eq!(play.world.player.facing, Facing::Right);
    assert!(play.world.player_drawn());
    assert!(!play.world.things[1].active);
    play.step(&[]);
    assert_eq!(play.speed(), (0.0, 0.4375));
    assert_eq!(play.at(), (xb, yb + 0.4375));
    assert!(play.world.things[1].active);
    assert!(play.world.player.sling.is_none());
    play.step(&[]);
    assert!(play.caught());
    // Moved by whole px onto the barrel: its feet, rounded up, there.
    let caught_at = play.at();
    play.step(&[]);
    assert_eq!(play.at(), (xb, caught_at.1 - (caught_at.1.ceil() - yb)));
    for frame in 4..=30 {
        play.step(&[]);
        assert_eq!(play.at().1.ceil(), yb, "frame {frame}");
        assert!(play.world.things[1].active, "frame {frame}");
    }
    play.step(&[]);
    assert_eq!(play.world.frame, 32);
    assert_eq!(play.speed(), (0.0, 8.46484375));
    assert_eq!(play.world.things[1].phase, Phase::Broken);
    let mut landed = None;
    while landed.is_none() && play.world.frame < 130 {
        play.step(&[]);
        if play.world.events.contains(&Event::Land) {
            landed = Some(play.world.frame - 1);
        }
    }
    assert_eq!(landed, Some(81));
    assert_eq!(play.at().0, xb);
    assert_eq!(play.at().1, yb + 400.0);
}

/// "The record and the barrel": the record decides where the lead
/// starts, and it's caught on frame 2 from 20 px right or 30 px lower.
#[test]
fn caught_on_frame_two_from_near_the_barrel() {
    for (dx, dy) in [(20.0, 0.0), (0.0, 30.0)] {
        let mut play = Play::at_point(f64::from(XB) + dx, f64::from(YB) + dy);
        play.step(&[]);
        assert_eq!(play.at(), (f64::from(XB) + dx, f64::from(YB) + dy));
        play.step(&[]);
        assert!(!play.caught(), "{dx}, {dy}");
        play.step(&[]);
        assert!(play.caught(), "{dx}, {dy}");
    }
}

/// The picture fades in from black: after frame `f` the brightness is
/// ⌊`f` / 3⌋ fifteenths of full, full from frame 45.
#[test]
fn the_picture_fades_in() {
    let mut play = Play::back();
    for f in 0..45 {
        play.step(&[]);
        assert_eq!(play.world.brightness(), (f / 3, 15), "frame {f}");
    }
    play.step(&[]);
    let (now, full) = play.world.brightness();
    assert_eq!(now, full);
}

/// The level's own entrance takes no part.
#[test]
fn the_level_entrance_takes_no_part() {
    let mut play = Play::back();
    for frame in 0..100 {
        play.step(&[]);
        assert!(
            play.world.bursts.iter().all(|b| b.thing != 0),
            "frame {frame}"
        );
        assert!(!play.world.things[0].active, "frame {frame}");
    }
}
