//! The camera: follows the player within the level's camera rules.
//!
//! The camera is presentation: it reads the world after each step and is
//! never read by the simulation, so it works in floats.

use clamber_level::{CameraRule, Fixed, Level, Rect, Vec2};
use clamber_sim::{Facing, HEIGHT, World};

/// The view's size in world units. It's drawn at any size, so the
/// window's resolution doesn't change how much of the world is seen.
pub const VIEW: (u32, u32) = (384, 216);

/// How fast the look-ahead offset moves to where it's headed, px/frame.
const LOOK_SPEED: f64 = 2.0;

/// Vertical speed beyond which the camera looks ahead vertically, px/frame.
const LOOK_Y_SPEED: f64 = 1.0;

/// The view only moves vertically once the player is this far from where
/// it's looking, px, so hops don't bob the screen.
const BAND_Y: f64 = 40.0;

/// How fast the view settles back on a grounded player vertically,
/// px/frame.
const SETTLE_SPEED: f64 = 3.0;

/// Frames to ease between rules when the player crosses into a region with
/// different ones.
const TRANSITION_FRAMES: u32 = 30;

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    /// The middle of the view, in world pixels.
    pub centre: (f64, f64),
    /// Where it was before the last update, for interpolation.
    pub previous: (f64, f64),
    look: (f64, f64),
    /// The vertical position followed before rules apply.
    follow_y: f64,
    rule: CameraRule,
    /// The rule being left and frames into the move away from it.
    leaving: Option<(CameraRule, u32)>,
}

impl Camera {
    /// A camera already where it belongs for the world as it is.
    pub fn new(world: &World) -> Camera {
        let rule = rule_at(world);
        let look = (look_target(world, &rule).0, 0.0);
        let free = offset(focus(world), look);
        let centre = apply(&rule, &world.level, free);
        Camera {
            centre,
            previous: centre,
            look,
            follow_y: free.1,
            rule,
            leaving: None,
        }
    }

    /// Follows the world after a step.
    pub fn update(&mut self, world: &World) {
        self.previous = self.centre;

        let rule = rule_at(world);
        if rule != self.rule {
            self.leaving = Some((self.rule, 0));
            self.rule = rule;
        }

        let target = look_target(world, &self.rule);
        self.look = (
            approach(self.look.0, target.0, LOOK_SPEED),
            approach(self.look.1, target.1, LOOK_SPEED),
        );
        let wanted = offset(focus(world), self.look);
        if world.player.grounded() {
            self.follow_y = approach(self.follow_y, wanted.1, SETTLE_SPEED);
        }
        self.follow_y = self.follow_y.clamp(wanted.1 - BAND_Y, wanted.1 + BAND_Y);
        let free = (wanted.0, self.follow_y);
        let centre = apply(&self.rule, &world.level, free);

        self.centre = match &mut self.leaving {
            Some((old, frame)) => {
                *frame += 1;
                let t = ease(*frame as f64 / TRANSITION_FRAMES as f64);
                let from = apply(old, &world.level, free);
                if *frame >= TRANSITION_FRAMES {
                    self.leaving = None;
                }
                (lerp(from.0, centre.0, t), lerp(from.1, centre.1, t))
            }
            None => centre,
        };
    }

    /// The view's top-left corner in world units, `alpha` of the way from
    /// the previous update to the last. It isn't snapped to whole units,
    /// so the view scrolls smoothly at any resolution.
    pub fn top_left(&self, alpha: f64) -> (f64, f64) {
        let x = lerp(self.previous.0, self.centre.0, alpha);
        let y = lerp(self.previous.1, self.centre.1, alpha);
        (x - VIEW.0 as f64 / 2.0, y - VIEW.1 as f64 / 2.0)
    }
}

fn rule_at(world: &World) -> CameraRule {
    world.level.camera_at(world.player.pos)
}

/// The point the camera centres on before look-ahead: the middle of the
/// player's body.
fn focus(world: &World) -> (f64, f64) {
    let p = world.player.pos;
    (p.x.to_f64(), (p.y - half(HEIGHT)).to_f64())
}

fn half(v: Fixed) -> Fixed {
    Fixed::from_bits(v.to_bits() / 2)
}

/// Where the look-ahead offset is headed: ahead of the player's facing,
/// and of its vertical motion while that's fast.
fn look_target(world: &World, rule: &CameraRule) -> (f64, f64) {
    let ahead = rule.look_ahead.unwrap_or_default();
    let x = match world.player.facing {
        Facing::Left => -ahead.x.to_f64(),
        Facing::Right => ahead.x.to_f64(),
    };
    let vy = world.player.vel.y.to_f64();
    let y = if vy.abs() > LOOK_Y_SPEED {
        ahead.y.to_f64().copysign(vy)
    } else {
        0.0
    };
    (x, y)
}

fn offset(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}

/// The view's centre under `rule` for a camera that would like to be at
/// `free`: locks fix an axis, then the view stays inside the limits (the
/// level's bounds unless the rule sets its own).
fn apply(rule: &CameraRule, level: &Level, free: (f64, f64)) -> (f64, f64) {
    let limits = rule.limits.unwrap_or(Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: level.size.x,
        h: level.size.y,
    });
    let x = rule.lock_x.map_or(free.0, Fixed::to_f64);
    let y = rule.lock_y.map_or(free.1, Fixed::to_f64);
    (
        clamp_view(x, limits.x, limits.w, VIEW.0),
        clamp_view(y, limits.y, limits.h, VIEW.1),
    )
}

/// Keeps a view `size` wide centred at `c` within `start..start + len`,
/// or centres it there if it doesn't fit.
fn clamp_view(c: f64, start: Fixed, len: Fixed, size: u32) -> f64 {
    let (start, len, half) = (start.to_f64(), len.to_f64(), size as f64 / 2.0);
    if len <= size as f64 {
        start + len / 2.0
    } else {
        c.clamp(start + half, start + len - half)
    }
}

fn approach(from: f64, to: f64, step: f64) -> f64 {
    from + (to - from).clamp(-step, step)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Smoothstep: starts and ends gently.
fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A point in world pixels as a float pair, for drawing.
pub fn point(v: Vec2) -> (f64, f64) {
    (v.x.to_f64(), v.y.to_f64())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clamber_input::Action;
    use clamber_sim::{InputFrame, generic_feel, step};

    use super::*;

    fn world(src: &str) -> World {
        let level = Level::from_toml_str(src, "test").unwrap();
        World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
    }

    fn run(world: &mut World, camera: &mut Camera, held: Option<Action>, frames: u32) {
        let mut previous = clamber_input::Actions::NONE;
        for _ in 0..frames {
            let now = held.map_or(clamber_input::Actions::NONE, Into::into);
            step(
                world,
                &InputFrame::single(clamber_input::Frame::after(previous, now)),
            );
            previous = now;
            camera.update(world);
        }
    }

    const WIDE: &str = r#"
        format = 1
        size = [4000, 1000]
        start = [2000, 900]

        [[collision]]
        shape = "rect"
        rect = [0, 900, 4000, 100]

        [[regions]]
        rect = [3000, 0, 1000, 1000]
        camera = { lock_y = 300 }
    "#;

    #[test]
    fn centres_on_the_player_body() {
        let w = world(WIDE);
        let camera = Camera::new(&w);
        assert_eq!(camera.centre, (2000.0, 884.0));
        assert_eq!(camera.top_left(1.0), (2000.0 - 192.0, 884.0 - 108.0));
    }

    #[test]
    fn stays_inside_the_level() {
        let mut w = world(WIDE);
        w.player.pos = Vec2::ints(10, 1000);
        let camera = Camera::new(&w);
        assert_eq!(camera.centre, (192.0, 1000.0 - 108.0));
    }

    #[test]
    fn centres_levels_smaller_than_the_view() {
        let w = world("format = 1\nsize = [200, 100]\nstart = [10, 10]");
        assert_eq!(Camera::new(&w).centre, (100.0, 50.0));
    }

    #[test]
    fn eases_into_a_regions_rules() {
        let mut w = world(WIDE);
        let mut camera = Camera::new(&w);
        w.player.pos = Vec2::ints(3500, 900);
        camera.update(&w);
        let first = camera.centre.1;
        assert!(first < 892.0 && first > 300.0, "{first}");
        for _ in 0..TRANSITION_FRAMES {
            camera.update(&w);
        }
        assert_eq!(camera.centre, (3500.0, 300.0));
    }

    #[test]
    fn looks_ahead_of_the_facing() {
        let src = WIDE.replace("[[regions]]", "[camera]\nlook_ahead = [40, 0]\n[[regions]]");
        let mut w = world(&src);
        let mut camera = Camera::new(&w);
        assert_eq!(camera.centre.0, 2040.0);
        run(&mut w, &mut camera, Some(Action::Left), 1);
        run(&mut w, &mut camera, None, 60);
        assert_eq!(camera.centre.0, w.player.pos.x.to_f64() - 40.0);
    }

    #[test]
    fn follows_vertically_outside_a_band_and_settles_on_landing() {
        let mut w = world(WIDE);
        let mut camera = Camera::new(&w);
        let start = camera.centre.1;
        w.player.ground = None;
        w.player.pos = Vec2::ints(2000, 870);
        camera.update(&w);
        assert_eq!(camera.centre.1, start);
        w.player.pos = Vec2::ints(2000, 800);
        camera.update(&w);
        assert_eq!(camera.centre.1, 784.0 + BAND_Y);

        let mut w = world(WIDE);
        run(&mut w, &mut camera, None, 60);
        assert!(w.player.grounded());
        assert_eq!(camera.centre.1, start);
    }

    #[test]
    fn interpolates_between_updates() {
        let mut w = world(WIDE);
        let mut camera = Camera::new(&w);
        w.player.pos = Vec2::ints(2010, 900);
        camera.update(&w);
        assert_eq!(camera.top_left(0.0).0, 2000.0 - 192.0);
        assert_eq!(camera.top_left(0.5).0, 2005.0 - 192.0);
        assert_eq!(camera.top_left(1.0).0, 2010.0 - 192.0);
        assert_eq!(camera.top_left(0.25).0, 2002.5 - 192.0);
    }
}
