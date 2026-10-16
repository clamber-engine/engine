//! The player: its body, its movement states and contact with the level.
//!
//! Each step reads one [`Frame`] of input and moves through the states in
//! [`State`]: standing, walking and running on the ground, rolling,
//! jumping and falling, the ground action, and swapping to the next
//! roster character. What its hands are doing with an object ([`Hands`])
//! changes what the input does.
//! Collision keeps the body on slopes and one-way platforms and stops it
//! at walls and ceilings.
//! With `face_slope`, a grounded body goes over the ground column by
//! column instead (see `columns`): steep faces carry it down and stop it
//! going up. An airborne body meets higher columns the same way, and is
//! lifted onto ones up to a step higher. Nothing above its head stops
//! it: it rises into one-way platforms and through them, and other ground
//! stops its feet.

use clamber_fixed::FRAC_BITS;
use clamber_fixed::Fixed;
use clamber_input::{Action, Frame, JumpBuffer};
use clamber_level::{Dir, Hit, Level, Shape, Vec2};
use clamber_pack::physics::MAX_STEP;
use clamber_pack::{Feel, Param, Physics};

use crate::columns;
use crate::loss::Lost;
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

/// How long swapping to the next character in place takes, in frames
/// (`swap_moves` 0).
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
    /// In the ground action, started by down and action on the ground
    /// (`ground_action_time` above 0).
    GroundAction,
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
            State::GroundAction => "ground_action",
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
    /// The speed across that `vel.x` was moved toward on the last step:
    /// 0 when nothing moved it on.
    pub target: Fixed,
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
    /// Jump has been held on every frame since it was last pressed.
    pub jump_held_since_press: bool,
    /// The jump under way has had jump held on every frame since its
    /// press, so it uses `held_jump_gravity`.
    pub jump_hold: bool,
    /// The jump started this frame on the ground and leaves it next frame.
    pub takeoff: bool,
    /// Frames of swapping left.
    pub swap: u8,
    /// Frames left that the player is held still after a bounce.
    pub frozen: u8,
    /// Crouching, or getting up from a crouch.
    pub crouch: Crouch,
    /// Frames since the turn under way began, if one is.
    pub turn: Option<u8>,
    /// Standing, walking or running, outside a turn (`stride_steps`
    /// above 0).
    pub gait: Gait,
    /// The timed roll under way, if there is one (`roll_time` above 0).
    pub roll: Option<Roll>,
    /// Which frame of the stop after a timed roll this is, counting from
    /// 1, or 0 outside a stop.
    pub roll_stop: u8,
    /// The timed roll whose stop this is, during a stop.
    pub stopped: Option<Roll>,
    /// The ground action under way, if there is one.
    pub ground_action: Option<GroundAction>,
    /// A boost waiting for action to be held, after a roll or its stop
    /// defeated an enemy (`roll_boost` above 0).
    pub boost: Option<Boost>,
    /// An action press waiting until a timed roll can start: how many
    /// frames ago it was made, how many it waits at most, and whether
    /// action must stay held meanwhile.
    pub roll_press: Option<(u8, u8, bool)>,
    /// How many frames ago the player last ran, outside a roll.
    pub since_run: Option<u8>,
    /// That run was a single frame from standing still, and no direction
    /// has been held since.
    pub lone_run: bool,
    /// A jump pressed during a timed roll ended it, and starts on the
    /// next frame, in the air too.
    pub roll_jump: bool,
    /// Just come off the low end of a steep face: the way it fell, and
    /// for how many more frames a direction held back up it counts as
    /// none.
    pub off_face: Option<(Facing, u8)>,
    /// Frames since the jump under way started, until it lands.
    pub jump_age: Option<u8>,
    /// Frames since the player last landed, while it stays on the ground.
    pub since_land: Option<u8>,
    /// The enemy whose touch hurts the player on the next step, found
    /// at the end of this one (`hurt_width` above 0).
    pub hurt_ahead: Option<usize>,
    /// The knock-out under way, if the player was hurt and is being
    /// knocked out (`knockout_jump` above 0).
    pub knockout: Option<Knockout>,
    /// Frames into the hop that ends taking over from a lost character,
    /// until it lands (see [`crate::Loss`]).
    pub hop: Option<u8>,
    /// What the hands are doing with an object, if anything.
    pub hands: Option<Hands>,
    /// The object a pick-up starts with on this step if action is held,
    /// found before it (see [`crate::carry`]).
    pub pick: Option<usize>,
    /// In the air inside a one-way platform, by columns: risen into it
    /// from below its underside, with the feet still between its
    /// underside and top.
    pub inside: bool,
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
            target: Fixed::ZERO,
            ground: None,
            facing: Facing::Right,
            state: State::Idle,
            jump_buffer: JumpBuffer::new(0),
            coyote: None,
            lockout: 0,
            jump_needs_hold: false,
            jump_held_since_press: false,
            jump_hold: false,
            takeoff: false,
            swap: 0,
            frozen: 0,
            crouch: Crouch::default(),
            turn: None,
            gait: Gait::Standing,
            roll: None,
            roll_stop: 0,
            stopped: None,
            ground_action: None,
            boost: None,
            roll_press: None,
            since_run: None,
            lone_run: false,
            roll_jump: false,
            off_face: None,
            jump_age: None,
            since_land: None,
            hurt_ahead: None,
            knockout: None,
            hop: None,
            hands: None,
            pick: None,
            inside: false,
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
        self.end_roll();
        self.ground_action = None;
        self.vel = vel;
        self.ground = None;
        self.coyote = None;
        self.jump_age = None;
        self.jump_hold = false;
        self.takeoff = false;
        self.state = State::Fall;
    }

    /// Bounces up at `speed` off something landed on, then holds still
    /// for `freeze` frames. Jump held from the frame after those on makes
    /// a held bounce.
    pub fn bounce(&mut self, speed: Fixed, freeze: u8, events: &mut Vec<Event>) {
        self.vel.y = -speed;
        self.ground = None;
        self.coyote = None;
        self.frozen = freeze;
        self.jump_age = None;
        self.jump_hold = true;
        self.takeoff = false;
        self.end_roll();
        self.ground_action = None;
        if !events.contains(&Event::Bounce) {
            events.push(Event::Bounce);
        }
        self.state = State::Bounce;
    }

    /// Hurts the player where it is: it doesn't move on this step.
    pub fn hit(&mut self, events: &mut Vec<Event>) {
        self.touched.clear();
        self.vel = Vec2::default();
        self.frozen = 0;
        self.takeoff = false;
        self.end_roll();
        self.ground_action = None;
        self.turn = None;
        events.push(Event::Hurt);
    }

    /// Ends the timed roll under way or the stop after one, with any
    /// boost waiting.
    fn end_roll(&mut self) {
        self.roll = None;
        self.roll_stop = 0;
        self.stopped = None;
        self.boost = None;
    }

    /// Holds a rolling player still for `freeze` frames after its roll,
    /// or the stop after it, defeated an enemy. A roll that did carries
    /// on into running, and the next frame with action held can boost it.
    pub fn defeat_rolling(&mut self, freeze: u8) {
        self.frozen = freeze;
        if let Some(roll) = &mut self.roll {
            roll.defeated = true;
        }
        if let Some(roll) = self.roll.or(self.stopped) {
            self.boost = Some(Boost {
                dir: roll.dir,
                size: roll.size,
                boosts: roll.boosts.saturating_add(1),
            });
        }
    }

    /// Advances a knocked-out player one step: still, then flying,
    /// rebounding and lying still. Once it has lain still long enough, or
    /// flown out of the bottom of the level, the knock-out is over.
    pub fn knocked_out(&mut self, level: &Level, physics: &Physics, events: &mut Vec<Event>) {
        self.touched.clear();
        let Some(mut knockout) = self.knockout else {
            return;
        };
        knockout.frame = knockout.frame.saturating_add(1);
        let hold = frames(physics[Param::KnockoutHold]);
        if let Some(left) = knockout.rest {
            knockout.rest = Some(left.saturating_sub(1));
        } else if knockout.frame >= hold {
            if knockout.frame == hold {
                let speed = physics[Param::KnockoutSpeed];
                self.vel.x = match self.facing {
                    Facing::Left => speed,
                    Facing::Right => -speed,
                };
                self.vel.y = -physics[Param::KnockoutJump];
                self.ground = None;
            }
            self.fly_knocked_out(level, physics, &mut knockout, events);
        }
        let over = knockout.rest == Some(0) || self.body().1 > level.size.y;
        self.knockout = (!over).then_some(knockout);
    }

    /// One frame of the knock-out flight. The first two landings rebound
    /// at the `knockout_rebound` speeds; the one after stops the flight.
    fn fly_knocked_out(
        &mut self,
        level: &Level,
        physics: &Physics,
        knockout: &mut Knockout,
        events: &mut Vec<Event>,
    ) {
        let gravity = if self.vel.y.is_negative() {
            physics[Param::Gravity]
        } else {
            physics[Param::FallGravity]
        };
        self.vel.y = (self.vel.y + gravity).min(physics[Param::KnockoutMaxFall]);
        let moved = self.move_horizontally(level, HALF_WIDTH, |_| true);
        let mut landed = Vec::new();
        self.move_vertically(
            level,
            moved,
            physics[Param::LandingReach],
            false,
            &mut landed,
        );
        if landed.is_empty() {
            return;
        }
        knockout.landings = knockout.landings.saturating_add(1);
        let rebound = match knockout.landings {
            1 => physics[Param::KnockoutRebound1],
            2 => physics[Param::KnockoutRebound2],
            _ => Fixed::ZERO,
        };
        if rebound > Fixed::ZERO {
            self.vel.y = -rebound;
            self.ground = None;
        } else {
            self.vel = Vec2::default();
            events.extend(landed);
            knockout.rest = Some(frames(physics[Param::KnockoutRest]).saturating_sub(1));
        }
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
        if self.frozen > 0 {
            self.frozen -= 1;
            return;
        }
        self.swap = self.swap.saturating_sub(1);
        // While swapping, nothing is pressed or held.
        let input = if self.swap > 0 {
            &Frame::default()
        } else {
            input
        };
        let held = |a| input.held.contains(a);
        let pressed = |a| input.pressed.contains(a);
        let held_dir = match (held(Action::Left), held(Action::Right)) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        let timed = physics[Param::RollTime] > Fixed::ZERO;
        let start_vx = self.vel.x;

        // A press waits out the jump delay in the buffer, however short
        // the buffer is.
        let delay = frames(physics[Param::JumpDelay]);
        self.jump_buffer
            .set_window(frames(physics[Param::JumpBuffer]).max(delay));
        self.jump_buffer.update(pressed(Action::Jump));
        if pressed(Action::Jump) {
            self.jump_needs_hold = false;
        }
        self.jump_held_since_press =
            pressed(Action::Jump) || (self.jump_held_since_press && held(Action::Jump));
        self.jump_hold &= held(Action::Jump);
        if self.takeoff {
            // The jump that started on the ground last frame leaves it.
            self.takeoff = false;
            self.ground = None;
            self.coyote = None;
            self.state = State::Jump;
        }
        // The ground action under way goes on to its next frame. Its frame
        // after the last is over, but still moves by its rules.
        let action_time = frames(physics[Param::GroundActionTime]);
        let mut acting = self.ground_action.take().map(|a| GroundAction {
            frame: a.frame.saturating_add(1),
            ..a
        });
        if acting.is_some() {
            // Nothing is picked up during it.
            self.pick = None;
        }
        let busy = self.use_hands(input, physics, events);
        if busy.no_jump {
            self.jump_buffer.take();
        }
        let locked = self.lockout > 0;
        self.lockout = self.lockout.saturating_sub(1);

        // The way the steep face the player starts the frame on falls, if
        // any. Rolls on faces aren't in the spec yet.
        let slope = physics[Param::FaceSlope];
        let face = if slope > Fixed::ZERO && self.grounded() && self.state != State::Roll {
            columns::face(level, columns::column(self.pos.x), self.pos.y, slope)
        } else {
            None
        };
        // A direction held up the face, or back up one just come off,
        // counts as none.
        let downhill = face.or(self.off_face.map(|(way, _)| direction(way)));
        let dir = if downhill == Some(-held_dir) {
            0
        } else {
            held_dir
        };

        if timed {
            self.remember_roll_press(input, physics);
            self.roll_after_throw(input, physics);
        }
        if busy.no_roll || acting.is_some() {
            self.roll_press = None;
        }

        // With the other character in the level, swaps are the world's.
        if self.grounded()
            && self.state != State::Roll
            && acting.is_none()
            && pressed(Action::Swap)
            && feel.roster.len() > 1
            && feel.world[Param::SwapMoves] == Fixed::ZERO
        {
            self.character = (self.character + 1) % feel.roster.len();
            self.swap = SWAP_FRAMES;
            self.jump_buffer.take();
        }

        // A jump pressed during a timed roll can start wherever the roll
        // has taken the player, in the air too.
        let roll_jump = std::mem::take(&mut self.roll_jump) || self.roll.is_some();
        let ready = self
            .jump_buffer
            .age()
            .is_some_and(|age| age >= delay.saturating_add(busy.jump_delay));
        let can_jump = self.swap == 0
            && !locked
            && face.is_none()
            && (self.grounded() || self.coyote.is_some() || roll_jump)
            && ready;
        if face.is_some() && ready {
            // No jump starts on a face: the press is treated as one made
            // in the air.
            self.jump_needs_hold = physics[Param::JumpBufferHold] > Fixed::ZERO;
        }
        let takeoff_frame = physics[Param::GroundedTakeoff] > Fixed::ZERO;
        let mut air_takeoff = None;
        if can_jump && self.jump_buffer.take() {
            if self.state == State::Roll {
                events.push(Event::RollEnd);
            }
            self.end_roll();
            let mut speed = -physics[Param::JumpSpeed];
            if !held(Action::Jump) {
                // A buffered tap that's already over makes a short hop.
                speed = speed.max(-physics[Param::JumpReleaseSpeed]);
            }
            self.jump_hold = self.jump_held_since_press;
            if takeoff_frame && self.grounded() {
                self.vel.y = speed;
                self.takeoff = true;
            } else if takeoff_frame && roll_jump {
                // Out of a roll in the air, the jump spends its first
                // frame moving as before, and rises from the end of it.
                air_takeoff = Some(speed);
            } else {
                self.vel.y = speed;
                self.ground = None;
            }
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
        // A jump pressed during a timed roll ends it on this frame.
        if self.roll.is_some() && pressed(Action::Jump) {
            self.roll_jump = true;
        }
        // So does one pressed during the ground action, which still moves
        // by its rules on this frame.
        let mut goes_on = acting.is_some_and(|a| a.frame < action_time) && !pressed(Action::Jump);
        if acting.is_none()
            && action_time > 0
            && pressed(Action::Action)
            && held(Action::Down)
            && self.grounded()
            && self.swap == 0
            && !busy.no_roll
            && self.roll.is_none()
            && self.roll_stop == 0
            && !events.contains(&Event::JumpStart)
        {
            // It goes the way held now, whatever the player was doing.
            acting = Some(GroundAction {
                frame: 0,
                dir: held_dir,
            });
            goes_on = true;
            self.vel.x = Fixed::ZERO;
        }

        let roll_input = held(Action::Down)
            && held(Action::Action)
            && (pressed(Action::Down) || pressed(Action::Action));
        if !timed
            && roll_input
            && !busy.no_roll
            && self.grounded()
            && self.swap == 0
            && self.state != State::Roll
            && self.vel.x.abs() >= physics[Param::RollMinSpeed]
            && feel.roster[self.character].abilities.contains_key(ROLL)
        {
            self.state = State::Roll;
            events.push(Event::RollStart);
        }
        let can_roll = timed
            && !busy.no_roll
            && !events.contains(&Event::JumpStart)
            && feel.roster[self.character].abilities.contains_key(ROLL);
        let boost = if can_roll {
            self.use_boost(held(Action::Action), physics)
        } else {
            None
        };
        let boosted = boost.is_some();
        let started = can_roll && !boosted && self.start_timed_roll(input, physics);
        if started || boost == Some(true) {
            self.state = State::Roll;
            events.push(Event::RollStart);
            // A roll ends the throw it starts in.
            self.hands = None;
        }
        let rolling = self.state == State::Roll;

        // The ground action stands the player up, and down held during it
        // only counts from the frame after it's over.
        let crouching = if acting.is_some() {
            self.crouch = Crouch::default();
            false
        } else {
            self.crouch.update(
                held(Action::Down) && !busy.no_crouch,
                self.grounded() && !rolling,
                frames(physics[Param::CrouchStart]),
                frames(physics[Param::CrouchMin]),
                frames(physics[Param::CrouchRecovery]),
            )
        };

        let turn_time = frames(physics[Param::TurnTime]);
        let slide = frames(physics[Param::RollStopSlide]);
        // With a stride cycle, whether a turn starts is decided once the
        // frame's speed is known.
        let striding = physics[Param::StrideSteps] >= Fixed::ONE
            && turn_time > 0
            && !busy.keep_facing
            && acting.is_none()
            && self.grounded()
            && self.roll.is_none()
            && !rolling
            && self.roll_stop == 0;
        let turn_ends = self.turn.is_some_and(|n| n + 1 >= turn_time);
        let facing = if acting.is_some() {
            self.turn = None;
            self.facing
        } else if let Some(mut roll) = self.roll {
            if !started {
                roll.steer(dir, boosted, physics);
            }
            self.roll = Some(roll);
            self.turn = None;
            roll.dir
        } else if rolling {
            // A roll keeps facing the way it goes.
            self.turn = None;
            if self.vel.x.is_negative() {
                Facing::Left
            } else {
                Facing::Right
            }
        } else if busy.keep_facing {
            self.turn = None;
            self.facing
        } else if self.roll_stop > slide {
            // Standing still in the stop after a roll, the player turns
            // at once.
            self.turn = None;
            facing_toward(dir, self.facing)
        } else if turn_time > 0 && self.grounded() {
            self.turn_on_ground(dir, turn_time, !striding)
        } else {
            self.turn = None;
            facing_toward(dir, self.facing)
        };
        if facing != self.facing {
            self.facing = facing;
            events.push(Event::Turn);
        }
        // A crouch holds the player where they are, and so does ground
        // ahead that's too high to start walking toward.
        let run_dir = dir;
        let ahead = physics[Param::StartRiseAhead].floor();
        let walled = ahead > 0
            && dir != 0
            && start_vx == Fixed::ZERO
            && self.grounded()
            && !rolling
            && !events.contains(&Event::JumpStart)
            && columns::rise_ahead(level, columns::column(self.pos.x), dir, ahead, self.pos.y);
        let dir = if crouching || walled || busy.still {
            0
        } else {
            dir
        };

        let cap = if held(Action::Action) {
            physics[Param::TopSpeed]
        } else {
            physics[Param::WalkSpeed]
        };
        let response = physics[Param::SpeedResponse];
        self.target = Fixed::ZERO;
        self.vel.x = if let Some(action) = acting {
            self.target = physics[Param::GroundActionSpeed] * Fixed::from_int(action.dir);
            if action.frame < action_time {
                ease(self.vel.x, self.target, response)
            } else {
                self.vel.x
            }
        } else if let Some(roll) = self.roll {
            if roll.still {
                Fixed::ZERO
            } else {
                self.target = roll.target();
                ease(self.vel.x, roll.target(), response)
            }
        } else if rolling {
            let vx = roll_speed(self.vel.x, dir, physics);
            self.target = vx;
            vx
        } else if self.roll_stop > 0 {
            if self.roll_stop <= slide {
                ease(self.vel.x, Fixed::ZERO, response)
            } else {
                Fixed::ZERO
            }
        } else if response > Fixed::ZERO {
            let mut target = match dir {
                -1 => -cap,
                1 => cap,
                _ => Fixed::ZERO,
            };
            match face {
                Some(-1) => target -= physics[Param::FaceSpeed],
                Some(_) => target += physics[Param::FaceSpeed],
                None => {}
            }
            self.target = target;
            ease(self.vel.x, target, response)
        } else {
            self.target = Fixed::from_int(dir) * cap;
            horizontal_speed(self.vel.x, dir, self.grounded(), cap, physics)
        };
        if !striding {
            // Where the stride cycle starts after a landing, a roll or its
            // stop isn't in the spec; the player starts out standing.
            self.gait = Gait::Standing;
        } else if self.stride(run_dir, held(Action::Action), turn_ends, physics) {
            self.facing = facing_of(-direction(self.facing));
            events.push(Event::Turn);
        }
        let airborne = !self.grounded();
        if let Some(n) = self.hop.filter(|_| airborne) {
            // The hop has its own speeds, whatever jump does.
            let n = n.saturating_add(1);
            self.hop = Some(n);
            let vy = if n == 3 {
                -physics[Param::TakeoverHop3]
            } else {
                self.vel.y + physics[Param::Gravity]
            };
            self.vel.y = vy.min(physics[Param::MaxFallSpeed]);
        } else if airborne {
            let held_gravity = physics[Param::HeldJumpGravity];
            // A jump pressed during a roll in the air falls under the
            // held gravity from its press.
            let jump_hold = self.jump_hold || (self.roll_jump && self.jump_held_since_press);
            let gravity = if jump_hold && held_gravity > Fixed::ZERO {
                held_gravity
            } else if self.vel.y.is_negative() {
                physics[Param::Gravity]
            } else {
                physics[Param::FallGravity]
            };
            self.vel.y = (self.vel.y + gravity).min(physics[Param::MaxFallSpeed]);
        }
        let hold = self.jump_hold;
        let feet = self.pos.y;
        let moved = if slope > Fixed::ZERO && airborne {
            self.move_in_air_by_columns(level, slope, events)
        } else if slope > Fixed::ZERO {
            self.move_by_columns(level, slope)
        } else {
            self.move_horizontally(level, HALF_WIDTH, |_| true)
        };
        if self.roll_stop > 0 && self.roll_stop == slide {
            // The slide's last frame moves, but ends standing.
            self.vel.x = Fixed::ZERO;
        }

        if !airborne {
            let drop = physics[Param::LeaveDrop];
            // By columns, the floor is the one under the middle of the
            // player's column.
            let x = if slope > Fixed::ZERO {
                columns::middle(columns::column(self.pos.x))
            } else {
                self.pos.x
            };
            if self.follow_ground(level, x, events) && drop > Fixed::ZERO {
                // There's no ground within a step below, so the drop
                // can't land.
                self.vel.y = drop;
                self.move_vertically(level, moved, Fixed::ZERO, slope > Fixed::ZERO, events);
                self.vel.y = physics[Param::LeaveFallSpeed];
            }
        } else {
            // Stepping onto a column while falling has landed already.
            if !self.grounded() {
                self.move_vertically(
                    level,
                    moved,
                    physics[Param::LandingReach],
                    slope > Fixed::ZERO,
                    events,
                );
            }
            if let Some(speed) = air_takeoff {
                // Landing on the take-off frame takes off from the ground
                // on the next one.
                self.jump_hold = hold;
                self.vel.y = speed;
                self.takeoff = self.grounded();
            }
        }

        // Inside a platform from a frame that rises into its layer from
        // below its underside, until the feet leave the layer.
        self.inside = airborne
            && slope > Fixed::ZERO
            && !self.grounded()
            && columns::layer(level, columns::column(self.pos.x), self.pos.y)
                .is_some_and(|p| self.inside || feet > p.bottom);

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
        self.off_face = if !self.grounded() {
            None
        } else if let Some(way) = face {
            let exit = frames(physics[Param::FaceExit]);
            (exit > 0).then_some((facing_of(way), exit))
        } else {
            self.off_face
                .and_then(|(way, n)| (n > 1).then_some((way, n - 1)))
        };
        // A press on a frame that ends on the ground, landing included,
        // jumps when it can whether or not jump is still held.
        if self.jump_buffer.age() == Some(0) {
            self.jump_needs_hold = physics[Param::JumpBufferHold] > Fixed::ZERO && !self.grounded();
        }

        if timed {
            // Running frames before a roll make it fast; the roll's own
            // frames don't count.
            if self.roll.is_none() && acting.is_none() {
                if held(Action::Action) && run_dir != 0 {
                    self.lone_run = self.since_run != Some(1) && start_vx == Fixed::ZERO;
                    self.since_run = Some(0);
                } else if held(Action::Left) || held(Action::Right) {
                    self.lone_run = false;
                }
            }
            self.advance_timed_roll(held(Action::Action), physics, events);
        } else if rolling && (!self.grounded() || self.vel.x.abs() < physics[Param::RollMinSpeed]) {
            events.push(Event::RollEnd);
        }
        if self.roll_stop == 0 {
            self.stopped = None;
        }
        self.jump_age = if events.contains(&Event::JumpStart) {
            Some(0)
        } else if self.grounded() && !self.takeoff {
            None
        } else {
            self.jump_age.map(|n| n.saturating_add(1))
        };
        if self.grounded() {
            self.hop = None;
        }
        // Going off a ledge ends the ground action.
        self.ground_action = acting.filter(|_| goes_on && self.grounded());
        self.state = self.next_state(held(Action::Action), events);
    }

    /// One step of a character lost on a hit, on its way out of the
    /// level, under the physics of `character`'s own: it flies straight
    /// up and down under `gravity` until it lands, holds still on the
    /// frame after, then runs right at `lose_run_speed`, following the
    /// ground and falling off ledges.
    pub(crate) fn run_off(
        &mut self,
        level: &Level,
        physics: &Physics,
        lost: &mut Lost,
        events: &mut Vec<Event>,
    ) {
        self.touched.clear();
        let slope = physics[Param::FaceSlope];
        let by_columns = slope > Fixed::ZERO;
        match lost.landed {
            Some(0) => {
                lost.landed = Some(1);
                self.vel = Vec2::default();
                self.state = State::Idle;
                return;
            }
            Some(_) => self.vel.x = physics[Param::LoseRunSpeed],
            None => {
                self.vel.x = Fixed::ZERO;
                self.ground = None;
            }
        }
        if self.grounded() {
            self.state = State::Run;
            let x = if by_columns {
                self.move_by_columns(level, slope);
                columns::middle(columns::column(self.pos.x))
            } else {
                self.move_horizontally(level, HALF_WIDTH, |_| true);
                self.pos.x
            };
            self.follow_ground(level, x, events);
            return;
        }
        self.vel.y = (self.vel.y + lost.gravity).min(physics[Param::MaxFallSpeed]);
        let moved = self.move_horizontally(level, HALF_WIDTH, |_| true);
        self.move_vertically(
            level,
            moved,
            physics[Param::LandingReach],
            by_columns,
            events,
        );
        if self.grounded() && lost.landed.is_none() {
            lost.landed = Some(0);
        }
    }

    /// Goes on with what the hands are doing at the start of a step, and
    /// says what that rules out on it. A pick-up starts with the object
    /// in [`Player::pick`] if action is held. Letting go of action throws
    /// the object, or puts it down if the player is carrying it on the
    /// ground with down held.
    fn use_hands(&mut self, input: &Frame, physics: &Physics, events: &mut Vec<Event>) -> Busy {
        let held = |a| input.held.contains(a);
        let time = |param| frames(physics[param]);
        if let Some(hands) = &mut self.hands {
            hands.frame = hands.frame.saturating_add(1);
            let over = match hands.act {
                Act::PickUp if hands.frame >= time(Param::PickUpTime) => {
                    hands.act = Act::Carry;
                    false
                }
                Act::Throw => hands.frame >= time(Param::ThrowTime),
                Act::PutDown => hands.frame >= time(Param::PutDownTime),
                _ => false,
            };
            if over {
                self.hands = None;
            }
        }
        if let Some(thing) = self.pick.take()
            && self.hands.is_none()
            && self.grounded()
            && held(Action::Action)
        {
            // A roll into the object ends.
            if self.state == State::Roll {
                events.push(Event::RollEnd);
            }
            self.end_roll();
            self.turn = None;
            self.hands = Some(Hands::new(thing, Act::PickUp));
        }
        if let Some(hands) = self.hands
            && matches!(hands.act, Act::PickUp | Act::Carry)
            && !held(Action::Action)
        {
            let down = hands.act == Act::Carry && self.grounded() && held(Action::Down);
            let act = if down { Act::PutDown } else { Act::Throw };
            self.hands = Some(Hands::new(hands.thing, act));
        }
        let pressed = input.pressed.contains(Action::Action);
        let throw_still = time(Param::ThrowStill);
        if self.hands.is_some_and(|h| {
            h.act == Act::Throw && h.frame >= throw_still && input.pressed.contains(Action::Jump)
        }) {
            // A jump ends the throw.
            self.hands = None;
        }
        let Some(hands) = &mut self.hands else {
            return Busy::default();
        };
        match hands.act {
            Act::PickUp => Busy {
                still: hands.frame < time(Param::PickUpStill),
                keep_facing: true,
                no_jump: true,
                no_roll: true,
                no_crouch: true,
                jump_delay: 0,
            },
            Act::Carry => Busy {
                no_roll: true,
                no_crouch: true,
                jump_delay: time(Param::CarryJumpDelay),
                ..Busy::default()
            },
            Act::Throw => {
                let early = hands.frame < throw_still;
                if pressed && early {
                    hands.press = Some(hands.frame);
                }
                Busy {
                    still: early,
                    keep_facing: true,
                    no_jump: early,
                    no_roll: early,
                    ..Busy::default()
                }
            }
            Act::PutDown => {
                if pressed && hands.frame >= time(Param::PutDownRegrab) {
                    hands.press = Some(hands.frame);
                }
                // Jumps and rolls during a put-down aren't in the spec;
                // there are none.
                Busy {
                    still: true,
                    keep_facing: true,
                    no_jump: true,
                    no_roll: true,
                    ..Busy::default()
                }
            }
        }
    }

    /// On the first frame of a throw that a roll can start on, an action
    /// press made from `throw_roll_press` on and still held starts one.
    fn roll_after_throw(&mut self, input: &Frame, physics: &Physics) {
        let Some(hands) = self.hands else {
            return;
        };
        let from = frames(physics[Param::ThrowRollPress]);
        if hands.act == Act::Throw
            && hands.frame == frames(physics[Param::ThrowStill])
            && input.held.contains(Action::Action)
            && hands.press.is_some_and(|n| n >= from)
            && self.roll_press.is_none()
        {
            self.roll_press = Some((0, 0, false));
        }
    }

    /// Remembers an action press for starting a timed roll, and forgets
    /// one that has waited too long.
    fn remember_roll_press(&mut self, input: &Frame, physics: &Physics) {
        let held = |a| input.held.contains(a);
        self.since_run = self.since_run.map(|n| n.saturating_add(1));
        let press = if !input.pressed.contains(Action::Action) {
            self.roll_press
                .map(|(age, window, hold)| (age.saturating_add(1), window, hold))
        } else if !self.grounded() {
            Some((0, frames(physics[Param::RollAirBuffer]), true))
        } else if held(Action::Down) && physics[Param::GroundActionTime] > Fixed::ZERO {
            // Down and action on the ground start the ground action
            // instead.
            None
        } else if self.roll.is_some() || self.roll_stop > 0 {
            Some((0, frames(physics[Param::RollBuffer]), false))
        } else {
            Some((0, 0, false))
        };
        self.roll_press =
            press.filter(|&(age, window, hold)| age <= window && (!hold || held(Action::Action)));
    }

    /// Starts a timed roll from a remembered press if one can start now.
    /// Returns whether it did.
    fn start_timed_roll(&mut self, input: &Frame, physics: &Physics) -> bool {
        let Some((age, _, _)) = self.roll_press else {
            return false;
        };
        let stop_time = frames(physics[Param::RollStopTime]);
        let can_start = self.grounded()
            && self.swap == 0
            && self.roll.is_none()
            && (self.roll_stop == 0 || self.roll_stop == stop_time);
        if !can_start {
            return false;
        }
        let dir_held = input.held.contains(Action::Left) || input.held.contains(Action::Right);
        let window = frames(
            physics[if dir_held {
                Param::RollFastWindow
            } else {
                Param::RollFastWindowNeutral
            }],
        );
        let fast = self.since_run.is_some_and(|n| n <= window) && (dir_held || !self.lone_run);
        let size = if fast {
            RollSize::Fast
        } else if dir_held {
            RollSize::Normal
        } else {
            RollSize::Slow
        };
        // A roll starting on the stop's last frame stands still on it, and
        // lasts a frame longer if its press was made earlier.
        let still = self.roll_stop > 0;
        let length = frames(physics[Param::RollTime]).saturating_add(u8::from(still && age > 0));
        self.roll = Some(Roll {
            left: length,
            dir: self.facing,
            size,
            speed: size.speed(physics),
            boosts: 0,
            defeated: false,
            held: true,
            carry: false,
            still,
        });
        self.roll_press = None;
        self.roll_stop = 0;
        true
    }

    /// Uses the boost waiting after a defeat if action is held, starting
    /// the roll over, or a new one in the stop. `None` if there was no
    /// boost to use; otherwise whether a new roll started. A boost not
    /// used by the end of the roll or its stop is lost.
    fn use_boost(&mut self, action: bool, physics: &Physics) -> Option<bool> {
        let boost = self.boost?;
        if self.roll.is_none() && self.roll_stop == 0 {
            self.boost = None;
            return None;
        }
        if !action || physics[Param::RollBoost] <= Fixed::ZERO {
            return None;
        }
        self.boost = None;
        let speed = boost.size.speed(physics)
            + physics[Param::RollBoost] * Fixed::from_int(boost.boosts.into());
        let left = frames(physics[Param::RollTime]);
        if let Some(roll) = &mut self.roll {
            roll.left = left;
            roll.speed = speed;
            roll.boosts = boost.boosts;
            roll.carry = false;
            return Some(false);
        }
        self.roll = Some(Roll {
            left,
            dir: boost.dir,
            size: boost.size,
            speed,
            boosts: boost.boosts,
            defeated: true,
            held: true,
            carry: false,
            still: false,
        });
        self.roll_stop = 0;
        self.stopped = None;
        self.roll_press = None;
        Some(true)
    }

    /// Counts down the timed roll at the end of a frame, ending it or
    /// lengthening it as it runs out, and goes on with the stop after one.
    fn advance_timed_roll(&mut self, action: bool, physics: &Physics, events: &mut Vec<Event>) {
        let stop_time = frames(physics[Param::RollStopTime]);
        if self.roll_stop > 0 {
            self.roll_stop = if self.roll_stop >= stop_time || !self.grounded() {
                0
            } else {
                self.roll_stop + 1
            };
        }
        let Some(mut roll) = self.roll.take() else {
            return;
        };
        roll.held &= action;
        roll.still = false;
        let carry_time = frames(physics[Param::RollCarryTime]);
        let end = if self.roll_jump {
            true
        } else if events.contains(&Event::Land) {
            // A roll off a ledge ends as if it started just after landing.
            roll.left = frames(physics[Param::RollTime]);
            false
        } else if !self.grounded() || roll.left > 0 {
            // It doesn't end in the air.
            roll.left = roll.left.saturating_sub(1);
            false
        } else if !roll.carry
            && (roll.defeated || (roll.held && roll.size != RollSize::Fast))
            && carry_time > 0
        {
            roll.carry = true;
            roll.left = carry_time - 1;
            false
        } else {
            if !roll.carry && stop_time > 0 {
                self.roll_stop = 1;
                self.stopped = Some(roll);
            }
            true
        };
        if end {
            events.push(Event::RollEnd);
        } else {
            self.roll = Some(roll);
        }
    }

    /// Goes on with the turn under way, or, if `start`, starts one if
    /// `dir` points behind the player, and returns the facing for this
    /// frame.
    fn turn_on_ground(&mut self, dir: i32, turn_time: u8, start: bool) -> Facing {
        let mut facing = self.facing;
        self.turn = self.turn.map(|n| n + 1).filter(|&n| n < turn_time);
        if start && self.turn.is_none() && dir == -direction(facing) {
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

    /// Goes on through the stride cycle once the frame's speed is known,
    /// holding `dir`, and starts a turn when one can start. `turn_ends`
    /// says the turn under way ended this frame. Returns whether facing
    /// changes now, which only a turn shorter than 2 frames does.
    fn stride(&mut self, dir: i32, action: bool, turn_ends: bool, physics: &Physics) -> bool {
        if self.turn.is_some() {
            return false;
        }
        let forward = direction(self.facing);
        let opposite = dir == -forward;
        let v = self.vel.x.abs();
        let steps = physics[Param::StrideSteps]
            .floor()
            .clamp(1, MAX_STEP.into()) as u32;
        let has =
            |set: Param, step: u32| clamber_pack::physics::steps(physics[set]).any(|n| n == step);
        let mut turn = false;
        self.gait = match self.gait {
            Gait::Walking(mut stride) if !turn_ends => {
                let rate =
                    stride_rate(v, physics[Param::WalkStride]) - physics[Param::WalkStrideLess];
                let ended = stride.advance(rate.max(physics[Param::StrideMin]), steps);
                let checked = ended.is_some_and(|n| has(Param::WalkTurnSteps, n));
                if checked && opposite {
                    turn = true;
                    Gait::Standing
                } else if checked && v < physics[Param::WalkStandSpeed] && dir == 0 {
                    Gait::Standing
                } else if checked && v < physics[Param::WalkStandSpeed] && dir == forward {
                    Gait::Walking(Stride::START)
                } else if ended.is_some_and(|n| has(Param::WalkRunSteps, n)) && action && dir != 0 {
                    Gait::Running(Stride::START)
                } else {
                    Gait::Walking(stride)
                }
            }
            Gait::Running(mut stride) if !turn_ends => {
                let rate = stride_rate(v, physics[Param::RunStride]);
                let ended = stride.advance(rate.max(physics[Param::StrideMin]), steps);
                let checked = ended.is_some_and(|n| has(Param::RunTurnSteps, n));
                if checked && opposite {
                    turn = true;
                    Gait::Standing
                } else if checked && v < physics[Param::RunStandSpeed] && dir == 0 {
                    Gait::Standing
                } else if !action
                    && dir != 0
                    && v < physics[Param::RunWalkSpeed]
                    && stride.first_done
                {
                    Gait::Walking(Stride::START)
                } else {
                    Gait::Running(stride)
                }
            }
            // Standing, or at the end of a turn.
            _ => {
                if opposite {
                    turn = true;
                    Gait::Standing
                } else if dir == 0 {
                    Gait::Standing
                } else if turn_ends && action && v >= physics[Param::TurnRunSpeed] {
                    Gait::Running(Stride::START)
                } else {
                    Gait::Walking(Stride::START)
                }
            }
        };
        if !turn {
            return false;
        }
        self.turn = Some(0);
        frames(physics[Param::TurnTime]) / 2 == 0
    }

    /// The state after this step's movement.
    fn next_state(&self, running: bool, events: &[Event]) -> State {
        if self.swap > 0 || self.hop.is_some() {
            State::Swap
        } else if self.ground_action.is_some() {
            State::GroundAction
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

    /// Moves a grounded body sideways column by column, as far as the
    /// ground allows, and returns how far. A column it can't go into
    /// stops it at the left edge of the column it was in.
    fn move_by_columns(&mut self, level: &Level, slope: Fixed) -> Fixed {
        if self.vel.x == Fixed::ZERO {
            return Fixed::ZERO;
        }
        let dir = if self.vel.x.is_negative() { -1 } else { 1 };
        let from = columns::column(self.pos.x);
        let to = columns::column(self.pos.x + self.vel.x);
        let mut next = from;
        while next != to {
            next += i64::from(dir);
            if let Some(collider) = columns::blocks(level, next, dir, self.pos.y, slope) {
                let edge = columns::left_edge(from);
                let moved = (self.pos.x - edge).abs();
                self.pos.x = edge;
                self.vel.x = Fixed::ZERO;
                self.touched.push(collider);
                return moved;
            }
        }
        // Walls that aren't ground, such as segments, still stop the
        // feet.
        self.move_horizontally(level, Fixed::ZERO, |_| true)
    }

    /// Moves an airborne body sideways by columns, after the frame's
    /// speeds are worked out, and returns how far. Only the column it
    /// would move into counts. Ground there higher than the feet stops
    /// the body at the left edge of its own column if it's a face column
    /// or more than a step higher; otherwise the body is lifted onto it
    /// by whole pixels, or lands on it if falling. Ground whose underside
    /// is above the feet doesn't count.
    ///
    /// Inside a one-way platform (see [`Player::inside`]), the columns
    /// whose layer, between underside and top, holds the feet don't stop
    /// or lift the body, except that rising with the feet up to
    /// [`PLATFORM_LIFT`] below the top lifts it level with the top or
    /// above by whole pixels.
    fn move_in_air_by_columns(
        &mut self,
        level: &Level,
        slope: Fixed,
        events: &mut Vec<Event>,
    ) -> Fixed {
        let from = columns::column(self.pos.x);
        let to = columns::column(self.pos.x + self.vel.x);
        let feet = self.pos.y;
        if self.inside
            && let Some(platform) = columns::layer(level, to, feet)
        {
            let below = feet - platform.top;
            if self.vel.y.is_negative() && below <= PLATFORM_LIFT {
                self.pos.y -= Fixed::from_int(ceil(below) as i32);
            }
            return self.move_horizontally(level, Fixed::ZERO, only_segments(level));
        }
        let floor = (to != from)
            .then(|| columns::floor_in_air(level, to, feet))
            .flatten()
            .filter(|f| f.y < feet);
        if let Some(floor) = floor {
            let rise = feet - floor.y;
            if rise > STEP || columns::face(level, to, feet, slope).is_some() {
                let edge = columns::left_edge(from);
                let moved = (self.pos.x - edge).abs();
                self.pos.x = edge;
                self.vel.x = Fixed::ZERO;
                self.touched.push(floor.collider);
                return moved;
            }
            // A frame with no vertical speed isn't in the spec; it's
            // lifted as if rising.
            if self.vel.y > Fixed::ZERO {
                self.land(floor.y, floor.collider, events);
            } else {
                // At most a step, so it fits.
                self.pos.y -= Fixed::from_int(ceil(rise) as i32);
            }
        }
        // Walls that aren't ground, such as segments, still stop the
        // feet.
        self.move_horizontally(level, Fixed::ZERO, only_segments(level))
    }

    /// Moves sideways as far as walls that `keep` accepts allow a body
    /// `half_width` either side of its position, and returns how far.
    fn move_horizontally(
        &mut self,
        level: &Level,
        half_width: Fixed,
        keep: impl Fn(&Hit) -> bool,
    ) -> Fixed {
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
            .sweep(from, cast_dir, distance + half_width, reach, |hit| {
                is_wall(hit) && keep(hit)
            });
        let moved = match wall.map(|hit| (hit.distance - half_width, hit.collider)) {
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

    /// Keeps a grounded body on the ground under `x` as it moves over
    /// slopes and bumps, or lets it leave the ground when there's none
    /// within reach. A body taking off keeps its upward speed. Returns
    /// whether the body walked off the ground.
    fn follow_ground(&mut self, level: &Level, x: Fixed, events: &mut Vec<Event>) -> bool {
        let from = Vec2::new(x, self.pos.y - STEP);
        match level.collision.cast(from, Dir::Down, STEP + STEP) {
            Some(hit) => {
                self.pos.y = hit.point.y;
                if !self.takeoff {
                    self.vel.y = Fixed::ZERO;
                }
                self.ground = Some(hit.collider);
                self.touched.push(hit.collider);
                false
            }
            None => {
                self.ground = None;
                if !self.takeoff {
                    events.push(Event::LeaveGround);
                }
                !self.takeoff
            }
        }
    }

    /// Moves up or down as far as the level allows, after moving `moved`
    /// sideways. Falling, the body lands if it ends less than `reach`
    /// above the ground. `by_columns`, rising is stopped by the feet, not
    /// the head (see [`Player::rise_by_columns`]), and a fall that ends
    /// below a one-way platform's underside doesn't land on its top.
    fn move_vertically(
        &mut self,
        level: &Level,
        moved: Fixed,
        reach: Fixed,
        by_columns: bool,
        events: &mut Vec<Event>,
    ) {
        if self.vel.y.is_negative() && by_columns {
            self.rise_by_columns(level);
            return;
        }
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
        // Only a body moving down lands short of the ground.
        let reach = if self.vel.y > Fixed::ZERO {
            reach
        } else {
            Fixed::ZERO
        };
        let to = feet + self.vel.y;
        let spans = if by_columns {
            level.collision.spans(self.pos.x)
        } else {
            Vec::new()
        };
        // Thin platforms, segments, catch any fall onto them.
        let below_underside = |hit: &Hit| {
            !only_segments(level)(hit)
                && spans.iter().any(|s| {
                    s.collider == hit.collider
                        && s.top <= hit.point.y
                        && hit.point.y <= s.bottom
                        && to > s.bottom
                })
        };
        let ground = level.collision.sweep(
            from,
            Dir::Down,
            above + self.vel.y + reach,
            (Fixed::ZERO, Fixed::ZERO),
            |hit| !hit.one_way || (hit.point.y >= feet && !below_underside(hit)),
        );
        match ground {
            Some(hit) if hit.point.y <= to || hit.point.y - to < reach => {
                self.land(hit.point.y, hit.collider, events);
            }
            _ => self.pos.y += self.vel.y,
        }
    }

    /// Rises by the frame's speed, by columns. Nothing above the head
    /// stops the body, but ground that isn't one-way stops the feet: a
    /// rise from below the ground's underside that would end with the
    /// feet line (the feet rounded down the screen to a whole pixel) in
    /// it, below its top and level with its underside or above, leaves the
    /// feet line where it was and stops the rise.
    fn rise_by_columns(&mut self, level: &Level) {
        let to = self.pos.y + self.vel.y;
        let (before, after) = (ceil(self.pos.y), ceil(to));
        let column = columns::column(self.pos.x);
        let line = Fixed::from_int(after as i32);
        let stop = columns::spans(level, column)
            .into_iter()
            .find(|s| !s.one_way && self.pos.y > s.bottom && s.top < line && line <= s.bottom);
        self.pos.y = to;
        if let Some(ground) = stop {
            self.pos.y += Fixed::from_int((before - after) as i32);
            self.vel.y = Fixed::ZERO;
            self.touched.push(ground.collider);
        }
    }

    /// Lands with the feet at `y` on `collider`.
    fn land(&mut self, y: Fixed, collider: usize, events: &mut Vec<Event>) {
        self.pos.y = y;
        self.vel.y = Fixed::ZERO;
        self.ground = Some(collider);
        self.jump_hold = false;
        self.touched.push(collider);
        events.push(Event::Land);
    }
}

/// What the player's hands are doing with an object that can be
/// carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hands {
    /// The object's index among the things.
    pub thing: usize,
    pub act: Act,
    /// Frames since the act started, which is 0.
    pub frame: u8,
    /// During a throw or a put-down, the frame of the latest action press
    /// that may start a roll or pick the object up again.
    pub press: Option<u8>,
}

impl Hands {
    fn new(thing: usize, act: Act) -> Hands {
        Hands {
            thing,
            act,
            frame: 0,
            press: None,
        }
    }
}

/// Something done with an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Act {
    PickUp,
    /// Holding it after the pick-up.
    Carry,
    /// From letting go of action; the object leaves the hands part way.
    Throw,
    /// Setting it down in front.
    PutDown,
}

/// What the hands rule out on a step.
#[derive(Clone, Copy, Debug, Default)]
struct Busy {
    /// The target speed is 0 whatever is held.
    still: bool,
    /// Facing doesn't change and no turn starts.
    keep_facing: bool,
    /// Jump presses do nothing, now or later.
    no_jump: bool,
    no_roll: bool,
    /// Down doesn't crouch.
    no_crouch: bool,
    /// Frames more before a jump takes off.
    jump_delay: u8,
}

/// A knock-out under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Knockout {
    /// The enemy that hurt the player, if it was one.
    pub by: Option<usize>,
    /// Frames since the hit frame, which is 0.
    pub frame: u8,
    /// Landings so far in the flight.
    pub landings: u8,
    /// Once the flight has stopped, how many more frames the player lies
    /// still.
    pub rest: Option<u8>,
}

impl Knockout {
    /// A knock-out whose hit frame is this one.
    pub fn new(by: Option<usize>) -> Knockout {
        Knockout {
            by,
            frame: 0,
            landings: 0,
            rest: None,
        }
    }
}

/// The ground action under way. Enemies it runs into aren't knocked out
/// yet: the spec doesn't say which, or when they're touched (#103).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GroundAction {
    /// Frames since its press frame, which is 0.
    pub frame: u8,
    /// The way it goes: -1, 0 or 1.
    pub dir: i32,
}

/// A timed roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Roll {
    /// Frames left before the frame the roll ends on.
    pub left: u8,
    /// The way it goes.
    pub dir: Facing,
    /// Its size as it started, before steering or boosts.
    pub size: RollSize,
    /// The speed it eases toward, the way it goes.
    pub speed: Fixed,
    /// Boosts so far in this chain of rolls.
    pub boosts: u8,
    /// It defeated an enemy, so it carries on into running.
    pub defeated: bool,
    /// Action has been held on every frame since the roll started.
    pub held: bool,
    /// The roll has been lengthened to carry on into running.
    pub carry: bool,
    /// The roll doesn't move on this frame, its first.
    pub still: bool,
}

/// Which roll speed a timed roll eases toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RollSize {
    Slow,
    Normal,
    Fast,
}

/// A boost waiting to be used after a defeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Boost {
    /// The way the roll went.
    pub dir: Facing,
    /// The size the chain of rolls started at.
    pub size: RollSize,
    /// Boosts in the chain once this one is used.
    pub boosts: u8,
}

impl RollSize {
    /// The speed a roll of this size starts out easing toward.
    fn speed(self, physics: &Physics) -> Fixed {
        physics[match self {
            RollSize::Slow => Param::RollSlowSpeed,
            RollSize::Normal => Param::RollSpeed,
            RollSize::Fast => Param::RollFastSpeed,
        }]
    }
}

impl Roll {
    /// Holding the way the roll goes makes a slower roll normal, except
    /// on the frame it's boosted; holding the other way turns it around.
    fn steer(&mut self, dir: i32, boosted: bool, physics: &Physics) {
        let along = direction(self.dir);
        let normal = physics[Param::RollSpeed];
        if dir == along && !boosted && self.speed < normal {
            self.speed = normal;
        } else if dir == -along {
            self.dir = facing_toward(dir, self.dir);
        }
    }

    /// The speed the roll eases toward, signed.
    fn target(&self) -> Fixed {
        match self.dir {
            Facing::Left => -self.speed,
            Facing::Right => self.speed,
        }
    }
}

/// What a grounded player outside a turn is doing, which decides when a
/// turn can start (`stride_steps` above 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Gait {
    /// A turn starts as soon as the other way is held. The player may
    /// still be sliding.
    #[default]
    Standing,
    Walking(Stride),
    Running(Stride),
}

/// Progress through the stride cycle while walking or running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stride {
    /// The step under way, from 1.
    pub step: u32,
    /// How much of it is left, in steps.
    pub left: Fixed,
    /// The first step has ended.
    pub first_done: bool,
}

impl Stride {
    /// At the start of the cycle. Nothing more happens on the frame a
    /// stride starts.
    const START: Stride = Stride {
        step: 1,
        left: Fixed::ONE,
        first_done: false,
    };

    /// Goes on by `rate` steps in a cycle of `steps`, and returns the
    /// step that ended, if one did.
    fn advance(&mut self, rate: Fixed, steps: u32) -> Option<u32> {
        self.left -= rate;
        if self.left > Fixed::ZERO {
            return None;
        }
        self.left += Fixed::ONE;
        let ended = self.step;
        self.step = ended % steps + 1;
        self.first_done |= ended == 1;
        Some(ended)
    }
}

/// Stride progress per frame at speed `v` (not negative) for a stride
/// factor: each binary digit of `factor` takes its share of `v` rounded
/// down to a whole [`SPEED_GRAIN`] on its own.
fn stride_rate(v: Fixed, factor: Fixed) -> Fixed {
    let grain = SPEED_GRAIN.to_bits();
    let (v, factor) = (v.to_bits(), factor.to_bits());
    let total = (0..63)
        .filter(|bit| factor >> bit & 1 == 1)
        .map(|bit| {
            let share = if bit < FRAC_BITS {
                v >> (FRAC_BITS - bit)
            } else {
                v.checked_shl(bit - FRAC_BITS).unwrap_or(i64::MAX)
            };
            share / grain * grain
        })
        .fold(0i64, i64::saturating_add);
    Fixed::from_bits(total)
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
    /// Advances one frame, with `down` held or not and the player
    /// `on_ground` (not rolling) or not, and returns whether the player is
    /// crouching on this frame. On the ground, down counts as held until
    /// it has been for `min` frames.
    fn update(&mut self, down: bool, on_ground: bool, start: u8, min: u8, recovery: u8) -> bool {
        if start == 0 {
            *self = Crouch::default();
            return false;
        }
        if on_ground && (down || (self.held > 0 && self.held < min)) {
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

/// The facing for holding `dir` (-1, 0 or 1), or `facing` with none.
fn facing_toward(dir: i32, facing: Facing) -> Facing {
    match dir {
        -1 => Facing::Left,
        1 => Facing::Right,
        _ => facing,
    }
}

/// A whole number of frames from a parameter, clamped to what fits.
pub(crate) fn frames(value: Fixed) -> u8 {
    value.floor().clamp(0, u8::MAX.into()) as u8
}

/// How far below a one-way platform's top the feet can be, inside it,
/// for a rise to lift the body onto it, px.
const PLATFORM_LIFT: Fixed = Fixed::from_int(2);

/// The smallest whole number not less than `v`.
pub(crate) fn ceil(v: Fixed) -> i64 {
    -(-v).floor()
}

/// Accepts hits on segments only.
fn only_segments(level: &Level) -> impl Fn(&Hit) -> bool + '_ {
    |hit| {
        matches!(
            level.collision.colliders[hit.collider].shape,
            Shape::Segment { .. }
        )
    }
}

/// Edges steeper than 45 degrees block; shallower ones are walked on.
pub(crate) fn is_wall(hit: &Hit) -> bool {
    let (a, b) = hit.edge;
    (b.y - a.y).abs() > (b.x - a.x).abs()
}

/// -1 for left, 1 for right.
fn direction(facing: Facing) -> i32 {
    match facing {
        Facing::Left => -1,
        Facing::Right => 1,
    }
}

/// Left for a negative `dir`, right otherwise.
fn facing_of(dir: i32) -> Facing {
    if dir < 0 { Facing::Left } else { Facing::Right }
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
pub(crate) fn ease(v: Fixed, target: Fixed, response: Fixed) -> Fixed {
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
    fn stride_rates_round_each_share_down() {
        // spec/behavior/ground-speed.md, "The stride cycle", in 1/256
        // steps: walking is v/4 + v/8 - 32 and running v/8 + v/16 + v/32,
        // each share rounded down.
        let rate = |v: i32, factor: f64| {
            stride_rate(Fixed::from_ratio_pow2(v, 8), fx(factor)).to_f64() * 256.0
        };
        assert_eq!(rate(356, 0.375) - 32.0, 101.0);
        assert_eq!(rate(512, 0.375) - 32.0, 160.0);
        assert_eq!(rate(768, 0.21875), 168.0);
        assert_eq!(rate(7, 0.375), 1.0);
        assert_eq!(rate(0, 0.375), 0.0);
    }

    #[test]
    fn frame_counts_from_params() {
        assert_eq!(frames(fx(5.75)), 5);
        assert_eq!(frames(fx(-1.0)), 0);
        assert_eq!(frames(fx(1000.0)), 255);
    }
}
