//! How many placed objects can be there at once, on `classic-1`: 12
//! places, and 11 pictures for those that take one
//! (spec/formats/placements.md, "How many objects can be there"). The view
//! is put where each step starts.

use std::sync::Arc;

use clamber_level::{Level, Vec2};
use clamber_sim::{Burst, Fixed, InputFrame, Phase, View, World, generic_feel, step};

/// Flat ground at y = 400 and the player standing at x = 300. One section
/// holds a bunch at level (x, 300) for each of `spots`, in that order,
/// taking a picture as it says.
fn world(spots: &[(i32, bool)]) -> World {
    let mut src = String::from(
        r#"
        format = 1
        size = [4000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]
        "#,
    );
    for (x, _) in spots {
        src += &format!("\n[[entities]]\ntype = \"cluster\"\nat = [{x}, 300]\n");
    }
    let spots: Vec<String> = spots
        .iter()
        .enumerate()
        .map(|(i, (_, picture))| format!("{{ entities = [{i}], picture = {picture} }}"))
        .collect();
    src += &format!("\n[[sections]]\nspots = [{}]\n", spots.join(", "));
    let level = Level::from_toml_str(&src, "test").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One step with the view's left edge at `left` and its top at 200 when
/// it starts. Returns which things are in the level after it.
fn step_at(world: &mut World, left: i32) -> Vec<bool> {
    world.view = Some(View {
        left: Fixed::from_int(left),
        top: Fixed::from_int(200),
        ..View::default()
    });
    step(world, &InputFrame::default());
    world.things.iter().map(|t| t.active).collect()
}

/// Taken, as collect.rs does with a bunch.
fn take(world: &mut World, i: usize) {
    world.things[i].active = false;
    world.things[i].phase = Phase::Gone;
}

/// The spec's synthetic example: frame F starts with 11 places and 10
/// pictures taken, and A (taking a picture), B (not) and C (taking one)
/// come up.
#[test]
fn the_example() {
    let mut spots = vec![(900, true); 10];
    spots.push((900, false));
    spots.extend([(1100, true), (1100, false), (1100, true)]);
    let mut w = world(&spots);
    assert_eq!(step_at(&mut w, 700)[11..], [false; 3]);
    assert!(step_at(&mut w, 700)[..11].iter().all(|&a| a));
    // F: A takes the last place and picture; B and C wait.
    assert_eq!(step_at(&mut w, 812)[11..], [true, false, false]);
    // Also on F, the player takes a bunch: it's free from F + 1.
    take(&mut w, 0);
    assert_eq!(step_at(&mut w, 812)[11..], [true, true, false]);
    // C needs a place too.
    assert_eq!(step_at(&mut w, 812)[11..], [true, true, false]);
    take(&mut w, 1);
    assert_eq!(step_at(&mut w, 812)[11..], [true, true, true]);
}

/// An entry with no picture free waits, and one stored after it that
/// needs none still appears.
#[test]
fn a_waiting_entry_holds_back_none_after_it() {
    let mut spots = vec![(900, true); 11];
    spots.extend([(1100, true), (1100, false)]);
    let mut w = world(&spots);
    step_at(&mut w, 700);
    assert_eq!(step_at(&mut w, 812)[11..], [false, true]);
    take(&mut w, 0);
    assert_eq!(step_at(&mut w, 812)[11..], [true, true]);
}

/// A burst keg's pieces and flash take a picture each, until the step
/// after they go.
#[test]
fn a_bursts_pieces_and_flash_take_pictures() {
    let mut spots = vec![(900, true); 6];
    spots.push((1100, true));
    let mut w = world(&spots);
    step_at(&mut w, 700);
    let at = Vec2::new(Fixed::from_int(900), Fixed::from_int(300));
    w.bursts.push(Burst {
        thing: 0,
        frame: 0,
        pieces: [at; 4],
        speeds: [Vec2::new(Fixed::ZERO, Fixed::ZERO); 4],
        flash: at,
        gravity: Fixed::ZERO,
        rise: Fixed::ZERO,
        time: 3,
        object: None,
    });
    assert!(!step_at(&mut w, 812)[6]);
    assert!(!step_at(&mut w, 812)[6]);
    // The burst goes on this step.
    assert!(!step_at(&mut w, 812)[6]);
    assert!(w.bursts.is_empty());
    assert!(step_at(&mut w, 812)[6]);
}
