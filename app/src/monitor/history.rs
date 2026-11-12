//! The launch-wide retained history and its per-update ingestion system.

use std::collections::VecDeque;
use std::sync::Arc;

use bevy::prelude::*;

use super::capture::MonitorCapture;
use super::line::MonitorLine;
use super::{INGEST_PER_UPDATE, RETAINED_CAPACITY};

/// The launch-wide retained history: at most [`RETAINED_CAPACITY`] lines,
/// oldest evicted first. Independent of any panel visibility or a loaded
/// [`crate::Runtime`]. Lines are shared with the display projection by [`Arc`],
/// so projecting does not duplicate payload text and an evicted line is dropped
/// as soon as every holder lets it go.
#[derive(Resource, Default)]
pub struct MonitorHistory {
    lines: VecDeque<Arc<MonitorLine>>,
}

impl MonitorHistory {
    /// The retained lines, oldest first.
    pub fn lines(&self) -> &VecDeque<Arc<MonitorLine>> {
        &self.lines
    }

    /// How many lines are retained.
    pub fn retained(&self) -> usize {
        self.lines.len()
    }

    /// The oldest surviving admission identity, if any. A projection uses this
    /// to drop entries that retention evicted without keeping a second history.
    pub fn oldest_sequence(&self) -> Option<u64> {
        self.lines.front().map(|line| line.sequence)
    }

    /// The newest surviving admission identity, if any.
    pub fn newest_sequence(&self) -> Option<u64> {
        self.lines.back().map(|line| line.sequence)
    }

    /// All retained logical lines joined by real newlines — the escaped text as
    /// the viewer would present it, for sink assertions.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(&line.text);
        }
        out
    }

    pub(super) fn push(&mut self, line: MonitorLine, capture: &MonitorCapture) {
        if self.lines.len() >= RETAINED_CAPACITY {
            self.lines.pop_front();
            capture.record_evicted(1);
        }
        self.lines.push_back(Arc::new(line));
    }

    /// Seed retained lines directly, without a tracing subscriber. Test-only:
    /// the drawer's projection tests need deterministic admission sequences and
    /// eviction behavior that a real subscriber can only produce slowly.
    #[cfg(test)]
    pub(crate) fn seed_lines(&mut self, lines: impl IntoIterator<Item = MonitorLine>) {
        self.lines.clear();
        for line in lines {
            self.lines.push_back(Arc::new(line));
        }
    }
}

/// Move at most [`INGEST_PER_UPDATE`] pending lines into retained history.
///
/// Registered by [`crate::RuntimeCorePlugin`] only while a [`MonitorCapture`]
/// resource is present, and then not gated by a loaded [`crate::Runtime`], the
/// title/menu, a defensive hold or the panel's visibility — so it keeps
/// ingesting on every app update in those states.
pub fn ingest_capture(
    capture: Option<Res<MonitorCapture>>,
    history: Option<ResMut<MonitorHistory>>,
) {
    let (Some(capture), Some(mut history)) = (capture, history) else {
        return;
    };
    // The staged batch stays charged until each line is handed to history, so a
    // stalled consumer can never let a producer refill a freed slot.
    let mut staged = capture.take_staged(INGEST_PER_UPDATE);
    while let Some(line) = staged.pop_front() {
        history.push(line, &capture);
        staged.release_one();
    }
}
