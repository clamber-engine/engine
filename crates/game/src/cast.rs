//! The player and entities as sprites, animated by what they're doing.
//!
//! A [`Cast`] holds the sheets: `player` for the player, and one per
//! entity kind, named after the kind. Each logic frame, every actor picks
//! the animation for what it's doing ([`Poses::update`]); an animation
//! carries on while it stays the same and starts over when it changes.
//!
//! The player's animations are named after its states, with `hurt` while
//! it's losing a life, `recover` while it's held still after a roll, and
//! `land` while it stands after a landing ([`player_animation`]). Sprites face right and are mirrored to face left.
//! A character in roster slot `n` past the first uses the sheet's `n`th
//! named palette or variant image, in name order, if it has one. Where a
//! pack has a sheet of a character's own pictures ([`CHARACTER`] and its
//! roster slot), the player shows them on the frames they're given for
//! ([`crate::pictures`]) instead, and so do walkers
//! ([`crate::walker_pictures`]), throwers and balls
//! ([`crate::thrower_pictures`]) with sheets of theirs. Single
//! collectibles that spin show their step's picture
//! ([`crate::spin_pictures`]), and bunches their entry's
//! ([`crate::bunch_pictures`]), and thrown objects in flight and lying
//! theirs ([`crate::thrown_pictures`]). Entities play their
//! sheet's first animation, except a reached checkpoint, which plays
//! `reached`, a defeated enemy, which plays `defeated`, a launcher
//! that just fired, which plays `fire`, and a spring whose top is pressed
//! down, which plays `squash`. Entities facing left are
//! mirrored too.

use std::collections::BTreeMap;
use std::path::Path;

use clamber_sim::{
    Behavior, CHECKPOINT, Event, Facing, FlowEvent, Outcome, Phase, State, Thing, World, spring_top,
};
use clamber_sprite::{Playback, SpriteSheet};

use crate::bunch_pictures;
use crate::pictures::{self, PictureClock};
use crate::spin_pictures;
use crate::sprites::{PaletteId, SheetId, Sprite, SpriteBank, SpriteBankBuilder};
use crate::surface::Surface;
use crate::thrower_pictures::{self, Group};
use crate::thrown_pictures;
use crate::walker_pictures::{self, Size, WalkerClock};

/// The sheet the player is drawn with.
pub const PLAYER: &str = "player";

/// What a character's own sheet is named, before its roster slot: the
/// pictures it shows when held still ([`crate::pictures`]).
pub const CHARACTER: &str = "character-";

/// The sprite sheets the player and entities are drawn with.
#[derive(Clone, Debug)]
pub struct Cast {
    bank: SpriteBank,
    player: Option<SheetId>,
    /// The player's palettes or variants for roster slots after the first.
    alternates: Vec<PaletteId>,
    /// Characters' own sheets, by roster slot.
    characters: BTreeMap<usize, SheetId>,
    kinds: BTreeMap<String, SheetId>,
}

impl Default for Cast {
    /// No sheets: everything is drawn as markers.
    fn default() -> Cast {
        Cast::new(Vec::new())
    }
}

impl Cast {
    /// Sheets by name: [`PLAYER`], or an entity kind.
    pub fn new(sheets: Vec<(String, SpriteSheet)>) -> Cast {
        let mut builder = SpriteBankBuilder::new();
        let ids: Vec<_> = sheets
            .into_iter()
            .map(|(name, sheet)| (name, builder.add(&sheet)))
            .collect();
        let bank = builder.build();
        let mut kinds: BTreeMap<_, _> = ids.into_iter().collect();
        let player = kinds.remove(PLAYER);
        let alternates = player.map_or_else(Vec::new, |id| bank.palettes(id).collect());
        let mut characters = BTreeMap::new();
        kinds.retain(|name, &mut id| {
            let slot = name.strip_prefix(CHARACTER).and_then(|n| n.parse().ok());
            if let Some(slot) = slot {
                characters.insert(slot, id);
            }
            slot.is_none()
        });
        Cast {
            bank,
            player,
            alternates,
            characters,
            kinds,
        }
    }

    /// Reads every sheet in `dir`, each named after its metadata file.
    pub fn load(dir: &Path) -> Result<Cast, String> {
        Cast::load_layered(&[dir])
    }

    /// Reads every sheet in each of `dirs`, each named after its
    /// metadata file. A sheet replaces one of the same name from an
    /// earlier folder, so a pack's sheets stand in for the engine's own
    /// and the engine's draw what the pack has no sheet for.
    pub fn load_layered(dirs: &[&Path]) -> Result<Cast, String> {
        Ok(Cast::new(Cast::load_sheets(dirs)?.into_iter().collect()))
    }

    /// The sheets [`Cast::load_layered`] makes a cast of, by name.
    pub fn load_sheets(dirs: &[&Path]) -> Result<BTreeMap<String, SpriteSheet>, String> {
        let mut sheets = BTreeMap::new();
        for dir in dirs {
            let err = |e: std::io::Error| format!("{}: {e}", dir.display());
            let mut paths = Vec::new();
            for entry in std::fs::read_dir(dir).map_err(err)? {
                let path = entry.map_err(err)?.path();
                if path.extension().is_some_and(|e| e == "json") {
                    paths.push(path);
                }
            }
            for path in paths {
                let name = path.file_stem().unwrap_or_default().to_string_lossy();
                let sheet = SpriteSheet::load(&path).map_err(|e| e.to_string())?;
                sheets.insert(name.into_owned(), sheet);
            }
        }
        Ok(sheets)
    }

    /// Whether the player has a sheet to be drawn with.
    pub fn has_player(&self) -> bool {
        self.player.is_some()
    }

    /// Whether any character has a sheet of its own pictures.
    pub fn has_pictures(&self) -> bool {
        !self.characters.is_empty()
    }

    /// Whether things of `kind` have a sheet to be drawn with.
    pub fn has_kind(&self, kind: &str) -> bool {
        self.kinds.contains_key(kind)
    }

    /// Draws the player in `pose` with its feet at `(x, y)`.
    pub fn draw_player(
        &self,
        canvas: &mut dyn Surface,
        world: &World,
        pose: &Pose,
        x: f32,
        y: f32,
    ) {
        let player = &world.player;
        if let Some(sprite) = self.picture(player.character, pose, x, y) {
            let sprite = Sprite {
                flip_x: player.facing == Facing::Left,
                ..sprite
            };
            self.bank.draw(canvas, &sprite);
            return;
        }
        self.draw_character(canvas, player.character, player.facing, pose, x, y);
    }

    /// The player's own picture for `pose`, from the sheet of the
    /// character in roster slot `slot`, if it has the picture's whole set.
    /// Its origin is a px left of the feet (`formats/sprites.md`, "Where
    /// the picture is drawn").
    fn picture(&self, slot: usize, pose: &Pose, x: f32, y: f32) -> Option<Sprite> {
        let (set, label) = pose.pictures.shown()?;
        let &id = self.characters.get(&slot)?;
        let sheet = self.bank.sheet(id);
        let all = pictures::labels(slot, set)
            .iter()
            .all(|l| sheet.animation(l).is_some());
        let frame = *sheet.animation(label).filter(|_| all)?.frames.first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
    }

    /// Draws the other character in `pose` with its feet at `(x, y)`, if
    /// there is one.
    pub fn draw_other(&self, canvas: &mut dyn Surface, world: &World, pose: &Pose, x: f32, y: f32) {
        if let Some(other) = &world.other {
            self.draw_character(canvas, other.character, other.facing, pose, x, y);
        }
    }

    /// Draws the character lost on a hit in `pose` with its feet at
    /// `(x, y)`, if one is on its way out.
    pub fn draw_leaving(
        &self,
        canvas: &mut dyn Surface,
        world: &World,
        pose: &Pose,
        x: f32,
        y: f32,
    ) {
        if let Some(body) = world.leaving() {
            self.draw_character(canvas, body.character, body.facing, pose, x, y);
        }
    }

    /// Draws the character in roster slot `character` with the player's
    /// sheet, in its palette or variant.
    fn draw_character(
        &self,
        canvas: &mut dyn Surface,
        character: usize,
        facing: Facing,
        pose: &Pose,
        x: f32,
        y: f32,
    ) {
        let Some(id) = self.player else {
            return;
        };
        let palette = character
            .checked_sub(1)
            .and_then(|slot| self.alternates.get(slot).copied());
        let sprite = Sprite {
            flip_x: facing == Facing::Left,
            palette,
            ..self.sprite(id, pose, x, y)
        };
        self.bank.draw(canvas, &sprite);
    }

    /// Draws `thing` in `pose` with its origin at `(x, y)`.
    pub fn draw_thing(&self, canvas: &mut dyn Surface, thing: &Thing, pose: &Pose, x: f32, y: f32) {
        self.draw_thing_faded(canvas, thing, pose, (x, y), 255);
    }

    /// Draws `thing` as [`Cast::draw_thing`] does, `opacity` from 0 (not
    /// at all) to 255 (as it is).
    pub fn draw_thing_faded(
        &self,
        canvas: &mut dyn Surface,
        thing: &Thing,
        pose: &Pose,
        (x, y): (f32, f32),
        opacity: u8,
    ) {
        if let Some(step) = pose.spin {
            if let Some(sprite) = self.spin_picture(thing, pose, step, x, y) {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        if let Some(sprite) = self.carried_picture(thing, pose, x, y) {
            self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            return;
        }
        if let Some(count) = pose.bunch {
            if let Some(sprite) = self.bunch_picture(thing, pose, count, x, y) {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        let facing = pose.walker.facing().unwrap_or(thing.facing);
        let shown = thrower_pictures::shown(thing);
        let sprite = match self.walker_picture(pose, x, y) {
            Some(sprite) => Some(Sprite {
                flip_x: facing == Facing::Left,
                ..sprite
            }),
            None => shown.and_then(|shown| self.thrower_picture(shown, x, y)),
        };
        let sprite = sprite.or_else(|| {
            let &id = self.kinds.get(&thing.kind)?;
            // The engine's art keeps a ball's mirroring.
            let (flip_x, flip_y) = match shown {
                Some(shown) if shown.group == Group::Ball => (shown.flip_x, shown.flip_y),
                _ => (facing == Facing::Left, false),
            };
            Some(Sprite {
                flip_x,
                flip_y,
                ..self.sprite(id, pose, x, y)
            })
        });
        if let Some(sprite) = sprite {
            self.bank.draw(canvas, &Sprite { opacity, ..sprite });
        }
    }

    /// A thrower's or a ball's own picture, from its group's sheet, if
    /// that has the whole group, mirrored as `shown` says. Its origin is
    /// a px left of the thing's position (`formats/sprites.md`, "Where
    /// the picture is drawn").
    fn thrower_picture(&self, shown: thrower_pictures::Shown, x: f32, y: f32) -> Option<Sprite> {
        let &id = self.kinds.get(shown.group.sheet())?;
        let sheet = self.bank.sheet(id);
        let all = thrower_pictures::labels(shown.group)
            .iter()
            .all(|l| sheet.animation(l).is_some());
        let frame = *sheet
            .animation(shown.label)
            .filter(|_| all)?
            .frames
            .first()?;
        Some(Sprite {
            flip_x: shown.flip_x,
            flip_y: shown.flip_y,
            ..Sprite::new(id, frame, x - 1.0, y)
        })
    }

    /// A spinning collectible's picture on `step`: its own from
    /// [`spin_pictures::SHEET`], mirrored left to right with its origin
    /// at the collectible's position, if the step has one and the sheet
    /// has them all; otherwise the frame of its kind's animation that
    /// far through the turn.
    fn spin_picture(&self, thing: &Thing, pose: &Pose, step: u8, x: f32, y: f32) -> Option<Sprite> {
        let own = self.kinds.get(spin_pictures::SHEET).and_then(|&id| {
            let sheet = self.bank.sheet(id);
            let all = spin_pictures::LABELS
                .iter()
                .all(|l| sheet.animation(l).is_some());
            let label = spin_pictures::label(step).filter(|_| all)?;
            let frame = *sheet.animation(label)?.frames.first()?;
            Some(Sprite {
                flip_x: true,
                ..Sprite::new(id, frame, x, y)
            })
        });
        own.or_else(|| {
            let &id = self.kinds.get(&thing.kind)?;
            let frames = self
                .bank
                .sheet(id)
                .animations
                .get(pose.animation)
                .map_or(&[][..], |a| &a.frames[..]);
            let at = usize::from(step - 1) * frames.len() / spin_pictures::STEPS as usize;
            Some(Sprite::new(id, frames.get(at).copied().unwrap_or(0), x, y))
        })
    }

    /// A bunch's picture with `count` counted frames since it appeared:
    /// its own from [`bunch_pictures::SHEET`], unmirrored with its
    /// origin at the bunch's position, if the sheet has all six;
    /// otherwise the frame of its kind's animation that far through the
    /// cycle.
    fn bunch_picture(
        &self,
        thing: &Thing,
        pose: &Pose,
        count: u32,
        x: f32,
        y: f32,
    ) -> Option<Sprite> {
        let entry = bunch_pictures::entry(count);
        let own = self.kinds.get(bunch_pictures::SHEET).and_then(|&id| {
            let sheet = self.bank.sheet(id);
            let all = bunch_pictures::LABELS
                .iter()
                .all(|l| sheet.animation(l).is_some());
            let label = all.then_some(bunch_pictures::label(entry))?;
            let frame = *sheet.animation(label)?.frames.first()?;
            Some(Sprite::new(id, frame, x, y))
        });
        own.or_else(|| {
            let &id = self.kinds.get(&thing.kind)?;
            let frames = self
                .bank
                .sheet(id)
                .animations
                .get(pose.animation)
                .map_or(&[][..], |a| &a.frames[..]);
            let at = entry as usize * frames.len() / bunch_pictures::CYCLE as usize;
            Some(Sprite::new(id, frames.get(at).copied().unwrap_or(0), x, y))
        })
    }

    /// The picture of an object in flight or lying where it was put
    /// down, if it shows one: its own from [`thrown_pictures::SHEET`]
    /// at the object's position a px left, mirrored when it moves left;
    /// in flight without all eight of its pictures, the frame of its
    /// kind's animation that far through the cycle, mirrored the same.
    fn carried_picture(&self, thing: &Thing, pose: &Pose, x: f32, y: f32) -> Option<Sprite> {
        if let Some(count) = pose.thrown {
            let left = matches!(thing.phase, Phase::Thrown(f) if f.vx.is_negative());
            let entry = thrown_pictures::flight(count);
            let own = self.kinds.get(thrown_pictures::SHEET).and_then(|&id| {
                let sheet = self.bank.sheet(id);
                let all = thrown_pictures::FLIGHT
                    .iter()
                    .all(|l| sheet.animation(l).is_some());
                let frame = *sheet
                    .animation(thrown_pictures::FLIGHT[entry])
                    .filter(|_| all)?
                    .frames
                    .first()?;
                Some(Sprite {
                    flip_x: left,
                    ..Sprite::new(id, frame, x - 1.0, y)
                })
            });
            return own.or_else(|| {
                let &id = self.kinds.get(&thing.kind)?;
                let frames = self
                    .bank
                    .sheet(id)
                    .animations
                    .get(pose.animation)
                    .map_or(&[][..], |a| &a.frames[..]);
                let at = entry * frames.len() / thrown_pictures::FLIGHT.len();
                Some(Sprite {
                    flip_x: left,
                    ..Sprite::new(id, frames.get(at).copied().unwrap_or(0), x, y)
                })
            });
        }
        let id = *self.kinds.get(thrown_pictures::SHEET)?;
        if !pose.put_down.is_some_and(thrown_pictures::lying) {
            return None;
        }
        let frame = *self
            .bank
            .sheet(id)
            .animation(thrown_pictures::LYING)?
            .frames
            .first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
    }

    /// A walker's own picture for `pose`, from its kind's sheet, if that
    /// has the picture's whole group. Its origin is a px left of the
    /// walker's position (`formats/sprites.md`, "Where the picture is
    /// drawn").
    fn walker_picture(&self, pose: &Pose, x: f32, y: f32) -> Option<Sprite> {
        let (size, group, label) = pose.walker.shown()?;
        let &id = self.kinds.get(size.name())?;
        let sheet = self.bank.sheet(id);
        let all = walker_pictures::labels(size, group)
            .iter()
            .all(|l| sheet.animation(l).is_some());
        let frame = *sheet.animation(label).filter(|_| all)?.frames.first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
    }

    fn sprite(&self, id: SheetId, pose: &Pose, x: f32, y: f32) -> Sprite {
        let sheet = self.bank.sheet(id);
        let frame = match sheet.animations.get(pose.animation) {
            Some(animation) => pose.playback.frame(animation),
            None => 0,
        };
        Sprite::new(id, frame, x, y)
    }

    /// The index of the animation called `name` in `id`'s sheet, or its
    /// first if there's none by that name.
    fn animation(&self, id: Option<SheetId>, name: Option<&str>) -> usize {
        let Some(id) = id else {
            return 0;
        };
        let animations = &self.bank.sheet(id).animations;
        name.and_then(|name| animations.iter().position(|a| a.name == name))
            .unwrap_or(0)
    }

    /// Whether `pose` is partway through the animation called `name` in
    /// `id`'s sheet.
    fn playing(&self, id: Option<SheetId>, pose: &Pose, name: &str) -> bool {
        id.and_then(|id| self.bank.sheet(id).animations.get(pose.animation))
            .is_some_and(|animation| animation.name == name && !pose.playback.finished())
    }

    /// Moves `pose` on a logic frame, onto `animation` of `id`'s sheet.
    fn advance(&self, pose: &mut Pose, id: Option<SheetId>, animation: usize) {
        if pose.animation != animation {
            *pose = Pose {
                animation,
                playback: Playback::new(),
                ..*pose
            };
            return;
        }
        let Some(id) = id else {
            return;
        };
        let sheet = self.bank.sheet(id);
        if let Some(animation) = sheet.animations.get(animation) {
            pose.playback.advance(animation, &sheet.frames);
        }
    }
}

/// Which animation an actor is playing, and how far through it, and for
/// the player or a walker which of its own pictures it shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pose {
    /// Index into the sheet's animations.
    pub animation: usize,
    pub playback: Playback,
    pub pictures: PictureClock,
    pub walker: WalkerClock,
    /// A spinning collectible's step ([`crate::spin_pictures`]).
    pub spin: Option<u8>,
    /// A bunch's counted frames since it appeared
    /// ([`crate::bunch_pictures`]).
    pub bunch: Option<u32>,
    /// Frames since a thrown object left the hands
    /// ([`crate::thrown_pictures`]).
    pub thrown: Option<u32>,
    /// Frames since an object was put down.
    pub put_down: Option<u32>,
}

/// Every actor's pose: the player's, the other character's, the lost
/// character's, and each thing's by its index in the world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Poses {
    pub player: Pose,
    pub other: Pose,
    pub leaving: Pose,
    pub things: Vec<Pose>,
    /// The latest frame's number plus one, with frames numbered from the
    /// level's start or from play starting again after a lost life.
    pub clock: u32,
}

impl Poses {
    /// Poses for `world` as it starts, each at the start of its animation.
    pub fn new(cast: &Cast, world: &World) -> Poses {
        let mut poses = Poses {
            player: Pose::default(),
            other: Pose::default(),
            leaving: Pose::default(),
            things: vec![Pose::default(); world.things.len()],
            clock: 0,
        };
        poses.player.animation = cast.animation(cast.player, Some(player_animation(world, false)));
        poses.other.animation = cast.animation(cast.player, Some(other_animation(world)));
        poses.leaving.animation = cast.animation(cast.player, Some(leaving_animation(world)));
        for (pose, thing) in poses.things.iter_mut().zip(&world.things) {
            let id = cast.kinds.get(&thing.kind).copied();
            pose.animation = cast.animation(id, thing_animation(world, thing));
            pose.spin = spins(world, thing).then(|| spin_pictures::step(0, thing.pos));
            pose.bunch = (is_bunch(world, thing) && thing.active).then_some(0);
            pose.put_down = matches!(thing.phase, Phase::Placed).then_some(0);
            pose.thrown = thrown(thing).then_some(0);
        }
        poses
    }

    /// Moves every pose on a logic frame, for `world` as it is now.
    /// `jump_in`: frames until a jump starts (`PictureClock::update`).
    pub fn update(&mut self, cast: &Cast, world: &World, jump_in: u8) {
        let respawn = world.flow.events.contains(&FlowEvent::Respawn);
        self.clock = if respawn { 1 } else { self.clock + 1 };
        self.player.pictures.update(world, jump_in);
        let id = cast.player;
        let landing = cast.playing(id, &self.player, LAND);
        let animation = cast.animation(id, Some(player_animation(world, landing)));
        cast.advance(&mut self.player, id, animation);
        let animation = cast.animation(id, Some(other_animation(world)));
        cast.advance(&mut self.other, id, animation);
        let animation = cast.animation(id, Some(leaving_animation(world)));
        cast.advance(&mut self.leaving, id, animation);
        self.things.resize(world.things.len(), Pose::default());
        for (pose, thing) in self.things.iter_mut().zip(&world.things) {
            let id = cast.kinds.get(&thing.kind).copied();
            let animation = cast.animation(id, thing_animation(world, thing));
            cast.advance(pose, id, animation);
            match walker_size(world, thing) {
                Some(size) => pose.walker.update(thing, size),
                None => pose.walker = WalkerClock::default(),
            }
            pose.spin = spins(world, thing).then(|| spin_pictures::step(self.clock, thing.pos));
            // A bunch's cycle starts afresh on the frame it appears and
            // when play starts again, and steps on the frames walkers
            // move on.
            pose.bunch = match pose.bunch {
                _ if !is_bunch(world, thing) || !thing.active => None,
                Some(count) if !respawn => Some(count + u32::from(!world.still)),
                _ => Some(0),
            };
            // A thrown object's frames count from the one it leaves the
            // hands on, and a put-down one's from the one it is set down.
            pose.thrown = thrown(thing).then(|| pose.thrown.map_or(0, |n| n + 1));
            pose.put_down =
                matches!(thing.phase, Phase::Placed).then(|| pose.put_down.map_or(0, |n| n + 1));
        }
    }
}

/// The player's animation after a landing.
const LAND: &str = "land";

/// The player's animation: its state's, `jump` while bouncing, `fall`
/// while swimming (there's no swimming art yet), or `hurt` while losing
/// a life. So that a freeze never looks like the player has stopped
/// responding, it plays `recover` on the ground through the stop after a
/// timed roll (`roll_stop_time`), and `land` from a landing while it
/// stands, until `land` has played through (`landing` says it's still
/// playing). A sheet without these plays its first animation instead.
fn player_animation(world: &World, landing: bool) -> &'static str {
    let player = &world.player;
    match (world.flow.outcome, player.state) {
        (Outcome::Dying(_) | Outcome::GameOver, _) => "hurt",
        (_, State::Bounce) => "jump",
        (_, State::Swim) => "fall",
        (_, State::Idle | State::Walk) if player.roll_stop > 0 => "recover",
        (_, State::Idle) if landing || world.events.contains(&Event::Land) => LAND,
        (_, state) => state.name(),
    }
}

/// The other character's animation: `swap` during a swap, `idle`
/// otherwise.
fn other_animation(world: &World) -> &'static str {
    if world.swap.is_some() {
        State::Swap.name()
    } else {
        State::Idle.name()
    }
}

/// The lost character's animation: its state's.
fn leaving_animation(world: &World) -> &'static str {
    world
        .leaving()
        .map_or(State::Idle.name(), |body| body.state.name())
}

/// The walker kind whose pictures `thing` shows, as its entity's
/// `pictures` param names it.
fn walker_size(world: &World, thing: &Thing) -> Option<Size> {
    Size::named(pictures_param(world, thing)?)
}

/// Whether `thing` is a collectible that spins, as its entity's
/// `pictures` param says ([`crate::spin_pictures`]).
fn spins(world: &World, thing: &Thing) -> bool {
    pictures_param(world, thing) == Some(spin_pictures::SPIN)
}

/// Whether `thing` is a collectible drawn as a bunch, as its entity's
/// `pictures` param says ([`crate::bunch_pictures`]).
fn is_bunch(world: &World, thing: &Thing) -> bool {
    pictures_param(world, thing) == Some(bunch_pictures::BUNCH)
}

/// Whether `thing` is an object thrown (not dropped) and in flight.
fn thrown(thing: &Thing) -> bool {
    thing.active && matches!(thing.phase, Phase::Thrown(f) if f.thrown)
}

/// `thing`'s entity's `pictures` param, if it has one.
fn pictures_param<'w>(world: &'w World, thing: &Thing) -> Option<&'w str> {
    world
        .level
        .entities
        .get(thing.index)?
        .params
        .get(PICTURES)?
        .as_str()
}

/// The entity param naming the pictures a walker
/// ([`crate::walker_pictures`]) or a collectible
/// ([`crate::spin_pictures`], [`crate::bunch_pictures`]) shows.
pub const PICTURES: &str = "pictures";

/// A thing's animation, if not its sheet's first.
fn thing_animation(world: &World, thing: &Thing) -> Option<&'static str> {
    match thing.phase {
        Phase::Defeated(_) | Phase::Falling { .. } | Phase::Knocked { .. } => {
            return Some("defeated");
        }
        Phase::Fired(_) => return Some("fire"),
        Phase::Opened(since) => {
            let out = thing.archetype.is_some_and(|a| match a.behavior {
                Behavior::Hutch(hutch) => since >= hutch.animal_after,
                _ => true,
            });
            return Some(if out { "animal" } else { "break" });
        }
        Phase::Mounted(_) | Phase::Ridden | Phase::Bolting { .. } | Phase::Waiting(_) => {
            return Some("animal");
        }
        Phase::Squashed { .. } | Phase::Wobbling { .. } => {
            // A spring is drawn squashed while its top is below where
            // it rests.
            let low = thing.archetype.is_some_and(|a| match a.behavior {
                Behavior::Spring(spring) => spring_top(&spring, thing.phase) < spring.top,
                _ => false,
            });
            return low.then_some("squash");
        }
        Phase::Ready
        | Phase::Held
        | Phase::Thrown(_)
        | Phase::LetGo { .. }
        | Phase::Placed
        | Phase::Flying { .. }
        | Phase::Swaying { .. }
        | Phase::Flitting { .. }
        | Phase::Rope { .. }
        | Phase::Caught(_)
        | Phase::Broken
        | Phase::Item { .. }
        | Phase::Landed(_)
        | Phase::Gone
        | Phase::Open => {}
    }
    let reached = thing.kind == CHECKPOINT && world.flow.checkpoints.contains(&thing.index);
    reached.then_some("reached")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_level::Level;
    use clamber_sim::{State, generic_feel};
    use clamber_sprite::{Image, Palette, Pixels, Sheet};

    use super::*;
    use crate::canvas::Canvas;
    use crate::canvas::{Color, rgb};

    /// Two animations of 1x1 frames: `idle` shows red then green, two
    /// ticks each; `walk` shows blue. Index 1 is off-centre so mirroring
    /// shows. `alt` turns red white.
    const SHEET: &str = r##"{
        "frames": [
            { "frame": { "x": 0, "y": 0, "w": 2, "h": 1 }, "ticks": 2 },
            { "frame": { "x": 2, "y": 0, "w": 2, "h": 1 }, "ticks": 2 },
            { "frame": { "x": 4, "y": 0, "w": 2, "h": 1 }, "ticks": 2 }
        ],
        "meta": {
            "image": "test.png",
            "frameTags": [
                { "name": "idle", "from": 0, "to": 1, "direction": "forward" },
                { "name": "walk", "from": 2, "to": 2, "direction": "forward" }
            ],
            "clamber": {
                "pixelArt": true,
                "palettes": { "alt": ["#00000000", "#ffffff", "#00ff00", "#0000ff"] }
            }
        }
    }"##;

    const R: Color = rgb(255, 0, 0);
    const G: Color = rgb(0, 255, 0);
    const B: Color = rgb(0, 0, 255);
    const W: Color = rgb(255, 255, 255);

    fn sheet() -> SpriteSheet {
        let sheet = Sheet::from_json_str(SHEET, "test").unwrap();
        let mut indices = Pixels::new(6, 1);
        indices.data = vec![1, 0, 2, 0, 3, 0];
        let palette = Palette(vec![
            [0, 0, 0, 0],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
        ]);
        SpriteSheet::new(sheet, Image::Indexed { indices, palette }).unwrap()
    }

    const LEVEL: &str = r#"
        format = 1
        size = [100, 100]
        start = [50, 50]

        [[collision]]
        shape = "rect"
        rect = [0, 50, 100, 50]

        [[entities]]
        type = "checkpoint"
        at = [20, 50]

        [[entities]]
        type = "unknown"
        at = [80, 50]
    "#;

    fn world() -> World {
        let level = Level::from_toml_str(LEVEL, "test").unwrap();
        World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
    }

    fn cast() -> Cast {
        Cast::new(vec![
            (PLAYER.to_string(), sheet()),
            (CHECKPOINT.to_string(), sheet()),
        ])
    }

    /// The two pixels right of `(x, y)` after drawing the player there.
    fn player_pixels(cast: &Cast, world: &World, pose: &Pose) -> [Color; 2] {
        let mut canvas = Canvas::new(4, 1);
        cast.draw_player(&mut canvas, world, pose, 1.0, 0.0);
        [canvas.get(1, 0).unwrap(), canvas.get(2, 0).unwrap()]
    }

    #[test]
    fn plays_the_animation_for_the_state() {
        let (cast, mut world) = (cast(), world());
        let mut poses = Poses::new(&cast, &world);
        let mut shown = Vec::new();
        for _ in 0..5 {
            shown.push(player_pixels(&cast, &world, &poses.player)[0]);
            poses.update(&cast, &world, 0);
        }
        assert_eq!(shown, [R, R, G, G, R]);
        world.player.state = State::Walk;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.player.playback, Playback::new(), "starts over");
        assert_eq!(player_pixels(&cast, &world, &poses.player)[0], B);
        // No `jump` animation: the sheet's first plays instead.
        world.player.state = State::Jump;
        poses.update(&cast, &world, 0);
        assert_eq!(player_pixels(&cast, &world, &poses.player)[0], R);
    }

    #[test]
    fn faces_and_recolours_the_player() {
        let (cast, mut world) = (cast(), world());
        let pose = Poses::new(&cast, &world).player;
        assert_eq!(player_pixels(&cast, &world, &pose), [R, 0]);
        world.player.facing = Facing::Left;
        let mut canvas = Canvas::new(4, 1);
        cast.draw_player(&mut canvas, &world, &pose, 2.0, 0.0);
        assert_eq!(canvas.pixels, [0, R, 0, 0], "mirrored about the origin");
        world.player.facing = Facing::Right;
        world.player.character = 1;
        assert_eq!(player_pixels(&cast, &world, &pose), [W, 0]);
        world.player.character = 2;
        assert_eq!(
            player_pixels(&cast, &world, &pose),
            [R, 0],
            "no third palette"
        );
    }

    #[test]
    fn things_play_their_kind_and_progress() {
        let (cast, mut world) = (cast(), world());
        assert!(cast.has_kind(CHECKPOINT));
        assert!(!cast.has_kind("unknown"));
        let mut poses = Poses::new(&cast, &world);
        assert_eq!(poses.things.len(), 2);
        assert_eq!(poses.things[0].animation, 0);
        // The test sheet has no `reached`, so pretend `walk` is it.
        let mut renamed = sheet();
        renamed.sheet.animations[1].name = "reached".into();
        let cast = Cast::new(vec![(CHECKPOINT.to_string(), renamed)]);
        world.flow.checkpoints.push(0);
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].animation, 1);
        let mut canvas = Canvas::new(2, 1);
        let thing = world.things[0].clone();
        cast.draw_thing(&mut canvas, &thing, &poses.things[0], 0.0, 0.0);
        assert_eq!(canvas.pixels[0], B);
        // Without a sheet, nothing's drawn.
        let mut canvas = Canvas::new(2, 1);
        cast.draw_thing(&mut canvas, &world.things[1], &poses.things[1], 0.0, 0.0);
        assert_eq!(canvas.pixels, [0, 0]);
    }

    #[test]
    fn hurts_while_losing_a_life() {
        let mut sheet = sheet();
        sheet.sheet.animations[1].name = "hurt".into();
        let cast = Cast::new(vec![(PLAYER.to_string(), sheet)]);
        let mut world = world();
        let mut poses = Poses::new(&cast, &world);
        world.flow.outcome = Outcome::Dying(10);
        poses.update(&cast, &world, 0);
        assert_eq!(player_pixels(&cast, &world, &poses.player)[0], B);
    }

    /// The test cast with its `walk` animation renamed `name`.
    fn cast_with(name: &str) -> Cast {
        let mut sheet = sheet();
        sheet.sheet.animations[1].name = name.into();
        Cast::new(vec![(PLAYER.to_string(), sheet)])
    }

    #[test]
    fn recovers_through_the_stop_after_a_roll() {
        let recovering = cast_with("recover");
        let mut world = world();
        let mut poses = Poses::new(&recovering, &world);
        world.player.roll_stop = 1;
        poses.update(&recovering, &world, 0);
        assert_eq!(player_pixels(&recovering, &world, &poses.player)[0], B);
        // Sliding in the stop too.
        world.player.state = State::Walk;
        poses.update(&recovering, &world, 0);
        assert_eq!(player_pixels(&recovering, &world, &poses.player)[0], B);
        world.player.roll_stop = 0;
        poses.update(&recovering, &world, 0);
        assert_eq!(player_pixels(&recovering, &world, &poses.player)[0], R);
        // Not in the air, nor without the art.
        world.player.roll_stop = 1;
        world.player.state = State::Fall;
        assert_eq!(player_animation(&world, false), "fall");
        world.player.state = State::Idle;
        let mut poses = Poses::new(&cast(), &world);
        poses.update(&cast(), &world, 0);
        assert_eq!(poses.player.animation, 0, "the first animation");
    }

    #[test]
    fn lands_while_standing_until_it_has_played() {
        let mut sheet = sheet();
        sheet.sheet.animations[1].name = LAND.into();
        sheet.sheet.animations[1].repeat = Some(1);
        let cast = Cast::new(vec![(PLAYER.to_string(), sheet)]);
        let mut world = world();
        let mut poses = Poses::new(&cast, &world);
        world.events.push(Event::Land);
        poses.update(&cast, &world, 0);
        world.events.clear();
        let mut shown = Vec::new();
        for _ in 0..4 {
            shown.push(player_pixels(&cast, &world, &poses.player)[0]);
            poses.update(&cast, &world, 0);
        }
        assert_eq!(
            shown,
            [B, B, B, R],
            "`land` through its last frame, then idle"
        );
        // Walking off cuts it short.
        world.events.push(Event::Land);
        poses.update(&cast, &world, 0);
        world.events.clear();
        world.player.state = State::Walk;
        poses.update(&cast, &world, 0);
        assert_eq!(player_animation(&world, true), "walk");
    }

    #[test]
    fn loads_the_bundled_sheets() {
        let cast = Cast::load(&crate::content_dir().join(crate::SPRITES)).unwrap();
        assert!(cast.has_player());
        assert_eq!(cast.alternates.len(), 1);
        let player = cast.bank.sheet(cast.player.unwrap());
        for state in [
            State::Idle,
            State::Walk,
            State::Run,
            State::Roll,
            State::Jump,
            State::Fall,
            State::Swap,
        ] {
            assert!(player.animation(state.name()).is_some(), "{state:?}");
        }
        assert!(player.animation("hurt").is_some());
        for kind in [
            clamber_sim::GOAL,
            CHECKPOINT,
            clamber_sim::GEM,
            clamber_sim::RELIC,
            clamber_sim::TONIC,
        ] {
            assert!(cast.has_kind(kind), "{kind}");
        }
        let checkpoint = cast.bank.sheet(cast.kinds[CHECKPOINT]);
        assert!(checkpoint.animation("reached").is_some());
    }

    #[test]
    fn a_later_folder_s_sheets_replace_and_add_to_the_earlier_s() {
        let own = crate::content_dir().join(crate::SPRITES);
        let pack = std::env::temp_dir().join(format!("clamber-cast-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pack);
        std::fs::create_dir_all(&pack).unwrap();
        // The pack draws checkpoints as crates, and has a kind of its own.
        for (from, to) in [("crate", CHECKPOINT), ("crate", "made-up")] {
            let json = std::fs::read_to_string(own.join(format!("{from}.json"))).unwrap();
            std::fs::write(pack.join(format!("{to}.json")), json).unwrap();
            for suffix in [".png", "-normals.png"] {
                let image = own.join(format!("{from}{suffix}"));
                if image.exists() {
                    std::fs::copy(image, pack.join(format!("{from}{suffix}"))).unwrap();
                }
            }
        }
        let alone = Cast::load(&own).unwrap();
        let layered = Cast::load_layered(&[&own, &pack]).unwrap();
        std::fs::remove_dir_all(&pack).unwrap();
        assert!(layered.has_player());
        assert!(layered.has_kind("made-up"));
        assert!(!alone.has_kind("made-up"));
        assert_eq!(layered.kinds.len(), alone.kinds.len() + 1);
        let checkpoint = layered.bank.sheet(layered.kinds[CHECKPOINT]);
        let crate_sheet = alone.bank.sheet(alone.kinds["crate"]);
        assert_eq!(checkpoint.animations, crate_sheet.animations);
        assert!(checkpoint.animation("reached").is_none());
    }

    /// A character sheet of 1x1 frames like [`sheet`]'s, with an
    /// animation for each of `labels`.
    fn character_sheet(labels: &[&str]) -> SpriteSheet {
        labelled_sheet(labels, 0)
    }

    /// A sheet whose `labels` all show the 2-px frame `x` px into
    /// [`sheet`]'s image.
    fn labelled_sheet(labels: &[&str], x: u32) -> SpriteSheet {
        let tags: Vec<String> = labels
            .iter()
            .map(|l| format!(r#"{{ "name": "{l}", "from": 0, "to": 0, "direction": "forward" }}"#))
            .collect();
        let json = format!(
            r#"{{
                "frames": [ {{ "frame": {{ "x": {x}, "y": 0, "w": 2, "h": 1 }}, "ticks": 1 }} ],
                "meta": {{ "image": "c.png", "frameTags": [{}], "clamber": {{ "pixelArt": true }} }}
            }}"#,
            tags.join(",")
        );
        let SpriteSheet { image, .. } = sheet();
        SpriteSheet::new(Sheet::from_json_str(&json, "c").unwrap(), image).unwrap()
    }

    #[test]
    fn the_player_shows_its_own_picture_a_px_left_when_the_whole_set_is_there() {
        let world = world();
        let pose = Pose {
            pictures: PictureClock::showing(crate::pictures::Set::Bounce, "bounce-1"),
            ..Pose::default()
        };
        let set = crate::pictures::labels(0, crate::pictures::Set::Bounce);
        for (labels, want) in [
            (set, [R, Color::default()]),
            // Without its last picture the set isn't there: the engine's art.
            (&set[..set.len() - 1], [Color::default(), R]),
        ] {
            let cast = Cast::new(vec![
                (PLAYER.to_string(), sheet()),
                (format!("{CHARACTER}0"), character_sheet(labels)),
            ]);
            assert!(cast.has_pictures());
            assert!(!cast.has_kind("character-0"));
            let mut canvas = Canvas::new(4, 1);
            cast.draw_player(&mut canvas, &world, &pose, 2.0, 0.0);
            let got = [canvas.get(1, 0).unwrap(), canvas.get(2, 0).unwrap()];
            assert_eq!(got, want, "{labels:?}");
        }
    }

    #[test]
    fn a_walker_shows_its_kinds_picture_a_px_left_when_the_whole_group_is_there() {
        let level = LEVEL.to_string()
            + r#"
            [[entities]]
            type = "crawler"
            at = [30, 50]
            params = { pictures = "small-walker" }
        "#;
        let level = Level::from_toml_str(&level, "test").unwrap();
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let walking =
            crate::walker_pictures::labels(Size::Small, crate::walker_pictures::Group::Walking);
        for (labels, want) in [
            (walking, [R, Color::default()]),
            // Without its last picture the group isn't there: the
            // engine's art.
            (&walking[..walking.len() - 1], [Color::default(), R]),
        ] {
            let cast = Cast::new(vec![
                (PLAYER.to_string(), sheet()),
                ("crawler".to_string(), sheet()),
                ("small-walker".to_string(), character_sheet(labels)),
            ]);
            let mut poses = Poses::new(&cast, &world);
            poses.update(&cast, &world, 0);
            let thing = world.things.iter().find(|t| t.kind == "crawler").unwrap();
            let pose = poses.things[thing.index];
            assert_eq!(pose.walker.shown().unwrap().2, "small-walk-1");
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, thing, &pose, 2.0, 0.0);
            // Facing left, as placed, the 2-px frame is mirrored about
            // its origin, so its first pixel is left of it.
            let got = [canvas.get(0, 0).unwrap(), canvas.get(1, 0).unwrap()];
            assert_eq!(got, want, "{labels:?}");
        }
    }

    #[test]
    fn a_thrower_shows_its_pictures_with_its_timetable() {
        let level = LEVEL.to_string()
            + r#"
            [[entities]]
            type = "thrower"
            at = [50, 20]
        "#;
        let level = Level::from_toml_str(&level, "test").unwrap();
        let mut world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let thrower = |world: &World| {
            let thing = world.things.iter().find(|t| t.kind == "thrower").unwrap();
            thrower_pictures::shown(thing).unwrap().label
        };
        let balls = |world: &World| {
            world
                .things
                .iter()
                .filter(|t| t.active && t.kind == clamber_sim::BALL)
                .count()
        };
        let mut seen = Vec::new();
        for _ in 0..4 {
            clamber_sim::step(&mut world, &clamber_sim::InputFrame::default());
            seen.push((thrower(&world), balls(&world)));
        }
        // It appears on the first step, and throws up on the third.
        assert_eq!(
            seen,
            [
                ("throw-1", 0),
                ("throw-2", 0),
                ("throw-3", 1),
                ("throw-3", 1)
            ]
        );
        let ball = world
            .things
            .iter()
            .find(|t| t.kind == clamber_sim::BALL)
            .unwrap();
        assert_eq!(thrower_pictures::shown(ball).unwrap().label, "ball-1");
    }

    #[test]
    fn a_thrower_shows_its_picture_a_px_left_unmirrored_when_the_whole_group_is_there() {
        let level = LEVEL.to_string()
            + r#"
            [[entities]]
            type = "thrower"
            at = [50, 20]
        "#;
        let level = Level::from_toml_str(&level, "test").unwrap();
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let mut thing = world
            .things
            .iter()
            .find(|t| t.kind == "thrower")
            .unwrap()
            .clone();
        thing.facing = Facing::Left;
        let group = thrower_pictures::labels(Group::Thrower);
        for (labels, want) in [
            (group, [Color::default(), G, Color::default()]),
            // Without its last picture the group isn't there: the
            // engine's art, mirrored to face left.
            (
                &group[..group.len() - 1],
                [Color::default(), R, Color::default()],
            ),
        ] {
            let cast = Cast::new(vec![
                ("thrower".to_string(), sheet()),
                (
                    thrower_pictures::THROWER.to_string(),
                    labelled_sheet(labels, 2),
                ),
            ]);
            let mut canvas = Canvas::new(3, 1);
            cast.draw_thing(&mut canvas, &thing, &Pose::default(), 2.0, 0.0);
            let got = [0, 1, 2].map(|x| canvas.get(x, 0).unwrap());
            assert_eq!(got, want, "{labels:?}");
        }
    }

    #[test]
    fn a_ball_is_mirrored_as_its_turn_says_with_the_engines_art_too() {
        let mut thing = world().things[0].clone();
        thing.kind = clamber_sim::BALL.to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named(clamber_sim::BALL);
        thing.facing = Facing::Right;
        // Steps 15 to 17: `ball-2`, mirrored both ways.
        thing.clock = 15;
        let group = thrower_pictures::labels(Group::Ball);
        for (sheets, want) in [
            (vec![(thrower_pictures::BALL, labelled_sheet(group, 2))], G),
            (vec![(clamber_sim::BALL, sheet())], R),
        ] {
            let cast = Cast::new(
                sheets
                    .into_iter()
                    .map(|(n, s)| (n.to_string(), s))
                    .collect(),
            );
            let mut canvas = Canvas::new(4, 4);
            cast.draw_thing(&mut canvas, &thing, &Pose::default(), 2.0, 2.0);
            // Mirrored about its origin both ways, its first pixel is up
            // and left of it.
            let lit: Vec<_> = (0..4)
                .flat_map(|y| (0..4).map(move |x| (x, y)))
                .filter(|&(x, y)| canvas.get(x, y).unwrap() != Color::default())
                .collect();
            let origin = if want == G { 1 } else { 2 };
            assert_eq!(lit, [(origin - 1, 1)], "{want:?}");
            assert_eq!(canvas.get(origin - 1, 1).unwrap(), want);
        }
    }

    /// A level with a spinning collectible at (112, 65), whose steps
    /// begin at 5 (collectibles.md, "Example (synthetic)").
    fn spinning() -> World {
        let level = LEVEL.to_string()
            + r#"
            [[entities]]
            type = "spark"
            at = [112, 65]
            params = { pictures = "spin" }
        "#;
        let level = Level::from_toml_str(&level, "test").unwrap();
        World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
    }

    #[test]
    fn a_spinning_collectible_counts_its_steps_from_the_latest_frame_0() {
        let mut world = spinning();
        let cast = Cast::new(vec![("spark".to_string(), sheet())]);
        let index = world.things.iter().position(|t| t.kind == "spark").unwrap();
        let mut poses = Poses::new(&cast, &world);
        let mut steps = Vec::new();
        for _ in 0..8 {
            poses.update(&cast, &world, 0);
            steps.push(poses.things[index].spin.unwrap());
        }
        assert_eq!(poses.clock, 8);
        assert_eq!(steps, [5, 5, 5, 6, 6, 6, 6, 7]);
        // Play starting again is frame 0 once more.
        world.flow.events.push(FlowEvent::Respawn);
        poses.update(&cast, &world, 0);
        assert_eq!(poses.clock, 1);
        assert_eq!(poses.things[index].spin, Some(5));
        // Other things don't spin.
        assert!(poses.things.iter().filter(|p| p.spin.is_some()).count() == 1);
    }

    #[test]
    fn a_spinning_collectible_shows_its_own_pictures_mirrored_while_all_five_are_there() {
        let world = spinning();
        let thing = world.things.iter().find(|t| t.kind == "spark").unwrap();
        let labels = &crate::spin_pictures::LABELS[..];
        for (labels, step, want) in [
            // Its own picture, mirrored about its origin.
            (labels, 5, [B, Color::default()]),
            // Step 6 has no picture of its own: the engine's art, as far
            // through its animation as the step is through the turn.
            (labels, 6, [Color::default(), G]),
            // Without one of the five, none are shown.
            (&labels[1..], 1, [Color::default(), R]),
            (&labels[1..], 5, [Color::default(), G]),
        ] {
            let cast = Cast::new(vec![
                ("spark".to_string(), sheet()),
                (
                    crate::spin_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let pose = Pose {
                spin: Some(step),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, thing, &pose, 2.0, 0.0);
            let got = [canvas.get(1, 0).unwrap(), canvas.get(2, 0).unwrap()];
            assert_eq!(got, want, "{labels:?} on {step}");
        }
    }

    /// A level with a bunch at (30, 40).
    fn bunched() -> World {
        let level = LEVEL.to_string()
            + r#"
            [[entities]]
            type = "cluster"
            at = [30, 40]
            params = { pictures = "bunch" }
        "#;
        let level = Level::from_toml_str(&level, "test").unwrap();
        World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
    }

    #[test]
    fn a_bunch_counts_the_frames_walkers_move_on_from_when_it_appears() {
        let mut world = bunched();
        let cast = Cast::new(vec![("cluster".to_string(), sheet())]);
        let index = world
            .things
            .iter()
            .position(|t| t.kind == "cluster")
            .unwrap();
        let mut poses = Poses::new(&cast, &world);
        assert_eq!(poses.things[index].bunch, Some(0));
        let mut counts = Vec::new();
        for still in [false, false, true, true, false] {
            world.still = still;
            poses.update(&cast, &world, 0);
            counts.push(poses.things[index].bunch.unwrap());
        }
        assert_eq!(counts, [1, 2, 2, 2, 3]);
        // Play starting again starts its cycle afresh.
        world.still = false;
        world.flow.events.push(FlowEvent::Respawn);
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[index].bunch, Some(0));
        world.flow.events.clear();
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[index].bunch, Some(1));
        // Gone, it shows nothing; appearing again, it starts afresh.
        world.things[index].active = false;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[index].bunch, None);
        world.things[index].active = true;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[index].bunch, Some(0));
        // Other things aren't bunches.
        assert_eq!(poses.things.iter().filter(|p| p.bunch.is_some()).count(), 1);
    }

    #[test]
    fn a_bunch_shows_its_own_pictures_unmirrored_while_all_six_are_there() {
        let world = bunched();
        let thing = world.things.iter().find(|t| t.kind == "cluster").unwrap();
        let labels = &crate::bunch_pictures::LABELS[..];
        for (labels, count, want) in [
            // Its own picture, at its origin.
            (labels, 0, [B, Color::default()]),
            (labels, 29, [B, Color::default()]),
            // Without one of the six, the engine's art, as far through
            // its animation as the entry is through the cycle.
            (&labels[1..], 0, [R, Color::default()]),
            (&labels[1..], 28, [R, Color::default()]),
            (&labels[1..], 29, [G, Color::default()]),
        ] {
            let cast = Cast::new(vec![
                ("cluster".to_string(), sheet()),
                (
                    crate::bunch_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let pose = Pose {
                bunch: Some(count),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, thing, &pose, 1.0, 0.0);
            let got = [canvas.get(1, 0).unwrap(), canvas.get(2, 0).unwrap()];
            assert_eq!(got, want, "{labels:?} on {count}");
        }
    }

    fn thrown_thing(vx: i32) -> Thing {
        let mut thing = world().things[0].clone();
        thing.phase = Phase::Thrown(clamber_sim::Flight {
            vx: clamber_fixed::Fixed::from_int(vx),
            vy: clamber_fixed::Fixed::ZERO,
            target: clamber_fixed::Fixed::ZERO,
            response: clamber_fixed::Fixed::ZERO,
            gravity: clamber_fixed::Fixed::ZERO,
            max_fall: clamber_fixed::Fixed::ZERO,
            thrown: true,
            margin: clamber_fixed::Fixed::ZERO,
            bounce_min: clamber_fixed::Fixed::ZERO,
            bounce: clamber_fixed::Fixed::ZERO,
            rolling: false,
            age: 0,
        });
        thing
    }

    #[test]
    fn a_thrown_object_counts_its_flight_and_its_time_lying() {
        let mut world = world();
        let cast = Cast::new(vec![]);
        let mut poses = Poses::new(&cast, &world);
        let flight = thrown_thing(2).phase;
        world.things[0].phase = flight;
        let mut seen = Vec::new();
        for _ in 0..3 {
            poses.update(&cast, &world, 0);
            seen.push(poses.things[0].thrown);
        }
        assert_eq!(seen, [Some(0), Some(1), Some(2)]);
        // Picked up and thrown again, the cycle starts afresh.
        world.things[0].phase = Phase::Held;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].thrown, None);
        world.things[0].phase = flight;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].thrown, Some(0));
        world.things[0].phase = Phase::Placed;
        poses.update(&cast, &world, 0);
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].thrown, None);
        assert_eq!(poses.things[0].put_down, Some(1));
    }

    #[test]
    fn a_thrown_object_shows_its_own_pictures_mirrored_when_it_moves_left() {
        let labels = &thrown_pictures::FLIGHT[..];
        let kind = world().things[0].kind.clone();
        let none = Color::default();
        for (vx, labels, count, want) in [
            // Its own picture a px left of its position; mirrored about
            // that when it moves left.
            (2, labels, 0, [none, B, none, none]),
            (-2, labels, 0, [B, none, none, none]),
            // Without one of the eight, the engine's art, as far
            // through its animation as the entry is through the cycle.
            (2, &labels[1..], 0, [none, none, R, none]),
            (2, &labels[1..], 11, [none, none, G, none]),
            (-2, &labels[1..], 11, [none, G, none, none]),
        ] {
            let cast = Cast::new(vec![
                (kind.clone(), sheet()),
                (
                    thrown_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let pose = Pose {
                thrown: Some(count),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thrown_thing(vx), &pose, 2.0, 0.0);
            let got = [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap());
            assert_eq!(got, want, "{vx} {count} {}", labels.len());
        }
    }

    #[test]
    fn an_object_put_down_shows_lying_from_the_frame_after_it_is_set_down() {
        let kind = world().things[0].kind.clone();
        let none = Color::default();
        let cast = Cast::new(vec![
            (kind.clone(), sheet()),
            (
                thrown_pictures::SHEET.to_string(),
                labelled_sheet(&[thrown_pictures::LYING], 4),
            ),
        ]);
        let mut thing = world().things[0].clone();
        thing.phase = Phase::Placed;
        thing.facing = Facing::Left;
        for (count, want) in [(0, [none, none, R, none]), (1, [none, B, none, none])] {
            let pose = Pose {
                put_down: Some(count),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &pose, 2.0, 0.0);
            let got = [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap());
            // Not mirrored, whichever way it faces.
            let want = if count == 0 {
                [none, R, none, none]
            } else {
                want
            };
            assert_eq!(got, want, "{count}");
        }
    }
}
