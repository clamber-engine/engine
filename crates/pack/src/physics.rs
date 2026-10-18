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
    /// Above 0, `held_jump_gravity` stops at the top of a jump: a frame
    /// that starts with the body not rising uses `fall_gravity`, even with
    /// jump still held.
    HeldGravityUntilTop => "held_gravity_until_top",
    /// Downward speed never exceeds this.
    MaxFallSpeed => "max_fall_speed",
    /// A falling body lands when it ends a frame less than this far above
    /// the ground, and is placed on it. 0: only on reaching the ground.
    LandingReach => "landing_reach",
    /// Above 0, with `face_slope` and `slope_piece`, a falling body whose
    /// column is in a sloped piece of ground lands when it ends a frame
    /// this far above the ground or less, and is placed on it. Elsewhere
    /// `landing_reach` applies.
    SlopeLandingReach => "slope_landing_reach",
    /// The ground is made of pieces this many columns wide, the first
    /// starting at x = 0. A piece is sloped if the floor climbs or falls
    /// between two of its neighbouring columns, by no more than
    /// `face_slope`. Whole columns. 0: no piece is sloped.
    SlopePiece => "slope_piece",
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
    /// With a stride cycle (`stride_steps`) only a walk-off down a steep
    /// face does: after one where the ground simply ends, the gait goes
    /// on through the fall as on the ground. 0: facing follows the
    /// direction held, as in a jump.
    WalkOffKeepsFacing => "walk_off_keeps_facing",
    /// Above 0, on the frame after a body walks off the ground its
    /// downward speed becomes this, instead of growing by gravity. Not
    /// while it crouches in the air (`crouch_in_air`), where the frame
    /// after the crouch ends is the one, nor while a jump waits to take
    /// off.
    LeaveStepSpeed => "leave_step_speed",
    /// Above 0, a player who walks off the ground keeps facing for this
    /// many frames, counting the leave frame, whatever is held, and so
    /// does one whose crouch ends in that fall, counting the frame it
    /// ends. Facing then also stays in the air while crouching and while
    /// a jump waits to take off, its take-off frame included. Whole
    /// frames.
    LeaveFacingWait => "leave_facing_wait",
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
    /// Above 0, `top_speed` applies only if, on the previous frame,
    /// action was held together with Left or Right alone; otherwise
    /// `walk_speed` does. 0: action held on this frame decides.
    RunFromPreviousFrame => "run_from_previous_frame",
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
    /// Above 0, Up held alone keeps a crouch going once it has started,
    /// and Down and Up held together end one or don't start one, though
    /// the player still stops as if crouching while Down is held.
    CrouchUpHolds => "crouch_up_holds",
    /// Above 0, no turn starts while crouching, a crouch ends a turn under
    /// way (facing stays as it is), and no turn starts for this many
    /// frames from the frame a crouch ends, counting it. Whole frames.
    CrouchTurnLock => "crouch_turn_lock",
    /// Above 0, a jump that starts while a crouch goes on doesn't take off
    /// if the crouch ends before it would. If jump is held on the frame
    /// the crouch ends, a jump starts again there.
    CrouchTakeoffCancel => "crouch_takeoff_cancel",
    /// Above 0, a crouch going on when the player walks off the ground,
    /// or starting on that frame, goes on in the fall, under the usual
    /// rules; the player slows as on the ground. A jump can start while
    /// it does, and for `coyote_time` frames after it ends. A jump's
    /// take-off ends it. 0: a crouch ends on leaving the ground.
    CrouchInAir => "crouch_in_air",
    /// Turning around on the ground takes this many frames, and facing
    /// changes halfway through. A turn, once started, finishes. 0: facing
    /// follows the direction held at once. Whole frames.
    TurnTime => "turn_time",
    /// Above 0, a ground turn lasts a frame less, facing still changing
    /// halfway through the full `turn_time`, if on its second frame the
    /// player moves toward the new facing at this speed or faster. 0:
    /// every turn lasts `turn_time`.
    ShortTurnSpeed => "short_turn_speed",
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
    /// Above 0, `coyote_time` is only for ground that simply ends: after
    /// walking off from a frame that starts on a steep face, no jump
    /// starts in the air.
    NoFaceCoyote => "no_face_coyote",
    /// Above 0, a jump press on a frame that starts on the ground and off
    /// a steep face, whose jump doesn't start because the next frame
    /// starts on the face, still gives the fall `held_jump_gravity` for
    /// as long as jump stays held, from the drop off the ledge on.
    FacePressGravity => "face_press_gravity",
    /// Above 0, a jump press on a frame that starts on a steep face, or
    /// whose next frame does, is remembered while jump stays held and
    /// takes off on the frame after the first one that starts on the
    /// floor off the face, within `jump_buffer`. The take-off frame isn't
    /// stopped from starting toward a rise, and has no push down a face
    /// or exit from one.
    FaceJumpWait => "face_jump_wait",
    /// Above 0, with `face_slope`, timed rolls on the ground meet steep
    /// faces: on a frame that starts on a face steering does nothing,
    /// and a roll going down the face eases toward its speed plus
    /// `face_speed`. A roll is stopped by a rise as walking is, and by
    /// floor more than a step higher than its own; it is then stopped as
    /// with `roll_edge_push`, carried down a face it starts a frame on at
    /// `face_speed`, and pushes off only from a frame that doesn't start
    /// on a face.
    FaceRolls => "face_rolls",
    /// Above 0, a jump leaves the ground on the frame after it starts:
    /// on the start frame the body still follows the ground, with its
    /// upward speed already set. 0 leaves on the start frame.
    GroundedTakeoff => "grounded_takeoff",
    /// Above 0, with `grounded_takeoff`, a jump whose start frame goes
    /// over the end of the ground moves down on it as a walk-off would,
    /// by `leave_drop` with the jump's gravity in place of
    /// `fall_gravity`, and rises from there. 0: it rises from the height
    /// of the ground it left.
    TakeoffOverEnd => "takeoff_over_end",
    /// A jump starts this many frames after its press at the earliest.
    /// 0 is on the frame of the press. Whole frames.
    JumpDelay => "jump_delay",
    /// After landing, a jump can't start for this many frames after the
    /// next one. 0 lets a jump start on the frame after landing. Whole
    /// frames.
    LandingLockout => "landing_lockout",
    /// As `landing_lockout`, after landing from a fall that didn't start
    /// with a jump, such as walking off a ledge. Whole frames.
    FallLandingLockout => "fall_landing_lockout",
    /// Above 0, a landing after a jump (not one out of a roll) has a
    /// switch frame M, and the first frame a jump can start is
    /// `landing_lockout` frames after M; other landings count as M on the
    /// frame after landing. M comes from the jump's approach frame A: the
    /// first frame from this many after the one the jump started on that
    /// ends with floor within reach (`switch_reach`), on the floor
    /// included. A landing before A has M on A plus one frame for each
    /// step of `switch_steps`; one on A or later has M on the landing
    /// frame plus one for each step that comes after it. 0: every landing
    /// uses `landing_lockout` as it is. Whole frames.
    SwitchFrom => "switch_from",
    /// Floor is within reach of a player in the air when, in the column
    /// holding it, the floor below its feet line is at most this far
    /// down, and at most `view_reach` bands of `view_band` px below the
    /// band of the feet. Whole px.
    SwitchReach => "switch_reach",
    /// The frames that put a switch frame off, counting the approach frame
    /// as step 1, when the approach frame is the first frame it can be.
    /// A step set.
    SwitchSteps => "switch_steps",
    /// As `switch_steps`, when the approach frame is a later one.
    SwitchLateSteps => "switch_late_steps",
    /// With a stride cycle (`stride_steps`), the frames from a jump's
    /// landing are in no gait: no turn starts, and facing follows the
    /// input as in the air. Running starts on the switch frame (see
    /// `switch_from`) if action and a direction are held; otherwise
    /// walking, running or standing starts this many frames after a
    /// switch frame later than the landing frame. Whole frames.
    LandingModeWait => "landing_mode_wait",
    /// As `landing_mode_wait`, the frames after the landing frame when
    /// the switch frame is the landing frame and that is the last frame
    /// of `switch_steps` or `switch_late_steps`. Whole frames.
    LandingModeOnLast => "landing_mode_on_last",
    /// As `landing_mode_on_last`, when the landing frame comes after that
    /// last frame. Whole frames.
    LandingModeAfterLast => "landing_mode_after_last",
    /// With a stride cycle (`stride_steps`), after a fall off a steep
    /// face (`walk_off_keeps_facing`) the landing frame and this many
    /// frames less one after it are in no gait: no turn starts and facing
    /// doesn't change. Then a gait starts as at the end of a turn. 0: the
    /// gait goes on as before the landing. Whole frames.
    FaceFallLanding => "face_fall_landing",
    /// Above 0, a jump pressed in the air is buffered only while jump
    /// stays held: letting go before the jump is possible forgets it.
    /// 0 keeps a buffered press for all of `jump_buffer`.
    JumpBufferHold => "jump_buffer_hold",
    /// Above 0, a jump only starts on a frame jump is held, however it
    /// was buffered: a press let go of before then is forgotten.
    JumpStartHold => "jump_start_hold",
    /// A jump takes off this many frames after it starts. Until then
    /// nothing about it shows (no `jump_start` event, no upward speed),
    /// the player moves as before and may leave the ground, and no other
    /// jump starts. 0 takes off on the start frame. Whole frames.
    /// While a jump waits in the air, its take-off frame included, it
    /// falls under `held_jump_gravity`, moving down or not, if jump has
    /// been held since it started or it started while crouching. While
    /// it waits on the ground, its take-off frame included, facing
    /// doesn't change and no timed roll starts. A timed roll, or the stop
    /// after one, that a jump starts in goes on while the jump waits, its
    /// take-off frame included, and ends then without a stop.
    TakeoffWait => "takeoff_wait",
    /// Speed lost per frame while rolling with no direction pressed.
    RollFriction => "roll_friction",
    /// Speed lost per frame while rolling and pressing against the motion.
    RollDeceleration => "roll_deceleration",
    /// A roll needs at least this speed to start, and ends below it.
    RollMinSpeed => "roll_min_speed",
    /// Above 0, a roll that isn't timed also ends on the frame it slows
    /// from above this speed to it or below with a direction held the way
    /// it rolls, and the player walks or runs on from there. A roll that
    /// starts no faster, or with 0, rolls on until `roll_min_speed`.
    RollFlowSpeed => "roll_flow_speed",
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
    /// before it, with a direction held as it starts. With
    /// `roll_fast_hold`, the most frames without action there can be
    /// between that hold and the roll's first frame. Whole frames.
    RollFastWindow => "roll_fast_window",
    /// The same with no direction held as it starts. Whole frames.
    RollFastWindowNeutral => "roll_fast_window_neutral",
    /// Above 0, a timed roll whose only running frame before it was a
    /// single one from standing still is slow with no direction held as
    /// it starts, even if a direction was held in between.
    RollLoneRunAnyDir => "roll_lone_run_any_dir",
    /// Above 0, a timed roll's size comes from action's last hold
    /// instead: with Left or Right alone held as it starts, the roll is
    /// fast if action's last hold before the press that starts it lasted
    /// at least this many frames and ended within `roll_fast_window`,
    /// and normal otherwise; with no direction, it's slow. Whole frames.
    RollFastHold => "roll_fast_hold",
    /// Above 0, a direction held the way a timed roll goes doesn't make
    /// a slower roll normal.
    RollKeepsSize => "roll_keeps_size",
    /// Above 0, the speed a slower timed roll eases toward once the way
    /// it goes is held, instead of `roll_speed`.
    RollSteerSpeed => "roll_steer_speed",
    /// Above 0, an action press on the ground with down held doesn't
    /// start a timed roll.
    RollNotOnDown => "roll_not_on_down",
    /// A timed roll with action held all the way through it goes on for
    /// this many frames more and carries on into running, with no stop.
    /// 0: it carries on from the frame after its `roll_end` frame.
    /// Whole frames.
    RollCarryTime => "roll_carry_time",
    /// Above 0, a timed roll that doesn't carry on ends in a stop that
    /// lasts this many frames: the player can't move along the ground,
    /// but can jump, and can start the next roll on its last frame. A
    /// roll that starts then doesn't move on that frame. Whole frames.
    RollStopTime => "roll_stop_time",
    /// Above 0, the next roll can't start on the stop's last frame, only
    /// after the stop, and facing doesn't change during the stop.
    RollAfterStop => "roll_after_stop",
    /// For this many frames at the start of the stop the player still
    /// slides, slowing by `speed_response` toward 0, and then stands.
    /// 0: the roll's `roll_end` frame still moves, but ends standing.
    /// Whole frames.
    RollStopSlide => "roll_stop_slide",
    /// Above 0, facing turns at once while the player still slides in
    /// the stop, as it does once they stand.
    RollSlideTurns => "roll_slide_turns",
    /// An action press during a timed roll or its stop starts the next
    /// roll as soon as one can start, up to this many frames after the
    /// press. A roll from a press remembered to the stop's last frame
    /// lasts one frame longer. Whole frames.
    RollBuffer => "roll_buffer",
    /// Above 0, a roll from a new press on the stop's last frame lasts
    /// one frame longer too.
    RollStopPressLengthens => "roll_stop_press_lengthens",
    /// Above 0, such a press is only remembered while action stays held,
    /// as in the air.
    RollBufferHold => "roll_buffer_hold",
    /// An action press in the air starts a roll just after landing, up
    /// to this many frames after the press, if action stays held until
    /// then. Whole frames.
    RollAirBuffer => "roll_air_buffer",
    /// Above 0, a timed roll ends in the air as it does on the ground,
    /// the stop after it goes on in the air, and landing doesn't change
    /// when it ends. At the end of the roll's frame this many frames
    /// after its first, a roll in the air is lengthened by this many
    /// frames, and again every this many frames for as long as it keeps
    /// being lengthened. The frame after a roll, or its stop, ends in the
    /// air falls as the frame after leaving the ground does, and a jump
    /// can start on it and during the stop. 0: a roll doesn't end in the
    /// air, and one that lands ends as if it had started just after
    /// landing. Whole frames.
    RollAirLengthen => "roll_air_lengthen",
    /// Above 0, a timed roll that leaves the ground hangs in the air from
    /// its second frame after leaving it until this many frames after
    /// the action press that started the roll: it doesn't move up or
    /// down, and falls from rest after that. Whole frames.
    RollHover => "roll_hover",
    /// Above 0, a timed roll that leaves the ground on its first frame,
    /// or up to this many frames after it, drifts down at
    /// `leave_fall_speed` for a while, ending each frame with that speed,
    /// and falls from it after that. Leaving on its first frame, it
    /// drifts from the frame after leaving to `roll_leave_hover_time` - 2
    /// frames after it; leaving on a later frame before this one, from
    /// the leave frame to `roll_leave_hover_time` - 1 frames after;
    /// leaving this many frames after its first, on the leave frame only.
    /// Whole frames.
    RollLeaveHover => "roll_leave_hover",
    /// How many frames a roll that leaves the ground under
    /// `roll_leave_hover` drifts down for, counting the leave frame.
    /// Whole frames.
    RollLeaveHoverTime => "roll_leave_hover_time",
    /// Above 0, without `roll_air_lengthen`, a timed roll that lands
    /// exactly this many frames after leaving the ground ends on the
    /// landing frame, as if its last frame had been the one before.
    /// Whole frames.
    RollFallEnd => "roll_fall_end",
    /// Above 0, a frame of a timed roll on the ground that starts at
    /// this speed or slower, with no floor in the column
    /// `roll_edge_ahead` px ahead of the player's, doesn't move: the
    /// speed becomes 0.
    RollEdgeSpeed => "roll_edge_speed",
    /// How far ahead, px, `roll_edge_speed` looks for a floor.
    RollEdgeAhead => "roll_edge_ahead",
    /// Above 0, floor this far below the feet or less, or higher, counts
    /// as a floor for `roll_edge_speed`; floor further down doesn't.
    /// 0: as far down as a grounded body sees floor (two steps).
    RollEdgeDepth => "roll_edge_depth",
    /// Above 0, a roll that has had such a frame is stopped: its speed
    /// eases toward 0 until, on a frame after the roll's first, with only
    /// the way the roll goes held (after steering), it pushes off at this
    /// speed, which from the next frame is its size until steering makes
    /// it a normal roll.
    RollEdgePush => "roll_edge_push",
    /// Above 0, a jump started during a timed roll waits to take off
    /// under `held_jump_gravity`, whether jump is held or not.
    RollJumpHeldGravity => "roll_jump_held_gravity",
    /// Above 0, after a timed roll's stop in the air no roll starts on
    /// the frame after landing. An action press on it, or one in the
    /// air, starts one on the next frame if action is held until then.
    RollAirStopWait => "roll_air_stop_wait",
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
    /// Above 0, whether a roll is boosted is decided on the frame it
    /// defeats an enemy, by action being held then. Boosted, the roll
    /// eases toward its speed plus `roll_boost` once the freeze is over,
    /// and goes on for `roll_time` more frames from then, carrying on into
    /// running. Otherwise it goes on as if nothing had been defeated.
    RollBoostAtDefeat => "roll_boost_at_defeat",
    /// Above 0, the walker a roll defeats doesn't move on that frame, and
    /// every walker holds still on the `roll_defeat_freeze` frames after
    /// it.
    RollDefeatHoldsWalkers => "roll_defeat_holds_walkers",
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
    /// Above 0, facing doesn't change from a bounce to its landing,
    /// whatever is held. With a stride cycle (`stride_steps`) a gait
    /// starts on the landing frame: running with action and a direction
    /// held, otherwise a turn, walking or standing by the direction held.
    BounceKeepsFacing => "bounce_keeps_facing",
    /// Above 0, a landing that ends a bounce has its switch frame on the
    /// landing frame: a jump can start `landing_lockout` frames after
    /// it. 0: it counts as M on the frame after landing, as other
    /// landings without a switch frame do (`switch_from`).
    BounceLandingSwitch => "bounce_landing_switch",
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
    /// Above 0, the walker bounced on doesn't move on the bounce's frame,
    /// and every walker holds still on the `bounce_freeze` frames after
    /// it.
    BounceHoldsWalkers => "bounce_holds_walkers",
    /// Above 0, nothing else moves on the `bounce_freeze` frames after a
    /// bounce: things, throws, hidden objects' items in the air and the
    /// pieces of a burst all keep where they are, and nothing is made up
    /// afterwards.
    BounceHoldsAll => "bounce_holds_all",
    /// For walkers with a `contact` table: on the ground, a player moving
    /// faster than this is walking or running, and slower, standing.
    StanceStandSpeed => "stance_stand_speed",
    /// For walkers with a `contact` table: on the ground, a player moving
    /// faster than this is running from then on, until it stands, or
    /// moves at this or slower with a direction held.
    StanceRunSpeed => "stance_run_speed",
    /// Above 0, a grounded body moves over the ground column by column:
    /// its `x` is a point with no width, and the floor of each 1-px column
    /// is the top of the ground under the column's middle. Where the floor
    /// changes by more than this from one column to the next, but by no
    /// more than 8 px, two or more times in a row the same way, the ground
    /// is a steep face; a single such change is a step, walked up and
    /// down; more than 8 px is a wall. A body moving into a wall or into a
    /// face that climbs is stopped at the left edge of the column it was
    /// in, with no speed. One-way platforms with more than 1 px under them
    /// aren't there for a grounded body; one right over its feet with a
    /// top more than 8 px up makes it fall through the floor. On a face no
    /// jump starts. 0: the
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
    /// Above 0, with `face_speed` and `switch_from`, `face_speed` is a
    /// push worked out at the end of each frame on a face column, given
    /// once the frame's target and speed both point down the face or are
    /// 0, and kept otherwise; after a jump's landing it, and
    /// `start_rise_ahead`, wait until after a switch frame. That is the
    /// jump's switch frame, unless its approach frame ends in a face
    /// column: then this many frames after the first frame from the
    /// landing and the approach frame on that ends on the floor off a
    /// face. It comes earlier on a frame that ends on the floor of a face
    /// column with no speed or speed down it. Whole frames.
    FaceSlideSwitch => "face_slide_switch",
    /// Above 0, a grounded body standing still doesn't start moving toward
    /// ground that, this many columns ahead, is 8 px or more higher than
    /// its own column. Whole columns.
    StartRiseAhead => "start_rise_ahead",
    /// Above 0, with `face_slope`, an airborne body moving into a column
    /// whose floor is above its feet by this much or less is lifted onto
    /// it, or lands on it if falling; higher floor stops it as a wall.
    /// 0: 8 px.
    AirStep => "air_step",
    /// Above 0, with `face_slope`, how far a column's floor is above an
    /// airborne body's feet is counted in whole pixels, the feet rounded
    /// down: floor less than a pixel above them isn't above, and the lift
    /// onto a column is that whole number. 0: exactly.
    AirWholePixels => "air_whole_pixels",
    /// Above 0, an airborne body with no vertical speed counts as
    /// falling: it lands on a column it's lifted onto, and on ground it
    /// ends the frame within landing reach of. 0: it counts as rising
    /// for the lift and doesn't land short of the ground.
    StillFalls => "still_falls",
    /// On a frame after one that started on a steep face, the target
    /// speed has this much added the way the face falls, whatever is
    /// held, on the ground or, just off the face, in the air.
    FacePull => "face_pull",
    /// Above 0, on a frame that starts on a steep face and isn't stopped
    /// by a rise, a speed of more than this the way the face falls
    /// becomes `face_limited_speed`.
    FaceSpeedLimit => "face_speed_limit",
    /// The speed `face_speed_limit` sets, the way the face falls.
    FaceLimitedSpeed => "face_limited_speed",
    /// Above 0, on a frame that starts on a steep face and isn't stopped
    /// by a rise, a speed of less than this the way the face falls, none
    /// or up the face included, becomes this.
    FaceMinSpeed => "face_min_speed",
    /// Above 0, on a frame that starts on a steep face the body faces the
    /// way the face falls, turning at once with no turn, except on the
    /// frame after landing.
    FaceTurns => "face_turns",
    /// Above 0, a body leaving the ground on a frame that starts on a
    /// steep face moves down this far instead of `leave_drop`, and its
    /// fall has no step to `leave_step_speed`.
    FaceLeaveDrop => "face_leave_drop",
    /// Above 0, with `face_slope`, on a frame that doesn't start on a
    /// steep face the body follows the ground down into its new column
    /// only if it's this much lower or less; lower, it leaves the ground.
    /// On a face it follows the ground down a step. 0: a step.
    FollowDrop => "follow_drop",
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
    /// A jump has the airborne pose for the hurt box from this many
    /// frames after it starts. Whole frames.
    HurtJumpTime => "hurt_jump_time",
    /// How far the bottom of the hurt box rises in the airborne pose, in
    /// the air and on the frames after landing that are lifted
    /// (`hurt_land_lift`, `hurt_fall_lift`), px. A walker with its own
    /// `hurt_jump_lift` uses that instead.
    HurtJumpLift => "hurt_jump_lift",
    /// Above 0, in the airborne pose the hurt box is this wide instead of
    /// `hurt_width`, its middle `hurt_air_offset` in front of the
    /// player's position. 0: the airborne pose has the usual hurt box.
    HurtAirWidth => "hurt_air_width",
    /// How far the middle of the airborne pose's hurt box is in front of
    /// the player's position, px. Negative is behind.
    HurtAirOffset => "hurt_air_offset",
    /// After a jump lands, the airborne pose goes on up to this many
    /// frames after the landing frame. Whole frames.
    HurtLandHold => "hurt_land_hold",
    /// Of those frames, the first this many, from the landing frame, keep
    /// the box lifted. Whole frames.
    HurtLandLift => "hurt_land_lift",
    /// A jump with Jump held for from this many frames up to
    /// `hurt_long_jump_end`, from the frame it starts on, keeps the
    /// airborne pose `hurt_long_land_hold` frames after landing instead,
    /// none of them lifted. 0: no jump does. Whole frames.
    HurtLongJump => "hurt_long_jump",
    /// The most frames of Jump held for `hurt_long_jump`. Whole frames.
    HurtLongJumpEnd => "hurt_long_jump_end",
    /// How long such a jump's airborne pose goes on after landing. Whole
    /// frames.
    HurtLongLandHold => "hurt_long_land_hold",
    /// After a fall without a jump in the airborne pose lands, the pose
    /// goes on up to this many frames after the landing frame, the first
    /// `hurt_fall_lift` of them lifted. Whole frames.
    HurtFallHold => "hurt_fall_hold",
    /// How many frames from the landing frame of that fall keep the box
    /// lifted. Whole frames.
    HurtFallLift => "hurt_fall_lift",
    /// Above 0, in the fidget pose the hurt box is this wide instead of
    /// `hurt_width`, its middle `hurt_fidget_offset` in front of the
    /// player's position. 0: the fidget pose has the usual hurt box.
    HurtFidgetWidth => "hurt_fidget_width",
    /// How far the middle of the fidget pose's hurt box is in front of
    /// the player's position, px. Negative is behind.
    HurtFidgetOffset => "hurt_fidget_offset",
    /// A player standing still takes the fidget pose this many frames
    /// into standing, counting the first frame of standing as 0. Whole
    /// frames.
    FidgetStart => "fidget_start",
    /// Frames the fidget pose lasts each time. Whole frames.
    FidgetLength => "fidget_length",
    /// Above 0, while the player keeps standing the fidget pose comes
    /// again this many frames after each start. 0: only once. Whole
    /// frames.
    FidgetEvery => "fidget_every",
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
    /// hold still on the same frames, after the hit frame. 0: the hit
    /// frame moves as any other, the flight's speeds are set at its end,
    /// and the flight starts on the next frame. Whole frames.
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
    /// flight, before the life is lost. With a hop (`knockout_hop_at`),
    /// the knock-out goes on at least this many frames counting the
    /// hop's. Whole frames.
    KnockoutRest => "knockout_rest",
    /// Above 0, the knock-out flight falls under this instead of
    /// `gravity` and `fall_gravity`.
    KnockoutGravity => "knockout_gravity",
    /// Above 0, a player knocked out by an enemy flies the way the enemy
    /// is heading and faces the other way. 0, or hurt by something else:
    /// away from the way it faces.
    KnockoutWithEnemy => "knockout_with_enemy",
    /// Above 0, every enemy holds still this many frames after the hit
    /// frame, then makes up the moves it missed on the next one. Whole
    /// frames.
    KnockoutEnemyWait => "knockout_enemy_wait",
    /// Above 0, the knock-out has a hop instead of rebounds: the flight
    /// doesn't stop on landing but slides on along the ground, and the
    /// first frame from this many after the hit frame on which the
    /// player is on the ground, it leaves at `knockout_hop`. Whole
    /// frames.
    KnockoutHopAt => "knockout_hop_at",
    /// The upward speed of the knock-out's hop.
    KnockoutHop => "knockout_hop",
    /// After the hop, the horizontal speed moves toward 0 by this share
    /// of itself each frame, rounded toward 0 to a whole grain; once that
    /// rounds to nothing, it's 0.
    KnockoutHopSlowing => "knockout_hop_slowing",
    /// With a hop, the knock-out goes on at least this many frames,
    /// counting the hit frame. Whole frames.
    KnockoutTime => "knockout_time",
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
    /// Above 0, after a swap in which this character was the player, a
    /// tandem press on the frame the other character becomes the player
    /// starts the next swap. 0: no swap press counts on that frame.
    SwapTandemOnEnd => "swap_tandem_on_end",
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
    /// Above 0, an object still in the player's hands at a knock-out is
    /// let go on the hit frame and falls away, touching nothing: its
    /// speed across becomes this, the way the player faces, and it falls
    /// by `throw_gravity` up to `throw_max_fall`. 0: it drops straight
    /// down. px per frame.
    LetGoSpeed => "let_go_speed",
    /// The upward speed an object let go of (`let_go_speed`) starts
    /// with, px per frame.
    LetGoRise => "let_go_rise",
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
    /// Above 0, the letter row shows an ending after the last letter of
    /// the word joins (see `clamber_sim::World::letter_row`): from the
    /// frame after, it doesn't slide out; each letter blinks, then moves
    /// right from this many frames after the join, letter 1 first.
    /// 0: the row slides out as for any letter. Whole frames.
    LetterEndMove => "letter_end_move",
    /// Each letter starts moving this many frames after the one before.
    /// Whole frames.
    LetterEndStagger => "letter_end_stagger",
    /// The letters blink this many times: off for `letter_end_blink`
    /// frames from the frame after the join, then on for as many. Whole.
    LetterEndBlinks => "letter_end_blinks",
    /// See `letter_end_blinks`. Whole frames.
    LetterEndBlink => "letter_end_blink",
    /// The letter that joined last does all of this this many frames
    /// later than the others. Whole frames.
    LetterEndLate => "letter_end_late",
    /// Where a moving letter's image's left edge goes, px right of the
    /// picture's left edge. On the frame after it gets there it's no
    /// longer drawn. Whole px.
    LetterEndX => "letter_end_x",
    /// Each frame a moving letter covers 1 / `letter_end_approach` of
    /// the distance left, but at least `letter_end_least`, and it's
    /// there once less than that would be left. Whole.
    LetterEndApproach => "letter_end_approach",
    /// See `letter_end_approach`. px.
    LetterEndLeast => "letter_end_least",
    /// With an ending, the lives display is first drawn this many
    /// frames after the join, sliding in as from `lives_in`. Whole
    /// frames.
    LetterEndLivesIn => "letter_end_lives_in",
    /// With an ending, the lives display shows the new number from the
    /// first frame the last letter of the word is gone, and slides back
    /// up from this many frames after that. Whole frames.
    LetterEndLivesOut => "letter_end_lives_out",
    /// Above 0, the world has a count display (see
    /// `clamber_sim::World::count_display`): the number shown follows the
    /// collectible count one step a frame, and the display is shown while
    /// a pickup waits for its steps and up to this many frames after the
    /// latest step. 0: no count display. Whole frames.
    CountShowFor => "count_show_for",
    /// The top-left corner of the count display's collectible image, px
    /// right of the picture's left edge. Whole px.
    CountX => "count_x",
    /// The top of the count display's image and digits, px below the
    /// picture's top. Whole px.
    CountY => "count_y",
    /// The left edge of the number's first digit, px right of the
    /// picture's left edge. Whole px.
    CountDigitX => "count_digit_x",
    /// Each digit is this far right of the one before. Whole px.
    CountDigitGap => "count_digit_gap",
    /// With a count display, when the number shown goes from
    /// `count_life` less one to 0 the lives display is first drawn this
    /// many frames after that step. Whole frames.
    LivesIn => "lives_in",
    /// Frames the lives display slides in for, as the letter row does
    /// with `letter_row_slide`, by `lives_y`, `lives_rise` and
    /// `lives_speed`. Whole frames.
    LivesSlide => "lives_slide",
    /// From this many frames after the step, the lives display slides
    /// back up, as the letter row does from `letter_row_out`. Whole
    /// frames.
    LivesOut => "lives_out",
    /// Where the top of the lives number is while it's shown, px below
    /// the picture's top. Whole px.
    LivesY => "lives_y",
    /// See `lives_slide`. Whole px.
    LivesRise => "lives_rise",
    /// Sliding back up, the lives display is last drawn on the frame it's
    /// this many px above `lives_y` or less. Whole px.
    LivesGone => "lives_gone",
    /// See `lives_slide` and `lives_out`. Whole px.
    LivesSpeed => "lives_speed",
    /// The left edge of the lives number's digit, px right of the
    /// picture's left edge. Whole px.
    LivesX => "lives_x",
    /// A lives number of more than one digit has each digit this far
    /// right of the one before, the digits centred on where a single one
    /// would be. Whole px.
    LivesDigitGap => "lives_digit_gap",
    /// The lives display shows the lives before the new one until this
    /// many frames after the step, and the new number from then on. Whole
    /// frames.
    LivesChange => "lives_change",
    /// After a collectible giving `lives` is taken, the lives display is
    /// first drawn this many frames after, sliding in as from `lives_in`.
    /// Whole frames.
    LifeItemLivesIn => "life_item_lives_in",
    /// The lives display shows the lives before the item until this many
    /// frames after it's taken, then one more every
    /// `life_item_lives_every` frames up to the new number. Whole frames.
    LifeItemLivesStep => "life_item_lives_step",
    /// See `life_item_lives_step`. Whole frames.
    LifeItemLivesEvery => "life_item_lives_every",
    /// The lives display slides back up from this many frames after it
    /// first shows the new number. Whole frames.
    LifeItemLivesOut => "life_item_lives_out",
    /// Above 0, the image of a taken `relic` is shown flying to its
    /// resting place (see `clamber_sim::World::relic_image`): each frame
    /// it covers this part of the distance left, 1 / `relic_approach`.
    /// 0: no image. Whole.
    RelicApproach => "relic_approach",
    /// The relic image's box is this many px square. It starts the
    /// flight centred on where the relic was in the picture. Whole px.
    RelicSize => "relic_size",
    /// Where the relic image's box rests: its top-left corner, px right
    /// of the picture's left edge. Whole px.
    RelicX => "relic_x",
    /// The same, px below the picture's top. Whole px.
    RelicY => "relic_y",
    /// Frames the relic image rests for, counting the first it's there.
    /// Whole frames.
    RelicRest => "relic_rest",
    /// After resting, the relic image moves up this many px a frame,
    /// and isn't drawn once its box is wholly above the picture. Whole
    /// px.
    RelicRise => "relic_rise",
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
    /// Above 0, the view aims 1 px further left while the player's `x`
    /// rounded down is less than this less the speed part of its aim
    /// (`view_lead_speed`). Whole px.
    ViewNearLeft => "view_near_left",
    /// Above 0, the view starts this far left of the lead when an area
    /// begins, as it aims for a lead standing still with this in place
    /// of `view_back` (see `clamber_sim::View`), kept inside the
    /// picture's ends, plus 1/2 px. While the characters are in the
    /// level's entrance it aims with this in place of `view_back` and
    /// the facing part, whatever the facing. Whole px.
    ViewBeginBack => "view_begin_back",
    /// After the lead is thrown out of the entrance, the view aims with
    /// this in place of `view_back` and the facing part until it lands.
    /// Whole px.
    ViewBeginBackFar => "view_begin_back_far",
    /// From this many frames after the throw's until the lead lands, the
    /// view follows quickly: its speed closes the gap divided by
    /// `view_fast_speed_share` instead, and it moves by a growing
    /// multiple of its speed. Whole frames.
    ViewBeginFar => "view_begin_far",
    /// What the view's speed closes the gap to its aim by on those
    /// frames. Whole frames.
    ViewFastSpeedShare => "view_fast_speed_share",
    /// Above 0, with a view, the picture is kept inside the level's
    /// camera limits across, and the player is stopped this far inside
    /// the range it can show at the level's left and right ends: a move
    /// toward an end from that far inside it or closer leaves `x` as it
    /// is, with no speed across. Whole px.
    EndStop => "end_stop",
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
    /// A placed thing that also has to be within the view's rows to
    /// appear (see `clamber_level::Spot`) is there from this far above
    /// the view's top edge... Whole px.
    AppearAbove => "appear_above",
    /// ... to this far below its bottom edge. Whole px.
    AppearBelow => "appear_below",
    /// A walker whose start isn't near the view is removed once its `x`,
    /// rounded down, is this far left of the view's left edge or
    /// further. Whole px.
    RemoveLeft => "remove_left",
    /// ... or this far right of the view's right edge or further. Whole
    /// px.
    RemoveRight => "remove_right",
    /// Above 0, the objects that can be there at once (see
    /// `clamber_sim::Placing`): each placed object in a level's sections
    /// takes a place while it's there, as do thrown balls and animals out
    /// of their crates. Whole.
    Places => "places",
    /// Above 0, the pictures there are: each placed object whose
    /// `clamber_level::Spot` says it takes one does while it's there, and
    /// so do a burst keg's pieces and flash. Whole.
    Pictures => "pictures",
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
    /// For an end marker that leads to another area: the fade waits for
    /// the lead to get `end_fade_at` px right of the view's left edge on
    /// a frame from this many after the walk-out's first on. Whole
    /// frames.
    EndOnwardFadeFrom => "end_onward_fade_from",
    /// ... and the area's last frame comes this many after the one the
    /// level's would be, black too. Whole frames.
    EndOnwardHold => "end_onward_hold",
    /// Above 0, an object let go of at the start of the walk-out (see
    /// `let_go_speed`) is also removed once its `x`, rounded down, is
    /// this far right of the view's left edge or further. Whole px.
    EndLetGoRight => "end_let_go_right",
    /// Above 0, ... or once its `y`, rounded down, is this far above the
    /// view's top or further. Whole px.
    EndLetGoAbove => "end_let_go_above",
    /// Above 0, a level's `exit` entities take the lead out of the area
    /// (see `clamber_sim::Leaving`), and it walks out at this speed.
    /// px/frame.
    ExitWalkSpeed => "exit_walk_speed",
    /// An exit takes over a lead standing, walking or running and facing
    /// left, or on the frame it lands, with its `x` rounded down this far
    /// right of the exit or less. Whole px.
    ExitReach => "exit_reach",
    /// ... a lead rolling, facing left. Whole px.
    ExitReachRoll => "exit_reach_roll",
    /// ... a lead crouching and still sliding, facing left. Whole px.
    ExitReachCrouch => "exit_reach_crouch",
    /// ... a lead standing or turning, facing right. Whole px.
    ExitReachRight => "exit_reach_right",
    /// ... a lead standing still carrying an object, facing left. Whole
    /// px.
    ExitReachCarry => "exit_reach_carry",
    /// ... a lead standing still carrying an object, facing right. Whole
    /// px.
    ExitReachCarryRight => "exit_reach_carry_right",
    /// An open wall exit takes over a lead standing, walking or running
    /// and facing right, or on the frame it lands, with its `x` rounded
    /// down this far left of the wall exit or less. Whole px.
    WallExitReach => "wall_exit_reach",
    /// ... a lead rolling, facing right. Whole px.
    WallExitReachRoll => "wall_exit_reach_roll",
    /// ... a lead crouching, still sliding or not, facing right. Whole
    /// px.
    WallExitReachCrouch => "wall_exit_reach_crouch",
    /// ... a lead standing or turning, facing left. Whole px.
    WallExitReachLeft => "wall_exit_reach_left",
    /// An exit takes over a lead whose `y` is from this far above the
    /// exit's... Whole px.
    ExitAbove => "exit_above",
    /// ... to this far below it. Whole px.
    ExitBelow => "exit_below",
    /// The area is left from the first frame the walking lead ends this
    /// far left of the exit or further, `x` rounded down. Whole px.
    ExitLeaveAt => "exit_leave_at",
    /// Frames from that one to the area's last, on which the picture is
    /// black. Whole frames.
    ExitFadeTime => "exit_fade_time",
    /// Frames the picture stays at each brightness during that fade.
    /// Whole frames.
    ExitFadeStep => "exit_fade_step",
    /// Above 0, characters arriving in an area by walking in (see
    /// `clamber_sim::WalkingIn`) walk this fast, and the lead has this
    /// speed when the player takes over. px/frame.
    WalkInSpeed => "walk_in_speed",
    /// On the area's first frame the lead is this far behind the point it
    /// walks in to. Whole px.
    WalkInBack => "walk_in_back",
    /// The frame the lead takes its last step on, to `walk_in_ahead` past
    /// the point; the player has control from the frame after. Whole
    /// frames.
    WalkInTime => "walk_in_time",
    /// How far past the point the lead is after its last step. Whole px.
    WalkInAhead => "walk_in_ahead",
    /// How far behind the lead the other character walks in. Whole px.
    WalkInOther => "walk_in_other",
    /// The picture fades in from black a step every this many frames.
    /// Whole frames.
    WalkInFadeStep => "walk_in_fade_step",
    /// How many steps that fade takes to full brightness.
    WalkInFadeSteps => "walk_in_fade_steps",
    /// Above 0, a player falls out of the level on the first frame it
    /// ends with its feet more than this far below the level's bottom.
    /// 0: once the top of its body is below the bottom. px.
    FallOutDepth => "fall_out_depth",
    /// Above 0, frames the level goes on for after the frame the player
    /// falls out on, with input ignored and the player and the view
    /// still. 0: as for any lost life. Whole frames.
    FallOutTime => "fall_out_time",
    /// Above 0, play coming back after a lost life finds the level's
    /// things as at its start, but for restart points already broken
    /// and, coming back at one, the letters taken. 0: they stay as
    /// they were.
    ResumeAfresh => "resume_afresh",
    /// Above 1, the hits a life can take: a hit costs one hit point, and
    /// only the hit that takes the last costs the life. Coming back after
    /// a lost life restores them all. 0 or 1: every hit costs the life.
    /// Whole hit points.
    HitPoints => "hit_points",
    /// Frames from a hit that costs a hit point but not the life,
    /// counting the hit frame, that nothing hurts the player. Whole
    /// frames.
    HitProtectTime => "hit_protect_time",
    /// While protected after such a hit, the player is drawn for the
    /// first half of each stretch of this many frames, and not drawn for
    /// the second. 0: it doesn't blink. Whole frames.
    HitFlashPeriod => "hit_flash_period",
    /// Frames from such a hit, counting the hit frame, that input is
    /// ignored. Whole frames.
    HitControlWait => "hit_control_wait",
    /// On such a hit the player is knocked back, away from the enemy that
    /// hit it, or the way it doesn't face if it was something else, at
    /// this speed across. px/frame.
    HitKnockback => "hit_knockback",
    /// ... and at this speed up. px/frame.
    HitKnockbackRise => "hit_knockback_rise",
    /// Above 0, a level's `dim` entity starts the dimming of the level's
    /// background once it's near the view (see `clamber_sim::Dimming`),
    /// and this is its number of steps. 0: no dimming. Whole steps.
    DimSteps => "dim_steps",
    /// Frames from one step of the dimming to the next. Steps fall only on
    /// frames whose number is a whole multiple of it. Whole frames.
    DimStepTime => "dim_step_time",
    /// The dimming's first step is on the first frame it can fall on this
    /// many frames or more after the one its entity came near the view on.
    /// Whole frames.
    DimDelay => "dim_delay",
    /// Inside a water region the player swims (see `clamber_sim::Player`)
    /// and its horizontal speed eases toward this with Left or Right held,
    /// or 0 with no direction. px/frame.
    SwimSpeed => "swim_speed",
    /// Every swimming frame, horizontal speed closes this fraction of the
    /// gap to its target, as `speed_response` does on land.
    SwimResponse => "swim_response",
    /// A Jump press while swimming is a stroke: it sets the upward speed
    /// to this, whatever it was. px/frame.
    StrokeSpeed => "stroke_speed",
    /// ... or to this with Up held. px/frame.
    StrokeUpSpeed => "stroke_up_speed",
    /// ... or to this with Down held. px/frame.
    StrokeDownSpeed => "stroke_down_speed",
    /// Added to the downward speed every swimming frame. px/frame².
    SinkGravity => "sink_gravity",
    /// A swimmer's downward speed never exceeds this. px/frame.
    SinkSpeed => "sink_speed",
    /// ... or this on frames with Down held. px/frame.
    SinkDownSpeed => "sink_down_speed",
    /// A swimmer moving across into ground this deep or less is lifted
    /// onto it, or pushed below it when it's nearer the ground's
    /// underside; deeper ground is a wall. Whole px.
    SwimStep => "swim_step",
    /// How far below the underside of ground a swimmer pushed below it
    /// ends, between the underside and the top of its feet. Whole px.
    SwimCeilingGap => "swim_ceiling_gap",
    /// Above 0, a swimmer moving left with `x` rounded down below this
    /// stops. 0: only below x = 0. Whole px.
    SwimLeftEdge => "swim_left_edge",
    /// Above 0, a swimmer that ends a frame off the floor it rested on
    /// at the end of the frame before, without a stroke, sinks at this
    /// speed. 0: it goes on sinking as it was. px/frame.
    SwimLeaveSpeed => "swim_leave_speed",
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
                | Param::HurtFidgetOffset
                | Param::HurtAirOffset
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
                | Param::SwitchSteps
                | Param::SwitchLateSteps
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
