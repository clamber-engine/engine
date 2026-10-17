//! Songs (`spec/formats/songs.md`): plays a [`Song`]'s eight tracks step
//! by step and gives what each voice is handed (notes, releases, pitch
//! and volume changes) and the song-wide settings, on every step. Sound
//! effects (`spec/formats/sound-effects.md`) play in the same player, each
//! taking a voice from the song while it plays.
//!
//! How the voices turn those settings into sound is in
//! [`crate::voice_output`], and [`crate::music`] plays a whole song as
//! audio.

use crate::Location;
use crate::sound::{Read, Song};

/// Channels in a song, each playing its track on the voice of the same
/// number.
pub const CHANNELS: usize = 8;

/// Bytes at the start of song data before the tracks: the track
/// addresses, the tempo and a second tempo.
pub const HEADER_SIZE: usize = 18;

/// The period when a song starts.
pub const START_PERIOD: u8 = 60;

/// Audio outputs a step lasts per unit of period.
pub const OUTPUTS_PER_PERIOD: u32 = 4;

/// The gain setting every note is handed.
pub const GAIN: u8 = 127;

/// Pitch at which a voice plays its sample as stored.
pub const UNIT_PITCH: u16 = 4096;

/// Calls inside calls the spec covers.
const MAX_CALLS: usize = 8;

/// The player's tracks: the song's channels, then an effect slot for each
/// voice (slot `v` is track [`CHANNELS`] + `v`).
const TRACKS: usize = 2 * CHANNELS;

/// The effect block (`sound-effects.md`): laid out like a song block, its
/// data starting with the effect table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effects {
    /// The effect data, at its base address.
    pub block: Song,
    /// Where each effect's track starts in the data.
    pub tracks: Vec<usize>,
}

impl Effects {
    /// Reads the block that starts at `bytes`.
    pub fn read(bytes: &[u8]) -> Result<Effects, String> {
        let block = Song::read(bytes).map_err(|e| format!("the effect block: {e}"))?;
        // The table runs up to the first track, the lowest address in it.
        let mut tracks = Vec::new();
        let mut first = block.data.len();
        while 2 * tracks.len() < first {
            let at = 2 * tracks.len();
            let address = block
                .data
                .get(at..at + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .ok_or("the effect table runs past the end of the effect data")?;
            let track = offset(&block, address)?;
            first = first.min(track);
            tracks.push(track);
        }
        if 2 * tracks.len() != first {
            return Err("an effect's track starts inside the effect table".into());
        }
        Ok(Effects { block, tracks })
    }
}

/// Reads the effect block at `at` (`locators.md`, "Sound: sound-effect
/// block").
pub fn read_effects<'a>(read: impl Read<'a>, at: Location) -> Result<Effects, String> {
    let bytes = read(at).ok_or("the effect block isn't in the image")?;
    Effects::read(bytes)
}

/// A byte count where 0 means 256.
fn count(n: u8) -> u16 {
    if n == 0 { 256 } else { u16::from(n) }
}

/// What a voice has been handed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Voice {
    pub sample: u8,
    /// Sample outputs per audio output, in 1/[`UNIT_PITCH`]s.
    pub pitch: u16,
    /// Left, right.
    pub volume: [u8; 2],
    /// `e1`, `e2`.
    pub envelope: [u8; 2],
}

/// A voice on one step: what it has been handed by the end of the step,
/// and what the song did to it during the step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoiceStep {
    pub voice: Voice,
    pub echo: bool,
    pub noise: bool,
    /// The note was let go.
    pub released: bool,
    /// A note started, with the voice's values as they are at the end of
    /// the step.
    pub started: bool,
    /// Set this step without a note starting (rests, effects, presets).
    pub pitch_set: bool,
    pub volume_set: bool,
    pub envelope_set: bool,
    /// Set this step, even to what it was.
    pub echo_set: bool,
    pub noise_set: bool,
}

/// The song-wide settings that reach the sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub period: u8,
    pub main_volume: [u8; 2],
    pub echo_volume: [u8; 2],
    pub echo_feedback: u8,
    pub echo_filter: [u8; 8],
    pub noise_rate: u8,
    pub echo_writing: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            period: START_PERIOD,
            main_volume: [127, 127],
            echo_volume: [0, 0],
            echo_feedback: 0,
            // The spec leaves the filter values before a command 24 to
            // the game; the engine starts them at 0.
            echo_filter: [0; 8],
            noise_rate: 0,
            echo_writing: false,
        }
    }
}

/// Which song-wide settings the song set on a step, even to what they
/// were.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsSet {
    pub main_volume: bool,
    pub echo_volume: bool,
    pub echo_feedback: bool,
    pub echo_filter: bool,
    pub noise_rate: bool,
    pub echo_writing: bool,
}

/// One step of a song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// Whether the song's tracks moved on.
    pub beat: bool,
    /// Whether the effects' tracks moved on.
    pub effect_beat: bool,
    pub voices: [VoiceStep; CHANNELS],
    /// As they are at the end of the step; the period sets the gap after
    /// it.
    pub settings: Settings,
    pub set: SettingsSet,
}

impl Step {
    /// Audio outputs from this step to the next.
    pub fn outputs(&self) -> u32 {
        OUTPUTS_PER_PERIOD * u32::from(self.settings.period)
    }
}

/// A change to a pitch or a volume: add `amount`, or take it off.
#[derive(Clone, Copy)]
struct Change {
    amount: i8,
    off: bool,
}

impl Change {
    fn to_pitch(self, pitch: u16) -> u16 {
        let amount = self.amount as i16 as u16;
        if self.off {
            pitch.wrapping_sub(amount)
        } else {
            pitch.wrapping_add(amount)
        }
    }

    fn to_volume(self, volume: u8) -> u8 {
        let amount = self.amount as u8;
        if self.off {
            volume.wrapping_sub(amount)
        } else {
            volume.wrapping_add(amount)
        }
    }
}

/// The pitch slide: its settings (`w`, `r`, `n`, amount, `f`) and
/// counters.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Slide {
    on: bool,
    wait: u8,
    every: u8,
    changes: u8,
    amount: i8,
    down: u8,
    /// Steps to the next change, counting the step it's on; 0 for none.
    countdown: u16,
    made: u16,
}

impl Slide {
    fn restart(&mut self) {
        self.countdown = match self.wait {
            0 => count(self.every),
            255 => 0,
            w => u16::from(w),
        };
        self.made = 0;
    }

    fn run(&mut self) -> Option<Change> {
        if !self.on || self.countdown == 0 || self.made >= count(self.changes) {
            return None;
        }
        self.countdown -= 1;
        if self.countdown > 0 {
            return None;
        }
        self.countdown = count(self.every);
        let off = self.made < u16::from(self.down);
        self.made += 1;
        Some(Change {
            amount: self.amount,
            off,
        })
    }
}

/// The vibrato or the volume sweep: settings (`N`, `r`, `d`, `w`) and
/// counters.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Swing {
    on: bool,
    turn: u8,
    every: u8,
    amount: i8,
    wait: u8,
    /// Steps to the next change, counting the step it's on; 0 for none.
    countdown: u16,
    /// Changes until the sign turns.
    left: u16,
    off: bool,
}

impl Swing {
    fn set(&mut self, args: &[u8], wait: u8) {
        self.on = true;
        self.turn = args[0];
        self.every = args[1];
        self.amount = args[2] as i8;
        self.wait = wait;
    }

    fn restart(&mut self) {
        self.amount = self.amount.wrapping_abs();
        self.countdown = u16::from(self.wait) + count(self.every);
        self.left = count(self.turn / 2);
        self.off = false;
    }

    fn run(&mut self) -> Option<Change> {
        if !self.on || self.countdown == 0 {
            return None;
        }
        self.countdown -= 1;
        if self.countdown > 0 {
            return None;
        }
        self.countdown = count(self.every);
        let change = Change {
            amount: self.amount,
            off: self.off,
        };
        self.left -= 1;
        if self.left == 0 {
            self.off = !self.off;
            self.left = count(self.turn);
        }
        Some(change)
    }
}

/// A call (command 4): where it goes, how many plays are left, and where
/// it returns to.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Call {
    to: usize,
    left: u16,
    back: usize,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Channel {
    at: usize,
    ended: bool,
    wait: u32,
    calls: Vec<Call>,
    two_byte: bool,
    fixed: [u8; 2],
    fine: i8,
    transpose: u8,
    pitch: u16,
    sample: u8,
    volume: [u8; 2],
    saved_volume: [u8; 2],
    envelope: [u8; 2],
    slide: Slide,
    vibrato: Swing,
    sweep: Swing,
    /// Set by commands 22 and 23.
    echo: bool,
}

/// Song-wide state beyond [`Settings`].
#[derive(Clone, Default, PartialEq, Eq)]
struct Wide {
    tempo: u8,
    tempo_sum: u16,
    effect_tempo: u8,
    effect_sum: u16,
    main_base: [u8; 2],
    echo_base: [u8; 2],
    fade: u8,
    fade_speed: u8,
    branch: u8,
    /// The spec doesn't say whether presets are kept per channel; the
    /// engine keeps one set for the song.
    presets: [[u8; 4]; 5],
}

/// Plays a song, and effects over it: see [`Player::step`] and
/// [`Player::start_effect`].
#[derive(Clone)]
pub struct Player<'a> {
    song: &'a Song,
    effects: Option<&'a Effects>,
    channels: [Channel; TRACKS],
    /// The voices an effect slot plays on.
    taken: [bool; CHANNELS],
    /// Whether an effect was started since the last step.
    started: bool,
    voices: [VoiceStep; CHANNELS],
    settings: Settings,
    wide: Wide,
}

impl<'a> Player<'a> {
    /// Starts `song` from its header. What a voice holds before a song
    /// sets it, and effect counters the song hasn't restarted, are left
    /// by whatever played before in the game; the engine starts them at
    /// 0.
    pub fn new(song: &'a Song) -> Result<Player<'a>, String> {
        Player::with_effects(song, None)
    }

    /// Starts `song` as [`Player::new`] does, with `effects` to start
    /// effects from.
    pub fn with_effects(
        song: &'a Song,
        effects: Option<&'a Effects>,
    ) -> Result<Player<'a>, String> {
        let header = song
            .data
            .get(..HEADER_SIZE)
            .ok_or("a song is shorter than its header")?;
        let mut channels: [Channel; TRACKS] = Default::default();
        for (i, channel) in channels[..CHANNELS].iter_mut().enumerate() {
            let address = u16::from_le_bytes([header[2 * i], header[2 * i + 1]]);
            channel.at = offset(song, address)?;
            channel.wait = 1;
        }
        Ok(Player {
            song,
            effects,
            channels,
            taken: [false; CHANNELS],
            started: false,
            voices: Default::default(),
            settings: Settings::default(),
            wide: Wide {
                tempo: header[16],
                effect_tempo: header[17],
                main_base: [127, 127],
                ..Wide::default()
            },
        })
    }

    /// Starts `song` in place of the one playing. No voice is taken
    /// afterwards; the effect slots and the channels' echo flags are kept,
    /// and so is what the voices were handed.
    pub fn start_song(&mut self, song: &'a Song) -> Result<(), String> {
        let mut next = Player::with_effects(song, self.effects)?;
        next.channels[CHANNELS..].clone_from_slice(&self.channels[CHANNELS..]);
        for (channel, old) in next.channels.iter_mut().zip(&self.channels[..CHANNELS]) {
            channel.echo = old.echo;
        }
        next.voices = self.voices;
        *self = next;
        Ok(())
    }

    /// Whether every one of the song's tracks has ended.
    pub fn ended(&self) -> bool {
        self.channels[..CHANNELS].iter().all(|c| c.ended)
    }

    /// Whether this player is where `other` is: every track, effect slot,
    /// voice and setting as there, so that from here on both play the
    /// same steps. Which song and effects they play isn't compared.
    pub fn same_place(&self, other: &Player) -> bool {
        self.channels == other.channels
            && self.taken == other.taken
            && self.started == other.started
            && self.voices == other.voices
            && self.settings == other.settings
            && self.wide == other.wide
    }

    /// Starts effect `effect` on voice `voice`, between the last step and
    /// the next. At most one effect starts between two steps.
    pub fn start_effect(&mut self, effect: u8, voice: usize) -> Result<(), String> {
        let effects = self
            .effects
            .ok_or("there is no effect block to start effects from")?;
        let at = *effects
            .tracks
            .get(usize::from(effect))
            .ok_or_else(|| format!("effect {effect} isn't in the effect table"))?;
        if voice >= CHANNELS {
            return Err(format!("there is no voice {voice}"));
        }
        if self.started {
            return Err("only one effect can start between two steps".into());
        }
        self.started = true;
        self.taken[voice] = true;
        self.voices[voice].noise = false;
        self.voices[voice].echo = false;
        // The sample, the current pitch and the effects' settings and
        // counters are kept from whatever last played in the slot.
        let slot = &mut self.channels[CHANNELS + voice];
        *slot = Channel {
            at,
            wait: 2,
            pitch: slot.pitch,
            sample: slot.sample,
            volume: [127, 127],
            saved_volume: [127, 127],
            slide: Slide {
                on: false,
                ..slot.slide
            },
            vibrato: Swing {
                on: false,
                ..slot.vibrato
            },
            sweep: Swing {
                on: false,
                ..slot.sweep
            },
            ..Channel::default()
        };
        Ok(())
    }

    /// Plays the next step.
    pub fn step(&mut self) -> Result<Step, String> {
        for voice in &mut self.voices {
            *voice = VoiceStep {
                voice: voice.voice,
                echo: voice.echo,
                noise: voice.noise,
                ..VoiceStep::default()
            };
        }
        self.started = false;
        let mut set = SettingsSet::default();

        self.wide.tempo_sum += u16::from(self.wide.tempo);
        let beat = self.wide.tempo_sum >= 256;
        if beat {
            self.wide.tempo_sum -= 256;
            self.fade(&mut set);
        }
        self.wide.effect_sum += u16::from(self.wide.effect_tempo);
        let effect_beat = self.wide.effect_sum >= 256;
        if effect_beat {
            self.wide.effect_sum -= 256;
        }

        for i in 0..CHANNELS {
            self.run(i, beat, &mut set)?;
            if self.taken[i] {
                self.run(CHANNELS + i, effect_beat, &mut set)?;
            }
        }

        Ok(Step {
            beat,
            effect_beat,
            voices: self.voices,
            settings: self.settings,
            set,
        })
    }

    /// Runs track `i` for a step; `beat` is whether its tracks move on.
    fn run(&mut self, i: usize, beat: bool, set: &mut SettingsSet) -> Result<(), String> {
        if beat && !self.channels[i].ended {
            let channel = &mut self.channels[i];
            channel.wait -= 1;
            match channel.wait {
                1 => {
                    if self.hands(i) {
                        self.voices[i % CHANNELS].released = true;
                    }
                }
                0 => self.read(i, set)?,
                _ => {}
            }
        }
        // An ended track's effects only run on steps that aren't beats;
        // it can only end on a beat.
        if !(beat && self.channels[i].ended) {
            self.effects(i);
        }
        Ok(())
    }

    /// Whether track `i` hands its voice what it plays: a song's channel
    /// doesn't while an effect has taken its voice.
    fn hands(&self, i: usize) -> bool {
        i >= CHANNELS || !self.taken[i]
    }

    /// Where track `i` reads from.
    fn source(&self, i: usize) -> &'a Song {
        match self.effects {
            Some(effects) if i >= CHANNELS => &effects.block,
            _ => self.song,
        }
    }

    /// Effect slot `v` has read its track's end.
    fn end_effect(&mut self, v: usize) {
        self.taken[v] = false;
        let voice = &mut self.voices[v];
        voice.noise = false;
        voice.noise_set = true;
        voice.echo = self.channels[v].echo;
        voice.echo_set = true;
    }

    fn fade(&mut self, set: &mut SettingsSet) {
        let wide = &mut self.wide;
        if wide.fade_speed == 0 || wide.fade == 255 {
            return;
        }
        wide.fade = wide.fade.saturating_add(wide.fade_speed);
        let less = |base: [u8; 2]| base.map(|v| v.saturating_sub(wide.fade));
        self.settings.main_volume = less(wide.main_base);
        self.settings.echo_volume = less(wide.echo_base);
        set.main_volume = true;
        set.echo_volume = true;
    }

    fn effects(&mut self, i: usize) {
        let hands = self.hands(i);
        let channel = &mut self.channels[i];
        let voice = &mut self.voices[i % CHANNELS];
        let mut pitch_set = false;
        if let Some(change) = channel.slide.run() {
            channel.pitch = change.to_pitch(channel.pitch);
            pitch_set = true;
        }
        if let Some(change) = channel.vibrato.run() {
            channel.pitch = change.to_pitch(channel.pitch);
            pitch_set = true;
        }
        let volume_set = match channel.sweep.run() {
            Some(change) => {
                channel.volume = channel.volume.map(|v| change.to_volume(v));
                true
            }
            None => false,
        };
        if hands && pitch_set {
            voice.voice.pitch = channel.pitch;
            voice.pitch_set = true;
        }
        if hands && volume_set {
            voice.voice.volume = channel.volume;
            voice.volume_set = true;
        }
    }

    /// Byte `at` of the song's data.
    fn byte(&self, at: usize) -> Result<u8, String> {
        byte(self.song, at)
    }

    /// `n` bytes at the track's position, moving past them.
    fn take(&mut self, i: usize, n: usize) -> Result<&'a [u8], String> {
        let song = self.source(i);
        let at = self.channels[i].at;
        let bytes = song
            .data
            .get(at..at + n)
            .ok_or("a song's track runs past the end of its data")?;
        self.channels[i].at += n;
        Ok(bytes)
    }

    fn address(&mut self, i: usize) -> Result<usize, String> {
        let bytes = self.take(i, 2)?;
        offset(self.source(i), u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn length(&mut self, i: usize) -> Result<u32, String> {
        let channel = &self.channels[i];
        let length = if channel.fixed[1] != 0 {
            256 * u32::from(channel.fixed[0]) + u32::from(channel.fixed[1])
        } else if channel.two_byte {
            let bytes = self.take(i, 2)?;
            256 * u32::from(bytes[0]) + u32::from(bytes[1])
        } else {
            u32::from(self.take(i, 1)?[0])
        };
        Ok(length.max(1))
    }

    /// Reads track `i`'s items up to its next note or rest, or the end of
    /// its track.
    fn read(&mut self, i: usize, set: &mut SettingsSet) -> Result<(), String> {
        let hands = self.hands(i);
        let v = i % CHANNELS;
        loop {
            let b = self.take(i, 1)?[0];
            match b {
                0 => {
                    self.voices[v].released |= hands;
                    self.channels[i].ended = true;
                    if i >= CHANNELS {
                        self.end_effect(v);
                    }
                    return Ok(());
                }
                1..=48 => self.command(i, b, set)?,
                49..=127 => return Err(format!("song item {b} isn't covered by the spec")),
                128 => {
                    self.channels[i].wait = self.length(i)?;
                    if hands {
                        let voice = &mut self.voices[v];
                        voice.released = true;
                        voice.voice.pitch = 0;
                        voice.pitch_set = true;
                    }
                    return Ok(());
                }
                _ => {
                    self.channels[i].wait = self.length(i)?;
                    // A channel whose voice is taken keeps its pitch, and
                    // its effects don't restart.
                    if hands {
                        self.note(i, b - 128)?;
                    }
                    return Ok(());
                }
            }
        }
    }

    fn note(&mut self, i: usize, k: u8) -> Result<(), String> {
        let channel = &self.channels[i];
        let m = k.wrapping_add(channel.transpose) % 128;
        let p = match m {
            21 => 1626,
            28 => 2436,
            1..=60 => (4096.0 * 2f64.powf((f64::from(m) - 37.0) / 12.0)).round() as u16,
            61 => 16_383,
            0 | 62..=92 => 0,
            _ => {
                let at = 2 * usize::from(m) - 186;
                u16::from_le_bytes([self.byte(at)?, self.byte(at + 1)?])
            }
        };
        let fine = channel.fine;
        let d = (u32::from(p) * u32::from(fine.unsigned_abs()) / 1024) as u16;
        let pitch = if fine > 0 {
            p.wrapping_add(d)
        } else {
            p.wrapping_sub(d)
        };

        let channel = &mut self.channels[i];
        channel.pitch = pitch;
        let voice = &mut self.voices[i % CHANNELS];
        voice.voice = Voice {
            sample: channel.sample,
            pitch,
            volume: channel.volume,
            envelope: channel.envelope,
        };
        voice.started = true;
        if channel.slide.on {
            channel.slide.restart();
        }
        if channel.vibrato.on {
            channel.vibrato.restart();
        }
        if channel.sweep.on {
            channel.sweep.restart();
            channel.volume = channel.saved_volume;
        }
        Ok(())
    }

    fn command(&mut self, i: usize, b: u8, set: &mut SettingsSet) -> Result<(), String> {
        match b {
            1 => {
                let s = self.take(i, 1)?[0];
                let channel = &mut self.channels[i];
                channel.sample = s;
                channel.fine = 0;
            }
            2 => {
                let args = self.take(i, 2)?;
                self.channels[i].volume = [args[0], args[1]];
            }
            3 => self.channels[i].at = self.address(i)?,
            4 => {
                let n = self.take(i, 1)?[0];
                let to = self.address(i)?;
                let channel = &mut self.channels[i];
                if channel.calls.len() == MAX_CALLS {
                    return Err("a song's calls go deeper than the spec covers".into());
                }
                channel.calls.push(Call {
                    to,
                    left: count(n),
                    back: channel.at,
                });
                channel.at = to;
            }
            5 => {
                let channel = &mut self.channels[i];
                let call = channel
                    .calls
                    .last_mut()
                    .ok_or("a song returns from a call it isn't in")?;
                call.left -= 1;
                channel.at = if call.left > 0 {
                    call.to
                } else {
                    let back = call.back;
                    channel.calls.pop();
                    back
                };
            }
            6 => {
                if self.channels[i].two_byte {
                    let args = self.take(i, 2)?;
                    self.channels[i].fixed = [args[0], args[1]];
                } else {
                    self.channels[i].fixed[1] = self.take(i, 1)?[0];
                }
            }
            7 => self.channels[i].fixed = [0, 0],
            8 | 9 | 38 | 39 => {
                let args = self.take(i, 5 - usize::from(b >= 38))?;
                let slide = &mut self.channels[i].slide;
                slide.on = true;
                slide.wait = args[0];
                slide.every = args[1];
                let (changes, down, amount) = if b >= 38 {
                    (args[2].wrapping_mul(2), args[2], args[3] as i8)
                } else {
                    (args[2], args[4], args[3] as i8)
                };
                slide.changes = changes;
                slide.down = down;
                // −(−128) stays −128.
                slide.amount = if b == 9 || b == 38 {
                    amount.wrapping_neg()
                } else {
                    amount
                };
            }
            10 => self.channels[i].slide.on = false,
            11 => self.wide.tempo = self.take(i, 1)?[0],
            12 => self.wide.tempo = self.wide.tempo.wrapping_add(self.take(i, 1)?[0]),
            13 => {
                let args = self.take(i, 3)?;
                self.channels[i].vibrato.set(args, 0);
            }
            14 => self.channels[i].vibrato.on = false,
            15 => {
                let args = self.take(i, 4)?;
                self.channels[i].vibrato.set(args, args[3]);
            }
            16 => {
                let args = self.take(i, 2)?;
                self.channels[i].envelope = [args[0], args[1]];
            }
            17 => {
                let args = self.take(i, 2)?;
                self.wide.main_base = [args[0], args[1]];
                self.settings.main_volume = [args[0], args[1]];
                set.main_volume = true;
            }
            18 => self.channels[i].fine = self.take(i, 1)?[0] as i8,
            19 => self.channels[i].transpose = self.take(i, 1)?[0],
            20 => {
                let t = self.take(i, 1)?[0];
                let channel = &mut self.channels[i];
                channel.transpose = channel.transpose.wrapping_add(t);
            }
            21 => {
                let args = self.take(i, 3)?;
                let settings = &mut self.settings;
                settings.echo_feedback = args[0];
                settings.echo_volume = [args[1], args[2]];
                settings.echo_writing = true;
                settings.noise_rate = 0;
                self.wide.echo_base = [args[1], args[2]];
                set.echo_feedback = true;
                set.echo_volume = true;
                set.echo_writing = true;
                set.noise_rate = true;
            }
            22 | 23 => {
                let voice = &mut self.voices[i % CHANNELS];
                voice.echo = b == 22;
                voice.echo_set = true;
                self.channels[i].echo = b == 22;
            }
            24 => {
                let args = self.take(i, 8)?;
                self.settings.echo_filter.copy_from_slice(args);
                set.echo_filter = true;
            }
            25 => {
                // It keeps echo writing as it is, but sets it again.
                self.settings.noise_rate = self.take(i, 1)?[0];
                set.noise_rate = true;
                set.echo_writing = true;
            }
            26 | 27 => {
                let voice = &mut self.voices[i % CHANNELS];
                voice.noise = b == 26;
                voice.noise_set = true;
            }
            28..=32 => {
                let args = self.take(i, 4)?;
                self.wide.presets[usize::from(b - 28)].copy_from_slice(args);
            }
            33..=37 => {
                if !self.hands(i) {
                    return Err("a preset from a channel whose voice is taken \
                         isn't covered by the spec"
                        .into());
                }
                let [l, r, e1, e2] = self.wide.presets[usize::from(b - 33)];
                let voice = &mut self.voices[i % CHANNELS];
                voice.voice.volume = [l, r];
                voice.voice.envelope = [e1, e2];
                voice.volume_set = true;
                voice.envelope_set = true;
            }
            40 => {
                let args = self.take(i, 3)?;
                let channel = &mut self.channels[i];
                channel.sample = args[0];
                channel.volume = [args[1], args[2]];
                channel.fine = 0;
            }
            41 => self.wide.fade_speed = self.take(i, 1)?[0],
            42 => {
                let period = self.take(i, 1)?[0];
                if period == 0 {
                    return Err("a song's period of 0 isn't covered by the spec".into());
                }
                self.settings.period = period;
            }
            43 => self.channels[i].two_byte = true,
            44 => self.channels[i].two_byte = false,
            45 => {
                let source = self.source(i);
                let at = self.channels[i].at + 2 * usize::from(self.wide.branch);
                let address = u16::from_le_bytes([byte(source, at)?, byte(source, at + 1)?]);
                self.channels[i].at = offset(source, address)?;
            }
            46 => self.wide.branch = self.take(i, 1)?[0],
            47 => {
                let args = self.take(i, 4)?;
                let channel = &mut self.channels[i];
                channel.sweep.set(args, args[3]);
                channel.saved_volume = channel.volume;
            }
            48 => self.channels[i].sweep.on = false,
            _ => unreachable!("command {b}"),
        }
        Ok(())
    }
}

/// Byte `at` of `song`'s data.
fn byte(song: &Song, at: usize) -> Result<u8, String> {
    song.data
        .get(at)
        .copied()
        .ok_or_else(|| "a song's track runs past the end of its data".to_string())
}

/// Where song address `address` is in the song's data.
fn offset(song: &Song, address: u16) -> Result<usize, String> {
    address
        .checked_sub(song.base)
        .map(usize::from)
        .filter(|&at| at < song.data.len())
        .ok_or_else(|| "a song refers to a place outside its data".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `name` in `spec/vectors/{dir}`.
    fn vector(dir: &str, name: &str) -> String {
        let path = format!(
            "{}/../../spec/vectors/{dir}/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// A block's bytes, checking each line's position.
    fn block(dir: &str, name: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for line in vector(dir, name).lines().filter(|l| !l.trim().is_empty()) {
            let mut fields = line.split_whitespace();
            let at = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
            assert_eq!(at, bytes.len(), "{line}");
            bytes.extend(fields.map(|b| u8::from_str_radix(b, 16).unwrap()));
        }
        bytes
    }

    fn join<T: ToString>(values: &[T]) -> String {
        values
            .iter()
            .map(T::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn on(on: bool) -> String {
        if on { "on" } else { "off" }.to_string()
    }

    /// The rows of `events.tsv` in `spec/vectors/{dir}`.
    fn events(dir: &str) -> Vec<String> {
        vector(dir, "events.tsv")
            .lines()
            .skip(1)
            .map(str::to_string)
            .collect()
    }

    /// The rows for a step as the vectors list them: step, voice, event,
    /// values. `starts` are the effects started on voices after the step;
    /// `effect_beats` is whether to list effect beats.
    fn rows(
        n: usize,
        step: &Step,
        period: Option<u8>,
        starts: &[(usize, u8)],
        effect_beats: bool,
        rows: &mut Vec<String>,
    ) {
        let mut row = |voice: String, event: &str, values: String| {
            rows.push(format!("{n}\t{voice}\t{event}\t{values}"));
        };
        if let Some(period) = period {
            row("-".into(), "period", period.to_string());
        }
        for (i, v) in step.voices.iter().enumerate() {
            let voice = &v.voice;
            let i = i.to_string();
            if v.released {
                row(i.clone(), "release", String::new());
            }
            if v.started {
                row(
                    i.clone(),
                    "note",
                    format!(
                        "{} {} {} {}",
                        voice.pitch,
                        voice.sample,
                        join(&voice.volume),
                        join(&voice.envelope)
                    ),
                );
            } else {
                if v.pitch_set {
                    row(i.clone(), "pitch", voice.pitch.to_string());
                }
                if v.volume_set {
                    row(i.clone(), "volume", join(&voice.volume));
                }
                if v.envelope_set {
                    row(i.clone(), "envelope", join(&voice.envelope));
                }
            }
            for (_, effect) in starts.iter().filter(|(voice, _)| voice.to_string() == i) {
                row(i.clone(), "start", effect.to_string());
            }
            if v.echo_set {
                row(i.clone(), "echo", on(v.echo));
            }
            if v.noise_set {
                row(i.clone(), "noise", on(v.noise));
            }
        }
        let (s, set) = (&step.settings, &step.set);
        let wide = |event: &str, values: String| (event.to_string(), values);
        let all = [
            (set.main_volume, wide("main-volume", join(&s.main_volume))),
            (set.echo_volume, wide("echo-volume", join(&s.echo_volume))),
            (
                set.echo_feedback,
                wide("echo-feedback", s.echo_feedback.to_string()),
            ),
            (set.echo_filter, wide("echo-filter", join(&s.echo_filter))),
            (set.noise_rate, wide("noise-rate", s.noise_rate.to_string())),
            (set.echo_writing, wide("echo-writing", on(s.echo_writing))),
        ];
        for (_, (event, values)) in all.into_iter().filter(|(set, _)| *set) {
            row("-".into(), &event, values);
        }
        if step.beat {
            row("-".into(), "beat", String::new());
        }
        if effect_beats && step.effect_beat {
            row("-".into(), "effect-beat", String::new());
        }
    }

    fn same_rows(got: &[String], want: &[String]) {
        for (k, (got, want)) in got.iter().zip(want).enumerate() {
            assert_eq!(got, want, "row {}", k + 2);
        }
        assert_eq!(got.len(), want.len());
    }

    #[test]
    fn steps_match_the_spec_vectors() {
        let song = Song::read(&block("songs", "block.hex")).unwrap();
        assert_eq!(song.base, 0x3000);
        let mut player = Player::new(&song).unwrap();
        let mut got = Vec::new();
        let mut period = None;
        for n in 1..=900 {
            let step = player.step().unwrap();
            let changed = (period != Some(step.settings.period)).then_some(step.settings.period);
            period = Some(step.settings.period);
            rows(n, &step, changed, &[], false, &mut got);
        }
        same_rows(&got, &events("songs"));
    }

    #[test]
    fn effects_match_the_spec_vectors() {
        let song = Song::read(&block("sound-effects", "song.hex")).unwrap();
        let effects = Effects::read(&block("sound-effects", "effects.hex")).unwrap();
        assert_eq!(effects.block.base, 0x5000);
        assert_eq!(effects.tracks.len(), 6);
        let starts: Vec<(usize, u8, usize)> = vector("sound-effects", "starts.tsv")
            .lines()
            .skip(1)
            .map(|l| {
                let f: Vec<usize> = l.split('\t').map(|f| f.parse().unwrap()).collect();
                (f[0], f[1] as u8, f[2])
            })
            .collect();
        let mut player = Player::with_effects(&song, Some(&effects)).unwrap();
        let mut got = Vec::new();
        let mut period = None;
        for n in 1..=700 {
            let mut step = player.step().unwrap();
            let changed = (period != Some(step.settings.period)).then_some(step.settings.period);
            period = Some(step.settings.period);
            let mut started = Vec::new();
            for &(_, effect, voice) in starts.iter().filter(|s| s.0 == n) {
                player.start_effect(effect, voice).unwrap();
                let v = &mut step.voices[voice];
                (v.echo, v.noise, v.echo_set, v.noise_set) = (false, false, true, true);
                started.push((voice, effect));
            }
            rows(n, &step, changed, &started, true, &mut got);
        }
        same_rows(&got, &events("sound-effects"));
    }

    /// A song with the header's tracks at `tracks` and the rest of the
    /// data after it.
    fn song(tempo: u8, tracks: [u16; CHANNELS], rest: &[u8]) -> Song {
        let mut data: Vec<u8> = tracks.iter().flat_map(|t| t.to_le_bytes()).collect();
        data.extend([tempo, 0]);
        data.extend(rest);
        Song { base: 0, data }
    }

    #[test]
    fn the_spec_example() {
        // Every track but channel 0's ends at once, at byte 18.
        let track = [
            0, 1, 3, 2, 64, 64, 16, 143, 224, 13, 4, 2, 10, 165, 2, 128, 1, 0,
        ];
        let mut tracks = [18; CHANNELS];
        tracks[0] = 19;
        let song = song(128, tracks, &track);
        let mut player = Player::new(&song).unwrap();
        let steps: Vec<Step> = (0..20).map(|_| player.step().unwrap()).collect();
        let v = |n: usize| steps[n - 1].voices[0];

        assert!(!v(1).started && !v(1).released);
        assert!(steps[1].beat && v(2).started);
        assert_eq!(
            v(2).voice,
            Voice {
                sample: 3,
                pitch: 4096,
                volume: [64, 64],
                envelope: [143, 224],
            }
        );
        assert_eq!(v(3).voice.pitch, 4106);
        assert!(v(4).released);
        assert_eq!(v(5).voice.pitch, 4116);
        assert!(v(6).released && v(6).voice.pitch == 0);
        assert_eq!(v(7).voice.pitch, 4106);
        assert!(v(8).released && !v(8).pitch_set && player.ended());
        assert!(!v(9).pitch_set && !v(10).pitch_set);
        let pitches: Vec<u16> = (11..=19)
            .filter(|&n| v(n).pitch_set)
            .map(|n| v(n).voice.pitch)
            .collect();
        assert_eq!(pitches, [4096, 4086, 4076]);
        assert_eq!(
            (11..=19).filter(|&n| v(n).pitch_set).collect::<Vec<_>>(),
            [11, 15, 19]
        );
    }

    #[test]
    fn note_pitches() {
        let mut tracks = [18; CHANNELS];
        tracks[0] = 19;
        let mut data = vec![0];
        for k in [1, 13, 25, 37, 49, 21, 28, 61, 62] {
            data.extend([128 + k, 1]);
        }
        let song = song(255, tracks, &data);
        let mut player = Player::new(&song).unwrap();
        let mut pitches = Vec::new();
        while pitches.len() < 9 {
            let step = player.step().unwrap();
            if step.voices[0].started {
                pitches.push(step.voices[0].voice.pitch);
            }
        }
        assert_eq!(
            pitches,
            [512, 1024, 2048, 4096, 8192, 1626, 2436, 16_383, 0]
        );
    }

    #[test]
    fn steps_last_four_outputs_per_period() {
        let song = song(1, [18; CHANNELS], &[0]);
        let mut player = Player::new(&song).unwrap();
        assert_eq!(player.step().unwrap().outputs(), 240);
    }

    #[test]
    fn bad_tracks_are_errors() {
        let song = song(255, [18; CHANNELS], &[60]);
        let mut player = Player::new(&song).unwrap();
        assert!(player.step().is_ok());
        assert!(player.step().is_err());
        assert!(Player::new(&song_with_track_at(0x9000)).is_err());
    }

    fn song_with_track_at(address: u16) -> Song {
        song(1, [address; CHANNELS], &[0])
    }

    /// An effect block at base 0 with `tracks` after a table pointing at
    /// each.
    fn effects(tracks: &[&[u8]]) -> Effects {
        let mut data = Vec::new();
        let mut at = 2 * tracks.len() as u16;
        for track in tracks {
            data.extend(at.to_le_bytes());
            at += track.len() as u16;
        }
        data.extend(tracks.concat());
        let mut block = (data.len() as u16).to_le_bytes().to_vec();
        block.extend([0, 0]);
        block.extend(data);
        Effects::read(&block).unwrap()
    }

    #[test]
    fn the_effect_table_runs_up_to_the_first_track() {
        let effects = effects(&[&[0], &[1, 2, 0], &[0]]);
        assert_eq!(effects.tracks, [6, 7, 10]);
        // An entry pointing into the table.
        let block = [4, 0, 0, 0, 1, 0, 0, 0];
        assert!(Effects::read(&block).is_err());
    }

    #[test]
    fn a_song_start_gives_back_the_voices() {
        // Channel 0 plays note 37 with a length of 2, over and over.
        let mut tracks = [18; CHANNELS];
        tracks[0] = 19;
        let mut tune = song(255, tracks, &[0, 165, 2, 3, 19, 0]);
        tune.data[17] = 255;
        // Sample 9, then a long note.
        let fx = effects(&[&[1, 9, 165, 200]]);
        let mut player = Player::with_effects(&tune, Some(&fx)).unwrap();
        player.step().unwrap();
        player.start_effect(0, 0).unwrap();
        assert!(player.start_effect(0, 1).is_err());
        player.step().unwrap();
        let step = player.step().unwrap();
        assert!(step.voices[0].started && step.voices[0].voice.sample == 9);
        // While taken, the song's own notes don't reach the voice.
        for _ in 0..4 {
            assert!(!player.step().unwrap().voices[0].started);
        }
        player.start_song(&tune).unwrap();
        let notes = (0..4).filter(|_| player.step().unwrap().voices[0].started);
        assert_eq!(notes.count(), 2);
        assert_eq!(player.channels[CHANNELS].sample, 9);
    }

    #[test]
    fn bad_effect_starts_are_errors() {
        let tune = song(1, [18; CHANNELS], &[0]);
        let fx = effects(&[&[0]]);
        assert!(Player::new(&tune).unwrap().start_effect(0, 0).is_err());
        let mut player = Player::with_effects(&tune, Some(&fx)).unwrap();
        assert!(player.start_effect(1, 0).is_err());
        assert!(player.start_effect(0, CHANNELS).is_err());
        assert!(player.start_effect(0, 7).is_ok());
    }
}
