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
//! A collectible that gives `lives` adds them on the step it's taken,
//! and the lives display comes in and steps up to the new number one
//! at a time (see [`World::lives_display`]). One that's there `once` is
//! gone for good when it's taken. Taken while that display is shown,
//! another holds its steps until its image arrives, and brings it back
//! down if it's leaving. One that's `late` does all that a frame later.
//! The count reaching `count_life` and the letter row's ending give
//! their lives through the same display (see [`LivesGain`]).
//!
//! A letter taken while the row is shown, before its last frame at rest,
//! keeps it there for as long as if it had been taken first; one taken
//! later brings in another row, while the first slides out. A letter
//! after that acts on the row brought in last.
//!
//! Engine choices where the spec says nothing: a letter taken during the
//! ending shows the row for it instead, and a third row coming in while
//! two are drawn takes the place of the older one. The number
//! of lives goes past 99, and displays show it as 99 (extra-lives.md,
//! "More than 99 lives"); how far past isn't covered, and Clamber's
//! stops at 255. A part of what
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
use crate::pose::Air;
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

/// Lives gained, for the lives display: by a collectible giving
/// `lives`, by the number shown going from `count_life` less one to 0,
/// or by the letter row's ending.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LivesGain {
    /// The step it acts on the display from: it starts one if none is
    /// shown, and brings it back down if it's leaving.
    pub at: u32,
    /// The steps it holds the display's steps on: from the first, up to
    /// but not including the second.
    pub held: (u32, u32),
    /// The step from which the number the display steps toward counts
    /// the lives it gave.
    pub counts: u32,
    /// With the letter row's ending, the display steps up on the first
    /// step from this one that's not held, and doesn't leave from `at`
    /// until then.
    pub step: Option<u32>,
    /// The lives before it, and with it.
    pub before: u8,
    pub after: u8,
}

/// A life lost by a hit or a fall-out, for the lives display
/// (count-display.md, "Losing a life").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LostLife {
    /// The step the life was lost on: the hit, or the fall-out.
    pub at: u32,
    /// The step the display first slides in on.
    pub into: u32,
    /// The step the image bursts on, once it's known: after a hit, from
    /// the step the knock-out's flight stops.
    pub burst: Option<u32>,
    /// The lives before the loss, and after it.
    pub before: u8,
    pub after: u8,
    /// Lost by a knock-out, not by falling out.
    pub knocked: bool,
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

/// A letter row shown after a letter was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LetterShow {
    /// The step the letter that brought the row in was taken on.
    pub from: u32,
    /// The step the last letter it shows was taken on.
    pub last: u32,
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
    /// Whether the image beside the number is drawn: not while it bursts
    /// after a lost life.
    pub image: bool,
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
    /// drawn in it: of [`letter_rows`](World::letter_rows), the one that
    /// came in last.
    pub fn letter_row(&self) -> Option<LetterRow> {
        self.letter_rows().pop()
    }

    /// The letter rows in the picture made from the last step, the one
    /// that came in first first: while one slides out, a letter taken
    /// then brings in another (letters.md, "Another letter while the row
    /// is shown").
    pub fn letter_rows(&self) -> Vec<LetterRow> {
        let physics = &self.feel.world;
        let int = |p: Param| i32::try_from(physics[p].floor()).unwrap_or(0);
        let count = whole(physics[Param::LetterCount]).min(u32::from(MAX_LETTERS));
        let (Some(shown), Some(n)) = (self.flow.letter_shown, self.frame.checked_sub(1)) else {
            return Vec::new();
        };
        if count == 0 {
            return Vec::new();
        }
        if let Some(end) = self.letter_end().filter(|e| e.at >= shown.last) {
            return self.ending_row(end, n, count as u8).into_iter().collect();
        }
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
        let (left, gap) = (int(Param::LetterRowX), int(Param::LetterRowGap));
        let mut x = [0; MAX_LETTERS as usize];
        for (k, x) in x.iter_mut().enumerate() {
            *x = left.saturating_add(gap.saturating_mul(k as i32));
        }
        [self.flow.letter_before, Some(shown)]
            .into_iter()
            .flatten()
            .filter(|s| s.from <= n)
            .filter_map(|s| {
                // Letters taken while it's shown move its end on.
                let out = out + i64::from(s.last - s.from);
                let since = i64::from(n - s.from);
                slide_y(since, (into, slide, out), (rest, rise, rise, speed))
            })
            .map(|y| LetterRow {
                y: i32::try_from(y).unwrap_or(i32::MAX),
                x,
                count: count as u8,
                letters: self.flow.letters,
            })
            .collect()
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
    /// one to 0, with the letter row's ending, or after a collectible
    /// giving lives was taken, sliding in and out. Of those, the one that
    /// started last. From a lost life on, the display for it instead.
    pub fn lives_display(&self) -> Option<LivesDisplay> {
        if whole(self.feel.world[Param::CountShowFor]) == 0 {
            return None;
        }
        let n = self.frame.checked_sub(1)?;
        match self.flow.lost.filter(|l| l.at <= n) {
            Some(lost) => self.lost_lives(lost, n),
            None => self.gained_lives(n),
        }
    }

    /// The lives display after a lost life, in the picture made on frame
    /// `n`: in with the lives before the loss, the image bursting, then
    /// in again with the lives after it, which stays. A display from a
    /// gain at rest on the frame of the loss stays as it is up to the
    /// burst instead of sliding in. A knock-out from just before the
    /// letter row's ending on has its own (see
    /// [`knocked_out_ending`](Self::knocked_out_ending)). Engine choice
    /// where the spec says nothing: another display sliding in or out
    /// then gives way to this one.
    fn lost_lives(&self, lost: LostLife, n: u32) -> Option<LivesDisplay> {
        let hold = whole(self.player_physics()[Param::KnockoutHold]).saturating_sub(1);
        if let Some(end) = self
            .letter_end()
            .filter(|e| lost.knocked && e.at <= n && e.at <= lost.at + hold + 1)
        {
            return self.knocked_out_ending(end, lost, hold, n);
        }
        let physics = &self.feel.world;
        let int = |p: Param| i64::from(i32::try_from(physics[p].floor()).unwrap_or(0));
        let rest = int(Param::LivesY);
        let burst = lost.burst.map_or(i64::MAX, i64::from);
        let hidden = burst.saturating_add(int(Param::LostLivesBurstFor));
        let again = burst.saturating_add(int(Param::LostLivesAgain));
        let n = i64::from(n);
        let held = self
            .gained_lives(lost.at)
            .filter(|d| i64::from(d.y) == rest && n < hidden);
        if let Some(held) = held {
            let shown = self.gained_lives(u32::try_from(n).unwrap_or(u32::MAX));
            return Some(LivesDisplay {
                number: shown.map_or(held.number, |d| d.number),
                image: n < burst,
                ..held
            });
        }
        let (from, number) = if n < hidden {
            (i64::from(lost.into), lost.before)
        } else {
            (again, lost.after)
        };
        let y = slide_y(
            n - from,
            (0, int(Param::LivesSlide), i64::MAX),
            (rest, int(Param::LivesRise), 0, int(Param::LivesSpeed)),
        )?;
        Some(self.lives_at(number, y, n < burst || n >= again))
    }

    /// The lives display with the letter row's ending `end` and a
    /// knock-out that began `hold` + 1 frames or less before the last
    /// letter joined, or later, in the picture made on frame `n`
    /// (letters.md, "While the player is knocked out").
    ///
    /// The number has a place on every frame, drawn or not: as without
    /// the knock-out up to the hit, held there for `hold` frames, then
    /// coming down from there to rest, leaving no more. From 3 frames
    /// after the burst it comes in again from above, as after any lost
    /// life. It shows the lives before the ending's life and the loss,
    /// one less from 3 frames after the burst, and one more from when
    /// the ending gives its life, a little later when that falls on the
    /// frames held after the hit or those it comes in again on.
    fn knocked_out_ending(
        &self,
        end: LetterEnd,
        lost: LostLife,
        hold: u32,
        n: u32,
    ) -> Option<LivesDisplay> {
        let physics = &self.feel.world;
        let int = |p: Param| i64::from(i32::try_from(physics[p].floor()).unwrap_or(0));
        let (rest, rise) = (int(Param::LivesY), int(Param::LivesRise));
        let (gone, speed) = (int(Param::LivesGone), int(Param::LivesSpeed));
        let (into, out) = (int(Param::LetterEndLivesIn), int(Param::LetterEndLivesOut));
        let (burst_for, again) = (int(Param::LostLivesBurstFor), int(Param::LostLivesAgain));
        let (hold, n) = (i64::from(hold), i64::from(n));
        let (joined, hit) = (i64::from(end.at), i64::from(lost.at));
        let shows = joined + i64::from(self.letter_gone(end, self.letter_count()));
        let burst = lost.burst.map_or(i64::MAX, i64::from);
        // Where the number is while it waits to come in.
        let off = rest - rise - speed * (again - burst_for);
        let mut y = if hit < joined {
            off
        } else if hit < shows + out {
            (rest - rise + speed * (hit - joined - into)).min(rest)
        } else if hit < shows + int(Param::LetterEndLivesEnd) {
            rest - speed * (hit - shows - out + 1)
        } else {
            off
        };
        for f in hit + hold + 1..=n {
            y = if f == burst.saturating_add(burst_for) {
                off
            } else {
                (y + speed).min(rest)
            };
        }
        // When the number shown goes up for the ending's life.
        let back = burst.saturating_add(again + int(Param::LivesSlide));
        let up = if (hit + 1..=hit + hold).contains(&shows) {
            hit + hold + 1
        } else if (burst.saturating_add(burst_for + 1)..=back).contains(&shows) {
            burst.saturating_add(LATE_LIFE)
        } else {
            shows
        };
        let mut number = if joined <= hit {
            end.before
        } else {
            lost.before
        };
        if n >= burst.saturating_add(burst_for) {
            number = number.saturating_sub(1);
        }
        if n >= up {
            number = number.saturating_add(1);
        }
        let image = n < burst || n >= burst.saturating_add(again);
        (y >= rest - gone).then(|| self.lives_at(number, y, image))
    }

    /// The lives display after a gain, in the picture made on frame `n`.
    fn gained_lives(&self, n: u32) -> Option<LivesDisplay> {
        let (number, y) = self.item_lives(n)?;
        y.map(|y| self.lives_at(number, y, true))
    }

    /// The lives display for the lives gained since it last started, in
    /// the picture made on frame `n`: the number shown, and the top of
    /// the number if it's drawn. None once it has ended, or before it
    /// starts.
    ///
    /// Frames a gain holds are held; the number shown goes up one at a
    /// time toward the lives the gains count by then, counting only
    /// frames that aren't held (extra-lives.md, "Several items").
    pub(crate) fn item_lives(&self, n: u32) -> Option<(u8, Option<i64>)> {
        let gains = &self.flow.gains;
        let first = gains.iter().map(|g| g.at).min().filter(|&at| at <= n)?;
        let physics = &self.feel.world;
        let int = |p: Param| i64::from(i32::try_from(physics[p].floor()).unwrap_or(0));
        let (into, slide) = (int(Param::LifeItemLivesIn), int(Param::LivesSlide));
        let (rest, rise) = (int(Param::LivesY), int(Param::LivesRise));
        let (gone, speed) = (int(Param::LivesGone), int(Param::LivesSpeed));
        let (out, end) = (int(Param::LifeItemLivesOut), int(Param::LifeItemLivesEnd));
        let first_step = whole(physics[Param::LifeItemLivesStep]);
        let every = whole(physics[Param::LifeItemLivesEvery]);
        // The step the display last started on, and the lives then.
        let mut start: Option<(u32, u32)> = None;
        let lives_on = |(from, base): (u32, u32), f: u32| {
            gains
                .iter()
                .filter(|g| g.at >= from && g.counts <= f)
                .fold(base, |l, g| l + u32::from(g.after.saturating_sub(g.before)))
        };
        let (mut shown, mut lives) = (0, 0);
        // Free frames until the next step, counting the one it's on.
        let mut wait: Option<u32> = None;
        // The last step, which made the number shown the lives.
        let mut last: Option<u32> = None;
        // The step from which the letter row's ending steps it up, until
        // it has.
        let mut pending: Option<u32> = None;
        let mut y = None;
        // Turned back while leaving, it comes down to rest.
        let mut down = false;
        for f in first..=n {
            let (before, short) = (y, shown < lives);
            let mut taken = gains.iter().filter(|g| g.at == f).peekable();
            if start.is_none() {
                let Some(g) = taken.peek() else { continue };
                start = Some((f, u32::from(g.before)));
                (shown, wait, last, pending, y, down) =
                    (u32::from(g.before), None, None, None, None, false);
            }
            let leaving = pending.is_none()
                && last.is_some_and(|e| shown == lives && i64::from(f - e) >= out);
            for g in taken {
                match g.step {
                    Some(at) => pending = Some(at),
                    // Taken on the frame after the last step, the next
                    // comes on the first free frame.
                    None if last.is_some_and(|e| e + 1 == f) => wait = Some(1),
                    None => {}
                }
                down |= leaving;
            }
            let (from, base) = start?;
            lives = lives_on((from, base), f);
            if pending == Some(f) {
                wait = Some(1);
            }
            let held = gains.iter().any(|g| (g.held.0..g.held.1).contains(&f));
            if !held {
                match wait {
                    Some(w) if w > 1 => wait = Some(w - 1),
                    Some(_) if shown < lives => {
                        shown += 1;
                        pending = pending.filter(|&at| at > f);
                        wait = (shown < lives).then_some(every);
                        last = (shown == lives).then_some(f).or(last);
                    }
                    Some(_) => wait = None,
                    // Counted from the first free frame on which it's
                    // short of the lives, after one at rest and short.
                    None if shown < lives && short && before == Some(rest) => {
                        wait = Some(first_step)
                    }
                    None => {}
                }
            }
            let since = i64::from(f - from);
            y = if since < into + slide {
                (since >= into).then(|| rest - rise + speed * (since - into))
            } else if down {
                let to = y.unwrap_or(rest).saturating_add(speed).min(rest);
                down = to < rest;
                Some(to)
            } else if let Some(e) = last.filter(|_| shown == lives && pending.is_none()) {
                let after = i64::from(f - e);
                if after >= end {
                    start = None;
                    None
                } else {
                    Some(rest - speed * (after - out + 1).max(0))
                }
            } else {
                Some(rest)
            };
        }
        start?;
        let shown = u8::try_from(shown).unwrap_or(u8::MAX);
        Some((shown, y.filter(|&y| y >= rest - gone)))
    }

    /// The lives display showing `number` lives with its top at `y`.
    fn lives_at(&self, number: u8, y: i64, image: bool) -> LivesDisplay {
        let physics = &self.feel.world;
        let int = |p: Param| i32::try_from(physics[p].floor()).unwrap_or(0);
        let number = number.min(MAX_SHOWN);
        let gap = int(Param::LivesDigitGap);
        let digits = if number >= 10 { 2 } else { 1 };
        LivesDisplay {
            number,
            x: int(Param::LivesX) - (digits - 1) * gap / 2,
            y: i32::try_from(y).unwrap_or(i32::MAX),
            image,
            gap,
        }
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

/// With a knock-out in the letter row's ending, the ending's life shown
/// this many frames after the burst when it would have been shown while
/// the lives number was away or coming in again (letters.md, "While the
/// player is knocked out").
const LATE_LIFE: i64 = 28;

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
    // The gains of a lives display that has ended are done with.
    let frame = world.frame;
    if world.flow.gains.iter().all(|g| g.at < frame)
        && frame
            .checked_sub(1)
            .is_none_or(|n| world.item_lives(n).is_none())
    {
        world.flow.gains.clear();
    }
    if can_take && world.flow.outcome == Outcome::Playing && world.player.knockout.is_none() {
        take(world, start);
    }
    fly(world);
    let life = whole(world.feel.world[Param::CountLife]);
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
    // The ending's life, for the lives display (extra-lives.md, "The
    // four-letter ending").
    if let Some(end) = world.letter_end().filter(|e| e.at == frame) {
        let shows = frame + world.letter_gone(end, world.letter_count());
        let held = frame + whole(world.feel.world[Param::LetterEndLivesHeld]);
        world.flow.gains.push(LivesGain {
            at: frame,
            held: (held, shows),
            counts: shows,
            step: Some(shows),
            before: end.before,
            after: end.after,
        });
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
        // As a collectible giving a life taken on the next step, with
        // its image there at once (extra-lives.md, "The count reaching
        // 100").
        flow.gains.push(LivesGain {
            at: frame + 1,
            held: (frame + 1, frame + 1),
            counts: frame + 1,
            step: None,
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
/// on the ground, and since its last bounce.
fn note_landing(player: &mut Player, events: &[Event]) {
    let was_down = player.since_land.is_some();
    player.since_land = if events.contains(&Event::Land) {
        Some(0)
    } else if player.grounded() {
        player.since_land.map(|n| n.saturating_add(1))
    } else {
        None
    };
    player.since_bounce = if events.contains(&Event::Bounce) {
        Some((0, player.frozen))
    } else if events.contains(&Event::JumpStart) {
        None
    } else {
        match player.since_bounce {
            // Into the air again some other way, it's over.
            Some(_) if !player.grounded() && was_down => None,
            Some((n, held)) if !player.grounded() => Some((n.saturating_add(1), held)),
            kept => kept,
        }
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
    let walking_out = world.ending.is_some();
    let arrive = whole(world.feel.world[Param::LifeItemLivesArrive]);
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
            lives,
            rise,
            once,
            kept,
            late,
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
        let y = if rise > Fixed::ZERO {
            -(-thing.pos.y).floor()
        } else {
            thing.pos.y.floor()
        };
        let below = feet - y;
        let (Ok(past), Ok(below)) = (i32::try_from(past), i32::try_from(below)) else {
            continue;
        };
        // During the walk-out, by the walking ranges.
        let span = match walking_out {
            true => reach.walk,
            false => span(&reach, player, crouch_start),
        };
        if !span.contains(past, below) {
            continue;
        }
        thing.active = false;
        if once || kept {
            thing.phase = Phase::Gone;
        }
        let flow = &mut world.flow;
        flow.events.push(FlowEvent::Collect(thing.index));
        if lives > 0 {
            let before = flow.lives;
            flow.lives = flow.lives.saturating_add(lives);
            for _ in before..flow.lives {
                flow.events.push(FlowEvent::Life);
            }
            // A late one counts as if taken on the next step.
            let at = world.frame + u32::from(late);
            flow.gains.push(LivesGain {
                at,
                held: (at, at + arrive),
                counts: at,
                step: None,
                before,
                after: flow.lives,
            });
        }
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
            // Taken before the row's last frame at rest, the row goes on
            // with its end moved; later, another row comes in.
            let out = whole(world.feel.world[Param::LetterRowOut]);
            flow.letter_shown = match flow.letter_shown {
                Some(s) if world.frame < (s.last + out).saturating_sub(1) => Some(LetterShow {
                    last: world.frame,
                    ..s
                }),
                before => {
                    flow.letter_before = before;
                    Some(LetterShow {
                        from: world.frame,
                        last: world.frame,
                    })
                }
            };
        }
    }
}

/// The offsets `player` takes a collectible from, by what it's doing at
/// the end of a step. Down held on the ground for `crouch_start` frames
/// (above 0) is crouching. The ground action stands, also in the air
/// after it goes over a ledge. With a `bounce` row, a bounce takes it
/// until landing, except on the frames held still after it, and the
/// landing goes straight to the ranges on the ground.
pub(crate) fn span(reach: &Reach, player: &Player, crouch_start: u8) -> Span {
    if let Some(age) = player.jump_age {
        if age < reach.jump_early {
            return reach.jump;
        }
        return air(reach, player.vel.y);
    }
    if player.ground_action.is_some() {
        return reach.stand;
    }
    let bounced = reach.bounce.zip(player.since_bounce);
    if let Some((bounce, (n, held))) = bounced
        && !player.grounded()
    {
        return if (1..=held).contains(&n) {
            Span::NONE
        } else {
            bounce
        };
    }
    let clock = &player.pose;
    let upright = reach.upright_fall && clock.upright_fall && clock.air == Air::Fall;
    if !upright && !player.grounded() {
        return air(reach, player.vel.y);
    }
    if !upright
        && bounced.is_none()
        && player
            .since_land
            .is_some_and(|n| n < land_hold(reach, player))
    {
        return reach.air;
    }
    let (on, stopped) = reach.roll_after;
    let rolled = clock
        .rolled
        .is_some_and(|(n, ran_on)| n <= if ran_on { on } else { stopped });
    if player.state == State::Roll || player.roll.is_some() || rolled {
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

/// How many frames from the landing frame on `player` keeps the `air`
/// offsets after its last landing.
fn land_hold(reach: &Reach, player: &Player) -> u8 {
    let clock = &player.pose;
    let hold = if clock.landed_running {
        reach.land_ran
    } else {
        reach.land_hold
    };
    let more = reach
        .land_drop
        .filter(|&(below, _)| clock.drop > Fixed::from_int(below))
        .map_or(0, |(_, more)| more);
    hold.saturating_add(more)
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
