//! The menu's save, load, legacy-import and settings-flush paths: one capture
//! or read each, completed on the surface that asked for it.

use unmei_engine::save::{self, Slot};

use crate::failure::{self, Failure, FailureRow};
use crate::session::{LoadOrigin, LoadOutcome, ReadOutcome, SessionInput};
use crate::settings::{self, SettingsState};
use crate::slots;

use super::state::{MenuFocus, MenuState};
use super::transition::{close, enter_play};

/// The single manual-save path: stamp, write one slot, refresh the cards on
/// success, notice row 14 on failure. A failure leaves the slot untouched. The
/// capture stays a dedicated owner: this borrows the runner and staging
/// read-only through the session bridge and never touches the replacement
/// recipes.
pub(super) fn save_slot(menu: &mut MenuState, session: &mut SessionInput, slot: Slot) {
    let path = save::save_path_for(session.pack(), slot);
    match slots::write_manual(session.runner(), session.staging(), slot) {
        Ok(()) => menu.refresh(session.pack()),
        Err(error) => {
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
}

/// The single overlay load path: read the slot, then complete the overlay's
/// own surface. Only a successful restoration closes the overlay.
pub(super) fn load_slot(menu: &mut MenuState, session: &mut SessionInput, slot: Slot) {
    let read = session.transitions().read(slot, LoadOrigin::Menu);
    complete_load(menu, session, read, LoadSurface::Overlay);
}

/// Which menu surface a load completion belongs to. The overlay closes on
/// success and never refreshes the title's metadata; the title enters play on
/// success and refreshes its Continue metadata on refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoadSurface {
    Overlay,
    Title,
}

/// Complete a menu-origin load from the read stage's result. The restoration of
/// an already-read save runs here, so the refusal that gates completion is the
/// real restore outcome rather than a caller-supplied one. Only
/// [`LoadOutcome::Loaded`] closes the overlay or enters play — the menu's one
/// success door; a refusal leaves the surface up and, for the title, re-reads
/// the metadata and focus that can flip Continue to disabled. The old runner,
/// staging and pending/held session survive a refusal.
pub(crate) fn complete_load(
    menu: &mut MenuState,
    session: &mut SessionInput,
    read: ReadOutcome,
    surface: LoadSurface,
) -> LoadOutcome {
    let outcome = match read {
        ReadOutcome::Save { save, path } => session.transitions().restore_loaded(*save, &path),
        ReadOutcome::Missing => LoadOutcome::Missing,
        ReadOutcome::Refused => LoadOutcome::Refused,
    };
    match (outcome, surface) {
        (LoadOutcome::Loaded, LoadSurface::Overlay) => close(menu),
        (LoadOutcome::Loaded, LoadSurface::Title) => enter_play(menu),
        (LoadOutcome::Missing | LoadOutcome::Refused, LoadSurface::Title) => {
            menu.refresh(session.pack());
            menu.refresh_continue(session.scene_id());
            menu.focus = if menu.continue_enabled {
                MenuFocus::TitleContinue
            } else {
                MenuFocus::TitleNewGame
            };
        }
        (LoadOutcome::Missing | LoadOutcome::Refused, LoadSurface::Overlay) => {}
    }
    outcome
}

/// The legacy import path: write the chosen manual slot from the legacy file,
/// then rename the legacy file aside. On success the card retires (the refresh
/// re-reads `list_slots` and `legacy_save`); on failure row 18 notices and both
/// the card and the picker stay, so the player can pick another target.
pub(super) fn import_legacy(menu: &mut MenuState, session: &mut SessionInput, slot: Slot) {
    let pack = session.pack();
    match save::import_legacy(pack, slot) {
        Ok(()) => {
            menu.picker = None;
            menu.refresh(session.pack());
            menu.focus = MenuFocus::Card(0);
        }
        Err(error) => {
            let path = save::legacy_save_path_for(session.pack());
            let (notice, hold) = session.notice_and_hold();
            failure::report(
                Failure::new(
                    FailureRow::LegacyImport,
                    format!("`{}`: {error}", path.display()),
                ),
                notice,
                hold,
            );
        }
    }
}

/// Flush a dirty settings file through the session bridge's narrow failure-sink
/// reborrow, so the settings owner still routes its rows through the one
/// funnel.
pub(super) fn flush_settings(settings: &mut SettingsState, session: &mut SessionInput) {
    let (notice, hold) = session.notice_and_hold();
    settings::flush(settings, notice, hold);
}
