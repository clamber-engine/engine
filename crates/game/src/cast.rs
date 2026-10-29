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

use clamber_level::Vec2;
use clamber_sim::{
    Behavior, CHECKPOINT, Event, Facing, FlowEvent, Outcome, Phase, State, Thing, World, spring_top,
};
use clamber_sprite::{Playback, SpriteSheet};

use crate::bunch_pictures;
use crate::critter_pictures;
use crate::hidden_pictures;
use crate::hutch_pictures;
use crate::item_pictures;
use crate::letter_pictures;
use crate::metal_pictures;
use crate::pictures::{self, PictureClock};
use crate::ride_pictures;
use crate::sign_pictures;
use crate::sling_pictures;
use crate::sparkles::Sparkles;
use crate::spin_pictures;
use crate::sprites::{PaletteId, SheetId, Sprite, SpriteBank, SpriteBankBuilder};
use crate::stinger_pictures;
use crate::surface::Surface;
use crate::thrower_pictures::{self, Group};
use crate::thrown_pictures;
use crate::tyre_pictures;
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

    /// Draws the colours that dim in those of the dimming's `step`.
    pub fn set_dim(&self, step: u32) {
        self.bank.set_dim(step);
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
        // Inside a launch barrel nothing of the character is drawn
        // (player-frames.md, "In a launch barrel").
        if player
            .sling
            .is_some_and(|s| (1..=u16::from(s.values.hold)).contains(&s.since))
        {
            return;
        }
        // The mounting pictures of the animal's place are the character
        // itself (animal-crate.md, "While mounting").
        if world.things.iter().any(|t| {
            is_hutch(t)
                && matches!(t.phase, Phase::Mounted(since)
                    if hutch_pictures::mounting(u32::from(since)).is_some())
        }) && self.has_group(hutch_pictures::SHEET, &hutch_pictures::MOUNTING)
        {
            return;
        }
        if let Some(shown) = pose.ride.shown()
            && ride_pictures::rider_group(shown.rider)
                .is_some_and(|group| self.has_group(hutch_pictures::SHEET, group))
            && let Some(sprite) = self.ride_sprite(shown.rider, shown, (x, y))
        {
            self.bank.draw(canvas, &sprite);
            return;
        }
        if let Some(sprite) = self.picture(player.character, pose, x, y) {
            let sprite = Sprite {
                flip_x: player.facing == Facing::Left,
                ..sprite
            };
            self.bank.draw(canvas, &sprite);
            return;
        }
        if let Some(sprite) = self.stand_in(player.character, player.state, x, y) {
            let sprite = Sprite {
                flip_x: player.facing == Facing::Left,
                ..sprite
            };
            self.bank.draw(canvas, &sprite);
            return;
        }
        self.draw_character(canvas, player.character, player.facing, pose, x, y);
    }

    /// The nearest picture of the character in roster slot `slot` to what
    /// it does in `state`, from its own sheet, when it has none for this
    /// frame ([`pictures::stand_in`]). None if the pack has no pictures
    /// of that character at all.
    fn stand_in(&self, slot: usize, state: State, x: f32, y: f32) -> Option<Sprite> {
        let &id = self.characters.get(&slot)?;
        let sheet = self.bank.sheet(id);
        let label = pictures::stand_in(slot, state, |l| sheet.animation(l).is_some())?;
        let frame = *sheet.animation(label)?.frames.first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
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

    /// Draws the lead's picture on the level map, `n` of 0 to 9
    /// (`map-lead-1` to `map-lead-10`), with its origin at `(x, y)`
    /// (`level-map.md`, "Pictures"). False, with nothing drawn, unless
    /// the pack's [`pictures::MAP_SHEET`] has all the map's pictures.
    pub fn draw_map_lead(
        &self,
        canvas: &mut dyn Surface,
        n: usize,
        flip_x: bool,
        at: (i32, i32),
    ) -> bool {
        self.draw_map_picture(canvas, &pictures::MAP, n % 10, flip_x, at)
    }

    /// Draws the other character's picture on the level map, `n` of 0 to
    /// 7 (`map-other-1` to `map-other-8`), as [`Cast::draw_map_lead`].
    pub fn draw_map_other(
        &self,
        canvas: &mut dyn Surface,
        n: usize,
        flip_x: bool,
        at: (i32, i32),
    ) -> bool {
        self.draw_map_picture(canvas, &pictures::MAP, 10 + n % 8, flip_x, at)
    }

    /// Draws picture `n` of `set` (one of the map's other sets: the
    /// finish pictures of either character, or the start's) on the level
    /// map, as [`Cast::draw_map_lead`], needing the whole set in the sheet.
    pub fn draw_map_set(
        &self,
        canvas: &mut dyn Surface,
        set: &[&str],
        n: usize,
        at: (i32, i32),
    ) -> bool {
        self.draw_map_picture(canvas, set, n % set.len(), false, at)
    }

    fn draw_map_picture(
        &self,
        canvas: &mut dyn Surface,
        set: &[&str],
        which: usize,
        flip_x: bool,
        at: (i32, i32),
    ) -> bool {
        let Some(&id) = self.kinds.get(pictures::MAP_SHEET) else {
            return false;
        };
        let sheet = self.bank.sheet(id);
        if !set.iter().all(|l| sheet.animation(l).is_some()) {
            return false;
        }
        let Some(&frame) = sheet.animation(set[which]).and_then(|a| a.frames.first()) else {
            return false;
        };
        let sprite = Sprite {
            flip_x,
            ..Sprite::new(id, frame, at.0 as f32, at.1 as f32)
        };
        self.bank.draw(canvas, &sprite);
        true
    }

    /// Draws the other character in `pose` with its feet at `(x, y)`, if
    /// there is one.
    pub fn draw_other(
        &self,
        canvas: &mut dyn Surface,
        world: &World,
        pose: &Pose,
        state: State,
        x: f32,
        y: f32,
    ) {
        if let Some(other) = &world.other {
            // The spec gives the follower's pictures only in a scene
            // (`following.md`, "Not covered"); elsewhere the nearest
            // picture its own sheet has to what it does stands in.
            let sprite = self
                .picture(other.character, pose, x, y)
                .or_else(|| self.stand_in(other.character, state, x, y));
            if let Some(sprite) = sprite {
                let sprite = Sprite {
                    flip_x: other.facing == Facing::Left,
                    ..sprite
                };
                self.bank.draw(canvas, &sprite);
                return;
            }
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
        if let Some(shown) = self.hutch_picture(thing, pose, x, y) {
            if let Some(sprite) = shown {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        if let Some(mirrored) = hidden_pictures::shown(thing) {
            if let Some(sprite) = self.patch_picture(mirrored, x, y) {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        if let Some(sprite) = self.tyre_picture(thing, x, y) {
            self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            return;
        }
        if let Some(sprite) = self.sign_picture(thing, x, y) {
            self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            return;
        }
        if let Some(sprite) = self.barrel_picture(thing, pose, x, y) {
            self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            return;
        }
        if let Some(clock) = pose.critter {
            if let Some(sprite) = self.critter_picture(thing, pose, clock, x, y) {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        if let Some(count) = pose.stinger {
            if let Some(sprite) = self.stinger_picture(thing, pose, count, x, y) {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        if let Some(count) = pose.item {
            if let Some(sprite) = self.item_picture(thing, pose, count, x, y) {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        if let Some(count) = pose.bunch {
            if let Some(sprite) = self.bunch_picture(thing, pose, count, x, y) {
                self.bank.draw(canvas, &Sprite { opacity, ..sprite });
            }
            return;
        }
        if let Some(sprite) = self.letter_picture(thing, x, y) {
            self.bank.draw(canvas, &Sprite { opacity, ..sprite });
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

    /// A letter's own picture, from [`letter_pictures::SHEET`], if that
    /// has all four. Its origin is a px left of the letter's position.
    fn letter_picture(&self, thing: &Thing, x: f32, y: f32) -> Option<Sprite> {
        let label = letter_pictures::shown(thing)?;
        let &id = self.kinds.get(letter_pictures::SHEET)?;
        let sheet = self.bank.sheet(id);
        if !letter_pictures::LABELS
            .iter()
            .all(|l| sheet.animation(l).is_some())
        {
            return None;
        }
        let frame = *sheet.animation(label)?.frames.first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
    }

    /// Draws the count display's image with its top-left corner at `at`
    /// (count-display.md, "Pictures"): the spinning collectible's
    /// picture on the frame with count `clock`, unmirrored, with its
    /// origin 8 px right of the corner and 9 below. Returns false,
    /// drawing nothing, when the step has no picture of its own or the
    /// sheet doesn't have all five.
    pub fn draw_count_image(&self, canvas: &mut dyn Surface, clock: u32, at: (i32, i32)) -> bool {
        let step = ((clock / 4) % spin_pictures::STEPS + 1) as u8;
        let Some(&id) = self.kinds.get(spin_pictures::SHEET) else {
            return false;
        };
        let sheet = self.bank.sheet(id);
        if !spin_pictures::LABELS
            .iter()
            .all(|l| sheet.animation(l).is_some())
        {
            return false;
        }
        let Some(frame) = spin_pictures::label(step)
            .and_then(|l| sheet.animation(l))
            .and_then(|a| a.frames.first().copied())
        else {
            return false;
        };
        let (x, y) = (at.0 as f32 + 8.0, at.1 as f32 + 9.0);
        self.bank.draw(canvas, &Sprite::new(id, frame, x, y));
        true
    }

    /// A spinning collectible's picture on `step`: its own from
    /// [`spin_pictures::SHEET`], mirrored left to right with its origin
    /// at the collectible's position, if the sheet has them all (the
    /// nearest step's for steps without one); otherwise the frame of its kind's animation that
    /// far through the turn.
    fn spin_picture(&self, thing: &Thing, pose: &Pose, step: u8, x: f32, y: f32) -> Option<Sprite> {
        let own = self.kinds.get(spin_pictures::SHEET).and_then(|&id| {
            let sheet = self.bank.sheet(id);
            let all = spin_pictures::LABELS
                .iter()
                .all(|l| sheet.animation(l).is_some());
            let label = all.then(|| spin_pictures::pack_label(step))?;
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

    /// A stinger's picture with `count` counted frames since it
    /// appeared: its own from [`stinger_pictures::SHEET`] if the sheet
    /// has all four, otherwise the frame of its kind's animation for the
    /// same wing position; mirrored left to right either way.
    fn stinger_picture(
        &self,
        thing: &Thing,
        pose: &Pose,
        count: u32,
        x: f32,
        y: f32,
    ) -> Option<Sprite> {
        let at = stinger_pictures::picture(count);
        let own = self.kinds.get(stinger_pictures::SHEET).and_then(|&id| {
            let sheet = self.bank.sheet(id);
            let all = stinger_pictures::LABELS
                .iter()
                .all(|l| sheet.animation(l).is_some());
            let frame = *sheet
                .animation(stinger_pictures::LABELS[at])?
                .frames
                .first()?;
            all.then(|| Sprite {
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
            let frame = frames.len() * at / stinger_pictures::LABELS.len();
            Some(Sprite {
                flip_x: true,
                ..Sprite::new(id, frames.get(frame).copied().unwrap_or(0), x, y)
            })
        })
    }

    /// The patch a hidden barrel is drawn as, with its origin at the
    /// object's position: the pack's, or the engine's art of that name.
    fn patch_picture(&self, mirrored: bool, x: f32, y: f32) -> Option<Sprite> {
        let &id = self.kinds.get(hidden_pictures::SHEET)?;
        let frame = *self
            .bank
            .sheet(id)
            .animation(hidden_pictures::LABEL)?
            .frames
            .first()?;
        Some(Sprite {
            flip_x: mirrored,
            ..Sprite::new(id, frame, x, y)
        })
    }

    /// A tyre's own picture for its phase, a px left, if the pack's sheet
    /// for its kind has all seven; otherwise none, and the engine's art is drawn.
    fn tyre_picture(&self, thing: &Thing, x: f32, y: f32) -> Option<Sprite> {
        if !matches!(thing.archetype?.behavior, Behavior::Spring(_)) {
            return None;
        }
        let name = if thing.kind == "stiff-tyre" {
            tyre_pictures::STIFF
        } else {
            tyre_pictures::TYRE
        };
        let &id = self.kinds.get(name)?;
        let sheet = self.bank.sheet(id);
        if !tyre_pictures::LABELS
            .iter()
            .all(|l| sheet.animation(l).is_some())
        {
            return None;
        }
        let label = tyre_pictures::LABELS[tyre_pictures::picture(thing.phase)];
        let frame = *sheet.animation(label)?.frames.first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
    }

    /// A sign's own picture, a px left, if the pack's
    /// [`sign_pictures::SHEET`] has all three; otherwise none, and the
    /// engine's art is drawn.
    fn sign_picture(&self, thing: &Thing, x: f32, y: f32) -> Option<Sprite> {
        let Behavior::Scenery { picture } = thing.archetype?.behavior else {
            return None;
        };
        let &id = self.kinds.get(sign_pictures::SHEET)?;
        let sheet = self.bank.sheet(id);
        if !sign_pictures::LABELS
            .iter()
            .all(|l| sheet.animation(l).is_some())
        {
            return None;
        }
        let label = sign_pictures::LABELS.get(usize::from(picture))?;
        let frame = *sheet.animation(label)?.frames.first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
    }

    /// A launch barrel's own picture, a px left, if the pack's
    /// [`sling_pictures::SHEET`] has all those of its kind; otherwise
    /// none, and the engine's art is drawn on every frame.
    fn barrel_picture(&self, thing: &Thing, pose: &Pose, x: f32, y: f32) -> Option<Sprite> {
        let Behavior::Sling(sling) = thing.archetype?.behavior else {
            return None;
        };
        let down = sling_pictures::shoots_down(&sling);
        let needs: &[&str] = if down {
            &sling_pictures::DOWN
        } else {
            &sling_pictures::LEFT
        };
        let label = sling_pictures::picture(down, pose.sling);
        let at = (x, y);
        self.group_sprite(
            sling_pictures::SHEET,
            needs,
            needs.iter().position(|&l| l == label)?,
            false,
            at,
        )
    }

    /// A critter's picture at `clock`: its own from
    /// [`critter_pictures::SHEET`] if the sheet has all six, otherwise
    /// the frame of its kind's animation for the same wing position;
    /// mirrored as the clock says either way.
    fn critter_picture(
        &self,
        thing: &Thing,
        pose: &Pose,
        clock: critter_pictures::Clock,
        x: f32,
        y: f32,
    ) -> Option<Sprite> {
        let at = clock.picture();
        let flip_x = clock.mirrored;
        let own = self.kinds.get(critter_pictures::SHEET).and_then(|&id| {
            let sheet = self.bank.sheet(id);
            let all = critter_pictures::LABELS
                .iter()
                .all(|l| sheet.animation(l).is_some());
            let frame = *sheet
                .animation(critter_pictures::LABELS[at])?
                .frames
                .first()?;
            all.then(|| Sprite {
                flip_x,
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
            let frame = frames.len() * at / critter_pictures::LABELS.len();
            Some(Sprite {
                flip_x,
                ..Sprite::new(id, frames.get(frame).copied().unwrap_or(0), x, y)
            })
        })
    }

    /// An extra-life item's picture `count` frames after it appeared: its
    /// own from [`item_pictures::SHEET`], a px left of its position, if
    /// the sheet has all eight; otherwise the frame of its kind's
    /// animation that far through the cycle.
    fn item_picture(
        &self,
        thing: &Thing,
        pose: &Pose,
        count: u32,
        x: f32,
        y: f32,
    ) -> Option<Sprite> {
        let entry = item_pictures::entry(count);
        let own = self.kinds.get(item_pictures::SHEET).and_then(|&id| {
            let sheet = self.bank.sheet(id);
            let all = item_pictures::LABELS
                .iter()
                .all(|l| sheet.animation(l).is_some());
            let frame = *sheet
                .animation(item_pictures::LABELS[entry])?
                .frames
                .first()?;
            all.then(|| Sprite::new(id, frame, x - 1.0, y))
        });
        own.or_else(|| {
            let &id = self.kinds.get(&thing.kind)?;
            let frames = self
                .bank
                .sheet(id)
                .animations
                .get(pose.animation)
                .map_or(&[][..], |a| &a.frames[..]);
            let at = entry * frames.len() / item_pictures::LABELS.len();
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
        let metal = is_metal(thing);
        let (sheet_name, flight, lying) = if metal {
            (
                metal_pictures::SHEET,
                &metal_pictures::FLIGHT,
                metal_pictures::LYING,
            )
        } else {
            (
                thrown_pictures::SHEET,
                &thrown_pictures::FLIGHT,
                thrown_pictures::LYING,
            )
        };
        if let Some(count) = pose.thrown {
            let left = matches!(thing.phase, Phase::Thrown(f) if f.vx.is_negative());
            let entry = thrown_pictures::flight(count);
            let own = self.kinds.get(sheet_name).and_then(|&id| {
                let sheet = self.bank.sheet(id);
                let all = flight.iter().all(|l| sheet.animation(l).is_some());
                let frame = *sheet
                    .animation(flight[entry])
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
                let at = entry * frames.len() / flight.len();
                Some(Sprite {
                    flip_x: left,
                    ..Sprite::new(id, frames.get(at).copied().unwrap_or(0), x, y)
                })
            });
        }
        let id = *self.kinds.get(sheet_name)?;
        // A metal barrel that hasn't been picked up lies there too.
        let down = pose.put_down.is_some_and(thrown_pictures::lying)
            || (metal && matches!(thing.phase, Phase::Ready));
        if !down {
            return None;
        }
        let frame = *self.bank.sheet(id).animation(lying)?.frames.first()?;
        Some(Sprite::new(id, frame, x - 1.0, y))
    }

    /// Draws entry `entry` of a group of eight pictures of
    /// [`thrown_pictures::SHEET`] (a broken object's flash or pieces) a px
    /// left of `(x, y)`, mirrored if `flip`. Returns false, drawing
    /// nothing, when the sheet doesn't have all eight.
    pub fn draw_broken(
        &self,
        canvas: &mut dyn Surface,
        group: &[&str; 8],
        entry: usize,
        flip: bool,
        at: (f32, f32),
    ) -> bool {
        self.draw_group(canvas, thrown_pictures::SHEET, group, entry, flip, at)
    }

    /// Draws entry `entry` of `group` from the sheet called `sheet`, a px
    /// left of `at`, mirrored if `flip`. Returns false, drawing nothing,
    /// when the sheet doesn't have the whole group.
    fn draw_group(
        &self,
        canvas: &mut dyn Surface,
        sheet: &str,
        group: &[&str],
        entry: usize,
        flip: bool,
        (x, y): (f32, f32),
    ) -> bool {
        let Some(sprite) = self.group_sprite(sheet, group, entry, flip, (x, y)) else {
            return false;
        };
        self.bank.draw(canvas, &sprite);
        true
    }

    /// The sprite of entry `entry` of `group` in the sheet called
    /// `sheet`, a px left of `(x, y)`, if the sheet has the whole group.
    fn group_sprite(
        &self,
        sheet: &str,
        group: &[&str],
        entry: usize,
        flip: bool,
        (x, y): (f32, f32),
    ) -> Option<Sprite> {
        let &id = self.kinds.get(sheet)?;
        let sheet = self.bank.sheet(id);
        if !group.iter().all(|l| sheet.animation(l).is_some()) {
            return None;
        }
        let frame = *sheet.animation(group.get(entry)?)?.frames.first()?;
        Some(Sprite {
            flip_x: flip,
            ..Sprite::new(id, frame, x - 1.0, y)
        })
    }

    /// Riding picture `label` of the hutch sheet, mirrored as `shown` says,
    /// the rider's `d` px lower. Its origin is a px left of the position.
    fn ride_sprite(
        &self,
        label: &str,
        shown: ride_pictures::Shown,
        (x, y): (f32, f32),
    ) -> Option<Sprite> {
        let &id = self.kinds.get(hutch_pictures::SHEET)?;
        let frame = *self.bank.sheet(id).animation(label)?.frames.first()?;
        let d = if ride_pictures::rider_group(label).is_some() {
            shown.d
        } else {
            0
        };
        Some(Sprite {
            flip_x: shown.flip,
            ..Sprite::new(id, frame, x - 1.0, y + d as f32)
        })
    }

    /// The standing animal's picture `n` frames into its cycle, if the
    /// pack has the standing group.
    fn standing_sprite(&self, thing: &Thing, n: u32, at: (f32, f32)) -> Option<Sprite> {
        let entry = ride_pictures::standing_picture(n);
        let flip = thing.facing == Facing::Left;
        self.group_sprite(
            hutch_pictures::SHEET,
            &hutch_pictures::STANDING,
            entry,
            flip,
            at,
        )
    }

    /// What an animal crate's own pictures say `thing` shows: `None` if
    /// the engine's art does, `Some(None)` if nothing (the crate is
    /// drawn apart, by [`Cast::draw_crate`]), or its animal's picture
    /// while waiting from B + 18 and while mounted from M + 2 to M + 21.
    fn hutch_picture(&self, thing: &Thing, pose: &Pose, x: f32, y: f32) -> Option<Option<Sprite>> {
        if !is_hutch(thing) {
            return None;
        }
        let clock = pose.hutch;
        let at = (x, y);
        let sheet = hutch_pictures::SHEET;
        let own = match thing.phase {
            Phase::Mounted(since) => match hutch_pictures::mounting(u32::from(since)) {
                Some(entry) => {
                    let flip = clock.flip;
                    self.group_sprite(sheet, &hutch_pictures::MOUNTING, entry, flip, at)
                }
                // M and M + 1 are not covered: the animal keeps standing.
                None => self.standing_sprite(thing, clock.frames.unwrap_or(0), at),
            },
            // Left behind or running off: not covered either; the
            // standing animal, facing where it faces.
            Phase::Waiting(_) | Phase::Bolting { .. } => {
                self.standing_sprite(thing, clock.frames.unwrap_or(0), at)
            }
            Phase::Ridden => pose
                .ridden
                .filter(|_| {
                    ride_pictures::ANIMAL_GROUPS
                        .iter()
                        .all(|group| self.has_group(sheet, group))
                })
                .and_then(|shown| self.ride_sprite(shown.animal, shown, at)),
            Phase::Opened(_) if clock.frames >= Some(hutch_pictures::ANIMAL_FROM) => {
                let entry = hutch_pictures::waiting(clock.a);
                self.group_sprite(sheet, &hutch_pictures::ANIMAL, entry, false, at)
            }
            _ => None,
        };
        if own.is_some() {
            return Some(own);
        }
        let crate_own = self.has_group(sheet, &hutch_pictures::CRATE);
        let before_animal = match thing.phase {
            Phase::Ready => true,
            Phase::Opened(_) => clock.frames < Some(hutch_pictures::ANIMAL_FROM),
            _ => false,
        };
        (crate_own && before_animal).then_some(None)
    }

    /// Whether the sheet called `sheet` has the whole `group`.
    fn has_group(&self, sheet: &str, group: &[&str]) -> bool {
        self.kinds.get(sheet).is_some_and(|&id| {
            let sheet = self.bank.sheet(id);
            group.iter().all(|l| sheet.animation(l).is_some())
        })
    }

    /// Draws an animal crate itself, whole or broken, with its origin at
    /// `at`: its position, which stays where it was. Returns false,
    /// drawing nothing, when the sheet doesn't have all three pictures.
    pub fn draw_crate(&self, canvas: &mut dyn Surface, pose: &Pose, at: (f32, f32)) -> bool {
        let entry = hutch_pictures::crate_picture(pose.hutch.frames);
        let sheet = hutch_pictures::SHEET;
        self.draw_group(canvas, sheet, &hutch_pictures::CRATE, entry, false, at)
    }

    /// Draws the puff of smoke of a crate's breaking with its origin at
    /// `at`, if it's there and the sheet has all six pictures. Returns
    /// whether the sheet has them, as [`Cast::draw_crate`] does.
    pub fn draw_smoke(&self, canvas: &mut dyn Surface, pose: &Pose, at: (f32, f32)) -> bool {
        let group = &hutch_pictures::SMOKE;
        if !self.has_group(hutch_pictures::SHEET, group) {
            return false;
        }
        if let Some(entry) = pose
            .hutch
            .frames
            .and_then(|_| hutch_pictures::smoke(pose.hutch.c))
        {
            self.draw_group(canvas, hutch_pictures::SHEET, group, entry, false, at);
        }
        true
    }

    /// Draws entry `entry` of the smoke's six pictures with its origin at
    /// `at`. Returns false, drawing nothing, when the sheet doesn't have
    /// all six.
    pub fn draw_smoke_entry(&self, canvas: &mut dyn Surface, entry: usize, at: (f32, f32)) -> bool {
        let group = &hutch_pictures::SMOKE;
        self.draw_group(canvas, hutch_pictures::SHEET, group, entry, false, at)
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
    /// An extra-life item's frames since it appeared
    /// ([`crate::item_pictures`]).
    pub item: Option<u32>,
    /// A stinger's counted frames since it appeared
    /// ([`crate::stinger_pictures`]).
    pub stinger: Option<u32>,
    /// A critter's place in its pictures
    /// ([`crate::critter_pictures`]).
    pub critter: Option<critter_pictures::Clock>,
    /// Frames since a thrown object left the hands
    /// ([`crate::thrown_pictures`]).
    pub thrown: Option<u32>,
    /// Frames since an object was put down.
    pub put_down: Option<u32>,
    /// An animal crate's frames since it broke
    /// ([`crate::hutch_pictures`]).
    pub hutch: hutch_pictures::Clock,
    /// A launch barrel's frames since it caught the player
    /// ([`crate::sling_pictures`]).
    pub sling: Option<u32>,
    /// The player's riding pictures ([`crate::ride_pictures`]).
    pub ride: ride_pictures::Clock,
    /// What a ridden animal shows, from its rider's clock.
    pub ridden: Option<ride_pictures::Shown>,
}

/// Every actor's pose: the player's, the other character's, the lost
/// character's, and each thing's by its index in the world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Poses {
    pub player: Pose,
    pub other: Pose,
    pub leaving: Pose,
    /// What the other character seems to be doing, from how it moved
    /// ([`Poses::update`]), and where it was a frame ago.
    pub other_state: State,
    other_seen: Option<Vec2>,
    pub things: Vec<Pose>,
    /// The latest frame's number plus one, with frames numbered from the
    /// level's start or from play starting again after a lost life.
    pub clock: u32,
    /// The sparkles around letters.
    pub sparkles: Sparkles,
}

impl Poses {
    /// Poses for `world` as it starts, each at the start of its animation.
    pub fn new(cast: &Cast, world: &World) -> Poses {
        let mut poses = Poses {
            player: Pose::default(),
            other: Pose::default(),
            leaving: Pose::default(),
            other_state: State::Idle,
            other_seen: world.other.as_ref().map(|o| o.pos),
            things: vec![Pose::default(); world.things.len()],
            clock: 0,
            sparkles: Sparkles::default(),
        };
        poses.player.animation = cast.animation(cast.player, Some(player_animation(world, false)));
        poses.other.animation = cast.animation(cast.player, Some(other_animation(world)));
        poses.leaving.animation = cast.animation(cast.player, Some(leaving_animation(world)));
        for (pose, thing) in poses.things.iter_mut().zip(&world.things) {
            let id = cast.kinds.get(&thing.kind).copied();
            pose.animation = cast.animation(id, thing_animation(world, thing));
            pose.spin = spins(world, thing).then(|| spin_pictures::step(0, thing.pos));
            pose.bunch = (is_bunch(world, thing) && thing.active).then_some(0);
            pose.item = (is_item(thing) && thing.active).then_some(0);
            pose.stinger = (is_stinger(thing) && thing.active).then_some(0);
            pose.critter = critter(thing)
                .filter(|_| thing.active)
                .map(|_| critter_pictures::Clock::appearing(thing));
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
        self.sparkles.update(world, self.clock - 1);
        self.player.pictures.update(world, jump_in);
        let seen = world.other.as_ref().map(|o| o.pos);
        self.other_state = match (self.other_seen, seen) {
            (Some(a), Some(b)) if b.y < a.y => State::Jump,
            (Some(a), Some(b)) if b.y > a.y => State::Fall,
            (Some(a), Some(b)) if b.x != a.x => State::Walk,
            _ => State::Idle,
        };
        self.other_seen = seen;
        self.other
            .pictures
            .update_other(world, self.other_state == State::Walk);
        let id = cast.player;
        let landing = cast.playing(id, &self.player, LAND);
        let animation = cast.animation(id, Some(player_animation(world, landing)));
        cast.advance(&mut self.player, id, animation);
        self.player.ride.update(&world.player);
        let ride_shown = self.player.ride.shown();
        let animation = cast.animation(id, Some(other_animation(world)));
        cast.advance(&mut self.other, id, animation);
        let animation = cast.animation(id, Some(leaving_animation(world)));
        cast.advance(&mut self.leaving, id, animation);
        self.things.resize(world.things.len(), Pose::default());
        let still = crate::sparkles::nothing_moves(world);
        for (pose, thing) in self.things.iter_mut().zip(&world.things) {
            let id = cast.kinds.get(&thing.kind).copied();
            let animation = cast.animation(id, thing_animation(world, thing));
            cast.advance(pose, id, animation);
            pose.ridden = ride_shown.filter(|_| thing.phase == Phase::Ridden);
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
            // An item's cycle starts afresh on the frame it appears and
            // when play starts again, and steps on every frame.
            pose.item = match pose.item {
                _ if !is_item(thing) || !thing.active => None,
                Some(count) if !respawn => Some(count + 1),
                _ => Some(0),
            };
            // A stinger's cycle starts afresh on the frame it appears and
            // steps on every frame but those it holds still on.
            pose.stinger = match pose.stinger {
                _ if !is_stinger(thing) || !thing.active => None,
                Some(count) if !respawn => Some(count + u32::from(!thing.still)),
                _ => Some(0),
            };
            // A critter's pictures start afresh on the frame it appears.
            pose.critter = match (critter(thing), pose.critter) {
                (Some(perch), Some(clock)) if thing.active && !respawn => {
                    Some(clock.next(thing, perch))
                }
                (Some(_), _) if thing.active => Some(critter_pictures::Clock::appearing(thing)),
                _ => None,
            };
            // A thrown object's frames count from the one it leaves the
            // hands on, and a put-down one's from the one it is set down.
            pose.thrown = thrown(thing).then(|| pose.thrown.map_or(0, |n| n + 1));
            pose.put_down =
                matches!(thing.phase, Phase::Placed).then(|| pose.put_down.map_or(0, |n| n + 1));
            if matches!(
                thing.archetype.map(|a| a.behavior),
                Some(Behavior::Sling(_))
            ) {
                let caught = matches!(thing.phase, Phase::Caught(_));
                let reset = respawn || thing.phase == Phase::Ready;
                pose.sling = sling_pictures::next(pose.sling, caught, reset);
            }
            if is_hutch(thing) {
                let seed = self.clock.wrapping_add(thing.index as u32);
                let broken = thing.phase != Phase::Ready;
                pose.hutch = pose.hutch.next(broken, still, seed);
                if thing.phase == Phase::Mounted(0) {
                    pose.hutch.flip = world.player.facing == Facing::Left;
                }
            }
        }
    }
}

/// Whether `thing` is an animal crate (or the animal that came out of it).
fn is_hutch(thing: &Thing) -> bool {
    matches!(
        thing.archetype.map(|a| a.behavior),
        Some(Behavior::Hutch(_))
    )
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

/// A critter's perch length in frames, if `thing` is a critter.
fn critter(thing: &Thing) -> Option<u32> {
    match thing.archetype?.behavior {
        Behavior::Critter(c) => Some(u32::from(c.perch)),
        _ => None,
    }
}

/// Whether `thing` is an extra-life item.
fn is_item(thing: &Thing) -> bool {
    matches!(
        thing.archetype.map(|a| a.behavior),
        Some(Behavior::Collect { lives: 1.., .. })
    )
}

/// Whether `thing` is a stinger.
fn is_stinger(thing: &Thing) -> bool {
    matches!(
        thing.archetype.map(|a| a.behavior),
        Some(Behavior::Stinger(_))
    )
}

/// Whether `thing` is a collectible drawn as a bunch, as its entity's
/// `pictures` param says ([`crate::bunch_pictures`]).
fn is_bunch(world: &World, thing: &Thing) -> bool {
    pictures_param(world, thing) == Some(bunch_pictures::BUNCH)
}

/// Whether `thing` is a metal barrel: one that turns back at a wall.
fn is_metal(thing: &Thing) -> bool {
    matches!(
        thing.archetype.as_ref().map(|a| &a.behavior),
        Some(Behavior::Carry {
            turns_at_walls: true,
            ..
        })
    )
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
        | Phase::Struck(_)
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
    fn the_map_lead_is_drawn_from_the_map_sheet_when_all_the_pictures_are_there() {
        let all = crate::pictures::MAP;
        for (labels, drawn) in [(&all[..], true), (&all[..17], false), (&[][..], false)] {
            let cast = Cast::new(vec![(
                pictures::MAP_SHEET.to_string(),
                character_sheet(labels),
            )]);
            let mut canvas = Canvas::new(4, 1);
            assert_eq!(cast.draw_map_lead(&mut canvas, 3, false, (1, 0)), drawn);
            assert_eq!(canvas.get(1, 0).unwrap() == R, drawn);
        }
        let cast = Cast::new(vec![(
            pictures::MAP_SHEET.to_string(),
            character_sheet(&all),
        )]);
        let mut canvas = Canvas::new(4, 1);
        assert!(cast.draw_map_lead(&mut canvas, 3, true, (1, 0)));
        // mirrored about the origin, so the picture's pixels lie left of it
        assert_eq!(canvas.get(0, 0).unwrap(), R);
        assert_eq!(canvas.get(1, 0).unwrap(), Color::default());
    }

    #[test]
    fn a_map_set_is_drawn_when_the_whole_set_is_there_whatever_else_is() {
        let start = crate::pictures::MAP_START;
        let finish = crate::pictures::MAP_LEAD_FINISH;
        for (labels, start_drawn, finish_drawn) in [
            ([&start[..], &finish[..]].concat(), true, true),
            ([&start[..], &finish[..10]].concat(), true, false),
            ([&start[..8], &finish[..]].concat(), false, true),
        ] {
            let cast = Cast::new(vec![(
                pictures::MAP_SHEET.to_string(),
                character_sheet(&labels),
            )]);
            let mut canvas = Canvas::new(4, 1);
            assert_eq!(
                cast.draw_map_set(&mut canvas, &start, 2, (1, 0)),
                start_drawn
            );
            assert_eq!(
                cast.draw_map_set(&mut canvas, &finish, 2, (1, 0)),
                finish_drawn
            );
        }
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
            // Without its last picture the set isn't there: the nearest
            // picture the character has stands in, here the same one.
            (&set[..set.len() - 1], [R, Color::default()]),
            // With no picture of the character at all: the engine's art.
            (&[][..], [Color::default(), R]),
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
    fn the_follower_shows_its_own_slots_nearest_picture_when_the_spec_gives_none() {
        let mut world = world();
        let at = world.player.pos;
        world.other = Some(clamber_sim::Other::new(1, at, Facing::Right, at));
        let standing = crate::pictures::labels(1, crate::pictures::Set::Standing);
        for (labels, want) in [
            (&standing[..1], [R, Color::default()]),
            // With no picture of the character at all: the engine's art.
            (&[][..], [Color::default(), rgb(255, 255, 255)]),
        ] {
            let cast = Cast::new(vec![
                (PLAYER.to_string(), sheet()),
                (format!("{CHARACTER}1"), character_sheet(labels)),
            ]);
            let mut poses = Poses::new(&cast, &world);
            poses.update(&cast, &world, 0);
            assert_eq!(poses.other_state, State::Idle);
            let mut canvas = Canvas::new(4, 1);
            cast.draw_other(
                &mut canvas,
                &world,
                &poses.other,
                poses.other_state,
                2.0,
                0.0,
            );
            let got = [canvas.get(1, 0).unwrap(), canvas.get(2, 0).unwrap()];
            assert_eq!(got, want, "{labels:?}");
        }
        // Moving along the floor it is walking.
        let cast = Cast::new(vec![(PLAYER.to_string(), sheet())]);
        let mut poses = Poses::new(&cast, &world);
        world.other.as_mut().unwrap().pos.x += clamber_fixed::Fixed::from_int(1);
        poses.update(&cast, &world, 0);
        assert_eq!(poses.other_state, State::Walk);
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

    #[test]
    fn a_letter_gets_a_sparkle_on_frames_14_mod_16_that_lasts_forty_counted_frames() {
        let archetypes = clamber_sim::Archetypes::builtin();
        let mut world = world();
        let thing = &mut world.things[0];
        thing.kind = "letter".to_string();
        thing.archetype = archetypes.named("letter");
        thing.active = true;
        let cast = Cast::new(Vec::new());
        let mut poses = Poses::new(&cast, &world);
        let mut seen = Vec::new();
        for m in 0..60u32 {
            // Frames 20 and 21 are ones on which nothing moves.
            world.still = m == 20 || m == 21;
            poses.update(&cast, &world, 0);
            seen.push(
                poses
                    .sparkles
                    .list
                    .iter()
                    .map(|s| s.step())
                    .collect::<Vec<_>>(),
            );
        }
        assert!(seen[13].is_empty());
        assert_eq!(seen[14], [1]);
        // Frames 20 and 21 don't count: the step stays.
        assert_eq!(seen[19], [2]);
        assert_eq!(seen[20], [2]);
        assert_eq!(seen[21], [2]);
        assert_eq!(seen[22], [2]);
        // Frame 30 is a sparkle frame again, 16 on.
        assert_eq!(seen[30], [3, 1]);
        // The one from frame 14 shows step 8 through its 39th counted
        // frame after it, and is gone on the 40th.
        assert_eq!(seen[55], [8, 6, 2]);
        assert_eq!(seen[56], [6, 3]);
        // A letter that's gone gets no sparkle, and those there go on.
        world.still = false;
        world.things[0].active = false;
        for _ in 60..63 {
            poses.update(&cast, &world, 0);
        }
        assert!(poses.sparkles.list.iter().all(|s| s.step() > 1));
        assert!(!poses.sparkles.list.is_empty());
    }

    #[test]
    fn a_letter_shows_its_own_picture_when_the_sheet_has_all_four() {
        let archetypes = clamber_sim::Archetypes::builtin();
        let mut thing = world().things[0].clone();
        thing.kind = "letter".to_string();
        thing.archetype = archetypes.named("letter");
        assert_eq!(letter_pictures::shown(&thing), Some("letter-pic-1"));
        let labels = &letter_pictures::LABELS[..];
        for (labels, lit) in [(labels, true), (&labels[1..], false)] {
            let cast = Cast::new(vec![(
                letter_pictures::SHEET.to_string(),
                labelled_sheet(labels, 2),
            )]);
            let mut canvas = Canvas::new(4, 4);
            cast.draw_thing(&mut canvas, &thing, &Pose::default(), 2.0, 2.0);
            // Unmirrored, a px left of the letter's position.
            assert_eq!(
                canvas.get(1, 2).unwrap() == G,
                lit,
                "{} pictures",
                labels.len()
            );
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
            // Steps 6 and 7 have none of their own: the pack's nearest
            // (step 5's), never the engine's art.
            (labels, 6, [B, Color::default()]),
            (labels, 7, [B, Color::default()]),
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

    #[test]
    fn the_count_image_is_the_spin_picture_unmirrored_and_only_with_all_five() {
        let labels = &crate::spin_pictures::LABELS[..];
        for (labels, clock, drawn) in [
            // Step 5 from frame count 16, with the origin 8 px right of
            // and 9 below the corner.
            (labels, 16, true),
            (labels, 19, true),
            // Steps 6 to 8 have no pictures of their own.
            (labels, 20, false),
            (&labels[1..], 16, false),
        ] {
            let cast = Cast::new(vec![(
                crate::spin_pictures::SHEET.to_string(),
                labelled_sheet(labels, 4),
            )]);
            let mut canvas = Canvas::new(4, 1);
            let did = cast.draw_count_image(&mut canvas, clock, (-7, -9));
            assert_eq!(did, drawn, "{labels:?} on {clock}");
            assert_eq!(
                canvas.get(1, 0).unwrap() == B,
                drawn,
                "{labels:?} on {clock}"
            );
            assert_eq!(canvas.get(2, 0).unwrap(), Color::default());
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

    fn critter_at(vx: i32, clock: u32, still: bool) -> Thing {
        let mut thing = world().things[0].clone();
        thing.vx = clamber_fixed::Fixed::from_int(vx);
        thing.clock = clock;
        thing.still = still;
        thing
    }

    #[test]
    fn it_counts_flying_frames_and_perches_for_a_perch() {
        let mut clock = critter_pictures::Clock::appearing(&critter_at(-1, 0, false));
        assert!(clock.mirrored);
        clock = clock.next(&critter_at(1, 0, false), 360);
        assert_eq!((clock.flying, clock.mirrored), (1, false));
        // Held still, nothing changes, not even the mirroring.
        assert_eq!(clock.next(&critter_at(-1, 0, true), 360), clock);
        // It perches, keeping its mirroring, for 360 frames.
        clock = clock.next(&critter_at(-1, 359, false), 360);
        assert_eq!((clock.perched, clock.mirrored), (Some(0), false));
        clock = clock.next(&critter_at(-1, 358, false), 360);
        assert_eq!(clock.perched, Some(1));
        clock = clock.next(&critter_at(-1, 0, false), 360);
        assert_eq!(clock.perched, Some(2));
        clock.perched = Some(358);
        clock = clock.next(&critter_at(-1, 0, false), 360);
        assert_eq!(clock.perched, Some(359));
        clock = clock.next(&critter_at(-1, 0, false), 360);
        assert_eq!(
            (clock.perched, clock.flying, clock.mirrored),
            (None, 0, true)
        );
    }

    fn critter_thing() -> Thing {
        let mut thing = world().things[0].clone();
        thing.kind = "critter".to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named("critter");
        thing.active = true;
        thing
    }

    #[test]
    fn a_critter_shows_its_own_pictures_mirrored_as_it_flies() {
        let thing = critter_thing();
        let labels = &crate::critter_pictures::LABELS[..];
        for (labels, mirrored, want) in [
            (labels, true, [B, Color::default()]),
            (labels, false, [Color::default(), B]),
            // Without one of the six, the engine's art, mirrored the same.
            (&labels[1..], true, [R, Color::default()]),
        ] {
            let cast = Cast::new(vec![
                ("critter".to_string(), sheet()),
                (
                    crate::critter_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let pose = Pose {
                critter: Some(crate::critter_pictures::Clock {
                    mirrored,
                    ..Default::default()
                }),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &pose, 1.0, 0.0);
            let got = [canvas.get(0, 0).unwrap(), canvas.get(1, 0).unwrap()];
            assert_eq!(got, want, "{labels:?} mirrored {mirrored}");
        }
    }

    #[test]
    fn a_tyre_shows_its_own_pictures_only_with_all_seven() {
        let mut thing = world().things[0].clone();
        thing.kind = "spring".to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named("spring");
        thing.active = true;
        let labels = &crate::tyre_pictures::LABELS[..];
        for (labels, own) in [(labels, true), (&labels[1..], false)] {
            let cast = Cast::new(vec![
                ("spring".to_string(), sheet()),
                (
                    crate::tyre_pictures::TYRE.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &Pose::default(), 1.0, 0.0);
            let got = [canvas.get(0, 0).unwrap(), canvas.get(1, 0).unwrap()];
            assert_eq!(got == [B, Color::default()], own, "{labels:?}");
        }
    }

    #[test]
    fn an_item_shows_its_own_pictures_a_px_left_only_with_all_eight() {
        let mut thing = world().things[0].clone();
        thing.kind = "heart".to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named("heart");
        thing.active = true;
        let labels = &crate::item_pictures::LABELS[..];
        for (labels, own) in [(labels, true), (&labels[1..], false)] {
            let cast = Cast::new(vec![
                ("heart".to_string(), sheet()),
                (
                    crate::item_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let pose = Pose {
                item: Some(0),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &pose, 1.0, 0.0);
            let got = [canvas.get(0, 0).unwrap(), canvas.get(1, 0).unwrap()];
            assert_eq!(got == [B, Color::default()], own, "{labels:?}");
        }
    }

    #[test]
    fn a_sign_shows_its_own_picture_a_px_left_only_with_all_three() {
        let mut thing = world().things[0].clone();
        thing.kind = "signpost".to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named("signpost");
        thing.active = true;
        let labels = &crate::sign_pictures::LABELS[..];
        for (labels, own) in [(labels, true), (&labels[1..], false)] {
            let cast = Cast::new(vec![
                ("signpost".to_string(), sheet()),
                (
                    crate::sign_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &Pose::default(), 1.0, 0.0);
            let got = [canvas.get(0, 0).unwrap(), canvas.get(1, 0).unwrap()];
            assert_eq!(got == [B, Color::default()], own, "{labels:?}");
        }
    }

    #[test]
    fn a_barrel_shows_its_own_picture_only_with_all_of_its_kind() {
        let mut thing = world().things[0].clone();
        thing.kind = "sling".to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named("sling");
        thing.active = true;
        let labels = &crate::sling_pictures::LEFT[..];
        for (labels, own) in [(labels, true), (&labels[1..], false)] {
            let cast = Cast::new(vec![
                ("sling".to_string(), sheet()),
                (
                    crate::sling_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &Pose::default(), 1.0, 0.0);
            let got = [canvas.get(0, 0).unwrap(), canvas.get(1, 0).unwrap()];
            assert_eq!(got == [B, Color::default()], own, "{labels:?}");
        }
    }

    #[test]
    fn a_barrel_counts_the_frames_from_its_catch_until_it_is_ready_again() {
        let mut world = world();
        world.things[0].kind = "sling".to_string();
        world.things[0].archetype = clamber_sim::Archetypes::builtin().named("sling");
        let cast = Cast::new(Vec::new());
        let mut poses = Poses::new(&cast, &world);
        let mut counts = Vec::new();
        for phase in [
            Phase::Ready,
            Phase::Caught(0),
            Phase::Caught(1),
            Phase::Broken,
            Phase::Ready,
        ] {
            world.things[0].phase = phase;
            poses.update(&cast, &world, 0);
            counts.push(poses.things[0].sling);
        }
        assert_eq!(counts, [None, Some(0), Some(1), Some(2), None]);
    }

    /// A level with a hidden object at its start, with `params`.
    fn hidden_thing(params: &str) -> Thing {
        let src = format!(
            r#"
            format = 1
            size = [200, 100]
            start = [10, 10]

            [[entities]]
            type = "hidden"
            at = [20, 20]
            params = {params}
            "#
        );
        let level = Level::from_toml_str(&src, "test").unwrap();
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let mut thing = world.things[0].clone();
        thing.active = true;
        thing
    }

    #[test]
    fn a_hidden_barrel_is_drawn_as_the_patch_mirrored_if_it_goes_left() {
        let falling = |becomes: &str, across: i32| {
            hidden_thing(&format!(
                r#"{{ strike_frames = [], fall_height = 160, across = [[-28, 42], [-42, 28]], above = [-27, 20], becomes = "{becomes}", becomes_at = [0, 0], item_speed = [{across}, -6] }}"#
            ))
        };
        let cast = Cast::new(vec![(
            crate::hidden_pictures::SHEET.to_string(),
            labelled_sheet(&[crate::hidden_pictures::LABEL], 4),
        )]);
        let drawn = |thing: &Thing| {
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, thing, &Pose::default(), 2.0, 0.0);
            (0..4)
                .map(|x| canvas.get(x, 0).unwrap())
                .collect::<Vec<_>>()
        };
        let blank = vec![Color::default(); 4];
        let (barrel, metal) = (falling("crate", -1), falling("canister", 1));
        let (barrel, metal) = (drawn(&barrel), drawn(&metal));
        assert_ne!(metal, blank);
        assert_eq!(
            barrel.iter().rev().collect::<Vec<_>>(),
            metal.iter().collect::<Vec<_>>()
        );
        assert_ne!(barrel, metal);
        // Set off, it's no longer the patch; a hidden bunch never is.
        let mut set_off = falling("crate", -1);
        set_off.phase = Phase::Gone;
        assert_eq!(drawn(&set_off), blank);
        let bunch = hidden_thing(
            r#"{ strike_frames = [25], fall_height = 0, across = [[-53, 0], [0, 53]], above = [-52, 8], becomes = "cluster", becomes_at = [-1, 0], item_speed = [1, -6] }"#,
        );
        assert_eq!(drawn(&bunch), blank);
    }

    fn stinger_thing() -> Thing {
        let mut thing = world().things[0].clone();
        thing.kind = "stinger".to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named("stinger");
        thing.active = true;
        thing
    }

    #[test]
    fn a_stinger_counts_the_frames_it_moves_on_from_when_it_appears() {
        let mut world = world();
        world.things[0] = stinger_thing();
        let cast = Cast::new(Vec::new());
        let mut poses = Poses::new(&cast, &world);
        assert_eq!(poses.things[0].stinger, Some(0));
        let mut counts = Vec::new();
        for still in [false, false, true, true, false] {
            world.things[0].still = still;
            poses.update(&cast, &world, 0);
            counts.push(poses.things[0].stinger.unwrap());
        }
        assert_eq!(counts, [1, 2, 2, 2, 3]);
        // Gone, it shows nothing; back, it starts afresh.
        world.things[0].active = false;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].stinger, None);
        world.things[0].active = true;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].stinger, Some(0));
    }

    #[test]
    fn a_stinger_shows_its_own_pictures_mirrored_while_all_four_are_there() {
        let thing = stinger_thing();
        let labels = &crate::stinger_pictures::LABELS[..];
        for (labels, count, want) in [
            (labels, 0, [B, Color::default()]),
            (labels, 5, [B, Color::default()]),
            // Without one of the four, the engine's art, mirrored too, as
            // far through its animation as the wing position says.
            (&labels[1..], 0, [R, Color::default()]),
        ] {
            let cast = Cast::new(vec![
                ("stinger".to_string(), sheet()),
                (
                    crate::stinger_pictures::SHEET.to_string(),
                    labelled_sheet(labels, 4),
                ),
            ]);
            let pose = Pose {
                stinger: Some(count),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &pose, 1.0, 0.0);
            let got = [canvas.get(0, 0).unwrap(), canvas.get(1, 0).unwrap()];
            assert_eq!(got, want, "{labels:?} on {count}");
        }
    }

    fn crate_thing() -> Thing {
        let mut thing = world().things[0].clone();
        thing.kind = "hutch".to_string();
        thing.archetype = clamber_sim::Archetypes::builtin().named("hutch");
        thing.active = true;
        thing
    }

    #[test]
    fn a_crate_counts_frames_since_it_broke_and_those_on_which_something_moves() {
        let mut world = world();
        world.things[0] = crate_thing();
        let cast = Cast::new(Vec::new());
        let mut poses = Poses::new(&cast, &world);
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].hutch.frames, None);
        // B, then two frames on which nothing moves.
        world.things[0].phase = Phase::Opened(0);
        let mut seen = Vec::new();
        for m in 0..22u32 {
            world.still = m == 1 || m == 2;
            poses.update(&cast, &world, 0);
            let h = poses.things[0].hutch;
            seen.push((h.frames.unwrap(), h.c, h.a));
        }
        assert_eq!(seen[..4], [(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 1, 0)]);
        assert_eq!(seen[17], (17, 15, 0));
        assert_eq!(seen[18], (18, 16, 0));
        assert_eq!(seen[21], (21, 19, 3));
        // Mounted facing left, the pictures are mirrored.
        world.player.facing = Facing::Left;
        world.things[0].phase = Phase::Mounted(0);
        poses.update(&cast, &world, 0);
        assert!(poses.things[0].hutch.flip);
        // A crate that comes back is whole.
        world.things[0].phase = Phase::Ready;
        poses.update(&cast, &world, 0);
        assert_eq!(poses.things[0].hutch, hutch_pictures::Clock::default());
    }

    #[test]
    fn a_crate_and_its_animal_show_their_own_pictures_by_group() {
        let thing = crate_thing();
        let pose = |frames| Pose {
            hutch: hutch_pictures::Clock {
                frames,
                ..Default::default()
            },
            ..Pose::default()
        };
        let cast = |groups: &[&[&str]]| {
            let labels: Vec<&str> = groups.iter().flat_map(|g| g.iter().copied()).collect();
            Cast::new(vec![
                ("hutch".to_string(), sheet()),
                (
                    hutch_pictures::SHEET.to_string(),
                    labelled_sheet(&labels, 4),
                ),
            ])
        };
        let none = Color::default();
        let crates = &hutch_pictures::CRATE[..];
        let animal = &hutch_pictures::ANIMAL[..];
        let mounting = &hutch_pictures::MOUNTING[..];
        let draw = |cast: &Cast, thing: &Thing, pose: &Pose| {
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, thing, pose, 2.0, 0.0);
            [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap())
        };
        let mut waiting = thing.clone();
        waiting.phase = Phase::Opened(22);
        // Own animal a px left of the thing (its sheet frame is at x 4).
        let all = cast(&[crates, animal, mounting]);
        assert_eq!(draw(&all, &waiting, &pose(Some(30))), [none, B, none, none]);
        // Without the animal group the engine's art stays: the sheet's
        // first frame is red.
        let no_animal = cast(&[crates, mounting]);
        assert_eq!(
            draw(&no_animal, &waiting, &pose(Some(30))),
            [none, none, R, none]
        );
        // Before the animal is out, the crate is drawn apart, not here.
        let mut breaking = thing.clone();
        breaking.phase = Phase::Opened(3);
        assert_eq!(draw(&all, &breaking, &pose(Some(3))), [none; 4]);
        // Mounted, M + 2 on, mirrored if the player faced left.
        let mut mounted = thing.clone();
        mounted.phase = Phase::Mounted(2);
        let mut left = pose(Some(30));
        left.hutch.flip = true;
        assert_eq!(draw(&all, &mounted, &left), [B, none, none, none]);
        let mut canvas = Canvas::new(4, 1);
        assert!(all.draw_crate(&mut canvas, &pose(None), (2.0, 0.0)));
        assert_eq!(canvas.get(1, 0), Some(B));
        let no_crate = cast(&[animal, mounting]);
        assert!(!no_crate.draw_crate(&mut Canvas::new(4, 1), &pose(None), (2.0, 0.0)));
    }

    #[test]
    fn a_left_animal_and_the_first_mount_frames_show_the_standing_pictures() {
        let mut thing = crate_thing();
        let labels: Vec<&str> = hutch_pictures::STANDING.to_vec();
        let with = Cast::new(vec![
            ("hutch".to_string(), sheet()),
            (
                hutch_pictures::SHEET.to_string(),
                labelled_sheet(&labels, 4),
            ),
        ]);
        let none = Color::default();
        let draw = |thing: &Thing| {
            let mut canvas = Canvas::new(4, 1);
            with.draw_thing(&mut canvas, thing, &Pose::default(), 2.0, 0.0);
            [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap())
        };
        // animal-1 (the sheet's first frame, blue) a px left of the thing,
        // never the engine's red art.
        for phase in [Phase::Waiting(None), Phase::Mounted(0), Phase::Mounted(1)] {
            thing.phase = phase;
            assert_eq!(draw(&thing), [none, B, none, none], "{phase:?}");
        }
    }

    #[test]
    fn a_ridden_animal_shows_its_riding_pictures_by_group() {
        let mut thing = crate_thing();
        thing.phase = Phase::Ridden;
        let shown = ride_pictures::Shown {
            animal: "ride-walk-1",
            rider: "rider-14",
            d: 1,
            flip: false,
        };
        let pose = Pose {
            ridden: Some(shown),
            ..Pose::default()
        };
        let cast = |groups: &[&[&str]]| {
            let labels: Vec<&str> = groups.iter().flat_map(|g| g.iter().copied()).collect();
            Cast::new(vec![
                ("hutch".to_string(), sheet()),
                (
                    hutch_pictures::SHEET.to_string(),
                    labelled_sheet(&labels, 4),
                ),
            ])
        };
        let none = Color::default();
        let draw = |cast: &Cast| {
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &pose, 2.0, 0.0);
            [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap())
        };
        let mut groups: Vec<&[&str]> = ride_pictures::ANIMAL_GROUPS.to_vec();
        assert_eq!(draw(&cast(&groups)), [none, B, none, none]);
        // Without a group of them the engine's art stays.
        groups.pop();
        assert_eq!(draw(&cast(&groups)), [none, none, R, none]);
    }

    #[test]
    fn a_rider_is_drawn_from_its_own_characters_group() {
        let sheet_with = |labels: &[&str]| {
            Cast::new(vec![(
                hutch_pictures::SHEET.to_string(),
                labelled_sheet(labels, 4),
            )])
        };
        let none = Color::default();
        let draw = |cast: &Cast, rider: &'static str| {
            let mut world = world();
            world.player.character = if rider.starts_with("rider2-") { 1 } else { 0 };
            let pose = Pose {
                ride: ride_pictures::Clock::showing(ride_pictures::Shown {
                    animal: "ride-walk-1",
                    rider,
                    d: 0,
                    flip: false,
                }),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_player(&mut canvas, &world, &pose, 2.0, 0.0);
            [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap())
        };
        // Only the first character's group: the second character shows
        // none of it.
        let first = sheet_with(&ride_pictures::RIDER);
        assert_eq!(draw(&first, "rider-14"), [none, B, none, none]);
        assert_eq!(draw(&first, "rider2-6"), [none; 4]);
        // Only the second character's group.
        let second = sheet_with(&ride_pictures::RIDER2);
        assert_eq!(draw(&second, "rider2-6"), [none, B, none, none]);
        assert_eq!(draw(&second, "rider-14"), [none; 4]);
        // A group short of one picture is not drawn.
        let short = sheet_with(&ride_pictures::RIDER2[1..]);
        assert_eq!(draw(&short, "rider2-6"), [none; 4]);
    }

    #[test]
    fn a_broken_objects_pictures_need_the_whole_group() {
        let flash = &thrown_pictures::FLASH[..];
        let none = Color::default();
        for (labels, flip, want, drawn) in [
            (flash, false, [none, B, none, none], true),
            (flash, true, [B, none, none, none], true),
            (&flash[1..], false, [none; 4], false),
        ] {
            let cast = Cast::new(vec![(
                thrown_pictures::SHEET.to_string(),
                labelled_sheet(labels, 4),
            )]);
            let mut canvas = Canvas::new(4, 1);
            let got = cast.draw_broken(&mut canvas, &thrown_pictures::FLASH, 0, flip, (2.0, 0.0));
            assert_eq!(got, drawn);
            assert_eq!([0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap()), want);
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
            wait: 0,
            launch: false,
            second: false,
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

    fn metal_thing(phase: Phase) -> Thing {
        let mut thing = world().things[0].clone();
        thing.archetype = clamber_sim::Archetypes::builtin().named("canister");
        thing.phase = phase;
        thing
    }

    #[test]
    fn a_metal_barrel_shows_its_own_pictures_and_a_barrel_does_not_use_them() {
        let none = Color::default();
        let kind = world().things[0].kind.clone();
        let cast = Cast::new(vec![
            (kind.clone(), sheet()),
            (
                metal_pictures::SHEET.to_string(),
                labelled_sheet(&[metal_pictures::LYING], 4),
            ),
            (
                thrown_pictures::SHEET.to_string(),
                labelled_sheet(&[thrown_pictures::LYING], 4),
            ),
        ]);
        let drawn = |thing: &Thing, put_down| {
            let pose = Pose {
                put_down,
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, thing, &pose, 2.0, 0.0);
            [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap())
        };
        // Standing, and put down from the frame after, not mirrored.
        let metal_lying = drawn(&metal_thing(Phase::Placed), Some(1));
        assert_ne!(metal_lying, [none; 4]);
        assert_eq!(drawn(&metal_thing(Phase::Ready), None), metal_lying);
        // A barrel standing on the floor keeps its ordinary picture.
        let mut barrel = world().things[0].clone();
        barrel.phase = Phase::Ready;
        assert_ne!(drawn(&barrel, None), metal_lying);
    }

    #[test]
    fn a_thrown_metal_barrel_cycles_its_pictures_mirrored_with_its_direction() {
        let none = Color::default();
        let kind = world().things[0].kind.clone();
        let flying = |vx, labels: &[&str], count| {
            let cast = Cast::new(vec![
                (kind.clone(), sheet()),
                (metal_pictures::SHEET.to_string(), labelled_sheet(labels, 4)),
            ]);
            let mut thing = metal_thing(Phase::Ready);
            thing.phase = thrown_thing(vx).phase;
            let pose = Pose {
                thrown: Some(count),
                ..Pose::default()
            };
            let mut canvas = Canvas::new(4, 1);
            cast.draw_thing(&mut canvas, &thing, &pose, 2.0, 0.0);
            [0, 1, 2, 3].map(|x| canvas.get(x, 0).unwrap())
        };
        let labels = &metal_pictures::FLIGHT[..];
        assert_eq!(flying(2, labels, 0), [none, B, none, none]);
        assert_eq!(flying(-2, labels, 0), [B, none, none, none]);
        // Without one of the eight, the engine's art.
        assert_eq!(flying(2, &labels[1..], 0), [none, none, R, none]);
    }
}
