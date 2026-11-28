//! The settle rule (spec §4.3), kept apart from `take.rs` so each file stays
//! under the source-size limit.

use std::sync::atomic::Ordering;
use std::time::Duration;

use unmei_engine::keiko::{Fault, KeyCheckFault};

use super::super::finding::{rules, Finding, Location};
use super::super::observe::Observation;
use super::{frame, observe_step, TakeRequest, TakeState};
use bevy::prelude::*;

/// The settle rule: zero-duration frames until the world is quiet, or 600
/// frames, whichever comes first (spec §4.3).
pub(super) fn settle(
    app: &mut App,
    state: &mut TakeState,
    request: &TakeRequest,
) -> Result<(), Finding> {
    let held_decision = request.case.headers.iter().any(|line| {
        matches!(
            line.header,
            unmei_engine::keiko::Header::Fault {
                fault: Fault::DecisionHold { .. } | Fault::DecisionSlow { .. }
            }
        )
    });
    let held_key_check = request.case.headers.iter().any(|line| {
        matches!(
            line.header,
            unmei_engine::keiko::Header::Fault {
                fault: Fault::KeyCheck {
                    verdict: KeyCheckFault::Hold
                }
            }
        )
    });

    let mut previous: Option<Observation> = None;
    let mut quiet = 0u32;
    // A live answer arrives on the network's schedule, so its pending frames
    // wait in real time and do not spend the quiet-frame budget (spec §6).
    let live_deadline = std::time::Instant::now() + LIVE_ANSWER_LIMIT;
    let asset_deadline = std::time::Instant::now() + PENDING_WORK_LIMIT;
    while quiet < 600 {
        frame(app, &state.timed, &state.clock, Duration::ZERO);
        observe_step(app, state);
        let observation = &state.observation;
        let decision_pending = observation.decision.is_some() && !(held_decision || held_key_check);
        if decision_pending && request.live.is_some() {
            if std::time::Instant::now() >= live_deadline {
                return Err(Finding::new(
                    request.case_file.clone(),
                    Location::File,
                    rules::UNSETTLED,
                    "the live decision did not answer within 60 s",
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
            previous = None;
            continue;
        }
        let assets_settled = app
            .world()
            .get_resource::<crate::presentation::assets::AssetWatch>()
            .is_none_or(|watch| watch.is_empty());
        let released = state.key_check_release.load(Ordering::Relaxed);
        let key_check_pending = app
            .world()
            .get_resource::<crate::core::PendingKeyCheck>()
            .is_some_and(|pending| pending.task.is_some())
            && (!held_key_check || released);
        let stable = previous
            .as_ref()
            .is_some_and(|previous| previous.same_world(observation));
        if !decision_pending && (key_check_pending || !assets_settled) && stable {
            if std::time::Instant::now() >= asset_deadline {
                return Err(Finding::new(
                    request.case_file.clone(),
                    Location::File,
                    rules::UNSETTLED,
                    format!(
                        "an asset load or key check did not finish within {} s",
                        PENDING_WORK_LIMIT.as_secs()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        quiet += 1;
        if !decision_pending && !key_check_pending && assets_settled && stable {
            state.records.push(observation.clone());
            return Ok(());
        }
        previous = Some(observation.clone());
    }
    Err(Finding::new(
        request.case_file.clone(),
        Location::File,
        rules::UNSETTLED,
        "the world did not become quiet within 600 frames",
    ))
}

/// How long a live answer may take before the take gives up (spec §6).
const LIVE_ANSWER_LIMIT: Duration = Duration::from_secs(60);

/// An asset load or key check runs on the task pools, not in a frame, so a
/// starved machine can spend far more than 600 frames before one finishes (a
/// background still `Loading` after 56 s). A frame that waits only on such work,
/// with the observation unchanged, waits in real time and spends no quiet
/// frame (spec §4.3).
const PENDING_WORK_LIMIT: Duration = Duration::from_secs(120);
