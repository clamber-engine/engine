//! Points, rectangles and areas in world units (pixels, +y down), as
//! fixed-point numbers so geometry is the same on every platform.

use clamber_fixed::Fixed;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Vec2 {
    pub x: Fixed,
    pub y: Fixed,
}

impl Vec2 {
    pub fn new(x: Fixed, y: Fixed) -> Vec2 {
        Vec2 { x, y }
    }

    pub fn ints(x: i32, y: i32) -> Vec2 {
        Vec2::new(Fixed::from_int(x), Fixed::from_int(y))
    }
}

/// An axis-aligned rectangle: top-left corner and size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: Fixed,
    pub y: Fixed,
    pub w: Fixed,
    pub h: Fixed,
}

impl Rect {
    pub fn right(&self) -> Fixed {
        self.x + self.w
    }

    pub fn bottom(&self) -> Fixed {
        self.y + self.h
    }

    /// Inclusive of the top and left edges, exclusive of the others.
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    pub fn corners(&self) -> Vec<Vec2> {
        vec![
            Vec2::new(self.x, self.y),
            Vec2::new(self.right(), self.y),
            Vec2::new(self.right(), self.bottom()),
            Vec2::new(self.x, self.bottom()),
        ]
    }
}

/// Where a region applies.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    Rect(Rect),
    /// A simple polygon, at least three points.
    Polygon(Vec<Vec2>),
}

impl Area {
    pub fn contains(&self, p: Vec2) -> bool {
        match self {
            Area::Rect(r) => r.contains(p),
            Area::Polygon(points) => polygon_contains(points, p),
        }
    }
}

/// Raw bits widened, so products and cross products can't overflow.
fn w(f: Fixed) -> i128 {
    i128::from(f.to_bits())
}

/// Even-odd rule, half-open edges so a point on a shared edge belongs to
/// exactly one of two neighbouring polygons.
pub(crate) fn polygon_contains(points: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    for (i, &a) in points.iter().enumerate() {
        let b = points[(i + 1) % points.len()];
        if (a.y > p.y) != (b.y > p.y) {
            // x of the edge at p.y, compared without dividing:
            // p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y)
            let lhs = (w(p.x) - w(a.x)) * (w(b.y) - w(a.y));
            let rhs = (w(p.y) - w(a.y)) * (w(b.x) - w(a.x));
            if (lhs < rhs) == (b.y > a.y) {
                inside = !inside;
            }
        }
    }
    inside
}

/// Sign of the cross product (b - a) × (c - a).
pub(crate) fn orient(a: Vec2, b: Vec2, c: Vec2) -> i8 {
    let cross = (w(b.x) - w(a.x)) * (w(c.y) - w(a.y)) - (w(b.y) - w(a.y)) * (w(c.x) - w(a.x));
    cross.signum() as i8
}

/// 1 if the polygon is wound clockwise on screen (+y down), -1 if
/// anticlockwise.
pub(crate) fn winding(points: &[Vec2]) -> i8 {
    let twice_area: i128 = points
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            let b = points[(i + 1) % points.len()];
            w(a.x) * w(b.y) - w(b.x) * w(a.y)
        })
        .sum();
    if twice_area < 0 { -1 } else { 1 }
}

/// Whether segments ab and cd cross or touch.
pub(crate) fn segments_intersect(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    let within = |p: Vec2, q: Vec2, r: Vec2| {
        r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
    };
    (o1 != o2 && o3 != o4)
        || (o1 == 0 && within(a, b, c))
        || (o2 == 0 && within(a, b, d))
        || (o3 == 0 && within(c, d, a))
        || (o4 == 0 && within(c, d, b))
}

/// Why a polygon isn't usable, if it isn't.
pub(crate) fn polygon_problem(points: &[Vec2]) -> Option<&'static str> {
    if points.len() < 3 {
        return Some("needs at least 3 points");
    }
    let n = points.len();
    for i in 0..n {
        if points[i] == points[(i + 1) % n] {
            return Some("has two equal consecutive points");
        }
    }
    for i in 0..n {
        for j in i + 1..n {
            // neighbouring edges share a point; skip them
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (a, b) = (points[i], points[(i + 1) % n]);
            let (c, d) = (points[j], points[(j + 1) % n]);
            if segments_intersect(a, b, c, d) {
                return Some("crosses itself");
            }
        }
    }
    if (0..n).all(|i| orient(points[0], points[1], points[i]) == 0) {
        return Some("has no area");
    }
    None
}

/// The y where segment ab crosses the vertical line at `x`, rounded down
/// to the nearest representable value. `a.x != b.x`.
pub(crate) fn y_at(a: Vec2, b: Vec2, x: Fixed) -> Fixed {
    let num = (w(x) - w(a.x)) * (w(b.y) - w(a.y));
    let y = w(a.y) + num.div_euclid(w(b.x) - w(a.x));
    Fixed::from_bits(y as i64)
}

/// The x where segment ab crosses the horizontal line at `y`. `a.y != b.y`.
pub(crate) fn x_at(a: Vec2, b: Vec2, y: Fixed) -> Fixed {
    let num = (w(y) - w(a.y)) * (w(b.x) - w(a.x));
    let x = w(a.x) + num.div_euclid(w(b.y) - w(a.y));
    Fixed::from_bits(x as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(list: &[(i32, i32)]) -> Vec<Vec2> {
        list.iter().map(|&(x, y)| Vec2::ints(x, y)).collect()
    }

    #[test]
    fn polygon_containment() {
        let tri = pts(&[(0, 0), (10, 0), (0, 10)]);
        assert!(polygon_contains(&tri, Vec2::ints(2, 2)));
        assert!(!polygon_contains(&tri, Vec2::ints(8, 8)));
        // the largest coordinates levels allow don't overflow
        let big = pts(&[(-1 << 30, -1 << 30), (1 << 30, 0), (0, 1 << 30)]);
        assert!(polygon_contains(&big, Vec2::ints(0, 0)));
    }

    #[test]
    fn polygon_problems() {
        assert_eq!(
            polygon_problem(&pts(&[(0, 0), (1, 0)])),
            Some("needs at least 3 points")
        );
        assert_eq!(
            polygon_problem(&pts(&[(0, 0), (10, 10), (10, 0), (0, 10)])),
            Some("crosses itself")
        );
        assert_eq!(
            polygon_problem(&pts(&[(0, 0), (5, 5), (10, 10)])),
            Some("has no area")
        );
        assert_eq!(
            polygon_problem(&pts(&[(0, 0), (10, 0), (10, 10), (0, 10)])),
            None
        );
    }

    #[test]
    fn line_crossings() {
        let (a, b) = (Vec2::ints(0, 10), Vec2::ints(20, 0));
        assert_eq!(y_at(a, b, Fixed::from_int(10)), Fixed::from_int(5));
        assert_eq!(x_at(a, b, Fixed::from_int(5)), Fixed::from_int(10));
    }
}
