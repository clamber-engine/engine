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

/// The first level is made by `tools/levels/first.py`; this checks it
/// hasn't been edited by hand or left behind by a change to the layout.
#[test]
fn the_first_level_is_up_to_date_with_its_layout() {
    let root = content_dir().join("..");
    let out = std::env::temp_dir().join(format!("first-{}.level", std::process::id()));
    let made = std::process::Command::new("python3")
        .arg(root.join("tools/levels/first.py"))
        .arg(&out)
        .status();
    match made {
        Ok(status) => assert!(status.success(), "tools/levels/first.py failed"),
        Err(e) => {
            eprintln!("skipped: can't run python3 ({e})");
            return;
        }
    }
    let made = std::fs::read_to_string(&out).unwrap();
    std::fs::remove_file(&out).unwrap();
    let committed = std::fs::read_to_string(root.join("content/base/levels/first.level")).unwrap();
    assert!(
        made == committed,
        "content/base/levels/first.level is out of date: run tools/levels/first.py"
    );
}
