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

/// The floor of `column` for a thrown object at `y`: the top of the
/// ground that holds `y`, even above it, or else of the nearest ground
/// below, down to two steps. Ground whose top is more than a step above
/// `y` isn't a floor: the object moves on as if there were none
/// (thrown-objects.md, "The floor of a column"). One-way platforms count
/// as other ground does, so an object below a platform's underside has
/// the floor under the platform.
pub(crate) fn object_floor(level: &Level, column: i64, y: Fixed) -> Option<Floor> {
    spans(level, column)
        .into_iter()
        .find(|s| s.bottom >= y)
        .filter(|s| s.top >= y - STEP && s.top <= y + STEP + STEP)
        .map(|s| Floor {
            y: s.top,
            collider: s.collider,
        })
}

/// The ground in `column` that stops a thrown object rising from `y` to
/// `to`, as it stops a rising player's feet (see
/// [`Player`](crate::Player)): ground that isn't one-way, whose
/// underside `y` is below, with `to` rounded up in it, below its top and
/// level with its underside or above. Returns how far the stop moves the
/// object back down from `to`: the whole px it rose.
pub(crate) fn stops_rise(level: &Level, column: i64, y: Fixed, to: Fixed) -> Option<Fixed> {
    let (before, after) = (ceil(y), ceil(to));
    let line = Fixed::from_int(after as i32);
    spans(level, column)
        .into_iter()
        .any(|s| !s.one_way && y > s.bottom && s.top < line && line <= s.bottom)
        .then(|| Fixed::from_int((before - after) as i32))
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

/// How much the floor of the sloped piece holding `column` changes per
/// column on average, as seen by a body with its feet at `feet`: over
/// the neighbouring columns of the piece, neither part of a steep face,
/// whose floors differ by `slope` or less. 0 if the piece isn't sloped
/// (see [`sloped`]).
pub(crate) fn gradient(level: &Level, column: i64, feet: Fixed, slope: Fixed, piece: i64) -> Fixed {
    if piece <= 0 || !sloped(level, column, feet, slope, piece) {
        return Fixed::ZERO;
    }
    let first = column.div_euclid(piece) * piece;
    let floors: Vec<_> = (first..first + piece)
        .map(|c| {
            floor(level, c, feet)
                .map(|f| f.y)
                .filter(|_| face(level, c, feet, slope).is_none())
        })
        .collect();
    let changes: Vec<_> = floors
        .windows(2)
        .filter_map(|w| match w {
            [Some(a), Some(b)] if (*a - *b).abs() <= slope => Some((*a - *b).abs()),
            _ => None,
        })
        .collect();
    let total = changes.iter().fold(Fixed::ZERO, |sum, &c| sum + c);
    match changes.len() as i64 {
        0 => Fixed::ZERO,
        n => Fixed::from_bits(total.to_bits() / n),
    }
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

/// Whether `column` has ground for a body with its feet at `feet`, from
/// a body's height above the feet down to `depth` px below them. Ground
/// reaching higher counts.
pub(crate) fn ground_down_to(level: &Level, column: i64, feet: Fixed, depth: Fixed) -> bool {
    spans(level, column)
        .iter()
        .any(|s| s.bottom >= feet - HEIGHT && s.top <= feet + depth)
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

/// Whether the floor of `column` is more than a step higher than that of
/// the column before it going `dir` (-1 or 1): one of the rises
/// [`blocks`] stops at that isn't a face.
pub(crate) fn steps_up(level: &Level, column: i64, dir: i32, feet: Fixed) -> bool {
    rise(level, column - i64::from(dir), column, feet).is_some_and(|h| h > STEP)
}

/// The ground holding a thrown object's `y` in `column`, or else the
/// nearest below, down to two steps: as [`object_floor`], however far
/// above `y` its top is. Breaking sees the floor this way
/// (thrown-objects.md, "Breaking against a wall").
fn object_ground(level: &Level, column: i64, y: Fixed) -> Option<Span> {
    spans(level, column)
        .into_iter()
        .find(|s| s.bottom >= y)
        .filter(|s| s.top <= y + STEP + STEP)
}

/// Whether a thrown object at `y` moving `dir` (-1 or 1) breaks on a move
/// from column `from` to column `to` (thrown-objects.md, "Breaking
/// against a wall"). It does when `to` is part of a steep face climbing
/// the way it moves whose floor ([`object_ground`]) is higher than `y`,
/// by any amount. It also does when the floor of `to` is more than a
/// step higher than `y`, if that ground isn't one-way or the move passes
/// such a face column on the way, but not while it's inside one-way
/// ground in `from`, which it can only have come up into from below.
/// Moves within a column don't break it.
pub(crate) fn breaks_object(
    level: &Level,
    from: i64,
    to: i64,
    dir: i32,
    y: Fixed,
    slope: Fixed,
) -> bool {
    if from == to {
        return false;
    }
    let higher = |column: i64| object_ground(level, column, y).filter(|s| s.top < y);
    let face_up = |column: i64| {
        higher(column).is_some_and(|s| face(level, column, s.top, slope) == Some(-dir))
    };
    if face_up(to) {
        return true;
    }
    let Some(end) = higher(to).filter(|s| s.top < y - STEP) else {
        return false;
    };
    if higher(from).is_some_and(|s| s.one_way) {
        return false;
    }
    let dir = i64::from(dir);
    !end.one_way || (1..=(to - from).abs()).any(|n| face_up(from + dir * n))
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

    /// thrown-objects.md, "The floor of a column": inside a platform's
    /// layer up to a step below its top, the top is the floor; lower in
    /// it there's none; below its underside, the floor under it.
    #[test]
    fn a_thrown_objects_floor_can_be_a_step_above_it() {
        let level = under_platform(-20, -4);
        let y = |feet: f64| {
            object_floor(&level, 25, Fixed::from_f64(feet).unwrap()).map(|f| f.y.to_f64())
        };
        assert_eq!(y(-40.0), None);
        assert_eq!(y(-30.0), Some(-20.0));
        assert_eq!(y(-25.0), Some(-20.0));
        assert_eq!(y(-12.0), Some(-20.0));
        assert_eq!(y(-11.5), None);
        assert_eq!(y(-4.0), None);
        assert_eq!(y(-3.5), Some(0.0));
        assert_eq!(y(0.0), Some(0.0));
        // Solid ground too.
        assert_eq!(y(4.0), Some(0.0));
        assert_eq!(y(8.5), None);
    }

    /// thrown-objects.md, "Ground passable from below": rising into
    /// ground that isn't one-way from below its underside stops, keeping
    /// the whole px; a one-way platform doesn't.
    #[test]
    fn solid_ground_stops_a_rise_from_below() {
        let src = "format = 1\nname = \"p\"\nsize = [100, 100]\nstart = [0, 0]\n\
                   [[collision]]\nshape = \"rect\"\nrect = [0, -20, 10, 16]\n\
                   [[collision]]\nshape = \"rect\"\nrect = [20, -20, 10, 16]\n\
                   one_way = true\n";
        let level = Level::from_toml_str(src, "p").unwrap();
        let f = |v: f64| Fixed::from_f64(v).unwrap();
        let stop = |column, y, to| stops_rise(&level, column, f(y), f(to)).map(|b| b.to_f64());
        // From 1.25 below the underside to 0.5 above it: back 2 px.
        assert_eq!(stop(5, -2.75, -4.5), Some(2.0));
        // Ending on the underside is in the ground.
        assert_eq!(stop(5, -2.75, -4.0), Some(2.0));
        assert_eq!(stop(5, -2.75, -3.5), None);
        // Level with the top or above, it isn't.
        assert_eq!(stop(5, -2.75, -20.5), None);
        assert_eq!(stop(25, -2.75, -4.5), None);
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
    fn faces_and_walls_break_thrown_objects_but_slopes_dont() {
        // Flat, then a face rising to the right from 1 px up.
        let level = staircase(&[0, 0, -1, -5, -9, -13]);
        let feet = Fixed::ZERO;
        let breaks =
            |level: &Level, from, to, feet| breaks_object(level, from, to, 1, feet, slope());
        assert!(!breaks(&level, 0, 1, feet));
        assert!(breaks(&level, 1, 2, feet));
        assert!(breaks(&level, 3, 4, Fixed::from_int(-5)));
        // However much higher a face column's floor is.
        assert!(breaks(&level, 3, 4, feet));
        assert!(breaks(&level, 4, 5, Fixed::from_int(12)));
        // Level with its floor or above it, nothing is hit.
        assert!(!breaks(&level, 1, 2, Fixed::from_int(-1)));
        // Within a column, nothing is hit.
        assert!(!breaks(&level, 2, 2, feet));
        // Going the other way, the face falls.
        let falling = staircase(&[-13, -9, -5, -1, 0, 0]);
        assert!(!breaks(&falling, 1, 2, Fixed::ZERO));
        let gentle = staircase(&[0, 0, 0, 0, -1, -1, -1, -1, -2]);
        for c in 0..9 {
            assert!(!breaks(&gentle, c - 1, c, Fixed::ZERO), "{c}");
        }
        let wall = staircase(&[0, 0, -64, -64]);
        assert!(breaks(&wall, 1, 2, Fixed::ZERO));
        assert!(!breaks(&wall, 0, 1, Fixed::ZERO));
        // Up to a step below its top, a wall is landed on, not hit.
        assert!(!breaks(&wall, 1, 2, Fixed::from_int(-56)));
        assert!(breaks(&wall, 1, 2, Fixed::from_int(-55)));
    }

    /// A platform from column 10 whose left edge climbs as a face of 4
    /// steps of 4 px to its top 16 px above the floor at 0, then flat on
    /// to column 30, and another as high from column 50 to 60 with
    /// upright sides, both `one_way` or not.
    fn edge(one_way: bool) -> Level {
        let mut points = vec![[10, 4]];
        for k in 0..4 {
            points.extend([[10 + k, -4 * k], [11 + k, -4 * k]]);
        }
        points.extend([[14, -16], [30, -16], [30, 4]]);
        let src = format!(
            "format = 1\nname = \"edge\"\nsize = [100, 100]\nstart = [0, 0]\n\
             [[collision]]\nshape = \"rect\"\nrect = [-50, 0, 150, 50]\n\
             [[collision]]\nshape = \"polygon\"\npoints = {points:?}\n\
             one_way = {one_way}\n\
             [[collision]]\nshape = \"rect\"\nrect = [50, -16, 10, 20]\n\
             one_way = {one_way}\n"
        );
        Level::from_toml_str(&src, "edge").unwrap()
    }

    /// thrown-objects.md, "A floor more than 8 px higher": a move into a
    /// column more than a step higher that isn't a face column breaks it
    /// when that ground isn't one-way, or when the move passes a face
    /// column; not inside one-way ground it came up into from below.
    #[test]
    fn a_floor_more_than_a_step_higher_breaks_thrown_objects() {
        for one_way in [false, true] {
            let level = edge(one_way);
            let breaks = |from, to, feet: i32| {
                breaks_object(&level, from, to, 1, Fixed::from_int(feet), slope())
            };
            // Past the whole face into its top, 16 px higher.
            assert!(breaks(8, 16, 0), "{one_way}");
            // Into the top from no lower than a step below it.
            assert!(!breaks(8, 16, -8), "{one_way}");
            // Into the top past no face column.
            assert_eq!(breaks(49, 50, 0), !one_way, "{one_way}");
            assert_eq!(breaks(45, 52, 0), !one_way, "{one_way}");
            // Inside the platform, moving on through it.
            assert_eq!(breaks(14, 18, -4), !one_way, "{one_way}");
        }
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
