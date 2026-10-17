//! Riding an animal (spec/behavior/riding.md, riding-slopes.md,
//! riding-faces.md).
//!
//! A rider has none of the player's own moves. Its speed across eases
//! toward a target set by Left or Right, faster with Action, closing a
//! share of the gap each frame: a larger share on **steered** frames,
//! which follow a jump. It turns in a fixed number of frames, jumps after
//! a crouch, and on an Action press **charges**: a while later it hops,
//! and once it's back on the floor the charge ends with a pause and one
//! settling frame.
//!
//! On the floor its feet are on the floor of the column holding `x`,
//! followed down as far as `follow` and left beyond that. Taking off, it
//! starts from the higher of where it was and the floor under its new
//! `x`. In the air it falls with the preset's jump gravity, meets walls
//! as the player does, and lands on the floor of its column, but not on
//! floor whose top it's already below.
//!
//! On a steep face it can't jump, a direction held up the face counts as
//! none, and on frames that aren't steered a `push` down the face is
//! added to its target speed. Rising face columns stop it, and the low
//! end of a face over a drop is always left. A jump that lands soon
//! after its **approach frame**, when floor first comes within reach
//! below, stays steered and lands for longer.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::Level;
use clamber_pack::{Param, Physics};

use crate::archetype::Ride;
use crate::columns;
use crate::player::{Player, Reach, State, ceil, direction, ease, facing_of};
use crate::world::Event;

/// Which part of riding a frame is in, with how many frames of it came
/// before this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RidePart {
    /// On the floor, under control.
    Riding,
    Turn(u8),
    /// A jump: its crouch frames, then the take-off frame, then the air.
    Jump(u8),
    /// The landing frames after a jump.
    Landing(u8),
    /// A charge, `steered` if it started during a jump and keeps its
    /// steering.
    Charge {
        age: u8,
        steered: bool,
    },
    /// The end of a charge.
    ChargeEnd {
        age: u8,
        steered: bool,
    },
    /// The frame after a charge's end.
    Settling,
}

impl RidePart {
    /// The part the frame after one in this part is in, before anything
    /// starts on it.
    fn next(self, ride: &Ride, landing: u8) -> RidePart {
        let on = |age: u8, length: u8| age.saturating_add(1) < length;
        match self {
            RidePart::Turn(age) if on(age, ride.turn) => RidePart::Turn(age + 1),
            RidePart::Jump(age) => RidePart::Jump(age.saturating_add(1)),
            RidePart::Landing(age) if on(age, landing) => RidePart::Landing(age + 1),
            RidePart::Charge { age, steered } => RidePart::Charge {
                age: age.saturating_add(1),
                steered,
            },
            RidePart::ChargeEnd { age, steered } if on(age, ride.charge_end) => {
                RidePart::ChargeEnd {
                    age: age + 1,
                    steered,
                }
            }
            RidePart::ChargeEnd { .. } => RidePart::Settling,
            _ => RidePart::Riding,
        }
    }

    /// Whether a jump's steering comes on or stays on at the end of a
    /// frame in this part.
    fn steers(self) -> bool {
        match self {
            RidePart::Jump(_) => true,
            RidePart::Charge { steered, .. } | RidePart::ChargeEnd { steered, .. } => steered,
            _ => false,
        }
    }
}

/// A player riding an animal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Riding {
    /// How the animal is ridden.
    pub ride: Ride,
    pub part: RidePart,
    /// Steering was on at the end of the last frame, so this one is
    /// steered.
    pub steer: bool,
    /// On the floor at the end of the last frame. A take-off frame ends
    /// on the floor.
    pub on_floor: bool,
    /// How many frames ago Jump was pressed, while it has been held since
    /// and the press hasn't started a jump.
    pub press: Option<u8>,
    /// Jump has been held on every frame since the jump under way, or the
    /// one the charge under way started from, was pressed: it uses
    /// `held_jump_gravity`.
    pub held: bool,
    /// Where the feet are put at the start of the next frame, the first
    /// one riding.
    pub mount: Option<Fixed>,
    /// The speed down a steep face added to the next frame's target.
    pub push: Fixed,
    /// The jump under way has ended a frame after its take-off moving
    /// down.
    pub fell: bool,
    /// How many frames ago the jump under way, or the last one, had its
    /// approach frame.
    pub approach: Option<u8>,
    /// How many landing frames the last jump's landing has.
    pub landing: u8,
}

impl Riding {
    /// Riding `ride` from the next frame, with the feet put at `y` then.
    /// A jump can't start until a frame has ended on the floor.
    pub fn new(ride: Ride, y: Fixed) -> Riding {
        Riding {
            mount: Some(y),
            ride,
            part: RidePart::Riding,
            steer: false,
            on_floor: false,
            press: None,
            held: false,
            push: Fixed::ZERO,
            fell: false,
            approach: None,
            landing: ride.landing,
        }
    }
}

impl Player {
    /// Advances a riding player one step, with `dir` the direction held
    /// (-1, 0 or 1).
    pub(crate) fn ride(
        &mut self,
        level: &Level,
        physics: &Physics,
        input: &Frame,
        dir: i32,
        events: &mut Vec<Event>,
    ) {
        let Some(mut r) = self.riding else {
            return;
        };
        let ride = r.ride;
        let held = |a| input.held.contains(a);
        let pressed = |a| input.pressed.contains(a);
        if let Some(y) = r.mount.take() {
            self.put_down(level, y, events);
        }
        let steered = r.steer;
        let grounded = self.grounded();
        r.part = r.part.next(&ride, r.landing);
        r.approach = r.approach.map(|age| age.saturating_add(1));

        // The way the steep face the frame starts on falls, if any, and a
        // direction held up it, which counts as none.
        let slope = physics[Param::FaceSlope];
        let (start_column, start_y) = (columns::column(self.pos.x), self.pos.y);
        let face = if slope > Fixed::ZERO && r.on_floor {
            columns::face(level, start_column, start_y, slope)
        } else {
            None
        };
        let way = if face == Some(-dir) { 0 } else { dir };

        // Jump presses are remembered for a while if Jump stays held.
        r.press = if pressed(Action::Jump) {
            Some(0)
        } else if held(Action::Jump) {
            r.press.map(|age| age.saturating_add(1))
        } else {
            None
        };
        r.press = r.press.filter(|&age| age <= ride.jump_memory);
        r.held &= held(Action::Jump);
        if r.press.is_some() && r.on_floor && !steered && face.is_none() {
            r.press = None;
            r.part = RidePart::Jump(0);
            r.held = true;
            r.fell = false;
            r.approach = None;
        }
        // A jump started on this frame is taken first, so an Action press
        // with it charges from its crouch.
        if pressed(Action::Action) {
            let from_jump = match r.part {
                RidePart::Jump(_) => true,
                RidePart::Landing(_) => steered,
                _ => false,
            };
            if from_jump
                || matches!(
                    r.part,
                    RidePart::Riding | RidePart::Turn(_) | RidePart::Landing(_)
                )
            {
                r.part = RidePart::Charge {
                    age: 0,
                    steered: from_jump,
                };
                r.held &= from_jump;
            }
        }
        if !steered
            && matches!(r.part, RidePart::Riding | RidePart::Settling)
            && way == -direction(self.facing)
        {
            r.part = RidePart::Turn(0);
        }
        let facing = if steered && dir != 0 {
            facing_of(dir)
        } else if r.part == RidePart::Turn(ride.turn_face) {
            facing_of(-direction(self.facing))
        } else {
            self.facing
        };
        if facing != self.facing {
            self.facing = facing;
            events.push(Event::Turn);
        }

        let speed = if held(Action::Action) {
            ride.run_speed
        } else {
            ride.speed
        };
        let target = speed * Fixed::from_int(way);
        let (push, response) = if steered {
            (Fixed::ZERO, ride.steer_response)
        } else {
            (r.push, ride.response)
        };
        self.target = target + push;
        self.vel.x = ease(self.vel.x, self.target, response);
        let moved = if grounded {
            self.move_by_columns(level, slope, None)
        } else {
            self.move_in_air_by_columns(level, physics, events)
        };

        let gravity = if r.held {
            physics[Param::HeldJumpGravity]
        } else {
            physics[Param::Gravity]
        };
        let column = columns::column(self.pos.x);
        // The low end of a face that ends at a drop is always left.
        let low_end = face.is_some_and(|way| {
            columns::floor(level, start_column + i64::from(way), start_y).is_none()
        });
        if grounded && self.vel.y.is_negative() {
            // Taking off, from the higher of where it was and the floor
            // under it now.
            if let Some(floor) = columns::floor(level, column, self.pos.y) {
                self.pos.y = self.pos.y.min(floor.y);
            }
            self.ground = None;
            self.vel.y += gravity;
            self.ride_vertically(level, physics, moved, events);
        } else if grounded {
            match columns::floor(level, column, self.pos.y) {
                Some(floor) if !low_end && floor.y - self.pos.y <= ride.follow => {
                    self.pos.y = floor.y;
                    self.ground = Some(floor.collider);
                    self.touched.push(floor.collider);
                }
                _ => {
                    self.ground = None;
                    self.pos.y += ride.leave_drop + gravity;
                    self.vel.y = ride.leave_speed;
                    events.push(Event::LeaveGround);
                }
            }
        } else if !self.grounded() {
            self.vel.y = (self.vel.y + gravity).min(physics[Param::MaxFallSpeed]);
            self.ride_vertically(level, physics, moved, events);
        }

        if matches!(r.part, RidePart::Jump(age) if age > ride.crouch) {
            if r.fell && r.approach.is_none() && self.approaching(level, &ride) {
                r.approach = Some(0);
            }
            r.fell |= self.vel.y > Fixed::ZERO;
            if !grounded && self.grounded() {
                r.part = RidePart::Landing(0);
                r.landing = match r.approach {
                    Some(age) if age <= ride.approach_steer => ride.approach_land - age,
                    _ => ride.landing,
                };
            }
        }
        match r.part {
            RidePart::Jump(age) if age == ride.crouch => {
                self.vel.y = -ride.jump_speed;
                events.push(Event::JumpStart);
            }
            RidePart::Charge { age, .. } if age == ride.hop_at && self.grounded() => {
                self.vel.y = -ride.hop_speed;
            }
            RidePart::Charge { age, steered } if age >= ride.charge && self.grounded() => {
                r.part = RidePart::ChargeEnd { age: 0, steered };
            }
            _ => {}
        }

        // The push for the next frame, from where this one ended.
        let end_face = if self.grounded() && !steered && slope > Fixed::ZERO {
            columns::face(level, columns::column(self.pos.x), self.pos.y, slope)
        } else {
            None
        };
        r.push = match end_face {
            None => Fixed::ZERO,
            Some(way) => {
                let down = |v: Fixed| v * Fixed::from_int(way) >= Fixed::ZERO;
                if down(target) && down(self.vel.x) {
                    ride.push * Fixed::from_int(way)
                } else {
                    r.push
                }
            }
        };
        r.on_floor = self.grounded();
        // Landing soon after the approach frame stays steered until a
        // while after it.
        r.steer = r.part.steers()
            || (matches!(r.part, RidePart::Landing(_))
                && r.approach.is_some_and(|age| age < ride.approach_steer));
        self.riding = Some(r);
        self.state = if !self.grounded() {
            if self.vel.y.is_negative() {
                State::Jump
            } else {
                State::Fall
            }
        } else if self.vel.x == Fixed::ZERO {
            State::Idle
        } else if held(Action::Action) {
            State::Run
        } else {
            State::Walk
        };
    }

    /// Moves up or down in the air by the frame's speed after moving
    /// `moved` across. Falling, it lands on the floor of the column holding
    /// `x`, the highest top that isn't above the feet, as the player does;
    /// floor whose top is above the feet doesn't catch it.
    fn ride_vertically(
        &mut self,
        level: &Level,
        physics: &Physics,
        moved: Fixed,
        events: &mut Vec<Event>,
    ) {
        if self.vel.y.is_negative() {
            self.move_vertically(level, moved, Reach::new(physics), true, events);
            return;
        }
        let to = self.pos.y + self.vel.y;
        let floor = columns::spans(level, columns::column(self.pos.x))
            .into_iter()
            .filter(|s| s.top >= self.pos.y)
            .min_by_key(|s| s.top);
        match floor {
            Some(s)
                if self.vel.y > Fixed::ZERO
                    && Reach::new(physics).lands(level, self.pos.x, s.top, to) =>
            {
                self.land(s.top, s.collider, events);
            }
            _ => self.pos.y = to,
        }
    }

    /// Whether there's floor within an approach's reach below the feet
    /// line, in the column holding `x`.
    fn approaching(&self, level: &Level, ride: &Ride) -> bool {
        let feet = ceil(self.pos.y);
        let row = |y: i64| (y - 1).div_euclid(i64::from(ride.approach_row));
        columns::spans(level, columns::column(self.pos.x))
            .into_iter()
            .map(|s| ceil(s.top))
            .filter(|&top| top >= feet)
            .min()
            .is_some_and(|top| {
                top - feet <= i64::from(ride.approach_reach)
                    && row(top) - row(feet) <= i64::from(ride.approach_rows)
            })
    }
}
