//! The Decisions key field: entering the built-in editor, discarding or
//! committing the typed value, and the `Copy path` control beside it.

use std::sync::Arc;

use bevy::clipboard::Clipboard;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;
use unmei_engine::KeyCheck;

use crate::core::{spawn_key_check, PendingKeyCheck};
use crate::credentials::{self, ClipboardPort, CopyPathLabel, CredentialsState, KeyField, KeyRow};
use crate::session::SessionInput;

use super::state::{MenuFocus, MenuState};

/// Put the built-in field into edit mode. `From environment` is read-only, so
/// the row is only focused there (`docs/title-and-menu.md` § Decisions settings
/// §4.1).
pub(super) fn enter_key_editing(
    menu: &mut MenuState,
    credentials: &CredentialsState,
    fields: &mut Query<(Entity, &mut EditableText), With<KeyField>>,
    input_focus: Option<&mut InputFocus>,
) {
    menu.focus = MenuFocus::KeyRow;
    // `From environment` is read-only, and `Checking…` disables the input while
    // the ping runs (`docs/title-and-menu.md` § Decisions settings §4.1).
    if credentials.row.is_from_environment() || credentials.row.is_checking() {
        return;
    }
    let Some((entity, _)) = fields.iter_mut().next() else {
        return;
    };
    menu.key_editing = true;
    if let Some(input_focus) = input_focus {
        input_focus.set(entity, FocusCause::Navigated);
    }
}

/// Leave the field without committing: clear the buffer and return to the walk.
/// The first `Esc` and a click elsewhere both land here
/// (`docs/title-and-menu.md` § Decisions settings §4.2).
pub(super) fn discard_key_field(
    menu: &mut MenuState,
    fields: &mut Query<(Entity, &mut EditableText), With<KeyField>>,
    input_focus: Option<&mut InputFocus>,
) {
    let Some((entity, mut field)) = fields.iter_mut().next() else {
        menu.key_editing = false;
        menu.focus = MenuFocus::KeyRow;
        return;
    };
    leave_key_field(menu, entity, &mut field, input_focus, true);
}

/// Commit one `Enter`: the local shape check first, then the atomic write, then
/// the ping. On a shape failure nothing is written and the typed text is kept so
/// it can be corrected; on a write failure the row is left as it was and the
/// text is also kept. A successful write shows `Checking…` and spawns the ping,
/// whose verdict [`crate::core::poll_key_check`] lands
/// (`docs/title-and-menu.md` § Decisions settings §4.2).
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_key(
    menu: &mut MenuState,
    credentials: &mut CredentialsState,
    fields: &mut Query<(Entity, &mut EditableText), With<KeyField>>,
    input_focus: Option<&mut InputFocus>,
    session: &mut SessionInput,
    checker: Option<&Arc<dyn KeyCheck>>,
    pending_key_check: &mut PendingKeyCheck,
) {
    let Some((entity, mut field)) = fields.iter_mut().next() else {
        menu.key_editing = false;
        return;
    };
    let typed = field.value().to_string();
    let key = credentials::trim_ascii_whitespace(&typed).to_string();

    if !credentials::shape_ok(&key) {
        credentials.row = KeyRow::Invalid;
        leave_key_field(menu, entity, &mut field, input_focus, false);
        return;
    }

    let (notice, hold) = session.notice_and_hold();
    if !credentials::store(credentials, &key, notice, hold) {
        // Row 23 already noticed; the row is left as it was and the buffer
        // stays so the player can retry. No ping runs.
        leave_key_field(menu, entity, &mut field, input_focus, false);
        return;
    }

    // The write landed, so the key is on disk before any network call. Show
    // `Checking…`, clear the field, and let the poll system resolve the
    // verdict.
    credentials.row = KeyRow::Checking;
    pending_key_check.task = Some(spawn_key_check(checker.cloned(), key));
    leave_key_field(menu, entity, &mut field, input_focus, true);
}

/// Return to the walk after a commit, optionally clearing the buffer and
/// releasing `InputFocus` from the field.
pub(super) fn leave_key_field(
    menu: &mut MenuState,
    entity: Entity,
    field: &mut EditableText,
    input_focus: Option<&mut InputFocus>,
    clear: bool,
) {
    if clear {
        field.clear();
    }
    menu.key_editing = false;
    menu.focus = MenuFocus::KeyRow;
    if let Some(input_focus) = input_focus {
        if input_focus.get() == Some(entity) {
            input_focus.clear();
        }
    }
}

/// `Copy path`: write the credential file's path to the clipboard and start the
/// label's `Copied` countdown. No path (an underivable directory) copies
/// nothing (`docs/title-and-menu.md` § Decisions settings §4.1).
pub(super) fn copy_key_path(
    credentials: &CredentialsState,
    port: &mut ClipboardPort,
    clipboard: Option<&mut Clipboard>,
    label: &mut CopyPathLabel,
) {
    let Some(path) = credentials.path.as_deref() else {
        return;
    };
    let text = path.display().to_string();
    credentials::clipboard_write(port, clipboard, &text);
    label.remaining = credentials::COPIED_LABEL_SECONDS;
}
