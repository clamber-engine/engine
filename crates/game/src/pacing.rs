//! The time between the last frames drawn, shown as a graph in the debug
//! view so frame pacing can be checked by eye.

use std::collections::VecDeque;
use std::time::Duration;

use crate::canvas::{Color, rgb};
use crate::surface::Surface;
use crate::text::{LINE, draw_text};

/// Frames graphed, one unit wide each.
pub const GRAPHED: usize = 120;

/// The graph's height in view units, a unit to a millisecond.
const HEIGHT: i32 = 40;

/// How far the graph is from the view's bottom-left corner.
const MARGIN: i32 = 4;

/// How far a frame can be from the time it's meant to take, as a share
/// of it, and still count as on time.
const ON_TIME: f64 = 0.1;

const BACK: Color = rgb(0x10, 0x10, 0x18);
const EXPECTED: Color = rgb(0xf4, 0xf0, 0xe8);
const GOOD: Color = rgb(0x60, 0xd0, 0x70);
const OFF: Color = rgb(0xf0, 0xb0, 0x50);
const LATE: Color = rgb(0xe0, 0x50, 0x40);
const TEXT: Color = rgb(0xf4, 0xf0, 0xe8);

/// The time between the last [`GRAPHED`] frames, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameTimes {
    times: VecDeque<Duration>,
}

impl FrameTimes {
    pub fn new() -> FrameTimes {
        FrameTimes::default()
    }

    /// Notes a frame drawn `interval` after the last.
    pub fn record(&mut self, interval: Duration) {
        if self.times.len() == GRAPHED {
            self.times.pop_front();
        }
        self.times.push_back(interval);
    }

    /// Forgets the frames so far, as after a pause in drawing.
    pub fn clear(&mut self) {
        self.times.clear();
    }

    /// The mean time between frames, and how far they are from it, as a
    /// standard deviation. `None` before any frame.
    pub fn stats(&self) -> Option<(Duration, Duration)> {
        if self.times.is_empty() {
            return None;
        }
        let secs: Vec<f64> = self.times.iter().map(Duration::as_secs_f64).collect();
        let mean = secs.iter().sum::<f64>() / secs.len() as f64;
        let variance = secs.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / secs.len() as f64;
        Some((
            Duration::from_secs_f64(mean),
            Duration::from_secs_f64(variance.sqrt()),
        ))
    }

    /// Draws the graph in the bottom-left corner of `canvas`: a bar for
    /// each frame, a unit high for each millisecond, with a line at the
    /// time frames are meant to take (`expected`) if that's known. Bars
    /// within [`ON_TIME`] of it are green, longer ones orange, and ones
    /// half as long again red. Above it, the frame rate and spread.
    pub fn draw(&self, canvas: &mut dyn Surface, expected: Option<Duration>) {
        let (_, view_h) = canvas.view();
        let (left, bottom) = (MARGIN, view_h as i32 - MARGIN);
        let top = bottom - HEIGHT;
        canvas.fill_rect(left - 1, top - 1, GRAPHED as i32 + 2, HEIGHT + 2, BACK);
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        for (i, &time) in self.times.iter().enumerate() {
            let color = match expected {
                Some(e) if ms(time) > ms(e) * 1.5 => LATE,
                Some(e) if (ms(time) - ms(e)).abs() > ms(e) * ON_TIME => OFF,
                _ => GOOD,
            };
            let h = (ms(time).round() as i32).clamp(1, HEIGHT);
            canvas.fill_rect(left + i as i32, bottom - h, 1, h, color);
        }
        if let Some(e) = expected.filter(|&e| ms(e) <= HEIGHT as f64) {
            let y = bottom as f64 - ms(e);
            let right = (left + GRAPHED as i32) as f64;
            canvas.line((left as f64, y), (right, y), EXPECTED);
        }
        if let Some((mean, spread)) = self.stats() {
            let fps = 1.0 / mean.as_secs_f64().max(1e-6);
            let text = format!("{fps:.1} fps  {:.2} ms ±{:.2}", ms(mean), ms(spread));
            draw_text(canvas, left, top - LINE - 1, &text, TEXT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;

    fn ms(n: f64) -> Duration {
        Duration::from_secs_f64(n / 1000.0)
    }

    #[test]
    fn keeps_the_last_frames() {
        let mut times = FrameTimes::new();
        for i in 0..GRAPHED + 5 {
            times.record(ms(i as f64));
        }
        assert_eq!(times.times.len(), GRAPHED);
        assert_eq!(times.times[0], ms(5.0));
        times.clear();
        assert_eq!(times.stats(), None);
    }

    #[test]
    fn measures_the_rate_and_spread() {
        let mut times = FrameTimes::new();
        for n in [16.0, 17.0, 16.0, 17.0] {
            times.record(ms(n));
        }
        let (mean, spread) = times.stats().unwrap();
        assert!((mean.as_secs_f64() * 1000.0 - 16.5).abs() < 1e-6);
        assert!((spread.as_secs_f64() * 1000.0 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn colours_frames_by_how_late_they_are() {
        let mut times = FrameTimes::new();
        for n in [16.7, 22.0, 34.0] {
            times.record(ms(n));
        }
        let mut canvas = Canvas::scaled((256, 224), 1);
        times.draw(&mut canvas, Some(ms(16.7)));
        let at = |x: i32| canvas.pixels[((224 - MARGIN - 1) * 256 + MARGIN + x) as usize];
        assert_eq!([at(0), at(1), at(2)], [GOOD, OFF, LATE]);
        assert_eq!(at(3), BACK, "no frame there yet");
    }
}
