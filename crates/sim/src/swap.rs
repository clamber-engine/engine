//! Swapping places with the other character (`swap_moves` above 0).
//!
//! With more than one character in the roster, the next one is in the
//! level with the player: the other character, which follows the player
//! ([`follow`](crate::follow)). A swap press trades
//! places with it: both hold still, then each moves to where the other
//! stood, and the other character becomes the player. Input is ignored
//! until it does.

use clamber_fixed::{FRAC_BITS, Fixed};
use clamber_input::{Action, Frame};
use clamber_level::Vec2;
use clamber_pack::{Param, Physics};

use crate::follow::{self, Follow};
use crate::player::{Facing, Player, State, ceil, frames};
use crate::world::World;

/// The character in the level that isn't under control.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Other {
    /// Roster slot.
    pub character: usize,
    /// The middle of the feet.
    pub pos: Vec2,
    pub facing: Facing,
    /// How it follows the player.
    pub follow: Follow,
}

impl Other {
    /// Roster slot `character` at `pos`, facing `facing`, about to follow
    /// a player at `lead`.
    pub fn new(character: usize, pos: Vec2, facing: Facing, lead: Vec2) -> Other {
        Other {
            character,
            pos,
            facing,
            follow: Follow::at(follow::point(lead)),
        }
    }
}

/// A swap under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Swap {
    /// Frames since the frame of the press, which is 0.
    pub frame: u16,
    /// The frame the other character becomes the player on.
    pub end: u16,
    /// How the player moves.
    pub player: Move,
    /// How the other character moves.
    pub other: Move,
}

/// One character's way to where the other stood.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Move {
    /// Where it started.
    pub from: Vec2,
    /// How far it goes across, whole px.
    pub across: i64,
    /// The height it ends at: the other's, on a whole pixel.
    pub target: Fixed,
    /// Vertical speed, px per frame, +y down.
    pub vy: Fixed,
    pub gravity: Fixed,
    pub max_fall: Fixed,
    /// The swap's frame it first moves on.
    pub first: u16,
    /// How many frames it moves for.
    pub moves: u16,
}

impl Move {
    fn new(from: Vec2, to: Vec2, first: u16, gravity: Fixed, physics: &Physics) -> Move {
        Move {
            from,
            across: to.x.floor() - from.x.floor(),
            target: Fixed::from_int(ceil(to.y) as i32),
            vy: -physics[Param::SwapHop],
            gravity,
            max_fall: physics[Param::MaxFallSpeed],
            first,
            moves: frames(physics[Param::SwapMoves]).into(),
        }
    }

    /// The position after the swap's frame `frame`, from `pos` before it.
    /// Across, it goes an equal share of the way each frame. Up and
    /// down, it hops, and a fall below the target is lifted back to it or
    /// above by whole pixels.
    fn step(&mut self, frame: u16, pos: Vec2) -> Vec2 {
        if frame < self.first || frame - self.first >= self.moves {
            return pos;
        }
        let done = i64::from(frame - self.first + 1);
        let x = self.from.x
            + Fixed::from_bits((self.across << FRAC_BITS) * done / i64::from(self.moves));
        self.vy = (self.vy + self.gravity).min(self.max_fall);
        let mut y = pos.y + self.vy;
        if self.vy > Fixed::ZERO && y > self.target {
            y -= Fixed::from_int(ceil(y - self.target) as i32);
        }
        Vec2::new(x, y)
    }
}

/// What was held lately that decides whether a swap press starts a swap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Recent {
    /// Frames since the first frame of the latest run of down held, and,
    /// once it's let go, since the frame it was.
    pub down: Option<(u16, Option<u16>)>,
    /// Frames since jump was last pressed, and whether it has been held
    /// on every frame since.
    pub jump: Option<(u16, bool)>,
    /// The last step landed.
    pub landed: bool,
}

impl Recent {
    /// Takes in a step's input.
    pub(crate) fn note(&mut self, input: &Frame, landed: bool) {
        let held = |a| input.held.contains(a);
        self.down = match self.down {
            _ if held(Action::Down) && self.down.is_none_or(|(_, up)| up.is_some()) => {
                Some((0, None))
            }
            Some((since, up)) => Some((
                since.saturating_add(1),
                match up {
                    Some(n) => Some(n.saturating_add(1)),
                    None if held(Action::Down) => None,
                    None => Some(0),
                },
            )),
            None => None,
        };
        self.jump = if input.pressed.contains(Action::Jump) {
            Some((0, true))
        } else {
            self.jump
                .map(|(age, kept)| (age.saturating_add(1), kept && held(Action::Jump)))
        };
        self.landed = landed;
    }
}

/// Goes on with the swap under way, or starts one on a swap press that
/// can. Returns whether this step was a swap step, which the player
/// doesn't move on by itself. On the frame the other character becomes
/// the player it isn't: the new player moves as usual, but no swap press
/// counts on it.
pub(crate) fn update(world: &mut World, physics: &Physics, input: &Frame) -> bool {
    let Some(other) = world.other.clone() else {
        return false;
    };
    let mut swap = match world.swap.take() {
        Some(mut swap) => {
            swap.frame += 1;
            swap
        }
        None if swap_press(input) && can_start(world, physics, &other) => {
            start(world, physics, input, &other)
        }
        None => return false,
    };
    if swap.frame >= swap.end {
        hand_over(world, &swap);
        return false;
    }
    let player = &mut world.player;
    let to = swap.player.step(swap.frame, player.pos);
    player.vel = Vec2::new(to.x - player.pos.x, to.y - player.pos.y);
    player.pos = to;
    player.state = State::Swap;
    player.touched.clear();
    if let Some(other) = &mut world.other {
        other.pos = swap.other.step(swap.frame, other.pos);
    }
    world.swap = Some(swap);
    true
}

/// Whether this frame has a swap press: Swap, or Tandem, which counts
/// only on the frame of its press (`controls.md`). Swap's presses are
/// the same so far, since a refused press isn't kept.
pub(crate) fn swap_press(input: &Frame) -> bool {
    input.pressed.contains(Action::Swap) || input.pressed.contains(Action::Tandem)
}

/// Whether a swap press on this frame starts a swap, from how things
/// were on the frame before.
fn can_start(world: &World, physics: &Physics, other: &Other) -> bool {
    let p = &world.player;
    let recent = &world.recent;
    if !p.grounded() || recent.landed {
        return false;
    }
    let busy = p.state == State::Roll
        || p.roll.is_some()
        || p.roll_stop > 0
        || p.ground_action.is_some()
        || p.frozen > 0
        || p.knockout.is_some()
        // Swapping with an object in hand isn't in the spec yet.
        || p.hands.is_some();
    if busy {
        return false;
    }
    if let Some((since, up)) = recent.down {
        let crouch = u16::from(frames(physics[Param::SwapCrouchWait]));
        let rise = u16::from(frames(physics[Param::SwapRiseWait]));
        if since < crouch || up.is_none_or(|n| n < rise) {
            return false;
        }
    }
    let height = physics[Param::SwapHeight];
    let apart = Fixed::from_int((ceil(p.pos.y) - ceil(other.pos.y)).abs() as i32);
    height <= Fixed::ZERO || apart < height
}

/// Starts a swap on this frame: what the player was doing stops, and
/// both characters hold still where they were.
fn start(world: &mut World, physics: &Physics, input: &Frame, other: &Other) -> Swap {
    let recent = &world.recent;
    let jumped = matches!(recent.jump, Some((0 | 1, _)));
    let wait = u16::from(frames(physics[Param::SwapWait]));
    let wait = if jumped {
        wait.saturating_sub(frames(physics[Param::SwapJumpWait]).into())
    } else {
        wait
    };
    let lag = u16::from(frames(physics[Param::SwapLag]));
    // A jump pressed up to two frames before, and held since, keeps its
    // lighter gravity, but not in a swap started by Tandem alone.
    let held_jump = input.pressed.contains(Action::Swap)
        && input.held.contains(Action::Jump)
        && (input.pressed.contains(Action::Jump) || matches!(recent.jump, Some((0 | 1, true))));
    let held_gravity = physics[Param::HeldJumpGravity];
    let gravity = if held_jump && held_gravity > Fixed::ZERO {
        held_gravity
    } else {
        physics[Param::Gravity]
    };
    let theirs = &world.feel.roster[other.character].physics;
    let player = &mut world.player;
    let mine = Move::new(player.pos, other.pos, wait, gravity, physics);
    let yours = Move::new(
        other.pos,
        player.pos,
        wait + lag,
        theirs[Param::Gravity],
        theirs,
    );
    player.vel = Vec2::default();
    player.takeoff = false;
    player.takeoff_wait = None;
    player.turn = None;
    Swap {
        frame: 0,
        end: (mine.first + mine.moves).max(yours.first + yours.moves),
        player: mine,
        other: yours,
    }
}

/// Makes the other character the player, standing still at its place
/// with its height on the whole pixel it went to. The player becomes the
/// other character where it is. How it follows from there isn't in the
/// spec; it makes its way back to the player as after resting.
fn hand_over(world: &mut World, swap: &Swap) {
    let Some(other) = world.other.take() else {
        return;
    };
    let mut player = Player::new(&world.level, Vec2::new(other.pos.x, swap.other.target));
    player.character = other.character;
    player.facing = other.facing;
    let old = std::mem::replace(&mut world.player, player);
    let points = world.feel.world[Param::FollowRejoin].floor();
    world.other = Some(Other {
        character: old.character,
        pos: old.pos,
        facing: old.facing,
        follow: Follow::rejoin(
            follow::point(old.pos),
            follow::point(world.player.pos),
            points,
        ),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(v: f64) -> Fixed {
        Fixed::from_f64(v).unwrap()
    }

    #[test]
    fn notes_runs_of_down_and_jump_presses() {
        let mut recent = Recent::default();
        let mut held = clamber_input::Actions::NONE;
        let mut press = |recent: &mut Recent, now: &[Action]| {
            let now: clamber_input::Actions = now.iter().copied().collect();
            recent.note(&Frame::after(held, now), false);
            held = now;
        };
        press(&mut recent, &[Action::Down, Action::Jump]);
        assert_eq!(recent.down, Some((0, None)));
        assert_eq!(recent.jump, Some((0, true)));
        press(&mut recent, &[Action::Down]);
        assert_eq!(recent.down, Some((1, None)));
        assert_eq!(recent.jump, Some((1, false)));
        press(&mut recent, &[]);
        press(&mut recent, &[]);
        assert_eq!(recent.down, Some((3, Some(1))));
        press(&mut recent, &[Action::Down]);
        assert_eq!(recent.down, Some((0, None)));
    }

    #[test]
    fn moves_an_equal_share_across_and_hops() {
        // swap.md's made-up example: from 100.25 to 128.5 on flat floor.
        let physics = clamber_pack::Presets::builtin().world("classic-1").unwrap();
        let from = Vec2::new(f(100.25), f(50.0));
        let to = Vec2::new(f(128.5), f(50.0));
        let mut m = Move::new(from, to, 47, physics[Param::Gravity], &physics);
        let mut pos = from;
        let mut ys = Vec::new();
        for frame in 0..=78 {
            pos = m.step(frame, pos);
            if frame == 46 {
                assert_eq!(pos, from);
            }
            if (47..=52).contains(&frame) {
                ys.push(pos.y);
            }
            if frame == 47 {
                assert_eq!(pos.x, f(101.125));
            }
            if frame >= 66 {
                assert_eq!(pos.y, f(49.953125), "{frame}");
            }
        }
        assert_eq!(pos.x, f(128.25));
        assert_eq!(
            ys,
            [
                49.44140625,
                49.3203125,
                49.63671875,
                49.390625,
                49.58203125,
                49.2109375
            ]
            .map(f)
        );
    }
}
