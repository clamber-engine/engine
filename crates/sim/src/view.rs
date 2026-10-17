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
//! - With `view_near_left` above 0, while the player's `x` rounded down
//!   is less than it less the speed part, the target is 1 px further
//!   left.
//!
//! With `view_begin_back` above 0, the view starts when an area begins
//! on the target of a lead standing still with `view_begin_back` in
//! place of `view_back` (whatever the facing), kept inside the picture's
//! ends, plus 1/2 px ([`View::begun`]): at the level's start, on
//! resuming in a keg and on walking in. While the characters are in the
//! level's entrance, the target has `view_begin_back` in place of the
//! back and facing parts, and from the throw until the lead lands
//! `view_begin_back_far`; from `view_begin_far` frames after the throw
//! until the landing the view follows quickly ([`Begin`]).
//!
//! With `view_height` above 0, the view's top also follows the player,
//! after it has moved:
//!
//! - It aims `view_feet` px above a ground line: the player's feet, or
//!   in the air, the ground below them if it's within `view_reach` bands
//!   of `view_band` px of theirs, found straight down from the middle of
//!   the feet. With no ground in reach, it aims by the feet. While there
//!   is ground and the feet are in the top `view_feet_top` + 1 rows of
//!   the view, it aims by the view's top row instead, so it moves up.
//! - That aim is kept inside the camera rule's `limits` for where the
//!   player is (the level's bounds without them), and the view aims 1/2
//!   px below it.
//! - A smoothed gap eases toward the gap from the view's top to its aim
//!   (a gap of less than `view_slack_y` downward counts as none), and the
//!   view moves by part of it, both kept to [`Y_GRAIN`]. The view follows
//!   quickly (`view_share_y_quick`, `view_speed_share_y_quick`) when the
//!   feet are low in the view, when they're fairly low and falling fast,
//!   or where the camera rule says `quick`.
//! - It holds still with the horizontal motion after a defeat.
//! - With `view_shake_count` above 0, a ground action makes it shake:
//!   from `view_shake_delay` frames after the press frame, every
//!   `view_shake_period` frames, an offset is added to its top after it
//!   has moved. The offsets alternate down and up, starting with
//!   `view_shake_size` down and halving after each pair. The smoothed gap
//!   isn't changed by them.
//!
//! Otherwise the view starts, and starts again when the player comes
//! back after losing a life, on its target for a player standing still. It isn't
//! kept inside the level horizontally, but with `end_stop` above 0 the
//! picture is (level-ends.md): its left edge, [`View::x`], is the view's
//! rounded down and kept between the level-wide camera limits' left and
//! their right less the view's width, while the view itself moves on
//! past them. The player is stopped `end_stop` px inside the range the
//! picture can show (see [`stops`]).
//!
//! The camera rule is chosen by the player's position, not just its `x`
//! as the spec's list of limits is.

use std::collections::{HashMap, HashSet};

use clamber_fixed::{FRAC_BITS, Fixed};
use clamber_level::{Dir, Level, Rect, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::Behavior;
use crate::flow::FlowEvent;
use crate::player::{Facing, Player, ceil, frames};
use crate::sections::{Opening, Window};
use crate::world::{Phase, Thing, World};

/// What the view's speed is kept to, px per frame.
pub const SPEED_GRAIN: Fixed = Fixed::from_ratio_pow2(1, 13);

/// What the view moves by, px.
pub const POS_GRAIN: Fixed = Fixed::from_ratio_pow2(1, 8);

/// What the view's top and its smoothed gap are kept to, px.
pub const Y_GRAIN: Fixed = Fixed::from_ratio_pow2(1, 8);

/// How far below its aim, kept inside the limits, the view's top aims.
const AIM_BELOW: Fixed = Fixed::from_ratio_pow2(1, 1);

/// How fast frames multiply the view's speed: on the `k`-th fast frame in
/// a row, by `m` while `k` is at most `upto`, and by [`FAST_LAST`] after
/// these.
const FAST_TIMES: [(u32, i64); 3] = [(4, 1), (10, 2), (14, 4)];
const FAST_LAST: i64 = 8;

/// How the view follows on one frame: as usual, or as when an area
/// begins (view.md, "When an area begins").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Begin {
    /// What the target has in place of the back and the facing parts,
    /// whatever the facing, if anything.
    pub back: Option<Fixed>,
    /// The `k`-th fast frame in a row, or 0 on a frame that isn't fast.
    pub fast: u32,
}

/// The view's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct View {
    /// The level `x` of the view's left edge.
    pub left: Fixed,
    /// How far it moves per step, before rounding.
    pub speed: Fixed,
    /// Steps left of holding still.
    pub hold: u8,
    /// The level `y` of the view's top edge.
    pub top: Fixed,
    /// The gap from the top to where it aims, smoothed.
    pub gap: Fixed,
    /// Frames since the press frame of the ground action whose shake is
    /// under way, if one is.
    pub shake: Option<u8>,
    /// What the picture's left edge is kept between, if anything (see
    /// [`ends`]).
    pub ends: Option<(i64, i64)>,
}

impl View {
    /// The view on an area's first frame with `player` the lead: with
    /// `view_begin_back` above 0, on the target for it standing still
    /// with that in place of the back and facing parts, kept inside the
    /// picture's ends, plus 1/2 px; otherwise [`View::settled`].
    pub fn begun(player: &Player, level: &Level, physics: &Physics) -> View {
        let mut view = View::settled(player, level, physics);
        if let Some(left) = begin_left(player.pos.x, level, physics) {
            view.left = left;
        }
        view
    }

    /// A view settled on `player`, as if it were standing still on the
    /// ground.
    pub fn settled(player: &Player, level: &Level, physics: &Physics) -> View {
        let mut still = player.clone();
        still.vel.x = Fixed::ZERO;
        let top = if vertical(physics) {
            let feet = ceil(player.pos.y);
            let aim = feet - whole(physics[Param::ViewFeet]);
            limit(aim, player, level, physics).0
        } else {
            Fixed::ZERO
        };
        View {
            left: target(&still, physics, None),
            speed: Fixed::ZERO,
            hold: 0,
            top,
            gap: Fixed::ZERO,
            shake: None,
            ends: ends(level, physics),
        }
    }

    /// The picture's left edge: the view's rounded down to a whole px,
    /// kept inside its `ends`.
    pub fn x(&self) -> i64 {
        let x = self.left.floor();
        match self.ends {
            Some((left, right)) => x.max(left).min(right),
            None => x,
        }
    }

    /// The top edge rounded down to a whole px.
    pub fn y(&self) -> i64 {
        self.top.floor()
    }

    /// One step of following `player`, as `begin` says. `defeated` says
    /// the player defeated an enemy by bouncing or rolling on this step.
    fn follow(
        &mut self,
        player: &Player,
        level: &Level,
        physics: &Physics,
        defeated: bool,
        begin: Begin,
    ) {
        self.shake = if player.ground_action.is_some_and(|a| a.frame == 0) {
            Some(0)
        } else {
            self.shake.map(|n| n.saturating_add(1))
        };
        let count = u32::try_from(physics[Param::ViewShakeCount].floor()).unwrap_or(0);
        let last = frames(physics[Param::ViewShakeDelay]).saturating_add(
            frames(physics[Param::ViewShakePeriod])
                .saturating_mul(u8::try_from(count.saturating_sub(1)).unwrap_or(u8::MAX)),
        );
        if count == 0 || self.shake.is_some_and(|n| n > last) {
            self.shake = None;
        }
        if defeated {
            self.hold = frames(physics[Param::ViewHold]);
        }
        if self.hold > 0 {
            self.hold -= 1;
            return;
        }
        if vertical(physics) {
            self.follow_y(player, level, physics);
        }
        let gap = target(player, physics, begin.back) - self.left;
        let aim = div_floor(gap, whole(physics[Param::ViewShare]));
        let (share, times) = match begin.fast {
            0 => (Param::ViewSpeedShare, 1),
            k => (Param::ViewFastSpeedShare, fast_times(k)),
        };
        let step = div_floor(aim - self.speed, whole(physics[share]));
        self.speed = round_down(self.speed + step, SPEED_GRAIN);
        let grain = POS_GRAIN.to_bits();
        let moved = self.speed.to_bits() * times / grain * grain;
        self.left += Fixed::from_bits(moved);
    }

    /// One step of following `player` vertically.
    fn follow_y(&mut self, player: &Player, level: &Level, physics: &Physics) {
        let feet = ceil(player.pos.y);
        let top = self.y();
        let below = feet - top;
        let near_top = (0..=whole(physics[Param::ViewFeetTop])).contains(&below);
        let (aim, below) = match ground_below(player, level, physics) {
            None => (feet, below),
            Some(_) if near_top => (top, 0),
            Some(ground) => (ground, below),
        };
        let (target, quick_here) = limit(
            aim - whole(physics[Param::ViewFeet]),
            player,
            level,
            physics,
        );
        let quick = quick_here
            || below >= whole(physics[Param::ViewQuickBelow])
            || (below >= whole(physics[Param::ViewQuickFallBelow])
                && player.vel.y > physics[Param::ViewQuickFall]);
        let (share, speed_share) = if quick {
            (Param::ViewShareYQuick, Param::ViewSpeedShareYQuick)
        } else {
            (Param::ViewShareY, Param::ViewSpeedShareY)
        };
        let mut gap = target - self.top;
        if gap >= Fixed::ZERO && gap < physics[Param::ViewSlackY] {
            gap = Fixed::ZERO;
        }
        let ease = whole(physics[speed_share]);
        let grain = Y_GRAIN.to_bits();
        let units = |v: Fixed| v.to_bits().div_euclid(grain);
        let smoothed = units(self.gap) + (units(gap) - units(self.gap)).div_euclid(ease);
        self.gap = Fixed::from_bits(smoothed * grain);
        let moved = smoothed.div_euclid(whole(physics[share]));
        self.top += Fixed::from_bits(moved * grain);
        self.top += self.shake_offset(physics);
    }

    /// The shake's offset to the top on this frame, down positive.
    fn shake_offset(&self, physics: &Physics) -> Fixed {
        let Some(since) = self.shake else {
            return Fixed::ZERO;
        };
        let delay = frames(physics[Param::ViewShakeDelay]);
        let period = whole(physics[Param::ViewShakePeriod]);
        let Some(after) = since.checked_sub(delay).map(i64::from) else {
            return Fixed::ZERO;
        };
        if after % period != 0 {
            return Fixed::ZERO;
        }
        let n = after / period;
        let size = physics[Param::ViewShakeSize].to_bits() >> (n / 2).min(63);
        Fixed::from_bits(if n % 2 == 0 { size } else { -size })
    }
}

/// Whether the view follows the player vertically: its preset's
/// `view_height` is above 0.
pub(crate) fn vertical(physics: &Physics) -> bool {
    physics[Param::ViewHeight] > Fixed::ZERO
}

/// The line of the ground the view aims by for `player`, whole px: its
/// feet on the ground, or in the air the ground straight below them if
/// it's in reach.
fn ground_below(player: &Player, level: &Level, physics: &Physics) -> Option<i64> {
    let feet = ceil(player.pos.y);
    if player.grounded() {
        return Some(feet);
    }
    let band = whole(physics[Param::ViewBand]);
    let feet_band = (feet - 1).div_euclid(band);
    let last = band * (feet_band + physics[Param::ViewReach].floor() + 1);
    let reach = Fixed::from_int(i32::try_from(last).ok()?) - player.pos.y;
    let hit = level.collision.cast(player.pos, Dir::Down, reach)?;
    Some(ceil(hit.point.y))
}

/// Where the view's top aims for an aim `aim` (whole px) for `player`,
/// kept inside the camera rule's limits there, and whether that rule
/// says to follow quickly.
fn limit(aim: i64, player: &Player, level: &Level, physics: &Physics) -> (Fixed, bool) {
    let rule = level.camera_at(player.pos);
    let limits = rule.limits.unwrap_or(Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: level.size.x,
        h: level.size.y,
    });
    let highest = limits.y.floor();
    let lowest = (limits.y + limits.h).floor() - whole(physics[Param::ViewHeight]);
    let aim = aim.max(highest).min(lowest);
    let aim = Fixed::from_int(i32::try_from(aim).unwrap_or(i32::MAX));
    (aim + AIM_BELOW, rule.quick == Some(true))
}

/// Where the view's left edge aims for `player`, with `back` in place of
/// the back and facing parts if it's given.
fn target(player: &Player, physics: &Physics, back: Option<Fixed>) -> Fixed {
    let vx = player.vel.x;
    let facing = shares(vx.abs(), physics[Param::ViewLeadFacing], false);
    let (back, facing) = match (back, player.facing) {
        (Some(back), _) => (back, 0),
        (None, Facing::Right) => (physics[Param::ViewBack], facing),
        (None, Facing::Left) => (physics[Param::ViewBackLeft], -facing),
    };
    let speed = shares(vx, physics[Param::ViewLeadSpeed], true);
    let near = physics[Param::ViewNearLeft].floor();
    let near_left = i64::from(near > 0 && player.pos.x.floor() < near - speed);
    player.pos.x - back + Fixed::from_int((facing + speed - near_left) as i32)
}

/// Where the view's left edge starts on an area's first frame with the
/// lead at `x`, with `view_begin_back` above 0: `view_begin_back` left of
/// `x` rounded down, 1 px further left from 0 to `view_near_left` less 1,
/// kept inside the picture's ends, plus 1/2 px.
pub(crate) fn begin_left(x: Fixed, level: &Level, physics: &Physics) -> Option<Fixed> {
    let back = physics[Param::ViewBeginBack].floor();
    if back <= 0 {
        return None;
    }
    let w = x.floor();
    let near = physics[Param::ViewNearLeft].floor();
    let mut t = w - back - i64::from((0..near).contains(&w));
    if let Some((left, right)) = ends(level, physics) {
        t = t.clamp(left, right);
    }
    let t = Fixed::from_int(i32::try_from(t).unwrap_or(i32::MAX));
    Some(t + Fixed::from_ratio_pow2(1, 1))
}

/// What the view's speed is multiplied by on the `k`-th fast frame in a
/// row.
fn fast_times(k: u32) -> i64 {
    FAST_TIMES
        .iter()
        .find(|&&(upto, _)| k <= upto)
        .map_or(FAST_LAST, |&(_, m)| m)
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

/// The range the picture's left edge is kept in, with `end_stop` above
/// 0: from the level-wide camera limits' left to their right less the
/// view's width (level-ends.md, `A` and `B`).
pub(crate) fn ends(level: &Level, physics: &Physics) -> Option<(i64, i64)> {
    if !enabled(physics) || physics[Param::EndStop] <= Fixed::ZERO {
        return None;
    }
    let limits = level.camera.rule.limits?;
    let left = limits.x.floor();
    let right = (limits.x + limits.w).floor() - physics[Param::ViewWidth].floor();
    Some((left, right.max(left)))
}

/// Whether a player at `x` at the start of a frame, moving across at
/// `vx`, is stopped at one of the level's ends: moving right from
/// `end_stop` px inside the right of the range the picture can show or
/// past it, or left from closer than that to its left (level-ends.md).
pub(crate) fn stops(x: Fixed, vx: Fixed, level: &Level, physics: &Physics) -> bool {
    let Some((left, right)) = ends(level, physics) else {
        return false;
    };
    let stop = physics[Param::EndStop].floor();
    let width = physics[Param::ViewWidth].floor();
    let x = x.floor();
    (vx > Fixed::ZERO && x >= right + width - stop) || (vx < Fixed::ZERO && x < left + stop)
}

/// Whether the world has a view: its preset's `view_width` is above 0.
pub(crate) fn enabled(physics: &Physics) -> bool {
    physics[Param::ViewWidth] > Fixed::ZERO
}

/// Moves the view after the player has, if there is one, as `begin`
/// says.
pub(crate) fn update(world: &mut World, begin: Begin) {
    let physics = world.player_physics();
    let defeated = world
        .flow
        .events
        .iter()
        .any(|e| matches!(e, FlowEvent::Defeat(_)));
    if let Some(view) = &mut world.view {
        view.follow(&world.player, &world.level, &physics, defeated, begin);
    }
}

/// Settles the view on the player again, if there is one, as when it
/// comes back after losing a life. Sections begin again from there, as
/// at the start of an area (the engine's choice).
pub(crate) fn restart(world: &mut World) {
    let physics = world.player_physics();
    if let Some(view) = &mut world.view {
        *view = View::settled(&world.player, &world.level, &physics);
        if let Some(placing) = &mut world.placing {
            placing.opening = Opening::begin(&placing.placed.sections, view.y());
        }
    }
}

/// Whether a walker starting at `start` (whole px) is near a view whose
/// left edge is at `left` (whole px). The window never reaches left of
/// level x 1: while it would, it stays where it is with its left end
/// there (placements.md, "When placed objects appear").
pub(crate) fn near(start: i64, left: i64, physics: &Physics) -> bool {
    let width = physics[Param::ViewWidth].floor();
    let reach = physics[Param::AppearLeft].floor();
    let left = left.max(1 + reach);
    let from = left - reach;
    let to = left + width + physics[Param::AppearRight].floor();
    (from..=to).contains(&start)
}

/// Whether a walker at `x` (whole px) is far enough from a view whose
/// left edge is at `left` (whole px) to be removed.
pub(crate) fn far(x: i64, left: i64, physics: &Physics) -> bool {
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
/// The walkers of a group (the same `group` above 0) appear together
/// when any of their starts is near the view (walker-view.md, "Groups").
/// The spec doesn't say how a group is removed; the engine's own rule is
/// that its walkers are removed together, once none of their starts is
/// near the view and every one of them in the level is far from it.
///
/// At the start of the level, walkers whose start is near the view are
/// in the level as placed, at full speed, and the others aren't.
///
/// Throwers appear and are removed as walkers on their own do, starting
/// their timetable afresh each time they appear, and stay gone once
/// knocked out; a ball placed in the
/// level appears as one would, once (thrower.md). Thrown balls leave by
/// their own rule (see [`throwers`](crate::throwers)).
///
/// Stingers appear and are removed as walkers on their own do, and come
/// back afresh (stinger.md).
///
/// Scenery appears and is removed as throwers are, and comes back each
/// time its start is near the view again (signs.md).
///
/// A collectible there `once` appears as throwers do, rising from where
/// it's placed if it rises, and is removed on the first step its start
/// isn't near the view. Removed or taken, it doesn't come back
/// (extra-lives.md).
///
/// In a level with sections, its things in them appear only by the
/// sections' rules ([`Placing`]), as walkers, throwers, balls and
/// stingers otherwise would; other things there appear as they are
/// placed. They are removed as they would be otherwise, and can then
/// appear again. Things that aren't removed by the view stay gone once
/// they leave by their own rule.
pub(crate) fn walkers(world: &mut World, (left, top): (i64, i64), appear: bool) {
    let physics = world.player_physics();
    if appear && let Some(placing) = &mut world.placing {
        let placed = &placing.placed;
        let there = &placing.there;
        let window = Window::of(&physics);
        let up = placing
            .opening
            .step(&placed.sections, &placed.spots, &window, (left, top), |i| {
                there[i]
            });
        for i in up {
            placing.there[i] = true;
            for &e in &placed.entities[i] {
                arrive(&mut world.things[e], world.level.entities[e].at);
            }
        }
    }
    let placing = world.placing.as_mut();
    let (spot_of, mut there) = match placing {
        Some(p) => (Some(&p.placed.spot_of), Some(&mut p.there)),
        None => (None, None),
    };
    let in_spot = |index: usize| spot_of.and_then(|s| s.get(index).copied().flatten());
    // A thing in a spot that the view removes is no longer there.
    let mut removed = |index: usize| {
        if let (Some(spot), Some(there)) = (in_spot(index), there.as_deref_mut()) {
            there[spot] = false;
        }
    };
    let level = &world.level;
    // Each group's walkers: whether any start is near the view, and
    // whether every one in the level could be removed.
    let mut groups: HashMap<u32, (bool, bool)> = HashMap::new();
    for thing in &world.things {
        if let Some(Behavior::Walker { group, .. }) = thing.archetype.map(|a| a.behavior)
            && group > 0
        {
            let start = level.entities[thing.index].at.x.floor();
            let (near_any, far_all) = groups.entry(group).or_insert((false, true));
            *near_any |= near(start, left, &physics);
            *far_all &= !thing.active || far(thing.pos.x.floor(), left, &physics);
        }
    }
    for thing in &mut world.things {
        let Some(Behavior::Walker { group, .. }) = thing.archetype.map(|a| a.behavior) else {
            continue;
        };
        let start = level.entities[thing.index].at;
        let (start_near, far) = match groups.get(&group) {
            Some(&(near_any, far_all)) => (near_any, far_all),
            None => (
                near(start.x.floor(), left, &physics),
                far(thing.pos.x.floor(), left, &physics),
            ),
        };
        if appear {
            if in_spot(thing.index).is_none() && start_near {
                arrive(thing, start);
            }
        } else if thing.active && !start_near && far {
            thing.active = false;
            removed(thing.index);
        }
    }
    for thing in &mut world.things {
        let (Some(archetype), Some(entity)) = (thing.archetype, level.entities.get(thing.index))
        else {
            continue;
        };
        let start = entity.at;
        let start_near = near(start.x.floor(), left, &physics);
        let by_view = in_spot(thing.index).is_none();
        match archetype.behavior {
            Behavior::Thrower { .. }
            | Behavior::Ball(_)
            | Behavior::Stinger(_)
            | Behavior::Scenery
            | Behavior::Collect { once: true, .. }
                if appear && by_view && start_near =>
            {
                arrive(thing, start);
            }
            Behavior::Collect { once: true, .. } if !appear && thing.active && !start_near => {
                thing.active = false;
                thing.phase = Phase::Gone;
                removed(thing.index);
            }
            Behavior::Thrower { .. } | Behavior::Scenery
                if !appear && thing.active && far(start.x.floor(), left, &physics) =>
            {
                thing.active = false;
                removed(thing.index);
            }
            Behavior::Stinger(_)
                if !appear
                    && thing.active
                    && !start_near
                    && far(thing.pos.x.floor(), left, &physics) =>
            {
                thing.active = false;
                removed(thing.index);
            }
            _ => {}
        }
    }
}

/// A thing that isn't in the level appearing at `start`: a walker at
/// rest, heading the way it was placed, waiting its `appear_wait` steps;
/// a thrower starting its timetable; a stinger afresh, even after being
/// knocked out. Defeated walkers, knocked-out throwers and thrown balls
/// don't appear.
fn arrive(thing: &mut Thing, start: Vec2) {
    if thing.active {
        return;
    }
    let behavior = thing.archetype.map(|a| a.behavior);
    if thing.phase != Phase::Ready && !matches!(behavior, Some(Behavior::Stinger(_))) {
        return;
    }
    thing.active = true;
    match behavior {
        Some(Behavior::Walker {
            facing,
            appear_wait,
            ..
        }) => {
            thing.pos = start;
            thing.vx = Fixed::ZERO;
            thing.facing = facing;
            thing.heading = facing;
            thing.turn = 0;
            thing.clock = 0;
            thing.wait = appear_wait;
        }
        Some(Behavior::Thrower { .. }) => thing.clock = 0,
        Some(Behavior::Ball(_)) => thing.pos = start,
        Some(Behavior::Collect { .. }) => {
            thing.pos = start;
            thing.clock = 0;
        }
        // It starts afresh when it moves on this step.
        Some(Behavior::Stinger(_)) => thing.phase = Phase::Ready,
        _ => {}
    }
}

/// Whether each thing is in the level at the start of a level with a
/// view whose left edge is at `left`: walkers only when their start, or
/// another start in their group, is near the view, throwers, stingers,
/// scenery and collectibles there `once` only when theirs is, and balls
/// not yet; everything else is.
pub(crate) fn at_start(things: &[Thing], level: &Level, left: i64, physics: &Physics) -> Vec<bool> {
    let walker = |thing: &Thing| match thing.archetype.map(|a| a.behavior) {
        Some(Behavior::Walker { group, .. }) => Some(group),
        _ => None,
    };
    let near_start = |thing: &Thing| near(level.entities[thing.index].at.x.floor(), left, physics);
    let near_groups: HashSet<u32> = things
        .iter()
        .filter_map(|t| walker(t).filter(|&g| g > 0 && near_start(t)))
        .collect();
    things
        .iter()
        .map(|t| match t.archetype.map(|a| a.behavior) {
            Some(Behavior::Walker { group, .. }) => near_start(t) || near_groups.contains(&group),
            Some(
                Behavior::Thrower { .. }
                | Behavior::Stinger(_)
                | Behavior::Scenery
                | Behavior::Collect { once: true, .. },
            ) => near_start(t),
            Some(Behavior::Ball(_)) => false,
            _ => true,
        })
        .collect()
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

    /// A level with nothing in it.
    fn level() -> Level {
        Level::from_toml_str("format = 1\nsize = [4000, 2000]\nstart = [0, 0]", "t").unwrap()
    }

    /// A level with ground at 400 all along, and `extra` added.
    fn ground(extra: &str) -> Level {
        let src = format!(
            "format = 1\nsize = [4000, 2000]\nstart = [0, 0]\n\
             [[collision]]\nshape = \"rect\"\nrect = [0, 400, 4000, 100]\n{extra}"
        );
        Level::from_toml_str(&src, "t").unwrap()
    }

    fn player(x: f64, vx: f64, facing: Facing) -> Player {
        let mut p = Player::new(&level(), Vec2::new(fx(x), Fixed::ZERO));
        p.vel.x = fx(vx);
        p.facing = facing;
        p
    }

    #[test]
    fn leads_by_facing_and_speed() {
        // spec/behavior/view.md, "The target": lead for some speeds.
        let physics = physics();
        let lead = |vx: f64, facing| {
            (target(&player(1000.0, vx, facing), &physics, None) - fx(1000.0)).floor()
        };
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

    #[test]
    fn leads_1_px_less_near_the_left_end() {
        // view.md, "Near the left end": standing still below 128, and
        // running right at 3 px per frame (speed part 72) below 56.
        let physics = physics();
        let lead = |x: f64, vx: f64| {
            (target(&player(x, vx, Facing::Right), &physics, None) - fx(x)).floor()
        };
        assert_eq!(lead(127.75, 0.0), -72);
        assert_eq!(lead(128.0, 0.0), -71);
        assert_eq!(lead(55.5, 3.0), 19 - 1);
        assert_eq!(lead(56.0, 3.0), 19);
        // In place of the back and facing parts, as when an area begins.
        let begin = |x: f64| target(&player(x, 0.0, Facing::Left), &physics, Some(fx(79.0)));
        assert_eq!(begin(100.0), fx(20.0));
        assert_eq!(begin(1000.0), fx(921.0));
    }

    #[test]
    fn begins_79_left_of_the_lead_plus_a_half() {
        // view.md, "When an area begins", frame 0.
        let physics = physics();
        let left = |x: f64| begin_left(fx(x), &level(), &physics).unwrap();
        assert_eq!(left(1000.75), fx(921.5));
        assert_eq!(left(128.0), fx(49.5));
        assert_eq!(left(127.5), fx(47.5));
        // Kept inside the picture's ends: `A` = 0 here.
        assert_eq!(left(24.0), fx(0.5));
        assert_eq!(fast_times(1), 1);
        assert_eq!(fast_times(5), 2);
        assert_eq!(fast_times(14), 4);
        assert_eq!(fast_times(15), 8);
    }

    /// Steps a view settled on a player standing at 1000 facing right
    /// with the player moved by `at` before each step, checking the
    /// view's left edge and speed (in 1/8192 px per frame) after the
    /// frames given.
    fn follow(at: impl Fn(u32) -> Player, expected: &[(u32, f64, i64)]) {
        let physics = physics();
        let mut view = View::settled(&player(1000.0, 0.0, Facing::Right), &level(), &physics);
        assert_eq!(view.left, fx(929.0));
        let last = expected.last().unwrap().0;
        let mut rows = expected.iter();
        let mut next = rows.next();
        for frame in 1..=last {
            view.follow(&at(frame), &level(), &physics, false, Begin::default());
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
        let level = level();
        let mut view = View::settled(&player(1000.0, 0.0, Facing::Right), &level, &physics);
        let top = view.top;
        let moved = player(1100.0, 0.0, Facing::Right);
        let mut moved_down = moved.clone();
        moved_down.pos.y = fx(300.0);
        view.follow(&moved_down, &level, &physics, true, Begin::default());
        view.follow(&moved_down, &level, &physics, false, Begin::default());
        assert_eq!((view.left, view.top), (fx(929.0), top));
        view.follow(&moved_down, &level, &physics, false, Begin::default());
        assert!(view.left > fx(929.0));
        assert!(view.top > top);
    }

    /// A player at 1000 with its feet at `feet`, falling at `vy`, on the
    /// ground (collider 0) if `grounded`.
    fn at_feet(feet: i64, vy: i64, grounded: bool) -> Player {
        let mut p = player(1000.0, 0.0, Facing::Right);
        p.pos.y = Fixed::from_int(feet as i32);
        p.vel.y = Fixed::from_int(vy as i32);
        p.ground = grounded.then_some(0);
        p
    }

    /// Steps a view settled on a player standing with its feet at
    /// `settle` in `level`, with the player given by `at` before each
    /// step, checking the view's top and smoothed gap (in 1/256 px) after
    /// the frames given.
    fn follow_y(
        level: &Level,
        settle: i64,
        at: impl Fn(u32) -> Player,
        expected: &[(u32, f64, i64)],
    ) {
        let physics = physics();
        let mut view = View::settled(&at_feet(settle, 0, true), level, &physics);
        let last = expected.last().unwrap().0;
        let mut rows = expected.iter();
        let mut next = rows.next();
        for frame in 1..=last {
            view.follow(&at(frame), level, &physics, false, Begin::default());
            if let Some(&(f, top, gap)) = next
                && f == frame
            {
                assert_eq!(view.top, fx(top), "frame {f}");
                assert_eq!(view.gap.to_bits() >> (FRAC_BITS - 8), gap, "frame {f}");
                next = rows.next();
            }
        }
    }

    #[test]
    fn settles_on_higher_ground() {
        // spec/behavior/view-vertical.md, example (a).
        let level = ground("");
        let physics = physics();
        let view = View::settled(&at_feet(400, 0, true), &level, &physics);
        assert_eq!((view.top, view.gap), (fx(240.5), Fixed::ZERO));
        follow_y(
            &level,
            400,
            |_| at_feet(368, 0, true),
            &[
                (1, 240.46875, -512),
                (2, 240.40625, -992),
                (3, 240.31640625, -1441),
                (10, 239.05859375, -3833),
                (30, 232.4140625, -6052),
                (60, 222.25390625, -4653),
                (100, 214.10546875, -2190),
                (200, 208.8359375, -175),
            ],
        );
    }

    #[test]
    fn follows_a_jump_out_of_reach_of_the_ground() {
        // view-vertical.md, example (b).
        let feet = |f: u32| {
            let f = i64::from(f);
            if f <= 15 {
                400 - 6 * f
            } else {
                (310 + 6 * (f - 15)).min(400)
            }
        };
        follow_y(
            &ground(""),
            400,
            |f| at_feet(feet(f), if f <= 15 { -6 } else { 6 }, f >= 30),
            &[
                (1, 240.5, 0),
                (13, 240.5, 0),
                (14, 240.41796875, -1344),
                (15, 240.25, -2699),
                (16, 240.01171875, -3871),
                (17, 239.7890625, -3622),
                (30, 237.984375, -1306),
                (46, 237.45703125, 10),
                (60, 237.66796875, 450),
            ],
        );
    }

    #[test]
    fn follows_a_fast_fall_quickly() {
        // view-vertical.md, example (c).
        follow_y(
            &level(),
            300,
            |f| at_feet(300 + 8 * i64::from(f), 8, false),
            &[
                (1, 140.5625, 256),
                (2, 140.73828125, 734),
                (5, 142.3984375, 3197),
                (10, 150.58984375, 9264),
                (20, 190.61328125, 21824),
                (30, 255.71875, 29845),
            ],
        );
    }

    #[test]
    fn ground_in_reach_depends_on_its_band() {
        // view-vertical.md, "The ground used": the last feet line that
        // still uses each ground line.
        let physics = physics();
        for (ground_line, last) in [(353, 289), (337, 257), (320, 225)] {
            let level = Level::from_toml_str(
                &format!(
                    "format = 1\nsize = [4000, 2000]\nstart = [0, 0]\n\
                     [[collision]]\nshape = \"rect\"\nrect = [0, {ground_line}, 4000, 100]"
                ),
                "t",
            )
            .unwrap();
            let used = |feet| ground_below(&at_feet(feet, 0, false), &level, &physics);
            assert_eq!(used(last), Some(ground_line), "{ground_line}");
            assert_eq!(used(last - 1), None, "{ground_line}");
        }
    }

    #[test]
    fn feet_near_the_top_hold_the_aim() {
        // view-vertical.md, "The target", case 2: on the ground, feet in
        // the top 48 rows of the view aim by the view's top row (at 80.5
        // here, 160 px up), and lower feet by themselves (128.5).
        let level = ground("");
        let physics = physics();
        let settled = View::settled(&at_feet(400, 0, true), &level, &physics);
        let gap = |feet| {
            let mut view = settled;
            view.follow(
                &at_feet(feet, 0, true),
                &level,
                &physics,
                false,
                Begin::default(),
            );
            view.gap.to_bits() >> (FRAC_BITS - 8)
        };
        assert_eq!(gap(240 + 47), -160 * 256 / 16);
        assert_eq!(gap(240 + 48), -112 * 256 / 16);
    }

    #[test]
    fn limits_and_quick_regions() {
        // view-vertical.md, "The level's limits": the aim is kept between
        // the camera rule's top and bottom, and its rate applies.
        let physics = physics();
        let level = ground(
            "[[regions]]\nrect = [900, 0, 200, 2000]\n\
             camera = { limits = [900, 100, 200, 300], quick = true }",
        );
        let settled =
            |feet, level: &Level| View::settled(&at_feet(feet, 0, true), level, &physics).top;
        assert_eq!(settled(400, &level), fx(176.5));
        assert_eq!(settled(150, &level), fx(100.5));
        assert_eq!(settled(150, &ground("")), fx(0.5), "kept inside the level");
        // From 240.5, the aim is 176.5: 64 px up, at the quick rates.
        let mut view = View::settled(&at_feet(400, 0, true), &ground(""), &physics);
        view.follow(
            &at_feet(368, 0, true),
            &level,
            &physics,
            false,
            Begin::default(),
        );
        assert_eq!(view.gap.to_bits() >> (FRAC_BITS - 8), -64 * 256 / 8);
    }

    #[test]
    fn ends_and_stops() {
        // level-ends.md, examples, with A = 64 and B = 4800.
        let src = "format = 1\nsize = [6000, 2000]\nstart = [0, 0]\n\
                   camera = { limits = [64, 0, 4992, 2000] }";
        let level = Level::from_toml_str(src, "t").unwrap();
        let mut physics = physics();
        assert_eq!(ends(&level, &physics), Some((64, 4800)));
        let stops_at = |x: f64, vx: f64, physics: &Physics| stops(fx(x), fx(vx), &level, physics);
        assert!(!stops_at(5037.75, 2.0, &physics));
        assert!(stops_at(5038.0, 2.0, &physics));
        assert!(!stops_at(5038.5, -2.0, &physics));
        assert!(!stops_at(82.0, -2.0, &physics));
        assert!(stops_at(81.25, -2.0, &physics));
        assert!(!stops_at(81.25, 2.0, &physics));
        assert!(!stops_at(5038.5, 0.0, &physics));
        // Without `end_stop`, neither.
        physics.set(Param::EndStop, Fixed::ZERO);
        assert_eq!(ends(&level, &physics), None);
        assert!(!stops_at(5038.0, 2.0, &physics));
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

    #[test]
    fn left_of_32_the_window_is_the_one_for_32() {
        // spec/formats/placements.md, "When placed objects appear".
        let physics = physics();
        for left in [0, 11, 31, 32] {
            assert!(near(1, left, &physics) && near(320, left, &physics));
            assert!(!near(0, left, &physics) && !near(321, left, &physics));
        }
        assert!(near(321, 33, &physics) && !near(1, 33, &physics));
    }
}
