//! The action set and its application: one [`MenuAction`] per control, so the
//! pointer and the keyboard are equal citizens, and the one dispatch that
//! applies an action to the pause surface.

use bevy::prelude::*;
use unmei_engine::save::Slot;

use crate::session::{LoadOrigin, SessionInput};
use crate::settings::{AudioControl, SettingsState};

use super::confirm::{accept_confirm, activate_card};
use super::input::KeyRowInput;
use super::key_field::{copy_key_path, enter_key_editing};
use super::labels::control_focus;
use super::persistence::{complete_load, import_legacy, LoadSurface};
use super::state::{
    legacy_slot, legacy_visible, live_session, ConfirmKind, LegacyPicker, MenuFocus, MenuMode,
    MenuOrigin, MenuState, MenuView,
};
use super::transition::{
    back_out_of_gallery, back_out_of_settings, back_out_of_timeline, close, confirm_focus,
    enter_play, open_gallery, open_timeline,
};

/// A control's identity, attached to every clickable node. Both the pointer and
/// the keyboard funnel through [`MenuAction`], so click and key are equal
/// citizens.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Resume,
    SetMode(MenuMode),
    OpenSettings,
    BackToSlots,
    Card(usize),
    /// The Legacy card's `Import…` control: open the slot picker.
    LegacyImport,
    /// One row of the legacy slot picker.
    LegacyTarget(Slot),
    /// Cancel the legacy slot picker, back to the Legacy card.
    LegacyCancel,
    /// Focus (and, under a drag, adjust) one slider.
    AudioSlider(AudioControl),
    /// Toggle the audio mute.
    AudioMute,
    /// Open the audio-only Reset confirmation.
    AudioReset,
    /// Focus (and, under a drag, step) the Text-speed control.
    TextSpeed,
    /// Toggle `play.auto`.
    PlayAuto,
    /// Toggle `play.fullscreen`.
    PlayFullscreen,
    /// Open the play-only Reset confirmation.
    PlayReset,
    /// The title's `Continue` row.
    Continue,
    /// The title's `New Game` row.
    NewGame,
    /// Focus the Decisions key row; activating it enters the field's edit mode.
    KeyRow,
    /// Copy the credential file's path to the clipboard.
    CopyKeyPath,
    /// Open the one-confirm Clear key dialog.
    ClearKey,
    /// The title's `Gallery` row (opens the gallery with the title as origin).
    Gallery,
    /// The overlay top bar's `Gallery` control (opens the gallery from the
    /// slot grid, with the grid as origin).
    OpenGallery,
    /// The overlay top bar's `Timeline` control (opens the Timeline from the
    /// slot grid; its only door).
    OpenTimeline,
    /// The Timeline's door-aware back control.
    TimelineBack,
    /// One card of the Timeline, in declared order. Activation confirms a jump
    /// when the card is visited; a locked card is inert
    /// (`docs/title-and-menu.md` § Timeline §2/§3).
    TimelineCard(usize),
    /// The gallery's door-aware back control.
    GalleryBack,
    /// One card of the gallery grid, in entry order. Activation opens the still
    /// view on that entry (`docs/title-and-menu.md` § Gallery `### Behavior`
    /// §4/§5).
    GalleryCard(usize),
    /// The still view's inert click surface: one click anywhere closes back to
    /// the grid, focused on the entry being shown
    /// (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
    GalleryStillClose,
    /// The title's `Quit` row.
    Quit,
    /// The overlay Settings rail's `Exit to title`.
    ExitToTitle,
    ConfirmAccept,
    ConfirmCancel,
}

pub(crate) fn action_for_focus(focus: MenuFocus) -> MenuAction {
    match focus {
        MenuFocus::Card(index) => MenuAction::Card(index),
        MenuFocus::GalleryCard(index) => MenuAction::GalleryCard(index),
        MenuFocus::TimelineCard(index) => MenuAction::TimelineCard(index),
        MenuFocus::SaveTab => MenuAction::SetMode(MenuMode::Save),
        MenuFocus::LoadTab => MenuAction::SetMode(MenuMode::Load),
        MenuFocus::SettingsButton => MenuAction::OpenSettings,
        MenuFocus::Resume => MenuAction::Resume,
        MenuFocus::SettingsBack => MenuAction::BackToSlots,
        MenuFocus::LegacyCard => MenuAction::LegacyImport,
        MenuFocus::LegacyTarget(index) => MenuAction::LegacyTarget(legacy_slot(index)),
        MenuFocus::LegacyCancel => MenuAction::LegacyCancel,
        MenuFocus::ConfirmYes => MenuAction::ConfirmAccept,
        MenuFocus::ConfirmNo => MenuAction::ConfirmCancel,
        MenuFocus::AudioMaster => MenuAction::AudioSlider(AudioControl::Master),
        MenuFocus::AudioMusic => MenuAction::AudioSlider(AudioControl::Music),
        MenuFocus::AudioSfx => MenuAction::AudioSlider(AudioControl::Sfx),
        MenuFocus::AudioMute => MenuAction::AudioMute,
        MenuFocus::AudioReset => MenuAction::AudioReset,
        MenuFocus::TextSpeed => MenuAction::TextSpeed,
        MenuFocus::PlayAuto => MenuAction::PlayAuto,
        MenuFocus::PlayFullscreen => MenuAction::PlayFullscreen,
        MenuFocus::PlayReset => MenuAction::PlayReset,
        MenuFocus::KeyRow => MenuAction::KeyRow,
        MenuFocus::CopyKeyPath => MenuAction::CopyKeyPath,
        MenuFocus::ClearKey => MenuAction::ClearKey,
        MenuFocus::TitleContinue => MenuAction::Continue,
        MenuFocus::TitleNewGame => MenuAction::NewGame,
        MenuFocus::TitleSettings => MenuAction::OpenSettings,
        MenuFocus::TitleGallery => MenuAction::Gallery,
        MenuFocus::TitleQuit => MenuAction::Quit,
        MenuFocus::GalleryButton => MenuAction::OpenGallery,
        MenuFocus::TimelineButton => MenuAction::OpenTimeline,
    }
}

pub(super) fn apply(
    action: MenuAction,
    menu: &mut MenuState,
    session: &mut SessionInput,
    settings: &mut SettingsState,
    exit: &mut MessageWriter<AppExit>,
    key: &mut KeyRowInput,
) {
    // Defence for a direct caller: while the still is up only its own close
    // runs, so no chrome action can move focus off the entry being shown
    // (`docs/title-and-menu.md` § Gallery `### Behavior` §5).
    if menu.view == MenuView::Gallery
        && menu.gallery_still
        && action != MenuAction::GalleryStillClose
    {
        return;
    }
    match action {
        MenuAction::Resume => close(menu),
        MenuAction::SetMode(mode) => {
            menu.picker = None;
            menu.mode = mode;
            menu.focus = match mode {
                MenuMode::Save => MenuFocus::SaveTab,
                MenuMode::Load => MenuFocus::LoadTab,
            };
        }
        MenuAction::OpenSettings => {
            menu.picker = None;
            // Record the door: the back control and the overlay's `Exit to
            // title` rail both branch on it.
            menu.settings_origin = if menu.view == MenuView::Title {
                MenuOrigin::Title
            } else {
                MenuOrigin::Slots
            };
            menu.view = MenuView::Settings;
            menu.focus = MenuFocus::AudioMaster;
            menu.key_editing = false;
        }
        MenuAction::BackToSlots => {
            menu.picker = None;
            back_out_of_settings(menu);
        }
        MenuAction::Continue => {
            if menu.confirm.is_none() {
                menu.focus = MenuFocus::TitleContinue;
                // A disabled row stays walkable but activating it is a no-op.
                if menu.continue_enabled {
                    if live_session(menu) {
                        // The Esc resume one level up: the session's recipe
                        // preserves the remaining dwell and the whole live
                        // session, then the menu enters play through its one
                        // door.
                        session.transitions().resume_live_entry();
                        enter_play(menu);
                    } else {
                        // The autosave path completes the title's own surface:
                        // a successful restoration enters play, while a read or
                        // restore refusal leaves the title up and re-reads the
                        // metadata (`Missing` is defensive for a menu origin: an
                        // absent slot reads as a row-17 refusal).
                        let read = session.transitions().read(Slot::Autosave, LoadOrigin::Menu);
                        complete_load(menu, session, read, LoadSurface::Title);
                    }
                }
            }
        }
        MenuAction::NewGame => {
            if menu.confirm.is_none() {
                menu.focus = MenuFocus::TitleNewGame;
                if live_session(menu) {
                    menu.confirm = Some(ConfirmKind::NewGame);
                    menu.focus = MenuFocus::ConfirmYes;
                } else {
                    session.transitions().new_game();
                    enter_play(menu);
                }
            }
        }
        // The title's Gallery row opens the gallery with the title as origin,
        // writing nothing and reading no slot
        // (`docs/title-and-menu.md` § Gallery `### Behavior` §1/§3).
        MenuAction::Gallery => {
            if menu.confirm.is_none() {
                open_gallery(menu, MenuOrigin::Title);
            }
        }
        // The overlay's Gallery control opens it from the slot grid.
        MenuAction::OpenGallery => {
            if menu.confirm.is_none() && menu.view == MenuView::Slots {
                open_gallery(menu, MenuOrigin::Slots);
            }
        }
        // The overlay's Timeline control opens it from the slot grid, its only
        // door; opening reads no slot and writes nothing
        // (`docs/title-and-menu.md` § Timeline §1).
        MenuAction::OpenTimeline => {
            if menu.confirm.is_none() && menu.view == MenuView::Slots {
                open_timeline(menu, session);
            }
        }
        MenuAction::TimelineBack => {
            if menu.view == MenuView::Timeline && menu.confirm.is_none() {
                back_out_of_timeline(menu);
            }
        }
        // A Timeline card's activation confirms a jump when visited; a locked
        // card is inert (no confirm, jump or Notice)
        // (`docs/title-and-menu.md` § Timeline §2/§3).
        MenuAction::TimelineCard(index) => {
            if menu.view == MenuView::Timeline && menu.confirm.is_none() {
                let visited = session
                    .pack()
                    .timeline_entries()
                    .get(index)
                    .is_some_and(|id| session.is_visited(id));
                if visited {
                    menu.confirm = Some(ConfirmKind::TimelineJump(index));
                    menu.focus = MenuFocus::ConfirmYes;
                }
            }
        }
        MenuAction::GalleryBack => {
            if menu.view == MenuView::Gallery && menu.confirm.is_none() {
                back_out_of_gallery(menu);
            }
        }
        // A gallery card's activation opens the still on that entry, with the
        // grid's focus and the still's current entry left as the one value
        // (`docs/title-and-menu.md` § Gallery `### Behavior` §4/§5).
        MenuAction::GalleryCard(index) => {
            if menu.view == MenuView::Gallery {
                let count = session.pack().gallery_entries().len();
                if index < count {
                    menu.focus = MenuFocus::GalleryCard(index);
                    menu.gallery_still = true;
                }
            }
        }
        // A click anywhere in the still closes back to the grid, focused on the
        // entry it was showing (`docs/title-and-menu.md` § Gallery `### Behavior`
        // §5).
        MenuAction::GalleryStillClose => {
            if menu.view == MenuView::Gallery && menu.gallery_still {
                menu.gallery_still = false;
            }
        }
        MenuAction::Quit => {
            if menu.confirm.is_none() {
                menu.focus = MenuFocus::TitleQuit;
                if live_session(menu) {
                    menu.confirm = Some(ConfirmKind::Quit);
                    menu.focus = MenuFocus::ConfirmYes;
                } else {
                    exit.write(AppExit::Success);
                }
            }
        }
        MenuAction::ExitToTitle => {
            if menu.confirm.is_none() {
                menu.confirm = Some(ConfirmKind::ExitToTitle);
                menu.focus = MenuFocus::ConfirmYes;
            }
        }
        MenuAction::Card(index) => {
            if menu.picker.is_none() {
                menu.focus = MenuFocus::Card(index);
                activate_card(index, menu, session);
            }
        }
        MenuAction::LegacyImport => {
            if menu.confirm.is_none() && legacy_visible(menu) {
                menu.picker = Some(LegacyPicker { target: 0 });
                menu.focus = MenuFocus::LegacyTarget(0);
            }
        }
        MenuAction::LegacyTarget(slot) => {
            // The picker is a save-mode surface: re-check the card's visibility
            // so a mode switch under an open picker cannot still import.
            if menu.picker.is_some() && legacy_visible(menu) && matches!(slot, Slot::Manual(_)) {
                import_legacy(menu, session, slot);
            }
        }
        MenuAction::LegacyCancel => {
            if menu.picker.take().is_some() {
                menu.focus = MenuFocus::LegacyCard;
            }
        }
        MenuAction::AudioSlider(control) => {
            menu.focus = control_focus(control);
        }
        MenuAction::AudioMute => {
            if menu.view == MenuView::Settings && menu.confirm.is_none() {
                menu.focus = MenuFocus::AudioMute;
                settings.toggle_mute();
            }
        }
        MenuAction::AudioReset => {
            if menu.view == MenuView::Settings && menu.confirm.is_none() {
                menu.confirm = Some(ConfirmKind::ResetAudio);
                menu.focus = MenuFocus::ConfirmYes;
            }
        }
        MenuAction::TextSpeed => {
            if menu.view == MenuView::Settings && menu.confirm.is_none() {
                menu.focus = MenuFocus::TextSpeed;
            }
        }
        MenuAction::PlayAuto => {
            if menu.view == MenuView::Settings && menu.confirm.is_none() {
                menu.focus = MenuFocus::PlayAuto;
                settings.toggle_auto();
            }
        }
        MenuAction::PlayFullscreen => {
            if menu.view == MenuView::Settings && menu.confirm.is_none() {
                menu.focus = MenuFocus::PlayFullscreen;
                settings.toggle_fullscreen();
            }
        }
        MenuAction::PlayReset => {
            if menu.view == MenuView::Settings && menu.confirm.is_none() {
                menu.confirm = Some(ConfirmKind::ResetPlay);
                menu.focus = MenuFocus::ConfirmYes;
            }
        }
        MenuAction::KeyRow => {
            if menu.view == MenuView::Settings && menu.confirm.is_none() {
                enter_key_editing(
                    menu,
                    &key.credentials,
                    &mut key.fields,
                    key.input_focus.as_deref_mut(),
                );
            }
        }
        MenuAction::CopyKeyPath => {
            copy_key_path(
                &key.credentials,
                &mut key.clipboard_port,
                key.clipboard.as_deref_mut(),
                &mut key.copy_label,
            );
        }
        MenuAction::ClearKey => {
            // `Clear key` is disabled while the environment variable wins
            // (`docs/title-and-menu.md` § Decisions settings §4.1).
            if menu.view == MenuView::Settings
                && menu.confirm.is_none()
                && !key.credentials.row.is_from_environment()
            {
                menu.confirm = Some(ConfirmKind::ClearKey);
                menu.focus = MenuFocus::ConfirmYes;
            }
        }
        MenuAction::ConfirmAccept => accept_confirm(
            menu,
            session,
            settings,
            exit,
            &mut key.credentials,
            &mut key.runtime_config,
            &mut key.decision_channel,
            &mut key.pending_key_check,
        ),
        MenuAction::ConfirmCancel => {
            if let Some(confirm) = menu.confirm.take() {
                menu.focus = confirm_focus(confirm);
            }
        }
    }
}
