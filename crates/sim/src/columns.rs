//! The ground measured column by column, for physics with `face_slope`.
//!
//! The level is split into 1-px-wide columns of `x`, and each column's
//! floor is the top of the ground under its middle. A body's column is
//! the one holding its `x`. Between neighbouring columns the floor climbs
//! or falls by some amount:
//!
//! - by `face_slope` or less, it's walkable ground: flat floor and slopes;
//! - by more than that, up to a step ([`STEP`]), the columns are part of
//!   a steep face, which carries a grounded body down it and stops one
//!   going up it;
//! - by more than a step, it's a wall.
//!
//! These rules don't care how the level draws its ground: a face can be a
//! steep edge of a polygon or a staircase of small steps.

use clamber_fixed::Fixed;
use clamber_level::{Dir, Level, Span, Vec2};

use crate::player::{HEIGHT, STEP};

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

/// The floor of `column` as seen by a body with its feet at `feet`: the
/// top of the ground under the column's middle, from a body's height
/// above the feet to two steps below them. Ground that reaches a body's
/// height above the feet or more counts as that high. `None` if there's
/// no ground in that range.
pub(crate) fn floor(level: &Level, column: i64, feet: Fixed) -> Option<Floor> {
    let top = Vec2::new(middle(column), feet - HEIGHT);
    if let Some(collider) = level.collision.solid_at(top) {
        return Some(Floor { y: top.y, collider });
    }
    let across = (Fixed::ZERO, Fixed::ZERO);
    level
        .collision
        .sweep(top, Dir::Down, HEIGHT + STEP + STEP, across, |_| true)
        .map(|hit| Floor {
            y: hit.point.y,
            collider: hit.collider,
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

/// Where the ground is under the middle of `column`, from the top down.
pub(crate) fn spans(level: &Level, column: i64) -> Vec<Span> {
    level.collision.spans(middle(column))
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

/// The way a steep face falls at `column`, -1 (left) or 1 (right), or
/// `None` if the column isn't part of one. A column is part of a face if
/// the floor changes steeply from it to either neighbour; the face falls
/// toward a neighbour it drops steeply to, or else away from one it
/// climbs steeply to.
pub(crate) fn face(level: &Level, column: i64, feet: Fixed, slope: Fixed) -> Option<i32> {
    let right = rise(level, column + 1, column, feet);
    let left = rise(level, column - 1, column, feet);
    let falls = |h: Option<Fixed>| steep(h, slope) && h.is_some_and(|h| h > Fixed::ZERO);
    match (
        falls(right),
        falls(left),
        steep(right, slope),
        steep(left, slope),
    ) {
        (true, ..) => Some(1),
        (_, true, ..) => Some(-1),
        (_, _, true, _) => Some(-1),
        (_, _, _, true) => Some(1),
        _ => None,
    }
}

/// Whether a grounded body moving `dir` (-1 or 1) is stopped going into
/// `column`: the floor climbs into it from the column behind by more than
/// `slope`, a wall included, or it's part of a face that climbs onward
/// from it. Returns the collider that stops the body.
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
    let climbs = into.is_some_and(|h| h > slope)
        || (steep(onward, slope) && onward.is_some_and(|h| h > Fixed::ZERO));
    if climbs {
        floor(level, column, feet).map(|f| f.collider)
    } else {
        None
    }
}

/// Whether the floor `ahead` columns from `column` in direction `dir` is
/// more than a step higher than the floor of `column`.
pub(crate) fn rise_ahead(level: &Level, column: i64, dir: i32, ahead: i64, feet: Fixed) -> bool {
    rise(level, column, column + i64::from(dir) * ahead, feet).is_some_and(|h| h > STEP)
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
        assert_eq!(
            floor(&level, 5, Fixed::ZERO).map(|f| f.y),
            Some(Fixed::from_int(-20))
        );
        assert_eq!(y(0), Some(Fixed::ZERO));
        assert_eq!(y(-20), Some(Fixed::from_int(-20)));
        assert_eq!(y(-25), Some(Fixed::from_int(-20)));
    }

    #[test]
    fn walls_block_only_the_column_past_them() {
        let level = staircase(&[0, 0, -64, -64]);
        let feet = Fixed::ZERO;
        assert_eq!(blocks(&level, 1, 1, feet, slope()), None);
        assert_eq!(blocks(&level, 2, 1, feet, slope()), Some(0));
        assert!(rise_ahead(&level, 0, 1, 2, feet));
        assert!(!rise_ahead(&level, 0, 1, 1, feet));
    }
}
