//! The Settings view: Audio, Play and Decisions groups, the window
//! mode, the key row/field and the pointer drag, for both doors.
//!
//! This root owns the view's one scroll container marker, the drag/ownership
//! registration and the re-exports that keep every `settings_view::<Item>` path
//! callers already use.

mod controls;
mod decisions;
mod drag;
mod groups;
mod sync;
mod title_door;

use bevy::prelude::*;
use bevy::ui::UiSystems;

use crate::PresentationSet;

// The controls' markers are addressed by full path from the app's tests, the
// only reader outside this view, so both marker re-exports are test-only.
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "the pre-split `settings_view::<Marker>` paths stay valid; `SettingsBar` and `SettingsTrack` have no reader outside the view"
)]
pub(crate) use controls::{SettingsBar, SettingsLabel, SettingsLabelKind, SettingsTrack};
pub(crate) use decisions::{
    discard_key_edit_on_outside_press, discard_queued_key_edits_when_blocked, sync_copy_path_label,
    sync_key_row, trim_pasted_text,
};
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "the pre-split `settings_view::<Marker>` paths stay valid; `CopyPathLabelText` has no reader outside the view"
)]
pub(crate) use decisions::{CopyPathLabelText, KeyPlaceholder, KeyRowText};
pub(crate) use drag::{settings_drag, SettingsDrag};
pub(super) use groups::settings_body;
pub(crate) use sync::{
    scroll_settings_focus, scroll_settings_wheel, sync_settings_labels, sync_window_mode,
};
pub(super) use title_door::build_title_settings;

/// The Settings view's scroll container: the title door's panel and the overlay
/// door's body. Exactly one is built at a time, and it is the one viewport the
/// wheel and the focus walk scroll
/// (`docs/title-and-menu.md` § Title layout §5).
#[derive(Component)]
pub(crate) struct SettingsBody;

/// Initialize the Settings view's drag state. The headless
/// [`RuntimeCorePlugin`](crate::RuntimeCorePlugin) keeps it so the ownership
/// tests can exercise the real state.
pub(crate) fn init(app: &mut App) {
    app.init_resource::<SettingsDrag>();
}

/// Register the Settings view's own systems: the in-place label/row/window
/// syncs in `Update`, the wheel/focus scrolls in the `PostUpdate` geometry group
/// (after layout, so the revealed rects are this frame's), and the outside-press
/// discard observer. The pointer drag and the queued-edit drain are the
/// ownership-gated group `lib.rs` composes.
pub(super) fn register(app: &mut App) {
    app.add_systems(
        Update,
        (
            sync_settings_labels,
            sync_window_mode,
            sync_key_row,
            sync_copy_path_label,
        )
            .in_set(PresentationSet),
    )
    .add_systems(
        PostUpdate,
        (scroll_settings_focus, scroll_settings_wheel)
            .chain()
            .after(UiSystems::Layout)
            // After the Timeline's focus scroll, which itself follows the
            // gallery's still fit and column sync: the settings scroll must be
            // ordered against those systems' `MenuState`/`ComputedNode` access.
            .after(super::timeline::scroll_timeline_focus)
            .before(UiSystems::PostLayout),
    )
    .add_observer(discard_key_edit_on_outside_press);
}
