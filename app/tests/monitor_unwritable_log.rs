//! LogPlugin composition with an **unavailable** session log.
//!
//! When `<pack-dir>.log` cannot be opened, the entry point records the real
//! `SessionLog::open` failure as [`LogOpenError`]; the optional file layer is
//! absent but the mandatory kira observer and the dev-gated Monitor capture
//! must still be installed and independent. The row-21 log failure is reported
//! once as a `warn!` with no Notice, and play continues console-only (D4.07).
//!
//! Unlike the earlier injected-resource stand-in, this builds the failing path
//! the way the shipping entry point does: `<pack-dir>.log` is a *directory*, so
//! the real `SessionLog::open` returns an `Err` that is turned into the resource
//! the startup funnel reads. One test only: `LogPlugin` installs a
//! process-global subscriber/logger.

use bevy::app::App;
use bevy::input::InputPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use unmei_app::failure::{
    app_log_layer, session_log_path_for, AudioSetupFailure, LogOpenError, Notice, SessionLog,
};
use unmei_app::monitor::MonitorHistory;
use unmei_app::{Runtime, RuntimeConfig, RuntimeCorePlugin};

#[test]
fn a_real_failed_open_keeps_capture_and_kira_independent_without_a_notice() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pack_root = dir.path().join("pack");
    // A healthy, playable pack: the failure under test is the sink, not the
    // content, so "play continues console-only" is a real observation.
    std::fs::create_dir_all(pack_root.join("scenes")).expect("scenes dir");
    std::fs::create_dir_all(pack_root.join("npcs")).expect("npcs dir");
    std::fs::write(
        pack_root.join("pack.ron"),
        "(name: \"logless\", entry: \"entry\")",
    )
    .expect("manifest");
    std::fs::write(pack_root.join("state.ron"), "{}").expect("state");
    std::fs::write(
        pack_root.join("scenes/entry.unmei"),
        "Narrator: the logless pack plays.\n",
    )
    .expect("scene");

    // The real failing open: a directory occupies the log path, so
    // `File::create` cannot truncate it. The entry point derives the path and
    // records the error exactly as `main` does.
    let log_path = session_log_path_for(&pack_root);
    std::fs::create_dir_all(&log_path).expect("the log path is a directory");
    let error = match SessionLog::open(log_path.clone(), &unmei_app::launch::version_text()) {
        Ok(_) => panic!("a directory is not writable"),
        Err(error) => error,
    };

    let mut app = App::new();
    app.insert_resource(LogOpenError {
        path: log_path.clone(),
        message: error.to_string(),
    })
    // The dev gate must be present before `LogPlugin` runs.
    .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
    .add_plugins(MinimalPlugins)
    .add_plugins(InputPlugin)
    .add_plugins(LogPlugin {
        custom_layer: app_log_layer,
        ..Default::default()
    })
    .add_plugins(RuntimeCorePlugin);
    app.update(); // Startup: the load, the funnel and the log failure settle

    // Play continues console-only: the healthy pack still builds a Runtime and
    // the failed sink is not a startup failure.
    assert!(
        app.world().get_resource::<Runtime>().is_some(),
        "the pack plays despite the unavailable log"
    );
    assert!(
        app.world()
            .get_resource::<unmei_app::failure::StartupFailure>()
            .is_none(),
        "an unavailable log is not a startup failure"
    );
    // The log path is still a directory: a failed open never created a file.
    assert!(log_path.is_dir(), "the failed open left the path as it was");

    // The kira observer and the dev-gated Monitor capture are installed
    // independently of the missing file sink.
    assert!(
        app.world().get_resource::<AudioSetupFailure>().is_some(),
        "the kira observer must stay installed when the file sink is unavailable"
    );
    let history = app
        .world()
        .get_resource::<MonitorHistory>()
        .expect("dev mode on installs capture and its history even without a file sink");

    // Exactly one row-21 warning reaches the shared subscriber (and therefore
    // both the console sink and the Monitor capture); the file name is carried
    // in its detail, and no Notice is raised.
    let warnings: Vec<_> = history
        .lines()
        .iter()
        .filter(|line| {
            line.target == "unmei::failure"
                && line.level == log::Level::Warn
                && line.text.contains("pack.log")
        })
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "exactly one row-21 warning, no duplicate: {}",
        history.text()
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "row 21 must not show a Notice"
    );
}
