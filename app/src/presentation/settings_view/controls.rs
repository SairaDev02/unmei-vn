//! The Settings view's controls: the live labels and bars its syncs repaint, and
//! what the app mounts into the plugin's tree because the format cannot carry it.
//!
//! A tree names no node but a slot, a control and a list, so the pieces a row
//! holds without a name — the track a slider is dragged against, the fill that
//! shows its value, and the words that change while the view stays open — are
//! spawned here into the nodes the renderer reports. A slot draws one string,
//! resolved once at spawn, so a value that moves is a node of the app's, marked
//! and repainted in place by [`sync_settings_labels`](super::sync_settings_labels)
//! rather than rebuilt (`docs/title-and-menu.md` § Type system §2/§4).

use bevy::prelude::*;
use unmei_engine::copy;
use unmei_engine::ui::{Node as PluginNode, NodeKind};

use super::super::render::tree::{slot_holder, Rendered, SlotHolder};
use super::super::style::{TextFaces, FOCUS, PANEL_BG, TEXT};
use crate::settings::{AudioControl, SettingsState, TrackControl};

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

/// The face every value the app draws itself uses: the format's `typography.value`
/// role at its own size, because a tree cannot show a string that changes while
/// the view is open.
const SETTINGS_VALUE_SIZE: f32 = 16.0;

/// The gap between a switch row's label and its value, as the hand-built row
/// spaced them before the tree drew the row.
const SETTINGS_SWITCH_GAP: f32 = 8.0;

/// The stepped text-speed control's track. The audio rows' tracks are the tree's
/// empty row boxes; the stepped row has no box of its own in the format, so the
/// engine draws the same 220×10 the plugin's `track` component gives a slider.
const SETTINGS_TRACK_WIDTH: f32 = 220.0;
const SETTINGS_TRACK_HEIGHT: f32 = 10.0;

/// The copy key a slider row's label draws.
pub(super) fn audio_control_key(control: AudioControl) -> &'static str {
    match control {
        AudioControl::Master => "settings.control.master",
        AudioControl::Music => "settings.control.music",
        AudioControl::Sfx => "settings.control.sfx",
    }
}

/// `On` / `Off`, the shared rendering of every boolean Settings switch.
pub(super) fn on_off(value: bool) -> &'static str {
    if value {
        copy::text("settings.value.on")
    } else {
        copy::text("settings.value.off")
    }
}

/// The rendered value of one live Settings label, repainted in place by
/// [`sync_settings_labels`](super::sync_settings_labels). The spawned words
/// derive the same value; the switch states are the machine value alone, since
/// the row's sans label is a separate node (`docs/title-and-menu.md`
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

/// Mount one audio row inside the tree's item: the track the pointer drags
/// against, its filled portion and the live percentage label. The row's own
/// slot, label, value and control are the tree's; only what a slot cannot carry
/// is the app's.
pub(super) fn mount_audio_row(
    parent: &mut ChildSpawnerCommands,
    spawned: &Rendered,
    item: &PluginNode,
    control: AudioControl,
    percent: u32,
) {
    let tracked = TrackControl::Audio(control);
    if let Some(track) = track_entity(spawned, item) {
        parent
            .commands()
            .entity(track)
            .insert(SettingsTrack { control: tracked })
            .with_children(|node| {
                node.spawn((
                    SettingsBar { control: tracked },
                    Node {
                        width: Val::Percent(percent as f32),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(FOCUS),
                ));
            });
    }
    if let Some(value) = spawned.slots.get("settings.audio.value").copied() {
        parent.commands().entity(value).insert(SettingsLabel {
            kind: SettingsLabelKind::Percent(control),
        });
    }
}

/// Mount a switch row's live value beside the tree's own label: a slot draws one
/// string, so the value the toggle turns is the app's node.
pub(super) fn mount_switch(
    parent: &mut ChildSpawnerCommands,
    spawned: &Rendered,
    slot: &str,
    kind: SettingsLabelKind,
    value: bool,
    faces: &TextFaces,
) {
    let Some(control) = spawned.controls.get(slot).copied() else {
        return;
    };
    let words = on_off(value).to_string();
    parent.commands().entity(control).with_children(|node| {
        node.spawn((
            SettingsLabel { kind },
            Text::new(words),
            faces.mono(SETTINGS_VALUE_SIZE),
            TextColor::from(TEXT),
            Node {
                margin: UiRect::left(Val::Px(SETTINGS_SWITCH_GAP)),
                ..default()
            },
        ));
    });
}

/// Mount the stepped text-speed row's track, fill and current step beside the
/// tree's own label. The format has no property for either, so both are the
/// app's, sized as the plugin's own `track` component sizes a slider.
pub(super) fn mount_text_speed(
    parent: &mut ChildSpawnerCommands,
    spawned: &Rendered,
    settings: Option<&SettingsState>,
    faces: &TextFaces,
) {
    let Some(SlotHolder { boxes, .. }) = slot_holder(spawned, "settings.text-speed") else {
        return;
    };
    let [track, value] = boxes[..] else {
        return;
    };
    let percent = settings
        .map(|state| state.text_speed_percent())
        .unwrap_or(0);
    let name = settings
        .map(|state| state.text_speed_name().to_string())
        .unwrap_or_else(|| copy::text("settings.text-speed.instant").to_string());
    parent
        .commands()
        .entity(track)
        .insert(SettingsTrack {
            control: TrackControl::TextSpeed,
        })
        .insert(Node {
            width: Val::Px(SETTINGS_TRACK_WIDTH),
            height: Val::Px(SETTINGS_TRACK_HEIGHT),
            ..default()
        })
        .insert(BackgroundColor(PANEL_BG))
        .with_children(|node| {
            node.spawn((
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
    parent.commands().entity(value).with_children(|node| {
        node.spawn((
            SettingsLabel {
                kind: SettingsLabelKind::TextSpeed,
            },
            Text::new(name),
            faces.mono(SETTINGS_VALUE_SIZE),
            TextColor::from(TEXT),
        ));
    });
}

/// The track the engine fills inside one audio row: the item's one box with no
/// children. The format gives no node a name, so the row's empty box is the
/// place the plugin leaves for the control's value, and the renderer's spawn
/// order — a node, then its children — is how its entity is read back.
fn track_entity(spawned: &Rendered, item: &PluginNode) -> Option<Entity> {
    let mut seen = 0;
    let index = empty_box(item, &mut seen)?;
    spawned.nodes.get(index).copied()
}

fn empty_box(node: &PluginNode, seen: &mut usize) -> Option<usize> {
    let here = *seen;
    *seen += 1;
    match &node.kind {
        NodeKind::Box(container) => {
            if container.children.is_empty() {
                return Some(here);
            }
            container
                .children
                .iter()
                .find_map(|child| empty_box(child, seen))
        }
        NodeKind::Slot(slot) => slot
            .children
            .iter()
            .find_map(|child| empty_box(child, seen)),
        NodeKind::List(list) => empty_box(&list.item, seen),
        _ => None,
    }
}
