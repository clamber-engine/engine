//! Ogg Vorbis files written by [`Vorbis`] keep their loops exact.
#![cfg(feature = "encode")]

use clamber_audio::encode::Vorbis;
use clamber_audio::{Input, Sound, seam};

const RATE: u32 = 32_000;

/// One frame of a stereo phrase that repeats every `length` frames:
/// two notes a fifth apart, about 440 and 660 Hz but a whole number of
/// cycles a phrase so that it repeats smoothly, swelling four times a
/// phrase and never silent.
fn phrase(f: usize, length: usize) -> [i16; 2] {
    let x = std::f64::consts::TAU * (f % length) as f64 / length as f64;
    let cycles = (220.0 * length as f64 / RATE as f64).round();
    let pulse = 0.6 - 0.4 * (4.0 * x).cos();
    let at = |n: f64| ((n * x + 1.0).sin() * pulse * 12000.0) as i16;
    [at(2.0 * cycles), at(3.0 * cycles)]
}

/// An intro of `lead_in` frames of a lower note, then the phrase twice:
/// a song as the importer renders it, looping over its second time.
fn song(lead_in: usize, length: usize) -> Vec<i16> {
    let mut out = Vec::new();
    for f in 0..lead_in {
        let t = f as f64 / RATE as f64;
        let v = ((std::f64::consts::TAU * 220.0 * t).sin() * 8000.0) as i16;
        out.extend([v, v]);
    }
    for f in 0..2 * length {
        out.extend(phrase(f, length));
    }
    out
}

/// The error between `decoded` and `source` with `decoded` shifted by
/// `shift` frames, relative to the source, in dB.
fn error_db(source: &[i16], decoded: &[f32], shift: isize) -> f64 {
    let (mut diff, mut level) = (0.0, 0.0);
    let frames = source.len() / 2;
    for f in 4096..frames - 4096 {
        for c in 0..2 {
            let s = f64::from(source[2 * f + c]) / 32768.0;
            let d = f64::from(decoded[2 * (f as isize + shift) as usize + c]);
            diff += (s - d).powi(2);
            level += s * s;
        }
    }
    10.0 * (diff / level).log10()
}

#[test]
fn a_looping_song_keeps_its_length_alignment_and_seam() {
    let (lead_in, length) = (3001, 3 * RATE as usize + 7);
    let source = song(lead_in, length);
    let ogg = Vorbis::default().encode(&source, 2, RATE).unwrap();
    assert!(ogg.len() < source.len(), "{} bytes", ogg.len());
    let input = Input::bytes(ogg, Some("ogg"));

    // Every frame comes back, none added before or after.
    let (decoded, rate) = Sound::decode(&input).unwrap();
    assert_eq!((rate, decoded.channels), (RATE, 2));
    assert_eq!(decoded.frames(), source.len() / 2);
    // No encoder delay: the decode lines up with the source best
    // unshifted, and closely.
    let unshifted = error_db(&source, &decoded.samples, 0);
    assert!(unshifted < -25.0, "{unshifted} dB");
    for shift in [-2, -1, 1, 2, -64, 64] {
        let shifted = error_db(&source, &decoded.samples, shift);
        assert!(
            shifted > unshifted + 6.0,
            "{shift}: {shifted} vs {unshifted} dB"
        );
    }

    // The loop played through its seam, as the game plays it.
    let start = (lead_in + length) as u64;
    let seam = seam::check(&input, start, start + length as u64).unwrap();
    assert_eq!(seam.gap, 0.0, "{seam:?}");
    assert!(seam.click < 6.0, "{seam:?}");
    assert!(seam.mismatch < -25.0, "{seam:?}");
    assert!(seam.after < -60.0, "{seam:?}");
}

#[test]
fn mono_encodes_too() {
    let source: Vec<i16> = (0..RATE as usize / 5).map(|f| phrase(f, 1000)[0]).collect();
    let ogg = Vorbis::default().encode(&source, 1, RATE).unwrap();
    let (decoded, _) = Sound::decode(&Input::bytes(ogg, Some("ogg"))).unwrap();
    assert_eq!((decoded.channels, decoded.frames()), (1, source.len()));
}

#[test]
fn the_same_audio_encodes_to_the_same_file() {
    let source = song(100, 4000);
    let encode = || Vorbis::default().encode(&source, 2, RATE).unwrap();
    assert_eq!(encode(), encode());
}

#[test]
fn tags_are_written() {
    let vorbis = Vorbis {
        tags: vec![("TITLE".into(), "made-up".into())],
        ..Vorbis::default()
    };
    let ogg = vorbis.encode(&song(0, 2000), 2, RATE).unwrap();
    assert!(ogg.windows(13).any(|w| w == b"TITLE=made-up"));
}

#[test]
fn a_quality_out_of_range_is_refused() {
    let vorbis = Vorbis {
        quality: 11.0,
        ..Vorbis::default()
    };
    let err = vorbis.encode(&[0; 4], 2, RATE).unwrap_err();
    assert!(err.0.contains("quality 11"), "{err}");
}
