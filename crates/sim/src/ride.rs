//! Riding an animal (spec/behavior/riding.md, riding-slopes.md,
//! riding-faces.md, getting-off.md).
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
//!
//! Walkers meet a rider by their [`RiderReach`]
//! (spec/behavior/riding-walkers.md). One **in front of the animal**
//! after the rider's move starts a charge, unless one is under way, and
//! a charge far enough along throws it out of the level, sending the
//! rider up. One that reached the rider at the end of a step knocks it
//! off the animal on the next: the player holds still with everything
//! else for a while, then falls on foot, and the animal runs off.
//!
//! A Tandem press gets off (spec/behavior/getting-off.md): with a riding
//! jump starting on that frame it's a **jump off**, which takes off on
//! the next frame as a jump on foot; otherwise a **hop off**, whose
//! landing is followed by landing frames before the player's own moves
//! start. The animal is left where the rider was (see
//! [`hutch`](crate::hutch)).

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{Param, Physics};

use crate::archetype::{Behavior, Ride, RiderReach};
use crate::columns;
use crate::flow::FlowEvent;
use crate::player::{Facing, Player, Reach, State, ceil, direction, ease, facing_of};
use crate::world::{Event, Phase, Thing, World};

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
    /// The frame a walker in front of the animal starts a charge on,
    /// which counts from the next frame.
    Rush {
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
            RidePart::Rush { steered } => RidePart::Charge { age: 0, steered },
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
            RidePart::Charge { steered, .. }
            | RidePart::Rush { steered }
            | RidePart::ChargeEnd { steered, .. } => steered,
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
    /// The last frame was steered.
    pub steered: bool,
    /// On the floor at the end of the last frame. A take-off frame ends
    /// on the floor.
    pub on_floor: bool,
    /// ... and at the end of the one before.
    pub was_on_floor: bool,
    /// How many frames ago Jump was pressed, while it has been held since
    /// and the press hasn't started a jump.
    pub press: Option<u8>,
    /// Jump has been held on every frame since the jump under way, or the
    /// one the charge under way started from, was pressed: it uses
    /// `held_jump_gravity`.
    pub held: bool,
    /// Jump has been held on every frame since the press of the jump on
    /// foot the player mounted from: a charge or a hop off uses
    /// `held_jump_gravity`.
    pub mounted_held: bool,
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
            steered: false,
            on_floor: false,
            was_on_floor: false,
            press: None,
            held: false,
            mounted_held: false,
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
        let mut ride = r.ride;
        if let Some(second) = ride.second
            && self.character != 0
        {
            ride.speed = second.speed;
            ride.run_speed = second.run_speed;
        }
        let held = |a| input.held.contains(a);
        let pressed = |a| input.pressed.contains(a);
        // Getting off on the first riding frame starts from where the
        // mount left the player.
        let off = pressed(Action::Tandem);
        if let Some(y) = r.mount.take()
            && !off
        {
            self.put_down(level, y, events);
        }
        let steered = r.steer;
        r.steered = steered;
        let grounded = self.grounded();
        let last = r.part;
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
        r.mounted_held &= held(Action::Jump);
        let jumps = r.press.is_some() && r.on_floor && !steered && face.is_none();
        if off {
            // A riding jump starting now makes it a jump off, with the
            // held gravity; otherwise it's a hop, with the held gravity
            // in a jump that has it, or after a mount that does. Down with
            // an Action press on the floor gets off into the ground action,
            // which the spec doesn't cover yet; that's a hop here too.
            let in_jump = !matches!(
                last,
                RidePart::Riding | RidePart::Turn(_) | RidePart::Settling
            );
            let light = jumps || (held(Action::Jump) && ((r.held && in_jump) || r.mounted_held));
            self.get_off(
                level, physics, input, &ride, dir, steered, jumps, light, events,
            );
            return;
        }
        if jumps {
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
                r.held = (r.held && from_jump) || r.mounted_held;
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
            self.move_by_columns(level, slope, None, None)
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
        r.was_on_floor = r.on_floor;
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

    /// Gets off the animal on this frame, with `dir` held: the speed across
    /// follows the riding rule, steered or not, and facing turns to `dir`
    /// on a steered frame. Then the player is on foot in the air, moving
    /// up at `off_rise` less the frame's gravity, the held one if `light`.
    /// A jump off takes off on the next frame; a hop off keeps its facing
    /// until it lands. The animal stays where the rider was.
    #[allow(clippy::too_many_arguments)]
    fn get_off(
        &mut self,
        level: &Level,
        physics: &Physics,
        input: &Frame,
        ride: &Ride,
        dir: i32,
        steered: bool,
        jump: bool,
        light: bool,
        events: &mut Vec<Event>,
    ) {
        self.left = Some((self.pos, self.facing));
        let speed = if input.held.contains(Action::Action) {
            ride.run_speed
        } else {
            ride.speed
        };
        let response = if steered {
            ride.steer_response
        } else {
            ride.response
        };
        self.target = speed * Fixed::from_int(dir);
        let vx = ease(self.vel.x, self.target, response);
        if steered && dir != 0 && facing_of(dir) != self.facing {
            self.facing = facing_of(dir);
            events.push(Event::Turn);
        }
        let gravity = if light {
            physics[Param::HeldJumpGravity]
        } else {
            physics[Param::Gravity]
        };
        self.riding = None;
        self.launch(Vec2::new(vx, gravity - ride.off_rise), events);
        self.jump_hold = light;
        self.jump_held_since_press = light;
        self.jump_buffer.take();
        self.lockout = 0;
        let moved = self.move_in_air_by_columns(level, physics, events);
        self.ride_vertically(level, physics, moved, events);
        self.state = State::Jump;
        if jump {
            self.jumping_off = true;
        } else {
            self.hopped_off = Some(ride.off_landing);
        }
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
        columns::floor_within(
            level,
            columns::column(self.pos.x),
            self.pos.y,
            i64::from(ride.approach_reach),
            i64::from(ride.approach_row),
            i64::from(ride.approach_rows),
        )
    }
}

/// A walker's [`RiderReach`], if it's a walker going about its walk.
fn rider_reach(thing: &Thing) -> Option<RiderReach> {
    match thing.archetype.map(|a| a.behavior) {
        Some(Behavior::Walker { rider, .. }) if thing.active && thing.phase == Phase::Ready => {
            Some(rider)
        }
        _ => None,
    }
}

/// A walker's offsets from the rider, whole px: the rider's `x` less the
/// walker's, rounded down, and how far the rider is above the walker,
/// both rounded toward the floor.
fn offsets(player: &Player, walker: &Thing) -> (i64, i64) {
    (
        player.pos.x.floor() - walker.pos.x.floor(),
        ceil(walker.pos.y) - ceil(player.pos.y),
    )
}

/// Whether `across` is within the range of `ranges` for a rider facing
/// `rider` and a walker facing `walker`.
fn across_within(ranges: [(i32, i32); 2], rider: Facing, walker: Facing, across: i64) -> bool {
    let (from, to) = RiderReach::across(ranges, rider, walker);
    (i64::from(from)..=i64::from(to)).contains(&across)
}

/// Starts a charge for walkers in front of the animal, or throws them,
/// with the rider where it is after its move on a step and the walkers
/// where they were before theirs.
pub(crate) fn attack(world: &mut World) {
    let player = &world.player;
    let Some(mut r) = player.riding else {
        return;
    };
    let in_front: Vec<usize> = (0..world.things.len())
        .filter(|&i| {
            let walker = &world.things[i];
            rider_reach(walker).is_some_and(|reach| {
                let (across, above) = offsets(player, walker);
                above <= i64::from(reach.attack_above)
                    && across_within(reach.attack_across, player.facing, walker.facing, across)
            })
        })
        .collect();
    if in_front.is_empty() {
        return;
    }
    match r.part {
        RidePart::Charge { age, .. } if age >= r.ride.throw_from => {
            let vx = world.player.vel.x;
            world.player.vel.y = -r.ride.throw_rise;
            for i in in_front {
                throw(&mut world.things[i], vx);
                world
                    .flow
                    .events
                    .push(FlowEvent::Defeat(world.things[i].index));
            }
        }
        RidePart::Charge { .. } | RidePart::Rush { .. } => {}
        part => {
            // As an Action press would, keeping a jump's steering.
            let from_jump = match part {
                RidePart::Jump(_) => true,
                RidePart::Landing(_) => r.steered,
                _ => false,
            };
            r.part = RidePart::Rush { steered: from_jump };
            r.held = (r.held && from_jump) || r.mounted_held;
            r.steer = r.part.steers();
            world.player.riding = Some(r);
        }
    }
}

/// Throws a walker by a rider moving at `vx`: it moves across at once,
/// then leaves the level as a defeated walker does, from the next step.
fn throw(walker: &mut Thing, vx: Fixed) {
    let Some(Behavior::Walker { rider, defeat, .. }) = walker.archetype.map(|a| a.behavior) else {
        return;
    };
    walker.vx = vx * rider.throw_speed;
    walker.pos.x += walker.vx;
    walker.phase = Phase::Falling {
        vy: -defeat.jump,
        hold: 1,
        glide: None,
    };
}

/// The top of the floor below the rider's feet in its column, rounded
/// toward the floor.
fn floor_below(level: &Level, player: &Player) -> Option<i64> {
    columns::spans(level, columns::column(player.pos.x))
        .into_iter()
        .map(|s| s.top)
        .filter(|&top| top >= player.pos.y)
        .min()
        .map(ceil)
}

/// Knocks the rider off the animal if a walker on its floor reached it
/// by the end of the last step. The player doesn't move on this step,
/// but goes up from the next one it moves on, and the animal runs off.
/// Returns whether it did.
pub(crate) fn unseat(world: &mut World) -> bool {
    let player = &world.player;
    let Some(r) = player.riding else {
        return false;
    };
    let floor = floor_below(&world.level, player);
    let on_floor = r.on_floor && r.was_on_floor;
    let knocked = world.things.iter().any(|walker| {
        rider_reach(walker).is_some_and(|reach| {
            let (across, above) = offsets(player, walker);
            Some(ceil(walker.pos.y)) == floor
                && across_within(reach.knock_across, player.facing, walker.facing, across)
                && (on_floor || above <= i64::from(reach.knock_above))
        })
    });
    if !knocked {
        return false;
    }
    let ride = r.ride;
    let player = &mut world.player;
    let vx = player.vel.x;
    player.launch(Vec2::new(vx, -ride.knock_rise), &mut world.events);
    player.jump_hold = r.held;
    player.frozen = ride.knock_hold;
    player.unseated = true;
    player.riding = None;
    let (pos, facing) = (player.pos, player.facing);
    let speed = match facing {
        Facing::Left => -ride.bolt_speed,
        Facing::Right => ride.bolt_speed,
    };
    for animal in world.things.iter_mut() {
        if animal.phase == Phase::Ridden {
            animal.phase = Phase::Bolting { vx: speed };
            animal.clock = 0;
            animal.facing = facing;
            animal.pos = Vec2::new(pos.x, floor.map_or(pos.y, |y| Fixed::from_int(y as i32)));
        }
    }
    true
}

/// Moves an animal running off after its rider was knocked off, by
/// `ride`, from `vx`: it holds still with everything else first. It's
/// gone once it's past either end of the level.
pub(crate) fn bolt(level: &Level, animal: &mut Thing, ride: &Ride, vx: Fixed) {
    let age = animal.clock;
    animal.clock = animal.clock.saturating_add(1);
    let hold = u32::from(ride.knock_hold);
    if age <= hold {
        return;
    }
    let vx = if age > hold + u32::from(ride.bolt_dash) {
        let glide = if vx.is_negative() {
            -ride.bolt_glide
        } else {
            ride.bolt_glide
        };
        ease(vx, glide, ride.bolt_response)
    } else {
        vx
    };
    animal.phase = Phase::Bolting { vx };
    animal.pos.x += vx;
    if animal.pos.x < Fixed::ZERO || animal.pos.x > level.size.x {
        animal.active = false;
    }
}
