//! Synchronous live-session entry and replacement recipes.
//!
//! The menu owns navigation, focus, confirmation, origins, cached metadata,
//! surface changes and the one private launch-entry marker. This module owns
//! *when* an accepted entry or replacement performs its coordinated mutation:
//! a slot load, an already-read restore, a New Game repark, the title→play
//! resume, or a named-Scene restart
//! ([`docs/runtime-hardening.md` § Menu and live-session transitions],
//! [ADR 0012](../../docs/adr/0012-menu-and-live-session-transition-ownership.md)).
//!
//! The recipes are ordinary borrowed operations on [`SessionTransitions`]; the
//! single Bevy [`SessionInput`] bridge is the only place a system acquires the
//! resources, and callers can do nothing with the session but invoke concrete
//! operations, read bounded facts, or take the one documented narrow
//! `notice`/`hold` reborrow the dedicated owners need for their own funnel
//! calls. Pending/held decision cancellation, the comfort-clock and skip
//! resets, and the presentation replay request cannot be recreated at a call
//! site. The resources stay separately stored; the menu also keeps its own
//! marker and the presentation keeps the applied caches the replay request
//! invalidates.
//!
//! [`docs/runtime-hardening.md` § Menu and live-session transitions]: ../../docs/runtime-hardening.md#menu-and-live-session-transitions--ownership-design

use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use unmei_engine::save::{self, SaveFile, Slot, StagingState};
use unmei_engine::{Pack, ScriptRunner};

use crate::core::{self, AutoDwell, HeldDecision, PendingDecision, SkipState};
use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};
use crate::presentation::play::{
    request_replay, AppliedAudio, AppliedUi, AppliedVisuals, TextReveal,
};
use crate::read_log::ReadLogState;
use crate::session_lines;
use crate::slots::PendingAutosave;
use crate::Runtime;

// ---------------------------------------------------------------------------
// Origin and outcome vocabulary
// ---------------------------------------------------------------------------

/// Who asked for a load; the typed origin, never caller-supplied rows or
/// Notice booleans, selects the read-stage failure policy
/// (`docs/runtime-hardening.md` § Menu and live-session transitions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadOrigin {
    /// The overlay's cards and the title's Continue: an absent slot and every
    /// read refusal are the existing row 17 + Notice.
    Menu,
    /// The F9 developer shortcut: an absent manual slot is a clean no-op, and a
    /// read refusal is the Notice-less row 16 with the engine's reason and the
    /// slot path.
    DevShortcut,
}

/// The load-stage outcome. Only [`LoadOutcome::Loaded`] permits the caller to
/// close a surface or enter play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadOutcome {
    /// The slot was absent and the origin says absence is a no-op.
    Missing,
    /// The save was read and restored; the successful-replacement cleanup ran.
    Loaded,
    /// The read or the restore was refused; the old session is untouched and
    /// the origin's row was already reported.
    Refused,
}

/// The read stage's result before restoration: the already-read save and where
/// it came from, or a terminal outcome whose row the origin's policy already
/// reported. A menu caller completes a load from this value on its own surface.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReadOutcome {
    /// The slot was read and accepted; `path` is where it came from.
    Save { save: Box<SaveFile>, path: PathBuf },
    /// The slot was absent and the origin says absence is a no-op.
    Missing,
    /// The read was refused; the origin's row was already reported.
    Refused,
}

/// The named-Scene restart outcome. Only [`RestartOutcome::Restarted`] permits
/// the caller to close the Timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestartOutcome {
    /// The target scene was restored at position zero; cleanup ran.
    Restarted,
    /// The restore was refused; the old session survives and defensive row 13
    /// was reported.
    Refused,
}

// ---------------------------------------------------------------------------
// The borrowed recipes
// ---------------------------------------------------------------------------

/// A short-lived ordinary borrow of the live-session resources: the recipe
/// owner. Every mutation of the coordinated transition lives here; callers get
/// outcomes and nothing else. Fields are private to this module.
pub(crate) struct SessionTransitions<'a> {
    runtime: &'a mut Runtime,
    pending: &'a mut PendingDecision,
    held: &'a mut HeldDecision,
    hold: &'a mut DefensiveHold,
    audio: &'a mut AppliedAudio,
    visuals: &'a mut AppliedVisuals,
    ui: &'a mut AppliedUi,
    auto: &'a mut AutoDwell,
    skip: &'a mut SkipState,
    read_log: &'a mut ReadLogState,
    notice: &'a mut Notice,
    autosave: &'a mut PendingAutosave,
    reveal: &'a mut TextReveal,
}

impl SessionTransitions<'_> {
    /// Load one slot for the typed origin: the missing probe where the origin
    /// has one, read → restore → successful cleanup, and the origin's
    /// read-stage row mapping. Callers with no menu surface of their own (the
    /// F9 shortcut) use this whole operation; a menu caller instead takes the
    /// read stage and hands it to its own surface completion. Save-file IO and
    /// validation stay engine-owned.
    pub(crate) fn load(&mut self, slot: Slot, origin: LoadOrigin) -> LoadOutcome {
        match self.read(slot, origin) {
            ReadOutcome::Save { save, path } => self.restore_loaded(*save, &path),
            ReadOutcome::Missing => LoadOutcome::Missing,
            ReadOutcome::Refused => LoadOutcome::Refused,
        }
    }

    /// The read stage alone: the missing probe where the origin has one, the
    /// read, and the origin's read-stage row mapping. The returned save is
    /// already read but not restored — acceptance is only ever for the checked
    /// value, so a later edit can still be refused at restore.
    pub(crate) fn read(&mut self, slot: Slot, origin: LoadOrigin) -> ReadOutcome {
        let path = save::save_path_for(self.runtime.runner.pack(), slot);
        if origin == LoadOrigin::DevShortcut && !path.exists() {
            // Nothing to load is not a failure: a clean no-op with no log or
            // Notice (`docs/runtime-hardening.md` § Load and transition
            // contract).
            return ReadOutcome::Missing;
        }
        match save::read_slot(self.runtime.runner.pack(), slot) {
            Ok(save) => ReadOutcome::Save {
                save: Box::new(save),
                path,
            },
            Err(error) => {
                match origin {
                    // Row 16: an unreadable slot refuses to load with its own
                    // reason and stays overwritable. Its surface is the menu
                    // card, so the funnel logs the reason Notice-less; the
                    // machine detail it is followed by is the slot path.
                    LoadOrigin::DevShortcut => failure::report(
                        Failure::with_short(
                            FailureRow::UnreadableSlot,
                            error.to_string(),
                            format!("`{}`", path.display()),
                        ),
                        self.notice,
                        self.hold,
                    ),
                    // Row 17: the overlay's load surface already classified the
                    // card, so a read that fails at activation is the
                    // "slot changed under the overlay" race.
                    LoadOrigin::Menu => failure::report(
                        Failure::new(
                            FailureRow::LoadRefused,
                            format!("`{}`: {error}", path.display()),
                        ),
                        self.notice,
                        self.hold,
                    ),
                }
                ReadOutcome::Refused
            }
        }
    }

    /// Restore an already-read save and signal the presentation to replay what
    /// it carries. This is the ordinary restore seam: success performs the
    /// successful-replacement cleanup, while a refusal reports row 17 and
    /// leaves the old runner, staging and pending/held session untouched, so
    /// the caller must complete its load as a no-op.
    ///
    /// A load is a discontinuity: the in-flight request is cancelled and a held
    /// answer discarded, a defensive hold is released, both comfort clocks
    /// reset and both skip modes disarm, the arrival status is recomputed and a
    /// flush requested, and audio/visual/UI replay is forced. It deliberately
    /// adds no explicit Notice clear, reveal reset or pending-autosave clear
    /// (`docs/runtime-hardening.md` § Menu and live-session transitions).
    pub(crate) fn restore_loaded(&mut self, save: SaveFile, path: &Path) -> LoadOutcome {
        match save.restore(&mut self.runtime.runner) {
            Ok(staging) => {
                self.runtime.staging = staging;
                self.pending.0 = None;
                self.held.0 = None;
                self.hold.release();
                core::reset_for_replacement(self.auto, self.skip);
                self.read_log.on_position_replaced();
                self.read_log.request_flush();
                request_replay(self.audio, self.visuals, self.ui);
                log::info!("unmei: loaded {}", path.display());
                session_lines::entered_scene(self.runtime.runner.scene_id());
                LoadOutcome::Loaded
            }
            // Row 17: the read succeeded, but the restore was refused. The old
            // runner, staging and pending/held session are untouched, so much
            // so that the caller can keep going with them.
            Err(error) => {
                failure::report(
                    Failure::new(
                        FailureRow::LoadRefused,
                        format!("`{}`: {error}", path.display()),
                    ),
                    self.notice,
                    self.hold,
                );
                LoadOutcome::Refused
            }
        }
    }

    /// New Game's load-like reset (`docs/title-and-menu.md` § Title §3):
    /// rebuild the runner at the entry scene with the pack's declared initial
    /// state, run it once to its first wait and re-apply the leading staging,
    /// then clear every in-flight and pending piece of the discarded session.
    /// It writes no slot, reads no slot and changes no settings; the Read log's
    /// knowledge, preferences, credentials and Save slot bytes are retained.
    pub(crate) fn new_game(&mut self) {
        core::park_at_entry(self.runtime);
        session_lines::entered_scene(self.runtime.runner.scene_id());

        self.pending.0 = None;
        self.held.0 = None;
        self.hold.release();
        // A discontinuity and a position replacement: the dwell and both skip
        // modes reset, so a toggle or hold carried from the discarded session
        // cannot skip the new entry park
        // (`docs/runtime-hardening.md` § Text skip §5).
        core::reset_for_replacement(self.auto, self.skip);
        self.reveal.reset();
        self.notice.clear();
        self.autosave.0 = false;

        // The arrival status is recomputed even when the entry park is the one
        // the runner was already sitting on; existing knowledge is retained and
        // only ordinary entry marks are added.
        self.read_log.on_position_replaced();

        // Force audio and visuals to replay from the top, exactly as a load
        // does.
        request_replay(self.audio, self.visuals, self.ui);

        self.read_log.request_flush();
    }

    /// Enter play from the title over the live session — the Esc resume one
    /// level up. No slot is read and nothing is replaced or cancelled: the
    /// runner, remaining auto dwell, reveal, Notice and pending/held decisions
    /// survive. The seen/force skip machine still disarms on title→play entry,
    /// so the preserved dwell cannot become a fast-forward, and the entry
    /// remains a Read log flush moment
    /// (`docs/runtime-hardening.md` § Menu and live-session transitions).
    pub(crate) fn resume_live_entry(&mut self) {
        self.skip.disarm();
        self.read_log.request_flush();
    }

    /// Restart one named scene from position zero with the current state and
    /// staging carried over — scene selection, not a historical rewind
    /// (`docs/title-and-menu.md` § Timeline §4). The restore runs before any
    /// destructive cleanup, so a refusal retains the old session and reports
    /// the existing defensive row 13. On success the old request is cancelled,
    /// the held answer and stale `PendingAutosave` dropped, the defensive hold
    /// released, the arrival status recomputed, both float clocks reset, the
    /// Notice cleared and audio/visual/UI replay forced. The ordinary
    /// target-script visit/autosave effects remain untouched.
    pub(crate) fn restart_scene(&mut self, scene_id: &str) -> RestartOutcome {
        let state = self.runtime.runner.state().clone();
        if let Err(error) =
            self.runtime
                .runner
                .restore_position(scene_id.to_string(), 0, state, None)
        {
            // Row 13 — defensive: the restore was refused. The old
            // runner/staging and the pending/held session are untouched, and
            // the funnel's hold freezes the play path until a load recovers.
            failure::report_runner_call(format!("`{scene_id}`: {error}"), self.notice, self.hold);
            return RestartOutcome::Refused;
        }

        // Successful replacement cleanup, before the new play path can run.
        self.pending.0 = None;
        self.held.0 = None;
        self.hold.release();
        self.autosave.0 = false;
        self.reveal.reset();
        self.notice.clear();
        core::reset_for_replacement(self.auto, self.skip);
        self.read_log.on_position_replaced();
        self.read_log.request_flush();
        request_replay(self.audio, self.visuals, self.ui);
        session_lines::entered_scene(scene_id);
        RestartOutcome::Restarted
    }
}

// ---------------------------------------------------------------------------
// The Bevy acquisition bridge
// ---------------------------------------------------------------------------

/// The one module-owned bridge that acquires the separately stored session
/// resources; every caller reborrows it into short-lived
/// [`SessionTransitions`]. It also exposes the bounded read-only facts the menu
/// needs for navigation and eligibility, and a narrow `notice`/`hold` reborrow
/// for the dedicated owners (manual save, Legacy import, credential commit,
/// settings flush).
#[derive(SystemParam)]
pub(crate) struct SessionInput<'w> {
    runtime: ResMut<'w, Runtime>,
    pending: ResMut<'w, PendingDecision>,
    held: ResMut<'w, HeldDecision>,
    hold: ResMut<'w, DefensiveHold>,
    audio: ResMut<'w, AppliedAudio>,
    visuals: ResMut<'w, AppliedVisuals>,
    ui: ResMut<'w, AppliedUi>,
    auto: ResMut<'w, AutoDwell>,
    skip: ResMut<'w, SkipState>,
    read_log: ResMut<'w, ReadLogState>,
    notice: ResMut<'w, Notice>,
    autosave: ResMut<'w, PendingAutosave>,
    reveal: ResMut<'w, TextReveal>,
}

impl SessionInput<'_> {
    /// Reborrow the recipe owner for one synchronous operation.
    pub(crate) fn transitions(&mut self) -> SessionTransitions<'_> {
        SessionTransitions {
            runtime: &mut self.runtime,
            pending: &mut self.pending,
            held: &mut self.held,
            hold: &mut self.hold,
            audio: &mut self.audio,
            visuals: &mut self.visuals,
            ui: &mut self.ui,
            auto: &mut self.auto,
            skip: &mut self.skip,
            read_log: &mut self.read_log,
            notice: &mut self.notice,
            autosave: &mut self.autosave,
            reveal: &mut self.reveal,
        }
    }

    /// The loaded pack, for navigation, card metadata and Timeline eligibility.
    pub(crate) fn pack(&self) -> &Pack {
        self.runtime.runner.pack()
    }

    /// The live scene id, for Continue's line and the title's return.
    pub(crate) fn scene_id(&self) -> &str {
        self.runtime.runner.scene_id()
    }

    /// Whether the Read log has visited a scene — Timeline's eligibility rule.
    pub(crate) fn is_visited(&self, scene_id: &str) -> bool {
        self.read_log.is_visited(scene_id)
    }

    /// The runner, read-only: the dedicated manual-save owner captures it
    /// verbatim. No caller may drive or mutate it through this module.
    pub(crate) fn runner(&self) -> &ScriptRunner {
        &self.runtime.runner
    }

    /// The staging snapshot, read-only, for the same capture.
    pub(crate) fn staging(&self) -> &StagingState {
        &self.runtime.staging
    }

    /// Narrowly reborrow the shared failure sink for a dedicated owner's
    /// existing funnel call — the "shared Notice/hold access is acquired once
    /// and narrowly reborrowed" rule (`docs/runtime-hardening.md`
    /// § Menu and live-session transitions). Not simultaneously aliased:
    /// callers take this borrow, report, and return.
    pub(crate) fn notice_and_hold(&mut self) -> (&mut Notice, &mut DefensiveHold) {
        (&mut self.notice, &mut self.hold)
    }
}
