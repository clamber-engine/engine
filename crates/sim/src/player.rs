//! The player: its body, its movement states and contact with the level.
//!
//! Each step reads one [`Frame`] of input and moves through the states in
//! [`State`]: standing, walking and running on the ground, rolling,
//! jumping and falling, and swapping to the next roster character.
//! Collision keeps the body on slopes and one-way platforms and stops it
//! at walls and ceilings.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame, JumpBuffer};
use clamber_level::{Dir, Hit, Level, Vec2};
use clamber_pack::{Feel, Param, Physics};

use crate::world::Event;

/// Half the body's width, px. The body is a box standing on its
/// position, which is the middle of its feet.
pub const HALF_WIDTH: Fixed = Fixed::from_int(8);

/// The body's height, px.
pub const HEIGHT: Fixed = Fixed::from_int(32);

/// How far a grounded body follows the ground up or down in one step, px.
pub(crate) const STEP: Fixed = Fixed::from_int(8);

/// The smallest distance there is.
pub(crate) const TINY: Fixed = Fixed::from_bits(1);

/// The ability a character needs to roll.
pub const ROLL: &str = "roll";

/// How long swapping to the next character takes, in frames.
pub const SWAP_FRAMES: u8 = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Facing {
    Left,
    #[default]
    Right,
}

/// What the player is doing. Names follow `spec/traces/VOCABULARY.md`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum State {
    /// Grounded, not moving horizontally.
    #[default]
    Idle,
    /// Grounded, moving, action not held.
    Walk,
    /// Grounded, moving, action held.
    Run,
    /// Rolling along the ground on its own momentum.
    Roll,
    /// Airborne and rising after a jump.
    Jump,
    /// Airborne otherwise.
    Fall,
    /// Switching to the next roster character; control is suspended.
    Swap,
    /// Airborne after bouncing off an enemy.
    Bounce,
    /// Hurt, and losing a life; control is suspended.
    Hurt,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Walk => "walk",
            State::Run => "run",
            State::Roll => "roll",
            State::Jump => "jump",
            State::Fall => "fall",
            State::Swap => "swap",
            State::Bounce => "bounce",
            State::Hurt => "hurt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Player {
    /// Roster slot of the character being played.
    pub character: usize,
    /// The middle of the feet.
    pub pos: Vec2,
    /// px per frame, +y down.
    pub vel: Vec2,
    /// The collider being stood on, if any.
    pub ground: Option<usize>,
    pub facing: Facing,
    pub state: State,
    /// A recent jump press waiting until a jump is possible.
    pub jump_buffer: JumpBuffer,
    /// Frames since walking off a ledge, while a jump is still allowed
    /// for that reason.
    pub coyote: Option<u8>,
    /// Frames left after landing before a jump can start.
    pub lockout: u8,
    /// The buffered jump press was made in the air and is forgotten if
    /// jump is let go before it's used.
    pub jump_needs_hold: bool,
    /// Frames of swapping left.
    pub swap: u8,
    /// Crouching, or getting up from a crouch.
    pub crouch: Crouch,
    /// Frames since the turn under way began, if one is.
    pub turn: Option<u8>,
    /// Colliders the body ran into or stood on during the last step:
    /// walls it was stopped by, a ceiling it hit, the ground.
    pub touched: Vec<usize>,
}

impl Player {
    /// A player standing still at `pos`, on the ground if there is ground
    /// right under it.
    pub fn new(level: &Level, pos: Vec2) -> Player {
        let mut player = Player {
            character: 0,
            pos,
            vel: Vec2::default(),
            ground: None,
            facing: Facing::Right,
            state: State::Idle,
            jump_buffer: JumpBuffer::new(0),
            coyote: None,
            lockout: 0,
            jump_needs_hold: false,
            swap: 0,
            crouch: Crouch::default(),
            turn: None,
            touched: Vec::new(),
        };
        player.ground = level
            .collision
            .cast(pos, Dir::Down, Fixed::ZERO)
            .map(|hit| hit.collider);
        if !player.grounded() {
            player.state = State::Fall;
        }
        player
    }

    pub fn grounded(&self) -> bool {
        self.ground.is_some()
    }

    /// The body as `(left, top, right, bottom)`.
    pub fn body(&self) -> (Fixed, Fixed, Fixed, Fixed) {
        let p = self.pos;
        (p.x - HALF_WIDTH, p.y - HEIGHT, p.x + HALF_WIDTH, p.y)
    }

    /// Sends the player flying at `vel`, as a launcher does.
    pub fn launch(&mut self, vel: Vec2, events: &mut Vec<Event>) {
        if self.state == State::Roll {
            events.push(Event::RollEnd);
        }
        self.vel = vel;
        self.ground = None;
        self.coyote = None;
        self.state = State::Fall;
    }

    /// Bounces up at `speed` off something landed on.
    pub fn bounce(&mut self, speed: Fixed, events: &mut Vec<Event>) {
        self.vel.y = -speed;
        self.ground = None;
        self.coyote = None;
        if !events.contains(&Event::Bounce) {
            events.push(Event::Bounce);
        }
        self.state = State::Bounce;
    }

    /// Advances the player one step under `physics`, the physics in
    /// effect where it is. `feel` says which characters there are and
    /// what each can do.
    pub fn update(
        &mut self,
        level: &Level,
        feel: &Feel,
        physics: &Physics,
        input: &Frame,
        events: &mut Vec<Event>,
    ) {
        self.touched.clear();
        self.swap = self.swap.saturating_sub(1);
        // While swapping, nothing is pressed or held.
        let input = if self.swap > 0 {
            &Frame::default()
        } else {
            input
        };
        let held = |a| input.held.contains(a);
        let pressed = |a| input.pressed.contains(a);
        let dir = match (held(Action::Left), held(Action::Right)) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };

        // A press waits out the jump delay in the buffer, however short
        // the buffer is.
        let delay = frames(physics[Param::JumpDelay]);
        self.jump_buffer
            .set_window(frames(physics[Param::JumpBuffer]).max(delay));
        self.jump_buffer.update(pressed(Action::Jump));
        if pressed(Action::Jump) {
            self.jump_needs_hold = false;
        }
        let locked = self.lockout > 0;
        self.lockout = self.lockout.saturating_sub(1);

        if self.grounded()
            && self.state != State::Roll
            && pressed(Action::Swap)
            && feel.roster.len() > 1
        {
            self.character = (self.character + 1) % feel.roster.len();
            self.swap = SWAP_FRAMES;
            self.jump_buffer.take();
        }

        let can_jump = self.swap == 0
            && !locked
            && (self.grounded() || self.coyote.is_some())
            && self.jump_buffer.age().is_some_and(|age| age >= delay);
        if can_jump && self.jump_buffer.take() {
            if self.state == State::Roll {
                events.push(Event::RollEnd);
            }
            self.vel.y = -physics[Param::JumpSpeed];
            if !held(Action::Jump) {
                // A buffered tap that's already over makes a short hop.
                self.vel.y = self.vel.y.max(-physics[Param::JumpReleaseSpeed]);
            }
            self.ground = None;
            self.coyote = None;
            self.state = State::Jump;
            events.push(Event::JumpStart);
        } else if self.state == State::Jump
            && input.released.contains(Action::Jump)
            && self.vel.y.is_negative()
        {
            self.vel.y = self.vel.y.max(-physics[Param::JumpReleaseSpeed]);
            events.push(Event::JumpRelease);
        }
        // Held through the frame before the jump is enough: a press let
        // go of on the frame it's used still jumps.
        if self.jump_needs_hold && !held(Action::Jump) {
            self.jump_buffer.take();
        }

        let roll_input = held(Action::Down)
            && held(Action::Action)
            && (pressed(Action::Down) || pressed(Action::Action));
        if roll_input
            && self.grounded()
            && self.swap == 0
            && self.state != State::Roll
            && self.vel.x.abs() >= physics[Param::RollMinSpeed]
            && feel.roster[self.character].abilities.contains_key(ROLL)
        {
            self.state = State::Roll;
            events.push(Event::RollStart);
        }
        let rolling = self.state == State::Roll;

        let crouching = self.crouch.update(
            held(Action::Down) && self.grounded() && !rolling,
            frames(physics[Param::CrouchStart]),
            frames(physics[Param::CrouchRecovery]),
        );

        let turn_time = frames(physics[Param::TurnTime]);
        let facing = if rolling {
            // A roll keeps facing the way it goes.
            self.turn = None;
            if self.vel.x.is_negative() {
                Facing::Left
            } else {
                Facing::Right
            }
        } else if turn_time > 0 && self.grounded() {
            self.turn_on_ground(dir, turn_time)
        } else {
            self.turn = None;
            match dir {
                -1 => Facing::Left,
                1 => Facing::Right,
                _ => self.facing,
            }
        };
        if facing != self.facing {
            self.facing = facing;
            events.push(Event::Turn);
        }
        // A crouch holds the player where they are.
        let dir = if crouching { 0 } else { dir };

        let cap = if held(Action::Action) {
            physics[Param::TopSpeed]
        } else {
            physics[Param::WalkSpeed]
        };
        let response = physics[Param::SpeedResponse];
        self.vel.x = if rolling {
            roll_speed(self.vel.x, dir, physics)
        } else if response > Fixed::ZERO {
            let target = match dir {
                -1 => -cap,
                1 => cap,
                _ => Fixed::ZERO,
            };
            ease(self.vel.x, target, response)
        } else {
            horizontal_speed(self.vel.x, dir, self.grounded(), cap, physics)
        };
        let moved = self.move_horizontally(level);

        if self.grounded() {
            self.follow_ground(level, events);
        } else {
            let gravity = if self.vel.y.is_negative() {
                physics[Param::Gravity]
            } else {
                physics[Param::FallGravity]
            };
            self.vel.y = (self.vel.y + gravity).min(physics[Param::MaxFallSpeed]);
            self.move_vertically(level, moved, events);
        }

        self.coyote = if self.grounded() {
            None
        } else if events.contains(&Event::LeaveGround) {
            Some(1)
        } else {
            self.coyote.map(|n| n.saturating_add(1))
        }
        .filter(|&n| n <= frames(physics[Param::CoyoteTime]));

        if events.contains(&Event::Land) {
            self.lockout = frames(physics[Param::LandingLockout]);
        }
        // A press on a frame that ends on the ground, landing included,
        // jumps when it can whether or not jump is still held.
        if self.jump_buffer.age() == Some(0) {
            self.jump_needs_hold = physics[Param::JumpBufferHold] > Fixed::ZERO && !self.grounded();
        }

        if rolling && (!self.grounded() || self.vel.x.abs() < physics[Param::RollMinSpeed]) {
            events.push(Event::RollEnd);
        }
        self.state = self.next_state(held(Action::Action), events);
    }

    /// Goes on with the turn under way, or starts one if `dir` points
    /// behind the player, and returns the facing for this frame. A turn
    /// starts at once whether the player is standing or moving; when one
    /// starts while moving in Classic 1 isn't in the spec yet
    /// (clamber-engine/spec#12, #55).
    fn turn_on_ground(&mut self, dir: i32, turn_time: u8) -> Facing {
        let mut facing = self.facing;
        self.turn = self.turn.map(|n| n + 1).filter(|&n| n < turn_time);
        let behind = match facing {
            Facing::Left => 1,
            Facing::Right => -1,
        };
        if self.turn.is_none() && dir == behind {
            self.turn = Some(0);
        }
        if self.turn == Some(turn_time / 2) {
            facing = match facing {
                Facing::Left => Facing::Right,
                Facing::Right => Facing::Left,
            };
        }
        facing
    }

    /// The state after this step's movement.
    fn next_state(&self, running: bool, events: &[Event]) -> State {
        if self.swap > 0 {
            State::Swap
        } else if self.state == State::Roll && !events.contains(&Event::RollEnd) {
            State::Roll
        } else if self.grounded() {
            if self.vel.x == Fixed::ZERO {
                State::Idle
            } else if running {
                State::Run
            } else {
                State::Walk
            }
        } else if self.state == State::Jump && self.vel.y.is_negative() {
            State::Jump
        } else if self.state == State::Bounce {
            State::Bounce
        } else {
            State::Fall
        }
    }

    /// Moves sideways as far as walls allow, and returns how far.
    fn move_horizontally(&mut self, level: &Level) -> Fixed {
        if self.vel.x == Fixed::ZERO {
            return Fixed::ZERO;
        }
        let (cast_dir, distance) = if self.vel.x.is_negative() {
            (Dir::Left, -self.vel.x)
        } else {
            (Dir::Right, self.vel.x)
        };
        // Sweep the body from the feet to just below the head, so nothing
        // slips past. On the ground, the bottom step's height is left out:
        // low bumps are stepped over and slopes walked up, as following
        // the ground goes up to a step. In the air there's no stepping
        // over, since landing only happens with the middle of the feet
        // over the ground.
        let lift = if self.grounded() { STEP } else { Fixed::ZERO };
        let from = Vec2::new(self.pos.x, self.pos.y - lift - TINY);
        let reach = (HEIGHT - Fixed::ONE - lift - TINY, Fixed::ZERO);
        let wall = level
            .collision
            .sweep(from, cast_dir, distance + HALF_WIDTH, reach, is_wall);
        let moved = match wall.map(|hit| (hit.distance - HALF_WIDTH, hit.collider)) {
            Some((room, collider)) if room < distance => {
                self.vel.x = Fixed::ZERO;
                self.touched.push(collider);
                room.max(Fixed::ZERO)
            }
            _ => distance,
        };
        match cast_dir {
            Dir::Left => self.pos.x -= moved,
            _ => self.pos.x += moved,
        }
        moved
    }

    /// Keeps a grounded body on the ground as it moves over slopes and
    /// bumps, or lets it leave the ground when there's none within reach.
    fn follow_ground(&mut self, level: &Level, events: &mut Vec<Event>) {
        let from = Vec2::new(self.pos.x, self.pos.y - STEP);
        match level.collision.cast(from, Dir::Down, STEP + STEP) {
            Some(hit) => {
                self.pos.y = hit.point.y;
                self.vel.y = Fixed::ZERO;
                self.ground = Some(hit.collider);
                self.touched.push(hit.collider);
            }
            None => {
                self.ground = None;
                events.push(Event::LeaveGround);
            }
        }
    }

    /// Moves up or down as far as the level allows, after moving `moved`
    /// sideways.
    fn move_vertically(&mut self, level: &Level, moved: Fixed, events: &mut Vec<Event>) {
        if self.vel.y.is_negative() {
            // One-way platforms aren't found going up, so jumps pass
            // through them from below.
            let head = Vec2::new(self.pos.x, self.pos.y - HEIGHT);
            match level.collision.cast(head, Dir::Up, -self.vel.y) {
                Some(hit) => {
                    self.pos.y = hit.point.y + HEIGHT;
                    self.vel.y = Fixed::ZERO;
                    self.touched.push(hit.collider);
                }
                None => self.pos.y += self.vel.y,
            }
            return;
        }
        // Look for ground from above the feet: moving sideways into a
        // slope, which isn't a wall, takes the feet below its surface by
        // up to as far as they moved, and they land on it rather than
        // falling on inside it. One-way platforms only catch feet coming
        // down onto them.
        let feet = self.pos.y;
        let above = moved.max(STEP);
        let from = Vec2::new(self.pos.x, feet - above);
        let reach = (Fixed::ZERO, Fixed::ZERO);
        let ground = level
            .collision
            .sweep(from, Dir::Down, above + self.vel.y, reach, |hit| {
                !hit.one_way || hit.point.y >= feet
            });
        match ground {
            Some(hit) => {
                self.pos.y = hit.point.y;
                self.vel.y = Fixed::ZERO;
                self.ground = Some(hit.collider);
                self.touched.push(hit.collider);
                events.push(Event::Land);
            }
            None => self.pos.y += self.vel.y,
        }
    }
}

/// Crouching on the ground, from holding down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Crouch {
    /// Frames down has been held on the ground.
    pub held: u8,
    /// Frames left of getting up after letting go.
    pub rising: u8,
}

impl Crouch {
    /// Advances one frame, with `down` held on the ground or not, and
    /// returns whether the player is crouching on this frame.
    fn update(&mut self, down: bool, start: u8, recovery: u8) -> bool {
        if start == 0 {
            *self = Crouch::default();
            return false;
        }
        if down {
            self.held = self.held.saturating_add(1);
        } else {
            if self.held >= start {
                self.rising = recovery;
            }
            self.held = 0;
        }
        let crouching = self.held >= start || self.rising > 0;
        self.rising = self.rising.saturating_sub(1);
        crouching
    }
}

/// A whole number of frames from a parameter, clamped to what fits.
fn frames(value: Fixed) -> u8 {
    value.floor().clamp(0, u8::MAX.into()) as u8
}

/// Edges steeper than 45 degrees block; shallower ones are walked on.
pub(crate) fn is_wall(hit: &Hit) -> bool {
    let (a, b) = hit.edge;
    (b.y - a.y).abs() > (b.x - a.x).abs()
}

/// `vx` moved toward zero by `amount`, without reversing it.
fn slow_down(vx: Fixed, amount: Fixed) -> Fixed {
    if vx.abs() <= amount {
        Fixed::ZERO
    } else if vx.is_negative() {
        vx + amount
    } else {
        vx - amount
    }
}

/// The horizontal speed after one step of pressing `dir` (-1, 0 or 1),
/// where acceleration stops at `cap`.
fn horizontal_speed(vx: Fixed, dir: i32, grounded: bool, cap: Fixed, physics: &Physics) -> Fixed {
    let friction = physics[Param::GroundFriction];
    if dir == 0 {
        return if grounded {
            slow_down(vx, friction)
        } else {
            vx
        };
    }
    // Work as if pressing right, then mirror.
    let v = if dir < 0 { -vx } else { vx };
    let v = if !grounded {
        // Air control doesn't pass the cap, but keeps speed beyond it.
        (v + physics[Param::AirAcceleration]).min(cap.max(v))
    } else if v.is_negative() {
        v + physics[Param::GroundDeceleration]
    } else if v > cap {
        // Faster than the cap, say after letting go of run: ease down.
        (v - friction).max(cap)
    } else {
        (v + physics[Param::GroundAcceleration]).min(cap)
    };
    if dir < 0 { -v } else { v }
}

/// The smallest change easing makes to a speed, px per frame.
pub(crate) const SPEED_GRAIN: Fixed = Fixed::from_ratio_pow2(1, 8);

/// `v` moved toward `target` by `response` of the gap between them,
/// rounded toward zero to a whole number of [`SPEED_GRAIN`]s. A step that
/// rounds to nothing closes the gap, so easing always arrives.
fn ease(v: Fixed, target: Fixed, response: Fixed) -> Fixed {
    let gap = target - v;
    let step = gap.checked_mul(response).unwrap_or(gap);
    let grain = SPEED_GRAIN.to_bits();
    let whole = step.to_bits().abs() / grain * grain;
    let step = Fixed::from_bits(if step.is_negative() { -whole } else { whole });
    if step == Fixed::ZERO {
        target
    } else {
        v + step
    }
}

/// The speed after one step of rolling while pressing `dir`. A roll never
/// speeds up; pressing against it brakes harder.
fn roll_speed(vx: Fixed, dir: i32, physics: &Physics) -> Fixed {
    let against = (dir < 0 && vx > Fixed::ZERO) || (dir > 0 && vx.is_negative());
    let param = if against {
        Param::RollDeceleration
    } else {
        Param::RollFriction
    };
    slow_down(vx, physics[param])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(x: f64) -> Fixed {
        Fixed::from_f64(x).unwrap()
    }

    fn physics() -> Physics {
        clamber_pack::Presets::builtin().world("default").unwrap()
    }

    #[test]
    fn accelerates_to_the_cap_and_stops_with_friction() {
        let p = physics();
        let top = p[Param::TopSpeed];
        let mut v = Fixed::ZERO;
        for _ in 0..1000 {
            v = horizontal_speed(v, 1, true, top, &p);
        }
        assert_eq!(v, top);
        assert_eq!(horizontal_speed(-v, -1, true, top, &p), -top);
        for _ in 0..1000 {
            v = horizontal_speed(v, 0, true, top, &p);
        }
        assert_eq!(v, Fixed::ZERO);
    }

    #[test]
    fn turning_uses_deceleration() {
        let p = physics();
        let top = p[Param::TopSpeed];
        let v = horizontal_speed(fx(2.0), -1, true, top, &p);
        assert_eq!(v, fx(2.0) - p[Param::GroundDeceleration]);
        // in the air, momentum is kept with nothing pressed
        assert_eq!(horizontal_speed(fx(2.0), 0, false, top, &p), fx(2.0));
    }

    #[test]
    fn speed_over_the_cap_is_kept_in_the_air_and_eased_on_the_ground() {
        let p = physics();
        let walk = p[Param::WalkSpeed];
        let fast = p[Param::TopSpeed];
        assert_eq!(horizontal_speed(fast, 1, false, walk, &p), fast);
        assert_eq!(
            horizontal_speed(fast, 1, true, walk, &p),
            fast - p[Param::GroundFriction]
        );
        let eased = horizontal_speed(walk + fx(0.01), 1, true, walk, &p);
        assert_eq!(eased, walk);
    }

    #[test]
    fn rolls_only_slow_down() {
        let p = physics();
        let v = fx(4.0);
        assert_eq!(roll_speed(v, 1, &p), v - p[Param::RollFriction]);
        assert_eq!(roll_speed(v, 0, &p), v - p[Param::RollFriction]);
        assert_eq!(roll_speed(v, -1, &p), v - p[Param::RollDeceleration]);
        assert_eq!(roll_speed(-v, 1, &p), -v + p[Param::RollDeceleration]);
        assert_eq!(roll_speed(fx(0.01), 0, &p), Fixed::ZERO);
    }

    #[test]
    fn easing_closes_part_of_the_gap_in_whole_grains() {
        let eighth = fx(0.125);
        let speeds = |from: f64, to: f64, n: usize| {
            let mut v = fx(from);
            (0..n)
                .map(|_| {
                    v = ease(v, fx(to), eighth);
                    (v.to_f64() * 256.0) as i64
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(speeds(0.0, 2.0, 4), [64, 120, 169, 211]);
        // Rounding is toward zero going down too: a gap of -686 grains
        // steps by -85.
        assert_eq!(speeds(686.0 / 256.0, 0.0, 1), [686 - 85]);
        // Within 8 grains, a step rounds to nothing and the gap closes.
        assert_eq!(speeds(505.0 / 256.0, 2.0, 2), [512, 512]);
        assert_eq!(speeds(-0.5, -0.5, 1), [-128]);
    }

    #[test]
    fn frame_counts_from_params() {
        assert_eq!(frames(fx(5.75)), 5);
        assert_eq!(frames(fx(-1.0)), 0);
        assert_eq!(frames(fx(1000.0)), 255);
    }
}
