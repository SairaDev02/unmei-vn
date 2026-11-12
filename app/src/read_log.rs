//! App-side read-log lifecycle: what the player has read, and when it lands on
//! disk.
//!
//! The engine owns the file's format, path and IO
//! ([`unmei_engine::read_log`]); this module owns *when*: the in-memory log, the
//! **arrival status** the skip gate consumes, the visited set, the dirty flag
//! and the flush moments (`docs/runtime-hardening.md` § Read log).
//!
//! **Read once per launch**, after the pack loads; never re-read. The log is
//! filtered against the loaded pack at that boundary — unknown scene ids, line
//! `0`, out-of-range source lines and source lines that identify commands rather
//! than say-lines are dropped, without dirtying or writing anything.
//!
//! **Mark at park**: a say-line is read the moment play parks on it. On the
//! transition into a park the app evaluates the arrival status *before* the
//! mark lands and keeps that boolean with the park while it lasts; a position
//! replacement (a load, a New Game) recomputes it even at the same scene and
//! `pc`, while an ordinary pause resumes with the arrival status it had. A
//! departure and re-entry — an advance that loops the script back to the same
//! coordinates — is a real transition too and recomputes. The boot park behind
//! the title is not play: it marks nothing.
//!
//! **Flush at the listed moments only** — play entry, a scene crossing, a choice
//! resolve, a decision resolve (model *or* author fallback) and app exit — and
//! only when dirty. One atomic write per flush; a failed write keeps the dirty
//! flag so the next moment retries.

use std::collections::BTreeSet;

use bevy::app::AppExit;
use bevy::prelude::*;
use unmei_engine::content::{Line, Pack, Scene};
use unmei_engine::read_log::{self as engine_read_log, ReadLog};
use unmei_engine::runner::Wait;

use crate::core::SkipState;
use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};
use crate::menu::{self, MenuState};
use crate::Runtime;

/// The session's read log plus the park bookkeeping the mark rule needs.
#[derive(Resource, Default)]
pub struct ReadLogState {
    /// The whole file, as the session holds it.
    pub log: ReadLog,
    /// The park the [`Self::arrival_read`] belongs to: `(scene id, pc)`.
    park: Option<(String, usize)>,
    /// Whether the say-line at the current park was read *before* this visit's
    /// mark landed. `None` at a non-say park or before play. The skip gate
    /// reads it; the Timeline reads the visited set.
    arrival_read: Option<bool>,
    /// A position replacement forces the next observation to evaluate the
    /// arrival status afresh, even at the same scene and `pc`.
    replaced: bool,
    /// Whether the log holds marks not yet written.
    dirty: bool,
    /// A flush moment has been reached; consumed by
    /// [`write_pending_read_log`].
    flush_requested: bool,
}

impl ReadLogState {
    /// The arrival status of the park the runner is sitting in, when it is a
    /// say-park: `true` when the line was read before this visit.
    pub fn arrival_read(&self) -> Option<bool> {
        self.arrival_read
    }

    /// Whether the line has been read, as the in-memory log currently stands.
    pub fn is_read(&self, scene_id: &str, source_line: usize) -> bool {
        self.log.is_read(scene_id, source_line)
    }

    /// Whether the scene has been visited.
    pub fn is_visited(&self, scene_id: &str) -> bool {
        self.log.is_visited(scene_id)
    }

    /// Whether the log holds unwritten marks.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// A position replacement — a load or a New Game — recomputes the arrival
    /// status on the next observation, even when the runner lands back on the
    /// same scene and `pc`.
    pub fn on_position_replaced(&mut self) {
        self.replaced = true;
    }

    /// The runner is leaving the observed park — an advance, a choice pick or a
    /// resolved decision. The next observation is a fresh arrival even when the
    /// script loops back to the same scene and `pc`, so a re-entered line
    /// recomputes its arrival status instead of inheriting the departed park's
    /// (`docs/runtime-hardening.md` § Read log §4).
    pub fn on_park_departure(&mut self) {
        self.park = None;
    }

    /// Record a flush moment. The write happens only if the log is dirty.
    pub fn request_flush(&mut self) {
        self.flush_requested = true;
    }
}

/// Read the log once, after a successful pack load, and filter it against the
/// pack (`docs/runtime-hardening.md` § Read log §3).
///
/// A *missing* file is the defined first-run state and logs nothing; a *present*
/// file that will not decode, carries another version or cannot be read logs
/// once (row 26) and yields the empty log. The bytes on disk are left exactly as
/// they are. Filtering drops entries the loaded pack cannot use and is not a
/// dirtying or writing act.
pub(crate) fn load_once(
    pack: &Pack,
    state: &mut ReadLogState,
    notice: &mut Notice,
    hold: &mut DefensiveHold,
) {
    let path = engine_read_log::read_log_path_for(pack);
    let (mut log, reason) = engine_read_log::load_read_log(&path);
    if let Some(reason) = reason {
        failure::report(Failure::new(FailureRow::ReadLog, reason), notice, hold);
    }
    filter_against_pack(&mut log, pack);

    state.log = log;
    state.park = None;
    state.arrival_read = None;
    state.replaced = false;
    state.dirty = false;
    state.flush_requested = false;
}

/// Drop entries the loaded pack cannot use: unknown scene ids, and source lines
/// that are zero, out of range or identify a command rather than a say-line.
fn filter_against_pack(log: &mut ReadLog, pack: &Pack) {
    log.read.retain(|scene_id, lines| {
        let Some(scene) = pack.scenes.get(scene_id) else {
            return false;
        };
        let say_lines: BTreeSet<usize> = (0..scene.lines.len())
            .filter_map(|index| authored_say_line(scene, index))
            .collect();
        lines.retain(|line| say_lines.contains(line));
        !lines.is_empty()
    });
    log.visited
        .retain(|scene_id| pack.scenes.contains_key(scene_id));
}

/// Observe the runner's park and apply the mark rule on a transition.
///
/// Runs after the frame's stepping systems, so the park it sees is the one the
/// player is about to be shown. The boot park behind the title is skipped: only
/// a live play session marks. A transition caused by a **force-skip advance**
/// still marks the scene visited and still computes the arrival status, but its
/// read mark is withheld — the exemption belongs to the advance, not the line,
/// so a later ordinary park marks normally (`docs/runtime-hardening.md` § Text
/// skip §8).
pub(crate) fn observe_parks(
    runtime: Option<Res<Runtime>>,
    menu: Res<MenuState>,
    mut state: ResMut<ReadLogState>,
    mut skip: ResMut<SkipState>,
) {
    let Some(runtime) = runtime else {
        return;
    };
    if !menu::live_session(&menu) {
        return;
    }

    let park = (runtime.runner.scene_id().to_string(), runtime.runner.pc());
    if state.park.as_ref() == Some(&park) && !state.replaced {
        return;
    }
    state.replaced = false;
    // Consume the force-advance exemption on this transition, whatever park it
    // landed on, so it can never leak onto a later ordinary mark.
    let suppress_mark = skip.take_force_advance();

    // The scene became the live play scene: a play entry, a crossing, a load or
    // a New Game. Idempotent, so a same-scene park transition is harmless —
    // force skip still records the scenes it crosses.
    if state.log.mark_visited(&park.0) {
        state.dirty = true;
    }

    state.arrival_read = None;
    if matches!(runtime.runner.wait(), Some(Wait::Say { .. })) {
        if let Some(source_line) = say_source_line(runtime.runner.pack(), &park.0, park.1) {
            // Arrival status first: a line marked by *this* visit must still
            // read as unread to the skip gate.
            state.arrival_read = Some(state.log.is_read(&park.0, source_line));
            if !suppress_mark && state.log.mark_read(&park.0, source_line) {
                state.dirty = true;
            }
        }
    }
    // A park the seen toggle cannot pass — a choice, a decision, the end of the
    // scene or an arrival-unread say-line — ends it on the frame the park lands,
    // so the cue follows the effective state instead of lagging a frame
    // (`docs/runtime-hardening.md` § Text skip §3/§7). A force hold is left
    // alone: it suspends the seen toggle without changing it, waits at these
    // parks and resumes on the next say-line; a release ends the toggle on that
    // frame through the ordinary skip step.
    if skip.is_seen() && !skip.is_force_active() && state.arrival_read != Some(true) {
        skip.end();
    }
    state.park = Some(park);
}

/// The authored source line of the say-line parked at `pc`, when `pc` names one.
/// The runner increments before parking, so the line's index is `pc - 1`.
fn say_source_line(pack: &Pack, scene_id: &str, pc: usize) -> Option<usize> {
    let scene = pack.scenes.get(scene_id)?;
    authored_say_line(scene, pc.checked_sub(1)?)
}

/// The authored 1-based source line of the say-line at `index`, when that line
/// is a say-line. One predicate for both the launch filter and the mark rule,
/// so a line can never be filtered out yet still marked.
fn authored_say_line(scene: &Scene, index: usize) -> Option<usize> {
    match scene.lines.get(index) {
        Some(Line::Say { .. }) => {
            let source_line = scene.source_line(index);
            (source_line > 0).then_some(source_line)
        }
        _ => None,
    }
}

/// Consume a requested flush moment and write the whole log atomically when it
/// is dirty. A successful write clears the flag; a failed write keeps
/// everything — the previous bytes stay, and the next moment retries.
pub(crate) fn write_pending_read_log(
    runtime: Option<Res<Runtime>>,
    mut state: ResMut<ReadLogState>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    if !state.flush_requested {
        return;
    }
    // No runtime means no pack (a rejected load): there is nothing to write and
    // nothing to flush, so the request stays put rather than being consumed.
    let Some(runtime) = runtime else {
        return;
    };
    state.flush_requested = false;
    flush_dirty(runtime.runner.pack(), &mut state, &mut notice, &mut hold);
}

/// The app-exit flush: the last moment, independent of any in-frame request.
pub(crate) fn flush_on_exit(
    mut exits: MessageReader<AppExit>,
    runtime: Option<Res<Runtime>>,
    mut state: ResMut<ReadLogState>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    if exits.read().next().is_none() {
        return;
    }
    let Some(runtime) = runtime else {
        return;
    };
    flush_dirty(runtime.runner.pack(), &mut state, &mut notice, &mut hold);
}

/// Write the dirty log once, through the one failure funnel (row 26).
fn flush_dirty(
    pack: &Pack,
    state: &mut ReadLogState,
    notice: &mut Notice,
    hold: &mut DefensiveHold,
) {
    if !state.dirty {
        return;
    }
    let path = engine_read_log::read_log_path_for(pack);
    match engine_read_log::write_read_log(&path, &state.log) {
        Ok(()) => state.dirty = false,
        // Row 26: Continue-silently. The previous bytes stay and the dirty flag
        // survives, so the next flush moment retries.
        Err(error) => failure::report(
            Failure::new(FailureRow::ReadLog, error.to_string()),
            notice,
            hold,
        ),
    }
}
