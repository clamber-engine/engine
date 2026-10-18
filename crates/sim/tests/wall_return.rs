//! Coming back to the first level from the area behind its first wall
//! exit on `classic-1` (spec/behavior/wall-exits.md, "Coming back from
//! the first wall exit's area"): thrown out of an entrance an arrival
//! names.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Event, Facing, InputFrame, Phase, World, generic_feel, step};

const R: Action = Action::Right;
const A: Action = Action::Action;

/// The wall-return entrance's values, as the importer gives them.
const WALL_RETURN: &str = "throw_speed = [3.75, 2], rise = 6, steer_from = 7";

/// The arrival's point. The entrance is 16 px left of it and 2 px lower,
/// as in the first level, and the floor 21 px below it.
const XW: f64 = 600.0;
const YW: f64 = 300.0;

/// The level's own entrance at its start (entity 0), the wall-return
/// entrance (entity 1) and the arrival `from-wall` out of it.
fn level() -> Arc<Level> {
    let src = format!(
        r#"
        format = 1
        name = "wall-return"
        size = [3000, 700]
        start = [100, 300]

        [[arrivals]]
        name = "from-wall"
        at = [{XW}, {YW}]
        entrance = "wall-return"

        [[collision]]
        shape = "rect"
        rect = [0, 321, 3000, 100]

        [[entities]]
        type = "entrance"
        at = [100, 300]

        [[entities]]
        type = "entrance"
        id = "wall-return"
        at = [584, 302]
        params = {{ {WALL_RETURN} }}
        "#
    );
    Arc::new(Level::from_toml_str(&src, "wall-return").unwrap())
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// Coming back at `from-wall`.
    fn back() -> Play {
        let level = level();
        let feel = Arc::new(generic_feel("classic-1").unwrap());
        let arrival = level.arrival("from-wall").unwrap().clone();
        Play {
            world: World::arriving(level, feel, &arrival),
            held: Actions::NONE,
        }
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// Steps up to and including frame `last`, holding `keys`.
    fn until(&mut self, last: u32, keys: &[Action]) {
        while self.world.frame <= last {
            self.step(keys);
        }
    }

    fn at(&self) -> (f64, f64) {
        let p = self.world.player.pos;
        (p.x.to_f64(), p.y.to_f64())
    }

    fn speed(&self) -> (f64, f64) {
        let v = self.world.player.vel;
        (v.x.to_f64(), v.y.to_f64())
    }

    /// The speed across in 1/256 px per frame.
    fn vx256(&self) -> f64 {
        self.world.player.vel.x.to_f64() * 256.0
    }
}

/// The table: hidden at the point up to frame 50, the burst from 41, the
/// throw on T = 51, then 6 frames at the throw's speed and the fall.
#[test]
fn thrown_out_of_the_wall_return_entrance() {
    let mut play = Play::back();
    assert!(play.world.in_entrance());
    for frame in 0..=50 {
        play.step(&[]);
        assert_eq!(play.at(), (XW, YW), "frame {frame}");
        assert!(!play.world.player_drawn(), "frame {frame}");
        let burst: Vec<_> = play.world.bursts.iter().map(|b| b.thing).collect();
        let want: &[usize] = if frame < 41 { &[] } else { &[1] };
        assert_eq!(burst, want, "frame {frame}");
        assert!(play.world.things[1].active, "frame {frame}");
    }
    // T: drawn, the entrance gone, not moved yet.
    play.step(&[]);
    assert_eq!(play.at(), (XW, YW));
    assert_eq!(play.speed(), (3.75, -2.0));
    assert!(play.world.player_drawn());
    assert!(!play.world.things[1].active);
    assert_eq!(play.world.things[1].phase, Phase::Broken);
    // The level's own entrance takes no part.
    assert!(play.world.things[0].active);
    assert_ne!(play.world.things[0].phase, Phase::Broken);
    for n in 1..=6 {
        play.step(&[]);
        let n = f64::from(n);
        assert_eq!(play.at(), (XW + 3.75 * n, YW - 2.0 * n), "T + {n}");
        assert_eq!(play.speed(), (3.75, -2.0), "T + {n}");
    }
    // With nothing held the speed eases from 960/256.
    play.step(&[]);
    assert_eq!((play.vx256(), play.speed().1), (930.0, -1.5625));
    play.step(&[]);
    assert_eq!(play.vx256(), 901.0);
    // The top of the arc.
    play.until(61, &[]);
    assert_eq!(play.speed().1, -0.25);
    play.step(&[]);
    assert_eq!(play.speed().1, 0.1875);
    for frame in 63..74 {
        play.step(&[]);
        assert!(!play.world.player.grounded(), "frame {frame}");
        assert_eq!(play.world.player.facing, Facing::Right);
    }
    // Lands on T + 23, 21 px below the point.
    play.step(&[]);
    assert!(play.world.events.contains(&Event::Land));
    assert_eq!(play.at().1, YW + 21.0);
}

/// "Steering": every input is ignored up to T + 6, and from T + 7 the
/// speed moves toward the target by the gap ÷ 32.
#[test]
fn steered_from_the_seventh_frame() {
    let mut play = Play::back();
    play.until(57, &[R, A]);
    assert_eq!(play.at(), (XW + 3.75 * 6.0, YW - 12.0));
    assert_eq!(play.world.player.facing, Facing::Right);
    play.step(&[]);
    assert_eq!(play.vx256(), 930.0);
    let mut play = Play::back();
    play.until(57, &[R]);
    play.step(&[R]);
    // 960 + (512 − 960) / 32
    assert_eq!(play.vx256(), 946.0);
}

/// "The picture fades in from black as at the start of a level".
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

/// "Entering from the level map": the level begins out of its own
/// entrance, and the wall-return entrance never bursts.
#[test]
fn from_the_level_map_it_does_nothing() {
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let mut world = World::new(level(), feel);
    assert_eq!(world.entering.map(|e| e.entrance), Some(0));
    for frame in 0..200 {
        step(&mut world, &InputFrame::single(Frame::default()));
        assert!(world.bursts.iter().all(|b| b.thing == 0), "frame {frame}");
    }
    assert!(world.entering.is_none());
    assert!(world.player.grounded());
    assert_ne!(world.things[1].phase, Phase::Broken);
}
