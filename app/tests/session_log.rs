//! End-to-end session-log check: with Bevy's real `LogPlugin` and the
//! `app_log_layer` custom layer installed, a bad-pack run's row-1 failure
//! reaches `<pack-dir>.log` — the same subscriber and filter the console sink
//! uses (ADR 0008; failure inventory rows 1 and 21).
//!
//! One test only: `LogPlugin` installs a process-global subscriber/logger.

use bevy::app::App;
use bevy::input::InputPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use unmei_app::failure::{app_log_layer, SessionLog};
use unmei_app::monitor::MonitorHistory;
use unmei_app::{RuntimeConfig, RuntimeCorePlugin};

#[test]
fn a_broken_pack_run_writes_every_error_to_the_session_log() {
    let dir = tempfile::tempdir().expect("temp dir");
    let pack_root = dir.path().join("broken-pack");
    std::fs::create_dir_all(pack_root.join("npcs")).expect("npcs dir");
    std::fs::create_dir_all(pack_root.join("scenes")).expect("scenes dir");
    std::fs::write(
        pack_root.join("pack.ron"),
        "(name: \"broken\", entry: \"scene\")",
    )
    .expect("manifest");
    std::fs::write(pack_root.join("state.ron"), "{}").expect("state");
    std::fs::write(pack_root.join("scenes/scene.unmei"), "Narrator: hi\n").expect("scene");
    // 21 NPC files whose ids do not match their filename stems.
    for i in 0..21 {
        std::fs::write(
            pack_root.join(format!("npcs/npc{i}.ron")),
            format!("(id: \"other{i}\", name: \"N{i}\", decisions: {{}})"),
        )
        .expect("npc");
    }

    // The app's entry point computes `<pack-dir>.log` and truncates it before
    // the pack load; mirror that here.
    let log_path = pack_root.with_extension("log");
    std::fs::write(&log_path, "a previous session\n").expect("seed the old log");
    let log = SessionLog::open(log_path.clone(), &unmei_app::launch::version_text())
        .expect("open the session log");

    let mut app = App::new();
    app.insert_resource(log)
        // The effective dev gate must be present before `LogPlugin` runs, since
        // the custom-layer hook reads it to decide whether to install capture.
        // Forced on so a release test run protects the same behavior.
        .insert_resource(RuntimeConfig::new(pack_root, None).with_dev_mode(true))
        .add_plugins(MinimalPlugins)
        .add_plugins(InputPlugin)
        .add_plugins(LogPlugin {
            custom_layer: app_log_layer,
            ..Default::default()
        })
        .add_plugins(RuntimeCorePlugin);
    app.update(); // Startup: the failed load is funneled, not exited

    let bytes = std::fs::read_to_string(&log_path).expect("read the log");
    assert!(
        !bytes.contains("a previous session"),
        "opening the log must truncate the previous session: {bytes:?}"
    );
    assert!(
        bytes.starts_with(&unmei_app::launch::version_text()),
        "the release version is the first line: {bytes:?}"
    );
    for i in 0..21 {
        assert!(
            bytes.contains(&format!("npcs/npc{i}.ron")),
            "every LoadError must reach the session log; missing npc{i}: {bytes:?}"
        );
    }

    // The hidden capture shares the same subscriber and filter: the row-1
    // failure lands in the retained Monitor history too, escaped and bounded,
    // independently of the file sink.
    let history = app
        .world()
        .get_resource::<MonitorHistory>()
        .expect("dev mode on installs capture and its history");
    assert!(
        history.retained() >= 1,
        "the captured startup failure must be ingested"
    );
    let captured = history.text();
    for i in 0..21 {
        assert!(
            captured.contains(&format!("npcs/npc{i}.ron")),
            "every LoadError must reach Monitor history; missing npc{i}: {captured:?}"
        );
    }
    assert!(
        history.lines().iter().all(|line| !line.text.contains('\n')),
        "every stored line is one escaped logical line"
    );
}
