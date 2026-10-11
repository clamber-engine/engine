//! Paths drawn as curves, evaluated in exact integer arithmetic so walking
//! a path is the same on every platform.

use clamber_level::{Fixed, Vec2};

/// Polyline points per curve segment between two waypoints.
const STEPS: i128 = 16;

fn w(f: Fixed) -> i128 {
    i128::from(f.to_bits())
}

fn fx(bits: i128) -> Fixed {
    Fixed::from_bits(bits as i64)
}

/// A Catmull-Rom curve through `points` (at least two), as a polyline that
/// starts and ends exactly on the first and last points.
pub fn smooth(points: &[Vec2]) -> Vec<Vec2> {
    assert!(points.len() >= 2);
    let n = points.len();
    let at = |i: isize| points[i.clamp(0, n as isize - 1) as usize];
    let mut out = vec![points[0]];
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (
            at(i as isize - 1),
            at(i as isize),
            at(i as isize + 1),
            at(i as isize + 2),
        );
        for k in 1..=STEPS {
            // 2·S³·p(k/S), then divided back down
            let axis = |a: Fixed, b: Fixed, c: Fixed, d: Fixed| {
                let (a, b, c, d) = (w(a), w(b), w(c), w(d));
                let num = 2 * b * STEPS.pow(3)
                    + (c - a) * k * STEPS.pow(2)
                    + (2 * a - 5 * b + 4 * c - d) * k.pow(2) * STEPS
                    + (3 * b - a - 3 * c + d) * k.pow(3);
                fx(num.div_euclid(2 * STEPS.pow(3)))
            };
            out.push(Vec2::new(
                axis(p0.x, p1.x, p2.x, p3.x),
                axis(p0.y, p1.y, p2.y, p3.y),
            ));
        }
    }
    out
}

/// Length of the segment ab, rounded down.
pub fn distance(a: Vec2, b: Vec2) -> Fixed {
    let (dx, dy) = (w(b.x) - w(a.x), w(b.y) - w(a.y));
    // bits are scaled by 2^32, so the root of the squared sum is too
    fx(((dx * dx + dy * dy) as u128).isqrt() as i128)
}

/// Total length of a polyline.
pub fn length(line: &[Vec2]) -> Fixed {
    line.windows(2)
        .fold(Fixed::ZERO, |sum, p| sum + distance(p[0], p[1]))
}

/// Cosine of the angle between two directions, as a fixed-point number
/// from -1 to 1, or `None` if either has no length.
pub fn cosine(a: Vec2, b: Vec2) -> Option<Fixed> {
    let zero = Vec2::default();
    let (la, lb) = (w(distance(zero, a)), w(distance(zero, b)));
    if la == 0 || lb == 0 {
        return None;
    }
    let dot = w(a.x) * w(b.x) + w(a.y) * w(b.y);
    // dot is scaled by 2^64 and la·lb by 2^64, so scale up once more
    Some(fx((dot << 32).div_euclid(la * lb)))
}

/// Something moving along a polyline at a given speed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Walker {
    line: Vec<Vec2>,
    segment: usize,
    /// Distance travelled along the current segment.
    along: Fixed,
}

impl Walker {
    pub fn new(line: Vec<Vec2>) -> Walker {
        assert!(!line.is_empty());
        Walker {
            line,
            segment: 0,
            along: Fixed::ZERO,
        }
    }

    pub fn arrived(&self) -> bool {
        self.segment + 1 >= self.line.len()
    }

    /// Moves `step` further along; returns the new position.
    pub fn advance(&mut self, step: Fixed) -> Vec2 {
        self.along += step;
        while !self.arrived() {
            let seg = distance(self.line[self.segment], self.line[self.segment + 1]);
            if self.along < seg {
                break;
            }
            self.along -= seg;
            self.segment += 1;
        }
        self.position()
    }

    pub fn position(&self) -> Vec2 {
        if self.arrived() {
            return *self.line.last().unwrap();
        }
        let (a, b) = (self.line[self.segment], self.line[self.segment + 1]);
        let seg = w(distance(a, b));
        if seg == 0 {
            return a;
        }
        let lerp = |p: Fixed, q: Fixed| fx(w(p) + ((w(q) - w(p)) * w(self.along)).div_euclid(seg));
        Vec2::new(lerp(a.x, b.x), lerp(a.y, b.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i32, y: i32) -> Vec2 {
        Vec2::ints(x, y)
    }

    #[test]
    fn curve_passes_through_points() {
        let pts = [v(0, 0), v(100, 50), v(200, 0)];
        let line = smooth(&pts);
        assert_eq!(line.len(), 1 + 2 * STEPS as usize);
        assert_eq!(line[0], pts[0]);
        assert_eq!(line[STEPS as usize], pts[1]);
        assert_eq!(*line.last().unwrap(), pts[2]);
        // it bulges smoothly: the middle of the first span is above the chord
        assert!(line[STEPS as usize / 2].y > Fixed::from_int(25));
    }

    #[test]
    fn straight_two_point_path() {
        let line = smooth(&[v(0, 0), v(160, 0)]);
        assert_eq!(length(&line), Fixed::from_int(160));
        assert!(line.iter().all(|p| p.y == Fixed::ZERO));
    }

    #[test]
    fn distances_and_angles() {
        assert_eq!(distance(v(0, 0), v(3, 4)), Fixed::from_int(5));
        assert_eq!(cosine(v(1, 0), v(5, 0)), Some(Fixed::ONE));
        assert_eq!(cosine(v(1, 0), v(0, 7)), Some(Fixed::ZERO));
        assert_eq!(cosine(v(1, 0), v(-2, 0)), Some(-Fixed::ONE));
        assert_eq!(cosine(v(0, 0), v(1, 0)), None);
    }

    #[test]
    fn walker_moves_at_constant_speed() {
        let mut w = Walker::new(vec![v(0, 0), v(10, 0), v(10, 10)]);
        assert_eq!(w.advance(Fixed::from_int(4)), v(4, 0));
        assert_eq!(w.advance(Fixed::from_int(10)), v(10, 4));
        assert!(!w.arrived());
        assert_eq!(w.advance(Fixed::from_int(100)), v(10, 10));
        assert!(w.arrived());
    }
}
