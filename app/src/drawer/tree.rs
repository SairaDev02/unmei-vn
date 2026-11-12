//! The drawer tree: spawn it on open, tear it down on close, repaint controls.

use bevy::prelude::*;
use bevy::text::LineHeight;
use bevy::ui::UiRect;

use crate::presentation::style::TextFaces;

use super::components::{
    MonitorCheckboxLabel, MonitorControlTag, MonitorGapSpacer, MonitorLogBody, MonitorLogViewport,
    MonitorPanelNode, MonitorReaderNode, MonitorRoot, MonitorRowsContent, MonitorScrollbarThumb,
    MonitorScrollbarTrack, MonitorStatusText,
};
use super::geometry::{
    BUTTON_BG, DIM, FOCUS, FOOTER_HEIGHT, HEADER_HEIGHT, PANEL_BG, PANEL_BOTTOM_RESERVATION,
    PANEL_HEIGHT, PANEL_SIDE_INSET, SCROLLBAR_THUMB, SCROLLBAR_TRACK, SCROLLBAR_WIDTH, TEXT,
};
use super::state::{MonitorControl, MonitorPanel};

/// Build or tear down the drawer tree when its visibility flips. There is no
/// hidden tree, no hidden text work and no hidden projection: closed means the
/// tree does not exist.
pub(crate) fn sync_panel_ui(
    mut commands: Commands,
    faces: Option<Res<TextFaces>>,
    panel: Res<MonitorPanel>,
    roots: Query<Entity, With<MonitorRoot>>,
) {
    let built = !roots.is_empty();
    if built == panel.open {
        return;
    }
    if !panel.open {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }
    let Some(faces) = faces else {
        // A headless world without the embedded faces has no drawable drawer;
        // the input state still works.
        return;
    };

    commands
        .spawn((
            MonitorRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            // Above the menu overlay, the title and the startup error screen,
            // whichever order the UI roots were spawned in.
            GlobalZIndex(100),
        ))
        .with_children(|root| {
            root.spawn((
                MonitorPanelNode,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(PANEL_SIDE_INSET),
                    right: Val::Px(PANEL_SIDE_INSET),
                    bottom: Val::Px(PANEL_BOTTOM_RESERVATION),
                    height: Val::Px(PANEL_HEIGHT),
                    border: UiRect::all(Val::Px(1.0)),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                BorderColor::all(Color::srgba(
                    230.0 / 255.0,
                    158.0 / 255.0,
                    92.0 / 255.0,
                    0.25,
                )),
            ))
            .with_children(|panel_node| {
                build_header(panel_node, &faces);
                panel_node
                    .spawn((
                        MonitorLogBody,
                        Node {
                            width: Val::Percent(100.0),
                            // The log body takes exactly the space between the
                            // header and the footer: with `flex_basis: 0` a
                            // wrapped row taller than the body can never grow
                            // the panel or squeeze the header, it clips inside
                            // the body.
                            flex_basis: Val::Px(0.0),
                            flex_grow: 1.0,
                            min_height: Val::Px(0.0),
                            flex_direction: FlexDirection::Row,
                            overflow: Overflow::clip(),
                            ..default()
                        },
                    ))
                    .with_children(|body| {
                        // The flexible log column: following bottom-aligns its
                        // rows; reading bottom-aligns a short tail through the
                        // gap spacer and shifts the rows group by the within-row
                        // offset.
                        body.spawn((
                            MonitorLogViewport,
                            MonitorReaderNode::Viewport,
                            Node {
                                flex_grow: 1.0,
                                min_width: Val::Px(0.0),
                                flex_direction: FlexDirection::Column,
                                justify_content: JustifyContent::FlexEnd,
                                ..default()
                            },
                        ))
                        .with_children(|viewport| {
                            viewport.spawn((
                                MonitorGapSpacer,
                                MonitorReaderNode::Spacer,
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Px(0.0),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                            ));
                            viewport.spawn((
                                MonitorRowsContent,
                                MonitorReaderNode::Rows,
                                Node {
                                    width: Val::Percent(100.0),
                                    flex_direction: FlexDirection::Column,
                                    position_type: PositionType::Relative,
                                    top: Val::Px(0.0),
                                    ..default()
                                },
                            ));
                        });
                        body.spawn((
                            MonitorScrollbarTrack,
                            Button,
                            Node {
                                width: Val::Px(SCROLLBAR_WIDTH),
                                height: Val::Percent(100.0),
                                flex_shrink: 0.0,
                                position_type: PositionType::Relative,
                                ..default()
                            },
                            BackgroundColor(SCROLLBAR_TRACK),
                        ))
                        .with_children(|track| {
                            track.spawn((
                                MonitorScrollbarThumb,
                                MonitorReaderNode::Thumb,
                                Button,
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: Val::Px(0.0),
                                    top: Val::Percent(0.0),
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(SCROLLBAR_THUMB),
                            ));
                        });
                    });
                panel_node.spawn((
                    MonitorStatusText,
                    Text::new(""),
                    faces.mono(11.0),
                    TextColor::from(DIM),
                    LineHeight::Px(FOOTER_HEIGHT),
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(FOOTER_HEIGHT),
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(0.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                ));
            });
        });
}

fn build_header(parent: &mut ChildSpawnerCommands<'_>, faces: &TextFaces) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(HEADER_HEIGHT),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(0.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.16, 0.17, 0.22)),
        ))
        .with_children(|header| {
            header.spawn((
                Text::new("Monitor"),
                faces.sans_semibold(15.0),
                TextColor::from(TEXT),
            ));
            header
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|controls| {
                    control_button(
                        controls,
                        faces,
                        MonitorControl::ShowEverything,
                        "[ ] Show everything",
                        Some(MonitorCheckboxLabel),
                    );
                    control_button::<MonitorCheckboxLabel>(
                        controls,
                        faces,
                        MonitorControl::FollowLatest,
                        "Follow latest",
                        None,
                    );
                    control_button::<MonitorCheckboxLabel>(
                        controls,
                        faces,
                        MonitorControl::Close,
                        "Close",
                        None,
                    );
                });
        });
}

fn control_button<M: Component>(
    parent: &mut ChildSpawnerCommands<'_>,
    faces: &TextFaces,
    control: MonitorControl,
    label: &str,
    marker: Option<M>,
) {
    let mut entity = parent.spawn((
        MonitorControlTag(control),
        Button,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(BUTTON_BG),
        BorderColor::all(Color::NONE),
        Text::new(label),
        faces.mono(12.0),
        TextColor::from(TEXT),
    ));
    if let Some(marker) = marker {
        entity.insert(marker);
    }
}

/// Repaint the header controls: the checkbox reflects the live predicate and
/// the focused control carries the accent border.
pub(crate) fn sync_panel_controls(
    panel: Res<MonitorPanel>,
    mut checkboxes: Query<&mut Text, (With<MonitorCheckboxLabel>, Without<MonitorStatusText>)>,
    mut controls: Query<(&MonitorControlTag, &mut BorderColor)>,
) {
    if !panel.open {
        return;
    }
    let want = panel.view.checkbox_label();
    for mut text in &mut checkboxes {
        if text.as_str() != want {
            **text = want.to_string();
        }
    }
    for (tag, mut border) in &mut controls {
        let color = if tag.0 == panel.focus {
            FOCUS
        } else {
            Color::NONE
        };
        if border.top != color {
            *border = BorderColor::all(color);
        }
    }
}
