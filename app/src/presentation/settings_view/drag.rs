//! The pointer drag over a Settings track: the fraction the cursor's x names,
//! snapped through the shared control math.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy::ui::{ComputedUiRenderTargetInfo, UiGlobalTransform, UiScale};

use super::controls::SettingsTrack;
use crate::menu::MenuAction;
use crate::settings::{self, SettingsState, TrackControl};

#[cfg(test)]
mod tests;

/// The control currently under a pointer drag, if any. Set on the press that
/// starts the drag and cleared on release, when the overlay opens and when the
/// window loses focus. The storage is private to this module
/// ([ADR 0013](../../../../docs/adr/0013-presentation-surface-partition.md)): the
/// drawer cancels through [`SettingsDrag::cancel`], never by clearing it.
#[derive(Resource)]
pub(crate) struct SettingsDrag(Option<TrackControl>);

impl FromWorld for SettingsDrag {
    fn from_world(_: &mut World) -> Self {
        SettingsDrag(None)
    }
}

impl SettingsDrag {
    /// Cancel the drag at its last applied value: the target clears, nothing is
    /// rolled back or flushed. The concrete operation the drawer's
    /// open/focus-loss paths call instead of clearing the flag itself.
    pub(crate) fn cancel(&mut self) {
        self.0 = None;
    }

    /// The control an in-progress drag targets, if any. A read-only fact for
    /// the app's drag/release/focus-loss tests; production reads the
    /// underlying flag only inside [`settings_drag`].
    #[cfg(test)]
    pub(crate) fn control(&self) -> Option<TrackControl> {
        self.0
    }
}

/// Pointer drag on a Settings track: the click that starts the drag is captured,
/// then every held frame maps the cursor's x to a fraction of the track and
/// snaps it — to the sliders' 5% grid, or to the text-speed ladder's steps. The
/// click that *opens* Settings cannot start a drag, because no track exists
/// until the view is built.
pub(crate) fn settings_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    ui_scale: Option<Res<UiScale>>,
    windows: Query<&Window>,
    mut drag: ResMut<SettingsDrag>,
    mut settings: ResMut<SettingsState>,
    interactions: Query<(&MenuAction, &Interaction), Changed<Interaction>>,
    tracks: Query<(
        &SettingsTrack,
        &ComputedNode,
        &ComputedUiRenderTargetInfo,
        &UiGlobalTransform,
    )>,
) {
    for (action, interaction) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            MenuAction::AudioSlider(control) => drag.0 = Some(TrackControl::Audio(*control)),
            MenuAction::TextSpeed => drag.0 = Some(TrackControl::TextSpeed),
            _ => {}
        }
    }

    if !mouse.pressed(MouseButton::Left) {
        drag.0 = None;
        return;
    }
    let Some(control) = drag.0 else {
        return;
    };
    let Some(window) = windows.iter().next() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let scale = ui_scale.map(|scale| scale.0).unwrap_or(1.0);

    for (track, node, target, transform) in &tracks {
        if track.control != control {
            continue;
        }
        let point = cursor * target.scale_factor() / scale;
        if let Some(normalized) = node.normalize_point(*transform, point) {
            if let Some(fraction) = settings::pointer_fraction(normalized.x) {
                control.set_fraction(&mut settings, fraction);
            }
        }
    }
}
