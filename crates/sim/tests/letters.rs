//! Letters, the letters taken and the letter row in Classic 1
//! (spec/behavior/letters.md).

use std::sync::Arc;

use clamber_input::{Actions, Frame};
use clamber_level::Level;
use clamber_sim::{
    Facing, FlowEvent, InputFrame, LetterRow, LivesDisplay, START_LIVES, World, generic_feel, step,
};

/// The player stands at x = 200 on flat ground at y = 400.
const X: i32 = 200;
const F: i32 = 400;

/// A level with the player standing at (`X`, `F`) and a letter at each
/// `(x, y, letter)`.
fn world(letters: &[(i32, i32, u8)]) -> World {
    let mut src = format!(
        r#"
        format = 1
        name = "letters"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]
        "#
    );
    for (x, y, k) in letters {
        src += &format!(
            "\n[[entities]]\ntype = \"letter\"\nat = [{x}, {y}]\nparams = {{ letter = {k} }}\n"
        );
    }
    let level = Arc::new(Level::from_toml_str(&src, "letters").unwrap());
    let mut world = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    // Without the lives display shown when play starts.
    world.flow.play_from = u32::MAX;
    assert!(world.player.grounded());
    world
}

fn idle(world: &mut World) {
    step(
        world,
        &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
    );
}

/// Whether a letter `past` px behind the player the way it faces and
/// `below` px above its feet is taken on the first step, standing still
/// facing `facing`.
fn taken_standing(past: i32, below: i32, facing: Facing) -> bool {
    let xl = match facing {
        Facing::Right => X - past,
        Facing::Left => X + past,
    };
    let mut w = world(&[(xl, F - below, 1)]);
    w.player.facing = facing;
    idle(&mut w);
    w.flow.events.contains(&FlowEvent::Collect(0))
}

#[test]
fn standing_takes_letters_inside_the_standing_ranges_either_way() {
    for facing in [Facing::Right, Facing::Left] {
        for (past, below) in [(-15, -8), (21, 40), (-15, 40), (21, -8), (0, 0)] {
            assert!(
                taken_standing(past, below, facing),
                "{past}, {below}, {facing:?}"
            );
        }
        for (past, below) in [(-16, 0), (22, 0), (0, -9), (0, 41)] {
            assert!(
                !taken_standing(past, below, facing),
                "{past}, {below}, {facing:?}"
            );
        }
    }
}

#[test]
fn facing_left_reverses_the_horizontal_range() {
    // letters.md: facing left, X - xl from -21 to 15.
    let at = |dx: i32| {
        let mut w = world(&[(X - dx, F - 10, 1)]);
        w.player.facing = Facing::Left;
        idle(&mut w);
        w.flow.events.contains(&FlowEvent::Collect(0))
    };
    assert!(at(-21) && at(15));
    assert!(!at(-22) && !at(16));
}

#[test]
fn a_letter_joins_thirty_frames_after_and_never_comes_back() {
    let mut w = world(&[(X, F - 10, 2)]);
    let mut joined = None;
    let mut taken = 0;
    for n in 0..60 {
        idle(&mut w);
        taken += w
            .flow
            .events
            .iter()
            .filter(|e| **e == FlowEvent::Collect(0))
            .count();
        if w.flow.events.contains(&FlowEvent::Letter(2)) {
            assert_eq!(joined, None);
            joined = Some(n);
        }
        if n < 30 {
            assert_eq!(w.flow.letters, 0, "frame {n}");
        }
    }
    assert_eq!(taken, 1);
    assert_eq!(joined, Some(30));
    assert_eq!(w.flow.letters, 0b10);
    assert_eq!(w.flow.gems, 0, "letters add nothing to the count");
    assert_eq!(w.flow.lives, START_LIVES);
}

#[test]
fn the_fourth_letter_gives_a_life_in_any_order() {
    for last in [4, 1] {
        let mut w = world(&[(X, F - 10, last)]);
        w.flow.letters = 0b1111 & !(1 << (last - 1));
        let mut life = None;
        for n in 0..40 {
            idle(&mut w);
            if w.flow.events.contains(&FlowEvent::Life) {
                life = Some(n);
            }
        }
        assert_eq!(life, Some(30), "letter {last} last");
        assert_eq!(w.flow.lives, START_LIVES + 1);
        assert_eq!(w.flow.letters, 0, "the letters taken start again");
    }
}

#[test]
fn the_letter_row_slides_in_stays_and_slides_out() {
    let mut w = world(&[(X, F - 10, 1)]);
    let mut ys = Vec::new();
    for _ in 0..140 {
        idle(&mut w);
        ys.push(w.letter_row().map(|r| r.y));
    }
    // Taken on the first step, P = 0.
    assert_eq!(ys[0], None);
    assert_eq!(ys[1], None);
    let slide: Vec<_> = ys[2..10].iter().map(|y| y.unwrap()).collect();
    assert_eq!(slide, [-12, -8, -4, 0, 4, 8, 12, 16]);
    assert!(ys[10..122].iter().all(|&y| y == Some(12)));
    let out: Vec<_> = ys[122..128].iter().map(|y| y.unwrap()).collect();
    assert_eq!(out, [8, 4, 0, -4, -8, -12]);
    assert!(ys[128..].iter().all(Option::is_none));
}

#[test]
fn the_row_shows_the_letters_taken_in_their_slots() {
    // letters.md's example: letters 1 and 2 taken, letter 3 taken on
    // frame 100.
    let mut w = world(&[(X + 1000, F - 10, 3)]);
    w.flow.letters = 0b011;
    for _ in 0..100 {
        idle(&mut w);
    }
    assert_eq!(w.letter_row(), None, "not shown during ordinary play");
    w.things[0].pos = w.player.pos;
    let mut rows = Vec::new();
    for _ in 100..230 {
        idle(&mut w);
        rows.push(w.letter_row());
    }
    let at = |frame: usize| rows[frame - 100];
    let row = at(110).unwrap();
    assert_eq!(
        (row.y, row.x[0], row.x[1], row.x[2], row.x[3]),
        (12, 84, 108, 132, 156)
    );
    assert_eq!(row.count, 4);
    assert_eq!(row.letters, 0b011);
    assert_eq!(at(130).unwrap().letters, 0b111);
    assert_eq!(at(221).unwrap().y, 12);
    assert!(at(228).is_none() && at(229).is_none());
}

/// Letters.md's example of another letter while the row is shown: with
/// letter 4 taken, letter 1 is taken on frame 100 and letter 2 on `p2`.
/// Gives the y of each row drawn in the picture made on each frame.
fn two_letters(p2: usize) -> Vec<Vec<i32>> {
    let mut w = world(&[(X + 1000, F - 10, 1), (X + 1500, F - 10, 2)]);
    w.flow.letters = 0b1000;
    let mut ys = Vec::new();
    for n in 0..400 {
        if n == 100 {
            w.things[0].pos = w.player.pos;
        }
        if n == p2 {
            w.things[1].pos = w.player.pos;
            w.things[1].active = true;
        }
        idle(&mut w);
        ys.push(w.letter_rows().iter().map(|r| r.y).collect());
    }
    ys
}

#[test]
fn a_letter_taken_before_the_rows_end_keeps_it_shown() {
    let ys = two_letters(150);
    assert_eq!(ys[101], Vec::<i32>::new());
    assert_eq!(ys[102], [-12]);
    assert!(ys[110..=271].iter().all(|y| y == &[12]));
    let out: Vec<_> = (272..=277).map(|n| ys[n][0]).collect();
    assert_eq!(out, [8, 4, 0, -4, -8, -12]);
    assert!(ys[278..].iter().all(Vec::is_empty));

    // Taken while the row still slides in, it goes on sliding in.
    let ys = two_letters(105);
    let slide: Vec<_> = (102..=109).map(|n| ys[n][0]).collect();
    assert_eq!(slide, [-12, -8, -4, 0, 4, 8, 12, 16]);
    assert!(ys[110..=226].iter().all(|y| y == &[12]));
    assert_eq!(ys[227], [8]);

    // On the frame before the end, as late as it can be.
    let ys = two_letters(220);
    assert!(ys[110..=341].iter().all(|y| y == &[12]));
    assert_eq!(ys[342], [8]);
}

#[test]
fn a_letter_taken_on_the_rows_end_or_later_brings_in_a_second_row() {
    let ys = two_letters(224);
    assert!(ys[110..=221].iter().all(|y| y == &[12]));
    let first: Vec<_> = (222..=227).map(|n| ys[n][0]).collect();
    assert_eq!(first, [8, 4, 0, -4, -8, -12]);
    assert_eq!(ys[225], [-4]);
    assert_eq!(ys[226], [-8, -12]);
    assert_eq!(ys[227], [-12, -8]);
    assert_eq!(ys[228], [-4]);
    assert_eq!(ys[229], [0]);
    assert!(ys[234..=345].iter().all(|y| y == &[12]));
    assert_eq!(ys[346], [8]);
    assert!(ys[352..].iter().all(Vec::is_empty));

    // Taken on the end itself.
    let ys = two_letters(221);
    assert_eq!(ys[221], [12]);
    assert_eq!(ys[222], [8]);
    assert_eq!(ys[223], [4, -12]);
}

/// With no letter taken before, each `(frame, letter)` is taken on its
/// frame. Gives the y of each row drawn in the picture made on each
/// frame.
fn letters_at(taken: &[(usize, u8)]) -> Vec<Vec<i32>> {
    let spots: Vec<_> = (0..taken.len() as i32)
        .zip(taken)
        .map(|(i, &(_, k))| (X + 1000 + 500 * i, F - 10, k))
        .collect();
    let mut w = world(&spots);
    let mut ys = Vec::new();
    for n in 0..500 {
        for (i, &(p, _)) in taken.iter().enumerate() {
            if n == p {
                w.things[i].pos = w.player.pos;
            }
        }
        idle(&mut w);
        ys.push(w.letter_rows().iter().map(|r| r.y).collect());
    }
    ys
}

#[test]
fn a_letter_on_the_next_frame_or_earlier_in_the_word_moves_the_end() {
    for taken in [[(100, 4), (101, 1)], [(100, 3), (101, 1)]] {
        let ys = letters_at(&taken);
        assert!(ys[110..=222].iter().all(|y| y == &[12]), "{taken:?}");
        let out: Vec<_> = (223..=228).map(|n| ys[n][0]).collect();
        assert_eq!(out, [8, 4, 0, -4, -8, -12]);
        assert!(ys[229..].iter().all(Vec::is_empty));
    }
    let ys = letters_at(&[(100, 3), (150, 1)]);
    assert!(ys[110..=271].iter().all(|y| y == &[12]));
    assert!(ys[278..].iter().all(Vec::is_empty));
}

#[test]
fn a_third_letter_acts_on_the_row_brought_in_last() {
    let ys = letters_at(&[(100, 1), (160, 2), (260, 3)]);
    assert!(ys[110..=381].iter().all(|y| y == &[12]));
    let out: Vec<_> = (382..=387).map(|n| ys[n][0]).collect();
    assert_eq!(out, [8, 4, 0, -4, -8, -12]);
    assert!(ys[388..].iter().all(Vec::is_empty));

    // Letter 2 on the first row's end brings in a second row; letter 3
    // while both are drawn moves the second row's end.
    let ys = letters_at(&[(100, 1), (221, 2), (226, 3)]);
    let first: Vec<_> = (222..=227).map(|n| ys[n][0]).collect();
    assert_eq!(first, [8, 4, 0, -4, -8, -12]);
    assert!(ys[231..=347].iter().all(|y| y == &[12]));
    let out: Vec<_> = (348..=353).map(|n| ys[n][0]).collect();
    assert_eq!(out, [8, 4, 0, -4, -8, -12]);
    assert!(ys[354..].iter().all(Vec::is_empty));
}

#[test]
fn the_default_preset_has_no_letter_row() {
    let level = r#"
        format = 1
        size = [400, 300]
        start = [100, 200]
        [[collision]]
        shape = "rect"
        rect = [0, 200, 400, 100]
        [[entities]]
        type = "letter"
        at = [100, 190]
    "#;
    let level = Arc::new(Level::from_toml_str(level, "test").unwrap());
    let mut w = World::new(level, Arc::new(generic_feel("default").unwrap()));
    for _ in 0..40 {
        idle(&mut w);
        assert_eq!(w.letter_row(), None);
    }
    assert_eq!(w.flow.letters, 1);
    assert_eq!(w.flow.lives, START_LIVES);
}

/// The letter row and lives display in a picture.
type Shown = (Option<LetterRow>, Option<LivesDisplay>);

/// The ending after the last of the word joins (letters.md, "The
/// ending"): with the letters in `held` already taken and `lives`
/// lives, `last` is taken on the first step. Gives the step it joined
/// on (`J`), and the letter row and lives display in the picture made on
/// each frame from 0 to `J` + 300.
fn ending(held: u8, last: u8, lives: u8) -> (u32, Vec<Shown>) {
    let mut w = world(&[(X, F - 10, last)]);
    w.flow.letters = held;
    w.flow.lives = lives;
    let mut shown = Vec::new();
    let mut joined = None;
    while joined.is_none_or(|j| shown.len() <= j as usize + 300) {
        idle(&mut w);
        if w.flow.events.contains(&FlowEvent::Life) {
            joined = Some(w.frame - 1);
        }
        shown.push((w.letter_row(), w.lives_display()));
    }
    (joined.unwrap(), shown)
}

/// Where letter `k` is drawn in a row, if it is.
fn letter_x(row: Option<LetterRow>, k: u8) -> Option<i32> {
    row.filter(|r| r.letters & (1 << (k - 1)) != 0)
        .map(|r| r.x[usize::from(k - 1)])
}

#[test]
fn the_ending_follows_the_spec_example() {
    // Letters 1, 2 and 4 taken, 5 lives, letter 3 taken last.
    let (j, shown) = ending(0b1011, 3, 5);
    let at = |f: u32| shown[f as usize];
    assert_eq!(at(j).0.unwrap().letters, 0b1111, "all four on J");
    assert!((1..=4).all(|k| letter_x(at(j).0, k) == Some(84 + 24 * (i32::from(k) - 1))));
    let off = |k: u8, n: u32| {
        let late = u32::from(k == 3);
        let n = n - late;
        (1..=8).contains(&n)
            || (17..=24).contains(&n)
            || (33..=40).contains(&n)
            || (49..=56).contains(&n)
    };
    for n in 1..67 {
        for k in 1..=4 {
            let x = letter_x(at(j + n).0, k);
            let slot = 84 + 24 * (i32::from(k) - 1);
            let expected = (!off(k, n)).then_some(slot);
            assert_eq!(x, expected, "letter {k} on J + {n}");
        }
        assert_eq!(at(j + n).0.unwrap().y, 12);
    }
    // Letter 1 moves from J + 67 and is gone from J + 99.
    let xs: Vec<i32> = (67..=98)
        .map(|n| letter_x(at(j + n).0, 1).unwrap())
        .collect();
    assert_eq!(
        xs,
        [
            97, 109, 119, 128, 136, 143, 149, 154, 159, 163, 167, 170, 172, 175, 177, 179, 180,
            182, 183, 184, 185, 186, 186, 187, 188, 188, 189, 189, 190, 190, 191, 192
        ]
    );
    assert_eq!(letter_x(at(j + 66).0, 1), Some(84));
    // Each letter moves from, and is gone from (letter 3 is late).
    for (k, from, gone) in [(1, 67, 99), (2, 91, 121), (3, 116, 144), (4, 139, 163)] {
        let slot = 84 + 24 * (i32::from(k) - 1);
        assert_eq!(letter_x(at(j + from - 1).0, k), Some(slot), "letter {k}");
        assert!(letter_x(at(j + from).0, k).unwrap() > slot, "letter {k}");
        assert_eq!(letter_x(at(j + gone - 1).0, k), Some(192), "letter {k}");
        assert_eq!(letter_x(at(j + gone).0, k), None, "letter {k}");
    }
    assert!(shown[j as usize + 163..].iter().all(|s| s.0.is_none()));

    // The lives display: 5 until G = J + 163, then 6.
    let g = j + 163;
    let ys: Vec<Option<i32>> = (j..=g + 128).map(|f| at(f).1.map(|l| l.y)).collect();
    let mut expected = vec![None; 7];
    expected.extend([-12, -8, -4, 0, 4, 8].map(Some));
    expected.extend(std::iter::repeat_n(Some(10), (g + 122 - (j + 13)) as usize));
    expected.extend([6, 2, -2, -6, -10, -14].map(Some));
    expected.push(None);
    assert_eq!(ys, expected);
    assert_eq!(at(g - 1).1.unwrap().number, 5);
    assert_eq!(at(g).1.unwrap().number, 6);
    assert_eq!(at(g).1.unwrap().x, 220);
    assert!(shown[g as usize + 128..].iter().all(|s| s.1.is_none()));
}

#[test]
fn with_letter_4_last_its_move_and_the_new_number_are_a_frame_later() {
    let (j, shown) = ending(0b0111, 4, 1);
    let at = |f: u32| shown[f as usize];
    assert!(letter_x(at(j + 162).0, 4).unwrap() < 192);
    assert_eq!(letter_x(at(j + 163).0, 4), Some(192));
    assert_eq!(letter_x(at(j + 164).0, 4), None);
    assert_eq!(at(j + 163).1.unwrap().number, 1);
    assert_eq!(at(j + 164).1.unwrap().number, 2);
    // Letter 1 isn't late: off from J + 1.
    assert_eq!(letter_x(at(j + 1).0, 1), None);
    assert_eq!(letter_x(at(j + 1).0, 4), Some(156));
}

#[test]
fn two_digit_lives_and_ninety_nine() {
    let (j, shown) = ending(0b1110, 1, 9);
    let g = j + 163;
    let before = shown[g as usize - 1].1.unwrap();
    let after = shown[g as usize].1.unwrap();
    assert_eq!((before.number, before.x), (9, 220));
    assert_eq!((after.number, after.x, after.x + after.gap), (10, 215, 225));

    let (j, shown) = ending(0b1110, 1, 99);
    for f in j + 7..j + 163 + 128 {
        assert_eq!(shown[f as usize].1.unwrap().number, 99, "frame {f}");
    }
}

/// The lives number shown and the top of it, if it's drawn.
type Numbers = Option<(u8, i32)>;

/// [`ending`]'s pictures from `J` to the level's last frame `E`, with a
/// walker put on the player so that it's hit on `J` + `hit`. Gives the
/// burst `B` (`E` − 122) less `J`, and where the lives number is drawn
/// and the number it shows on each frame, from `J`. With no `hit`, up
/// to `J` + 300 and no burst.
fn knocked_out_ending(
    held: u8,
    last: u8,
    lives: u8,
    hit: Option<i64>,
) -> (i64, Vec<Numbers>, Vec<Option<LetterRow>>) {
    let src = format!(
        r#"
        format = 1
        name = "letters"
        size = [2000, 800]
        start = [{X}, {F}]

        [[collision]]
        shape = "rect"
        rect = [0, {F}, 2000, 200]

        [[entities]]
        type = "letter"
        at = [{X}, {}]
        params = {{ letter = {last} }}

        [[entities]]
        type = "crawler"
        at = [1800, {F}]
        params = {{ speed = 0 }}
        "#,
        F - 10
    );
    let level = Arc::new(Level::from_toml_str(&src, "letters").unwrap());
    let mut w = World::new(level, Arc::new(generic_feel("classic-1").unwrap()));
    // Without the lives display shown when play starts.
    w.flow.play_from = u32::MAX;
    w.flow.letters = held;
    w.flow.lives = lives;
    // The letter joins on frame 30.
    let j: i64 = 30;
    let h = hit.map_or(i64::MAX, |hit| j + hit);
    let (mut shown, mut rows) = (Vec::new(), Vec::new());
    let mut joined = None;
    let mut burst = None;
    loop {
        // Put on the player on H - 1, it hits on H.
        if i64::from(w.frame) == h - 1 {
            w.things[1].pos = w.player.pos;
            w.things[1].active = true;
        }
        idle(&mut w);
        let n = i64::from(w.frame) - 1;
        if w.flow.events.contains(&FlowEvent::Life) {
            joined = Some(n);
        }
        if n == h - 1 {
            assert!(w.player.knockout.is_none(), "not hit before H");
        }
        if n == h {
            assert!(w.player.knockout.is_some(), "hit on H");
        }
        if w.flow.events.contains(&FlowEvent::Respawn) || (hit.is_none() && n == j + 300) {
            break;
        }
        burst = w.flow.lost.and_then(|l| l.burst).or(burst);
        if n >= j {
            shown.push(w.lives_display().map(|d| (d.number, d.y)));
            rows.push(w.letter_row());
        }
        assert!(n < 2000, "never came back");
    }
    assert_eq!(joined, Some(j));
    (burst.map_or(0, |b| i64::from(b) - j), shown, rows)
}

#[test]
fn knocked_out_in_the_ending_the_number_stays_and_goes_down_and_up() {
    // letters.md, "While the player is knocked out", the first example:
    // 5 lives, letter 3 last, hit on J + 29. B = J + 132, E = B + 122,
    // G = J + 163.
    let (b, shown, rows) = knocked_out_ending(0b1011, 3, 5, Some(29));
    assert_eq!(b, 29 + 103);
    assert_eq!(shown.len() as i64, b + 123, "E = B + 122");
    let mut expected = vec![None; 7];
    expected.extend([-12, -8, -4, 0, 4, 8].map(|y| Some((5, y))));
    expected.resize(b as usize + 3, Some((5, 10)));
    expected.resize(b as usize + 11, None);
    expected.extend([-12, -8, -4, 0, 4, 8].map(|y| Some((4, y))));
    expected.resize(163, Some((4, 10)));
    expected.resize(b as usize + 123, Some((5, 10)));
    assert_eq!(shown, expected);
    // The four letters are drawn as without the hit.
    let (_, _, plain) = knocked_out_ending(0b1011, 3, 5, None);
    assert_eq!(rows[..=200], plain[..=200]);
    assert!(rows[163..].iter().all(Option::is_none));
}

#[test]
fn knocked_out_while_the_number_leaves_it_comes_back_and_stays() {
    // The second example: hit on 819 = J + 288, while leaving at y -6.
    let (b, shown, _) = knocked_out_ending(0b1011, 3, 5, Some(288));
    assert_eq!(b, 288 + 103);
    let at = |f: i64| shown[f as usize];
    assert_eq!(at(287), Some((6, -2)));
    for f in 288..=298 {
        assert_eq!(at(f), Some((6, -6)), "J + {f}");
    }
    assert_eq!([299, 300, 301].map(at), [-2, 2, 6].map(|y| Some((6, y))));
    for f in 302..b + 3 {
        assert_eq!(at(f), Some((6, 10)), "J + {f}");
    }
    for f in b + 3..b + 11 {
        assert_eq!(at(f), None, "J + {f}");
    }
    assert_eq!(at(b + 11), Some((5, -12)));
    for f in b + 17..=b + 122 {
        assert_eq!(at(f), Some((5, 10)), "J + {f}");
    }
}

#[test]
fn knocked_out_just_before_the_last_letter_joins() {
    // H = J - 11: the new life is still given on J, and the number comes
    // in from H + 11 = J as without the hit, held at 10 from J + 13.
    let (b, shown, _) = knocked_out_ending(0b1011, 3, 1, Some(-11));
    assert_eq!(&shown[..7], [None; 7]);
    assert_eq!(shown[7], Some((1, -12)));
    assert!(
        shown[13..b as usize + 3]
            .iter()
            .all(|&s| s == Some((1, 10)))
    );
    // From B + 3 one less (0), up again on G = J + 163 as usual, unless
    // that's on the frames it comes in again on.
    assert_eq!(shown[b as usize + 11], Some((0, -12)));
    let g = 163;
    assert!(!(b + 4..=b + 17).contains(&g));
    assert_eq!(shown[g as usize - 1].unwrap().0, 0);
    assert_eq!(shown[g as usize].unwrap().0, 1);
}

#[test]
fn the_new_life_comes_late_when_it_falls_while_the_number_comes_in_again() {
    // G in B + 4 to B + 17 shows on B + 28; in H + 1 to H + 10, on H + 11.
    let g = 163;
    let (b, shown, _) = knocked_out_ending(0b1011, 3, 5, Some(g - 103 - 10));
    assert_eq!(b, g - 10);
    assert_eq!(shown[b as usize + 27].unwrap().0, 4);
    assert_eq!(shown[b as usize + 28].unwrap().0, 5);
    let (_, shown, _) = knocked_out_ending(0b1011, 3, 5, Some(g - 4));
    assert_eq!(shown[g as usize + 6], Some((5, 10)));
    assert_eq!(shown[g as usize + 7], Some((6, 10)));
}
