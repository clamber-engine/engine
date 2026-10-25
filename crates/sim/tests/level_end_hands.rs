//! An object in the lead's hands at the level end on `classic-1`
//! (spec/behavior/level-end.md, "An object in the lead's hands").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Param, Presets};
use clamber_sim::{Act, Facing, InputFrame, Outcome, Phase, World, generic_feel, step};

const R: Action = Action::Right;
const A: Action = Action::Action;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat ground at y = 400, the player at x = 200, a crate 20 px to its
/// right, holding the missing character if `holder`, and an end marker
/// at `xm` on the ground.
fn world(holder: bool, xm: i32, second: bool) -> World {
    let src = format!(
        r#"
        format = 1
        name = "hands"
        size = [3000, 800]
        start = [200, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 3000, 200]

        [[entities]]
        type = "crate"
        at = [220, 400]
        params = {{ holds_character = {holder} }}

        [[entities]]
        type = "end"
        at = [{xm}, 400]
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "hands").unwrap());
    let mut feel = generic_feel("classic-1").unwrap();
    if second {
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::new(),
        });
    }
    let mut world = World::new(level, Arc::new(feel));
    if second {
        world.player.character = 1;
        if let Some(other) = &mut world.other {
            other.character = 0;
        }
    }
    world
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// Picks up the crate and carries it with action held, standing.
    fn carrying(holder: bool, xm: i32) -> Play {
        Play::carrying_by(false, holder, xm)
    }

    /// The same, with the second character as the lead if `second`.
    fn carrying_by(second: bool, holder: bool, xm: i32) -> Play {
        let mut play = Play {
            world: world(holder, xm, second),
            held: Actions::NONE,
        };
        play.hold(&[], 2);
        play.hold(&[A], 21);
        assert_eq!(play.world.player.hands.map(|h| h.act), Some(Act::Carry));
        assert!(play.world.ending.is_none());
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

    /// Holds `keys` until the lead passes the marker, on frame `n`.
    fn until_passed(&mut self, keys: &[Action]) {
        for _ in 0..300 {
            self.step(keys);
            if self.world.ending.is_some() {
                return;
            }
        }
        panic!("never passed the marker");
    }

    fn object(&self) -> &clamber_sim::Thing {
        &self.world.things[0]
    }
}

fn vy(phase: Phase) -> Fixed {
    match phase {
        Phase::LetGo { vy, .. } => vy,
        other => panic!("{other:?}"),
    }
}

/// From the end of `n`, with the object still in the hands: checks it's
/// let go on `T` facing `way` (1 right, -1 left), then follows it frame
/// by frame against the rules until it's removed, calling `nudge` with
/// the frame after `T` before each step to move it as the spec's runs
/// did. Returns the frame after `T` it's removed on.
fn falls_away(play: &mut Play, way: f64, mut nudge: impl FnMut(u32, &mut World)) -> u32 {
    assert!(play.world.player.hands.is_some(), "held at the end of n");
    let held_at = play.object().pos;
    // T: let go where it was held, not moving yet.
    play.step(&[A, R]);
    let t = play.object();
    assert!(t.active);
    assert_eq!(t.pos, held_at, "T: where it was held");
    assert_eq!(t.vx, f(way));
    assert_eq!(vy(t.phase), f(-4.0));
    assert!(play.world.player.hands.is_none());
    let mut v = f(-4.0);
    for k in 1..200u32 {
        nudge(k, &mut play.world);
        let Vec2 { mut x, mut y } = play.object().pos;
        let view = play.world.view.unwrap();
        let (left, top) = (view.x(), view.y());
        // Input makes no difference.
        let keys: &[Action] = if k % 2 == 0 { &[A] } else { &[Action::Left] };
        play.step(keys);
        v = (v + f(0.4375)).min(f(8.0));
        x += f(way);
        y += v;
        let gone = x < Fixed::ZERO
            || x.floor() - left >= 289
            || y.floor() - top >= 256
            || top - y.floor() >= 33;
        let t = play.object();
        if gone {
            assert!(!t.active, "T + {k}: removed");
            return k;
        }
        assert!(t.active, "T + {k}");
        assert_eq!(t.pos, Vec2::new(x, y), "T + {k}");
        assert_eq!(t.vx, f(way), "T + {k}");
        assert_eq!(vy(t.phase), v, "T + {k}");
    }
    panic!("never removed");
}

/// Walking in with action held, facing right: let go on `T`, it moves
/// from `T` + 1, through the floor, and is removed below the picture
/// before the fade has ended.
#[test]
fn carried_in_it_is_let_go_on_t_and_falls_away() {
    let mut play = Play::carrying(false, 300);
    play.until_passed(&[R, A]);
    let removed = falls_away(&mut play, 1.0, |_, _| {});
    assert!((30..40).contains(&removed), "{removed}");
    assert!(!play.world.flow.outcome.is_over());
    // The lead's walk-out and the fade go on as without an object.
    while !play.world.flow.outcome.is_over() {
        play.step(&[]);
        assert!(play.world.frame < 400);
    }
    assert_eq!(play.world.flow.outcome, Outcome::Complete);
}

/// A holder is let go the same way, from `T` + 1 too, and keeps the
/// missing character.
#[test]
fn a_holder_is_let_go_the_same_way() {
    let mut play = Play::carrying(true, 300);
    play.until_passed(&[R, A]);
    falls_away(&mut play, 1.0, |_, _| {});
    assert!(play.world.join.is_none(), "it didn't break");
}

/// A throw begun on `Q` with `Q` + 21 later than `n` never happens: the
/// object is let go on `T` instead.
#[test]
fn a_throw_still_under_way_never_happens() {
    let mut play = Play::carrying(false, 190);
    play.step(&[]);
    assert_eq!(play.world.player.hands.map(|h| h.act), Some(Act::Throw));
    play.until_passed(&[R]);
    assert_eq!(play.world.things[0].phase, Phase::Held);
    falls_away(&mut play, 1.0, |_, _| {});
}

/// A lead turning toward the right on `T` still faces left: the object
/// moves left.
#[test]
fn a_lead_turning_right_lets_it_go_to_the_left() {
    let mut play = Play::carrying(false, 150);
    play.world.player.facing = Facing::Left;
    play.hold(&[A], 2);
    play.until_passed(&[R, A]);
    assert_eq!(play.world.player.facing, Facing::Left);
    falls_away(&mut play, -1.0, |_, _| {});
}

/// Removed on the first frame its `x` rounded down is 289 or more right
/// of the view's left edge.
#[test]
fn removed_289_right_of_the_view() {
    let mut play = Play::carrying(false, 300);
    play.until_passed(&[R, A]);
    let removed = falls_away(&mut play, 1.0, |k, world| {
        if k == 5 {
            let left = world.view.unwrap().x();
            world.things[0].pos.x = Fixed::from_int(left as i32 + 286) + f(0.5);
        }
    });
    // 287.5 after the move on T + 5, 288.5 on T + 6, 289.5 on T + 7.
    assert_eq!(removed, 7);
}

/// Removed on the first frame its `y` rounded down is 33 or more above
/// the view's top: 32.25 above is, 31.3125 above isn't.
#[test]
fn removed_33_above_the_view() {
    for (above, removed) in [(31.3125, false), (32.25, true)] {
        let mut play = Play::carrying(false, 300);
        play.until_passed(&[R, A]);
        let k = falls_away(&mut play, 1.0, |k, world| {
            if k == 4 {
                // The move on T + 4 is 2.25 up.
                let top = world.view.unwrap().y();
                world.things[0].pos.y = Fixed::from_int(top as i32) - f(above - 2.25);
            }
        });
        assert_eq!(k == 4, removed, "{above}: removed on T + {k}");
    }
}

/// Without a `let_go_speed`, the engine keeps the object in the hands
/// through the walk-out.
#[test]
fn without_a_let_go_speed_it_stays_in_the_hands() {
    let mut play = Play::carrying(false, 300);
    let mut feel = (*play.world.feel).clone();
    for character in &mut feel.roster {
        character.physics.set(Param::LetGoSpeed, Fixed::ZERO);
    }
    play.world.feel = Arc::new(feel);
    play.until_passed(&[R, A]);
    play.hold(&[], 10);
    assert!(play.world.player.hands.is_some());
    assert_eq!(play.object().phase, Phase::Held);
}

/// With the second character leading and carrying, the rules hold as
/// they are: let go on `T`, the same fall and removal.
#[test]
fn the_second_character_leading_lets_it_go_the_same_way() {
    for holder in [false, true] {
        let mut play = Play::carrying_by(true, holder, 300);
        assert_eq!(play.world.player.character, 1);
        play.until_passed(&[R, A]);
        let removed = falls_away(&mut play, 1.0, |_, _| {});
        assert!((30..40).contains(&removed), "{removed}");
    }
}

/// ... and a throw still under way never happens then either.
#[test]
fn the_second_character_leading_never_finishes_a_throw() {
    let mut play = Play::carrying_by(true, false, 190);
    play.step(&[]);
    assert_eq!(play.world.player.hands.map(|h| h.act), Some(Act::Throw));
    play.until_passed(&[R]);
    assert_eq!(play.world.things[0].phase, Phase::Held);
    falls_away(&mut play, 1.0, |_, _| {});
}
