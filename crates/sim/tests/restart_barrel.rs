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
