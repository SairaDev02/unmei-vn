//! The drawer's input ownership, focus handoff and cancellation.
//!
//! F3/Esc/control activation, the older-line reader's navigation, the suspended
//! focus restore and the window-focus-loss cancellation all run from here.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::ButtonInput;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::{ComputedNode, ComputedUiRenderTargetInfo, UiGlobalTransform};
use bevy::window::PrimaryWindow;

use crate::credentials::{CredentialsState, KeyField, KeyRow};
use crate::input::{InputOwnership, InputQuarantine};
use crate::menu::MenuState;
use crate::monitor::MonitorHistory;
use crate::presentation::settings_view::SettingsDrag;

use super::components::{
    MonitorControlTag, MonitorLogBody, MonitorScrollbarThumb, MonitorScrollbarTrack,
};
use super::geometry::{
    reader_columns, reader_metrics, PAGE_REPEAT_SECONDS, ROW_LINE_PX, VISIBLE_LINES, WHEEL_LINES,
};
use super::projection::{reader_frame, MonitorProjection};
use super::state::{
    MonitorControl, MonitorPageRepeat, MonitorPanel, MonitorScrollbarDrag, MONITOR_CLAIM,
};

/// The whole Monitor input resolution, scheduled in `PreUpdate` after the
/// frame's physical input and UI focus are settled, and before focused input
/// dispatch and every `Update` action.
///
/// It is the first production caller of [`InputOwnership`]: it claims app input
/// on the opening frame, keeps the claim through the whole closing frame and
/// releases it on the next frame. Closing therefore cannot act underneath,
/// while ordinary timed runtime work (the script, Decisions polling, skip,
/// reveal, auto, audio and credential checks) is never frozen.
#[allow(clippy::too_many_arguments)]
pub(crate) fn panel_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut panel: ResMut<MonitorPanel>,
    mut ownership: ResMut<InputOwnership>,
    mut settings_drag: Option<ResMut<SettingsDrag>>,
    mut input_focus: Option<ResMut<InputFocus>>,
    mut menu: Option<ResMut<MenuState>>,
    credentials: Option<Res<CredentialsState>>,
    quarantine: Option<Res<InputQuarantine>>,
    mut fields: Query<(Entity, &mut EditableText), With<KeyField>>,
    controls: Query<(&MonitorControlTag, &Interaction), Changed<Interaction>>,
) {
    if panel.open {
        // F3 has priority over everything: an open drawer closes once. Esc is
        // the ordinary dismissal. Either way the claim is deliberately **not**
        // released here — it must cover the rest of this closing frame.
        let f3 = keys.just_pressed(KeyCode::F3);
        let esc = keys.just_pressed(KeyCode::Escape);
        if f3 || esc {
            panel.close();
            return;
        }

        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        if keys.just_pressed(KeyCode::Tab) {
            panel.focus = if shift {
                panel.focus.previous()
            } else {
                panel.focus.next()
            };
        }

        // Pointer activation names its control directly and focuses it.
        let mut clicked = None;
        for (tag, interaction) in &controls {
            if *interaction == Interaction::Pressed {
                clicked = Some(tag.0);
            }
        }
        let activated = clicked.or_else(|| {
            let enter = keys.just_pressed(KeyCode::Enter);
            let space = keys.just_pressed(KeyCode::Space);
            // A fresh non-repeat press only: OS repeats never produce
            // `just_pressed`, and a held key cannot re-activate.
            (enter || space).then_some(panel.focus)
        });
        if let Some(control) = activated {
            panel.focus = control;
            match control {
                MonitorControl::ShowEverything => panel.toggle(),
                // The one deliberate, idempotent return to following latest.
                MonitorControl::FollowLatest => panel.reader.follow_latest(),
                MonitorControl::Close => panel.close(),
            }
        }
        return;
    }

    // Closed: this is the frame after a close. Release our own claim (never
    // another surface's), then restore the suspended key editing if the field
    // still exists and is editable. Held input defers the restore: the check
    // reads the **live** physical state, not the quarantine set, because
    // `track_quarantine`'s drain runs after this system, so the release frame
    // restores the draft before the ordinary focused-editing sync could
    // discard it. A key/button still held from the transition cannot type in.
    ownership.release_claim(MONITOR_CLAIM);
    let held = quarantine
        .as_deref()
        .is_some_and(|quarantine| quarantine.has_live_hold(&keys, &mouse));
    if !held {
        if let Some(focus) = input_focus.as_deref_mut() {
            restore_suspended_focus(
                focus,
                &mut panel,
                menu.as_deref_mut(),
                credentials.as_deref(),
                &mut fields,
            );
        }
    }

    // A fresh F3 opens. A held F3 (or an OS repeat) is not a fresh press, so it
    // cannot oscillate the drawer.
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }
    panel.open(input_focus.as_deref());
    // Opening cancels an active underlying Settings pointer drag at its last
    // applied value: the drag target clears, nothing is rolled back or flushed.
    if let Some(drag) = settings_drag.as_deref_mut() {
        drag.cancel();
    }
    // The claim is taken last and held for every following open frame.
    ownership.claim_if_free(MONITOR_CLAIM);
}

/// Reassert the drawer's claim on every open frame, including the closing one,
/// so the frame is covered even if something else touched the ownership
/// resource. A claim held by a different claimant is never stolen.
pub(crate) fn hold_monitor_claim(panel: Res<MonitorPanel>, mut ownership: ResMut<InputOwnership>) {
    if panel.open {
        ownership.claim_if_free(MONITOR_CLAIM);
    }
}

/// The older-line reader's `PreUpdate` input: wheel hit-tested to the log body,
/// PgUp/PgDn paging with a fresh press plus OS-repeat cadence, fresh Home/End,
/// and the draggable scrollbar.
///
/// It runs **after** [`panel_input`], so F3/Esc/Close, control activation and
/// the opening/closing transition frames win: a closing frame scrolls nothing
/// and an opening frame takes no navigation. A deliberate bottom arrival, `End`
/// or **Follow latest** is the only way back to following; every automatic
/// clamp is explicit elsewhere.
///
/// Simultaneous conflicting navigation is ignored rather than applied
/// unpredictably: an active scrollbar drag owns the frame, and two or more
/// distinct key/wheel intents in one frame are all dropped.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn reader_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut wheel: Option<MessageReader<MouseWheel>>,
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    bodies: Query<
        (
            &ComputedNode,
            &UiGlobalTransform,
            Option<&ComputedUiRenderTargetInfo>,
        ),
        With<MonitorLogBody>,
    >,
    tracks: Query<
        (
            &ComputedNode,
            &UiGlobalTransform,
            Option<&ComputedUiRenderTargetInfo>,
        ),
        With<MonitorScrollbarTrack>,
    >,
    interactions: Query<
        &Interaction,
        (
            Changed<Interaction>,
            Or<(With<MonitorScrollbarThumb>, With<MonitorScrollbarTrack>)>,
        ),
    >,
    projection: Res<MonitorProjection>,
    history: Option<Res<MonitorHistory>>,
    mut panel: ResMut<MonitorPanel>,
    mut drag: ResMut<MonitorScrollbarDrag>,
    mut repeat: ResMut<MonitorPageRepeat>,
    mut was_open: Local<bool>,
) {
    // The opening frame takes no navigation (the reader state it shows is the
    // previous one); the closing frame is already handled by `panel.open`.
    let process = panel.open && *was_open;
    *was_open = panel.open;
    if !process {
        drag.active = false;
        repeat.reset();
        // Drain the wheel so a closed drawer never leaves a stale delta.
        if let Some(reader) = wheel.as_mut() {
            for _ in reader.read() {}
        }
        return;
    }

    let delta = time.delta_secs();
    let mut wheel_lines = 0.0_f32;
    if let Some(reader) = wheel.as_mut() {
        for event in reader.read() {
            wheel_lines += match event.unit {
                MouseScrollUnit::Line => event.y * WHEEL_LINES,
                MouseScrollUnit::Pixel => event.y / ROW_LINE_PX,
            };
        }
    }

    let cursor = windows.iter().next().and_then(Window::cursor_position);
    // Wheel and drag are hit-tested to the log body only: outside-panel wheel is
    // swallowed (the drawer owns the frame), never passed to Settings, Gallery,
    // Timeline or play.
    let over_body = cursor.is_some_and(|cursor| {
        bodies
            .iter()
            .next()
            .is_some_and(|(node, transform, target)| {
                let point = cursor * target.map_or(1.0, ComputedUiRenderTargetInfo::scale_factor);
                node.contains_point(*transform, point)
            })
    });

    // A fresh press on the thumb or track starts the drag; releasing the button
    // ends it. A drag owns the frame, so a simultaneous wheel/key command is
    // ignored rather than fighting the pointer.
    if interactions
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        drag.active = true;
    }
    if !mouse.pressed(MouseButton::Left) {
        drag.active = false;
    }
    let metrics = reader_metrics(reader_columns(&windows));
    let frame = reader_frame(&projection, history.as_deref(), metrics);
    if drag.active {
        if let Some(fraction) = scrollbar_fraction(&tracks, cursor) {
            panel.reader.place_scrollbar(&frame, fraction);
        }
        return;
    }

    let page_up = tick_page_repeat(
        &mut repeat.page_up,
        keys.pressed(KeyCode::PageUp),
        keys.just_pressed(KeyCode::PageUp),
        delta,
    );
    let page_down = tick_page_repeat(
        &mut repeat.page_down,
        keys.pressed(KeyCode::PageDown),
        keys.just_pressed(KeyCode::PageDown),
        delta,
    );
    let home = keys.just_pressed(KeyCode::Home);
    let end = keys.just_pressed(KeyCode::End);
    let wheel_older = over_body && wheel_lines > 0.0;
    let wheel_newer = over_body && wheel_lines < 0.0;

    // One distinct intent per frame; conflicts are ignored whole.
    let intents = [page_up, page_down, home, end, wheel_older, wheel_newer]
        .iter()
        .filter(|intent| **intent)
        .count();
    if intents != 1 {
        return;
    }
    // A pending refresh does not freeze navigation: a deliberate command
    // starts from the same bottom view the placeholder shows (older/Home) or
    // from the settled anchor, and its explicit anchor supersedes the pending
    // resolution. `End` remains the one command that works with no rows at all.
    if end {
        panel.reader.follow_latest();
        return;
    }
    if frame.rows.is_empty() {
        return;
    }
    if page_up || wheel_older {
        let lines = if wheel_older {
            wheel_lines.abs()
        } else {
            VISIBLE_LINES
        };
        panel.reader.move_older(&frame, lines);
    } else if page_down || wheel_newer {
        let lines = if wheel_newer {
            wheel_lines.abs()
        } else {
            VISIBLE_LINES
        };
        panel.reader.move_newer(&frame, lines);
    } else if home {
        panel.reader.home(&frame);
    }
}

/// A fresh paging press acts immediately; a held key repeats once per
/// [`PAGE_REPEAT_SECONDS`]. A released key disarms its countdown.
fn tick_page_repeat(timer: &mut f32, pressed: bool, fresh: bool, delta: f32) -> bool {
    if fresh {
        *timer = PAGE_REPEAT_SECONDS;
        return true;
    }
    if !pressed {
        *timer = 0.0;
        return false;
    }
    *timer -= delta;
    if *timer <= 0.0 {
        *timer += PAGE_REPEAT_SECONDS;
        return true;
    }
    false
}

/// The cursor's vertical fraction across the scrollbar track, mirrored from the
/// Settings drag's own point mapping (physical node space from the logical
/// cursor).
fn scrollbar_fraction(
    tracks: &Query<
        (
            &ComputedNode,
            &UiGlobalTransform,
            Option<&ComputedUiRenderTargetInfo>,
        ),
        With<MonitorScrollbarTrack>,
    >,
    cursor: Option<Vec2>,
) -> Option<f32> {
    let cursor = cursor?;
    let (node, transform, target) = tracks.iter().next()?;
    let point = cursor * target.map_or(1.0, ComputedUiRenderTargetInfo::scale_factor);
    node.normalize_point(*transform, point)
        .map(|normalized| (normalized.y + 0.5).clamp(0.0, 1.0))
}

/// Restore the focus the drawer suspended, but only to a key field that still
/// exists and is editable in the current row. An invalidated, removed,
/// read-only, `Checking…` or environment field has its stale secret draft
/// cleared and the current safe focus left in place: the ordinary
/// focus-discard path (frozen while the drawer held input) then applies as it
/// would for any other focus change, and no departed view is recreated.
fn restore_suspended_focus(
    input_focus: &mut InputFocus,
    panel: &mut MonitorPanel,
    menu: Option<&mut MenuState>,
    credentials: Option<&CredentialsState>,
    fields: &mut Query<(Entity, &mut EditableText), With<KeyField>>,
) {
    let Some(suspended) = panel.suspended_focus else {
        return;
    };
    panel.suspended_focus = None;
    let key_editing = menu.as_deref().is_some_and(|menu| menu.key_editing);
    if !key_editing {
        return;
    }
    let row = credentials
        .map(|state| state.row.clone())
        .unwrap_or_default();
    let editable = row != KeyRow::Checking && !row.is_from_environment();
    let mut alive = false;
    for (entity, mut field) in fields.iter_mut() {
        if entity != suspended {
            continue;
        }
        alive = true;
        if editable {
            continue;
        }
        // Clear the invalidated secret draft; the field's view must not keep a
        // value the current row no longer accepts.
        field.clear();
    }
    if !alive {
        return;
    }
    if editable {
        if input_focus.get() != Some(suspended) {
            input_focus.set(suspended, FocusCause::Navigated);
        }
    } else {
        if let Some(menu) = menu {
            menu.key_editing = false;
        }
        if input_focus.get() == Some(suspended) {
            input_focus.clear();
        }
    }
}

/// Window focus loss cancels the underlying Settings drag, the drawer's
/// scrollbar drag and the held-input quarantine. The platform may never deliver
/// the releases of keys/buttons it already considers gone, so the quarantine
/// must not be able to lock ordinary input forever. Visibility, filter and
/// reader choices are preserved, and no new game pause is added.
pub(crate) fn cancel_on_window_focus_loss(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut was_focused: Local<Option<bool>>,
    mut quarantine: Option<ResMut<InputQuarantine>>,
    mut settings_drag: Option<ResMut<SettingsDrag>>,
    mut scroll_drag: Option<ResMut<MonitorScrollbarDrag>>,
) {
    let Some(window) = windows.iter().next() else {
        return;
    };
    let focused = window.focused;
    let previous = was_focused.replace(focused);
    if focused || previous == Some(false) {
        return;
    }
    if let Some(quarantine) = quarantine.as_deref_mut() {
        quarantine.clear();
    }
    if let Some(drag) = settings_drag.as_deref_mut() {
        drag.cancel();
    }
    if let Some(drag) = scroll_drag.as_deref_mut() {
        drag.active = false;
    }
}
