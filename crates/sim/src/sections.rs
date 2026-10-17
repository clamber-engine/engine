//! Placed objects in sections (`spec/formats/placements.md`, "In a list
//! with sections"): which of a level's placed objects can appear depends
//! on the path the view has taken, not only on where it is.
//!
//! The placed objects are [`Spot`]s in a fixed order, divided into runs
//! ([`Sections`]) one after another, each pair of neighbouring runs
//! meeting at a point. The **home** run is always open; while a point next
//! to it is near the view, the run on the point's other side is open too,
//! and the edge the view leaves the point by decides which of the two is
//! home afterwards. [`Opening`] keeps that state and says, frame by frame,
//! which spots appear.
//!
//! A level's [`Section`](clamber_level::Section)s give the spots and runs
//! ([`Placed`]). Engine choices where the spec says nothing: a spot of
//! one entity is an object and one of more is a group, whose first and
//! last members are its first and last entities; and an empty section is
//! one marker.

use std::ops::RangeInclusive;
use std::sync::Arc;

use clamber_level::Level;
use clamber_pack::{Param, Physics};

/// Where a spot or a point is near the view: level x − the view's left
/// edge in `across`, and level y − the view's top in `down` (whole px).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub across: RangeInclusive<i64>,
    pub down: RangeInclusive<i64>,
}

impl Window {
    /// The window for a preset's view: `appear_left` left of it to
    /// `appear_right` right of it, and `appear_above` above it to
    /// `appear_below` below it.
    pub fn of(physics: &Physics) -> Window {
        let whole = |param: Param| physics[param].floor();
        Window {
            across: -whole(Param::AppearLeft)..=whole(Param::ViewWidth) + whole(Param::AppearRight),
            down: -whole(Param::AppearAbove)..=whole(Param::ViewHeight) + whole(Param::AppearBelow),
        }
    }

    /// The view's left edge as the window uses it: never so far left that
    /// the window reaches left of level x 1.
    fn left(&self, view_left: i64) -> i64 {
        view_left.max(1 - self.across.start())
    }

    pub(crate) fn holds_x(&self, x: i64, view_left: i64) -> bool {
        self.across.contains(&(x - self.left(view_left)))
    }

    fn holds_y(&self, y: i64, view_top: i64) -> bool {
        self.down.contains(&(y - view_top))
    }

    /// Whether `x` is left of the window.
    fn left_of(&self, x: i64, view_left: i64) -> bool {
        x - self.left(view_left) < *self.across.start()
    }

    /// Whether `x` is right of the window.
    fn right_of(&self, x: i64, view_left: i64) -> bool {
        x - self.left(view_left) > *self.across.end()
    }
}

/// One placed entry, in the run order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// Places nothing, and never holds back the spots after it.
    Marker,
    /// One object at level (`x`, `y`). With `rows`, it appears only when
    /// `y` is inside the window's rows too.
    Object { x: i64, y: i64, rows: bool },
    /// Objects that appear together. The group is left of the window by
    /// its own `x`, and near it while its `first` member's x isn't right
    /// of the window and its `last` member's isn't left of it.
    Group { x: i64, first: i64, last: i64 },
}

impl Spot {
    /// The level x that decides whether the spot is left of the window.
    fn x(self) -> Option<i64> {
        match self {
            Spot::Marker => None,
            Spot::Object { x, .. } | Spot::Group { x, .. } => Some(x),
        }
    }
}

/// How a level's spots are divided into runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sections {
    /// Each run's first and last spot.
    pub runs: Vec<RangeInclusive<usize>>,
    /// Where run `i` and run `i` + 1 meet: one fewer than the runs.
    pub joins: Vec<(i64, i64)>,
}

/// A level's sections as spots and runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub spots: Vec<Spot>,
    pub sections: Sections,
    /// The entities of each spot: none for a marker.
    pub entities: Vec<Vec<usize>>,
    /// The spot of each of the level's entities, if it's in one.
    pub spot_of: Vec<Option<usize>>,
}

impl Placed {
    /// The level's sections, if its entities appear by sections.
    pub fn of(level: &Level) -> Option<Placed> {
        if level.sections.is_empty() {
            return None;
        }
        let whole = |v: clamber_level::Vec2| (v.x.floor(), v.y.floor());
        let mut placed = Placed {
            spots: Vec::new(),
            sections: Sections {
                runs: Vec::new(),
                joins: Vec::new(),
            },
            entities: Vec::new(),
            spot_of: vec![None; level.entities.len()],
        };
        for section in &level.sections {
            let first = placed.spots.len();
            for spot in &section.spots {
                let at = |i: usize| whole(level.entities[spot.entities[i]].at);
                let (x, y) = spot.at.map_or_else(|| at(0), whole);
                placed.spots.push(match spot.entities.len() {
                    1 => Spot::Object {
                        x,
                        y,
                        rows: spot.rows,
                    },
                    n => Spot::Group {
                        x,
                        first: at(0).0,
                        last: at(n - 1).0,
                    },
                });
                for &e in &spot.entities {
                    placed.spot_of[e] = Some(placed.entities.len());
                }
                placed.entities.push(spot.entities.clone());
            }
            if section.spots.is_empty() {
                placed.spots.push(Spot::Marker);
                placed.entities.push(Vec::new());
            }
            placed.sections.runs.push(first..=placed.spots.len() - 1);
            placed.sections.joins.extend(section.meets.map(whole));
        }
        Some(placed)
    }
}

/// A level's things appearing by sections: in a world whose level has
/// sections and whose preset has a view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placing {
    pub placed: Arc<Placed>,
    pub opening: Opening,
    /// Whether each spot's things are there: from when they appear until
    /// the view removes them. Things that leave by their own rule, such
    /// as collected or defeated ones, stay there.
    pub there: Vec<bool>,
}

impl Placing {
    /// At the start of an area whose view's top is at level y `view_top`,
    /// with nothing there yet.
    pub fn begin(placed: Arc<Placed>, view_top: i64) -> Placing {
        Placing {
            opening: Opening::begin(&placed.sections, view_top),
            there: vec![false; placed.spots.len()],
            placed,
        }
    }
}

/// Which edge of the window a point is past.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Past {
    Left,
    Right,
    Top,
    Bottom,
}

/// Where a point is: in the window, or past one of its edges, a side
/// edge counting first.
fn past(window: &Window, (x, y): (i64, i64), view: (i64, i64)) -> Option<Past> {
    let (left, top) = view;
    if window.left_of(x, left) {
        Some(Past::Left)
    } else if window.right_of(x, left) {
        Some(Past::Right)
    } else if y - top < *window.down.start() {
        Some(Past::Top)
    } else if !window.holds_y(y, top) {
        Some(Past::Bottom)
    } else {
        None
    }
}

/// Which runs are open: the home run, and the run across the crossing
/// join, if there is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opening {
    pub home: usize,
    /// The join whose point is near the view, once it has come near.
    pub crossing: Option<usize>,
}

impl Opening {
    /// At the start of an area whose view's top is at level y `view_top`:
    /// the home run is the one after every join, counted from the first,
    /// whose point isn't above the view's top, up to the first that is.
    pub fn begin(sections: &Sections, view_top: i64) -> Opening {
        let home = sections
            .joins
            .iter()
            .take_while(|&&(_, y)| y >= view_top)
            .count();
        Opening {
            home,
            crossing: None,
        }
    }

    /// The run across the crossing join from home, if one is open.
    pub fn far(&self) -> Option<usize> {
        self.crossing
            .map(|join| if self.home == join { join + 1 } else { join })
    }

    /// One frame with the view's left edge and top at `view` (whole px,
    /// as at the start of the frame): moves the opening on, then gives the
    /// spots that appear, in order. `there` tells whether a spot's object
    /// (for a group, its objects) is in the level.
    pub fn step(
        &mut self,
        sections: &Sections,
        spots: &[Spot],
        window: &Window,
        view: (i64, i64),
        there: impl Fn(usize) -> bool,
    ) -> Vec<usize> {
        self.cross(sections, window, view);
        let home = coming_up(&sections.runs[self.home], spots, window, view, &there);
        match self.far() {
            Some(far) => {
                let far = coming_up(&sections.runs[far], spots, window, view, &there);
                // The home run waits while the far run's spots appear.
                if far.is_empty() { home } else { far }
            }
            None => home,
        }
    }

    /// Starts or ends a crossing.
    fn cross(&mut self, sections: &Sections, window: &Window, view: (i64, i64)) {
        match self.crossing {
            None => {
                // The joins on either side of home, the left one first.
                let next = (self.home < sections.joins.len()).then_some(self.home);
                self.crossing = self
                    .home
                    .checked_sub(1)
                    .into_iter()
                    .chain(next)
                    .find(|&j| past(window, sections.joins[j], view).is_none());
            }
            Some(join) => match past(window, sections.joins[join], view) {
                None => {}
                Some(edge) => {
                    // The view has gone right or up past the point, or left
                    // or down past it.
                    let onwards = matches!(edge, Past::Left | Past::Bottom);
                    if onwards && self.home == join {
                        self.home = join + 1;
                    } else if !onwards && self.home == join + 1 {
                        self.home = join;
                    }
                    self.crossing = None;
                }
            },
        }
    }
}

/// The spots of `run` that come up this frame.
fn coming_up(
    run: &RangeInclusive<usize>,
    spots: &[Spot],
    window: &Window,
    (left, top): (i64, i64),
    there: &impl Fn(usize) -> bool,
) -> Vec<usize> {
    let is_left = |i: usize| spots[i].x().is_some_and(|x| window.left_of(x, left));
    if is_left(*run.end()) {
        return Vec::new();
    }
    let lead = run
        .clone()
        .rev()
        .find(|&i| is_left(i))
        .map_or(*run.start(), |i| i + 1);
    let mut up = Vec::new();
    for (i, &spot) in spots.iter().enumerate().take(*run.end() + 1).skip(lead) {
        let clear = match spot {
            Spot::Marker => true,
            Spot::Object { x, .. } => window.holds_x(x, left),
            Spot::Group { first, last, .. } => {
                there(i) || !window.right_of(first, left) && !window.left_of(last, left)
            }
        };
        if !clear {
            break;
        }
        let comes_up = match spot {
            Spot::Marker => false,
            Spot::Object { y, rows, .. } => !there(i) && (!rows || window.holds_y(y, top)),
            Spot::Group { .. } => !there(i),
        };
        if comes_up {
            up.push(i);
        }
    }
    up
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Window {
        Window {
            across: -31..=288,
            down: -31..=256,
        }
    }

    fn two_runs() -> Sections {
        Sections {
            runs: vec![0..=1, 2..=3],
            joins: vec![(500, 300)],
        }
    }

    fn spots() -> Vec<Spot> {
        let object = |x, y| Spot::Object { x, y, rows: true };
        vec![
            object(100, 400),
            object(400, 400),
            object(600, 100),
            object(700, 100),
        ]
    }

    #[test]
    fn the_home_run_starts_after_the_joins_at_or_below_the_top() {
        assert_eq!(Opening::begin(&two_runs(), 300).home, 1);
        assert_eq!(Opening::begin(&two_runs(), 301).home, 0);
    }

    #[test]
    fn the_window_comes_from_the_preset() {
        let physics = clamber_pack::Presets::builtin().world("classic-1").unwrap();
        assert_eq!(Window::of(&physics), window());
    }

    #[test]
    fn left_of_level_x_32_the_window_stays_put() {
        let w = window();
        assert!(w.holds_x(320, 0) && w.holds_x(320, 31) && !w.holds_x(321, 31));
        assert!(w.holds_x(1, 0) && !w.holds_x(0, 0));
        assert!(w.holds_x(321, 33) && !w.holds_x(1, 33));
    }

    #[test]
    fn going_up_past_a_point_moves_home_across_it() {
        let (sections, spots) = (two_runs(), spots());
        let mut open = Opening::begin(&sections, 400);
        let step =
            |open: &mut Opening, view| open.step(&sections, &spots, &window(), view, |_| false);
        assert_eq!(step(&mut open, (300, 400)), [1]);
        assert_eq!(open.crossing, None);
        // The point comes within 31 px above the top.
        assert_eq!(step(&mut open, (300, 331)), [1]);
        assert_eq!(open.crossing, Some(0));
        assert_eq!(open.far(), Some(1));
        // Far below the bottom of the window: past the bottom edge.
        assert_eq!(step(&mut open, (300, 43)), Vec::<usize>::new());
        assert_eq!((open.home, open.crossing), (1, None));
    }

    #[test]
    fn a_side_edge_counts_before_the_top_or_bottom() {
        let sections = two_runs();
        let mut open = Opening {
            home: 1,
            crossing: Some(0),
        };
        // Past the right edge and the bottom edge at once: back to run 0.
        open.cross(&sections, &window(), (100, 0));
        assert_eq!((open.home, open.crossing), (0, None));
        let mut open = Opening {
            home: 1,
            crossing: Some(0),
        };
        // Past the top edge only: home goes back too.
        open.cross(&sections, &window(), (400, 400));
        assert_eq!((open.home, open.crossing), (0, None));
        let mut open = Opening {
            home: 1,
            crossing: Some(0),
        };
        // Past the left edge: home stays.
        open.cross(&sections, &window(), (600, 300));
        assert_eq!((open.home, open.crossing), (1, None));
    }

    #[test]
    fn spots_wait_for_the_ones_before_them() {
        let sections = Sections {
            runs: vec![RangeInclusive::new(0, 2)],
            joins: vec![],
        };
        let object = |x| Spot::Object {
            x,
            y: 0,
            rows: false,
        };
        // The middle spot is stored after one further right.
        let spots = [object(100), object(400), object(200)];
        let mut open = Opening::begin(&sections, 0);
        let mut step = |view| open.step(&sections, &spots, &window(), (view, 0), |_| false);
        assert_eq!(step(0), [0]);
        assert_eq!(step(112), [0, 1, 2]);
        // Once the first is left of the window, the lead moves past it.
        assert_eq!(step(132), [1, 2]);
        // With the last left of the window, nothing comes up.
        assert_eq!(step(500), Vec::<usize>::new());
    }
}
