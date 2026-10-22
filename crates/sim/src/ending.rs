//! Finishing a level by walking past its end marker, as
//! `spec/behavior/level-end.md` describes.
//!
//! An [`END`] entity marks where a level ends. It doesn't move and isn't
//! drawn. The lead passes it at the end of a step on which Right was
//! held, standing on the ground at or right of the marker (`x` rounded
//! down) with its feet line no more than `end_reach` px above or below
//! it.
//!
//! With `end_walk_speed` above 0 the walk-out follows ([`Ending`]):
//! nothing held has any effect, and the lead walks right at that speed
//! from the step after the first, rising `end_rise` px a step through
//! whatever is in its way, turning to face right if it doesn't. The view
//! holds still and the characters are drawn behind the raised scenery. Once the
//! lead ends a step `end_fade_at` px or more right of the view's left
//! edge (at once, without a view), the picture fades to black over
//! `end_fade_time` frames, a step of brightness every `end_fade_step`,
//! and the lead stops rising. The level is complete on the fade's last
//! frame. With `end_walk_speed` 0, passing the marker completes the
//! level at once.
//!
//! A marker that names another area of the pack ([`area`]), as the first
//! level's room's does, leads there instead of completing the level
//! ([`Outcome::Exited`]), with the timing of "In the first level's room":
//! the fade waits for a frame from `end_onward_fade_from` after the
//! walk-out's first on, and the area's last frame is `end_onward_hold`
//! after the fade's black one.
//!
//! As "Hits and bounces" gives, the marker isn't passed on the frame the
//! lead is hurt or while it's knocked out, and after losing a character
//! the other passes it only from its hop's landing; a bounce is a time in
//! the air, so it passes at the earliest on landing.
//!
//! Engine choices where the spec says nothing: the marker can't be
//! passed while swapping; during the walk-out things go on moving but don't touch
//! the characters, and without a `let_go_speed` an object in the hands
//! stays there; the other character goes on following. With one, it's
//! let go on the walk-out's first step ([`carry`]).
//!
//! During the walk-out walkers go on moving as usual but don't hurt the
//! lead, and collectibles are still taken, with their walking ranges
//! whatever the step, as "Walkers and bunches" describes for bunches.
//! The spec covers only bunches; here every collectible is taken so.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::Vec2;
use clamber_pack::{Param, Physics};

use crate::carry;
use crate::collect;
use crate::exit::area;
use crate::flow::{FlowEvent, Outcome};
use crate::follow::{self, Lead};
use crate::loss;
use crate::player::{Crouch, Facing, Player, State, ceil, frames};
use crate::things;
use crate::view;
use crate::world::{Event, World};

/// The kind of entity that marks the end of a level.
pub const END: &str = "end";

/// The walk-out after the lead passed the end marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Ending {
    /// The marker's entity index.
    pub marker: usize,
    /// The marker leads to another area ([`area`]) rather than finishing
    /// the level.
    pub onward: bool,
    /// Steps of the walk-out taken so far.
    pub frame: u32,
    /// Frames into the fade, its first being 0, once it has started.
    pub fade: Option<u8>,
    /// The fade starts on the next step.
    pub due: bool,
}

impl World {
    /// Whether the characters are drawn behind the level's raised
    /// scenery, as from the first frame of the level's walk-out and from
    /// the second of an exit's (`spec/formats/graphics.md`, "Moving things
    /// and raised tiles").
    pub fn behind_scenery(&self) -> bool {
        self.ending.is_some() || self.leaving.is_some_and(|l| l.frame >= 1)
    }

    /// How bright the picture is, as a step of brightness out of the
    /// number at full brightness: `(full, full)` outside the fade, and
    /// `(0, full)` on its last frame. After resuming in a keg, it fades
    /// in the other way (see [`keg`](crate::keg)), and so it does at the
    /// start of a level with an entrance. After an exit took over the
    /// lead, it fades by the exit's values, and in a scene by the
    /// scene's.
    pub fn brightness(&self) -> (u32, u32) {
        let physics = &self.feel.world;
        let step = u32::from(frames(physics[Param::EndFadeStep]).max(1));
        let full = u32::from(frames(physics[Param::EndFadeTime])) / step + 1;
        if let Some(now) = self.resume_brightness(full) {
            return (now, full);
        }
        if let Some(fade) = self.entering_brightness() {
            return fade;
        }
        if let Some(fade) = self.walking_in_brightness() {
            return fade;
        }
        if let Some(fade) = self.dropping_in_brightness() {
            return fade;
        }
        if let Some(fade) = self.leaving_brightness() {
            return fade;
        }
        if let Some(fade) = self.scene_brightness() {
            return fade;
        }
        match self.ending.and_then(|e| e.fade) {
            Some(f) => (full - 1 - (u32::from(f) / step).min(full - 1), full),
            None => (full, full),
        }
    }
}

/// Starts the walk-out, or completes the level, if the lead passed an end
/// marker at the end of this step, holding `input`.
pub(crate) fn pass(world: &mut World, input: &Frame) {
    let player = &world.player;
    let can_pass = world.flow.outcome == Outcome::Playing
        && world.ending.is_none()
        && world.swap.is_none()
        && !world.losing()
        && player.knockout.is_none()
        && !world.events.contains(&Event::Hurt);
    if !can_pass || !input.held.contains(Action::Right) || !player.grounded() {
        return;
    }
    let physics = world.player_physics();
    let reach = physics[Param::EndReach].floor();
    let (x, feet) = (player.pos.x.floor(), ceil(player.pos.y));
    let passed = world.level.entities.iter().position(|e| {
        e.kind == END && x >= e.at.x.floor() && (feet - e.at.y.floor()).abs() <= reach
    });
    let Some(marker) = passed else {
        return;
    };
    let onward = area(&world.level.entities[marker]).is_some();
    if physics[Param::EndWalkSpeed] > Fixed::ZERO {
        world.ending = Some(Ending {
            marker,
            onward,
            ..Ending::default()
        });
    } else {
        finish(world, marker, onward);
    }
}

/// Ends the level, or leaves the area if the marker passed leads on.
fn finish(world: &mut World, marker: usize, onward: bool) {
    let (outcome, event) = match onward {
        true => (Outcome::Exited(marker), FlowEvent::Exited(marker)),
        false => (Outcome::Complete, FlowEvent::Complete),
    };
    world.flow.outcome = outcome;
    world.flow.events.push(event);
}

/// One step of the walk-out, whatever is held.
pub(crate) fn step(world: &mut World) {
    let Some(mut ending) = world.ending else {
        return;
    };
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
    ending.frame += 1;
    ending.fade = match ending.fade {
        Some(f) => Some(f.saturating_add(1)),
        None => ending.due.then_some(0),
    };
    // What moves during the step sees which step of the walk-out it is.
    world.ending = Some(ending);
    let previous = world.player.pos;
    walk(&mut world.player, &ending, &physics, &mut world.events);
    things::update(world, previous, false);
    if follow::due(world) == Some(false) {
        follow::update(world, Lead::of(&world.player));
    }
    if follow::due(world).is_some() {
        follow::note(world);
    }
    if let Some(at) = view_at {
        view::walkers(world, at, false);
    }
    carry::after(world);
    collect::update(world, true, start_view);

    let x = world.player.pos.x.floor();
    let fade_at = physics[Param::EndFadeAt].floor();
    let world_physics = &world.feel.world;
    let (counts_from, hold) = match ending.onward {
        true => (
            frames(world_physics[Param::EndOnwardFadeFrom]),
            frames(world_physics[Param::EndOnwardHold]),
        ),
        false => (0, 0),
    };
    // The walk-out's first frame is T, step 1.
    let counts = ending.frame > u32::from(counts_from);
    if ending.fade.is_none() && counts && view_at.is_none_or(|(left, _)| x - left >= fade_at) {
        ending.due = true;
    }
    let last = frames(world_physics[Param::EndFadeTime]).saturating_add(hold);
    if ending.fade == Some(last) {
        finish(world, ending.marker, ending.onward);
    }
    world.ending = Some(ending);
}

/// Moves the lead on one step of the walk-out.
fn walk(player: &mut Player, ending: &Ending, physics: &Physics, events: &mut Vec<Event>) {
    let speed = physics[Param::EndWalkSpeed];
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
    if ending.frame == 1 {
        // The lead stays where it is, and stops any turn under way.
        player.turn = None;
        return;
    }
    player.pos.x += speed;
    // T + 1 rises off the ground (`level-end.md`, "In a trace").
    if player.ground.is_some() {
        events.push(Event::LeaveGround);
    }
    player.ground = None;
    player.spring = None;
    player.spring_arc = None;
    player.rope = None;
    player.sling = None;
    player.rope_fall = false;
    if ending.fade.is_none() {
        let rise = physics[Param::EndRise];
        player.pos.y -= rise;
        player.vel.y = -rise;
    }
    // A lead facing left turns to face right as on the ground.
    let turn_time = frames(physics[Param::TurnTime]);
    player.turn = player.turn.map(|n| n + 1).filter(|&n| n < turn_time);
    if ending.frame == 2 && player.facing == Facing::Left {
        player.turn = Some(0);
    }
    if player.facing == Facing::Left && player.turn == Some(turn_time / 2) {
        player.facing = Facing::Right;
        events.push(Event::Turn);
    }
}
