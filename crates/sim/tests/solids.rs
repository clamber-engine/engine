//! Nothing the engine moves passes through solid collision, at any speed
//! it can reach: launches into walls, thin blocks, fast walkers, and the
//! first level's launchers.

use std::path::Path;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{
    Fixed, FlowEvent, HALF_WIDTH, HEIGHT, InputFrame, Player, World, generic_feel, step,
};

fn int(n: i32) -> Fixed {
    Fixed::from_int(n)
}

fn world_from(level: Level) -> World {
    World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
}

/// Ground at y = 400 under a ceiling at y = 16, with a wall from x = 400
/// to 408 between them, too thin to show up between two probes, and the
/// player at `start`. `entities` is appended.
fn walled(start: [i32; 2], entities: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "walled"
        size = [1000, 600]
        start = [{}, {}]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 1000, 200]

        [[collision]]
        shape = "rect"
        rect = [0, 0, 1000, 16]

        [[collision]]
        shape = "rect"
        rect = [400, 16, 8, 384]

        {entities}
        "#,
        start[0], start[1]
    );
    world_from(Level::from_toml_str(&src, "test").unwrap())
}

const WALL: i32 = 400;

/// Steps once holding `held` after `previous`, and returns the flow
/// events of the step.
fn step_holding(world: &mut World, previous: Actions, held: Actions) -> Vec<FlowEvent> {
    step(world, &InputFrame::single(Frame::after(previous, held)));
    world.flow.events.clone()
}

fn run_right() -> Actions {
    Actions::from(Action::Right) | Action::Action.into()
}

#[test]
fn launches_into_a_wall_stop_at_it_at_any_speed() {
    // The launcher archetype's own velocity, then faster and faster: up
    // to many times any speed the player reaches by moving, and the
    // fastest falls.
    let velocities = [
        "[0, -11]",
        "[11, -11]",
        "[40, -11]",
        "[64, 0]",
        "[500, -3]",
        "[300, 12]",
        "[90, -90]",
    ];
    for velocity in velocities {
        // Holding right from different frames, so the launch meets the
        // wall at every height and with every bit of air control.
        for delay in 0..40 {
            let entities = format!(
                r#"
                [[entities]]
                type = "launcher"
                at = [300, 392]
                params = {{ velocity = {velocity} }}
                "#
            );
            let mut w = walled([280, 400], &entities);
            let mut previous = Actions::NONE;
            let mut launched = false;
            for frame in 0..240 {
                let held = if frame >= delay {
                    run_right()
                } else {
                    Action::Right.into()
                };
                let flow = step_holding(&mut w, previous, held);
                previous = held;
                launched |= flow.contains(&FlowEvent::Launch(0));
                let p = &w.player;
                assert!(
                    p.pos.x + HALF_WIDTH <= int(WALL),
                    "velocity {velocity}, delay {delay}, frame {frame}: \
                     body at x = {} reaches into the wall",
                    p.pos.x
                );
                assert!(
                    !flow.contains(&FlowEvent::Die),
                    "velocity {velocity}, delay {delay}: fell out of the level"
                );
            }
            assert!(
                launched,
                "velocity {velocity}, delay {delay}: never launched"
            );
        }
    }
}

#[test]
fn launches_up_past_a_ledge_land_on_it_or_stop_at_its_side() {
    // A ledge at x = 400 at every height from 60 to 320 px above the
    // ground, so some launches pass its corner with the feet in the top
    // few pixels of its side, where a step up would be.
    for height in 60..=320 {
        for delay in 0..30 {
            let src = format!(
                r#"
                format = 1
                size = [3000, 600]
                start = [280, 400]

                [[collision]]
                shape = "rect"
                rect = [0, 400, 3000, 200]

                [[collision]]
                shape = "rect"
                rect = [400, {}, 2600, {height}]

                [[entities]]
                type = "launcher"
                at = [300, 392]
                "#,
                400 - height
            );
            let level = Level::from_toml_str(&src, "test").unwrap();
            let mut w = world_from(level.clone());
            let mut previous = Actions::NONE;
            for frame in 0..240 {
                let held = if frame >= delay {
                    run_right()
                } else {
                    Action::Right.into()
                };
                let flow = step_holding(&mut w, previous, held);
                previous = held;
                assert!(
                    !inside(&level, &w.player),
                    "ledge {height} px high, delay {delay}: inside it at ({}, {}) on frame {frame}",
                    w.player.pos.x,
                    w.player.pos.y
                );
                assert!(!flow.contains(&FlowEvent::Die), "fell out of the level");
            }
        }
    }
}

#[test]
fn a_thin_block_at_body_height_stops_the_player() {
    // 16 px tall, its bottom 12 px above the ground: above step height
    // and below the head, so it's in the way.
    let mut w = world_from(
        Level::from_toml_str(
            r#"
            format = 1
            size = [1000, 600]
            start = [100, 400]

            [[collision]]
            shape = "rect"
            rect = [0, 400, 1000, 200]

            [[collision]]
            shape = "rect"
            rect = [400, 372, 8, 16]
            "#,
            "test",
        )
        .unwrap(),
    );
    let mut previous = Actions::NONE;
    for _ in 0..300 {
        step_holding(&mut w, previous, run_right());
        previous = run_right();
        assert!(w.player.pos.x + HALF_WIDTH <= int(WALL));
    }
    assert_eq!(w.player.pos.x + HALF_WIDTH, int(WALL));
    assert_eq!(w.player.vel.x, Fixed::ZERO);
}

#[test]
fn fast_walkers_turn_at_thin_walls_and_low_blocks() {
    // Walls 8 px thick at x = 100 and x = 400, and a block 8 px tall
    // from 10 to 18 px above the ground at x = 250, in the top of an
    // 18 px tall walker.
    let level = |speed: &str| {
        let src = format!(
            r#"
            format = 1
            size = [1000, 600]
            start = [50, 400]

            [[collision]]
            shape = "rect"
            rect = [0, 400, 1000, 200]

            [[collision]]
            shape = "rect"
            rect = [100, 300, 8, 100]

            [[collision]]
            shape = "rect"
            rect = [400, 300, 8, 100]

            [[collision]]
            shape = "rect"
            rect = [250, 382, 8, 8]

            [[entities]]
            type = "crawler"
            at = [200, 400]
            params = {{ speed = {speed}, facing = "right" }}
            "#
        );
        world_from(Level::from_toml_str(&src, "test").unwrap())
    };
    for speed in ["0.5", "7", "40", "250"] {
        let mut w = level(speed);
        for _ in 0..600 {
            step_holding(&mut w, Actions::NONE, Actions::NONE);
            let x = w.things[0].pos.x;
            // 20 px wide, so 10 px either side of its position
            assert!(
                x - int(10) >= int(108) && x + int(10) <= int(250),
                "speed {speed}: walker at x = {x} is past a wall or the block"
            );
        }
    }
}

/// Whether part of the player's body is inside a solid.
fn inside(level: &Level, player: &Player) -> bool {
    let (x, y) = (player.pos.x, player.pos.y);
    let half_height = Fixed::from_bits(HEIGHT.to_bits() / 2);
    [
        Vec2::new(x, y - Fixed::ONE),
        Vec2::new(x, y - half_height),
        Vec2::new(x, y - HEIGHT + Fixed::ONE),
    ]
    .into_iter()
    .any(|p| level.collision.solid_at(p).is_some())
}

#[test]
fn the_first_levels_launchers_never_put_the_player_inside_the_ground() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let level = Arc::new(Level::load(&root.join("content/base/levels/first.level")).unwrap());
    let launchers: Vec<Vec2> = level
        .entities
        .iter()
        .filter(|e| e.kind == "launcher")
        .map(|e| e.at)
        .collect();
    assert_eq!(launchers.len(), 2);
    let feel = Arc::new(generic_feel("default").unwrap());
    for at in launchers {
        for (dir, side) in [(Action::Right, -1), (Action::Left, 1)] {
            // Walk onto the launcher from either side, then keep going
            // that way, running, from different frames on.
            for delay in 0..60 {
                let mut w = World::new(level.clone(), feel.clone());
                let start = Vec2::new(at.x + int(24 * side), at.y);
                w.player = Player::new(&level, start);
                let mut previous = Actions::NONE;
                for frame in 0..240 {
                    let held = if frame < delay {
                        Actions::from(if side < 0 {
                            Action::Right
                        } else {
                            Action::Left
                        })
                    } else {
                        Actions::from(dir) | Action::Action.into()
                    };
                    let flow = step_holding(&mut w, previous, held);
                    previous = held;
                    assert!(
                        !inside(&level, &w.player),
                        "launcher at ({}, {}), {dir:?} from frame {delay}: \
                         inside the ground at ({}, {}) on frame {frame}",
                        at.x,
                        at.y,
                        w.player.pos.x,
                        w.player.pos.y
                    );
                    // Enemies wait beyond some launchers; after losing a
                    // life the run is over.
                    if flow.contains(&FlowEvent::Die) {
                        break;
                    }
                }
            }
        }
    }
}
