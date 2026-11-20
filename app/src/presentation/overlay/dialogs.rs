//! The overlay's two dialogs: the one confirmation every destructive action
//! passes through, and the one-time legacy import picker.

use bevy::prelude::*;
use bevy::text::{LineBreak, TextLayout};

use super::super::focus_border;
use super::super::style::{TextFaces, BUTTON_BG, DIM, FAINT, PANEL_BG, TEXT};
use crate::credentials::CredentialsState;
use crate::menu::{self, MenuAction, MenuState};
use crate::Runtime;
use unmei_engine::copy;

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
            copy::text("confirm.overwrite.title").to_string(),
            copy::render(
                "confirm.overwrite.prompt",
                &[("slot", menu::slot_label(slot))],
            ),
            copy::text("confirm.button.yes"),
        ),
        menu::ConfirmKind::Load(slot) => (
            copy::text("confirm.load.title").to_string(),
            copy::render("confirm.load.prompt", &[("slot", menu::slot_label(slot))]),
            copy::text("confirm.button.yes"),
        ),
        menu::ConfirmKind::ResetAudio => (
            copy::text("confirm.reset-audio.title").to_string(),
            copy::text("confirm.reset-audio.prompt").to_string(),
            copy::text("confirm.button.yes"),
        ),
        menu::ConfirmKind::ResetPlay => (
            copy::text("confirm.reset-play.title").to_string(),
            copy::text("confirm.reset-play.prompt").to_string(),
            copy::text("confirm.button.yes"),
        ),
        menu::ConfirmKind::NewGame => (
            copy::text("confirm.new-game.title").to_string(),
            copy::text("confirm.new-game.prompt").to_string(),
            copy::text("confirm.button.start"),
        ),
        menu::ConfirmKind::Quit => (
            copy::text("confirm.quit.title").to_string(),
            copy::text("confirm.quit.prompt").to_string(),
            copy::text("confirm.button.quit"),
        ),
        menu::ConfirmKind::ExitToTitle => (
            copy::text("confirm.exit-to-title.title").to_string(),
            copy::text("confirm.exit-to-title.prompt").to_string(),
            copy::text("confirm.button.exit"),
        ),
        menu::ConfirmKind::ClearKey => {
            let path = credentials
                .and_then(|state| state.path.as_deref())
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| copy::text("confirm.clear-key.none").to_string());
            (
                copy::text("confirm.clear-key.title").to_string(),
                copy::render("confirm.clear-key.prompt", &[("path", &path)]),
                copy::text("confirm.button.yes"),
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
                copy::render("confirm.timeline-jump.title", &[("scene", &scene_id)]),
                menu::TIMELINE_CONFIRM_DETAIL.to_string(),
                copy::text("confirm.button.jump"),
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
                                Text::new(copy::text("confirm.button.cancel")),
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
                        Text::new(copy::text("legacy-picker.heading")),
                        faces.sans_semibold(22.0),
                        TextColor::from(TEXT),
                    ));
                    panel.spawn((
                        Text::new(copy::text("legacy-picker.prompt")),
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
                        Text::new(copy::text("legacy-picker.cancel")),
                        faces.sans(16.0),
                        TextColor::from(TEXT),
                    ));
                });
        });
}
