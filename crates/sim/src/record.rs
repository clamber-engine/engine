//! Recording traces (`spec/traces/FORMAT.md`) from the simulation.

use std::sync::Arc;

use clamber_fixed::{FRAC_BITS, Fixed};
use clamber_input::Frame;
use clamber_level::{Level, Vec2};
use clamber_pack::Feel;
use clamber_trace::trace::{self, Source};
use clamber_trace::{Buttons, Decimal, Row, Scenario, Trace};

use crate::archetype::ArchetypeError;
use crate::player::{Facing, State};
use crate::script::{InputFrame, Script};
use crate::world::{Event, World, run, step};

/// Runs the scenario's inputs from the level's start and records a trace
/// of the player, one row per step. The player starts as a new world puts
/// it: standing still, facing right, the lead character. That is the same
/// state as having stood idle for any number of frames, so the usual
/// scenario `start:` needs no settling time. A player that starts in
/// water first sinks with nothing held until its speed stops changing,
/// so a swimming scenario starts sinking at the limit; frame 0 is the
/// first step after that.
///
/// The feel's preset is written to the header for reference only. Its
/// title, if it has one, is written as `# title:`: the title whose
/// measured values the run used, which comparisons check.
pub fn record(
    level: Arc<Level>,
    feel: Arc<Feel>,
    scenario: &Scenario,
) -> Result<Trace, ArchetypeError> {
    let mut extra = vec![("preset".to_string(), feel.preset.clone())];
    if let Some(title) = &feel.title {
        extra.push(("title".to_string(), title.clone()));
    }
    let mut world = World::try_new(level, feel)?;
    settle(&mut world);
    let start = world.player.pos;
    let mut rows = Vec::with_capacity(scenario.frames as usize);
    run(&mut world, &Script::from_scenario(scenario), |w| {
        let frame = rows.len() as u32;
        rows.push(row(w, frame, scenario.held(frame), start, true));
    });
    Ok(Trace {
        scenario: scenario.id.clone(),
        source: Source::Engine,
        extra,
        rows,
    })
}

/// The trace row for frame `frame` of play in `world`, with `input`
/// held, the player's position measured from `start`. Its events are
/// those of the world's last step if it `stepped` on this frame, and
/// none if it didn't.
pub fn row(world: &World, frame: u32, input: Buttons, start: Vec2, stepped: bool) -> Row {
    let p = &world.player;
    Row {
        frame,
        input,
        x: decimal(p.pos.x - start.x),
        y: decimal(p.pos.y - start.y),
        vx: decimal(p.vel.x),
        vy: decimal(p.vel.y),
        state: state(p.state),
        // A swimmer on the floor is on the ground (`swimming.md`).
        ground: p.grounded() || p.swim_floor,
        facing: match p.facing {
            Facing::Left => trace::Facing::Left,
            Facing::Right => trace::Facing::Right,
        },
        events: match stepped {
            true => world.events.iter().map(|&e| event(e)).collect(),
            false => Vec::new(),
        },
    }
}

fn decimal(v: Fixed) -> Decimal {
    Decimal::from_binary(v.to_bits(), FRAC_BITS)
}

/// How many steps a swimming start may take to settle.
const SETTLE_LIMIT: u32 = 600;

/// Steps a player that starts swimming with nothing held until a step
/// leaves its speed as it was.
fn settle(world: &mut World) {
    let nothing = InputFrame::single(Frame::default());
    for _ in 0..SETTLE_LIMIT {
        if world.player.state != State::Swim {
            return;
        }
        let vel = world.player.vel;
        step(world, &nothing);
        if world.player.vel == vel {
            return;
        }
    }
}

/// Simulation states and events are named after the trace vocabulary.
fn state(s: State) -> clamber_trace::State {
    clamber_trace::State::from_name(s.name())
        .unwrap_or_else(|| panic!("state `{}` isn't in the vocabulary", s.name()))
}

fn event(e: Event) -> clamber_trace::Event {
    clamber_trace::Event::from_name(e.name())
        .unwrap_or_else(|| panic!("event `{}` isn't in the vocabulary", e.name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::generic_feel;

    const FLAT: &str = r#"
        format = 1
        name = "flat"
        size = [512, 256]
        start = [32, 200]

        [[collision]]
        shape = "rect"
        rect = [0, 200, 512, 56]
    "#;

    fn scenario(inputs: &str, frames: u32) -> Scenario {
        Scenario::parse(
            &format!(
                "# clamber-scenario v1\n\
                 id: t\nsummary: s\nrequires: r\nstart: s\nframes: {frames}\n\
                 [inputs]\n{inputs}\n"
            ),
            "test",
        )
        .unwrap()
    }

    fn record_flat(scenario: &Scenario) -> Trace {
        let level = Level::from_toml_str(FLAT, "test").unwrap();
        let feel = generic_feel("default").unwrap();
        record(Arc::new(level), Arc::new(feel), scenario).unwrap()
    }

    #[test]
    fn records_the_title_a_preset_holds() {
        let level = Level::from_toml_str(FLAT, "test").unwrap();
        let feel = generic_feel("classic-1").unwrap();
        let trace = record(Arc::new(level), Arc::new(feel), &scenario("0-9 R", 10)).unwrap();
        assert_eq!(
            trace.extra,
            [
                ("preset".into(), "classic-1".into()),
                ("title".into(), "title-1".into())
            ]
        );
    }

    #[test]
    fn records_one_row_per_frame() {
        let s = scenario("0-39 R\n5-9 J", 40);
        let trace = record_flat(&s);
        assert_eq!(trace.scenario, "t");
        assert_eq!(trace.source, Source::Engine);
        assert_eq!(trace.extra, [("preset".into(), "default".into())]);
        assert_eq!(trace.rows.len(), 40);
        for (i, row) in trace.rows.iter().enumerate() {
            assert_eq!(row.frame, i as u32);
            assert_eq!(row.input, s.held(row.frame));
        }

        let first = &trace.rows[0];
        assert_eq!(first.state, clamber_trace::State::Walk);
        assert!(first.x > Decimal::ZERO);
        assert_eq!(first.y, Decimal::ZERO);

        let jump = trace
            .rows
            .iter()
            .find(|r| r.has(clamber_trace::Event::JumpStart))
            .expect("a jump");
        assert!(!jump.ground);
        assert!(jump.vy.is_negative());
        assert!(
            trace.rows[jump.frame as usize..]
                .iter()
                .any(|r| r.has(clamber_trace::Event::Land))
        );
    }

    #[test]
    fn written_traces_parse_back() {
        let trace = record_flat(&scenario("0-29 L\n10-29 A", 30));
        assert!(trace.rows[0].has(clamber_trace::Event::Turn));
        let text = trace.to_string();
        assert_eq!(Trace::parse(&text, "recorded").unwrap(), trace);
    }

    #[test]
    fn a_swimming_start_sinks_to_the_limit_first() {
        // All water, with a floor far below.
        let src = r#"
            format = 1
            name = "pool"
            size = [512, 2000]
            start = [128, 100]

            [[regions]]
            rect = [0, 0, 512, 2000]
            water = true

            [[collision]]
            shape = "rect"
            rect = [0, 1900, 512, 100]
        "#;
        let level = Level::from_toml_str(src, "test").unwrap();
        let feel = generic_feel("classic-1").unwrap();
        let s = scenario("0-1 R\n1 J", 3);
        let trace = record(Arc::new(level), Arc::new(feel), &s).unwrap();
        let limit = Decimal::from_binary(5, 2);
        let rows = &trace.rows;
        assert_eq!(rows.iter().map(|r| r.frame).collect::<Vec<_>>(), [0, 1, 2]);
        assert!(rows.iter().all(|r| r.state == clamber_trace::State::Swim));
        assert!(rows.iter().all(|r| !r.ground));
        assert_eq!((rows[0].y, rows[0].vy), (limit, limit));
        assert!(rows[1].has(clamber_trace::Event::Stroke));
        assert!(rows[1].vy.is_negative());
        assert!(rows[2].events.is_empty());
    }
}
