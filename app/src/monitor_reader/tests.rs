//! The reader's tests: the anchor, follow and walk contracts.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::monitor::MonitorLine;

use super::*;

/// The 12px/1.45 Fira Mono line pitch at the reference size.
const LINE_PX: f32 = 12.0 * 1.45;
/// The single-line row pitch: the line plus the row's 1px padding on both
/// edges — the reader's navigation unit.
const ROW_LINE_PX: f32 = LINE_PX + 2.0;
/// The fixed target column's capacity (224px / 7.2px advance).
const TARGET_COLUMNS: f32 = 224.0 / 7.2;

/// The 1280px reference metrics: a 113-cell message column, the ~15-line
/// viewport and the 32-row display window.
fn reference() -> Metrics {
    Metrics::new(113.0, TARGET_COLUMNS, LINE_PX, 2.0, ROW_LINE_PX, 15.0, 32)
}

/// A nominal single-line retained event: a short `unmei::` target and a
/// short body both fit their columns.
fn line(sequence: u64) -> Arc<MonitorLine> {
    with_body(sequence, "unmei::seq", &format!("event {sequence}"))
}

/// A retained event with an explicit body, so one fixture row can wrap
/// without touching the others.
fn with_body(sequence: u64, target: &str, message: &str) -> Arc<MonitorLine> {
    let text = format!("{sequence:010}ms INFO  {target} {message}");
    let body_start = text.len() - message.len();
    Arc::new(MonitorLine {
        sequence,
        elapsed_ms: sequence,
        level: log::Level::Info,
        target: target.to_string(),
        target_display: target.to_string(),
        text,
        body_start,
        truncated: false,
    })
}

/// The rows `range` as the projection hands them over, oldest first.
fn rows(range: std::ops::Range<u64>) -> VecDeque<Arc<MonitorLine>> {
    range.map(line).collect()
}

/// A settled frame: the whole retained history is projected and the
/// examined band has reached its oldest entry.
fn settled<'a>(rows: &'a VecDeque<Arc<MonitorLine>>, metrics: Metrics) -> ReaderFrame<'a> {
    ReaderFrame::new(
        rows,
        metrics,
        rows.front().map(|row| row.sequence),
        false,
        true,
    )
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

// -- Navigation and the one metric derivation -------------------------

/// Three display lines older from a fractional position consume the
/// starting offset exactly once (finding A08), and newer motion returns to
/// the same fractional position.
#[test]
fn fractional_navigation_consumes_the_start_offset_once() {
    let rows = rows(0..100);
    let metrics = reference();
    let frame = settled(&rows, metrics);
    let mut reader = Reader::default();

    // Production navigation establishes the fractional position: the
    // scrollbar's mid travel over 100 single-line rows is event 42 with
    // half a row read into it.
    reader.place_scrollbar(&frame, 0.5);
    let anchor = reader.anchor().expect("the drag anchored the reader");
    assert_eq!(anchor.sequence, 42);
    assert_close(anchor.offset_lines, 0.5);
    assert!(!reader.is_following());

    // 42 + 0.5 moved three single-line units older is 39 + 0.5.
    reader.move_older(&frame, 3.0);
    let anchor = reader.anchor().expect("the walk anchored the reader");
    assert_eq!(anchor.sequence, 39);
    assert_close(anchor.offset_lines, 0.5);

    // Three units newer is 42 + 0.5 again.
    reader.move_newer(&frame, 3.0);
    let anchor = reader.anchor().expect("the walk anchored the reader");
    assert_eq!(anchor.sequence, 42);
    assert_close(anchor.offset_lines, 0.5);

    // The paging path is the same walk: a full-viewport 15-line step from
    // 42 + 0.5 lands on 27 + 0.5, not on a whole line.
    reader.move_older(&frame, 15.0);
    let anchor = reader.anchor().expect("the walk anchored the reader");
    assert_eq!(anchor.sequence, 27);
    assert_close(anchor.offset_lines, 0.5);
}

/// The first older step starts from the bottom view's top, and landing on
/// that view with newer motion is the deliberate bottom arrival that
/// resumes following.
#[test]
fn newer_motion_resumes_following_only_at_the_deliberate_bottom() {
    let rows = rows(0..100);
    let metrics = reference();
    let frame = settled(&rows, metrics);
    let mut reader = Reader::default();
    assert!(reader.is_following());

    // The 15-line viewport's tail is events 85..99, so the first older walk
    // from following must not jump to the newest row.
    reader.move_older(&frame, 18.0);
    let anchor = reader.anchor().expect("the walk anchored the reader");
    assert_eq!(
        anchor.sequence, 67,
        "the bottom view's top, three rows older"
    );
    assert_eq!(anchor.offset_lines, 0.0);
    assert!(!reader.is_following());

    // Newer motion back onto the bottom view resumes following.
    reader.move_newer(&frame, 18.0);
    assert!(reader.is_following());
    assert_eq!(reader.anchor(), None);
}

/// The one metric derivation wraps per column and counts wide glyphs as
/// two cells; a byte-truncated event keeps its single warm line.
#[test]
fn metric_extents_wrap_per_column_and_count_wide_glyphs() {
    let metrics = reference();
    assert_close(metrics.height_units(&line(1)), 1.0);
    assert_close(metrics.text_lines(&line(1)), 1.0);

    // A 400-cell body over the 113-cell message column is four lines.
    let tall = with_body(2, "unmei::seq", &"x".repeat(400));
    assert_close(metrics.text_lines(&tall), 4.0);

    // The fixed target column wraps in its own 31.1-cell extent even with
    // a short body.
    let long_target = with_body(3, &format!("unmei::{}", "t".repeat(40)), "short");
    assert_close(metrics.text_lines(&long_target), 2.0);

    // Wide CJK glyphs cost two cells: 60 of them fill 120 cells and wrap
    // where 60 ordinary characters do not.
    assert_close(
        metrics.text_lines(&with_body(4, "unmei::seq", &"w".repeat(60))),
        1.0,
    );
    assert_close(
        metrics.text_lines(&with_body(5, "unmei::seq", &"界".repeat(60))),
        2.0,
    );

    // A byte-truncated event keeps its preserved single warm line.
    let mut truncated = with_body(6, "unmei::seq", &"x".repeat(500));
    Arc::make_mut(&mut truncated).truncated = true;
    assert_close(metrics.text_lines(&truncated), 1.0);
}

/// A rewrap clamps the stored offset inside the row's new extent instead of
/// scrolling the anchor off the top, and a tail that fits the viewport
/// drops the offset entirely.
#[test]
fn a_rewrap_clamps_the_offset_into_the_rows_new_extent() {
    let mut wrapped = rows(0..100);
    wrapped[42] = with_body(42, "unmei::seq", &"x".repeat(400));
    let metrics = reference();
    let frame = settled(&wrapped, metrics);
    let mut reader = Reader::default();
    reader.place_scrollbar(&frame, 0.5);
    let anchor = reader.anchor().expect("the drag anchored the reader");
    assert_eq!(anchor.sequence, 42);
    assert!(
        anchor.offset_lines > 1.0,
        "the four-line row admits a fractional offset past one line: {}",
        anchor.offset_lines
    );

    // A fullscreen-like rewrap collapses event 42 to one line.
    let wide = Metrics::new(400.0, TARGET_COLUMNS, LINE_PX, 2.0, ROW_LINE_PX, 15.0, 32);
    let frame = settled(&wrapped, wide);
    reader.reconcile(&frame);
    let anchor = reader.anchor().expect("the anchor survived the rewrap");
    assert_eq!(anchor.sequence, 42, "the event is kept across a rewrap");
    assert!(
        anchor.offset_lines < 1.0,
        "the offset clamped into the one-line row: {}",
        anchor.offset_lines
    );

    // When the whole remaining tail fits the viewport the offset drops to
    // zero rather than shifting the short content up.
    let short = rows(42..45);
    let frame = settled(&short, metrics);
    reader.reconcile(&frame);
    assert_eq!(
        reader.anchor(),
        Some(MonitorAnchor {
            sequence: 42,
            offset_lines: 0.0
        })
    );
    let view = reader.view(&frame);
    assert_eq!(view.range, 0..3);
    assert_close(view.spacer_lines, 12.0);
    assert_close(view.shift_lines, 0.0);
}

/// The view's visible range, shift, spacer and scrollbar mapping follow the
/// reader's range and never retain a row.
#[test]
fn the_view_maps_the_anchored_range_to_the_scrollbar() {
    let rows = rows(0..100);
    let metrics = reference();
    let frame = settled(&rows, metrics);
    let mut reader = Reader::default();

    let view = reader.view(&frame);
    assert!(view.following);
    assert_eq!(view.range, 68..100, "following shows the newest window");
    assert_close(view.thumb_size, 0.15);
    assert_close(view.thumb_top, 0.85);

    reader.place_scrollbar(&frame, 0.5);
    let view = reader.view(&frame);
    assert!(!view.following);
    assert_eq!(view.range, 42..74);
    assert_eq!(view.anchor.map(|anchor| anchor.sequence), Some(42));
    assert_close(view.shift_lines, 0.5);
    assert_close(view.spacer_lines, 0.0);
    assert_close(view.thumb_size, 0.15);
    assert_close(view.thumb_top, 0.42);
    assert!(!view.expired);
    assert!(!view.pending);
}

// -- Reconciliation ----------------------------------------------------

/// A surviving anchor keeps its event identity and fractional offset
/// through arrivals, and follow stays off.
#[test]
fn a_surviving_anchor_is_kept_through_arrivals() {
    let before = rows(0..100);
    let metrics = reference();
    let frame = settled(&before, metrics);
    let mut reader = Reader::default();
    reader.place_scrollbar(&frame, 0.5);
    let anchor = reader.anchor();

    let after = rows(0..105);
    let frame = settled(&after, metrics);
    reader.reconcile(&frame);
    assert_eq!(reader.anchor(), anchor);
    assert!(!reader.is_following());
    assert!(!reader.is_pending());
}

/// Eviction clamps to the oldest surviving match only once age completeness
/// proves it; before that the old anchor is kept and the loss acknowledged.
#[test]
fn eviction_clamps_only_when_age_completeness_proves_the_survivor() {
    let full = rows(0..100);
    let metrics = reference();
    let frame = settled(&full, metrics);
    let mut reader = Reader::default();
    reader.place_scrollbar(&frame, 0.5);
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(42));

    // Retention moved past the anchor, but the pass has not examined the
    // oldest survivor: the anchor stays where it was, the loss is
    // acknowledged, and the reader waits.
    let surviving = rows(80..180);
    let incomplete = ReaderFrame::new(&surviving, metrics, Some(80), true, false);
    reader.reconcile(&incomplete);
    assert_eq!(
        reader.anchor().map(|anchor| anchor.sequence),
        Some(42),
        "the anchor is not moved before the survivor is proven"
    );
    assert!(reader.is_expired());
    assert!(reader.is_pending());
    assert!(!reader.is_following());

    // Once the examined band proves the oldest survivor, the anchor clamps
    // to it.
    let complete = ReaderFrame::new(&surviving, metrics, Some(80), false, true);
    reader.reconcile(&complete);
    assert_eq!(
        reader.anchor(),
        Some(MonitorAnchor {
            sequence: 80,
            offset_lines: 0.0
        })
    );
    assert!(!reader.is_pending());
    assert!(reader.is_expired());
}

/// The expiry acknowledgement survives hiding and repeated eviction, and
/// clears only on a deliberate navigation, a filter change or Follow
/// latest.
#[test]
fn the_expiry_acknowledgement_clears_only_deliberately() {
    let full = rows(0..100);
    let metrics = reference();
    let frame = settled(&full, metrics);
    let mut reader = Reader::default();
    reader.place_scrollbar(&frame, 0.5);

    let evicted = rows(80..180);
    let frame = ReaderFrame::new(&evicted, metrics, Some(80), false, true);
    reader.reconcile(&frame);
    assert!(reader.is_expired());
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(80));

    // Hiding preserves anchor, follow choice and acknowledgement.
    reader.on_hidden();
    assert!(reader.is_expired());
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(80));
    assert!(!reader.is_following());

    // A repeated eviction keeps it set.
    let later = rows(200..300);
    let frame = ReaderFrame::new(&later, metrics, Some(200), false, true);
    reader.reconcile(&frame);
    assert!(reader.is_expired());
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(200));

    // Deliberate navigation acknowledges it.
    reader.move_older(&frame, 3.0);
    assert!(!reader.is_expired());

    // A filter change acknowledges it and preserves the anchor.
    let evicted_again = rows(300..400);
    let frame = ReaderFrame::new(&evicted_again, metrics, Some(300), false, true);
    reader.reconcile(&frame);
    assert!(reader.is_expired());
    reader.on_view_change();
    assert!(!reader.is_expired());
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(300));

    // Follow latest clears it and resumes following.
    let evicted_once_more = rows(400..500);
    let frame = ReaderFrame::new(&evicted_once_more, metrics, Some(400), false, true);
    reader.reconcile(&frame);
    assert!(reader.is_expired());
    reader.follow_latest();
    assert!(reader.is_following());
    assert_eq!(reader.anchor(), None);
    assert!(!reader.is_expired());
}

/// A filter change keeps a surviving match, else falls back to the first
/// surviving newer event, else to the nearest preceding one.
#[test]
fn filter_fallbacks_keep_same_first_newer_then_nearest_earlier() {
    let full = rows(0..100);
    let metrics = reference();
    let frame = settled(&full, metrics);
    let mut reader = Reader::default();
    reader.place_scrollbar(&frame, 0.725); // event 65, no fractional offset
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(65));

    // A matching anchor survives unchanged.
    reader.reconcile(&frame);
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(65));

    // The anchor no longer matches, but a newer match survives.
    let newer: VecDeque<_> = full
        .iter()
        .filter(|row| row.sequence != 65)
        .cloned()
        .collect();
    let frame = ReaderFrame::new(&newer, metrics, Some(0), false, true);
    reader.reconcile(&frame);
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(66));
    assert_eq!(reader.anchor().map(|anchor| anchor.offset_lines), Some(0.0));
    assert!(!reader.is_following());

    // No newer match survives: the nearest preceding one is used.
    let older = rows(0..65);
    let frame = ReaderFrame::new(&older, metrics, Some(0), false, true);
    reader.reconcile(&frame);
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(64));
    assert!(!reader.is_following());
}

/// An empty matching view is never a follow reset; later arrivals anchor
/// below the reader's intent instead of silently enabling follow.
#[test]
fn an_empty_view_and_later_arrivals_never_enable_following() {
    let full = rows(0..100);
    let metrics = reference();
    let frame = settled(&full, metrics);
    let mut reader = Reader::default();
    reader.place_scrollbar(&frame, 0.5);
    assert!(!reader.is_following());

    let empty: VecDeque<Arc<MonitorLine>> = VecDeque::new();
    let frame = ReaderFrame::new(&empty, metrics, Some(0), false, true);
    reader.reconcile(&frame);
    assert_eq!(reader.anchor(), None);
    assert!(!reader.is_following(), "an empty view never enables follow");
    assert!(!reader.is_pending());

    // The first later matching arrival anchors the empty view.
    let later = rows(100..101);
    let frame = ReaderFrame::new(&later, metrics, Some(0), false, true);
    reader.reconcile(&frame);
    assert_eq!(
        reader.anchor(),
        Some(MonitorAnchor {
            sequence: 100,
            offset_lines: 0.0
        })
    );
    assert!(!reader.is_following());

    // Further arrivals accumulate below the anchored event.
    let later = rows(100..105);
    let frame = ReaderFrame::new(&later, metrics, Some(0), false, true);
    reader.reconcile(&frame);
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(100));
    assert!(!reader.is_following());
}

/// An incomplete pass that already holds matching rows locates the anchor
/// without presenting the matching tail as its position.
#[test]
fn an_incomplete_pass_locates_without_showing_the_tail() {
    let full = rows(0..100);
    let metrics = reference();
    let frame = settled(&full, metrics);
    let mut reader = Reader::default();
    reader.place_scrollbar(&frame, 0.5);
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(42));

    // The restarted pass's first window is the newest block, which already
    // holds matching rows while event 42 is unscanned.
    let newest_block = rows(500..520);
    let frame = ReaderFrame::new(&newest_block, metrics, Some(0), true, false);
    reader.reconcile(&frame);
    assert_eq!(
        reader.anchor().map(|anchor| anchor.sequence),
        Some(42),
        "the anchor is kept while locating"
    );
    assert!(reader.is_pending());
    assert_eq!(
        reader.view(&frame).range,
        0..0,
        "the matching tail is not shown as the reader's location"
    );

    // The pass reaches the anchor and the view jumps straight to it.
    let all = rows(0..520);
    let frame = ReaderFrame::new(&all, metrics, Some(0), false, true);
    reader.reconcile(&frame);
    assert!(!reader.is_pending());
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(42));
    assert_eq!(reader.view(&frame).range, 42..74);
}

/// Home selects the oldest currently displayed event and keeps it as older
/// matches are prepended by the incomplete refresh.
#[test]
fn home_keeps_the_oldest_projected_event_as_older_matches_prepend() {
    let metrics = reference();
    let first = rows(128..300);
    let frame = ReaderFrame::new(&first, metrics, Some(0), true, false);
    let mut reader = Reader::default();
    reader.home(&frame);
    assert_eq!(reader.anchor().map(|anchor| anchor.sequence), Some(128));
    assert!(!reader.is_following());

    let more = rows(0..300);
    let frame = ReaderFrame::new(&more, metrics, Some(0), true, false);
    reader.reconcile(&frame);
    assert_eq!(
        reader.anchor().map(|anchor| anchor.sequence),
        Some(128),
        "the selected event is not retargeted to the newly arrived oldest"
    );
}

/// A pending refresh does not freeze navigation: a deliberate command
/// starts from the top of the bottom view the placeholder shows.
#[test]
fn navigation_while_pending_starts_from_the_bottom_view() {
    let metrics = reference();
    let full = rows(0..100);
    let mut reader = Reader::default();
    reader.place_scrollbar(&settled(&full, metrics), 0.5);

    // The restarted pass's first window holds none of the anchor's block.
    let newest_block = rows(500..520);
    let frame = ReaderFrame::new(&newest_block, metrics, Some(0), true, false);
    reader.reconcile(&frame);
    assert!(reader.is_pending());

    // A deliberate older step starts from the 15-line bottom view's top
    // (event 505), not from the newest row.
    reader.move_older(&frame, 3.0);
    let anchor = reader.anchor().expect("the walk anchored the reader");
    assert_eq!(anchor.sequence, 502);
    assert_eq!(anchor.offset_lines, 0.0);
    assert!(!reader.is_pending());
    assert!(!reader.is_following());
}
