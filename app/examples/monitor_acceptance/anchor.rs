//! The rendered reading-anchor checks and the row reads they share.

use crate::prelude::*;

pub(crate) fn check_anchor_top(harness: &mut Harness, world: &mut World, snap: &Snap, label: &str) {
    let Some(geometry) = snap.geometry() else {
        return;
    };
    let Some(row) = snap.top_visible_row() else {
        harness
            .report
            .fail(format!("{label}: a top visible row is rendered"));
        return;
    };
    let delta = row.top - geometry.body_top;
    let within_row = geometry.body_top - row.top;
    let rendered = world
        .get::<unmei_app::drawer::MonitorRow>(row.entity)
        .map(|row| row.0);
    let stored = world
        .resource::<unmei_app::drawer::MonitorPanel>()
        .anchor()
        .map(|anchor| anchor.sequence);
    println!(
        "  {label} anchor: {delta:.1}px from the body top, within-row {within_row:.1} of {:.1}px, id {rendered:?} (stored {stored:?})",
        row.height
    );
    // The anchor must be the actual top-visible event: the drawn row's
    // admission identity is the reader's stored anchor, it is not pushed below
    // the body top, and it is not scrolled fully out. A fully clipped anchor
    // whose row is above the body cannot pass as the next row.
    harness.report.check(
        format!("{label}: the top visible event is the stored anchor ({rendered:?})"),
        rendered.is_some() && rendered == stored && delta <= 1.0 && within_row < row.height,
    );
}

/// The top-visible event is the reader's stored anchor and is genuinely
/// part-way into its own row — the within-row offset a fractional wheel
/// produces, observed from the rendered geometry rather than read back from the
/// reader's stored offset.
pub(crate) fn check_anchor_fractional(
    harness: &mut Harness,
    world: &mut World,
    snap: &Snap,
    label: &str,
) {
    let Some(geometry) = snap.geometry() else {
        return;
    };
    let Some(row) = snap.top_visible_row() else {
        harness
            .report
            .fail(format!("{label}: a top visible row is rendered"));
        return;
    };
    let within_row = geometry.body_top - row.top;
    let rendered = world
        .get::<unmei_app::drawer::MonitorRow>(row.entity)
        .map(|row| row.0);
    let stored = world
        .resource::<unmei_app::drawer::MonitorPanel>()
        .anchor()
        .map(|anchor| anchor.sequence);
    println!(
        "  {label} within-row offset: {within_row:.1} of {:.1}px, id {rendered:?} (stored {stored:?})",
        row.height
    );
    harness.report.check(
        format!("{label}: the within-row offset is fractional ({within_row:.1}px)"),
        rendered.is_some() && rendered == stored && within_row > 0.5 && within_row < row.height,
    );
}

/// Every rendered text under the surface, for the credential-row checks.
pub(crate) fn rendered_surface_text(world: &mut World) -> String {
    let snap = crate::harness::snap(world);
    let mut out = String::new();
    for node in &snap.nodes {
        if let Some(text) = snap.text(node.entity) {
            out.push_str(text);
            out.push('\n');
        }
    }
    out
}

/// Record every row body currently drawn in the log body, so a paging walk can
/// prove the whole retained startup list was readable on the rendered rows
/// rather than only in the disk log.
pub(crate) fn read_visible_rows(world: &mut World, harness: &mut Harness) {
    let snap = crate::harness::snap(world);
    let rows = snap.rows();
    let bytes: usize = rows.iter().map(|row| row.body(&snap).len()).sum();
    println!("  read {} visible rows ({bytes} bytes)", rows.len());
    for row in rows {
        harness.read_bodies.push(row.body(&snap));
    }
}
