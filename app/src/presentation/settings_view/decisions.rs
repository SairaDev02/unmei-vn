//! The Decisions group: the interactive key row and its read-only siblings, the
//! paste and ownership filtering of the built-in field, the outside-press
//! discard and the pair of in-place row repaints.
//!
//! The group's frame, its state line, its `Stored in <path>` line and its three
//! controls are the plugin's tree; this module mounts the editable field the
//! format has no slot for, the placeholder's visibility and the id of the
//! `Copy path` label the countdown repaints.

use bevy::input_focus::InputFocus;
use bevy::picking::events::{Pointer, Press};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter, TextEdit};
use bevy::ui_widgets::SelectAllOnFocus;

use super::super::render::tree::{ControlLabel, Rendered};
use super::super::style::{TextFaces, FAINT, TEXT};
use crate::credentials::{
    self, ClipboardPort, CopyPathLabel, CredentialsState, KeyField, KeyRow, KEY_PLACEHOLDER,
};
use crate::input::InputOwnership;
use crate::menu::{MenuAction, MenuState};
use unmei_engine::copy;

/// The key row's live state line, repainted in place so a row-state change
/// never rebuilds the tree (which would drop the field's typed text).
#[derive(Component)]
pub(crate) struct KeyRowText;

/// The empty field's placeholder, shown only on a Not-set row.
#[derive(Component)]
pub(crate) struct KeyPlaceholder;

/// The `Copy path` control's label, flipped to `Copied` during its countdown.
#[derive(Component)]
pub(crate) struct CopyPathLabelText;

/// Mount the Decisions group's engine-owned pieces into the tree's row: the
/// state line's id, the placeholder's visibility, the `Copy path` label's id and
/// the built-in `EditableText` field. A `From environment` row is read-only, so
/// it gets no field, and `Clear key` is disabled by the surface's own binding.
///
/// The row is a walk stop; the field enters edit mode through `Enter` or a
/// click, at which point `↑`/`↓` stop walking
/// (`docs/title-and-menu.md` § Decisions settings §4.1/§4.2).
pub(super) fn mount_decisions(
    parent: &mut ChildSpawnerCommands,
    rendered: &Rendered,
    faces: &TextFaces,
    credentials: Option<&CredentialsState>,
) {
    let row = credentials
        .map(|state| state.row.clone())
        .unwrap_or_default();
    if let Some(state_line) = rendered.slots.get("settings.key-state").copied() {
        parent.commands().entity(state_line).insert(KeyRowText);
    }
    if let Some(key) = rendered.controls.get("settings.key").copied() {
        let label = key_label(&row, row == KeyRow::NotSet);
        parent.commands().entity(key).insert(ControlLabel(label));
    }
    if let Some(label) = rendered.slots.get("settings.copy-path-label").copied() {
        parent.commands().entity(label).insert(CopyPathLabelText);
    }
    if let Some(placeholder) = rendered.slots.get("settings.key-placeholder").copied() {
        parent.commands().entity(placeholder).insert((
            KeyPlaceholder,
            if row == KeyRow::NotSet {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ));
    }
    if row.is_from_environment() {
        return;
    }
    let Some(field) = rendered.controls.get("settings.key").copied() else {
        return;
    };
    parent.commands().entity(field).with_children(|node| {
        node.spawn((
            KeyField,
            EditableText::new(""),
            faces.mono(15.0),
            TextColor::from(TEXT),
            // Newlines are rejected outright. A paste is trimmed of
            // its surrounding whitespace before it reaches this
            // filter, so `key\n` is not dropped whole
            // (`docs/title-and-menu.md` § Decisions settings §4.2).
            EditableTextFilter::new(|character| character != '\n' && character != '\r'),
            SelectAllOnFocus,
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(22.0),
                ..default()
            },
        ));
    });
}

/// Resolve queued pastes before the widget's own `apply_text_edits`, so the OS
/// clipboard is read once, trimmed of surrounding ASCII whitespace, and never
/// handed to the all-or-nothing filter as a raw `Paste`
/// (`docs/title-and-menu.md` § Decisions settings §4.2).
pub(crate) fn trim_pasted_text(
    port: Res<ClipboardPort>,
    mut clipboard: Option<ResMut<Clipboard>>,
    mut fields: Query<&mut EditableText, With<KeyField>>,
) {
    for mut field in &mut fields {
        if !field
            .pending_edits
            .iter()
            .any(|edit| matches!(edit, TextEdit::Paste))
        {
            continue;
        }
        let text = credentials::clipboard_read(&port, clipboard.as_deref_mut());
        credentials::substitute_paste_edits(&mut field.pending_edits, text.as_deref());
    }
}

/// While a claimant owns app input, or a held key/button is still quarantined,
/// drop the widget edits the keyboard/pointer observers queued this frame
/// instead of letting `apply_text_edits` store them for a later frame. This is
/// the consume half of the ownership boundary: the queued edits never become a
/// late insertion, commit or paste after release. The live draft's text and
/// caret are untouched — only the queued actions are discarded — so an ordinary
/// frame (every shipping frame with nothing held) is a no-op.
///
/// The polarity is deliberate. `bevy_ui` puts the apply set in
/// `UiSystems::Content`, while the focus-change producer runs later in
/// `PostLayout`, so the drain cannot both run after that producer and before
/// apply without cycling the schedule. It runs *before* apply every frame
/// instead: an edit queued by the later producer is consumed on the next
/// blocked frame, and if the claim has already ended the ordinary
/// focus-loss discard applies then — a genuine focus loss is never hidden, and
/// a discarded draft is never resurrected.
pub(crate) fn discard_queued_key_edits_when_blocked(
    ownership: Option<Res<InputOwnership>>,
    quarantine: Option<Res<crate::input::InputQuarantine>>,
    mut fields: Query<&mut EditableText, With<KeyField>>,
) {
    if crate::input::app_input_ready(ownership, quarantine) {
        return;
    }
    for mut field in &mut fields {
        field.pending_edits.clear();
        field.pending_paste = None;
    }
}

/// A primary press anywhere but the key field's row leaves the field and
/// discards, so a click on empty space behaves like a click elsewhere
/// (`docs/title-and-menu.md` § Decisions settings §4.2). The widget's own press
/// on the field is left to focus it.
///
/// `Pointer` events auto-propagate, so this fires once per ancestor with the
/// event target re-pointed at each; the guard uses the *original* hit and walks
/// up from there, so a press inside the row never discards and a press on the
/// backdrop does.
#[allow(clippy::too_many_arguments)]
pub(crate) fn discard_key_edit_on_outside_press(
    press: On<Pointer<Press>>,
    mut menu: ResMut<MenuState>,
    actions: Query<&MenuAction>,
    parents: Query<&ChildOf>,
    mut editables: Query<&mut EditableText, With<KeyField>>,
    input_focus: Option<ResMut<InputFocus>>,
    ownership: Option<Res<InputOwnership>>,
    quarantine: Option<Res<crate::input::InputQuarantine>>,
) {
    if press.button != PointerButton::Primary || !menu.key_editing {
        return;
    }
    // While a claimant owns app input, or a button held across its transition
    // is still quarantined, an outside press must not discard the suspended
    // draft; the block rides the same ownership boundary as the overlay's own
    // input.
    if !crate::input::app_input_ready(ownership, quarantine) {
        return;
    }
    let target = press.original_event_target();
    let in_key_row = std::iter::successors(Some(target), |entity| {
        parents.get(*entity).ok().map(ChildOf::parent)
    })
    .any(|entity| {
        actions
            .get(entity)
            .map(|action| *action == MenuAction::KeyRow)
            .unwrap_or(false)
    });
    if in_key_row {
        return;
    }
    menu.key_editing = false;
    for mut field in &mut editables {
        field.clear();
    }
    if let Some(mut input_focus) = input_focus {
        input_focus.clear();
    }
}

/// Repaint the key row's state line and placeholder in place, so a row change
/// (a commit, a Clear, a read-on-open) never rebuilds the tree and drops the
/// field's typed text. The placeholder shows only while the field is empty and
/// the row is Not set (`docs/title-and-menu.md` § Decisions settings §4.1).
///
/// The control's own label follows the two: the row draws its state line in a
/// sibling slot, so the words a reader asks the control for are kept here.
pub(crate) fn sync_key_row(
    credentials: Option<Res<CredentialsState>>,
    mut texts: Query<&mut Text, With<KeyRowText>>,
    fields: Query<&EditableText, With<KeyField>>,
    mut field_colors: Query<&mut TextColor, With<KeyField>>,
    mut placeholders: Query<&mut Visibility, With<KeyPlaceholder>>,
    mut labels: Query<(&MenuAction, &mut ControlLabel)>,
) {
    let row = credentials
        .map(|state| state.row.clone())
        .unwrap_or_default();
    let want = row.row_text();
    for mut text in &mut texts {
        if text.as_str() != want {
            **text = want.clone();
        }
    }

    // `Checking…` disables the input: the field is greyed until the verdict
    // lands (`docs/title-and-menu.md` § Decisions settings §4.1).
    let field_color = if row.is_checking() { FAINT } else { TEXT };
    for mut color in &mut field_colors {
        if color.0 != field_color {
            color.0 = field_color;
        }
    }

    let empty = fields
        .iter()
        .next()
        .map(|field| field.value().to_string().is_empty())
        .unwrap_or(true);
    let show = empty && row == KeyRow::NotSet;
    for mut visibility in &mut placeholders {
        *visibility = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let label = key_label(&row, show);
    for (action, mut drawn) in &mut labels {
        if *action == MenuAction::KeyRow && drawn.0 != label {
            drawn.0 = label.clone();
        }
    }
}

/// The label the key row's control stands for: its state line, and the
/// placeholder while the empty field shows one. The rendered row draws the two as
/// separate nodes, so this is the one place a reader asking the control what it
/// says finds both, as the hand-built row's children read before it. The row's
/// own line breaks (the environment note's) collapse to single spaces, the way a
/// reader of a control's words has always received them.
fn key_label(row: &KeyRow, placeholder: bool) -> String {
    let mut line = row.row_text();
    if placeholder {
        line.push(' ');
        line.push_str(KEY_PLACEHOLDER);
    }
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Tick the `Copy path` label's countdown and repaint it, so the control reads
/// `Copied` for [`COPIED_LABEL_SECONDS`] and then returns to `Copy path`
/// (`docs/title-and-menu.md` § Decisions settings §4.1).
pub(crate) fn sync_copy_path_label(
    time: Res<Time>,
    mut label: ResMut<CopyPathLabel>,
    mut texts: Query<&mut Text, With<CopyPathLabelText>>,
    mut labels: Query<(&MenuAction, &mut ControlLabel)>,
) {
    if label.remaining > 0.0 {
        label.remaining = (label.remaining - time.delta_secs()).max(0.0);
    }
    let want = if label.remaining > 0.0 {
        copy::text("settings.control.copy-path.copied")
    } else {
        copy::text("settings.control.copy-path")
    };
    for mut text in &mut texts {
        if text.as_str() != want {
            **text = want.to_string();
        }
    }
    for (action, mut drawn) in &mut labels {
        if *action == MenuAction::CopyKeyPath && drawn.0 != want {
            drawn.0 = want.to_string();
        }
    }
}
