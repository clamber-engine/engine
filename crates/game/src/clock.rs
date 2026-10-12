//! Turns wall-clock time into logic steps at exactly 60 Hz.

use std::time::Duration;

use clamber_sim::STEPS_PER_SECOND;

/// After a stall (a dragged window, a debugger), at most this many steps
/// catch up; the rest of the lost time is dropped rather than replayed.
const MAX_CATCH_UP: u32 = 5;

const NANOS: u128 = 1_000_000_000;

/// Time owed to the simulation, kept in units of 1/(60 × 10^9) s so steps
/// never drift.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    owed: u128,
}

impl Clock {
    pub fn new() -> Clock {
        Clock::default()
    }

    /// Adds elapsed time and returns how many steps are due now.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.owed += elapsed.as_nanos() * STEPS_PER_SECOND as u128;
        let steps = (self.owed / NANOS) as u32;
        self.owed %= NANOS;
        if steps > MAX_CATCH_UP {
            self.owed = 0;
            return MAX_CATCH_UP;
        }
        steps
    }

    /// How far into the next step the clock is, from 0 to 1, for drawing
    /// between the last two states.
    pub fn alpha(&self) -> f64 {
        self.owed as f64 / NANOS as f64
    }

    /// Time until the next step is due.
    pub fn until_next(&self) -> Duration {
        let left = NANOS - self.owed;
        Duration::from_nanos(left.div_ceil(STEPS_PER_SECOND as u128) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_sixty_times_a_second_without_drift() {
        let mut clock = Clock::new();
        let steps: u32 = (0..1000)
            .map(|_| clock.advance(Duration::from_millis(1)))
            .sum();
        assert_eq!(steps, 60);
        assert_eq!(clock.alpha(), 0.0);
    }

    #[test]
    fn reports_progress_into_the_next_step() {
        let mut clock = Clock::new();
        assert_eq!(clock.advance(Duration::from_micros(25_000)), 1);
        assert!((clock.alpha() - 0.5).abs() < 1e-9);
        assert_eq!(clock.until_next(), Duration::from_nanos(8_333_334));
        assert_eq!(clock.advance(clock.until_next()), 1);
    }

    #[test]
    fn drops_time_after_a_stall() {
        let mut clock = Clock::new();
        assert_eq!(clock.advance(Duration::from_secs(3)), MAX_CATCH_UP);
        assert_eq!(clock.alpha(), 0.0);
    }
}
