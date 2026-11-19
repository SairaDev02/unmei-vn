//! A Keiko-only recorder for the funnel's lines (spec §9.2, §15 rule 28).
//!
//! The session log's text does not carry a failure row id, and `expect log …
//! row <n>` needs one. The funnel therefore hands every line it logs to this
//! recorder as well.
//!
//! The recorder is a resource of one app: a take inserts it into the app it
//! builds and reads it back from that app's world. Other apps in the same
//! process, such as concurrently running tests, record into their own
//! recorder or, like the shipping player, into none, so a take never sees
//! another app's lines.

use bevy::prelude::*;

use super::rows::FailureRow;

/// One line the funnel logged during a take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedLine {
    pub row: u32,
    pub level: String,
    pub text: String,
}

/// Every funnel line one take's app has logged, oldest first. Present only in
/// a take's app.
#[derive(Resource, Debug, Default)]
pub struct TapLog {
    lines: Vec<RecordedLine>,
}

impl TapLog {
    /// Record one funnel line.
    pub(crate) fn record(&mut self, row: FailureRow, level: log::Level, text: String) {
        self.lines.push(RecordedLine {
            row: row.number(),
            level: match level {
                log::Level::Error => "error".to_string(),
                log::Level::Warn => "warn".to_string(),
                _ => "info".to_string(),
            },
            text,
        });
    }

    /// The lines recorded since `cursor`, and the new cursor.
    pub fn since(&self, cursor: usize) -> (Vec<RecordedLine>, usize) {
        let from = cursor.min(self.lines.len());
        (self.lines[from..].to_vec(), self.lines.len())
    }
}
