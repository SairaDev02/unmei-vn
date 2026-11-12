//! The Settings body both doors mount, and its three groups in walk order.

use bevy::prelude::*;

use super::super::focus_border;
use super::super::style::{TextFaces, BUTTON_BG, DIM, FOCUS, PANEL_BG, TEXT};
use super::controls::{
    audio_control_label, reset_button, switch_row, text_speed_row, SettingsBar, SettingsLabel,
    SettingsLabelKind, SettingsTrack, SETTINGS_LABEL_WIDTH,
};
use super::decisions::decisions_group;
use crate::credentials::CredentialsState;
use crate::menu::{MenuAction, MenuState};
use crate::settings::{AudioControl, SettingsState, TrackControl};

/// The one Settings body both doors mount inside their scroll container: the
/// heading, the three groups in walk order and the door-aware back control. The
/// caller owns the container's chrome (the title's bordered panel, the overlay's
/// main column) and its group gap; the groups and the walk are shared
/// (`docs/title-and-menu.md` § Title §4, § Title layout §5).
pub(in crate::presentation) fn settings_body(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    settings: Option<&SettingsState>,
    credentials: Option<&CredentialsState>,
    back_label: &str,
) {
    parent.spawn((
        Text::new("Settings"),
        faces.sans_semibold(26.0),
        TextColor::from(TEXT),
    ));
    settings_group(parent, faces, menu, settings);
    play_group(parent, faces, menu, settings);
    decisions_group(parent, faces, menu, credentials);
    parent.spawn((
        MenuAction::BackToSlots,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
            border: UiRect::all(Val::Px(2.0)),
            align_self: AlignSelf::Start,
            ..default()
        },
        BackgroundColor(BUTTON_BG),
        BorderColor::all(focus_border(menu, MenuAction::BackToSlots)),
        Text::new(back_label),
        faces.sans(16.0),
        TextColor::from(TEXT),
    ));
}

/// Render the audio group: Master, Music, SFX sliders, a Mute toggle, and an
/// audio-only Reset. The values are live: [`sync_settings_labels`] repaints them
/// in place while the group is open.
pub(in crate::presentation) fn settings_group(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    settings: Option<&SettingsState>,
) {
    parent.spawn((Text::new("Audio"), faces.mono(18.0), TextColor::from(DIM)));

    for control in AudioControl::ALL {
        let percent = settings.map(|s| s.percent(control)).unwrap_or(100);
        let action = MenuAction::AudioSlider(control);
        parent
            .spawn((
                action,
                Button,
                Node {
                    width: Val::Px(440.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(BUTTON_BG),
                BorderColor::all(focus_border(menu, action)),
            ))
            .with_children(|row| {
                row.spawn((
                    Node {
                        width: Val::Px(SETTINGS_LABEL_WIDTH),
                        ..default()
                    },
                    Text::new(audio_control_label(control)),
                    faces.sans(16.0),
                    TextColor::from(TEXT),
                ));
                row.spawn((
                    SettingsTrack {
                        control: TrackControl::Audio(control),
                    },
                    Node {
                        width: Val::Px(220.0),
                        height: Val::Px(10.0),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                ))
                .with_children(|track| {
                    track.spawn((
                        SettingsBar {
                            control: TrackControl::Audio(control),
                        },
                        Node {
                            width: Val::Percent(percent as f32),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(FOCUS),
                    ));
                });
                row.spawn((
                    SettingsLabel {
                        kind: SettingsLabelKind::Percent(control),
                    },
                    Text::new(format!("{percent}%")),
                    faces.mono(16.0),
                    TextColor::from(TEXT),
                ));
            });
    }

    let mute_action = MenuAction::AudioMute;
    let muted = settings.map(|s| s.muted()).unwrap_or(false);
    switch_row(
        parent,
        faces,
        menu,
        mute_action,
        SettingsLabelKind::Mute,
        muted,
    );

    reset_button(parent, faces, menu, MenuAction::AudioReset, "Reset audio");
}

/// Render the Play group: the stepped Text-speed control, the Auto and
/// Fullscreen switches, and a play-only Reset. Placed after the audio group, in
/// the order the plan fixes (Text speed, Auto, Fullscreen, Reset).
pub(in crate::presentation) fn play_group(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    settings: Option<&SettingsState>,
) {
    parent.spawn((Text::new("Play"), faces.mono(18.0), TextColor::from(DIM)));
    text_speed_row(parent, faces, menu, settings);

    let auto_action = MenuAction::PlayAuto;
    let auto = settings.map(|s| s.auto()).unwrap_or(false);
    switch_row(
        parent,
        faces,
        menu,
        auto_action,
        SettingsLabelKind::Auto,
        auto,
    );

    let fullscreen_action = MenuAction::PlayFullscreen;
    let fullscreen = settings.map(|s| s.fullscreen()).unwrap_or(false);
    switch_row(
        parent,
        faces,
        menu,
        fullscreen_action,
        SettingsLabelKind::Fullscreen,
        fullscreen,
    );

    reset_button(parent, faces, menu, MenuAction::PlayReset, "Reset play");
}
