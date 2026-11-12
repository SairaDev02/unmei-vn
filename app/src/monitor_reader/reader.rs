//! The Monitor's logical reading state: follow, anchor, expiry and locating.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Arc;

use crate::monitor::MonitorLine;

use super::metrics::{Metrics, OFFSET_EPSILON};
use super::walk::{
    bottom_anchor, clamp_anchor_offset, first_after, index_of, last_before, walk_newer, walk_older,
};
use super::MonitorAnchor;

/// The borrowed current view one reader operation reads: the ordered projected
/// rows, the drawer's current metric inputs and the projection's own
/// completeness facts.
///
/// Every fact comes from the same current projection/history state the caller
/// then renders; the frame is valid only for that snapshot and is never
/// retained. Age completeness and a fully settled filter pass stay distinct
/// facts: the examined band can prove the oldest matching survivor while the
/// pass still has newer arrivals to examine.
pub(crate) struct ReaderFrame<'a> {
    pub(crate) rows: &'a VecDeque<Arc<MonitorLine>>,
    pub(crate) metrics: Metrics,
    pub(crate) oldest_retained: Option<u64>,
    pub(crate) updating: bool,
    pub(crate) age_complete: bool,
}

impl<'a> ReaderFrame<'a> {
    pub(crate) fn new(
        rows: &'a VecDeque<Arc<MonitorLine>>,
        metrics: Metrics,
        oldest_retained: Option<u64>,
        updating: bool,
        age_complete: bool,
    ) -> Self {
        Self {
            rows,
            metrics,
            oldest_retained,
            updating,
            age_complete,
        }
    }
}

/// The reader's read-only view for the frame it was derived from: the logical
/// state and the numbers the drawer turns into Nodes and pixels. Nothing here
/// is cached — a view is a pure derivation valid only for its frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReaderView {
    /// Whether the reader follows the newest matching line.
    pub(crate) following: bool,
    /// The stable reading anchor, if any.
    pub(crate) anchor: Option<MonitorAnchor>,
    /// Whether retention evicted the anchor and the reader has not
    /// acknowledged it.
    pub(crate) expired: bool,
    /// Whether the projection is still locating the anchor.
    pub(crate) pending: bool,
    /// The visible slice of the frame's rows; empty while locating or on a
    /// genuinely empty view.
    pub(crate) range: Range<usize>,
    /// Display lines scrolled past the anchor row's top.
    pub(crate) shift_lines: f32,
    /// The short-tail spacer above the rows group, in display lines.
    pub(crate) spacer_lines: f32,
    /// The scrollbar thumb's height, normalized over the track.
    pub(crate) thumb_size: f32,
    /// The scrollbar thumb's top, normalized over the track's travel.
    pub(crate) thumb_top: f32,
}

/// The Monitor's logical reading state: the follow choice, the stable anchor,
/// the local expiry acknowledgement and the transient locating flag.
///
/// It lives inside the existing [`crate::drawer::MonitorPanel`] resource and is
/// changed only through these operations; the drawer keeps input, widgets and
/// pixels, and reads back a [`ReaderView`].
#[derive(Debug)]
pub(crate) struct Reader {
    following: bool,
    anchor: Option<MonitorAnchor>,
    expired: bool,
    pending: bool,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            following: true,
            anchor: None,
            expired: false,
            pending: false,
        }
    }
}

impl Reader {
    /// Whether the reader follows the newest matching line.
    pub(crate) fn is_following(&self) -> bool {
        self.following
    }

    /// The stable reading anchor, if any.
    pub(crate) fn anchor(&self) -> Option<MonitorAnchor> {
        self.anchor
    }

    /// Whether retention evicted the anchor and the reader has not
    /// acknowledged it.
    pub(crate) fn is_expired(&self) -> bool {
        self.expired
    }

    /// Whether the projection is still locating the anchor.
    pub(crate) fn is_pending(&self) -> bool {
        self.pending
    }

    /// The one deliberate, idempotent return to following latest: the **Follow
    /// latest** control, `End` and a deliberate bottom arrival all share it. It
    /// is also an expiry acknowledgement.
    pub(crate) fn follow_latest(&mut self) {
        self.following = true;
        self.anchor = None;
        self.pending = false;
        self.expired = false;
    }

    /// A filter change preserves the anchor (re-resolved against the new
    /// predicate) and acknowledges a loss.
    pub(crate) fn on_view_change(&mut self) {
        self.expired = false;
    }

    /// Hidden: preserve the anchor, follow choice and acknowledgement; drop
    /// only the transient locating flag.
    pub(crate) fn on_hidden(&mut self) {
        self.pending = false;
    }

    /// Resolve the stable anchor against the current frame.
    ///
    /// The anchor identity is preserved whenever it survives and matches.
    /// Otherwise the one defined fallback applies in retained admission order:
    /// the **first surviving matching newer event**, else the **nearest
    /// preceding match**, else an empty view — never a silent switch to
    /// following. Retention eviction sets the local `older lines expired`
    /// acknowledgement and clamps to the oldest surviving matching event once
    /// the projection proves it has reached that survivor. While the pass is
    /// still locating the anchor the reader stays `pending`, so the body shows
    /// its locating placeholder rather than the matching tail.
    pub(crate) fn reconcile(&mut self, frame: &ReaderFrame<'_>) {
        if self.following {
            self.pending = false;
            return;
        }
        let rows = frame.rows;
        let metrics = frame.metrics;

        let Some(anchor) = self.anchor else {
            // A resolved empty view: the first later matching arrival anchors
            // at the oldest new match, so later arrivals accumulate below the
            // fold instead of silently enabling follow.
            if rows.is_empty() {
                self.pending = false;
            } else if frame.age_complete {
                self.anchor = Some(MonitorAnchor {
                    sequence: rows.front().expect("rows are non-empty").sequence,
                    offset_lines: 0.0,
                });
                self.pending = false;
            } else {
                self.pending = true;
            }
            return;
        };

        if let Some(index) = index_of(rows, anchor.sequence) {
            // The anchor survived and still matches: keep the event, clamp only
            // the within-row offset against its current estimated wrapping.
            self.anchor = Some(MonitorAnchor {
                sequence: anchor.sequence,
                offset_lines: clamp_anchor_offset(rows, index, anchor.offset_lines, metrics),
            });
            self.pending = false;
            return;
        }

        if frame
            .oldest_retained
            .is_some_and(|oldest| oldest > anchor.sequence)
        {
            // Retention evicted the anchor. The acknowledgement stays set until
            // a deliberate navigation, filter change or Follow latest clears
            // it.
            self.expired = true;
            if frame.age_complete {
                self.anchor = rows.front().map(|row| MonitorAnchor {
                    sequence: row.sequence,
                    offset_lines: 0.0,
                });
                self.pending = false;
            } else {
                self.pending = true;
            }
            return;
        }

        // The anchor exists in retained history but not under the current
        // predicate: attempt the defined fallback once the bounded pass has
        // settled every entry, so no closer match can be missed.
        if !frame.updating {
            if let Some(index) = first_after(rows, anchor.sequence) {
                self.anchor = Some(MonitorAnchor {
                    sequence: rows[index].sequence,
                    offset_lines: 0.0,
                });
                self.pending = false;
            } else if let Some(index) = last_before(rows, anchor.sequence) {
                self.anchor = Some(MonitorAnchor {
                    sequence: rows[index].sequence,
                    offset_lines: 0.0,
                });
                self.pending = false;
            } else {
                // No matches under this predicate: an empty view, never a
                // filter or history reset and never an automatic follow.
                self.anchor = None;
                self.pending = false;
            }
        } else {
            // The anchor is retained but not yet re-projected: stay pending
            // even with zero current-predicate rows, so the body shows the
            // truthful `updating view` placeholder rather than a false empty
            // state.
            self.pending = true;
        }
    }

    /// Move `lines` display lines toward older events. Deliberate navigation is
    /// an expiry acknowledgement. Following — or a pending refresh showing the
    /// newest rows as a placeholder — starts from the top of the current
    /// bottom view, so the first scroll-away never jumps.
    pub(crate) fn move_older(&mut self, frame: &ReaderFrame<'_>, lines: f32) {
        let rows = frame.rows;
        if rows.is_empty() {
            return;
        }
        self.expired = false;
        let Some(start) = self.start_position(frame) else {
            return;
        };
        let (index, offset) = walk_older(rows, frame.metrics, start.0, start.1, lines);
        self.anchor = Some(MonitorAnchor {
            sequence: rows[index].sequence,
            offset_lines: offset,
        });
        self.following = false;
        self.pending = false;
    }

    /// Move `lines` display lines toward newer events. Landing at (or past) the
    /// bottom view is the deliberate bottom arrival that resumes following.
    pub(crate) fn move_newer(&mut self, frame: &ReaderFrame<'_>, lines: f32) {
        let rows = frame.rows;
        if rows.is_empty() {
            return;
        }
        self.expired = false;
        let Some(start) = self.start_position(frame) else {
            return;
        };
        let (index, offset, at_end) = walk_newer(rows, frame.metrics, start.0, start.1, lines);
        if at_end || index >= bottom_anchor(rows, frame.metrics) {
            self.follow_latest();
            return;
        }
        self.anchor = Some(MonitorAnchor {
            sequence: rows[index].sequence,
            offset_lines: offset,
        });
        self.following = false;
        self.pending = false;
    }

    /// Home: the oldest **currently projected** event. Deliberate navigation is
    /// an expiry acknowledgement, and the selected event is kept as older
    /// matches are prepended by an incomplete refresh.
    pub(crate) fn home(&mut self, frame: &ReaderFrame<'_>) {
        let rows = frame.rows;
        if rows.is_empty() {
            return;
        }
        self.expired = false;
        self.anchor = rows.front().map(|row| MonitorAnchor {
            sequence: row.sequence,
            offset_lines: 0.0,
        });
        self.following = false;
        self.pending = false;
    }

    /// Place the reader from a scrollbar drag: the thumb's centre follows the
    /// cursor over the track's travel, the bottom of the travel is a deliberate
    /// bottom arrival, and the fractional remainder becomes the within-row
    /// offset. Deliberate navigation is an expiry acknowledgement.
    pub(crate) fn place_scrollbar(&mut self, frame: &ReaderFrame<'_>, track_fraction: f32) {
        let rows = frame.rows;
        let len = rows.len();
        if len == 0 {
            return;
        }
        let metrics = frame.metrics;
        let bottom = bottom_anchor(rows, metrics);
        let visible_events = len.saturating_sub(bottom).max(1);
        let thumb = (visible_events as f32 / len as f32).clamp(0.0, 1.0);
        let travel = (1.0 - thumb).max(0.0);
        if travel <= f32::EPSILON {
            return;
        }
        self.expired = false;
        let thumb_top = (track_fraction - thumb / 2.0).clamp(0.0, travel);
        let fraction = thumb_top / travel;
        if fraction >= 1.0 - OFFSET_EPSILON {
            self.follow_latest();
            return;
        }
        let position = fraction * bottom as f32;
        let index = (position.floor() as usize).min(len - 1);
        let offset = (position - index as f32) * metrics.height_units(&rows[index]);
        self.anchor = Some(MonitorAnchor {
            sequence: rows[index].sequence,
            offset_lines: metrics.clamp_offset(&rows[index], offset),
        });
        self.following = false;
        self.pending = false;
    }

    /// The read-only view of the reader's state and geometry for this frame.
    pub(crate) fn view(&self, frame: &ReaderFrame<'_>) -> ReaderView {
        let rows = frame.rows;
        let metrics = frame.metrics;
        let range = if self.following {
            let start = rows.len().saturating_sub(metrics.window_rows());
            start..rows.len()
        } else if self.pending {
            // The anchor is still being located: nothing is shown rather than
            // the matching tail.
            0..0
        } else {
            match self
                .anchor
                .and_then(|anchor| index_of(rows, anchor.sequence))
            {
                Some(index) => index..(index + metrics.window_rows()).min(rows.len()),
                None => 0..0,
            }
        };
        let anchored = !self.following && !self.pending && self.anchor.is_some();
        let content_lines: f32 = rows
            .range(range.clone())
            .map(|row| metrics.height_units(row))
            .sum();
        let shift_lines = if anchored {
            self.anchor.map_or(0.0, |anchor| anchor.offset_lines)
        } else {
            0.0
        };
        let spacer_lines = if anchored {
            (metrics.visible_lines() - content_lines).max(0.0)
        } else {
            0.0
        };

        // The scrollbar thumb: its size is the visible share of the event
        // range, and its travel maps the anchor's range to the track.
        let len = rows.len();
        let bottom = bottom_anchor(rows, metrics);
        let visible_events = len.saturating_sub(bottom).max(1);
        let thumb_size = if len == 0 {
            1.0
        } else {
            (visible_events as f32 / len as f32).clamp(0.0, 1.0)
        };
        let thumb_travel = (1.0 - thumb_size).max(0.0);
        let thumb_top = match self
            .anchor
            .and_then(|anchor| index_of(rows, anchor.sequence))
        {
            Some(index) if !self.pending => {
                let fraction = if bottom == 0 {
                    0.0
                } else {
                    (index as f32 / bottom as f32).clamp(0.0, 1.0)
                };
                fraction * thumb_travel
            }
            // Following, a pending refresh or no anchor all park the thumb at
            // the bottom of its travel.
            _ => thumb_travel,
        };

        ReaderView {
            following: self.following,
            anchor: self.anchor,
            expired: self.expired,
            pending: self.pending,
            range,
            shift_lines,
            spacer_lines,
            thumb_size,
            thumb_top,
        }
    }

    /// The walk's starting position: the settled anchor while reading, or the
    /// top of the bottom view while following or locating one.
    fn start_position(&self, frame: &ReaderFrame<'_>) -> Option<(usize, f32)> {
        if self.following || self.pending {
            return Some((bottom_anchor(frame.rows, frame.metrics), 0.0));
        }
        let anchor = self.anchor?;
        let index = index_of(frame.rows, anchor.sequence)?;
        Some((index, anchor.offset_lines))
    }
}
