//! Swapping places with the other character in Classic 1
//! (spec/behavior/swap.md).

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_pack::{CharacterFeel, Fixed, Param, Presets};
use clamber_sim::{Event, Facing, InputFrame, Other, State, World, generic_feel, record, step};
use clamber_trace::Scenario;

/// Flat ground at y = 400 from x = 0 to 4096.
const FLAT: &str = r#"
    format = 1
    name = "flat"
    size = [4096, 600]
    start = [400, 400]

    [[collision]]
    shape = "rect"
    rect = [0, 400, 4096, 200]
"#;

const S: Action = Action::Swap;
const J: Action = Action::Jump;
const R: Action = Action::Right;
const L: Action = Action::Left;
const D: Action = Action::Down;
const A: Action = Action::Action;
const T: Action = Action::Tandem;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(f(x), f(y))
}

/// Classic 1 with both characters.
fn feel() -> Arc<clamber_pack::Feel> {
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    // The other character stands where it's put in these examples;
    // following is tested in `following.rs`.
    feel.world.set(Param::FollowTrail, Fixed::ZERO);
    Arc::new(feel)
}

/// What a step left behind, as a trace row would show it.
#[derive(Clone, Debug)]
struct Row {
    character: usize,
    pos: Vec2,
    vel: Vec2,
    state: State,
    ground: bool,
    facing: Facing,
    events: Vec<Event>,
    other: Other,
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    /// `player` (a roster slot) standing at `at` facing `facing`, and the
    /// other character standing at `other` facing the other way.
    fn new(player: usize, at: Vec2, facing: Facing, other: Vec2) -> Play {
        let level = Arc::new(Level::from_toml_str(FLAT, "flat").unwrap());
        let mut world = World::new(level, feel());
        world.player.character = player;
        world.player.pos = at;
        world.player.facing = facing;
        let theirs = match facing {
            Facing::Left => Facing::Right,
            Facing::Right => Facing::Left,
        };
        world.other = Some(Other::new(1 - player, other, theirs, at));
        let mut play = Play {
            world,
            held: Actions::NONE,
        };
        play.idle(2);
        assert!(play.world.player.grounded());
        assert_eq!(play.world.player.pos, at);
        play
    }

    /// Both characters side by side on the floor, the first under
    /// control.
    fn flat() -> Play {
        Play::new(0, v(400.0, 400.0), Facing::Right, v(420.0, 400.0))
    }

    fn step(&mut self, keys: &[Action]) -> Row {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        let p = &self.world.player;
        Row {
            character: p.character,
            pos: p.pos,
            vel: p.vel,
            state: p.state,
            ground: p.grounded(),
            facing: p.facing,
            events: self.world.events.clone(),
            other: self.world.other.clone().unwrap(),
        }
    }

    fn hold(&mut self, keys: &[Action], frames: usize) -> Vec<Row> {
        (0..frames).map(|_| self.step(keys)).collect()
    }

    fn idle(&mut self, frames: usize) -> Vec<Row> {
        self.hold(&[], frames)
    }

    /// Presses swap and holds nothing else, through `frames` rows from
    /// the press.
    fn swap(&mut self, frames: usize) -> Vec<Row> {
        let mut rows = vec![self.step(&[S])];
        rows.extend(self.idle(frames - 1));
        rows
    }
}

#[test]
fn trading_places_on_flat_floor() {
    // swap.md's made-up example, 350 px lower.
    let start = v(100.25, 400.0);
    let theirs = v(128.5, 400.0);
    let mut play = Play::new(0, start, Facing::Left, theirs);
    let rows = play.swap(81);
    for (n, row) in rows[..47].iter().enumerate() {
        assert_eq!(row.state, State::Swap, "Q + {n}");
        assert_eq!((row.pos, row.vel), (start, Vec2::default()), "Q + {n}");
        assert_eq!(row.other.pos, theirs, "Q + {n}");
    }
    for row in &rows[..79] {
        assert!(row.ground);
        assert!(row.events.is_empty(), "{:?}", row.events);
        assert_eq!(row.facing, Facing::Left);
        assert_eq!(row.character, 0);
    }
    assert_eq!(rows[47].pos.x, f(101.125));
    assert_eq!(rows[47].other.pos.x, f(127.625));
    let ys: Vec<_> = rows[47..53].iter().map(|r| r.pos.y).collect();
    let flat = [
        399.44140625,
        399.3203125,
        399.63671875,
        399.390625,
        399.58203125,
        399.2109375,
    ];
    assert_eq!(ys, flat.map(f));
    for row in &rows[66..79] {
        assert_eq!(row.pos.y, f(399.953125));
    }
    // vx and vy are the change since the row before.
    assert_eq!(rows[50].vel, v(0.875, 399.390625 - 399.63671875));
    assert_eq!(rows[78].pos, v(128.25, 399.953125));
    assert_eq!(rows[78].other.pos, v(100.5, 399.953125));

    // E = Q + 79: the second character is the player, standing still on
    // its target, facing as it did.
    let e = &rows[79];
    assert_eq!(e.character, 1);
    assert_eq!(e.pos, v(100.5, 400.0));
    assert_eq!((e.state, e.vel), (State::Idle, Vec2::default()));
    assert!(e.ground);
    assert_eq!(e.facing, Facing::Right);
    assert!(e.events.is_empty(), "no turn: {:?}", e.events);
    assert_eq!(e.other.character, 0);
    assert_eq!(e.other.pos, v(128.25, 399.953125));
    assert_eq!(e.other.facing, Facing::Left);
    assert!(play.world.swap.is_none());
}

#[test]
fn trading_places_with_the_other_character_lower() {
    // The same, with the second character 4 px lower.
    let mut play = Play::new(0, v(100.25, 400.0), Facing::Left, v(128.5, 404.0));
    let rows = play.swap(80);
    let ys: Vec<_> = rows[47..55].iter().map(|r| r.pos.y).collect();
    let lower = [
        399.44140625,
        399.3203125,
        399.63671875,
        400.390625,
        401.58203125,
        403.2109375,
        403.27734375,
        403.78125,
    ];
    assert_eq!(ys, lower.map(f));
    assert_eq!(rows[66].pos.y, f(403.953125));
    let theirs: Vec<_> = rows[47..53].iter().map(|r| r.other.pos.y).collect();
    let rising = [
        403.44140625,
        403.3203125,
        399.63671875,
        399.390625,
        399.58203125,
        399.2109375,
    ];
    assert_eq!(theirs, rising.map(f));
    assert_eq!(rows[79].pos, v(100.5, 400.0));
    assert_eq!(rows[79].other.pos, v(128.25, 403.953125));
}

#[test]
fn the_second_character_as_player_waits_less_and_is_followed_a_frame_behind() {
    let start = v(400.0, 400.0);
    let theirs = v(432.0, 400.0);
    let mut play = Play::new(1, start, Facing::Right, theirs);
    let rows = play.swap(74);
    for row in &rows[..39] {
        assert_eq!((row.state, row.pos), (State::Swap, start));
    }
    assert_eq!(rows[39].pos.x, f(401.0));
    assert_eq!(rows[39].other.pos, theirs);
    assert_eq!(rows[40].other.pos.x, f(431.0));
    assert_eq!(rows[70].pos.x, f(432.0));
    assert_eq!(rows[71].state, State::Swap);
    assert_eq!(rows[71].pos.x, f(432.0));
    assert_eq!(rows[71].other.pos.x, f(400.0));
    assert_eq!(rows[71].character, 1);
    assert_eq!(rows[72].character, 0);
    assert_eq!(rows[72].pos, start);
    assert_eq!(rows[72].state, State::Idle);
}

#[test]
fn a_jump_press_just_before_shortens_the_wait_and_keeps_its_gravity() {
    // Pressed on Q - 1 and held: the jump never starts, the player moves
    // from Q + 46 with the lighter gravity, and E is Q + 78.
    let mut play = Play::flat();
    play.step(&[J]);
    let mut rows = vec![play.step(&[J, S])];
    rows.extend(play.idle(79));
    assert!(rows.iter().all(|r| !r.events.contains(&Event::JumpStart)));
    assert_eq!(rows[45].pos.y, f(400.0));
    assert_eq!(rows[46].pos.y, f(400.0 - 0.99609375 + 0.28125));
    assert_eq!(rows[77].character, 0);
    assert_eq!(rows[78].character, 1);

    // Pressed on Q - 2 and let go: the jump starts on Q - 1 as usual, the
    // swap still cancels it, and the gravity is the usual one.
    let mut play = Play::flat();
    play.step(&[J]);
    let before = play.step(&[]);
    assert_eq!(before.events, [Event::JumpStart]);
    let rows = play.swap(79);
    assert_eq!(rows[0].state, State::Swap);
    assert_eq!(rows[0].pos.y, f(400.0));
    assert_eq!(rows[0].vel, Vec2::default());
    assert_eq!(rows[46].pos.y, f(400.0 - 0.99609375 + 0.4375));
    assert_eq!(rows[78].character, 1);
}

#[test]
fn a_tandem_press_swaps_as_a_swap_press_does() {
    // controls.md, "Tandem in `title-1`": the same swap, frame by frame.
    let mut by_swap = Play::flat();
    let mut by_tandem = Play::flat();
    let swapped = by_swap.swap(81);
    let mut rows = vec![by_tandem.step(&[T])];
    rows.extend(by_tandem.idle(80));
    for (n, (s, t)) in swapped.iter().zip(&rows).enumerate() {
        assert_eq!(format!("{s:?}"), format!("{t:?}"), "Q + {n}");
    }
    assert_eq!(rows[80].character, 1);
}

#[test]
fn a_tandem_swap_never_keeps_the_jumps_gravity() {
    // Jump pressed on Q - 1 and held, Tandem on Q: the wait is shortened
    // as with Swap, but the gravity is the usual one (difference 2).
    let mut play = Play::flat();
    play.step(&[J]);
    let mut rows = vec![play.step(&[J, T])];
    rows.extend(play.idle(79));
    assert!(rows.iter().all(|r| !r.events.contains(&Event::JumpStart)));
    assert_eq!(rows[45].pos.y, f(400.0));
    assert_eq!(rows[46].pos.y, f(400.0 - 0.99609375 + 0.4375));
    assert_eq!(rows[77].character, 0);
    assert_eq!(rows[78].character, 1);
}

#[test]
fn holding_tandem_after_a_refused_press_does_nothing() {
    let mut play = Play::flat();
    play.step(&[J]);
    play.idle(2);
    assert_eq!(play.step(&[T]).state, State::Jump);
    let rows = play.hold(&[T], 120);
    assert!(rows.iter().any(|r| r.events.contains(&Event::Land)));
    assert!(rows.iter().all(|r| r.state != State::Swap));
}

#[test]
fn no_swap_in_the_air_or_on_landing() {
    // Pressed on P + 3 of a jump: in the air.
    let mut play = Play::flat();
    play.step(&[J]);
    play.idle(2);
    assert_eq!(play.step(&[S]).state, State::Jump);
    // A refused press is forgotten, however long swap is held.
    let rows = play.hold(&[S], 120);
    assert!(rows.iter().any(|r| r.events.contains(&Event::Land)));
    assert!(rows.iter().all(|r| r.state != State::Swap));

    // Fall onto the floor: not on the frame after landing; on the next.
    let fall = |play: &mut Play| {
        play.world.player.pos = v(400.0, 360.0);
        play.world.player.ground = None;
        play.world.player.state = State::Fall;
        let mut frames = 0;
        while !play.step(&[]).events.contains(&Event::Land) {
            frames += 1;
            assert!(frames < 60);
        }
    };
    let mut play = Play::flat();
    fall(&mut play);
    assert_ne!(play.step(&[S]).state, State::Swap);
    let mut play = Play::flat();
    fall(&mut play);
    play.idle(1);
    assert_eq!(play.step(&[S]).state, State::Swap);
}

/// Whether a swap press `after` frames after holding down for `held`
/// frames starts a swap, with `player` under control.
fn swaps_after_crouching(player: usize, held: usize, after: usize) -> bool {
    let mut play = Play::new(player, v(400.0, 400.0), Facing::Right, v(420.0, 400.0));
    play.hold(&[D], held);
    play.idle(after - held);
    play.step(&[S]).state == State::Swap
}

#[test]
fn crouching_holds_off_a_swap() {
    // Down held 5 frames from C: from C + 23 for the first character,
    // C + 22 for the second.
    assert!(!swaps_after_crouching(0, 5, 22));
    assert!(swaps_after_crouching(0, 5, 23));
    assert!(!swaps_after_crouching(1, 5, 21));
    assert!(swaps_after_crouching(1, 5, 22));
    // Held 20 frames, let go on U = C + 20: from U + 10 and U + 13.
    assert!(!swaps_after_crouching(0, 20, 29));
    assert!(swaps_after_crouching(0, 20, 30));
    assert!(!swaps_after_crouching(1, 20, 32));
    assert!(swaps_after_crouching(1, 20, 33));
    // While down is still held.
    let mut play = Play::flat();
    play.hold(&[D], 60);
    assert_ne!(play.step(&[D, S]).state, State::Swap);
}

#[test]
fn down_or_action_pressed_with_swap_does_nothing() {
    let mut play = Play::flat();
    let rows = play.hold(&[D, A, S], 79);
    assert!(rows.iter().all(|r| r.state == State::Swap));
    assert!(rows.iter().all(|r| r.events.is_empty()));
}

/// Whether a swap press `after` frames after the press frame of a
/// ground action in place starts a swap.
fn swaps_after_ground_action(after: usize) -> bool {
    let mut play = Play::flat();
    assert_eq!(play.step(&[D, A]).state, State::GroundAction);
    play.idle(after - 1);
    play.step(&[S]).state == State::Swap
}

#[test]
fn no_swap_during_the_ground_action() {
    // spec/behavior/ground-action.md, "Inputs during the action": refused
    // up to and including E = P + 59, possible from E + 1.
    for after in [1, 30, 58, 59] {
        assert!(!swaps_after_ground_action(after), "P + {after}");
    }
    assert!(swaps_after_ground_action(60));
}

#[test]
fn heights_7_px_apart_swap_and_8_px_dont() {
    for (other, swaps) in [(392.0, false), (393.0, true), (407.0, true), (408.0, false)] {
        let mut play = Play::new(0, v(400.0, 400.0), Facing::Right, v(420.0, other));
        assert_eq!(play.step(&[S]).state == State::Swap, swaps, "{other}");
    }
    // A fraction rounds toward the floor: 392.5 counts as 393.
    let mut play = Play::new(0, v(400.0, 400.0), Facing::Right, v(420.0, 392.5));
    assert_eq!(play.step(&[S]).state, State::Swap);
}

#[test]
fn a_swap_stops_a_walk_dead_and_abandons_a_turn() {
    let mut play = Play::flat();
    play.hold(&[R], 30);
    let before = play.world.player.pos;
    let row = play.step(&[R, S]);
    assert_eq!(
        (row.state, row.pos, row.vel),
        (State::Swap, before, Vec2::default())
    );

    // A turn under way: facing stays as it was on Q - 1.
    let mut play = Play::flat();
    play.hold(&[R], 30);
    play.hold(&[L], 4);
    assert_eq!(play.world.player.facing, Facing::Right);
    let rows = play.hold(&[L, S], 80);
    assert!(rows[..79].iter().all(|r| r.facing == Facing::Right));
    assert_eq!(rows[79].other.facing, Facing::Right);
}

#[test]
fn input_is_ignored_until_control_returns() {
    let mut play = Play::flat();
    play.step(&[S]);
    // Another press, jump held and a direction: nothing.
    play.idle(10);
    let rows = play.hold(&[S, J, R], 68);
    assert!(
        rows.iter()
            .all(|r| r.state == State::Swap && r.events.is_empty())
    );
    // E: Right held starts walking, and jump held from before doesn't
    // jump.
    let e = play.step(&[S, J, R]);
    assert_eq!(e.character, 1);
    assert_eq!(e.state, State::Walk);
    assert!(e.vel.x > Fixed::ZERO);
    let rows = play.hold(&[J, R], 10);
    assert!(rows.iter().all(|r| !r.events.contains(&Event::JumpStart)));
}

#[test]
fn a_swap_press_counts_from_the_frame_after_control_returns() {
    let mut play = Play::flat();
    play.swap(79);
    let e = play.step(&[S]);
    assert_eq!((e.character, e.state), (1, State::Idle));
    assert_ne!(play.step(&[S]).state, State::Swap);

    let mut play = Play::flat();
    play.swap(80);
    assert_eq!(play.step(&[S]).state, State::Swap);
    assert_eq!(play.world.player.character, 1);
}

#[test]
fn no_swap_during_a_roll_or_its_stop() {
    let mut play = Play::flat();
    let rows = play.hold(&[A], 1);
    assert_eq!(rows[0].events, [Event::RollStart]);
    // Press after press: refused until the roll and its stop are over.
    let mut refused = 0;
    loop {
        let p = &play.world.player;
        let busy = p.state == State::Roll || p.roll.is_some() || p.roll_stop > 0;
        let row = play.step(&[S]);
        if !busy {
            assert_eq!(row.state, State::Swap);
            break;
        }
        assert_ne!(row.state, State::Swap);
        refused += 1;
        play.step(&[]);
    }
    assert!(refused > 20, "{refused}");
}

#[test]
fn traces_follow_the_player_until_control_returns() {
    let scenario = Scenario::parse(
        "# clamber-scenario v1\n\
         id: swap\nsummary: s\nrequires: r\nstart: s\nframes: 140\n\
         [inputs]\n0-39 R\n60 S\n",
        "test",
    )
    .unwrap();
    let level = Arc::new(Level::from_toml_str(FLAT, "flat").unwrap());
    let trace = record(level, feel(), &scenario).unwrap();
    let rows = &trace.rows;
    let x = |n: usize| rows[n].x.to_string().parse::<f64>().unwrap();
    let walked = x(59);
    assert!(walked > 50.0, "{walked}");
    for row in &rows[60..139] {
        assert_eq!(row.state.name(), "swap");
        assert!(row.ground);
        assert!(row.events.is_empty());
    }
    // The player walks back to where the other character stood, at the
    // start, keeping its fraction; on E the rows jump to the other
    // character, at the whole pixel where the player was.
    assert_eq!(x(138), walked.fract());
    assert_eq!(x(139), walked.floor());
    assert!(rows[139].vx.is_zero());
    assert_eq!(rows[139].state.name(), "idle");
}
