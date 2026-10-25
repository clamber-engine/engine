//! An object carried into a bounce leaves the hands and flies on
//! (spec/behavior/thrown-objects.md, "Carried into a bounce").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Presets};
use clamber_sim::{Act, Event, Facing, Fixed, Hands, InputFrame, Phase, World, generic_feel, step};

fn fx(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// A player at x = 300 facing `facing`, carrying a crate, falling onto a
/// still large walker so that it bounces on the next step. The walker is
/// thing 0, the crate thing 1. `character` 1 is the second character.
fn falling(facing: Facing, preset: &str, character: usize) -> World {
    let src = r#"
        format = 1
        name = "bounce"
        size = [1000, 600]
        start = [300, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 1000, 200]

        [[entities]]
        type = "crawler"
        at = [300, 400]
        params = { speed = 0, facing = "left", size = [26, 38], offset = 0.5, response = 0.125, turn_delay = 4, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }

        [[entities]]
        type = "crate"
        at = [900, 400]
        "#;
    let level = Level::from_toml_str(src, "bounce").unwrap();
    let mut feel = generic_feel(preset).unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot(preset, 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    let mut world = World::new(Arc::new(level), Arc::new(feel));
    world.player.character = character;
    world.player.pos.x = fx(300.0);
    world.player.facing = facing;
    world.player.hands = Some(Hands {
        thing: 1,
        act: Act::Carry,
        frame: 30,
        press: None,
    });
    world.things[1].phase = Phase::Held;
    world.player.ground = None;
    // After this frame's gravity it falls 1/16 px, to 10 above the floor.
    world.player.pos.y = Fixed::from_int(390) - fx(0.0625);
    world.player.vel.y = fx(0.0625 - 0.4375);
    world
}

fn tick(world: &mut World) {
    let held: Actions = Action::Action.into();
    step(world, &InputFrame::single(Frame::after(held, held)));
}

fn flies(world: &World) -> bool {
    matches!(world.things[1].phase, Phase::Thrown(_))
}

#[test]
fn a_carried_object_leaves_the_hands_on_a_bounce_and_flies_on() {
    for (facing, sign) in [(Facing::Right, 1.0), (Facing::Left, -1.0)] {
        let mut w = falling(facing, "classic-1", 0);
        let held = w.things[1].pos;
        tick(&mut w);
        assert!(w.events.contains(&Event::Bounce));
        assert!(w.player.hands.is_none());
        assert!(flies(&w));
        let c = w.things[1].pos;
        assert_eq!(c.x, Fixed::from_int(300), "on the player's x");
        assert_ne!(c, held);
        // Still on the two frozen frames.
        for _ in 0..2 {
            tick(&mut w);
            assert_eq!(w.things[1].pos, c);
        }
        // The table of thrown-objects.md: C + 3 to C + 6.
        for (n, (dx, dy)) in [
            (5.0, 1.4375),
            (10.0, 1.87890625),
            (15.0, 2.7578125),
            (20.0, 4.07421875),
        ]
        .into_iter()
        .enumerate()
        {
            tick(&mut w);
            let at = w.things[1].pos;
            assert_eq!(at.x, c.x + fx(sign * dx), "x on C + {}", n + 3);
            assert_eq!(at.y, c.y + fx(dy), "y on C + {}", n + 3);
        }
    }
}

#[test]
fn the_second_character_loses_the_barrel_the_same_way() {
    let mut w = falling(Facing::Right, "classic-1", 1);
    tick(&mut w);
    assert!(w.events.contains(&Event::Bounce) || w.player.thrown_back);
    assert!(w.player.hands.is_none());
    assert!(flies(&w));
}
