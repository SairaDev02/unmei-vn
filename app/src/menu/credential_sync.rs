//! Keeping the Decisions key row in step with the built-in field's focus and
//! with the credential file behind the Settings view.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;

use crate::credentials::{self, CredentialsState, KeyField, KeyRow};
use crate::failure::{self, DefensiveHold, Failure, FailureRow, Notice};

use super::state::{MenuState, MenuView};

/// Keep [`MenuState::key_editing`] in step with the widget's `InputFocus`: a
/// click on the built-in field sets focus and enters edit mode, and focus
/// leaving the field discards the typed text. A world without the focus
/// resource (the headless core tests) leaves the flag to [`menu_input`].
pub(crate) fn sync_key_focus(
    mut menu: ResMut<MenuState>,
    input_focus: Option<Res<InputFocus>>,
    fields: Query<Entity, With<KeyField>>,
    mut editables: Query<&mut EditableText, With<KeyField>>,
) {
    let Some(input_focus) = input_focus else {
        return;
    };
    let focused = fields
        .iter()
        .any(|entity| input_focus.get() == Some(entity));
    if focused {
        menu.key_editing = true;
    } else if menu.key_editing {
        menu.key_editing = false;
        for mut field in &mut editables {
            field.clear();
        }
    }
}

/// Re-read the credential file on every Settings-view open, so a hand-edit
/// behind the app is reflected in the row without a relaunch. A read never
/// rewrites the file, and the `From environment` row is left alone because the
/// file is never used while the variable wins (`docs/title-and-menu.md`
/// § Decisions settings §1).
///
/// A live verdict is **not** clobbered by the re-read when the stored value is
/// unchanged: `Checking…` stays while a ping is in flight, and a `Rejected` or
/// `Set — … not checked` verdict survives for the session. A hand-edit changes
/// the fingerprint, so the read wins and the new value is shown.
pub(crate) fn read_on_settings_open(
    menu: Res<MenuState>,
    mut credentials: ResMut<CredentialsState>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
    mut was_open: Local<bool>,
) {
    let open = menu.open && menu.view == MenuView::Settings;
    let just_opened = open && !*was_open;
    *was_open = open;
    if !just_opened || credentials.row.is_from_environment() {
        return;
    }
    let read = credentials::read(credentials.config_dir.as_deref(), None);
    if !keeps_live_verdict(&credentials.row, &read.state.row) {
        credentials.row = read.state.row;
    }
    if let Some(detail) = read.warning {
        failure::report(
            Failure::new(FailureRow::CredentialRead, detail),
            &mut notice,
            &mut hold,
        );
    }
}

/// Whether the current row's live verdict should survive a Settings re-open,
/// because the just-read file holds the same shape-valid value. A different
/// value (a hand-edit) or a read that no longer yields `Set` lets the read win.
fn keeps_live_verdict(current: &KeyRow, read: &KeyRow) -> bool {
    if current.is_checking() {
        return true;
    }
    match (current.fingerprint(), read) {
        (Some(current), KeyRow::Set { fingerprint }) => current == fingerprint,
        _ => false,
    }
}
