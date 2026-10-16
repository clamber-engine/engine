//! The other character following the one under control in Classic 1
//! (spec/behavior/following.md).

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{
    Facing, Follow, InputFrame, Mode, Other, Outcome, World, follow_point, generic_feel, step,
};

/// Flat ground at y = 300 from x = 0 to 4096, and a block 40 px high
/// from x = 2000 to 2100.
const FLAT: &str = r#"
    format = 1
    name = "flat"
    size = [4096, 600]
    start = [400, 300]

    [[collision]]
    shape = "rect"
    rect = [0, 300, 4096, 300]

    [[collision]]
    shape = "rect"
    rect = [2000, 260, 100, 40]
"#;

const R: Action = Action::Right;
const J: Action = Action::Jump;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(f(x), f(y))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// Classic 1 with both characters, the first leading, at the level's
    /// start.
    fn new() -> Play {
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::new(),
        });
        let level = Arc::new(Level::from_toml_str(FLAT, "flat").unwrap());
        Play {
            world: World::new(level, Arc::new(feel)),
            held: Actions::NONE,
        }
    }

    /// The made-up example's frame 10: the lead standing still at 1004
    /// facing right, and the follower at 974, its current point, 15
    /// points of 2 px behind the newest with the catching-up mark set.
    fn stopping(facing: Facing) -> Play {
        let mut play = Play::new();
        let w = &mut play.world;
        w.player.pos = v(1004.0, 300.0);
        w.player.facing = Facing::Right;
        play.idle(1);
        let w = &mut play.world;
        assert!(w.player.grounded());
        let mut other = Other::new(1, v(974.0, 300.0), facing, v(974.0, 300.0));
        other.follow.trail = (0..16).map(|k| (974 + 2 * k, 300)).collect();
        other.follow.catching_up = true;
        w.other = Some(other);
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

    fn idle(&mut self, frames: usize) {
        self.hold(&[], frames);
    }

    fn other(&self) -> &Other {
        self.world.other.as_ref().unwrap()
    }
}

#[test]
fn rests_behind_a_lead_that_stops_then_rejoins_its_trail() {
    let mut play = Play::stopping(Facing::Right);
    // Frame 10: on to the current point, a step on, and resting from
    // there toward 976.
    play.idle(1);
    assert_eq!(play.other().pos, v(974.0, 300.0));
    assert_eq!(
        play.other().follow.mode,
        Mode::Rest {
            target: 976,
            gained: false
        }
    );
    let mut xs = Vec::new();
    for _ in 11..=39 {
        play.idle(1);
        xs.push(play.other().pos.x);
    }
    assert_eq!(xs[0], f(974.125), "frame 11");
    assert_eq!(xs[18 - 11], f(975.0), "frame 18");
    assert_eq!(xs[19 - 11], f(975.0625), "frame 19");
    assert_eq!(xs[33 - 11], f(975.9375), "frame 33");
    assert_eq!(xs[34 - 11], f(976.0), "frame 34");
    assert_eq!(xs[39 - 11], f(976.0), "frame 39");

    // Frame 40: the lead's x becomes 1005, and the point is added at the
    // end of the frame.
    play.world.player.pos.x = f(1005.0);
    play.idle(1);
    assert_eq!(play.other().follow.trail.back(), Some(&(1005, 300)));
    // Frame 41: still at 976, it leaves rest with a new trail.
    play.idle(1);
    assert_eq!(play.other().pos.x, f(976.0));
    let xs: Vec<_> = play.other().follow.trail.iter().map(|p| p.0).collect();
    assert_eq!(
        xs,
        [
            976, 977, 979, 981, 983, 985, 986, 988, 990, 992, 994, 995, 997, 999, 1001, 1003, 1005
        ]
    );
    assert_eq!(play.other().follow.mode, Mode::Wait);
    // Frame 42: stays at 976. Frame 43: follows the trail from its first
    // point.
    play.idle(1);
    assert_eq!(play.other().pos.x, f(976.0));
    assert_eq!(play.other().follow.mode, Mode::Trail);
    play.idle(1);
    assert_eq!(play.other().pos.x, f(976.0));
    // The gap is 29: the mark is set, and the step comes a frame later.
    assert!(play.other().follow.catching_up);
    assert_eq!(play.other().follow.trail.front(), Some(&(976, 300)));
    play.idle(1);
    assert_eq!(play.other().follow.trail.front(), Some(&(977, 300)));
}

#[test]
fn turns_toward_the_lead_on_the_fourth_frame_of_a_turn() {
    let mut play = Play::stopping(Facing::Left);
    let mut facings = Vec::new();
    for _ in 0..8 {
        play.idle(1);
        facings.push(play.other().facing);
    }
    use Facing::{Left, Right};
    assert_eq!(
        facings,
        [Left, Left, Left, Right, Right, Right, Right, Right]
    );
    assert_eq!(play.other().follow.turn, None);
}

#[test]
fn faces_the_lead_at_once_off_the_ground() {
    let mut play = Play::stopping(Facing::Left);
    // Its current point is in the air.
    let other = play.world.other.as_mut().unwrap();
    other.follow.trail = VecDeque::from([(974, 280), (1004, 300)]);
    play.idle(1);
    assert_eq!(play.other().facing, Facing::Right);
    assert_eq!(play.other().follow.turn, None);
    assert_eq!(play.other().pos, v(974.0, 280.0));
}

#[test]
fn walks_and_jumps_where_the_lead_did() {
    let mut play = Play::new();
    play.idle(10);
    let mut leads = Vec::new();
    let mut seen = Vec::new();
    // Walk right, jump onto the block, walk on and stop.
    for frame in 0..900 {
        let x = play.world.player.pos.x;
        let keys: &[Action] = if x < f(1960.0) || (x > f(2000.0) && frame < 800) {
            &[R]
        } else if x < f(2000.0) {
            &[R, J]
        } else {
            &[]
        };
        play.step(keys);
        leads.push(follow_point(play.world.player.pos));
        seen.push(follow_point(play.other().pos));
    }
    // Once it's back on the lead's trail after resting at the start, the
    // follower is only ever on a place the lead was at, until the lead
    // stops.
    for (i, at) in seen
        .iter()
        .enumerate()
        .filter(|(i, at)| *i < 800 && at.0 > 1000)
    {
        assert!(leads[..i].contains(at), "frame {i}: {at:?}");
    }
    // It left the ground on the way up to the block, as the lead did.
    assert!(seen.iter().any(|&(x, y)| x < 2000 && y < 300));
    let lead = play.world.player.pos;
    assert_eq!(play.world.player.vel.x, Fixed::ZERO);
    // Once the lead stops on the block, the follower rests 28 px behind
    // it, or in front if behind is down off the block, too far below.
    play.idle(120);
    let other = play.other();
    assert!(matches!(other.follow.mode, Mode::Rest { .. }), "{other:?}");
    let behind = lead.x.floor() - 28;
    let (spot, facing) = if behind < 2000 {
        (lead.x.floor() + 28, Facing::Left)
    } else {
        (behind, Facing::Right)
    };
    assert_eq!(other.pos.x.floor(), spot, "{other:?}");
    assert_eq!(other.pos.y, f(260.0));
    assert_eq!(other.facing, facing);
}

#[test]
fn both_come_back_at_the_start_after_falling_out() {
    let mut play = Play::new();
    play.idle(10);
    play.world.player.pos = v(400.0, 700.0);
    play.idle(1);
    assert!(matches!(play.world.flow.outcome, Outcome::Dying(_)));
    while play.world.flow.outcome != Outcome::Playing {
        play.idle(1);
    }
    assert_eq!(play.world.player.character, 0);
    let other = play.other();
    assert_eq!(other.character, 1);
    assert_eq!(other.pos, play.world.player.pos);
    assert_eq!(other.follow, Follow::at(follow_point(other.pos)));
}
