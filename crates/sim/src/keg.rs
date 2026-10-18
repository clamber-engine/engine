//! Restart barrels (spec/behavior/restart-barrel.md): things whose
//! archetype has a [`Keg`] behavior.
//!
//! A keg never moves. The player touches it when its offsets from it at
//! the end of a step, after its move, are within the keg's contact
//! ranges for its pose and facing then ([`touch`]). It breaks: it's gone
//! for the rest of the visit, a [`Burst`] of pieces and a flash is left
//! in its place, and the level's restart point is reached: the keg's
//! position moved by its `restart`, where a checkpoint's is its position
//! (`Flow::checkpoints`, [`World::restart_point`]).
//!
//! After a lost life with a keg's restart point reached, play resumes
//! there inside one ([`Resume`]): the step the player comes back on is
//! frame 0, with the player at the restart point and the view where it
//! starts when an area begins (or the keg's `view_back` left of it).
//! With `view_begin_back` above 0 the view follows as usual from frame
//! 1, also while the player is in the keg. From frame 1 the player is `lift`
//! higher, inside the keg, and not drawn. The keg shakes, and bursts on
//! frame `burst` as when broken; from the frame after, the player is in
//! the air and in play, falling as any fall does, except that a jump can
//! start after landing `land_lockout` frames sooner. Its gait goes on in
//! the fall as off a ledge where the ground simply ends, so a turn there
//! takes as long as on the ground. The picture is black
//! until `fade_from` and fades in from there ([`World::brightness`]).
//!
//! Engine choices where the spec says nothing:
//!
//! - Only the character under control touches a keg, whatever it's
//!   doing; nothing else does.
//! - While the player is in the keg, everything else waits too, as while
//!   a character joins, and the other character isn't drawn either. It
//!   comes back at the restart point as after any lost life
//!   (following.md), and follows from there.
//! - Input on frame 1 does nothing, like on the frames after it.
//! - The pieces and the flash start at the keg's position, move from the
//!   step after it breaks, and are gone after `burst_time` frames. They
//!   touch nothing.

use clamber_fixed::Fixed;
use clamber_level::Vec2;

use crate::archetype::{Behavior, KEG_PIECES, Keg};
use crate::flow::{FlowEvent, Outcome};
use crate::player::{State, ceil};
use crate::pose;
use crate::view;
use crate::world::{Event, Phase, World};

/// The pieces and the flash left where a keg broke or burst, or where
/// an entrance burst.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Burst {
    /// The thing that burst.
    pub thing: usize,
    /// Frames since it broke, from 0 on the frame it did.
    pub frame: u8,
    pub pieces: [Vec2; KEG_PIECES],
    /// Each piece's speed, px per frame, +y down.
    pub speeds: [Vec2; KEG_PIECES],
    pub flash: Vec2,
    /// What the pieces' speed down grows by each frame.
    pub gravity: Fixed,
    /// How far the flash rises each frame.
    pub rise: Fixed,
    /// How many frames it lasts.
    pub time: u8,
}

/// Play resuming at a keg's restart point, from the step the player comes
/// back on until it's in play and the picture has faded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Resume {
    /// The keg's thing.
    pub keg: usize,
    /// Frames since the player came back, from 0 on the step it did.
    pub frame: u16,
    /// The restart point.
    pub at: Vec2,
    /// The player has landed since the burst.
    pub landed: bool,
}

/// The keg values of thing `index`, if it's a keg.
fn keg(world: &World, index: usize) -> Option<Keg> {
    match world.things.get(index)?.archetype?.behavior {
        Behavior::Keg(keg) => Some(keg),
        _ => None,
    }
}

impl Burst {
    fn new(keg: usize, at: Vec2, values: &Keg) -> Burst {
        Burst::of(
            keg,
            at,
            values.pieces,
            (values.piece_gravity, values.flash_rise, values.burst_time),
        )
    }

    /// A burst of `thing` at `at` into `pieces`, each `[across, up]`,
    /// that moves by `(gravity, rise, time)`.
    pub(crate) fn of(
        thing: usize,
        at: Vec2,
        pieces: [Vec2; KEG_PIECES],
        (gravity, rise, time): (Fixed, Fixed, u8),
    ) -> Burst {
        Burst {
            thing,
            frame: 0,
            pieces: [at; KEG_PIECES],
            speeds: pieces.map(|p| Vec2::new(p.x, -p.y)),
            flash: at,
            gravity,
            rise,
            time,
        }
    }
}

impl World {
    /// Where play comes back after a lost life with the checkpoint or keg
    /// of thing `index` reached.
    pub fn restart_point(&self, index: usize) -> Vec2 {
        let at = self.level.entities[index].at;
        match keg(self, index) {
            Some(k) => Vec2::new(at.x + k.restart.x, at.y + k.restart.y),
            None => at,
        }
    }

    /// Whether the player is inside a keg, waiting for it to burst.
    pub fn in_keg(&self) -> bool {
        self.resume.is_some_and(|r| {
            keg(self, r.keg).is_some_and(|k| (1..=u16::from(k.burst)).contains(&r.frame))
        })
    }

    /// Where the keg the player waits in is drawn, shaking, until it
    /// bursts.
    pub fn resume_keg(&self) -> Option<Vec2> {
        let r = self.resume?;
        let k = keg(self, r.keg)?;
        if r.frame == 0 || r.frame >= u16::from(k.burst) {
            return None;
        }
        Some(Vec2::new(
            r.at.x + Fixed::from_int(shake(&k, r.frame)),
            r.at.y,
        ))
    }

    /// The step of brightness while the picture fades in after resuming
    /// at a keg, out of `full`, or `None` outside the fade.
    pub(crate) fn resume_brightness(&self, full: u32) -> Option<u32> {
        let r = self.resume?;
        let k = keg(self, r.keg)?;
        let from = u16::from(k.fade_from);
        if r.frame < from {
            return Some(0);
        }
        let step = u32::from((r.frame - from) / u16::from(k.fade_step)) + 1;
        (step < full).then_some(step)
    }
}

/// How far the keg the player waits in is moved sideways on `frame`.
fn shake(keg: &Keg, frame: u16) -> i32 {
    let stages = &keg.shake[..usize::from(keg.shake_stages)];
    let Some(&(from, first, second)) = stages.iter().rev().find(|s| u16::from(s.0) <= frame) else {
        return 0;
    };
    i32::from(if (frame - u16::from(from)) % 4 < 2 {
        first
    } else {
        second
    })
}

/// Breaks the first keg the player touches at the end of this step, if
/// it touches one: it's gone, it leaves a burst, and its restart point
/// is reached.
pub(crate) fn touch(world: &mut World) {
    let player = &world.player;
    let x = player.pos.x.floor();
    let feet = ceil(player.pos.y);
    let touched = world.things.iter().find_map(|t| {
        let Some(Behavior::Keg(keg)) = t.archetype.map(|a| a.behavior) else {
            return None;
        };
        if !t.active || t.phase != Phase::Ready {
            return None;
        }
        let contact = keg.contact;
        let reach = contact.reach(pose::pose(player, &contact.poses), player.facing, 0);
        reach
            .contains(x - t.pos.x.floor(), feet - ceil(t.pos.y))
            .then_some((t.index, t.pos, keg))
    });
    let Some((index, at, keg)) = touched else {
        return;
    };
    let thing = &mut world.things[index];
    thing.active = false;
    thing.phase = Phase::Broken;
    world.bursts.push(Burst::new(index, at, &keg));
    let flow = &mut world.flow;
    if !flow.checkpoints.contains(&index) {
        flow.checkpoints.push(index);
        flow.events.push(FlowEvent::Checkpoint(index));
    }
}

/// Moves the bursts on a step, and forgets the ones whose time is up.
pub(crate) fn bursts(world: &mut World) {
    world.bursts.retain_mut(|b| {
        b.frame = b.frame.saturating_add(1);
        for (piece, speed) in b.pieces.iter_mut().zip(&mut b.speeds) {
            piece.x += speed.x;
            piece.y += speed.y;
            speed.y += b.gravity;
        }
        b.flash.y -= b.rise;
        b.frame < b.time
    });
}

/// On the step the player comes back after a lost life: if the restart
/// point reached is a keg's, play resumes inside one, with the view
/// where it starts when an area begins ([`View::begun`]), or without
/// `view_begin_back` the keg's `view_back` left of it.
///
/// [`View::begun`]: crate::View::begun
pub(crate) fn begin(world: &mut World) {
    let Some(&index) = world.flow.checkpoints.last() else {
        return;
    };
    let Some(keg) = keg(world, index) else {
        return;
    };
    let at = world.restart_point(index);
    world.resume = Some(Resume {
        keg: index,
        frame: 0,
        at,
        landed: false,
    });
    let physics = world.player_physics();
    if let Some(view) = &mut world.view {
        view.left = view::begin_left(at.x, &world.level, &physics).unwrap_or(at.x - keg.view_back);
    }
}

/// Counts a frame of a resume, if one is under way. Returns whether the
/// player is held in the keg on this step, so nothing else moves.
pub(crate) fn hold(world: &mut World) -> bool {
    let Some(mut r) = world.resume else {
        return false;
    };
    let Some(keg) = keg(world, r.keg) else {
        world.resume = None;
        return false;
    };
    r.frame = r.frame.saturating_add(1);
    let burst = u16::from(keg.burst);
    world.resume = Some(r);
    if world.flow.outcome != Outcome::Playing || r.frame > burst {
        return false;
    }
    let player = &mut world.player;
    if r.frame == 1 {
        player.pos = Vec2::new(r.at.x, r.at.y - keg.lift);
        player.ground = None;
        player.spring = None;
        player.spring_arc = None;
        player.rope = None;
        player.sling = None;
        player.rope_fall = false;
        player.state = State::Fall;
        player.keg_fall = true;
    }
    player.vel = Vec2::default();
    if r.frame == burst {
        world.bursts.push(Burst::new(r.keg, r.at, &keg));
    }
    true
}

/// After a step in play: the first landing since the keg burst has the
/// keg's lockout, and the resume is over once the player has landed and
/// the picture has faded in.
pub(crate) fn after(world: &mut World) {
    let Some(mut r) = world.resume else {
        return;
    };
    let Some(keg) = keg(world, r.keg) else {
        world.resume = None;
        return;
    };
    if r.frame <= u16::from(keg.burst) {
        return;
    }
    if !r.landed && world.events.contains(&Event::Land) {
        world.player.lockout = keg.land_lockout;
        r.landed = true;
    }
    world.resume = Some(r);
    let (_, full) = world.brightness();
    if r.landed && world.resume_brightness(full).is_none() {
        world.resume = None;
    }
}
