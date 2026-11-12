//! The overlay's chrome: the left rail (pack, scene, Resume, `Exit to title`),
//! the top bar's segments and the main column that dispatches the slot grid and
//! the sub-views.

use bevy::prelude::*;
use bevy::ui::ScrollPosition;
use bevy::ui_widgets::ScrollArea;

use super::super::assets::AssetWatch;
use super::super::gallery::gallery_surface;
use super::super::settings_view::{settings_body, SettingsBody};
use super::super::style::{TextFaces, BUTTON_BG, CARD_BG, DIM, PANEL_BG, TEXT};
use super::super::timeline::timeline_surface;
use super::super::{focus_border, MenuRoot};
use super::dialogs::{confirm_dialog, legacy_picker};
use crate::credentials::CredentialsState;
use crate::menu::{self, MenuAction, MenuState, MenuView};
use crate::settings::SettingsState;
use crate::Runtime;

const OVERLAY_BG: Color = Color::srgba(0.03, 0.035, 0.05, 0.86);
const RAIL_BG: Color = Color::srgba(0.07, 0.08, 0.11, 0.96);
const TAB_ON: Color = Color::srgb(0.46, 0.28, 0.18);

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn build_overlay(
    commands: &mut Commands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    settings: Option<&SettingsState>,
    credentials: Option<&CredentialsState>,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
    read_log: Option<&crate::read_log::ReadLogState>,
    timeline_scroll: Option<Vec2>,
) {
    let pack_name = runtime.runner.pack().manifest.name.clone();
    let scene_id = runtime.runner.scene_id().to_string();

    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            BackgroundColor(OVERLAY_BG),
        ))
        .with_children(|root| {
            // ---- left rail: pack name, current scene, Resume ----
            root.spawn((
                Node {
                    width: Val::Px(240.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(20.0)),
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                BackgroundColor(RAIL_BG),
            ))
            .with_children(|rail| {
                rail.spawn((
                    Text::new(pack_name),
                    faces.sans_semibold(22.0),
                    TextColor::from(TEXT),
                ));
                rail.spawn((
                    Text::new(format!("scene: {scene_id}")),
                    faces.mono(14.0),
                    TextColor::from(DIM),
                ));
                rail.spawn((
                    MenuAction::Resume,
                    Button,
                    Node {
                        margin: UiRect::top(Val::Px(16.0)),
                        padding: UiRect::all(Val::Px(8.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(BUTTON_BG),
                    BorderColor::all(focus_border(menu, MenuAction::Resume)),
                    Text::new("\u{25b6} Resume  (Esc)"),
                    faces.sans(16.0),
                    TextColor::from(TEXT),
                ));
                // `Exit to title` is session-scoped and appears only from the
                // overlay's Settings door: the title's door has no live session
                // to leave (`docs/title-and-menu.md` § Title §4).
                if menu.view == MenuView::Settings
                    && menu.settings_origin == menu::MenuOrigin::Slots
                    && menu::live_session(menu)
                {
                    rail.spawn((
                        MenuAction::ExitToTitle,
                        Button,
                        Node {
                            padding: UiRect::all(Val::Px(8.0)),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(BUTTON_BG),
                        BorderColor::all(focus_border(menu, MenuAction::ExitToTitle)),
                        Text::new("Exit to title"),
                        faces.sans(16.0),
                        TextColor::from(TEXT),
                    ));
                }
            });

            // ---- main column: top bar + grid or Settings view ----
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                padding: UiRect::all(Val::Px(24.0)),
                row_gap: Val::Px(16.0),
                ..default()
            })
            .with_children(|main| {
                main.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|bar| {
                    segment(
                        bar,
                        faces,
                        menu,
                        MenuAction::SetMode(menu::MenuMode::Save),
                        "Save",
                        menu.mode == menu::MenuMode::Save,
                    );
                    segment(
                        bar,
                        faces,
                        menu,
                        MenuAction::SetMode(menu::MenuMode::Load),
                        "Load",
                        menu.mode == menu::MenuMode::Load,
                    );
                    bar.spawn(Node {
                        flex_grow: 1.0,
                        ..default()
                    });
                    // The sub-view segments render only while the slot grid is
                    // up, in Timeline, Gallery, Settings order
                    // (`docs/title-and-menu.md` § Gallery `### Behavior` §1).
                    if menu.view == MenuView::Slots {
                        segment(
                            bar,
                            faces,
                            menu,
                            MenuAction::OpenTimeline,
                            menu::TIMELINE_HEADING,
                            false,
                        );
                        segment(
                            bar,
                            faces,
                            menu,
                            MenuAction::OpenGallery,
                            menu::GALLERY_HEADING,
                            false,
                        );
                        segment(
                            bar,
                            faces,
                            menu,
                            MenuAction::OpenSettings,
                            "\u{2699} Settings",
                            false,
                        );
                    }
                });

                if menu.view == MenuView::Slots {
                    main.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        row_gap: Val::Px(12.0),
                        column_gap: Val::Px(12.0),
                        ..default()
                    })
                    .with_children(|grid| {
                        for (index, model) in menu::card_models(menu).into_iter().enumerate() {
                            let mut card = grid.spawn((
                                MenuAction::Card(index),
                                Button,
                                Node {
                                    width: Val::Px(260.0),
                                    height: Val::Px(120.0),
                                    padding: UiRect::all(Val::Px(12.0)),
                                    border: UiRect::all(Val::Px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(CARD_BG),
                                BorderColor::all(focus_border(menu, MenuAction::Card(index))),
                                // The label is the sans root; kind, status, detail
                                // and saved-at are mono spans and the action
                                // label a sans span, all on the one card
                                // (`docs/title-and-menu.md` § Type system §4).
                                Text::new(model.label),
                                faces.sans(14.0),
                                TextColor::from(TEXT),
                            ));
                            card.with_children(|card| {
                                for line in [
                                    format!("  ({})\n", model.kind),
                                    format!("{}\n", model.status),
                                    format!("{}\n", model.detail),
                                    format!("{}\n", model.saved_at),
                                ] {
                                    card.spawn((
                                        TextSpan::new(line),
                                        faces.mono(14.0),
                                        TextColor::from(TEXT),
                                    ));
                                }
                                card.spawn((
                                    TextSpan::new(model.action.label),
                                    faces.sans(14.0),
                                    TextColor::from(TEXT),
                                ));
                            });
                        }
                    });

                    // Full-width, and only while the legacy file is importable.
                    // Bevy UI has no dashed border, so the card is a faint
                    // full-width outline.
                    if menu::legacy_visible(menu) {
                        main.spawn((
                            MenuAction::LegacyImport,
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                                border: UiRect::all(Val::Px(2.0)),
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(PANEL_BG),
                            BorderColor::all(focus_border(menu, MenuAction::LegacyImport)),
                            Text::new("Legacy save \u{2014} Import\u{2026}"),
                            faces.sans(15.0),
                            TextColor::from(DIM),
                        ));
                    }
                } else if menu.view == MenuView::Gallery {
                    gallery_surface(main, faces, menu, runtime, asset_server, watch);
                } else if menu.view == MenuView::Timeline {
                    timeline_surface(main, faces, menu, runtime, read_log, timeline_scroll);
                } else {
                    // The overlay's Settings body is its own scroll viewport in
                    // the main column, so the rail and top bar stand unchanged
                    // while the groups keep their natural height
                    // (`docs/title-and-menu.md` § Title layout §5,
                    // `docs/runtime-hardening.md` § Menu overlay §5).
                    main.spawn((
                        SettingsBody,
                        ScrollArea,
                        ScrollPosition::default(),
                        Node {
                            width: Val::Percent(100.0),
                            flex_grow: 1.0,
                            min_height: Val::Px(0.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(16.0),
                            overflow: Overflow::scroll_y(),
                            ..default()
                        },
                    ))
                    .with_children(|body| {
                        settings_body(
                            body,
                            faces,
                            menu,
                            settings,
                            credentials,
                            "\u{2190} Back to slots",
                        );
                    });
                }

                if let Some(confirm) = menu.confirm {
                    confirm_dialog(main, faces, menu, confirm, credentials, runtime);
                }
                if menu.picker.is_some() {
                    legacy_picker(main, faces, menu);
                }
            });
        });
}

fn segment(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    action: MenuAction,
    label: &str,
    active: bool,
) {
    parent.spawn((
        action,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(if active { TAB_ON } else { BUTTON_BG }),
        BorderColor::all(focus_border(menu, action)),
        Text::new(label.to_string()),
        faces.sans(16.0),
        TextColor::from(TEXT),
    ));
}
