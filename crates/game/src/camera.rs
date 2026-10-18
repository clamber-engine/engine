//! The camera: follows the player within the level's camera rules.
//!
//! The camera is presentation: it reads the world after each step and is
//! never read by the simulation, so it works in floats. When the world
//! has a view of its own ([`World::view_middle`]), the camera centres on
//! it across instead of following the player itself, and up and down too
//! when that view moves vertically ([`World::view_middle_y`]). With a
//! window wider than that view that things go by ([`World::window`]),
//! it shows exactly that window across.

use clamber_level::{CameraRule, Fixed, Level, Rect, Vec2};
use clamber_sim::{Facing, HEIGHT, World};

/// The view's size in world units, unless a pack's [`Look`](crate::Look)
/// sets its own. It's drawn at any size, so the window's resolution
/// doesn't change how much of the world is seen.
pub const VIEW: (u32, u32) = (384, 216);

/// How fast the look-ahead offset moves to where it's headed, px/frame.
const LOOK_SPEED: f64 = 2.0;

/// Vertical speed beyond which the camera looks ahead vertically, px/frame.
const LOOK_Y_SPEED: f64 = 1.0;

/// Frames of the player's motion the view looks further ahead by, so
/// there's time to react to what's coming at speed.
const LEAD_FRAMES: f64 = 12.0;

/// The most the lead adds to the look-ahead, px, across and vertically.
const MAX_LEAD: (f64, f64) = (80.0, 40.0);

/// A move further than this in one frame, px, is a jump to a new place
/// (a restart, say), not motion to look ahead of.
const MAX_MOTION: f64 = 24.0;

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
    /// Where the player's body was at the last update.
    last: (f64, f64),
    rule: CameraRule,
    /// The rule being left and frames into the move away from it.
    leaving: Option<(CameraRule, u32)>,
    /// The size of the view it keeps within the level, in world units.
    pub view: (u32, u32),
}

impl Camera {
    /// A camera already where it belongs for the world as it is, with a
    /// [`VIEW`]-sized view.
    pub fn new(world: &World) -> Camera {
        Camera::with_view(world, VIEW)
    }

    /// A camera already where it belongs for the world as it is, with a
    /// view `view` units in size.
    pub fn with_view(world: &World, view: (u32, u32)) -> Camera {
        let rule = rule_at(world);
        let look = (look_target(world, &rule, (0.0, 0.0)).0, 0.0);
        let free = across(world, offset(focus(world), look));
        let centre = windowed(world, apply(&rule, &world.level, free, view));
        Camera {
            centre,
            previous: centre,
            look,
            follow_y: free.1,
            last: focus(world),
            rule,
            leaving: None,
            view,
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

        let now = focus(world);
        let motion = (now.0 - self.last.0, now.1 - self.last.1);
        self.last = now;
        let motion = if motion.0.abs().max(motion.1.abs()) > MAX_MOTION {
            (0.0, 0.0)
        } else {
            motion
        };

        let target = look_target(world, &self.rule, motion);
        self.look = (
            approach(self.look.0, target.0, LOOK_SPEED),
            approach(self.look.1, target.1, LOOK_SPEED),
        );
        let wanted = across(world, offset(focus(world), self.look));
        if world.view_middle_y().is_some() {
            self.follow_y = wanted.1;
        } else if world.player.grounded() {
            // At least as fast as the player goes up or down a slope.
            let speed = SETTLE_SPEED + motion.1.abs();
            self.follow_y = approach(self.follow_y, wanted.1, speed);
        }
        self.follow_y = self.follow_y.clamp(wanted.1 - BAND_Y, wanted.1 + BAND_Y);
        let free = (wanted.0, self.follow_y);
        let centre = apply(&self.rule, &world.level, free, self.view);

        let centre = match &mut self.leaving {
            Some((old, frame)) => {
                *frame += 1;
                let t = ease(*frame as f64 / TRANSITION_FRAMES as f64);
                let from = apply(old, &world.level, free, self.view);
                if *frame >= TRANSITION_FRAMES {
                    self.leaving = None;
                }
                (lerp(from.0, centre.0, t), lerp(from.1, centre.1, t))
            }
            None => centre,
        };
        self.centre = windowed(world, centre);
    }

    /// The view's top-left corner in world units, `alpha` of the way from
    /// the previous update to the last. It isn't snapped to whole units,
    /// so the view scrolls smoothly at any resolution.
    pub fn top_left(&self, alpha: f64) -> (f64, f64) {
        self.top_left_of(self.view, alpha)
    }

    /// The top-left corner of a view `view` units in size around the same
    /// centre as [`top_left`](Camera::top_left)'s, for drawing somewhere
    /// that shows more or less than the camera's own view.
    pub fn top_left_of(&self, view: (u32, u32), alpha: f64) -> (f64, f64) {
        let x = lerp(self.previous.0, self.centre.0, alpha);
        let y = lerp(self.previous.1, self.centre.1, alpha);
        (x - view.0 as f64 / 2.0, y - view.1 as f64 / 2.0)
    }
}

/// `free` with the world's own view's middle across, if it has a view,
/// and up and down, if that view moves vertically.
fn across(world: &World, free: (f64, f64)) -> (f64, f64) {
    let x = world.view_middle().map_or(free.0, Fixed::to_f64);
    let y = world.view_middle_y().map_or(free.1, Fixed::to_f64);
    (x, y)
}

/// `centre` with the middle of the world's window across, if it has
/// one ([`World::window`]): things appear, act and are removed by that
/// window, so it's drawn exactly where it is.
fn windowed(world: &World, centre: (f64, f64)) -> (f64, f64) {
    match world.window() {
        Some((left, width)) => (left as f64 + width as f64 / 2.0, centre.1),
        None => centre,
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
/// and of its vertical motion while that's fast, whether in the air or
/// along a slope. Without a view of the world's own, it leads by
/// [`LEAD_FRAMES`] of the player's `motion` this frame too, so the
/// faster the player goes the further ahead it looks: across always,
/// and vertically on the ground, up or down a slope (in the air that
/// would bob the view with every hop).
fn look_target(world: &World, rule: &CameraRule, motion: (f64, f64)) -> (f64, f64) {
    let ahead = rule.look_ahead.unwrap_or_default();
    let x = match world.player.facing {
        Facing::Left => -ahead.x.to_f64(),
        Facing::Right => ahead.x.to_f64(),
    };
    let y = if motion.1.abs() > LOOK_Y_SPEED {
        ahead.y.to_f64().copysign(motion.1)
    } else {
        0.0
    };
    if world.view.is_some() {
        return (x, y);
    }
    let lead = |v: f64, max: f64| (v * LEAD_FRAMES).clamp(-max, max);
    let lead_y = if world.player.grounded() {
        lead(motion.1, MAX_LEAD.1)
    } else {
        0.0
    };
    (x + lead(motion.0, MAX_LEAD.0), y + lead_y)
}

fn offset(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 + b.0, a.1 + b.1)
}

/// The view's centre under `rule` for a camera that would like to be at
/// `free`: locks fix an axis, then a view `view` in size stays inside the
/// limits (the level's bounds unless the rule sets its own).
fn apply(rule: &CameraRule, level: &Level, free: (f64, f64), view: (u32, u32)) -> (f64, f64) {
    let limits = rule.limits.unwrap_or(Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: level.size.x,
        h: level.size.y,
    });
    let x = rule.lock_x.map_or(free.0, Fixed::to_f64);
    let y = rule.lock_y.map_or(free.1, Fixed::to_f64);
    (
        clamp_view(x, limits.x, limits.w, view.0),
        clamp_view(y, limits.y, limits.h, view.1),
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

    use clamber_input::{Action, Actions};
    use clamber_sim::{InputFrame, generic_feel, step};

    use super::*;

    fn world(src: &str) -> World {
        let level = Level::from_toml_str(src, "test").unwrap();
        World::new(Arc::new(level), Arc::new(generic_feel("default").unwrap()))
    }

    fn run(world: &mut World, camera: &mut Camera, held: Option<Action>, frames: u32) {
        let held = held.map_or(Actions::NONE, Into::into);
        run_holding(world, camera, held, frames);
    }

    fn run_holding(world: &mut World, camera: &mut Camera, now: Actions, frames: u32) {
        let mut previous = Actions::NONE;
        for _ in 0..frames {
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
    fn keeps_a_view_of_any_size_inside_the_level() {
        let w = world(&WIDE.replace("start = [2000, 900]", "start = [10, 900]"));
        let camera = Camera::new(&w);
        assert_eq!(camera.centre, (192.0, 884.0));
        let camera = Camera::with_view(&w, (256, 224));
        assert_eq!(camera.centre, (128.0, 884.0));
        assert_eq!(camera.top_left(1.0), (0.0, 884.0 - 112.0));
        // Drawn somewhere that shows more, around the same centre.
        assert_eq!(camera.top_left_of((384, 216), 1.0), (-64.0, 884.0 - 108.0));
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
    fn centres_on_the_worlds_own_view_across() {
        let level = Level::from_toml_str(WIDE, "test").unwrap();
        let feel = generic_feel("classic-1").unwrap();
        let mut w = World::new(Arc::new(level), Arc::new(feel));
        let mut camera = Camera::with_view(&w, (256, 224));
        // A player standing facing right is 71 px from the left edge.
        assert_eq!(camera.top_left(1.0).0, 2000.0 - 71.0);
        run(&mut w, &mut camera, Some(Action::Right), 60);
        let middle = w.view_middle().unwrap().to_f64();
        assert_eq!(camera.centre.0, middle);
        assert!(middle - 128.0 > 2000.0 - 71.0);
        // Standing on the floor at 900, the view's top is 160 px above
        // the feet, rounded down from 740.5.
        assert_eq!(camera.top_left(1.0).1, 740.0);
        assert_eq!(camera.centre.1, w.view_middle_y().unwrap().to_f64());
    }

    #[test]
    fn interpolates_between_updates() {
        let mut w = world(WIDE);
        let mut camera = Camera::new(&w);
        w.player.pos = Vec2::ints(2010, 900);
        camera.update(&w);
        // Moving 10 px also starts the look-ahead 2 px that way.
        assert_eq!(camera.top_left(0.0).0, 2000.0 - 192.0);
        assert_eq!(camera.top_left(0.5).0, 2006.0 - 192.0);
        assert_eq!(camera.top_left(1.0).0, 2012.0 - 192.0);
        assert_eq!(camera.top_left(0.25).0, 2003.0 - 192.0);
    }

    const DOWNHILL: &str = r#"
        format = 1
        size = [4000, 3000]
        start = [200, 400]

        [camera]
        look_ahead = [32, 24]

        [[collision]]
        shape = "polygon"
        points = [[0, 400], [400, 400], [2400, 1400], [2400, 3000], [0, 3000]]
    "#;

    #[test]
    fn looks_further_ahead_and_down_running_fast_downhill() {
        let mut w = world(DOWNHILL);
        let mut camera = Camera::new(&w);
        let run_right = Actions::from(Action::Right) | Action::Action.into();
        run_holding(&mut w, &mut camera, run_right, 120);
        assert!(w.player.grounded());
        let (x, y) = focus(&w);
        assert!(y > 500.0, "{y}");
        // At top speed (5 px/frame across, 2.5 down): 12 frames of that
        // ahead of the 32 and 24 the level asks for.
        assert_eq!(camera.centre, (x + 32.0 + 60.0, y + 24.0 + 30.0));
    }

    #[test]
    fn looks_only_as_far_ahead_as_the_level_asks_at_rest() {
        let mut w = world(DOWNHILL);
        let mut camera = Camera::new(&w);
        let run_right = Actions::from(Action::Right) | Action::Action.into();
        run_holding(&mut w, &mut camera, run_right, 120);
        run(&mut w, &mut camera, None, 120);
        let (x, y) = focus(&w);
        assert_eq!(camera.centre, (x + 32.0, y));
    }

    #[test]
    fn a_restart_is_not_motion_to_look_ahead_of() {
        let mut w = world(WIDE);
        let mut camera = Camera::new(&w);
        w.player.pos = Vec2::ints(3000, 900);
        camera.update(&w);
        assert_eq!(camera.look, (0.0, 0.0));
    }
}
