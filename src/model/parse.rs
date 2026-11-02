//! The error of the text parsers in `model` and `scales`.

use std::fmt;

/// Why text did not parse as a colour, timeframe, time zone or price scale mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    kind: &'static str,
    input: String,
    reason: &'static str,
}

impl ParseError {
    pub(crate) fn new(kind: &'static str, input: &str, reason: &'static str) -> Self {
        Self {
            kind,
            input: input.to_owned(),
            reason,
        }
    }

    /// The text that did not parse, as given.
    pub fn input(&self) -> &str {
        &self.input
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {} {:?}: {}", self.kind, self.input, self.reason)
    }
}

impl std::error::Error for ParseError {}
