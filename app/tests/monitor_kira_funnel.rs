//! The kira observer → failure funnel join through the production subscriber.
//!
//! `bevy_kira_audio` logs its own backend setup failure under
//! [`KIRA_SETUP_TARGET`]; the app's `app_log_layer` observes that one upstream
//! warning and hands it to [`report_audio_setup_failure`], which puts a distinct
//! `unmei::failure` row-7 line through the same funnel as every other failure.
//!
//! This drives the *upstream source* warning once (not both the source and the
//! funnel by hand) through the real `LogPlugin` + `app_log_layer` composition,
//! then asserts the source and funnel events both survive with unchanged
//! multiplicity and no Notice. One test only: `LogPlugin` installs a
//! process-global subscriber/logger.

use bevy::app::App;
use bevy::input::InputPlugin;
use bevy::log::tracing;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use unmei_app::failure::{
    app_log_layer, AudioSetupFailure, Notice, KIRA_SETUP_MESSAGE, KIRA_SETUP_TARGET,
};
use unmei_app::monitor::MonitorHistory;
use unmei_app::{RuntimeConfig, RuntimeCorePlugin};

/// A minimal healthy pack, so startup adds no unrelated failure rows.
fn healthy_pack(dir: &std::path::Path) -> std::path::PathBuf {
    let pack_root = dir.join("pack");
    std::fs::create_dir_all(pack_root.join("scenes")).expect("scenes dir");
    std::fs::create_dir_all(pack_root.join("npcs")).expect("npcs dir");
    std::fs::write(
        pack_root.join("pack.ron"),
        "(name: \"kira\", entry: \"entry\")",
    )
    .expect("manifest");
    std::fs::write(pack_root.join("state.ron"), "{}").expect("state");
    std::fs::write(
        pack_root.join("scenes/entry.unmei"),
        "Narrator: the kira fixture plays.\n",
    )
    .expect("scene");
    pack_root
}

#[test]
fn the_kira_upstream_warning_reaches_the_funnel_once_through_the_real_layer() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pack_root = healthy_pack(dir.path());

    let mut app = App::new();
    app.insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(MinimalPlugins)
        .add_plugins(InputPlugin)
        .add_plugins(LogPlugin {
            custom_layer: app_log_layer,
            ..Default::default()
        })
        .add_plugins(RuntimeCorePlugin);

    // The one upstream source event: exactly what `bevy_kira_audio`'s
    // `AudioOutput` emits when the backend cannot start.
    tracing::warn!(
        target: KIRA_SETUP_TARGET,
        "Failed to setup audio: {:?}",
        "NoDevice"
    );

    // Startup reads the observer's slot through `report_audio_setup_failure`;
    // Update ingests both admitted events into the shared history.
    app.update();

    // The observer saw the upstream warning.
    assert_eq!(
        app.world()
            .resource::<AudioSetupFailure>()
            .message()
            .as_deref(),
        Some("Failed to setup audio: \"NoDevice\""),
        "the real kira layer captured the upstream source warning"
    );

    let history = app
        .world()
        .get_resource::<MonitorHistory>()
        .expect("dev mode on installs the capture history");

    let sources: Vec<_> = history
        .lines()
        .iter()
        .filter(|line| line.target == KIRA_SETUP_TARGET)
        .collect();
    assert_eq!(
        sources.len(),
        1,
        "the upstream source warning is retained exactly once: {}",
        history.text()
    );
    assert_eq!(sources[0].level, log::Level::Warn);
    assert!(
        sources[0].text.contains(KIRA_SETUP_MESSAGE),
        "the source line carries the upstream text: {}",
        sources[0].text
    );

    let funnel: Vec<_> = history
        .lines()
        .iter()
        .filter(|line| line.target == "unmei::failure")
        .collect();
    assert_eq!(
        funnel.len(),
        1,
        "the funnel emits the row-7 line exactly once: {}",
        history.text()
    );
    assert_eq!(funnel[0].level, log::Level::Warn);
    assert!(
        funnel[0]
            .body()
            .trim_start()
            .starts_with("couldn't start audio"),
        "the funnel row carries the row-7 short text: {}",
        funnel[0].body()
    );

    // Two distinct events: the source is admitted before its funnel mirror.
    assert_ne!(sources[0].target, funnel[0].target);
    assert!(
        sources[0].sequence < funnel[0].sequence,
        "the source precedes its funnel mirror"
    );

    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 7 must not show a Notice"
    );
}
