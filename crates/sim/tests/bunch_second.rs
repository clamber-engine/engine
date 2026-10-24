//! Bunches taken with the second character leading (spec/behavior/
//! bunches.md, "Second character leading" and "Landing, second
//! character leading").

use std::collections::BTreeMap;
use std::sync::Arc;

use clamber_input::{Action, Actions, Frame};
use clamber_level::Level;
use clamber_pack::{CharacterFeel, Fixed, Param, Presets};
use clamber_sim::{Event, Facing, FlowEvent, InputFrame, Other, World, generic_feel, step};

const STAND: [i32; 4] = [-12, 11, -10, 38];
const CROUCH: [i32; 4] = [-19, 14, -9, 25];

/// Flat ground at y = 400, the second character at (400, 400) leading
/// and the first beside it, and a bunch at `at`.
fn world(at: Option<(i32, i32)>) -> World {
    world_with("", at)
}

/// `world`, with `extra` entities in the level's source.
fn world_with(extra: &str, at: Option<(i32, i32)>) -> World {
    let mut src = String::from(
        r#"
        format = 1
        name = "flat"
        size = [4096, 600]
        start = [400, 400]

        [[collision]]
        shape = "rect"
        rect = [0, 400, 4096, 200]
        "#,
    );
    src += extra;
    if let Some((x, y)) = at {
        src += &format!("\n[[entities]]\ntype = \"cluster\"\nat = [{x}, {y}]\n");
    }
    let mut feel = generic_feel("classic-1").unwrap();
    feel.roster.push(CharacterFeel {
        physics: Presets::builtin().slot("classic-1", 1).unwrap(),
        abilities: BTreeMap::new(),
    });
    feel.world.set(Param::FollowTrail, Fixed::ZERO);
    let level = Arc::new(Level::from_toml_str(&src, "flat").unwrap());
    let mut world = World::new(level, Arc::new(feel));
    world.player.character = 1;
    world.other = Some(Other::new(
        0,
        world.player.pos,
        Facing::Left,
        world.player.pos,
    ));
    world
}

/// The first step on which a bunch at `at` is taken by playing `keys`.
fn taken(at: (i32, i32), keys: &[&[Action]]) -> Option<usize> {
    let mut w = world(Some(at));
    let mut held = Actions::NONE;
    for (n, k) in keys.iter().enumerate() {
        let now: Actions = k.iter().copied().collect();
        step(&mut w, &InputFrame::single(Frame::after(held, now)));
        held = now;
        if w.flow
            .events
            .iter()
            .any(|e| matches!(e, FlowEvent::Collect(_)))
        {
            return Some(n);
        }
    }
    None
}

/// Whether holding `keys` for 30 frames takes a bunch `past` px past
/// the character (facing right) and `below` px above its feet.
fn takes(keys: &[Action], past: i32, below: i32) -> bool {
    taken((400 - past, 400 - below), &vec![keys; 30]).is_some()
}

#[test]
fn standing_and_crouching_have_the_second_characters_ranges() {
    for past in -25..25 {
        for below in -15..45 {
            let want =
                (STAND[0]..=STAND[1]).contains(&past) && (STAND[2]..=STAND[3]).contains(&below);
            assert_eq!(takes(&[], past, below), want, "{past} {below}");
        }
    }
    // Crouching is reached after a few frames of Down; a bunch in its
    // row but outside the standing one is then taken, and one in the
    // standing row only is taken on the way in.
    assert!(!takes(&[], -19, 20));
    assert!(takes(&[Action::Down], -19, 20));
    assert!(!takes(&[Action::Down], -19, 30));
    assert_eq!(CROUCH[3], 25);
}

#[test]
fn a_landing_has_the_walking_ranges_to_l_plus_16() {
    // Dropped from 100 px up, with a bunch 10 px below the feet: it's in
    // the standing row only, so it's first taken when that row applies
    // again, on L + 17.
    let drop = |at: Option<(i32, i32)>| {
        let mut w = world(at);
        w.player.pos.y = Fixed::from_int(300);
        let mut landed = None;
        for n in 0..80 {
            step(
                &mut w,
                &InputFrame::single(Frame::after(Actions::NONE, Actions::NONE)),
            );
            if landed.is_none() && w.player.grounded() {
                landed = Some(n);
            }
            if w.flow
                .events
                .iter()
                .any(|e| matches!(e, FlowEvent::Collect(_)))
            {
                return (landed, Some(n));
            }
        }
        (landed, None)
    };
    let (l, _) = drop(None);
    let l = l.unwrap();
    assert_eq!(drop(Some((400, 410))).1, Some(l + 17));
}

/// One step of a bounce off a small walker: the character's place, its
/// facing, and whether it bounced or landed on the step.
struct Mark {
    x: i32,
    y: i32,
    facing: Facing,
    bounce: bool,
    land: bool,
}

/// Plays `keys` in a world with a walker ahead and a bunch at `at`:
/// each step's mark, and the first step a bunch is taken on.
fn bounce_run(at: Option<(i32, i32)>, keys: &[&[Action]]) -> (Vec<Mark>, Option<usize>) {
    let walker =
        "\n[[entities]]\ntype = \"crawler\"\nat = [500, 400]\nparams = { facing = \"left\" }\n";
    let mut w = world_with(walker, at);
    let mut held = Actions::NONE;
    let (mut marks, mut got) = (Vec::new(), None);
    for (n, k) in keys.iter().enumerate() {
        let now: Actions = k.iter().copied().collect();
        step(&mut w, &InputFrame::single(Frame::after(held, now)));
        held = now;
        let p = &w.player;
        marks.push(Mark {
            x: p.pos.x.floor() as i32,
            y: -((-p.pos.y).floor() as i32),
            facing: p.facing,
            bounce: w.events.contains(&Event::Bounce),
            land: w.events.contains(&Event::Land),
        });
        if got.is_none()
            && w.flow
                .events
                .iter()
                .any(|e| matches!(e, FlowEvent::Collect(_)))
        {
            got = Some(n);
        }
    }
    (marks, got)
}

#[test]
fn a_bounce_off_a_walker_has_its_own_ranges_and_none_on_b_plus_1_and_2() {
    use Action::{Jump, Right};
    const AIR: [i32; 4] = [-18, 12, -6, 38];
    const BOUNCE: [i32; 4] = [-23, 19, -10, 32];
    let play = |wait: usize| -> Vec<&'static [Action]> {
        let mut k: Vec<&[Action]> = vec![&[Right]; wait];
        k.extend(vec![&[Right, Jump][..]; 20]);
        k.extend(vec![&[Right][..]; 100]);
        k
    };
    let wait = (0..60)
        .find(|&w| bounce_run(None, &play(w)).0.iter().any(|m| m.bounce))
        .expect("a bounce");
    let keys = play(wait);
    let (marks, _) = bounce_run(None, &keys);
    let b = marks.iter().position(|m| m.bounce).unwrap();
    let land = b + marks[b..].iter().position(|m| m.land).expect("a landing");
    assert!(land > b + 6, "B {b}, L {land}");
    let ranges = |n: usize| match n {
        n if n < b => AIR,
        n if n == b + 1 || n == b + 2 => [1, 0, 1, 0],
        _ => BOUNCE,
    };
    for a in [b - 2, b, b + 1, b + 2, b + 3, land - 1] {
        for past in -30..30 {
            for below in [-11, -10, -9, -7, -6, -5, 31, 32, 33, 37, 38, 39] {
                let at = (marks[a].x - past, marks[a].y - below);
                let (_, got) = bounce_run(Some(at), &keys);
                let want = (0..land).find(|&n| {
                    let [i0, i1, d0, d1] = ranges(n);
                    let i = match marks[n].facing {
                        Facing::Right => marks[n].x - at.0,
                        Facing::Left => at.0 - marks[n].x,
                    };
                    (i0..=i1).contains(&i) && (d0..=d1).contains(&(marks[n].y - at.1))
                });
                assert_eq!(
                    got.filter(|&g| g < land),
                    want,
                    "{past} px past and {below} px above on step {a} (B {b})"
                );
            }
        }
    }
}
