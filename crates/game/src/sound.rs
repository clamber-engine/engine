//! What the game sounds like: [`Cue`]s for what happens in play, a
//! [`SoundList`] naming a file for each, and [`Audio`], which plays them
//! and the music through `clamber-audio`.
//!
//! Sound is never needed to play: a missing or unreadable file is a
//! warning, and the cue it was for stays silent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clamber_audio::{AudioHandle, Category, Input, Looping, Music, Play, Sound};
use clamber_level::Track;
use clamber_sim::{Event, FlowEvent, World};
use serde::Deserialize;

use crate::music::{MusicList, MusicTrack};
use crate::prefs::Volume;
use crate::session::{Session, Tune};

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
    /// An enemy was defeated.
    Defeat,
    /// A launcher fired.
    Launch,
}

impl Cue {
    pub const ALL: [Cue; 11] = [
        Cue::Jump,
        Cue::Land,
        Cue::Roll,
        Cue::Collect,
        Cue::Checkpoint,
        Cue::Hurt,
        Cue::Complete,
        Cue::GameOver,
        Cue::Pause,
        Cue::Defeat,
        Cue::Launch,
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
            Cue::Defeat => "defeat",
            Cue::Launch => "launch",
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
        FlowEvent::Defeat(_) => Some(Cue::Defeat),
        FlowEvent::Launch(_) => Some(Cue::Launch),
        FlowEvent::Respawn
        | FlowEvent::Life
        | FlowEvent::Letter(_)
        | FlowEvent::Exited(_)
        | FlowEvent::Ended => None,
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
/// Paths are relative to the list. Files can be Ogg Vorbis, MP3, FLAC or
/// WAV. Cues without a file are silent.
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

/// Fade in from silence, in seconds.
const MUSIC_IN: f32 = 0.05;
/// Crossfade from one track to another, as between the title and play.
const CROSSFADE: f32 = 1.0;
/// Fade out when the music stops, as when the level ends.
const MUSIC_OUT: f32 = 0.75;

/// Plays a session's cues, and the music for its screen, on a mixer.
pub struct Audio {
    handle: AudioHandle,
    rate: u32,
    effects: BTreeMap<Cue, Sound>,
    title: Option<TrackToPlay>,
    level: Option<TrackToPlay>,
    /// The loops and volumes of music, from [`set_title_music`].
    ///
    /// [`set_title_music`]: Audio::set_title_music
    list: MusicList,
    /// The folder and track of the level the session was last seen
    /// playing, to follow it to another.
    level_seen: Option<(PathBuf, Option<Track>)>,
    /// The session's tick on which the level's music starts, when play
    /// came into the level with the other's music cut and a silence
    /// before its own.
    silent_until: Option<u64>,
    /// The volumes last set on the mixer, from the prefs.
    volume: Option<Volume>,
}

struct TrackToPlay {
    path: PathBuf,
    looping: Option<Looping>,
    volume: f32,
    /// It couldn't be opened, which was reported once.
    failed: bool,
}

impl TrackToPlay {
    fn new(path: PathBuf, track: Option<&MusicTrack>) -> TrackToPlay {
        TrackToPlay {
            path,
            looping: track.and_then(|t| t.looping),
            volume: track.map_or(1.0, |t| t.volume),
            failed: false,
        }
    }
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
            title: None,
            level: None,
            list: MusicList::default(),
            level_seen: None,
            silent_until: None,
            volume: None,
        }
    }

    /// Sets the title screen's music, from `list`, which also gives the
    /// loops and volumes of levels played after the first.
    pub fn set_title_music(&mut self, list: &MusicList) {
        self.list = list.clone();
        self.title = list
            .title()
            .map(|t| TrackToPlay::new(t.file.clone(), Some(t)));
    }

    /// Sets the level's music, whose file is relative to `dir`. Its loop
    /// and volume come from `list`, unless the level gives a loop. A level
    /// without music plays `list`'s track for one.
    pub fn set_music(&mut self, track: Option<&Track>, dir: &Path, list: &MusicList) {
        let Some(track) = track else {
            self.level = list
                .level()
                .map(|t| TrackToPlay::new(t.file.clone(), Some(t)));
            return;
        };
        let path = dir.join(&track.file);
        let mut play = TrackToPlay::new(path.clone(), list.find(&path));
        if let Some(start) = track.loop_start {
            play.looping = Some(Looping::From {
                start,
                length: track.loop_length,
            });
        }
        self.level = Some(play);
    }

    /// Cues with a sound.
    pub fn effects(&self) -> impl Iterator<Item = Cue> + '_ {
        self.effects.keys().copied()
    }

    /// Plays the cues of `session`'s last step, sets the volumes from
    /// its prefs, and changes the music as its screen asks: fading in
    /// from silence, crossfading from another track, or fading out.
    /// Music that can't be opened is passed to `warn`, once. When the
    /// session plays another level, its music becomes the level's.
    pub fn update(&mut self, session: &Session, mut warn: impl FnMut(String)) {
        let dir = session.level_path.parent().unwrap_or(Path::new(""));
        let music = &session.game.world.level.music;
        let seen = self.level_seen.as_ref();
        if seen.is_none_or(|(d, m)| d != dir || m != music) {
            if seen.is_some() {
                let list = std::mem::take(&mut self.list);
                self.set_music(music.as_ref(), dir, &list);
                self.list = list;
                // A track with a silence is cut to and started after it,
                // not crossfaded to (`music.md`, "A change of song").
                // Counted from the step that brought play here.
                if let Some(n) = music.as_ref().and_then(|m| m.silence) {
                    self.silent_until = Some(session.ticks + u64::from(n));
                    self.handle.stop_music(0);
                }
            }
            self.level_seen = Some((dir.to_path_buf(), music.clone()));
        }

        let volume = session.prefs.volume;
        if self.volume != Some(volume) {
            let set = |c, percent| self.handle.set_volume(c, Volume::gain(percent));
            set(Category::Music, volume.music);
            set(Category::Effects, volume.effects);
            self.volume = Some(volume);
        }

        for cue in &session.cues {
            if let Some(sound) = self.effects.get(cue) {
                self.handle.play(sound, Play::default());
            }
        }

        let playing = self.handle.music().is_some();
        let fade = self.frames(if playing { CROSSFADE } else { MUSIC_IN });
        let out = self.frames(MUSIC_OUT);
        let rate = self.rate;
        let wanted = match session.tune() {
            Some(Tune::Title) => self.title.as_mut(),
            Some(Tune::Level) => self.level.as_mut(),
            None => None,
        };
        let silent = match self.silent_until {
            Some(until) if session.ticks < until => true,
            Some(_) => {
                self.silent_until = None;
                false
            }
            None => false,
        };
        let wanted = wanted.filter(|_| !silent || session.tune() != Some(Tune::Level));
        let Some(track) = wanted.filter(|t| !t.failed) else {
            if playing {
                self.handle.stop_music(out);
            }
            return;
        };
        let id = track.path.display().to_string();
        let result = self.handle.play_music(&id, fade, || {
            Music::open(Input::file(&track.path), track.looping, rate)
                .map(|m| m.with_volume(track.volume))
        });
        if let Err(e) = result {
            track.failed = true;
            warn(format!("{id}: {e}; playing without music"));
            if playing {
                self.handle.stop_music(out);
            }
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
    use crate::menu::Menu;
    use crate::session::{Item, Screen};

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
        level_wav(frames, i16::MAX)
    }

    /// A mono 16-bit WAV of `frames` frames, all `value`.
    fn level_wav(frames: usize, value: i16) -> Vec<u8> {
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
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }

    /// A folder of its own for one test's files.
    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clamber-sound-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A session with the volumes at full, so that gains are the
    /// tracks' own.
    fn session() -> Session {
        let level = Level::from_toml_str(LEVEL, "test").unwrap();
        let mut s = Session::new(Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("default").unwrap()),
        )));
        s.prefs.volume = Volume {
            music: 100,
            effects: 100,
        };
        s
    }

    fn press(action: Action) -> InputFrame {
        InputFrame::single(Frame::after(Actions::NONE, action.into()))
    }

    /// Mixes `frames` frames and returns the last frame's left sample.
    fn last(mixer: &mut Mixer, frames: usize) -> f32 {
        let mut out = vec![0.0; frames * 2];
        mixer.render(&mut out);
        out[out.len() - 2]
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
    fn title_and_level_music_crossfade_and_fade_out_when_the_level_ends() {
        // the title's track is positive and the level's negative, at the
        // volumes the music list gives them
        let dir = folder("music");
        std::fs::write(dir.join("title.wav"), level_wav(RATE as usize, 16384)).unwrap();
        std::fs::write(dir.join("level.wav"), level_wav(RATE as usize, -16384)).unwrap();
        let list = MusicList::from_toml_str(
            "title = \"t\"\n\
             [tracks.t]\nfile = \"title.wav\"\nvolume = 0.5\n\
             [tracks.l]\nfile = \"level.wav\"\nloop_start = 0\nloop_end = 4000\n",
            &dir,
            "music.toml",
        )
        .unwrap();
        let (handle, mut mixer) = mixer(8);
        let mut audio = Audio::new(handle, RATE, &SoundList::default(), |w| panic!("{w}"));
        audio.set_title_music(&list);
        let track = Track {
            file: "level.wav".into(),
            loop_start: None,
            loop_length: None,
            silence: None,
        };
        audio.set_music(Some(&track), &dir, &list);
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        let crossfade = (RATE as f32 * CROSSFADE) as usize;

        let mut s = session();
        audio.update(&s, |w| panic!("{w}"));
        assert!(near(last(&mut mixer, 800), 0.25), "the title's music");

        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        let halfway = last(&mut mixer, crossfade / 2);
        assert!(halfway > -0.5 && halfway < 0.25, "crossfading: {halfway}");
        assert!(near(last(&mut mixer, crossfade), -0.5), "the level's music");

        s.step(&press(Action::Pause));
        audio.update(&s, |w| panic!("{w}"));
        assert!(near(last(&mut mixer, 800), -0.5), "carries on while paused");

        // Pause menu: Resume, Restart level, Controls, Options, Quit.
        s.step(&press(Action::Up));
        s.step(&press(Action::Jump));
        assert!(matches!(s.screen, Screen::Title(_)));
        audio.update(&s, |w| panic!("{w}"));
        assert!(
            near(last(&mut mixer, crossfade + 1), 0.25),
            "back to the title's"
        );

        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        last(&mut mixer, crossfade + 1);
        s.screen = Screen::GameOver(Menu::new([("Quit", Item::ToTitle)]));
        audio.update(&s, |w| panic!("{w}"));
        let fade = (RATE as f32 * MUSIC_OUT) as usize;
        let fading = last(&mut mixer, fade / 2);
        assert!(fading < -0.05 && fading > -0.5, "fading out: {fading}");
        assert_eq!(last(&mut mixer, fade), 0.0, "faded out at game over");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plays_at_the_volumes_in_the_prefs() {
        let dir = folder("volume");
        std::fs::write(dir.join("jump.wav"), level_wav(400, 16384)).unwrap();
        std::fs::write(dir.join("level.wav"), level_wav(400, 16384)).unwrap();
        let sounds =
            SoundList::from_toml_str("[effects]\njump = \"jump.wav\"\n", &dir, "s").unwrap();
        let (handle, mut mixer) = mixer(8);
        let mut audio = Audio::new(handle, RATE, &sounds, |w| panic!("{w}"));
        let track = Track {
            file: "level.wav".into(),
            loop_start: None,
            loop_length: None,
            silence: None,
        };
        audio.set_music(Some(&track), &dir, &MusicList::default());

        let mut s = session();
        s.prefs.volume.music = 50;
        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        // past the fade in
        let music = last(&mut mixer, 800);
        assert!((music - 0.5 * 0.25).abs() < 0.01, "{music}");

        s.prefs.volume.music = 0;
        s.prefs.volume.effects = 50;
        s.step(&press(Action::Jump));
        assert_eq!(s.cues, [Cue::Jump]);
        audio.update(&s, |w| panic!("{w}"));
        let jump = last(&mut mixer, 100);
        assert!((jump - 0.5 * 0.25).abs() < 0.01, "{jump}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn follows_the_session_to_another_level() {
        let dir = folder("follow");
        std::fs::write(dir.join("first.wav"), level_wav(RATE as usize, -16384)).unwrap();
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("second.wav"), level_wav(RATE as usize, 16384)).unwrap();
        let (handle, mut mixer) = mixer(8);
        let mut audio = Audio::new(handle, RATE, &SoundList::default(), |w| panic!("{w}"));
        audio.set_title_music(&MusicList::default());
        let track = Track {
            file: "first.wav".into(),
            loop_start: None,
            loop_length: None,
            silence: None,
        };
        audio.set_music(Some(&track), &dir, &MusicList::default());

        let mut s = session();
        s.level_path = dir.join("first.level");
        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        assert!(last(&mut mixer, 800) < -0.2, "the first level's music");

        let src = format!("{LEVEL}\n[music]\nfile = \"second.wav\"\n");
        let level = Level::from_toml_str(&src, "test").unwrap();
        s.game = Game::new(World::new(Arc::new(level), s.game.world.feel.clone()));
        s.level_path = other.join("second.level");
        audio.update(&s, |w| panic!("{w}"));
        let crossfade = (RATE as f32 * CROSSFADE) as usize;
        assert!(last(&mut mixer, crossfade + 1) > 0.2, "the second level's");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_level_with_a_silence_is_cut_to_and_started_after_it() {
        let dir = folder("silence");
        std::fs::write(dir.join("first.wav"), level_wav(RATE as usize, -16384)).unwrap();
        std::fs::write(dir.join("second.wav"), level_wav(RATE as usize, 16384)).unwrap();
        let (handle, mut mixer) = mixer(8);
        let mut audio = Audio::new(handle, RATE, &SoundList::default(), |w| panic!("{w}"));
        audio.set_title_music(&MusicList::default());
        let track = Track {
            file: "first.wav".into(),
            loop_start: None,
            loop_length: None,
            silence: None,
        };
        audio.set_music(Some(&track), &dir, &MusicList::default());

        let mut s = session();
        s.level_path = dir.join("first.level");
        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        assert!(last(&mut mixer, 800) < -0.2, "the first level's music");

        let src = format!("{LEVEL}\n[music]\nfile = \"second.wav\"\nsilence = 3\n");
        let level = Level::from_toml_str(&src, "test").unwrap();
        s.game = Game::new(World::new(Arc::new(level), s.game.world.feel.clone()));
        s.level_path = dir.join("second.level");
        audio.update(&s, |w| panic!("{w}"));
        assert_eq!(last(&mut mixer, 10), 0.0, "cut at once, no crossfade");
        for _ in 0..2 {
            s.step(&press(Action::Left));
            audio.update(&s, |w| panic!("{w}"));
            assert_eq!(last(&mut mixer, 10), 0.0, "silent");
        }
        s.step(&press(Action::Left));
        audio.update(&s, |w| panic!("{w}"));
        assert!(last(&mut mixer, 800) > 0.2, "the second level's, after 3");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_level_without_music_plays_the_list_s_track_for_one() {
        let dir = folder("fallback");
        std::fs::write(dir.join("fallback.wav"), level_wav(RATE as usize, 16384)).unwrap();
        let list = MusicList::from_toml_str(
            "level = \"f\"\n[tracks.f]\nfile = \"fallback.wav\"\nvolume = 0.5\n",
            &dir,
            "music.toml",
        )
        .unwrap();
        let (handle, mut mixer) = mixer(8);
        let mut audio = Audio::new(handle, RATE, &SoundList::default(), |w| panic!("{w}"));
        audio.set_music(None, &dir, &list);
        let mut s = session();
        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        let music = last(&mut mixer, 800);
        assert!((music - 0.25).abs() < 0.01, "{music}");

        audio.set_music(None, &dir, &MusicList::default());
        s.step(&press(Action::Jump));
        audio.update(&s, |w| panic!("{w}"));
        let fade = (RATE as f32 * MUSIC_OUT) as usize;
        assert_eq!(
            last(&mut mixer, fade + 800),
            0.0,
            "no track for one: silence"
        );
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
            silence: None,
        };
        audio.set_music(
            Some(&track),
            Path::new("no-such-folder"),
            &MusicList::default(),
        );
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
