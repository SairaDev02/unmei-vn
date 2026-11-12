//! Rendered row identity, the filter-fallback expectation and the newest marker.

use crate::prelude::*;

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct RowMark {
    pub(crate) target: String,
    pub(crate) body: String,
}

pub(crate) fn rendered_row_marks(snap: &Snap) -> Vec<RowMark> {
    snap.rows()
        .into_iter()
        .map(|row| RowMark {
            target: row.target(snap),
            body: row.body(snap),
        })
        .collect()
}

/// What the filter fallback should do with the recorded anchor: keep it when it
/// still matches, else the **first surviving newer** match, else the nearest
/// preceding one (`docs/runtime-hardening.md` § Monitor → Interaction). The
/// newer-first order is the parent contract; the rendered walk derives its
/// expectation from the real pre-flip rows instead of assuming an identity.
pub(crate) struct ExpectedFallback {
    pub(crate) rule: &'static str,
    pub(crate) target: String,
    pub(crate) body: String,
}

pub(crate) fn expected_fallback(harness: &Harness) -> ExpectedFallback {
    let rows = &harness.rows_before_flip;
    let index = harness.anchor_index.unwrap_or(0);
    let matches = |mark: &RowMark| mark.target.starts_with("unmei::");
    let (rule, chosen) = if rows.get(index).is_some_and(matches) {
        ("kept", rows.get(index))
    } else if let Some(next) = (index..rows.len()).find(|i| matches(&rows[*i])) {
        ("first-newer", rows.get(next))
    } else if let Some(previous) = (0..index).rev().find(|i| matches(&rows[*i])) {
        ("nearest-earlier", rows.get(previous))
    } else {
        ("none", None)
    };
    ExpectedFallback {
        rule,
        target: chosen.map(|mark| mark.target.clone()).unwrap_or_default(),
        body: chosen.map(|mark| mark.body.clone()).unwrap_or_default(),
    }
}

/// Whether the retained history holds a matching event on both sides of the
/// middle pair — the "both sides" property of the fallback case, checked
/// against the retained history rather than the rendered window (which starts
/// at the anchor and so cannot show the earlier side).
pub(crate) fn retained_matches_bracket_the_middle_pair(world: &mut World) -> bool {
    let Some(history) = world.get_resource::<unmei_app::monitor::MonitorHistory>() else {
        return false;
    };
    let mut pair_first: Option<u64> = None;
    let mut earlier = false;
    let mut newer = false;
    for line in history.lines() {
        if line.target == "bevy_acceptance_middle" {
            pair_first = Some(line.sequence);
        } else if line.target.starts_with("unmei::") {
            if pair_first.is_none() {
                earlier = true;
            } else {
                newer = true;
            }
        }
    }
    earlier && newer
}

/// Whether no matching event is retained after the earlier-only marker — the
/// "earlier-only" side of the fallback case, checked against the retained
/// history so an off-screen newer match cannot slip past the rendered window.
pub(crate) fn retained_has_no_match_after_the_marker(world: &mut World) -> bool {
    let Some(history) = world.get_resource::<unmei_app::monitor::MonitorHistory>() else {
        return false;
    };
    let mut seen_marker = false;
    for line in history.lines() {
        if line.body().contains("the earlier-only match marker") {
            seen_marker = true;
        } else if seen_marker && line.target.starts_with("unmei::") {
            return false;
        }
    }
    seen_marker
}

/// The newest-retained marker: a warm wash and a 2px left inset.
pub(crate) fn marked_rows(snap: &Snap) -> Vec<RowSnap> {
    snap.rows()
        .into_iter()
        .filter(|row| {
            snap.node(row.entity).is_some_and(|node| {
                node.background == Some(NEWEST_WASH) && node.border == Some(NEWEST_INSET)
            })
        })
        .collect()
}

pub(crate) fn row_with_body(snap: &Snap, needle: &str) -> Option<RowSnap> {
    snap.rows()
        .into_iter()
        .find(|row| row.body(snap).contains(needle))
}

pub(crate) fn column_widths(snap: &Snap, row: &RowSnap) -> Vec<f32> {
    row.parts
        .iter()
        .map(|part| snap.node(*part).map_or(0.0, |node| node.size.x))
        .collect()
}
