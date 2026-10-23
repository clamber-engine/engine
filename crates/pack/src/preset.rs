//! Feel presets: named bundles of world physics and per-slot roster
//! adjustments.
//!
//! The engine ships its own presets in `content/base/presets.toml`, and a
//! pack may add more. Each preset builds on a `base` (`default` if none is
//! given); `default` is the root and sets every movement parameter, which
//! makes it the engine's generic world defaults.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use toml::Table;

use crate::load::{LoadError, Reader};
use crate::physics::{Adjust, Param, ParamsPatch, Physics, PhysicsPatch};

/// The root preset.
pub const DEFAULT: &str = "default";

/// Adjustments a preset makes to the character in one roster slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotPatch {
    pub physics: PhysicsPatch,
    /// By ability name.
    pub abilities: BTreeMap<String, ParamsPatch>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: String,
    /// Shown to players, for example in an options menu.
    pub label: String,
    pub description: Option<String>,
    /// Exists so it can be chosen, but its values aren't final yet.
    pub placeholder: bool,
    /// The title (`spec/TITLES.md`) whose measured values this preset
    /// holds, if any. Not inherited: a preset that builds on one with a
    /// title plays differently, so it represents no title unless it says
    /// so.
    pub title: Option<String>,
    /// `None` only for [`DEFAULT`].
    pub base: Option<String>,
    pub world: PhysicsPatch,
    /// By roster slot. Presets know slots, not characters, so the same
    /// preset applies to any pack.
    pub slots: BTreeMap<usize, SlotPatch>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPreset {
    label: String,
    description: Option<String>,
    #[serde(default)]
    placeholder: bool,
    title: Option<String>,
    base: Option<String>,
    #[serde(default)]
    world: Table,
    #[serde(default)]
    roster: Table,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPresetsFile {
    presets: Table,
}

/// A set of presets, in the order they were written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presets(Vec<Preset>);

impl Presets {
    /// The engine's own presets.
    pub fn builtin() -> &'static Presets {
        static BUILTIN: OnceLock<Presets> = OnceLock::new();
        BUILTIN.get_or_init(|| {
            Presets::from_toml_str(
                include_str!("../../../content/base/presets.toml"),
                "content/base/presets.toml",
            )
            .unwrap_or_else(|e| panic!("built-in presets: {e}"))
        })
    }

    /// Reads a file with a `[presets]` table. It must define [`DEFAULT`].
    pub fn from_toml_str(src: &str, origin: &str) -> Result<Presets, LoadError> {
        let reader = Reader { origin };
        let raw: RawPresetsFile = reader.parse_document(src)?;
        let presets = Presets(read_presets(&reader, &raw.presets)?);

        let root = presets
            .get(DEFAULT)
            .ok_or_else(|| reader.err("presets", format!("no `{DEFAULT}` preset")))?;
        for &param in Param::ALL {
            if !matches!(root.world.0.get(&param), Some(Adjust::Set(_))) {
                return Err(reader.err(
                    &format!("presets.{DEFAULT}.world.{param}"),
                    "the default preset must set every movement parameter",
                ));
            }
        }
        presets.check(&reader)?;
        Ok(presets)
    }

    pub fn get(&self, id: &str) -> Option<&Preset> {
        self.0.iter().find(|p| p.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Preset> {
        self.0.iter()
    }

    /// Adds `more` after these presets. Ids must not collide.
    pub(crate) fn extended(
        &self,
        reader: &Reader,
        more: Vec<Preset>,
    ) -> Result<Presets, LoadError> {
        let mut all = self.0.clone();
        for preset in more {
            if self.get(&preset.id).is_some() {
                return Err(reader.err(
                    &format!("presets.{}", preset.id),
                    "a built-in preset already has this id",
                ));
            }
            all.push(preset);
        }
        let presets = Presets(all);
        presets.check(reader)?;
        Ok(presets)
    }

    /// The preset and its bases, root first.
    pub fn chain(&self, id: &str) -> Option<Vec<&Preset>> {
        let mut chain = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            let preset = self.get(id)?;
            if chain.iter().any(|p: &&Preset| p.id == id) {
                return None;
            }
            chain.push(preset);
            next = preset.base.as_deref();
        }
        chain.reverse();
        Some(chain)
    }

    /// The preset's world physics.
    pub fn world(&self, id: &str) -> Option<Physics> {
        let mut physics = Physics::default();
        for preset in self.chain(id)? {
            physics.apply(&preset.world);
        }
        Some(physics)
    }

    /// The physics the preset gives the character in roster slot `slot`
    /// if the character adds nothing of its own: the world physics, then
    /// each preset's adjustments for the slot, root first.
    pub fn slot(&self, id: &str, slot: usize) -> Option<Physics> {
        let mut physics = self.world(id)?;
        for preset in self.chain(id)? {
            if let Some(patch) = preset.slots.get(&slot) {
                physics.apply(&patch.physics);
            }
        }
        Some(physics)
    }

    /// Every base names a preset, and no chain loops.
    fn check(&self, reader: &Reader) -> Result<(), LoadError> {
        for preset in &self.0 {
            let path = format!("presets.{}.base", preset.id);
            if let Some(base) = &preset.base
                && self.get(base).is_none()
            {
                return Err(reader.err(&path, format!("no preset named `{base}`")));
            }
            if self.chain(&preset.id).is_none() {
                return Err(reader.err(&path, "presets build on each other in a loop"));
            }
        }
        Ok(())
    }
}

/// Reads a `[presets]` table.
pub(crate) fn read_presets(reader: &Reader, table: &Table) -> Result<Vec<Preset>, LoadError> {
    table
        .iter()
        .map(|(id, value)| read_preset(reader, id, value))
        .collect()
}

fn read_preset(reader: &Reader, id: &str, value: &toml::Value) -> Result<Preset, LoadError> {
    let path = format!("presets.{id}");
    reader.id(&path, id)?;
    let raw: RawPreset = reader.structure(&path, value)?;

    let base = match raw.base {
        Some(base) if id == DEFAULT => {
            return Err(reader.err(
                &path,
                format!("`{DEFAULT}` is the root and has no base (`{base}`)"),
            ));
        }
        Some(base) => Some(base),
        None if id == DEFAULT => None,
        None => Some(DEFAULT.to_string()),
    };

    if let Some(title) = &raw.title {
        reader.id(&format!("{path}.title"), title)?;
    }
    let world = reader.physics_patch(&format!("{path}.world"), &raw.world)?;

    let mut slots = BTreeMap::new();
    for (key, value) in &raw.roster {
        let at = format!("{path}.roster.{key}");
        let slot = key
            .parse::<usize>()
            .ok()
            .filter(|n| n.to_string() == *key)
            .ok_or_else(|| reader.err(&at, "roster slots are numbered 0, 1, 2, …"))?;
        slots.insert(slot, read_slot(reader, &at, reader.table(&at, value)?)?);
    }

    Ok(Preset {
        id: id.to_string(),
        label: raw.label,
        description: raw.description,
        placeholder: raw.placeholder,
        title: raw.title,
        base,
        world,
        slots,
    })
}

/// Movement parameters, plus an optional `abilities` table of ability
/// parameter adjustments.
fn read_slot(reader: &Reader, path: &str, table: &Table) -> Result<SlotPatch, LoadError> {
    let mut physics = table.clone();
    let mut abilities = BTreeMap::new();
    if let Some(value) = physics.remove("abilities") {
        let at = format!("{path}.abilities");
        for (name, params) in reader.table(&at, &value)? {
            let at = format!("{at}.{name}");
            reader.name(&at, name)?;
            abilities.insert(
                name.clone(),
                reader.params_patch(&at, reader.table(&at, params)?)?,
            );
        }
    }
    Ok(SlotPatch {
        physics: reader.physics_patch(path, &physics)?,
        abilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clamber_fixed::Fixed;

    fn load(src: &str) -> Result<Presets, LoadError> {
        Presets::from_toml_str(src, "test")
    }

    fn full_default() -> String {
        let mut src =
            String::from("[presets.default]\nlabel = \"Default\"\n[presets.default.world]\n");
        for p in Param::ALL {
            let value = if p.is_step_set() { "[1]" } else { "1" };
            src += &format!("{p} = {value}\n");
        }
        src
    }

    #[test]
    fn builtin_presets_load() {
        let presets = Presets::builtin();
        let ids: Vec<_> = presets.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "default",
                "snappy",
                "floaty",
                "classic-1",
                "classic-2",
                "classic-3"
            ]
        );
        for preset in presets.iter() {
            assert!(presets.world(&preset.id).is_some());
            assert_eq!(
                preset.placeholder,
                ["classic-2", "classic-3"].contains(&preset.id.as_str()),
                "{}",
                preset.id
            );
            assert_eq!(
                preset.title.as_deref(),
                match preset.id.as_str() {
                    "classic-1" => Some("title-1"),
                    "classic-2" => Some("title-2"),
                    _ => None,
                },
                "{}",
                preset.id
            );
        }
        // The default world has a positive value for every parameter but
        // the ones where 0 turns something off.
        let world = presets.world(DEFAULT).unwrap();
        for &p in Param::ALL {
            let off = matches!(
                p,
                Param::SpeedResponse
                    | Param::CrouchStart
                    | Param::CrouchMin
                    | Param::CrouchRecovery
                    | Param::CrouchUpHolds
                    | Param::CrouchTurnLock
                    | Param::CrouchTakeoffCancel
                    | Param::CrouchInAir
                    | Param::CrouchLandWait
                    | Param::CrouchJumpPress
                    | Param::ActionJumpPress
                    | Param::TurnTime
                    | Param::ShortTurnSpeed
                    | Param::StrideSteps
                    | Param::BlockedStands
                    | Param::WalkStride
                    | Param::WalkStrideLess
                    | Param::RunStride
                    | Param::StrideMin
                    | Param::StrideScale
                    | Param::RunStrideSteps
                    | Param::WalkTurnSteps
                    | Param::WalkRunSteps
                    | Param::RunTurnSteps
                    | Param::WalkStandSpeed
                    | Param::RunStandSpeed
                    | Param::TurnRunSpeed
                    | Param::RunWalkSpeed
                    | Param::RunWalkOnLastStep
                    | Param::JumpDelay
                    | Param::LandingLockout
                    | Param::FallLandingLockout
                    | Param::FaceFallJump
                    | Param::SwitchFrom
                    | Param::SwitchReach
                    | Param::SwitchSteps
                    | Param::SwitchLateSteps
                    | Param::RollJumpSwitch
                    | Param::LandingModeWait
                    | Param::LandingModeOnLast
                    | Param::LandingModeAfterLast
                    | Param::JumpLandingMode
                    | Param::FaceFallLanding
                    | Param::JumpStartHold
                    | Param::TakeoffWait
                    | Param::JumpBufferHold
                    | Param::HeldJumpGravity
                    | Param::LandingReach
                    | Param::LeaveDrop
                    | Param::LeaveFallSpeed
                    | Param::WalkOffKeepsFacing
                    | Param::RunFromPreviousFrame
                    | Param::LeaveStepSpeed
                    | Param::LeaveFacingWait
                    | Param::HeldGravityUntilTop
                    | Param::GroundedTakeoff
                    | Param::NoFaceCoyote
                    | Param::FacePressGravity
                    | Param::FaceJumpWait
                    | Param::FaceRolls
                    | Param::FaceRollWait
                    | Param::TakeoffOverEnd
                    | Param::RollTime
                    | Param::RollSpeed
                    | Param::RollSlowSpeed
                    | Param::RollFastSpeed
                    | Param::RollFastWindow
                    | Param::RollFastWindowNeutral
                    | Param::RollFastHold
                    | Param::RollLoneRunAnyDir
                    | Param::RollKeepsSize
                    | Param::RollSteerSpeed
                    | Param::RollSlideTurns
                    | Param::RollStopPressLengthens
                    | Param::RollNotOnDown
                    | Param::RollCarryTime
                    | Param::RollStopTime
                    | Param::RollAfterStop
                    | Param::RollStopSlide
                    | Param::RollBuffer
                    | Param::RollBufferHold
                    | Param::RollAirBuffer
                    | Param::RollAirLengthen
                    | Param::RollHover
                    | Param::RollLeaveHover
                    | Param::RollLeaveHoverTime
                    | Param::RollFallEnd
                    | Param::RollEdgeSpeed
                    | Param::RollEdgeAhead
                    | Param::RollEdgeDepth
                    | Param::RollEdgePush
                    | Param::RollJumpHeldGravity
                    | Param::RopeHeldGravity
                    | Param::RollAirStopWait
                    | Param::RollStopAir
                    | Param::RollAttackWidth
                    | Param::RollAttackOffset
                    | Param::RollAttackStop
                    | Param::RollStopGuard
                    | Param::RollDefeatFreeze
                    | Param::RollBoost
                    | Param::RollBoostCap
                    | Param::RollBoostAtDefeat
                    | Param::RollDefeatHoldsWalkers
                    | Param::GroundActionTime
                    | Param::GroundActionSpeed
                    | Param::GroundAttackWidth
                    | Param::GroundAttackOffset
                    | Param::GroundAttackFrames
                    | Param::GroundAttackPause
                    | Param::BounceFreeze
                    | Param::BounceKeepsFacing
                    | Param::BounceLandingSwitch
                    | Param::BounceWidth
                    | Param::BounceOffset
                    | Param::BounceBelow
                    | Param::BounceMaxRise
                    | Param::BounceHoldsWalkers
                    | Param::BounceHoldsAll
                    | Param::BounceAgain
                    | Param::BounceHoldFromStart
                    | Param::BounceBackRise
                    | Param::BounceBackAway
                    | Param::BounceBackSame
                    | Param::BounceBackMinAbove
                    | Param::PushSpeed
                    | Param::PushEnd
                    | Param::StanceStandSpeed
                    | Param::StanceRunSpeed
                    | Param::FaceSlope
                    | Param::SlopeLandingReach
                    | Param::SlopePiece
                    | Param::FaceSpeed
                    | Param::FaceSlideSwitch
                    | Param::FaceTopLanding
                    | Param::FaceDropLeave
                    | Param::StartRiseAhead
                    | Param::RiseKeepsTarget
                    | Param::AirStep
                    | Param::AirWholePixels
                    | Param::StillFalls
                    | Param::FacePull
                    | Param::FaceSpeedLimit
                    | Param::FaceLimitedSpeed
                    | Param::FaceMinSpeed
                    | Param::FaceTurns
                    | Param::FaceLeaveDrop
                    | Param::FaceKeptFraction
                    | Param::FollowDrop
                    | Param::FollowSteepSlope
                    | Param::FollowOntoSlope
                    | Param::LeaveStepNear
                    | Param::HurtWidth
                    | Param::HurtOffset
                    | Param::HurtJumpTime
                    | Param::HurtJumpLift
                    | Param::HurtFidgetWidth
                    | Param::HurtFidgetOffset
                    | Param::HurtCrouchWidth
                    | Param::HurtCrouchOffset
                    | Param::HurtCarryWidth
                    | Param::HurtCarryOffset
                    | Param::HurtAirWidth
                    | Param::HurtAirOffset
                    | Param::HurtLandHold
                    | Param::HurtLandLift
                    | Param::HurtLongJump
                    | Param::HurtLongJumpEnd
                    | Param::HurtLongLandHold
                    | Param::HurtFallHold
                    | Param::HurtFallLift
                    | Param::FidgetStart
                    | Param::FidgetLength
                    | Param::FidgetEvery
                    | Param::KnockoutJump
                    | Param::KnockoutSpeed
                    | Param::KnockoutMaxFall
                    | Param::KnockoutHold
                    | Param::KnockoutPause
                    | Param::KnockoutPauseAfter
                    | Param::KnockoutRebound1
                    | Param::KnockoutRebound2
                    | Param::KnockoutRest
                    | Param::KnockoutGravity
                    | Param::KnockoutWithEnemy
                    | Param::KnockoutEnemyWait
                    | Param::KnockoutHopAt
                    | Param::KnockoutHop
                    | Param::KnockoutHopSlowing
                    | Param::KnockoutTime
                    | Param::SwapMoves
                    | Param::SwapWait
                    | Param::SwapJumpWait
                    | Param::SwapLag
                    | Param::SwapHop
                    | Param::SwapHeight
                    | Param::SwapCrouchWait
                    | Param::SwapRiseWait
                    | Param::SwapTandemOnEnd
                    | Param::LoseJump
                    | Param::LoseRunSpeed
                    | Param::TakeoverWait
                    | Param::TakeoverShare
                    | Param::TakeoverMinMove
                    | Param::TakeoverNudge
                    | Param::TakeoverHop
                    | Param::TakeoverHop3
                    | Param::ProtectTime
                    | Param::FlashStart
                    | Param::FlashEnd
                    | Param::FlashPeriod
                    | Param::FollowTrail
                    | Param::FollowLag
                    | Param::FollowGap
                    | Param::FollowRestSpeed
                    | Param::FollowRestReach
                    | Param::FollowRestShare
                    | Param::FollowRejoin
                    | Param::FollowTurn
                    | Param::FollowTurnFlip
                    | Param::PickUpTime
                    | Param::PickUpStill
                    | Param::CarryJumpDelay
                    | Param::CarryJumpSpeed
                    | Param::CarryHeldGravity
                    | Param::CarryStrideSteps
                    | Param::CarryStride
                    | Param::CarryStrideMin
                    | Param::CarryStrideScale
                    | Param::CarryStandSteps
                    | Param::CarryStandSpeed
                    | Param::ThrowTime
                    | Param::ThrowStill
                    | Param::ThrowRelease
                    | Param::ThrowRollPress
                    | Param::ThrowLift
                    | Param::ThrowSpeed
                    | Param::ThrowDrop
                    | Param::ThrowGlide
                    | Param::ThrowGravity
                    | Param::ThrowMaxFall
                    | Param::ThrowLandMargin
                    | Param::ThrowBounceMin
                    | Param::ThrowBounce
                    | Param::PutDownTime
                    | Param::PutDownAhead
                    | Param::PutDownLift
                    | Param::PutDownRegrab
                    | Param::LetGoSpeed
                    | Param::LetGoRise
                    | Param::CountLife
                    | Param::LetterCount
                    | Param::LetterRowIn
                    | Param::LetterRowSlide
                    | Param::LetterRowOut
                    | Param::LetterRowY
                    | Param::LetterRowRise
                    | Param::LetterRowSpeed
                    | Param::LetterRowX
                    | Param::LetterRowGap
                    | Param::LetterEndMove
                    | Param::LetterEndStagger
                    | Param::LetterEndBlinks
                    | Param::LetterEndBlink
                    | Param::LetterEndLate
                    | Param::LetterEndX
                    | Param::LetterEndApproach
                    | Param::LetterEndLeast
                    | Param::LetterEndLivesIn
                    | Param::LetterEndLivesOut
                    | Param::LetterEndLivesEnd
                    | Param::LetterEndLivesHeld
                    | Param::CountShowFor
                    | Param::CountX
                    | Param::CountY
                    | Param::CountDigitX
                    | Param::CountDigitGap
                    | Param::LivesSlide
                    | Param::PlayLivesIn
                    | Param::PlayLivesOut
                    | Param::LivesY
                    | Param::LivesRise
                    | Param::LivesGone
                    | Param::LivesSpeed
                    | Param::LivesX
                    | Param::LivesDigitGap
                    | Param::LifeItemLivesIn
                    | Param::LifeItemLivesArrive
                    | Param::LifeItemLivesStep
                    | Param::LifeItemLivesEvery
                    | Param::LifeItemLivesOut
                    | Param::LifeItemLivesEnd
                    | Param::LostLivesIn
                    | Param::LostLivesFallIn
                    | Param::LostLivesBurst
                    | Param::LostLivesFallBurst
                    | Param::LostLivesBurstFor
                    | Param::LostLivesAgain
                    | Param::RelicApproach
                    | Param::RelicSize
                    | Param::RelicX
                    | Param::RelicY
                    | Param::RelicRest
                    | Param::RelicRise
                    | Param::ViewWidth
                    | Param::ViewBack
                    | Param::ViewBackLeft
                    | Param::ViewLeadFacing
                    | Param::ViewLeadSpeed
                    | Param::ViewShare
                    | Param::ViewSpeedShare
                    | Param::ViewHold
                    | Param::ViewKnockoutHold
                    | Param::ViewNearLeft
                    | Param::ViewBeginBack
                    | Param::ViewBeginBackFar
                    | Param::ViewBeginFar
                    | Param::ViewFastSpeedShare
                    | Param::EndStop
                    | Param::ViewHeight
                    | Param::ViewFeet
                    | Param::ViewFeetTop
                    | Param::ViewBand
                    | Param::ViewReach
                    | Param::ViewShareY
                    | Param::ViewSpeedShareY
                    | Param::ViewShareYQuick
                    | Param::ViewSpeedShareYQuick
                    | Param::ViewShareYFast
                    | Param::ViewSpeedShareYFast
                    | Param::ViewBeginHoldY
                    | Param::ViewQuickBelow
                    | Param::ViewQuickFallBelow
                    | Param::ViewQuickFall
                    | Param::ViewSlackY
                    | Param::ViewShakeCount
                    | Param::ViewShakeDelay
                    | Param::ViewShakePeriod
                    | Param::ViewShakeSize
                    | Param::ViewBeginShakeCount
                    | Param::ViewBeginShakeDelay
                    | Param::ViewBeginShakeSize
                    | Param::ViewPullShare
                    | Param::ViewPullMax
                    | Param::ViewLagKeep
                    | Param::ViewLagShare
                    | Param::ViewLagMin
                    | Param::ViewSway
                    | Param::ViewSwayRate
                    | Param::ViewPullShareY
                    | Param::ViewPullMaxY
                    | Param::ViewLagShareY
                    | Param::ViewPullShareYMedium
                    | Param::ViewPullMaxYMedium
                    | Param::ViewLagShareYMedium
                    | Param::ViewPullShareYQuick
                    | Param::ViewPullMaxYQuick
                    | Param::ViewLagShareYQuick
                    | Param::ViewMediumFall
                    | Param::ViewRiseRows
                    | Param::ViewRise
                    | Param::ViewFall
                    | Param::ViewMoveMaxY
                    | Param::ViewSwayRateY
                    | Param::ViewSwayPhaseY
                    | Param::ViewBandOffset
                    | Param::ViewBandAbove
                    | Param::AppearLeft
                    | Param::AppearRight
                    | Param::AppearAbove
                    | Param::AppearBelow
                    | Param::RemoveLeft
                    | Param::RemoveRight
                    | Param::AppearByView
                    | Param::Places
                    | Param::Pictures
                    | Param::EndReach
                    | Param::EndWalkSpeed
                    | Param::EndRise
                    | Param::EndFadeAt
                    | Param::EndFadeTime
                    | Param::EndFadeStep
                    | Param::EndOnwardFadeFrom
                    | Param::EndOnwardHold
                    | Param::EndLetGoRight
                    | Param::EndLetGoAbove
                    | Param::ExitWalkSpeed
                    | Param::ExitReach
                    | Param::ExitReachRoll
                    | Param::ExitReachCrouch
                    | Param::ExitReachRight
                    | Param::ExitReachCarry
                    | Param::ExitReachCarryRight
                    | Param::WallExitReach
                    | Param::WallExitReachRoll
                    | Param::WallExitReachCrouch
                    | Param::WallExitReachLeft
                    | Param::ExitAbove
                    | Param::ExitBelow
                    | Param::ExitLeaveAt
                    | Param::ExitFadeTime
                    | Param::ExitFadeStep
                    | Param::WalkInSpeed
                    | Param::WalkInBack
                    | Param::WalkInTime
                    | Param::WalkInAhead
                    | Param::WalkInOther
                    | Param::WalkInFadeStep
                    | Param::WalkInFadeSteps
                    | Param::FallOutDepth
                    | Param::FallOutTime
                    | Param::ResumeAfresh
                    | Param::DimSteps
                    | Param::DimStepTime
                    | Param::DimDelay
                    | Param::JoinTime
                    | Param::JoinLift
                    | Param::JoinRise
                    | Param::JoinSag
                    | Param::SwimLeftEdge
                    | Param::SwimLeaveSpeed
                    | Param::SwimFastSpeed
                    | Param::SinkDownKeptOnUp
                    | Param::SwimTurnFace
                    | Param::SwimTurnTime
                    | Param::SwimTurnStrokeFace
                    | Param::SwimTurnStrokeFree1
                    | Param::SwimTurnStrokeFree2
                    | Param::SwimTurnStrokeFree3
                    | Param::SwimTurnStrokeFree4
                    | Param::SwimTurnStrokeFree5
                    | Param::SwimTurnStrokeFree6
                    | Param::SwimTurnStrokeFree7
                    | Param::SwimTurnStrokeFree8
                    | Param::SwimTurnStrokeFree9
                    | Param::SwimTurnStrokeFree10
                    | Param::SwimTurnStrokeFree11
                    | Param::SwimGroundBelow
                    | Param::SwimScreenMargin
            );
            assert!(world[p] > Fixed::ZERO || off, "{p}");
        }
    }

    #[test]
    fn inheritance_applies_root_first() {
        let src = full_default()
            + "[presets.a]\nlabel = \"A\"\nworld.gravity = 2\n\
               [presets.b]\nlabel = \"B\"\nbase = \"a\"\nworld.gravity = { scale = 1.5 }\n";
        let presets = load(&src).unwrap();
        assert_eq!(presets.get("a").unwrap().base.as_deref(), Some(DEFAULT));
        let ids: Vec<_> = presets.chain("b").unwrap().iter().map(|p| &p.id).collect();
        assert_eq!(ids, ["default", "a", "b"]);
        let world = presets.world("b").unwrap();
        assert_eq!(world[Param::Gravity], Fixed::from_int(3));
        assert_eq!(world[Param::TopSpeed], Fixed::ONE);
    }

    #[test]
    fn slots_and_ability_patches() {
        let src = full_default()
            + "[presets.a]\nlabel = \"A\"\n\
               [presets.a.roster.1]\ntop_speed = 4\nabilities.glide.fall_speed = { scale = 0.5 }\n";
        let presets = load(&src).unwrap();
        let slot = &presets.get("a").unwrap().slots[&1];
        assert_eq!(
            slot.physics.0[&Param::TopSpeed],
            Adjust::Set(Fixed::from_int(4))
        );
        assert_eq!(
            slot.abilities["glide"].0["fall_speed"],
            Adjust::Scale(Fixed::from_ratio_pow2(1, 1))
        );
        assert_eq!(
            presets.slot("a", 1).unwrap()[Param::TopSpeed],
            Fixed::from_int(4)
        );
        assert_eq!(presets.slot("a", 0), presets.world("a"));
        assert_eq!(presets.slot("b", 1), None);
    }

    #[test]
    fn rejects_bad_presets() {
        let cases = [
            (
                "[presets.x]\nlabel = \"X\"\n",
                "test: presets: no `default` preset",
            ),
            (
                "[presets.default]\nlabel = \"D\"\nworld.gravity = 1\n",
                "test: presets.default.world.fall_gravity: the default preset must set every movement parameter",
            ),
        ];
        for (src, want) in cases {
            assert_eq!(load(src).unwrap_err().to_string(), want);
        }

        let cases = [
            (
                "[presets.a]\nlabel = \"A\"\nbase = \"nope\"\n",
                "presets.a.base: no preset named `nope`",
            ),
            (
                "[presets.a]\nlabel = \"A\"\nbase = \"b\"\n[presets.b]\nlabel = \"B\"\nbase = \"a\"\n",
                "presets.a.base: presets build on each other in a loop",
            ),
            (
                "[presets.a]\nlabel = \"A\"\nroster.01.top_speed = 1\n",
                "presets.a.roster.01: roster slots are numbered 0, 1, 2, …",
            ),
            (
                "[presets.a]\nlabel = \"A\"\nroster.x.top_speed = 1\n",
                "presets.a.roster.x: roster slots are numbered 0, 1, 2, …",
            ),
            (
                "[presets.a]\nlabel = \"A\"\nroster.0.speed = 1\n",
                "presets.a.roster.0.speed: unknown movement parameter",
            ),
            (
                "[presets.A]\nlabel = \"A\"\n",
                "presets.A: `A` is not a valid id (use a-z, 0-9, - and _)",
            ),
        ];
        for (extra, want) in cases {
            let err = load(&(full_default() + extra)).unwrap_err();
            assert_eq!(err.message, want);
        }

        let err = load(&(full_default() + "[presets.a]\nlabel = \"A\"\ncolour = 1\n")).unwrap_err();
        assert!(
            err.message.starts_with("presets.a: unknown field `colour`"),
            "{err}"
        );
        let err = load(
            &full_default().replace("[presets.default]\n", "[presets.default]\nbase = \"x\"\n"),
        )
        .unwrap_err();
        assert!(err.message.contains("is the root"), "{err}");
    }
}
