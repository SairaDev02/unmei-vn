//! The Decisions group: the interactive key row and its read-only siblings, the
//! paste and ownership filtering of the built-in field, the outside-press
//! discard and the pair of in-place row repaints.

use bevy::input_focus::InputFocus;
use bevy::picking::events::{Pointer, Press};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter, TextEdit};
use bevy::ui_widgets::SelectAllOnFocus;

use super::super::focus_border;
use super::super::style::{TextFaces, BUTTON_BG, DIM, FAINT, TEXT};
use crate::credentials::{
    self, ClipboardPort, CopyPathLabel, CredentialsState, KeyField, KeyRow, KEY_PLACEHOLDER,
};
use crate::input::InputOwnership;
use crate::menu::{MenuAction, MenuState};

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

/// Render the Decisions group — the last group, after Audio and Play, in both
/// Settings surfaces: the interactive key row (its state line, the built-in
/// `EditableText` field and its placeholder), `Copy path`, `Clear key` and the
/// `Stored in <path>` line. A `From environment` row is read-only, so its field
/// is a plain line and `Clear key` is disabled.
///
/// The row is a walk stop; the field enters edit mode through `Enter` or a
/// click, at which point `↑`/`↓` stop walking
/// (`docs/title-and-menu.md` § Decisions settings §4.1/§4.2).
pub(super) fn decisions_group(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    credentials: Option<&CredentialsState>,
) {
    parent.spawn((
        Text::new("Decisions"),
        faces.mono(18.0),
        TextColor::from(DIM),
    ));

    let row = credentials
        .map(|state| state.row.clone())
        .unwrap_or_default();
    let from_environment = row.is_from_environment();

    parent
        .spawn((
            MenuAction::KeyRow,
            Button,
            Node {
                width: Val::Px(440.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(BUTTON_BG),
            BorderColor::all(focus_border(menu, MenuAction::KeyRow)),
        ))
        .with_children(|row_node| {
            row_node.spawn((
                KeyRowText,
                Text::new(row.row_text()),
                faces.mono(14.0),
                TextColor::from(TEXT),
            ));
            if from_environment {
                // The env note already carries the fingerprint; the field is
                // not editable, so it is a plain (empty) line.
                row_node.spawn((Text::new(""), faces.mono(15.0), TextColor::from(FAINT)));
                return;
            }
            row_node
                .spawn(Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(22.0),
                    position_type: PositionType::Relative,
                    ..default()
                })
                .with_children(|field| {
                    field.spawn((
                        KeyPlaceholder,
                        Text::new(KEY_PLACEHOLDER),
                        faces.mono(15.0),
                        TextColor::from(FAINT),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(2.0),
                            ..default()
                        },
                    ));
                    field.spawn((
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
        });

    // The `Stored in <path>` line follows the row and is itself followed by the
    // `Copy path` control (`docs/title-and-menu.md` § Decisions settings §4.1).
    let path_line = credentials::path_line(credentials.and_then(|state| state.path.as_deref()));
    parent.spawn((
        Text::new(path_line),
        faces.mono(13.0),
        TextColor::from(FAINT),
    ));

    parent.spawn((
        MenuAction::CopyKeyPath,
        Button,
        Node {
            width: Val::Px(440.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(BUTTON_BG),
        BorderColor::all(focus_border(menu, MenuAction::CopyKeyPath)),
        CopyPathLabelText,
        Text::new("Copy path"),
        faces.sans(16.0),
        TextColor::from(TEXT),
    ));

    parent.spawn((
        MenuAction::ClearKey,
        Button,
        Node {
            width: Val::Px(440.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(BUTTON_BG),
        BorderColor::all(focus_border(menu, MenuAction::ClearKey)),
        Text::new("Clear key"),
        faces.sans(16.0),
        TextColor::from(if from_environment { FAINT } else { TEXT }),
    ));
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
pub(crate) fn sync_key_row(
    credentials: Option<Res<CredentialsState>>,
    mut texts: Query<&mut Text, With<KeyRowText>>,
    fields: Query<&EditableText, With<KeyField>>,
    mut field_colors: Query<&mut TextColor, With<KeyField>>,
    mut placeholders: Query<&mut Visibility, With<KeyPlaceholder>>,
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
}

/// Tick the `Copy path` label's countdown and repaint it, so the control reads
/// `Copied` for [`COPIED_LABEL_SECONDS`] and then returns to `Copy path`
/// (`docs/title-and-menu.md` § Decisions settings §4.1).
pub(crate) fn sync_copy_path_label(
    time: Res<Time>,
    mut label: ResMut<CopyPathLabel>,
    mut texts: Query<&mut Text, With<CopyPathLabelText>>,
) {
    if label.remaining > 0.0 {
        label.remaining = (label.remaining - time.delta_secs()).max(0.0);
    }
    let want = if label.remaining > 0.0 {
        "Copied"
    } else {
        "Copy path"
    };
    for mut text in &mut texts {
        if text.as_str() != want {
            **text = want.to_string();
        }
    }
}
