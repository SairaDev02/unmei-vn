//! Stepping the runner: the one script pipeline that carries a park to the next
//! wait, starts a decision request, and applies any held answer first.

use bevy::prelude::*;
use unmei_engine::runner::Wait;

use super::decision::{apply_resolution, start_decision, DecisionStart};
use super::{DecisionChannel, HeldDecision, PendingDecision, SkipState};
use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};
use crate::read_log::ReadLogState;
use crate::session_lines;
use crate::slots::PendingAutosave;
use crate::Runtime;

/// Step the runner until it parks, staging as it goes; if it parks on a decision,
/// start the (async) call or resolve the fallback when no resolver is configured.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drive_script(
    mut runtime: ResMut<Runtime>,
    channel: Res<DecisionChannel>,
    mut pending: ResMut<PendingDecision>,
    mut held: ResMut<HeldDecision>,
    mut autosave: ResMut<PendingAutosave>,
    mut read_log: ResMut<ReadLogState>,
    mut skip: ResMut<SkipState>,
    mut hold: ResMut<DefensiveHold>,
    mut notice: ResMut<Notice>,
) {
    // A held answer applies the moment the overlay closes; the runner then
    // continues to its next wait in this same call, before the autosave writes.
    if let Some(resolved) = held.0.take() {
        apply_resolution(
            &resolved,
            &mut runtime,
            &mut autosave,
            &mut read_log,
            &mut hold,
            &mut notice,
        );
        if hold.0 {
            return;
        }
    }

    if pending.0.is_some() {
        return;
    }

    if runtime.runner.wait().is_none() {
        let outcome = runtime.runner.run();
        runtime.staging.apply_all(&outcome.staged);
        // A `jump` that landed in a different scene file is an autosave write
        // point; the app has applied the staged commands, so the snapshot it
        // captures matches the screen.
        autosave.arm_if_crossed(&outcome);
        // A crossing is a read-log flush moment and a scene entry.
        if let Some(scene_id) = &outcome.entered_scene {
            read_log.request_flush();
            session_lines::entered_scene(scene_id);
        }
        // The force-advance exemption applies only when the advance itself
        // lands on a say-park. A choice, a decision or the end of the scene
        // passed no say-line, so the exemption is void and the next say-park
        // (a decision fallback's landing, say) marks normally — the exemption
        // attaches to the advance, not to everything that follows while the key
        // is held (`docs/runtime-hardening.md` § Text skip §8).
        if !matches!(&outcome.wait, Some(Wait::Say { .. })) {
            skip.clear_force_advance();
        }
        if outcome.wait.is_none() {
            // Row 22 is not a failure: no log, no Notice, no hold.
            failure::report(
                Failure::new(FailureRow::EndOfScript, ""),
                &mut notice,
                &mut hold,
            );
            return;
        }
    }

    let Some(wait) = runtime.runner.wait().cloned() else {
        return;
    };
    if !matches!(wait, Wait::Decision { .. }) {
        return;
    }

    // A decision park is not a say-park: the seen toggle cannot pass it. It
    // ends here, before an offline author fallback resolves and runs to its
    // landing in this same frame, so the decision stop is never bypassed and
    // the cue never shows a mode the next frame would refuse
    // (`docs/runtime-hardening.md` § Text skip §3). A *force* hold suspends the
    // seen toggle without changing it, so only the toggle is ended here, and
    // only when no hold is effective: a held `Ctrl` still waits at the park
    // and resumes while down.
    if !skip.is_force_active() {
        skip.end();
    }

    match start_decision(&channel, &runtime.runner, &wait) {
        Some(DecisionStart::Immediate(resolved)) => {
            apply_resolution(
                &resolved,
                &mut runtime,
                &mut autosave,
                &mut read_log,
                &mut hold,
                &mut notice,
            );
            if !hold.0 {
                // Offline fallback lands in the same frame so the UI never
                // flashes an end-of-scene state between resolve and the park.
                let outcome = runtime.runner.run();
                runtime.staging.apply_all(&outcome.staged);
                // A fallback resolve writes no autosave of its own, but a
                // scene crossing it lands on still does.
                autosave.arm_if_crossed(&outcome);
                if let Some(scene_id) = &outcome.entered_scene {
                    read_log.request_flush();
                    session_lines::entered_scene(scene_id);
                }
            }
        }
        Some(DecisionStart::Task(task)) => pending.0 = Some(task),
        // Row 11 — defensive: the loader's validation makes it unreachable.
        // `report` engages the hold from the row's posture.
        None => failure::report(
            Failure::new(
                FailureRow::DecidePointMissing,
                "decide point has no matching NPC/question in the loaded pack",
            ),
            &mut notice,
            &mut hold,
        ),
    }
}
