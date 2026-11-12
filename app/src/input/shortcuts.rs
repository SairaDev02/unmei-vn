//! The developer and comfort hotkeys: F5/F9 save/load, `F11` fullscreen and `A`
//! auto mode.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use unmei_engine::runner::Wait;
use unmei_engine::save::{self, Slot};

use crate::core::SkipState;
use crate::failure::{self, Failure, FailureRow};
use crate::session::{LoadOrigin, SessionInput};
use crate::settings::SettingsState;
use crate::slots;
use crate::Runtime;

/// F5 saves the whole position to the first manual slot; F9 restores it through
/// the shared [`crate::session`] recipe. Both no-op cleanly when there is
/// nothing to do (no save file). F5 is processed before F9, so a same-frame pair
/// still saves and then loads the save it just wrote. The Esc overlay
/// ([`crate::menu`]) is the player-facing save/load surface; these remain as
/// developer shortcuts behind it, inert while the overlay is open.
pub(crate) fn save_load_input(keys: Res<ButtonInput<KeyCode>>, mut session: SessionInput) {
    if keys.just_pressed(KeyCode::F5) {
        let slot = Slot::Manual(1);
        let path = save::save_path_for(session.pack(), slot);
        // The write announces `unmei: saved to <path>` itself, exactly as the
        // overlay's Save does.
        // Row 14: a failed write leaves the slot file and its card as they were.
        if let Err(error) = slots::write_manual(session.runner(), session.staging(), slot) {
            let (notice, hold) = session.notice_and_hold();
            failure::report(
                Failure::new(
                    FailureRow::ManualSave,
                    format!("`{}`: {error}", path.display()),
                ),
                notice,
                hold,
            );
        }
    }

    if keys.just_pressed(KeyCode::F9) {
        // The typed `DevShortcut` origin owns the whole recipe: an absent slot
        // is a clean no-op, a read refusal logs row 16 Notice-less with the
        // reason and path, and a restore refusal reports row 17. The shortcut
        // has no pause surface to complete, so the outcome is discarded.
        let _ = session
            .transitions()
            .load(Slot::Manual(1), LoadOrigin::DevShortcut);
    }
}

/// The in-play `F11` fullscreen toggle: flip `play.fullscreen` and mark the
/// settings dirty; the presentation syncs the window from the field. Inert at a
/// choice-wait — the digits own that park — and never runs while the overlay is
/// open (its run condition) or while a defensive hold blocks the runner, so
/// there is no write on key press: the change rides the existing flush points.
pub(crate) fn fullscreen_input(
    keys: Res<ButtonInput<KeyCode>>,
    runtime: Res<Runtime>,
    settings: Option<ResMut<SettingsState>>,
) {
    if !keys.just_pressed(KeyCode::F11) {
        return;
    }
    if matches!(runtime.runner.wait(), Some(Wait::Choice { .. })) {
        return;
    }
    let Some(mut settings) = settings else {
        return;
    };
    settings.toggle_fullscreen();
}

/// The in-play `A` auto-mode toggle: flip `play.auto` and mark the settings
/// dirty; the presentation flips the `AUTO` cue from the same field. Inert at a
/// choice-wait — the digits own that park — and never runs while the overlay is
/// open (its run condition) or while a defensive hold blocks the runner. It
/// shares [`SettingsState::toggle_auto`] with the Settings switch, so there is
/// no write on key press: the change rides the existing flush points.
pub(crate) fn auto_input(
    keys: Res<ButtonInput<KeyCode>>,
    runtime: Res<Runtime>,
    mut skip: ResMut<SkipState>,
    settings: Option<ResMut<SettingsState>>,
) {
    if !keys.just_pressed(KeyCode::KeyA) {
        return;
    }
    // `A` is the explicit handoff: it ends the seen toggle and toggles auto.
    // Under a force hold it still does both; only the physical hold survives
    // (`docs/runtime-hardening.md` § Text skip §3/§6).
    skip.end();
    if matches!(runtime.runner.wait(), Some(Wait::Choice { .. })) {
        return;
    }
    let Some(mut settings) = settings else {
        return;
    };
    settings.toggle_auto();
}
