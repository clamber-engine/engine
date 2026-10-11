//! Jump buffering: a jump pressed a little too early still happens.

/// Remembers a jump press for a few frames, so a press just before the
/// character can jump (say, just before landing) isn't lost.
///
/// This belongs to the simulation state, not the device layer: it's fed
/// from each frame's [`Frame`](crate::Frame), which comes from held actions
/// alone, so recorded input replays the same way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct JumpBuffer {
    window: u8,
    /// Frames since the buffered press, if one is buffered.
    age: Option<u8>,
}

impl JumpBuffer {
    /// A press stays buffered on the frame it happens and the `window`
    /// frames after it. A window of 0 only allows jumping on the frame of
    /// the press itself, which is the same as no buffering.
    pub fn new(window: u8) -> JumpBuffer {
        JumpBuffer { window, age: None }
    }

    /// Advances one frame. Call once per logic frame, before asking
    /// whether a jump is buffered, with whether jump was pressed this frame.
    pub fn update(&mut self, pressed: bool) {
        self.age = if pressed {
            Some(0)
        } else {
            self.age
                .and_then(|age| age.checked_add(1))
                .filter(|&age| age <= self.window)
        };
    }

    /// Whether a recent jump press is waiting to be used.
    pub fn is_buffered(&self) -> bool {
        self.age.is_some()
    }

    /// Uses the buffered press, if any, so one press makes one jump.
    pub fn take(&mut self) -> bool {
        self.age.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames, starting with the press, on which a jump is buffered.
    fn buffered_frames(window: u8) -> Vec<usize> {
        let mut buf = JumpBuffer::new(window);
        (0..10)
            .filter(|&f| {
                buf.update(f == 0);
                buf.is_buffered()
            })
            .collect()
    }

    #[test]
    fn press_lasts_window_frames_after() {
        assert_eq!(buffered_frames(0), [0]);
        assert_eq!(buffered_frames(3), [0, 1, 2, 3]);
        assert_eq!(buffered_frames(255).len(), 10);
    }

    #[test]
    fn take_consumes_once() {
        let mut buf = JumpBuffer::new(4);
        buf.update(true);
        buf.update(false);
        assert!(buf.take());
        assert!(!buf.take());
        buf.update(false);
        assert!(!buf.is_buffered());
    }

    #[test]
    fn new_press_restarts_window() {
        let mut buf = JumpBuffer::new(2);
        buf.update(true);
        buf.update(false);
        buf.update(true);
        buf.update(false);
        buf.update(false);
        assert!(buf.is_buffered());
        buf.update(false);
        assert!(!buf.is_buffered());
    }
}
