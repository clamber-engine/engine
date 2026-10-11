use std::fmt;

/// A scenario or trace file that doesn't follow its format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Where the text came from: a file path, or a name given by the caller.
    pub origin: String,
    /// 1-based line number, or 0 when the problem is with the file as a
    /// whole (a missing header, no rows).
    pub line: usize,
    pub message: String,
}

impl ParseError {
    pub(crate) fn new(origin: &str, line: usize, message: impl Into<String>) -> Self {
        ParseError {
            origin: origin.to_string(),
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.origin, self.message)
        } else {
            write!(f, "{}:{}: {}", self.origin, self.line, self.message)
        }
    }
}

impl std::error::Error for ParseError {}
