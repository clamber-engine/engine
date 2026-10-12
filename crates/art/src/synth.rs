//! Placeholder sound: simple waveforms written as 16-bit mono WAV.
//!
//! A **sound** is a sequence of tones, each a waveform sweeping from one
//! frequency to another. **Music** is a set of voices playing note
//! patterns on a shared step grid; shorter patterns repeat to fill the
//! longest, and the file is exactly one pass, so it loops end to start.

use serde::Deserialize;

/// Samples per second of everything written.
pub const RATE: u32 = 22050;

/// How long a tone or note takes to reach full volume, in seconds, so it
/// starts without a click.
const ATTACK: f64 = 0.003;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wave {
    #[default]
    Square,
    Triangle,
    Saw,
    /// Random levels, changing at the tone's frequency.
    Noise,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundSpec {
    /// The WAV, relative to the content folder.
    pub out: String,
    pub tones: Vec<Tone>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tone {
    #[serde(default)]
    pub wave: Wave,
    /// Frequency at the start, in Hz.
    pub from: f64,
    /// Frequency at the end; the sweep is even in pitch.
    pub to: Option<f64>,
    /// Seconds.
    pub length: f64,
    #[serde(default = "full")]
    pub volume: f64,
    /// For square waves, the fraction of each cycle that is high.
    #[serde(default = "half")]
    pub duty: f64,
    /// The fraction of the tone at its end over which it fades out.
    #[serde(default = "full")]
    pub fade: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MusicSpec {
    pub out: String,
    /// Beats per minute.
    pub tempo: f64,
    /// Pattern steps per beat.
    pub steps_per_beat: u32,
    pub voices: Vec<Voice>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Voice {
    #[serde(default)]
    pub wave: Wave,
    #[serde(default = "full")]
    pub volume: f64,
    #[serde(default = "half")]
    pub duty: f64,
    /// The fraction of each note at its end over which it fades out.
    #[serde(default = "quarter")]
    pub fade: f64,
    /// One token per step, separated by whitespace: a note such as `C4`,
    /// `F#3` or `Bb5`; `.` to hold the note before; `-` for silence.
    pub notes: String,
}

fn full() -> f64 {
    1.0
}

fn half() -> f64 {
    0.5
}

fn quarter() -> f64 {
    0.25
}

/// A sound effect as WAV.
pub fn sound(spec: &SoundSpec) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut noise = Noise::default();
    for (i, t) in spec.tones.iter().enumerate() {
        if !(t.length > 0.0 && t.from > 0.0 && t.to.is_none_or(|f| f > 0.0)) {
            return Err(format!("tone {i}: length and frequencies must be positive"));
        }
        let n = (t.length * RATE as f64).round() as usize;
        let to = t.to.unwrap_or(t.from);
        let mut osc = Oscillator::new(t.wave, t.duty);
        for s in 0..n {
            let x = s as f64 / n as f64;
            let freq = t.from * (to / t.from).powf(x);
            let level = osc.next(freq, &mut noise);
            out.push(level * t.volume * envelope(s, n, t.fade));
        }
    }
    Ok(wav(&out))
}

/// A music track as WAV.
pub fn music(spec: &MusicSpec) -> Result<Vec<u8>, String> {
    if spec.tempo.is_nan() || spec.tempo <= 0.0 || spec.steps_per_beat == 0 {
        return Err("tempo and steps_per_beat must be positive".into());
    }
    let patterns = spec
        .voices
        .iter()
        .enumerate()
        .map(|(i, v)| pattern(&v.notes).map_err(|e| format!("voice {i}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    let steps = patterns.iter().map(Vec::len).max().unwrap_or(0);
    if steps == 0 {
        return Err("no notes".into());
    }
    if let Some(i) = patterns.iter().position(|p| steps % p.len() != 0) {
        return Err(format!(
            "voice {i}: its {} steps don't divide the track's {steps}",
            patterns[i].len()
        ));
    }
    let step_seconds = 60.0 / spec.tempo / spec.steps_per_beat as f64;
    // Steps start on whole samples, so every pass is the same length.
    let start = |step: usize| (step as f64 * step_seconds * RATE as f64).round() as usize;
    let mut mix = vec![0.0; start(steps)];
    let mut noise = Noise::default();
    for (voice, pat) in spec.voices.iter().zip(&patterns) {
        let mut step = 0;
        while step < steps {
            let Step::Note(midi) = pat[step % pat.len()] else {
                step += 1;
                continue;
            };
            let mut end = step + 1;
            while end < steps && pat[end % pat.len()] == Step::Hold {
                end += 1;
            }
            let freq = 440.0 * 2f64.powf((midi as f64 - 69.0) / 12.0);
            let (a, b) = (start(step), start(end));
            let mut osc = Oscillator::new(voice.wave, voice.duty);
            for (s, out) in mix[a..b].iter_mut().enumerate() {
                let level = osc.next(freq, &mut noise);
                *out += level * voice.volume * envelope(s, b - a, voice.fade);
            }
            step = end;
        }
    }
    Ok(wav(&mix))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Note(i32),
    Hold,
    Rest,
}

fn pattern(src: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for token in src.split_whitespace() {
        let step = match token {
            "." if steps.is_empty() => return Err("starts by holding a note".into()),
            "." => Step::Hold,
            "-" => Step::Rest,
            _ => Step::Note(midi(token).ok_or_else(|| format!("bad note `{token}`"))?),
        };
        steps.push(step);
    }
    Ok(steps)
}

/// A note name such as `C4` (middle C, 60), `F#3` or `Bb5` as a MIDI note.
fn midi(token: &str) -> Option<i32> {
    let mut chars = token.chars();
    let base = match chars.next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, octave) = match rest.as_bytes().first()? {
        b'#' => (1, &rest[1..]),
        b'b' => (-1, &rest[1..]),
        _ => (0, rest),
    };
    let octave: i32 = octave.parse().ok().filter(|o| (0..=9).contains(o))?;
    Some((octave + 1) * 12 + base + shift)
}

/// Volume at sample `s` of `n`: a short rise, then a straight fade over
/// the last `fade` of the length.
fn envelope(s: usize, n: usize, fade: f64) -> f64 {
    let t = s as f64 / RATE as f64;
    let rise = (t / ATTACK).min(1.0);
    let x = s as f64 / n as f64;
    let fade = fade.clamp(0.0, 1.0);
    let fall = if fade == 0.0 {
        1.0
    } else {
        ((1.0 - x) / fade).min(1.0)
    };
    rise * fall
}

struct Oscillator {
    wave: Wave,
    duty: f64,
    /// Through the current cycle, 0 to 1.
    phase: f64,
    held: f64,
}

impl Oscillator {
    fn new(wave: Wave, duty: f64) -> Oscillator {
        Oscillator {
            wave,
            duty,
            phase: 0.0,
            held: 0.0,
        }
    }

    fn next(&mut self, freq: f64, noise: &mut Noise) -> f64 {
        let p = self.phase;
        let level = match self.wave {
            Wave::Square => {
                if p < self.duty {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            Wave::Saw => 2.0 * p - 1.0,
            Wave::Noise => self.held,
        };
        self.phase += freq / RATE as f64;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.held = noise.next();
        }
        level
    }
}

/// A fixed sequence of random levels, so output is the same every run.
struct Noise(u32);

impl Default for Noise {
    fn default() -> Noise {
        Noise(0x1234_5678)
    }
}

impl Noise {
    fn next(&mut self) -> f64 {
        // xorshift32
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x as f64 / u32::MAX as f64 * 2.0 - 1.0
    }
}

/// Samples, -1 to 1, as a 16-bit mono WAV file.
fn wav(samples: &[f64]) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend(b"RIFF");
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(1u16.to_le_bytes()); // mono
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for s in samples {
        out.extend(((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names() {
        assert_eq!(midi("C4"), Some(60));
        assert_eq!(midi("A4"), Some(69));
        assert_eq!(midi("F#3"), Some(54));
        assert_eq!(midi("Bb5"), Some(82));
        assert_eq!(midi("H4"), None);
        assert_eq!(midi("C"), None);
    }

    #[test]
    fn music_is_a_whole_number_of_steps() {
        let spec: MusicSpec = toml::from_str(
            "out = \"m.wav\"\ntempo = 120\nsteps_per_beat = 2\n\
             [[voices]]\nnotes = \"C4 . E4 -\"\n[[voices]]\nnotes = \"C3 G3\"\n",
        )
        .unwrap();
        let bytes = music(&spec).unwrap();
        // 4 steps of a quarter second
        assert_eq!(bytes.len(), 44 + RATE as usize * 2);
    }

    #[test]
    fn patterns_must_fit_the_track() {
        let spec: MusicSpec = toml::from_str(
            "out = \"m.wav\"\ntempo = 120\nsteps_per_beat = 2\n\
             [[voices]]\nnotes = \"C4 . E4 -\"\n[[voices]]\nnotes = \"C3 G3 E3\"\n",
        )
        .unwrap();
        assert!(music(&spec).unwrap_err().contains("don't divide"));
    }

    #[test]
    fn sounds_start_and_end_silent() {
        let spec: SoundSpec =
            toml::from_str("out = \"s.wav\"\n[[tones]]\nfrom = 440\nto = 880\nlength = 0.1\n")
                .unwrap();
        let bytes = sound(&spec).unwrap();
        let sample = |i: usize| i16::from_le_bytes([bytes[44 + 2 * i], bytes[45 + 2 * i]]);
        let n = (bytes.len() - 44) / 2;
        assert_eq!(n, 2205);
        assert_eq!(sample(0), 0);
        assert!(sample(n - 1).abs() < 100);
    }
}
