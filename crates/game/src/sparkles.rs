//! The sparkles that come and go around a letter
//! (`spec/behavior/letters.md`, "Pictures", "Sparkles").
//!
//! On every sparkle frame (frame number 14 mod 16, counted from the
//! latest frame 0) each letter that is there gets one sparkle at a random
//! point in a 32 x 32 px square around its position. A sparkle shows one
//! of 8 steps, 5 counted frames each, and is gone after 40. Frames on
//! which nothing moves neither start a sparkle nor count. The spec
//! doesn't say which points come, so they come from a small generator
//! that every game starts the same way; the pictures are the engine's
//! own. Nothing here touches the simulation.

use clamber_level::Vec2;
use clamber_sim::{Behavior, FlowEvent, Outcome, World};

/// Frames a sparkle lasts, counted frames after the one it comes on.
const LIFETIME: u32 = 40;

/// Counted frames per step.
const STEP: u32 = 5;

/// Sparkle frames are the ones whose number is this modulo [`EVERY`].
const ON: u32 = 14;
const EVERY: u32 = 16;

/// How far a step's bars reach from the sparkle's point, step 1 to 8:
/// the largest fills 13 x 13 px.
pub const REACH: [i32; 8] = [1, 2, 4, 6, 5, 4, 2, 1];

/// One sparkle: a level point and the counted frames since it came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sparkle {
    pub x: i32,
    pub y: i32,
    age: u32,
}

impl Sparkle {
    /// The step it shows, 1 to 8.
    pub fn step(&self) -> u32 {
        self.age / STEP + 1
    }

    /// How far its bars reach from its point.
    pub fn reach(&self) -> i32 {
        REACH[(self.step() - 1) as usize]
    }
}

/// The sparkles that are there and the generator that places new ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sparkles {
    pub list: Vec<Sparkle>,
    seed: u32,
}

impl Default for Sparkles {
    fn default() -> Sparkles {
        Sparkles {
            list: Vec::new(),
            seed: 0x2545_f491,
        }
    }
}

impl Sparkles {
    /// Moves the sparkles on a logic frame, for `world` as it is now.
    /// `number` is the latest frame's number, from 0 at the level's start
    /// and at play starting again.
    pub fn update(&mut self, world: &World, number: u32) {
        if world.flow.events.contains(&FlowEvent::Respawn) {
            self.list.clear();
        }
        if nothing_moves(world) {
            return;
        }
        for sparkle in &mut self.list {
            sparkle.age += 1;
        }
        self.list.retain(|s| s.age < LIFETIME);
        if number % EVERY != ON {
            return;
        }
        for thing in world.things.iter().filter(|t| t.active) {
            if matches!(
                thing.archetype.map(|a| a.behavior),
                Some(Behavior::Collect { letter, .. }) if letter > 0
            ) {
                let at = letter_at(thing.pos);
                let (a, b) = (self.next(32) - 16, self.next(32) - 21);
                self.list.push(Sparkle {
                    x: at.0 + a,
                    y: at.1 + b,
                    age: 0,
                });
            }
        }
    }

    /// A number from 0 up to but not including `below`.
    fn next(&mut self, below: u32) -> i32 {
        // xorshift32, whose high bits are the better ones.
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        ((self.seed >> 8) % below) as i32
    }
}

/// A letter's position (`xl`, `yl`): 1 px left of its entry's x.
fn letter_at(pos: Vec2) -> (i32, i32) {
    (
        pos.x.to_f64().round() as i32 - 1,
        pos.y.to_f64().round() as i32,
    )
}

/// Whether the last frame was one on which nothing moves: a bounce's,
/// a defeat's or a knock-out's frozen frames, or a character joining.
pub(crate) fn nothing_moves(world: &World) -> bool {
    world.still
        && !matches!(world.flow.outcome, Outcome::Dying(_) | Outcome::GameOver)
        && world.ending.is_none()
        && world.leaving.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sparkle(age: u32) -> Sparkle {
        Sparkle { x: 0, y: 0, age }
    }

    #[test]
    fn a_sparkle_shows_each_step_for_five_counted_frames_and_is_gone_at_forty() {
        let steps: Vec<_> = (0..LIFETIME).map(|c| sparkle(c).step()).collect();
        assert_eq!(steps[..6], [1, 1, 1, 1, 1, 2]);
        assert_eq!(steps[39], 8);
        assert_eq!(steps.iter().filter(|&&s| s == 8).count(), 5);
        // The largest is step 4.
        assert_eq!(REACH.iter().max(), Some(&6));
        assert_eq!(REACH[3], 6);
    }

    #[test]
    fn points_stay_in_the_square_around_the_letter() {
        let mut sparkles = Sparkles::default();
        let (mut a, mut b) = ((i32::MAX, i32::MIN), (i32::MAX, i32::MIN));
        for _ in 0..4000 {
            let (x, y) = (sparkles.next(32) - 16, sparkles.next(32) - 21);
            a = (a.0.min(x), a.1.max(x));
            b = (b.0.min(y), b.1.max(y));
        }
        assert_eq!((a, b), ((-16, 15), (-21, 10)));
    }
}
