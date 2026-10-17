//! The player and entities as sprites, animated by what they're doing.
//!
//! A [`Cast`] holds the sheets: `player` for the player, and one per
//! entity kind, named after the kind. Each logic frame, every actor picks
//! the animation for what it's doing ([`Poses::update`]); an animation
//! carries on while it stays the same and starts over when it changes.
//!
//! The player's animations are named after its states, with `hurt` while
//! it's losing a life. Sprites face right and are mirrored to face left.
//! A character in roster slot `n` past the first uses the sheet's `n`th
//! named palette or variant image, in name order, if it has one. Entities play their
//! sheet's first animation, except a reached checkpoint, which plays
//! `reached`, a defeated enemy, which plays `defeated`, and a launcher
//! that just fired, which plays `fire`. Entities facing left are
//! mirrored too.

use std::collections::BTreeMap;
use std::path::Path;

use clamber_sim::{Behavior, CHECKPOINT, Facing, Outcome, Phase, State, Thing, World};
use clamber_sprite::{Playback, SpriteSheet};

use crate::sprites::{PaletteId, SheetId, Sprite, SpriteBank, SpriteBankBuilder};
use crate::surface::Surface;

/// The sheet the player is drawn with.
pub const PLAYER: &str = "player";

/// The sprite sheets the player and entities are drawn with.
#[derive(Clone, Debug)]
pub struct Cast {
    bank: SpriteBank,
    player: Option<SheetId>,
    /// The player's palettes or variants for roster slots after the first.
    alternates: Vec<PaletteId>,
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
        Cast {
            bank,
            player,
            alternates,
            kinds,
        }
    }

    /// Reads every sheet in `dir`, each named after its metadata file.
    pub fn load(dir: &Path) -> Result<Cast, String> {
        let err = |e: std::io::Error| format!("{}: {e}", dir.display());
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(err)? {
            let path = entry.map_err(err)?.path();
            if path.extension().is_some_and(|e| e == "json") {
                paths.push(path);
            }
        }
        paths.sort();
        let mut sheets = Vec::new();
        for path in paths {
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            let sheet = SpriteSheet::load(&path).map_err(|e| e.to_string())?;
            sheets.push((name.into_owned(), sheet));
        }
        Ok(Cast::new(sheets))
    }

    /// Whether the player has a sheet to be drawn with.
    pub fn has_player(&self) -> bool {
        self.player.is_some()
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
        self.draw_character(canvas, player.character, player.facing, pose, x, y);
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
        if let Some(&id) = self.kinds.get(&thing.kind) {
            let sprite = Sprite {
                flip_x: thing.facing == Facing::Left,
                ..self.sprite(id, pose, x, y)
            };
            self.bank.draw(canvas, &sprite);
        }
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

    /// Moves `pose` on a logic frame, onto `animation` of `id`'s sheet.
    fn advance(&self, pose: &mut Pose, id: Option<SheetId>, animation: usize) {
        if pose.animation != animation {
            *pose = Pose {
                animation,
                playback: Playback::new(),
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

/// Which animation an actor is playing, and how far through it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pose {
    /// Index into the sheet's animations.
    pub animation: usize,
    pub playback: Playback,
}

/// Every actor's pose: the player's, the other character's, the lost
/// character's, and each thing's by its index in the world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Poses {
    pub player: Pose,
    pub other: Pose,
    pub leaving: Pose,
    pub things: Vec<Pose>,
}

impl Poses {
    /// Poses for `world` as it starts, each at the start of its animation.
    pub fn new(cast: &Cast, world: &World) -> Poses {
        let mut poses = Poses {
            player: Pose::default(),
            other: Pose::default(),
            leaving: Pose::default(),
            things: vec![Pose::default(); world.things.len()],
        };
        poses.player.animation = cast.animation(cast.player, Some(player_animation(world)));
        poses.other.animation = cast.animation(cast.player, Some(other_animation(world)));
        poses.leaving.animation = cast.animation(cast.player, Some(leaving_animation(world)));
        for (pose, thing) in poses.things.iter_mut().zip(&world.things) {
            let id = cast.kinds.get(&thing.kind).copied();
            pose.animation = cast.animation(id, thing_animation(world, thing));
        }
        poses
    }

    /// Moves every pose on a logic frame, for `world` as it is now.
    pub fn update(&mut self, cast: &Cast, world: &World) {
        let id = cast.player;
        let animation = cast.animation(id, Some(player_animation(world)));
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
        }
    }
}

/// The player's animation: its state's, `jump` while bouncing, `fall`
/// while swimming (there's no swimming art yet), or `hurt` while losing
/// a life.
fn player_animation(world: &World) -> &'static str {
    match (world.flow.outcome, world.player.state) {
        (Outcome::Dying(_) | Outcome::GameOver, _) => "hurt",
        (_, State::Bounce) => "jump",
        (_, State::Swim) => "fall",
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
        Phase::Mounted(_) | Phase::Ridden | Phase::Bolting { .. } => return Some("animal"),
        Phase::Ready
        | Phase::Held
        | Phase::Thrown(_)
        | Phase::Placed
        | Phase::Flying { .. }
        | Phase::Swaying { .. } => {}
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
            poses.update(&cast, &world);
        }
        assert_eq!(shown, [R, R, G, G, R]);
        world.player.state = State::Walk;
        poses.update(&cast, &world);
        assert_eq!(poses.player.playback, Playback::new(), "starts over");
        assert_eq!(player_pixels(&cast, &world, &poses.player)[0], B);
        // No `jump` animation: the sheet's first plays instead.
        world.player.state = State::Jump;
        poses.update(&cast, &world);
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
        poses.update(&cast, &world);
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
        poses.update(&cast, &world);
        assert_eq!(player_pixels(&cast, &world, &poses.player)[0], B);
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
        ] {
            assert!(cast.has_kind(kind), "{kind}");
        }
        let checkpoint = cast.bank.sheet(cast.kinds[CHECKPOINT]);
        assert!(checkpoint.animation("reached").is_some());
    }
}
