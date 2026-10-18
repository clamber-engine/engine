//! What's drawn beyond the world's own view in original activation
//! (#236): the window shows more than the movement's view, but things
//! only come into the level, move and act by that view's rules.
//!
//! So nothing pops in, a thing that isn't in the level but would appear
//! at its start if the view reached it ([`World::would_appear`]) is
//! drawn there already, as it will appear: when the world brings it in,
//! it's where it was drawn. Things that go while outside the original
//! picture, as the view's rules remove them, fade out where they were,
//! and things that come to be drawn waiting where the window already
//! shows, such as a walker removed while its start is in sight, fade in.
//! Inside the picture, things come and go as they do in the original.

use std::collections::HashMap;

use clamber_sim::{Thing, World};

use crate::cast::Pose;

/// Frames a thing takes to fade in or out.
pub const FADE: u8 = 16;

/// The things drawn beyond what the world itself has in the level.
#[derive(Clone, Debug, Default)]
pub struct Beyond {
    /// Things that would appear, as they would, at their start.
    pub waiting: Vec<Thing>,
    /// Things that went while outside the picture, as they last were,
    /// in the pose they had, with the frames left of their fade.
    pub leaving: Vec<(Thing, Pose, u8)>,
    /// The frames each thing that's fading in has been, by its index,
    /// drawn waiting or in the level.
    pub fading_in: HashMap<usize, u8>,
}

/// Where the window and the picture are across: each one's left edge
/// and width, in level px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Across {
    pub window: (i64, i64),
    pub picture: (i64, i64),
}

impl Across {
    fn in_window(&self, x: i64) -> bool {
        (self.window.0..self.window.0 + self.window.1).contains(&x)
    }

    fn in_picture(&self, x: i64) -> bool {
        (self.picture.0..self.picture.0 + self.picture.1).contains(&x)
    }
}

impl Beyond {
    /// What's beyond `world` as it is, with nothing fading.
    pub fn of(world: &World, across: Across) -> Beyond {
        Beyond {
            waiting: world.would_appear(across.window.0, across.window.1),
            ..Beyond::default()
        }
    }

    /// Moves on a step after which `world` is as it is, from the things
    /// as they were before it, `before`, in `poses`.
    pub fn update(&mut self, before: &[Thing], poses: &[Pose], world: &World, across: Across) {
        let waiting = world.would_appear(across.window.0, across.window.1);
        let in_level = |i: usize| world.things.get(i).is_some_and(|t| t.active);
        let is_waiting = |i: usize| waiting.iter().any(|t| t.index == i);
        let pose = |i: usize| poses.get(i).copied().unwrap_or_default();
        let x = |thing: &Thing| thing.pos.x.floor();

        self.leaving.retain_mut(|(_, _, left)| {
            *left -= 1;
            *left > 0
        });
        for thing in before.iter().filter(|t| t.active) {
            let i = thing.index;
            if !in_level(i) && !across.in_picture(x(thing)) {
                self.leaving.push((thing.clone(), pose(i), FADE));
            }
        }
        for thing in &self.waiting {
            let i = thing.index;
            if !in_level(i) && !is_waiting(i) {
                let shown = self.fading_in.get(&i).copied().unwrap_or(FADE);
                self.leaving.push((thing.clone(), pose(i), shown));
            }
        }

        self.fading_in.retain(|&i, frames| {
            *frames += 1;
            *frames < FADE && (in_level(i) || is_waiting(i))
        });
        for thing in &waiting {
            let i = thing.index;
            let was_waiting = self.waiting.iter().any(|t| t.index == i);
            if !was_waiting && across.in_window(x(thing)) {
                self.fading_in.entry(i).or_insert(0);
            }
        }
        self.waiting = waiting;
    }

    /// How opaque thing `index` is drawn, in the level or waiting, from
    /// 0 to 255.
    pub fn opacity(&self, index: usize) -> u8 {
        self.fading_in
            .get(&index)
            .map_or(255, |&frames| opacity(frames))
    }
}

/// How opaque something `frames` into a fade in, or with `frames` left
/// of a fade out, is.
pub fn opacity(frames: u8) -> u8 {
    (u32::from(frames.min(FADE)) * 255 / u32::from(FADE)) as u8
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_level::Level;
    use clamber_sim::{Fixed, generic_feel};

    use super::*;

    /// The player at x = 300 on `classic-1`, its picture from 229, and a
    /// walker starting at x = 600, out of the level.
    fn world() -> World {
        let src = r#"
            format = 1
            size = [4000, 600]
            start = [300, 400]

            [[collision]]
            shape = "rect"
            rect = [0, 400, 4000, 200]

            [[entities]]
            type = "crawler"
            at = [600, 400]
            params = { speed = 1, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, appear_wait = 0, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }
        "#;
        let level = Level::from_toml_str(src, "test").unwrap();
        World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        )
    }

    const ACROSS: Across = Across {
        window: (100, 512),
        picture: (229, 256),
    };

    /// The walker as it was before a step, in the level at `x`.
    fn was_at(world: &World, x: i32) -> Vec<Thing> {
        let mut before = world.things.clone();
        before[0].active = true;
        before[0].pos.x = Fixed::from_int(x);
        before
    }

    #[test]
    fn what_goes_outside_the_picture_fades_out_where_it_was() {
        let world = world();
        assert!(!world.things[0].active);
        let mut beyond = Beyond::of(&world, ACROSS);
        assert_eq!(beyond.waiting.len(), 1);
        beyond.update(&was_at(&world, 560), &[], &world, ACROSS);
        let (thing, _, left) = &beyond.leaving[0];
        assert_eq!((thing.pos.x, *left), (Fixed::from_int(560), FADE));
        assert_eq!(beyond.opacity(0), 255, "waiting all along");
        for _ in 0..FADE {
            beyond.update(&world.things, &[], &world, ACROSS);
        }
        assert!(beyond.leaving.is_empty());

        // Inside the picture, things go as in the original.
        beyond.update(&was_at(&world, 300), &[], &world, ACROSS);
        assert!(beyond.leaving.is_empty());
    }

    #[test]
    fn what_comes_to_wait_in_sight_fades_in() {
        let world = world();
        let mut beyond = Beyond::default();
        beyond.update(&world.things, &[], &world, ACROSS);
        assert_eq!(beyond.opacity(0), 0);
        beyond.update(&world.things, &[], &world, ACROSS);
        assert_eq!(beyond.opacity(0), opacity(1));
        for _ in 1..FADE {
            beyond.update(&world.things, &[], &world, ACROSS);
        }
        assert_eq!(beyond.opacity(0), 255);

        // Coming into the window from outside it, it's there whole.
        let mut beyond = Beyond::default();
        let off = Across {
            window: (80, 512),
            ..ACROSS
        };
        beyond.update(&world.things, &[], &world, off);
        assert_eq!(beyond.waiting.len(), 1, "near the window");
        assert_eq!(beyond.opacity(0), 255);
    }
}
