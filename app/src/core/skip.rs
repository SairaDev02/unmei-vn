//! The seen/force skip machine: its state, its in-play step, and the physical
//! `Ctrl` mirror.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use unmei_engine::runner::Wait;

use crate::failure::{self, DefensiveHold, Notice};
use crate::presentation::play::TextReveal;
use crate::read_log::ReadLogState;
use crate::settings::{self, SettingsState};
use crate::Runtime;

/// How long a skip step holds a completed line before advancing. About three
/// frames at 60 fps (~20 lines/s); not scaled by line length and not a `play`
/// field (`docs/runtime-hardening.md` § Text skip §2).
pub(crate) const SKIP_DWELL_SECONDS: f32 = 0.05;

/// The seen/force skip machine (`docs/runtime-hardening.md` § Text skip §1–§5).
///
/// Off at launch, off on entering play, and gone on exit: it is play-session
/// state, not a preference. `Tab` engages the **seen** toggle only on an
/// **arrival-read** say-park, and any player input, a park it cannot pass (an
/// arrival-unread say-line, a choice, a decision, the end of the scene) or a
/// discontinuity (a load, a New Game, title→play, a Timeline jump) ends it. A
/// held `Ctrl` is the **force** hold: a fresh press observed in play arms it, it
/// skips every say-line read or not, holds at any park it cannot pass and
/// resumes while the key is still down. Both modes share the one step machine;
/// force overrides seen, and releasing it restores an otherwise eligible seen
/// toggle. Its only action is `advance_say`; the ordinary `drive_script`
/// pipeline executes the commands and jumps after it.
#[derive(Resource, Debug)]
pub struct SkipState {
    /// The `Tab` toggle is engaged — the cue and auto suppression read this.
    seen: bool,
    /// A fresh `Ctrl` press was observed in play and no release or discontinuity
    /// has happened since. This distinguishes a live hold from a key that was
    /// already down when a gate opened, so a press made under the overlay or the
    /// title never starts a skip.
    force_armed: bool,
    /// The force hold is effective this frame: armed **and** physically down.
    /// The cue and auto suppression read this through [`Self::is_on`].
    force_active: bool,
    /// The park the reveal has been completed for: `(scene id, pc)`. `None`
    /// before the first step at a park, so the reveal is completed exactly once
    /// per park.
    park: Option<(String, usize)>,
    /// Seconds left before the next advance.
    remaining: f32,
    /// Set by [`Self::engage`]/[`Self::arm_force`]: the first park observation
    /// after engaging starts the dwell **without** spending that frame's delta,
    /// because the line only became whole on that frame. Without it the
    /// engagement frame's delta would shorten the first hold by up to a frame.
    fresh: bool,
    /// A force step advanced the runner: the next park transition's read mark is
    /// withheld (the exemption belongs to the advance, not the line;
    /// `docs/runtime-hardening.md` § Text skip §8).
    force_advance: bool,
}

impl Default for SkipState {
    fn default() -> Self {
        SkipState {
            seen: false,
            force_armed: false,
            force_active: false,
            park: None,
            remaining: SKIP_DWELL_SECONDS,
            fresh: false,
            force_advance: false,
        }
    }
}

impl SkipState {
    /// Whether the seen toggle is engaged.
    pub fn is_seen(&self) -> bool {
        self.seen
    }

    /// Whether a fresh `Ctrl` press is latched and no release or discontinuity
    /// has cleared it. The play-frame mirror combines this with the physical key
    /// to set [`Self::is_force_active`].
    pub fn is_force_armed(&self) -> bool {
        self.force_armed
    }

    /// Whether the force hold is effective this frame.
    pub fn is_force_active(&self) -> bool {
        self.force_active
    }

    /// Whether either mode is effective: the `SKIP` cue shows and the auto dwell
    /// is suppressed while this is true.
    pub fn is_on(&self) -> bool {
        self.seen || self.force_active
    }

    /// Engage the seen toggle. The caller has already checked the arrival-read
    /// gate; the first step at the current park completes its reveal.
    pub fn engage(&mut self) {
        self.seen = true;
        self.park = None;
        self.remaining = SKIP_DWELL_SECONDS;
        self.fresh = true;
    }

    /// Arm the force hold on a fresh `Ctrl` press observed in play. A hold
    /// starts a fresh step at the current park.
    pub fn arm_force(&mut self) {
        self.force_armed = true;
        if !self.force_active {
            self.park = None;
            self.remaining = SKIP_DWELL_SECONDS;
            self.fresh = true;
        }
    }

    /// Mirror the physical key on each play frame: the hold is effective only
    /// while it is armed and still down.
    pub fn set_force_active(&mut self, active: bool) {
        self.force_active = active;
    }

    /// A `Ctrl` release observed on any frame — including under the overlay or
    /// the title — invalidates the latch, so a press made behind a gate can
    /// never start a skip when play resumes. A no-op when no hold is latched, so
    /// the every-frame mirror never disturbs the seen toggle's own dwell.
    pub fn release_force(&mut self) {
        if self.force_armed || self.force_active {
            self.force_armed = false;
            self.force_active = false;
            self.park = None;
            self.remaining = SKIP_DWELL_SECONDS;
            self.fresh = false;
        }
    }

    /// Record a force step's advance: the next park observation withholds the
    /// arrival's read mark.
    pub fn mark_force_advance(&mut self) {
        self.force_advance = true;
    }

    /// Consume the force-advance exemption.
    pub fn take_force_advance(&mut self) -> bool {
        std::mem::take(&mut self.force_advance)
    }

    /// Void a pending force-advance exemption: the advance landed on a non-say
    /// park (a choice, a decision or the end of the scene), so it passed no
    /// say-line and the next say-park marks normally — the exemption attaches to
    /// the advance, not to everything that follows while the key is held.
    pub fn clear_force_advance(&mut self) {
        self.force_advance = false;
    }

    /// End the seen toggle at the player's bidding (a cancelling press, `Tab`, or
    /// a park skip cannot pass). The force hold is deliberately untouched.
    pub fn end(&mut self) {
        self.seen = false;
        self.park = None;
        self.remaining = SKIP_DWELL_SECONDS;
        self.fresh = false;
    }

    /// A discontinuity — a load, a New Game, title→play or a Timeline jump —
    /// ends the seen toggle, disarms the force hold and resets both clocks
    /// (`docs/runtime-hardening.md` § Text skip §5). A held `Ctrl` then needs a
    /// release and a fresh press in play before it runs again.
    pub fn disarm(&mut self) {
        self.end();
        self.force_armed = false;
        self.force_active = false;
        self.force_advance = false;
    }
}

/// The skip step (`docs/runtime-hardening.md` § Text skip §2–§5): while either
/// mode is effective, complete the current park's reveal, hold the whole line
/// for [`SKIP_DWELL_SECONDS`], then `advance_say` exactly once. The ordinary
/// `drive_script` pipeline (which runs after this system in the same frame)
/// executes the intervening commands and jumps, so a crossing is not a stop and
/// the read-log park observer derives the new park's arrival status.
///
/// The **seen** toggle ends at anything it cannot pass: a non-say park (a
/// choice, a decision, the end of the scene) and the first **arrival-unread**
/// say-line — which then reveals at the player's own `text_speed`. The **force**
/// hold has no arrival gate: it skips every say-line, holds at a non-say park
/// and at the end of the scene, and resumes on the next say-park while the key
/// is still down. A `delta` spike (hitch) advances once and discards the
/// remainder, so skip never catches up in a burst. The system is gated to the
/// play path by the plugin, so the overlay, the title and a defensive hold
/// freeze its clock without resetting it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn skip_advance(
    time: Res<Time>,
    settings: Option<Res<SettingsState>>,
    mut read_log: ResMut<ReadLogState>,
    mut skip: ResMut<SkipState>,
    mut reveal: ResMut<TextReveal>,
    mut runtime: ResMut<Runtime>,
    mut hold: ResMut<DefensiveHold>,
    mut notice: ResMut<Notice>,
) {
    if !skip.is_on() {
        return;
    }
    let force = skip.is_force_active();
    let Some(wait) = runtime.runner.wait().cloned() else {
        // The end of the scene: the seen toggle drops; a force hold waits.
        if !force {
            skip.end();
        }
        return;
    };
    // Skip only ever advances say-lines; a non-say park ends seen without
    // spending an advance, while force waits for the next say-park.
    if !matches!(wait, Wait::Say { .. }) {
        if !force {
            skip.end();
        }
        return;
    }
    // The seen gate: a line is skippable only when it was read before this
    // visit. The first arrival-unread line stops seen and is revealed by the
    // ordinary presentation at the player's speed. Force has no gate.
    if !force && read_log.arrival_read() != Some(true) {
        skip.end();
        return;
    }

    // Complete the park's reveal on its first skip frame, so the whole line is
    // on screen for the dwell. Idempotent; the park key only avoids re-doing it
    // every frame. A frame that actually changed a revealing line into a whole
    // one spends no delta, so the line renders whole for the full `0.05` no
    // matter how large the frame's delta was; an already-whole (or Instant,
    // or stale) line charges from this frame.
    let park = (runtime.runner.scene_id().to_string(), runtime.runner.pc());
    if skip.park.as_ref() != Some(&park) {
        skip.park = Some(park);
        let speed = settings::effective_text_speed(settings.as_deref());
        let was_whole = reveal.is_whole_for(&wait, speed);
        reveal.complete(speed);
        if skip.fresh {
            // The engage/arm frame only completes the line: the full `0.05`
            // starts next frame rather than being shortened by this frame's
            // delta.
            skip.fresh = false;
            return;
        }
        if !was_whole {
            // A successor line actually became whole on this frame: it spends
            // no delta, so it renders whole for the whole step too.
            return;
        }
    }

    // Decrement then test: a `0.05` second step advances on the frame it is
    // first spent, and a spike advances once without banking the remainder.
    skip.remaining -= time.delta_secs();
    if skip.remaining > f32::EPSILON {
        return;
    }
    skip.remaining = SKIP_DWELL_SECONDS;
    skip.park = None;
    // The runner is leaving this park: even if the script loops back to the
    // same scene and `pc`, the next observation is a fresh arrival.
    read_log.on_park_departure();
    match runtime.runner.advance_say() {
        Ok(()) => {
            if force {
                // The force step's arrival is exempt from the read mark; the
                // park observer consumes this on the transition it causes.
                skip.mark_force_advance();
            }
        }
        // Row 13 — defensive: the wait check above makes it unreachable.
        // `report_runner_call` engages the hold, which pauses skip with it.
        Err(error) => failure::report_runner_call(error, &mut notice, &mut hold),
    }
}

/// Whether either physical `Ctrl` is down. The force control is the key, not a
/// side: both presses mean the same hold.
pub(crate) fn ctrl_down(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
}

/// Whether either physical `Ctrl` was pressed this frame.
pub(crate) fn ctrl_just_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.just_pressed(KeyCode::ControlLeft) || keys.just_pressed(KeyCode::ControlRight)
}

/// The force hold's release mirror (`docs/runtime-hardening.md` § Text skip §5).
/// Runs on every `Update`, including under the overlay and the title, so a
/// `Ctrl` release behind a gate invalidates the latch: a press first made under
/// the overlay can never start a skip merely because play resumes. It only ever
/// clears — arming happens in [`crate::input::skip_input`], which is gated to
/// the play path.
pub(crate) fn force_release_mirror(keys: Res<ButtonInput<KeyCode>>, mut skip: ResMut<SkipState>) {
    if !ctrl_down(&keys) {
        skip.release_force();
    }
}
