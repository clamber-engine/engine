//! Throwers and their balls (spec/behavior/thrower.md).
//!
//! A thrower never moves. With a view, it appears and is removed as a
//! walker on its own does (see [`view::walkers`](crate::view)), and its
//! timetable starts again each time it appears. On each of its throws'
//! frames of every period, counted from the one it appeared on, it throws
//! a [`BALL`] (see [`throw`]), which moves on that same step: the spec
//! says a ball is either at its start or one frame on on the frame it's
//! thrown, without saying when, and the engine always has it one frame
//! on.
//!
//! A ball moves as [`fly`] says until it's removed, and doesn't come
//! back. A ball placed in a level appears as a walker on its own would,
//! once, and also moves on the step it appears; without a view it starts
//! moving on the first step.
//!
//! A ball with a [`Contact`](crate::Contact) hurts the player as [`hurt_ahead`] says
//! (spec/behavior/ball-contact.md); it isn't affected itself. It can
//! hurt on the step it appears, which the spec doesn't cover.
//!
//! While the player is knocked out, the thrower, knocked out or not
//! (whether by a bounce, a roll or the ground action), and
//! every ball stop on the frames the player holds still after the hit
//! (`knockout_hold`), and no ball is thrown, so all of them go on that
//! many frames late (thrower.md, "While the player is knocked out").
//! With `view_knockout_hold`, the view doesn't move from the hit until
//! just before they go on, so none appears or leaves then, and a placed
//! ball that wasn't there on the hit frame appears by the usual rule from
//! the frame they go on at the earliest. While a character is lost,
//! nothing stops: they go on as without the hit, and leave by the
//! view of each frame (thrower.md, "While a character is lost"). A
//! thrower knocked out is there again after the player's knock-out, as
//! everything is (see [`flow`](crate::flow)). A thrower with a [`ThrowerContact`](crate::ThrowerContact)
//! hurts the player as [`thrower_hurt_ahead`] says, and is bounced on as
//! [`bounced_on`] says and rolled into as [`rolled_into`] says, which
//! knock it out (spec/behavior/thrower-contact.md). Knocked out, it
//! touches nothing and falls through every floor, and with a view it's
//! removed by its `window` once it moves (see
//! [`things`](crate::things)); it never comes back. Neither touches
//! other enemies. A ball passes through walls, and rising into a floor
//! from below, through that, which the spec doesn't cover either.
//!
//! Engine choices where thrower-contact.md says nothing: crouching, the
//! fidget pose and the ball's rolling pose after a roll take the upright
//! range; a knocked-out thrower touches nothing on the frames from its
//! knock-out on, not only from the fourth, and isn't removed on the
//! frames it holds still; and a rider, or a player on a bounce's frozen
//! frames, doesn't bounce on it.

use clamber_fixed::Fixed;
use clamber_level::{Level, Vec2};
use clamber_pack::{Feel, Param, Physics};

use crate::archetype::{Archetypes, Ball, Behavior};
use crate::columns;
use crate::player::{Facing, State, ceil, frames};
use crate::pose::{self, Pose};
use crate::sections::Window;
use crate::view::View;
use crate::world::{Event, Phase, Thing, World};

/// The kind of the things throwers throw. Its archetype says how they
/// move.
pub const BALL: &str = "ball";

/// Where the picture was at the start of a step, for removing balls:
/// its left and top edges and its size, whole px. A `height` of 0 is a
/// view that doesn't move vertically, which removes nothing above or
/// below it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Picture {
    left: i64,
    top: i64,
    width: i64,
    height: i64,
}

impl Picture {
    /// The picture as things leave it: as wide as the [`window`] around
    /// it, and there.
    ///
    /// [`window`]: crate::view::window
    pub(crate) fn of(view: View, level: &Level, feel: &Feel, physics: &Physics) -> Picture {
        let (left, physics) = crate::view::window(level, feel, view.x(), physics);
        Picture {
            left,
            top: view.y(),
            width: physics[Param::ViewWidth].floor(),
            height: physics[Param::ViewHeight].floor(),
        }
    }

    /// Whether a ball or an object at `pos` is inside `window` around the picture,
    /// with its `x` rounded down and its `y` up.
    pub(crate) fn holds(self, pos: Vec2, window: (i32, i32, i32, i32)) -> bool {
        let (left, right, top, bottom) = window;
        let x = pos.x.floor() - self.left;
        let y = ceil(pos.y) - self.top;
        let across = (-i64::from(left)..=self.width + i64::from(right)).contains(&x);
        let down =
            self.height == 0 || (-i64::from(top)..=self.height + i64::from(bottom)).contains(&y);
        across && down
    }

    /// Whether a defeated walker at `pos` has left `window` around the
    /// picture (walker-view.md, "Defeated walkers falling out"): its `x`
    /// rounded down is left or right of it, or its `y` is below the
    /// window's bottom edge. Never below with a view that doesn't move
    /// vertically, and never above: the spec doesn't cover a defeated
    /// walker above the picture.
    pub(crate) fn lost(self, pos: Vec2, window: &Window) -> bool {
        let x = pos.x.floor() - self.left;
        let below = self.height != 0 && ceil(pos.y) - self.top > *window.down.end();
        !window.across.contains(&x) || below
    }

    /// Whether `pos`, with its `y` rounded down, is `depth` or more below
    /// the picture's top. Never with a view that doesn't move vertically.
    pub(crate) fn below(self, pos: Vec2, depth: i64) -> bool {
        self.height != 0 && pos.y.floor() - self.top >= depth
    }

    /// Whether `pos`, rounded down, is `right` or more right of the
    /// picture's left edge, or `above` or more above its top. An edge of
    /// 0 is never passed, nor is the top with a view that doesn't move
    /// vertically.
    pub(crate) fn beyond(self, pos: Vec2, right: i64, above: i64) -> bool {
        let past_right = right > 0 && pos.x.floor() - self.left >= right;
        let past_top = above > 0 && self.height != 0 && self.top - pos.y.floor() >= above;
        past_right || past_top
    }
}

/// Throws a ball for every throw due this step from a thrower in the
/// level. The thrower's clock counts the steps since it appeared, from 0
/// on the one it appeared on; it moves on when the thrower does.
pub(crate) fn throw(world: &mut World) {
    let mut thrown = Vec::new();
    for thing in world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
    {
        let Some(Behavior::Thrower {
            period,
            throws,
            throw_count,
            from,
            ..
        }) = thing.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        let frame = thing.clock % period;
        let start = Vec2::new(thing.pos.x + from.x, thing.pos.y + from.y);
        for t in throws[..usize::from(throw_count)]
            .iter()
            .filter(|t| t.frame == frame)
        {
            thrown.push((start, t.velocity));
        }
    }
    for (pos, velocity) in thrown {
        spawn(world, pos, velocity);
    }
}

/// Adds a ball at `pos` moving at `velocity`. It takes the place of a
/// ball that's gone, if there is one, so the things a level started with
/// keep their indices and thrown balls don't pile up.
fn spawn(world: &mut World, pos: Vec2, velocity: Vec2) {
    let first = world.level.entities.len();
    let index = world.things[first..]
        .iter()
        .position(|t| !t.active)
        .map_or(world.things.len(), |i| first + i);
    let facing = if velocity.x.is_negative() {
        Facing::Left
    } else {
        Facing::Right
    };
    let ball = Thing {
        index,
        kind: BALL.to_string(),
        pos,
        active: true,
        archetype: Some(
            Archetypes::builtin()
                .named(BALL)
                .expect("the built-in archetypes have a ball"),
        ),
        facing,
        heading: facing,
        turn: 0,
        vx: velocity.x,
        clock: 0,
        wait: 0,
        air: None,
        phase: Phase::Flying {
            vy: velocity.y,
            ignoring: None,
            kept: false,
        },
        still: false,
    };
    if index < world.things.len() {
        world.things[index] = ball;
    } else {
        world.things.push(ball);
    }
}

/// Moves a ball one step: it falls, moves across, and bounces or passes
/// through floors. A ball that hasn't started moving starts at its
/// placed `velocity`.
///
/// While not ignoring floors, its floor is the highest in the column of
/// its new `x` no more than `floor_reach` above where it was. Moving down
/// to less than `touch` above that floor, or below it, it bounces: it's
/// put on the floor at `bounce` upward, and ignores floors until it's
/// more than `drop` below that one.
///
/// With `picture`, where the view was at the start of the step, it's
/// removed once outside its window, except on a step on which it bounced
/// or stopped ignoring floors. Without one, it's removed once its top is
/// below the level.
///
/// Its clock counts its steps, from 0 at its start, but not the steps on
/// which it bounces or stops ignoring floors (ball-contact.md, "The
/// ball's turn").
pub(crate) fn fly(
    level: &Level,
    thing: &mut Thing,
    ball: &Ball,
    size: Vec2,
    picture: Option<Picture>,
) {
    let (vy, ignoring, steps) = match thing.phase {
        Phase::Flying { vy, ignoring, .. } => (vy, ignoring, thing.clock),
        _ => {
            thing.vx = ball.velocity.x;
            (ball.velocity.y, None, 0)
        }
    };
    let mut vy = (vy + ball.gravity).min(ball.max_fall);
    let x = thing.pos.x + thing.vx;
    let mut y = thing.pos.y + vy;
    let mut ignoring = ignoring;
    let mut kept = false;
    match ignoring {
        Some(floor) => {
            if y > floor + ball.drop {
                ignoring = None;
                kept = true;
            }
        }
        None => {
            let lowest = thing.pos.y - ball.floor_reach;
            let floor = columns::spans(level, columns::column(x))
                .into_iter()
                .map(|s| s.top)
                .filter(|&top| top >= lowest)
                .min();
            if let Some(floor) = floor
                && vy > Fixed::ZERO
                && y > floor - ball.touch
            {
                y = floor;
                vy = -ball.bounce;
                ignoring = Some(floor);
                kept = true;
            }
        }
    }
    thing.pos = Vec2::new(x, y);
    thing.phase = Phase::Flying { vy, ignoring, kept };
    thing.clock = if kept { steps } else { steps + 1 };
    let gone = match picture {
        Some(picture) => !kept && !picture.holds(thing.pos, ball.window),
        None => y - size.y > level.size.y,
    };
    if gone {
        thing.active = false;
    }
}

/// The ball that hurts the player on the next step, if any: one with a
/// [`Contact`](crate::Contact) that didn't bounce or stop ignoring floors on this step,
/// and whose offsets from the player, at the end of it, are within the
/// ranges for the player's pose and facing, moved by the part of its
/// turn it was in before its move on it.
pub(crate) fn hurt_ahead(world: &World) -> Option<usize> {
    let player = &world.player;
    let x = player.pos.x.floor();
    let y = ceil(player.pos.y);
    world
        .things
        .iter()
        .filter(|t| t.active && matches!(t.phase, Phase::Flying { kept: false, .. }))
        .find(|t| {
            let Some(Behavior::Ball(Ball {
                contact: Some(contact),
                ..
            })) = t.archetype.map(|a| a.behavior)
            else {
                return false;
            };
            let pose = pose::pose(player, &contact.poses);
            let reach = contact.reach(pose, player.facing, t.clock.saturating_sub(1));
            reach.contains(x - t.pos.x.floor(), y - ceil(t.pos.y))
        })
        .map(|t| t.index)
}

/// The thrower that hurts the player on the next step, if any: one with
/// a [`ThrowerContact`](crate::ThrowerContact) that isn't knocked out,
/// whose offsets from the player at the end of this step are within the
/// range for the player's pose and facing then. On its ledge (on the
/// ground with `above` at `ledge`) only `across` counts, and not on the
/// step the player landed on. Not when the next step is the one before
/// one of its throws, nor after a step on which the player bounced, nor
/// while the player rolls or is in the first `roll_stop_guard` frames of
/// the stop after a roll, as with walkers.
pub(crate) fn thrower_hurt_ahead(world: &World) -> Option<usize> {
    let player = &world.player;
    let physics = world.player_physics();
    let guarded = player.roll_stop > 0 && player.roll_stop <= frames(physics[Param::RollStopGuard]);
    if world.events.contains(&Event::Bounce) || player.state == State::Roll || guarded {
        return None;
    }
    let x = player.pos.x.floor();
    let y = ceil(player.pos.y);
    world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
        .find(|t| {
            let Some(Behavior::Thrower {
                period,
                throws,
                throw_count,
                contact: Some(contact),
                ..
            }) = t.archetype.map(|a| a.behavior)
            else {
                return false;
            };
            // Its clock is already the next step's frame.
            let before = (t.clock + 1) % period;
            if throws[..usize::from(throw_count)]
                .iter()
                .any(|t| t.frame == before)
            {
                return false;
            }
            let range = match pose::pose(player, &contact.poses) {
                Pose::Airborne => contact.airborne,
                Pose::Upright | Pose::Rolling | Pose::Crouching | Pose::Fidget => contact.upright,
            };
            let mut range = contact.facing(range, player.facing);
            let above = y - ceil(t.pos.y);
            if player.grounded() && above == i64::from(contact.ledge) {
                if player.pose.since_land == Some(0) {
                    return false;
                }
                range.above = (contact.ledge, contact.ledge);
            }
            range.contains(x - t.pos.x.floor(), above)
        })
        .map(|t| t.index)
}

/// The thrower the player bounces off on this step, if any: one with a
/// [`ThrowerContact`](crate::ThrowerContact) that isn't knocked out, with
/// the player in the air after its move, not rising at `max_rise` or
/// faster, and its offsets from the thrower then within `bounce` for its
/// facing.
pub(crate) fn bounced_on(world: &World, max_rise: Fixed) -> Option<usize> {
    let player = &world.player;
    let rising_fast = if max_rise > Fixed::ZERO {
        player.vel.y <= -max_rise
    } else {
        player.vel.y.is_negative()
    };
    if player.grounded() || rising_fast {
        return None;
    }
    let x = player.pos.x.floor();
    let y = ceil(player.pos.y);
    world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
        .find(|t| {
            let Some(Behavior::Thrower {
                contact: Some(contact),
                ..
            }) = t.archetype.map(|a| a.behavior)
            else {
                return false;
            };
            contact
                .facing(contact.bounce, player.facing)
                .contains(x - t.pos.x.floor(), y - ceil(t.pos.y))
        })
        .map(|t| t.index)
}

/// The thrower a rolling player defeats on this step, if any: one with a
/// `roll` range in its [`ThrowerContact`](crate::ThrowerContact) that
/// isn't knocked out, on an attack frame (a roll's frames, or the first
/// `roll_attack_stop` frames of the stop after one, not frozen), with the
/// player's offsets from it after its move within that range for its
/// facing. On a roll's first frame (its `roll_start`), the offsets from
/// `before`, where the player was at the start of the step, count too.
pub(crate) fn rolled_into(world: &World, physics: &Physics, before: Vec2) -> Option<usize> {
    let player = &world.player;
    let stop = player.roll_stop > 0 && player.roll_stop <= frames(physics[Param::RollAttackStop]);
    if player.roll.is_none() && !stop {
        return None;
    }
    let first = world.events.contains(&Event::RollStart);
    let offsets =
        |at: Vec2, t: &Thing| (at.x.floor() - t.pos.x.floor(), ceil(at.y) - ceil(t.pos.y));
    world
        .things
        .iter()
        .filter(|t| t.active && t.phase == Phase::Ready)
        .find(|t| {
            let Some(Behavior::Thrower {
                contact: Some(contact),
                ..
            }) = t.archetype.map(|a| a.behavior)
            else {
                return false;
            };
            let Some(roll) = contact.roll else {
                return false;
            };
            let range = contact.facing(roll, player.facing);
            let (across, above) = offsets(player.pos, t);
            let (across_before, above_before) = offsets(before, t);
            range.contains(across, above) || (first && range.contains(across_before, above_before))
        })
        .map(|t| t.index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64) -> Fixed {
        Fixed::from_f64(v).unwrap()
    }

    /// Ground at y = 400 and a thin ledge at y = 300, from x = 0 to 100.
    fn level() -> Level {
        let src = "format = 1\nname = \"t\"\nsize = [100, 600]\nstart = [0, 0]\n\
            [[collision]]\nshape = \"rect\"\nrect = [0, 400, 100, 100]\n\
            [[collision]]\nshape = \"rect\"\nrect = [0, 300, 100, 2]\n";
        Level::from_toml_str(src, "t").unwrap()
    }

    fn ball() -> (Ball, Vec2) {
        let a = Archetypes::builtin().named(BALL).unwrap();
        match a.behavior {
            Behavior::Ball(ball) => (ball, a.size),
            other => panic!("{other:?}"),
        }
    }

    fn at(y: f64, vy: f64, ignoring: Option<f64>) -> Thing {
        Thing {
            index: 0,
            kind: BALL.to_string(),
            pos: Vec2::new(f(50.0), f(y)),
            active: true,
            archetype: None,
            facing: Facing::Right,
            heading: Facing::Right,
            turn: 0,
            vx: Fixed::ZERO,
            clock: 0,
            wait: 0,
            air: None,
            phase: Phase::Flying {
                vy: f(vy),
                ignoring: ignoring.map(f),
                kept: false,
            },
            still: false,
        }
    }

    /// One step from `y` falling at `vy` (before gravity), with the
    /// result's `y`, `vy` and whether it's ignoring floors.
    fn fly_from(y: f64, vy: f64, ignoring: Option<f64>) -> (Fixed, Fixed, bool) {
        let (ball, size) = ball();
        let mut thing = at(y, vy, ignoring);
        fly(&level(), &mut thing, &ball, size, None);
        let Phase::Flying { vy, ignoring, .. } = thing.phase else {
            unreachable!()
        };
        (thing.pos.y, vy, ignoring.is_some())
    }

    /// thrower.md, "Bouncing": measured limits of touching and of a
    /// floor's reach.
    #[test]
    fn touches_within_5_px_and_reaches_floors_8_px_up() {
        // A new y 4.9375 above the floor touches it; 5 above doesn't.
        // Gravity adds 0.4375 to vy first.
        assert_eq!(fly_from(290.0, 4.625, None), (f(300.0), f(-5.0), true));
        assert_eq!(fly_from(290.0, 4.5625, None), (f(295.0), f(5.0), false));
        // From 8 px below the ledge it bounces back onto it; from 8.5 px
        // below, the ledge is out of reach and only the ground counts.
        assert_eq!(fly_from(308.0, 1.0, None), (f(300.0), f(-5.0), true));
        assert_eq!(fly_from(308.5, 1.0, None), (f(309.9375), f(1.4375), false));
        // Rising, it doesn't touch.
        assert_eq!(
            fly_from(301.0, -2.0, None),
            (f(299.4375), f(-1.5625), false)
        );
    }

    /// thrower.md, "Ignoring floors": it stops more than 47 px below the
    /// floor it bounced on, and passes through the ones in between.
    #[test]
    fn ignores_floors_until_47_px_below_the_bounce() {
        let (y, _, ignoring) = fly_from(296.5625, 0.0, Some(250.0));
        assert_eq!((y, ignoring), (f(297.0), true));
        let (y, _, ignoring) = fly_from(296.625, 0.0, Some(250.0));
        assert_eq!((y, ignoring), (f(297.0625), false));
        // Passing the ledge at 300 while ignoring floors.
        let (y, _, ignoring) = fly_from(298.0, 3.0, Some(280.0));
        assert_eq!((y, ignoring), (f(301.4375), true));
    }

    /// thrower.md, "Leaving the picture": the window's edges, and a ball
    /// kept on a frame on which it bounced.
    #[test]
    fn leaves_outside_the_window_but_not_on_a_bounce() {
        let picture = Picture {
            left: 0,
            top: 100,
            width: 256,
            height: 224,
        };
        let (ball, size) = ball();
        let holds = |x: f64, y: f64| picture.holds(Vec2::new(f(x), f(y)), ball.window);
        assert!(holds(288.9, 200.0) && !holds(289.0, 200.0));
        assert!(holds(-31.0, 200.0) && !holds(-31.1, 200.0));
        assert!(holds(10.0, 356.0) && !holds(10.0, 356.1));
        // y rounds up.
        assert!(holds(10.0, 68.1) && !holds(10.0, 68.0));

        // Bouncing on the ground at 400 with the picture's bottom far
        // above: kept on the bounce, gone on the next frame.
        let high = Picture { top: 0, ..picture };
        let mut thing = at(396.0, 4.0, None);
        fly(&level(), &mut thing, &ball, size, Some(high));
        assert!(thing.active);
        assert_eq!(thing.pos.y, f(400.0));
        fly(&level(), &mut thing, &ball, size, Some(high));
        assert!(!thing.active);
    }
}
