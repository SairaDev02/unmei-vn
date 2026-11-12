//! The reader's position walks and clamps over the projected rows.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::monitor::MonitorLine;

use super::metrics::{Metrics, OFFSET_EPSILON};

/// The index of the row carrying `sequence`, if it is projected.
pub(super) fn index_of(rows: &VecDeque<Arc<MonitorLine>>, sequence: u64) -> Option<usize> {
    let index = rows.partition_point(|row| row.sequence < sequence);
    rows.get(index)
        .filter(|row| row.sequence == sequence)
        .map(|_| index)
}

/// The first (oldest) projected row newer than `sequence`.
pub(super) fn first_after(rows: &VecDeque<Arc<MonitorLine>>, sequence: u64) -> Option<usize> {
    let index = rows.partition_point(|row| row.sequence <= sequence);
    (index < rows.len()).then_some(index)
}

/// The nearest projected row older than `sequence`.
pub(super) fn last_before(rows: &VecDeque<Arc<MonitorLine>>, sequence: u64) -> Option<usize> {
    let index = rows.partition_point(|row| row.sequence < sequence);
    index.checked_sub(1)
}

/// Clamp the anchor's within-row offset to its current estimated wrapping, and
/// drop it entirely when the remaining window fits the viewport (nothing below
/// the anchor can be scrolled into view).
pub(super) fn clamp_anchor_offset(
    rows: &VecDeque<Arc<MonitorLine>>,
    index: usize,
    offset: f32,
    metrics: Metrics,
) -> f32 {
    let mut offset = metrics.clamp_offset(&rows[index], offset);
    let window_lines: f32 = rows
        .iter()
        .skip(index)
        .take(metrics.window_rows())
        .map(|row| metrics.height_units(row))
        .sum();
    if window_lines <= metrics.visible_lines() {
        offset = 0.0;
    }
    offset
}

/// The topmost anchor whose remaining tail still fills at least one viewport:
/// the bottom position of the reader's event range. Everything at or newer
/// than this index is the same bottom view, so a deliberate step there resumes
/// following.
pub(super) fn bottom_anchor(rows: &VecDeque<Arc<MonitorLine>>, metrics: Metrics) -> usize {
    let mut lines = 0.0;
    let mut index = rows.len();
    while index > 0 && lines < metrics.visible_lines() {
        index -= 1;
        lines += metrics.height_units(&rows[index]);
    }
    index
}

/// Move the top-of-view position `lines` display lines toward older events.
pub(super) fn walk_older(
    rows: &VecDeque<Arc<MonitorLine>>,
    metrics: Metrics,
    mut index: usize,
    mut offset: f32,
    lines: f32,
) -> (usize, f32) {
    // The starting offset is consumed by the walk itself, so it is not added
    // to the distance again (finding A08: that double-counted a fractional
    // start position).
    let mut remaining = lines;
    while remaining > 0.0 {
        if offset > 0.0 {
            let step = offset.min(remaining);
            offset -= step;
            remaining -= step;
            continue;
        }
        if index == 0 {
            offset = 0.0;
            break;
        }
        index -= 1;
        offset = metrics.height_units(&rows[index]);
    }
    (index, metrics.clamp_offset(&rows[index], offset))
}

/// Move the top-of-view position `lines` display lines toward newer events.
/// Returns whether the walk ran off the end of the newest row — the deliberate
/// bottom arrival.
pub(super) fn walk_newer(
    rows: &VecDeque<Arc<MonitorLine>>,
    metrics: Metrics,
    mut index: usize,
    mut offset: f32,
    lines: f32,
) -> (usize, f32, bool) {
    let mut remaining = lines;
    let mut at_end = false;
    while remaining > 0.0 {
        let row_lines = metrics.height_units(&rows[index]);
        let rest = (row_lines - offset).max(0.0);
        if rest > remaining {
            offset += remaining;
            break;
        }
        remaining -= rest;
        if index + 1 < rows.len() {
            index += 1;
            offset = 0.0;
        } else {
            offset = row_lines - OFFSET_EPSILON;
            at_end = true;
            break;
        }
    }
    (index, metrics.clamp_offset(&rows[index], offset), at_end)
}
