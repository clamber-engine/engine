//! Collectibles and the collectible count in Classic 1
//! (spec/behavior/collectibles.md, spec/behavior/bunches.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{
    Behavior, Event, Facing, FlowEvent, InputFrame, Phase, Riding, START_LIVES, View, World,
    generic_feel, step,
};

use Action::{Down, Jump, Left};

/// The player stands at x = 600 on flat ground at y = 400.
const X: i32 = 600;
const F: i32 = 400;

/// A level with the player standing at (`X`, `F`) and things of `kind`
/// at `at`.
fn level(kind: &str, at: &[(i32, i32)]) -> Arc<Level> {
    let mut src = format!(
        r#"
        format = 1
        name = "collect"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]
        "#
    );
    for (x, y) in at {
        src += &format!("\n[[entities]]\ntype = \"{kind}\"\nat = [{x}, {y}]\n");
    }
    Arc::new(Level::from_toml_str(&src, "collect").unwrap())
}

/// Classic 1 with one character, or the second character leading.
fn world(kind: &str, at: &[(i32, i32)], second: bool) -> World {
    let mut feel = generic_feel("classic-1").unwrap();
    if second {
        feel.roster.push(CharacterFeel {
            physics: Presets::builtin().slot("classic-1", 1).unwrap(),
            abilities: BTreeMap::new(),
        });
    }
    let mut world = World::new(level(kind, at), Arc::new(feel));
    if second {
        world.player.character = 1;
        if let Some(other) = &mut world.other {
            other.character = 0;
        }
    }
    assert!(world.player.grounded());
    world
}

/// What a step left behind.
#[derive(Clone, Debug)]
struct Row {
    x: i32,
    feet: i32,
    vy: Fixed,
    ground: bool,
    events: Vec<Event>,
    taken: Vec<usize>,
    gems: u32,
    lives: u8,
    life: bool,
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(world: World) -> Play {
        Play {
            world,
            held: Actions::NONE,
        }
    }

    fn step(&mut self, keys: &[Action]) -> Row {
        let held: Actions = keys.iter().copied().collect();
        step(
            &mut self.world,
            &InputFrame::single(Frame::after(self.held, held)),
        );
        self.held = held;
        let w = &self.world;
        let p = &w.player;
        Row {
            x: p.pos.x.floor() as i32,
            feet: -((-p.pos.y).floor() as i32),
            vy: p.vel.y,
            ground: p.grounded(),
            events: w.events.clone(),
            taken: w
                .flow
                .events
                .iter()
                .filter_map(|e| match e {
                    FlowEvent::Collect(i) => Some(*i),
                    _ => None,
                })
                .collect(),
            gems: w.flow.gems,
            lives: w.flow.lives,
            life: w.flow.events.contains(&FlowEvent::Life),
        }
    }

    fn hold(&mut self, keys: &[Action], frames: usize) -> Vec<Row> {
        (0..frames).map(|_| self.step(keys)).collect()
    }

    /// One step standing still, with the view held still for it with
    /// its left edge at `left` and, if given, its top at `top`.
    fn step_viewed(&mut self, left: i32, top: Option<i32>) -> Row {
        let view = self.world.view.unwrap();
        self.world.view = Some(View {
            left: Fixed::from_int(left),
            top: top.map_or(view.top, Fixed::from_int),
            hold: 2,
            ..view
        });
        self.step(&[])
    }
}

/// The steps on which the count went up from 0, once for each time it
/// went up on that step.
fn count_ups(rows: &[Row]) -> Vec<usize> {
    let mut before = 0;
    let mut ups = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        ups.extend(std::iter::repeat_n(n, (row.gems - before) as usize));
        before = row.gems;
    }
    ups
}

/// Whether a thing at `(x, y)` is taken on the first step, standing
/// still.
fn taken_standing(kind: &str, x: i32, y: i32, second: bool) -> bool {
    let mut play = Play::new(world(kind, &[(x, y)], second));
    !play.step(&[]).taken.is_empty()
}

/// Checks that a standing player takes a `kind` exactly when `I` and `D`
/// are in the given ranges, at and just past each end, facing right.
fn check_standing(kind: &str, second: bool, i: (i32, i32), d: (i32, i32)) {
    for past in [i.0 - 1, i.0, i.1, i.1 + 1] {
        for below in [d.0 - 1, d.0, d.1, d.1 + 1] {
            let want = (i.0..=i.1).contains(&past) && (d.0..=d.1).contains(&below);
            assert_eq!(
                taken_standing(kind, X - past, F - below, second),
                want,
                "{kind}: I = {past}, D = {below}"
            );
        }
    }
}

#[test]
fn a_standing_first_character_takes_a_single_from_its_ranges() {
    check_standing("spark", false, (-16, 22), (-7, 41));
}

#[test]
fn a_standing_first_character_takes_a_bunch_from_its_ranges() {
    check_standing("cluster", false, (-15, 21), (-8, 40));
}

#[test]
fn a_standing_second_character_takes_a_single_from_its_ranges() {
    check_standing("spark", true, (-13, 12), (-9, 39));
}

#[test]
fn ranges_are_measured_the_way_the_player_faces() {
    // Facing left, X - xc runs from -22 to 16.
    let facing_left = |dx: i32| {
        let mut play = Play::new(world("spark", &[(X - dx, F)], false));
        play.world.player.facing = clamber_sim::Facing::Left;
        !play.step(&[]).taken.is_empty()
    };
    assert!(!facing_left(-23));
    assert!(facing_left(-22));
    assert!(facing_left(16));
    assert!(!facing_left(17));
}

/// The step on which a crouching player first takes a `kind` at offsets
/// `past`, `below`, holding down from the first step, if within 30.
fn crouch_take(kind: &str, second: bool, past: i32, below: i32) -> Option<usize> {
    let mut play = Play::new(world(kind, &[(X - past, F - below)], second));
    (0..30).find(|_| !play.step(&[Down]).taken.is_empty())
}

#[test]
fn crouching_reaches_further_ahead_and_less_high() {
    // In the crouching range, not the standing one: taken once crouching
    // starts (crouch_start = 2: on the second step). Just past it and
    // out of the standing range: never.
    assert_eq!(crouch_take("spark", false, -19, 0), Some(1));
    assert_eq!(crouch_take("spark", false, -20, 0), None);
    assert_eq!(crouch_take("spark", false, 13, -11), Some(1));
    assert_eq!(crouch_take("spark", false, 14, -11), None);
    assert_eq!(crouch_take("spark", false, -19, 33), None);
    assert_eq!(crouch_take("cluster", false, -18, -12), Some(1));
    assert_eq!(crouch_take("cluster", false, -19, 0), None);
    assert_eq!(crouch_take("spark", true, -20, 16), Some(1));
    assert_eq!(crouch_take("spark", true, -20, 17), None);
}

/// The first character's ranges for a single, as collectibles.md gives
/// them, for a step that ends `age` steps into a jump (`None`: not in
/// one), `since_land` steps after landing, standing still otherwise.
fn single_ranges(row: &Row, age: Option<usize>, since_land: Option<usize>) -> [i32; 4] {
    const STAND: [i32; 4] = [-16, 22, -7, 41];
    let late = |vy: Fixed| {
        let lowest = if vy <= Fixed::from_ratio_pow2(-9, 1) {
            20
        } else if vy <= Fixed::from_ratio_pow2(-7, 1) {
            21
        } else if vy <= Fixed::from_ratio_pow2(-5, 1) {
            22
        } else if vy <= Fixed::ZERO {
            14
        } else {
            16
        };
        [-16, 19, lowest, 58]
    };
    match (age, since_land) {
        (Some(a), _) if a <= 6 => STAND,
        (Some(_), _) => late(row.vy),
        (None, Some(n)) if n <= 6 => [-16, 19, 16, 58],
        _ => STAND,
    }
}

/// The bunch's, as bunches.md gives them.
fn bunch_ranges(_row: &Row, age: Option<usize>, since_land: Option<usize>) -> [i32; 4] {
    const STAND: [i32; 4] = [-15, 21, -8, 40];
    const LATE: [i32; 4] = [-15, 18, 13, 57];
    match (age, since_land) {
        (Some(a), _) if a <= 5 => STAND,
        (Some(_), _) => LATE,
        (None, Some(n)) if n <= 3 => LATE,
        _ => STAND,
    }
}

/// A jump straight up from standing, with Jump held for `held` steps,
/// then standing until well after landing. The steps, with each one's
/// jump age and steps since landing.
fn jump_keys(held: usize) -> Vec<&'static [Action]> {
    let mut keys: Vec<&[Action]> = vec![&[]; 3];
    keys.extend(vec![&[Jump][..]; held]);
    keys.extend(vec![&[][..]; 90 - held]);
    keys
}

fn jump_rows(keys: &[&[Action]]) -> Vec<(Row, Option<usize>, Option<usize>)> {
    let mut play = Play::new(world("spark", &[], false));
    let mut start = None;
    let mut land = None;
    let mut out = Vec::new();
    for (n, k) in keys.iter().enumerate() {
        let row = play.step(k);
        if row.events.contains(&Event::JumpStart) {
            start = Some(n);
        }
        if row.events.contains(&Event::Land) {
            start = None;
            land = Some(n);
        }
        let age = start.map(|s| n - s);
        let since = land.filter(|_| row.ground).map(|l| n - l);
        out.push((row, age, since));
    }
    assert!(land.is_some(), "the jump lands");
    out
}

/// For things of `kind` at many offsets from where the player jumps,
/// checks that each is taken on the first step whose ranges hold it.
fn check_jump(kind: &str, held: usize, ranges: fn(&Row, Option<usize>, Option<usize>) -> [i32; 4]) {
    let keys = jump_keys(held);
    let rows = jump_rows(&keys);
    for past in [-17, -16, -15, 18, 19, 20, 21, 22, 23] {
        for below in -10..64 {
            let want = rows.iter().position(|(row, age, since)| {
                let [i0, i1, d0, d1] = ranges(row, *age, *since);
                let (i, d) = (row.x - (X - past), row.feet - (F - below));
                (i0..=i1).contains(&i) && (d0..=d1).contains(&d)
            });
            let mut play = Play::new(world(kind, &[(X - past, F - below)], false));
            let got = keys.iter().position(|k| !play.step(k).taken.is_empty());
            assert_eq!(
                got, want,
                "{kind}, jump held {held}: I = {past}, D = {below}"
            );
        }
    }
}

#[test]
fn a_single_is_taken_in_a_jump_as_its_speed_says() {
    check_jump("spark", 40, single_ranges);
}

#[test]
fn a_single_is_taken_in_a_short_jump_as_its_speed_says() {
    check_jump("spark", 4, single_ranges);
}

#[test]
fn a_bunch_is_taken_in_a_jump_from_six_steps_on_with_one_range() {
    check_jump("cluster", 40, bunch_ranges);
    check_jump("cluster", 4, bunch_ranges);
}

/// Classic 1 on flat ground at `F` up to x = 700 and, with `drop`, a
/// floor 176 px lower beyond it (else the same floor on), the player at
/// (500, `F`), and things of `kind` at `at`.
fn ledge_world(kind: &str, drop: bool, at: &[(i32, i32)]) -> World {
    let lower = if drop { F + 176 } else { F };
    let mut src = format!(
        r#"
        format = 1
        name = "ledge"
        size = [3000, 800]
        start = [500, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 700, 400]

        [[collision]]
        shape = "rect"
        rect = [700, {lower}, 2300, 200]
        "#
    );
    for (x, y) in at {
        src += &format!("\n[[entities]]\ntype = \"{kind}\"\nat = [{x}, {y}]\n");
    }
    let level = Arc::new(Level::from_toml_str(&src, "ledge").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

/// Checks that a `kind` placed anywhere around where a jump facing right
/// with `keys` lands is taken on the first step whose ranges hold it,
/// with the jump's ranges up to `hold` steps after the landing step
/// (bunches.md and letters.md, "Landing").
fn check_landing(kind: &str, drop: bool, keys: &[Vec<Action>], hold: usize) {
    let mut play = Play::new(ledge_world(kind, drop, &[]));
    let rows: Vec<Row> = keys.iter().map(|k| play.step(k)).collect();
    let start = rows
        .iter()
        .position(|r| r.events.contains(&Event::JumpStart))
        .unwrap();
    let land = rows
        .iter()
        .position(|r| r.events.contains(&Event::Land))
        .unwrap();
    assert!(rows[land..].iter().all(|r| r.ground));
    let ranges = |n: usize| -> [i32; 4] {
        const STAND: [i32; 4] = [-15, 21, -8, 40];
        const LATE: [i32; 4] = [-15, 18, 13, 57];
        if (start + 6..land + hold).contains(&n) {
            LATE
        } else {
            STAND
        }
    };
    let (xl, fl) = (rows[land].x, rows[land].feet);
    for past in -30..30 {
        for below in [-9, -8, 12, 13, 40, 41, 50, 57, 58] {
            let at = (xl - past, fl - below);
            let want = rows.iter().enumerate().position(|(n, row)| {
                let [i0, i1, d0, d1] = ranges(n);
                let (i, d) = (row.x - at.0, row.feet - at.1);
                (i0..=i1).contains(&i) && (d0..=d1).contains(&d)
            });
            let mut play = Play::new(ledge_world(kind, drop, &[at]));
            let got = keys.iter().position(|k| !play.step(k).taken.is_empty());
            assert_eq!(
                got, want,
                "{kind}: I = {past}, D = {below} from the landing"
            );
        }
    }
}

/// `before` steps of `moving`, then Jump with `moving` until landing and
/// beyond.
fn jump_on(moving: &[Action], before: usize) -> Vec<Vec<Action>> {
    let mut keys = vec![moving.to_vec(); before];
    keys.extend(vec![[moving, &[Jump]].concat(); 100]);
    keys
}

#[test]
fn a_landing_keeps_the_jump_ranges_while_the_airborne_pose_goes_on() {
    use Action::{Action as Act, Right};
    for kind in ["cluster", "letter"] {
        // Walking right off the ledge onto the floor 176 px lower: L + 4.
        check_landing(kind, true, &jump_on(&[Right], 60), 5);
        // Running on with Action and Right held through the landing: up
        // to L, or L + 1 onto the lower floor.
        check_landing(kind, false, &jump_on(&[Right, Act], 60), 1);
        check_landing(kind, true, &jump_on(&[Right, Act], 60), 2);
        // Walking right onto the same floor: L + 3.
        check_landing(kind, false, &jump_on(&[Right], 60), 4);
    }
}

#[test]
fn a_letter_landing_from_a_jump_in_place_keeps_the_jump_ranges_to_l_plus_3() {
    // Jump held 3, 10, 14 or 30 steps (letters.md, "Landing").
    for held in [3, 10, 14, 30] {
        let mut keys = vec![vec![]; 10];
        keys.extend(vec![vec![Jump]; held]);
        keys.extend(vec![vec![]; 100]);
        check_landing("letter", false, &keys, 4);
    }
}

#[test]
fn only_the_character_under_control_takes_collectibles() {
    // The other character starts with the player, and stays when the
    // player walks off; a spark at its feet but out of the player's reach
    // isn't taken.
    let mut play = Play::new(world("spark", &[(X - 40, F)], true));
    let rows = play.hold(&[Left], 30);
    let first = rows.iter().position(|r| !r.taken.is_empty()).unwrap();
    let row = &rows[first];
    // Taken by the player's own reach (walking, facing left: X - xc from
    // -13 to 19).
    assert!((-13..=19).contains(&(row.x - (X - 40))), "{row:?}");
}

#[test]
fn a_single_counts_33_steps_after_it_is_taken() {
    let mut play = Play::new(world("spark", &[(X, F)], false));
    let rows = play.hold(&[], 40);
    assert_eq!(rows[0].taken, [0]);
    assert!(rows[..33].iter().all(|r| r.gems == 0));
    assert_eq!(rows[33].gems, 1);
    assert!(!play.world.things[0].active);
    assert!(play.hold(&[], 40).iter().all(|r| r.taken.is_empty()));
}

#[test]
fn a_bunch_counts_its_ten_items_six_steps_apart() {
    let mut play = Play::new(world("cluster", &[(X, F)], false));
    let rows = play.hold(&[], 80);
    assert_eq!(rows[0].taken, [0]);
    let ups: Vec<usize> = (1..rows.len())
        .filter(|&n| rows[n].gems > rows[n - 1].gems)
        .collect();
    assert_eq!(ups, [17, 23, 29, 35, 41, 47, 53, 59, 65, 71]);
    assert_eq!(rows[79].gems, 10);
}

#[test]
fn the_count_gives_a_life_at_100_and_starts_again() {
    let mut play = Play::new(world("cluster", &[(X, F)], false));
    play.world.flow.gems = 95;
    let rows = play.hold(&[], 80);
    let life = rows.iter().position(|r| r.life).unwrap();
    assert_eq!(life, 41);
    assert_eq!(rows[life].gems, 0);
    assert_eq!(rows[life].lives, START_LIVES + 1);
    assert_eq!(rows[79].gems, 5);
    assert_eq!(rows.iter().filter(|r| r.life).count(), 1);
}

#[test]
fn the_default_preset_keeps_counting_past_100() {
    let src = format!(
        r#"
        format = 1
        name = "gems"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]

        [[entities]]
        type = "gem"
        at = [{X}, {}]
        "#,
        F - 8
    );
    let level = Arc::new(Level::from_toml_str(&src, "gems").unwrap());
    let mut play = Play::new(World::new(
        level,
        Arc::new(generic_feel("default").unwrap()),
    ));
    play.world.flow.gems = 99;
    let row = play.step(&[]);
    assert_eq!(row.taken, [0]);
    assert_eq!((row.gems, row.lives, row.life), (100, START_LIVES, false));
}

/// Whether a player that rolls right from standing takes a spark 30 px
/// ahead and `below` px above its feet while still rolling.
fn taken_rolling(below: i32) -> bool {
    let mut play = Play::new(world("spark", &[(X + 30, F - below)], false));
    let mut keys = vec![&[Action::Action][..]];
    keys.extend(vec![&[][..]; 40]);
    for k in keys {
        let row = play.step(k);
        if play.world.player.state != clamber_sim::State::Roll && row.x > X {
            return false;
        }
        if !row.taken.is_empty() {
            return true;
        }
    }
    false
}

#[test]
fn rolling_reaches_less_low() {
    // A rolling player's feet are at most 3 px above a single it takes;
    // standing, 7.
    assert!(taken_rolling(-3));
    assert!(!taken_rolling(-4));
    assert!(taken_standing("spark", X, F + 4, false));
}

/// A standing player on a spark, with the view's left edge `at` px left
/// of it for 40 steps: whether it's taken on the first, and the steps on
/// which the count goes up.
fn single_in_view(at: i32) -> (bool, Vec<usize>) {
    let mut play = Play::new(world("spark", &[(X, F)], false));
    let rows: Vec<Row> = (0..40).map(|_| play.step_viewed(X - at, None)).collect();
    let taken = rows[0].taken == [0];
    assert!(rows[1..].iter().all(|r| r.taken.is_empty()));
    (taken, count_ups(&rows))
}

#[test]
fn a_single_is_only_taken_inside_the_view_window() {
    for (at, want) in [(-40, false), (-39, true), (263, true), (264, false)] {
        assert_eq!(single_in_view(at).0, want, "xc - V = {at}");
    }
}

#[test]
fn a_single_counts_by_where_it_is_in_the_view() {
    // collectibles.md, "When the count goes up".
    for (at, ups) in [
        (-39, vec![]),
        (7, vec![]),
        (8, vec![33]),
        (128, vec![33]),
        (255, vec![33]),
        (256, vec![1]),
        (263, vec![1]),
    ] {
        assert_eq!(single_in_view(at), (true, ups), "xc - V = {at}");
    }
}

#[test]
fn a_single_outside_the_window_is_taken_once_it_comes_inside() {
    let mut play = Play::new(world("spark", &[(X, F)], false));
    assert!(play.step_viewed(X - 264, None).taken.is_empty());
    assert!(play.step_viewed(X - 264, None).taken.is_empty());
    assert_eq!(play.step_viewed(X - 263, None).taken, [0]);
}

/// A standing player on a bunch, with the view at the start of step `n`
/// given by `view(n)` as `(xb - V, yb - W)`: the steps on which the
/// count goes up.
fn bunch_in_view(view: impl Fn(usize) -> (i32, i32)) -> Vec<usize> {
    let mut play = Play::new(world("cluster", &[(X, F)], false));
    let rows: Vec<Row> = (0..80)
        .map(|n| {
            let (x, y) = view(n);
            play.step_viewed(X - x, Some(F - y))
        })
        .collect();
    assert_eq!(rows[0].taken, [0]);
    count_ups(&rows)
}

#[test]
fn a_bunch_item_flies_only_well_inside_the_view() {
    // bunches.md, "Releasing the items".
    let flies: Vec<usize> = (0..10).map(|k| 17 + 6 * k).collect();
    let at_once: Vec<usize> = (0..10).map(|k| 6 * k).collect();
    assert_eq!(bunch_in_view(|_| (9, 11)), flies);
    assert_eq!(bunch_in_view(|_| (999, 260)), flies);
    assert_eq!(bunch_in_view(|_| (8, 100)), at_once);
    assert_eq!(bunch_in_view(|_| (100, 10)), at_once);
}

#[test]
fn a_bunch_leaving_the_view_flies_its_first_items_and_counts_the_rest_at_once() {
    // bunches.md's example: `xb - V` drops below 9 just before P + 30.
    let ups = bunch_in_view(|n| (if n < 30 { 100 } else { 8 }, 100));
    assert_eq!(ups, [17, 23, 29, 30, 35, 36, 41, 42, 48, 54]);
}

/// A point in the picture.
type Corner = (i32, i32);

/// The copies drawn after each of 40 steps with a standing player on a
/// spark whose `xc - V` is 100 (the view held still), and the corner
/// the first copy starts from.
fn single_copies() -> (Vec<Vec<Corner>>, Corner) {
    let mut play = Play::new(world("spark", &[(X, F)], false));
    let mut drawn = Vec::new();
    let mut top = 0;
    for _ in 0..40 {
        play.step_viewed(X - 100, None);
        top = play.world.view.unwrap().top.floor() as i32;
        drawn.push(play.world.flying_copies());
    }
    (drawn, (100 - 8, F - 9 - top))
}

#[test]
fn a_single_taken_in_the_view_has_a_copy_fly_to_the_corner_on_the_next_32_steps() {
    // collectibles.md, "The flying copy".
    let (drawn, (sx, sy)) = single_copies();
    assert!(drawn[0].is_empty(), "not on the step it is taken");
    for j in 1..=32 {
        let at = (sx * (32 - j) / 32, sy * (32 - j) / 32);
        assert_eq!(drawn[j as usize], [at], "step P + {j}");
    }
    assert_eq!(drawn[32], [(0, 0)]);
    assert!(drawn[33..].iter().all(Vec::is_empty));
}

#[test]
fn a_single_that_counts_at_once_has_no_copy() {
    let mut play = Play::new(world("spark", &[(X, F)], false));
    for _ in 0..40 {
        play.step_viewed(X - 256, None);
        assert!(play.world.flying_copies().is_empty());
    }
}

#[test]
fn a_bunch_item_has_a_copy_fly_to_the_corner_on_its_release_step_and_the_16_after() {
    // bunches.md, "The flying copy of an item": `xb - V` 75, `yb - W` 46.
    let mut play = Play::new(world("cluster", &[(X, F)], false));
    let drawn: Vec<Vec<(i32, i32)>> = (0..30)
        .map(|_| {
            play.step_viewed(X - 75, Some(F - 46));
            play.world.flying_copies()
        })
        .collect();
    assert_eq!(drawn[0], [(66, 35)]);
    assert_eq!(drawn[1], [(61, 32)]);
    assert_eq!(drawn[2], [(57, 30)]);
    // The items released on steps 6 and 12 are on the same path.
    assert_eq!(drawn[8], [(33, 17), (57, 30)]);
    assert_eq!(drawn[15], [(4, 2), (28, 15), (53, 28)]);
    assert_eq!(drawn[16], [(0, 0), (24, 13), (49, 26)]);
    assert_eq!(drawn[17].len(), 2, "the first item's copy is gone");
}

/// A world with the first character riding the animal on the floor at
/// `X`, facing `facing`, and a `spark` far away to move.
fn riding(facing: Facing) -> Play {
    let src = format!(
        "format = 1\nname = \"riding\"\nsize = [2000, 800]\nstart = [{X}, {F}]\n\
         [[collision]]\nshape = \"rect\"\nrect = [0, {F}, 2000, 200]\n\
         [[entities]]\ntype = \"hutch\"\nat = [40, {F}]\n\
         [[entities]]\ntype = \"spark\"\nat = [1900, {F}]\n"
    );
    let level = Level::from_toml_str(&src, "riding").unwrap();
    let mut world = World::new(
        Arc::new(level),
        Arc::new(generic_feel("classic-1").unwrap()),
    );
    let Some(Behavior::Hutch(hutch)) = world.things[0].archetype.map(|a| a.behavior) else {
        panic!("no animal crate");
    };
    world.things[0].phase = Phase::Ridden;
    world.things[0].pos = world.player.pos;
    world.player.facing = facing;
    world.player.riding = Some(Riding {
        on_floor: true,
        was_on_floor: true,
        mount: None,
        ..Riding::new(hutch.ride, Fixed::ZERO)
    });
    Play::new(world)
}

/// Moves the spark to `dx` right of the rider and `below` above its feet.
fn put_spark(play: &mut Play, dx: i32, below: i32) {
    play.world.things[1].pos = Vec2::new(Fixed::from_int(X + dx), Fixed::from_int(F - below));
}

#[test]
fn riding_has_its_own_ranges_on_every_frame() {
    for (past, below, want) in [
        (-16, 7, true),
        (23, 51, true),
        (-17, 20, false),
        (24, 20, false),
        (0, 6, false),
        (0, 52, false),
    ] {
        let mut play = riding(Facing::Right);
        put_spark(&mut play, -past, below);
        assert_eq!(
            !play.step(&[]).taken.is_empty(),
            want,
            "I = {past}, D = {below}"
        );
    }
}

#[test]
fn a_turn_frame_takes_by_the_facing_before() {
    for (frame, want) in [(4, true), (5, false)] {
        let mut play = riding(Facing::Left);
        play.hold(&[Action::Right], frame);
        // 20 px right of the rider: inside the facing-left range only.
        put_spark(&mut play, 20, 20);
        assert_eq!(
            !play.step(&[Action::Right]).taken.is_empty(),
            want,
            "frame {frame}"
        );
    }
}
