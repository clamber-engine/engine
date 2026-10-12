//! The bundled content the game starts with.

use std::sync::Arc;

use clamber_audio::{Input, Sound};
use clamber_game::{
    Cast, Cue, DEFAULT_LEVEL, DEFAULT_SOUNDS, Game, LayerArt, SPRITES, SoundList, content_dir,
};
use clamber_level::Level;
use clamber_sim::{Archetypes, InputFrame, World, generic_feel};

#[test]
fn the_default_level_loads_and_plays() {
    let level = Level::load(&content_dir().join(DEFAULT_LEVEL)).unwrap();
    let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
    let cast = Cast::load(&content_dir().join(SPRITES)).unwrap();
    let mut game = Game::new(world).with_cast(cast);
    for _ in 0..120 {
        game.step(&InputFrame::default());
    }
    assert!(game.world.player.grounded());
    let mut canvas = Game::canvas();
    game.draw(&mut canvas, 0.5);
}

#[test]
fn the_default_level_has_its_art_and_music() {
    let path = content_dir().join(DEFAULT_LEVEL);
    let level = Level::load(&path).unwrap();
    let dir = path.parent().unwrap();
    LayerArt::load(&level, dir).unwrap();
    let track = level.music.as_ref().expect("the default level has music");
    Sound::load(&Input::file(dir.join(&track.file)), 48_000).unwrap();
}

#[test]
fn every_cue_has_a_sound_that_decodes() {
    let list = SoundList::load(&content_dir().join(DEFAULT_SOUNDS)).unwrap();
    for cue in Cue::ALL {
        let path = list.effects.get(&cue).unwrap_or_else(|| panic!("{cue:?}"));
        let sound = Sound::load(&Input::file(path), 48_000).unwrap();
        assert!(sound.frames() > 0, "{cue:?}");
    }
}

#[test]
fn every_archetype_has_a_sprite_sheet() {
    let cast = Cast::load(&content_dir().join(SPRITES)).unwrap();
    for name in Archetypes::builtin().names() {
        assert!(cast.has_kind(name), "{name}");
    }
}
