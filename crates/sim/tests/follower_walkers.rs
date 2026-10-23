//! Walkers and the follower in Classic 1 (spec/behavior/following.md,
//! "Walkers and the follower"): a walker neither hurts the follower nor
//! gives it a bounce, and walks on as if it weren't there.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{Event, Facing, InputFrame, Other, World, generic_feel, step};

/// A walker with a contact table, as `title-2`'s (the ranges are the
/// walker's own, whatever the game).
const WALKER: &str = "size = [20, 18], offset = 0, response = [0.0625, 0.015625], \
    move_first = true, columns = true, turn_delay = 0, bounced_speed = 1, \
    defeat_jump = 8, defeat_gravity = 0.4375, defeat_max_fall = 12, \
    contact = { stand = [-23, 30, -36, 20], crouch = [-23, 30, -22, 22], \
    walk = [-25, 28, -36, 20], run = [-28, 29, -32, 17], air = [-26, 29, -28, 21], \
    roll = [-25, 27, -36, 20], turn = 7, land_hold = [17, 8], bounce_rise = 1.25, \
    bounce_above = -10 }";

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Classic 1 with both characters on flat floor at y = 400: the lead far
/// from the follower, which rests just behind the lead, to its right, and a walker at `walker_x`
/// heading left, if there is one.
fn world(walker_x: Option<i32>) -> World {
    let entities = walker_x
        .map(|x| {
            format!(
                r#"
                [[entities]]
                type = "crawler"
                at = [{x}, 400]
                params = {{ speed = 1, facing = "left", {WALKER} }}
                "#
            )
        })
        .unwrap_or_default();
    let src = format!(
        r#"
        format = 1
        name = "arena"
        size = [1000, 600]
        start = [330, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 1000, 200]

        {entities}
        "#
    );
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    let level = Level::from_toml_str(&src, "test").unwrap();
    let mut w = World::new(Arc::new(level), Arc::new(feel));
    step(
        &mut w,
        &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
    );
    w.player.facing = Facing::Left;
    let lead = w.player.pos;
    let spot = Vec2::new(lead.x + fx(27.0), fx(400.0));
    w.other = Some(Other::new(1, spot, Facing::Left, lead));
    w
}

#[test]
fn a_walker_passing_through_the_follower_changes_nothing() {
    let mut with = world(Some(520));
    let mut without = world(None);
    let mut met = false;
    for frame in 0..200 {
        for w in [&mut with, &mut without] {
            step(
                w,
                &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
            );
        }
        let gap = with.things[0].pos.x.to_f64() - with.other.as_ref().unwrap().pos.x.to_f64();
        met |= gap.abs() < 10.0;
        if with.events.contains(&Event::Hurt) {
            // The lead met the walker after the follower had; only the
            // lead is hurt.
            break;
        }
        assert_eq!(with.other, without.other, "frame {frame}");
    }
    assert!(met, "the walker never met the follower");
}
