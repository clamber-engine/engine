//! The ground measured column by column, for physics with `face_slope`.
//!
//! The level is split into 1-px-wide columns of `x`, and each column's
//! floor is the top of the ground under its middle. A body's column is
//! the one holding its `x`. Between neighbouring columns the floor climbs
//! or falls by some amount:
//!
//! - by `face_slope` or less, it's walkable ground: flat floor and slopes;
//! - by more than that, up to a step ([`STEP`]), two or more times in a
//!   row the same way, the columns are part of a steep face, which
//!   carries a grounded body down it and stops one going up it; once on
//!   its own, it's a step, walked up and down;
//! - by more than a step, it's a wall.
//!
//! A grounded body walks under a one-way platform unless the platform's
//! underside is right over its feet. Then the platform's top is the
//! floor, and if it's more than a step up the body falls through the
//! floor it was on.
//!
//! These rules don't care how the level draws its ground: a face can be a
//! steep edge of a polygon or a staircase of small steps.

use clamber_fixed::Fixed;
use clamber_level::{Level, Span};

use crate::player::{HEIGHT, STEP, ceil};

/// A column's floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Floor {
    pub y: Fixed,
    /// The collider it's the top of.
    pub collider: usize,
}

/// The column holding `x`.
pub(crate) fn column(x: Fixed) -> i64 {
    x.floor()
}

/// The left edge of `column`.
pub(crate) fn left_edge(column: i64) -> Fixed {
    Fixed::from_bits(column << clamber_fixed::FRAC_BITS)
}

/// The middle of `column`.
pub(crate) fn middle(column: i64) -> Fixed {
    left_edge(column) + Fixed::from_ratio_pow2(1, 1)
}

/// How far above the feet a one-way platform's underside can be for a
/// grounded body to see the platform's top as the floor: one with more
/// room under it, the body walks under.
const UNDERSIDE_REACH: Fixed = Fixed::ONE;

/// The floor of `column` as seen by a grounded body with its feet at
/// `feet`: the top of the ground under the column's middle, from a body's
/// height above the feet to two steps below them. Ground that reaches a
/// body's height above the feet or more counts as that high. One-way
/// platforms whose underside is more than [`UNDERSIDE_REACH`] above the
/// feet aren't there. `None` if there's no ground in that range.
pub(crate) fn floor(level: &Level, column: i64, feet: Fixed) -> Option<Floor> {
    let head = feet - HEIGHT;
    spans(level, column)
        .into_iter()
        .filter(|s| !(s.one_way && s.bottom < feet - UNDERSIDE_REACH))
        .find(|s| s.bottom >= head && s.top <= feet + STEP + STEP)
        .map(|s| Floor {
            y: s.top.max(head),
            collider: s.collider,
        })
}

/// Whether a grounded body with its feet at `feet` falls through the
/// floor going into `column`: its floor is the top of a one-way platform
/// right over the feet (see [`floor`]), more than a step above them.
pub(crate) fn falls_through(level: &Level, column: i64, feet: Fixed) -> bool {
    floor(level, column, feet).is_some_and(|f| {
        f.y < feet - STEP
            && spans(level, column)
                .iter()
                .any(|s| s.collider == f.collider && s.one_way && s.bottom < feet)
    })
}

/// The floor of `column` as an airborne body with its feet at `feet`
/// meets it: as [`floor`], but without ground whose underside is above
/// the feet, which the body passes under or rises into.
pub(crate) fn floor_in_air(level: &Level, column: i64, feet: Fixed) -> Option<Floor> {
    let head = feet - HEIGHT;
    spans(level, column)
        .into_iter()
        .find(|s| s.bottom >= feet && s.top <= feet + STEP + STEP)
        .map(|s| Floor {
            y: s.top.max(head),
            collider: s.collider,
        })
}

/// Whether `column` is in a sloped piece of ground: of the runs of
/// `piece` columns from x = 0, one where the floor, as seen by a body
/// with its feet at `feet`, climbs or falls between two neighbouring
/// columns by no more than `slope`, neither of them part of a steep
/// face. The small steps joining a face to the floor at its ends don't
/// make a piece sloped.
pub(crate) fn sloped(level: &Level, column: i64, feet: Fixed, slope: Fixed, piece: i64) -> bool {
    let first = column.div_euclid(piece) * piece;
    let floors: Vec<_> = (first..first + piece)
        .map(|c| {
            floor(level, c, feet)
                .map(|f| f.y)
                .filter(|_| face(level, c, feet, slope).is_none())
        })
        .collect();
    floors.windows(2).any(|w| match w {
        [Some(a), Some(b)] => a != b && (*a - *b).abs() <= slope,
        _ => false,
    })
}

/// Where the ground is under the middle of `column`, from the top down.
pub(crate) fn spans(level: &Level, column: i64) -> Vec<Span> {
    level.collision.spans(middle(column))
}

/// Whether there's floor within reach below feet at `feet` in `column`:
/// the nearest top at or below the feet line (`feet` rounded up) is at
/// most `reach` px down, and at most `bands` bands of `band` px below
/// the feet line's band. The line on a band's bottom edge is in it.
pub(crate) fn floor_within(
    level: &Level,
    column: i64,
    feet: Fixed,
    reach: i64,
    band: i64,
    bands: i64,
) -> bool {
    let feet = ceil(feet);
    let band_of = |y: i64| (y - 1).div_euclid(band.max(1));
    spans(level, column)
        .into_iter()
        .map(|s| ceil(s.top))
        .filter(|&top| top >= feet)
        .min()
        .is_some_and(|top| top - feet <= reach && band_of(top) - band_of(feet) <= bands)
}

/// The one-way platform in `column` whose layer holds feet at `feet`:
/// below its top, and level with its underside or above.
pub(crate) fn layer(level: &Level, column: i64, feet: Fixed) -> Option<Span> {
    spans(level, column)
        .into_iter()
        .find(|s| s.one_way && s.top < feet && feet <= s.bottom)
}

/// How much higher the floor of `to` is than the floor of `from`, px.
/// `None` if either has no floor.
fn rise(level: &Level, from: i64, to: i64, feet: Fixed) -> Option<Fixed> {
    Some(floor(level, from, feet)?.y - floor(level, to, feet)?.y)
}

/// Whether a change of `height` from one column to the next is part of a
/// steep face rather than walkable ground or a wall.
fn steep(height: Option<Fixed>, slope: Fixed) -> bool {
    height.is_some_and(|h| h.abs() > slope && h.abs() <= STEP)
}

/// Whether the floor changes steeply from `from` to its neighbour `to` as
/// part of a steep face: the change next to it on either side is steep
/// the same way too. A single steep step between other floor isn't part
/// of one.
fn face_step(level: &Level, from: i64, to: i64, feet: Fixed, slope: Fixed) -> bool {
    let way = to - from;
    let height = rise(level, from, to, feet);
    let same = |h: Option<Fixed>| {
        steep(h, slope)
            && h.zip(height)
                .is_some_and(|(h, height)| (h > Fixed::ZERO) == (height > Fixed::ZERO))
    };
    steep(height, slope)
        && (same(rise(level, to, to + way, feet)) || same(rise(level, from - way, from, feet)))
}

/// The way a steep face falls at `column`, -1 (left) or 1 (right), or
/// `None` if the column isn't part of one. A column is part of a face if
/// the floor changes steeply from it to either neighbour as part of a
/// face; the face falls toward a neighbour it drops steeply to, or else
/// away from one it climbs steeply to.
pub(crate) fn face(level: &Level, column: i64, feet: Fixed, slope: Fixed) -> Option<i32> {
    // How much higher `column` is than its neighbour `n`, if steeply.
    let step = |n: i64| {
        face_step(level, n, column, feet, slope)
            .then(|| rise(level, n, column, feet))
            .flatten()
    };
    let (right, left) = (step(column + 1), step(column - 1));
    let falls = |h: Option<Fixed>| h.is_some_and(|h| h > Fixed::ZERO);
    match (falls(right), falls(left), right.is_some(), left.is_some()) {
        (true, ..) => Some(1),
        (_, true, ..) => Some(-1),
        (_, _, true, _) => Some(-1),
        (_, _, _, true) => Some(1),
        _ => None,
    }
}

/// As [`face`], for the nearest floor at or below `feet` in `column`,
/// however far down.
pub(crate) fn face_below(level: &Level, column: i64, feet: Fixed, slope: Fixed) -> Option<i32> {
    let top = spans(level, column)
        .into_iter()
        .map(|s| s.top)
        .filter(|&top| top >= feet)
        .min()?;
    face(level, column, top, slope)
}

/// Whether a grounded body moving `dir` (-1 or 1) is stopped going into
/// `column`: the floor climbs into it from the column behind by more than
/// a step, a wall, or it's part of a face that climbs into it or onward
/// from it. A single step up of a step or less is walked up. Returns the
/// collider that stops the body.
pub(crate) fn blocks(
    level: &Level,
    column: i64,
    dir: i32,
    feet: Fixed,
    slope: Fixed,
) -> Option<usize> {
    let dir = i64::from(dir);
    let into = rise(level, column - dir, column, feet);
    let onward = rise(level, column, column + dir, feet);
    let rises = |h: Option<Fixed>| h.is_some_and(|h| h > Fixed::ZERO);
    let climbs = into.is_some_and(|h| h > STEP)
        || (rises(into) && face_step(level, column - dir, column, feet, slope))
        || (rises(onward) && face_step(level, column, column + dir, feet, slope));
    if climbs {
        floor(level, column, feet).map(|f| f.collider)
    } else {
        None
    }
}

/// Whether a body at `feet` moving `dir` (-1 or 1) from column `from`
/// runs into `column` as into a wall: its floor is higher than the feet
/// and holds from below, and the column is part of a steep face climbing
/// the way the body moves, or the floor climbs into it from `from` by
/// more than a step, or there's no floor at `from` to climb from.
pub(crate) fn hits_side(
    level: &Level,
    from: i64,
    column: i64,
    dir: i32,
    feet: Fixed,
    slope: Fixed,
) -> bool {
    let Some(f) = floor(level, column, feet) else {
        return false;
    };
    let one_way = spans(level, column)
        .iter()
        .any(|s| s.collider == f.collider && s.one_way);
    if f.y >= feet || one_way {
        return false;
    }
    let into = rise(level, from, column, feet);
    face(level, column, feet, slope) == Some(-dir) || into.is_none_or(|h| h > STEP)
}

/// Whether the floor `ahead` columns from `column` in direction `dir` is
/// a step or more higher than the floor of `column`.
pub(crate) fn rise_ahead(level: &Level, column: i64, dir: i32, ahead: i64, feet: Fixed) -> bool {
    rise(level, column, column + i64::from(dir) * ahead, feet).is_some_and(|h| h >= STEP)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A level whose floor has the given height in each column from
    /// column 0, drawn as a staircase, with flat floor at the first height
    /// to the left and the last to the right.
    fn staircase(heights: &[i32]) -> Level {
        let last = *heights.last().unwrap();
        let mut points = vec![[-200, 400], [-200, heights[0]]];
        for (x, &h) in (0..).zip(heights) {
            points.extend([[x, h], [x + 1, h]]);
        }
        let end = heights.len() as i32 + 200;
        points.extend([[end, last], [end, 400]]);
        points.dedup();
        let src = format!(
            "format = 1\nname = \"steps\"\nsize = [1000, 600]\nstart = [0, 0]\n\
             [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n"
        );
        Level::from_toml_str(&src, "steps").unwrap()
    }

    /// The steepest walkable change between columns in these tests: a
    /// face's ends join the floor by up to 3 px.
    fn slope() -> Fixed {
        Fixed::from_int(3)
    }

    #[test]
    fn floors_are_measured_under_the_middle_of_columns() {
        let level = staircase(&[0, 0, 2, 6, 10]);
        let feet = Fixed::ZERO;
        let y = |c| floor(&level, c, feet).map(|f| f.y);
        assert_eq!(y(1), Some(Fixed::ZERO));
        assert_eq!(y(2), Some(Fixed::from_int(2)));
        assert_eq!(y(4), Some(Fixed::from_int(10)));
        // Two steps below the feet is as far down as it looks.
        let above = |c| floor(&level, c, Fixed::from_int(-7)).map(|f| f.y);
        assert_eq!(above(3), Some(Fixed::from_int(6)));
        assert_eq!(above(4), None);
    }

    #[test]
    fn faces_fall_away_from_where_they_climb() {
        // Flat, then a face rising to the right, joined by 1 px at the
        // bottom and 3 px at the top.
        let level = staircase(&[0, 0, -1, -5, -9, -13, -16, -16]);
        let feet = Fixed::ZERO;
        let face = |c| face(&level, c, feet, slope());
        assert_eq!(face(1), None);
        assert_eq!(face(2), Some(-1));
        assert_eq!(face(4), Some(-1));
        assert_eq!(face(5), Some(-1));
        assert_eq!(face(6), None);
        // A wall is not a face.
        let wall = staircase(&[0, 0, -64, -64]);
        assert_eq!(super::face(&wall, 1, feet, slope()), None);
        assert_eq!(
            super::face(&wall, 2, feet - Fixed::from_int(64), slope()),
            None
        );
    }

    #[test]
    fn rises_block_from_their_first_column() {
        let level = staircase(&[0, 0, -1, -5, -9]);
        let feet = Fixed::ZERO;
        assert_eq!(blocks(&level, 1, 1, feet, slope()), None);
        assert_eq!(blocks(&level, 2, 1, feet, slope()), Some(0));
        assert_eq!(blocks(&level, 3, 1, feet, slope()), Some(0));
        // Going down it nothing blocks.
        let feet = Fixed::from_int(-9);
        assert_eq!(blocks(&level, 3, -1, feet, slope()), None);
        assert_eq!(blocks(&level, 1, -1, feet, slope()), None);
        // Gentle slopes never block.
        let gentle = staircase(&[0, 0, 0, 0, -1, -1, -1, -1, -2]);
        for c in 0..9 {
            assert_eq!(blocks(&gentle, c, 1, Fixed::ZERO, slope()), None, "{c}");
        }
    }

    #[test]
    fn one_way_platforms_above_the_feet_arent_there_in_the_air() {
        let src = "format = 1\nname = \"p\"\nsize = [100, 100]\nstart = [0, 0]\n\
                   [[collision]]\nshape = \"segment\"\nfrom = [0, -20]\nto = [10, -20]\n\
                   one_way = true\n\
                   [[collision]]\nshape = \"polygon\"\n\
                   points = [[0, 0], [10, 0], [10, 50], [0, 50]]\n";
        let level = Level::from_toml_str(src, "p").unwrap();
        let y = |feet: i32| floor_in_air(&level, 5, Fixed::from_int(feet)).map(|f| f.y);
        assert_eq!(y(0), Some(Fixed::ZERO));
        assert_eq!(y(-20), Some(Fixed::from_int(-20)));
        assert_eq!(y(-25), Some(Fixed::from_int(-20)));
    }

    /// Floor at y = 0 in columns 0 to 99, and a one-way platform over
    /// columns 20 to 39 from `top` down to `underside`.
    fn under_platform(top: i32, underside: i32) -> Level {
        let src = format!(
            "format = 1\nname = \"p\"\nsize = [100, 100]\nstart = [0, 0]\n\
             [[collision]]\nshape = \"polygon\"\n\
             points = [[20, {top}], [40, {top}], [40, {underside}], [20, {underside}]]\n\
             one_way = true\n\
             [[collision]]\nshape = \"polygon\"\n\
             points = [[0, 0], [100, 0], [100, 32], [0, 32]]\n"
        );
        Level::from_toml_str(&src, "p").unwrap()
    }

    #[test]
    fn platforms_count_as_floor_only_right_over_the_feet() {
        let y = |level: &Level| floor(level, 25, Fixed::ZERO).map(|f| f.y);
        // With 2 px or more under it, a platform isn't there.
        assert_eq!(y(&under_platform(-30, -2)), Some(Fixed::ZERO));
        assert_eq!(y(&under_platform(-12, -5)), Some(Fixed::ZERO));
        // Right over the feet, its top is the floor.
        assert_eq!(y(&under_platform(-12, -1)), Some(Fixed::from_int(-12)));
        assert_eq!(y(&under_platform(-4, -1)), Some(Fixed::from_int(-4)));
        // More than a step up, it's fallen through rather than walked into.
        let feet = Fixed::ZERO;
        assert!(falls_through(&under_platform(-12, -1), 20, feet));
        assert!(!falls_through(&under_platform(-8, -1), 20, feet));
        assert!(!falls_through(&under_platform(-12, -2), 20, feet));
        assert!(!falls_through(&under_platform(-12, -1), 19, feet));
        // A square end a step high or less is walked up, not blocked.
        assert_eq!(blocks(&under_platform(-8, -1), 20, 1, feet, slope()), None);
        assert_eq!(blocks(&under_platform(-4, -1), 20, 1, feet, slope()), None);
    }

    #[test]
    fn a_single_steep_step_isnt_a_face() {
        let level = staircase(&[0, 0, -4, -4, -4]);
        let feet = Fixed::ZERO;
        assert_eq!(face(&level, 1, feet, slope()), None);
        assert_eq!(face(&level, 2, feet, slope()), None);
        assert_eq!(blocks(&level, 2, 1, feet, slope()), None);
        // Two in a row are.
        let level = staircase(&[0, 0, -4, -8, -8]);
        assert_eq!(face(&level, 2, feet, slope()), Some(-1));
        assert_eq!(blocks(&level, 2, 1, feet, slope()), Some(0));
    }

    #[test]
    fn faces_and_walls_are_hit_from_the_side_but_slopes_arent() {
        // Flat, then a face rising to the right from 1 px up.
        let level = staircase(&[0, 0, -1, -5, -9, -13]);
        let feet = Fixed::ZERO;
        assert!(!hits_side(&level, 0, 1, 1, feet, slope()));
        assert!(hits_side(&level, 1, 2, 1, feet, slope()));
        assert!(hits_side(&level, 3, 4, 1, feet, slope()));
        // Level with its floor or above it, nothing is hit.
        assert!(!hits_side(&level, 1, 2, 1, Fixed::from_int(-1), slope()));
        // Going the other way, the face falls.
        let falling = staircase(&[-13, -9, -5, -1, 0, 0]);
        assert!(!hits_side(&falling, 1, 2, 1, Fixed::ZERO, slope()));
        let gentle = staircase(&[0, 0, 0, 0, -1, -1, -1, -1, -2]);
        for c in 0..9 {
            assert!(
                !hits_side(&gentle, c - 1, c, 1, Fixed::ZERO, slope()),
                "{c}"
            );
        }
        let wall = staircase(&[0, 0, -64, -64]);
        assert!(hits_side(&wall, 1, 2, 1, Fixed::ZERO, slope()));
        assert!(!hits_side(&wall, 0, 1, 1, Fixed::ZERO, slope()));
    }

    #[test]
    fn the_steps_at_the_ends_of_a_face_dont_make_a_piece_sloped() {
        // Pieces of 4 columns: flat, then a face from column 5 joined by
        // 1 px at its foot and 3 px at its top, then flat.
        let level = staircase(&[0, 0, 0, 0, 0, -1, -5, -9, -12, -12, -12, -12]);
        let sloped = |c| sloped(&level, c, Fixed::ZERO, slope(), 4);
        for c in 0..12 {
            assert!(!sloped(c), "{c}");
        }
        let gentle = staircase(&[0, 0, 0, 0, 0, -1, -1, -2]);
        assert!(super::sloped(&gentle, 4, Fixed::ZERO, slope(), 4));
        assert!(!super::sloped(&gentle, 0, Fixed::ZERO, slope(), 4));
    }

    #[test]
    fn walls_block_only_the_column_past_them() {
        let level = staircase(&[0, 0, -64, -64]);
        let feet = Fixed::ZERO;
        assert_eq!(blocks(&level, 1, 1, feet, slope()), None);
        assert_eq!(blocks(&level, 2, 1, feet, slope()), Some(0));
        assert!(rise_ahead(&level, 0, 1, 2, feet));
        assert!(!rise_ahead(&level, 0, 1, 1, feet));
        // A rise of a step is enough to stop a start.
        let step = staircase(&[0, 0, -8, -8]);
        assert!(rise_ahead(&step, 0, 1, 2, feet));
        let lower = staircase(&[0, 0, -5, -5]);
        assert!(!rise_ahead(&lower, 0, 1, 2, feet));
    }
}
