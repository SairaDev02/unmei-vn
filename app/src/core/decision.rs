//! The Decisions seam: the request/answer state, the resolver and factory
//! resources, and the one apply path a completed answer takes.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool, Task};
use unmei_engine::decision::{resolve_or_fallback, DecisionError, ResolvedDecision};
use unmei_engine::runner::Wait;
use unmei_engine::{DecisionResolver, ScriptRunner};

use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};
use crate::menu::MenuState;
use crate::read_log::ReadLogState;
use crate::slots::PendingAutosave;
use crate::Runtime;

/// A decision request in flight. Dropping it cancels the task (a load mid-flight
/// replaces it).
#[derive(Resource, Default)]
pub struct PendingDecision(pub Option<Task<ResolvedDecision>>);

/// A completed answer buffered while the Esc overlay is open. The request keeps
/// running and its answer is held here, not applied: on resume it lands the
/// runner at the next wait; a load discards it. `None` means nothing is held.
#[derive(Resource, Default)]
pub struct HeldDecision(pub Option<ResolvedDecision>);

/// The Decisions resolver, if one is configured (the environment provided a key,
/// or a test injected a fake). `None` means every decision point resolves
/// straight to its author fallback.
#[derive(Resource)]
pub struct DecisionChannel {
    pub resolver: Option<Arc<dyn DecisionResolver>>,
}

/// Builds a Decisions resolver from a raw key. `main` installs the concrete
/// `DecisionsClient`; tests install a fake so the in-session resolver rebuild is
/// drivable without a network (`docs/title-and-menu.md` § Decisions settings
/// §2/§4.2). `None` means a commit can store a key but cannot rebuild a
/// resolver, which is the hermetic offline posture.
pub type ResolverBuilder = Arc<dyn Fn(&str) -> Arc<dyn DecisionResolver> + Send + Sync>;

/// The resolver factory resource, if one is installed.
#[derive(Resource, Clone, Default)]
pub struct DecisionFactory {
    /// The builder, or `None` when no rebuild is possible (the hermetic offline
    /// posture).
    pub builder: Option<ResolverBuilder>,
}

/// A completed decision task lands the runner on `<prefix>.<action>`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn poll_decision(
    mut runtime: ResMut<Runtime>,
    mut pending: ResMut<PendingDecision>,
    mut held: ResMut<HeldDecision>,
    mut autosave: ResMut<PendingAutosave>,
    mut read_log: ResMut<ReadLogState>,
    mut hold: ResMut<DefensiveHold>,
    mut notice: ResMut<Notice>,
    menu: Res<MenuState>,
) {
    let Some(task) = pending.0.as_mut() else {
        return;
    };
    let Some(resolved) = block_on(poll_once(task)) else {
        return;
    };
    pending.0 = None;

    // Presentation pause: a completed answer is held, not applied. The script
    // does not advance until resume, when `drive_script` lands it.
    if menu.open {
        held.0 = Some(resolved);
        return;
    }

    apply_resolution(
        &resolved,
        &mut runtime,
        &mut autosave,
        &mut read_log,
        &mut hold,
        &mut notice,
    );
}

/// Log and land one resolved answer: a model answer is an autosave write point,
/// an author fallback is not, and a label miss is row 12's defensive hold.
pub(super) fn apply_resolution(
    resolved: &ResolvedDecision,
    runtime: &mut Runtime,
    autosave: &mut PendingAutosave,
    read_log: &mut ReadLogState,
    hold: &mut DefensiveHold,
    notice: &mut Notice,
) {
    log_resolution(resolved, notice, hold);
    match runtime.runner.resolve_decision(&resolved.action) {
        Ok(()) => {
            // The decision park is left; a landing back on the same coordinates
            // is still a fresh arrival.
            read_log.on_park_departure();
            // A resolution that actually landed — from a model answer *or* its
            // author fallback — is a read-log flush moment. The divergence from
            // the autosave's churn rule is deliberate: knowledge and a
            // checkpoint answer different questions. A defensive hold is *not*
            // a flush moment, so a failed resolution requests nothing.
            read_log.request_flush();
            // A model answer is an autosave write point; an author fallback is
            // not, so a transient outage never churns the checkpoint.
            if resolved.fallback_reason.is_none() {
                autosave.arm();
            }
        }
        // Row 12 — defensive: the loader's validation makes it unreachable.
        // `report` engages the hold from the row's posture.
        Err(error) => failure::report(
            Failure::new(FailureRow::DecisionLabelMissing, error.to_string()),
            notice,
            hold,
        ),
    }
}

/// How a parked decision resolves: now, or on a task in flight.
pub(super) enum DecisionStart {
    /// Resolve now (no resolver, or a config error that forces the fallback).
    Immediate(ResolvedDecision),
    /// A live request in flight.
    Task(Task<ResolvedDecision>),
}

/// Build the decision request for a parked [`Wait::Decision`].
///
/// Missing resolver, missing NPC config, or a missing pinned model all resolve
/// to the author fallback *now* — the slice's offline posture. Otherwise the
/// call runs on the async compute pool and is polled by [`poll_decision`].
pub(super) fn start_decision(
    channel: &DecisionChannel,
    runner: &ScriptRunner,
    wait: &Wait,
) -> Option<DecisionStart> {
    let Wait::Decision {
        npc_id, question, ..
    } = wait
    else {
        return None;
    };
    let npc = runner.pack().npcs.get(npc_id)?;
    let config = npc.decisions.get(question)?.clone();
    let state = runner.state().clone();

    let Some(resolver) = channel.resolver.clone() else {
        return Some(DecisionStart::Immediate(resolve_or_fallback(
            &config,
            Err(DecisionError::Config("no Decisions client".to_string())),
        )));
    };
    let Some(model) = npc.model.clone() else {
        return Some(DecisionStart::Immediate(resolve_or_fallback(
            &config,
            Err(DecisionError::Config(format!(
                "npc `{npc_id}` has no pinned decision model"
            ))),
        )));
    };

    let question_id = question.clone();
    let pool = AsyncComputeTaskPool::get();
    let task = pool.spawn(async move {
        resolver
            .resolve(&model, &question_id, &config, &state)
            .await
    });
    Some(DecisionStart::Task(task))
}

fn log_resolution(resolved: &ResolvedDecision, notice: &mut Notice, hold: &mut DefensiveHold) {
    match &resolved.fallback_reason {
        // Rows 8–10: the authored fallback plays; the player is never told.
        Some(reason) => failure::report(
            Failure::new(
                FailureRow::for_decision_reason(reason),
                format!("`{}`: {reason}", resolved.action),
            ),
            notice,
            hold,
        ),
        None => log::info!(
            target: "unmei::decision",
            "decision -> `{}` via `{}`",
            resolved.action,
            resolved.model.as_deref().unwrap_or("?")
        ),
    }
}
