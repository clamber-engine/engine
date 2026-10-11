//! The collision layer: solid areas and segments, separate from visuals,
//! with fast deterministic queries.

use std::collections::BTreeMap;

use clamber_fixed::Fixed;

use crate::geom::{self, Vec2};

/// A piece of collision geometry. The set is meant to grow: a new kind of
/// shape is a new variant and a new `shape` name in the level format.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Shape {
    /// A solid area: a simple polygon, in either winding order. Its edges
    /// can be any angle, so slopes are just edges.
    Solid(Vec<Vec2>),
    /// A line at any angle. Solid from both sides, unless `one_way`: then
    /// it only holds up what lands on it from above, like a thin platform.
    Segment { a: Vec2, b: Vec2, one_way: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Collider {
    pub shape: Shape,
    /// A surface type name, defined in the level's or the built-in surfaces.
    pub surface: String,
}

/// A direction to probe in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Down,
    Up,
    Left,
    Right,
}

/// The nearest collision found by [`Collision::cast`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hit {
    /// Index into [`Collision::colliders`].
    pub collider: usize,
    pub distance: Fixed,
    pub point: Vec2,
    /// The edge that was hit, from left to right (top to bottom for a
    /// vertical edge). Its direction gives the slope.
    pub edge: (Vec2, Vec2),
    pub one_way: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Edge {
    a: Vec2,
    b: Vec2,
    collider: usize,
    one_way: bool,
}

/// log2 of the broadphase cell size in pixels.
const CELL_SHIFT: u32 = 8;

fn cell(v: Fixed) -> i64 {
    (v.to_bits() >> clamber_fixed::FRAC_BITS) >> CELL_SHIFT
}

/// Colliders with a grid index over their edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collision {
    pub colliders: Vec<Collider>,
    edges: Vec<Edge>,
    grid: BTreeMap<(i64, i64), Vec<usize>>,
}

impl Collision {
    pub fn new(colliders: Vec<Collider>) -> Collision {
        let mut edges = Vec::new();
        for (i, c) in colliders.iter().enumerate() {
            match &c.shape {
                Shape::Solid(points) => {
                    for (n, &a) in points.iter().enumerate() {
                        let b = points[(n + 1) % points.len()];
                        edges.push(edge(a, b, i, false));
                    }
                }
                Shape::Segment { a, b, one_way } => edges.push(edge(*a, *b, i, *one_way)),
            }
        }
        let mut grid: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        for (n, e) in edges.iter().enumerate() {
            for cx in cell(e.a.x.min(e.b.x))..=cell(e.a.x.max(e.b.x)) {
                for cy in cell(e.a.y.min(e.b.y))..=cell(e.a.y.max(e.b.y)) {
                    grid.entry((cx, cy)).or_default().push(n);
                }
            }
        }
        Collision {
            colliders,
            edges,
            grid,
        }
    }

    /// The nearest edge within `max` of `from` in direction `dir`,
    /// including one at distance 0. One-way segments are only found going
    /// down. Ties go to the edge listed first.
    pub fn cast(&self, from: Vec2, dir: Dir, max: Fixed) -> Option<Hit> {
        let vertical = matches!(dir, Dir::Down | Dir::Up);
        let (along, across) = if vertical {
            (from.y, from.x)
        } else {
            (from.x, from.y)
        };
        let (lo, hi) = match dir {
            Dir::Down | Dir::Right => (along, along + max),
            Dir::Up | Dir::Left => (along - max, along),
        };
        let mut candidates: Vec<usize> = (cell(lo)..=cell(hi))
            .flat_map(|c| {
                let key = if vertical {
                    (cell(across), c)
                } else {
                    (c, cell(across))
                };
                self.grid.get(&key).into_iter().flatten().copied()
            })
            .collect();
        candidates.sort_unstable();
        candidates.dedup();

        let mut best: Option<Hit> = None;
        for n in candidates {
            let e = &self.edges[n];
            if e.one_way && dir != Dir::Down {
                continue;
            }
            // the edge must cross the ray's line and not lie along it
            let (pos, point) = if vertical {
                if e.a.x == e.b.x || across < e.a.x || across > e.b.x {
                    continue;
                }
                let y = geom::y_at(e.a, e.b, across);
                (y, Vec2::new(across, y))
            } else {
                let (top, bottom) = (e.a.y.min(e.b.y), e.a.y.max(e.b.y));
                if e.a.y == e.b.y || across < top || across > bottom {
                    continue;
                }
                let x = geom::x_at(e.a, e.b, across);
                (x, Vec2::new(x, across))
            };
            if pos < lo || pos > hi {
                continue;
            }
            let distance = (pos - along).abs();
            if best.is_none_or(|b| distance < b.distance) {
                best = Some(Hit {
                    collider: e.collider,
                    distance,
                    point,
                    edge: (e.a, e.b),
                    one_way: e.one_way,
                });
            }
        }
        best
    }

    /// The first solid area containing `p`, if any.
    pub fn solid_at(&self, p: Vec2) -> Option<usize> {
        self.colliders.iter().position(|c| match &c.shape {
            Shape::Solid(points) => geom::polygon_contains(points, p),
            Shape::Segment { .. } => false,
        })
    }
}

/// An edge ordered left to right, or top to bottom when vertical.
fn edge(a: Vec2, b: Vec2, collider: usize, one_way: bool) -> Edge {
    let (a, b) = if (a.x, a.y) <= (b.x, b.y) {
        (a, b)
    } else {
        (b, a)
    };
    Edge {
        a,
        b,
        collider,
        one_way,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i32, y: i32) -> Vec2 {
        Vec2::ints(x, y)
    }

    fn f(n: i32) -> Fixed {
        Fixed::from_int(n)
    }

    fn world() -> Collision {
        let ground = Collider {
            shape: Shape::Solid(vec![v(0, 100), v(1000, 100), v(1000, 200), v(0, 200)]),
            surface: "normal".into(),
        };
        // a slope rising to the right, from (300, 100) to (500, 50)
        let ramp = Collider {
            shape: Shape::Solid(vec![v(300, 100), v(500, 50), v(500, 100)]),
            surface: "normal".into(),
        };
        let ledge = Collider {
            shape: Shape::Segment {
                a: v(600, 40),
                b: v(700, 40),
                one_way: true,
            },
            surface: "slippery".into(),
        };
        Collision::new(vec![ground, ramp, ledge])
    }

    #[test]
    fn cast_down_onto_flat_slope_and_platform() {
        let w = world();
        let hit = w.cast(v(100, 0), Dir::Down, f(500)).unwrap();
        assert_eq!((hit.collider, hit.distance), (0, f(100)));

        let hit = w.cast(v(400, 0), Dir::Down, f(500)).unwrap();
        assert_eq!(hit.collider, 1);
        assert_eq!(hit.point, v(400, 75));
        assert_eq!(hit.edge, (v(300, 100), v(500, 50)));

        let hit = w.cast(v(650, 0), Dir::Down, f(500)).unwrap();
        assert!(hit.one_way);
        assert_eq!(hit.distance, f(40));

        // out of reach
        assert_eq!(w.cast(v(100, 0), Dir::Down, f(99)), None);
        // exactly at reach, and touching
        assert!(w.cast(v(100, 0), Dir::Down, f(100)).is_some());
        assert_eq!(w.cast(v(100, 100), Dir::Down, f(5)).unwrap().distance, f(0));
    }

    #[test]
    fn one_way_only_blocks_from_above() {
        let w = world();
        // jumping up through the ledge at y = 40 finds nothing
        assert_eq!(w.cast(v(650, 60), Dir::Up, f(100)), None);
        assert_eq!(w.cast(v(550, 40), Dir::Right, f(500)), None);
    }

    #[test]
    fn cast_sideways_into_walls() {
        let w = world();
        let hit = w.cast(v(450, 80), Dir::Right, f(500)).unwrap();
        assert_eq!((hit.collider, hit.distance), (1, f(50)));
        let hit = w.cast(v(450, 80), Dir::Left, f(500)).unwrap();
        // the ramp's sloped edge at y = 80 is at x = 380
        assert_eq!(hit.point, v(380, 80));
        assert_eq!(w.cast(v(450, 80), Dir::Left, f(69)), None);
    }

    #[test]
    fn broadphase_spans_cells() {
        // a long platform across many cells, probed far from its ends
        let long = Collider {
            shape: Shape::Segment {
                a: v(-5000, 300),
                b: v(5000, 300),
                one_way: false,
            },
            surface: "normal".into(),
        };
        let w = Collision::new(vec![long]);
        assert_eq!(
            w.cast(v(-4321, -900), Dir::Down, f(2000)).unwrap().distance,
            f(1200)
        );
        assert_eq!(
            w.cast(v(1234, 1000), Dir::Up, f(700)).unwrap().distance,
            f(700)
        );
    }

    #[test]
    fn solid_containment() {
        let w = world();
        assert_eq!(w.solid_at(v(50, 150)), Some(0));
        assert_eq!(w.solid_at(v(480, 70)), Some(1));
        assert_eq!(w.solid_at(v(50, 50)), None);
    }
}
