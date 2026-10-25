//! A first character meeting a thrown metal barrel
//! (spec/behavior/metal-barrel.md, "The characters").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{Flight, InputFrame, Phase, ROLL, State, World, generic_feel, step};

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat floor at y = 400, a canister placed at x = 100, and `more`.
fn level(more: &str) -> String {
    format!(
        r#"
        format = 1
        name = "ride"
        size = [3000, 800]
        start = [100, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 3000, 200]

        [[entities]]
        type = "canister"
        at = [100, 400]
        {more}
        "#
    )
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// The metal barrel rolls right from `barrel`; the player is in the
    /// air at `x`, `y`.
    fn new(src: &str, barrel: f64, x: f64, y: f64) -> Play {
        let level = Arc::new(Level::from_toml_str(src, "ride").unwrap());
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::from([(ROLL.to_string(), BTreeMap::new())]),
        });
        let mut world = World::new(level, Arc::new(feel));
        let physics = world.player_physics();
        let thing = &mut world.things[0];
        thing.pos = Vec2::new(f(barrel), f(400.0));
        thing.phase = Phase::Thrown(Flight {
            vx: f(3.5),
            vy: Fixed::ZERO,
            target: f(3.5),
            response: Fixed::ZERO,
            gravity: physics[clamber_pack::Param::ThrowGravity],
            max_fall: physics[clamber_pack::Param::ThrowMaxFall],
            thrown: true,
            margin: physics[clamber_pack::Param::ThrowLandMargin],
            bounce_min: physics[clamber_pack::Param::ThrowBounceMin],
            bounce: physics[clamber_pack::Param::ThrowBounce],
            rolling: true,
            age: 30,
            wait: 0,
            launch: false,
        });
        let p = &mut world.player;
        p.pos = Vec2::new(f(x), f(y));
        p.ground = None;
        p.state = State::Fall;
        Play {
            world,
            held: Actions::NONE,
        }
    }

    fn step(&mut self, keys: &[Action]) {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
    }

    /// Steps until the player is on a barrel; the frame it is caught.
    fn until_caught(&mut self) {
        for _ in 0..40 {
            self.step(&[]);
            if self.world.player.state == State::Astride {
                return;
            }
        }
        panic!("not caught: {:?}", self.world.player.pos);
    }

    fn barrel(&self) -> Vec2 {
        self.world.things[0].pos
    }

    fn gap(&self) -> Fixed {
        self.barrel().x - self.world.player.pos.x
    }
}

#[test]
fn a_falling_character_over_the_barrel_is_caught_and_carried() {
    let mut play = Play::new(&level(""), 100.0, 160.0, 300.0);
    play.until_caught();
    let p = &play.world.player;
    assert_eq!(p.pos.y, f(400.0 - 16.5));
    assert_eq!(play.gap(), f(7.75));
    // It stays across on the next frame, then goes with the barrel.
    play.step(&[]);
    let gap = play.gap();
    assert_eq!(gap, f(7.75 + 3.5));
    assert_eq!(play.world.player.vel.x, Fixed::ZERO);
    for _ in 0..10 {
        let before = play.world.player.pos.x;
        play.step(&[Action::Left, Action::Down, Action::Action]);
        let p = &play.world.player;
        assert_eq!(p.state, State::Astride);
        assert_eq!(play.gap(), gap);
        assert_eq!(p.pos.x - before, f(3.5));
        assert_eq!(p.vel.x, f(3.5));
        assert_eq!(p.pos.y, f(400.0 - 16.5));
    }
}

#[test]
fn a_character_coming_down_out_of_range_lands_and_the_barrel_goes_through() {
    let mut play = Play::new(&level(""), 100.0, 300.0, 300.0);
    for _ in 0..60 {
        play.step(&[]);
        assert_ne!(play.world.player.state, State::Astride);
    }
    assert!(play.world.player.grounded());
    assert!(play.barrel().x > f(300.0));
}

#[test]
fn a_standing_character_is_not_touched() {
    let mut play = Play::new(&level(""), 100.0, 180.0, 400.0);
    for _ in 0..3 {
        play.step(&[]);
    }
    assert!(play.world.player.grounded());
    let at = play.world.player.pos;
    for _ in 0..40 {
        play.step(&[]);
        assert_eq!(play.world.player.state, State::Idle);
        assert_eq!(play.world.player.pos, at);
    }
    assert!(play.barrel().x > at.x + f(10.0));
}

#[test]
fn jump_off_from_two_frames_after_the_catch() {
    let mut play = Play::new(&level(""), 100.0, 160.0, 300.0);
    play.until_caught();
    // A press on C + 1 does nothing and isn't remembered.
    play.step(&[Action::Jump]);
    assert_eq!(play.world.player.state, State::Astride);
    play.step(&[]);
    play.step(&[]);
    assert_eq!(play.world.player.state, State::Astride);
    let at = play.world.player.pos;
    play.step(&[Action::Jump]);
    let p = &play.world.player;
    assert_eq!(p.state, State::Jump);
    assert_eq!(p.pos, at);
    assert_eq!(p.vel.x, f(3.5));
    assert_eq!(p.vel.y, f(-7.0));
    play.step(&[Action::Jump]);
    assert_eq!(play.world.player.vel.x, f(3.5 * 0.875));
    for _ in 0..80 {
        play.step(&[]);
    }
    assert!(play.world.player.grounded());
    assert!(play.world.player.pos.x < play.barrel().x);
}

#[test]
fn the_ride_ends_where_the_barrel_turns() {
    let wall = "[[collision]]\nshape = \"rect\"\nrect = [300, 200, 20, 200]";
    let mut play = Play::new(&level(wall), 100.0, 160.0, 300.0);
    play.until_caught();
    let mut ended = false;
    for _ in 0..80 {
        let before = play.world.player.pos.x;
        play.step(&[]);
        if play.world.player.state != State::Astride {
            // On the floor at its own x, in the same frame.
            let p = &play.world.player;
            assert_eq!(p.state, State::Idle);
            assert_eq!(p.pos, Vec2::new(before, f(400.0)));
            ended = true;
            break;
        }
    }
    assert!(ended);
}

/// A rolling metal barrel is picked up from the floor by a character who
/// isn't riding it.
#[test]
fn a_rolling_metal_barrel_can_be_picked_up() {
    let mut play = Play::new(&level(""), 20.0, 120.0, 400.0);
    play.world.player.facing = clamber_sim::Facing::Left;
    for _ in 0..3 {
        play.step(&[]);
    }
    assert!(play.world.player.grounded());
    for _ in 0..30 {
        play.step(&[Action::Action]);
        if play.world.player.hands.is_some() {
            break;
        }
    }
    let hands = play.world.player.hands.expect("a pick-up starts");
    assert_eq!(hands.thing, 0);
    assert_eq!(hands.act, clamber_sim::Act::PickUp);
    for _ in 0..30 {
        play.step(&[Action::Action]);
    }
    assert_eq!(play.world.things[0].phase, Phase::Held);
}

/// A press that began on C and is still held on C + 2 jumps on C + 2;
/// one that began on C + 1 never does.
#[test]
fn a_press_held_through_c_plus_2_jumps_then() {
    let mut play = Play::new(&level(""), 100.0, 160.0, 300.0);
    play.until_caught();
    play.world.player.jump_held = 1; // began on C
    play.step(&[Action::Jump]);
    assert_eq!(play.world.player.state, State::Astride);
    play.step(&[Action::Jump]);
    assert_eq!(play.world.player.state, State::Jump);

    let mut play = Play::new(&level(""), 100.0, 160.0, 300.0);
    play.until_caught();
    for _ in 0..8 {
        play.step(&[Action::Jump]);
        assert_eq!(play.world.player.state, State::Astride);
    }
}

fn second(barrel: f64, x: f64, y: f64) -> Play {
    let mut play = Play::new(&level(""), barrel, x, y);
    play.world.player.character = 1;
    play
}

#[test]
fn the_second_character_is_caught_and_rides_a_little_higher() {
    let mut play = second(100.0, 150.0, 300.0);
    play.until_caught();
    assert_eq!(play.world.player.pos.y, f(400.0 - 16.7));
    assert_eq!(play.gap(), f(8.15));
    play.step(&[]);
    assert_eq!(play.gap(), f(8.15 + 3.5));
    play.step(&[Action::Left]);
    assert_eq!(play.gap(), f(8.15 + 3.5));
}

/// Whether a character starting to fall at `y`, level with a barrel
/// just behind it, is caught on the first frame.
fn caught_from(character: usize, y: f64) -> bool {
    let mut play = Play::new(&level(""), 96.0, 100.0, y);
    play.world.player.character = character;
    play.world.player.vel.y = f(2.0);
    play.step(&[]);
    play.world.player.state == State::Astride
}

#[test]
fn the_second_character_is_caught_in_a_band_by_where_the_frame_began() {
    // 18 px above the barrel: both. 12 px above: the first only, for its
    // fall would end below the reach; the second began under the band.
    assert!(caught_from(0, 382.0));
    assert!(caught_from(1, 382.0));
    assert!(caught_from(0, 388.0));
    assert!(!caught_from(1, 388.0));
    // Over 46.5 px above: not caught.
    assert!(!caught_from(1, 350.0));
}

fn barrel_vy(play: &Play) -> Fixed {
    match play.world.things[0].phase {
        Phase::Thrown(flight) => flight.vy,
        other => panic!("not thrown: {other:?}"),
    }
}

#[test]
fn a_barrel_in_the_air_with_a_rider_falls_more_slowly() {
    let mut play = Play::new(&level(""), 100.0, 130.0, 186.0);
    let Phase::Thrown(flight) = &mut play.world.things[0].phase else {
        unreachable!();
    };
    flight.rolling = false;
    play.world.things[0].pos.y = f(200.0);
    // Free, it grows by throw_gravity.
    let mut free = Play::new(&level(""), 100.0, 2000.0, 0.0);
    let Phase::Thrown(flight) = &mut free.world.things[0].phase else {
        unreachable!();
    };
    flight.rolling = false;
    free.world.things[0].pos.y = f(200.0);
    free.step(&[]);
    assert_eq!(barrel_vy(&free), f(0.4375));
    play.until_caught();
    assert_eq!(play.gap(), f(7.75));
    let caught = barrel_vy(&play);
    play.step(&[]);
    assert_eq!(barrel_vy(&play) - caught, f(0.3125));
    play.step(&[]);
    assert_eq!(barrel_vy(&play) - caught, f(0.625));
    assert_eq!(play.world.player.state, State::Astride);
}
