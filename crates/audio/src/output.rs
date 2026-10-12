//! Sending the mixer to the system's default sound device.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, ErrorKind, FromSample, OutputCallbackInfo, SampleFormat, SizedSample, StreamConfig,
    SupportedBufferSize,
};

use crate::sound::Resampler;
use crate::{AudioError, Mixer};

/// Logic frames per second; device buffers aim to be no longer than one.
const LOGIC_RATE: u32 = 60;
/// How often to look for a device again after losing it.
const RETRY: Duration = Duration::from_millis(500);

/// Plays a [`Mixer`] on the default output device, from the device's own
/// audio thread. If the device goes away (headphones unplugged, a USB
/// interface switched off), the output reopens on whatever is the default
/// device then, retrying until there is one.
///
/// Sounds and music must be loaded at [`rate`](Self::rate), which stays
/// fixed for the life of the output; a device that can't play at that
/// rate after a reopen is fed through a resampler.
pub struct Output {
    shared: Arc<Shared>,
    rate: u32,
    events: Sender<Event>,
    thread: Option<JoinHandle<()>>,
}

/// What the output knows about the device it's playing on.
#[derive(Default)]
struct Shared {
    device: Mutex<String>,
    device_rate: AtomicU32,
    /// Frames in the latest device buffer.
    buffer: AtomicU32,
    /// Latest time from a buffer being filled to it being heard, in
    /// nanoseconds.
    latency: AtomicU64,
    reopened: AtomicU32,
}

enum Event {
    /// The stream of this generation stopped working.
    Lost(u64),
    Close,
}

impl Output {
    /// Opens the default output device and starts playing `mixer` on it,
    /// at the device's preferred rate.
    pub fn open(mixer: Mixer) -> Result<Output, AudioError> {
        let shared = Arc::new(Shared::default());
        let (events, rx) = channel();
        let (opened_tx, opened_rx) = channel();
        let thread = {
            let shared = shared.clone();
            let events = events.clone();
            std::thread::Builder::new()
                .name("audio output".into())
                .spawn(move || output_thread(mixer, shared, events, rx, opened_tx))
                .map_err(|e| AudioError(format!("can't start audio output thread: {e}")))?
        };
        let rate = opened_rx
            .recv()
            .map_err(|_| AudioError("audio output thread stopped".into()))??;
        Ok(Output {
            shared,
            rate,
            events,
            thread: Some(thread),
        })
    }

    /// The rate sounds and music are loaded at, in frames per second.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The device playing now, or an empty string while there's none.
    pub fn device(&self) -> String {
        self.shared.device.lock().unwrap().clone()
    }

    /// The device's rate, which differs from [`rate`](Self::rate) only if
    /// a reopen landed on a device that can't play at that rate.
    pub fn device_rate(&self) -> u32 {
        self.shared.device_rate.load(Ordering::Relaxed)
    }

    /// Frames in the latest buffer the device asked for, 0 before the
    /// first. At most `rate / 60` when the device allows it.
    pub fn buffer_frames(&self) -> u32 {
        self.shared.buffer.load(Ordering::Relaxed)
    }

    /// The latest time from the mixer filling a buffer to the device
    /// playing it, as the device reports it. A sound triggered on a frame
    /// is heard within about one buffer plus this.
    pub fn latency(&self) -> Duration {
        Duration::from_nanos(self.shared.latency.load(Ordering::Relaxed))
    }

    /// Times the device was lost and the output reopened.
    pub fn reopened(&self) -> u32 {
        self.shared.reopened.load(Ordering::Relaxed)
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = self.events.send(Event::Close);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Opens streams and reopens them when they fail. Owns the mixer between
/// streams.
fn output_thread(
    mixer: Mixer,
    shared: Arc<Shared>,
    events: Sender<Event>,
    rx: Receiver<Event>,
    opened: Sender<Result<u32, AudioError>>,
) {
    let mut feed = Feed::new(mixer);
    let mut generation = 0;
    let first = match open_stream(None, &mut feed, &shared, &events, generation) {
        Ok((stream, rate)) => {
            let _ = opened.send(Ok(rate));
            Some(stream)
        }
        Err(e) => {
            let _ = opened.send(Err(e));
            return;
        }
    };
    let mut stream = first;
    loop {
        let event = if stream.is_some() {
            rx.recv().unwrap_or(Event::Close)
        } else {
            match rx.recv_timeout(RETRY) {
                Ok(e) => e,
                Err(RecvTimeoutError::Timeout) => Event::Lost(generation),
                Err(RecvTimeoutError::Disconnected) => Event::Close,
            }
        };
        match event {
            Event::Close => return,
            // a late report from a stream already replaced
            Event::Lost(g) if g != generation => continue,
            Event::Lost(_) => {}
        }
        if let Some(s) = stream.take() {
            drop(s);
            shared.reopened.fetch_add(1, Ordering::Relaxed);
        } else {
            // keep time moving with no device, so music stays in step and
            // sounds triggered meanwhile don't all start at once later
            feed.discard(RETRY);
        }
        generation += 1;
        *shared.device.lock().unwrap() = String::new();
        let rate = feed.rate;
        match open_stream(Some(rate), &mut feed, &shared, &events, generation) {
            Ok((s, _)) => stream = Some(s),
            Err(_) => stream = None,
        }
    }
}

/// Builds and starts a stream on the default device. With `rate`, prefers
/// that rate; otherwise uses the device's own and makes it the mix rate.
/// Returns the stream and the mix rate.
fn open_stream(
    rate: Option<u32>,
    feed: &mut Feed,
    shared: &Arc<Shared>,
    events: &Sender<Event>,
    generation: u64,
) -> Result<(cpal::Stream, u32), AudioError> {
    let err = |e: cpal::Error| AudioError(format!("audio output: {e}"));
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| AudioError("no audio output device".into()))?;
    let default = device.default_output_config().map_err(err)?;
    let wanted = rate.and_then(|rate| {
        device
            .supported_output_configs()
            .ok()?
            .filter(|c| c.channels() == default.channels())
            .find_map(|c| c.try_with_sample_rate(rate))
    });
    let supported = wanted.unwrap_or(default);
    let mix_rate = rate.unwrap_or(supported.sample_rate());
    let device_rate = supported.sample_rate();
    let mut config = supported.config();
    let frames = device_rate / LOGIC_RATE;
    config.buffer_size = BufferSize::Fixed(match *supported.buffer_size() {
        SupportedBufferSize::Range { min, max } => frames.clamp(min, max),
        SupportedBufferSize::Unknown => frames,
    });

    let engine = Arc::new(Mutex::new(Engine::new(
        feed.take(),
        mix_rate,
        device_rate,
        usize::from(config.channels),
    )));
    let format = supported.sample_format();
    let stream =
        build(&device, config, format, &engine, shared, events, generation).or_else(|_| {
            // some devices refuse a fixed size; take theirs
            config.buffer_size = BufferSize::Default;
            build(&device, config, format, &engine, shared, events, generation)
        });
    let stream = match stream.and_then(|s| s.play().map(|()| s)) {
        Ok(s) => s,
        Err(e) => {
            feed.put_back(&engine);
            return Err(err(e));
        }
    };
    feed.engine = Some(engine);
    feed.rate = mix_rate;
    *shared.device.lock().unwrap() = device.to_string();
    shared.device_rate.store(device_rate, Ordering::Relaxed);
    Ok((stream, mix_rate))
}

fn build(
    device: &cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    engine: &Arc<Mutex<Engine>>,
    shared: &Arc<Shared>,
    events: &Sender<Event>,
    generation: u64,
) -> Result<cpal::Stream, cpal::Error> {
    match format {
        SampleFormat::F32 => build_as::<f32>(device, config, engine, shared, events, generation),
        SampleFormat::F64 => build_as::<f64>(device, config, engine, shared, events, generation),
        SampleFormat::I16 => build_as::<i16>(device, config, engine, shared, events, generation),
        SampleFormat::U16 => build_as::<u16>(device, config, engine, shared, events, generation),
        SampleFormat::I32 => build_as::<i32>(device, config, engine, shared, events, generation),
        SampleFormat::U32 => build_as::<u32>(device, config, engine, shared, events, generation),
        SampleFormat::I8 => build_as::<i8>(device, config, engine, shared, events, generation),
        SampleFormat::U8 => build_as::<u8>(device, config, engine, shared, events, generation),
        // anything rarer, ask for floats and let the device refuse
        _ => build_as::<f32>(device, config, engine, shared, events, generation),
    }
}

fn build_as<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: StreamConfig,
    engine: &Arc<Mutex<Engine>>,
    shared: &Arc<Shared>,
    events: &Sender<Event>,
    generation: u64,
) -> Result<cpal::Stream, cpal::Error> {
    let engine = engine.clone();
    let shared = shared.clone();
    let events = events.clone();
    device.build_output_stream(
        config,
        move |data: &mut [T], info: &OutputCallbackInfo| {
            let t = info.timestamp();
            let latency = t.playback.duration_since(t.callback);
            shared
                .latency
                .store(latency.as_nanos() as u64, Ordering::Relaxed);
            let mut engine = engine.lock().unwrap();
            let frames = data.len() / engine.channels;
            shared.buffer.store(frames as u32, Ordering::Relaxed);
            engine.fill(data);
        },
        move |e: cpal::Error| match e.kind() {
            // glitches and rerouting the stream recovers from by itself
            ErrorKind::Xrun | ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
            _ => {
                let _ = events.send(Event::Lost(generation));
            }
        },
        None,
    )
}

/// The mixer, wherever it is between the output thread and a stream.
struct Feed {
    engine: Option<Arc<Mutex<Engine>>>,
    idle: Option<Mixer>,
    rate: u32,
}

impl Feed {
    fn new(mixer: Mixer) -> Feed {
        Feed {
            engine: None,
            idle: Some(mixer),
            rate: 0,
        }
    }

    /// Takes the mixer back from the last stream, which must be dropped.
    fn take(&mut self) -> Mixer {
        if let Some(engine) = self.engine.take() {
            self.put_back(&engine);
        }
        self.idle.take().expect("the mixer is somewhere")
    }

    fn put_back(&mut self, engine: &Arc<Mutex<Engine>>) {
        if let Some(m) = engine.lock().unwrap().mixer.take() {
            self.idle = Some(m);
        }
    }

    /// Mixes `time` of audio into nothing.
    fn discard(&mut self, time: Duration) {
        if let Some(engine) = self.engine.take() {
            self.put_back(&engine);
        }
        if let Some(m) = &mut self.idle {
            let frames = (u128::from(self.rate) * time.as_millis() / 1000) as usize;
            let mut buf = vec![0.0; frames * 2];
            m.mix(&mut buf);
        }
    }
}

/// Turns the mixer's stereo output into a device's samples: converting
/// the rate if the device's differs, and spreading or folding channels.
struct Engine {
    mixer: Option<Mixer>,
    channels: usize,
    resampler: Option<Resampler>,
    /// Mix rate over device rate, as frames to mix per device frame.
    ratio: f64,
    /// Stereo frames at the mix rate.
    mixed: Vec<f32>,
    /// Stereo frames at the device rate, not yet played.
    pending: Vec<f32>,
}

impl Engine {
    fn new(mixer: Mixer, mix_rate: u32, device_rate: u32, channels: usize) -> Engine {
        Engine {
            mixer: Some(mixer),
            channels: channels.max(1),
            resampler: (mix_rate != device_rate).then(|| Resampler::new(mix_rate, device_rate, 2)),
            ratio: f64::from(mix_rate) / f64::from(device_rate),
            mixed: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Fills a device buffer of interleaved samples.
    fn fill<T: FromSample<f32>>(&mut self, data: &mut [T]) {
        let frames = data.len() / self.channels;
        let Some(mixer) = &mut self.mixer else {
            data.iter_mut().for_each(|s| *s = T::from_sample_(0.0));
            return;
        };
        match &mut self.resampler {
            None => {
                self.pending.resize(frames * 2, 0.0);
                mixer.mix(&mut self.pending);
            }
            Some(r) => {
                while self.pending.len() < frames * 2 {
                    let short = frames - self.pending.len() / 2;
                    let n = (short as f64 * self.ratio).ceil() as usize + 1;
                    self.mixed.resize(n * 2, 0.0);
                    mixer.mix(&mut self.mixed);
                    r.process(&self.mixed, &mut self.pending);
                }
            }
        }
        for (out, s) in data
            .chunks_exact_mut(self.channels)
            .zip(self.pending.as_chunks::<2>().0)
        {
            let &[l, r] = s;
            if self.channels == 1 {
                out[0] = T::from_sample_((l + r) * 0.5);
            } else {
                out[0] = T::from_sample_(l);
                out[1] = T::from_sample_(r);
                for o in &mut out[2..] {
                    *o = T::from_sample_(0.0);
                }
            }
        }
        self.pending.drain(..frames * 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Play, Sound, mixer};

    /// A mono sound with values 1, 2, 3... scaled down.
    fn ramp(frames: usize) -> Sound {
        Sound {
            channels: 1,
            samples: (1..=frames).map(|i| i as f32 / 1024.0).collect(),
        }
    }

    fn values(s: &[f32]) -> Vec<i32> {
        s.iter().map(|s| (s * 1024.0).round() as i32).collect()
    }

    fn playing(sound: &Sound, mix_rate: u32, device_rate: u32, channels: usize) -> Engine {
        let (mut handle, mixer) = mixer(4);
        handle.play(
            sound,
            Play {
                // full left, so the right channel tells channels apart
                pan: -1.0,
                volume: std::f32::consts::FRAC_1_SQRT_2,
                ..Play::default()
            },
        );
        // the handle may go; the queued command stays
        Engine::new(mixer, mix_rate, device_rate, channels)
    }

    #[test]
    fn same_rate_stereo_passes_through() {
        let mut e = playing(&ramp(4), 48000, 48000, 2);
        let mut out = [9.0f32; 10];
        e.fill(&mut out);
        assert_eq!(values(&out), [1, 0, 2, 0, 3, 0, 4, 0, 0, 0]);
    }

    #[test]
    fn mono_device_folds_and_wide_device_pads() {
        let mut mono = playing(&ramp(3), 48000, 48000, 1);
        let mut out = [0.0f32; 3];
        mono.fill(&mut out);
        assert_eq!(values(&out.map(|s| s * 2.0)), [1, 2, 3]);

        let mut wide = playing(&ramp(2), 48000, 48000, 4);
        let mut out = [9.0f32; 8];
        wide.fill(&mut out);
        assert_eq!(values(&out), [1, 0, 0, 0, 2, 0, 0, 0]);
    }

    #[test]
    fn integer_formats_convert() {
        let mut e = playing(&ramp(1), 48000, 48000, 2);
        let mut out = [0i16; 2];
        e.fill(&mut out);
        // 1/1024 of full scale, give or take rounding
        assert!((31..=32).contains(&out[0]), "{out:?}");
        assert_eq!(out[1], 0);
    }

    #[test]
    fn resampling_fills_every_buffer_exactly_and_continuously() {
        // 24 kHz mix, 48 kHz device: every mixed frame becomes two
        let mut e = playing(&ramp(64), 24000, 48000, 2);
        let mut left = Vec::new();
        for len in [7, 1, 30, 16, 50] {
            let mut out = vec![0.0f32; len * 2];
            e.fill(&mut out);
            // in half steps
            left.extend(out.chunks(2).map(|f| (f[0] * 2048.0).round() as i32));
        }
        let expected: Vec<i32> = (2..106).collect();
        assert_eq!(left, expected);
    }

    #[test]
    fn downsampling_keeps_pace() {
        let mut e = playing(&ramp(4800), 48000, 44100, 2);
        let mut played = 0;
        for _ in 0..60 {
            let mut out = vec![0.0f32; 735 * 2];
            e.fill(&mut out);
            played += 735;
        }
        // one second of device frames used one second of mixed frames,
        // give or take what's buffered
        assert_eq!(played, 44100);
        assert!(e.pending.len() / 2 <= 2, "{}", e.pending.len());
    }
}
