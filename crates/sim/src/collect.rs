//! Collectibles: things with the `collect` behavior.
//!
//! At the end of a step, after everything has moved, the player takes
//! every collectible it's within [`Reach`] of, by what it's doing then.
//! Only the character under control takes them, and not while it's
//! knocked out or losing a life. A taken collectible is gone for the
//! rest of the visit to the level, losing a life included.
//!
//! What it's worth reaches the collectible count one at a time: it
//! releases one on the step it's taken and then one every
//! `release_every` steps, and each arrives `count_delay` steps after its
//! release. Arrivals keep coming while the player is out of play. With
//! `count_life` above 0, the count starts again from 0 when it reaches
//! that, and the player gets one more life.
//!
//! With a view, a collectible with a `window` is only taken while its
//! `x` less the view's left edge, both whole px, is inside it, by where
//! the view is after it has moved on the step. When each released part
//! reaches the count, or whether it ever does, is then up to the
//! collectible's `arrive` rules, by where it is in the view on its
//! release step (see [`Arrive`]).

use clamber_fixed::Fixed;
use clamber_pack::Param;

use crate::archetype::{Arrive, Behavior, Reach, Span};
use crate::flow::{Flow, FlowEvent, Outcome};
use crate::player::{Facing, Player, State, frames};
use crate::view::{self, View};
use crate::world::{Event, Phase, World};

/// A taken collectible still releasing what it's worth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Release {
    /// Still to release.
    pub left: u8,
    /// Steps until the next release.
    pub wait: u8,
    pub every: u8,
    /// Index into the world's things.
    pub thing: usize,
}

/// Takes what the player reaches, if it can take anything on this step,
/// then releases and counts what's due. `start` is the view as it was at
/// the start of the step.
pub(crate) fn update(world: &mut World, can_take: bool, start: Option<View>) {
    note_landing(&mut world.player, &world.events);
    if can_take && world.flow.outcome == Outcome::Playing && world.player.knockout.is_none() {
        take(world);
    }
    let life = whole(world.feel.world[Param::CountLife]);
    let frame = world.frame;
    let vertical = view::vertical(&world.player_physics());
    let mut released = Vec::new();
    for r in &mut world.flow.releases {
        r.wait = r.wait.saturating_sub(1);
        while r.wait == 0 && r.left > 0 {
            r.left -= 1;
            r.wait = r.every;
            released.push(r.thing);
        }
    }
    world.flow.releases.retain(|r| r.left > 0);
    for i in released {
        let thing = &world.things[i];
        let Some(Behavior::Collect {
            count_delay,
            arrive,
            ..
        }) = thing.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        let view = if arrive.at_start { start } else { world.view };
        let delay = match view {
            Some(v) => {
                let y = vertical.then(|| thing.pos.y.floor() - v.y());
                arrival(&arrive, thing.pos.x.floor() - v.x(), y)
            }
            None => Some(count_delay),
        };
        if let Some(delay) = delay {
            world.flow.due.push(frame + u32::from(delay));
        }
    }
    let flow = &mut world.flow;
    let arrived = flow.due.iter().filter(|&&f| f == frame).count();
    flow.due.retain(|&f| f != frame);
    for _ in 0..arrived {
        count(flow, life);
    }
}

/// Adds one to the collectible count, starting it again from 0 with one
/// more life when it reaches `life` (above 0).
pub(crate) fn count(flow: &mut Flow, life: u32) {
    flow.gems += 1;
    if life > 0 && flow.gems >= life {
        flow.gems = 0;
        flow.lives = flow.lives.saturating_add(1);
        flow.events.push(FlowEvent::Life);
    }
}

/// Keeps count of the steps since the player last landed, while it stays
/// on the ground.
fn note_landing(player: &mut Player, events: &[Event]) {
    player.since_land = if events.contains(&Event::Land) {
        Some(0)
    } else if player.grounded() {
        player.since_land.map(|n| n.saturating_add(1))
    } else {
        None
    };
}

/// The delay `arrive` gives a collectible `x` px right of the view's
/// left edge and, with vertical view motion, `y` px below its top.
fn arrival(arrive: &Arrive, x: i64, y: Option<i64>) -> Option<u8> {
    let inside = |(from, to): (i32, i32), v: i64| (i64::from(from)..=i64::from(to)).contains(&v);
    arrive.rules[..usize::from(arrive.rule_count)]
        .iter()
        .find(|r| inside(r.x, x) && y.is_none_or(|y| inside(r.y, y)))
        .map_or(arrive.otherwise, |r| r.delay)
}

fn take(world: &mut World) {
    let view_left = world.view.map(|v| v.x());
    let player = &world.player;
    let crouch_start = frames(world.player_physics()[Param::CrouchStart]);
    let x = player.pos.x.floor();
    let feet = -(-player.pos.y).floor();
    for (i, thing) in world
        .things
        .iter_mut()
        .enumerate()
        .filter(|(_, t)| t.active)
    {
        let Some(Behavior::Collect {
            worth,
            release_every,
            reach,
            window,
            ..
        }) = thing.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        if thing.phase != Phase::Ready {
            continue;
        }
        if let (Some(left), Some((from, to))) = (view_left, window) {
            let x = thing.pos.x.floor() - left;
            if !(i64::from(from)..=i64::from(to)).contains(&x) {
                continue;
            }
        }
        let reach = reach[player.character.min(1)];
        let past = match player.facing {
            Facing::Right => x - thing.pos.x.floor(),
            Facing::Left => thing.pos.x.floor() - x,
        };
        let below = feet - thing.pos.y.floor();
        let (Ok(past), Ok(below)) = (i32::try_from(past), i32::try_from(below)) else {
            continue;
        };
        if !span(&reach, player, crouch_start).contains(past, below) {
            continue;
        }
        thing.active = false;
        world.flow.events.push(FlowEvent::Collect(thing.index));
        world.flow.releases.push(Release {
            left: worth,
            wait: 0,
            every: release_every,
            thing: i,
        });
    }
}

/// The offsets `player` takes a collectible from, by what it's doing at
/// the end of a step. Down held on the ground for `crouch_start` frames
/// (above 0) is crouching.
fn span(reach: &Reach, player: &Player, crouch_start: u8) -> Span {
    if let Some(age) = player.jump_age {
        if age < reach.jump_early {
            return reach.jump;
        }
        return air(reach, player.vel.y);
    }
    if !player.grounded() {
        return air(reach, player.vel.y);
    }
    if player.since_land.is_some_and(|n| n < reach.land_hold) {
        return reach.air;
    }
    if player.state == State::Roll || player.roll.is_some() {
        return reach.roll;
    }
    if crouch_start > 0 && player.crouch.held >= crouch_start {
        return reach.crouch;
    }
    match player.state {
        State::Idle => reach.stand,
        _ => reach.walk,
    }
}

/// `air`, with its lowest `below` lifted for the vertical speed `vy`.
fn air(reach: &Reach, vy: Fixed) -> Span {
    let mut span = reach.air;
    let steps = &reach.air_below[..usize::from(reach.air_steps)];
    if let Some(&(_, below)) = steps.iter().find(|&&(at, _)| vy <= at) {
        span.below.0 = below;
    }
    span
}

/// A whole number from a parameter, 0 if it's below 1.
fn whole(v: Fixed) -> u32 {
    u32::try_from(v.floor()).unwrap_or(0)
}
