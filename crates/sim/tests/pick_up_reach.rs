//! When a pick-up starts, and from how far, in Classic 1
//! (spec/behavior/carrying.md, "Picking up" and "Reach").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{Act, Event, Facing, Fixed, InputFrame, View, World, generic_feel, step};

use Action::{Jump, Left, Right};

const A: Action = Action::Action;

/// Flat ground at y = 400, the player standing at x = 200, and an
/// object that holds nothing (or, with `holder`, the missing character)
/// on the floor at `x`.
fn world(x: i32, holder: bool) -> World {
    let src = format!(
        r#"
        format = 1
        name = "reach"
        size = [3000, 800]
        start = [200, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 2000, 200]

        [[entities]]
        type = "crate"
        at = [{x}, 400]
        params = {{ holds_character = {holder} }}
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "reach").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(x: i32, holder: bool) -> Play {
        let mut play = Play {
            world: world(x, holder),
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        assert!(play.world.player.grounded());
        play
    }

    fn step(&mut self, keys: &[Action]) -> Option<Act> {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        self.world.player.hands.map(|h| h.act)
    }

    fn hold(&mut self, keys: &[Action], frames: usize) -> Vec<Option<Act>> {
        (0..frames).map(|_| self.step(keys)).collect()
    }
}

/// With the player standing still facing `facing`, the object is moved
/// to `across` (its `x` less the player's) and `below` (its `y` less the
/// player's) at the end of a frame, and its look is `age` frames into
/// its cycle; then Action is pressed. Whether the pick-up starts on the
/// press frame.
fn picks_up(holder: bool, facing: Facing, across: i32, below: i32, age: u32) -> bool {
    let mut play = Play::new(600, holder);
    play.world.player.facing = facing;
    let p = play.world.player.pos;
    let thing = &mut play.world.things[0];
    thing.pos = Vec2::new(p.x + Fixed::from_int(across), p.y + Fixed::from_int(below));
    // Its clock counts the frame it appeared on as 1. The step before
    // counts it on to `age` frames after that.
    thing.clock = age;
    play.step(&[]);
    assert_eq!(play.world.things[0].clock, age + 1);
    let act = play.step(&[A]);
    let rolled = play.world.events.contains(&Event::RollStart);
    assert_eq!(act.is_some(), !rolled, "a press out of reach rolls");
    act == Some(Act::PickUp)
}

/// Whether `range` (ends included) is exactly the set of `v` from
/// `range.0 - 3` to `range.1 + 3` for which `inside(v)` holds.
fn edges(range: (i32, i32), inside: impl Fn(i32) -> bool) {
    for v in range.0 - 3..=range.1 + 3 {
        let expected = (range.0..=range.1).contains(&v);
        assert_eq!(inside(v), expected, "{v} in {range:?}");
    }
}

/// "Reach": a barrel's ranges, both facings. A holder looking still has
/// the same.
#[test]
fn a_barrels_reach() {
    for holder in [false, true] {
        // Frame 10 of the holder's cycle looks still.
        let age = 10;
        edges((-7, 38), |x| picks_up(holder, Facing::Right, x, 0, age));
        edges((-33, 12), |x| picks_up(holder, Facing::Left, x, 0, age));
        edges((-22, 32), |e| picks_up(holder, Facing::Right, 0, e, age));
        edges((-22, 32), |e| picks_up(holder, Facing::Left, -10, e, age));
    }
}

/// "Reach": a holder shaking.
#[test]
fn a_shaking_holders_reach() {
    let age = 0;
    edges((-10, 43), |x| picks_up(true, Facing::Right, x, 0, age));
    edges((-36, 17), |x| picks_up(true, Facing::Left, x, 0, age));
    edges((-21, 31), |e| picks_up(true, Facing::Right, 0, e, age));
    edges((-21, 31), |e| picks_up(true, Facing::Left, -10, e, age));
}

/// "The holder's look": still on frames 9 to 13 and 34 to 38 of its
/// 40-frame cycle, shaking on the others. With the holder 40 px ahead,
/// only a shaking one is within reach. A barrel has no look.
#[test]
fn the_holders_look() {
    for age in 0..80 {
        let still = matches!(age % 40, 9..=13 | 34..=38);
        assert_eq!(picks_up(true, Facing::Right, 40, 0, age), !still, "{age}");
        assert!(!picks_up(false, Facing::Right, 40, 0, age));
    }
}

/// "The holder's look": its cycle counts from the frame it appears with
/// the view.
#[test]
fn the_holders_look_counts_from_its_appearance() {
    let src = r#"
        format = 1
        size = [4000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]

        [[entities]]
        type = "crate"
        at = [1000, 400]
        params = { holds_character = true }

        [[sections]]
        spots = [0]
        "#;
    let level = Level::from_toml_str(src, "test").unwrap();
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    let mut step_at = |left: i32| {
        world.view = Some(View {
            left: Fixed::from_int(left),
            top: Fixed::from_int(200),
            ..View::default()
        });
        step(&mut world, &InputFrame::default());
        (world.things[0].active, world.things[0].clock)
    };
    assert!(!step_at(0).0);
    let mut left = 600;
    loop {
        let (active, clock) = step_at(left);
        if active {
            assert_eq!(clock, 1);
            break;
        }
        left += 8;
        assert!(left < 1000);
    }
    // Its clock counts the frame it appears on as 1, that frame being 0
    // of its cycle.
    for k in 2..50 {
        assert_eq!(step_at(left), (true, k));
    }
}

/// "Picking up": moving into the object with Action held, the pick-up
/// starts on the frame after the first that ends within reach; so does
/// moving away from it, from within reach.
#[test]
fn moving_with_action_held_picks_up_from_within_reach() {
    // Walking left into an object on the left, facing left.
    let mut play = Play::new(100, false);
    play.hold(&[Left], 30);
    assert_eq!(play.world.player.facing, Facing::Left);
    let mut ends = Vec::new();
    let mut started = None;
    for n in 0..200 {
        let act = play.step(&[Left, A]);
        if act.is_some() {
            started = Some(n);
            break;
        }
        ends.push(100 - play.world.player.pos.x.floor());
    }
    let first_within = ends.iter().position(|&x| x >= -33).unwrap();
    assert_eq!(started, Some(first_within + 1));

    // Standing within reach, then walking away with Action held: it's
    // picked up on the first frame Action is held.
    let mut play = Play::new(220, false);
    assert_eq!(play.step(&[Left, A]), Some(Act::PickUp));
}

/// "Picking up", landing and take-off: a jump that lands within reach
/// with Action held picks the object up on the frame after the landing
/// frame; a take-off within reach with Action held picks it up on the
/// next frame, in the air.
#[test]
fn landing_and_taking_off_within_reach() {
    // Take-off within reach (P + 1), Action pressed on the next frame.
    let mut play = Play::new(220, false);
    play.step(&[Jump]);
    assert_eq!(play.step(&[Jump]), None);
    assert!(play.world.events.contains(&Event::JumpStart));
    assert!(play.world.player.grounded());
    assert_eq!(play.step(&[Jump, A]), Some(Act::PickUp));
    assert!(!play.world.player.grounded());
    // Out of reach, the same jump doesn't.
    let mut play = Play::new(300, false);
    play.step(&[Jump]);
    play.step(&[Jump]);
    assert_eq!(play.step(&[Jump, A]), None);

    // A jump from out of reach to the right, landing within reach.
    let mut play = Play::new(340, false);
    play.step(&[Right, Jump]);
    let mut landed = None;
    for n in 0..120 {
        let act = play.step(&[Right, Jump, A]);
        if let Some(at) = landed {
            assert_eq!((n, act), (at + 1, Some(Act::PickUp)));
            break;
        }
        assert_eq!(act, None, "{n}");
        if play.world.events.contains(&Event::Land) {
            let x = 340 - play.world.player.pos.x.floor();
            assert!((-7..=38).contains(&x), "landed at {x}");
            landed = Some(n);
        }
    }
    assert!(landed.is_some());
}

/// "Reach", facing, the example: Action pressed in a turn's first four
/// frames, out of reach, starts a roll facing the old way; once the
/// player faces the held direction, that facing decides reach. An
/// object 20 px left of a player facing right is out of reach, and
/// within it facing left.
#[test]
fn reach_goes_by_the_facing_the_frame_before_ends_with() {
    for press in 0..4 {
        let mut play = Play::new(180, false);
        assert_eq!(play.world.player.facing, Facing::Right);
        let mut rows = play.hold(&[Left], press);
        let act = play.step(&[Left, A]);
        assert!(play.world.events.contains(&Event::RollStart));
        rows.push(act);
        let mut turned = None;
        for n in press + 1..30 {
            if turned.is_none() && play.world.player.facing == Facing::Left {
                turned = Some(n);
            }
            rows.push(play.step(&[Left, A]));
        }
        let turned = turned.expect("turned left");
        assert!(rows[..turned].iter().all(Option::is_none), "{press}");
        assert_eq!(rows[turned], Some(Act::PickUp), "{press}");
    }
}
