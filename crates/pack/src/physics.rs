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
    /// Above 0, a player falling after walking off the ground keeps
    /// facing the way they walked off until they land, whatever is held.
    /// 0: facing follows the direction held, as in a jump.
    WalkOffKeepsFacing => "walk_off_keeps_facing",
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
    /// Once down is held on the ground, it counts as held for at least
    /// this many frames, counting the first, even if let go sooner. 0 or
    /// 1: only while it's held. Whole frames.
    CrouchMin => "crouch_min",
    /// After letting go of down, a crouch lasts this many frames more.
    /// Whole frames.
    CrouchRecovery => "crouch_recovery",
    /// Turning around on the ground takes this many frames, and facing
    /// changes halfway through. A turn, once started, finishes. 0: facing
    /// follows the direction held at once. Whole frames.
    TurnTime => "turn_time",
    /// Above 0, a grounded player outside a turn is standing, walking or
    /// running, and walking and running go through a stride cycle of this
    /// many steps. While walking or running a turn starts only when a step
    /// in `walk_turn_steps` or `run_turn_steps` ends; standing, it starts
    /// at once. 0: a turn starts at once whenever the other way is held.
    /// Needs `turn_time`. Whole steps, up to 31.
    StrideSteps => "stride_steps",
    /// While walking, a step ends after this much stride progress per
    /// px/frame of speed, less `walk_stride_less`, adds up to one step.
    /// Each binary digit of the factor takes its share of the speed
    /// rounded down to 1/256 px/frame on its own.
    WalkStride => "walk_stride",
    /// Taken off the walking stride progress per frame.
    WalkStrideLess => "walk_stride_less",
    /// The same factor as `walk_stride` while running.
    RunStride => "run_stride",
    /// Stride progress per frame is never less than this.
    StrideMin => "stride_min",
    /// The steps whose end checks for a turn while walking: one starts if
    /// the other way is held. Otherwise, below `walk_stand_speed`, the
    /// player stands with no direction held, or starts the stride over
    /// holding forward. A step set, written as a list of step numbers.
    WalkTurnSteps => "walk_turn_steps",
    /// The steps whose end starts running while walking, if action and a
    /// direction are held and that step's turn check changed nothing. A
    /// step set.
    WalkRunSteps => "walk_run_steps",
    /// The steps whose end checks for a turn while running: one starts if
    /// the other way is held. Otherwise, below `run_stand_speed`, the
    /// player stands with no direction held. A step set.
    RunTurnSteps => "run_turn_steps",
    /// See `walk_turn_steps`.
    WalkStandSpeed => "walk_stand_speed",
    /// See `run_turn_steps`.
    RunStandSpeed => "run_stand_speed",
    /// At the end of a turn with forward and action held, the player runs
    /// from this speed on, and walks below it.
    TurnRunSpeed => "turn_run_speed",
    /// Running with action let go and a direction held, the player walks
    /// below this speed, once the run's first step has ended.
    RunWalkSpeed => "run_walk_speed",
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
    /// Above 0, a timed roll defeats an enemy with a box on whole-pixel
    /// positions instead of by touching it, and only on its attack
    /// frames: every frame of the roll before its `roll_end` frame, and
    /// the first `roll_attack_stop` frames of the stop after it. The box
    /// is this wide and stands on the player's position, as tall as the
    /// body; it's checked after the player moves and before enemies do.
    /// A roll that defeats an enemy carries on into running. 0: a roll
    /// defeats any enemy its body touches.
    RollAttackWidth => "roll_attack_width",
    /// How far the middle of the roll's attack box is in front of the
    /// player's position, px. Negative is behind.
    RollAttackOffset => "roll_attack_offset",
    /// How many frames at the start of the stop after a timed roll,
    /// counting its `roll_end` frame, still defeat enemies. Whole frames.
    RollAttackStop => "roll_attack_stop",
    /// An enemy touching the hurt box at the end of one of this many
    /// frames at the start of the stop after a timed roll, counting its
    /// `roll_end` frame, doesn't hurt the player. Whole frames.
    RollStopGuard => "roll_stop_guard",
    /// For this many frames after a roll defeats an enemy, the player
    /// and the enemy don't move and input is ignored; the roll or its
    /// stop then goes on where it was. Whole frames.
    RollDefeatFreeze => "roll_defeat_freeze",
    /// Above 0, action held after a roll's freeze, until the roll or its
    /// stop is over, starts the roll over at the size it had when it
    /// started plus this much for each such boost in the chain of rolls.
    /// One boost per enemy defeated.
    RollBoost => "roll_boost",
    /// Above 0, the ground action's length: an action press on the
    /// ground with down held starts it, instead of a roll, and it lasts
    /// this many frames counting the press frame. The speed starts from 0
    /// and eases by `speed_response` toward `ground_action_speed` the way
    /// held on the press frame, or toward 0 with neither or both held;
    /// facing and other presses don't change it, and swaps are refused.
    /// On the frame after, it's over but the speed stays as it was. A
    /// jump press ends it early. 0: there's no ground action. Whole
    /// frames.
    GroundActionTime => "ground_action_time",
    /// See `ground_action_time`. px/frame.
    GroundActionSpeed => "ground_action_speed",
    /// Above 0, the ground action knocks out enemies with a box on
    /// whole-pixel positions, on its attack frames only (see
    /// `ground_attack_frames`). The box is this wide and stands on the
    /// player's position, as tall as the body; it's checked after the
    /// player moves and before enemies do. Each frame something is
    /// knocked out starts a pause (see `ground_attack_pause`). 0: the
    /// ground action knocks nothing out.
    GroundAttackWidth => "ground_attack_width",
    /// How far the middle of the ground action's attack box is in front
    /// of the player's position, px. Negative is behind.
    GroundAttackOffset => "ground_attack_offset",
    /// The ground action's attack frames, a step set of frames of the
    /// action counted from its press frame as 0, leaving out pause
    /// frames.
    GroundAttackFrames => "ground_attack_frames",
    /// For this many frames after the ground action knocks something
    /// out, the player doesn't move and input is ignored, enemies hold
    /// still, and nothing hurts the player until the frame after them;
    /// the action then goes on where it was, this many frames longer.
    /// Whole frames.
    GroundAttackPause => "ground_attack_pause",
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
    /// Above 0, with more than one character in the roster, the next one
    /// is in the level with the player, and a swap press trades places
    /// with it: both hold still, then each moves to where the other
    /// stood over this many frames, and the other character becomes the
    /// player. 0: a swap changes the character in place. Whole frames.
    SwapMoves => "swap_moves",
    /// Frames both characters hold still, counting the frame of the
    /// press, before the player moves. The player's character's value
    /// counts. Whole frames.
    SwapWait => "swap_wait",
    /// Frames less of holding still when jump was pressed on one of the
    /// two frames before the swap press. Whole frames.
    SwapJumpWait => "swap_jump_wait",
    /// Frames the other character starts moving after the player. The
    /// other character becomes the player on the frame after both have
    /// finished moving. Whole frames.
    SwapLag => "swap_lag",
    /// The upward speed each character's swap starts with. Each frame it
    /// moves adds `gravity`, or `held_jump_gravity` for a player whose
    /// jump press up to two frames before the swap press is still held,
    /// up to `max_fall_speed`. A fall that ends below the other's place
    /// is lifted back by whole pixels.
    SwapHop => "swap_hop",
    /// Above 0, no swap starts while the characters' heights, each on a
    /// whole pixel, differ by this much or more, px.
    SwapHeight => "swap_height",
    /// A swap press up to this many frames after the first frame down is
    /// held, or while it's still held, doesn't start a swap. Whole frames.
    SwapCrouchWait => "swap_crouch_wait",
    /// A swap press up to this many frames after the frame down is let go
    /// doesn't start a swap. Whole frames.
    SwapRiseWait => "swap_rise_wait",
    /// Above 0, a hit while the other character is in the level loses
    /// the hit character instead of a life: on the hit frame it stops
    /// where it is, facing right, with this upward speed, then flies
    /// straight up and down under `gravity`, or `held_jump_gravity` if
    /// its jump had jump held since the press, up to `max_fall_speed`.
    /// The hit character's value counts. 0: a hit is a hit with one
    /// character.
    LoseJump => "lose_jump",
    /// How fast a lost character runs right, out of the level, from the
    /// second frame after it lands.
    LoseRunSpeed => "lose_run_speed",
    /// Frames from the hit, counting the hit frame, before the other
    /// character is the one under control. It moves toward the hit spot
    /// from the frame after. Whole frames.
    TakeoverWait => "takeover_wait",
    /// On its way to the hit spot, each frame the other character moves
    /// a share of the gap on each axis: the gap divided by this, while
    /// that is `takeover_min_move` or more.
    TakeoverShare => "takeover_share",
    /// The smallest move on the way to the hit spot, or the gap if that
    /// is smaller, px.
    TakeoverMinMove => "takeover_min_move",
    /// On the frame after it reaches the hit spot, the other character is
    /// placed this far right of it and this far up, px.
    TakeoverNudge => "takeover_nudge",
    /// The upward speed of the hop the other character starts there.
    /// Each frame after, it moves `gravity` less far up, up to
    /// `max_fall_speed` down, except on the third.
    TakeoverHop => "takeover_hop",
    /// The hop's upward speed on its third frame.
    TakeoverHop3 => "takeover_hop_3",
    /// Frames from the hit, counting the hit frame, that the character
    /// under control can't be hurt after losing a character. Whole
    /// frames.
    ProtectTime => "protect_time",
    /// Frames from the hit to the first on which the character under
    /// control flashes. Whole frames.
    FlashStart => "flash_start",
    /// Frames from the hit to the first on which it no longer flashes.
    /// Whole frames.
    FlashEnd => "flash_end",
    /// Flashing, the character is drawn for the first half of each
    /// stretch of this many frames, and not drawn for the second, except
    /// on the frame after its hop starts. Whole frames.
    FlashPeriod => "flash_period",
    /// Above 0, the other character follows the one under control along
    /// a trail of the whole-pixel places it was at, holding at most this
    /// many points. 0: it stays where it is. Whole points.
    FollowTrail => "follow_trail",
    /// 1: the other character moves before the one under control on each
    /// frame, by where that one was at the end of the frame before. 0:
    /// after it, by where it is now. The following character's value
    /// counts.
    FollowLag => "follow_lag",
    /// Following, the other character steps on along the trail while the
    /// newest point is this far ahead across or further, from the second
    /// such frame in a row. It rests this far from the character under
    /// control. Whole px.
    FollowGap => "follow_gap",
    /// The other character starts to rest only when the character under
    /// control moves slower than this.
    FollowRestSpeed => "follow_rest_speed",
    /// ... and when the ground at its resting place is less than this far
    /// above or below its feet. Whole px.
    FollowRestReach => "follow_rest_reach",
    /// Resting, each frame it moves the whole px still to go divided by
    /// this.
    FollowRestShare => "follow_rest_share",
    /// Points on the way back from where the other character stops
    /// resting to the trail's newest point. Whole points.
    FollowRejoin => "follow_rejoin",
    /// Frames the other character's turn toward the one under control
    /// lasts, counting its first. Whole frames.
    FollowTurn => "follow_turn",
    /// The frame of the turn, counting its first as 1, on which it faces
    /// the other way. Whole frames.
    FollowTurnFlip => "follow_turn_flip",
    /// Frames a character freed from an object flies to the one under
    /// control for, from the frame after the object breaks. It stands
    /// there on the frame after, and following starts from the next.
    /// Meanwhile the one under control, walkers and throwers hold still.
    /// Whole frames.
    JoinTime => "join_time",
    /// The flight's height above the straight line to the one under
    /// control on its `k`th frame is `join_lift` + `join_rise` × `k` −
    /// `join_sag` × `k` × (`k` + 1), plus the fraction of the height on
    /// the frame before.
    JoinLift => "join_lift",
    JoinRise => "join_rise",
    JoinSag => "join_sag",
    /// Above 0, the player can pick up objects that can be carried, and
    /// a pick-up lasts this many frames, counting its first; holding an
    /// object needs action held. 0: nothing can be picked up. Whole
    /// frames.
    PickUpTime => "pick_up_time",
    /// Frames from the start of a pick-up, counting its first, that the
    /// target speed is 0 whatever is held. Whole frames.
    PickUpStill => "pick_up_still",
    /// Frames later a jump takes off while carrying than without an
    /// object. Whole frames.
    CarryJumpDelay => "carry_jump_delay",
    /// Frames a throw lasts, counting the frame action is let go. Facing
    /// doesn't change during them. Whole frames.
    ThrowTime => "throw_time",
    /// Frames from letting go, counting that frame, that the target
    /// speed is 0 whatever is held, and jump and action presses do
    /// nothing. Whole frames.
    ThrowStill => "throw_still",
    /// The frame of a throw, counting from 0 on letting go, that the
    /// object leaves the player's hands on. Whole frames.
    ThrowRelease => "throw_release",
    /// An action press from this frame of a throw, counting from 0,
    /// still held when `throw_still` is over, starts a roll then. Whole
    /// frames.
    ThrowRollPress => "throw_roll_press",
    /// How far above the player's feet a thrown object leaves its hands,
    /// px.
    ThrowLift => "throw_lift",
    /// A thrown object's speed across as it leaves the player's hands,
    /// the way the player faces.
    ThrowSpeed => "throw_speed",
    /// A thrown object's downward speed as it leaves the player's hands.
    ThrowDrop => "throw_drop",
    /// The speed across a thrown object eases toward, by
    /// `speed_response`, the way it was thrown.
    ThrowGlide => "throw_glide",
    /// Added to a thrown object's downward speed every frame.
    ThrowGravity => "throw_gravity",
    /// A thrown object's downward speed never exceeds this.
    ThrowMaxFall => "throw_max_fall",
    /// A thrown object falling onto the floor touches it once it ends a
    /// frame less than this far above it, and is put on it. px.
    ThrowLandMargin => "throw_land_margin",
    /// A thrown object that touches the floor falling at least this fast
    /// bounces; slower, it rolls along the floor from then on, its speed
    /// across easing on toward `throw_glide`.
    ThrowBounceMin => "throw_bounce_min",
    /// The part of its downward speed before the frame a thrown object
    /// bounces up with, rounded down to a whole 1/256 px per frame.
    ThrowBounce => "throw_bounce",
    /// Frames a put-down lasts, counting the frame action is let go; the
    /// target speed is 0 whatever is held. The object is set down on
    /// the last. Whole frames.
    PutDownTime => "put_down_time",
    /// How far in front of the player's feet an object is set down, px.
    PutDownAhead => "put_down_ahead",
    /// How far above the floor an object is set down, the frame before
    /// it rests on it, px.
    PutDownLift => "put_down_lift",
    /// An action press from this frame of a put-down, counting from 0 on
    /// letting go, still held when the put-down is over, picks the object
    /// up again then. Whole frames.
    PutDownRegrab => "put_down_regrab",
    /// Above 0, when the collectible count reaches this, it starts again
    /// from 0 and the player gets one more life. 0: it just goes up.
    /// Whole collectibles.
    CountLife => "count_life",
    /// Above 0, a level's letters spell a word of this many, numbered
    /// from 1 (see `clamber_sim::Flow::letters`). Once every one of them
    /// has joined the letters taken, the player gets one more life and
    /// the letters taken start again from none. 0: letters give nothing.
    /// Whole letters, up to 8.
    LetterCount => "letter_count",
    /// The letter row is first drawn this many frames after the frame a
    /// letter is taken. Whole frames.
    LetterRowIn => "letter_row_in",
    /// Frames the row slides in for, counting the first it's drawn on.
    /// Each frame it's `letter_row_speed` px lower, from
    /// `letter_row_rise` px above `letter_row_y`; on the frame after,
    /// it's at `letter_row_y`, even if it went past it. Whole frames.
    LetterRowSlide => "letter_row_slide",
    /// From this many frames after the frame a letter is taken, the row
    /// slides back up at `letter_row_speed` px a frame. It's no longer
    /// drawn after the frame it's `letter_row_rise` px above
    /// `letter_row_y` or more. Whole frames.
    LetterRowOut => "letter_row_out",
    /// Where the top of the letter row's images is in the picture while
    /// it's shown, px below the picture's top. Whole px.
    LetterRowY => "letter_row_y",
    /// See `letter_row_slide` and `letter_row_out`. Whole px.
    LetterRowRise => "letter_row_rise",
    /// See `letter_row_slide` and `letter_row_out`. Whole px.
    LetterRowSpeed => "letter_row_speed",
    /// The left edge of the image for letter 1, px right of the
    /// picture's left edge. Whole px.
    LetterRowX => "letter_row_x",
    /// Each letter's image is this far right of the one before. Whole
    /// px.
    LetterRowGap => "letter_row_gap",
    /// Above 0, the world has a view this wide that follows the player
    /// (see `clamber_sim::View`), and walkers are in the level only near
    /// it: one appears at its start, at rest, when its start comes within
    /// the view's appear margins, and is removed once its start is
    /// outside them and the walker is past a remove margin. 0: there's no
    /// view in the world, and walkers are in the level all the time.
    /// Whole px.
    ViewWidth => "view_width",
    /// How far right of the view's left edge it aims to have a player
    /// facing right and standing still, px.
    ViewBack => "view_back",
    /// The same for a player facing left, px.
    ViewBackLeft => "view_back_left",
    /// The view aims this many px further the way the player faces for
    /// each px/frame of speed. Each binary digit of the factor takes its
    /// share of the speed rounded down to a whole px on its own.
    ViewLeadFacing => "view_lead_facing",
    /// The view aims this many px further the way the player moves for
    /// each px/frame of speed. Each binary digit of the factor takes its
    /// share of the speed, signed, rounded up to a whole px on its own.
    ViewLeadSpeed => "view_lead_speed",
    /// The view's speed eases toward the gap to where it aims divided by
    /// this. Whole frames.
    ViewShare => "view_share",
    /// Each frame the view's speed closes the gap to that speed divided
    /// by this. Whole frames.
    ViewSpeedShare => "view_speed_share",
    /// Frames the view holds still, counting the frame, when the player
    /// defeats an enemy by bouncing or rolling. Whole frames.
    ViewHold => "view_hold",
    /// Above 0, the view is this tall and follows the player vertically
    /// too (see `clamber_sim::View`), kept inside the level's camera
    /// limits. 0: it doesn't move vertically. Whole px.
    ViewHeight => "view_height",
    /// How far below the view's top a standing player's feet are with
    /// the view settled, less 1/2. Whole px.
    ViewFeet => "view_feet",
    /// While the player has ground and its feet are this close below
    /// the view's top or closer (but not above it), the view aims as if
    /// the ground were at its top row. Whole px.
    ViewFeetTop => "view_feet_top",
    /// In the air, the view aims by the ground below the player when the
    /// ground is no more than `view_reach` bands of this height below the
    /// band of the player's feet. Bands start at the level's top, and
    /// the line on a band's bottom edge is in it. Whole px.
    ViewBand => "view_band",
    /// See `view_band`. Whole bands.
    ViewReach => "view_reach",
    /// The view's vertical speed is its smoothed gap to where it aims
    /// divided by this, rounded down to 1/256 px. Whole frames.
    ViewShareY => "view_share_y",
    /// Each frame the smoothed gap closes the gap to the actual gap
    /// divided by this. Whole frames.
    ViewSpeedShareY => "view_speed_share_y",
    /// `view_share_y` when the view follows quickly. Whole frames.
    ViewShareYQuick => "view_share_y_quick",
    /// `view_speed_share_y` when the view follows quickly. Whole frames.
    ViewSpeedShareYQuick => "view_speed_share_y_quick",
    /// The view follows quickly when the player's feet are this far
    /// below its top or further. Whole px.
    ViewQuickBelow => "view_quick_below",
    /// ... or this far or further, and falling faster than
    /// `view_quick_fall`. Whole px.
    ViewQuickFallBelow => "view_quick_fall_below",
    /// See `view_quick_fall_below`. px/frame.
    ViewQuickFall => "view_quick_fall",
    /// A gap to where the view aims that's below the view by less than
    /// this counts as none. px.
    ViewSlackY => "view_slack_y",
    /// Above 0, a ground action makes the view's top shake this many
    /// times: it moves by an extra offset, first down by
    /// `view_shake_size`, then alternately up and down, the size halving
    /// after each pair. Whole times.
    ViewShakeCount => "view_shake_count",
    /// The first shake is this many frames after the ground action's
    /// press frame. Whole frames.
    ViewShakeDelay => "view_shake_delay",
    /// The shakes are this many frames apart. Whole frames.
    ViewShakePeriod => "view_shake_period",
    /// See `view_shake_count`. px.
    ViewShakeSize => "view_shake_size",
    /// A walker's start is near the view from this far left of the view's
    /// left edge. Whole px.
    AppearLeft => "appear_left",
    /// ... to this far right of the view's right edge. Whole px.
    AppearRight => "appear_right",
    /// A walker whose start isn't near the view is removed once its `x`,
    /// rounded down, is this far left of the view's left edge or
    /// further. Whole px.
    RemoveLeft => "remove_left",
    /// ... or this far right of the view's right edge or further. Whole
    /// px.
    RemoveRight => "remove_right",
    /// The lead passes a level's `end` marker only with its feet line
    /// this far above or below the marker or closer (see
    /// `clamber_sim::Ending`). Whole px.
    EndReach => "end_reach",
    /// Above 0, after passing the end marker the lead walks out of the
    /// level at this speed, on its own. 0: passing it completes the
    /// level at once. px/frame.
    EndWalkSpeed => "end_walk_speed",
    /// How far the lead rises each frame of the walk-out, until the fade
    /// starts. px/frame.
    EndRise => "end_rise",
    /// The fade starts on the frame after the lead ends one this far
    /// right of the view's left edge or further, `x` rounded down. Whole
    /// px.
    EndFadeAt => "end_fade_at",
    /// Frames from the first of the fade to the level's last, on which
    /// the picture is black. Whole frames.
    EndFadeTime => "end_fade_time",
    /// Frames the picture stays at each brightness during the fade. Whole
    /// frames.
    EndFadeStep => "end_fade_step",
    /// Above 0, a player falls out of the level on the first frame it
    /// ends with its feet more than this far below the level's bottom.
    /// 0: once the top of its body is below the bottom. px.
    FallOutDepth => "fall_out_depth",
    /// Above 0, frames the level goes on for after the frame the player
    /// falls out on, with input ignored and the player and the view
    /// still. 0: as for any lost life. Whole frames.
    FallOutTime => "fall_out_time",
}

impl Param {
    pub const COUNT: usize = Param::ALL.len();

    /// Whether the parameter may be set to a negative value. Everything
    /// except gravity and offsets is a magnitude.
    pub fn allows_negative(self) -> bool {
        matches!(
            self,
            Param::Gravity
                | Param::FallGravity
                | Param::BounceOffset
                | Param::HurtOffset
                | Param::RollAttackOffset
                | Param::GroundAttackOffset
        )
    }

    /// Whether the parameter is a step set: a set of step numbers from 1
    /// to [`MAX_STEP`], held as a whole number whose bit `n - 1` stands
    /// for step `n`. Content writes it as a list of step numbers.
    pub fn is_step_set(self) -> bool {
        matches!(
            self,
            Param::WalkTurnSteps
                | Param::WalkRunSteps
                | Param::RunTurnSteps
                | Param::GroundAttackFrames
        )
    }
}

/// The highest step number in a step set.
pub const MAX_STEP: u32 = 31;

/// The step set holding `steps`, or `None` if one is out of range.
pub fn step_set(steps: impl IntoIterator<Item = u32>) -> Option<Fixed> {
    let mut bits = 0;
    for step in steps {
        if !(1..=MAX_STEP).contains(&step) {
            return None;
        }
        bits |= 1 << (step - 1);
    }
    Some(Fixed::from_int(bits))
}

/// The step numbers in a step set, in order.
pub fn steps(set: Fixed) -> impl Iterator<Item = u32> {
    let bits = set.floor();
    (1..=MAX_STEP).filter(move |n| bits >> (n - 1) & 1 == 1)
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
