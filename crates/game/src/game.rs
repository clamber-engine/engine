//! A level being played: the world, its camera, and drawing both.

use std::sync::Arc;

use clamber_import::Widescreen;
use clamber_level::{Area, Depth, Layer, Shape, Vec2};
use clamber_pack::{Feel, Param};
use clamber_sim::{
    Behavior, DIM, END, EXIT, Facing, FlowEvent, HALF_WIDTH, HEIGHT, HIDDEN, InputFrame, State,
    Thing, WALL_EXIT, World, step,
};

use crate::beyond::{Across, Beyond, opacity};
use crate::camera::{Camera, VIEW, point};
use crate::canvas::{Canvas, Color, rgb};
use crate::cast::{Cast, Poses};
use crate::display::Look;
use crate::hidden_pictures;
use crate::hud::{draw_hud_with, picture_left};
use crate::hutch_pictures;
use crate::layers::{LayerArt, Viewing, nudge};
use crate::sling_pictures;
use crate::surface::{Lighting, PointLight, Surface};
use crate::thrown_pictures;

const SKY: Color = rgb(0x2a, 0x34, 0x4a);
const SOLID: Color = rgb(0x6b, 0x74, 0x80);
const EDGE: Color = rgb(0xc8, 0xd0, 0xd8);
const ONE_WAY: Color = rgb(0x6c, 0xd4, 0x8a);
const HAZARD: Color = rgb(0xe0, 0x5a, 0x4a);
const REGION: Color = rgb(0xd8, 0xb0, 0x4a);
const WATER: Color = rgb(0x3a, 0x7a, 0xd8);
const ENTITY: Color = rgb(0xd0, 0x6a, 0xd8);
const EYE: Color = rgb(0x10, 0x10, 0x18);
const BLACK: Color = rgb(0, 0, 0);

/// The kinds always drawn behind the ground layer's raised tiles
/// (graphics.md, "Walkers and objects and raised tiles"). A `sign` is too,
/// but it's also drawn over the characters, which needs a mask: not done.
const BEHIND_RAISED: [&str; 1] = ["tyre"];
/// The kinds always drawn in front of them, also when the level ends.
const IN_FRONT_OF_RAISED: [&str; 11] = [
    "small-walker",
    "medium-walker",
    "large-walker",
    "thrower",
    "bunch",
    "barrel",
    "holder",
    "animal-crate",
    "figure",
    "restart-barrel",
    "down-launch-barrel",
];

/// Which things [`Game::draw_things`] draws, and where in the order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// Kinds behind the raised tiles.
    Behind,
    /// Everything but scenery, under the characters.
    Things,
    /// Scenery, which is drawn over the characters (signs.md).
    Scenery,
    /// Kinds in front of the raised tiles when those are drawn over the
    /// characters.
    InFront,
}
const PIECE: Color = rgb(0xb5, 0x79, 0x3f);
const SMOKE: Color = rgb(0xb8, 0xb8, 0xc0);
const FLASH: Color = rgb(0xff, 0xf2, 0xb0);

/// Size of the box marking an entity that has no sprite, px.
const ENTITY_BOX: i32 = 12;

/// How far a thing can move in one step, px, and still be drawn between
/// where it was and where it is. Nothing moves this fast; a thing that
/// went further was put somewhere else in one jump.
const JUMP: f64 = 32.0;

/// A part of a game's picture, to draw on its own ([`Game::draw_part`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The colour behind everything.
    Backdrop,
    /// The level's layer with this index.
    Layer(usize),
    /// The characters and the things in the level, and what's left of
    /// broken kegs.
    Sprites,
    /// The HUD, and the image of a taken relic.
    Hud,
}

/// Replacement art for a game's layers and sprites, where it has any.
#[derive(Clone, Default)]
struct Replaced {
    art: Option<Arc<LayerArt>>,
    cast: Option<Arc<Cast>>,
}

pub struct Game {
    pub world: World,
    pub camera: Camera,
    /// The player's position before the last step, for interpolation.
    previous: Vec2,
    /// The other character's, if there is one.
    previous_other: Option<Vec2>,
    /// The lost character's, if one is on its way out.
    previous_leaving: Option<Vec2>,
    /// Each thing's, by its index, if it was in the level.
    previous_things: Vec<Option<Vec2>>,
    /// Draw collision geometry, regions and entity markers.
    pub debug: bool,
    /// The images the level's layers draw.
    art: Arc<LayerArt>,
    /// The sprites the player and entities are drawn with.
    cast: Arc<Cast>,
    /// `art` and `cast` with replacement art packs' pictures, drawn
    /// instead of them when the game isn't drawn on the original's pixel
    /// grid ([`Look::pixel_art`]).
    replaced: Replaced,
    /// Where each actor is in its animation.
    poses: Poses,
    /// Frames left of a pause the world asked for ([`World::pause`]).
    pause: u8,
    /// How it's drawn while being played.
    pub look: Look,
    /// What's drawn beyond the world's own view, in original activation.
    beyond: Beyond,
}

impl Game {
    /// Starts playing `world`. The debug view starts on when the level
    /// has no layers, since there'd be nothing else to see.
    pub fn new(world: World) -> Game {
        let cast = Arc::new(Cast::default());
        Game {
            camera: Camera::new(&world),
            previous: world.player.pos,
            previous_other: world.other.as_ref().map(|o| o.pos),
            previous_leaving: world.leaving().map(|p| p.pos),
            previous_things: positions(&world),
            debug: world.level.layers.is_empty(),
            poses: Poses::new(&cast, &world),
            pause: 0,
            look: Look::RENDERED,
            beyond: Beyond::default(),
            world,
            art: Arc::default(),
            cast,
            replaced: Replaced::default(),
        }
    }

    /// Draws with `art` and `cast` instead of the game's own, where they
    /// are given, when it isn't drawn on the original's pixel grid. They
    /// have the same layers, sheets and frames as the game's own.
    pub fn with_replaced(self, art: Option<LayerArt>, cast: Option<Cast>) -> Game {
        Game {
            replaced: Replaced {
                art: art.map(Arc::new),
                cast: cast.map(Arc::new),
            },
            ..self
        }
    }

    /// The art the level's layers are drawn with now.
    fn drawn_art(&self) -> &LayerArt {
        match &self.replaced.art {
            Some(art) if !self.look.pixel_art => art,
            _ => &self.art,
        }
    }

    /// The sprites the player and entities are drawn with now.
    /// The sheets the level map's characters are drawn with.
    pub fn map_cast(&self) -> &Cast {
        self.drawn_cast()
    }

    fn drawn_cast(&self) -> &Cast {
        match &self.replaced.cast {
            Some(cast) if !self.look.pixel_art => cast,
            _ => &self.cast,
        }
    }

    /// Draws the level's layers with `art`.
    pub fn with_art(self, art: LayerArt) -> Game {
        Game {
            art: Arc::new(art),
            ..self
        }
    }

    /// Draws the game the way `look` says, with a camera to match, and
    /// plays it in the window the look has ([`Look::window`]). A world
    /// begun without that window goes by it from its next step; to have
    /// it from the level's start, begin it with [`Look::feel`].
    pub fn with_look(mut self, look: Look) -> Game {
        self.look = look;
        self.settle_look();
        self
    }

    /// Plays the world in the window the look has, and settles the
    /// camera and what's drawn beyond the world's view for the look.
    fn settle_look(&mut self) {
        self.world.feel = self.look.feel(&self.world.feel);
        self.camera = Camera::with_view(&self.world, self.shown().view);
        self.look_beyond();
    }

    /// Where the window and the world's own picture are, while things
    /// are drawn beyond the picture: in original activation, with a
    /// window wider than the world's view.
    fn across(&self) -> Option<Across> {
        let view = self.world.view?;
        let picture = self.world.feel.world[Param::ViewWidth].floor();
        let width = i64::from(self.camera.view.0);
        let wide = self.look.widescreen == Widescreen::OriginalActivation;
        (wide && picture > 0 && width > picture).then(|| Across {
            window: (self.camera.top_left(1.0).0.floor() as i64, width),
            picture: (view.x(), picture),
        })
    }

    /// Finds what's drawn beyond the world's own view afresh.
    fn look_beyond(&mut self) {
        self.beyond = match self.across() {
            Some(across) => Beyond::of(&self.world, across),
            None => Beyond::default(),
        };
    }

    /// The look the game is drawn with: its own, framed as its
    /// widescreen mode says ([`Look::framing`]).
    pub fn shown(&self) -> Look {
        self.look.framing(&self.world)
    }

    /// Plays in widescreen mode `mode` from now on, with the camera
    /// settled again for it.
    pub fn set_widescreen(&mut self, mode: Widescreen) {
        if self.look.widescreen != mode {
            self.look.widescreen = mode;
            self.settle_look();
        }
    }

    /// Draws the player and entities with `cast`'s sprites, and those
    /// without one as markers.
    pub fn with_cast(self, cast: Cast) -> Game {
        Game {
            poses: Poses::new(&cast, &self.world),
            cast: Arc::new(cast),
            ..self
        }
    }

    /// Starts the level over, keeping the debug view setting.
    pub fn restart(&mut self) {
        let world = World::new(self.world.level.clone(), self.world.feel.clone());
        self.replace_world(world);
    }

    /// Goes on where the level's entity `exit` leads, once play has left
    /// through it: the level's arrival its `arrival` param names, or the
    /// level's start if it names none there. An exit that leads to
    /// another area of the pack ([`clamber_sim::area`]) is for whatever
    /// loads the pack's levels to follow, as [`Session`] does.
    ///
    /// [`Session`]: crate::Session
    pub fn leave_by(&mut self, exit: usize) {
        let level = self.world.level.clone();
        let arrival = level
            .entities
            .get(exit)
            .and_then(|e| e.params.get("arrival")?.as_str());
        self.come_back(arrival);
    }

    /// Comes back into the level at the arrival called `arrival`, as
    /// [`leave_by`](Game::leave_by) does, or restarts it if it has no
    /// such arrival.
    pub fn come_back(&mut self, arrival: Option<&str>) {
        let level = self.world.level.clone();
        match arrival.and_then(|name| level.arrival(name)) {
            Some(arrival) => {
                let world = self.world.coming_back(arrival);
                self.replace_world(world);
            }
            None => self.restart(),
        }
    }

    /// Plays `world`, of this game's level, in place of the one under
    /// way, keeping the debug view setting.
    pub fn replace_world(&mut self, world: World) {
        *self = Game {
            debug: self.debug,
            art: self.art.clone(),
            poses: Poses::new(&self.cast, &world),
            cast: self.cast.clone(),
            replaced: self.replaced.clone(),
            ..Game::new(world).with_look(self.look)
        };
    }

    /// Plays `feel` from the start of the level.
    pub fn set_feel(&mut self, feel: Feel) {
        self.world.feel = self.look.feel(&Arc::new(feel));
        self.restart();
    }

    /// Advances one logic frame. Once the level is complete or over, the
    /// world stays as it is until [`restart`](Game::restart).
    ///
    /// During a pause the world asked for ([`World::pause`]), holds the
    /// screen for a frame instead, and returns false.
    pub fn step(&mut self, input: &InputFrame) -> bool {
        self.previous = self.world.player.pos;
        self.previous_other = self.world.other.as_ref().map(|o| o.pos);
        self.previous_leaving = self.world.leaving().map(|p| p.pos);
        self.previous_things = positions(&self.world);
        let character = self.world.player.character;
        if self.pause > 0 {
            self.pause -= 1;
            self.camera.previous = self.camera.centre;
            return false;
        }
        let before = self.across().map(|_| self.world.things.clone());
        step(&mut self.world, input);
        self.pause = self.world.pause;
        if self.world.player.character != character {
            // The characters have traded places: each is where the
            // other was drawn.
            self.previous = self.world.player.pos;
            self.previous_other = self.world.other.as_ref().map(|o| o.pos);
        }
        if self.world.flow.events.contains(&FlowEvent::Respawn) {
            // A jump to somewhere else, not a move to draw between.
            self.previous = self.world.player.pos;
            self.previous_things = positions(&self.world);
            self.camera = Camera::with_view(&self.world, self.shown().view);
        } else {
            self.camera.update(&self.world);
        }
        match (self.across(), before) {
            (Some(across), Some(before)) => {
                self.beyond
                    .update(&before, &self.poses.things, &self.world, across);
            }
            _ => self.look_beyond(),
        }
        let jump_in = if self.cast.has_pictures() {
            jump_in(&self.world, input)
        } else {
            0
        };
        self.poses.update(&self.cast, &self.world, jump_in);
        true
    }

    /// A new canvas the size of the view.
    pub fn canvas() -> Canvas {
        Canvas::new(VIEW.0, VIEW.1)
    }

    /// Draws the world, and the HUD over it, `alpha` of the way from the
    /// state before the last step to the state after it. A canvas showing
    /// more or less than the camera's view is centred where it is.
    pub fn draw(&self, canvas: &mut dyn Surface, alpha: f64) {
        self.draw_parts(canvas, alpha, None);
        let (now, full) = self.world.brightness();
        if now < full {
            let dark = 255 * (full - now) / full;
            canvas.shade(BLACK, dark as u8);
        }
    }

    /// Draws one part of what [`draw`](Game::draw) does, as it would be
    /// drawn there, over what's on the canvas already: nothing else, and
    /// not darkened by the picture's brightness.
    pub fn draw_part(&self, canvas: &mut dyn Surface, alpha: f64, part: Part) {
        self.draw_parts(canvas, alpha, Some(part));
    }

    /// Draws every part, or only `only`; the debug view only with every
    /// part.
    fn draw_parts(&self, canvas: &mut dyn Surface, alpha: f64, only: Option<Part>) {
        let shows = |part: Part| only.is_none_or(|o| o == part);
        let origin = self.camera.top_left_of(canvas.view(), alpha);
        let to_screen = |p: (f64, f64)| (p.0 - origin.0, p.1 - origin.1);
        if shows(Part::Backdrop) {
            canvas.clear(SKY);
        }

        let level = &self.world.level;
        canvas.light(&lighting(level, origin, canvas.view()));
        let behind = self.world.behind_scenery();
        let dim = self.world.dim_step();
        let time = (self.world.frame as f64 + alpha) / clamber_sim::STEPS_PER_SECOND as f64;
        let viewing = Viewing {
            origin,
            grid: (self.look.pixel_art).then(|| self.world.view.map_or(0.0, |v| nudge(v.left))),
            dim,
            time,
            steps: self.world.frame,
        };
        let layers = |canvas: &mut dyn Surface, keep: &dyn Fn(&Layer) -> bool| {
            let keep = |i: usize, l: &Layer| shows(Part::Layer(i)) && keep(l);
            self.drawn_art().draw_some(canvas, level, &viewing, keep);
        };
        let sprites = shows(Part::Sprites);
        // Raised layers are behind the characters, except in the walk-outs.
        layers(canvas, &|l| l.depth == Depth::Back);
        // The things of some kinds are always behind the raised layers
        // (graphics.md, "Walkers and objects and raised tiles").
        if sprites {
            self.draw_things(canvas, &to_screen, alpha, Pass::Behind);
        }
        layers(canvas, &|l| l.depth == Depth::Raised && !behind);
        if self.debug && only.is_none() {
            for region in &level.regions {
                let points = match &region.area {
                    Area::Rect(r) => r.corners(),
                    Area::Polygon(points) => points.clone(),
                };
                let points: Vec<_> = points.into_iter().map(|p| to_screen(point(p))).collect();
                let color = if region.water { WATER } else { REGION };
                canvas.fill_polygon(&points, color, if region.water { 96 } else { 24 });
                outline(canvas, &points, color);
            }
            for collider in &level.collision.colliders {
                let hazard = level
                    .surfaces
                    .get(&collider.surface)
                    .is_some_and(|s| s.hazard);
                match &collider.shape {
                    Shape::Solid { points, one_way } => {
                        let points: Vec<_> = points.iter().map(|&p| to_screen(point(p))).collect();
                        canvas.fill_polygon(&points, SOLID, 255);
                        let color = match (hazard, one_way) {
                            (true, _) => HAZARD,
                            (false, true) => ONE_WAY,
                            (false, false) => EDGE,
                        };
                        outline(canvas, &points, color);
                    }
                    Shape::Segment { a, b, one_way } => {
                        let color = match (hazard, one_way) {
                            (true, _) => HAZARD,
                            (false, true) => ONE_WAY,
                            (false, false) => EDGE,
                        };
                        canvas.line(to_screen(point(*a)), to_screen(point(*b)), color);
                    }
                    _ => {}
                }
            }
        }

        if sprites {
            self.draw_crates(canvas, &to_screen, false);
            self.draw_things(canvas, &to_screen, alpha, Pass::Things);
            self.draw_crates(canvas, &to_screen, true);
            self.draw_barrel_effects(canvas, &to_screen);
            self.draw_kegs(canvas, &to_screen);
            self.draw_sparkles(canvas, &to_screen);
            self.draw_characters(canvas, alpha, origin);
            self.draw_things(canvas, &to_screen, alpha, Pass::Scenery);
        }
        layers(canvas, &|l| behind && l.depth == Depth::Raised);
        // Only the characters and the animal go behind them when the
        // level ends; the other kinds stay in front.
        if sprites && behind {
            self.draw_things(canvas, &to_screen, alpha, Pass::InFront);
        }
        layers(canvas, &|l| l.depth == Depth::Front);
        if shows(Part::Hud) {
            let clock = self.poses.clock;
            let cast = self.drawn_cast();
            draw_hud_with(canvas, &self.world, &|canvas, at| {
                cast.draw_count_image(canvas, clock, at)
            });
            self.draw_relic_image(canvas);
            self.draw_flying_copies(canvas);
        }
    }

    /// The things in the level that `pass` draws. Those in the level are
    /// drawn `alpha` of the way through the last step ([`thing_at`]).
    ///
    /// [`thing_at`]: Game::thing_at
    fn draw_things(
        &self,
        canvas: &mut dyn Surface,
        to_screen: &dyn Fn((f64, f64)) -> (f64, f64),
        alpha: f64,
        pass: Pass,
    ) {
        let behind = self.world.behind_scenery();
        let pose = |thing: &Thing| self.poses.things.get(thing.index).copied();
        let in_level = self.world.things.iter().filter(|t| t.active);
        let in_level = in_level.map(|t| (t, self.thing_at(t, alpha)));
        let waiting = self.beyond.waiting.iter().map(|t| (t, point(t.pos)));
        let drawn = in_level.chain(waiting).map(|(t, at)| {
            let pose = pose(t).unwrap_or_default();
            (t, at, pose, self.beyond.opacity(t.index))
        });
        let leaving = self.beyond.leaving.iter();
        let leaving = leaving.map(|(t, pose, left)| (t, point(t.pos), *pose, opacity(*left)));
        for (thing, at, pose, opacity) in drawn.chain(leaving) {
            // The end marker, exits, wall exits, the dimming's entity and hidden
            // objects (but for a hidden barrel's patch) aren't drawn.
            let is_scenery = thing
                .archetype
                .is_some_and(|a| matches!(a.behavior, Behavior::Scenery { .. }));
            let patch = hidden_pictures::shown(thing).is_some();
            let wanted = match pass {
                Pass::Behind => BEHIND_RAISED.contains(&thing.kind.as_str()),
                Pass::InFront => behind && IN_FRONT_OF_RAISED.contains(&thing.kind.as_str()),
                Pass::Things | Pass::Scenery => {
                    let hidden_by_raised = BEHIND_RAISED.contains(&thing.kind.as_str());
                    let later = behind && IN_FRONT_OF_RAISED.contains(&thing.kind.as_str());
                    (is_scenery == (pass == Pass::Scenery)) && !hidden_by_raised && !later
                }
            };
            if !wanted
                || ([END, EXIT, WALL_EXIT, DIM, HIDDEN].contains(&thing.kind.as_str())
                    && !patch
                    && !self.debug)
            {
                continue;
            }
            let (x, y) = to_screen(at);
            if patch || self.cast.has_kind(&thing.kind) {
                let at = (x as f32, y as f32);
                self.drawn_cast()
                    .draw_thing_faded(canvas, thing, &pose, at, opacity);
            }
            let (x, y) = (x.round() as i32, y.round() as i32);
            if self.debug || (!patch && !self.cast.has_kind(&thing.kind)) {
                let half = ENTITY_BOX / 2;
                canvas.stroke_rect(x - half, y - half, ENTITY_BOX, ENTITY_BOX, ENTITY);
                canvas.plot(x, y, ENTITY);
            }
        }
    }

    /// Where `thing`, which is in the level, is drawn `alpha` of the way
    /// through the last step: between where it was before the step and
    /// where it is, as the characters are. A thing that wasn't in the
    /// level before the step, or that jumped somewhere else in it, is
    /// drawn where it is.
    fn thing_at(&self, thing: &Thing, alpha: f64) -> (f64, f64) {
        let to = point(thing.pos);
        let Some(&Some(from)) = self.previous_things.get(thing.index) else {
            return to;
        };
        let (x, y) = (point(from).0 - to.0, point(from).1 - to.1);
        if x.abs() > JUMP || y.abs() > JUMP {
            return to;
        }
        between(from, thing.pos, alpha, (0.0, 0.0))
    }

    /// The copies of collectibles' images flying to the count: the
    /// spinning collectible's first picture, unmirrored, with each
    /// square's corner in the world's picture (collectibles.md, "The
    /// flying copy"). Without that picture there's nothing to draw.
    fn draw_flying_copies(&self, canvas: &mut dyn Surface) {
        let left = picture_left(canvas, &self.world);
        let cast = self.drawn_cast();
        for (x, y) in self.world.flying_copies() {
            cast.draw_count_image(canvas, 0, (left + x, y));
        }
    }

    /// The image of a taken relic, while the world shows it: its sprite
    /// centred in the image's box, which is in the world's picture. How
    /// it looks isn't in the spec; without a sprite, it's the box.
    fn draw_relic_image(&self, canvas: &mut dyn Surface) {
        let Some(image) = self.world.relic_image() else {
            return;
        };
        let left = picture_left(canvas, &self.world);
        let (x, y) = (left + image.x, image.y);
        let thing = &self.world.things[image.thing];
        if self.cast.has_kind(&thing.kind) {
            let pose = &self.poses.things[image.thing];
            let half = image.size as f32 / 2.0;
            self.drawn_cast()
                .draw_thing(canvas, thing, pose, x as f32 + half, y as f32 + half);
        } else {
            canvas.stroke_rect(x, y, image.size, image.size, ENTITY);
        }
    }

    /// The keg the player waits in after a lost life, and the pieces and
    /// flashes of kegs that broke. How the spec's pieces and flash look
    /// isn't covered; they're small squares.
    fn draw_kegs(&self, canvas: &mut dyn Surface, to_screen: &dyn Fn((f64, f64)) -> (f64, f64)) {
        if let (Some(at), Some(resume)) = (self.world.resume_keg(), self.world.resume) {
            let keg = &self.world.things[resume.keg];
            let (x, y) = to_screen(point(at));
            if self.cast.has_kind(&keg.kind) {
                let pose = &self.poses.things[resume.keg];
                self.drawn_cast()
                    .draw_thing(canvas, keg, pose, x as f32, y as f32);
            } else {
                let half = ENTITY_BOX / 2;
                let (x, y) = (x.round() as i32, y.round() as i32);
                canvas.stroke_rect(x - half, y - half, ENTITY_BOX, ENTITY_BOX, ENTITY);
            }
        }
        for burst in &self.world.bursts {
            let count = u32::from(burst.frame);
            for (i, &piece) in burst.pieces.iter().enumerate() {
                let (x, y) = to_screen(point(piece));
                if burst.object.is_some() {
                    let group = &thrown_pictures::PIECES[i / 2];
                    let entry = thrown_pictures::piece(count);
                    let at = (x as f32, y as f32);
                    if self
                        .drawn_cast()
                        .draw_broken(canvas, group, entry, false, at)
                    {
                        continue;
                    }
                }
                canvas.fill_rect(x.round() as i32 - 2, y.round() as i32 - 2, 4, 4, PIECE);
            }
            let (x, y) = to_screen(point(burst.flash));
            if let Some(right) = burst.object {
                // Its eight pictures last 19 frames, as long as the burst.
                let Some(entry) = thrown_pictures::flash(count) else {
                    continue;
                };
                let at = (x as f32, y as f32);
                let group = &thrown_pictures::FLASH;
                if self
                    .drawn_cast()
                    .draw_broken(canvas, group, entry, right, at)
                {
                    continue;
                }
            }
            canvas.fill_rect(x.round() as i32 - 3, y.round() as i32 - 3, 6, 6, FLASH);
        }
    }

    /// The animal crates themselves, whole or broken, standing where
    /// they were placed, or (`effects`) the flash and smoke of their
    /// breaking (animal-crate.md, "Pictures"). A group the sheet doesn't
    /// have is the engine's own: the crate is the thing's own art, and
    /// the flash and smoke are squares.
    fn draw_crates(
        &self,
        canvas: &mut dyn Surface,
        to_screen: &dyn Fn((f64, f64)) -> (f64, f64),
        effects: bool,
    ) {
        for thing in self.world.things.iter().filter(|t| t.active) {
            let hutch = thing
                .archetype
                .is_some_and(|a| matches!(a.behavior, Behavior::Hutch(_)));
            let Some(pose) = self.poses.things.get(thing.index).filter(|_| hutch) else {
                continue;
            };
            let Some(origin) = self.world.level.entities.get(thing.index).map(|e| e.at) else {
                continue;
            };
            let (x, y) = (origin.x.to_f64(), origin.y.to_f64());
            let at = |dx: f64, dy: f64| {
                let (x, y) = to_screen((x + dx, y + dy));
                (x as f32, y as f32)
            };
            let cast = self.drawn_cast();
            if !effects {
                cast.draw_crate(canvas, pose, at(0.0, 0.0));
                continue;
            }
            let clock = pose.hutch;
            if clock.frames.is_none() {
                continue;
            }
            if let Some(entry) = hutch_pictures::flash(clock.c) {
                let rise = -hutch_pictures::flash_rise(clock.c);
                let (fx, fy) = at(0.0, rise);
                let group = &thrown_pictures::FLASH;
                if !cast.draw_broken(canvas, group, entry, false, (fx, fy)) {
                    let (px, py) = (fx.round() as i32, fy.round() as i32);
                    canvas.fill_rect(px - 3, py - 3, 6, 6, FLASH);
                }
            }
            let smoke = at(f64::from(clock.dx), -f64::from(clock.dy));
            if !cast.draw_smoke(canvas, pose, smoke) && hutch_pictures::smoke(clock.c).is_some() {
                let (px, py) = (smoke.0.round() as i32, smoke.1.round() as i32);
                canvas.fill_rect(px - 4, py - 4, 8, 8, SMOKE);
            }
        }
    }

    /// The smoke at a launch barrel's catch and the flash at its shot
    /// (launch-barrel.md, "Pictures"), drawn while they last, also after
    /// a barrel that breaks is gone. A group the sheet doesn't have is the
    /// engine's own: squares, as for a crate.
    fn draw_barrel_effects(
        &self,
        canvas: &mut dyn Surface,
        to_screen: &dyn Fn((f64, f64)) -> (f64, f64),
    ) {
        let cast = self.drawn_cast();
        for thing in &self.world.things {
            let Some(Behavior::Sling(sling)) = thing.archetype.map(|a| a.behavior) else {
                continue;
            };
            let Some(n) = self.poses.things.get(thing.index).and_then(|p| p.sling) else {
                continue;
            };
            let (x, y) = point(thing.pos);
            let at = |dx: f64, dy: f64| {
                let (x, y) = to_screen((x + dx, y + dy));
                (x as f32, y as f32)
            };
            if let Some(entry) = sling_pictures::smoke(n) {
                let (sx, sy) = at(-15.0, -18.5);
                if !cast.draw_smoke_entry(canvas, entry, (sx, sy)) {
                    let (px, py) = (sx.round() as i32, sy.round() as i32);
                    canvas.fill_rect(px - 4, py - 4, 8, 8, SMOKE);
                }
            }
            let shot = u32::from(sling.hold) + 1;
            let Some(k) = n.checked_sub(shot) else {
                continue;
            };
            let Some(entry) = sling_pictures::flash(k) else {
                continue;
            };
            let moved = sling_pictures::flash_moved(k);
            let down = sling_pictures::shoots_down(&sling);
            let (dx, dy) = if down {
                (0.0, -16.0 + moved)
            } else {
                (-moved, -16.0)
            };
            let (fx, fy) = at(dx, dy);
            let group = &thrown_pictures::FLASH;
            if !cast.draw_broken(canvas, group, entry, false, (fx, fy)) {
                let (px, py) = (fx.round() as i32, fy.round() as i32);
                canvas.fill_rect(px - 3, py - 3, 6, 6, FLASH);
            }
        }
    }

    /// The sparkles around letters: the engine's own art, a cross that
    /// grows and shrinks, centred on each sparkle's point.
    fn draw_sparkles(
        &self,
        canvas: &mut dyn Surface,
        to_screen: &dyn Fn((f64, f64)) -> (f64, f64),
    ) {
        for sparkle in &self.poses.sparkles.list {
            let (x, y) = to_screen((f64::from(sparkle.x), f64::from(sparkle.y)));
            let (x, y, r) = (x.round() as i32, y.round() as i32, sparkle.reach());
            canvas.fill_rect(x - r, y, 2 * r + 1, 1, FLASH);
            canvas.fill_rect(x, y - r, 1, 2 * r + 1, FLASH);
            canvas.fill_rect(x - 1, y - 1, 3, 3, FLASH);
        }
    }

    fn draw_characters(&self, canvas: &mut dyn Surface, alpha: f64, origin: (f64, f64)) {
        self.draw_leaving(canvas, alpha, origin);
        self.draw_other(canvas, alpha, origin);
        self.draw_player(canvas, alpha, origin);
    }

    /// The player's sprite, or without one, its body box coloured by
    /// state. Not on the frames it flashes off.
    fn draw_player(&self, canvas: &mut dyn Surface, alpha: f64, origin: (f64, f64)) {
        if !self.world.player_drawn() {
            return;
        }
        let player = &self.world.player;
        let (x, y) = between(self.previous, player.pos, alpha, origin);
        if self.cast.has_player() {
            let (x, y) = (x as f32, y as f32);
            self.drawn_cast()
                .draw_player(canvas, &self.world, &self.poses.player, x, y);
            return;
        }
        draw_body(canvas, (x, y), player.state, player.facing);
    }

    /// The other character, if there is one, as the player is drawn: in
    /// the swap state during a swap, idle otherwise. Not while the player
    /// waits in a keg, or it waits in the entrance.
    fn draw_other(&self, canvas: &mut dyn Surface, alpha: f64, origin: (f64, f64)) {
        let Some(other) = self
            .world
            .other
            .as_ref()
            .filter(|_| !self.world.in_keg() && !self.world.other_in_entrance())
        else {
            return;
        };
        let previous = self.previous_other.unwrap_or(other.pos);
        let (x, y) = between(previous, other.pos, alpha, origin);
        if self.cast.has_player() {
            let (x, y) = (x as f32, y as f32);
            self.drawn_cast()
                .draw_other(canvas, &self.world, &self.poses.other, x, y);
            return;
        }
        let state = if self.world.swap.is_some() {
            State::Swap
        } else {
            State::Idle
        };
        draw_body(canvas, (x, y), state, other.facing);
    }

    /// The character lost on a hit, if one is on its way out of the
    /// level, as the player is drawn in its state.
    fn draw_leaving(&self, canvas: &mut dyn Surface, alpha: f64, origin: (f64, f64)) {
        let Some(body) = self.world.leaving() else {
            return;
        };
        let previous = self.previous_leaving.unwrap_or(body.pos);
        let (x, y) = between(previous, body.pos, alpha, origin);
        if self.cast.has_player() {
            let (x, y) = (x as f32, y as f32);
            self.drawn_cast()
                .draw_leaving(canvas, &self.world, &self.poses.leaving, x, y);
            return;
        }
        draw_body(canvas, (x, y), body.state, body.facing);
    }
}

/// Where each of `world`'s things is, by its index, if it's in the level.
fn positions(world: &World) -> Vec<Option<Vec2>> {
    world
        .things
        .iter()
        .map(|t| t.active.then_some(t.pos))
        .collect()
}

/// The view position `alpha` of the way from `from` to `to`, for a view
/// whose top left is at `origin`.
fn between(from: Vec2, to: Vec2, alpha: f64, origin: (f64, f64)) -> (f64, f64) {
    let (from, to) = (point(from), point(to));
    (
        from.0 + (to.0 - from.0) * alpha - origin.0,
        from.1 + (to.1 - from.1) * alpha - origin.1,
    )
}

/// A character's body box with its feet at `at`, coloured by `state`.
fn draw_body(canvas: &mut dyn Surface, at: (f64, f64), state: State, facing: Facing) {
    let (x, y) = (at.0.round() as i32, at.1.round() as i32);
    let half = HALF_WIDTH.floor() as i32;
    let height = HEIGHT.floor() as i32;
    // Rolling bodies are drawn crouched so the state reads at a glance.
    let height = if state == State::Roll {
        height / 2
    } else {
        height
    };
    canvas.fill_rect(x - half, y - height, half * 2, height, state_color(state));
    let eye = match facing {
        Facing::Left => x - half + 2,
        Facing::Right => x + half - 5,
    };
    canvas.fill_rect(eye, y - height + 4, 3, 3, EYE);
}

fn state_color(state: State) -> Color {
    match state {
        State::Idle => rgb(0xf0, 0xe6, 0xd2),
        State::Walk => rgb(0xf0, 0xd0, 0x8a),
        State::Run => rgb(0xf0, 0xa0, 0x50),
        State::Roll => rgb(0xe0, 0x70, 0x40),
        State::Jump => rgb(0x8a, 0xd0, 0xf0),
        State::Fall => rgb(0x60, 0x9c, 0xe0),
        State::Swap => rgb(0xc0, 0x90, 0xf0),
        State::Bounce => rgb(0x70, 0xe0, 0xb0),
        State::Hurt => rgb(0xf0, 0x40, 0x40),
        State::GroundAction => rgb(0xd0, 0x80, 0xb0),
        State::Swim => rgb(0x50, 0xc8, 0xd0),
        State::Hold => rgb(0xa0, 0xb8, 0x70),
    }
}

fn outline(canvas: &mut dyn Surface, points: &[(f64, f64)], color: Color) {
    for (i, &a) in points.iter().enumerate() {
        canvas.line(a, points[(i + 1) % points.len()], color);
    }
}

/// `level`'s lighting for a view whose top left is at `origin`, `view`
/// units across, with the point lights that reach into it.
fn lighting(level: &clamber_level::Level, origin: (f64, f64), view: (u32, u32)) -> Lighting {
    let lights = level
        .lights
        .iter()
        .filter_map(|l| {
            let (x, y) = point(l.at);
            let (x, y) = ((x - origin.0) as f32, (y - origin.1) as f32);
            let radius = l.radius.to_f64() as f32;
            let reaches = x + radius > 0.0
                && y + radius > 0.0
                && x - radius < view.0 as f32
                && y - radius < view.1 as f32;
            reaches.then(|| PointLight {
                at: (x, y),
                radius,
                color: l.color.map(|c| (c as f64 / 255.0 * l.intensity) as f32),
            })
        })
        .collect();
    Lighting::new(&level.lighting, lights)
}

/// How many frames on a jump starts, with what's held now still held
/// and nothing new pressed: 1 if this frame is the one before it, T − 1,
/// which has a picture of its own ([`crate::pictures`]), and 2 for the
/// one before that, if the player is carrying something, whose jump
/// shows its picture from there. 0 if none does. Only a buffered Jump
/// press starts one.
pub(crate) fn jump_in(world: &World, input: &InputFrame) -> u8 {
    if !world.player.jump_buffer.is_buffered() {
        return 0;
    }
    let mut next = world.clone();
    let mut held = InputFrame::default();
    for (to, from) in held.players.iter_mut().zip(&input.players) {
        *to = clamber_input::Frame::after(from.held, from.held);
    }
    let carrying = world
        .player
        .hands
        .is_some_and(|h| h.act == clamber_sim::Act::Carry);
    for ahead in 1..=if carrying { 2 } else { 1 } {
        step(&mut next, &held);
        if next.events.contains(&clamber_sim::Event::JumpStart) {
            return ahead;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_level::Level;
    use clamber_sim::{Fixed, Outcome, generic_feel};

    use super::*;
    use crate::surface::{DrawList, Drawn};

    const LEVEL: &str = r#"
        format = 1
        size = [1000, 400]
        start = [500, 300]

        [[collision]]
        shape = "rect"
        rect = [0, 300, 1000, 100]

        [[entities]]
        type = "marker"
        at = [560, 280]
    "#;

    fn game() -> Game {
        let level = Level::from_toml_str(LEVEL, "test").unwrap();
        Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("default").unwrap()),
        ))
    }

    #[test]
    fn draws_the_level_and_player_in_view() {
        let game = game();
        assert!(game.debug);
        let mut canvas = Game::canvas();
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32);
        assert_eq!(at(500.0, 290.0), Some(state_color(State::Idle)));
        assert_eq!(at(500.0, 350.0), Some(SOLID));
        assert_eq!(at(500.0, 300.0), Some(EDGE));
        assert_eq!(at(560.0, 280.0), Some(ENTITY));
        assert_eq!(at(400.0, 250.0), Some(SKY));
    }

    #[test]
    fn hides_debug_geometry_when_asked() {
        let mut game = game();
        game.debug = false;
        let mut canvas = Game::canvas();
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32);
        assert_eq!(at(500.0, 350.0), Some(SKY));
        assert_eq!(at(500.0, 290.0), Some(state_color(State::Idle)));
    }

    #[test]
    fn draws_back_layers_behind_the_player_and_front_ones_over() {
        let layers = r#"
            [[layers]]
            name = "far"
            image = "far.png"
            parallax = [0, 0]
            repeat = [true, true]

            [[layers]]
            name = "near"
            depth = "front"
            image = "near.png"
            offset = [540, 250]
        "#;
        let src = format!("{LEVEL}{layers}");
        let level = Level::from_toml_str(&src, "test").unwrap();
        let fill = |w, h, c: [u8; 4]| {
            let mut image = clamber_sprite::RgbaImage::new(w, h);
            image.data.fill(c);
            image
        };
        let mut art = LayerArt::default();
        art.insert("far.png", fill(4, 4, [1, 2, 3, 255]), false);
        art.insert("near.png", fill(30, 30, [4, 5, 6, 255]), false);
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let mut game = Game::new(world).with_art(art);
        assert!(!game.debug);
        game.restart();
        let mut canvas = Game::canvas();
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32);
        assert_eq!(at(400.0, 250.0), Some(rgb(1, 2, 3)));
        assert_eq!(at(500.0, 290.0), Some(state_color(State::Idle)));
        assert_eq!(at(545.0, 255.0), Some(rgb(4, 5, 6)));
    }

    #[test]
    fn draws_each_kind_in_its_order_against_the_raised_tiles() {
        let src = format!(
            r#"{LEVEL}
            [[entities]]
            type = "tyre"
            at = [605, 290]

            [[entities]]
            type = "barrel"
            at = [615, 290]

            [[layers]]
            name = "rock"
            depth = "raised"
            image = "rock.png"
            offset = [600, 280]
            "#
        );
        let level = Level::from_toml_str(&src, "test").unwrap();
        let mut rock = clamber_sprite::RgbaImage::new(20, 20);
        rock.data.fill([7, 8, 9, 255]);
        let mut art = LayerArt::default();
        art.insert("rock.png", rock, false);
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let mut game = Game::new(world).with_art(art);
        game.debug = false;
        game.restart();
        let pixel = |game: &Game, x: f64, y: f64| {
            let mut canvas = Game::canvas();
            game.draw(&mut canvas, 1.0);
            let (left, top) = game.camera.top_left(1.0);
            canvas.get((x - left) as i32, (y - top) as i32)
        };
        let rock = Some(rgb(7, 8, 9));
        assert_eq!(pixel(&game, 605.0, 290.0), rock);
        assert_eq!(pixel(&game, 615.0, 290.0), Some(ENTITY));

        // At a level's end the order changes for the characters only.
        game.world.ending = Some(clamber_sim::Ending::default());
        assert_eq!(pixel(&game, 605.0, 290.0), rock);
        assert_eq!(pixel(&game, 615.0, 290.0), Some(ENTITY));
    }

    #[test]
    fn draws_the_characters_behind_the_scenery_in_the_walk_out_then_fades() {
        let layers = r#"
            [[layers]]
            name = "far"
            image = "far.png"
            parallax = [0, 0]
            repeat = [true, true]

            [[layers]]
            name = "rock"
            depth = "raised"
            image = "rock.png"
            offset = [490, 280]
        "#;
        let src = format!("{LEVEL}{layers}");
        let level = Level::from_toml_str(&src, "test").unwrap();
        let fill = |w, h, c: [u8; 4]| {
            let mut image = clamber_sprite::RgbaImage::new(w, h);
            image.data.fill(c);
            image
        };
        let mut art = LayerArt::default();
        art.insert("far.png", fill(4, 4, [1, 2, 3, 255]), false);
        art.insert("rock.png", fill(20, 20, [7, 8, 9, 255]), false);
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let mut game = Game::new(world).with_art(art);
        game.restart();
        let pixel = |game: &Game, x: f64, y: f64| {
            let mut canvas = Game::canvas();
            game.draw(&mut canvas, 1.0);
            let (left, top) = game.camera.top_left(1.0);
            canvas.get((x - left) as i32, (y - top) as i32)
        };
        assert_eq!(pixel(&game, 500.0, 285.0), Some(state_color(State::Idle)));

        game.world.ending = Some(clamber_sim::Ending::default());
        assert_eq!(pixel(&game, 500.0, 285.0), Some(rgb(7, 8, 9)));
        assert_eq!(pixel(&game, 500.0, 275.0), Some(state_color(State::Idle)));
        assert_eq!(pixel(&game, 400.0, 250.0), Some(rgb(1, 2, 3)));

        // An exit's walk-out hides them from its second frame.
        game.world.ending = None;
        let leaving = |frame| {
            Some(clamber_sim::Leaving {
                exit: 0,
                way: clamber_sim::Facing::Left,
                frame,
                fade: None,
            })
        };
        game.world.leaving = leaving(0);
        assert_eq!(pixel(&game, 500.0, 285.0), Some(state_color(State::Idle)));
        game.world.leaving = leaving(1);
        assert_eq!(pixel(&game, 500.0, 285.0), Some(rgb(7, 8, 9)));
        game.world.leaving = None;

        // Default's fade is a single frame, straight to black.
        game.world.ending = Some(clamber_sim::Ending {
            fade: Some(0),
            ..Default::default()
        });
        assert_eq!(pixel(&game, 400.0, 250.0), Some(BLACK));
    }

    #[test]
    fn draws_pixel_art_without_normal_maps_unlit_at_its_view() {
        let dim = r##"
            [lighting]
            ambient = "#202020"

            [[layers]]
            name = "wall"
            image = "wall.png"
            pixel_art = true
            parallax = [0, 0]
            repeat = [true, true]
        "##;
        let level = Level::from_toml_str(&format!("{LEVEL}{dim}"), "test").unwrap();
        let mut image = clamber_sprite::RgbaImage::new(4, 4);
        image.data.fill([100, 110, 120, 255]);
        let mut art = LayerArt::default();
        art.insert("wall.png", image, true);
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let look = Look {
            view: (256, 224),
            pixel_art: true,
            widescreen: Widescreen::OriginalFraming,
        };
        let mut game = Game::new(world).with_art(art).with_look(look);
        game.debug = false;
        let mut canvas = Canvas::new(256, 224);
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        assert_eq!(game.camera.view, look.view);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32).unwrap();
        // The dim ambient light doesn't touch it.
        assert_eq!(at(420.0, 220.0), rgb(100, 110, 120));
        assert_eq!(at(620.0, 330.0), rgb(100, 110, 120));
    }

    #[test]
    fn draws_replacement_art_in_enhanced_mode_only() {
        let tiles = r#"
            [[layers]]
            name = "wall"
            pixel_art = true
            parallax = [0, 0]
            tiles = { tileset = "tiles.png", tile_size = [8, 8], columns = 1, data = [1] }
        "#;
        let level = Level::from_toml_str(&format!("{LEVEL}{tiles}"), "test").unwrap();
        let mut image = clamber_sprite::RgbaImage::new(8, 8);
        image.data.fill([100, 110, 120, 255]);
        let mut art = LayerArt::default();
        art.insert("tiles.png", image.clone(), true);

        let dir = std::env::temp_dir().join(format!("clamber-replaced-{}", std::process::id()));
        let pictures = dir.join("hd").join(clamber_import::PICTURES);
        std::fs::create_dir_all(&pictures).unwrap();
        std::fs::write(dir.join("hd/art.toml"), "format = 1\nname = \"HD\"\n").unwrap();
        let all = clamber_sprite::Rect {
            x: 0,
            y: 0,
            w: 8,
            h: 8,
        };
        let hash = clamber_import::image_hash(&image, all).unwrap();
        let mut hd = clamber_sprite::RgbaImage::new(32, 32);
        hd.data.fill([1, 2, 3, 255]);
        let png = clamber_sprite::encode_png(&hd);
        std::fs::write(pictures.join(format!("{hash}.png")), png).unwrap();
        let (replacements, problems) = clamber_import::Replacements::load(&dir);
        assert!(problems.is_empty(), "{problems:?}");
        let replaced = art.replaced(&level, &replacements, &mut Vec::new());
        std::fs::remove_dir_all(&dir).unwrap();

        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let look = Look {
            view: (256, 224),
            pixel_art: true,
            widescreen: Widescreen::OriginalFraming,
        };
        let mut game = Game::new(world)
            .with_art(art)
            .with_replaced(replaced, None)
            .with_look(look);
        game.debug = false;
        let tile = |game: &Game| {
            let mut list = DrawList::new(look.view);
            game.draw(&mut list, 1.0);
            list.items
                .into_iter()
                .find_map(|d| match d {
                    Drawn::Quad(q) if q.texture.pixel_art => Some(q),
                    _ => None,
                })
                .unwrap()
        };
        let original = tile(&game);
        game.look.pixel_art = false;
        let enhanced = tile(&game);
        // The same place and size, from four times the texels.
        assert_eq!(enhanced.dst, original.dst);
        assert_eq!((original.texture.width(), original.src.w), (8, 8));
        assert_eq!((enhanced.texture.width(), enhanced.src.w), (32, 32));
        assert_eq!(enhanced.texture.levels[0].get(31, 31), [1, 2, 3, 255]);
    }

    #[test]
    fn lights_layers_that_have_normal_maps() {
        let lit = r##"
            [lighting]
            ambient = "#808080"
            key = { intensity = 0 }

            [[lights]]
            at = [400, 250]
            radius = 60

            [[lights]]
            at = [5000, 250]
            radius = 60

            [[layers]]
            name = "wall"
            image = "wall.png"
            normals = "wall-normals.png"
            parallax = [0, 0]
            repeat = [true, true]
        "##;
        let level = Level::from_toml_str(&format!("{LEVEL}{lit}"), "test").unwrap();
        let fill = |c: [u8; 4]| {
            let mut image = clamber_sprite::RgbaImage::new(4, 4);
            image.data.fill(c);
            image
        };
        let mut art = LayerArt::default();
        art.insert("wall.png", fill([100, 100, 100, 255]), false);
        // Every texel faces the viewer.
        art.insert("wall-normals.png", fill([128, 128, 255, 255]), false);
        let world = World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()));
        let game = Game::new(world).with_art(art);
        let mut canvas = Game::canvas();
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32).unwrap();
        // Under the light: half from the ambient and all of the light's.
        let under = at(400.0, 250.0) >> 16;
        assert!((148..=151).contains(&under), "{under}");
        // Out of its reach: only the ambient.
        assert_eq!(at(600.0, 250.0), rgb(50, 50, 50));
        let lighting = lighting(&game.world.level, (left, top), VIEW);
        assert_eq!(lighting.lights.len(), 1, "the far light is left out");
        assert_eq!(
            lighting.lights[0].at,
            ((400.0 - left) as f32, (250.0 - top) as f32)
        );
    }

    #[test]
    fn draws_sprites_with_a_cast_and_markers_without() {
        let cast = Cast::load(&crate::content_dir().join(crate::SPRITES)).unwrap();
        let mut game = game().with_cast(cast);
        game.debug = false;
        let mut canvas = Game::canvas();
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32);
        // The sprite covers the middle of the body, but not in its box
        // colour.
        let body = at(500.0, 290.0).unwrap();
        assert_ne!(body, SKY);
        assert_ne!(body, state_color(State::Idle));
        // `marker` has no sheet, so it's still a box.
        assert_eq!(at(560.0, 280.0), Some(ENTITY));
        for _ in 0..60 {
            game.step(&InputFrame::default());
        }
        assert_ne!(game.poses.player, Poses::default().player, "animates");
        game.restart();
        assert_eq!(game.poses, Poses::new(&game.cast, &game.world));
    }

    #[test]
    fn steps_the_world_and_camera() {
        let mut game = game();
        let input = InputFrame::single(clamber_input::Frame::after(
            clamber_input::Actions::NONE,
            clamber_input::Action::Right.into(),
        ));
        game.step(&input);
        assert_eq!(game.world.frame, 1);
        assert!(game.world.player.pos.x > game.previous.x);
        assert!(game.camera.centre.0 > game.camera.previous.0);
    }

    #[test]
    fn draws_the_other_character_and_swaps_without_drawing_between() {
        let level = Level::from_toml_str(LEVEL, "test").unwrap();
        let mut feel = generic_feel("classic-1").unwrap();
        feel.roster.push(feel.roster[0].clone());
        let mut game = Game::new(World::new(Arc::new(level), Arc::new(feel)));
        // The player walks off, and the other character follows it.
        let walk = InputFrame::single(clamber_input::Frame::after(
            clamber_input::Actions::NONE,
            clamber_input::Action::Right.into(),
        ));
        for _ in 0..40 {
            game.step(&walk);
        }
        game.step(&InputFrame::default());
        game.debug = false;
        let mut canvas = Game::canvas();
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32);
        let other = game.world.other.as_ref().unwrap().pos;
        assert!(other.x > Fixed::from_int(500), "{other:?}");
        let (x, y) = (other.x.to_f64(), other.y.to_f64());
        assert_eq!(at(x, y - 10.0), Some(state_color(State::Idle)));

        let swap = InputFrame::single(clamber_input::Frame::after(
            clamber_input::Actions::NONE,
            clamber_input::Action::Swap.into(),
        ));
        game.step(&swap);
        assert!(game.world.swap.is_some());
        while game.world.swap.is_some() {
            game.step(&InputFrame::default());
        }
        assert_eq!(game.world.player.character, 1);
        assert_eq!(game.previous, game.world.player.pos);
        assert_eq!(
            game.previous_other,
            game.world.other.as_ref().map(|o| o.pos)
        );
    }

    #[test]
    fn holds_the_screen_through_a_pause_in_a_knock_out() {
        // A walker walks into the player on classic-1.
        let src = LEVEL.replace(
            "type = \"marker\"\n        at = [560, 280]",
            "type = \"crawler\"\n        at = [530, 300]\n        params = { speed = 1 }",
        );
        let level = Level::from_toml_str(&src, "test").unwrap();
        let mut game = Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        ));
        let idle = InputFrame::default();
        while game.world.pause == 0 {
            assert!(game.step(&idle));
            assert!(game.world.frame < 60, "no pause");
        }
        assert!(game.world.player.knockout.is_some());
        let (frame, poses) = (game.world.frame, game.poses.clone());
        for _ in 0..32 {
            assert!(!game.step(&idle));
            assert_eq!(game.camera.previous, game.camera.centre);
        }
        assert_eq!(game.world.frame, frame);
        assert_eq!(game.poses, poses);
        assert!(game.step(&idle));
        assert_eq!(game.world.frame, frame + 1);
    }

    #[test]
    fn draws_things_between_steps_unless_they_jumped() {
        let src = LEVEL.replace(
            "type = \"marker\"\n        at = [560, 280]",
            "type = \"crawler\"\n        at = [700, 300]\n        params = { speed = 1 }",
        );
        let level = Level::from_toml_str(&src, "test").unwrap();
        let mut game = Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        ));
        let idle = InputFrame::default();
        let was = game.world.things[0].pos;
        while game.world.things[0].pos == was {
            game.step(&idle);
            assert!(game.world.frame < 60, "the walker doesn't move");
        }
        let thing = &game.world.things[0];
        let (from, to) = (point(was), point(thing.pos));
        assert_eq!(game.thing_at(thing, 0.0), from);
        assert_eq!(game.thing_at(thing, 1.0), to);
        let half = game.thing_at(thing, 0.5);
        assert_eq!(half, ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0));
        assert_ne!(half, to);

        // Put somewhere else in one jump: drawn where it is.
        let mut moved = thing.clone();
        moved.pos = Vec2::new(moved.pos.x + Fixed::from_int(100), moved.pos.y);
        assert_eq!(game.thing_at(&moved, 0.5), point(moved.pos));
        // Not in the level before the step: drawn where it is.
        game.previous_things[0] = None;
        let thing = &game.world.things[0];
        assert_eq!(game.thing_at(thing, 0.5), point(thing.pos));
    }

    #[test]
    fn goes_on_at_the_arrival_an_exit_leads_to() {
        let src = format!(
            r#"{LEVEL}
        [[entities]]
        type = "exit"
        at = [100, 300]
        params = {{ arrival = "back" }}

        [[arrivals]]
        name = "back"
        at = [800, 300]
        walk = "left"
        "#
        );
        let level = Level::from_toml_str(&src, "test").unwrap();
        let mut game = Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        ));
        game.debug = false;
        game.step(&InputFrame::default());
        game.leave_by(1);
        assert_eq!(game.world.frame, 0);
        assert!(game.world.walking_in.is_some());
        assert_eq!(game.world.player.pos, clamber_level::Vec2::ints(880, 300));
        assert!(!game.debug, "keeps the debug view setting");
        // An exit that names no arrival leads back to the level's start.
        game.leave_by(0);
        assert!(game.world.walking_in.is_none());
        assert_eq!(game.world.player.pos, game.world.level.start);
    }

    #[test]
    fn restarts_after_the_last_life() {
        // Nothing to stand on, so every life falls out of the level.
        let src = LEVEL.replace("rect = [0, 300, 1000, 100]", "rect = [0, 0, 10, 10]");
        let level = Level::from_toml_str(&src, "test").unwrap();
        let mut game = Game::new(World::new(
            Arc::new(level),
            Arc::new(generic_feel("default").unwrap()),
        ));
        game.debug = false;
        let idle = InputFrame::default();
        let mut respawned = false;
        while game.world.flow.outcome != Outcome::GameOver {
            game.step(&idle);
            if game.world.flow.events.contains(&FlowEvent::Respawn) {
                respawned = true;
                assert_eq!(game.previous, game.world.level.start);
                assert_eq!(game.previous_things, positions(&game.world));
            }
        }
        assert!(respawned);
        let frame = game.world.frame;
        game.step(&idle);
        assert_eq!(game.world.frame, frame, "stays over");
        game.restart();
        assert_eq!(game.world.frame, 0);
        assert_eq!(game.world.flow.lives, clamber_sim::START_LIVES);
        assert!(!game.debug, "keeps the debug view setting");
    }

    /// Flat ground at y = 400, the player at x = 300 and a walker
    /// starting at x = 600, on `classic-1`, whose view is 256 px wide: the
    /// walker is past it, but inside a window 512 px wide.
    fn wide(widescreen: Widescreen) -> Game {
        let src = r#"
            format = 1
            size = [4000, 600]
            start = [300, 400]

            [[collision]]
            shape = "rect"
            rect = [0, 400, 4000, 200]

            [[entities]]
            type = "crawler"
            at = [600, 400]
            params = { speed = 1, facing = "left", size = [20, 12], offset = 0.5, response = 0.125, turn_delay = 4, appear_wait = 0, defeat_jump = 6, defeat_gravity = 0.4375, defeat_max_fall = 8 }
        "#;
        // More further on, for the view to bring in.
        let crawler = &src[src.find("[[entities]]").unwrap()..];
        let mut src = src.to_string();
        for x in [1500, 2500, 3500] {
            src.push_str(&crawler.replace("600, 400", &format!("{x}, 400")));
        }
        let level = Level::from_toml_str(&src, "test").unwrap();
        let world = World::new(
            Arc::new(level),
            Arc::new(generic_feel("classic-1").unwrap()),
        );
        let look = Look {
            view: (512, 224),
            pixel_art: true,
            widescreen,
        };
        Game::new(world).with_look(look)
    }

    #[test]
    fn original_activation_draws_what_would_appear_where_it_will() {
        let mut game = wide(Widescreen::OriginalActivation);
        assert_eq!(game.shown().view, (512, 224), "the whole window");
        assert!(!game.world.things[0].active, "not in the level");
        let waiting: Vec<_> = game.beyond.waiting.iter().map(|t| t.pos.x).collect();
        assert_eq!(waiting, [Fixed::from_int(600)]);
        let mut canvas = Canvas::new(512, 224);
        game.draw(&mut canvas, 1.0);
        let (left, top) = game.camera.top_left(1.0);
        let at = |x: f64, y: f64| canvas.get((x - left) as i32, (y - top) as i32);
        assert_eq!(at(600.0, 400.0), Some(ENTITY), "drawn at its start");

        // It plays exactly as in the original's view.
        let mut framed = wide(Widescreen::OriginalFraming);
        assert_eq!(framed.shown().view, (256, 224));
        assert!(framed.beyond.waiting.is_empty());
        let mut held = clamber_input::Actions::NONE;
        for frame in 0..120 {
            let now = clamber_input::Action::Right.into();
            let input = InputFrame::single(clamber_input::Frame::after(held, now));
            held = now;
            game.step(&input);
            framed.step(&input);
            assert_eq!(
                game.world.state_bytes(),
                framed.world.state_bytes(),
                "{frame}"
            );
            if game.world.things[0].active {
                assert!(game.beyond.waiting.is_empty(), "{frame}: in the level");
            }
        }
        assert!(game.world.things[0].active, "the view brought it in");
    }

    #[test]
    fn wide_activation_brings_things_in_from_outside_the_window() {
        let mut game = wide(Widescreen::WideActivation);
        assert_eq!(game.shown().view, (512, 224), "the whole window");
        assert_eq!(game.world.feel.window, Some(512), "things go by it");
        game.restart();
        assert!(game.world.things[0].active, "in the window at the start");
        // Out of the way of the run.
        game.world.things[0].active = false;
        game.world.things[0].phase = clamber_sim::Phase::Gone;
        let mut held = clamber_input::Actions::NONE;
        let mut came = 0;
        for frame in 0..1200 {
            let before: Vec<bool> = game.world.things.iter().map(|t| t.active).collect();
            let was_at = game.world.window().map(|(left, _)| left);
            let now = clamber_input::Action::Right.into();
            let input = InputFrame::single(clamber_input::Frame::after(held, now));
            held = now;
            game.step(&input);
            let (left, width) = game.world.window().expect("a window");
            assert_eq!(
                game.camera.top_left(1.0).0,
                left as f64,
                "{frame}: drawn there"
            );
            if was_at.is_some_and(|was| (left - was).abs() > 16) {
                // Coming back after losing a life starts the view
                // afresh: a cut.
                continue;
            }
            let shown = left as f64..(left + width) as f64;
            for (thing, was) in game.world.things.iter().zip(before) {
                if thing.active && !was {
                    came += 1;
                    assert!(!shown.contains(&thing.pos.x.to_f64()), "{frame}: popped in");
                }
            }
        }
        assert!(came > 0, "things came in");
    }
}
