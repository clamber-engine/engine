//! Scenario files (`spec/scenarios/FORMAT.md`).

use std::path::Path;

use crate::metrics::{Comparison, Curve, Field, Metric};
use crate::setup::{Geometry, Mode, Setup, Start, parse_geometry_line};
use crate::{Buttons, Decimal, Event, ParseError};

/// A scripted piece of play: a starting situation, held inputs, and the
/// metrics to compare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub id: String,
    pub summary: String,
    /// Plain-words description of the geometry the test level must have.
    pub requires: String,
    /// Plain-words player state when frame 0 begins (v1; empty in v2).
    pub start: String,
    /// The structured geometry and start of a v2 scenario.
    pub setup: Option<Setup>,
    /// Number of logic frames to record.
    pub frames: u32,
    pub inputs: Vec<InputLine>,
    /// Empty when there is no `[metrics]` section: the scenario only
    /// records traces.
    pub metrics: Vec<Comparison>,
}

/// One `[inputs]` line: `buttons` held on frames `first..=last`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputLine {
    pub first: u32,
    pub last: u32,
    pub buttons: Buttons,
}

impl Scenario {
    /// Actions held on `frame`: everything from every line that covers it.
    pub fn held(&self, frame: u32) -> Buttons {
        self.inputs
            .iter()
            .filter(|l| (l.first..=l.last).contains(&frame))
            .fold(Buttons::NONE, |held, l| held.union(l.buttons))
    }

    /// Reads and parses a scenario file, and checks that its `id` matches
    /// the file name.
    pub fn load(path: &Path) -> Result<Scenario, ParseError> {
        let origin = path.display().to_string();
        let text = std::fs::read_to_string(path)
            .map_err(|e| ParseError::new(&origin, 0, format!("can't read file: {e}")))?;
        let scenario = Scenario::parse(&text, &origin)?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if scenario.id != stem {
            return Err(ParseError::new(
                &origin,
                0,
                format!(
                    "id `{}` doesn't match the file name; expected `{stem}`",
                    scenario.id
                ),
            ));
        }
        Ok(scenario)
    }

    /// Parses scenario text. `origin` names it in errors.
    pub fn parse(text: &str, origin: &str) -> Result<Scenario, ParseError> {
        let err = |line: usize, message: String| ParseError::new(origin, line, message);

        #[derive(PartialEq)]
        enum Section {
            Header,
            Geometry,
            Start,
            Inputs,
            Metrics,
        }
        let mut section = Section::Header;
        let mut seen_geometry = false;
        let mut seen_start = false;
        let mut seen_inputs = false;
        let mut seen_metrics = false;

        let mut header: Vec<(&str, &str, usize)> = Vec::new();
        let mut inputs: Vec<(InputLine, usize)> = Vec::new();
        let mut geometry = Geometry::default();
        let mut seen_x = false;
        let mut start_lines: Vec<(&str, &str, usize)> = Vec::new();
        let mut v2 = false;
        let mut metrics: Vec<Comparison> = Vec::new();

        let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l));
        match lines.next() {
            Some((_, MAGIC)) => {}
            Some((_, MAGIC_V2)) => v2 = true,
            Some((n, l)) if l.starts_with("# clamber-scenario ") => {
                let version = l["# clamber-scenario ".len()..].trim();
                return Err(err(
                    n,
                    format!("unsupported scenario format `{version}`; expected v1 or v2"),
                ));
            }
            Some((n, _)) => return Err(err(n, format!("first line must be `{MAGIC}`"))),
            None => return Err(err(0, "file is empty".into())),
        }

        for (n, raw) in lines {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') {
                let (next, seen) = match line {
                    "[geometry]" if v2 => (Section::Geometry, &mut seen_geometry),
                    "[start]" if v2 => (Section::Start, &mut seen_start),
                    "[inputs]" => (Section::Inputs, &mut seen_inputs),
                    "[metrics]" => (Section::Metrics, &mut seen_metrics),
                    _ => {
                        return Err(err(
                            n,
                            format!(
                                "unknown section `{line}`; expected {}",
                                if v2 {
                                    "`[geometry]`, `[start]`, `[inputs]` or `[metrics]`"
                                } else {
                                    "`[inputs]` or `[metrics]`"
                                }
                            ),
                        ));
                    }
                };
                if *seen {
                    return Err(err(n, format!("section `{line}` appears twice")));
                }
                if v2 {
                    let rank = |s: &Section| match s {
                        Section::Header => 0,
                        Section::Geometry => 1,
                        Section::Start => 2,
                        Section::Inputs => 3,
                        Section::Metrics => 4,
                    };
                    if rank(&next) < rank(&section) {
                        return Err(err(
                            n,
                            format!(
                                "section `{line}` is out of order; the order is [geometry], [start], [inputs], [metrics]"
                            ),
                        ));
                    }
                }
                *seen = true;
                section = next;
                continue;
            }

            if !matches!(section, Section::Header | Section::Start) && looks_like_header(line) {
                return Err(err(
                    n,
                    "header lines must come before the first section".into(),
                ));
            }

            match section {
                Section::Header => {
                    let Some((key, value)) = line.split_once(':') else {
                        return Err(err(n, format!("expected `key: value`, found `{line}`")));
                    };
                    let (key, value) = (key.trim(), value.trim());
                    if v2 && key == "start" {
                        return Err(err(
                            n,
                            "`start` header key is not allowed in v2; use the [start] section"
                                .into(),
                        ));
                    }
                    let keys = if v2 { HEADER_KEYS_V2 } else { HEADER_KEYS };
                    if !keys.contains(&key) {
                        return Err(err(
                            n,
                            format!(
                                "unknown header key `{key}`; expected one of {}",
                                keys.join(", ")
                            ),
                        ));
                    }
                    if header.iter().any(|(k, _, _)| *k == key) {
                        return Err(err(n, format!("header `{key}` appears twice")));
                    }
                    if value.is_empty() {
                        return Err(err(n, format!("header `{key}` is empty")));
                    }
                    header.push((key, value, n));
                }
                Section::Geometry => {
                    parse_geometry_line(&mut geometry, &mut seen_x, line).map_err(|m| err(n, m))?
                }
                Section::Start => {
                    let Some((key, value)) = line.split_once(':') else {
                        return Err(err(n, format!("expected `key: value`, found `{line}`")));
                    };
                    start_lines.push((key.trim(), value.trim(), n));
                }
                Section::Inputs => inputs.push((parse_input(line).map_err(|m| err(n, m))?, n)),
                Section::Metrics => {
                    let comparison = parse_comparison(line).map_err(|m| err(n, m))?;
                    if metrics.iter().any(|c| c.label() == comparison.label()) {
                        return Err(err(
                            n,
                            format!("`{}` is compared twice", comparison.label()),
                        ));
                    }
                    metrics.push(comparison);
                }
            }
        }

        let get = |key: &str| -> Result<(&str, usize), ParseError> {
            header
                .iter()
                .find(|(k, _, _)| *k == key)
                .map(|(_, v, n)| (*v, *n))
                .ok_or_else(|| err(0, format!("missing required header `{key}`")))
        };

        let (id, id_line) = get("id")?;
        if !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(err(
                id_line,
                format!("id `{id}` may only contain a-z, 0-9 and `-`"),
            ));
        }
        let (frames, frames_line) = get("frames")?;
        let frames = parse_count(frames).filter(|&f| f > 0).ok_or_else(|| {
            err(
                frames_line,
                format!("frames must be a positive whole number, found `{frames}`"),
            )
        })?;
        for (input, n) in &inputs {
            if input.last >= frames {
                return Err(err(
                    *n,
                    format!(
                        "frame {} is past the end; the scenario has frames 0-{}",
                        input.last,
                        frames - 1
                    ),
                ));
            }
        }

        let summary = get("summary")?.0.to_string();
        let (requires, start, setup) = if v2 {
            if !seen_geometry || !seen_start {
                return Err(err(
                    0,
                    format!(
                        "missing required section `{}`",
                        if seen_geometry {
                            "[start]"
                        } else {
                            "[geometry]"
                        }
                    ),
                ));
            }
            let probe = match header.iter().find(|(k, _, _)| *k == "probe") {
                None => false,
                Some((_, "yes", _)) => true,
                Some((_, other, n)) => {
                    return Err(err(*n, format!("probe must be `yes`, found `{other}`")));
                }
            };
            let start = Start::from_lines(&start_lines).map_err(|(n, m)| err(n, m))?;
            geometry
                .check(start.mode == Mode::Rope)
                .map_err(|m| err(0, m))?;
            start.check(&geometry).map_err(|m| err(0, m))?;
            let requires = get("requires")
                .map(|(v, _)| v.to_string())
                .unwrap_or_default();
            (
                requires,
                String::new(),
                Some(Setup {
                    geometry,
                    start,
                    probe,
                }),
            )
        } else {
            (
                get("requires")?.0.to_string(),
                get("start")?.0.to_string(),
                None,
            )
        };

        Ok(Scenario {
            id: id.to_string(),
            summary,
            requires,
            start,
            setup,
            frames,
            inputs: inputs.into_iter().map(|(l, _)| l).collect(),
            metrics,
        })
    }
}

/// Required first line.
pub const MAGIC: &str = "# clamber-scenario v1";

/// First line of a v2 file.
pub const MAGIC_V2: &str = "# clamber-scenario v2";

const HEADER_KEYS: &[&str] = &["id", "summary", "requires", "start", "frames"];
const HEADER_KEYS_V2: &[&str] = &["id", "summary", "requires", "frames", "probe"];

/// A `key: value` line, whose first word ends in a colon.
fn looks_like_header(line: &str) -> bool {
    line.split_whitespace()
        .next()
        .is_some_and(|w| w.ends_with(':'))
}

/// A whole number written as plain digits.
fn parse_count(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn parse_input(line: &str) -> Result<InputLine, String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let [frames, buttons] = fields[..] else {
        return Err(format!("expected `FRAMES BUTTONS`, found `{line}`"));
    };
    let bad_frames = || format!("`{frames}` is not a frame or an inclusive range like `60-79`");
    let (first, last) = match frames.split_once('-') {
        Some((a, b)) => (
            parse_count(a).ok_or_else(bad_frames)?,
            parse_count(b).ok_or_else(bad_frames)?,
        ),
        None => {
            let f = parse_count(frames).ok_or_else(bad_frames)?;
            (f, f)
        }
    };
    if first > last {
        return Err(format!("range `{frames}` runs backwards"));
    }
    Ok(InputLine {
        first,
        last,
        buttons: Buttons::parse_scenario(buttons)?,
    })
}

fn parse_tol(s: &str) -> Result<Decimal, String> {
    let tol = Decimal::parse(s).map_err(|e| format!("tolerance: {e}"))?;
    if tol.is_negative() {
        return Err(format!("tolerance `{s}` is negative"));
    }
    Ok(tol)
}

fn parse_comparison(line: &str) -> Result<Comparison, String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields[..] {
        ["curve", names, "from", event, "for", frames, "tol", tol] => {
            let mut parsed = Vec::new();
            for name in names.split(',') {
                let field = Field::from_name(name).ok_or_else(|| {
                    format!("`{name}` can't be compared in a curve; expected some of x, y, vx, vy")
                })?;
                if parsed.contains(&field) {
                    return Err(format!("field `{name}` listed twice"));
                }
                parsed.push(field);
            }
            let event = Event::from_name(event)
                .ok_or_else(|| format!("unknown event `{event}` (see traces/VOCABULARY.md)"))?;
            let frames = parse_count(frames).filter(|&n| n > 0).ok_or_else(|| {
                format!("curve length must be a positive whole number, found `{frames}`")
            })?;
            Ok(Comparison::Curve(Curve {
                fields: parsed,
                event,
                frames: frames as usize,
                tol: parse_tol(tol)?,
            }))
        }
        ["curve", ..] => Err(format!(
            "expected `curve FIELDS from EVENT for N tol T`, found `{line}`"
        )),
        [name, "tol", tol] => {
            let metric = Metric::from_name(name)
                .ok_or_else(|| format!("unknown metric `{name}` (see traces/METRICS.md)"))?;
            Ok(Comparison::Scalar {
                metric,
                tol: parse_tol(tol)?,
            })
        }
        _ => Err(format!(
            "expected `NAME tol T` or `curve FIELDS from EVENT for N tol T`, found `{line}`"
        )),
    }
}
