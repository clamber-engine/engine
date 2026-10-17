//! Ropes (spec/behavior/ropes.md): things whose archetype has a [`Rope`]
//! behavior.
//!
//! A rope's position is the line the player holds, and its `y` its
//! bottom end, rounded down; it reaches up from there without end. Each
//! step, after the player has moved, it sways (see [`sway`]): its target
//! speed turns once it's past a swing limit, and its speed eases toward
//! that. Then it notes whether it has the player in reach.
//!
//! The player catches a rope on a frame of a jump (a jump from the
//! ground or off a rope) that ends in the air, if the rope had it in
//! reach at the end of the frame before, unless it's the rope last held
//! ([`catch`]). It stops where it moved to, and from the next frame holds
//! the rope ([`hold`]): it climbs, turns round the rope, jumps off it
//! with Left or Right and Jump, or slides off the end and falls.
//!
//! The rope last held is forgotten on a frame that ends on the ground,
//! and on a frame of a jump with no rope that had the player in reach.
//! Jumping off a rope sets the player's run memory, which makes every
//! target speed in the air Action's; it's cleared on a frame that ends
//! on the ground.
//!
//! In a trace, every frame that ends holding a rope is `rope`, the catch
//! frame with `rope_catch` and without the move's other events but a
//! turn; the let-go frame past the end is `fall` with `rope_slide_off`.
//!
//! Engine choices where the spec says nothing:
//!
//! - Only the character under control catches ropes, and not while
//!   riding, swapping, knocked out, lost or holding an object. Nothing
//!   but a jump catches one: not a spring's throw or spring jump, nor the
//!   hop after taking over.
//! - Holding a rope, nothing stops the player: it moves through the
//!   level's ground. Hurt or bounced, it lets go.
//! - A rope starts where it's placed, at full speed to the right.
//! - A jump off on the same frame as sliding off the end jumps.
//! - The run memory is cleared on the first frame that ends on the
//!   ground.

use clamber_fixed::Fixed;
use clamber_input::{Action, Frame};

use crate::archetype::{Behavior, Rope};
use crate::player::{ceil, direction, ease, facing_of};
use crate::world::{Event, Phase, Thing, World};

/// The kind of entity a rope is.
pub const ROPE: &str = "rope";

/// The rope the player holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OnRope {
    /// Its thing.
    pub thing: usize,
    pub values: Rope,
    /// Which frame of a turn round the rope the last one was, counting
    /// from 1, while one goes on.
    pub turn: Option<u8>,
}

/// The rope values of `thing`, if it is one.
fn rope(thing: &Thing) -> Option<Rope> {
    match thing.archetype?.behavior {
        Behavior::Rope(rope) => Some(rope),
        _ => None,
    }
}

/// Moves a rope one frame around `centre`, its place's `x` rounded down.
/// A rope that hasn't moved yet starts at full speed to the right.
pub(crate) fn sway(thing: &mut Thing, centre: i64, rope: &Rope) {
    let mut target = match thing.phase {
        Phase::Rope { target, .. } => target,
        _ => {
            thing.vx = rope.speed;
            rope.speed
        }
    };
    let x = thing.pos.x.floor();
    if (target.is_negative() && x < centre - i64::from(rope.swing.0))
        || (!target.is_negative() && x > centre + i64::from(rope.swing.1))
    {
        target = -target;
    }
    thing.vx = ease(thing.vx, target, rope.sway_response);
    thing.pos.x += thing.vx;
    let reach = matches!(thing.phase, Phase::Rope { reach: true, .. });
    thing.phase = Phase::Rope { target, reach };
}

/// After the player's move on a step it didn't hold a rope: a frame of
/// a jump that ends in the air catches a rope that had it in reach at
/// the end of the last one, other than the rope last held.
pub(crate) fn catch(world: &mut World) {
    let player = &world.player;
    let jumping = player.jump_age.is_some()
        && !player.grounded()
        && player.spring_arc.is_none()
        && player.hop.is_none()
        && player.knockout.is_none()
        && player.riding.is_none()
        && player.hands.is_none()
        && player.swap == 0
        && world.swap.is_none()
        && !world.losing();
    if !jumping {
        return;
    }
    let mut in_reach = world
        .things
        .iter()
        .filter(|t| t.active && matches!(t.phase, Phase::Rope { reach: true, .. }));
    let Some(first) = in_reach.next() else {
        world.player.rope_last = None;
        return;
    };
    let last = world.player.rope_last;
    let caught = std::iter::once(first)
        .chain(in_reach)
        .find(|t| Some(t.index) != last)
        .and_then(|t| Some((t.index, rope(t)?)));
    if let Some((thing, values)) = caught {
        world.player.catch_rope(OnRope {
            thing,
            values,
            turn: None,
        });
        // The catch frame is a `rope` row: of the frame's move in the air
        // only a turn is kept ("In a trace").
        world.events.retain(|e| {
            !matches!(
                e,
                Event::Land | Event::LeaveGround | Event::JumpStart | Event::JumpRelease
            )
        });
        world.events.push(Event::RopeCatch);
    }
}

/// One step of a player holding a rope, if it is. Returns whether it
/// was, so the step's moves are done.
pub(crate) fn hold(world: &mut World, input: &Frame) -> bool {
    let Some(mut on) = world.player.rope else {
        return false;
    };
    let Some(thing) = world.things.get(on.thing).filter(|t| t.active) else {
        world.player.let_go_of_rope();
        return false;
    };
    let rope = on.values;
    let (rope_x, bottom) = (thing.pos.x.floor(), thing.pos.y.floor());
    let view_top = world.view.map(|v| v.y() + i64::from(rope.view_top));
    let held = |a| input.held.contains(a);
    let held_dir = match (held(Action::Left), held(Action::Right)) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    };
    let events = &mut world.events;
    let p = &mut world.player;
    p.touched.clear();
    p.update_jump_held(input);
    // A press while holding never waits to jump on the ground.
    p.jump_buffer.take();
    // 1. The player's whole px across are the rope's.
    p.pos.x += Fixed::from_int((rope_x - p.pos.x.floor()) as i32);
    p.vel.x = Fixed::ZERO;
    p.target = Fixed::ZERO;
    p.rope_dir = (held_dir != 0).then(|| facing_of(held_dir));
    // 2. Past the end, it lets go, but this frame still climbs.
    let slides = ceil(p.pos.y) > bottom + i64::from(rope.below);
    let action = usize::from(held(Action::Action));
    let climb = |up: bool, down: bool| {
        if down {
            -rope.down[action]
        } else if up {
            rope.up[action]
        } else {
            Fixed::ZERO
        }
    };
    // Not up past the top of the view as it was after the last frame.
    let below_view = |p: &mut crate::Player| {
        if let Some(top) = view_top {
            let feet = ceil(p.pos.y);
            if feet < top {
                p.pos.y += Fixed::from_int((top - feet) as i32);
            }
        }
    };
    let (up, down) = (held(Action::Up), held(Action::Down));
    let start_facing = p.facing;
    if input.pressed.contains(Action::Jump) && held_dir != 0 {
        // 3. Jumping off, at the speed of the climb's first frame from
        // standing still added, Jump not counting.
        let target = climb(up, down);
        let vy = -rope.jump_speed - ease(Fixed::ZERO, target, rope.climb_response);
        p.facing = facing_of(held_dir);
        if !target.is_negative() {
            below_view(p);
        }
        p.vel.y = vy;
        p.pos.y += vy;
        p.jump_off_rope();
        events.push(Event::JumpStart);
    } else {
        // 4 and 5. Climbing, up positive.
        let target = climb(up || (held(Action::Jump) && held_dir == 0), down);
        let u = ease(-p.vel.y, target, rope.climb_response);
        p.vel.y = -u;
        // 6.
        if !target.is_negative() {
            below_view(p);
        }
        // 7.
        p.pos.y += p.vel.y;
        if slides {
            // A turn under way stops, and none starts.
            p.slide_off_rope();
            events.push(Event::RopeSlideOff);
        } else {
            on.turn = match on.turn {
                Some(n) if n < rope.turn_frames => Some(n + 1),
                _ if held_dir == direction(p.facing) => Some(1),
                _ => None,
            };
            if on.turn == Some(rope.turn_facing) {
                p.facing = facing_of(-direction(p.facing));
            }
            p.rope = Some(on);
        }
    }
    if p.facing != start_facing {
        events.push(Event::Turn);
    }
    true
}

/// After everything has moved on a step: every rope sways and notes
/// whether it has the player in reach, and a player that ended the step
/// on the ground forgets the rope last held and its run memory.
pub(crate) fn update(world: &mut World) {
    let player = &world.player;
    let (x, feet, speed) = (player.pos.x.floor(), ceil(player.pos.y), player.vel.x.abs());
    let level = &world.level;
    for thing in world.things.iter_mut().filter(|t| t.active) {
        let Some(rope) = rope(thing) else {
            continue;
        };
        sway(thing, level.entities[thing.index].at.x.floor(), &rope);
        let across = if speed < rope.slow {
            rope.near
        } else {
            rope.far
        };
        let reach = feet <= thing.pos.y.floor() + i64::from(rope.below)
            && (x - thing.pos.x.floor()).abs() < i64::from(across);
        if let Phase::Rope { target, .. } = thing.phase {
            thing.phase = Phase::Rope { target, reach };
        }
    }
    let player = &mut world.player;
    if player.grounded() {
        player.rope_last = None;
        player.rope_run = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::Facing;
    use clamber_level::Vec2;

    fn f(v: f64) -> Fixed {
        Fixed::from_f64(v).unwrap()
    }

    fn rope() -> Rope {
        Rope {
            speed: f(0.1875),
            swing: (3, 3),
            sway_response: f(0.03125),
            below: 24,
            near: 12,
            far: 4,
            slow: f(0.25),
            up: [f(1.5), f(2.5)],
            down: [f(2.5), f(3.5)],
            climb_response: f(0.125),
            view_top: 41,
            turn_frames: 8,
            turn_facing: 5,
            jump_speed: f(6.125),
        }
    }

    #[test]
    fn the_sway_example() {
        // ropes.md, "The rope", "Example": centre 1000, moving right at
        // full speed, from frame 1.
        let rope = rope();
        let mut thing = Thing {
            index: 0,
            kind: ROPE.to_string(),
            pos: Vec2::new(f(1004.16015625), Fixed::ZERO),
            active: true,
            archetype: None,
            facing: Facing::Right,
            heading: Facing::Right,
            turn: 0,
            vx: f(0.1875),
            clock: 0,
            wait: 0,
            phase: Phase::Rope {
                target: f(0.1875),
                reach: false,
            },
        };
        let mut rows = Vec::new();
        for _ in 1..=49 {
            sway(&mut thing, 1000, &rope);
            rows.push((thing.vx, thing.pos.x));
        }
        let at = |n: usize| rows[n - 1];
        let px = |n: f64| f(n / 256.0);
        assert_eq!(at(1), (px(45.0), f(1004.3359375)));
        assert_eq!(at(2), (px(43.0), f(1004.50390625)));
        assert_eq!(at(3), (px(41.0), f(1004.6640625)));
        assert_eq!(at(15), (px(17.0), f(1005.9765625)));
        assert_eq!(at(16), (px(15.0), f(1006.03515625)));
        assert_eq!(at(17), (px(14.0), f(1006.08984375)));
        assert_eq!(at(30), (px(1.0), f(1006.4453125)));
        assert_eq!(at(31), (Fixed::ZERO, f(1006.4453125)));
        assert_eq!(at(32), (px(-1.0), f(1006.44140625)));
        assert_eq!(at(48), (px(-17.0), f(1005.84765625)));
        assert_eq!(at(49), (px(-48.0), f(1005.66015625)));
        assert!(matches!(thing.phase, Phase::Rope { target, .. } if target == px(-48.0)));
    }
}
