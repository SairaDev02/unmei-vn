//! The Settings view's controls: the live labels and bars its syncs repaint, and
//! the row builders both doors share.

use bevy::prelude::*;

use super::super::focus_border;
use super::super::style::{TextFaces, BUTTON_BG, FOCUS, PANEL_BG, TEXT};
use crate::menu::{MenuAction, MenuState};
use crate::settings::{AudioControl, SettingsState, TrackControl};

/// The fixed width of a Settings row's label column, so every track in the view
/// starts at the same x.
pub(super) const SETTINGS_LABEL_WIDTH: f32 = 96.0;

/// One of the Settings view's live text labels: a slider's percentage, or the
/// mute row's state. Updated in place so the overlay tree is never rebuilt for a
/// value change (which would drop a button's `Interaction` state mid-drag).
#[derive(Component)]
pub(crate) struct SettingsLabel {
    pub(crate) kind: SettingsLabelKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsLabelKind {
    Percent(AudioControl),
    Mute,
    TextSpeed,
    Auto,
    Fullscreen,
}

impl SettingsLabelKind {
    /// The switch row's static label; `None` for a slider or the stepped
    /// text-speed value, whose label is drawn beside its track rather than from
    /// this kind.
    fn switch_label(self) -> Option<&'static str> {
        match self {
            SettingsLabelKind::Percent(_) | SettingsLabelKind::TextSpeed => None,
            SettingsLabelKind::Mute => Some("Mute"),
            SettingsLabelKind::Auto => Some("Auto"),
            SettingsLabelKind::Fullscreen => Some("Fullscreen"),
        }
    }
}

/// The filled portion of one control's track; its width is the control's
/// position on its own scale (percent, or ladder step).
#[derive(Component)]
pub(crate) struct SettingsBar {
    pub(crate) control: TrackControl,
}

/// A track node — the geometry a pointer drag maps against, so the fraction is
/// taken over the track rather than the whole row (label + track + value).
#[derive(Component)]
pub(crate) struct SettingsTrack {
    pub(crate) control: TrackControl,
}

pub(super) fn audio_control_label(control: AudioControl) -> &'static str {
    match control {
        AudioControl::Master => "Master",
        AudioControl::Music => "Music",
        AudioControl::Sfx => "SFX",
    }
}

/// `On` / `Off`, the shared rendering of every boolean Settings switch.
fn on_off(value: bool) -> &'static str {
    if value {
        "On"
    } else {
        "Off"
    }
}

/// The rendered value of one live Settings label, repainted in place by
/// [`sync_settings_labels`]. The row builders derive the same value when the
/// view opens; the switch states are the machine value alone, since the row's
/// sans label is a separate node (`docs/title-and-menu.md`
/// § Type system §2/§4).
pub(super) fn settings_label_text(kind: SettingsLabelKind, settings: &SettingsState) -> String {
    match kind {
        SettingsLabelKind::Percent(control) => format!("{}%", settings.percent(control)),
        SettingsLabelKind::TextSpeed => settings.text_speed_name().to_string(),
        SettingsLabelKind::Mute => on_off(settings.muted()).to_string(),
        SettingsLabelKind::Auto => on_off(settings.auto()).to_string(),
        SettingsLabelKind::Fullscreen => on_off(settings.fullscreen()).to_string(),
    }
}

/// A full-width Settings row: a sans label node and an `On`/`Off` mono value
/// node. The value is a [`SettingsLabel`] repainted in place, so a toggle never
/// rebuilds the tree.
pub(super) fn switch_row(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    action: MenuAction,
    kind: SettingsLabelKind,
    on: bool,
) {
    parent
        .spawn((
            action,
            Button,
            Node {
                width: Val::Px(440.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(BUTTON_BG),
            BorderColor::all(focus_border(menu, action)),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(kind.switch_label().unwrap_or("")),
                faces.sans(16.0),
                TextColor::from(TEXT),
            ));
            row.spawn((
                SettingsLabel { kind },
                Text::new(on_off(on)),
                faces.mono(16.0),
                TextColor::from(TEXT),
            ));
        });
}

/// A left-aligned Settings Reset button.
pub(super) fn reset_button(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    action: MenuAction,
    label: &str,
) {
    parent.spawn((
        action,
        Button,
        Node {
            align_self: AlignSelf::Start,
            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(BUTTON_BG),
        BorderColor::all(focus_border(menu, action)),
        Text::new(label),
        faces.sans(16.0),
        TextColor::from(TEXT),
    ));
}

/// A full-width Settings row for the stepped Text-speed control: the label, a
/// track whose fill is the ladder position, and the current step's name. The row
/// is focusable, `←`/`→` walk the ladder, and a click on the track snaps to the
/// nearest step through the shared drag path.
pub(super) fn text_speed_row(
    parent: &mut ChildSpawnerCommands,
    faces: &TextFaces,
    menu: &MenuState,
    settings: Option<&SettingsState>,
) {
    let action = MenuAction::TextSpeed;
    let name = settings.map(|s| s.text_speed_name()).unwrap_or("Instant");
    let percent = settings.map(|s| s.text_speed_percent()).unwrap_or(0);
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
                Text::new("Text speed"),
                faces.sans(16.0),
                TextColor::from(TEXT),
            ));
            row.spawn((
                SettingsTrack {
                    control: TrackControl::TextSpeed,
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
                        control: TrackControl::TextSpeed,
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
                    kind: SettingsLabelKind::TextSpeed,
                },
                Text::new(name.to_string()),
                faces.mono(16.0),
                TextColor::from(TEXT),
            ));
        });
}
