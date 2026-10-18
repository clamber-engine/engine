//! The restart barrel on `classic-1` (spec/behavior/restart-barrel.md).

use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::{Level, Vec2};
use clamber_sim::{Event, Facing, Fixed, FlowEvent, InputFrame, Phase, World, generic_feel, step};

const R: Action = Action::Right;
const J: Action = Action::Jump;

fn f(v: f64) -> Fixed {
    Fixed::from_f64(v).unwrap()
}

/// The spec's example: a keg at (1000, 400) over a floor at y = 409 from
/// x = 0 to 3000, the player standing at `start` on it.
fn world(start: i32) -> World {
    world_with(start, "{}")
}

/// [`world`] with the keg's `params`.
fn world_with(start: i32, params: &str) -> World {
    let src = format!(
        r#"
        format = 1
        name = "kegs"
        size = [3000, 700]
        start = [{start}, 409]

        [[collision]]
        shape = "rect"
        rect = [0, 409, 3000, 100]

        [[entities]]
        type = "keg"
        at = [1000, 400]
        params = {params}
        "#
    );
    let level = Arc::new(Level::from_toml_str(&src, "kegs").unwrap());
    World::new(level, Arc::new(generic_feel("classic-1").unwrap()))
}

struct Play {
    world: World,
    held: Actions,
}

impl Play {
    fn new(start: i32) -> Play {
        Play::in_world(world(start))
    }

    fn in_world(world: World) -> Play {
        let mut play = Play {
            world,
            held: Actions::NONE,
        };
        play.step(&[]);
        assert!(play.world.player.grounded());
        play
    }

    fn step(&mut self, keys: &[Action]) -> Vec<FlowEvent> {
        let held: Actions = keys.iter().copied().collect();
        let input = InputFrame::single(Frame::after(self.held, held));
        self.held = held;
        step(&mut self.world, &input);
        self.world.flow.events.clone()
    }

    fn touched(&self) -> bool {
        self.world.flow.events.contains(&FlowEvent::Checkpoint(0))
    }

    /// Walks right into the keg, then falls out of the level, and waits
    /// for the player to come back: frame 0 of the resume.
    fn touch_and_fall_out(&mut self) {
        for _ in 0..120 {
            self.step(&[R]);
            if self.touched() {
                break;
            }
        }
        assert_eq!(self.world.flow.checkpoints, [0]);
        self.fall_out();
    }

    /// Puts the player below the level and waits for it to come back.
    fn fall_out(&mut self) {
        self.world.player.pos.y = f(900.0);
        self.world.player.ground = None;
        for _ in 0..400 {
            if self.step(&[]).contains(&FlowEvent::Respawn) {
                return;
            }
        }
        panic!("never came back");
    }

    fn y(&self) -> f64 {
        self.world.player.pos.y.to_f64()
    }
}

/// restart-barrel.md, "Example", the touch: walking right at 2 px per
/// frame, the frame ending at `I` = -23 touches it, not the two before
/// (-27 and -25).
#[test]
fn walking_in_touches_it_where_the_upright_range_starts() {
    let mut play = Play::new(900);
    let mut walked = Vec::new();
    loop {
        play.step(&[R]);
        let x = play.world.player.pos.x;
        walked.push(x);
        if play.touched() {
            break;
        }
        assert!(walked.len() < 100, "never touched it");
    }
    let n = walked.len();
    assert_eq!(play.world.player.vel.x, f(2.0));
    let i: Vec<i64> = walked[n - 3..].iter().map(|x| x.floor() - 1000).collect();
    assert_eq!(i, [-27, -25, -23]);
    // It's gone, the player walks on as before, and the restart point is
    // reached.
    let keg = &play.world.things[0];
    assert!(!keg.active);
    assert_eq!(keg.phase, Phase::Broken);
    assert_eq!(play.world.flow.checkpoints, [0]);
    assert_eq!(play.world.bursts.len(), 1);
    let x = play.world.player.pos.x;
    play.step(&[R]);
    assert_eq!(play.world.player.pos.x, x + f(2.0));
    assert!(!play.touched());
}

#[test]
fn it_breaks_into_pieces_and_a_flash() {
    let mut play = Play::new(900);
    while !play.touched() {
        play.step(&[R]);
    }
    let burst = play.world.bursts[0];
    assert_eq!(burst.pieces, [Vec2::ints(1000, 400); 4]);
    play.step(&[R]);
    let burst = play.world.bursts[0];
    let want = [(2.0, -4.75), (-1.0, -5.75), (-2.0, -4.75), (1.0, -6.25)];
    for (piece, (x, y)) in burst.pieces.iter().zip(want) {
        assert_eq!(*piece, Vec2::new(f(1000.0 + x), f(400.0 + y)));
    }
    assert_eq!(burst.flash, Vec2::new(f(1000.0), f(399.5)));
    play.step(&[R]);
    let burst = play.world.bursts[0];
    assert_eq!(burst.pieces[0], Vec2::new(f(1004.0), f(400.0 - 4.75 - 4.5)));
    // Then they're gone.
    for _ in 0..60 {
        play.step(&[]);
    }
    assert!(play.world.bursts.is_empty());
}

/// restart-barrel.md, "Example": the resume, frame by frame.
#[test]
fn play_resumes_inside_a_keg_at_the_restart_point() {
    let mut play = Play::new(900);
    play.touch_and_fall_out();
    let w = &play.world;
    assert_eq!(w.player.pos, Vec2::ints(1000, 400));
    // view.md, "When an area begins": `V` = 921.5 on frame 0.
    assert_eq!(w.view.unwrap().left, f(921.5));
    assert_eq!(w.brightness().0, 0);
    assert!(w.resume_keg().is_none());

    for frame in 1..=62 {
        play.step(&[]);
        let w = &play.world;
        assert_eq!(play.y(), 391.4375, "frame {frame}");
        assert_eq!(w.player.pos.x, f(1000.0));
        assert!(!w.player_drawn() && w.in_keg(), "frame {frame}");
        if frame == 1 {
            // The usual rules from frame 1, inside the keg too: the
            // target 929 is 7.5 px ahead, so `S` = 240/8192 and `V`
            // moves by 7/256.
            assert_eq!(w.view.unwrap().left, f(921.5 + 7.0 / 256.0));
        }
        assert_eq!(w.resume_keg().is_some(), frame < 62, "frame {frame}");
        assert_eq!(!w.bursts.is_empty(), frame == 62, "frame {frame}");
        let dark = w.brightness().0 == 0;
        assert_eq!(dark, frame < 4, "frame {frame}");
    }
    let rows = [
        (391.875, 0.4375),
        (392.75, 0.875),
        (394.0625, 1.3125),
        (395.8125, 1.75),
        (398.0, 2.1875),
        (400.625, 2.625),
        (403.6875, 3.0625),
        (409.0, 0.0),
    ];
    for (i, (y, vy)) in rows.into_iter().enumerate() {
        play.step(&[]);
        let w = &play.world;
        assert!(w.player_drawn() && !w.in_keg());
        assert_eq!(
            (play.y(), w.player.vel.y.to_f64()),
            (y, vy),
            "frame {}",
            63 + i
        );
        assert_eq!(w.player.pos.x, f(1000.0));
        assert_eq!(w.player.grounded(), i == 7);
    }
    assert!(play.world.events.contains(&Event::Land));
    assert!(play.world.resume.is_none());
    let (now, full) = play.world.brightness();
    assert_eq!(now, full);
}

#[test]
fn the_shake_moves_the_keg_sideways() {
    let mut play = Play::new(900);
    play.touch_and_fall_out();
    let mut offsets = Vec::new();
    for _ in 1..=61 {
        play.step(&[]);
        let at = play.world.resume_keg().unwrap();
        assert_eq!(at.y, f(400.0));
        offsets.push(at.x.floor() - 1000);
    }
    let want: Vec<i64> = (1..=61)
        .map(|n| match n {
            1..=14 => 0,
            15..=44 => [1, 1, 0, 0][(n - 15) % 4],
            45..=56 => [-3, -3, 1, 1][(n - 45) % 4],
            _ => [-7, -7, 1, 1][(n - 57) % 4],
        })
        .collect();
    assert_eq!(offsets, want);
}

/// restart-barrel.md, "Resuming": Right held from frame 2, 50 or 62 on
/// walks from frame 63 as if pressed then.
#[test]
fn input_in_the_keg_does_nothing_and_a_held_right_acts_on_frame_63() {
    let resume = |from: usize| {
        let mut play = Play::new(900);
        play.touch_and_fall_out();
        let mut xs = Vec::new();
        for frame in 1..=80 {
            let keys: &[Action] = if frame >= from { &[R] } else { &[] };
            play.step(keys);
            xs.push(play.world.player.pos.x);
        }
        xs
    };
    let pressed_on_63 = resume(63);
    assert!(pressed_on_63[62] > f(1000.0));
    assert!(pressed_on_63[..62].iter().all(|&x| x == f(1000.0)));
    for from in [2, 3, 50, 60, 62] {
        assert_eq!(resume(from), pressed_on_63, "Right from frame {from}");
    }
}

/// restart-barrel.md, "Resuming": Jump held from frame 64 on jumps on
/// the frame after landing.
#[test]
fn a_jump_held_in_the_fall_starts_on_the_frame_after_landing() {
    let mut play = Play::new(900);
    play.touch_and_fall_out();
    let mut started = None;
    for frame in 1..=80 {
        let keys: &[Action] = if frame >= 64 { &[J] } else { &[] };
        play.step(keys);
        if play.world.events.contains(&Event::JumpStart) {
            started.get_or_insert(frame);
        }
    }
    assert_eq!(started, Some(71));
}

/// restart-barrel.md, "The restart point": it stays reached after a
/// resume, and the keg stays gone.
#[test]
fn a_second_lost_life_resumes_there_again() {
    let mut play = Play::new(900);
    play.touch_and_fall_out();
    for _ in 0..100 {
        play.step(&[]);
    }
    assert!(play.world.resume.is_none());
    // Far away and back: the keg doesn't come back.
    for _ in 0..200 {
        play.step(&[Action::Left]);
    }
    for _ in 0..200 {
        play.step(&[R]);
    }
    assert!(!play.world.things[0].active);
    play.fall_out();
    assert_eq!(play.world.player.pos, Vec2::ints(1000, 400));
    assert!(play.world.resume.is_some());
    play.step(&[]);
    assert!(play.world.in_keg());
}

/// level-start.md, "The first level": the restart point is the level's
/// restart record's, wherever the barrel is. A keg's `restart` moves it
/// from the keg's position.
#[test]
fn a_keg_s_restart_moves_the_restart_point() {
    let mut play = Play::in_world(world_with(900, "{ restart = [-60, 9] }"));
    play.touch_and_fall_out();
    let w = &play.world;
    assert_eq!(w.player.pos, Vec2::ints(940, 409));
    assert_eq!(w.view.unwrap().x(), 861);
    assert_eq!(w.restart_point(0), Vec2::ints(940, 409));
    play.step(&[]);
    assert!(play.world.in_keg());
    assert_eq!(play.y(), 409.0 - 8.5625);
    assert_eq!(play.world.resume_keg().map(|at| at.y), Some(f(409.0)));
}

/// Without the keg touched, play begins again at the level's start, as
/// before.
#[test]
fn without_it_play_begins_at_the_start() {
    let mut play = Play::new(300);
    play.fall_out();
    assert_eq!(play.world.player.pos, Vec2::ints(300, 409));
    assert!(play.world.resume.is_none());
    play.step(&[]);
    assert!(!play.world.in_keg() && play.world.player_drawn());
}

/// restart-barrel.md, "Touching the barrel": standing, the upright
/// ranges, turned around facing left.
#[test]
fn the_ranges_turn_around_facing_left() {
    let touches = |x: i32, facing: Facing| {
        let mut w = world(x);
        w.player.facing = facing;
        step(&mut w, &InputFrame::default());
        assert!(w.player.grounded());
        w.flow.checkpoints == [0]
    };
    // Facing right: -24 to 27.
    assert!(!touches(975, Facing::Right));
    assert!(touches(976, Facing::Right));
    assert!(touches(1027, Facing::Right));
    assert!(!touches(1028, Facing::Right));
    // Facing left: -30 to 21.
    assert!(!touches(969, Facing::Left));
    assert!(touches(970, Facing::Left));
    assert!(touches(1021, Facing::Left));
    assert!(!touches(1022, Facing::Left));
}

/// Touches the keg walking right, turns to face left, and leaves the
/// level by a knock-out or by falling out; frame 0 of the resume.
fn left_facing_left(knocked_out: bool) -> Play {
    let mut play = Play::new(900);
    for _ in 0..120 {
        play.step(&[R]);
        if play.touched() {
            break;
        }
    }
    for _ in 0..40 {
        play.step(&[Action::Left]);
    }
    for _ in 0..10 {
        play.step(&[]);
    }
    assert_eq!(play.world.player.facing, Facing::Left);
    if !knocked_out {
        play.fall_out();
        return play;
    }
    play.world.player.hurt_ahead = Some(0);
    play.step(&[]);
    assert!(play.world.player.knockout.is_some());
    assert_eq!(play.world.player.facing, Facing::Left);
    for _ in 0..600 {
        if play.step(&[]).contains(&FlowEvent::Respawn) {
            return play;
        }
    }
    panic!("never came back");
}

/// restart-barrel.md, "Resuming": the player faces right on frame 0 and
/// until an input turns it, though it was knocked out or fell out facing
/// left. Holding Left from frame 2 starts a turn on frame 63, and it
/// faces left from 67. With the same inputs, the resumed frames are the
/// same after a fall-out as after a knock-out.
#[test]
fn it_resumes_facing_right_however_it_left() {
    let resume = |knocked_out: bool, keys: &[Action]| {
        let mut play = left_facing_left(knocked_out);
        let p = &play.world.player;
        let mut frames = vec![(p.pos, p.facing, p.turn)];
        for frame in 1..=140 {
            play.step(if frame >= 2 { keys } else { &[] });
            let p = &play.world.player;
            frames.push((p.pos, p.facing, p.turn));
        }
        frames
    };
    for keys in [&[][..], &[R], &[Action::Left]] {
        let after_fall = resume(false, keys);
        assert_eq!(resume(true, keys), after_fall, "{keys:?}");
        let facing_left: Vec<usize> = (0..after_fall.len())
            .filter(|&n| after_fall[n].1 == Facing::Left)
            .collect();
        if keys == [Action::Left] {
            assert!(after_fall[62].2.is_none() && after_fall[63].2.is_some());
            assert_eq!(facing_left[0], 67);
        } else {
            assert!(facing_left.is_empty(), "{keys:?}");
        }
    }
}

/// One frame of a resume: the player's place, speed, facing and turn, and
/// whether it's on the ground.
type Seen = (Vec2, Vec2, Facing, Option<u8>, bool);

/// Resumes with `first` held on frame 1, `mid` on frames 2 to 62 and
/// `late` from 63 on; frames 0 to `last`.
fn resume_holding(first: &[Action], mid: &[Action], late: &[Action], last: usize) -> Vec<Seen> {
    let mut play = Play::new(900);
    play.touch_and_fall_out();
    let seen = |w: &World| {
        let p = &w.player;
        (p.pos, p.vel, p.facing, p.turn, p.grounded())
    };
    let mut frames = vec![seen(&play.world)];
    for frame in 1..=last {
        play.step(match frame {
            1 => first,
            2..=62 => mid,
            _ => late,
        });
        frames.push(seen(&play.world));
    }
    frames
}

const L: Action = Action::Left;
const A: Action = Action::Action;
const U: Action = Action::Up;
const D: Action = Action::Down;

/// restart-barrel.md, "Input on frame 1", "Position": Left and Right held
/// on frame 1 move the player right, and `x` stays there until frame 63.
#[test]
fn left_or_right_on_frame_1_moves_the_player_right() {
    let cases: [(&[Action], f64); 8] = [
        (&[R], 0.25),
        (&[R, A], 0.375),
        (&[L], 0.75),
        (&[L, A], 0.625),
        (&[L, R], 0.0),
        (&[L, R, A], 0.0),
        (&[], 0.0),
        (&[A], 0.0),
    ];
    for (keys, shift) in cases {
        let frames = resume_holding(keys, &[], &[], 62);
        assert_eq!(frames[0].0.x, f(1000.0), "{keys:?}: frame 0");
        for (n, seen) in frames.iter().enumerate().skip(1) {
            assert_eq!(seen.0.x, f(1000.0 + shift), "{keys:?}: frame {n}");
            assert_eq!(seen.0.y, f(391.4375), "{keys:?}: frame {n}");
        }
    }
}

/// From frame 63 the player moves from there as it would from `xr`:
/// after Right on frame 1, every frame is as with nothing held, 0.25 px
/// further right, and it doesn't start walking. Right with Up, Down or
/// Action, but not Down and Action without Up, changes only the
/// position; Up, Down and Action alone change nothing.
#[test]
fn input_on_frame_1_otherwise_changes_only_the_position() {
    let none = resume_holding(&[], &[], &[], 140);
    let shifted = |frames: &[Seen], by: f64| -> Vec<Seen> {
        frames
            .iter()
            .map(|&(pos, vel, facing, turn, ground)| {
                let x = if pos.x == f(1000.0) && frames[0].0 == pos {
                    pos.x
                } else {
                    pos.x + f(by)
                };
                (Vec2::new(x, pos.y), vel, facing, turn, ground)
            })
            .collect()
    };
    let cases: [(&[Action], f64); 7] = [
        (&[R], 0.25),
        (&[R, U], 0.25),
        (&[R, D], 0.25),
        (&[R, A, U, D], 0.375),
        (&[U], 0.0),
        (&[D], 0.0),
        (&[A], 0.0),
    ];
    for (keys, by) in cases {
        let frames = resume_holding(keys, &[], &[], 140);
        assert_eq!(frames, shifted(&none, by), "{keys:?}");
        assert!(
            frames.iter().all(|s| s.1.x == Fixed::ZERO),
            "{keys:?}: walks"
        );
    }
}

/// "Input on frame 1", "Jump held": the player waits 0.15625 px higher,
/// and frame 63 is the jump's take-off frame: it moves down by 0.4375, or
/// 0.28125 with Jump held on it, and `vy` is -7. Held on frame 1 only, the
/// top is 52.5 px above `y` on frame 63, and it lands on frame 96; held
/// on, 83.625 px and frame 114. Whatever is held on frames 2 to 62.
#[test]
fn jump_on_frame_1_jumps_on_frame_63() {
    let lift = 400.0 - 8.71875;
    for (mid, late, down, top, lands) in [
        (&[][..], &[][..], 0.4375, 52.5, 96),
        (&[J][..], &[][..], 0.4375, 52.5, 96),
        (&[][..], &[J][..], 0.28125, 83.625, 114),
        (&[J][..], &[J][..], 0.28125, 83.625, 114),
    ] {
        let at = format!("{mid:?} then {late:?}");
        let frames = resume_holding(&[J], mid, late, 130);
        for seen in &frames[1..=62] {
            assert_eq!(seen.0.y, f(lift), "{at}");
        }
        let y63 = frames[63].0.y;
        assert_eq!(y63, f(lift + down), "{at}");
        assert_eq!(frames[63].1.y, f(-7.0), "{at}");
        let highest = frames[63..].iter().map(|s| s.0.y).min().unwrap();
        assert_eq!(y63 - highest, f(top), "{at}");
        let landed = frames.iter().position(|s| s.4).unwrap();
        assert_eq!(landed, lands, "{at}");
        assert_eq!(frames[landed].0.y, f(409.0));
    }
}

/// "Input on frame 1", "Left held without Right": a turn starts on frame
/// 1 and goes on from 63; the player faces left from frame 64 on, and 67
/// is the turn's last frame. Holding Left on, it moves left in the air
/// and walks left after landing; with nothing held it lands and stands
/// facing left.
#[test]
fn left_on_frame_1_turns_from_frame_63() {
    for late in [&[][..], &[L][..]] {
        let frames = resume_holding(&[L], &[], late, 140);
        let facing_left: Vec<usize> = (0..frames.len())
            .filter(|&n| frames[n].2 == Facing::Left)
            .collect();
        assert_eq!(facing_left[0], 64, "{late:?}");
        assert_eq!(facing_left.len(), frames.len() - 64, "{late:?}");
        assert!(frames[67].3.is_some() && frames[68].3.is_none(), "{late:?}");
        let last = frames.last().unwrap();
        assert!(last.4, "{late:?}: landed");
        if late.is_empty() {
            assert_eq!(last.1.x, Fixed::ZERO);
            assert_eq!(last.0.x, f(1000.75));
        } else {
            assert!(frames[70].0.x < frames[63].0.x, "moves left in the air");
            assert!(last.1.x.is_negative(), "walks left");
        }
    }
    // Not with Right, nor with Jump, which jumps instead.
    for first in [&[L, R][..], &[L, J][..]] {
        let frames = resume_holding(first, &[], &[], 140);
        assert!(frames.iter().all(|s| s.2 == Facing::Right), "{first:?}");
    }
}
