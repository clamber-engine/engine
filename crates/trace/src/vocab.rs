//! State and event names from `spec/traces/VOCABULARY.md`.
//!
//! `tests/spec_files.rs` checks these lists against the pinned spec, so a
//! spec bump that adds names fails until they're added here too.

names! {
    /// A player state, as recorded in a trace's `state` column.
    pub enum State {
        Idle => "idle",
        Walk => "walk",
        Run => "run",
        Roll => "roll",
        Jump => "jump",
        Fall => "fall",
        Bounce => "bounce",
        Hurt => "hurt",
        Swap => "swap",
        GroundAction => "ground_action",
        Swim => "swim",
        Rope => "rope",
    }
}

names! {
    /// Something that happened on a frame, as recorded in a trace's
    /// `events` column.
    pub enum Event {
        JumpStart => "jump_start",
        JumpRelease => "jump_release",
        Land => "land",
        LeaveGround => "leave_ground",
        RollStart => "roll_start",
        RollEnd => "roll_end",
        Bounce => "bounce",
        Hurt => "hurt",
        Turn => "turn",
        Stroke => "stroke",
        RopeCatch => "rope_catch",
        RopeSlideOff => "rope_slide_off",
    }
}
