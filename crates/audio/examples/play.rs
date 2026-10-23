//! Plays a sound file on the default output device and reports the
//! output's rate, buffer size and latency.
//!
//! `cargo run -p clamber-audio --features output --example play -- FILE`

use std::time::Duration;

use clamber_audio::{Input, Output, Play, Sound, mixer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: play FILE")?;
    let (mut handle, mixer) = mixer(8);
    let output = Output::open(mixer, None)?;
    let sound = Sound::load(&Input::file(&path), output.rate())?;
    handle.play(&sound, Play::default());
    let length = sound.frames() as u64 * 1000 / u64::from(output.rate());
    std::thread::sleep(Duration::from_millis(length + 200));
    println!(
        "{} at {} Hz (device {} Hz): {} frames per buffer, {:.1} ms latency, reopened {} times",
        output.device(),
        output.rate(),
        output.device_rate(),
        output.buffer_frames(),
        output.latency().as_secs_f64() * 1000.0,
        output.reopened(),
    );
    Ok(())
}
