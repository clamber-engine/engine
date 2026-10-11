//! Trace files (`spec/traces/FORMAT.md`).

use std::fmt;
use std::path::Path;

use crate::{Buttons, Decimal, Event, ParseError, State};

/// Required first line.
pub const MAGIC: &str = "# clamber-trace v1";

/// Required column line.
pub const COLUMNS: &str = "frame\tinput\tx\ty\tvx\tvy\tstate\tground\tfacing\tevents";

/// One recorded run of a scenario, one row per logic frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    /// The `# scenario:` header.
    pub scenario: String,
    pub source: Source,
    /// Other `# key: value` header lines, in order. Comparisons ignore them.
    pub extra: Vec<(String, String)>,
    /// Never empty.
    pub rows: Vec<Row>,
}

/// Who recorded a trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Reference,
    Engine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    Left,
    Right,
}

/// Player state after one logic frame's update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub frame: u32,
    pub input: Buttons,
    /// Displacement from the start position, px, right is positive.
    pub x: Decimal,
    /// Displacement from the start position, px, down is positive.
    pub y: Decimal,
    /// px per frame.
    pub vx: Decimal,
    /// px per frame, down is positive.
    pub vy: Decimal,
    pub state: State,
    pub ground: bool,
    pub facing: Facing,
    pub events: Vec<Event>,
}

impl Row {
    pub fn has(&self, event: Event) -> bool {
        self.events.contains(&event)
    }
}

impl Trace {
    pub fn load(path: &Path) -> Result<Trace, ParseError> {
        let origin = path.display().to_string();
        let text = std::fs::read_to_string(path)
            .map_err(|e| ParseError::new(&origin, 0, format!("can't read file: {e}")))?;
        Trace::parse(&text, &origin)
    }

    /// Parses trace text. `origin` names it in errors.
    pub fn parse(text: &str, origin: &str) -> Result<Trace, ParseError> {
        let err = |line: usize, message: String| ParseError::new(origin, line, message);
        let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l));

        match lines.next() {
            Some((_, MAGIC)) => {}
            Some((n, l)) if l.starts_with("# clamber-trace ") => {
                return Err(err(
                    n,
                    format!("unsupported trace format `{l}`; expected `{MAGIC}`"),
                ));
            }
            Some((n, _)) => return Err(err(n, format!("first line must be `{MAGIC}`"))),
            None => return Err(err(0, "file is empty".into())),
        }

        let mut scenario = None;
        let mut source = None;
        let mut extra = Vec::new();
        let mut columns = false;
        let mut rows: Vec<Row> = Vec::new();

        for (n, line) in lines {
            if columns {
                let row = Row::parse(line).map_err(|m| err(n, m))?;
                match rows.last() {
                    None if row.frame != 0 => {
                        return Err(err(
                            n,
                            format!("the first frame must be 0, found {}", row.frame),
                        ));
                    }
                    Some(prev) if Some(row.frame) != prev.frame.checked_add(1) => {
                        return Err(err(
                            n,
                            format!(
                                "frame {} follows frame {}; frames must go up by 1",
                                row.frame, prev.frame
                            ),
                        ));
                    }
                    _ => {}
                }
                rows.push(row);
            } else if let Some(rest) = line.strip_prefix('#') {
                let Some((key, value)) = rest.split_once(':') else {
                    return Err(err(
                        n,
                        format!("header lines must be `# key: value`, found `{line}`"),
                    ));
                };
                let (key, value) = (key.trim(), value.trim());
                let duplicate = || err(n, format!("header `{key}` appears twice"));
                match key {
                    "scenario" => {
                        if scenario.replace(value.to_string()).is_some() {
                            return Err(duplicate());
                        }
                    }
                    "source" => {
                        let parsed = match value {
                            "reference" => Source::Reference,
                            "engine" => Source::Engine,
                            _ => {
                                return Err(err(
                                    n,
                                    format!(
                                        "source must be `reference` or `engine`, found `{value}`"
                                    ),
                                ));
                            }
                        };
                        if source.replace(parsed).is_some() {
                            return Err(duplicate());
                        }
                    }
                    "" => return Err(err(n, format!("header line has no key: `{line}`"))),
                    _ => extra.push((key.to_string(), value.to_string())),
                }
            } else if line == COLUMNS {
                columns = true;
            } else if line.split_whitespace().eq(COLUMNS.split('\t')) {
                return Err(err(n, "column line must be tab-separated".into()));
            } else {
                return Err(err(
                    n,
                    format!(
                        "expected the column line `{}` (tab-separated)",
                        COLUMNS.replace('\t', " ")
                    ),
                ));
            }
        }

        if !columns {
            return Err(err(0, "missing the column line".into()));
        }
        let scenario = scenario.ok_or_else(|| err(0, "missing `# scenario:` header".into()))?;
        let source = source.ok_or_else(|| err(0, "missing `# source:` header".into()))?;
        if rows.is_empty() {
            return Err(err(0, "trace has no frames".into()));
        }
        Ok(Trace {
            scenario,
            source,
            extra,
            rows,
        })
    }
}

impl Row {
    /// Parses one tab-separated row. Rows of 9 fields have no events.
    pub fn parse(line: &str) -> Result<Row, String> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 9 && f.len() != 10 {
            return Err(format!(
                "expected 10 tab-separated columns, found {}",
                f.len()
            ));
        }

        let frame = if !f[0].is_empty() && f[0].bytes().all(|b| b.is_ascii_digit()) {
            f[0].parse()
                .map_err(|_| format!("frame `{}` is too large", f[0]))?
        } else {
            return Err(format!("frame must be a whole number, found `{}`", f[0]));
        };
        let input = Buttons::parse_trace(f[1]).map_err(|e| format!("input: {e}"))?;
        let decimal =
            |i: usize, name: &str| Decimal::parse(f[i]).map_err(|e| format!("{name}: {e}"));
        let (x, y, vx, vy) = (
            decimal(2, "x")?,
            decimal(3, "y")?,
            decimal(4, "vx")?,
            decimal(5, "vy")?,
        );
        let state = State::from_name(f[6])
            .ok_or_else(|| format!("unknown state `{}` (see traces/VOCABULARY.md)", f[6]))?;
        let ground = match f[7] {
            "0" => false,
            "1" => true,
            other => return Err(format!("ground must be `0` or `1`, found `{other}`")),
        };
        let facing = match f[8] {
            "L" => Facing::Left,
            "R" => Facing::Right,
            other => return Err(format!("facing must be `L` or `R`, found `{other}`")),
        };

        let mut events = Vec::new();
        if let Some(&list) = f.get(9).filter(|s| !s.is_empty()) {
            for name in list.split(',') {
                let event = Event::from_name(name)
                    .ok_or_else(|| format!("unknown event `{name}` (see traces/VOCABULARY.md)"))?;
                if events.contains(&event) {
                    return Err(format!("event `{name}` listed twice"));
                }
                events.push(event);
            }
        }

        Ok(Row {
            frame,
            input,
            x,
            y,
            vx,
            vy,
            state,
            ground,
            facing,
            events,
        })
    }
}

/// The row as written in a trace: always 10 fields, so a row without
/// events ends in a tab.
impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let facing = match self.facing {
            Facing::Left => "L",
            Facing::Right => "R",
        };
        write!(
            f,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{facing}\t",
            self.frame,
            self.input,
            self.x,
            self.y,
            self.vx,
            self.vy,
            self.state,
            u8::from(self.ground),
        )?;
        for (i, event) in self.events.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{event}")?;
        }
        Ok(())
    }
}

/// The whole trace file, one `\n`-terminated line each.
impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{MAGIC}")?;
        writeln!(f, "# scenario: {}", self.scenario)?;
        writeln!(f, "# source: {}", self.source)?;
        for (key, value) in &self.extra {
            writeln!(f, "# {key}: {value}")?;
        }
        writeln!(f, "{COLUMNS}")?;
        for row in &self.rows {
            writeln!(f, "{row}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Source::Reference => "reference",
            Source::Engine => "engine",
        })
    }
}
