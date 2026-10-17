//! Collectibles: things with the `collect` behavior.
//!
//! At the end of a step, after everything has moved, the player takes
//! every collectible it's within [`Reach`] of, by what it's doing then.
//! Only the character under control takes them, and not while it's
//! knocked out or losing a life. A taken collectible is gone for the
//! rest of the visit to the level, losing a life included.
//!
//! What it's worth reaches the collectible count one at a time: it
//! releases one on the step it's taken and then one every
//! `release_every` steps, and each arrives `count_delay` steps after its
//! release. Arrivals keep coming while the player is out of play. With
//! `count_life` above 0, the count starts again from 0 when it reaches
//! that, and the player gets one more life.
//!
//! With a view, a collectible with a `window` is only taken while its
//! `x` less the view's left edge, both whole px, is inside it, by where
//! the view is after it has moved on the step. When each released part
//! reaches the count, or whether it ever does, is then up to the
//! collectible's `arrive` rules, by where it is in the view on its
//! release step (see [`Arrive`]).
//!
//! With a count display (`count_show_for` above 0), the number it shows
//! follows the count one step a frame, and each taken collectible is a
//! pickup the display waits on, from `pickup_start` steps after it's
//! taken, until as many steps as it's worth have happened; steps go to
//! the pickups in the order they were taken. See
//! [`World::count_display`] and [`World::lives_display`].
//!
//! A collectible with a `letter` is that letter of the level's word.
//! Taken, it joins the letters taken `join_delay` steps later, and the
//! letter row is shown (see [`World::letter_row`]). On the step the last
//! of the `letter_count` letters joins, the player gets one more life
//! and the letters taken start again from none. The letters taken last
//! for the visit to the level, losing a life included. With
//! `letter_end_move` above 0, the row then shows an ending instead of
//! sliding out: the letters blink and move right one at a time, and the
//! lives display comes in and shows the new number once the last letter
//! is gone.
//!
//! A collectible that's a `relic` adds one to the relic count when it's
//! taken (figure.md's figure count). With `relic_approach` above 0, its
//! image is then shown flying to a resting place at the top of the
//! picture, resting there and leaving upward (see
//! [`World::relic_image`]).
//!
//! Engine choices where the spec says nothing: a letter taken while the
//! row is shown for another shows it again from the start, and one taken
//! during the ending shows the row for it instead. Of the lives display
//! after the count reaches `count_life` and the one in the ending, the
//! one that started last is shown. Displays show at most 99 lives. A
//! part of what
//! a pickup is worth that never reaches the count is no longer owed, and
//! a step from a count with no pickup waiting (a generic gem) goes to
//! none. The relic image starts centred on where the relic was in the
//! picture before the step it's taken on, rounded down to whole px, and
//! each frame moves 1 / `relic_approach` of the distance left on each
//! axis, rounded down but at least 1 px. A relic taken while another's
//! image is shown replaces it.

use clamber_fixed::Fixed;
use clamber_pack::Param;

use crate::archetype::{Arrive, Behavior, MAX_LETTERS, Reach, Span};
use crate::flow::{Flow, FlowEvent, Outcome};
use crate::player::{Facing, Player, State, frames};
use crate::view::{self, View};
use crate::world::{Event, Phase, World};

/// A taken collectible still releasing what it's worth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Release {
    /// Still to release.
    pub left: u8,
    /// Steps until the next release.
    pub wait: u8,
    pub every: u8,
    /// Index into the world's things.
    pub thing: usize,
}

/// A taken collectible the count display waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pickup {
    /// The step it starts on.
    pub start: u32,
    /// Steps of the number shown still owed to it.
    pub owed: u8,
    /// Index into the world's things.
    pub thing: usize,
}

/// A step of the number shown from `count_life` less one to 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LivesStep {
    pub at: u32,
    /// The lives before the one it gave, and with it.
    pub before: u8,
    pub after: u8,
}

/// The letter row's ending, after the last letter of the word joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LetterEnd {
    /// The step the last letter joined on.
    pub at: u32,
    /// That letter, the late one.
    pub late: u8,
    /// The lives before the one it gave, and with it.
    pub before: u8,
    pub after: u8,
}

/// A taken relic's image, flying to its resting place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelicFlight {
    /// The step it was taken on.
    pub taken: u32,
    /// The top-left corner of the image's box, px from the picture's
    /// top-left corner.
    pub at: (i32, i32),
    /// The first step it was at its resting place on.
    pub arrived: Option<u32>,
    /// Index into the world's things.
    pub thing: usize,
}

/// A taken relic's image as drawn in a picture: the top-left corner of
/// its `size` px square box, px from the picture's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelicImage {
    /// Index into the world's things.
    pub thing: usize,
    pub x: i32,
    pub y: i32,
    pub size: i32,
}

/// The count display as drawn in a picture: the collectible image's
/// top-left corner and the number's digits, px from the picture's
/// top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CountDisplay {
    pub number: u32,
    pub image: (i32, i32),
    /// The left edge of the first digit, and the top of all of them.
    pub digits: (i32, i32),
    /// Each digit is this far right of the one before.
    pub gap: i32,
}

/// The lives display as drawn in a picture: the number of lives, and
/// the left edge of its first digit and the top of all of them, px from
/// the picture's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LivesDisplay {
    pub number: u8,
    pub x: i32,
    pub y: i32,
    /// Each digit is this far right of the one before.
    pub gap: i32,
}

/// The letter row as drawn in a picture: where the top of its images
/// is, px below the picture's top, and the left edge of each letter's
/// image, px right of its left edge, letter 1 first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LetterRow {
    pub y: i32,
    pub x: [i32; MAX_LETTERS as usize],
    /// How many of `x` are the level's word's.
    pub count: u8,
    /// The letters to draw in it: bit `k - 1` for letter `k`.
    pub letters: u8,
}

impl World {
    /// The letter row in the picture made from the last step, if it's
    /// drawn in it.
    pub fn letter_row(&self) -> Option<LetterRow> {
        let physics = &self.feel.world;
        let int = |p: Param| i32::try_from(physics[p].floor()).unwrap_or(0);
        let count = whole(physics[Param::LetterCount]).min(u32::from(MAX_LETTERS));
        let taken = self.flow.letter_taken?;
        let n = self.frame.checked_sub(1)?;
        let since = n.checked_sub(taken)?;
        if count == 0 {
            return None;
        }
        if let Some(end) = self.letter_end().filter(|e| e.at >= taken) {
            return self.ending_row(end, n, count as u8);
        }
        let since = i64::from(since);
        let (into, slide, out) = (
            i64::from(int(Param::LetterRowIn)),
            i64::from(int(Param::LetterRowSlide)),
            i64::from(int(Param::LetterRowOut)),
        );
        let (rest, rise, speed) = (
            i64::from(int(Param::LetterRowY)),
            i64::from(int(Param::LetterRowRise)),
            i64::from(int(Param::LetterRowSpeed)),
        );
        let y = slide_y(since, (into, slide, out), (rest, rise, rise, speed))?;
        let (left, gap) = (int(Param::LetterRowX), int(Param::LetterRowGap));
        let mut x = [0; MAX_LETTERS as usize];
        for (k, x) in x.iter_mut().enumerate() {
            *x = left.saturating_add(gap.saturating_mul(k as i32));
        }
        Some(LetterRow {
            y: i32::try_from(y).unwrap_or(i32::MAX),
            x,
            count: count as u8,
            letters: self.flow.letters,
        })
    }

    /// The count display in the picture made from the last step, if the
    /// world has one and it's shown in it: while a pickup that has
    /// started is still owed steps, and up to `count_show_for` frames
    /// after the latest step.
    pub fn count_display(&self) -> Option<CountDisplay> {
        let physics = &self.feel.world;
        let int = |p: Param| i32::try_from(physics[p].floor()).unwrap_or(0);
        let show_for = whole(physics[Param::CountShowFor]);
        let n = self.frame.checked_sub(1)?;
        let flow = &self.flow;
        if show_for == 0 {
            return None;
        }
        let waiting = flow.pickups.iter().any(|p| p.start <= n);
        let recent = flow
            .last_step
            .is_some_and(|s| u64::from(n) <= u64::from(s) + u64::from(show_for));
        if !waiting && !recent {
            return None;
        }
        Some(CountDisplay {
            number: flow.shown,
            image: (int(Param::CountX), int(Param::CountY)),
            digits: (int(Param::CountDigitX), int(Param::CountY)),
            gap: int(Param::CountDigitGap),
        })
    }

    /// The lives display in the picture made from the last step, if it's
    /// drawn in it: after the number shown went from `count_life` less
    /// one to 0, or with the letter row's ending, sliding in and out. Of
    /// the two, the one that started last.
    pub fn lives_display(&self) -> Option<LivesDisplay> {
        let physics = &self.feel.world;
        let int = |p: Param| i64::from(i32::try_from(physics[p].floor()).unwrap_or(0));
        if whole(physics[Param::CountShowFor]) == 0 {
            return None;
        }
        let n = self.frame.checked_sub(1)?;
        let step = self.flow.lives_step.filter(|s| s.at <= n);
        let end = self.letter_end().filter(|e| e.at <= n);
        // The step it started on, when it's drawn, when it slides out,
        // when it changes to the new number, and the numbers.
        let (at, into, out, change, before, after) = match (step, end) {
            (Some(s), e) if e.is_none_or(|e| e.at < s.at) => (
                s.at,
                int(Param::LivesIn),
                int(Param::LivesOut),
                int(Param::LivesChange),
                s.before,
                s.after,
            ),
            (_, Some(e)) => {
                let gone = i64::from(self.letter_gone(e, self.letter_count()));
                (
                    e.at,
                    int(Param::LetterEndLivesIn),
                    gone + int(Param::LetterEndLivesOut),
                    gone,
                    e.before,
                    e.after,
                )
            }
            _ => return None,
        };
        let since = i64::from(n - at);
        let y = slide_y(
            since,
            (into, int(Param::LivesSlide), out),
            (
                int(Param::LivesY),
                int(Param::LivesRise),
                int(Param::LivesGone),
                int(Param::LivesSpeed),
            ),
        )?;
        let number = if since < change { before } else { after }.min(MAX_SHOWN);
        let gap = int(Param::LivesDigitGap) as i32;
        let digits = if number >= 10 { 2 } else { 1 };
        Some(LivesDisplay {
            number,
            x: int(Param::LivesX) as i32 - (digits - 1) * gap / 2,
            y: i32::try_from(y).unwrap_or(i32::MAX),
            gap,
        })
    }

    /// The letter row's ending, if the world shows one and it has begun.
    fn letter_end(&self) -> Option<LetterEnd> {
        let end = self.flow.letter_end?;
        (whole(self.feel.world[Param::LetterEndMove]) > 0).then_some(end)
    }

    /// The letters in the level's word.
    fn letter_count(&self) -> u8 {
        whole(self.feel.world[Param::LetterCount]).min(u32::from(MAX_LETTERS)) as u8
    }

    /// Where letter `k` of the ending is in the picture made on frame
    /// `n`: its image's left edge, or `None` while it isn't drawn; and
    /// whether it's gone for good.
    fn ending_letter(&self, end: LetterEnd, k: u8, n: u32) -> (Option<i32>, bool) {
        let physics = &self.feel.world;
        let int = |p: Param| i64::from(i32::try_from(physics[p].floor()).unwrap_or(0));
        let slot = int(Param::LetterRowX) + int(Param::LetterRowGap) * i64::from(k - 1);
        let late = if k == end.late {
            int(Param::LetterEndLate)
        } else {
            0
        };
        let n = i64::from(n) - i64::from(end.at) - late;
        let at = |x: i64| Some(i32::try_from(x).unwrap_or(i32::MAX));
        let (blink, blinks) = (int(Param::LetterEndBlink), int(Param::LetterEndBlinks));
        if n >= 1 && blink > 0 && n <= 2 * blink * blinks && ((n - 1) / blink) % 2 == 0 {
            return (None, false);
        }
        let start = int(Param::LetterEndMove) + int(Param::LetterEndStagger) * i64::from(k - 1);
        if n < start {
            return (at(slot), false);
        }
        let moves = n - start + 1;
        match ending_path(
            Fixed::from_int(slot as i32),
            physics[Param::LetterEndX],
            whole(physics[Param::LetterEndApproach]),
            physics[Param::LetterEndLeast],
        )
        .nth(moves as usize - 1)
        {
            Some(x) => (at(x.floor()), false),
            None => (None, true),
        }
    }

    /// The first frame letter `count`, the word's last, is gone for good
    /// in the ending, counting from the frame the last letter joined.
    fn letter_gone(&self, end: LetterEnd, count: u8) -> u32 {
        let physics = &self.feel.world;
        let int = |p: Param| whole(physics[p]);
        let slot = int(Param::LetterRowX) + int(Param::LetterRowGap) * u32::from(count - 1);
        let late = if count == end.late {
            int(Param::LetterEndLate)
        } else {
            0
        };
        let start = int(Param::LetterEndMove) + int(Param::LetterEndStagger) * u32::from(count - 1);
        let path = ending_path(
            Fixed::from_int(slot as i32),
            physics[Param::LetterEndX],
            int(Param::LetterEndApproach),
            physics[Param::LetterEndLeast],
        );
        late + start + path.count() as u32
    }

    /// The letter row in the ending, in the picture made on frame `n`.
    fn ending_row(&self, end: LetterEnd, n: u32, count: u8) -> Option<LetterRow> {
        let mut x = [0; MAX_LETTERS as usize];
        let (mut letters, mut all_gone) = (0, true);
        for k in 1..=count {
            let (at, gone) = self.ending_letter(end, k, n);
            all_gone &= gone;
            if let Some(at) = at {
                x[usize::from(k - 1)] = at;
                letters |= 1 << (k - 1);
            }
        }
        (!all_gone).then(|| LetterRow {
            y: i32::try_from(self.feel.world[Param::LetterRowY].floor()).unwrap_or(0),
            x,
            count,
            letters,
        })
    }
}

/// The most a display shows of a number: two digits.
const MAX_SHOWN: u8 = 99;

/// Where a letter moving in the ending is on each frame it moves, from
/// `from` toward `to`: each frame 1 / `approach` of the distance left,
/// but at least `least`, and at `to` once less than `least` would be
/// left. The last is the frame it gets there.
fn ending_path(from: Fixed, to: Fixed, approach: u32, least: Fixed) -> impl Iterator<Item = Fixed> {
    let approach = i64::from(approach.max(1));
    let least = least.max(Fixed::from_bits(1));
    let mut x = Some(from);
    std::iter::from_fn(move || {
        let at = x?;
        if at >= to {
            x = None;
            return None;
        }
        let step = Fixed::from_bits((to - at).to_bits() / approach).max(least);
        let next = at + step;
        let next = if to - next < least { to } else { next };
        x = Some(next);
        Some(next)
    })
}

impl World {
    /// The image of the relic taken last in the picture made from the
    /// last step, if it's drawn in it: from the step after it's taken,
    /// flying to (`relic_x`, `relic_y`); resting there for `relic_rest`
    /// frames from the first it's there; then rising `relic_rise` px a
    /// frame until its box is wholly above the picture.
    pub fn relic_image(&self) -> Option<RelicImage> {
        let physics = &self.feel.world;
        let int = |p: Param| i64::from(i32::try_from(physics[p].floor()).unwrap_or(0));
        let n = self.frame.checked_sub(1)?;
        let flight = self.flow.relic_flight?;
        if int(Param::RelicApproach) <= 0 || n <= flight.taken {
            return None;
        }
        let size = int(Param::RelicSize);
        let (x, y) = (int(Param::RelicX), int(Param::RelicY));
        let (x, y) = match flight.arrived {
            None => (i64::from(flight.at.0), i64::from(flight.at.1)),
            Some(a) => {
                let since = i64::from(n - a) - int(Param::RelicRest);
                let y = y - int(Param::RelicRise) * (since + 1).max(0);
                if y + size <= 0 {
                    return None;
                }
                (x, y)
            }
        };
        Some(RelicImage {
            thing: flight.thing,
            x: i32::try_from(x).ok()?,
            y: i32::try_from(y).ok()?,
            size: i32::try_from(size).ok()?,
        })
    }
}

/// Moves the relic image on a step toward its resting place, noting the
/// step it gets there.
fn fly(world: &mut World) {
    let physics = &world.feel.world;
    let approach = whole(physics[Param::RelicApproach]) as i32;
    let int = |p: Param| i32::try_from(physics[p].floor()).unwrap_or(0);
    let to = (int(Param::RelicX), int(Param::RelicY));
    let frame = world.frame;
    let Some(flight) = &mut world.flow.relic_flight else {
        return;
    };
    if approach == 0 || flight.arrived.is_some() || frame <= flight.taken {
        return;
    }
    let close = |at: i32, to: i32| {
        let left = to.saturating_sub(at);
        at + left.signum() * (left.abs() / approach).max(1).min(left.abs())
    };
    flight.at = (close(flight.at.0, to.0), close(flight.at.1, to.1));
    if flight.at == to {
        flight.arrived = Some(frame);
    }
}

/// Where something that slides in and out is, `since` frames after it's
/// set off: first drawn `into` frames after, sliding down `speed` px a
/// frame for `slide` frames from `rise` above `rest`, then at `rest`,
/// and from `out` frames after sliding back up `speed` px a frame until
/// it's more than `gone` above `rest`.
fn slide_y(
    since: i64,
    (into, slide, out): (i64, i64, i64),
    (rest, rise, gone, speed): (i64, i64, i64, i64),
) -> Option<i64> {
    if since < into {
        None
    } else if since < into + slide {
        Some(rest - rise + speed * (since - into))
    } else if since < out {
        Some(rest)
    } else {
        let y = rest - speed * (since - out + 1);
        (speed != 0 && y >= rest - gone).then_some(y)
    }
}

/// Takes what the player reaches, if it can take anything on this step,
/// then releases and counts what's due. `start` is the view as it was at
/// the start of the step.
pub(crate) fn update(world: &mut World, can_take: bool, start: Option<View>) {
    note_landing(&mut world.player, &world.events);
    if can_take && world.flow.outcome == Outcome::Playing && world.player.knockout.is_none() {
        take(world, start);
    }
    fly(world);
    let life = whole(world.feel.world[Param::CountLife]);
    let frame = world.frame;
    let vertical = view::vertical(&world.player_physics());
    let mut released = Vec::new();
    for r in &mut world.flow.releases {
        r.wait = r.wait.saturating_sub(1);
        while r.wait == 0 && r.left > 0 {
            r.left -= 1;
            r.wait = r.every;
            released.push(r.thing);
        }
    }
    world.flow.releases.retain(|r| r.left > 0);
    for i in released {
        let thing = &world.things[i];
        let Some(Behavior::Collect {
            count_delay,
            arrive,
            ..
        }) = thing.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        let view = if arrive.at_start { start } else { world.view };
        let delay = match view {
            Some(v) => {
                let y = vertical.then(|| thing.pos.y.floor() - v.y());
                arrival(&arrive, thing.pos.x.floor() - v.x(), y)
            }
            None => Some(count_delay),
        };
        match delay {
            Some(delay) => world.flow.due.push(frame + u32::from(delay)),
            None => forgive(&mut world.flow, i),
        }
    }
    let flow = &mut world.flow;
    let arrived = flow.due.iter().filter(|&&f| f == frame).count();
    flow.due.retain(|&f| f != frame);
    for _ in 0..arrived {
        count(flow, life);
    }
    step_shown(flow, frame, life);
    let word = whole(world.feel.world[Param::LetterCount]).min(u32::from(MAX_LETTERS));
    let joined: Vec<u8> = flow
        .joining
        .iter()
        .filter(|&&(at, _)| at == frame)
        .map(|&(_, k)| k)
        .collect();
    flow.joining.retain(|&(at, _)| at != frame);
    let ending = whole(world.feel.world[Param::LetterEndMove]) > 0;
    for k in joined {
        join(flow, k, word, ending.then_some(frame));
    }
}

/// Takes the number shown one step towards the count, if it's behind,
/// paying the first pickup still owed and noting a step to 0 from
/// `life` less one (above 0) for the lives display.
fn step_shown(flow: &mut Flow, frame: u32, life: u32) {
    if flow.shown == flow.gems {
        return;
    }
    let wrapped = life > 0 && flow.shown + 1 >= life;
    flow.shown = if wrapped { 0 } else { flow.shown + 1 };
    flow.last_step = Some(frame);
    if let Some(p) = flow.pickups.first_mut() {
        p.owed -= 1;
    }
    flow.pickups.retain(|p| p.owed > 0);
    if wrapped {
        flow.lives_step = Some(LivesStep {
            at: frame,
            before: flow.lives.saturating_sub(1),
            after: flow.lives,
        });
    }
}

/// One part of what the thing at `thing` is worth will never reach the
/// count: the first pickup of it no longer waits for that step.
fn forgive(flow: &mut Flow, thing: usize) {
    if let Some(p) = flow.pickups.iter_mut().find(|p| p.thing == thing) {
        p.owed -= 1;
    }
    flow.pickups.retain(|p| p.owed > 0);
}

/// Adds letter `k` to the letters taken, with one more life and none
/// taken again once they hold all `word` letters (above 0). With an
/// ending, that begins it on step `ending`.
fn join(flow: &mut Flow, k: u8, word: u32, ending: Option<u32>) {
    flow.letters |= 1 << (k - 1);
    flow.events.push(FlowEvent::Letter(k));
    let all = (1u16 << word) - 1;
    if word > 0 && u16::from(flow.letters) & all == all {
        flow.letters = 0;
        let before = flow.lives;
        flow.lives = flow.lives.saturating_add(1);
        flow.events.push(FlowEvent::Life);
        if let Some(at) = ending {
            flow.letter_end = Some(LetterEnd {
                at,
                late: k,
                before,
                after: flow.lives,
            });
        }
    }
}

/// Adds one to the collectible count, starting it again from 0 with one
/// more life when it reaches `life` (above 0).
pub(crate) fn count(flow: &mut Flow, life: u32) {
    flow.gems += 1;
    if life > 0 && flow.gems >= life {
        flow.gems = 0;
        flow.lives = flow.lives.saturating_add(1);
        flow.events.push(FlowEvent::Life);
    }
}

/// Keeps count of the steps since the player last landed, while it stays
/// on the ground.
fn note_landing(player: &mut Player, events: &[Event]) {
    player.since_land = if events.contains(&Event::Land) {
        Some(0)
    } else if player.grounded() {
        player.since_land.map(|n| n.saturating_add(1))
    } else {
        None
    };
}

/// The delay `arrive` gives a collectible `x` px right of the view's
/// left edge and, with vertical view motion, `y` px below its top.
fn arrival(arrive: &Arrive, x: i64, y: Option<i64>) -> Option<u8> {
    let inside = |(from, to): (i32, i32), v: i64| (i64::from(from)..=i64::from(to)).contains(&v);
    arrive.rules[..usize::from(arrive.rule_count)]
        .iter()
        .find(|r| inside(r.x, x) && y.is_none_or(|y| inside(r.y, y)))
        .map_or(arrive.otherwise, |r| r.delay)
}

fn take(world: &mut World, start: Option<View>) {
    let view_left = world.view.map(|v| v.x());
    let (picture_x, picture_y) = start.map_or((0, 0), |v| (v.x(), v.y()));
    let shows = whole(world.feel.world[Param::RelicApproach]) > 0;
    let half = world.feel.world[Param::RelicSize].floor() / 2;
    let player = &world.player;
    let crouch_start = frames(world.player_physics()[Param::CrouchStart]);
    let x = player.pos.x.floor();
    let feet = -(-player.pos.y).floor();
    for (i, thing) in world
        .things
        .iter_mut()
        .enumerate()
        .filter(|(_, t)| t.active)
    {
        let Some(Behavior::Collect {
            worth,
            release_every,
            reach,
            window,
            letter,
            join_delay,
            pickup_start,
            relic,
            left_shift,
            ..
        }) = thing.archetype.map(|a| a.behavior)
        else {
            continue;
        };
        if thing.phase != Phase::Ready {
            continue;
        }
        if let (Some(left), Some((from, to))) = (view_left, window) {
            let x = thing.pos.x.floor() - left;
            if !(i64::from(from)..=i64::from(to)).contains(&x) {
                continue;
            }
        }
        let reach = reach[player.character.min(1)];
        let past = match player.facing {
            Facing::Right => x - thing.pos.x.floor(),
            Facing::Left => thing.pos.x.floor() + i64::from(left_shift) - x,
        };
        let below = feet - thing.pos.y.floor();
        let (Ok(past), Ok(below)) = (i32::try_from(past), i32::try_from(below)) else {
            continue;
        };
        if !span(&reach, player, crouch_start).contains(past, below) {
            continue;
        }
        thing.active = false;
        let flow = &mut world.flow;
        flow.events.push(FlowEvent::Collect(thing.index));
        if worth > 0 {
            flow.releases.push(Release {
                left: worth,
                wait: 0,
                every: release_every,
                thing: i,
            });
            flow.pickups.push(Pickup {
                start: world.frame + u32::from(pickup_start),
                owed: worth,
                thing: i,
            });
        }
        if relic {
            flow.relics += 1;
        }
        if relic && shows {
            let at = |v: Fixed, edge: i64| i32::try_from(v.floor() - edge - half).unwrap_or(0);
            flow.relic_flight = Some(RelicFlight {
                taken: world.frame,
                at: (at(thing.pos.x, picture_x), at(thing.pos.y, picture_y)),
                arrived: None,
                thing: i,
            });
        }
        if letter > 0 {
            flow.joining
                .push((world.frame + u32::from(join_delay), letter));
            flow.letter_taken = Some(world.frame);
        }
    }
}

/// The offsets `player` takes a collectible from, by what it's doing at
/// the end of a step. Down held on the ground for `crouch_start` frames
/// (above 0) is crouching.
fn span(reach: &Reach, player: &Player, crouch_start: u8) -> Span {
    if let Some(age) = player.jump_age {
        if age < reach.jump_early {
            return reach.jump;
        }
        return air(reach, player.vel.y);
    }
    if !player.grounded() {
        return air(reach, player.vel.y);
    }
    if player.since_land.is_some_and(|n| n < reach.land_hold) {
        return reach.air;
    }
    if player.state == State::Roll || player.roll.is_some() {
        return reach.roll;
    }
    if crouch_start > 0 && player.crouch.held >= crouch_start {
        return reach.crouch;
    }
    match player.state {
        State::Idle | State::GroundAction => reach.stand,
        _ => reach.walk,
    }
}

/// `air`, with its lowest `below` lifted for the vertical speed `vy`.
fn air(reach: &Reach, vy: Fixed) -> Span {
    let mut span = reach.air;
    let steps = &reach.air_below[..usize::from(reach.air_steps)];
    if let Some(&(_, below)) = steps.iter().find(|&&(at, _)| vy <= at) {
        span.below.0 = below;
    }
    span
}

/// A whole number from a parameter, 0 if it's below 1.
fn whole(v: Fixed) -> u32 {
    u32::try_from(v.floor()).unwrap_or(0)
}
