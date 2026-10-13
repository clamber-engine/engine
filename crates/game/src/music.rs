//! The music list: for each track, its file, how it loops and how loud
//! it plays, and which track plays on the title screen. A TOML file:
//!
//! ```toml
//! title = "opening"
//!
//! [tracks.opening]
//! file = "music/opening.mp3"
//! loop_start = 1282
//! loop_end = 1210902
//! volume = 0.8
//! ```
//!
//! Paths are relative to the list. Loop points are frames at the file's
//! rate: playback goes from the start of the file to `loop_end`, then on
//! from `loop_start`. Without them, the file's own loop tags apply, or
//! the whole file loops. `volume` is linear gain, 1 by default.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clamber_audio::Looping;
use serde::Deserialize;

/// The music list used when none is given, relative to the content
/// folder.
pub const DEFAULT_MUSIC: &str = "base/music.toml";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicList {
    /// The title screen's track, by name.
    pub title: Option<String>,
    pub tracks: BTreeMap<String, MusicTrack>,
}

/// A track in a [`MusicList`].
#[derive(Clone, Debug, PartialEq)]
pub struct MusicTrack {
    pub file: PathBuf,
    /// How it loops, when the list says.
    pub looping: Option<Looping>,
    /// Linear gain.
    pub volume: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MusicListFile {
    title: Option<String>,
    #[serde(default)]
    tracks: BTreeMap<String, TrackFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackFile {
    file: String,
    loop_start: Option<u64>,
    loop_end: Option<u64>,
    volume: Option<f32>,
}

impl MusicList {
    pub fn load(path: &Path) -> Result<MusicList, String> {
        let origin = path.display().to_string();
        let src = std::fs::read_to_string(path).map_err(|e| format!("{origin}: {e}"))?;
        MusicList::from_toml_str(&src, path.parent().unwrap_or(Path::new("")), &origin)
    }

    /// Reads a music list whose paths are relative to `dir`. `origin`
    /// names the source in error messages.
    pub fn from_toml_str(src: &str, dir: &Path, origin: &str) -> Result<MusicList, String> {
        let file: MusicListFile =
            toml::from_str(src).map_err(|e| format!("{origin}: {}", e.to_string().trim_end()))?;
        let mut tracks = BTreeMap::new();
        for (name, t) in file.tracks {
            let err = |m: &str| format!("{origin}: tracks.{name}: {m}");
            let looping = match (t.loop_start, t.loop_end) {
                (Some(start), Some(end)) if end <= start => {
                    return Err(err("`loop_end` must be after `loop_start`"));
                }
                (start, Some(end)) => Some(Looping::From {
                    start: start.unwrap_or(0),
                    length: Some(end - start.unwrap_or(0)),
                }),
                (Some(start), None) => Some(Looping::From {
                    start,
                    length: None,
                }),
                (None, None) => None,
            };
            let volume = t.volume.unwrap_or(1.0);
            if !(volume.is_finite() && volume >= 0.0) {
                return Err(err("`volume` must be 0 or more"));
            }
            let track = MusicTrack {
                file: dir.join(t.file),
                looping,
                volume,
            };
            tracks.insert(name, track);
        }
        if let Some(title) = &file.title
            && !tracks.contains_key(title)
        {
            return Err(format!("{origin}: title: no track named `{title}`"));
        }
        Ok(MusicList {
            title: file.title,
            tracks,
        })
    }

    /// The title screen's track.
    pub fn title(&self) -> Option<&MusicTrack> {
        self.tracks.get(self.title.as_ref()?)
    }

    /// The track whose file is `path`, however the path is written.
    pub fn find(&self, path: &Path) -> Option<&MusicTrack> {
        let wanted = std::fs::canonicalize(path).ok()?;
        self.tracks
            .values()
            .find(|t| std::fs::canonicalize(&t.file).is_ok_and(|f| f == wanted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_music_list() {
        let src = r#"
            title = "a"
            [tracks.a]
            file = "music/a.mp3"
            loop_start = 100
            loop_end = 500
            volume = 0.5
            [tracks.b]
            file = "b.ogg"
        "#;
        let list = MusicList::from_toml_str(src, Path::new("pack"), "music.toml").unwrap();
        let a = list.title().unwrap();
        assert_eq!(a.file, Path::new("pack/music/a.mp3"));
        assert_eq!(
            a.looping,
            Some(Looping::From {
                start: 100,
                length: Some(400)
            })
        );
        assert_eq!(a.volume, 0.5);
        let b = &list.tracks["b"];
        assert_eq!((b.looping, b.volume), (None, 1.0));
    }

    #[test]
    fn rejects_bad_lists() {
        let bad = |src: &str| MusicList::from_toml_str(src, Path::new(""), "m").unwrap_err();
        let e = bad("[tracks.a]\nfile = \"a.mp3\"\nloop_start = 5\nloop_end = 5\n");
        assert_eq!(e, "m: tracks.a: `loop_end` must be after `loop_start`");
        let e = bad("[tracks.a]\nfile = \"a.mp3\"\nvolume = -1\n");
        assert_eq!(e, "m: tracks.a: `volume` must be 0 or more");
        assert_eq!(bad("title = \"x\"\n"), "m: title: no track named `x`");
        bad("[tracks.a]\nfile = \"a.mp3\"\nloop = 3\n");
    }

    #[test]
    fn the_bundled_music_list_reads_and_finds_the_first_levels_track() {
        let content = crate::content_dir();
        let list = MusicList::load(&content.join(DEFAULT_MUSIC)).unwrap();
        assert!(
            list.title()
                .unwrap()
                .file
                .ends_with("music/intro-theme.mp3")
        );
        let level = content.join("base/levels/../music/grasslands-theme.mp3");
        let found = list.find(&level).unwrap();
        assert!(found.file.ends_with("music/grasslands-theme.mp3"));
        assert!(found.looping.is_some());
        for (name, track) in &list.tracks {
            assert!(track.file.is_file(), "{name}: {}", track.file.display());
        }
    }

    /// Every bundled loop's seam, played: no click, no gap, and the
    /// music after it decodes as it would straight through.
    #[test]
    fn the_bundled_loops_are_seamless() {
        let list = MusicList::load(&crate::content_dir().join(DEFAULT_MUSIC)).unwrap();
        for (name, track) in &list.tracks {
            let Some(Looping::From {
                start,
                length: Some(length),
            }) = track.looping
            else {
                panic!("{name}: no loop end");
            };
            let input = clamber_audio::Input::file(&track.file);
            let seam = clamber_audio::seam::check(&input, start, start + length).unwrap();
            assert!(seam.click < 0.0, "{name}: {seam:?}");
            assert!(seam.gap < 0.001, "{name}: {seam:?}");
            assert!(seam.after < -60.0, "{name}: {seam:?}");
        }
    }
}
