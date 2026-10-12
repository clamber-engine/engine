//! A level being played: the world, its camera, and drawing both.

use std::sync::Arc;

use clamber_level::{Area, Depth, Shape, Vec2};
use clamber_sim::{Facing, FlowEvent, HALF_WIDTH, HEIGHT, InputFrame, State, World, step};

use crate::camera::{Camera, VIEW, point};
use crate::canvas::{Canvas, Color, rgb};
use crate::hud::draw_hud;
use crate::layers::LayerArt;

const SKY: Color = rgb(0x2a, 0x34, 0x4a);
const SOLID: Color = rgb(0x6b, 0x74, 0x80);
const EDGE: Color = rgb(0xc8, 0xd0, 0xd8);
const ONE_WAY: Color = rgb(0x6c, 0xd4, 0x8a);
const HAZARD: Color = rgb(0xe0, 0x5a, 0x4a);
const REGION: Color = rgb(0xd8, 0xb0, 0x4a);
const WATER: Color = rgb(0x3a, 0x7a, 0xd8);
const ENTITY: Color = rgb(0xd0, 0x6a, 0xd8);
const EYE: Color = rgb(0x10, 0x10, 0x18);

/// Size of the box marking an entity until entities have sprites, px.
const ENTITY_BOX: i32 = 12;

pub struct Game {
    pub world: World,
    pub camera: Camera,
    /// The player's position before the last step, for interpolation.
    previous: Vec2,
    /// Draw collision geometry, regions and entity markers.
    pub debug: bool,
    /// The images the level's layers draw.
    art: Arc<LayerArt>,
}

impl Game {
    /// Starts playing `world`. The debug view starts on when the level
    /// has no layers, since there'd be nothing else to see.
    pub fn new(world: World) -> Game {
        Game {
            camera: Camera::new(&world),
            previous: world.player.pos,
            debug: world.level.layers.is_empty(),
            world,
            art: Arc::default(),
        }
    }

    /// Draws the level's layers with `art`.
    pub fn with_art(self, art: LayerArt) -> Game {
        Game {
            art: Arc::new(art),
            ..self
        }
    }

    /// Starts the level over, keeping the debug view setting.
    pub fn restart(&mut self) {
        let world = World::new(self.world.level.clone(), self.world.feel.clone());
        *self = Game {
            debug: self.debug,
            art: self.art.clone(),
            ..Game::new(world)
        };
    }

    /// Advances one logic frame. Once the level is complete or over, the
    /// world stays as it is until [`restart`](Game::restart).
    pub fn step(&mut self, input: &InputFrame) {
        self.previous = self.world.player.pos;
        step(&mut self.world, input);
        if self.world.flow.events.contains(&FlowEvent::Respawn) {
            // A jump to somewhere else, not a move to draw between.
            self.previous = self.world.player.pos;
            self.camera = Camera::new(&self.world);
        } else {
            self.camera.update(&self.world);
        }
    }

    /// A new canvas the size of the view.
    pub fn canvas() -> Canvas {
        Canvas::new(VIEW.0, VIEW.1)
    }

    /// Draws the world, and the HUD over it, `alpha` of the way from the
    /// state before the last step to the state after it.
    pub fn draw(&self, canvas: &mut Canvas, alpha: f64) {
        let origin = self.camera.top_left(alpha);
        let to_screen = |p: (f64, f64)| (p.0 - origin.0, p.1 - origin.1);
        canvas.clear(SKY);

        let level = &self.world.level;
        self.art.draw(canvas, level, Depth::Back, origin);
        if self.debug {
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
                    Shape::Solid(points) => {
                        let points: Vec<_> = points.iter().map(|&p| to_screen(point(p))).collect();
                        canvas.fill_polygon(&points, SOLID, 255);
                        outline(canvas, &points, if hazard { HAZARD } else { EDGE });
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
            for thing in self.world.things.iter().filter(|t| t.active) {
                let (x, y) = to_screen(point(thing.pos));
                let (x, y) = (x.round() as i32, y.round() as i32);
                let half = ENTITY_BOX / 2;
                canvas.stroke_rect(x - half, y - half, ENTITY_BOX, ENTITY_BOX, ENTITY);
                canvas.plot(x, y, ENTITY);
            }
        }

        self.draw_player(canvas, alpha, origin);
        self.art.draw(canvas, level, Depth::Front, origin);
        draw_hud(canvas, &self.world);
    }

    /// The player as its body box, coloured by state, until it has
    /// sprites.
    fn draw_player(&self, canvas: &mut Canvas, alpha: f64, origin: (f64, f64)) {
        let player = &self.world.player;
        let (from, to) = (point(self.previous), point(player.pos));
        let x = (from.0 + (to.0 - from.0) * alpha - origin.0).round() as i32;
        let y = (from.1 + (to.1 - from.1) * alpha - origin.1).round() as i32;
        let half = HALF_WIDTH.floor() as i32;
        let height = HEIGHT.floor() as i32;
        // Rolling bodies are drawn crouched so the state reads at a glance.
        let height = if player.state == State::Roll {
            height / 2
        } else {
            height
        };
        canvas.fill_rect(
            x - half,
            y - height,
            half * 2,
            height,
            state_color(player.state),
        );
        let eye = match player.facing {
            Facing::Left => x - half + 2,
            Facing::Right => x + half - 5,
        };
        canvas.fill_rect(eye, y - height + 4, 3, 3, EYE);
    }
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
    }
}

fn outline(canvas: &mut Canvas, points: &[(f64, f64)], color: Color) {
    for (i, &a) in points.iter().enumerate() {
        canvas.line(a, points[(i + 1) % points.len()], color);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_level::Level;
    use clamber_sim::{Outcome, generic_feel};

    use super::*;

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
        art.insert("far.png", fill(4, 4, [1, 2, 3, 255]));
        art.insert("near.png", fill(30, 30, [4, 5, 6, 255]));
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
}
