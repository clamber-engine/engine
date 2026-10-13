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
    /// can be any angle, so slopes are just edges. They only stop what
    /// moves into the area, so something inside comes out freely. If
    /// `one_way`, only its top holds anything up, and only what lands on
    /// it from above: a thick platform, passed up through from below.
    Solid { points: Vec<Vec2>, one_way: bool },
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

/// Where a collider is along a vertical line: see [`Collision::spans`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub top: Fixed,
    pub bottom: Fixed,
    /// Index into [`Collision::colliders`].
    pub collider: usize,
    pub one_way: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Edge {
    a: Vec2,
    b: Vec2,
    collider: usize,
    one_way: bool,
    /// For a solid's edge, the signs of its outward normal's x and y.
    /// Segments have none: they're solid from both sides.
    outward: Option<(i8, i8)>,
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
                Shape::Solid { points, one_way } => {
                    let winding = geom::winding(points);
                    for (n, &a) in points.iter().enumerate() {
                        let b = points[(n + 1) % points.len()];
                        // (dy, -dx) points out of an area wound clockwise
                        // on screen, +y down.
                        let sign = |v: Fixed| match v.cmp(&Fixed::ZERO) {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        };
                        let (dx, dy) = (sign(b.x - a.x), sign(b.y - a.y));
                        let outward = (dy * winding, -dx * winding);
                        edges.push(edge(a, b, i, *one_way, Some(outward)));
                    }
                }
                Shape::Segment { a, b, one_way } => edges.push(edge(*a, *b, i, *one_way, None)),
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
    /// down, and a solid's edges only going into it. Ties go to the edge
    /// listed first.
    pub fn cast(&self, from: Vec2, dir: Dir, max: Fixed) -> Option<Hit> {
        self.sweep(from, dir, max, (Fixed::ZERO, Fixed::ZERO), |_| true)
    }

    /// Like [`Collision::cast`], but for a line moving in `dir` that
    /// reaches `across.0` before `from` and `across.1` past it, across the
    /// way it moves (to the left and right going up or down, above and
    /// below going sideways), and only finding hits that `keep` accepts.
    /// The distance is to the nearest part of an edge the line passes
    /// over, from `from`.
    pub fn sweep(
        &self,
        from: Vec2,
        dir: Dir,
        max: Fixed,
        across: (Fixed, Fixed),
        keep: impl Fn(&Hit) -> bool,
    ) -> Option<Hit> {
        let vertical = matches!(dir, Dir::Down | Dir::Up);
        let (along, middle) = if vertical {
            (from.y, from.x)
        } else {
            (from.x, from.y)
        };
        let (near, far) = (middle - across.0, middle + across.1);
        let (lo, hi) = match dir {
            Dir::Down | Dir::Right => (along, along + max),
            Dir::Up | Dir::Left => (along - max, along),
        };
        let forward = matches!(dir, Dir::Down | Dir::Right);
        let mut candidates: Vec<usize> = (cell(lo)..=cell(hi))
            .flat_map(|c| {
                (cell(near)..=cell(far)).flat_map(move |a| {
                    let key = if vertical { (a, c) } else { (c, a) };
                    self.grid.get(&key).into_iter().flatten().copied()
                })
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
            if let Some((nx, ny)) = e.outward {
                let into = match dir {
                    Dir::Down => ny < 0,
                    Dir::Up => ny > 0,
                    Dir::Right => nx < 0,
                    Dir::Left => nx > 0,
                };
                if !into {
                    continue;
                }
            }
            // The part of the edge the line passes over; the edge must
            // cross the line's path and not lie along it.
            let across_of = |v: Vec2| if vertical { v.x } else { v.y };
            let along_of = |v: Vec2| if vertical { v.y } else { v.x };
            let (top, bottom) = if across_of(e.a) <= across_of(e.b) {
                (e.a, e.b)
            } else {
                (e.b, e.a)
            };
            if across_of(top) == across_of(bottom)
                || far < across_of(top)
                || near > across_of(bottom)
            {
                continue;
            }
            // Points on the edge where the line's ends cross it.
            let at_across = |t: Fixed| {
                if vertical {
                    Vec2::new(t, geom::y_at(e.a, e.b, t))
                } else {
                    Vec2::new(geom::x_at(e.a, e.b, t), t)
                }
            };
            let p = if near > across_of(top) {
                at_across(near)
            } else {
                top
            };
            let q = if far < across_of(bottom) {
                at_across(far)
            } else {
                bottom
            };
            // The end nearest `from`, or where the edge comes into reach
            // if that end is behind it.
            let (mut point, other) = if (along_of(p) <= along_of(q)) == forward {
                (p, q)
            } else {
                (q, p)
            };
            let start = if forward { lo } else { hi };
            let behind = |v: Vec2| {
                if forward {
                    along_of(v) < lo
                } else {
                    along_of(v) > hi
                }
            };
            if behind(point) {
                if behind(other) {
                    continue;
                }
                point = if vertical {
                    Vec2::new(geom::x_at(e.a, e.b, start), start)
                } else {
                    Vec2::new(start, geom::y_at(e.a, e.b, start))
                };
            }
            let pos = along_of(point);
            if pos < lo || pos > hi {
                continue;
            }
            let found = Hit {
                collider: e.collider,
                distance: (pos - along).abs(),
                point,
                edge: (e.a, e.b),
                one_way: e.one_way,
            };
            if best.is_none_or(|b| found.distance < b.distance) && keep(&found) {
                best = Some(found);
            }
        }
        best
    }

    /// The first solid area containing `p`, if any. One-way areas don't
    /// count.
    pub fn solid_at(&self, p: Vec2) -> Option<usize> {
        self.colliders.iter().position(|c| match &c.shape {
            Shape::Solid { points, one_way } => !one_way && geom::polygon_contains(points, p),
            Shape::Segment { .. } => false,
        })
    }

    /// Where each collider is along the vertical line at `x`, from the top
    /// down, ties in the order the colliders are listed. A solid area
    /// gives a span from each top edge to the bottom edge below it, and a
    /// segment a span of no height. Vertical edges, and edges that end at
    /// `x` going right, aren't crossed.
    pub fn spans(&self, x: Fixed) -> Vec<Span> {
        let cx = cell(x);
        let mut found: Vec<usize> = self
            .grid
            .range((cx, i64::MIN)..=(cx, i64::MAX))
            .flat_map(|(_, edges)| edges.iter().copied())
            .collect();
        found.sort_unstable();
        found.dedup();
        // Where each edge crosses, by collider and then height.
        let mut crossings: Vec<(usize, Fixed, &Edge)> = found
            .into_iter()
            .map(|n| &self.edges[n])
            .filter(|e| e.a.x <= x && x < e.b.x)
            .map(|e| (e.collider, geom::y_at(e.a, e.b, x), e))
            .collect();
        // A top before a bottom at the same height.
        crossings
            .sort_by_key(|&(collider, y, e)| (collider, y, e.outward.is_some_and(|o| o.1 > 0)));
        let mut spans = Vec::new();
        let mut top = None;
        for (collider, y, e) in crossings {
            match e.outward {
                None => spans.push(Span {
                    top: y,
                    bottom: y,
                    collider,
                    one_way: e.one_way,
                }),
                Some((_, ny)) if ny < 0 => top = Some(y),
                Some(_) => {
                    if let Some(top) = top.take() {
                        spans.push(Span {
                            top,
                            bottom: y,
                            collider,
                            one_way: e.one_way,
                        });
                    }
                }
            }
        }
        spans.sort_by_key(|s| (s.top, s.collider));
        spans
    }
}

/// An edge ordered left to right, or top to bottom when vertical.
fn edge(a: Vec2, b: Vec2, collider: usize, one_way: bool, outward: Option<(i8, i8)>) -> Edge {
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
        outward,
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
            shape: Shape::Solid {
                points: vec![v(0, 100), v(1000, 100), v(1000, 200), v(0, 200)],
                one_way: false,
            },
            surface: "normal".into(),
        };
        // a slope rising to the right, from (300, 100) to (500, 50)
        let ramp = Collider {
            shape: Shape::Solid {
                points: vec![v(300, 100), v(500, 50), v(500, 100)],
                one_way: false,
            },
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
    fn one_way_areas_only_hold_up_from_above() {
        // A platform 15 px thick with sloped ends.
        let platform = Collider {
            shape: Shape::Solid {
                points: vec![v(0, 15), v(20, 0), v(80, 0), v(100, 15)],
                one_way: true,
            },
            surface: "normal".into(),
        };
        let w = Collision::new(vec![platform]);
        let hit = w.cast(v(50, -10), Dir::Down, f(100)).unwrap();
        assert_eq!((hit.point, hit.one_way), (v(50, 0), true));
        assert_eq!(w.cast(v(50, 30), Dir::Up, f(100)), None);
        assert_eq!(w.cast(v(-10, 10), Dir::Right, f(100)), None);
        assert_eq!(w.solid_at(v(50, 10)), None);
    }

    #[test]
    fn spans_run_from_tops_to_bottoms() {
        let w = world();
        let at = |x: i32| {
            w.spans(f(x))
                .into_iter()
                .map(|s| (s.top, s.bottom, s.collider))
                .collect::<Vec<_>>()
        };
        // The ledge at y = 40 and the ground.
        assert_eq!(at(650), [(f(40), f(40), 2), (f(100), f(200), 0)]);
        // The ramp sits on the ground, listed first as it's higher.
        assert_eq!(at(400), [(f(75), f(100), 1), (f(100), f(200), 0)]);
        assert_eq!(at(1500), []);
    }

    #[test]
    fn cast_sideways_into_walls() {
        let w = world();
        let hit = w.cast(v(520, 80), Dir::Left, f(500)).unwrap();
        assert_eq!((hit.collider, hit.distance), (1, f(20)));
        let hit = w.cast(v(250, 80), Dir::Right, f(500)).unwrap();
        // the ramp's sloped edge at y = 80 is at x = 380
        assert_eq!(hit.point, v(380, 80));
        assert_eq!(w.cast(v(250, 80), Dir::Right, f(129)), None);
    }

    #[test]
    fn solids_only_stop_what_moves_into_them() {
        let w = world();
        // from inside the ramp, every way out is open
        for dir in [Dir::Left, Dir::Right, Dir::Up] {
            assert_eq!(w.cast(v(450, 80), dir, f(60)), None, "{dir:?}");
        }
        // and from inside the ground, there's no floor at its bottom
        assert_eq!(w.cast(v(100, 150), Dir::Down, f(500)), None);
        // the same holds whichever way a solid is wound
        let ccw = Collider {
            shape: Shape::Solid {
                points: vec![v(0, 100), v(0, 200), v(100, 200), v(100, 100)],
                one_way: false,
            },
            surface: "normal".into(),
        };
        let w = Collision::new(vec![ccw]);
        assert_eq!(
            w.cast(v(50, 0), Dir::Down, f(500)).unwrap().distance,
            f(100)
        );
        assert_eq!(w.cast(v(50, 150), Dir::Down, f(500)), None);
        assert_eq!(
            w.cast(v(-10, 150), Dir::Right, f(500)).unwrap().distance,
            f(10)
        );
        assert_eq!(w.cast(v(50, 150), Dir::Right, f(500)), None);
    }

    #[test]
    fn sweeps_find_edges_between_the_ends_of_the_line() {
        let w = world();
        // A line from y = 30 to y = 90 moving left from x = 560: the
        // ramp's wall from y = 50 to 100 is in its way.
        let from = v(560, 60);
        let hit = w.sweep(from, Dir::Left, f(500), (f(30), f(30)), |_| true);
        assert_eq!(hit.unwrap().distance, f(60));
        // A line from y = 20 to 30 passes over it.
        let hit = w.sweep(v(560, 25), Dir::Left, f(500), (f(5), f(5)), |_| true);
        assert_eq!(hit, None);
        // Moving right from x = 200, the slope is nearest at its bottom.
        let hit = w.sweep(v(200, 70), Dir::Right, f(500), (f(10), f(20)), |_| true);
        assert_eq!(hit.unwrap().point, v(340, 90));
        // A line already over part of an edge hits it at distance 0.
        let hit = w.sweep(v(400, 70), Dir::Right, f(500), (f(10), f(20)), |_| true);
        assert_eq!(hit.unwrap().distance, f(0));
        // Filtered out, the next one is found.
        let hit = w.sweep(v(200, 70), Dir::Right, f(500), (f(10), f(20)), |h| {
            h.collider != 1
        });
        assert_eq!(hit, None);
        // Going down, the line lands on the ground's top beside the ramp.
        let hit = w.sweep(v(290, 0), Dir::Down, f(500), (f(5), f(5)), |_| true);
        assert_eq!(hit.unwrap().point, v(285, 100));
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
