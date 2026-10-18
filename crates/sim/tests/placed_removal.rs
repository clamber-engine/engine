//! Placed objects being removed with the view and coming back, on
//! `classic-1` (spec/formats/placements.md, "When placed objects are
//! removed"). The view is put where each step starts.

use std::sync::Arc;

use clamber_level::Level;
use clamber_sim::{Facing, Fixed, InputFrame, Phase, View, World, generic_feel, step};

/// Flat ground at y = 400 and the player standing at x = 300. One section
/// holds, at x = 1000, a bunch at level y 300 (A, entity 0), a bunch at
/// level y 150 (B, entity 1) and a letter at level y 300 (entity 2), then
/// `extra` entities from entity 3 on, in spots of their own.
fn world(extra: &[(&str, i32, i32)], rows: bool) -> World {
    let mut src = String::from(
        r#"
        format = 1
        size = [4000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]

        [[entities]]
        type = "cluster"
        at = [1000, 300]

        [[entities]]
        type = "cluster"
        at = [1000, 150]

        [[entities]]
        type = "letter"
        at = [1000, 300]
        "#,
    );
    for (kind, x, y) in extra {
        src += &format!("\n[[entities]]\ntype = \"{kind}\"\nat = [{x}, {y}]\n");
    }
    let mut spots: Vec<String> = (0..3).map(|i| i.to_string()).collect();
    for i in 3..3 + extra.len() {
        spots.push(format!("{{ entities = [{i}], rows = {rows} }}"));
    }
    src += &format!("\n[[sections]]\nspots = [{}]\n", spots.join(", "));
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One step with the view's left edge at `left` and its top at `top`
/// when it starts. Returns which of the first `n` things are in the
/// level after it.
fn step_at<const N: usize>(world: &mut World, left: i32, top: i32) -> [bool; N] {
    world.view = Some(View {
        left: Fixed::from_int(left),
        top: Fixed::from_int(top),
        ..View::default()
    });
    step(world, &InputFrame::default());
    std::array::from_fn(|i| world.things[i].active)
}

/// The spec's synthetic example, with `U` = 200 throughout.
#[test]
fn the_example() {
    let mut w = world(&[], false);
    assert_eq!(step_at(&mut w, 712, 200), [true; 3]);
    // B is above the vertical window, and the letter goes at once.
    assert_eq!(step_at(&mut w, 711, 200), [true, false, false]);
    assert_eq!(step_at(&mut w, 616, 200), [true, false, false]);
    assert_eq!(step_at(&mut w, 615, 200), [false; 3]);
    assert_eq!(step_at(&mut w, 700, 200), [false; 3]);
    assert_eq!(step_at(&mut w, 712, 200), [true; 3], "all come back");
    // A in the margin, as the view's top moves up; B is lower in the
    // picture and stays.
    assert_eq!(step_at(&mut w, 650, 44), [true, true, false]);
    assert_eq!(step_at(&mut w, 650, 43), [false, true, false]);
}

#[test]
fn the_margin_on_the_left() {
    let mut w = world(&[], false);
    assert_eq!(step_at(&mut w, 1031, 200), [true; 3]);
    assert_eq!(step_at(&mut w, 1032, 200), [true, false, false]);
    assert_eq!(step_at(&mut w, 1127, 200), [true, false, false]);
    assert_eq!(step_at(&mut w, 1128, 200), [false; 3]);
    assert_eq!(step_at(&mut w, 1031, 200), [true; 3], "any number of times");
}

#[test]
fn a_taken_bunch_doesnt_come_back() {
    let mut w = world(&[], false);
    step_at::<3>(&mut w, 712, 200);
    // Taken, as collect.rs does with a bunch, which is kept.
    w.things[0].active = false;
    w.things[0].phase = Phase::Gone;
    step_at::<3>(&mut w, 615, 200);
    assert_eq!(step_at(&mut w, 712, 200), [false, true, true]);
}

#[test]
fn kegs_crates_hidden_objects_and_hutches() {
    let extra = [
        ("keg", 1000, 300),
        ("crate", 1000, 300),
        ("hidden", 1000, 300),
        ("hutch", 1000, 300),
    ];
    let mut w = world(&extra, false);
    assert_eq!(step_at::<7>(&mut w, 712, 200), [true; 7]);
    // The hutch goes at once, the others in the margin.
    assert_eq!(
        step_at::<7>(&mut w, 711, 200)[3..],
        [true, true, true, false]
    );
    assert_eq!(step_at::<7>(&mut w, 615, 200)[3..], [false; 4]);
    assert_eq!(step_at::<7>(&mut w, 712, 200)[3..], [true; 4]);
}

/// "Objects put down elsewhere", the spec's example: a crate whose start
/// is at x = 1000, put down at `at`, with the view's top at 200.
fn put_down(at: i32) -> World {
    let mut w = world(&[("crate", 1000, 300)], false);
    step_at::<4>(&mut w, 712, 200);
    w.things[3].pos.x = Fixed::from_int(at);
    w
}

#[test]
fn put_down_elsewhere_it_goes_by_its_start_and_its_own_x() {
    let mut w = put_down(800);
    // Its own x is in the margin, but its start is near the view.
    assert!(step_at::<4>(&mut w, 928, 200)[3]);
    assert!(step_at::<4>(&mut w, 1031, 200)[3]);
    assert!(!step_at::<4>(&mut w, 1032, 200)[3]);
    // It comes back at its start, not where it was put down.
    assert!(step_at::<4>(&mut w, 712, 200)[3]);
    assert_eq!(w.things[3].pos.x, Fixed::from_int(1000));

    // With the view moving left instead: its start leaves the window
    // first, and it stays until its own x is 385 right of the view.
    let mut w = put_down(800);
    assert!(step_at::<4>(&mut w, 711, 200)[3]);
    assert!(step_at::<4>(&mut w, 416, 200)[3]);
    assert!(!step_at::<4>(&mut w, 415, 200)[3]);

    // Put down at 1150: its own x reaches the margin first, and it stays
    // until its start leaves the window.
    let mut w = put_down(1150);
    assert!(step_at::<4>(&mut w, 765, 200)[3]);
    assert!(step_at::<4>(&mut w, 712, 200)[3]);
    assert!(!step_at::<4>(&mut w, 711, 200)[3]);
}

#[test]
fn while_its_there_or_carried_its_start_places_nothing_else() {
    let mut w = put_down(1150);
    // Its start leaves the window and comes back: it's the same crate,
    // where it was put down.
    assert!(step_at::<4>(&mut w, 1100, 200)[3]);
    assert!(step_at::<4>(&mut w, 900, 200)[3]);
    assert_eq!(w.things[3].pos.x, Fixed::from_int(1150));

    // Carried, it stays however far from its start, wherever that is.
    let mut w = put_down(1000);
    w.things[3].phase = Phase::Held;
    w.things[3].pos.x = Fixed::from_int(3000);
    assert!(step_at::<4>(&mut w, 2800, 200)[3]);
    assert!(step_at::<4>(&mut w, 900, 200)[3]);
    assert_eq!(w.things[3].pos.x, Fixed::from_int(3000));
}

/// "In lists with sections", "Objects put down elsewhere", the spec's
/// example: a crate whose start, at x = 200 on a ledge at level y 1000,
/// needs the view's rows to appear (a class 15 entry), put down at
/// x = 600 on the floor at level y 1400.
fn put_down_below() -> World {
    let src = r#"
        format = 1
        size = [4000, 1600]
        start = [3000, 1400]

        [[collision]]
        shape = "rect"
        rect = [0, 1400, 4000, 200]

        [[collision]]
        shape = "rect"
        rect = [100, 1000, 200, 20]

        [[entities]]
        type = "crate"
        at = [200, 1000]

        [[sections]]
        spots = [{ entities = [0], rows = true }]
    "#;
    let level = Level::from_toml_str(src, "test").unwrap();
    let mut w = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    assert!(step_at::<1>(&mut w, 0, 900)[0]);
    w.things[0].pos.x = Fixed::from_int(600);
    w.things[0].pos.y = Fixed::from_int(1400);
    w
}

#[test]
fn put_down_below_it_comes_back_only_with_its_start_in_the_rows() {
    let mut w = put_down_below();
    assert!(step_at::<1>(&mut w, 300, 1250)[0]);
    assert!(step_at::<1>(&mut w, 727, 1250)[0]);
    assert!(!step_at::<1>(&mut w, 728, 1250)[0]);
    // Its start is in the window across, but not in the rows.
    assert!(!step_at::<1>(&mut w, 150, 1250)[0]);
    assert!(step_at::<1>(&mut w, 150, 1031)[0]);
    let at = w.things[0].pos;
    assert_eq!((at.x, at.y), (Fixed::from_int(200), Fixed::from_int(1000)));

    // Going up before it's removed, it stays down on the floor: its
    // start places nothing else.
    let mut w = put_down_below();
    assert!(step_at::<1>(&mut w, 150, 1250)[0]);
    assert!(step_at::<1>(&mut w, 150, 1031)[0]);
    assert_eq!(w.things[0].pos.x, Fixed::from_int(600));
}

/// "Exits": there only in the window and the vertical window, removed
/// as soon as either no longer holds, even when only `U` changes.
#[test]
fn exits_need_the_vertical_window_too() {
    let mut w = world(&[("exit", 1500, 300)], true);
    step_at::<4>(&mut w, 1100, 200);
    assert!(step_at::<4>(&mut w, 1212, 44)[3]);
    assert!(!step_at::<4>(&mut w, 1212, 43)[3], "only U changed");
    assert!(step_at::<4>(&mut w, 1212, 44)[3]);
    assert!(step_at::<4>(&mut w, 1212, 331)[3]);
    assert!(!step_at::<4>(&mut w, 1212, 332)[3]);
    assert!(step_at::<4>(&mut w, 1212, 300)[3]);
    assert!(!step_at::<4>(&mut w, 1211, 300)[3], "no margin");
    assert!(step_at::<4>(&mut w, 1531, 300)[3]);
    assert!(!step_at::<4>(&mut w, 1532, 300)[3]);
}

/// "Wall exits": a closed one follows the window alone, however far
/// above or below the picture it is; an open one also needs the vertical
/// window, and comes back closed once removed.
#[test]
fn wall_exits_need_the_vertical_window_only_while_open() {
    let mut w = world(&[("wall-exit", 1500, 300)], false);
    assert!(!step_at::<4>(&mut w, 1211, 200)[3]);
    assert!(step_at::<4>(&mut w, 1212, 600)[3], "y - U = -300");
    assert!(step_at::<4>(&mut w, 1212, 0)[3], "y - U = 300");
    assert!(!step_at::<4>(&mut w, 1532, 0)[3], "no margin");
    assert!(step_at::<4>(&mut w, 1531, 0)[3]);
    let open = |w: &mut World| w.things[3].phase = Phase::Open;
    // Open, it goes on the first frame outside the vertical window, even
    // when only U changes, and is back closed on the next frame.
    step_at::<4>(&mut w, 1212, 200);
    open(&mut w);
    assert!(step_at::<4>(&mut w, 1212, 44)[3]);
    assert_eq!(w.things[3].phase, Phase::Open);
    assert!(!step_at::<4>(&mut w, 1212, 43)[3]);
    assert!(step_at::<4>(&mut w, 1212, 43)[3]);
    assert_eq!(w.things[3].phase, Phase::Ready, "closed");
    assert!(step_at::<4>(&mut w, 1212, 0)[3]);
    // Open, at the top edge too.
    step_at::<4>(&mut w, 1212, 331);
    open(&mut w);
    assert!(step_at::<4>(&mut w, 1212, 331)[3]);
    assert!(!step_at::<4>(&mut w, 1212, 332)[3]);
    // Removed open by the window, it comes back closed as well.
    step_at::<4>(&mut w, 1212, 200);
    open(&mut w);
    assert!(!step_at::<4>(&mut w, 1211, 200)[3]);
    assert!(step_at::<4>(&mut w, 1212, 200)[3]);
    assert_eq!(w.things[3].phase, Phase::Ready, "closed");
}

/// An exit that isn't there takes over no one.
#[test]
fn an_exit_that_isnt_there_doesnt_take_the_lead() {
    let src = r#"
        format = 1
        size = [3000, 700]
        start = [310, 449]

        [[collision]]
        shape = "rect"
        rect = [0, 449, 3000, 100]

        [[entities]]
        type = "exit"
        at = [300, 449]

        [[sections]]
        spots = [{ entities = [0], rows = true }]
        "#;
    let level = Arc::new(Level::from_toml_str(src, "exit").unwrap());
    let mut w = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    w.player.facing = Facing::Left;
    // The exit is 300 px below the view's top: out of its rows.
    for _ in 0..10 {
        step_at::<1>(&mut w, 100, 149);
        assert!(w.leaving.is_none());
    }
    step_at::<1>(&mut w, 100, 300);
    assert!(w.leaving.is_some());
}
