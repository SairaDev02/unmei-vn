//! The one-time legacy import picker. The confirmation every destructive action
//! passes through is a plugin surface now ([`super::confirm`]).

use bevy::prelude::*;

use super::super::focus_border;
use super::super::style::{TextFaces, BUTTON_BG, DIM, FAINT, PANEL_BG, TEXT};
use crate::menu::{self, MenuAction, MenuState};
use unmei_engine::copy;

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
