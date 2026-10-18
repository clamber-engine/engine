//! Critters (spec/behavior/critter.md): things whose archetype has a
//! [`Critter`] behavior.
//!
//! A critter is scenery that flies: nothing touches it and it touches
//! nothing. It appears when its start comes near the view, as scenery
//! does, but somewhere across the top of the picture rather than at its
//! start, and flies towards its start ([`fly`]). On reaching its target
//! it perches for a while, then sets off for a new target further along
//! at a fixed height, and so on. It's removed once neither it nor its
//! start is near the view (see [`view::walkers`](crate::view)), and
//! appears afresh the next time its start is.
//!
//! Its drawn position is its `pos`: `x`, and the flight height raised by
//! the flutter. Its speed across is its `vx`, and its `clock` counts down
//! the frames it stands still, perched or at the world's start.
//!
//! Engine choices where the spec says nothing: where across the picture
//! it appears and its first speeds are chosen from the world's frame and
//! its index, not at random, so a run is the same each time; the flutter
//! counts from the world's first frame however the world was entered;
//! with no view, the picture counts as being at level (0, 0); it's drawn
//! facing the way it flies.

use clamber_fixed::{FRAC_BITS, Fixed};
use clamber_level::Vec2;

use crate::archetype::Critter;
use crate::player::{Facing, ceil};
use crate::world::{Phase, Thing};

/// The kind of entity a critter is, as content names its archetype.
pub const CRITTER: &str = "critter";

/// A speed's grain: 1/256 px per frame.
const GRAIN: u32 = FRAC_BITS - 8;

/// The speed a critter heads for across, in 1/256 px a frame per px of
/// distance to its target, and how much of the gap it closes a frame.
const PULL_ACROSS: (i64, i64) = (16, 32);

/// The same down.
const PULL_DOWN: (i64, i64) = (4, 64);

/// Moves a critter one frame, on the world's frame `frame`, with the
/// picture's top left at level `view` (whole px) at the start of the
/// frame. `start` is its entry's position. A critter that has just
/// appeared starts there first, standing at its start's height for its
/// `settle` frames if this is the world's first frame.
pub(crate) fn step(
    thing: &mut Thing,
    start: Vec2,
    critter: &Critter,
    frame: u32,
    view: (i64, i64),
) {
    if thing.phase == Phase::Ready {
        appear(thing, start, critter, frame, view);
    }
    if thing.clock > 0 {
        thing.clock -= 1;
        return;
    }
    let Phase::Flitting {
        mut uy,
        mut z,
        target,
    } = thing.phase
    else {
        return;
    };
    let (a, b) = (i64::from(target.0), i64::from(target.1));
    // Each speed closes part of the gap to one that grows with the
    // distance to the target, measured from where it was drawn.
    let ux = speed(thing.vx);
    let ux = ux + (PULL_ACROSS.0 * (a - thing.pos.x.floor()) - ux).div_euclid(PULL_ACROSS.1);
    let vy = speed(uy);
    let vy = vy - (vy - PULL_DOWN.0 * (b - ceil(thing.pos.y))).div_euclid(PULL_DOWN.1);
    thing.vx = grains(ux);
    uy = grains(vy);
    thing.pos.x += thing.vx;
    z += uy;
    thing.pos.y = z - Fixed::from_int(i32::from(flutter(critter, frame)));
    if ux != 0 {
        thing.facing = if ux < 0 { Facing::Left } else { Facing::Right };
    }
    let across = thing.pos.x.floor() - a;
    let down = ceil(thing.pos.y) - b;
    let (left, right) = critter.perch_across;
    let (up, low) = critter.perch_down;
    if (i64::from(left)..=i64::from(right)).contains(&across)
        && (i64::from(up)..=i64::from(low)).contains(&down)
    {
        // It perches, and sets off for the next target when it's done.
        thing.clock = u32::from(critter.perch) - 1;
        thing.vx = critter.launch.x;
        uy = critter.launch.y;
        let target = (target.0 + critter.hop.0, critter.hop.1);
        thing.phase = Phase::Flitting { uy, z, target };
        return;
    }
    thing.phase = Phase::Flitting { uy, z, target };
}

/// Starts a critter that has just appeared: somewhere across the top of
/// the picture, flying towards `start` at speeds picked by [`pick`].
fn appear(thing: &mut Thing, start: Vec2, critter: &Critter, frame: u32, (left, top): (i64, i64)) {
    let (r, q) = pick(frame, thing.index);
    let target = (start.x.floor() as i32, start.y.floor() as i32);
    let whole = |n: i64| Fixed::from_int(n as i32);
    thing.pos = Vec2::new(whole(left + r), whole(i64::from(target.1)));
    // Up to 2 px a frame, rightward below 128 and leftward from it.
    thing.vx = grains(4 * if r < 128 { r } else { r - 256 });
    thing.facing = if r < 128 { Facing::Right } else { Facing::Left };
    thing.phase = Phase::Flitting {
        uy: grains(q),
        z: whole(top),
        target,
    };
    // At the world's start it stands at its start's height a while.
    thing.clock = if frame == 0 {
        u32::from(critter.settle)
    } else {
        0
    };
}

/// A critter's flutter on the world's frame `frame`: how far above its
/// flight height it's drawn.
fn flutter(critter: &Critter, frame: u32) -> u8 {
    let len = u32::from(critter.flutter_len);
    let place = (frame % len + u32::from(critter.flutter_from) % len) % len;
    critter.flutter[place as usize]
}

/// Two whole numbers for a critter appearing on `frame` with `index`: `r`
/// from 0 to 255 and `q` from 1 to 512. Any pair is as good as another;
/// these are mixed from the frame and the index, so a run is the same
/// every time.
fn pick(frame: u32, index: usize) -> (i64, i64) {
    let mut z = (u64::from(frame) << 32 | index as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    ((z & 0xff) as i64, ((z >> 8) & 0x1ff) as i64 + 1)
}

/// A speed in 1/256 px a frame, rounded down.
fn speed(v: Fixed) -> i64 {
    v.to_bits() >> GRAIN
}

/// `n` 1/256 px a frame.
fn grains(n: i64) -> Fixed {
    Fixed::from_bits(n << GRAIN)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archetype::Archetypes;
    use crate::archetype::Behavior;

    fn critter() -> Critter {
        let archetypes = Archetypes::builtin();
        let entity = entity(Vec2::ints(0, 0));
        match archetypes
            .resolve(&entity, "", "")
            .unwrap()
            .unwrap()
            .behavior
        {
            Behavior::Critter(c) => c,
            other => panic!("{other:?}"),
        }
    }

    fn entity(at: Vec2) -> clamber_level::Entity {
        clamber_level::Entity {
            kind: CRITTER.into(),
            id: None,
            at,
            params: Default::default(),
        }
    }

    fn f(v: f64) -> Fixed {
        Fixed::from_f64(v).unwrap()
    }

    /// A critter as critter.md's example has it just after appearing:
    /// entry at (400, 120), `V` = 300, `U` = 40, `r` = 40 and `q` = 256.
    fn example() -> Thing {
        let entity = entity(Vec2::new(f(400.0), f(120.0)));
        let mut thing = Thing::placed(0, &entity, None);
        thing.pos = Vec2::new(f(340.0), f(120.0));
        thing.vx = grains(160);
        thing.phase = Phase::Flitting {
            uy: grains(256),
            z: f(40.0),
            target: (400, 120),
        };
        thing
    }

    #[test]
    fn flies_as_the_example_says() {
        // critter.md, "Example (synthetic)".
        let critter = critter();
        let start = Vec2::new(f(400.0), f(120.0));
        let mut thing = example();
        let rows = [
            (185, 252, 340.72265625, 40.984375, 36.984375),
            (209, 254, 341.5390625, 41.9765625, 35.9765625),
            (231, 256, 342.44140625, 42.9765625, 35.9765625),
            (252, 258, 343.42578125, 43.984375, 38.984375),
        ];
        for (n, (ux, uy, x, z, y)) in rows.into_iter().enumerate() {
            step(&mut thing, start, &critter, 1000 + n as u32, (300, 40));
            let Phase::Flitting {
                uy: vy, z: height, ..
            } = thing.phase
            else {
                panic!("{:?}", thing.phase);
            };
            assert_eq!(
                (speed(thing.vx), speed(vy), thing.pos.x, height, thing.pos.y),
                (ux, uy, f(x), f(z), f(y)),
                "frame {}",
                1000 + n
            );
        }
        // It first perches on frame 1338, and stays there to 1697.
        let mut frame = 1004;
        while thing.clock == 0 {
            step(&mut thing, start, &critter, frame, (300, 40));
            frame += 1;
        }
        assert_eq!(frame - 1, 1338);
        assert_eq!(thing.pos, Vec2::new(f(399.01171875), f(123.23046875)));
        let perched = thing.pos;
        for n in 1339..=1697 {
            step(&mut thing, start, &critter, n, (300, 40));
            assert_eq!(thing.pos, perched, "frame {n}");
        }
        step(&mut thing, start, &critter, 1698, (300, 40));
        let Phase::Flitting { uy, target, .. } = thing.phase else {
            panic!();
        };
        assert_eq!((speed(thing.vx), speed(uy), target), (464, 244, (336, -16)));
    }

    #[test]
    fn flutters_from_its_place_at_the_first_frame() {
        // critter.md, "Flying": 4 on frame 0, 6 on frame 1, 0 on frame 6.
        let critter = critter();
        assert_eq!(flutter(&critter, 0), 4);
        assert_eq!(flutter(&critter, 1), 6);
        assert_eq!(flutter(&critter, 6), 0);
        assert_eq!(flutter(&critter, 14), 0);
    }

    #[test]
    fn appears_across_the_top_of_the_picture() {
        let critter = critter();
        let start = Vec2::new(f(400.0), f(120.0));
        for frame in 1..200 {
            let mut thing = example();
            thing.phase = Phase::Ready;
            step(&mut thing, start, &critter, frame, (300, 40));
            let (r, q) = pick(frame, 0);
            assert!((0..=255).contains(&r) && (1..=512).contains(&q));
            let x = thing.pos.x.floor() - 300;
            let y = ceil(thing.pos.y) - 40;
            assert!((-2..=258).contains(&x), "{x}");
            assert!((-7..=3).contains(&y), "{y}");
        }
    }

    #[test]
    fn stands_at_its_start_height_first_at_the_worlds_start() {
        // critter.md, "At the level's start": drawn at x = r and its
        // entry's y on frames 0 to 40, flying from frame 41.
        let critter = critter();
        let start = Vec2::new(f(150.0), f(100.0));
        let mut thing = example();
        thing.phase = Phase::Ready;
        let (r, _) = pick(0, 0);
        for frame in 0..=40 {
            step(&mut thing, start, &critter, frame, (0, 0));
            assert_eq!(thing.pos, Vec2::new(f(r as f64), f(100.0)), "frame {frame}");
        }
        step(&mut thing, start, &critter, 41, (0, 0));
        assert_ne!(thing.pos.x, f(r as f64));
    }
}
