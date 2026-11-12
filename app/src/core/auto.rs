//! The auto-mode dwell clock: how long a fully revealed say-line waits before
//! auto mode advances it.

use bevy::prelude::*;
use unmei_engine::runner::Wait;

use super::SkipState;
use crate::failure::{self, DefensiveHold, Notice};
use crate::presentation::play::TextReveal;
use crate::read_log::ReadLogState;
use crate::settings::{self, SettingsState};
use crate::Runtime;

/// How long a fully revealed say-line dwells before auto mode advances it.
pub(crate) const AUTO_DWELL_SECONDS: f32 = 2.0;

/// Auto mode's in-play dwell timer: which park the countdown belongs to, the
/// seconds left, and whether the dwell has already started for it. The
/// `play.auto` preference lives in the settings file; this is timing the
/// headless runner never sees, and it is deliberately **not** save data — a load
/// and a choice pick both leave it and the preference alone.
///
/// The countdown is keyed to the runner's parked position — `(scene_id, pc)` —
/// rather than the [`Wait`] alone: two consecutive identical say-lines are
/// indistinguishable by their text (the recorded reveal quirk), but they are
/// distinct parks with distinct `pc`, so the second still gets the full dwell.
#[derive(Resource, Debug)]
pub struct AutoDwell {
    park: Option<(String, usize)>,
    remaining: f32,
    /// Set the frame the parked line first becomes fully revealed, so the dwell
    /// spends the whole [`AUTO_DWELL_SECONDS`] from that moment rather than a
    /// frame's fraction of it.
    counting: bool,
}

impl Default for AutoDwell {
    fn default() -> Self {
        AutoDwell {
            park: None,
            remaining: AUTO_DWELL_SECONDS,
            counting: false,
        }
    }
}

impl AutoDwell {
    /// Forget the park and the countdown; the next parked say-line starts a
    /// fresh full dwell. Also called on a load, so a restored line never
    /// inherits elapsed time from the line it replaced.
    pub(crate) fn reset(&mut self) {
        self.park = None;
        self.remaining = AUTO_DWELL_SECONDS;
        self.counting = false;
    }
}

/// The auto-mode dwell: when `play.auto` is on and a say-line is fully revealed,
/// count down [`AUTO_DWELL_SECONDS`] and then step the runner exactly as a press
/// would. This is the **only** action auto takes — never `choose`, never
/// `resolve_decision`, never a resolver call — so a choice-wait, a decision-wait
/// (including a request still in flight) and the end of the scene all hold
/// indefinitely. The system is gated to the play path by the plugin, so the
/// overlay freezes the countdown without resetting it.
///
/// While either skip mode runs it wins: auto's dwell neither counts nor fires,
/// and its stale park is forgotten so that skip's end starts a fresh full dwell
/// (`docs/runtime-hardening.md` § Text skip §6).
#[allow(clippy::too_many_arguments)]
pub(crate) fn auto_dwell(
    time: Res<Time>,
    settings: Option<Res<SettingsState>>,
    skip: Res<SkipState>,
    reveal: Res<TextReveal>,
    mut read_log: ResMut<ReadLogState>,
    mut runtime: ResMut<Runtime>,
    mut auto: ResMut<AutoDwell>,
    mut hold: ResMut<DefensiveHold>,
    mut notice: ResMut<Notice>,
) {
    if skip.is_on() {
        auto.reset();
        return;
    }
    let enabled = settings
        .as_deref()
        .map(SettingsState::auto)
        .unwrap_or(false);
    let Some(wait) = runtime.runner.wait().cloned() else {
        auto.reset();
        return;
    };
    if !enabled || !matches!(wait, Wait::Say { .. }) {
        // Auto off, or parked where it must never act: a choice, a decision
        // (answered or in flight), or the end of the scene.
        auto.reset();
        return;
    }

    let park = (runtime.runner.scene_id().to_string(), runtime.runner.pc());
    if auto.park.as_ref() != Some(&park) {
        auto.reset();
        auto.park = Some(park);
    }

    let speed = settings::effective_text_speed(settings.as_deref());
    if !reveal.is_whole_for(&wait, speed) {
        // Still revealing, or not yet armed for this park: hold the full dwell
        // for the moment the line becomes whole.
        auto.remaining = AUTO_DWELL_SECONDS;
        auto.counting = false;
        return;
    }
    if !auto.counting {
        // The line just became whole: start the full dwell now, without
        // spending this frame's delta on it.
        auto.counting = true;
        auto.remaining = AUTO_DWELL_SECONDS;
        return;
    }

    auto.remaining -= time.delta_secs();
    if auto.remaining > 0.0 {
        return;
    }

    auto.reset();
    // Auto is the runner leaving the park; a same-position loop back still
    // counts as a fresh arrival for the mark rule.
    read_log.on_park_departure();
    if let Err(error) = runtime.runner.advance_say() {
        // Row 13 — defensive: the wait check above makes it unreachable.
        failure::report_runner_call(error, &mut notice, &mut hold);
    }
}
