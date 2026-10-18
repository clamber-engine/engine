//! Turns wall-clock time into logic steps at exactly 60 Hz, and the time
//! between frames drawn with vsync into whole refreshes of the display.

use std::time::Duration;

use clamber_sim::STEPS_PER_SECOND;

/// After a stall (a dragged window, a debugger), at most this many steps
/// catch up; the rest of the lost time is dropped rather than replayed.
const MAX_CATCH_UP: u32 = 5;

const NANOS: u128 = 1_000_000_000;

/// How far a display's refresh can be from a whole number of frames a
/// step, as a share, and still be drawn at exactly that number: a
/// display that says it's 59 Hz, or is 59.94 Hz, gets one step a frame.
const LOCK_TOLERANCE: f64 = 0.02;

/// The most frames a step a display is drawn at exactly.
const MOST_FRAMES_A_STEP: u32 = 4;

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
        self.add(units(elapsed))
    }

    /// Adds the time a frame drawn with vsync `elapsed` after the last
    /// stands for under `pace`, and returns how many steps are due now.
    ///
    /// On a display the steps follow exactly, a frame is drawn as late
    /// into its refresh as can be, so it shows the newest step rather
    /// than one from up to a step before (#192).
    pub fn advance_paced(&mut self, elapsed: Duration, pace: &mut Pace) -> u32 {
        let (units, waited) = pace.frame(elapsed);
        let steps = self.add(units);
        if waited && pace.locked {
            let period = pace.period;
            self.owed = (self.owed - self.owed % period + period - 1).min(NANOS - 1);
        }
        steps
    }

    fn add(&mut self, units: u128) -> u32 {
        self.owed += units;
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

/// `elapsed` in the clock's units.
fn units(elapsed: Duration) -> u128 {
    elapsed.as_nanos() * STEPS_PER_SECOND as u128
}

/// Paces the steps by the display when frames are drawn with vsync.
///
/// Each frame then shows a whole number of refreshes, but the time
/// measured between frames jitters by a millisecond or more, and drawing
/// between steps by that time shakes everything that moves (#190). So a
/// frame is taken to be the whole number of refreshes it's nearest. A
/// display at close to a whole number of frames a step, as at 60, 120
/// or 240 Hz, is followed exactly: every step is drawn the same number
/// of times, and the game runs at the display's rate if that's a little
/// off. At other rates, time left over from rounding is caught up once
/// it adds to a refresh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pace {
    /// A refresh, in the clock's units.
    period: u128,
    /// The display is drawn at a whole number of frames a step, which
    /// the steps follow exactly.
    locked: bool,
    /// Time measured but not yet passed on, when not locked.
    behind: i128,
}

impl Pace {
    /// Pacing for a display that refreshes `millihertz` / 1000 times a
    /// second, or `None` if that isn't known.
    pub fn new(millihertz: u32) -> Option<Pace> {
        if millihertz == 0 {
            return None;
        }
        let hz = millihertz as f64 / 1000.0;
        let frames = (hz / STEPS_PER_SECOND as f64).round() as u32;
        let near =
            |frames: u32| (hz / (frames * STEPS_PER_SECOND) as f64 - 1.0).abs() <= LOCK_TOLERANCE;
        let locked = (1..=MOST_FRAMES_A_STEP).contains(&frames) && near(frames);
        let period = match locked {
            true => NANOS / frames as u128,
            false => NANOS * STEPS_PER_SECOND as u128 * 1000 / millihertz as u128,
        };
        Some(Pace {
            period,
            locked,
            behind: 0,
        })
    }

    /// The time, in the clock's units, a frame drawn `elapsed` after the
    /// last stands for: whole refreshes, unless it's too far from any to
    /// have waited for the display, as when the window can't be seen.
    /// Also whether it waited.
    fn frame(&mut self, elapsed: Duration) -> (u128, bool) {
        let measured = units(elapsed);
        let period = self.period;
        let refreshes = (measured + period / 2) / period;
        if refreshes == 0 || measured.abs_diff(refreshes * period) > period / 4 {
            self.behind = 0;
            return (measured, false);
        }
        let paced = refreshes * period;
        if self.locked {
            return (paced, true);
        }
        self.behind += measured as i128 - paced as i128;
        if self.behind.unsigned_abs() < period {
            return (paced, true);
        }
        let caught_up = (paced as i128 + self.behind).max(0) as u128;
        self.behind = 0;
        (caught_up, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames about `period` apart, each up to a tenth of it early or
    /// late (1.5 ms at 60 Hz).
    fn jittery(period: Duration, frames: usize) -> Vec<Duration> {
        let wobble = [0.0, 0.09, -0.07, 0.04, -0.09, 0.02];
        let at = |i: usize| wobble[i % wobble.len()] * period.as_nanos() as f64;
        (0..frames)
            .map(|i| {
                let ns = period.as_nanos() as f64 + at(i + 1) - at(i);
                Duration::from_nanos(ns as u64)
            })
            .collect()
    }

    fn sixtieth() -> Duration {
        Duration::from_secs(1) / 60
    }

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
    fn draws_each_step_once_at_60_hz_despite_jitter() {
        for millihertz in [60_000, 59_940, 59_000] {
            let mut pace = Pace::new(millihertz).unwrap();
            let mut clock = Clock::new();
            clock.advance(sixtieth() / 2);
            let refresh = Duration::from_secs(1000) / millihertz;
            for elapsed in jittery(refresh, 600) {
                assert_eq!(clock.advance_paced(elapsed, &mut pace), 1, "{millihertz}");
                // Drawn at the newest step, not halfway to it.
                assert!(clock.alpha() > 1.0 - 1e-6);
            }
        }
    }

    #[test]
    fn draws_each_step_twice_at_120_hz() {
        let mut pace = Pace::new(120_000).unwrap();
        let mut clock = Clock::new();
        let steps: Vec<u32> = jittery(sixtieth() / 2, 600)
            .into_iter()
            .map(|elapsed| clock.advance_paced(elapsed, &mut pace))
            .collect();
        assert!(steps.chunks(2).all(|pair| pair == [0, 1]), "{steps:?}");
        // The frame after a step's is drawn at that step.
        assert!(clock.alpha() > 0.5 - 1e-6 && clock.alpha() < 0.5);
        clock.advance_paced(sixtieth() / 2, &mut pace);
        assert!(clock.alpha() > 1.0 - 1e-6);
    }

    #[test]
    fn keeps_in_step_at_180_hz() {
        let mut pace = Pace::new(180_000).unwrap();
        let mut clock = Clock::new();
        let steps: Vec<u32> = jittery(Duration::from_secs(1) / 180, 900)
            .into_iter()
            .map(|elapsed| clock.advance_paced(elapsed, &mut pace))
            .collect();
        assert!(steps.chunks(3).all(|c| c == [0, 0, 1]), "{steps:?}");
        assert!(clock.alpha() < 1.0);
    }

    #[test]
    fn keeps_time_at_144_hz() {
        let mut pace = Pace::new(144_000).unwrap();
        let mut clock = Clock::new();
        let frames = jittery(Duration::from_secs(1) / 144, 144 * 10);
        let mut alphas = Vec::new();
        let steps: u32 = frames
            .into_iter()
            .map(|elapsed| {
                let steps = clock.advance_paced(elapsed, &mut pace);
                alphas.push(clock.alpha());
                steps
            })
            .sum();
        assert!((599..=601).contains(&steps), "{steps}");
        // Every frame moves on by the same 60/144 of a step.
        for pair in alphas.windows(2) {
            let moved = (pair[1] - pair[0]).rem_euclid(1.0);
            assert!((moved - 60.0 / 144.0).abs() < 1e-6, "{moved}");
        }
    }

    #[test]
    fn follows_the_clock_when_frames_dont_wait_for_the_display() {
        let mut pace = Pace::new(60_000).unwrap();
        let mut clock = Clock::new();
        let steps: u32 = (0..1000)
            .map(|_| clock.advance_paced(Duration::from_millis(1), &mut pace))
            .sum();
        assert_eq!(steps, 60);
        assert_eq!(clock.advance_paced(Duration::from_millis(10), &mut pace), 0);
        assert_eq!(clock.advance_paced(Duration::from_secs(3), &mut pace), 5);
    }

    #[test]
    fn missed_refreshes_are_caught_up() {
        let mut pace = Pace::new(60_000).unwrap();
        let mut clock = Clock::new();
        let late = sixtieth() * 2 + Duration::from_millis(1);
        assert_eq!(clock.advance_paced(late, &mut pace), 2);
        assert!(clock.alpha() > 1.0 - 1e-6);
    }

    #[test]
    fn needs_a_refresh_rate() {
        assert_eq!(Pace::new(0), None);
    }

    #[test]
    fn drops_time_after_a_stall() {
        let mut clock = Clock::new();
        assert_eq!(clock.advance(Duration::from_secs(3)), MAX_CATCH_UP);
        assert_eq!(clock.alpha(), 0.0);
    }
}
