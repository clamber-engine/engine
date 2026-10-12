//! What the game sounds like: [`Cue`]s for what happens in play, a
//! [`SoundList`] naming a file for each, and [`Audio`], which plays them
//! and the level's music through `clamber-audio`.
//!
//! Sound is never needed to play: a missing or unreadable file is a
//! warning, and the cue it was for stays silent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clamber_audio::{AudioHandle, Input, Looping, Music, Play, Sound};
use clamber_level::Track;
use clamber_sim::{Event, FlowEvent, World};
use serde::Deserialize;

use crate::session::Session;

/// The sound list played when none is given, relative to the content
/// folder.
pub const DEFAULT_SOUNDS: &str = "base/sounds.toml";

/// Something that happens with a sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cue {
    Jump,
    Land,
    Roll,
    /// Something was picked up.
    Collect,
    Checkpoint,
    /// A life was lost.
    Hurt,
    Complete,
    GameOver,
    Pause,
}

impl Cue {
    pub const ALL: [Cue; 9] = [
        Cue::Jump,
        Cue::Land,
        Cue::Roll,
        Cue::Collect,
        Cue::Checkpoint,
        Cue::Hurt,
        Cue::Complete,
        Cue::GameOver,
        Cue::Pause,
    ];

    /// The cue's name in a sound list.
    pub fn name(self) -> &'static str {
        match self {
            Cue::Jump => "jump",
            Cue::Land => "land",
            Cue::Roll => "roll",
            Cue::Collect => "collect",
            Cue::Checkpoint => "checkpoint",
            Cue::Hurt => "hurt",
            Cue::Complete => "complete",
            Cue::GameOver => "game_over",
            Cue::Pause => "pause",
        }
    }

    pub fn from_name(name: &str) -> Option<Cue> {
        Cue::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// Cues for what happened in the world's last step, in order.
pub fn cues(world: &World) -> impl Iterator<Item = Cue> + '_ {
    let moves = world.events.iter().filter_map(|e| match e {
        Event::JumpStart => Some(Cue::Jump),
        Event::Land => Some(Cue::Land),
        Event::RollStart => Some(Cue::Roll),
        _ => None,
    });
    let flow = world.flow.events.iter().filter_map(|e| match e {
        FlowEvent::Collect(_) => Some(Cue::Collect),
        FlowEvent::Checkpoint(_) => Some(Cue::Checkpoint),
        FlowEvent::Die => Some(Cue::Hurt),
        FlowEvent::Complete => Some(Cue::Complete),
        FlowEvent::GameOver => Some(Cue::GameOver),
        FlowEvent::Respawn => None,
    });
    moves.chain(flow)
}

/// The sound file for each cue. A TOML file:
///
/// ```toml
/// [effects]
/// jump = "sounds/jump.ogg"
/// ```
///
/// Paths are relative to the list. Cues without a file are silent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundList {
    pub effects: BTreeMap<Cue, PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SoundListFile {
    #[serde(default)]
    effects: BTreeMap<String, String>,
}

impl SoundList {
    pub fn load(path: &Path) -> Result<SoundList, String> {
        let origin = path.display().to_string();
        let src = std::fs::read_to_string(path).map_err(|e| format!("{origin}: {e}"))?;
        SoundList::from_toml_str(&src, path.parent().unwrap_or(Path::new("")), &origin)
    }

    /// Reads a sound list whose paths are relative to `dir`. `origin`
    /// names the source in error messages.
    pub fn from_toml_str(src: &str, dir: &Path, origin: &str) -> Result<SoundList, String> {
        let file: SoundListFile =
            toml::from_str(src).map_err(|e| format!("{origin}: {}", e.to_string().trim_end()))?;
        let mut effects = BTreeMap::new();
        for (name, path) in file.effects {
            let cue = Cue::from_name(&name).ok_or_else(|| {
                let known: Vec<_> = Cue::ALL.iter().map(|c| c.name()).collect();
                format!(
                    "{origin}: effects.{name}: unknown cue; known cues are {}",
                    known.join(", ")
                )
            })?;
            effects.insert(cue, dir.join(path));
        }
        Ok(SoundList { effects })
    }
}

/// Crossfade into a level's music, in seconds.
const MUSIC_IN: f32 = 0.05;
/// Fade out of it when play stops, in seconds.
const MUSIC_OUT: f32 = 0.75;

/// Plays a session's cues and its level's music on a mixer.
pub struct Audio {
    handle: AudioHandle,
    rate: u32,
    effects: BTreeMap<Cue, Sound>,
    music: Option<LevelMusic>,
}

struct LevelMusic {
    path: PathBuf,
    looping: Option<Looping>,
    /// It couldn't be opened, which was reported once.
    failed: bool,
}

impl Audio {
    /// Plays through `handle`, loading sounds at `rate` (the output's).
    /// Effects that can't be loaded are passed to `warn` and stay silent.
    pub fn new(
        handle: AudioHandle,
        rate: u32,
        sounds: &SoundList,
        mut warn: impl FnMut(String),
    ) -> Audio {
        let mut effects = BTreeMap::new();
        for (&cue, path) in &sounds.effects {
            match Sound::load(&Input::file(path), rate) {
                Ok(sound) => {
                    effects.insert(cue, sound);
                }
                Err(e) => warn(format!(
                    "{}: {e}; `{}` is silent",
                    path.display(),
                    cue.name()
                )),
            }
        }
        Audio {
            handle,
            rate,
            effects,
            music: None,
        }
    }

    /// Sets the level's music, whose file is relative to `dir`.
    pub fn set_music(&mut self, track: Option<&Track>, dir: &Path) {
        self.music = track.map(|t| LevelMusic {
            path: dir.join(&t.file),
            looping: t.loop_start.map(|start| Looping::From {
                start,
                length: t.loop_length,
            }),
            failed: false,
        });
    }

    /// Cues with a sound.
    pub fn effects(&self) -> impl Iterator<Item = Cue> + '_ {
        self.effects.keys().copied()
    }

    /// Plays the cues of `session`'s last step, and starts or stops the
    /// music as play starts or stops. Music that can't be opened is
    /// passed to `warn`, once.
    pub fn update(&mut self, session: &Session, mut warn: impl FnMut(String)) {
        for cue in &session.cues {
            if let Some(sound) = self.effects.get(cue) {
                self.handle.play(sound, Play::default());
            }
        }

        let playing = self.handle.music().is_some();
        if session.music_on() == playing {
            return;
        }
        if playing {
            self.handle.stop_music(self.frames(MUSIC_OUT));
            return;
        }
        let (rate, fade) = (self.rate, self.frames(MUSIC_IN));
        let Some(music) = self.music.as_mut().filter(|m| !m.failed) else {
            return;
        };
        let id = music.path.display().to_string();
        let result = self.handle.play_music(&id, fade, || {
            Music::open(Input::file(&music.path), music.looping, rate)
        });
        if let Err(e) = result {
            music.failed = true;
            warn(format!("{id}: {e}; playing without music"));
        }
    }

    fn frames(&self, seconds: f32) -> u32 {
        (self.rate as f32 * seconds) as u32
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_audio::{Mixer, mixer};
    use clamber_input::{Action, Actions, Frame};
    use clamber_level::Level;
    use clamber_sim::{InputFrame, generic_feel};

    use super::*;
    use crate::game::Game;
    use crate::session::Screen;

    const RATE: u32 = 8000;

    const LEVEL: &str = r#"
        format = 1
        size = [1000, 400]
        start = [500, 300]

        [[collision]]
        shape = "rect"
        rect = [0, 300, 1000, 100]
    "#;

    /// A mono 16-bit WAV of `frames` frames at full scale.
    fn wav(frames: usize) -> Vec<u8> {
        let data_len = (frames * 2) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for _ in 0..frames {
            out.extend_from_slice(&i16::MAX.to_le_bytes());
        }
        out
    }

    /// A folder of its own for one test's files.
    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clamber-sound-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn session() -> Session {
        let level = Level::from_toml_str(LEVEL, "test").unwrap();
        Session::new(Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("default").unwrap()),
        )))
    }

    fn press(action: Action) -> InputFrame {
        InputFrame::single(Frame::after(Actions::NONE, action.into()))
    }

    /// Mixes `frames` frames and says whether any were loud.
    fn loud(mixer: &mut Mixer, frames: usize) -> bool {
        let mut out = vec![0.0; frames * 2];
        mixer.render(&mut out);
        out.iter().any(|s| s.abs() > 0.5)
    }

    #[test]
    fn reads_a_sound_list() {
        let list = SoundList::from_toml_str(
            "[effects]\njump = \"sounds/jump.ogg\"\ngame_over = \"over.wav\"\n",
            Path::new("pack"),
            "sounds.toml",
        )
        .unwrap();
        assert_eq!(list.effects.len(), 2);
        assert_eq!(
            list.effects[&Cue::Jump],
            Path::new("pack").join("sounds/jump.ogg")
        );
        assert_eq!(list.effects[&Cue::GameOver], Path::new("pack/over.wav"));

        let e = SoundList::from_toml_str("[effects]\nleap = \"a.wav\"\n", Path::new(""), "s")
            .unwrap_err();
        assert!(e.starts_with("s: effects.leap: unknown cue; known cues are jump, land"));
        assert!(SoundList::from_toml_str("[music]\n", Path::new(""), "s").is_err());
    }

    #[test]
    fn the_bundled_sound_list_reads() {
        let path = crate::content_dir().join(DEFAULT_SOUNDS);
        SoundList::load(&path).unwrap();
    }

    #[test]
    fn cue_names_round_trip() {
        for cue in Cue::ALL {
            assert_eq!(Cue::from_name(cue.name()), Some(cue));
        }
    }

    #[test]
    fn plays_cues_and_warns_about_missing_files() {
        let dir = folder("cues");
        std::fs::write(dir.join("jump.wav"), wav(400)).unwrap();
        let list = SoundList::from_toml_str(
            "[effects]\njump = \"jump.wav\"\nland = \"missing.wav\"\n",
            &dir,
            "sounds.toml",
        )
        .unwrap();
        let (handle, mut mixer) = mixer(8);
        let mut warnings = Vec::new();
        let mut audio = Audio::new(handle, RATE, &list, |w| warnings.push(w));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("missing.wav"), "{warnings:?}");
        assert!(warnings[0].ends_with("`land` is silent"), "{warnings:?}");
        assert_eq!(audio.effects().collect::<Vec<_>>(), [Cue::Jump]);

        let mut s = session();
        s.step(&press(Action::Jump));
        assert_eq!(s.screen, Screen::Playing);
        s.step(&InputFrame::default());
        audio.update(&s, |w| panic!("{w}"));
        assert!(!loud(&mut mixer, 100), "nothing happened");

        s.step(&press(Action::Jump));
        assert_eq!(s.cues, [Cue::Jump]);
        audio.update(&s, |w| panic!("{w}"));
        assert!(loud(&mut mixer, 100), "the jump");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn music_plays_during_play_and_fades_at_the_title() {
        let dir = folder("music");
        std::fs::write(dir.join("theme.wav"), wav(RATE as usize)).unwrap();
        let (handle, mut mixer) = mixer(8);
        let mut audio = Audio::new(handle, RATE, &SoundList::default(), |w| panic!("{w}"));
        let track = Track {
            file: "theme.wav".into(),
            loop_start: Some(0),
            loop_length: None,
        };
        audio.set_music(Some(&track), &dir);

        let mut s = session();
        audio.update(&s, |w| panic!("{w}"));
        assert!(!loud(&mut mixer, 800), "silent on the title screen");

        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        assert!(loud(&mut mixer, 800), "plays once play starts");

        s.step(&press(Action::Pause));
        assert_eq!(s.cues, [Cue::Pause]);
        audio.update(&s, |w| panic!("{w}"));
        assert!(loud(&mut mixer, 800), "carries on while paused");

        // Pause menu: Resume, Restart level, Controls, Quit.
        s.step(&press(Action::Up));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)));
        audio.update(&s, |w| panic!("{w}"));
        let fade = (RATE as f32 * MUSIC_OUT) as usize;
        loud(&mut mixer, fade);
        assert!(!loud(&mut mixer, 800), "faded out");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_music_warns_once_and_play_goes_on() {
        let (handle, mut mixer) = mixer(8);
        let mut audio = Audio::new(handle, RATE, &SoundList::default(), |w| panic!("{w}"));
        let track = Track {
            file: "nowhere.ogg".into(),
            loop_start: None,
            loop_length: None,
        };
        audio.set_music(Some(&track), Path::new("no-such-folder"));
        let mut s = session();
        s.step(&press(Action::Jump));
        let mut warnings = Vec::new();
        for _ in 0..3 {
            audio.update(&s, |w| warnings.push(w));
            s.step(&InputFrame::default());
        }
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].ends_with("playing without music"));
        assert!(!loud(&mut mixer, 100));
        assert_eq!(s.game.world.frame, 3);
    }
}
