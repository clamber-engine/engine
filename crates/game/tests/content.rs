//! The bundled content the game starts with.

use std::sync::Arc;

use clamber_game::{DEFAULT_LEVEL, Game, content_dir};
use clamber_level::Level;
use clamber_sim::{InputFrame, World, generic_feel};

#[test]
fn the_default_level_loads_and_plays() {
    let level = Level::load(&content_dir().join(DEFAULT_LEVEL)).unwrap();
    let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
    let mut game = Game::new(world);
    for _ in 0..120 {
        game.step(&InputFrame::default());
    }
    assert!(game.world.player.grounded());
    let mut canvas = Game::canvas();
    game.draw(&mut canvas, 0.5);
}
