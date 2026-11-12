//! View transitions: `Esc`'s four moments, the door-aware back paths, every
//! sub-view's open, and the two ways the surface stops being up.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use unmei_engine::Pack;

use crate::credentials::KeyField;
use crate::session::SessionInput;

use super::cards::slot_index;
use super::key_field::discard_key_field;
use super::state::{ConfirmKind, MenuFocus, MenuMode, MenuOrigin, MenuState, MenuView};

/// One key, four moments: cancel an open confirmation, back out of Settings,
/// close, or open. Called by [`menu_input`] on `Esc`.
///
/// On the title the only live moment is cancelling a confirm: `Esc` is not a
/// door to the overlay, and there is nothing else to back out of.
pub(super) fn escape(
    menu: &mut MenuState,
    pack: &Pack,
    fields: &mut Query<(Entity, &mut EditableText), With<KeyField>>,
    input_focus: Option<&mut InputFocus>,
) {
    if menu.title_up() {
        if let Some(confirm) = menu.confirm.take() {
            menu.focus = confirm_focus(confirm);
        }
        return;
    }

    if !menu.open {
        menu.open = true;
        menu.view = MenuView::Slots;
        menu.mode = MenuMode::Save;
        menu.focus = MenuFocus::Card(0);
        menu.confirm = None;
        menu.picker = None;
        menu.key_editing = false;
        menu.refresh(pack);
        return;
    }

    // `Esc`'s "one key, four moments" gains one step while the key field is
    // edited: the first `Esc` leaves the field and discards, the next backs out
    // of Settings (`docs/title-and-menu.md` § Decisions settings §4.2).
    if menu.key_editing {
        discard_key_field(menu, fields, input_focus);
        return;
    }

    if let Some(confirm) = menu.confirm.take() {
        menu.focus = confirm_focus(confirm);
        return;
    }

    if menu.picker.take().is_some() {
        menu.focus = MenuFocus::LegacyCard;
        return;
    }

    if menu.view == MenuView::Settings {
        back_out_of_settings(menu);
        return;
    }

    // The still closes to the grid first; the gallery's grid then backs out to
    // its door; from the overlay's grid the next Esc closes the overlay, and
    // with the title as door the chain ends at the title, where Esc is inert
    // (`docs/title-and-menu.md` § Gallery `### Behavior` §3/§5).
    if menu.view == MenuView::Gallery {
        if menu.gallery_still {
            menu.gallery_still = false;
        } else {
            back_out_of_gallery(menu);
        }
        return;
    }

    // Timeline backs out to the slot grid (its only door) before the next Esc
    // closes the overlay (`docs/title-and-menu.md` § Timeline §1).
    if menu.view == MenuView::Timeline {
        back_out_of_timeline(menu);
        return;
    }

    menu.open = false;
    menu.confirm = None;
}

/// Back out of Settings to whichever door opened it: the slot grid, or the
/// title (`docs/title-and-menu.md` § Title §4).
pub(super) fn back_out_of_settings(menu: &mut MenuState) {
    menu.key_editing = false;
    back_out_to_origin(menu, menu.settings_origin);
}

/// Back out of the gallery to whichever door opened it, dropping any open
/// still (`docs/title-and-menu.md` § Gallery `### Behavior` §3/§5).
pub(super) fn back_out_of_gallery(menu: &mut MenuState) {
    menu.gallery_still = false;
    back_out_to_origin(menu, menu.gallery_origin);
}

/// Open the Timeline from the overlay's slot grid, its only door. Focus starts
/// at the first visited card in declared order, otherwise the first declared
/// card; an empty order has no cards
/// (`docs/title-and-menu.md` § Timeline §1/§2). Opening reads no slot and
/// writes nothing.
pub(super) fn open_timeline(menu: &mut MenuState, session: &SessionInput) {
    menu.view = MenuView::Timeline;
    menu.picker = None;
    menu.key_editing = false;
    menu.confirm = None;
    menu.focus = first_timeline_focus(session);
}

/// Back out of the Timeline to the slot grid's first card, without re-reading
/// slot metadata (`docs/title-and-menu.md` § Timeline §1).
pub(super) fn back_out_of_timeline(menu: &mut MenuState) {
    menu.view = MenuView::Slots;
    menu.focus = MenuFocus::Card(0);
}

/// The first-selectable focus rule: the first visited declared card, otherwise
/// the first declared card. Empty yields a card focus that matches no rendered
/// card, so no card focus is drawn (`docs/title-and-menu.md` § Timeline §2).
pub(super) fn first_timeline_focus(session: &SessionInput) -> MenuFocus {
    let entries = session.pack().timeline_entries();
    match entries.iter().position(|id| session.is_visited(id)) {
        Some(index) => MenuFocus::TimelineCard(index),
        None => MenuFocus::TimelineCard(0),
    }
}

/// Open the gallery from `origin`, recording the door. Opening writes nothing
/// and reads no slot (`docs/title-and-menu.md` § Gallery `### Behavior`
/// §1/§3); focus starts on the first card at every open.
pub(super) fn open_gallery(menu: &mut MenuState, origin: MenuOrigin) {
    menu.gallery_origin = origin;
    menu.view = MenuView::Gallery;
    menu.picker = None;
    menu.key_editing = false;
    menu.gallery_still = false;
    menu.focus = MenuFocus::GalleryCard(0);
}

/// Return to the recorded door. From the overlay's door the slot grid comes
/// back; from the title's door the title does, focused as a fresh entry leaves
/// it.
pub(super) fn back_out_to_origin(menu: &mut MenuState, origin: MenuOrigin) {
    match origin {
        MenuOrigin::Slots => {
            menu.view = MenuView::Slots;
            menu.focus = MenuFocus::Card(0);
        }
        MenuOrigin::Title => {
            menu.view = MenuView::Title;
            menu.focus = if menu.continue_enabled {
                MenuFocus::TitleContinue
            } else {
                MenuFocus::TitleNewGame
            };
        }
    }
}

/// Where focus returns when a confirmation is cancelled.
pub(super) fn confirm_focus(confirm: ConfirmKind) -> MenuFocus {
    match confirm {
        ConfirmKind::Overwrite(slot) | ConfirmKind::Load(slot) => MenuFocus::Card(slot_index(slot)),
        ConfirmKind::ResetAudio => MenuFocus::AudioReset,
        ConfirmKind::ResetPlay => MenuFocus::PlayReset,
        ConfirmKind::NewGame => MenuFocus::TitleNewGame,
        ConfirmKind::Quit => MenuFocus::TitleQuit,
        ConfirmKind::ExitToTitle => MenuFocus::SettingsBack,
        ConfirmKind::ClearKey => MenuFocus::KeyRow,
        ConfirmKind::TimelineJump(index) => MenuFocus::TimelineCard(index),
    }
}

/// Close the overlay (Resume), dropping any half-open confirmation or picker.
pub(super) fn close(menu: &mut MenuState) {
    menu.open = false;
    menu.confirm = None;
    menu.picker = None;
    menu.key_editing = false;
    menu.gallery_still = false;
    menu.view = MenuView::Slots;
}

/// Enter play, closing whichever pause surface is up. The live-session flag is
/// set here, the one place play is entered — and only after a successful
/// session outcome reaches this door (a live resume, a `Loaded` load, or a New
/// Game repark). The session recipes already own the skip disarm and the Read
/// log flush (`docs/runtime-hardening.md` § Read log §6, § Text skip §5), and
/// the auto dwell is deliberately not touched here: live Continue preserves its
/// remainder (`docs/title-and-menu.md` § Title §2), while a load, a New Game
/// and a Timeline jump reset it in their own recipes.
pub(super) fn enter_play(menu: &mut MenuState) {
    menu.open = false;
    menu.view = MenuView::Slots;
    menu.confirm = None;
    menu.picker = None;
    menu.live_session = true;
}
