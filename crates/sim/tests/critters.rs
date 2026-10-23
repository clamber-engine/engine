//! Critters on `classic-1` (spec/behavior/critter.md): scenery that
//! appears across the top of the picture, flies towards its entry and is
//! removed once neither it nor its entry is in the window.

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_level::Level;
use clamber_pack::{CharacterFeel, Presets};
use clamber_sim::{Fixed, InputFrame, Phase, View, World, generic_feel, step};

/// Flat ground at y = 400, the player standing at x = 300, and a critter
/// with its entry at (`x`, 300).
fn world(x: i32) -> World {
    let src = format!(
        r#"
        format = 1
        size = [4000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4000, 200]

        [[entities]]
        type = "critter"
        at = [{x}, 300]
        "#
    );
    let level = Level::from_toml_str(&src, "critters").unwrap();
    World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    )
}

/// One step with the view's left edge at `left` and its top at 200 when
/// it starts. Returns the critter's whole-px position if it's there after
/// the step.
fn step_at(world: &mut World, left: i32) -> Option<(i64, i64)> {
    world.view = Some(View {
        left: Fixed::from_int(left),
        top: Fixed::from_int(200),
        ..View::default()
    });
    step(world, &InputFrame::default());
    let critter = &world.things[0];
    critter
        .active
        .then(|| (critter.pos.x.floor(), critter.pos.y.floor()))
}

#[test]
fn appears_across_the_top_of_the_picture_when_its_entry_is_in_the_window() {
    let x = 2000;
    let mut w = world(x);
    assert!(!w.things[0].active, "out of the window on the first frame");
    assert_eq!(step_at(&mut w, x - 289), None);
    let (cx, cy) = step_at(&mut w, x - 288).expect("appears");
    assert!((x - 290..=x - 288 + 257).contains(&(cx as i32)), "{cx}");
    assert!((200 - 8..=200 + 2).contains(&cy), "{cy}");
}

#[test]
fn flies_down_to_its_entry_and_perches_there() {
    let x = 2000;
    let mut w = world(x);
    // Not on the world's first frame, when it would stand a while.
    assert_eq!(step_at(&mut w, 0), None);
    let mut at = None;
    for _ in 0..1200 {
        at = step_at(&mut w, x - 150);
        if w.things[0].clock > 0 {
            break;
        }
    }
    let (cx, cy) = at.expect("there");
    assert!((x as i64 - 4..=x as i64 + 3).contains(&cx), "{cx}");
    assert!((300 - 4..=300 + 4).contains(&cy), "{cy}");
    let Phase::Flitting { target, .. } = w.things[0].phase else {
        panic!("{:?}", w.things[0].phase);
    };
    assert_eq!(target, (x - 64, -16), "the next target");
}

#[test]
fn is_removed_only_once_neither_it_nor_its_entry_is_in_the_window() {
    let x = 2000;
    let mut w = world(x);
    step_at(&mut w, x - 100);
    // Its entry leaves the window, but it is still in it.
    w.things[0].pos.x = Fixed::from_int(x + 400);
    assert!(step_at(&mut w, x + 200).is_some(), "itself in the window");
    // Both out: removed.
    w.things[0].pos.x = Fixed::from_int(x + 400 + 289);
    w.things[0].vx = Fixed::from_int(1);
    assert_eq!(step_at(&mut w, x + 400), None);
    // It appears afresh when its entry is back in the window.
    let (cx, _) = step_at(&mut w, x - 100).expect("back");
    assert!((x - 102..=x - 100 + 257).contains(&(cx as i32)), "{cx}");
}

#[test]
fn isnt_removed_while_its_entry_is_in_the_window_even_far_off() {
    let x = 2000;
    let mut w = world(x);
    step_at(&mut w, x - 100);
    w.things[0].pos.x = Fixed::from_int(x + 1000);
    assert!(
        step_at(&mut w, x - 100).is_some(),
        "its entry is in the window"
    );
}

#[test]
fn a_critter_there_at_the_start_stands_at_its_entrys_height_for_41_frames() {
    let mut w = world(300);
    assert!(w.things[0].active);
    let mut first = None;
    for frame in 0..41 {
        let at = step_at(&mut w, 0).expect("there");
        assert_eq!(at.1, 300, "frame {frame}");
        assert_eq!(*first.get_or_insert(at), at, "frame {frame}");
    }
    assert_ne!(step_at(&mut w, 0), first, "flies from frame 41");
}

/// critter.md, "The second character leading": a critter appears, flies
/// and is removed as with the first, and changes nothing.
#[test]
fn flies_the_same_with_the_second_character_leading() {
    let x = 2000;
    let mut first = world(x);
    let mut second = world(x);
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    second.feel = Arc::new(feel);
    second.player.character = 1;
    for left in (0..x + 600).step_by(4) {
        assert_eq!(step_at(&mut first, left), step_at(&mut second, left));
        assert_eq!(first.things[0].phase, second.things[0].phase);
    }
}
