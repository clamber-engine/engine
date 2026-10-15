//! Collectibles and the collectible count in Classic 1
//! (spec/behavior/collectibles.md, spec/behavior/bunches.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Fixed, Presets};
use clamber_sim::{Event, FlowEvent, InputFrame, START_LIVES, World, generic_feel, step};

use Action::{Down, Jump, Left};

/// The player stands at x = 200 on flat ground at y = 400.
const X: i32 = 200;
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
