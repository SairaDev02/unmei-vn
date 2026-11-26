//! The Settings body both doors mount, drawn from the plugin's `settings`
//! surface (`docs/title-and-menu.md` § Title §4, § Title layout §5, spec §11).
//!
//! The tree decides the look: the heading, the three group labels, the audio
//! rows, every row and button, the key row and the door's back control. This
//! module decides what they say — every word is the engine's copy table, read
//! through the same keys the hand-built view used — and mounts the handful of
//! things a slot cannot carry: a slider's fill, the values that change while the
//! view is open, the stepped row's track, the editable key field, the
//! placeholder's visibility and the Copy path label's flip.
//!
//! The back control is the door's own: the contents page moves its way out to the
//! page's left column (Default look §4), so the tree's box around it collapses
//! when [`Bind::shows`] says nothing is there, and the title's door still draws
//! its own. Both doors share the walk, the keys and the file flush, which stay
//! the menu's.

use bevy::prelude::*;

use super::super::render::tree::{Bind, ControlState, Rendered, Surface, Words};
use super::super::render::UiLook;
use super::super::style::TextFaces;
use super::controls::{
    audio_control_key, mount_audio_row, mount_switch, mount_text_speed, SettingsLabelKind,
};
use super::decisions::mount_decisions;
use crate::credentials::{self, CredentialsState, KeyRow};
use crate::menu::{MenuAction, MenuOrigin, MenuState};
use crate::settings::{AudioControl, SettingsState};

/// The surface's id, as the plugin folder names it.
const SURFACE: &str = "settings";

/// The audio group's list, whose items are the three sliders.
const AUDIO_ROWS: &str = "settings.audio-rows";

/// The three audio rows in walk order, each with the control its item is. The
/// item template places all three and the renderer shows the one the item's own
/// control names, so this table is both the row order and the action map.
const AUDIO_ROW_SLOTS: [(AudioControl, &str); 3] = [
    (AudioControl::Master, "settings.master"),
    (AudioControl::Music, "settings.music"),
    (AudioControl::Sfx, "settings.sfx"),
];

/// The body's spawned tree, kept on the body so the renderer's own visibility
/// pass can collapse the boxes a hidden slot leaves behind: the contents page's
/// Settings section hides the tree's back control, and the row around it must go
/// with it (Default look §4).
#[derive(Component, Clone, Debug)]
pub(crate) struct SettingsTree {
    pub(super) bound: Rendered,
}

/// The one Settings body both doors mount inside their scroll container. The
/// caller owns the container's chrome (the title's bordered panel, the overlay's
/// main column) and its group gap, exactly as it did over hand-built nodes.
pub(in crate::presentation) fn settings_body(
    parent: &mut ChildSpawnerCommands,
    look: Option<&UiLook>,
    faces: &TextFaces,
    menu: &MenuState,
    settings: Option<&SettingsState>,
    credentials: Option<&CredentialsState>,
    back_label: &str,
) {
    let Some(look) = look else {
        return;
    };
    let bind = Settings {
        menu,
        settings,
        credentials,
        back_label,
    };
    let surface = Surface {
        look,
        faces,
        bind: &bind,
    };
    let Some(rendered) = surface.spawn(SURFACE, parent) else {
        return;
    };
    if let Some(root) = rendered.root {
        parent
            .commands()
            .entity(root)
            .entry::<Node>()
            .and_modify(|mut node| {
                node.padding = UiRect::ZERO;
            });
    }
    let body = parent.target_entity();
    parent.commands().entity(body).insert(SettingsTree {
        bound: rendered.clone(),
    });
    mount_audio_rows(parent, &surface, &rendered, settings);
    mount_switch(
        parent,
        &rendered,
        "settings.mute",
        SettingsLabelKind::Mute,
        muted(settings),
        faces,
    );
    mount_switch(
        parent,
        &rendered,
        "settings.auto",
        SettingsLabelKind::Auto,
        auto(settings),
        faces,
    );
    mount_switch(
        parent,
        &rendered,
        "settings.fullscreen",
        SettingsLabelKind::Fullscreen,
        fullscreen(settings),
        faces,
    );
    mount_text_speed(parent, &rendered, settings, faces);
    mount_decisions(parent, &rendered, faces, credentials);
}

fn muted(settings: Option<&SettingsState>) -> bool {
    settings.is_some_and(|state| state.muted())
}

fn auto(settings: Option<&SettingsState>) -> bool {
    settings.is_some_and(|state| state.auto())
}

fn fullscreen(settings: Option<&SettingsState>) -> bool {
    settings.is_some_and(|state| state.fullscreen())
}

/// Spawn the audio group's three items, one copy of the tree's template each,
/// filled with that row's label, percentage and control.
fn mount_audio_rows(
    parent: &mut ChildSpawnerCommands,
    surface: &Surface<'_>,
    rendered: &Rendered,
    settings: Option<&SettingsState>,
) {
    let (Some(list), Some(item)) = (
        rendered.lists.get(AUDIO_ROWS).copied(),
        rendered.items.get(AUDIO_ROWS).cloned(),
    ) else {
        return;
    };
    let percent =
        |control: AudioControl| settings.map(|state| state.percent(control)).unwrap_or(100);
    parent.commands().entity(list).with_children(|rows| {
        for (control, slot) in AUDIO_ROW_SLOTS {
            let bind = AudioRow {
                control,
                slot,
                percent: percent(control),
            };
            let row = surface.spawn_item(&item, rows, &bind, Some(slot));
            mount_audio_row(rows, &row, &item, control, percent(control));
        }
    });
}

/// What the surface itself draws: the heading and every control's own words.
struct Settings<'a> {
    menu: &'a MenuState,
    settings: Option<&'a SettingsState>,
    credentials: Option<&'a CredentialsState>,
    back_label: &'a str,
}

impl Settings<'_> {
    /// The credential row the Decisions group reads, exactly as the hand-built
    /// group read it.
    fn row(&self) -> KeyRow {
        self.credentials
            .map(|state| state.row.clone())
            .unwrap_or_default()
    }
}

impl Bind for Settings<'_> {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "settings.heading" => Some(Words::new("settings.heading")),
            "settings.mute" => Some(Words::new("settings.control.mute")),
            "settings.reset-audio" => Some(Words::new("settings.control.reset-audio")),
            "settings.text-speed" => Some(Words::new("settings.control.text-speed")),
            "settings.auto" => Some(Words::new("settings.control.auto")),
            "settings.fullscreen" => Some(Words::new("settings.control.fullscreen")),
            "settings.reset-play" => Some(Words::new("settings.control.reset-play")),
            "settings.key-state" => Some(Words::text(self.row().row_text())),
            "settings.key-placeholder" => Some(Words::new("settings.key.placeholder")),
            "settings.key-path" => Some(Words::text(credentials::path_line(
                self.credentials.and_then(|state| state.path.as_deref()),
            ))),
            "settings.copy-path-label" | "settings.copy-path" => {
                Some(Words::new("settings.control.copy-path"))
            }
            "settings.clear-key" => Some(Words::new("settings.control.clear-key")),
            "settings.back" => Some(Words::text(self.back_label.to_string())),
            _ => None,
        }
    }

    fn shows(&self, name: &str) -> bool {
        match name {
            "settings.back" => self.menu.settings_origin == MenuOrigin::Title,
            _ => true,
        }
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        match name {
            "settings.back" => Some(MenuAction::BackToSlots),
            "settings.mute" => Some(MenuAction::AudioMute),
            "settings.reset-audio" => Some(MenuAction::AudioReset),
            "settings.text-speed" => Some(MenuAction::TextSpeed),
            "settings.auto" => Some(MenuAction::PlayAuto),
            "settings.fullscreen" => Some(MenuAction::PlayFullscreen),
            "settings.reset-play" => Some(MenuAction::PlayReset),
            "settings.key" => Some(MenuAction::KeyRow),
            "settings.copy-path" => Some(MenuAction::CopyKeyPath),
            "settings.clear-key" => Some(MenuAction::ClearKey),
            _ => audio_action(name),
        }
    }

    fn state(&self, name: &str) -> ControlState {
        match name {
            "settings.mute" => ControlState {
                lit: muted(self.settings),
                ..default()
            },
            "settings.auto" => ControlState {
                lit: auto(self.settings),
                ..default()
            },
            "settings.fullscreen" => ControlState {
                lit: fullscreen(self.settings),
                ..default()
            },
            "settings.clear-key" => ControlState {
                disabled: self.row().is_from_environment(),
                ..default()
            },
            _ => ControlState::default(),
        }
    }
}

/// One audio row's own words: the label the plugin's copy table gives its
/// control, the percentage the slider stands at, and the label the row's control
/// stands for — the same words its label slot draws, so a reader asking the
/// control what it is finds them. A list item is its own binding, so the
/// template's three controls are answered here too.
struct AudioRow {
    control: AudioControl,
    slot: &'static str,
    percent: u32,
}

impl Bind for AudioRow {
    fn words(&self, name: &str) -> Option<Words> {
        match name {
            "settings.audio.label" => Some(Words::new(audio_control_key(self.control))),
            "settings.audio.value" => Some(Words::text(format!("{}%", self.percent))),
            own if own == self.slot => Some(Words::new(audio_control_key(self.control))),
            _ => None,
        }
    }

    fn action(&self, name: &str) -> Option<MenuAction> {
        audio_action(name)
    }
}

fn audio_action(name: &str) -> Option<MenuAction> {
    AUDIO_ROW_SLOTS
        .iter()
        .find(|(_, slot)| *slot == name)
        .map(|(control, _)| MenuAction::AudioSlider(*control))
}
