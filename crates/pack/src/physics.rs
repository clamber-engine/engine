//! Movement parameters and the patches that adjust them.
//!
//! Units follow `spec/behavior/README.md`: pixels, and frames at 60 Hz.
//! Speeds are pixels per frame, accelerations pixels per frame per frame,
//! and +y points down.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Index;

use clamber_fixed::Fixed;

macro_rules! params {
    ($($(#[$meta:meta])* $variant:ident => $name:literal,)*) => {
        /// A movement parameter. The world sets every one; characters,
        /// presets and level regions adjust them.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Param {
            $($(#[$meta])* $variant,)*
        }

        impl Param {
            /// Every parameter, in declaration order.
            pub const ALL: &'static [Param] = &[$(Param::$variant,)*];

            /// The name used in content files.
            pub fn name(self) -> &'static str {
                match self {
                    $(Param::$variant => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<Param> {
                match name {
                    $($name => Some(Param::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

params! {
    /// Added to vertical speed every airborne frame while rising. May be
    /// negative.
    Gravity => "gravity",
    /// Used instead of `gravity` once the body stops rising, so falls can
    /// be quicker than climbs. May be negative.
    FallGravity => "fall_gravity",
    /// Above 0, the gravity of a jump while jump has been held on every
    /// frame since its press, rising or falling, used instead of
    /// `gravity` and `fall_gravity`. Once let go, the jump goes back to
    /// those for good. 0 turns it off.
    HeldJumpGravity => "held_jump_gravity",
    /// Downward speed never exceeds this.
    MaxFallSpeed => "max_fall_speed",
    /// A falling body lands when it ends a frame less than this far above
    /// the ground, and is placed on it. 0: only on reaching the ground.
    LandingReach => "landing_reach",
    /// Above 0, how far a body that walks off the ground without jumping
    /// moves down on the frame it leaves, whatever is below; it ends that
    /// frame falling at `leave_fall_speed`. 0: it doesn't move down until
    /// the next frame.
    LeaveDrop => "leave_drop",
    /// Downward speed at the end of the frame a body walks off the ground,
    /// when `leave_drop` is above 0.
    LeaveFallSpeed => "leave_fall_speed",
    /// Speed gained per frame when pressing in the direction of motion.
    GroundAcceleration => "ground_acceleration",
    /// Speed lost per frame when pressing against the direction of motion.
    GroundDeceleration => "ground_deceleration",
    /// Speed lost per frame on the ground with no direction pressed.
    GroundFriction => "ground_friction",
    /// Running speed (action held) reached by acceleration alone.
    TopSpeed => "top_speed",
    /// Walking speed (action not held) reached by acceleration alone.
    WalkSpeed => "walk_speed",
    /// Horizontal speed gained or lost per frame in the air.
    AirAcceleration => "air_acceleration",
    /// Above 0, horizontal speed eases toward a target instead of using
    /// the acceleration, deceleration and friction values: every frame,
    /// on the ground and in the air, it closes this fraction of the gap
    /// to `walk_speed`, `top_speed` with action held, or 0 with no
    /// direction. 0 turns easing off.
    SpeedResponse => "speed_response",
    /// Holding down on the ground crouches from this frame of holding it
    /// on (1 is at once). A crouching player doesn't move along the
    /// ground: it slows as if no direction were held. 0: down doesn't
    /// crouch. Whole frames.
    CrouchStart => "crouch_start",
    /// After letting go of down, a crouch lasts this many frames more.
    /// Whole frames.
    CrouchRecovery => "crouch_recovery",
    /// Turning around on the ground takes this many frames, and facing
    /// changes halfway through. A turn, once started, finishes. 0: facing
    /// follows the direction held at once. Whole frames.
    TurnTime => "turn_time",
    /// Upward speed at the start of a jump.
    JumpSpeed => "jump_speed",
    /// Releasing jump cuts upward speed down to this.
    JumpReleaseSpeed => "jump_release_speed",
    /// A jump pressed up to this many frames before it's possible, such
    /// as just before landing, still happens. Whole frames.
    JumpBuffer => "jump_buffer",
    /// A jump still works for this many frames after walking off a
    /// ledge. Whole frames.
    CoyoteTime => "coyote_time",
    /// Above 0, a jump leaves the ground on the frame after it starts:
    /// on the start frame the body still follows the ground, with its
    /// upward speed already set. 0 leaves on the start frame.
    GroundedTakeoff => "grounded_takeoff",
    /// A jump starts this many frames after its press at the earliest.
    /// 0 is on the frame of the press. Whole frames.
    JumpDelay => "jump_delay",
    /// After landing, a jump can't start for this many frames after the
    /// next one. 0 lets a jump start on the frame after landing. Whole
    /// frames.
    LandingLockout => "landing_lockout",
    /// Above 0, a jump pressed in the air is buffered only while jump
    /// stays held: letting go before the jump is possible forgets it.
    /// 0 keeps a buffered press for all of `jump_buffer`.
    JumpBufferHold => "jump_buffer_hold",
    /// Speed lost per frame while rolling with no direction pressed.
    RollFriction => "roll_friction",
    /// Speed lost per frame while rolling and pressing against the motion.
    RollDeceleration => "roll_deceleration",
    /// A roll needs at least this speed to start, and ends below it.
    RollMinSpeed => "roll_min_speed",
    /// Above 0, rolls are timed: an action press on the ground starts one
    /// (unless down is held), whatever the speed, and it ends this many
    /// frames later, or this many frames after landing if it went off a
    /// ledge. Its speed eases toward a set roll speed by
    /// `speed_response`, and holding the other way turns it around at
    /// once. 0: down and action start a roll while moving, and it slows
    /// under `roll_friction` and `roll_deceleration`. Whole frames.
    RollTime => "roll_time",
    /// A timed roll's speed with a direction held as it starts, or
    /// pressed the way it goes later.
    RollSpeed => "roll_speed",
    /// A timed roll's speed with no direction held as it starts.
    RollSlowSpeed => "roll_slow_speed",
    /// A timed roll's speed when the player ran (action and one direction
    /// held) shortly before it started. A fast roll always ends in a stop.
    RollFastSpeed => "roll_fast_speed",
    /// A timed roll is fast if the player ran within this many frames
    /// before it, with a direction held as it starts. Whole frames.
    RollFastWindow => "roll_fast_window",
    /// The same with no direction held as it starts. Whole frames.
    RollFastWindowNeutral => "roll_fast_window_neutral",
    /// A timed roll with action held all the way through it goes on for
    /// this many frames more and carries on into running, with no stop.
    /// Whole frames.
    RollCarryTime => "roll_carry_time",
    /// Above 0, a timed roll that doesn't carry on ends in a stop that
    /// lasts this many frames: the player can't move along the ground,
    /// but can jump, and can start the next roll on its last frame. A
    /// roll that starts then doesn't move on that frame. Whole frames.
    RollStopTime => "roll_stop_time",
    /// For this many frames at the start of the stop the player still
    /// slides, slowing by `speed_response` toward 0, and then stands.
    /// Whole frames.
    RollStopSlide => "roll_stop_slide",
    /// An action press during a timed roll or its stop starts the next
    /// roll as soon as one can start, up to this many frames after the
    /// press. A roll from a press remembered to the stop's last frame
    /// lasts one frame longer. Whole frames.
    RollBuffer => "roll_buffer",
    /// An action press in the air starts a roll just after landing, up
    /// to this many frames after the press, if action stays held until
    /// then. Whole frames.
    RollAirBuffer => "roll_air_buffer",
    /// Upward speed after an attack lands on something from above.
    BounceSpeed => "bounce_speed",
    /// For this many frames after a bounce the player doesn't move and
    /// input is ignored. Holding jump makes the bounce use
    /// `held_jump_gravity` only if it's held from the frame after these
    /// on. Whole frames.
    BounceFreeze => "bounce_freeze",
    /// Above 0, whether the player bounces off an enemy is decided with
    /// a box on whole-pixel positions instead of by landing on it: this
    /// wide, standing on the player's position and as tall as the body.
    /// 0: the player bounces by landing on the enemy, feet first.
    BounceWidth => "bounce_width",
    /// How far the middle of the bounce box is in front of the player's
    /// position, px. Negative is behind.
    BounceOffset => "bounce_offset",
    /// How far the bounce box reaches below the player's feet, px.
    BounceBelow => "bounce_below",
    /// A player rising this fast or faster doesn't bounce off an enemy
    /// its bounce box touches. 0: only when not rising.
    BounceMaxRise => "bounce_max_rise",
    /// Above 0, a grounded body moves over the ground column by column:
    /// its `x` is a point with no width, and the floor of each 1-px column
    /// is the top of the ground under the column's middle. Where the floor
    /// changes by more than this from one column to the next, but by no
    /// more than 8 px, the ground is a steep face; more than 8 px is a
    /// wall. A body moving into a column that climbs more than this, or
    /// into a face that climbs onward, is stopped at the left edge of the
    /// column it was in, with no speed. On a face no jump starts. 0: the
    /// body is a box that walks up slopes up to 45 degrees and is stopped
    /// by steeper ones.
    FaceSlope => "face_slope",
    /// On a frame that starts on a steep face, the target speed has this
    /// much added the way the face falls, and a direction held up the
    /// face counts as none.
    FaceSpeed => "face_speed",
    /// After coming off the low end of a steep face onto other ground, a
    /// direction held back up the face counts as none for this many
    /// frames. Whole frames.
    FaceExit => "face_exit",
    /// Above 0, a grounded body standing still doesn't start moving toward
    /// ground that, this many columns ahead, is more than 8 px higher than
    /// its own column. Whole columns.
    StartRiseAhead => "start_rise_ahead",
    /// Above 0, whether an enemy hurts the player is decided a frame
    /// ahead with a box on whole-pixel positions instead of by touching
    /// it: when the box touches an enemy at the end of a frame, the
    /// player is hurt on the next one, and doesn't move or bounce on it.
    /// The box is this wide and stands on the player's position, as tall
    /// as the body. 0: touching an enemy hurts on the frame it happens.
    HurtWidth => "hurt_width",
    /// How far the middle of the hurt box is in front of the player's
    /// position, px. Negative is behind.
    HurtOffset => "hurt_offset",
    /// From this many frames after a jump starts, until it lands, the
    /// bottom of the hurt box is `hurt_jump_lift` above the feet. Whole
    /// frames.
    HurtJumpTime => "hurt_jump_time",
    /// How far the bottom of the hurt box rises late in a jump, px.
    HurtJumpLift => "hurt_jump_lift",
    /// Above 0, a hurt player is knocked out before losing the life: it
    /// holds still for `knockout_hold` frames from the hit, flies up at
    /// this speed, and lands, rebounds and lies still. Its flight falls
    /// under `gravity` and `fall_gravity`. 0: the player stays where it
    /// was hurt until it comes back.
    KnockoutJump => "knockout_jump",
    /// The knock-out flight's horizontal speed, away from the way the
    /// player faces.
    KnockoutSpeed => "knockout_speed",
    /// The knock-out flight's downward speed never exceeds this.
    KnockoutMaxFall => "knockout_max_fall",
    /// Frames a knocked-out player holds still, counting the hit frame,
    /// before the flight. Enemies other than the one that hurt the player
    /// hold still on the same frames, after the hit frame. Whole frames.
    KnockoutHold => "knockout_hold",
    /// Above 0, the whole game stands still this many frames during a
    /// knock-out: nothing moves or changes on screen. Pause frames aren't
    /// steps of the world. Whole frames.
    KnockoutPause => "knockout_pause",
    /// The pause comes after this many frames of the knock-out, counting
    /// the hit frame. Whole frames.
    KnockoutPauseAfter => "knockout_pause_after",
    /// The upward speed the knock-out flight rebounds at on its first
    /// landing. 0: it stops there.
    KnockoutRebound1 => "knockout_rebound_1",
    /// The same on its second landing.
    KnockoutRebound2 => "knockout_rebound_2",
    /// Frames the player lies still, counting the landing that stops the
    /// flight, before the life is lost. Whole frames.
    KnockoutRest => "knockout_rest",
}

impl Param {
    pub const COUNT: usize = Param::ALL.len();

    /// Whether the parameter may be set to a negative value. Everything
    /// except gravity and offsets is a magnitude.
    pub fn allows_negative(self) -> bool {
        matches!(
            self,
            Param::Gravity | Param::FallGravity | Param::BounceOffset | Param::HurtOffset
        )
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A complete set of movement parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Physics([Fixed; Param::COUNT]);

impl Default for Physics {
    fn default() -> Self {
        Physics([Fixed::ZERO; Param::COUNT])
    }
}

impl Physics {
    pub fn set(&mut self, param: Param, value: Fixed) {
        self.0[param as usize] = value;
    }

    pub fn apply(&mut self, patch: &PhysicsPatch) {
        for (&param, &adjust) in &patch.0 {
            self.set(param, adjust.apply(self[param]));
        }
    }

    /// A copy with `patch` applied, for example the physics inside a water
    /// region of a level.
    pub fn patched(&self, patch: &PhysicsPatch) -> Physics {
        let mut physics = *self;
        physics.apply(patch);
        physics
    }
}

impl Index<Param> for Physics {
    type Output = Fixed;

    fn index(&self, param: Param) -> &Fixed {
        &self.0[param as usize]
    }
}

/// A change to one value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Adjust {
    /// Replace the value. Written as a plain number.
    Set(Fixed),
    /// Multiply the value by a non-negative factor. Written `{ scale = 0.5 }`.
    Scale(Fixed),
}

impl Adjust {
    pub fn apply(self, value: Fixed) -> Fixed {
        match self {
            Adjust::Set(v) => v,
            Adjust::Scale(factor) => value.checked_mul(factor).unwrap_or(if value.is_negative() {
                Fixed::MIN
            } else {
                Fixed::MAX
            }),
        }
    }
}

/// Adjustments to some values, applied in key order. Each layer of physics
/// (character profile, preset, level region) is one of these, so a layer
/// changes only what it needs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Patch<K: Ord>(pub BTreeMap<K, Adjust>);

impl<K: Ord> Default for Patch<K> {
    fn default() -> Self {
        Patch(BTreeMap::new())
    }
}

impl<K: Ord> Patch<K> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Adjustments to movement parameters.
pub type PhysicsPatch = Patch<Param>;

/// Adjustments to an ability's parameters, by name.
pub type ParamsPatch = Patch<String>;

impl ParamsPatch {
    /// Applies the patch to an ability's parameters. Setting a parameter
    /// the ability doesn't list adds it; scaling one leaves it absent.
    pub fn apply_to(&self, params: &mut BTreeMap<String, Fixed>) {
        for (name, &adjust) in &self.0 {
            match (params.get_mut(name), adjust) {
                (Some(value), _) => *value = adjust.apply(*value),
                (None, Adjust::Set(v)) => {
                    params.insert(name.clone(), v);
                }
                (None, Adjust::Scale(_)) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(x: f64) -> Fixed {
        Fixed::from_f64(x).unwrap()
    }

    #[test]
    fn names_round_trip() {
        for &p in Param::ALL {
            assert_eq!(Param::from_name(p.name()), Some(p));
        }
        assert_eq!(Param::from_name("abilities"), None);
    }

    #[test]
    fn patches_set_and_scale() {
        let mut physics = Physics::default();
        physics.set(Param::Gravity, fx(0.25));
        physics.set(Param::TopSpeed, fx(5.0));

        let water = Patch(BTreeMap::from([
            (Param::Gravity, Adjust::Scale(fx(0.5))),
            (Param::TopSpeed, Adjust::Set(fx(2.5))),
        ]));
        let wet = physics.patched(&water);
        assert_eq!(wet[Param::Gravity], fx(0.125));
        assert_eq!(wet[Param::TopSpeed], fx(2.5));
        assert_eq!(wet[Param::JumpSpeed], Fixed::ZERO);
        assert_eq!(physics[Param::Gravity], fx(0.25));
    }

    #[test]
    fn scaling_saturates() {
        assert_eq!(Adjust::Scale(fx(1e6)).apply(fx(1e6)), Fixed::MAX);
        assert_eq!(Adjust::Scale(fx(1e6)).apply(fx(-1e6)), Fixed::MIN);
    }

    #[test]
    fn ability_patches() {
        let mut params = BTreeMap::from([("speed".to_string(), fx(4.0))]);
        Patch(BTreeMap::from([
            ("speed".to_string(), Adjust::Scale(fx(1.5))),
            ("height".to_string(), Adjust::Scale(fx(2.0))),
            ("time".to_string(), Adjust::Set(fx(30.0))),
        ]))
        .apply_to(&mut params);
        assert_eq!(params["speed"], fx(6.0));
        assert_eq!(params["time"], fx(30.0));
        assert!(!params.contains_key("height"));
    }
}
