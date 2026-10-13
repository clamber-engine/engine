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
    /// Upward speed after an attack lands on something from above.
    BounceSpeed => "bounce_speed",
}

impl Param {
    pub const COUNT: usize = Param::ALL.len();

    /// Whether the parameter may be set to a negative value. Everything
    /// except gravity is a magnitude.
    pub fn allows_negative(self) -> bool {
        matches!(self, Param::Gravity | Param::FallGravity)
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A complete set of movement parameters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Physics([Fixed; Param::COUNT]);

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
