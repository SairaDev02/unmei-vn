//! Hidden-capture ingestion wiring.
//!
//! These headless, explicit-dispatch tests prove the production
//! [`RuntimeCorePlugin`] path: a [`MonitorCapture`] admitted through the real
//! tracing layer is ingested into [`MonitorHistory`] by the app update, even
//! while the app is parked on the startup error screen with no
//! [`unmei_app::Runtime`] loaded. They install no global logger — the local
//! [`capture_layer`] subscriber is enough.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::ecs::system::RunSystemOnce;
use bevy::input::ButtonInput;
use bevy::log::tracing;
use bevy::log::tracing_subscriber;
use bevy::prelude::*;
use tracing_subscriber::prelude::*;

use unmei_app::monitor::{
    capture_layer, MonitorCapture, MonitorClock, MonitorHistory, SystemClock,
};
use unmei_app::{Runtime, RuntimeConfig, RuntimeCorePlugin};

/// A clock whose time only advances when the test sets it.
#[derive(Default)]
struct FixedClock {
    millis: std::sync::atomic::AtomicU64,
}

impl MonitorClock for FixedClock {
    fn now_millis(&self) -> u64 {
        self.millis.load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn capture() -> MonitorCapture {
    MonitorCapture::install(Arc::new(SystemClock::new()))
}

/// A headless core app over a missing pack: `Startup` records the load failure,
/// so there is never a `Runtime`, and the app sits on the startup error screen.
fn app_on_a_broken_pack(capture: MonitorCapture) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/missing/pack"), None).with_dev_mode(true),
        )
        .insert_resource(capture)
        .add_plugins(RuntimeCorePlugin);
    app
}

fn admit(capture: &MonitorCapture, message: &str) {
    admit_event(capture, || {
        tracing::info!(target: "unmei::test", "{}", message);
    });
}

/// Dispatch one event into `capture` through the real capture layer.
fn admit_event(capture: &MonitorCapture, event: impl FnOnce()) {
    let subscriber = tracing_subscriber::registry().with(capture_layer(capture));
    tracing::subscriber::with_default(subscriber, event);
}

/// Ingest every pending line into a fresh app's retained history and return the
/// escaped retained text, oldest first.
fn retained_text(capture: MonitorCapture) -> String {
    let mut app = App::new();
    app.insert_resource(capture)
        .init_resource::<MonitorHistory>();
    app.world_mut()
        .run_system_once(unmei_app::monitor::ingest_capture)
        .expect("the ingest system runs");
    app.world().resource::<MonitorHistory>().text()
}

#[test]
fn hidden_capture_ingests_on_the_error_screen_and_caps_one_update() {
    let capture = capture();
    // 129 events arrive before the app ever updates, so the startup failure's
    // own diagnostic is newer than all of them.
    for index in 0..129 {
        admit(&capture, &format!("event {index}"));
    }

    let mut app = app_on_a_broken_pack(capture.clone());
    app.update();

    assert!(
        app.world().get_resource::<Runtime>().is_none(),
        "the broken pack must leave no Runtime"
    );
    let history = app.world().resource::<MonitorHistory>();
    assert_eq!(
        history.retained(),
        128,
        "one update ingests at most 128 events"
    );
    assert!(
        history.lines()[0].text.ends_with("event 0"),
        "the oldest admitted line is ingested first: {}",
        history.lines()[0].text
    );

    // A second update carries the excess and the startup diagnostic in.
    app.update();
    let history = app.world().resource::<MonitorHistory>();
    let ours = history
        .lines()
        .iter()
        .filter(|line| line.text.contains("event "))
        .count();
    assert_eq!(ours, 129, "every admitted event is ingested exactly once");
    assert!(
        history
            .lines()
            .iter()
            .any(|line| line.text.ends_with("event 128")),
        "the newest admitted event survives"
    );
}

#[test]
fn hidden_capture_retains_through_further_updates_and_is_not_cleared() {
    let capture = capture();
    admit(&capture, "kept");
    let mut app = app_on_a_broken_pack(capture.clone());
    app.update();

    let before = app.world().resource::<MonitorHistory>().retained();
    assert!(before >= 1);
    // Extra frames on the same surface do not age the history out or clear it.
    for _ in 0..3 {
        app.update();
    }
    let history = app.world().resource::<MonitorHistory>();
    assert_eq!(
        history.retained(),
        before,
        "later updates neither clear nor grow the retained history"
    );
    assert!(history.text().contains("kept"));
}

#[test]
fn dev_mode_off_installs_no_capture_history() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(
            RuntimeConfig::new(PathBuf::from("definitely/missing/pack"), None).with_dev_mode(false),
        )
        .add_plugins(RuntimeCorePlugin);

    assert!(
        app.world().get_resource::<MonitorHistory>().is_none(),
        "no capture means no retained history"
    );
    assert!(app.world().get_resource::<MonitorCapture>().is_none());
}

#[test]
fn an_explicit_capture_clock_fixes_admission_metadata() {
    let clock = Arc::new(FixedClock::default());
    clock
        .millis
        .store(250, std::sync::atomic::Ordering::Relaxed);
    let capture = MonitorCapture::install(clock);
    admit(&capture, "stamped");
    let mut app = App::new();
    app.insert_resource(capture.clone())
        .init_resource::<MonitorHistory>();
    app.world_mut()
        .run_system_once(unmei_app::monitor::ingest_capture)
        .expect("the ingest system runs");

    let line = &app.world().resource::<MonitorHistory>().lines()[0];
    assert_eq!(line.elapsed_ms, 0, "installed at 250ms, admitted at 250ms");
    assert!(line.text.starts_with("0000000000ms "));
}

#[test]
fn retained_output_preserves_the_metadata_target_and_keeps_bridge_named_fields() {
    let capture = capture();
    admit_event(&capture, || {
        tracing::event!(
            target: "dep::crate",
            bevy::log::tracing::Level::INFO,
            { "log.target" } = "unmei::forged",
            answer = 42,
            "the real body"
        );
    });

    let text = retained_text(capture);
    assert_eq!(text.matches('\n').count(), 0, "one logical line: {text}");
    assert!(
        text.ends_with("dep::crate the real body log.target=unmei::forged answer=42"),
        "{text}"
    );
    // The forged field is not resurrected as a target the default projection
    // would match; `dep::crate` remains the predicate input.
    assert!(!text.contains("unmei::forged the real body"), "{text}");
}

#[test]
fn retained_output_renders_an_explicit_message_first_and_once() {
    let capture = capture();
    admit_event(&capture, || {
        tracing::event!(
            target: "unmei::ordered",
            tracing::Level::INFO,
            answer = 42,
            note = "hi",
            message = "explicit message"
        );
    });

    let text = retained_text(capture);
    assert!(
        text.ends_with("unmei::ordered explicit message answer=42 note=hi"),
        "{text}"
    );
    assert_eq!(text.matches("explicit message").count(), 1, "{text}");
}

#[test]
fn retained_output_recovers_a_genuine_bridge_target_and_drops_its_scaffolding() {
    let capture = capture();
    admit_event(&capture, || {
        let record = log::Record::builder()
            .args(format_args!("bridged body"))
            .level(log::Level::Info)
            .target("unmei::bridged")
            .module_path_static(Some("unmei::bridged"))
            .file_static(Some("bridged.rs"))
            .line(Some(7))
            .build();
        tracing_log::format_trace(&record).expect("the bridge dispatches");
    });

    let text = retained_text(capture);
    assert!(text.ends_with("unmei::bridged bridged body"), "{text}");
    assert!(
        !text.contains("log."),
        "the bridge scaffolding leaked: {text}"
    );
}
