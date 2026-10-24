//! Which pose the player has, for whether a ball reaches it
//! (spec/behavior/ball-contact.md, "Which pose the player has").
//!
//! [`PoseClock`] keeps the facts a pose is read from up to date on every
//! step, after the player moves; [`pose`] reads the pose from them with
//! a ball's or a thrower's [`Poses`] timings.
//!
//! Engine choices where the spec says nothing:
//!
//! - A walk-off fall from a ledge with a face (a frame started on a
//!   steep face) is airborne from the frame the player leaves the ground,
//!   not the frames before it. One from a ledge without a face stays
//!   upright through the fall and its landing.
//! - A jump let go of before it lands counts as a hop, and a running
//!   jump as a walking one. Any other time in the air (a bounce, a fall
//!   without a jump) counts as a walk-off fall.
//! - The standing pose starts on the first frame the player is idle on
//!   the ground, outside a turn, a crouch, the ground action and
//!   carrying.
//! - Bouncing is airborne. Carrying, swapping and being hurt take the
//!   pose they would have otherwise.
//!
//! A walker with a [`WalkerContact`] reads the player's [`Stance`]
//! instead (spec/behavior/walker-contact.md, "Poses (`title-2`)"), with
//! these engine choices where the spec gives a range of frames or says
//! nothing:
//!
//! - Slowing down, the player stands once its speed is
//!   `stance_stand_speed` or less, and running with Action let go and a
//!   direction held, it walks once its speed is `stance_run_speed` or
//!   less. The spec saw these switches 18 to 34 and 8 to 21 frames after
//!   the release; the presets' values fall inside those.
//! - Any time in the air is the air stance, falls without a jump and
//!   bounces included, and the landing frames after them are too.
//! - Turning, the ground action and carrying take the stance they would
//!   have otherwise.
//!
//! A timed roll's frames are [`Stance::Rolling`] and the frames of the
//! stop after one [`Stance::Stopped`] (spec/behavior/rolling-into-walkers.md,
//! "In `title-2`"). The frame a roll ends on with a jump, or carrying on
//! into running, already has the stance the player has after it.

use clamber_fixed::Fixed;

use clamber_pack::{Param, Physics};

use crate::archetype::{Poses, SecondBall, WalkerContact};
use crate::player::{Act, Gait, Player, State, frames};
use crate::world::Event;

/// A pose with its own ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pose {
    Upright,
    Airborne,
    Rolling,
    Crouching,
    Fidget,
}

/// The pose a ball reads a character other than the first by
/// (spec/behavior/ball-contact.md, "The second character under
/// control"), for its three ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SecondPose {
    Standing,
    Moving,
    Crouching,
}

/// The stance a walker with a [`WalkerContact`] reads, for its ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stance {
    Standing,
    Crouching,
    Walking,
    Running,
    Air,
    /// On a frame of a timed roll.
    Rolling,
    /// In the stop after a timed roll.
    Stopped,
}

/// How the player went into the air, which says how long it still has
/// the airborne pose after landing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Air {
    /// A jump standing still, held until it lands.
    Still,
    /// A jump standing still, let go of before it lands.
    Hop,
    /// A jump while moving.
    Moving,
    /// A jump out of a roll.
    Roll,
    /// In the air without jumping.
    Fall,
}

impl Air {
    /// Its place in [`Poses::land_hold`].
    pub fn index(self) -> usize {
        self as usize
    }
}

/// What the player's pose is read from, as of the last step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PoseClock {
    /// How the player went into the air the last time.
    pub air: Air,
    /// Steps since it landed, from 0 on the landing frame, while it
    /// stays on the ground; `None` in the air.
    pub since_land: Option<u8>,
    /// Steps since a roll ended, from 1 on the first one after it, and
    /// whether it ran on into a run.
    pub rolled: Option<(u8, bool)>,
    /// Steps into the standing pose, from 0 on its first.
    pub standing: Option<u32>,
    /// Rolling on the last step.
    pub rolling: bool,
    /// Action and a direction were held on the landing frame.
    pub landed_running: bool,
    /// Running, for the [`Stance`].
    pub running: bool,
    /// The fall under way, or the last one landed, walked off a ledge
    /// without a face, so it keeps the upright pose.
    pub upright_fall: bool,
    /// Frames Jump was held in the last jump, from the one it started
    /// on, while it went on in the air.
    pub jump_held: u8,
    /// Where the feet were when the player last went into the air.
    pub from: Fixed,
    /// How far below that it landed the last time; negative is above.
    pub drop: Fixed,
    /// The object it carried broke on a walker while it was in the air,
    /// so it keeps the carrying pose until it lands.
    pub carried_air: bool,
    /// Steps since Down was last held on the ground, from 1 on the first
    /// without it; 0 while it is held.
    pub uncrouched: u8,
    /// Standing at the end of a floor, decided once, on the fourth
    /// standing step (`player-frames.md`, "Teetering at the end of a
    /// floor"): the second character's teetering pictures start there.
    pub teeters: bool,
}

impl Default for PoseClock {
    /// On the ground long since.
    fn default() -> PoseClock {
        PoseClock {
            air: Air::Fall,
            since_land: Some(u8::MAX),
            rolled: None,
            standing: None,
            rolling: false,
            landed_running: false,
            running: false,
            upright_fall: false,
            jump_held: 0,
            from: Fixed::ZERO,
            drop: Fixed::ZERO,
            carried_air: false,
            uncrouched: u8::MAX,
            teeters: false,
        }
    }
}

/// Whether `player` is rolling, as a pose: during a roll or a timed one.
fn rolling(player: &Player) -> bool {
    player.state == State::Roll || player.roll.is_some()
}

/// Brings `player`'s clock up to date after its move on a step that
/// had `events`, with `physics`.
pub(crate) fn note(player: &mut Player, events: &[Event], physics: &Physics) {
    let mut clock = player.pose;
    let grounded = player.grounded();
    if events.contains(&Event::JumpStart) {
        clock.air = if clock.rolling || events.contains(&Event::RollEnd) {
            Air::Roll
        } else if player.vel.x != Fixed::ZERO {
            Air::Moving
        } else {
            Air::Still
        };
        clock.since_land = None;
        clock.upright_fall = false;
        clock.jump_held = 0;
        clock.from = player.pos.y;
    } else if !grounded {
        if clock.since_land.is_some() {
            clock.air = Air::Fall;
            clock.since_land = None;
        }
        // A bounce ends the walk-off.
        clock.upright_fall = player.walked_off && !player.walked_off_face && !rolling(player);
    } else {
        if clock.since_land.is_none() {
            clock.landed_running = player.ran;
            clock.drop = player.pos.y - clock.from;
        }
        clock.from = player.pos.y;
        clock.carried_air = false;
        clock.since_land = Some(clock.since_land.map_or(0, |n| n.saturating_add(1)));
    }
    if clock.since_land.is_none() && clock.air != Air::Fall && player.jump_hold {
        clock.jump_held = clock.jump_held.saturating_add(1);
    }
    if clock.air == Air::Still && !player.jump_hold && clock.since_land.is_none() {
        clock.air = Air::Hop;
    }

    let now = rolling(player);
    clock.rolled = if now {
        None
    } else if clock.rolling {
        Some((1, player.state == State::Run))
    } else {
        clock.rolled.map(|(n, on)| (n.saturating_add(1), on))
    };
    clock.rolling = now;

    let standing = grounded
        && player.state == State::Idle
        && player.crouch.held == 0
        && player.ground_action.is_none()
        && player.turn.is_none()
        && player.hands.is_none();
    clock.standing = if standing {
        Some(clock.standing.map_or(0, |n| n.saturating_add(1)))
    } else {
        None
    };

    clock.uncrouched = if player.crouch.held > 0 {
        0
    } else {
        clock.uncrouched.saturating_add(1)
    };

    let speed = player.vel.x.abs();
    let run_speed = physics[Param::StanceRunSpeed];
    // With a direction held, something moves the speed on.
    let walking = player.target != Fixed::ZERO && speed <= run_speed;
    clock.running = grounded
        && speed > physics[Param::StanceStandSpeed]
        && (speed > run_speed || (clock.running && !walking));
    player.pose = clock;
}

/// The stance `player` has at the end of the last step, for a walker
/// with `contact`, or `None` while it rolls, or is in the stop after a
/// roll, and `contact` has no `roll` range.
pub fn stance(player: &Player, contact: &WalkerContact, physics: &Physics) -> Option<Stance> {
    let clock = player.pose;
    if rolling(player) || player.roll_stop > 0 {
        contact.roll?;
        return if player.roll.is_some() {
            Some(Stance::Rolling)
        } else if player.roll_stop > 0 {
            Some(Stance::Stopped)
        } else {
            // An untimed roll.
            None
        };
    }
    // A jump is in the air from the frame it starts on, still on the
    // ground.
    if !player.grounded() || player.takeoff || player.takeoff_wait.is_some() {
        return Some(Stance::Air);
    }
    let (hold, ran_on) = contact.land_hold;
    let hold = if clock.landed_running { ran_on } else { hold };
    if clock.since_land.is_some_and(|n| n <= hold) {
        return Some(Stance::Air);
    }
    if player.crouch.held > 0 || player.crouch.turn_lock > 0 {
        return Some(Stance::Crouching);
    }
    Some(if clock.running {
        Stance::Running
    } else if player.vel.x.abs() > physics[Param::StanceStandSpeed] {
        Stance::Walking
    } else {
        Stance::Standing
    })
}

/// The pose `player` has at the end of the last step, by `poses`'
/// timings.
pub fn pose(player: &Player, poses: &Poses) -> Pose {
    let clock = player.pose;
    if let Some(age) = player.jump_age {
        return if age <= poses.upright_jump {
            Pose::Upright
        } else {
            Pose::Airborne
        };
    }
    // A walk-off without a face stays upright through the fall and its
    // landing.
    let upright = clock.upright_fall && clock.air == Air::Fall;
    let held = clock
        .since_land
        .is_some_and(|n| n <= poses.land_hold[clock.air.index()]);
    if !upright && (!player.grounded() || held) {
        return Pose::Airborne;
    }
    let (on, stopped) = poses.roll_after;
    let rolled = clock
        .rolled
        .is_some_and(|(n, ran_on)| n <= if ran_on { on } else { stopped });
    if clock.rolling || rolled {
        return Pose::Rolling;
    }
    if player.crouch.held > 0 && player.ground_action.is_none() {
        return Pose::Crouching;
    }
    if fidgeting(&clock, poses.fidget_time) {
        return Pose::Fidget;
    }
    Pose::Upright
}

/// The pose `player`, a character other than the first, has at the end
/// of the last step, for a ball with `second`. Engine choices where the
/// spec doesn't say: a jump is in the air from its press frame,
/// and only one standing still (a hop too) holds the range after
/// landing; walking and running on the ground, with a roll's stop, are
/// moving, and a turn is standing; the crouching range is not given to
/// the ground action.
pub fn second_pose(player: &Player, second: &SecondBall) -> SecondPose {
    second_pose_for(player, second.air_hold, second.crouch_hold)
}

/// [`second_pose`] with the two holds given.
pub fn second_pose_for(player: &Player, air_hold: u8, crouch_hold: u8) -> SecondPose {
    let pose = second_pose_held(player, air_hold, crouch_hold);
    // Teetering has the moving range from its first picture, and the
    // landing of a fall from a steep face keeps it until the gait starts.
    let teetering = player.pose.teeters && player.pose.standing.is_some_and(|n| n >= 4);
    let face_landing = player.grounded()
        && player.walked_off_face
        && matches!(player.gait, Gait::Landing(l) if l.fixed && !l.bounce);
    if pose == SecondPose::Standing && (teetering || face_landing) {
        SecondPose::Moving
    } else {
        pose
    }
}

/// [`second_pose`] with its two holds given: the frames the moving pose
/// goes on after a landing from a jump standing still, and the crouching
/// one after Down.
pub fn second_pose_held(player: &Player, air_hold: u8, crouch_hold: u8) -> SecondPose {
    let clock = player.pose;
    let held = matches!(clock.air, Air::Still | Air::Hop)
        && clock.since_land.is_some_and(|n| n <= air_hold);
    if !player.grounded()
        || player.jump_age.is_some()
        || player.takeoff
        || player.takeoff_wait.is_some()
        || player.jump_buffer.age() == Some(0)
        || held
    {
        return SecondPose::Moving;
    }
    // A roll started with Down held keeps the moving range to its end;
    // Down still held then crouches.
    if player.state == State::Roll || clock.rolling {
        return SecondPose::Moving;
    }
    if (player.crouch.held > 0 && player.ground_action.is_none())
        || (1..=crouch_hold).contains(&clock.uncrouched)
    {
        return SecondPose::Crouching;
    }
    if player.turn.is_some() {
        return SecondPose::Standing;
    }
    if matches!(player.state, State::Walk | State::Run | State::Roll) || player.roll_stop > 0 {
        SecondPose::Moving
    } else {
        SecondPose::Standing
    }
}

/// The pose the preset's hurt box reads, by the preset's timings
/// (spec/behavior/walker-contact.md, "Poses"): whether the player has
/// the airborne pose at the end of the last step, and whether it shows
/// the pictures of the rise and fall or the closing ones, which lift the
/// box. A walk-off from a ledge without a face stays upright. Engine
/// choices: a walk-off from a ledge with a face is airborne from the
/// frame it leaves the ground, not the frames before; a fall other than
/// a walk-off (after a bounce) lands as one from a ledge with a face; and
/// Jump counts as held from the frame the jump starts on.
pub fn hurt_pose(player: &Player, physics: &Physics) -> (bool, bool) {
    let clock = player.pose;
    let frames = |p: Param| crate::player::frames(physics[p]);
    if let Some(age) = player.jump_age {
        let airborne = age >= frames(Param::HurtJumpTime);
        return (airborne, airborne);
    }
    if clock.upright_fall && clock.air == Air::Fall {
        return (false, false);
    }
    if !player.grounded() {
        return (true, true);
    }
    let Some(n) = clock.since_land else {
        return (false, false);
    };
    let long = frames(Param::HurtLongJump);
    let (hold, lifted) = if clock.air == Air::Fall {
        (frames(Param::HurtFallHold), frames(Param::HurtFallLift))
    } else if long > 0 && (long..=frames(Param::HurtLongJumpEnd)).contains(&clock.jump_held) {
        (frames(Param::HurtLongLandHold), 0)
    } else {
        (frames(Param::HurtLandHold), frames(Param::HurtLandLift))
    };
    (n <= hold, n < lifted)
}

/// Whether `player` has the crouching pose for the preset's hurt box at
/// the end of the last step (spec/behavior/walker-contact.md,
/// "Crouching"): on the ground, from the first frame Down is held, while
/// it crouches, and while it gets up but for the last frame of getting
/// up (`crouch_recovery`), whose end is U + 8 for title-1's 10 frames.
/// Engine choice: the ground action ends the pose at once, as it does
/// the crouch.
pub fn hurt_crouching(player: &Player) -> bool {
    player.grounded()
        && player.jump_age.is_none()
        && player.ground_action.is_none()
        && (player.crouch.held > 0 || player.crouch.rising > 0)
}

/// Whether `player` has the carrying pose for the preset's hurt box at
/// the end of the last step (spec/behavior/walker-contact.md,
/// "Carrying"): from the first frame of a pick-up while action stays
/// held, on the ground and in the air, on every frame of a throw, on a
/// put-down's frames but its last, and after the object breaks on a
/// walker in the air until the player lands (thrown-objects.md, "A
/// carried object"). Engine choice: a pick-up in the air has it too.
pub fn hurt_carrying(player: &Player, physics: &Physics) -> bool {
    let set_down = frames(physics[Param::PutDownSet]);
    player
        .hands
        .is_some_and(|h| h.act != Act::PutDown || h.frame < set_down)
        || player.pose.carried_air
}

/// Whether a player with `clock` is in the fidget pose at the end of the
/// last step, taking it `start` frames into standing, for `length`
/// frames, and again every `every` frames (0: only once).
pub fn fidgeting(clock: &PoseClock, (start, length, every): (u32, u32, u32)) -> bool {
    let Some(n) = clock.standing.filter(|&n| n >= start) else {
        return false;
    };
    let into = if every > 0 {
        (n - start) % every
    } else {
        n - start
    };
    into < length
}
