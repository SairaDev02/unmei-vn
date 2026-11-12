//! The title door's Settings view: the pack backdrop behind a bounded,
//! top-anchored panel holding the shared body.

use bevy::prelude::*;
use bevy::ui::ScrollPosition;
use bevy::ui_widgets::ScrollArea;

use super::super::assets::AssetWatch;
use super::super::overlay::confirm_dialog;
use super::super::style::{TextFaces, FAINT, PANEL_BG};
use super::super::title::{spawn_title_backdrop, title_gradient, TITLE_SETTINGS_WIDTH};
use super::super::MenuRoot;
use super::groups::settings_body;
use super::SettingsBody;
use crate::credentials::CredentialsState;
use crate::menu::MenuState;
use crate::settings::SettingsState;
use crate::Runtime;

/// The title door's Settings view: the same groups as the overlay's, in a
/// top-anchored panel, with `← Back to title` and no rail, Resume or
/// `Exit to title`. It keeps the title's backdrop.
#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn build_title_settings(
    commands: &mut Commands,
    faces: &TextFaces,
    menu: &MenuState,
    runtime: &Runtime,
    settings: Option<&SettingsState>,
    credentials: Option<&CredentialsState>,
    asset_server: Option<&AssetServer>,
    watch: Option<&mut AssetWatch>,
) {
    let backdrop = runtime.runner.pack().manifest.title_backdrop.clone();
    commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(Val::Px(52.0)),
                ..default()
            },
            title_gradient(),
        ))
        .with_children(|root| {
            // The same pack backdrop stands behind Settings opened from the
            // title (`docs/title-and-menu.md` § Title layout §5).
            spawn_title_backdrop(root, backdrop.as_deref(), asset_server, watch);
            root.spawn((
                Node {
                    width: Val::Px(TITLE_SETTINGS_WIDTH),
                    max_width: Val::Percent(100.0),
                    // The panel is bounded to the root's content box, so its
                    // inner viewport is real and the panel never runs past the
                    // window edge (`docs/title-and-menu.md` § Title layout §5,
                    // `### Validation`).
                    max_height: Val::Percent(100.0),
                    min_height: Val::Px(0.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(20.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                BorderColor::all(FAINT),
            ))
            .with_children(|panel| {
                // The scroll viewport is the panel's content box, so the border
                // and padding stay a fixed frame and the focused control is
                // revealed against the real clip, not under the border.
                panel
                    .spawn((
                        SettingsBody,
                        ScrollArea,
                        ScrollPosition::default(),
                        Node {
                            width: Val::Percent(100.0),
                            flex_grow: 1.0,
                            min_height: Val::Px(0.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(10.0),
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
                            "\u{2190} Back to title",
                        );
                    });
            });

            if let Some(confirm) = menu.confirm {
                confirm_dialog(root, faces, menu, confirm, credentials, runtime);
            }
        });
}
