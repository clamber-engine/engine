//! Leaving an area through an exit, as `spec/behavior/exits.md`
//! describes, or through a wall exit, as `spec/behavior/wall-exits.md`
//! does.
//!
//! An [`EXIT`] entity doesn't move and isn't drawn. With
//! `exit_walk_speed` above 0, it takes over the lead at the end of a
//! frame, call it T, on which the lead is on the ground, not moving
//! right, with its `y` from `exit_above` above the exit to `exit_below`
//! below it, and its `x` rounded down no further right of the exit than
//! the reach for its state and facing: `exit_reach` standing, walking or
//! running facing left (the landing frame of a jump or fall is one of
//! those), `exit_reach_roll` rolling facing left, `exit_reach_crouch`
//! crouching and still sliding facing left, and `exit_reach_right`
//! standing or turning facing right; standing still carrying an object,
//! `exit_reach_carry` facing left and `exit_reach_carry_right` facing
//! right. At the end of T the lead's `y` is
//! the exit's plus 1. It takes over only while it's there: placed in a
//! spot, it appears and is removed with the view, rows and all
//! (placements.md, "Exits"; see [`view::walkers`]).
//!
//! From T + 1 nothing held has any effect ([`Leaving`]). The lead stays
//! where it is on T + 1 and walks left at `exit_walk_speed` from T + 2,
//! through whatever is in its way and past the level's left end. A lead
//! facing right turns from T + 2. The view goes on following it until
//! the first frame after T that the lead ends `exit_leave_at` px or more
//! left of the exit, call it F, and holds still from F. The picture
//! fades from full brightness on F a step every `exit_fade_step` frames,
//! and F + `exit_fade_time` is the area's last frame
//! ([`Outcome::Exited`]).
//!
//! Engine choices where the spec says nothing: an exit takes over a lead
//! only from `exit_leave_at` px left of it rightwards, and not a lead in
//! a state the reaches above don't cover (crouching after the slide has
//! stopped, the stop after a roll, turning facing left, the ground
//! action, riding, on a spring), on a steep face, hurt, knocked out,
//! swapping or losing a character. During the walk-out things go on
//! moving but don't touch the characters, nothing is collected, and the
//! other character goes on following. An object still in the lead's
//! hands at the end of T, its throw begun or not, is let go on T + 1 and
//! falls away touching nothing from T + 2, as after the end marker
//! ([`carry`]). Where play goes after the exit is up to whatever runs the
//! world: an exit's `area` param names another area of the pack
//! ([`area`]), and its `arrival` param the [`Arrival`] there, or in this
//! level without an `area`, that play comes into next
//! ([`World::arriving`]). Coming back to the level keeps some of what
//! happened in it ([`World::coming_back_from`]), and play carries some
//! of it from area to area ([`World::carrying`]).
//!
//! A [`WALL_EXIT`] entity doesn't move and isn't drawn either. It's
//! closed until a thrown object that [`Opens`] wall exits hits it, moving
//! right and still there at the end of a frame; it opens on that frame
//! (the spec has it open on that frame or the next, and doesn't cover
//! what decides). Once open, it takes over the lead as an exit does,
//! turned around: the lead moving left is never taken, and the reaches
//! are `wall_exit_reach` standing, walking or running facing right,
//! `wall_exit_reach_roll` rolling facing right, `wall_exit_reach_crouch`
//! crouching facing right, still sliding or not, and
//! `wall_exit_reach_left` standing or turning facing left, how far left
//! of the wall exit the lead's `x` rounded down is. The lead walks out
//! right, and F is the first frame it ends `exit_leave_at` px or more
//! right of the wall exit; the rest is an exit's. Placed in a spot, a
//! closed wall exit appears and is removed with the view by its entry
//! alone, an open one also needs the view's rows to stay, and one
//! removed open comes back closed (placements.md, "Wall exits"; see
//! [`view::walkers`]). The spec covers barrels only: here any thrown
//! object with [`Opens`] boxes opens one, and one outside a spot stays
//! open for the rest of the visit.
//!
//! [`Opens`]: crate::Opens

use clamber_fixed::Fixed;
use clamber_input::Frame;
use clamber_level::{Arrival, Entity, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::Behavior;
use crate::carry;
use crate::collect;
use crate::columns;
use crate::flow::{FlowEvent, Outcome};
use crate::follow::{self, Lead};
use crate::hidden;
use crate::loss;
use crate::player::{Crouch, Facing, Player, State, frames};
use crate::things;
use crate::view::{self, Begin};
use crate::world::{Event, Phase, Thing, World};

/// The kind of entity that takes the lead out of the area.
pub const EXIT: &str = "exit";

/// The other area of the pack that an exit or an end marker leads to,
/// if `entity` names one: its `area` param, the file name of that area's
/// level without `.level`. Its `arrival` param names the arrival there.
pub fn area(entity: &Entity) -> Option<&str> {
    entity.params.get("area")?.as_str()
}

/// The kind of entity that takes the lead out of the area to the right
/// once a thrown object has opened it.
pub const WALL_EXIT: &str = "wall-exit";

/// The walk-out after an exit took over the lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Leaving {
    /// The exit's entity index.
    pub exit: usize,
    /// The way the lead walks out: left for an exit, right for a wall
    /// exit.
    pub way: Facing,
    /// Steps of the walk-out taken so far: `k` on T + `k`.
    pub frame: u32,
    /// Frames since F, 0 on it, once the lead has passed it.
    pub fade: Option<u8>,
}

impl World {
    /// The level as play comes back into it at `arrival` after this
    /// world left it through an exit (exits.md, "Coming back to the
    /// level"): [`World::coming_back_from`] this world itself.
    ///
    /// # Panics
    ///
    /// As [`World::new`].
    pub fn coming_back(&self, arrival: &Arrival) -> World {
        self.coming_back_from(arrival, self)
    }

    /// The level as play comes back into it at `arrival` after this
    /// world left it, play having gone on in other areas and left the
    /// last of them as `latest`. It begins as [`World::arriving`] has it,
    /// [`carrying`](World::carrying) what `latest` carries, and with the
    /// restart points reached as this world had them. Things gone for
    /// good here stay gone ([`gone_for_good`]): collectibles that are
    /// there `once` or `kept` once taken, kegs broken, hidden objects set
    /// off, with the item each released wherever it is, crates holding an
    /// animal once broken, with the animal, and objects holding the
    /// missing character once broken. Everything else is there again, as
    /// at the level's start.
    ///
    /// # Panics
    ///
    /// As [`World::new`].
    pub fn coming_back_from(&self, arrival: &Arrival, latest: &World) -> World {
        let mut world =
            World::arriving(self.level.clone(), self.feel.clone(), arrival).carrying(latest);
        world.flow.checkpoints = self.flow.checkpoints.clone();
        for ((new, old), entity) in world
            .things
            .iter_mut()
            .zip(&self.things)
            .zip(&self.level.entities)
        {
            if gone_for_good(old, entity) {
                new.active = false;
                new.phase = match old.phase {
                    Phase::Broken => Phase::Broken,
                    _ => Phase::Gone,
                };
            }
        }
        world
    }

    /// This world as play comes into it from `from`, another area or an
    /// earlier visit to this one: the collectible count, the lives, the
    /// letters taken and the relic count are kept as they were on
    /// leaving `from`, and so are the characters.
    ///
    /// Engine choices where the spec says nothing: the relic count is
    /// kept, the lead is the same character, and a character lost
    /// before stays lost. Restart points reached are a level's own, so
    /// they aren't carried.
    pub fn carrying(mut self, from: &World) -> World {
        let flow = &from.flow;
        self.flow.lives = flow.lives;
        self.flow.gems = flow.gems;
        self.flow.shown = flow.shown;
        self.flow.relics = flow.relics;
        self.flow.letters = flow.letters;
        self.player.character = from.player.character;
        match &from.other {
            Some(old) => {
                if let Some(other) = &mut self.other {
                    other.character = old.character;
                }
            }
            None => self.other = None,
        }
        self
    }

    /// The brightness of the fade after leaving through an exit, as in
    /// [`World::brightness`], once it has started.
    pub(crate) fn leaving_brightness(&self) -> Option<(u32, u32)> {
        let fade = u32::from(self.leaving?.fade?);
        let physics = &self.feel.world;
        let step = u32::from(frames(physics[Param::ExitFadeStep]).max(1));
        let full = u32::from(frames(physics[Param::ExitFadeTime])) / step;
        Some((full.saturating_sub(fade.div_ceil(step)), full))
    }
}

/// Starts the walk-out if an exit takes over the lead at the end of this
/// step.
/// Whether `thing`, as play left the level, stays gone when it comes
/// back (exits.md, "Coming back to the level").
///
/// A hidden object is gone once set off, whatever became of its item
/// (`entity` is the level's entry for `thing`), an animal crate once
/// broken, whatever became of the animal, and an object holding the
/// missing character once it broke, in the hands or thrown.
///
/// Engine choices where the spec says nothing: an extra-life item that
/// rose away or was removed with the view without being taken is there
/// again. An animal still ridden isn't counted, as an exit doesn't take
/// over a rider.
fn gone_for_good(thing: &Thing, entity: &Entity) -> bool {
    if thing.phase == Phase::Gone {
        return true;
    }
    if entity.kind == hidden::HIDDEN {
        return thing.kind != hidden::HIDDEN || thing.phase != Phase::Ready;
    }
    match thing.archetype.map(|a| a.behavior) {
        Some(Behavior::Keg(_)) => thing.phase == Phase::Broken,
        Some(Behavior::Hutch(_)) => !matches!(thing.phase, Phase::Ready | Phase::Ridden),
        Some(Behavior::Carry {
            holder: Some(_), ..
        }) => !thing.active && matches!(thing.phase, Phase::Thrown(_) | Phase::Held),
        _ => false,
    }
}

pub(crate) fn take_over(world: &mut World) {
    let physics = world.player_physics();
    let player = &world.player;
    let can_leave = physics[Param::ExitWalkSpeed] > Fixed::ZERO
        && world.flow.outcome == Outcome::Playing
        && world.ending.is_none()
        && world.leaving.is_none()
        && world.swap.is_none()
        && !world.losing()
        && player.knockout.is_none()
        && player.riding.is_none()
        && player.spring.is_none()
        && player.ground.is_some()
        && !world.events.contains(&Event::Hurt);
    if !can_leave {
        return;
    }
    let slope = physics[Param::FaceSlope];
    let column = columns::column(player.pos.x);
    if slope > Fixed::ZERO && columns::face(&world.level, column, player.pos.y, slope).is_some() {
        return;
    }
    // A lead moving right isn't taken by an exit, nor one moving left by
    // a wall exit.
    let reach = |way| {
        let still_or_going = match way {
            Facing::Left => player.vel.x <= Fixed::ZERO,
            Facing::Right => player.vel.x >= Fixed::ZERO,
        };
        still_or_going
            .then(|| reach(player, &physics, way))
            .flatten()
    };
    let (left, right) = (reach(Facing::Left), reach(Facing::Right));
    let x = player.pos.x.floor();
    let nearest = -physics[Param::ExitLeaveAt].floor() + 1;
    let (above, below) = (physics[Param::ExitAbove], physics[Param::ExitBelow]);
    let open = |i: usize| world.things.get(i).is_some_and(|t| t.phase == Phase::Open);
    let there = |i: usize| world.things.get(i).is_some_and(|t| t.active);
    let exit = world.level.entities.iter().enumerate().find_map(|(i, e)| {
        let (way, reach, ahead) = match e.kind.as_str() {
            EXIT if there(i) => (Facing::Left, left?, x - e.at.x.floor()),
            WALL_EXIT if open(i) => (Facing::Right, right?, e.at.x.floor() - x),
            _ => return None,
        };
        let down = player.pos.y - e.at.y;
        ((nearest..=reach).contains(&ahead) && -above <= down && down <= below).then_some((i, way))
    });
    let Some((exit, way)) = exit else {
        return;
    };
    let ye = world.level.entities[exit].at.y.floor() + 1;
    world.player.pos.y = Fixed::from_int(i32::try_from(ye).unwrap_or(i32::MAX));
    world.leaving = Some(Leaving {
        exit,
        way,
        frame: 0,
        fade: None,
    });
}

/// How far from an exit leading `way`, on the side it's entered from,
/// `x` rounded down, it takes over `player` in its state, if it does in
/// that state.
fn reach(player: &Player, physics: &Physics, way: Facing) -> Option<i64> {
    if player.ground_action.is_some() || player.roll_stop > 0 {
        return None;
    }
    let rolling = player.state == State::Roll || player.roll.is_some();
    let crouching = player.crouch.held > 0;
    let upright = matches!(player.state, State::Idle | State::Walk | State::Run);
    let carrying = player.hands.is_some() && player.vel.x == Fixed::ZERO && upright;
    let param = match (way, player.facing) {
        (Facing::Left, Facing::Left) if player.turn.is_some() => return None,
        (Facing::Left, Facing::Left) if rolling => Param::ExitReachRoll,
        (Facing::Left, Facing::Left) if crouching && player.vel.x != Fixed::ZERO => {
            Param::ExitReachCrouch
        }
        (Facing::Left, Facing::Left) if carrying && !crouching => Param::ExitReachCarry,
        (Facing::Left, Facing::Left) if upright && !crouching => Param::ExitReach,
        (Facing::Left, Facing::Right) if carrying && !rolling && !crouching => {
            Param::ExitReachCarryRight
        }
        (Facing::Left, Facing::Right) if upright && !rolling && !crouching => Param::ExitReachRight,
        (Facing::Right, Facing::Right) if player.turn.is_some() => return None,
        (Facing::Right, Facing::Right) if rolling => Param::WallExitReachRoll,
        (Facing::Right, Facing::Right) if crouching => Param::WallExitReachCrouch,
        (Facing::Right, Facing::Right) if upright => Param::WallExitReach,
        (Facing::Right, Facing::Left) if upright && !rolling && !crouching => {
            Param::WallExitReachLeft
        }
        _ => return None,
    };
    Some(physics[param].floor())
}

/// Opens every closed wall exit that a thrown object moving right, still
/// there at the end of this step, hits.
pub(crate) fn open(world: &mut World) {
    for i in 0..world.things.len() {
        let object = &world.things[i];
        let (Phase::Thrown(flight), Some(archetype)) = (object.phase, object.archetype) else {
            continue;
        };
        let Behavior::Carry {
            hit_early_time,
            opens: Some(opens),
            ..
        } = archetype.behavior
        else {
            continue;
        };
        if !object.active || !flight.thrown || !flight.right() {
            continue;
        }
        let hit = if flight.age <= hit_early_time {
            opens.hit_early
        } else {
            opens.hit
        };
        let pos = object.pos;
        for wall in world.things.iter_mut() {
            if wall.active
                && wall.kind == WALL_EXIT
                && wall.phase == Phase::Ready
                && hit.holds(pos, true, wall.pos)
            {
                wall.phase = Phase::Open;
            }
        }
    }
}

/// One step of the walk-out, whatever is held.
pub(crate) fn step(world: &mut World) {
    let Some(mut leaving) = world.leaving else {
        return;
    };
    let none = Frame::default();
    let physics = world.player_physics();
    let start_view = world.view;
    loss::tick(world);
    let view_at = start_view.map(|v| (v.x(), v.y()));
    if let Some(at) = view_at {
        view::walkers(world, at, true);
    }
    if follow::due(world) == Some(true) {
        follow::update(world, Lead::of(&world.player));
    }
    leaving.frame += 1;
    world.leaving = Some(leaving);
    let previous = world.player.pos;
    walk(
        &mut world.player,
        leaving.way,
        leaving.frame,
        &physics,
        &mut world.events,
    );
    things::update(world, previous, false);
    if follow::due(world) == Some(false) {
        follow::update(world, Lead::of(&world.player));
    }
    if follow::due(world).is_some() {
        follow::note(world);
    }
    let xe = world.level.entities[leaving.exit].at.x.floor();
    let leave_at = physics[Param::ExitLeaveAt].floor();
    let x = world.player.pos.x.floor();
    let past = match leaving.way {
        Facing::Left => x <= xe - leave_at,
        Facing::Right => x >= xe + leave_at,
    };
    leaving.fade = match leaving.fade {
        Some(f) => Some(f.saturating_add(1)),
        None => past.then_some(0),
    };
    if let Some(at) = view_at {
        view::walkers(world, at, false);
    }
    if leaving.fade.is_none() {
        view::update(world, Begin::default());
    }
    carry::after(world, &none);
    collect::update(world, false, start_view);
    if leaving.fade == Some(frames(world.feel.world[Param::ExitFadeTime])) {
        world.flow.outcome = Outcome::Exited(leaving.exit);
        world.flow.events.push(FlowEvent::Exited(leaving.exit));
    }
    world.leaving = Some(leaving);
}

/// Moves the lead on step `frame` of the walk-out `way`.
fn walk(player: &mut Player, way: Facing, frame: u32, physics: &Physics, events: &mut Vec<Event>) {
    let speed = match way {
        Facing::Left => -physics[Param::ExitWalkSpeed],
        Facing::Right => physics[Param::ExitWalkSpeed],
    };
    player.vel = Vec2::new(speed, Fixed::ZERO);
    player.target = speed;
    player.state = State::Walk;
    player.touched.clear();
    player.frozen = 0;
    player.crouch = Crouch::default();
    player.roll = None;
    player.roll_stop = 0;
    player.stopped = None;
    player.ground_action = None;
    player.boost = None;
    player.roll_press = None;
    player.hurt_ahead = None;
    if frame == 1 {
        // The lead stays where it is, and stops any turn under way.
        player.turn = None;
        return;
    }
    player.pos.x += speed;
    // A lead facing away turns to face `way` as on the ground.
    let turn_time = frames(physics[Param::TurnTime]);
    player.turn = player.turn.map(|n| n + 1).filter(|&n| n < turn_time);
    if frame == 2 && player.facing != way {
        player.turn = Some(0);
    }
    if player.facing != way && player.turn == Some(turn_time / 2) {
        player.facing = way;
        events.push(Event::Turn);
    }
}
