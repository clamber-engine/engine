use std::cell::Cell;

use clamber_audio::{Category, Input, LoopPoints, Looping, Music, Play, Sound, mixer};

/// A 16-bit PCM WAV.
fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Mono WAV whose frame `i` has the value `i + 1` (so silence is 0).
fn ramp(frames: usize) -> Input {
    let samples: Vec<i16> = (1..=frames as i16).collect();
    Input::bytes(wav(8000, 1, &samples), Some("wav"))
}

/// Recovers ramp values from output samples.
fn values(samples: &[f32]) -> Vec<i32> {
    samples
        .iter()
        .map(|s| (s * 32768.0).round() as i32)
        .collect()
}

/// Left channel of `frames` frames of music.
fn play(music: &mut Music, frames: usize) -> Vec<i32> {
    let mut out = vec![0.0; frames * 2];
    music.fill_blocking(&mut out);
    values(&out).into_iter().step_by(2).collect()
}

#[test]
fn sound_decodes_whole_file() {
    let s = Sound::load(&ramp(100), 8000).unwrap();
    assert_eq!(s.channels, 1);
    assert_eq!(values(&s.samples), (1..=100).collect::<Vec<_>>());

    let stereo = wav(8000, 2, &[1, -1, 2, -2]);
    let s = Sound::load(&Input::bytes(stereo, Some("wav")), 8000).unwrap();
    assert_eq!(s.channels, 2);
    assert_eq!(values(&s.samples), [1, -1, 2, -2]);
}

#[test]
fn sound_resampled_to_output_rate() {
    let s = Sound::load(&ramp(800), 16000).unwrap();
    assert!((s.frames() as i64 - 1600).abs() <= 2, "{}", s.frames());
    assert_eq!(values(&s.samples[..4]), [1, 2, 2, 3]); // 1, 1.5, 2, 2.5 rounded
}

#[test]
fn bad_audio_is_an_error() {
    let err = Sound::load(&Input::bytes(vec![1, 2, 3], Some("wav")), 8000).unwrap_err();
    assert!(err.0.starts_with("<wav data>: "), "{err}");
    let err = Sound::load(&Input::file("/nonexistent/x.ogg"), 8000).unwrap_err();
    assert!(err.0.starts_with("/nonexistent/x.ogg: "), "{err}");
}

#[test]
fn music_loops_between_points_exactly() {
    let looping = Looping::From {
        start: 200,
        length: Some(300),
    };
    let mut m = Music::open(ramp(1000), Some(looping), 8000).unwrap();
    let got = play(&mut m, 1200);
    let expected: Vec<i32> = (1..=500)
        .chain(201..=500)
        .chain(201..=500)
        .chain(201..=500)
        .take(1200)
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn music_loops_whole_track_or_plays_once() {
    let mut m = Music::open(ramp(5000), None, 8000).unwrap();
    let got = play(&mut m, 12000);
    let expected: Vec<i32> = (1..=5000).cycle().take(12000).collect();
    assert_eq!(got, expected);

    // a loop start with no length runs to the end of the file
    let from = Looping::From {
        start: 4000,
        length: None,
    };
    let mut m = Music::open(ramp(5000), Some(from), 8000).unwrap();
    let got = play(&mut m, 7000);
    assert_eq!(got[4999..5002], [5000, 4001, 4002]);

    let mut m = Music::open(ramp(3000), Some(Looping::Once), 8000).unwrap();
    let got = play(&mut m, 4000);
    assert_eq!(got[2999], 3000);
    assert!(got[3000..].iter().all(|&v| v == 0));
    assert!(m.is_finished());
}

#[test]
fn loop_points_from_tags() {
    let tags = [
        ("LoopStart", "4410"),
        ("LOOPLENGTH", " 88200 "),
        ("TITLE", "x"),
    ];
    assert_eq!(
        Looping::from_tags(tags),
        Ok(Some(Looping::From {
            start: 4410,
            length: Some(88200)
        }))
    );
    assert_eq!(Looping::from_tags([("TITLE", "x")]), Ok(None));
    assert!(Looping::from_tags([("LOOPLENGTH", "5")]).is_err());
    assert!(Looping::from_tags([("LOOPSTART", "-5")]).is_err());
}

#[test]
fn loop_points_from_metadata() {
    let p: LoopPoints = serde_json::from_str(r#"{ "loop_start": 10, "loop_length": 20 }"#).unwrap();
    assert_eq!(
        Looping::from(p),
        Looping::From {
            start: 10,
            length: Some(20)
        }
    );
    assert!(serde_json::from_str::<LoopPoints>(r#"{ "loop_end": 10 }"#).is_err());
}

#[test]
fn mixer_plays_effects_with_volume_and_pan() {
    let (mut handle, mut mixer) = mixer(8);
    let s = Sound::load(&ramp(4), 8000).unwrap();
    handle.play(&s, Play::default());
    let mut out = vec![0.0; 12];
    mixer.mix(&mut out);
    assert_eq!(values(&out), [1, 1, 2, 2, 3, 3, 4, 4, 0, 0, 0, 0]);
    assert_eq!(mixer.voices(), 0, "finished sounds are dropped");

    handle.set_volume(Category::Effects, 0.5);
    handle.play(
        &s,
        Play {
            pan: 1.0,
            volume: 2.0,
            ..Play::default()
        },
    );
    mixer.mix(&mut out[..2]);
    assert!(out[0].abs() < 1e-6, "panned right");
    assert_eq!(values(&out[1..2]), [1]);
}

#[test]
fn mixer_voice_limit_drops_oldest() {
    let (mut handle, mut mixer) = mixer(2);
    let s = Sound::load(&ramp(100), 8000).unwrap();
    let first = handle.play(&s, Play::default());
    handle.play(&s, Play::default());
    handle.play(&s, Play::default());
    let mut out = vec![0.0; 2];
    mixer.mix(&mut out);
    assert_eq!(mixer.voices(), 2);
    assert_eq!(values(&out), [2, 2]);
    handle.stop(first); // already dropped; harmless
    let looped = handle.play(
        &s,
        Play {
            looped: true,
            category: Category::Ambience,
            ..Play::default()
        },
    );
    mixer.mix(&mut out);
    handle.stop_category(Category::Effects);
    let mut long = vec![0.0; 400];
    mixer.mix(&mut long);
    assert_eq!(mixer.voices(), 1, "a looped sound keeps playing");
    handle.stop(looped);
    mixer.mix(&mut out);
    assert_eq!(mixer.voices(), 0);
}

#[test]
fn music_continues_and_crossfades() {
    let (mut handle, mut mixer) = mixer(8);
    let opened = Cell::new(0);
    let open = |frames| {
        opened.set(opened.get() + 1);
        Music::open(ramp(frames), None, 8000)
    };
    handle.play_music("field", 0, || open(4000)).unwrap();
    handle.play_music("field", 0, || open(4000)).unwrap();
    assert_eq!(opened.get(), 1, "the same track isn't restarted");
    assert_eq!(handle.music(), Some("field"));

    let mut out = vec![0.0; 8];
    mixer.render(&mut out);
    assert_eq!(values(&out), [1, 1, 2, 2, 3, 3, 4, 4]);

    handle.play_music("cave", 100, || open(4000)).unwrap();
    assert_eq!(opened.get(), 2);
    let mut fade = vec![0.0; 2 * 200];
    mixer.render(&mut fade);
    // halfway through, both tracks are at equal power
    let (old, new) = (5.0 + 50.0, 1.0 + 50.0);
    let expect = (old + new) * std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        (fade[100] * 32768.0 - expect).abs() < 2.0,
        "{}",
        fade[100] * 32768.0
    );
    // after the fade only the new track plays
    assert_eq!(values(&fade[398..400]), [200, 200]);

    handle.stop_music(0);
    mixer.mix(&mut out);
    assert!(out.iter().all(|&s| s == 0.0));
    assert_eq!(handle.music(), None);
}
