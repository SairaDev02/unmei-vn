//! [`RuntimeCorePlugin`]: pack load, script stepping, player input, the async
//! Decisions call, slot save/load and the autosave write points.

use bevy::prelude::*;

use crate::{
    core, credentials, dev_mode_enabled, dev_mode_is_on, drawer, failure, input, menu, monitor,
    presentation, read_log, settings, slots, DefensiveHold, HeldDecision, PendingDecision, Runtime,
    RuntimeConfig, RuntimeCorePlugin,
};

impl Plugin for RuntimeCorePlugin {
    fn build(&self, app: &mut App) {
        install_monitor_capture(app);
        install_monitor_drawer(app);
        init_core_state(app);
        init_input_ownership(app);
        register_startup(app);
        register_frame_schedules(app);
    }
}

/// Install hidden Monitor history and its ingestion system, only when the
/// entry point's capture layer is already present.
fn install_monitor_capture(app: &mut App) {
    // Hidden Monitor capture: the layer was installed by the entry point's
    // `LogPlugin::custom_layer` hook before this plugin was added. Its
    // presence is the gate; when it is absent (dev mode off, or a headless
    // harness with no global logger) neither the history resource nor the
    // ingestion system is installed at all. Registration is therefore tied
    // to capture being present, never to a loaded `Runtime`, the
    // title/menu, a defensive hold or the panel's visibility.
    if app.world().contains_resource::<monitor::MonitorCapture>() {
        app.init_resource::<monitor::MonitorHistory>()
            .add_systems(Update, monitor::ingest_capture);
    }
}

/// Install the Monitor drawer's state, its input claim and its projection,
/// only under the one app-lifetime dev-mode value.
fn install_monitor_drawer(app: &mut App) {
    // The Monitor drawer exists only under the one app-lifetime
    // dev-mode value resolved before the plugins were installed. Off really
    // installs nothing: no panel state, no projection, no quarantine and no
    // F3 system, so F3 can do no Monitor work at all. F5/F9 stay behind
    // their own gate below.
    if dev_mode_is_on(app.world().get_resource::<RuntimeConfig>()) {
        app.init_resource::<drawer::MonitorPanel>()
            .init_resource::<drawer::MonitorProjection>()
            .init_resource::<drawer::MonitorWork>()
            .init_resource::<drawer::MonitorScrollbarDrag>()
            .init_resource::<drawer::MonitorPageRepeat>()
            .init_resource::<input::InputQuarantine>()
            // The resolution point sits after the frame's physical input and
            // UI focus have settled, and before focused input dispatch and
            // every `Update` action, so one claim covers a whole opening or
            // closing frame. The quarantine tracker runs after the toggle,
            // so the frame's claim is already visible when it records held
            // keys/buttons.
            .configure_sets(
                PreUpdate,
                drawer::MonitorInputSet
                    .after(bevy::input::InputSystems)
                    .after(bevy::ui::UiSystems::Focus)
                    .before(bevy::input_focus::InputFocusSystems::Dispatch),
            )
            .add_systems(
                PreUpdate,
                (
                    drawer::panel_input,
                    // The reader runs after the toggle/close decision, so
                    // Esc, Close, F3 and control activation win the frame,
                    // and before the claim/quarantine bookkeeping.
                    drawer::reader_input,
                    drawer::hold_monitor_claim,
                    input::track_quarantine,
                    drawer::cancel_on_window_focus_loss,
                )
                    .chain()
                    .in_set(drawer::MonitorInputSet),
            )
            // Projection and drawer text are `Update` work ordered after
            // the core so a row materialized this frame sees this frame's
            // ingested history. They run even without a loaded `Runtime`
            // (the drawer reaches the title, menu and startup error too).
            .add_systems(
                Update,
                (
                    drawer::project_monitor,
                    // The anchor is resolved against the pass's outcome
                    // before any row is materialized from it.
                    drawer::resolve_reader,
                    drawer::sync_panel_ui,
                    drawer::sync_panel_rows,
                    drawer::sync_panel_row_markers,
                    drawer::sync_panel_controls,
                    drawer::sync_panel_status,
                )
                    .chain()
                    .in_set(crate::MonitorDrawerSet),
            );
    }
}

/// Initialise the core's own resources, then the presentation owners'
/// separately stored realization state.
fn init_core_state(app: &mut App) {
    app.init_resource::<PendingDecision>()
        .init_resource::<HeldDecision>()
        .init_resource::<DefensiveHold>()
        .init_resource::<core::AutoDwell>()
        .init_resource::<core::SkipState>()
        .init_resource::<core::KeyCheckChannel>()
        .init_resource::<core::DecisionFactory>()
        .init_resource::<core::PendingKeyCheck>()
        .init_resource::<menu::MenuState>()
        .init_resource::<credentials::CredentialsState>()
        .init_resource::<credentials::ClipboardPort>()
        .init_resource::<credentials::CopyPathLabel>()
        .init_resource::<failure::Notice>()
        .init_resource::<read_log::ReadLogState>()
        .init_resource::<slots::PendingAutosave>();
    // Each presentation owner initializes its own realization state, so a
    // headless core app keeps exactly the state its input and session
    // tests exercise.
    presentation::play::init(app);
    presentation::settings_view::init(app);
}

/// Initialise the single app-input ownership boundary and order its
/// resolution point.
fn init_input_ownership(app: &mut App) {
    app
        // The single app-input ownership boundary. Shipping leaves it
        // unclaimed (the ordinary app owns input); it is initialised here so
        // every run condition can read it, and the resolution point is
        // ordered ahead of physical input and focused dispatch.
        .init_resource::<input::InputOwnership>()
        .configure_sets(
            PreUpdate,
            input::InputOwnershipSet
                .before(bevy::input::InputSystems)
                .before(bevy::input_focus::InputFocusSystems::Dispatch),
        );
}

/// Register the one deterministic `Startup` order.
fn register_startup(app: &mut App) {
    app.add_systems(
        Startup,
        (
            // The entry point can record a startup failure it already knows —
            // no pack path exists because the executable's own location is
            // unknown. There is then nothing to load, and the screen draws that
            // one reason instead of a second, overwritten one.
            core::setup_pack.run_if(not(resource_exists::<failure::StartupFailure>)),
            settings::setup_settings,
            credentials::setup_credentials,
            failure::report_log_open_error,
            failure::report_audio_setup_failure,
        )
            // One deterministic startup order that fixes how multiple
            // simultaneous startup failures reach the one `Notice` (the
            // five systems all write it, so leaving the tuple unchained
            // made `Notice`/`DefensiveHold` access genuinely ambiguous).
            .chain(),
    );
}

/// Register the `Last` flushes and the frame's `Update` chains.
fn register_frame_schedules(app: &mut App) {
    app.add_systems(
        Last,
        (settings::flush_on_exit, read_log::flush_on_exit)
            // The app-exit message is written by Bevy's `ExitSystems` in
            // this same `Last` schedule, so both flushes read it after;
            // chaining them keeps the one `Notice`/`DefensiveHold` they
            // both write deterministic.
            .chain()
            .after(bevy::window::ExitSystems),
    )
    .add_systems(
        Update,
        (
            // The play path only exists once a pack loaded; a broken pack
            // leaves `Runtime` absent and the error screen owns the frame.
            (
                // In-play input first, frozen while the overlay is open
                // and while a defensive hold blocks the runner. Saving
                // and loading stay live so a load can clear the hold.
                // The overlay then reads the same frame's keys, so a
                // key that accepts a confirmation cannot also bleed into
                // the play path on the frame the overlay closes.
                (
                    // F5/F9 are a silent no-op before any shortcut work
                    // while dev mode is off: the system never runs, so
                    // it cannot inspect/read/write a slot, cancel a
                    // request, discard a held answer or touch the
                    // skip/auto/read-log state.
                    input::save_load_input.run_if(dev_mode_enabled), // `Esc`/`Tab` first: a cancelling `Esc` ends the
                    // toggle before `menu_input` opens the overlay. Not
                    // gated by `not_held`, so `Esc` still clears under a
                    // defensive hold; `skip_input` honours the hold for
                    // `Tab` itself.
                    input::skip_input,
                    (input::advance_say_input, input::choice_input)
                        .chain()
                        .run_if(core::not_held),
                    // `F11` toggles the fullscreen field and `A` toggles
                    // auto; the window and the `AUTO` cue are synced
                    // from those fields by the presentation. Both are
                    // inert at a choice-wait, and skipped under a
                    // defensive hold. They share `SettingsState`, so the
                    // pair is chained rather than left ambiguous.
                    (input::fullscreen_input, input::auto_input)
                        .chain()
                        .run_if(core::not_held),
                )
                    .chain()
                    .run_if(menu::menu_closed)
                    .run_if(input::app_input_ready),
                // The force hold's release mirror is deliberately **not**
                // gated by `menu_closed`: a `Ctrl` released under the
                // overlay or the title must clear the latch, so a press
                // made behind a gate can never start a skip when play
                // resumes (`docs/runtime-hardening.md` § Text skip §5).
                core::force_release_mirror,
                // A request in flight keeps running under the overlay.
                core::poll_decision,
                // A committed key's ping keeps running under the overlay
                // too; its verdict lands on the row and the resolver.
                core::poll_key_check,
                // The key field's focus follows the widget's
                // `InputFocus` before the overlay's own input, so a
                // click elsewhere is applied after the field is left.
                // Frozen while a claimant owns input or a held key is
                // quarantined, so a suspended draft is not discarded
                // just because focus moved. `ambiguous_with_all` is
                // deliberate: `bevy_ui_widgets`' private `MenuPlugin`
                // systems also write `InputFocus` from plain `Update`
                // and cannot be named for an explicit order. They only
                // match `MenuPopup` entities, which this overlay never
                // spawns, so the conflict is vacuous.
                menu::sync_key_focus
                    .run_if(input::app_input_ready)
                    .ambiguous_with_all(),
                // The overlay: Esc opens/closes it. Same vacuous
                // `bevy_ui_widgets` `InputFocus` conflict as above.
                menu::menu_input
                    .run_if(input::app_input_ready)
                    .ambiguous_with_all(),
                // After `menu_input` has opened the view, pick up a
                // hand-edit before the presentation builds the tree.
                menu::read_on_settings_open,
                // The seen-skip step runs before `drive_script`, so the
                // advance it makes is stepped to the next park by the
                // ordinary pipeline in the same frame; a cancelling
                // press has already ended the toggle by now. `auto_dwell`
                // then fires on a fully revealed say-line only, and is
                // suppressed while skip runs. All three share the
                // overlay/hold freeze.
                (core::skip_advance, core::drive_script, core::auto_dwell)
                    .chain()
                    .run_if(menu::menu_closed)
                    .run_if(core::not_held),
                // After the parks have settled: apply the arrival/mark
                // rule, then honour a flush moment from this frame.
                read_log::observe_parks,
                read_log::write_pending_read_log,
                // Last in the chain: the runner has parked and staging
                // matches the screen before the autosave captures.
                slots::write_pending_autosave,
            )
                .chain()
                .in_set(crate::RuntimeCoreSet)
                .run_if(resource_exists::<Runtime>),
            // The Notice freezes with the reveal while the overlay is open.
            // Both are app-owned, but they run on the startup error screen
            // too, where `Runtime` is absent, so they sit in `NoticeSet`
            // rather than the runtime-gated core chain.
            (
                failure::tick_notice.run_if(menu::menu_closed),
                failure::error_screen_input
                    .run_if(resource_exists::<failure::StartupFailure>)
                    .run_if(input::app_input_ready),
            )
                .in_set(crate::NoticeSet),
        ),
    );
}
