//! The bundled content the game starts with.

use std::sync::Arc;

use clamber_audio::{Input, Sound};
use clamber_game::{
    Cast, Cue, DEFAULT_LEVEL, DEFAULT_SOUNDS, Game, LayerArt, SPRITES, SoundList, content_dir,
};
use clamber_level::{LayerContent, Level};
use clamber_sim::{Archetypes, InputFrame, World, generic_feel};
use clamber_sprite::{SpriteSheet, decode_png};

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

/// The climber is rendered by `tools/render-art.sh` in high definition,
/// with an animation for every state and other colours for the second
/// roster slot.
#[test]
fn the_player_sheet_covers_every_state() {
    let sprite = SpriteSheet::load(&content_dir().join(SPRITES).join("player.json")).unwrap();
    let sheet = &sprite.sheet;
    assert!(sheet.scale > 1 && !sheet.pixel_art);
    for state in [
        "idle", "walk", "run", "roll", "jump", "fall", "swap", "hurt",
    ] {
        assert!(sheet.animation(state).is_some(), "{state}");
    }
    assert!(sprite.variants.contains_key("alt"));
    assert!(sheet.frames.iter().all(|f| !f.boxes.is_empty()));
}

/// The rest of the cast is rendered the same way, with a normal map and
/// the animations the game plays for each.
#[test]
fn the_cast_sheets_are_rendered_with_their_animations() {
    for (name, animations) in [
        ("crawler", &["walk", "defeated"][..]),
        ("flutter", &["fly", "defeated"]),
        ("gem", &["spin"]),
        ("relic", &["glint"]),
        ("checkpoint", &["waiting", "reached"]),
        ("goal", &["glow"]),
        ("launcher", &["waiting", "fire"]),
        ("crate", &["whole", "break"]),
    ] {
        let path = content_dir().join(SPRITES).join(format!("{name}.json"));
        let sprite = SpriteSheet::load(&path).unwrap();
        let sheet = &sprite.sheet;
        assert!(sheet.scale > 1 && !sheet.pixel_art, "{name}");
        assert!(sprite.normals.is_some(), "{name}");
        let names: Vec<_> = sheet.animations.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, animations, "{name}");
    }
}

/// The first level's terrain and backgrounds are rendered by
/// `tools/render-art.sh` at the sprites' scale: the tiles with a normal
/// map, and each background as wide as the view, so it tiles across it.
#[test]
fn the_first_level_is_drawn_with_rendered_art() {
    let path = content_dir().join(DEFAULT_LEVEL);
    let level = Level::load(&path).unwrap();
    let dir = path.parent().unwrap();
    let player = SpriteSheet::load(&content_dir().join(SPRITES).join("player.json")).unwrap();
    for layer in &level.layers {
        assert!(!layer.pixel_art, "{}", layer.name);
        assert_eq!(layer.scale, player.sheet.scale, "{}", layer.name);
        match &layer.content {
            LayerContent::Tiles(tiles) => {
                assert!(tiles.tileset.normals.is_some(), "{}", layer.name);
                assert_eq!(tiles.tileset.tile_width, 16 * layer.scale);
            }
            LayerContent::Image { path, .. } => {
                let bytes = std::fs::read(dir.join(path)).unwrap();
                let image = decode_png(&bytes).unwrap();
                assert_eq!(image.width(), 384 * layer.scale, "{}", layer.name);
            }
        }
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
