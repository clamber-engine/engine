//! Classic 1's player carrying an object among walkers: the carrying
//! pose's hurt ranges (spec/behavior/walker-contact.md, "Carrying") and
//! the carried object defeating walkers (spec/behavior/thrown-objects.md,
//! "A carried object").

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{
    Act, Event, Facing, Fixed, FlowEvent, Hands, InputFrame, Phase, World, generic_feel, step,
};

const CLASSIC_WALKER: &str = "response = 0.125, turn_delay = 4, \
    defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8";

/// Small, medium and large walkers as in walker-contact.md, with the
/// sizes and offsets the classic-1 boxes go with.
const SMALL: &str = "size = [20, 12], offset = 0.5";
const MEDIUM: &str = "size = [25, 25], offset = 5";
const LARGE: &str = "size = [26, 38], offset = 0.5";

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat floor at y = 400, the player at `player_x` facing `facing` and
/// carrying a crate, and a walker of `kind` at `walker_x` with `speed`
/// heading `heading`. The walker is thing 0, the crate thing 1.
fn carrying(player_x: f64, facing: Facing, kind: &str, walker: (f64, f64, &str)) -> World {
    let (walker_x, speed, heading) = walker;
    let src = format!(
        r#"
        format = 1
        name = "carried"
        size = [1000, 600]
        start = [{}, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 1000, 200]

        [[entities]]
        type = "crawler"
        at = [{walker_x}, 400]
        params = {{ speed = {speed}, facing = "{heading}", {kind}, {CLASSIC_WALKER} }}

        [[entities]]
        type = "crate"
        at = [900, 400]
        "#,
        player_x.floor()
    );
    let level = Level::from_toml_str(&src, "carried").unwrap();
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    world.player.pos.x = fx(player_x);
    world.player.facing = facing;
    world.player.hands = Some(Hands {
        thing: 1,
        act: Act::Carry,
        frame: 30,
        press: None,
    });
    world.things[1].phase = Phase::Held;
    world
}

/// One step with Action held.
fn carry_step(world: &mut World) {
    let held: Actions = Action::Action.into();
    step(world, &InputFrame::single(Frame::after(held, held)));
}

/// Whether a carrying player standing `i` px from a still walker of
/// `kind` heading `heading` is hurt by it on the second frame and not the
/// first. The crate is taken out so it can't break on the walker first.
fn carrying_hurt_at(kind: &str, i: i32, facing: Facing, heading: &str) -> bool {
    let mut w = carrying(f64::from(300 + i), facing, kind, (300.0, 0.0, heading));
    w.things[1].active = false;
    carry_step(&mut w);
    assert!(!w.events.contains(&Event::Hurt), "hurt on the first frame");
    carry_step(&mut w);
    w.events.contains(&Event::Hurt)
}

#[test]
fn carrying_narrows_the_hurt_range() {
    // walker-contact.md, "Carrying": facing right the low end is 1 px
    // higher and the high end 5 px lower; facing left 5 px higher and
    // 1 px lower. Upright ranges from the table under "Being hurt".
    let table = [
        (SMALL, Facing::Right, "left", -19, 24),
        (SMALL, Facing::Right, "right", -18, 25),
        (SMALL, Facing::Left, "left", -25, 18),
        (SMALL, Facing::Left, "right", -24, 19),
        (MEDIUM, Facing::Right, "left", -26, 22),
        (MEDIUM, Facing::Right, "right", -16, 32),
        (MEDIUM, Facing::Left, "left", -32, 16),
        (MEDIUM, Facing::Left, "right", -22, 26),
        (LARGE, Facing::Right, "left", -22, 27),
        (LARGE, Facing::Right, "right", -21, 28),
        (LARGE, Facing::Left, "left", -28, 21),
        (LARGE, Facing::Left, "right", -27, 22),
    ];
    for (n, (kind, facing, heading, low, high)) in table.into_iter().enumerate() {
        let (low, high) = match facing {
            Facing::Right => (low + 1, high - 5),
            Facing::Left => (low + 5, high - 1),
        };
        for (i, hurt) in [
            (low - 1, false),
            (low, true),
            (high, true),
            (high + 1, false),
        ] {
            assert_eq!(
                carrying_hurt_at(kind, i, facing, heading),
                hurt,
                "row {n}: I = {i}, facing {facing:?}, heading {heading}"
            );
        }
    }
}

#[test]
fn the_spec_examples_of_carrying_hurt_ranges() {
    // Facing right, a small walker heading left: −18 to 19. Facing left,
    // a medium walker heading right: −17 to 25; a large one heading left:
    // −23 to 20.
    for (kind, facing, heading, low, high) in [
        (SMALL, Facing::Right, "left", -18, 19),
        (MEDIUM, Facing::Left, "right", -17, 25),
        (LARGE, Facing::Left, "left", -23, 20),
    ] {
        assert!(!carrying_hurt_at(kind, low - 1, facing, heading));
        assert!(carrying_hurt_at(kind, low, facing, heading));
        assert!(carrying_hurt_at(kind, high, facing, heading));
        assert!(!carrying_hurt_at(kind, high + 1, facing, heading));
    }
}

/// Whether the crate a player carries at `object_x` breaks on a still
/// large walker at 300 facing `heading` on the first step.
fn breaks_on_large_walker(object_x: i32, facing: Facing, heading: &str) -> bool {
    let mut w = carrying(f64::from(object_x), facing, LARGE, (300.0, 0.0, heading));
    carry_step(&mut w);
    let broke = !w.things[1].active;
    assert_eq!(broke, w.player.hands.is_none());
    assert_eq!(broke, w.flow.events.contains(&FlowEvent::Defeat(0)));
    broke
}

#[test]
fn a_carried_object_breaks_on_a_large_walker_in_the_measured_ranges() {
    // thrown-objects.md, "When the walker is hit": I is the object's x
    // less the walker's.
    for (heading, facing, low, high) in [
        ("right", Facing::Right, -25, 31),
        ("right", Facing::Left, -30, 26),
        ("left", Facing::Right, -26, 30),
        ("left", Facing::Left, -31, 25),
    ] {
        let row = format!("walker {heading}, player {facing:?}");
        assert!(
            !breaks_on_large_walker(300 + low - 1, facing, heading),
            "{row}"
        );
        assert!(breaks_on_large_walker(300 + low, facing, heading), "{row}");
        assert!(breaks_on_large_walker(300 + high, facing, heading), "{row}");
        assert!(
            !breaks_on_large_walker(300 + high + 1, facing, heading),
            "{row}"
        );
    }
}

#[test]
fn a_carried_object_never_reaches_a_small_walker_on_the_players_floor() {
    for i in -40..=40 {
        let mut w = carrying(
            f64::from(300 + i),
            Facing::Right,
            SMALL,
            (300.0, 0.0, "left"),
        );
        carry_step(&mut w);
        assert!(w.things[1].active, "I = {i}");
    }
}

#[test]
fn a_carried_object_breaks_on_a_large_walker_as_in_the_example() {
    // thrown-objects.md, "A carried object", "Example", 100 px further
    // right and 400 lower: the walker heads left at full speed and is at
    // 227.5 on K − 1 (I = −27) and 226.5 on K (I = −26).
    let mut w = carrying(200.0, Facing::Right, LARGE, (237.5, 1.0, "left"));
    for frame in 1..=10 {
        carry_step(&mut w);
        assert!(w.things[1].active, "broke on frame {frame}");
    }
    assert_eq!(w.things[0].pos.x, fx(227.5));
    carry_step(&mut w);
    // K: the object breaks, the walker's vx is −4 and it heads left.
    assert!(!w.things[1].active);
    assert!(w.player.hands.is_none());
    assert_eq!(w.things[0].pos.x, fx(226.5));
    assert_eq!(w.things[0].vx, fx(-4.0));
    assert!(w.flow.events.contains(&FlowEvent::Defeat(0)));
    let rows = [
        (222.75, 400.0),
        (219.21875, 394.4375),
        (215.87890625, 389.3125),
    ];
    for (n, (x, y)) in rows.into_iter().enumerate() {
        carry_step(&mut w);
        let walker = &w.things[0];
        assert_eq!(
            (walker.pos.x, walker.pos.y),
            (fx(x), fx(y)),
            "K + {}",
            n + 1
        );
        assert!(!w.events.contains(&Event::Hurt));
    }
}

#[test]
fn the_walker_a_carried_object_broke_on_can_still_hurt_on_the_next_frame() {
    // thrown-objects.md, "What happens on a hit": at the end of K the
    // walker still counts as walking. Object and player at 300 with the
    // walker still at 300: within the hit range and the hurt range.
    let mut w = carrying(300.0, Facing::Right, LARGE, (300.0, 0.0, "left"));
    carry_step(&mut w);
    assert!(!w.things[1].active);
    assert!(!w.events.contains(&Event::Hurt));
    carry_step(&mut w);
    assert!(w.events.contains(&Event::Hurt));
}

#[test]
fn a_crate_just_picked_up_doesnt_break_on_its_first_carried_frame() {
    // "the player was carrying the object at the end of the frame
    // before": the carrying starts on G + 15. I = 29 is within the hit
    // range and outside the hurt range.
    let mut w = carrying(329.0, Facing::Right, LARGE, (300.0, 0.0, "left"));
    w.player.hands = Some(Hands {
        thing: 1,
        act: Act::PickUp,
        frame: 14,
        press: None,
    });
    carry_step(&mut w);
    assert_eq!(w.player.hands.map(|h| h.act), Some(Act::Carry));
    assert!(w.things[1].active);
    carry_step(&mut w);
    assert!(!w.things[1].active);
}
