//! Walking into an area on `classic-1` (spec/behavior/exits.md, "Walking
//! in", and view.md, "When an area begins").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Arrival, Dir, Level, Vec2};
use clamber_pack::{CharacterFeel, Presets};
use clamber_sim::{Facing, Fixed, InputFrame, World, generic_feel, step};

const R: Action = Action::Right;
const L: Action = Action::Left;
const J: Action = Action::Jump;
const A: Action = Action::Action;

/// A floor flat at y = 150, with an entrance at the level's start.
fn level() -> Arc<Level> {
    let src = r#"
        format = 1
        name = "walking-in"
        size = [3000, 700]
        start = [1000, 150]

        [[arrivals]]
        name = "from-the-left"
        at = [400, 150]
        walk = "right"

        [[collision]]
        shape = "rect"
        rect = [0, 150, 3000, 100]

        [[entities]]
        type = "entrance"
        at = [1000, 150]
        "#;
    Arc::new(Level::from_toml_str(src, "walking-in").unwrap())
}

fn world(characters: usize, at: (i32, i32), walk: Dir) -> World {
    let mut feel = generic_feel("classic-1").unwrap();
    for slot in 1..characters {
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", slot).unwrap(),
            abilities: BTreeMap::new(),
        });
    }
    let arrival = Arrival {
        name: "test".into(),
        at: Vec2::ints(at.0, at.1),
        walk: Some(walk),
    };
    World::arriving(level(), Arc::new(feel), &arrival)
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(characters: usize, at: (i32, i32), walk: Dir) -> Play {
        Play {
            world: world(characters, at, walk),
            held: Actions::NONE,
        }
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    fn x(&self) -> f64 {
        self.world.player.pos.x.to_f64()
    }

    fn other_x(&self) -> f64 {
        self.world.other.as_ref().unwrap().pos.x.to_f64()
    }
}

/// exits.md, "Example": walking in to (400, 150), rightward.
#[test]
fn the_example_walks_in_frame_by_frame() {
    let mut play = Play::new(2, (400, 150), Dir::Right);
    for k in 0..=27u32 {
        play.step(&[]);
        let want = if k == 27 {
            404.0
        } else {
            320.0 + 3.0 * f64::from(k)
        };
        assert_eq!(play.x(), want, "frame {k}");
        assert_eq!(play.world.player.pos.y, Fixed::from_int(150), "frame {k}");
        assert_eq!(play.world.player.facing, Facing::Right, "frame {k}");
        let other = if k >= 26 { 370.0 } else { want - 28.0 };
        assert_eq!(play.other_x(), other, "frame {k}");
        assert!(play.world.walking_in(), "frame {k}");
        let brightness = play.world.brightness().0;
        assert_eq!(brightness, k / 3, "frame {k}");
    }
    // The entrance is gone.
    assert!(!play.world.things[0].active);
    play.step(&[]);
    assert!(!play.world.walking_in());
    assert_eq!(play.world.player.vel.x.to_f64(), 2.625);
    assert_eq!(play.x(), 406.625);
    assert!(play.world.player.grounded());
    for k in 29..=44 {
        play.step(&[]);
        assert_eq!(play.world.brightness().0, k / 3, "frame {k}");
    }
    play.step(&[]);
    assert_eq!(play.world.brightness(), (15, 15));
    play.step(&[]);
    assert!(play.world.walking_in.is_none());
}

#[test]
fn right_held_on_frame_28_runs_on() {
    let mut play = Play::new(1, (400, 150), Dir::Right);
    for _ in 0..=27 {
        play.step(&[]);
    }
    play.step(&[R]);
    assert_eq!(play.world.player.vel.x.to_f64(), 2.875);
    assert_eq!(play.x(), 406.875);
}

/// Nothing held or pressed on frames 1 to 27 has an effect, and Jump
/// held from before frame 28 isn't a press on it.
#[test]
fn input_waits_for_frame_28() {
    let mut quiet = Play::new(2, (400, 150), Dir::Right);
    let mut busy = Play::new(2, (400, 150), Dir::Right);
    let keys: [&[Action]; 6] = [&[L], &[J], &[A], &[R, J], &[], &[L, A]];
    for k in 0..=27 {
        quiet.step(&[J]);
        busy.step(if k == 27 { &[J] } else { keys[k % keys.len()] });
        assert_eq!(quiet.world.player, busy.world.player, "frame {k}");
    }
    // Held on from before frame 28, it never jumps.
    for k in 28..40 {
        quiet.step(&[J]);
        assert!(quiet.world.player.grounded(), "frame {k}");
    }
    // A new press does (taking off on the jump's own frame after it).
    quiet.step(&[]);
    quiet.step(&[J]);
    let jumped = (0..4).any(|_| {
        quiet.step(&[J]);
        !quiet.world.player.grounded()
    });
    assert!(jumped);
}

#[test]
fn walking_left_mirrors_it() {
    let mut play = Play::new(2, (400, 150), Dir::Left);
    play.step(&[]);
    assert_eq!((play.x(), play.other_x()), (480.0, 508.0));
    assert_eq!(play.world.player.facing, Facing::Left);
    for _ in 1..=27 {
        play.step(&[]);
    }
    assert_eq!((play.x(), play.other_x()), (396.0, 430.0));
    play.step(&[]);
    assert_eq!(play.world.player.vel.x.to_f64(), -2.625);
}

/// view.md, "When an area begins", example (d): walking in to x = 300
/// with `A` = 0.
#[test]
fn the_view_waits_then_follows() {
    let mut play = Play::new(1, (300, 150), Dir::Right);
    let view = |play: &Play| {
        let v = play.world.view.unwrap();
        (v.left.to_f64(), v.speed.to_f64() * 8192.0)
    };
    play.step(&[]);
    assert_eq!(play.x(), 220.0);
    assert_eq!(view(&play), (141.5, 0.0));
    for k in 1..=26 {
        play.step(&[]);
        assert_eq!(view(&play), (141.5, 0.0), "frame {k}");
    }
    play.step(&[]);
    assert_eq!(play.x(), 304.0);
    assert_eq!(view(&play), (142.10546875, 4976.0));
    play.step(&[]);
    assert_eq!(play.x(), 306.625);
    assert_eq!(view(&play), (143.3046875, 9842.0));
}

/// exits.md, "Where the first level's exits lead": an arrival the
/// characters don't walk in by begins the level there as at its start,
/// through the entrance where it is.
#[test]
fn an_arrival_without_a_walk_begins_as_the_level_does() {
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let level = level();
    let arrival = Arrival {
        name: "back".into(),
        at: Vec2::ints(500, 150),
        walk: None,
    };
    let mut arrived = World::arriving(level.clone(), feel.clone(), &arrival);
    let mut started = World::new(level, feel);
    assert!(arrived.in_entrance() && arrived.walking_in.is_none());
    assert_eq!(arrived.player.pos, Vec2::ints(500, 150));
    assert_eq!(
        arrived.view.unwrap().left,
        Fixed::from_int(421) + Fixed::from_ratio_pow2(1, 1)
    );
    for _ in 0..120 {
        step(&mut arrived, &InputFrame::single(Frame::default()));
        step(&mut started, &InputFrame::single(Frame::default()));
        assert_eq!(
            arrived.player.pos.x - started.player.pos.x,
            Fixed::from_int(-500)
        );
    }
}

/// placements.md, "In the first level's room": walking into a room the
/// picture always shows all of, every entry's object is there from frame
/// 1 on, and none on frame 0.
#[test]
fn a_rooms_objects_are_there_from_frame_1() {
    let src = r#"
        format = 1
        name = "room"
        size = [256, 700]
        start = [200, 150]

        [[collision]]
        shape = "rect"
        rect = [0, 150, 256, 100]

        [[entities]]
        type = "spark"
        at = [40, 134]

        [[entities]]
        type = "crawler"
        at = [100, 150]

        [[entities]]
        type = "keg"
        at = [230, 150]
        "#;
    let level = Arc::new(Level::from_toml_str(src, "room").unwrap());
    let arrival = Arrival {
        name: "from-level".into(),
        at: level.start,
        walk: Some(Dir::Left),
    };
    let feel = Arc::new(generic_feel("classic-1").unwrap());
    let mut world = World::arriving(level, feel, &arrival);
    let there = |w: &World| w.things.iter().filter(|t| t.active).count();
    step(&mut world, &InputFrame::default());
    assert_eq!(there(&world), 0, "frame 0");
    step(&mut world, &InputFrame::default());
    assert_eq!(there(&world), 3, "frame 1");
}
