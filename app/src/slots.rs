//! App-side slot persistence: the `saved_at` stamp and the autosave write
//! points.
//!
//! The engine owns the four slot files ([`unmei_engine::save`]); this module
//! owns *when* the app writes them and the one value the app must add. Every
//! write reaches [`write_stamped`], so [`unmei_engine::SaveFile::capture`] stays
//! pure and `saved_at` is always the app's clock ([ADR 0006]). A player-facing
//! manual save goes through [`write_manual`], which stamps and then adds the
//! session log's one `saved to` line; the autosave calls `write_stamped` alone.
//!
//! The autosave is written for exactly three events: a scene **crossing**, a
//! player **choice** resolve, and a decision resolved from a **model** answer
//! ([`docs/runtime-hardening.md` § Save slots §4]). The first two are observed
//! where they happen ([`crate::input`] and [`crate::core::drive_script`]); the
//! third where the answer lands ([`crate::core::poll_decision`]). Each arms
//! [`PendingAutosave`]; [`write_pending_autosave`] then writes once, after the
//! script has parked and the staging snapshot matches the screen. A **fallback**
//! answer, a **load** and **boot** never arm it.
//!
//! [ADR 0006]: ../../docs/adr/0006-save-slots-on-disk-format-and-version-policy.md
//! [`docs/runtime-hardening.md` § Save slots §4]: ../../docs/runtime-hardening.md

use bevy::prelude::*;
use unmei_engine::save::{self, SaveError, SaveFile, Slot};
use unmei_engine::{RunOutcome, ScriptRunner, StagingState};

use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};
use crate::Runtime;

/// Set in the frame an autosave-worthy event happens, consumed by
/// [`write_pending_autosave`].
///
/// One bool deliberately coalesces events that coincide in a frame (a choice
/// that also crosses a scene, say) into a single write, so arming is
/// idempotent and one autosave is never written twice for one frame.
#[derive(Resource, Default)]
pub struct PendingAutosave(pub bool);

impl PendingAutosave {
    /// Mark an autosave-worthy event for this frame.
    pub fn arm(&mut self) {
        self.0 = true;
    }

    /// Arm when a run crossed into a different scene file: one of the three
    /// autosave write points, checked in one place.
    pub fn arm_if_crossed(&mut self, outcome: &RunOutcome) {
        if outcome.entered_scene.is_some() {
            self.arm();
        }
    }
}

/// The app's `saved_at` clock: Unix epoch seconds, or `None` when the clock is
/// before the epoch (or otherwise unavailable). `None` is left as `None` rather
/// than stamped `0`, so a menu never shows a fabricated 1970 time (ADR 0006).
pub fn now_epoch_seconds() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs())
}

/// Capture the position, stamp it with the app's clock, and write one slot
/// atomically. The single writer every manual save and autosave goes through,
/// so `capture` never needs to know about time.
pub fn write_stamped(
    runner: &ScriptRunner,
    staging: &StagingState,
    slot: Slot,
) -> Result<(), SaveError> {
    let mut save = SaveFile::capture(runner, staging);
    save.saved_at = now_epoch_seconds();
    save::write_slot(runner.pack(), slot, &save)
}

/// [`write_stamped`] for a player-facing manual save: the F5 shortcut and the
/// Esc overlay's Save both go through it, so the session log announces
/// `unmei: saved to <path>` once whichever surface asked. The silent autosave
/// keeps using [`write_stamped`].
pub fn write_manual(
    runner: &ScriptRunner,
    staging: &StagingState,
    slot: Slot,
) -> Result<(), SaveError> {
    write_stamped(runner, staging, slot)?;
    crate::session_lines::saved_to(&save::save_path_for(runner.pack(), slot));
    Ok(())
}

/// Write the autosave once for an armed [`PendingAutosave`].
///
/// The flag is disarmed *before* the write, so one event is one attempt whether
/// it lands or not. A failed write notices `couldn't autosave` (row 15) at each
/// event; a successful one says nothing.
pub fn write_pending_autosave(
    runtime: Res<Runtime>,
    mut pending: ResMut<PendingAutosave>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    if !pending.0 {
        return;
    }
    pending.0 = false;

    if let Err(error) = write_stamped(&runtime.runner, &runtime.staging, Slot::Autosave) {
        let path = save::save_path_for(runtime.runner.pack(), Slot::Autosave);
        failure::report(
            Failure::new(
                FailureRow::Autosave,
                format!("`{}`: {error}", path.display()),
            ),
            &mut notice,
            &mut hold,
        );
    }
}
