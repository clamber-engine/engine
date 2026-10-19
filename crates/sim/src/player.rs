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

use crate::archetype::{Entrance, Spring};
use crate::columns;
use crate::loss::Lost;
use crate::pose::PoseClock;
use crate::ride::Riding;
use crate::rope::OnRope;
use crate::sling::Slung;
use crate::spring::{OnSpring, SpringArc};
use crate::swim::SwimTurn;
use crate::view;
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
    /// Swimming, inside a water region (see `swim`).
    Swim,
    /// Holding a rope (see [`rope`](crate::rope)).
    Hold,
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
            State::Swim => "swim",
            State::Hold => "rope",
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
    /// A press that gave no jump at a steep face still holds the fall to
    /// `held_jump_gravity` while jump stays held (`face_press_gravity`).
    pub face_press: bool,
    /// The last frame started on the ground, and not on a steep face.
    pub clear_start: bool,
    /// The buffered press was made on a frame that started on a steep
    /// face, or whose next frame did (`face_jump_wait`).
    pub carried_press: bool,
    /// The last frame moved onto a steep face from other floor with only
    /// its uphill direction held, so this one doesn't count as starting
    /// on the face for jumps (`face_jump_wait`).
    pub uphill_onto_face: bool,
    /// The jump started this frame on the ground and leaves it next frame.
    pub takeoff: bool,
    /// A jump has started and takes off this many frames from now
    /// (`takeoff_wait`).
    pub takeoff_wait: Option<u8>,
    /// The jump waiting to take off started while a crouch went on.
    pub takeoff_crouched: bool,
    /// The jump waiting to take off started during a timed roll
    /// (`roll_jump_held_gravity`).
    pub takeoff_rolled: bool,
    /// A jump pressed during the ground action ended it on the last
    /// frame, and takes off on this one even onto a face.
    pub action_jump: bool,
    /// The player left the ground during the ground action, and hasn't
    /// been on the ground since the frame after landing.
    pub action_fall: bool,
    /// Action was pressed with Jump on the ground on the last frame
    /// (`action_jump_press`).
    pub action_with_jump: bool,
    /// That press made a crouch on the last frame instead of a jump: Jump
    /// held on this one counts as a press.
    pub jump_again: bool,
    /// Frames of swapping left.
    pub swap: u8,
    /// Frames left that the player is held still after a bounce.
    pub frozen: u8,
    /// Crouching, or getting up from a crouch.
    pub crouch: Crouch,
    /// Frames since the turn under way began, if one is.
    pub turn: Option<u8>,
    /// The turn under way lasts a frame less (`short_turn_speed`).
    pub short_turn: bool,
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
    /// A timed roll ended in a stop in the air, and the frame after the
    /// landing hasn't ended yet (`roll_air_stop_wait`).
    pub air_stopped: bool,
    /// The ground action under way, if there is one.
    pub ground_action: Option<GroundAction>,
    /// A boost waiting for action to be held, after a roll or its stop
    /// defeated an enemy (`roll_boost` above 0).
    pub boost: Option<Boost>,
    /// A boost was used on this frame, its new roll's first.
    pub boosted: bool,
    /// An action press waiting until a timed roll can start: how many
    /// frames ago it was made, how many it waits at most, and whether
    /// action must stay held meanwhile.
    pub roll_press: Option<(u8, u8, bool)>,
    /// How many frames ago the player last ran, outside a roll.
    pub since_run: Option<u8>,
    /// That run was a single frame from standing still, and no direction
    /// has been held since.
    pub lone_run: bool,
    /// Action's holds, for the size of a timed roll (`roll_fast_hold`).
    pub holds: Holds,
    /// A jump pressed during a timed roll ended it, and starts on the
    /// next frame, in the air too.
    pub roll_jump: bool,
    /// The last frame started on a steep face: the target it leaves for
    /// a frame held toward a rise (`rise_keeps_target`), the held
    /// direction's without the face's push down it.
    pub face_target: Option<Fixed>,
    /// The last frame started on a steep face that falls this way
    /// (`face_pull`).
    pub after_face: Option<Facing>,
    /// Frames since the jump under way started, until it lands.
    pub jump_age: Option<u8>,
    /// The jump under way, or the last one, reached its approach frame
    /// this many frames after it started (`switch_from`).
    pub approach: Option<u8>,
    /// Its approach frame ended in a steep face's column.
    pub approach_face: bool,
    /// Added to the next frame's target speed down the steep face under
    /// the player (`face_slide_switch`).
    pub push: Fixed,
    /// The last step's move was stopped by a rise: a frame held toward
    /// one next keeps a target of 0 (`rise_keeps_target`).
    pub blocked: bool,
    /// After a landing, what the switch frame steep faces wait for is,
    /// until it has come (`face_slide_switch`).
    pub face_switch: Option<FaceSwitch>,
    /// The jump under way, or the last one, started out of a roll.
    pub rolled_jump: bool,
    /// Falling after walking off the ground, until landing.
    pub walked_off: bool,
    /// Falling out of a restart barrel, until landing: the gait goes on
    /// in the air, as off a ledge where the ground simply ends.
    pub keg_fall: bool,
    /// The last walk-off started from a frame on a steep face.
    pub walked_off_face: bool,
    /// The last walk-off was a roll's.
    pub rolled_off: bool,
    /// Action was held with Left or Right alone on the last frame
    /// (`run_from_previous_frame`).
    pub ran: bool,
    /// The next frame of the fall steps its downward speed up to
    /// `leave_step_speed`.
    pub fall_step: bool,
    /// Frames left in which facing stays as it is in a fall
    /// (`leave_facing_wait`).
    pub fall_facing: u8,
    /// Frames since the player last landed, while it stays on the ground.
    pub since_land: Option<u8>,
    /// Since the last bounce, while it goes on: frames from 0 on the
    /// bounce's frame, counting on until it lands and staying there
    /// while on the ground, and how many frames it held still for.
    pub since_bounce: Option<(u8, u8)>,
    /// The enemy whose touch hurts the player on the next step, found
    /// at the end of this one (`hurt_width` above 0).
    pub hurt_ahead: Option<usize>,
    /// What its pose for a ball's reach is read from (see
    /// [`crate::pose`]).
    pub pose: PoseClock,
    /// The knock-out under way, if the player was hurt and is being
    /// knocked out (`knockout_jump` above 0).
    pub knockout: Option<Knockout>,
    /// Frames into the hop that ends taking over from a lost character,
    /// until it lands (see [`crate::Loss`]).
    pub hop: Option<u8>,
    /// What the hands are doing with an object, if anything.
    pub hands: Option<Hands>,
    /// Frames since letting go of action for the latest throw, from 0,
    /// up to the throw's last frame even if a jump or a roll ended it
    /// sooner.
    pub thrown: Option<u8>,
    /// The last jump took off with an object in the hands or during a
    /// throw's frames, until the player next leaves the ground.
    pub carried_jump: bool,
    /// The object a pick-up starts with on this step if action is held,
    /// found before it (see [`crate::carry`]).
    pub pick: Option<usize>,
    /// In the air inside a one-way platform, by columns: risen into it
    /// from below its underside, with the feet still between its
    /// underside and top.
    pub inside: bool,
    /// The ground the body fell through, by columns, going under the top
    /// of a one-way platform too high to step onto: it doesn't catch the
    /// body until it lands.
    pub through: Option<usize>,
    /// Colliders the body ran into or stood on during the last step:
    /// walls it was stopped by, a ceiling it hit, the ground.
    pub touched: Vec<usize>,
    /// Swimming, and resting on the floor at the end of the last step.
    pub swim_floor: bool,
    /// Swimming, with Action and Left or Right held on the last step.
    pub swim_fast: bool,
    /// Swimming, with the deeper sinking limit on the last step.
    pub swim_deep: bool,
    /// A swimmer's turn under way, if one is (see [`crate::swim`]).
    pub swim_turn: Option<SwimTurn>,
    /// Riding an animal, if it is (see [`crate::ride`]).
    pub riding: Option<Riding>,
    /// Knocked off an animal, until it lands or bounces: facing doesn't
    /// change, and Jump must stay held on the frames it holds still for
    /// the held gravity.
    pub unseated: bool,
    /// Hopped off an animal, until it lands: facing doesn't change, and
    /// this many landing frames follow the landing (see
    /// [`ride`](crate::ride)).
    pub hopped_off: Option<u8>,
    /// Jumped off an animal on the last step: this one is the jump's
    /// take-off frame.
    pub jumping_off: bool,
    /// Where the animal got off on this step was left, and the way it
    /// faces, until the animal is put there.
    pub left: Option<(Vec2, Facing)>,
    /// The smallest feet line it has had at the end of a step since it
    /// last stood on a floor, while in the air (see
    /// [`hidden`](crate::hidden)).
    pub fall_top: Option<i64>,
    /// The spring it stands on, if any (see [`spring`](crate::spring)).
    /// It's on the ground then.
    pub spring: Option<OnSpring>,
    /// A spring's throw or a spring jump under way, until it lands or a
    /// spring catches it.
    pub spring_arc: Option<SpringArc>,
    /// Frames Jump has been held without a break, this one included.
    pub jump_held: u8,
    /// The rope it holds, if any (see [`rope`](crate::rope)).
    pub rope: Option<OnRope>,
    /// The thing of the rope it most recently held, until it's
    /// forgotten.
    pub rope_last: Option<usize>,
    /// The direction held on the last frame spent holding a rope.
    pub rope_dir: Option<Facing>,
    /// Set by jumping off a rope until back on the ground: target speeds
    /// in the air are Action's.
    pub rope_run: bool,
    /// Falling after sliding off a rope's end, until landing: facing
    /// doesn't change.
    pub rope_fall: bool,
    /// Caught by a launch barrel, until it lands (see
    /// [`sling`](crate::sling)).
    pub sling: Option<Slung>,
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
            face_press: false,
            carried_press: false,
            uphill_onto_face: false,
            clear_start: false,
            takeoff: false,
            takeoff_wait: None,
            takeoff_crouched: false,
            takeoff_rolled: false,
            action_jump: false,
            action_fall: false,
            action_with_jump: false,
            jump_again: false,
            swap: 0,
            frozen: 0,
            crouch: Crouch::default(),
            turn: None,
            short_turn: false,
            gait: Gait::Standing,
            roll: None,
            roll_stop: 0,
            stopped: None,
            air_stopped: false,
            ground_action: None,
            boost: None,
            boosted: false,
            roll_press: None,
            since_run: None,
            lone_run: false,
            holds: Holds::default(),
            roll_jump: false,
            face_target: None,
            after_face: None,
            jump_age: None,
            approach: None,
            approach_face: false,
            push: Fixed::ZERO,
            blocked: false,
            face_switch: None,
            rolled_jump: false,
            walked_off: false,
            keg_fall: false,
            walked_off_face: false,
            rolled_off: false,
            ran: false,
            fall_step: false,
            fall_facing: 0,
            since_land: None,
            since_bounce: None,
            hurt_ahead: None,
            pose: PoseClock::default(),
            knockout: None,
            hop: None,
            hands: None,
            thrown: None,
            carried_jump: false,
            pick: None,
            inside: false,
            through: None,
            touched: Vec::new(),
            swim_floor: false,
            swim_fast: false,
            swim_deep: false,
            swim_turn: None,
            riding: None,
            unseated: false,
            hopped_off: None,
            jumping_off: false,
            left: None,
            fall_top: None,
            spring: None,
            spring_arc: None,
            jump_held: 0,
            rope: None,
            rope_last: None,
            rope_dir: None,
            rope_run: false,
            rope_fall: false,
            sling: None,
        };
        player.ground = level
            .collision
            .cast(pos, Dir::Down, Fixed::ZERO)
            .map(|hit| hit.collider);
        if Player::in_water(level, pos) {
            player.ground = None;
            player.state = State::Swim;
        } else if !player.grounded() {
            player.state = State::Fall;
        }
        player
    }

    pub fn grounded(&self) -> bool {
        self.ground.is_some() || self.spring.is_some()
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
        self.spring = None;
        self.spring_arc = None;
        self.sling = None;
        self.rope = None;
        self.rope_fall = false;
        self.coyote = None;
        self.jump_age = None;
        self.walked_off = false;
        self.keg_fall = false;
        self.fall_step = false;
        self.fall_facing = 0;
        self.jump_hold = false;
        self.face_press = false;
        self.takeoff = false;
        self.takeoff_wait = None;
        self.hopped_off = None;
        self.jumping_off = false;
        self.state = State::Fall;
    }

    /// Bounces up at `speed` off something landed on, then holds still
    /// for `freeze` frames. Jump held from the frame after those on makes
    /// a held bounce.
    pub fn bounce(&mut self, speed: Fixed, freeze: u8, events: &mut Vec<Event>) {
        self.vel.y = -speed;
        self.ground = None;
        self.spring = None;
        self.spring_arc = None;
        self.sling = None;
        self.rope = None;
        self.rope_fall = false;
        self.coyote = None;
        self.frozen = freeze;
        self.jump_age = None;
        self.walked_off = false;
        self.keg_fall = false;
        self.fall_step = false;
        self.fall_facing = 0;
        self.jump_hold = true;
        self.takeoff = false;
        self.takeoff_wait = None;
        self.unseated = false;
        self.hopped_off = None;
        self.end_roll();
        self.ground_action = None;
        if !events.contains(&Event::Bounce) {
            events.push(Event::Bounce);
        }
        self.state = State::Bounce;
    }

    /// Puts the player on an animal at `(x, feet)`, whole px: moved by
    /// whole px so that its `x` rounded down is `x` and its feet line (its
    /// `y` rounded up) is `feet`, keeping the fractions, with no speed
    /// down. It then holds still for `hold` frames.
    pub(crate) fn mount(&mut self, x: i64, feet: i64, hold: u8) {
        let whole = |n: i64| Fixed::from_int(n as i32);
        self.pos.x += whole(x - self.pos.x.floor());
        self.pos.y += whole(feet + (-self.pos.y).floor());
        self.vel.y = Fixed::ZERO;
        self.frozen = hold;
        // Kept for riding's held gravity, from a jump on foot only.
        self.jump_hold &= self.jump_age.is_some();
        self.face_press = false;
        self.takeoff = false;
        self.takeoff_wait = None;
        self.end_roll();
        self.ground_action = None;
        // Not a bounce's frozen frames, which can hold walkers.
        self.state = State::Fall;
    }

    /// Puts the feet at `y`, with no speed down, on the ground if there's
    /// ground right under them.
    pub(crate) fn put_down(&mut self, level: &Level, y: Fixed, events: &mut Vec<Event>) {
        self.pos.y = y;
        self.vel.y = Fixed::ZERO;
        if self.grounded() {
            return;
        }
        if let Some(hit) = level.collision.cast(self.pos, Dir::Down, Fixed::ZERO) {
            self.land(y, hit.collider, events);
            self.state = self.next_state(self.ran, events);
        }
    }

    /// Puts the player on a spring's top, its feet line at `feet`: moved
    /// by whole px, keeping the fraction, with no speed down. A player
    /// caught in the air stops falling as on landing, without a landing.
    pub(crate) fn stand_on_spring(&mut self, feet: i64, on: OnSpring) {
        self.pos.y += Fixed::from_int((feet - ceil(self.pos.y)) as i32);
        self.vel.y = Fixed::ZERO;
        let caught = !self.grounded();
        self.ground = None;
        self.through = None;
        self.spring = Some(on);
        self.spring_arc = None;
        if caught {
            self.coyote = None;
            self.jump_hold = false;
            self.face_press = false;
            self.jump_age = None;
            self.walked_off = false;
            self.keg_fall = false;
            self.fall_step = false;
            self.fall_facing = 0;
            self.unseated = false;
            self.hopped_off = None;
            self.rope_fall = false;
            self.hop = None;
            self.state = self.next_state(self.ran, &[]);
        }
    }

    /// Thrown up off `spring` on the frame after its squash: on the
    /// ground, at its `throw_speed` up, for this frame and the next, on
    /// which nothing changes, then in the air (see
    /// [`spring`](crate::spring)).
    pub fn throw_off_spring(&mut self, spring: &Spring) {
        self.vel.y = -spring.throw_speed;
        self.ground = None;
        self.through = None;
        self.spring = Some(OnSpring {
            values: *spring,
            since: 0,
            from_air: false,
            top: false,
            thrown: Some(0),
        });
        self.spring_arc = Some(SpringArc {
            held: false,
            jump_gravity: spring.gravity,
            gravity: spring.gravity,
            max_fall: spring.max_fall,
        });
        self.coyote = None;
        self.jump_hold = false;
        self.face_press = false;
        self.takeoff = false;
        self.takeoff_wait = None;
        self.end_roll();
        self.ground_action = None;
        if self.state == State::Roll {
            self.state = State::Idle;
        }
    }

    /// Counts the frames Jump has been held without a break.
    pub(crate) fn update_jump_held(&mut self, input: &Frame) {
        self.jump_held = if input.held.contains(Action::Jump) {
            self.jump_held.saturating_add(1)
        } else {
            0
        };
    }

    /// Catches a rope where the player moved to on this frame: it stops,
    /// and holds the rope from the next. Caught facing the direction held
    /// on its last frame holding one, it starts a turn round it at once.
    pub(crate) fn catch_rope(&mut self, mut on: OnRope) {
        self.vel = Vec2::default();
        self.target = Fixed::ZERO;
        if self.rope_dir == Some(self.facing) {
            on.turn = Some(1);
        }
        self.rope = Some(on);
        self.rope_last = Some(on.thing);
        self.rope_fall = false;
        self.state = State::Hold;
        self.clear_movement();
    }

    /// Caught by a sling where the player moved to on this frame: it
    /// stops, off the ground, facing as it is.
    pub(crate) fn catch_in_sling(&mut self, slung: Slung, events: &mut Vec<Event>) {
        if self.state == State::Roll {
            events.push(Event::RollEnd);
        }
        self.vel = Vec2::default();
        self.target = Fixed::ZERO;
        self.clear_movement();
        self.rope_run = false;
        self.rope_fall = false;
        self.unseated = false;
        self.hopped_off = None;
        self.frozen = 0;
        self.sling = Some(slung);
        self.state = State::Fall;
    }

    /// Lets go of the rope held, if any, without moving.
    pub(crate) fn let_go_of_rope(&mut self) {
        if self.rope.take().is_some() {
            self.state = State::Fall;
        }
    }

    /// Jumps off the rope held, at the speed it moved by on this frame:
    /// from the next frame a jump that has had Jump held since its press,
    /// with the run memory set.
    pub(crate) fn jump_off_rope(&mut self) {
        self.rope = None;
        self.rope_run = true;
        self.state = State::Jump;
        self.jump_age = Some(0);
        self.jump_hold = true;
        self.jump_held_since_press = true;
        self.approach = None;
        self.rolled_jump = false;
    }

    /// Lets go of a rope past its end: from the next frame a fall from
    /// the speed it moved by on this frame, facing as it is.
    pub(crate) fn slide_off_rope(&mut self) {
        self.rope = None;
        self.rope_fall = true;
        self.state = State::Fall;
    }

    /// Forgets the presses still waiting to be used, after a character
    /// joined (`following.md`, "The lead during the wait"): a Jump press
    /// waiting for a landing or a jump, and an Action press waiting for
    /// a roll.
    pub(crate) fn forget_presses(&mut self) {
        self.jump_buffer.take();
        self.jump_needs_hold = false;
        self.carried_press = false;
        self.jump_again = false;
        self.action_with_jump = false;
        self.roll_press = None;
    }

    /// Ends whatever the player was doing in the air or on the ground,
    /// for holding a rope.
    fn clear_movement(&mut self) {
        self.ground = None;
        self.spring = None;
        self.spring_arc = None;
        self.through = None;
        self.inside = false;
        self.coyote = None;
        self.jump_age = None;
        self.jump_hold = false;
        self.face_press = false;
        self.takeoff = false;
        self.takeoff_wait = None;
        self.walked_off = false;
        self.keg_fall = false;
        self.fall_step = false;
        self.fall_facing = 0;
        self.hop = None;
        self.turn = None;
        self.gait = Gait::Standing;
        self.end_roll();
        self.roll_jump = false;
        self.ground_action = None;
        self.crouch = Crouch::default();
    }

    /// Hurts the player where it is: it doesn't move on this step.
    pub fn hit(&mut self, events: &mut Vec<Event>) {
        self.touched.clear();
        self.vel = Vec2::default();
        self.spring = None;
        self.spring_arc = None;
        self.sling = None;
        self.rope = None;
        self.rope_fall = false;
        self.frozen = 0;
        self.takeoff = false;
        self.face_press = false;
        self.takeoff_wait = None;
        self.end_roll();
        self.ground_action = None;
        self.turn = None;
        events.push(Event::Hurt);
    }

    /// The gravity of a frame on which a jump waits to take off in the
    /// air, if it's `held_jump_gravity`: when jump has been held since the
    /// jump started, or it started while crouching or during a timed roll
    /// (`roll_jump_held_gravity`). `crouched` is for the frame such a jump
    /// is cancelled on, which keeps that gravity.
    fn waiting_gravity(&self, crouched: bool, physics: &Physics) -> Option<Fixed> {
        let held = physics[Param::HeldJumpGravity];
        let light =
            crouched || self.takeoff_crouched || self.takeoff_rolled || self.jump_held_since_press;
        (held > Fixed::ZERO && light).then_some(held)
    }

    /// Whether a roll that isn't timed hands over to walking or running:
    /// with `dir` held the way it rolls, on the frame it slows from above
    /// `roll_flow_speed` (`before`) to it or below. One that starts no
    /// faster rolls on to `roll_min_speed`.
    fn rolls_into_moving(&self, before: Fixed, dir: i32, physics: &Physics) -> bool {
        let flow = physics[Param::RollFlowSpeed];
        flow > Fixed::ZERO
            && dir != 0
            && self.vel.x * Fixed::from_int(dir) > Fixed::ZERO
            && before.abs() > flow
            && self.vel.x.abs() <= flow
    }

    /// Ends the timed roll under way or the stop after one, with any
    /// boost waiting.
    fn end_roll(&mut self) {
        self.roll = None;
        self.roll_stop = 0;
        self.stopped = None;
        self.air_stopped = false;
        self.boost = None;
    }

    /// Holds a rolling player still for `freeze` frames after its roll,
    /// or the stop after it, defeated an enemy. A roll that did carries
    /// on into running, and the next frame with action held can boost it.
    /// With `roll_boost_at_defeat`, action held on this frame boosts the
    /// roll instead, and without it the roll goes on as it was.
    pub fn defeat_rolling(&mut self, freeze: u8, physics: &Physics) {
        self.frozen = freeze;
        if physics[Param::RollBoostAtDefeat] > Fixed::ZERO {
            let boost = physics[Param::RollBoost];
            if let Some(roll) = &mut self.roll
                && self.holds.held > 0
                && boost > Fixed::ZERO
            {
                // The new roll frames start once the freeze is over.
                roll.left = frames(physics[Param::RollTime]);
                roll.speed += boost;
                roll.boosts = roll.boosts.saturating_add(1);
                roll.defeated = true;
                roll.carry = false;
            }
            return;
        }
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
        if physics[Param::KnockoutHopAt] > Fixed::ZERO {
            let over = self.slide_knocked_out(level, physics, &mut knockout, events);
            let over = over || self.body().1 > level.size.y;
            self.knockout = (!over).then_some(knockout);
            return;
        }
        let hold = u16::from(frames(physics[Param::KnockoutHold]));
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

    /// Sends a player hurt on this step flying at the knock-out's speeds
    /// at the end of it (`knockout_hold` 0), back from the way it faces
    /// or, if given, the way `toward`, and then facing the other way.
    pub fn knock(&mut self, physics: &Physics, toward: Option<Facing>) {
        self.frozen = 0;
        self.takeoff = false;
        self.takeoff_wait = None;
        self.end_roll();
        self.ground_action = None;
        self.turn = None;
        self.hop = None;
        self.coyote = None;
        self.jump_age = None;
        self.jump_hold = false;
        self.face_press = false;
        self.walked_off = false;
        self.keg_fall = false;
        self.fall_step = false;
        self.fall_facing = 0;
        let back = match toward {
            Some(way) => way,
            None => match self.facing {
                Facing::Left => Facing::Right,
                Facing::Right => Facing::Left,
            },
        };
        let speed = physics[Param::KnockoutSpeed];
        self.vel.x = match back {
            Facing::Left => -speed,
            Facing::Right => speed,
        };
        self.vel.y = -physics[Param::KnockoutJump];
        if toward.is_some() {
            self.facing = match back {
                Facing::Left => Facing::Right,
                Facing::Right => Facing::Left,
            };
        }
    }

    /// One frame of a knock-out with a hop (`knockout_hop_at`): flying,
    /// sliding along the ground once landed, then the hop, which slows
    /// down to a stop. Only landings are events. Returns whether the
    /// knock-out is over after this frame.
    fn slide_knocked_out(
        &mut self,
        level: &Level,
        physics: &Physics,
        knockout: &mut Knockout,
        events: &mut Vec<Event>,
    ) -> bool {
        if knockout.hop.is_some() {
            self.vel.x = ease(self.vel.x, Fixed::ZERO, physics[Param::KnockoutHopSlowing]);
        }
        if self.vel.y.is_negative() {
            // Leaving the ground, as on a jump's first airborne frame.
            self.ground = None;
        }
        let slope = physics[Param::FaceSlope];
        let by_columns = slope > Fixed::ZERO;
        let mut moves = Vec::new();
        if self.grounded() {
            let (x, below) = if by_columns {
                self.move_by_columns(level, slope, None, None);
                let below = match physics[Param::FollowDrop] {
                    d if d > Fixed::ZERO => d,
                    _ => STEP,
                };
                (columns::middle(columns::column(self.pos.x)), below)
            } else {
                self.move_horizontally(level, HALF_WIDTH, |_| true);
                (self.pos.x, STEP)
            };
            self.follow_ground(level, x, below, &mut moves);
        } else {
            let gravity = match physics[Param::KnockoutGravity] {
                g if g > Fixed::ZERO => g,
                _ if self.vel.y.is_negative() => physics[Param::Gravity],
                _ => physics[Param::FallGravity],
            };
            self.vel.y = (self.vel.y + gravity).min(physics[Param::KnockoutMaxFall]);
            let moved = if by_columns {
                self.move_in_air_by_columns(level, physics, &mut moves)
            } else {
                self.move_horizontally(level, HALF_WIDTH, |_| true)
            };
            if !self.grounded() {
                self.move_vertically(level, moved, Reach::new(physics), by_columns, &mut moves);
            }
        }
        events.extend(moves.into_iter().filter(|&e| e == Event::Land));
        let hop_at = u16::from(frames(physics[Param::KnockoutHopAt]));
        if knockout.hop.is_none() && knockout.frame >= hop_at && self.grounded() {
            knockout.hop = Some(knockout.frame);
            self.vel.y = -physics[Param::KnockoutHop];
        }
        let Some(hop) = knockout.hop else {
            return false;
        };
        let rest = u16::from(frames(physics[Param::KnockoutRest]));
        let time = u16::from(frames(physics[Param::KnockoutTime]));
        let last = (hop + rest).max(time).saturating_sub(1);
        knockout.frame >= last
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
        self.move_vertically(level, moved, Reach::new(physics), false, &mut landed);
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
    /// what each can do. `picture` is the level `x` of the picture's left
    /// edge, whole px, if there's a view.
    pub fn update(
        &mut self,
        level: &Level,
        feel: &Feel,
        physics: &Physics,
        input: &Frame,
        picture: Option<i64>,
        events: &mut Vec<Event>,
    ) {
        self.touched.clear();
        self.boosted = false;
        self.update_jump_held(input);
        if self.frozen > 0 {
            if self.unseated {
                self.jump_hold &= input.held.contains(Action::Jump);
            }
            self.frozen -= 1;
            return;
        }
        if let Some(on) = &mut self.spring
            && let Some(n) = on.thrown
        {
            if n == 0 {
                // Nothing changes on the frame after a spring's throw.
                on.thrown = Some(1);
                return;
            }
            // In the air from the one after.
            self.spring = None;
        }
        if self.grounded() {
            self.unseated = false;
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
        if self.riding.is_some() || Player::in_water(level, self.pos) {
            self.push = Fixed::ZERO;
            self.face_switch = None;
        }
        if self.riding.is_some() {
            self.ride(level, physics, input, held_dir, events);
            return;
        }
        if Player::in_water(level, self.pos) {
            self.swim(level, physics, input, held_dir, picture, events);
            return;
        }
        if self.state == State::Swim {
            // Out of the water, with nothing under the feet yet.
            self.state = State::Fall;
            self.swim_floor = false;
        }
        let ran_before = std::mem::replace(&mut self.ran, held(Action::Action) && held_dir != 0);
        let timed = physics[Param::RollTime] > Fixed::ZERO;
        let start_vx = self.vel.x;
        let start_facing = self.facing;
        let crouch_in_air = physics[Param::CrouchInAir] > Fixed::ZERO;
        let facing_wait = frames(physics[Param::LeaveFacingWait]);

        // A press waits out the jump delay in the buffer, however short
        // the buffer is.
        let delay = frames(physics[Param::JumpDelay]);
        self.jump_buffer
            .set_window(frames(physics[Param::JumpBuffer]).max(delay));
        // With `action_jump_press`, Jump held on the frame after a crouch
        // that an Action press with Jump started counts as a press.
        let jump_press =
            pressed(Action::Jump) || (std::mem::take(&mut self.jump_again) && held(Action::Jump));
        let action_jump_press = physics[Param::ActionJumpPress] > Fixed::ZERO;
        let now_with_jump =
            action_jump_press && jump_press && pressed(Action::Action) && self.grounded();
        let with_jump = std::mem::replace(&mut self.action_with_jump, now_with_jump);
        self.jump_buffer.update(jump_press);
        if jump_press {
            self.jump_needs_hold = false;
        }
        self.jump_held_since_press =
            jump_press || (self.jump_held_since_press && held(Action::Jump));
        self.jump_hold &= held(Action::Jump);
        self.face_press &= held(Action::Jump);
        if self.takeoff {
            // The jump that started on the ground last frame leaves it.
            self.takeoff = false;
            self.ground = None;
            self.spring = None;
            self.coyote = None;
            self.state = State::Jump;
        }
        // On a spring's top, a press makes a spring jump at once; caught
        // in the air, not before `jump_wait` frames after the catch, and
        // then for a press made up to `jump_early` frames before it and
        // held since.
        let on_top = self.spring.filter(|on| on.top);
        let spring_jump = on_top.filter(|_| self.swap == 0).is_some_and(|on| {
            let since = u32::from(on.since) + 1;
            let wait = u32::from(on.values.jump_wait);
            if !on.from_air || since > wait {
                pressed(Action::Jump)
            } else {
                since == wait
                    && u32::from(self.jump_held) <= wait + u32::from(on.values.jump_early) + 1
                    && held(Action::Jump)
            }
        });
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

        // Down as the crouch sees it. With Up keeping crouches going, a
        // crouch starts on Down without Up and goes on while exactly one
        // of them is held.
        let up_holds = physics[Param::CrouchUpHolds] > Fixed::ZERO;
        let down = !busy.no_crouch
            && if !up_holds {
                held(Action::Down)
            } else if self.crouch.held > 0 {
                held(Action::Down) != held(Action::Up)
            } else {
                held(Action::Down) && !held(Action::Up)
            };
        let crouch_ends = self.crouch.held > 0 && !down;

        // The way the steep face the player starts the frame on falls, if
        // any. A timed roll has its own rules there, with `face_rolls`.
        let slope = physics[Param::FaceSlope];
        let start_face = if slope > Fixed::ZERO && self.ground.is_some() {
            columns::face(level, columns::column(self.pos.x), self.pos.y, slope)
        } else {
            None
        };
        let face = start_face.filter(|_| self.state != State::Roll);
        let start_column = columns::column(self.pos.x);
        let face_rolls = physics[Param::FaceRolls] > Fixed::ZERO && slope > Fixed::ZERO;
        let roll_face = start_face.filter(|_| face_rolls && self.roll.is_some());
        // For jumps, with `face_jump_wait`, a frame that moves onto a face
        // with only its uphill direction held is followed by one that
        // doesn't count as starting on it.
        let face_wait = physics[Param::FaceJumpWait] > Fixed::ZERO;
        let jump_face = face.is_some() && !std::mem::take(&mut self.uphill_onto_face);
        // A jump that ended the ground action takes off even onto a face.
        let action_jumped = std::mem::take(&mut self.action_jump);
        let jump_face = jump_face && !action_jumped;
        // With `face_speed`, a direction held up the face counts as none.
        let downhill = face.filter(|_| physics[Param::FaceSpeed] > Fixed::ZERO);
        // The face the last frame started on pulls the speed down it.
        let pull = std::mem::take(&mut self.after_face);
        let dir = if downhill == Some(-held_dir) {
            0
        } else {
            held_dir
        };

        if timed {
            self.remember_roll_press(input, physics);
            self.roll_after_throw(input, physics);
        }
        // Nor does an Action press with Jump (`action_jump_press`).
        if busy.no_roll || acting.is_some() || self.action_with_jump {
            self.roll_press = None;
        }

        // With the other character in the level, swaps are the world's.
        if self.grounded()
            && self.state != State::Roll
            && acting.is_none()
            && crate::swap::swap_press(input)
            && feel.roster.len() > 1
            && feel.world[Param::SwapMoves] == Fixed::ZERO
        {
            self.character = (self.character + 1) % feel.roster.len();
            self.swap = SWAP_FRAMES;
            self.jump_buffer.take();
            self.takeoff_wait = None;
        }

        // A jump pressed during a timed roll can start wherever the roll
        // has taken the player, in the air too.
        let roll_jump = std::mem::take(&mut self.roll_jump) || self.roll.is_some();
        let ready = self
            .jump_buffer
            .age()
            .is_some_and(|age| age >= delay.saturating_add(busy.jump_delay));
        // The jump started `takeoff_wait` frames ago takes off on this one.
        let wait = frames(physics[Param::TakeoffWait]);
        let pending = self.takeoff_wait.is_some();
        let mut due = self.takeoff_wait == Some(1);
        self.takeoff_wait = self
            .takeoff_wait
            .and_then(|n| n.checked_sub(1))
            .filter(|&n| n > 0);
        if spring_jump {
            due = false;
            self.takeoff_wait = None;
        }
        let cancelled = pending && self.takeoff_crouched && crouch_ends;
        if cancelled {
            // A jump started while crouching doesn't take off once the
            // crouch ends. With jump held it starts again on this frame,
            // where the crouch has ended.
            due = false;
            self.takeoff_wait = held(Action::Jump).then_some(wait);
            self.takeoff_crouched = false;
        }
        let waiting = due || self.takeoff_wait.is_some();
        // A press on a frame that starts on the face, or whose next frame
        // does, is carried: held, it takes off on the frame after the
        // first that starts on the floor off the face.
        if face_wait {
            match self.jump_buffer.age() {
                Some(0) => self.carried_press = jump_face,
                Some(1) => self.carried_press |= jump_face && self.clear_start,
                _ => {}
            }
            if self.carried_press && self.jump_buffer.is_buffered() {
                self.jump_needs_hold = physics[Param::JumpBufferHold] > Fixed::ZERO;
            }
        }
        // After a fall from the face, that's the frame after the landing's,
        // whatever the landing lockout.
        let carried = face_wait && self.carried_press;
        let can_jump = self.swap == 0
            && (!locked || carried)
            && !waiting
            && !jump_face
            && !(carried && !self.clear_start)
            // Caught in the air, only a spring jump leaves the top.
            && !on_top.is_some_and(|on| on.from_air)
            && !spring_jump
            && (self.grounded() || self.coyote.is_some() || roll_jump)
            && (physics[Param::JumpStartHold] == Fixed::ZERO || held(Action::Jump))
            && ready;
        if face.is_some() && ready && !action_jumped {
            // No jump starts on a face: the press is treated as one made
            // in the air. Made on the frame before, if that one started on
            // the ground off the face, it still lightens the fall.
            self.jump_needs_hold = physics[Param::JumpBufferHold] > Fixed::ZERO;
            if physics[Param::FacePressGravity] > Fixed::ZERO
                && self.clear_start
                && self.jump_buffer.age() == Some(delay.saturating_add(busy.jump_delay))
            {
                self.face_press = true;
            }
        }
        let clear_start = self.grounded() && face.is_none();
        self.clear_start = clear_start;
        let takeoff_frame = physics[Param::GroundedTakeoff] > Fixed::ZERO;
        let mut air_takeoff = None;
        // A jump that waited to take off in a timed roll, or the stop
        // after one, ends it once this frame has moved.
        let mut takeoff_ends_roll = false;
        // An Action press with Jump on the last frame, and Down held on
        // this one: a crouch starts here instead of the jump.
        let crouch_instead = with_jump && down && can_jump;
        if crouch_instead {
            self.jump_buffer.take();
            self.jump_again = true;
            self.crouch.held = 0;
        }
        let starts = can_jump && !crouch_instead && self.jump_buffer.take();
        if let Some(on) = on_top.filter(|_| spring_jump) {
            self.jump_buffer.take();
            if self.state == State::Roll {
                events.push(Event::RollEnd);
            }
            self.end_roll();
            self.vel.y = -on.values.jump_speed;
            self.takeoff = true;
            self.jump_hold = false;
            self.face_press = false;
            self.coyote = None;
            self.spring_arc = Some(SpringArc {
                held: true,
                jump_gravity: on.values.jump_gravity,
                gravity: on.values.gravity,
                max_fall: on.values.max_fall,
            });
            self.state = State::Jump;
            events.push(Event::JumpStart);
        }
        if starts && wait > 0 {
            // Nothing shows yet: the player goes on as before.
            self.takeoff_wait = Some(wait);
            self.takeoff_crouched = physics[Param::CrouchTakeoffCancel] > Fixed::ZERO
                && down
                && (self.grounded() || self.crouch.held > 0);
            self.takeoff_rolled =
                physics[Param::RollJumpHeldGravity] > Fixed::ZERO && self.roll.is_some();
            self.coyote = None;
        }
        if (starts && wait == 0) || due {
            self.rolled_jump = self.state == State::Roll || roll_jump;
            self.approach = None;
            if self.state == State::Roll {
                events.push(Event::RollEnd);
            }
            if due {
                takeoff_ends_roll = true;
            } else {
                self.end_roll();
            }
            let mut speed = -physics[Param::JumpSpeed];
            if !held(Action::Jump) {
                // A buffered tap that's already over makes a short hop.
                speed = speed.max(-physics[Param::JumpReleaseSpeed]);
            }
            self.jump_hold = self.jump_held_since_press;
            if takeoff_frame && self.grounded() {
                self.vel.y = speed;
                self.takeoff = true;
            } else if takeoff_frame {
                // In the air (out of a roll, after waiting to take off
                // when the ground has gone, or just after walking off),
                // the jump spends its first frame moving as before, and
                // rises from the end of it.
                air_takeoff = Some(speed);
            } else {
                self.vel.y = speed;
                self.ground = None;
            }
            self.coyote = None;
            self.state = State::Jump;
            events.push(Event::JumpStart);
        } else if self.state == State::Jump
            && self.spring_arc.is_none()
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
        // With `face_jump_wait` the take-off frame moves as a frame of the
        // jump: a direction back up a face just left counts, and there's
        // no push down one.
        let face_takeoff = face_wait && self.takeoff;
        let dir = if face_takeoff && face.is_none() {
            held_dir
        } else {
            dir
        };
        // A jump pressed during a timed roll ends it on this frame.
        if self.roll.is_some() && pressed(Action::Jump) {
            self.roll_jump = true;
        }
        // So does one pressed during the ground action on a frame that
        // starts on the floor off a face, which still moves by its rules
        // on this frame, and one held through a landing during it, on the
        // frame after the landing. In the air a press is only kept for
        // the landing.
        let action_jump = acting.is_some_and(|a| {
            let on_floor = self.grounded() && face.is_none();
            // That press takes off on the next frame if it's still kept.
            let kept = self
                .jump_buffer
                .age()
                .is_some_and(|age| age < frames(physics[Param::JumpBuffer]));
            let landed = self.since_land == Some(0) && held(Action::Jump) && kept;
            (pressed(Action::Jump) && (a.air || on_floor)) || (on_floor && landed)
        });
        let mut goes_on = acting.is_some_and(|a| a.frame < action_time) && !action_jump;
        self.action_jump = action_jump;
        // Next to ground ahead too high to start toward, it turns the
        // player its way, and a moving start goes at walking speed the
        // way the player was headed.
        let ahead = physics[Param::StartRiseAhead].floor();
        let (column, feet) = (columns::column(self.pos.x), self.pos.y);
        let rise_ahead = |dir: i32| {
            ahead > 0 && dir != 0 && columns::rise_ahead(level, column, dir, ahead, feet)
        };
        let mut action_turn = None;
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
            && !self.action_with_jump
        {
            self.vel.x = Fixed::ZERO;
            // Not on a face, where the press is as without Down.
            if start_face.is_none() {
                // It goes the way held now, whatever the player was doing.
                let mut speed = None;
                if rise_ahead(held_dir) {
                    action_turn = Some(facing_of(held_dir));
                    if start_vx != Fixed::ZERO {
                        let way = if self.target.is_negative() {
                            -1
                        } else {
                            i32::from(self.target > Fixed::ZERO)
                        };
                        speed = Some(physics[Param::WalkSpeed] * Fixed::from_int(way));
                    }
                }
                acting = Some(GroundAction {
                    frame: 0,
                    dir: held_dir,
                    speed,
                    air: false,
                });
                goes_on = true;
            }
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
        // Nor does one while a jump waits to take off.
        let can_roll = timed
            && !busy.no_roll
            && !events.contains(&Event::JumpStart)
            && self.takeoff_wait.is_none()
            && !due
            && feel.roster[self.character].abilities.contains_key(ROLL);
        let boost = if can_roll {
            self.use_boost(held(Action::Action), physics)
        } else {
            None
        };
        let boosted = boost.is_some();
        self.boosted = boosted;
        let started = can_roll && !boosted && self.start_timed_roll(input, start_face, physics);
        if started || boost == Some(true) {
            self.state = State::Roll;
            events.push(Event::RollStart);
            // A roll ends the throw it starts in.
            self.hands = None;
        }
        let rolling = self.state == State::Roll;

        // The ground action stands the player up, and down held during it
        // only counts from the frame after it's over. With
        // `crouch_jump_press`, so does taking off.
        let crouch_jump_press = physics[Param::CrouchJumpPress] > Fixed::ZERO;
        let took_off = crouch_jump_press && !crouch_in_air && events.contains(&Event::JumpStart);
        let crouching = if acting.is_some() || took_off {
            self.crouch = Crouch {
                turn_lock: self.crouch.turn_lock,
                ..Crouch::default()
            };
            false
        } else {
            // With `crouch_in_air`, a crouch goes on in a fall off a ledge.
            let in_fall = crouch_in_air && self.walked_off && self.crouch.held > 0;
            // Down doesn't count on the frame after a hop's landing.
            let hop_landed = matches!(self.gait, Gait::Landing(l) if l.bounce && l.frame == 0)
                || (self.action_fall && self.grounded());
            // Nor before `crouch_land_wait` frames after a landing.
            let land_wait = frames(physics[Param::CrouchLandWait]);
            let just_landed = self
                .since_land
                .is_some_and(|n| n.saturating_add(1) < land_wait);
            // Nor on a Jump press on the first frame Down is held.
            let press_first = crouch_jump_press && jump_press && self.crouch.held == 0;
            self.crouch.update(
                down && !busy.no_crouch && !press_first,
                (self.grounded() || in_fall) && !rolling && !hop_landed && !just_landed,
                physics,
            )
        };

        // Held toward ground ahead too high to start toward, the target
        // is the last frame's and the player faces the held way at once,
        // but not on the frames whose target follows the input as in the
        // air.
        let toward_rise = physics[Param::RiseKeepsTarget] > Fixed::ZERO
            && self.grounded()
            && face.is_none()
            && acting.is_none()
            && self.roll.is_none()
            && !rolling
            && self.roll_stop == 0
            && !crouching
            && !busy.still
            && !events.contains(&Event::JumpStart)
            && self.face_switch.is_none()
            && rise_ahead(dir);
        // Just off a face, the face is the rise, and its frame's target
        // is the held direction's.
        let face_target = std::mem::take(&mut self.face_target);
        let kept_target = if self.blocked {
            Fixed::ZERO
        } else {
            face_target.unwrap_or(self.target)
        };

        let turn_time = frames(physics[Param::TurnTime]);
        let slide = frames(physics[Param::RollStopSlide]);
        // With a stride cycle, whether a turn starts is decided once the
        // frame's speed is known.
        let strides = physics[Param::StrideSteps] >= Fixed::ONE && turn_time > 0;
        // Facing stays as it is in a fall off a steep face; off a ledge
        // where the ground simply ends, the gait goes on as on the ground.
        let keep_walk_off_facing =
            self.walked_off && physics[Param::WalkOffKeepsFacing] > Fixed::ZERO;
        let air_gait = strides
            && ((keep_walk_off_facing && !self.walked_off_face && !self.rolled_off)
                || self.keg_fall);
        let striding = strides
            && !busy.keep_facing
            && acting.is_none()
            && (self.grounded() || air_gait)
            && self.roll.is_none()
            && !rolling
            && self.roll_stop == 0
            && !events.contains(&Event::JumpStart);
        let turn_length = turn_time.saturating_sub(self.short_turn.into());
        let turn_ends = self.turn.is_some_and(|n| n + 1 >= turn_length);
        // A jump waits to take off, or takes off on this frame.
        let waits = due || self.takeoff_wait.is_some();
        let keep_fall_facing =
            facing_wait > 0 && !self.grounded() && (self.fall_facing > 0 || crouching || waits);
        // Nor does it change while a jump waits on the ground.
        let keep_takeoff_facing = waits && self.grounded();
        let keep_bounce_facing =
            self.state == State::Bounce && physics[Param::BounceKeepsFacing] > Fixed::ZERO;
        self.fall_facing = self.fall_facing.saturating_sub(1);
        let facing = if acting.is_some() {
            self.turn = None;
            action_turn.unwrap_or(self.facing)
        } else if let Some(mut roll) = self.roll {
            // Steering does nothing on a frame that starts on a face.
            if !started && roll_face.is_none() {
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
        } else if self.roll_stop > slide
            || (self.roll_stop > 0 && physics[Param::RollSlideTurns] > Fixed::ZERO)
        {
            // Standing still in the stop after a roll, or sliding with
            // `roll_slide_turns`, the player turns at once, or not until
            // it's over.
            self.turn = None;
            if physics[Param::RollAfterStop] > Fixed::ZERO {
                self.facing
            } else {
                facing_toward(dir, self.facing)
            }
        } else if keep_fall_facing
            || keep_takeoff_facing
            || keep_bounce_facing
            || self.unseated
            || self.hopped_off.is_some()
            || self.rope_fall
            || (keep_walk_off_facing && !air_gait)
        {
            self.turn = None;
            self.facing
        } else if let Some(way) = face.filter(|_| physics[Param::FaceTurns] > Fixed::ZERO) {
            // On a face the player faces downhill at once, but not on the
            // frame after landing on it.
            self.turn = None;
            if self.since_land == Some(0) {
                self.facing
            } else {
                facing_of(way)
            }
        } else if physics[Param::CrouchTurnLock] > Fixed::ZERO
            && self.grounded()
            && (crouching || self.crouch.turn_lock > 0)
        {
            // No turn starts while crouching or just after, and a crouch
            // ends the turn under way where it is.
            self.turn = None;
            self.facing
        } else if striding && let Gait::Landing(landing) = self.gait {
            // On the frames after a jump's landing, facing follows the
            // input as in the air; after a fall's, it stays as it is.
            self.turn = None;
            if landing.fixed {
                self.facing
            } else {
                facing_toward(dir, self.facing)
            }
        } else if turn_time > 0
            && (self.grounded() || air_gait)
            && !events.contains(&Event::JumpStart)
        {
            self.turn_on_ground(dir, turn_time, !striding, toward_rise)
        } else {
            // In the air, and on the take-off frame, facing follows the
            // input at once.
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
        let walled = start_vx == Fixed::ZERO
            && !toward_rise
            && self.grounded()
            && !rolling
            && !events.contains(&Event::JumpStart)
            && self.face_switch.is_none()
            && rise_ahead(dir);
        // With Up keeping crouches going, Down held with Up still stops
        // the player, without a crouch.
        let down_stops = up_holds && held(Action::Down) && self.grounded();
        let dir = if crouching || walled || busy.still || down_stops {
            0
        } else {
            dir
        };

        // After jumping off a rope, targets are Action's until landing.
        let run = self.rope_run
            || if physics[Param::RunFromPreviousFrame] > Fixed::ZERO {
                ran_before
            } else {
                held(Action::Action)
            };
        let cap = if run {
            physics[Param::TopSpeed]
        } else {
            physics[Param::WalkSpeed]
        };
        let response = physics[Param::SpeedResponse];
        let pushed = physics[Param::FaceSlideSwitch] > Fixed::ZERO;
        let mut push = Fixed::ZERO;
        let mut held_target = None;
        self.target = Fixed::ZERO;
        self.vel.x = if let Some(action) = acting {
            // Down a face, faster; its last frame takes one more step,
            // unless it's in the air.
            self.target = action
                .speed
                .unwrap_or(physics[Param::GroundActionSpeed] * Fixed::from_int(action.dir))
                + face.map_or(Fixed::ZERO, |way| {
                    physics[Param::FaceSpeed] * Fixed::from_int(way)
                });
            if start_vx == Fixed::ZERO && self.grounded() && rise_ahead(action.dir) {
                Fixed::ZERO
            } else if action.frame < action_time || self.grounded() {
                ease(self.vel.x, self.target, response)
            } else {
                self.vel.x
            }
        } else if self.roll.is_some() {
            if let Some(vx) = self.stopped_roll(level, held_dir, roll_face, physics) {
                vx
            } else if let Some(roll) = self.roll.filter(|r| !r.still) {
                // Going down a face, it goes faster.
                let down = roll_face
                    .filter(|&way| way == direction(roll.dir))
                    .map_or(Fixed::ZERO, |way| {
                        physics[Param::FaceSpeed] * Fixed::from_int(way)
                    });
                self.target = roll.target() + down;
                ease(self.vel.x, self.target, response)
            } else {
                Fixed::ZERO
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
                _ if toward_rise => kept_target,
                -1 => -cap,
                1 => cap,
                _ => Fixed::ZERO,
            };
            held_target = Some(target);
            if face_takeoff {
                // Nothing down the face on the take-off frame.
            } else if pushed {
                push = self.push;
                target += push;
            } else {
                match face {
                    Some(-1) => target -= physics[Param::FaceSpeed],
                    Some(_) => target += physics[Param::FaceSpeed],
                    None => {}
                }
            }
            if let Some(way) = pull {
                target += Fixed::from_int(direction(way)) * physics[Param::FacePull];
            }
            self.target = target;
            ease(self.vel.x, target, response)
        } else {
            self.target = Fixed::from_int(dir) * cap;
            horizontal_speed(self.vel.x, dir, self.grounded(), cap, physics)
        };
        // Whether the turn is short is decided on its second frame, by
        // the speed toward the new facing.
        self.short_turn = match self.turn {
            Some(1) => {
                let short = physics[Param::ShortTurnSpeed];
                short > Fixed::ZERO
                    && self.vel.x * -Fixed::from_int(direction(self.facing)) >= short
            }
            Some(_) => self.short_turn,
            None => false,
        };
        if !striding {
            // In the air, rolling and in a roll's stop, the player isn't
            // walking or running; landings and the end of a roll set the
            // gait below.
            self.gait = Gait::Standing;
        } else if self.stride(
            run_dir,
            // Toward a rise, a turn starts by the way the target points.
            if toward_rise && self.target.is_negative() {
                -1
            } else if toward_rise {
                i32::from(self.target > Fixed::ZERO)
            } else {
                run_dir
            },
            held(Action::Action),
            turn_ends,
            physics,
        ) {
            self.facing = facing_of(-direction(self.facing));
            events.push(Event::Turn);
        }
        let airborne = !self.grounded();
        let fall_step = std::mem::take(&mut self.fall_step);
        // A timed roll that left the ground soon enough after its press
        // hangs in the air from its second frame after leaving it.
        let hover = frames(physics[Param::RollHover]);
        let hovering = airborne
            && hover > 0
            && self
                .roll
                .is_some_and(|r| r.air >= 2 && r.since_press <= hover);
        // One that left soon enough after its first frame drifts down.
        let drifting = airborne
            && self
                .roll
                .is_some_and(|r| r.leave_hovers(r.frame.saturating_sub(r.air), physics));
        // The gravity of a jump waiting to take off, if it's the held one.
        let waiting_gravity = if waits || cancelled {
            self.waiting_gravity(cancelled, physics)
        } else {
            None
        };
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
        } else if let Some(arc) = self.spring_arc.as_mut().filter(|_| airborne) {
            // A spring's throw, or a spring jump, has its own gravity.
            self.vel.y = arc.fall(self.vel.y, held(Action::Jump));
        } else if airborne {
            let held_gravity = physics[Param::HeldJumpGravity];
            // A jump pressed during a roll in the air falls under the
            // held gravity from its press.
            let jump_hold =
                self.jump_hold || self.face_press || (self.roll_jump && self.jump_held_since_press);
            let until_top = physics[Param::HeldGravityUntilTop] > Fixed::ZERO;
            let gravity = if let Some(gravity) = waiting_gravity {
                gravity
            } else if jump_hold
                && held_gravity > Fixed::ZERO
                && !(until_top && !self.vel.y.is_negative())
            {
                held_gravity
            } else if self.vel.y.is_negative() {
                physics[Param::Gravity]
            } else {
                physics[Param::FallGravity]
            };
            self.vel.y = if hovering {
                Fixed::ZERO
            } else if drifting {
                physics[Param::LeaveFallSpeed]
            } else if fall_step && !waits && !self.floor_just_below(level, physics) {
                physics[Param::LeaveStepSpeed]
            } else {
                self.vel.y + gravity
            }
            .min(physics[Param::MaxFallSpeed]);
        }
        // At the level's ends, a move toward the end leaves `x` as it is.
        if view::stops(self.pos.x, self.vel.x, level, physics) {
            self.vel.x = Fixed::ZERO;
        }
        let hold = self.jump_hold;
        let feet = self.pos.y;
        self.blocked = false;
        // Stopped in the air or on the floor (`face_slide_switch`).
        let mut stopped = false;
        let moved = if slope > Fixed::ZERO && airborne {
            let before = self.vel.x;
            let moved = self.move_in_air_by_columns(level, physics, events);
            stopped = before != Fixed::ZERO && self.vel.x == Fixed::ZERO;
            moved
        } else if slope > Fixed::ZERO {
            // A roll is also stopped by floor more than a step above its
            // own, and once blocked it's a stopped roll.
            let before = self.vel.x;
            let rise = (face_rolls && self.roll.is_some()).then_some(STEP);
            let moved = self.move_by_columns(level, slope, face.map(|way| (way, physics)), rise);
            self.blocked = before != Fixed::ZERO && self.vel.x == Fixed::ZERO;
            stopped = self.blocked;
            if let Some(roll) = self.roll.as_mut().filter(|_| face_rolls) {
                roll.stopped |= before != Fixed::ZERO && self.vel.x == Fixed::ZERO;
            }
            moved
        } else {
            self.move_horizontally(level, HALF_WIDTH, |_| true)
        };
        if self.roll_stop > 0 && self.roll_stop == slide {
            // The slide's last frame moves, but ends standing.
            self.vel.x = Fixed::ZERO;
        }
        if takeoff_ends_roll {
            self.end_roll();
        }

        // Off a face, the fall has its own drop and no step.
        let face_leave = face.is_some() && physics[Param::FaceLeaveDrop] > Fixed::ZERO;
        // Left a spring's top on the last frame without being put back.
        let off_spring = self.spring.filter(|on| !on.top);
        if let Some(on) = on_top {
            // Nothing holds the player up on a spring's top: it goes down
            // as if following the ground, and the spring puts it back.
            self.spring = Some(OnSpring { top: false, ..on });
            if spring_jump {
                if let Some(arc) = &mut self.spring_arc {
                    self.vel.y = arc.fall(self.vel.y, held(Action::Jump));
                }
                self.move_vertically(level, moved, Reach::NONE, slope > Fixed::ZERO, events);
            } else {
                let from = Vec2::new(self.pos.x, self.pos.y);
                match level.collision.cast(from, Dir::Down, on.values.drop) {
                    Some(hit) => {
                        self.pos.y = hit.point.y;
                        self.ground = Some(hit.collider);
                        self.spring = None;
                        self.touched.push(hit.collider);
                    }
                    None => self.pos.y += on.values.drop,
                }
            }
        } else if !airborne {
            // From a steep sloped piece the ground is followed farther
            // down, and leaving it drops as much farther.
            let steeper = match face {
                None if slope > Fixed::ZERO => steeper(level, start_column, feet, physics),
                _ => Fixed::ZERO,
            };
            let leave_hovers = self.roll.is_some_and(|r| r.leave_hovers(r.frame, physics));
            let drop = if let Some(on) = off_spring {
                on.values.leave_drop
            } else if face_leave {
                physics[Param::FaceLeaveDrop]
            } else if leave_hovers {
                physics[Param::LeaveFallSpeed]
            } else {
                physics[Param::LeaveDrop] + steeper
            };
            // By columns, the floor is the one under the middle of the
            // player's column, and off a face it's followed down less far.
            let (x, below) = if slope > Fixed::ZERO {
                let column = columns::column(self.pos.x);
                let below = match physics[Param::FollowDrop] {
                    d if d > Fixed::ZERO && face.is_none() => {
                        // Onto a slope or a face, farther.
                        let piece = physics[Param::SlopePiece].floor();
                        let onto_slope = columns::face(level, column, feet, slope).is_some()
                            || (piece > 0 && columns::sloped(level, column, feet, slope, piece));
                        let onto = match onto_slope {
                            true => physics[Param::FollowOntoSlope],
                            false => Fixed::ZERO,
                        };
                        d + steeper + onto
                    }
                    _ => STEP,
                };
                (columns::middle(column), below)
            } else {
                (self.pos.x, STEP)
            };
            // A jump starting on this frame, or pressed on it in time to
            // start on the next, drops under its own gravity if it's the
            // held one.
            let graced = frames(physics[Param::CoyoteTime]) >= 1
                && !(face.is_some() && physics[Param::NoFaceCoyote] > Fixed::ZERO);
            let jump_gravity =
                if self.takeoff || self.face_press || (graced && self.jump_buffer.age() == Some(0))
                {
                    self.waiting_gravity(false, physics)
                } else {
                    None
                };
            let over_end = self.takeoff && physics[Param::TakeoffOverEnd] > Fixed::ZERO;
            // Staying in a face's lowest column with no floor beyond it.
            let at_drop = face.is_some_and(|way| {
                physics[Param::FaceDropLeave] > Fixed::ZERO
                    && !self.takeoff
                    && !stopped
                    && columns::column(self.pos.x) == start_column
                    && !columns::spans(level, start_column + i64::from(way))
                        .iter()
                        .any(|s| s.top >= feet)
            });
            let left = if off_spring.is_some() {
                // However near the floor below.
                self.spring = None;
                if !self.takeoff {
                    events.push(Event::LeaveGround);
                }
                !self.takeoff
            } else if at_drop {
                self.ground = None;
                events.push(Event::LeaveGround);
                true
            } else {
                self.follow_ground(level, x, below, events)
            };
            if (left || (over_end && !self.grounded())) && drop > Fixed::ZERO {
                // There's no ground within a step below, so the drop
                // can't land. A jump waiting to take off drops under its
                // own gravity instead of `fall_gravity`, unless it hovers.
                let speed = self.vel.y;
                self.vel.y = match waiting_gravity.or(jump_gravity) {
                    Some(gravity) if !leave_hovers => drop - physics[Param::FallGravity] + gravity,
                    _ => drop,
                };
                if at_drop {
                    // The column's own floor is under the feet.
                    self.pos.y += self.vel.y;
                } else {
                    self.move_vertically(level, moved, Reach::NONE, slope > Fixed::ZERO, events);
                }
                self.vel.y = if left && self.through.is_some() {
                    // Falling through the floor keeps the drop's speed.
                    drop
                } else if left {
                    physics[Param::LeaveFallSpeed]
                } else {
                    // The jump rises from where the drop took it.
                    self.takeoff = false;
                    speed
                };
            }
        } else {
            // Stepping onto a column while falling has landed already.
            if !self.grounded() {
                self.move_vertically(
                    level,
                    moved,
                    Reach::new(physics),
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

        let leaving = events.contains(&Event::LeaveGround);
        if leaving && facing_wait > 0 && self.facing != start_facing && self.roll.is_none() {
            // Facing doesn't change on the leave frame, but follows a
            // timed roll.
            self.facing = start_facing;
            events.retain(|&e| e != Event::Turn);
        }
        if events.contains(&Event::JumpStart) && crouch_in_air {
            // Taking off ends a crouch.
            self.crouch.held = 0;
            self.crouch.rising = 0;
        }
        // Leaving the ground without crouching or rolling, or ending a
        // crouch in that fall, starts the fall proper on the next frame.
        let crouched_fall = crouch_in_air && self.crouch.held > 0;
        let starts_fall = !self.grounded()
            && ((leaving && !crouched_fall && self.roll.is_none())
                || (airborne && self.walked_off && crouch_ends));
        if starts_fall {
            self.fall_step = physics[Param::LeaveStepSpeed] > Fixed::ZERO && !face_leave;
            self.fall_facing = facing_wait.saturating_sub(1);
        }

        // Landing, or going on along the ground past the face, ends it.
        self.face_press &= !self.grounded() || face.is_some();
        self.coyote = if self.grounded() {
            None
        } else if crouched_fall {
            // A jump can start while crouching in the air.
            Some(0)
        } else if leaving && face.is_some() && physics[Param::NoFaceCoyote] > Fixed::ZERO {
            None
        } else if leaving {
            Some(1)
        } else {
            self.coyote.map(|n| n.saturating_add(1))
        }
        .filter(|&n| n <= frames(physics[Param::CoyoteTime]));

        self.after_face = face
            .filter(|_| physics[Param::FacePull] > Fixed::ZERO)
            .map(facing_of);
        self.face_target = held_target.filter(|_| face.is_some() && self.grounded());
        self.uphill_onto_face = face_wait
            && clear_start
            && self.grounded()
            && !held(Action::Action)
            && columns::face(level, columns::column(self.pos.x), self.pos.y, slope)
                .is_some_and(|way| held_dir == -way);
        // A press on a frame that ends on the ground, landing included,
        // also needs jump held up to the frame before its jump.
        if self.jump_buffer.age() == Some(0) {
            self.jump_needs_hold = physics[Param::JumpBufferHold] > Fixed::ZERO;
        }

        if timed {
            // Running frames before a roll make it fast; the roll's own
            // frames and its stop's don't count.
            if self.roll.is_none() && acting.is_none() {
                if held(Action::Action) && run_dir != 0 && self.roll_stop == 0 {
                    self.lone_run = self.since_run != Some(1) && start_vx == Fixed::ZERO;
                    self.since_run = Some(0);
                } else if (held(Action::Left) || held(Action::Right))
                    && physics[Param::RollLoneRunAnyDir] == Fixed::ZERO
                {
                    self.lone_run = false;
                }
            }
            let carried = self.roll.is_some_and(|r| r.carry);
            self.advance_timed_roll(held(Action::Action), physics, events);
            if carried && events.contains(&Event::RollEnd) && strides && self.grounded() {
                // A roll that carried on ends walking, whatever is held.
                self.gait = Gait::Walking(Stride::START);
            }
            if self.grounded() && !events.contains(&Event::Land) {
                self.air_stopped = false;
            }
        } else if rolling
            && (!self.grounded()
                || self.vel.x.abs() < physics[Param::RollMinSpeed]
                || self.rolls_into_moving(start_vx, held_dir, physics))
        {
            events.push(Event::RollEnd);
        }
        if self.roll_stop == 0 {
            self.stopped = None;
        }
        if std::mem::take(&mut self.jumping_off) && !self.grounded() {
            // A jump off an animal takes off after moving as in the air.
            self.vel.y = -physics[Param::JumpSpeed];
            self.approach = None;
            self.rolled_jump = false;
            self.state = State::Jump;
            events.push(Event::JumpStart);
        }
        let jump_age = if events.contains(&Event::JumpStart) {
            Some(0)
        } else {
            self.jump_age.map(|n| n.saturating_add(1))
        };
        let switch_from = frames(physics[Param::SwitchFrom]);
        if let Some(age) = jump_age
            && switch_from > 0
            && self.approach.is_none()
            && age >= switch_from
            && (self.grounded() || self.floor_within_reach(level, physics))
        {
            self.approach = Some(age);
            self.approach_face =
                columns::face_below(level, columns::column(self.pos.x), self.pos.y, slope)
                    .is_some();
        }
        if events.contains(&Event::Land) {
            let lockout = frames(
                physics[if self.walked_off {
                    Param::FallLandingLockout
                } else {
                    Param::LandingLockout
                }],
            );
            // A jump can start `lockout` frames after the switch frame,
            // which is otherwise the frame after landing.
            let (switch, settle) = match jump_age {
                Some(land) if switch_from > 0 && !self.rolled_jump => {
                    self.switch_wait(land, physics)
                }
                _ if (self.state == State::Bounce
                    || self.hopped_off.is_some()
                    || self.action_fall)
                    && physics[Param::BounceLandingSwitch] > Fixed::ZERO =>
                {
                    (0, 1 + frames(physics[Param::LandingModeWait]))
                }
                _ => (1, 1 + frames(physics[Param::LandingModeWait])),
            };
            // After a slide landing off the face, the jump waits for its
            // later switch frame (`face_top_landing`).
            let jump_switch = jump_age
                .filter(|_| switch_from > 0 && !self.rolled_jump)
                .and_then(|land| self.face_top_switch(level, land, physics))
                .unwrap_or(switch);
            self.lockout = (lockout + jump_switch).saturating_sub(1);
            if jump_age.is_some()
                && strides
                && self.grounded()
                && self.roll.is_none()
                && self.state != State::Roll
            {
                // After a jump the player is in no gait until its switch
                // frame or `settle`; one running on landing runs at once.
                self.gait = if switch == 0 && held(Action::Action) && held_dir != 0 {
                    Gait::Running(Stride::START)
                } else {
                    Gait::Landing(Landing {
                        frame: 0,
                        switch,
                        settle,
                        fixed: false,
                        bounce: false,
                    })
                };
            }
            let landed = strides && self.grounded() && self.roll.is_none();
            let fall_landing = frames(physics[Param::FaceFallLanding]);
            if landed && let Some(settle) = self.hopped_off {
                // After a hop off an animal the player is in no gait for
                // a while, then starts one as on a bounce's landing frame;
                // with Action and a direction held, it runs at once.
                self.gait = if held(Action::Action) && held_dir != 0 {
                    Gait::Running(Stride::START)
                } else {
                    Gait::Landing(Landing {
                        frame: 0,
                        switch: u8::MAX,
                        settle,
                        fixed: true,
                        bounce: true,
                    })
                };
            } else if landed && self.walked_off && self.walked_off_face && !self.rolled_off {
                if fall_landing > 0 {
                    // After a fall off a steep face the player is in no
                    // gait for a while, then starts one as at the end of
                    // a turn.
                    self.gait = Gait::Landing(Landing {
                        frame: 0,
                        switch: u8::MAX,
                        settle: fall_landing,
                        fixed: true,
                        bounce: false,
                    });
                }
            } else if landed
                && self.state == State::Bounce
                && physics[Param::BounceKeepsFacing] > Fixed::ZERO
            {
                // After a bounce a gait starts on the landing frame.
                self.start_bounce_gait(held(Action::Action), held_dir);
            }
            self.hopped_off = None;
        }
        if pushed {
            // A blocked frame's target counts as 0.
            let target = if stopped {
                Fixed::ZERO
            } else {
                self.target - push
            };
            self.push_down_face(level, events, jump_age, target, physics);
        }
        self.jump_age = if self.grounded() && !self.takeoff {
            None
        } else {
            jump_age
        };
        if leaving {
            self.walked_off_face = face.is_some();
            self.rolled_off = self.roll.is_some() || rolling;
        }
        self.walked_off = if events.contains(&Event::JumpStart) || self.grounded() {
            false
        } else {
            self.walked_off || events.contains(&Event::LeaveGround)
        };
        self.keg_fall &= !events.contains(&Event::JumpStart) && !self.grounded();
        if self.grounded() {
            self.hop = None;
        }
        // A fall that started in the ground action lands as a bounce
        // does, and Down doesn't count on the frame after its landing.
        self.action_fall = if leaving && acting.is_some() {
            true
        } else {
            self.action_fall && !(self.grounded() && !events.contains(&Event::Land))
        };
        // Going off a ledge doesn't end the ground action.
        self.ground_action = acting.filter(|_| goes_on);
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
                self.move_by_columns(level, slope, None, None);
                columns::middle(columns::column(self.pos.x))
            } else {
                self.move_horizontally(level, HALF_WIDTH, |_| true);
                self.pos.x
            };
            self.follow_ground(level, x, STEP, events);
            return;
        }
        self.vel.y = (self.vel.y + lost.gravity).min(physics[Param::MaxFallSpeed]);
        let moved = self.move_horizontally(level, HALF_WIDTH, |_| true);
        self.move_vertically(level, moved, Reach::new(physics), by_columns, events);
        if self.grounded() && lost.landed.is_none() {
            lost.landed = Some(0);
        }
    }

    /// One frame of being thrown out of a level's entrance, `since`
    /// frames after the throw, until the landing (see
    /// [`entrance`](crate::entrance)). Nothing held changes the motion
    /// until `steer_from` frames after the throw; from then the speed
    /// across eases toward the air's target speed. The player faces right
    /// throughout. The landing is as [`Player::land_from_entrance`] says,
    /// and from the frame after it the usual rules apply.
    pub(crate) fn thrown_out(
        &mut self,
        level: &Level,
        physics: &Physics,
        input: &Frame,
        values: &Entrance,
        since: u16,
        events: &mut Vec<Event>,
    ) {
        self.touched.clear();
        self.update_jump_held(input);
        if physics[Param::BounceLandingSwitch] > Fixed::ZERO {
            self.note_jump_in_throw(input, physics);
        }
        let held = |a| input.held.contains(a);
        let dir = match (held(Action::Left), held(Action::Right)) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        if since > u16::from(values.rise) {
            self.vel.y = (self.vel.y + values.gravity).min(values.max_fall);
        }
        if since >= u16::from(values.steer_from) {
            let cap = if held(Action::Action) {
                physics[Param::TopSpeed]
            } else {
                physics[Param::WalkSpeed]
            };
            self.target = cap * Fixed::from_int(dir);
            self.vel.x = ease(self.vel.x, self.target, values.steer_response);
        }
        self.move_through_air(level, physics, true, events);
        self.facing = Facing::Right;
        if self.grounded() {
            self.land_from_entrance(input, physics);
        }
        self.state = self.next_state(held(Action::Action), events);
    }

    /// Moves by the speed in the air as far as the level allows, landing
    /// on a floor it comes down on; only across without `down`.
    pub(crate) fn move_through_air(
        &mut self,
        level: &Level,
        physics: &Physics,
        down: bool,
        events: &mut Vec<Event>,
    ) {
        let slope = physics[Param::FaceSlope];
        let moved = if slope > Fixed::ZERO {
            self.move_in_air_by_columns(level, physics, events)
        } else {
            self.move_horizontally(level, HALF_WIDTH, |_| true)
        };
        if down && !self.grounded() {
            self.move_vertically(
                level,
                moved,
                Reach::new(physics),
                slope > Fixed::ZERO,
                events,
            );
        }
    }

    /// Starts the gait a bounce's landing frame starts, by what is held
    /// on it: running with Action and a direction, otherwise a turn with
    /// the way behind held, walking with the way ahead, or standing.
    fn start_bounce_gait(&mut self, action: bool, held_dir: i32) {
        let forward = direction(self.facing);
        self.gait = if action && held_dir != 0 {
            Gait::Running(Stride::START)
        } else if held_dir == -forward {
            self.turn = Some(0);
            Gait::Standing
        } else if held_dir == forward {
            Gait::Walking(Stride::START)
        } else {
            Gait::Standing
        };
    }

    /// On each frame of being thrown out of an entrance whose landing is a
    /// bounce's, a Jump press is noted as one in the air: it counts only if
    /// Jump is still held on the frames up to the one it's used on.
    fn note_jump_in_throw(&mut self, input: &Frame, physics: &Physics) {
        let press = input.pressed.contains(Action::Jump);
        let window = frames(physics[Param::JumpBuffer]).max(frames(physics[Param::JumpDelay]));
        self.jump_buffer.set_window(window);
        self.jump_buffer.update(press);
        self.jump_held_since_press =
            press || (self.jump_held_since_press && input.held.contains(Action::Jump));
        if press {
            self.jump_needs_hold = physics[Param::JumpBufferHold] > Fixed::ZERO;
        }
    }

    /// On the frame a thrown or shot player lands: a Jump press counts as
    /// on any other landing frame, and there's no landing lockout.
    pub(crate) fn land_from_throw(&mut self, input: &Frame, physics: &Physics) {
        let held = |a| input.held.contains(a);
        let window = frames(physics[Param::JumpBuffer]).max(frames(physics[Param::JumpDelay]));
        self.jump_buffer.set_window(window);
        self.jump_buffer
            .update(input.pressed.contains(Action::Jump));
        self.jump_held_since_press = input.pressed.contains(Action::Jump);
        self.ran = held(Action::Action) && (held(Action::Left) != held(Action::Right));
        self.lockout = 0;
    }

    /// On the frame the lead lands from the entrance's throw. With
    /// `bounce_landing_switch` the landing is a bounce's
    /// (level-start.md, "The landing"): the landing lockout counts from
    /// the landing frame, with `bounce_keeps_facing` a gait starts on it,
    /// and an Action press on it waits a frame for a roll. Without, it's
    /// a thrown player's landing ([`Player::land_from_throw`]).
    fn land_from_entrance(&mut self, input: &Frame, physics: &Physics) {
        if physics[Param::BounceLandingSwitch] == Fixed::ZERO {
            self.land_from_throw(input, physics);
            return;
        }
        let held = |a| input.held.contains(a);
        let held_dir = match (held(Action::Left), held(Action::Right)) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        self.ran = held(Action::Action) && held_dir != 0;
        self.lockout = frames(physics[Param::LandingLockout]).saturating_sub(1);
        if physics[Param::BounceKeepsFacing] > Fixed::ZERO {
            self.start_bounce_gait(held(Action::Action), held_dir);
        }
        if input.pressed.contains(Action::Action) {
            self.roll_press = Some((0, 1, true));
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
        self.thrown = self
            .thrown
            .map(|n| n.saturating_add(1))
            .filter(|&n| n < time(Param::ThrowTime));
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
        // The object was found at the end of the step before, with the
        // player on the ground then; it may have taken off since.
        if let Some(thing) = self.pick.take()
            && self.hands.is_none()
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
            if act == Act::Throw {
                self.thrown = Some(0);
            }
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
        self.holds.update(held(Action::Action));
        let down_blocks = physics[Param::GroundActionTime] > Fixed::ZERO
            || physics[Param::RollNotOnDown] > Fixed::ZERO;
        let press = if !input.pressed.contains(Action::Action) {
            self.roll_press
                .map(|(age, window, hold)| (age.saturating_add(1), window, hold))
        } else if !self.grounded() {
            Some((0, frames(physics[Param::RollAirBuffer]), true))
        } else if held(Action::Down) && down_blocks {
            // Down and action on the ground start the ground action, or
            // a crouch, instead.
            None
        } else if self.air_stopped {
            // On the frame after landing from a stop in the air, a press
            // waits a frame.
            Some((0, 1, true))
        } else if self.roll.is_some() || self.roll_stop > 0 {
            let hold = physics[Param::RollBufferHold] > Fixed::ZERO;
            Some((0, frames(physics[Param::RollBuffer]), hold))
        } else {
            Some((0, 0, false))
        };
        self.roll_press =
            press.filter(|&(age, window, hold)| age <= window && (!hold || held(Action::Action)));
    }

    /// Starts a timed roll from a remembered press if one can start now,
    /// with the frame starting on a steep face falling the way given, if
    /// any. Returns whether it did.
    fn start_timed_roll(
        &mut self,
        input: &Frame,
        start_face: Option<i32>,
        physics: &Physics,
    ) -> bool {
        let Some((age, _, _)) = self.roll_press else {
            return false;
        };
        let stop_time = frames(physics[Param::RollStopTime]);
        let after_stop = physics[Param::RollAfterStop] > Fixed::ZERO;
        // With `roll_stop_air`, a remembered press starts a roll on the
        // last frame of a stop that went on in the air.
        let in_air_stop = physics[Param::RollStopAir] > Fixed::ZERO
            && age > 0
            && self.roll_stop == stop_time
            && stop_time > 0;
        let can_start = (self.grounded() || in_air_stop)
            && self.swap == 0
            && self.roll.is_none()
            && !self.air_stopped
            && (self.roll_stop == 0 || (self.roll_stop == stop_time && !after_stop));
        if !can_start {
            return false;
        }
        // With `face_roll_wait`, not on a frame that starts on a face: the
        // press waits, held, for the first frame off it.
        let face_wait = frames(physics[Param::FaceRollWait]);
        if face_wait > 0 && start_face.is_some() {
            self.roll_press = Some((age, face_wait, true));
            return false;
        }
        let left = input.held.contains(Action::Left);
        let right = input.held.contains(Action::Right);
        let size = self.timed_roll_size(left, right, age, physics);
        // A roll starting on the stop's last frame stands still on it, and
        // lasts a frame longer if its press was made earlier, or always
        // with `roll_stop_press_lengthens`. It goes the way held then,
        // whether its press was remembered or new, as the stop turns the
        // player.
        let still = self.roll_stop > 0;
        let longer = age > 0 || physics[Param::RollStopPressLengthens] > Fixed::ZERO;
        let length = frames(physics[Param::RollTime]).saturating_add(u8::from(still && longer));
        let dir = if still && !after_stop {
            facing_toward(i32::from(right) - i32::from(left), self.facing)
        } else {
            self.facing
        };
        self.roll = Some(Roll {
            left: length,
            dir,
            size,
            speed: size.speed(physics),
            boosts: 0,
            defeated: false,
            held: true,
            carry: false,
            still,
            from_stop: still,
            stopped: false,
            since_press: age,
            air: 0,
            frame: 0,
        });
        self.roll_press = None;
        self.roll_stop = 0;
        true
    }

    /// The size of a timed roll whose press was `age` frames ago, with
    /// `left` and `right` held as its size is set.
    fn timed_roll_size(&self, left: bool, right: bool, age: u8, physics: &Physics) -> RollSize {
        let fast_hold = frames(physics[Param::RollFastHold]);
        let (dir_held, fast) = if fast_hold > 0 {
            // Action's last hold before the press counts, if it was long
            // and recent enough.
            let (hold, gap) = self.holds.before_press;
            let window = frames(physics[Param::RollFastWindow]);
            let dir_held = left != right;
            let recent = u32::from(gap) + u32::from(age) <= u32::from(window);
            (dir_held, dir_held && hold >= fast_hold && recent)
        } else {
            let dir_held = left || right;
            let window = frames(
                physics[if dir_held {
                    Param::RollFastWindow
                } else {
                    Param::RollFastWindowNeutral
                }],
            );
            let fast = self.since_run.is_some_and(|n| n <= window) && (dir_held || !self.lone_run);
            (dir_held, fast)
        };
        if fast {
            RollSize::Fast
        } else if dir_held {
            RollSize::Normal
        } else {
            RollSize::Slow
        }
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
        let mut speed = boost.size.speed(physics)
            + physics[Param::RollBoost] * Fixed::from_int(boost.boosts.into());
        let cap = physics[Param::RollBoostCap];
        if cap > Fixed::ZERO {
            speed = speed.min(cap);
        }
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
            from_stop: false,
            stopped: false,
            since_press: 0,
            air: 0,
            frame: 0,
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
        let lengthen = frames(physics[Param::RollAirLengthen]);
        let airborne = !self.grounded();
        // A jump waiting to take off holds the roll or stop where it is.
        let waiting = self.takeoff_wait.is_some();
        if self.roll_stop > 0 && !waiting {
            // Without `roll_air_lengthen`, the stop ends in the air, unless
            // `roll_stop_air`.
            let in_air = airborne && lengthen == 0 && physics[Param::RollStopAir] == Fixed::ZERO;
            let over = self.roll_stop >= stop_time || in_air;
            self.roll_stop = if over { 0 } else { self.roll_stop + 1 };
            if airborne && lengthen > 0 {
                if over {
                    self.fall_after_roll(physics);
                } else {
                    // A jump can start on any frame of the stop.
                    self.coyote = Some(0);
                }
            }
        }
        let Some(mut roll) = self.roll.take() else {
            return;
        };
        roll.held &= action;
        roll.still = false;
        roll.since_press = roll.since_press.saturating_add(1);
        roll.frame = roll.frame.saturating_add(1);
        let fell = roll.air;
        roll.air = if airborne {
            roll.air.saturating_add(1)
        } else {
            0
        };
        let carry_time = frames(physics[Param::RollCarryTime]);
        let carries = roll.defeated || (roll.held && roll.size != RollSize::Fast);
        let landed = lengthen == 0 && events.contains(&Event::Land);
        let fall_end = frames(physics[Param::RollFallEnd]);
        let ends_on_landing = landed && fall_end > 0 && fell == fall_end;
        if ends_on_landing {
            // The landing frame is the first after the roll.
            roll.left = 0;
        }
        let end = if waiting {
            roll.left = roll.left.saturating_sub(1);
            false
        } else if self.roll_jump {
            true
        } else if landed && !ends_on_landing {
            // A roll off a ledge ends as if it started just after landing.
            roll.left = frames(physics[Param::RollTime]);
            false
        } else if (lengthen == 0 && airborne) || roll.left > 0 {
            // It doesn't end in the air, or it's lengthened there every
            // `roll_air_lengthen` frames from its first.
            if lengthen > 0
                && airborne
                && roll.left == frames(physics[Param::RollTime]).saturating_sub(lengthen)
            {
                roll.left = roll.left.saturating_add(lengthen);
            }
            roll.left = roll.left.saturating_sub(1);
            false
        } else if !roll.carry && carries && carry_time > 0 {
            roll.carry = true;
            roll.left = carry_time - 1;
            false
        } else {
            if !roll.carry && !carries && stop_time > 0 {
                self.roll_stop = 1;
                self.stopped = Some(roll);
                if physics[Param::RollStopSlide] == Fixed::ZERO {
                    // With no slide, the roll's last frame ends standing.
                    self.vel.x = Fixed::ZERO;
                }
                if airborne {
                    self.air_stopped = physics[Param::RollAirStopWait] > Fixed::ZERO;
                    self.coyote = Some(0);
                }
            } else if airborne {
                self.fall_after_roll(physics);
            }
            true
        };
        if end {
            events.push(Event::RollEnd);
        } else {
            self.roll = Some(roll);
        }
    }

    /// The speed of a frame of a stopped timed roll, before it carries
    /// on, or of one that this frame stops: on the ground, starting
    /// slowly with no floor ahead (`roll_edge_speed`). A stopped roll
    /// eases toward 0, or down the steep face `face` falls if it starts
    /// on one, until it pushes off with `dir` held the way it goes, on a
    /// frame that doesn't start on a face. `None` for any other frame.
    fn stopped_roll(
        &mut self,
        level: &Level,
        dir: i32,
        face: Option<i32>,
        physics: &Physics,
    ) -> Option<Fixed> {
        let mut roll = self.roll.filter(|r| !r.carry)?;
        let way = direction(roll.dir);
        let slow = physics[Param::RollEdgeSpeed];
        if !roll.stopped && slow > Fixed::ZERO && self.grounded() && self.vel.x.abs() <= slow {
            let ahead = physics[Param::RollEdgeAhead].floor() * i64::from(way);
            let depth = match physics[Param::RollEdgeDepth] {
                Fixed::ZERO => STEP + STEP,
                depth => depth,
            };
            let column = columns::column(self.pos.x) + ahead;
            if !columns::ground_down_to(level, column, self.pos.y, depth) {
                roll.stopped = true;
                self.roll = Some(roll);
                return Some(Fixed::ZERO);
            }
        }
        if !roll.stopped {
            return None;
        }
        let push = physics[Param::RollEdgePush];
        let vx = if push > Fixed::ZERO && face.is_none() && roll.frame > 0 && dir == way {
            // It goes on at the push's speed until steered.
            roll.stopped = false;
            roll.speed = push;
            push * Fixed::from_int(way)
        } else if roll.still {
            Fixed::ZERO
        } else {
            self.target = face.map_or(Fixed::ZERO, |way| {
                physics[Param::FaceSpeed] * Fixed::from_int(way)
            });
            ease(self.vel.x, self.target, physics[Param::SpeedResponse])
        };
        self.roll = Some(roll);
        Some(vx)
    }

    /// Makes the frame after a timed roll, or its stop, ends in the air
    /// fall as the frame after leaving the ground does. A jump can start
    /// on it.
    fn fall_after_roll(&mut self, physics: &Physics) {
        self.fall_step = physics[Param::LeaveStepSpeed] > Fixed::ZERO;
        self.fall_facing = frames(physics[Param::LeaveFacingWait]).saturating_sub(1);
        self.coyote = (frames(physics[Param::CoyoteTime]) >= 1).then_some(1);
    }

    /// Goes on with the turn under way, or, if `start`, starts one if
    /// `dir` points behind the player, and returns the facing for this
    /// frame. Held `toward_rise`, the player faces `dir` at once, before
    /// the turn's own change.
    fn turn_on_ground(
        &mut self,
        dir: i32,
        turn_time: u8,
        start: bool,
        toward_rise: bool,
    ) -> Facing {
        let mut facing = self.facing;
        let length = turn_time.saturating_sub(self.short_turn.into());
        self.turn = self.turn.map(|n| n + 1).filter(|&n| n < length);
        if toward_rise {
            facing = facing_of(dir);
        }
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
    /// holding `dir`, and starts a turn when one can start and `turn_dir`
    /// points behind the player. `turn_ends` says the turn under way
    /// ended this frame. Returns whether facing changes now, which only a
    /// turn shorter than 2 frames does.
    fn stride(
        &mut self,
        dir: i32,
        turn_dir: i32,
        action: bool,
        turn_ends: bool,
        physics: &Physics,
    ) -> bool {
        if self.turn.is_some() {
            return false;
        }
        let forward = direction(self.facing);
        let opposite = turn_dir == -forward;
        let v = self.vel.x.abs();
        let steps = physics[Param::StrideSteps]
            .floor()
            .clamp(1, MAX_STEP.into()) as u32;
        let has =
            |set: Param, step: u32| clamber_pack::physics::steps(physics[set]).any(|n| n == step);
        let mut turn = false;
        // A fall's landing frames end as a turn does.
        let ends_as_turn = turn_ends || matches!(self.gait, Gait::Landing(l) if l.fixed);
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
            Gait::Landing(mut landing) if landing.fixed && landing.frame + 1 < landing.settle => {
                landing.frame += 1;
                Gait::Landing(landing)
            }
            Gait::Landing(mut landing) if !landing.fixed => {
                landing.frame += 1;
                if landing.frame == landing.switch && action && dir != 0 {
                    Gait::Running(Stride::START)
                } else if landing.frame < landing.settle {
                    Gait::Landing(landing)
                } else if dir == 0 {
                    Gait::Standing
                } else if action {
                    Gait::Running(Stride::START)
                } else {
                    Gait::Walking(Stride::START)
                }
            }
            // At the end of a hop's landing frames, as on a bounce's
            // landing frame.
            Gait::Landing(landing) if landing.bounce => {
                if action && dir != 0 {
                    Gait::Running(Stride::START)
                } else if opposite {
                    turn = true;
                    Gait::Standing
                } else if dir == forward {
                    Gait::Walking(Stride::START)
                } else {
                    Gait::Standing
                }
            }
            // Standing, at the end of a turn, or at the end of a fall's
            // landing frames.
            _ => {
                if opposite {
                    turn = true;
                    Gait::Standing
                } else if dir == 0 {
                    Gait::Standing
                } else if ends_as_turn && action && v >= physics[Param::TurnRunSpeed] {
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

    /// Whether there's floor within reach below the player for its
    /// approach frame (`switch_reach`).
    fn floor_within_reach(&self, level: &Level, physics: &Physics) -> bool {
        columns::floor_within(
            level,
            columns::column(self.pos.x),
            self.pos.y,
            physics[Param::SwitchReach].floor(),
            physics[Param::ViewBand].floor(),
            physics[Param::ViewReach].floor(),
        )
    }

    /// How many frames after a jump's landing frame its switch frame is,
    /// and how many until walking, running or standing starts if running
    /// doesn't start on the switch frame, for a landing `land` frames
    /// after the jump started.
    fn switch_wait(&self, land: u8, physics: &Physics) -> (u8, u8) {
        let from = frames(physics[Param::SwitchFrom]);
        // Landing before the approach frame, the player is on the floor
        // when the approach frame comes.
        let approach = self.approach.unwrap_or(from).max(from);
        let set = if approach > from {
            Param::SwitchLateSteps
        } else {
            Param::SwitchSteps
        };
        let stage = |step: u32| u32::from(approach) + step - 1;
        let after = clamber_pack::physics::steps(physics[set])
            .filter(|&step| stage(step) > u32::from(land))
            .count();
        let switch = approach.saturating_sub(land) + after as u8;
        let settle = if switch > 0 {
            switch + frames(physics[Param::LandingModeWait])
        } else if clamber_pack::physics::steps(physics[set])
            .last()
            .is_some_and(|step| stage(step) == u32::from(land))
        {
            frames(physics[Param::LandingModeOnLast])
        } else {
            frames(physics[Param::LandingModeAfterLast])
        };
        (switch, settle)
    }

    /// For a slide landing `land` frames after the jump started that ends
    /// on the floor off a face, how many frames after the landing frame
    /// its switch frame is (`face_top_landing`).
    fn face_top_switch(&self, level: &Level, land: u8, physics: &Physics) -> Option<u8> {
        let after = frames(physics[Param::FaceTopLanding]);
        let approach = self.approach.filter(|&a| a <= land)?;
        let on_face = columns::face(
            level,
            columns::column(self.pos.x),
            self.pos.y,
            physics[Param::FaceSlope],
        )
        .is_some();
        (after > 0 && self.approach_face && self.grounded() && !on_face).then(|| {
            (approach + after).saturating_sub(land) + frames(physics[Param::FaceSlideSwitch])
        })
    }

    /// At the end of a frame, after a landing, says whether the switch
    /// frame steep faces wait for has come, and works out the push down a
    /// face for the next frame (`face_slide_switch`). `jump_age` is the
    /// jump's age on this frame, and `target` this frame's target speed
    /// without the push.
    fn push_down_face(
        &mut self,
        level: &Level,
        events: &[Event],
        jump_age: Option<u8>,
        target: Fixed,
        physics: &Physics,
    ) {
        let from = frames(physics[Param::SwitchFrom]);
        if events.contains(&Event::Land) {
            // Other landings have their switch frame on the landing frame.
            self.face_switch = match jump_age {
                Some(land) if from > 0 && !self.rolled_jump => {
                    let (switch, _) = self.switch_wait(land, physics);
                    let approach = self.approach.unwrap_or(from).max(from);
                    Some(if approach > land {
                        let left = approach - land;
                        FaceSwitch::Approach {
                            left,
                            plain: switch - left,
                        }
                    } else if let Some(top) = self.face_top_switch(level, land, physics) {
                        // Counted down at the end of this frame.
                        FaceSwitch::Plain(top)
                    } else if self.approach_face {
                        FaceSwitch::Slide(None)
                    } else {
                        FaceSwitch::Plain(switch)
                    })
                }
                _ => None,
            };
        }
        let face = if self.grounded() {
            columns::face(
                level,
                columns::column(self.pos.x),
                self.pos.y,
                physics[Param::FaceSlope],
            )
        } else {
            None
        };
        let down = |v: Fixed, way: i32| v * Fixed::from_int(way) >= Fixed::ZERO;
        // On the floor of a face column with no speed or speed down it,
        // the switch frame has come at the latest.
        let stopped = face.is_some_and(|way| down(self.vel.x, way));
        self.face_switch = self
            .face_switch
            .filter(|_| self.grounded() && !stopped)
            .and_then(|s| s.advance(face.is_some(), frames(physics[Param::FaceSlideSwitch])));
        self.push = match face {
            Some(way) if self.face_switch.is_none() => {
                if down(target, way) && down(self.vel.x, way) {
                    physics[Param::FaceSpeed] * Fixed::from_int(way)
                } else {
                    self.push
                }
            }
            _ => Fixed::ZERO,
        };
    }

    /// The state after this step's movement.
    pub(crate) fn next_state(&self, running: bool, events: &[Event]) -> State {
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
    /// stops it at the left edge of the column it was in. Going under a
    /// platform's top too high to step onto, it falls through the floor
    /// it was on (see [`columns::falls_through`]). Starting on a
    /// steep face falling the way given, a body that isn't stopped has its
    /// speed limited by the physics given first (`face_speed_limit`,
    /// `face_min_speed`). With a `rise` given, a column whose floor is
    /// more than that above the body's feet stops it too.
    /// Whether, with `leave_step_near`, the floor of the column this
    /// frame's move goes into is that close below the feet or closer.
    fn floor_just_below(&self, level: &Level, physics: &Physics) -> bool {
        let near = physics[Param::LeaveStepNear];
        let feet = self.pos.y;
        near > Fixed::ZERO
            && columns::spans(level, columns::column(self.pos.x + self.vel.x))
                .iter()
                .any(|s| s.top >= feet && s.top - feet <= near)
    }

    pub(crate) fn move_by_columns(
        &mut self,
        level: &Level,
        slope: Fixed,
        face: Option<(i32, &Physics)>,
        rise: Option<Fixed>,
    ) -> Fixed {
        // On a face the speed is limited first, and the limited speed is
        // the one stopped by a rise.
        if let Some((way, physics)) = face {
            self.vel.x = limit_on_face(self.vel.x, way, physics);
        }
        if self.vel.x == Fixed::ZERO {
            return Fixed::ZERO;
        }
        let dir = if self.vel.x.is_negative() { -1 } else { 1 };
        let from = columns::column(self.pos.x);
        let to = columns::column(self.pos.x + self.vel.x);
        // A move that ends under a platform's top too high to step onto
        // falls through the floor, whatever it passes on the way.
        let through = |c| columns::falls_through(level, c, self.pos.y);
        let mut falls = through(to);
        let mut next = from;
        let mut past_face = false;
        while next != to && !falls {
            next += i64::from(dir);
            falls = through(next);
            if falls {
                break;
            }
            let high = rise
                .and_then(|rise| {
                    columns::floor(level, next, self.pos.y).filter(|f| self.pos.y - f.y > rise)
                })
                .map(|f| f.collider);
            let mut blocked = columns::blocks(level, next, dir, self.pos.y, slope);
            if next != to && blocked.is_some() && !columns::steps_up(level, next, dir, self.pos.y) {
                // Only the column the move ends in counts as a face
                // column it can't go into. Past a face's top it is
                // stopped by floor more than a step above where it was.
                blocked = None;
                past_face = true;
            } else if next == to && past_face && blocked.is_none() {
                blocked = columns::floor(level, from, self.pos.y)
                    .zip(columns::floor(level, to, self.pos.y))
                    .filter(|(was, end)| was.y - end.y > STEP)
                    .map(|(_, end)| end.collider);
            }
            if let Some(collider) = blocked.or(high) {
                let edge = columns::left_edge(from);
                let moved = (self.pos.x - edge).abs();
                self.pos.x = edge;
                self.vel.x = Fixed::ZERO;
                self.touched.push(collider);
                return moved;
            }
        }
        if falls {
            self.through = self.ground;
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
    pub(crate) fn move_in_air_by_columns(
        &mut self,
        level: &Level,
        physics: &Physics,
        events: &mut Vec<Event>,
    ) -> Fixed {
        let slope = physics[Param::FaceSlope];
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
        // How far the floor there is above the feet.
        let rise = |floor: &columns::Floor| {
            if physics[Param::AirWholePixels] > Fixed::ZERO {
                Fixed::from_int(feet.floor() as i32) - floor.y
            } else {
                feet - floor.y
            }
        };
        let floor = (to != from)
            .then(|| columns::floor_in_air(level, to, feet))
            .flatten()
            .filter(|f| Some(f.collider) != self.through)
            .map(|f| (rise(&f), f))
            .filter(|(rise, _)| *rise > Fixed::ZERO);
        if let Some((rise, floor)) = floor {
            let step = match physics[Param::AirStep] {
                Fixed::ZERO => STEP,
                step => step,
            };
            if rise > step || columns::face(level, to, feet, slope).is_some() {
                let edge = columns::left_edge(from);
                let moved = (self.pos.x - edge).abs();
                self.pos.x = edge;
                self.vel.x = Fixed::ZERO;
                self.touched.push(floor.collider);
                return moved;
            }
            if falls(self.vel.y, physics) {
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
    /// slopes and bumps, up to a step up or `below` down, or lets it
    /// leave the ground when there's none within reach. A body taking off
    /// keeps its upward speed. Returns whether the body walked off the
    /// ground.
    fn follow_ground(
        &mut self,
        level: &Level,
        x: Fixed,
        below: Fixed,
        events: &mut Vec<Event>,
    ) -> bool {
        let from = Vec2::new(x, self.pos.y - STEP);
        let ground = match self.through {
            Some(_) => None,
            None => level.collision.cast(from, Dir::Down, STEP + below),
        };
        match ground {
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
    /// sideways. Falling, the body lands if it ends near enough the ground
    /// for `reach`. `by_columns`, rising is stopped by the feet, not
    /// the head (see [`Player::rise_by_columns`]), and a fall that ends
    /// below a one-way platform's underside doesn't land on its top.
    pub(crate) fn move_vertically(
        &mut self,
        level: &Level,
        moved: Fixed,
        reach: Reach,
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
        let reach = if self.vel.y > Fixed::ZERO || (reach.still && self.vel.y == Fixed::ZERO) {
            reach
        } else {
            Reach::NONE
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
            above + self.vel.y + reach.most(),
            (Fixed::ZERO, Fixed::ZERO),
            |hit| {
                Some(hit.collider) != self.through
                    && (!hit.one_way || (hit.point.y >= feet && !below_underside(hit)))
            },
        );
        match ground {
            Some(hit) if reach.lands(level, self.pos.x, hit.point.y, to) => {
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
    pub(crate) fn land(&mut self, y: Fixed, collider: usize, events: &mut Vec<Event>) {
        self.pos.y = y;
        self.vel.y = Fixed::ZERO;
        self.ground = Some(collider);
        self.spring = None;
        self.spring_arc = None;
        self.rope = None;
        self.rope_fall = false;
        self.through = None;
        self.jump_hold = false;
        self.face_press = false;
        self.unseated = false;
        self.touched.push(collider);
        events.push(Event::Land);
    }
}

/// How near the ground a falling body lands.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Reach {
    /// `landing_reach`.
    flat: Fixed,
    /// `slope_landing_reach`, over a column of a sloped piece.
    sloped: Fixed,
    /// `slope_piece`, in columns.
    piece: i64,
    /// `face_slope`.
    slope: Fixed,
    /// `still_falls`: a body with no vertical speed lands within reach.
    still: bool,
}

impl Reach {
    /// Only on reaching the ground.
    const NONE: Reach = Reach {
        flat: Fixed::ZERO,
        sloped: Fixed::ZERO,
        piece: 0,
        slope: Fixed::ZERO,
        still: false,
    };

    pub(crate) fn new(physics: &Physics) -> Reach {
        Reach {
            flat: physics[Param::LandingReach],
            sloped: physics[Param::SlopeLandingReach],
            piece: physics[Param::SlopePiece].floor(),
            slope: physics[Param::FaceSlope],
            still: physics[Param::StillFalls] > Fixed::ZERO,
        }
    }

    /// The farthest above the ground it lands.
    fn most(self) -> Fixed {
        self.flat.max(self.sloped)
    }

    /// Whether a body at `x` that would end a frame with its feet at `to`
    /// lands on ground at `ground`. `slope_landing_reach` applies over a
    /// sloped piece and over a steep face's columns.
    pub(crate) fn lands(self, level: &Level, x: Fixed, ground: Fixed, to: Fixed) -> bool {
        let above = ground - to;
        let column = columns::column(x);
        above <= Fixed::ZERO
            || above < self.flat
            || (above <= self.sloped
                && self.slope > Fixed::ZERO
                && (columns::face(level, column, ground, self.slope).is_some()
                    || (self.piece > 0
                        && columns::sloped(level, column, ground, self.slope, self.piece))))
    }
}

/// How much farther than `follow_drop` the ground is followed down from
/// `column`, by how steep its sloped piece is (`follow_steep_slope`).
fn steeper(level: &Level, column: i64, feet: Fixed, physics: &Physics) -> Fixed {
    let steep = physics[Param::FollowSteepSlope];
    if steep <= Fixed::ZERO || physics[Param::FollowDrop] <= Fixed::ZERO {
        return Fixed::ZERO;
    }
    let slope = physics[Param::FaceSlope];
    let piece = physics[Param::SlopePiece].floor();
    match columns::gradient(level, column, feet, slope, piece) {
        g if g >= steep => Fixed::ONE + Fixed::from_int(g.floor() as i32),
        _ => Fixed::ZERO,
    }
}

/// A speed `vx` on a steep face falling `way` (-1 or 1), kept from
/// `face_min_speed` to just under `face_speed_limit` down the face, or
/// with a fraction `face_kept_fraction` keeps, and otherwise limited.
fn limit_on_face(vx: Fixed, way: i32, physics: &Physics) -> Fixed {
    let way = Fixed::from_int(way);
    let down = vx * way;
    let limit = physics[Param::FaceSpeedLimit];
    let min = physics[Param::FaceMinSpeed];
    let kept = physics[Param::FaceKeptFraction];
    // Up the face, a speed down it counts from the whole pixel above.
    let up = -down;
    let fraction = up - Fixed::from_int(up.floor() as i32);
    if (min..limit).contains(&down)
        || (kept > Fixed::ZERO && (fraction == Fixed::ZERO || fraction > kept))
    {
        vx
    } else if limit > Fixed::ZERO && down >= limit {
        physics[Param::FaceLimitedSpeed] * way
    } else if min > Fixed::ZERO && down < min {
        min * way
    } else {
        vx
    }
}

/// Whether an airborne body with vertical speed `vy` counts as falling.
fn falls(vy: Fixed, physics: &Physics) -> bool {
    vy > Fixed::ZERO || (vy == Fixed::ZERO && physics[Param::StillFalls] > Fixed::ZERO)
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
    pub frame: u16,
    /// Landings so far in the flight.
    pub landings: u8,
    /// Once the flight has stopped, how many more frames the player lies
    /// still.
    pub rest: Option<u8>,
    /// The frame of the hop (`knockout_hop_at`), once it has come.
    pub hop: Option<u16>,
}

impl Knockout {
    /// A knock-out whose hit frame is this one.
    pub fn new(by: Option<usize>) -> Knockout {
        Knockout {
            by,
            frame: 0,
            landings: 0,
            rest: None,
            hop: None,
        }
    }
}

/// The ground action under way. The walkers it knocks out are in
/// [`things`](crate::things).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GroundAction {
    /// Frames since its press frame, which is 0, leaving out the frames
    /// it pauses for after knocking something out.
    pub frame: u8,
    /// The way it goes: -1, 0 or 1.
    pub dir: i32,
    /// The speed it eases toward off a face, if not `dir` times
    /// `ground_action_speed`: next to a rise.
    pub speed: Option<Fixed>,
    /// It started in the air, on frame 1 of a resume, and goes on in the
    /// air (restart-barrel.md).
    pub air: bool,
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
    /// It started on the last frame of the stop after a roll, standing
    /// still on it.
    pub from_stop: bool,
    /// Stopped before a ledge (`roll_edge_speed`) or by a rise
    /// (`face_rolls`), until it pushes off.
    pub stopped: bool,
    /// Frames since the action press that started it.
    pub since_press: u8,
    /// Frames it has ended in the air in a row.
    pub air: u8,
    /// Frames since its first, 0 on it.
    pub frame: u8,
}

/// Which roll speed a timed roll eases toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RollSize {
    Slow,
    Normal,
    Fast,
}

/// Action's holds, for the size of a timed roll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Holds {
    /// Frames action has been held in a row, up to this one.
    pub held: u8,
    /// How long its last hold before this one lasted.
    pub last: u8,
    /// Frames without action since that hold ended.
    pub gap: u8,
    /// The last hold before the latest press, and the frames without
    /// action between them.
    pub before_press: (u8, u8),
}

impl Holds {
    /// Goes on to a frame with action `held` or not.
    fn update(&mut self, held: bool) {
        if !held {
            if self.held > 0 {
                self.last = self.held;
                self.held = 0;
                self.gap = 0;
            }
            self.gap = self.gap.saturating_add(1);
        } else {
            if self.held == 0 {
                self.before_press = (self.last, self.gap);
            }
            self.held = self.held.saturating_add(1);
        }
    }
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
    /// Holding the way the roll goes makes a slower roll normal, or
    /// `roll_steer_speed`, except on the frame it's boosted; holding the
    /// other way turns it around.
    fn steer(&mut self, dir: i32, boosted: bool, physics: &Physics) {
        let along = direction(self.dir);
        let normal = match physics[Param::RollSteerSpeed] {
            Fixed::ZERO => physics[Param::RollSpeed],
            steer => steer,
        };
        let keeps = physics[Param::RollKeepsSize] > Fixed::ZERO;
        if dir == along && !boosted && !keeps && self.speed < normal {
            self.speed = normal;
        } else if dir == -along {
            self.dir = facing_toward(dir, self.dir);
        }
    }

    /// Whether this frame of the roll drifts down in the air if it left
    /// the ground on its frame `leave` (`roll_leave_hover`).
    fn leave_hovers(&self, leave: u8, physics: &Physics) -> bool {
        let last = frames(physics[Param::RollLeaveHover]);
        let time = frames(physics[Param::RollLeaveHoverTime]);
        let after = self.frame.saturating_sub(leave);
        // A roll from the stop counts from the frame it stood still on,
        // and hovers a frame longer for each way of leaving.
        let shift = u8::from(self.from_stop);
        last > 0
            && match leave {
                0 => after >= 1 && after + 2 <= time + shift,
                l if l < last + shift => after < time,
                l if l == last + shift => after == 0,
                _ => false,
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
    /// On the frames after a jump's landing, or a fall's off a steep
    /// face, before any of the others starts. No turn starts.
    Landing(Landing),
}

/// What the switch frame after a landing that steep faces wait for is
/// waiting for (`face_slide_switch`). Each count is of frames from the
/// one ending next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceSwitch {
    /// The approach frame, this many frames on. The switch frame is
    /// `plain` frames after it, unless it ends in a face column.
    Approach { left: u8, plain: u8 },
    /// A plain landing: the switch frame, this many frames on.
    Plain(u8),
    /// A slide landing: the switch frame, this many frames on, once a
    /// frame has ended on the floor off a face.
    Slide(Option<u8>),
}

impl FaceSwitch {
    /// After a frame that ended on the floor, in a face column or not:
    /// `None` if it was the switch frame. A slide landing's switch frame
    /// is `wait` frames after the frame that ends off a face.
    fn advance(self, on_face: bool, wait: u8) -> Option<FaceSwitch> {
        match self {
            FaceSwitch::Approach { left: 0, .. } if on_face => {
                FaceSwitch::Slide(None).advance(on_face, wait)
            }
            FaceSwitch::Approach { left: 0, plain } => {
                FaceSwitch::Plain(plain).advance(on_face, wait)
            }
            FaceSwitch::Approach { left, plain } => Some(FaceSwitch::Approach {
                left: left - 1,
                plain,
            }),
            FaceSwitch::Plain(0) | FaceSwitch::Slide(Some(0)) => None,
            FaceSwitch::Plain(n) => Some(FaceSwitch::Plain(n - 1)),
            FaceSwitch::Slide(None) if on_face => Some(self),
            FaceSwitch::Slide(None) => FaceSwitch::Slide(Some(wait)).advance(on_face, wait),
            FaceSwitch::Slide(Some(n)) => Some(FaceSwitch::Slide(Some(n - 1))),
        }
    }
}

/// How far into the frames after a jump's landing the player is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Landing {
    /// Frames since the landing frame.
    pub frame: u8,
    /// The frame after the landing frame running can start on, with
    /// action and a direction held.
    pub switch: u8,
    /// The frame after the landing frame walking, running or standing
    /// starts on otherwise.
    pub settle: u8,
    /// After a fall rather than a jump: facing doesn't change, and the
    /// gait starts on `settle` as at the end of a turn.
    pub fixed: bool,
    /// ... or, after a hop off an animal, as on a bounce's landing frame.
    pub bounce: bool,
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
    /// Frames left in which no turn starts after a crouch ends.
    pub turn_lock: u8,
}

impl Crouch {
    /// Advances one frame, with `down` held or not and the player
    /// `on_ground` (not rolling) or not, and returns whether the player is
    /// crouching on this frame. On the ground, down counts as held until
    /// it has been for `min` frames. A crouch that ends locks turns for
    /// `turn_lock` frames from this one.
    fn update(&mut self, down: bool, on_ground: bool, physics: &Physics) -> bool {
        let start = frames(physics[Param::CrouchStart]);
        if start == 0 {
            *self = Crouch::default();
            return false;
        }
        let min = frames(physics[Param::CrouchMin]);
        self.turn_lock = self.turn_lock.saturating_sub(1);
        if on_ground && (down || (self.held > 0 && self.held < min)) {
            self.held = self.held.saturating_add(1);
        } else {
            if self.held >= start {
                self.rising = frames(physics[Param::CrouchRecovery]);
                self.turn_lock = frames(physics[Param::CrouchTurnLock]);
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
pub(crate) fn direction(facing: Facing) -> i32 {
    match facing {
        Facing::Left => -1,
        Facing::Right => 1,
    }
}

/// Left for a negative `dir`, right otherwise.
pub(crate) fn facing_of(dir: i32) -> Facing {
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

    /// `steep-faces.md`, "Speed on a face (`title-2`)": the limits on a
    /// face falling left, for every measured speed `s` (1/256), and on
    /// one falling right.
    #[test]
    fn classic_2_limits_on_a_face_keep_some_speeds() {
        let p = clamber_pack::Presets::builtin().world("classic-2").unwrap();
        let limit =
            |s: i64, way| (limit_on_face(Fixed::from_bits(s << 24), way, &p).to_bits()) >> 24;
        let table: [(i64, i64, Option<i64>); 15] = [
            (-1154, -1088, Some(-512)),
            (-1087, -1024, None),
            (-1023, -832, Some(-512)),
            (-831, -768, None),
            (-767, -576, Some(-512)),
            (-575, -192, None),
            (-191, -64, Some(-192)),
            (-63, 0, None),
            (1, 192, Some(-192)),
            (193, 256, None),
            (257, 448, Some(-192)),
            (449, 512, None),
            (513, 704, Some(-192)),
            (769, 960, Some(-192)),
            (1025, 1090, Some(-192)),
        ];
        for (from, to, after) in table {
            for s in from..=to {
                assert_eq!(limit(s, -1), after.unwrap_or(s), "{s}");
            }
        }
        // Falling right, downhill speeds the other way round.
        for s in 19..=448 {
            let after = match s {
                ..=62 | 196.. => s,
                68..=190 => 192,
                _ => continue,
            };
            assert_eq!(limit(s, 1), after, "{s}");
        }
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
