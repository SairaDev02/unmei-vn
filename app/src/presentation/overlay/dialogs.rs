//! The overlay's two dialogs: the one confirmation every destructive action
//! passes through, and the one-time legacy import picker.

use bevy::prelude::*;
use bevy::text::{LineBreak, TextLayout};

use super::super::focus_border;
use super::super::style::{TextFaces, BUTTON_BG, DIM, FAINT, PANEL_BG, TEXT};
use crate::credentials::CredentialsState;
use crate::menu::{self, MenuAction, MenuState};
use crate::Runtime;

const DANGER: Color = Color::srgb(0.48, 0.16, 0.16);

pub(in crate::presentation) fn confirm_dialog(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    confirm: menu::ConfirmKind,
    credentials: Option<&CredentialsState>,
    runtime: &Runtime,
) {
    let (title, detail, accept): (String, String, &str) = match confirm {
        menu::ConfirmKind::Overwrite(slot) => (
            "Overwrite".to_string(),
            format!(
                "Overwrite {}? Its current save is replaced.",
                menu::slot_label(slot)
            ),
            "Yes",
        ),
        menu::ConfirmKind::Load(slot) => (
            "Load".to_string(),
            format!(
                "Load {}? Unsaved progress is lost and any decision in flight is cancelled.",
                menu::slot_label(slot)
            ),
            "Yes",
        ),
        menu::ConfirmKind::ResetAudio => (
            "Reset audio".to_string(),
            "Reset Master, Music and SFX to 100% and turn mute off? The file is not deleted."
                .to_string(),
            "Yes",
        ),
        menu::ConfirmKind::ResetPlay => (
            "Reset play".to_string(),
            "Reset text speed to Instant, turn auto off and return to windowed? The audio mix is untouched and the file is not deleted."
                .to_string(),
            "Yes",
        ),
        menu::ConfirmKind::NewGame => (
            "Start a new game?".to_string(),
            "The session in progress is discarded. Your saved slots and the autosave are untouched."
                .to_string(),
            "Start",
        ),
        menu::ConfirmKind::Quit => (
            "Quit?".to_string(),
            "Unsaved progress is lost and any decision in flight is cancelled.".to_string(),
            "Quit",
        ),
        menu::ConfirmKind::ExitToTitle => (
            "Exit to the title?".to_string(),
            "Your session is kept \u{2014} Continue resumes it where you left off.".to_string(),
            "Exit",
        ),
        menu::ConfirmKind::ClearKey => {
            let path = credentials
                .and_then(|state| state.path.as_deref())
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "\u{2014}".to_string());
            (
                "Clear key?".to_string(),
                format!(
                    "The stored key is deleted from {path}. You can paste it again at any time."
                ),
                "Yes",
            )
        }
        menu::ConfirmKind::TimelineJump(index) => {
            let scene_id = runtime
                .runner
                .pack()
                .timeline_entries()
                .get(index)
                .cloned()
                .unwrap_or_default();
            (
                format!("Jump to {scene_id}?"),
                menu::TIMELINE_CONFIRM_DETAIL.to_string(),
                "Jump",
            )
        }
    };

    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        ))
        .with_children(|dimmer| {
            dimmer
                .spawn((
                    Node {
                        width: Val::Px(420.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(12.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                    BorderColor::all(FAINT),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new(title),
                        faces.sans_semibold(22.0),
                        TextColor::from(TEXT),
                        // A long raw scene id in `Jump to <id>?` is one
                        // unbroken token; fall back to character breaks so the
                        // title stays inside the 420px panel
                        // (`docs/title-and-menu.md` § Timeline §3/`### Layout`).
                        TextLayout {
                            linebreak: LineBreak::WordOrCharacter,
                            ..default()
                        },
                    ));
                    panel.spawn((Text::new(detail), faces.sans(15.0), TextColor::from(DIM)));
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(8.0),
                            justify_content: JustifyContent::End,
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                MenuAction::ConfirmAccept,
                                Button,
                                Node {
                                    padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
                                    border: UiRect::all(Val::Px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(DANGER),
                                BorderColor::all(focus_border(menu, MenuAction::ConfirmAccept)),
                                Text::new(accept.to_string()),
                                faces.sans(16.0),
                                TextColor::from(TEXT),
                            ));
                            row.spawn((
                                MenuAction::ConfirmCancel,
                                Button,
                                Node {
                                    padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
                                    border: UiRect::all(Val::Px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(BUTTON_BG),
                                BorderColor::all(focus_border(menu, MenuAction::ConfirmCancel)),
                                Text::new("Cancel"),
                                faces.sans(16.0),
                                TextColor::from(TEXT),
                            ));
                        });
                });
        });
}

/// The one-time legacy import picker: choose a manual target. Occupied targets
/// are labelled `(overwrite)`; there is deliberately no second prompt beyond
/// this pick. `Esc` or Cancel backs out to the Legacy card.
pub(super) fn legacy_picker(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
) {
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        ))
        .with_children(|dimmer| {
            dimmer
                .spawn((
                    Node {
                        width: Val::Px(460.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(20.0)),
                        row_gap: Val::Px(10.0),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                    BorderColor::all(FAINT),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Import legacy save"),
                        faces.sans_semibold(22.0),
                        TextColor::from(TEXT),
                    ));
                    panel.spawn((
                        Text::new("Choose a slot. Occupied slots are overwritten."),
                        faces.sans(15.0),
                        TextColor::from(DIM),
                    ));

                    for target in menu::legacy_targets(menu) {
                        // The target's label is the sans root and its
                        // status/detail a mono span, matching the slot card
                        // (`docs/title-and-menu.md` § Type system §2/§4).
                        let machine = format!("\n{} - {}", target.status, target.detail);
                        let mut row = panel.spawn((
                            MenuAction::LegacyTarget(target.slot),
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                                border: UiRect::all(Val::Px(2.0)),
                                flex_direction: FlexDirection::Column,
                                ..default()
                            },
                            BackgroundColor(BUTTON_BG),
                            BorderColor::all(focus_border(
                                menu,
                                MenuAction::LegacyTarget(target.slot),
                            )),
                            Text::new(target.display()),
                            faces.sans(15.0),
                            TextColor::from(TEXT),
                        ));
                        row.with_children(|row| {
                            row.spawn((
                                TextSpan::new(machine),
                                faces.mono(15.0),
                                TextColor::from(TEXT),
                            ));
                        });
                    }

                    panel.spawn((
                        MenuAction::LegacyCancel,
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            align_self: AlignSelf::End,
                            ..default()
                        },
                        BackgroundColor(BUTTON_BG),
                        BorderColor::all(focus_border(menu, MenuAction::LegacyCancel)),
                        Text::new("Cancel"),
                        faces.sans(16.0),
                        TextColor::from(TEXT),
                    ));
                });
        });
}
