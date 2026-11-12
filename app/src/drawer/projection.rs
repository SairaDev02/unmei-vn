//! The bounded display projection over the one shared retained history.

use std::collections::VecDeque;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::monitor::{MonitorHistory, MonitorLine};
use crate::monitor_reader::{Metrics, ReaderFrame};

use super::geometry::{reader_columns, reader_metrics, PROJECTION_SCAN_BUDGET};
use super::state::{MonitorPanel, MonitorView, MonitorWork};

/// The bounded display projection over the shared retained history.
///
/// Each pass extends **one contiguous examined band** `[frontier, ceiling]` by
/// at most [`PROJECTION_SCAN_BUDGET`] retained entries. The first pass
/// establishes the band newest-first, so following latest shows recent history
/// on the opening frame. Later passes extend the band at both ends without ever
/// leaving a hole:
///
/// - **upward** from `ceiling` for arrivals that landed above the examined
///   block, oldest-of-the-new-first, so a batch larger than the pass budget
///   resumes exactly where it stopped instead of being mistaken for reviewed;
/// - **downward** from `frontier` into older retained history.
///
/// Because the ends only ever advance contiguously, every retained entry is
/// examined exactly once and a refresh settles complete under bounded work.
/// Matching entries are shared with the history by [`Arc`]; the projection is
/// never a second copy of the payload.
#[derive(Resource, Debug, Default)]
pub(crate) struct MonitorProjection {
    view: MonitorView,
    /// Matching retained lines, oldest first.
    rows: VecDeque<Arc<MonitorLine>>,
    /// The lowest sequence in the examined band.
    frontier: Option<u64>,
    /// The highest sequence in the examined band.
    ceiling: Option<u64>,
    /// Whether the current pass still has unexamined retained entries.
    updating: bool,
}

impl MonitorProjection {
    /// How many matching rows the projection holds.
    pub(crate) fn shown(&self) -> usize {
        self.rows.len()
    }

    /// Whether a projection pass is still in progress (`updating view`).
    pub(crate) fn is_updating(&self) -> bool {
        self.updating
    }

    /// The projected targets, oldest first. Test/inspection accessor: the
    /// drawer renders the same rows (the tests assert observable output rather
    /// than row storage, but the projection's predicate is exactly what they
    /// must distinguish).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn targets(&self) -> Vec<String> {
        self.rows.iter().map(|row| row.target.clone()).collect()
    }

    /// The projected admission identities, oldest first.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn sequences(&self) -> Vec<u64> {
        self.rows.iter().map(|row| row.sequence).collect()
    }

    /// The matching rows, oldest first. Sorted by retained admission order.
    pub(crate) fn rows(&self) -> &VecDeque<Arc<MonitorLine>> {
        &self.rows
    }

    /// Whether the examined band has reached the oldest retained entry, i.e.
    /// `rows.front()` is the oldest surviving matching event.
    pub(crate) fn age_complete(&self, oldest: Option<u64>) -> bool {
        !self.updating || self.frontier_covers(oldest)
    }

    /// The one `frontier <= oldest` test shared by [`Self::age_complete`],
    /// [`Self::covers`] and the pass's own arrival-budget decision.
    fn frontier_covers(&self, oldest: Option<u64>) -> bool {
        self.frontier
            .is_some_and(|frontier| oldest.is_none_or(|oldest| frontier <= oldest))
    }

    /// Whether the examined band already covers every retained entry. When it
    /// does there is nothing left to scan, so a settled update can return
    /// without any history walk at all.
    fn covers(&self, oldest: Option<u64>, newest: Option<u64>) -> bool {
        self.frontier_covers(oldest)
            && self
                .ceiling
                .is_some_and(|ceiling| newest.is_none_or(|newest| ceiling >= newest))
    }

    fn restart(&mut self, view: MonitorView) {
        self.clear(view);
        self.updating = true;
    }

    /// Drop every row and forget the band, leaving the pass settled. Used when
    /// the view changes (via [`Self::restart`]) and when there is no history to
    /// project at all.
    fn clear(&mut self, view: MonitorView) {
        self.view = view;
        self.rows.clear();
        self.frontier = None;
        self.ceiling = None;
        self.updating = false;
    }

    /// Drop the rows retention aged out, so an evicted payload is never shown
    /// again and the projection does not keep a parallel history alive.
    fn prune_evicted(&mut self, oldest: Option<u64>) {
        while self
            .rows
            .front()
            .is_some_and(|row| oldest.is_none_or(|oldest| row.sequence < oldest))
        {
            self.rows.pop_front();
        }
    }
}

/// Advance the display projection at most [`PROJECTION_SCAN_BUDGET`] retained
/// entries per app update, only while the drawer is open. A view change cancels
/// the old pass and starts a fresh one, so rows from the previous predicate are
/// never presented as the new view. Evicted references are pruned on every
/// update, hidden or not, so a hidden projection can never pin an evicted
/// payload even though it does no hidden text work.
pub(crate) fn project_monitor(
    panel: Res<MonitorPanel>,
    history: Option<Res<MonitorHistory>>,
    mut projection: ResMut<MonitorProjection>,
    mut work: ResMut<MonitorWork>,
) {
    work.scanned = 0;
    work.text_ops = 0;

    // Prune before the visibility check: dropping an evicted `Arc` reference is
    // not text work and must happen while hidden too, or a previously full
    // projection would keep a second copy of evicted payloads alive until the
    // drawer reopens (finding A02).
    let oldest = history.as_deref().and_then(MonitorHistory::oldest_sequence);
    projection.prune_evicted(oldest);

    if !panel.open {
        // No hidden scan or text work. The pass resumes exactly where it stopped
        // on reopen.
        return;
    }

    let Some(history) = history else {
        // No history at all: drop every projected row rather than let a stale
        // cache outlive the one shared history.
        projection.clear(panel.view);
        return;
    };

    if projection.view != panel.view {
        projection.restart(panel.view);
    }

    let newest = history.newest_sequence();
    // A history whose newest entry is below the examined band is a replacement,
    // not an append: production retention evicts from the front and appends
    // strictly higher sequences, so this is reached only by a reseeded fixture
    // (see `a_shrunk_history_reprojects_without_stale_rows`). Start over instead
    // of letting rows that are no longer retained look like coverage.
    if projection
        .ceiling
        .is_some_and(|ceiling| newest.is_some_and(|newest| newest < ceiling))
    {
        projection.restart(panel.view);
    }

    if history.retained() == 0 {
        projection.clear(panel.view);
        return;
    }

    // Settled: the band already covers every retained entry, so there is nothing
    // to scan. This is the settled zero-work path.
    if projection.covers(oldest, newest) {
        projection.updating = false;
        return;
    }

    let mut frontier = projection.frontier;
    let mut ceiling = projection.ceiling;
    let mut examined = 0usize;

    match ceiling {
        None => {
            // First pass: establish the band newest-first so following latest
            // shows recent history on the opening frame, examining downward.
            for line in history.lines().iter().rev() {
                if examined >= PROJECTION_SCAN_BUDGET {
                    break;
                }
                examined += 1;
                if panel.view.matches(line) {
                    projection.rows.push_front(Arc::clone(line));
                }
                frontier =
                    Some(frontier.map_or(line.sequence, |frontier| frontier.min(line.sequence)));
                ceiling = Some(ceiling.map_or(line.sequence, |ceiling| ceiling.max(line.sequence)));
            }
        }
        Some(existing_ceiling) => {
            // Older history is what a continuous producer can starve, so while
            // the band has not reached the oldest retained entry arrivals are
            // capped at half the budget; the rest goes to older entries. With
            // no arrivals pending, the older side still gets the whole budget
            // through the shared `examined` count.
            let age_complete = projection.frontier_covers(oldest);
            let arrival_budget = if age_complete {
                PROJECTION_SCAN_BUDGET
            } else {
                PROJECTION_SCAN_BUDGET / 2
            };

            // Extend **upward** from the examined ceiling, oldest-of-the-new
            // first. A batch larger than the budget therefore resumes from
            // exactly where the last pass stopped, leaving no unexamined hole
            // above the band (finding A05).
            if let Some(newest) = newest {
                let lines = history.lines();
                let mut index = lines.partition_point(|line| line.sequence <= existing_ceiling);
                while examined < arrival_budget && index < lines.len() {
                    let line = &lines[index];
                    if line.sequence > newest {
                        break;
                    }
                    index += 1;
                    examined += 1;
                    if panel.view.matches(line) {
                        projection.rows.push_back(Arc::clone(line));
                    }
                    ceiling = Some(line.sequence);
                }
            }

            // Extend **downward** into older retained history with whatever
            // budget is left. The band stays contiguous: everything between
            // `frontier` and `ceiling` has now been examined exactly once.
            let lines = history.lines();
            let mut index = frontier
                .map(|frontier| lines.partition_point(|line| line.sequence < frontier))
                .unwrap_or(0);
            while examined < PROJECTION_SCAN_BUDGET && index > 0 {
                index -= 1;
                let line = &lines[index];
                examined += 1;
                if panel.view.matches(line) {
                    projection.rows.push_front(Arc::clone(line));
                }
                frontier = Some(line.sequence);
            }
        }
    }

    projection.frontier = frontier;
    projection.ceiling = ceiling;
    projection.updating = !projection.covers(oldest, newest);
    work.scanned = examined;
}

/// Hand the current projection/history view to the reader's reconciliation.
///
/// The rules themselves — survivor identity, the ordered filter fallbacks,
/// completeness-gated eviction, the local expiry acknowledgement and the
/// locating state — live in [`crate::monitor_reader`]; this system only
/// supplies the frame. It never scans the history: every reader decision is a
/// binary search or a constant-time check over the projection's own sorted
/// rows, so the one capped metadata scan per update stays the projection
/// pass's.
pub(crate) fn resolve_reader(
    mut panel: ResMut<MonitorPanel>,
    projection: Res<MonitorProjection>,
    history: Option<Res<MonitorHistory>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !panel.open {
        panel.reader.on_hidden();
        return;
    }
    let metrics = reader_metrics(reader_columns(&windows));
    let frame = reader_frame(&projection, history.as_deref(), metrics);
    panel.reader.reconcile(&frame);
}

/// The borrowed reader frame for the current projection/history view. The
/// projection owns its examined-band facts; the reader only consumes them.
pub(super) fn reader_frame<'a>(
    projection: &'a MonitorProjection,
    history: Option<&MonitorHistory>,
    metrics: Metrics,
) -> ReaderFrame<'a> {
    let oldest = history.and_then(MonitorHistory::oldest_sequence);
    ReaderFrame::new(
        projection.rows(),
        metrics,
        oldest,
        projection.is_updating(),
        projection.age_complete(oldest),
    )
}

/// The newest retained admission identity, if the history exists.
pub(super) fn newest_retained(history: Option<&MonitorHistory>) -> Option<u64> {
    history.and_then(MonitorHistory::newest_sequence)
}
