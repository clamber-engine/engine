//! Mixing effects and music into the output stream.

use std::f32::consts::FRAC_PI_2;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::{AudioError, Music, Sound};

/// A volume group the player can adjust separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Music,
    Effects,
    Ambience,
}

impl Category {
    const COUNT: usize = 3;
}

/// A playing sound, for stopping it later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceId(u64);

/// How to play a sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Play {
    pub category: Category,
    /// Linear gain, 1.0 for the sound as recorded.
    pub volume: f32,
    /// -1.0 is fully left, 1.0 fully right.
    pub pan: f32,
    /// Repeat until stopped, for ambience and engine hums.
    pub looped: bool,
}

impl Default for Play {
    fn default() -> Play {
        Play {
            category: Category::Effects,
            volume: 1.0,
            pan: 0.0,
            looped: false,
        }
    }
}

enum Command {
    Play(VoiceId, Sound, Play),
    Stop(VoiceId),
    StopCategory(Category),
    Volume(Category, f32),
    Master(f32),
    Music(Option<Box<Music>>, u32),
}

/// Creates a mixer that plays at most `max_voices` sounds at once (music
/// not included), and the handle that controls it. The mixer goes to the
/// audio output; the handle stays with the game. Sounds and music must be
/// loaded at the output's rate.
pub fn mixer(max_voices: usize) -> (AudioHandle, Mixer) {
    let (tx, rx) = channel();
    let handle = AudioHandle {
        commands: tx,
        next_voice: 0,
        music: None,
    };
    let mixer = Mixer {
        commands: rx,
        voices: Vec::new(),
        max_voices: max_voices.max(1),
        volumes: [1.0; Category::COUNT],
        master: 1.0,
        music: Vec::new(),
        scratch: Vec::new(),
    };
    (handle, mixer)
}

/// The game's side of the mixer. Commands take effect at the start of the
/// mixer's next buffer, so with buffers no longer than a logic frame, a
/// sound starts within the frame that triggered it.
pub struct AudioHandle {
    commands: Sender<Command>,
    next_voice: u64,
    music: Option<String>,
}

impl AudioHandle {
    fn send(&self, c: Command) {
        // a dropped mixer means no audio output; the game carries on
        let _ = self.commands.send(c);
    }

    pub fn play(&mut self, sound: &Sound, how: Play) -> VoiceId {
        let id = VoiceId(self.next_voice);
        self.next_voice += 1;
        self.send(Command::Play(id, sound.clone(), how));
        id
    }

    pub fn stop(&self, voice: VoiceId) {
        self.send(Command::Stop(voice));
    }

    pub fn stop_category(&self, category: Category) {
        self.send(Command::StopCategory(category));
    }

    /// Sets a category's volume, as linear gain.
    pub fn set_volume(&self, category: Category, volume: f32) {
        self.send(Command::Volume(category, volume.max(0.0)));
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.send(Command::Master(volume.max(0.0)));
    }

    /// The music track playing, by the id it was started with.
    pub fn music(&self) -> Option<&str> {
        self.music.as_deref()
    }

    /// Crossfades to the track `id` over `fade` frames. If `id` is already
    /// playing it carries on undisturbed, so music continues across level
    /// sections that share it; `open` is only called for a new track.
    pub fn play_music(
        &mut self,
        id: &str,
        fade: u32,
        open: impl FnOnce() -> Result<Music, AudioError>,
    ) -> Result<(), AudioError> {
        if self.music.as_deref() == Some(id) {
            return Ok(());
        }
        let music = open()?;
        self.send(Command::Music(Some(Box::new(music)), fade));
        self.music = Some(id.to_string());
        Ok(())
    }

    /// Fades the music out over `fade` frames.
    pub fn stop_music(&mut self, fade: u32) {
        self.send(Command::Music(None, fade));
        self.music = None;
    }
}

struct Voice {
    id: VoiceId,
    sound: Sound,
    play: Play,
    /// Next frame to play.
    at: usize,
}

struct MusicSlot {
    music: Box<Music>,
    /// Fade position from 0 (silent) to 1 (full), and its change per frame.
    level: f32,
    rate: f32,
}

/// Mixes playing sounds into stereo output. Runs wherever the audio output
/// wants samples.
pub struct Mixer {
    commands: Receiver<Command>,
    voices: Vec<Voice>,
    max_voices: usize,
    volumes: [f32; Category::COUNT],
    master: f32,
    music: Vec<MusicSlot>,
    scratch: Vec<f32>,
}

impl Mixer {
    /// Sounds playing now, music not included.
    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    fn apply(&mut self, c: Command) {
        match c {
            Command::Play(id, sound, play) => {
                if sound.frames() == 0 {
                    return;
                }
                if self.voices.len() >= self.max_voices {
                    // the oldest sound makes way
                    self.voices.remove(0);
                }
                self.voices.push(Voice {
                    id,
                    sound,
                    play,
                    at: 0,
                });
            }
            Command::Stop(id) => self.voices.retain(|v| v.id != id),
            Command::StopCategory(c) => self.voices.retain(|v| v.play.category != c),
            Command::Volume(c, v) => self.volumes[c as usize] = v,
            Command::Master(v) => self.master = v,
            Command::Music(music, fade) => {
                let step = if fade == 0 { 1.0 } else { 1.0 / fade as f32 };
                for slot in &mut self.music {
                    slot.rate = -step;
                }
                if let Some(music) = music {
                    self.music.push(MusicSlot {
                        music,
                        level: 0.0,
                        rate: step,
                    });
                }
                if fade == 0 {
                    self.music.retain(|s| s.rate > 0.0);
                    for s in &mut self.music {
                        s.level = 1.0;
                    }
                }
            }
        }
    }

    /// Fills `out` with interleaved stereo samples. Never waits: music that
    /// isn't decoded yet is silent.
    pub fn mix(&mut self, out: &mut [f32]) {
        self.mix_with(out, false);
    }

    /// Like [`mix`](Self::mix), but waits for music to decode. For offline
    /// rendering, never for live output.
    pub fn render(&mut self, out: &mut [f32]) {
        self.mix_with(out, true);
    }

    fn mix_with(&mut self, out: &mut [f32], wait: bool) {
        while let Ok(c) = self.commands.try_recv() {
            self.apply(c);
        }
        out.fill(0.0);
        let frames = out.len() / 2;

        for v in &mut self.voices {
            let gain = self.master * self.volumes[v.play.category as usize] * v.play.volume;
            // constant-power pan
            let angle = (v.play.pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_2 / 2.0;
            let (gl, gr) = (gain * angle.cos(), gain * angle.sin());
            let ch = v.sound.channels;
            let len = v.sound.frames();
            for f in 0..frames {
                if v.at == len {
                    if !v.play.looped {
                        break;
                    }
                    v.at = 0;
                }
                let s = &v.sound.samples[v.at * ch..v.at * ch + ch];
                let (l, r) = if ch == 1 { (s[0], s[0]) } else { (s[0], s[1]) };
                // centred is equal in both ears at full gain
                out[2 * f] += l * gl * std::f32::consts::SQRT_2;
                out[2 * f + 1] += r * gr * std::f32::consts::SQRT_2;
                v.at += 1;
            }
        }
        self.voices
            .retain(|v| v.play.looped || v.at < v.sound.frames());

        let music_gain = self.master * self.volumes[Category::Music as usize];
        self.scratch.resize(out.len(), 0.0);
        for slot in &mut self.music {
            let playing = if wait {
                slot.music.fill_blocking(&mut self.scratch)
            } else {
                slot.music.fill(&mut self.scratch)
            };
            let track_gain = music_gain * slot.music.volume();
            for f in 0..frames {
                let g = track_gain * (slot.level * FRAC_PI_2).sin();
                out[2 * f] += self.scratch[2 * f] * g;
                out[2 * f + 1] += self.scratch[2 * f + 1] * g;
                slot.level = (slot.level + slot.rate).clamp(0.0, 1.0);
            }
            if !playing {
                slot.rate = 0.0;
                slot.level = 0.0;
            }
        }
        self.music.retain(|s| s.level > 0.0 || s.rate > 0.0);

        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}
