//! Held actions: the neutral button codes shared by scenarios and traces.

use std::fmt;

/// Every action code, in the order traces write them.
pub const ORDER: &str = "LRUDJASPT";

/// A set of held actions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Buttons(u16);

impl Buttons {
    pub const NONE: Buttons = Buttons(0);
    pub const JUMP: Buttons = Buttons(1 << 4);

    /// The single action for `code`, if it is one.
    pub fn from_code(code: char) -> Option<Buttons> {
        ORDER.find(code).map(|i| Buttons(1 << i))
    }

    pub fn union(self, other: Buttons) -> Buttons {
        Buttons(self.0 | other.0)
    }

    pub fn contains(self, other: Buttons) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Parses a scenario `BUTTONS` field: one or more codes, any order,
    /// each at most once.
    pub fn parse_scenario(s: &str) -> Result<Buttons, String> {
        if s.is_empty() {
            return Err("no buttons given".into());
        }
        let mut held = Buttons::NONE;
        for c in s.chars() {
            let b = Buttons::from_code(c)
                .ok_or_else(|| format!("unknown button `{c}`; expected some of `{ORDER}`"))?;
            if held.contains(b) {
                return Err(format!("button `{c}` listed twice in `{s}`"));
            }
            held = held.union(b);
        }
        Ok(held)
    }

    /// Parses a trace `input` column: `.` for nothing held, otherwise the
    /// held codes in `LRUDJASPT` order.
    pub fn parse_trace(s: &str) -> Result<Buttons, String> {
        if s == "." {
            return Ok(Buttons::NONE);
        }
        if s.is_empty() {
            return Err("empty input; write `.` when nothing is held".into());
        }
        let held = Buttons::parse_scenario(s)?;
        if held.to_string() != s {
            return Err(format!("input `{s}` is out of order; write it as `{held}`"));
        }
        Ok(held)
    }
}

/// Trace notation: codes in `LRUDJASPT` order, or `.` for nothing.
impl fmt::Display for Buttons {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str(".");
        }
        for (i, c) in ORDER.chars().enumerate() {
            if self.0 & (1 << i) != 0 {
                write!(f, "{c}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_codes_in_any_order() {
        let a = Buttons::parse_scenario("AJR").unwrap();
        assert_eq!(a, Buttons::parse_scenario("RJA").unwrap());
        assert_eq!(a.to_string(), "RJA");
        assert!(Buttons::parse_scenario("RR").is_err());
        assert!(Buttons::parse_scenario("X").is_err());
        assert!(Buttons::parse_scenario("").is_err());
    }

    #[test]
    fn trace_codes_in_canonical_order() {
        assert_eq!(Buttons::parse_trace(".").unwrap(), Buttons::NONE);
        assert_eq!(
            Buttons::parse_trace("LRUDJASPT").unwrap().to_string(),
            "LRUDJASPT"
        );
        assert!(Buttons::parse_trace("JR").is_err());
        assert!(Buttons::parse_trace("").is_err());
        assert!(Buttons::parse_trace(".R").is_err());
    }
}
