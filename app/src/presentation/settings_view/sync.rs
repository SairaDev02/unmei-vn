//! The Settings view's in-place syncs and its two scrolls.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::ui::{ComputedUiRenderTargetInfo, ScrollPosition, UiGlobalTransform};
use bevy::ui_widgets::ScrollIntoView;
use bevy::window::PrimaryWindow;

use super::controls::{settings_label_text, SettingsBar, SettingsLabel};
use super::SettingsBody;
use crate::input::InputOwnership;
use crate::menu::{action_for_focus, MenuAction, MenuState, MenuView};
use crate::settings::{self, SettingsState};

/// Repaint every live Settings label and slider fill from the stored values.
/// Runs every frame; writes only where the value differs.
pub(crate) fn sync_settings_labels(
    settings: Option<Res<SettingsState>>,
    mut labels: Query<(&SettingsLabel, &mut Text)>,
    mut bars: Query<(&SettingsBar, &mut Node)>,
) {
    let Some(settings) = settings else {
        return;
    };
    for (label, mut text) in &mut labels {
        let want = settings_label_text(label.kind, &settings);
        if **text != want {
            **text = want;
        }
    }
    for (bar, mut node) in &mut bars {
        let want = Val::Percent(bar.control.percent(&settings) as f32);
        if node.width != want {
            node.width = want;
        }
    }
}

/// Push the stored `play.fullscreen` onto the primary window. The window is
/// already built from the same field in `main` (so the first frame is never
/// windowed when the startup read says fullscreen); this follows an in-play
/// `F11` or Settings-switch toggle. A headless app has no window, so the query
/// is simply empty and this is a no-op.
pub(crate) fn sync_window_mode(
    settings: Option<Res<SettingsState>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Some(settings) = settings else {
        return;
    };
    let want = settings::window_mode(settings.fullscreen());
    for mut window in &mut windows {
        if window.mode != want {
            window.mode = want;
        }
    }
}

/// Scroll the focused Settings control fully into view when the walk moves. The
/// control's own entity is the freshness signal — a real focus move and a
/// structural rebuild both land on a different entity — so the rebuilt body
/// (which starts at the top) re-reveals the unchanged focus, while an idle frame
/// or a wheel scroll never yanks the view (`docs/title-and-menu.md` § Title
/// layout §5).
pub(crate) fn scroll_settings_focus(
    menu: Res<MenuState>,
    controls: Query<(Entity, &MenuAction)>,
    mut last: Local<Option<Entity>>,
    mut commands: Commands,
) {
    let focused = if menu.open && menu.view == MenuView::Settings {
        let action = action_for_focus(menu.focus);
        controls
            .iter()
            .find(|(_, candidate)| **candidate == action)
            .map(|(entity, _)| entity)
    } else {
        None
    };
    if focused == *last {
        return;
    }
    *last = focused;
    if let Some(entity) = focused {
        commands.trigger(ScrollIntoView { entity });
    }
}

/// Scroll the Settings viewport with the mouse wheel, hit-tested to the body
/// under the cursor. `bevy_ui` 0.19 has no built-in wheel scrolling and the app's
/// `bevy_picking` wheel bridge does not deliver a `Pointer<Scroll>` to
/// `ScrollArea`, so the panel reads the same `MouseWheel` stream the Monitor
/// drawer does; while a claimant owns app input (Monitor open, or a held control
/// still quarantined) the wheel is left to it, and an open confirmation freezes
/// the whole menu, this panel included
/// (`docs/title-and-menu.md` § Title layout §5, `docs/runtime-hardening.md`
/// § Menu overlay §6).
pub(crate) fn scroll_settings_wheel(
    menu: Res<MenuState>,
    mut wheel: MessageReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut bodies: Query<
        (
            &ComputedNode,
            &UiGlobalTransform,
            Option<&ComputedUiRenderTargetInfo>,
            &mut ScrollPosition,
        ),
        With<SettingsBody>,
    >,
    ownership: Option<Res<InputOwnership>>,
    quarantine: Option<Res<crate::input::InputQuarantine>>,
) {
    let mut delta = 0.0_f32;
    for event in wheel.read() {
        delta += match event.unit {
            MouseScrollUnit::Line => event.y * MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR,
            MouseScrollUnit::Pixel => event.y,
        };
    }
    if delta == 0.0
        || menu.confirm.is_some()
        || !(menu.open && menu.view == MenuView::Settings)
        || !crate::input::app_input_ready(ownership, quarantine)
    {
        return;
    }
    let Some(window) = windows.iter().next() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    for (node, transform, target, mut scroll) in &mut bodies {
        let point = cursor * target.map_or(1.0, ComputedUiRenderTargetInfo::scale_factor);
        if !node.contains_point(*transform, point) {
            continue;
        }
        let visible = node.size() * node.inverse_scale_factor();
        let content = node.content_size() * node.inverse_scale_factor();
        let max_range = (content - visible).max(Vec2::ZERO);
        scroll.y = (scroll.y - delta).clamp(0.0, max_range.y);
    }
}
