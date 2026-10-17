//! Leaving an area through an exit, as `spec/behavior/exits.md`
//! describes.
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
//! standing or turning facing right. At the end of T the lead's `y` is
//! the exit's plus 1.
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
//! moving but don't touch the characters, nothing is collected, an
//! object in the hands stays there, and the other character goes on
//! following. Where play goes after the exit is up to whatever runs the
//! world.

use clamber_fixed::Fixed;
use clamber_input::Frame;
use clamber_level::Vec2;
use clamber_pack::{Param, Physics};

use crate::carry;
use crate::collect;
use crate::columns;
use crate::flow::{FlowEvent, Outcome};
use crate::follow::{self, Lead};
use crate::loss;
use crate::player::{Crouch, Facing, Player, State, frames};
use crate::things;
use crate::view;
use crate::world::{Event, World};

/// The kind of entity that takes the lead out of the area.
pub const EXIT: &str = "exit";

/// The walk-out after an exit took over the lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Leaving {
    /// The exit's entity index.
    pub exit: usize,
    /// Steps of the walk-out taken so far: `k` on T + `k`.
    pub frame: u32,
    /// Frames since F, 0 on it, once the lead has passed it.
    pub fade: Option<u8>,
}

impl World {
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
        && player.vel.x <= Fixed::ZERO
        && !world.events.contains(&Event::Hurt);
    if !can_leave {
        return;
    }
    let Some(reach) = reach(player, &physics) else {
        return;
    };
    let slope = physics[Param::FaceSlope];
    let column = columns::column(player.pos.x);
    if slope > Fixed::ZERO && columns::face(&world.level, column, player.pos.y, slope).is_some() {
        return;
    }
    let x = player.pos.x.floor();
    let nearest = -physics[Param::ExitLeaveAt].floor() + 1;
    let (above, below) = (physics[Param::ExitAbove], physics[Param::ExitBelow]);
    let exit = world.level.entities.iter().position(|e| {
        let ahead = x - e.at.x.floor();
        let down = player.pos.y - e.at.y;
        e.kind == EXIT && (nearest..=reach).contains(&ahead) && -above <= down && down <= below
    });
    let Some(exit) = exit else {
        return;
    };
    let ye = world.level.entities[exit].at.y.floor() + 1;
    world.player.pos.y = Fixed::from_int(i32::try_from(ye).unwrap_or(i32::MAX));
    world.leaving = Some(Leaving {
        exit,
        frame: 0,
        fade: None,
    });
}

/// How far right of an exit, `x` rounded down, it takes over `player`
/// in its state, if it does in that state.
fn reach(player: &Player, physics: &Physics) -> Option<i64> {
    if player.ground_action.is_some() || player.roll_stop > 0 {
        return None;
    }
    let rolling = player.state == State::Roll || player.roll.is_some();
    let crouching = player.crouch.held > 0;
    let upright = matches!(player.state, State::Idle | State::Walk | State::Run);
    let param = match player.facing {
        Facing::Left if player.turn.is_some() => return None,
        Facing::Left if rolling => Param::ExitReachRoll,
        Facing::Left if crouching && player.vel.x != Fixed::ZERO => Param::ExitReachCrouch,
        Facing::Left if upright && !crouching => Param::ExitReach,
        Facing::Right if upright && !rolling && !crouching => Param::ExitReachRight,
        _ => return None,
    };
    Some(physics[param].floor())
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
    let previous_feet = world.player.pos.y;
    walk(
        &mut world.player,
        leaving.frame,
        &physics,
        &mut world.events,
    );
    things::update(world, previous_feet, false);
    if follow::due(world) == Some(false) {
        follow::update(world, Lead::of(&world.player));
    }
    if follow::due(world).is_some() {
        follow::note(world);
    }
    let xe = world.level.entities[leaving.exit].at.x.floor();
    let leave_at = physics[Param::ExitLeaveAt].floor();
    leaving.fade = match leaving.fade {
        Some(f) => Some(f.saturating_add(1)),
        None => (world.player.pos.x.floor() <= xe - leave_at).then_some(0),
    };
    if let Some(at) = view_at {
        view::walkers(world, at, false);
    }
    if leaving.fade.is_none() {
        view::update(world);
    }
    carry::after(world, &none);
    collect::update(world, false, start_view);
    if leaving.fade == Some(frames(world.feel.world[Param::ExitFadeTime])) {
        world.flow.outcome = Outcome::Exited(leaving.exit);
        world.flow.events.push(FlowEvent::Exited(leaving.exit));
    }
    world.leaving = Some(leaving);
}

/// Moves the lead on step `frame` of the walk-out.
fn walk(player: &mut Player, frame: u32, physics: &Physics, events: &mut Vec<Event>) {
    let speed = -physics[Param::ExitWalkSpeed];
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
    // A lead facing right turns to face left as on the ground.
    let turn_time = frames(physics[Param::TurnTime]);
    player.turn = player.turn.map(|n| n + 1).filter(|&n| n < turn_time);
    if frame == 2 && player.facing == Facing::Right {
        player.turn = Some(0);
    }
    if player.facing == Facing::Right && player.turn == Some(turn_time / 2) {
        player.facing = Facing::Left;
        events.push(Event::Turn);
    }
}
