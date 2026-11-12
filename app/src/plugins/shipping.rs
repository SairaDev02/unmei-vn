//! [`UnmeiPlugin`]: the shipping playable app — the core frame plus presentation
//! and audio — and the ownership-gated presentation registration it shares with
//! the input-ownership harness.

use bevy::prelude::*;
use bevy_kira_audio::AudioApp;

use crate::{
    input, presentation, MonitorDrawerSet, NoticeSet, PresentationSet, RuntimeCorePlugin,
    RuntimeCoreSet, UnmeiPlugin,
};

impl Plugin for UnmeiPlugin {
    fn build(&self, app: &mut App) {
        // The interface faces are embedded in the binary, not read from the
        // pack's asset root. `TextPlugin` (in `DefaultPlugins`) has already
        // added `Assets<Font>` by the time `UnmeiPlugin` is built
        // (`docs/title-and-menu.md` § Type system §1).
        presentation::style::embed_text_faces(app);
        app.add_plugins(RuntimeCorePlugin)
            .add_audio_channel::<presentation::play::Bgm>()
            .add_audio_channel::<presentation::play::Sfx>()
            .init_resource::<presentation::assets::AssetWatch>()
            // The one `Update` phase order every runtime system is placed in:
            // the core frame, then notice ticking, then the presentation syncs,
            // then the Monitor drawer. `PresentationSet` is deliberately
            // ambiguous with itself: its systems update disjoint widgets from
            // the frame's settled state, so their mutual order is immaterial and
            // the edges that do matter are declared per group. Accepting that
            // intra-phase ambiguity keeps the gate focused on the cross-phase
            // schedule the app relies on.
            .configure_sets(
                Update,
                (RuntimeCoreSet, NoticeSet, PresentationSet, MonitorDrawerSet).chain(),
            )
            .configure_sets(Update, PresentationSet.ambiguous_with(PresentationSet))
            // Third-party `PostUpdate` ambiguities the app cannot order, named
            // explicitly so `ambiguity_detection: Error` still fails on any
            // app-owned pair:
            // - `bevy_kira_audio`'s three per-channel `PlayTypedChannels`
            //   systems and `play_dynamic_channels` all mutate the same
            //   `AudioOutput`; the crate leaves them unordered.
            // - `UiSystems::PostLayout` holds `bevy_ui`'s own unchained
            //   `button_changed`/`image_changed`/`label_changed` a11y trio; they
            //   mutate `AccessibilityNode` and are not app-schedulable. The app
            //   owns no system in that set (the gallery group runs before it).
            .configure_sets(
                PostUpdate,
                bevy_kira_audio::AudioSystemSet::PlayTypedChannels
                    .ambiguous_with(bevy_kira_audio::AudioSystemSet::PlayTypedChannels)
                    .ambiguous_with(bevy_kira_audio::AudioSystemSet::PlayDynamicChannels),
            )
            .configure_sets(
                PostUpdate,
                bevy::ui::UiSystems::PostLayout.ambiguous_with(bevy::ui::UiSystems::PostLayout),
            );
        // Each presentation owner registers its own group — its systems with
        // their intrinsic run conditions and ordering edges; the coordinator
        // composes them in today's order.
        presentation::register(app);
        register_ownership_gated_presentation(app);
    }
}

/// Register the app-input-ownership gates for the presentation side.
///
/// Shared by [`UnmeiPlugin`] and the input-ownership test harness, so the
/// production gate and the tests that prove it cannot drift: the harness calls
/// this rather than re-declaring the `run_if`/ordering itself. `settings_drag`
/// is the whole Settings pointer drag (start/update/release); `trim_pasted_text`
/// is the app-owned clipboard read and
/// [`crate::presentation::settings_view::discard_queued_key_edits_when_blocked`] consumes
/// whatever the
/// widget observers queued while a claimant held the frame, so nothing applies
/// late on release.
pub(crate) fn register_ownership_gated_presentation(app: &mut App) {
    // While a claimant owns the frame, or a held key/button is still
    // quarantined, the text apply set never mutates an `EditableText` at all,
    // whatever queued an edit. The drain below is the belt to this braces: it
    // empties what the producers queued so nothing survives to apply on
    // release.
    app.configure_sets(
        PostUpdate,
        bevy::text::EditableTextSystems.run_if(input::app_input_ready),
    )
    .add_systems(
        Update,
        presentation::settings_view::settings_drag
            .run_if(input::app_input_ready)
            .in_set(PresentationSet),
    )
    .add_systems(
        PostUpdate,
        (
            presentation::settings_view::trim_pasted_text.run_if(input::app_input_ready),
            presentation::settings_view::discard_queued_key_edits_when_blocked,
        )
            .chain()
            // The apply set is part of `UiSystems::Content` (bevy_ui puts
            // `EditableTextSystems` in Content so edits land before layout), so
            // the drain runs in Content too, immediately before it. A
            // focus-change edit queued by the later `FocusChangeEvents` system
            // is consumed here on the next frame before it can apply, and no
            // ordering edge to that later system can cycle the schedule
            // (`docs/runtime-hardening.md` § Monitor → Interaction).
            //
            // `bevy_ui`'s editable-text sizing systems are also
            // `.before(EditableTextSystems)` and touch `EditableText`, so the
            // app's drain is ordered ahead of them explicitly: the app's
            // systems stay ambiguity-free even where Bevy leaves its own pair
            // unordered.
            .in_set(bevy::ui::UiSystems::Content)
            .before(bevy::ui::widget::update_editable_text_content_size)
            .before(bevy::text::EditableTextSystems),
    );
}
