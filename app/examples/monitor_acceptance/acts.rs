//! The acts several plans share: drawer open/close, bookkeeping and reporting.

use crate::prelude::*;

pub(crate) fn open_drawer(world: &mut World, harness: &mut Harness) {
    let snap = crate::harness::snap(world);
    harness.before = Some(snap.underlying());
    harness.hud_before = Some(snap.hud());
    tap_key(world, harness, KeyCode::F3);
}

pub(crate) fn close_drawer(world: &mut World, harness: &mut Harness) {
    tap_key(world, harness, KeyCode::Escape);
}

/// Park the drawer's focus on the idempotent **Follow latest** control. A
/// pointer activation names its control, so a later Space/Enter under Monitor
/// activates that one instead of whatever the previous pointer step left
/// focused (notably `Close`).
pub(crate) fn park_focus(world: &mut World, harness: &mut Harness) {
    tap_control(world, harness, "Follow latest");
}

pub(crate) fn note_wait(world: &mut World, harness: &mut Harness) {
    harness.wait_before = wait_of(world);
}

pub(crate) fn note_draft(world: &mut World, harness: &mut Harness) {
    harness.draft_before = crate::harness::snap(world).draft();
}

/// Emit the queued numbered fixtures, at most 99 per frame — inside both the
/// 128-event ingestion and 256-slot pending production budgets.
pub(crate) fn drain_emissions(_world: &mut World, harness: &mut Harness) {
    if harness.emit_from >= harness.emit_to {
        return;
    }
    let count = (harness.emit_to - harness.emit_from).min(99);
    fixture::emit_sequence(harness.emit_from, count);
    harness.emit_from += count;
}

/// Whether the status footer is currently marked `updating view`.
pub(crate) fn record_updating(world: &mut World, harness: &mut Harness) {
    if harness.saw_updating {
        return;
    }
    let snap = crate::harness::snap(world);
    harness.saw_updating = snap
        .status()
        .is_some_and(|status| status.contains("updating view"));
}

/// Record the platform and GPU the rendered walk actually ran on.
pub(crate) fn report_platform(world: &mut World, harness: &mut Harness) {
    println!(
        "  platform: {} {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY
    );
    if let Some(info) = world.get_resource::<bevy::render::renderer::RenderAdapterInfo>() {
        println!(
            "  adapter: {} | backend {:?} | driver {} ({})",
            info.name, info.backend, info.driver, info.driver_info
        );
    }
    harness
        .report
        .pass("the platform, backend and adapter are recorded in this run's output");
}

/// The drawer's projection pass has finished (`updating view` is gone).
pub(crate) fn projection_settled(world: &mut World, _harness: &mut Harness) -> bool {
    status_of(world).0.is_some_and(|status| !status.updating)
}

pub(crate) fn drain_and_record(world: &mut World, harness: &mut Harness) {
    drain_emissions(world, harness);
    record_updating(world, harness);
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------
