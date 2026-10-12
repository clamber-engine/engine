//! Playing a sheet's animations, one logic frame at a time.

use crate::sheet::{Animation, Frame};

/// Where an animation is: which of its frames shows, and for how long it
/// has. Advanced once per logic frame, so playback is as deterministic as
/// the simulation driving it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Playback {
    /// Index into the animation's `frames`.
    step: usize,
    /// Logic frames the current frame has shown for, after the first.
    elapsed: u32,
    /// Passes completed.
    passes: u32,
    finished: bool,
}

impl Playback {
    /// At the start of an animation.
    pub fn new() -> Playback {
        Playback::default()
    }

    /// The sheet frame to draw now.
    pub fn frame(&self, animation: &Animation) -> usize {
        animation.frames[self.step.min(animation.frames.len() - 1)]
    }

    /// Whether a non-looping animation has played all its passes. It then
    /// holds its last frame.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Moves on one logic frame. `frames` are the sheet's, for durations.
    pub fn advance(&mut self, animation: &Animation, frames: &[Frame]) {
        if self.finished {
            return;
        }
        self.elapsed += 1;
        if self.elapsed < frames[self.frame(animation)].ticks {
            return;
        }
        self.elapsed = 0;
        self.step += 1;
        if self.step < animation.frames.len() {
            return;
        }
        self.passes += 1;
        if animation.repeat.is_some_and(|n| self.passes >= n) {
            self.step = animation.frames.len() - 1;
            self.finished = true;
        } else {
            self.step = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Rect;

    fn frames(ticks: &[u32]) -> Vec<Frame> {
        ticks
            .iter()
            .map(|&ticks| Frame {
                rect: Rect::default(),
                offset: (0, 0),
                ticks,
                boxes: Vec::new(),
            })
            .collect()
    }

    fn play(animation: &Animation, frames: &[Frame], logic_frames: usize) -> Vec<usize> {
        let mut playback = Playback::new();
        (0..logic_frames)
            .map(|_| {
                let shown = playback.frame(animation);
                playback.advance(animation, frames);
                shown
            })
            .collect()
    }

    #[test]
    fn shows_each_frame_for_its_ticks_and_loops() {
        let frames = frames(&[2, 1, 3]);
        let walk = Animation {
            name: "walk".into(),
            frames: vec![0, 1, 2],
            repeat: None,
        };
        assert_eq!(play(&walk, &frames, 8), [0, 0, 1, 2, 2, 2, 0, 0]);
    }

    #[test]
    fn follows_the_animation_order() {
        let frames = frames(&[1, 1, 1]);
        let bounce = Animation {
            name: "bounce".into(),
            frames: vec![0, 1, 2, 1],
            repeat: None,
        };
        assert_eq!(play(&bounce, &frames, 6), [0, 1, 2, 1, 0, 1]);
    }

    #[test]
    fn stops_on_the_last_frame_after_its_passes() {
        let frames = frames(&[1, 2]);
        let blink = Animation {
            name: "blink".into(),
            frames: vec![0, 1],
            repeat: Some(2),
        };
        let mut playback = Playback::new();
        let mut shown = Vec::new();
        for _ in 0..8 {
            shown.push(playback.frame(&blink));
            playback.advance(&blink, &frames);
        }
        assert_eq!(shown, [0, 1, 1, 0, 1, 1, 1, 1]);
        assert!(playback.finished());
    }

    #[test]
    fn single_frame_animations_hold() {
        let frames = frames(&[1]);
        let idle = Animation {
            name: "idle".into(),
            frames: vec![0],
            repeat: None,
        };
        assert_eq!(play(&idle, &frames, 3), [0, 0, 0]);
    }
}
