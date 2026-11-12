//! Error type for pack loading and parsing: one message per problem, with a
//! pack-relative location when one is known.

use std::fmt;

/// A load-time error. The spec's posture: catch typos at load, not at players —
/// every validation failure is a `LoadError` with a useful message.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadError {
    /// Pack-relative file the error came from (e.g. `scenes/first_meeting.unmei`).
    pub file: String,
    /// 1-based line number, when the error is line-scoped.
    pub line: Option<usize>,
    pub message: String,
}

impl LoadError {
    pub fn new(file: impl Into<String>, line: Option<usize>, message: impl Into<String>) -> Self {
        LoadError {
            file: file.into(),
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(n) => write!(f, "{}:{}: {}", self.file, n, self.message),
            None => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

impl std::error::Error for LoadError {}
