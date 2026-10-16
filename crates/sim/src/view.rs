//! The view: the part of the level a preset with `view_width` above 0
//! shows, kept in the simulation because walkers appear and are removed
//! by it (see [`walkers`]).
//!
//! The view's left edge eases toward a target ahead of the player each
//! step, after the player has moved:
//!
//! - The target is the player's `x` less `view_back` facing right, or
//!   `view_back_left` facing left, plus `view_lead_facing` px per px/frame
//!   of speed the way the player faces and `view_lead_speed` px per
//!   px/frame the way it moves. Each binary digit of a lead factor takes
//!   its own share of the speed, rounded to a whole px: down for the
//!   facing part, up for the speed part.
//! - The view's speed closes `1 / view_speed_share` of the gap to the gap
//!   to the target divided by `view_share`, rounded down to
//!   [`SPEED_GRAIN`]; the view moves by its speed rounded toward zero to
//!   [`POS_GRAIN`].
//! - On a step on which the player defeats an enemy by bouncing or
//!   rolling, and for `view_hold` steps in all counting that one, the
//!   view doesn't move.
//!
//! The view starts, and starts again when the player comes back after
//! losing a life, on its target for a player standing still. It isn't
//! kept inside the level.
//!
//! Only the horizontal position is simulated.

use clamber_fixed::{FRAC_BITS, Fixed};
use clamber_pack::{Param, Physics};

use crate::archetype::Behavior;
use crate::flow::FlowEvent;
use crate::player::{Facing, Player, frames};
use crate::world::{Phase, World};

/// What the view's speed is kept to, px per frame.
pub const SPEED_GRAIN: Fixed = Fixed::from_ratio_pow2(1, 13);

/// What the view moves by, px.
pub const POS_GRAIN: Fixed = Fixed::from_ratio_pow2(1, 8);

/// The view's horizontal state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct View {
    /// The level `x` of the view's left edge.
    pub left: Fixed,
    /// How far it moves per step, before rounding.
    pub speed: Fixed,
    /// Steps left of holding still.
    pub hold: u8,
}

impl View {
    /// A view settled on `player`, as if it were standing still.
    pub fn settled(player: &Player, physics: &Physics) -> View {
        let mut still = player.clone();
        still.vel.x = Fixed::ZERO;
        View {
            left: target(&still, physics),
            speed: Fixed::ZERO,
            hold: 0,
        }
    }

    /// The left edge rounded down to a whole px.
    pub fn x(&self) -> i64 {
        self.left.floor()
    }

    /// One step of following `player`. `defeated` says the player
    /// defeated an enemy by bouncing or rolling on this step.
    fn follow(&mut self, player: &Player, physics: &Physics, defeated: bool) {
        if defeated {
            self.hold = frames(physics[Param::ViewHold]);
        }
        if self.hold > 0 {
            self.hold -= 1;
            return;
        }
        let gap = target(player, physics) - self.left;
        let aim = div_floor(gap, whole(physics[Param::ViewShare]));
        let step = div_floor(aim - self.speed, whole(physics[Param::ViewSpeedShare]));
        self.speed = round_down(self.speed + step, SPEED_GRAIN);
        let grain = POS_GRAIN.to_bits();
        let moved = self.speed.to_bits() / grain * grain;
        self.left += Fixed::from_bits(moved);
    }
}

/// Where the view's left edge aims for `player`.
fn target(player: &Player, physics: &Physics) -> Fixed {
    let vx = player.vel.x;
    let facing = shares(vx.abs(), physics[Param::ViewLeadFacing], false);
    let (back, facing) = match player.facing {
        Facing::Right => (physics[Param::ViewBack], facing),
        Facing::Left => (physics[Param::ViewBackLeft], -facing),
    };
    let speed = shares(vx, physics[Param::ViewLeadSpeed], true);
    player.pos.x - back + Fixed::from_int((facing + speed) as i32)
}

/// `v` times `factor` in whole px, each binary digit of `factor` taking
/// its share of `v` rounded on its own: up if `up`, otherwise down.
fn shares(v: Fixed, factor: Fixed, up: bool) -> i64 {
    let unit = 1i128 << (2 * FRAC_BITS);
    let (v, factor) = (i128::from(v.to_bits()), factor.to_bits());
    (0..63)
        .filter(|bit| factor >> bit & 1 == 1)
        .map(|bit| {
            let share = v << bit;
            if up {
                -(-share).div_euclid(unit)
            } else {
                share.div_euclid(unit)
            }
        })
        .sum::<i128>() as i64
}

/// A whole-number param, at least 1.
fn whole(v: Fixed) -> i64 {
    v.floor().max(1)
}

fn div_floor(v: Fixed, by: i64) -> Fixed {
    Fixed::from_bits(v.to_bits().div_euclid(by))
}

fn round_down(v: Fixed, grain: Fixed) -> Fixed {
    let grain = grain.to_bits();
    Fixed::from_bits(v.to_bits().div_euclid(grain) * grain)
}

/// Whether the world has a view: its preset's `view_width` is above 0.
pub(crate) fn enabled(physics: &Physics) -> bool {
    physics[Param::ViewWidth] > Fixed::ZERO
}

/// Moves the view after the player has, if there is one.
pub(crate) fn update(world: &mut World) {
    let physics = world.player_physics();
    let defeated = world
        .flow
        .events
        .iter()
        .any(|e| matches!(e, FlowEvent::Defeat(_)));
    if let Some(view) = &mut world.view {
        view.follow(&world.player, &physics, defeated);
    }
}

/// Settles the view on the player again, if there is one, as when it
/// comes back after losing a life.
pub(crate) fn restart(world: &mut World) {
    let physics = world.player_physics();
    if let Some(view) = &mut world.view {
        *view = View::settled(&world.player, &physics);
    }
}

/// Whether a walker starting at `start` (whole px) is near a view whose
/// left edge is at `left` (whole px).
pub(crate) fn near(start: i64, left: i64, physics: &Physics) -> bool {
    let width = physics[Param::ViewWidth].floor();
    let from = left - physics[Param::AppearLeft].floor();
    let to = left + width + physics[Param::AppearRight].floor();
    (from..=to).contains(&start)
}

/// Whether a walker at `x` (whole px) is far enough from a view whose
/// left edge is at `left` (whole px) to be removed.
fn far(x: i64, left: i64, physics: &Physics) -> bool {
    let width = physics[Param::ViewWidth].floor();
    x <= left - physics[Param::RemoveLeft].floor()
        || x >= left + width + physics[Param::RemoveRight].floor()
}

/// Walkers appearing and being removed with the view, by where its left
/// edge was at the start of the step, `left`:
///
/// - Before things move (`appear`), each walker that isn't in the level
///   and hasn't been defeated appears if its start is near the view: at
///   its start, at rest, heading the way it was placed. It waits its
///   `appear_wait` steps, counting this one, before it moves.
/// - After things move, each walker whose start isn't near the view and
///   that is far from it is removed. One that hadn't been defeated comes
///   back afresh when its start is near the view again.
///
/// At the start of the level, walkers whose start is near the view are
/// in the level as placed, at full speed, and the others aren't.
pub(crate) fn walkers(world: &mut World, left: i64, appear: bool) {
    let physics = world.player_physics();
    let level = &world.level;
    for thing in &mut world.things {
        let Some(Behavior::Walker {
            facing,
            appear_wait,
            ..
        }) = thing.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        let start = level.entities[thing.index].at;
        let start_near = near(start.x.floor(), left, &physics);
        if appear {
            if !thing.active && thing.phase == Phase::Ready && start_near {
                thing.active = true;
                thing.pos = start;
                thing.vx = Fixed::ZERO;
                thing.facing = facing;
                thing.heading = facing;
                thing.turn = 0;
                thing.clock = 0;
                thing.wait = appear_wait;
            }
        } else if thing.active && !start_near && far(thing.pos.x.floor(), left, &physics) {
            thing.active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clamber_level::{Level, Vec2};

    fn fx(v: f64) -> Fixed {
        Fixed::from_f64(v).unwrap()
    }

    fn physics() -> Physics {
        clamber_pack::Presets::builtin().world("classic-1").unwrap()
    }

    fn player(x: f64, vx: f64, facing: Facing) -> Player {
        let level =
            Level::from_toml_str("format = 1\nsize = [4000, 400]\nstart = [0, 0]", "t").unwrap();
        let mut p = Player::new(&level, Vec2::new(fx(x), Fixed::ZERO));
        p.vel.x = fx(vx);
        p.facing = facing;
        p
    }

    #[test]
    fn leads_by_facing_and_speed() {
        // spec/behavior/view.md, "The target": lead for some speeds.
        let physics = physics();
        let lead =
            |vx: f64, facing| (target(&player(1000.0, vx, facing), &physics) - fx(1000.0)).floor();
        let right = [
            (0.0, -71),
            (0.0625, -69),
            (0.125, -68),
            (0.25, -64),
            (0.5, -56),
            (1.0, -41),
            (2.0, -11),
            (3.0, 19),
            (-0.0625, -72),
            (-2.0, -107),
            (-3.0, -125),
        ];
        let left = [
            -127, -125, -124, -122, -118, -109, -91, -73, -128, -187, -217,
        ];
        for ((vx, r), l) in right.into_iter().zip(left) {
            assert_eq!(lead(vx, Facing::Right), r, "{vx} right");
            assert_eq!(lead(vx, Facing::Left), l, "{vx} left");
        }
    }

    /// Steps a view settled on a player standing at 1000 facing right
    /// with the player moved by `at` before each step, checking the
    /// view's left edge and speed (in 1/8192 px per frame) after the
    /// frames given.
    fn follow(at: impl Fn(u32) -> Player, expected: &[(u32, f64, i64)]) {
        let physics = physics();
        let mut view = View::settled(&player(1000.0, 0.0, Facing::Right), &physics);
        assert_eq!(view.left, fx(929.0));
        let last = expected.last().unwrap().0;
        let mut rows = expected.iter();
        let mut next = rows.next();
        for frame in 1..=last {
            view.follow(&at(frame), &physics, false);
            if let Some(&(f, left, speed)) = next
                && f == frame
            {
                assert_eq!(view.left, fx(left), "frame {f}");
                assert_eq!(view.speed.to_bits() >> (FRAC_BITS - 13), speed, "frame {f}");
                next = rows.next();
            }
        }
    }

    #[test]
    fn follows_a_player_moving_right() {
        // view.md, example (a).
        follow(
            |f| player(1000.0 + 2.0 * f as f64, 2.0, Facing::Right),
            &[
                (1, 929.2421875, 1984),
                (2, 929.703125, 3776),
                (3, 930.359375, 5393),
                (10, 939.0546875, 12972),
                (20, 958.19140625, 17147),
                (30, 979.90625, 18078),
                (60, 1044.73828125, 17212),
                (100, 1126.59375, 16535),
                (200, 1326.90625, 16408),
            ],
        );
    }

    #[test]
    fn follows_a_player_turning_left() {
        // view.md, example (b).
        follow(
            |_| player(1000.0, 0.0, Facing::Left),
            &[
                (1, 928.78125, -1792),
                (2, 928.375, -3353),
                (3, 927.80078125, -4706),
                (10, 920.82421875, -9876),
                (20, 908.03515625, -10279),
                (30, 896.875, -8100),
                (60, 879.296875, -2481),
                (100, 873.94921875, -388),
                (200, 873.06640625, -24),
                (300, 873.06640625, -24),
            ],
        );
    }

    #[test]
    fn holds_still_after_a_defeat() {
        let physics = physics();
        let mut view = View::settled(&player(1000.0, 0.0, Facing::Right), &physics);
        let moved = player(1100.0, 0.0, Facing::Right);
        view.follow(&moved, &physics, true);
        view.follow(&moved, &physics, false);
        assert_eq!(view.left, fx(929.0));
        view.follow(&moved, &physics, false);
        assert!(view.left > fx(929.0));
    }

    #[test]
    fn window_and_removal_limits() {
        // spec/behavior/walker-view.md, "The window" and "Being removed".
        let physics = physics();
        assert!(!near(1000, 711, &physics));
        assert!(near(1000, 712, &physics));
        assert!(near(1000, 1031, &physics));
        assert!(!near(1000, 1032, &physics));
        assert!(far(879, 1031, &physics) && near(1000, 1031, &physics));
        assert!(far(878, 1032, &physics));
        assert!(!far(1884, 1500, &physics));
        assert!(far(1885, 1500, &physics));
        assert!(!far(1032 - 127, 1032, &physics));
    }
}
