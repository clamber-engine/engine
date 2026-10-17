//! Signs on `classic-1` (spec/behavior/signs.md): scenery that appears
//! and is removed with the view and that nothing touches.

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_sim::{Fixed, InputFrame, View, World, generic_feel, step};

/// Flat ground at y = 400, the player standing at x = `player`, a small
/// walker at x = 600 heading left, and, with `sign`, a sign standing on
/// the ground at that x.
fn world(player: i32, sign: Option<i32>) -> World {
    let mut src = format!(
        r#"
        format = 1
        size = [4000, 600]
        start = [{player}, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]

        [[entities]]
        type = "crawler"
        at = [600, 400]
        params = {{ speed = 1, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }}
        "#
    );
    if let Some(x) = sign {
        src += &format!(
            r#"
            [[entities]]
            type = "signpost"
            at = [{x}, 400]
            "#
        );
    }
    let level = Level::from_toml_str(&src, "signs").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One step with the view's left edge at `left` when it starts. Returns
/// whether the sign is in the level after the step.
fn step_at(world: &mut World, left: i32) -> bool {
    world.view = Some(View {
        left: Fixed::from_int(left),
        ..View::default()
    });
    step(world, &InputFrame::default());
    world.things[1].active
}

/// "Appearing and being removed", with the measured edges: there at
/// `x` − `V` = −127 and 384, removed at −128 and 385, appearing at 288
/// from the right and at −31 from the left, any number of times.
#[test]
fn appears_and_is_removed_with_the_view_and_comes_back() {
    let x = 2000;
    let mut w = world(300, Some(x));
    assert!(!w.things[1].active, "out of the window on the first frame");
    for _ in 0..2 {
        assert!(!step_at(&mut w, x - 290));
        assert!(!step_at(&mut w, x - 289));
        assert!(step_at(&mut w, x - 288));
        assert_eq!(w.things[1].pos.x, Fixed::from_int(x));
        assert!(step_at(&mut w, x - 384));
        assert!(!step_at(&mut w, x - 385));
        assert!(!step_at(&mut w, x + 33));
        assert!(!step_at(&mut w, x + 32));
        assert!(step_at(&mut w, x + 31));
        assert!(step_at(&mut w, x + 127));
        assert!(!step_at(&mut w, x + 128));
        assert!(!step_at(&mut w, x + 32));
    }
}

#[test]
fn a_sign_in_the_window_on_the_first_frame_is_there() {
    let w = world(300, Some(420));
    assert!(w.things[1].active);
}

/// "What a sign does": nothing. A run with a sign in the way goes exactly
/// as the same run without one: the player walks, runs, rolls, crouches
/// and jumps through it, and the walker walks past it.
#[test]
fn nothing_touches_a_sign() {
    let mut with = world(300, Some(420));
    let mut without = world(300, None);
    let choices: [&[Action]; 8] = [
        &[Action::Right],
        &[Action::Right, Action::Action],
        &[Action::Right, Action::Jump],
        &[Action::Down],
        &[Action::Action],
        &[Action::Left],
        &[Action::Jump],
        &[],
    ];
    let mut seed = 12345u32;
    let mut held = Actions::NONE;
    let mut near = 0;
    for frame in 0..900 {
        if frame % 8 == 0 {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            held = choices[(seed >> 16) as usize % choices.len()]
                .iter()
                .copied()
                .collect();
        }
        let input = InputFrame::single(Frame::after(held, held));
        let before = without.player.pos;
        step(&mut with, &input);
        step(&mut without, &input);
        let (a, b) = (&with.player, &without.player);
        assert_eq!(
            (a.pos, a.vel, a.facing, a.state),
            (b.pos, b.vel, b.facing, b.state),
            "frame {frame}"
        );
        let (a, b) = (&with.things[0], &without.things[0]);
        assert_eq!(
            (a.active, a.pos, a.vx, a.facing, a.phase),
            (b.active, b.pos, b.vx, b.facing, b.phase),
            "walker on frame {frame}"
        );
        assert_eq!(with.flow.events, without.flow.events, "frame {frame}");
        assert_eq!(with.things[1].pos.x, Fixed::from_int(420));
        if (before.x - Fixed::from_int(420)).abs() < Fixed::from_int(48) {
            near += 1;
        }
    }
    assert!(near > 100, "the player was near the sign on {near} frames");
}
