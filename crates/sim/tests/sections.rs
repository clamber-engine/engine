//! Placed things appearing by sections, on `classic-1`
//! (spec/formats/placements.md, "In a list with sections"). The view is
//! put where each step starts.

use std::sync::Arc;

use clamber_level::Level;
use clamber_sim::{Fixed, InputFrame, View, World, generic_feel, step};

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// Flat ground at y = 400 and the player standing at x = 300. Two
/// sections meet at (2000, 50): the first holds a walker starting at
/// x = 1000 (entity 0) and a post at x = 1100 (entity 1), the second a
/// post at (2100, 300) that needs the view's rows (entity 2).
fn world() -> World {
    let src = r#"
        format = 1
        size = [4000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]

        [[entities]]
        type = "crawler"
        at = [1000, 400]
        params = { speed = 1, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, appear_wait = 0, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }

        [[entities]]
        type = "post"
        at = [1100, 400]

        [[entities]]
        type = "post"
        at = [2100, 300]

        [[sections]]
        spots = [0, 1]
        meets = [2000, 50]

        [[sections]]
        spots = [{ entities = [2], rows = true }]
        "#;
    let level = Level::from_toml_str(src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One step with the view's left edge at `left` and its top at `top`
/// when it starts. Returns which things are in the level after it.
fn step_at(world: &mut World, left: i32, top: i32) -> [bool; 3] {
    world.view = Some(View {
        left: Fixed::from_int(left),
        top: Fixed::from_int(top),
        ..View::default()
    });
    step(world, &InputFrame::default());
    [0, 1, 2].map(|i| world.things[i].active)
}

#[test]
fn things_in_sections_appear_as_their_spots_come_up() {
    let mut w = world();
    assert_eq!(w.placing.as_ref().map(|p| p.opening.home), Some(0));
    assert_eq!([0, 1, 2].map(|i| w.things[i].active), [false; 3]);
    assert_eq!(step_at(&mut w, 711, 200), [false, false, false]);
    // The walker appears as it would by the view (walker-view.md).
    assert_eq!(step_at(&mut w, 712, 200), [true, false, false]);
    assert_eq!(w.things[0].pos.x, fx(999.875));
    assert_eq!(step_at(&mut w, 811, 200), [true, false, false]);
    assert_eq!(step_at(&mut w, 812, 200), [true, true, false]);
}

#[test]
fn a_thing_gone_by_its_own_rule_stays_gone() {
    let mut w = world();
    step_at(&mut w, 812, 200);
    // Collected, say.
    w.things[1].active = false;
    assert!(!step_at(&mut w, 812, 200)[1]);
}

#[test]
fn a_walker_the_view_removes_comes_back() {
    let mut w = world();
    step_at(&mut w, 712, 200);
    w.things[0].pos.x = fx(880.5);
    w.things[0].vx = fx(-1.0);
    assert!(!step_at(&mut w, 1032, 200)[0]);
    assert!(step_at(&mut w, 1031, 200)[0]);
    assert_eq!(w.things[0].pos.x, fx(999.875));
}

#[test]
fn the_far_section_opens_while_the_point_is_near() {
    let mut w = world();
    // The point is in the window, the post below the view's rows.
    assert!(!step_at(&mut w, 1850, 0)[2]);
    assert_eq!(w.placing.as_ref().unwrap().opening.crossing, Some(0));
    assert!(step_at(&mut w, 1850, 50)[2]);
    // The view goes down until the point is past its top edge: the
    // first section stays home.
    step_at(&mut w, 1850, 82);
    let opening = w.placing.as_ref().unwrap().opening;
    assert_eq!((opening.home, opening.crossing), (0, None));
    // Right past the point: the second section becomes home.
    step_at(&mut w, 1850, 50);
    step_at(&mut w, 2100, 50);
    let opening = w.placing.as_ref().unwrap().opening;
    assert_eq!((opening.home, opening.crossing), (1, None));
}

#[test]
fn a_wider_view_would_bring_in_spots_only_as_their_sections_place_them() {
    let mut w = world();
    step_at(&mut w, 300, 200);
    let would = |w: &World, width| -> Vec<usize> {
        let before = w.state_bytes();
        let things = w.would_appear(300, width);
        assert_eq!(w.state_bytes(), before, "nothing changes");
        things.iter().map(|t| t.index).collect()
    };
    assert_eq!(would(&w, 600), [] as [usize; 0]);
    assert_eq!(would(&w, 700), [0]);
    // And the post, once the window reaches it too.
    assert_eq!(would(&w, 800), [0, 1]);
}
