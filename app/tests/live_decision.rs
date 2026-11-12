//! Live end-to-end runtime smoke test: the Bevy core steps the demo pack through
//! a **real** OpenRouter Decisions call using the async task path, validating the
//! one piece the vertical slice deliberately left unvalidated (the async wait),
//! which only a real call can prove.
//!
//! Ignored by default: it makes a real network call and needs a key.
//!
//! ```console
//! $ OPENROUTER_API_KEY=... cargo test -p unmei-app --test live_decision -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::input::ButtonInput;
use bevy::prelude::*;
use unmei_app::{PendingDecision, Runtime, RuntimeConfig, RuntimeCorePlugin};
use unmei_engine::{DecisionsClient, Wait};

/// Log records captured so the test can assert *how* the decision resolved, not
/// just where the script landed (a fallback lands on the same `reply` shape).
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<(log::Level, String, String)>>>);

impl log::Log for Captured {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        self.0.lock().unwrap().push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
    }

    fn flush(&self) {}
}

fn demo_pack_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/demo-pack")
        .canonicalize()
        .expect("demo pack path exists")
}

fn live_app(client: DecisionsClient) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(demo_pack_root(), Some(Arc::new(client))))
        .add_plugins(RuntimeCorePlugin);
    app.update(); // Startup: load the pack
    app
}

/// Tap one key for one frame, then release it so it can tap again.
fn tap(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
}

#[test]
#[ignore = "live OpenRouter call; see the command in this file's header"]
fn live_decision_resolves_through_the_bevy_async_wait() {
    let capture = Captured::default();
    log::set_boxed_logger(Box::new(capture.clone())).expect("no logger set in this test binary");
    log::set_max_level(log::LevelFilter::Info);

    let client = DecisionsClient::from_env().expect("set OPENROUTER_API_KEY to run this test");
    let mut app = live_app(client);

    // Play : walk the dialogue, take the first choice, and wait out the decision.
    // The first choice sets `affinity_level = high`, so a live model should answer
    // `warm`; the author fallback is `cold`.
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut saw_async_task = false;
    let landed = loop {
        assert!(
            Instant::now() < deadline,
            "live decision did not land in 90s"
        );

        let wait = app.world().resource::<Runtime>().runner.wait().cloned();
        match wait {
            Some(Wait::Say { .. }) => tap(&mut app, KeyCode::Space),
            Some(Wait::Choice { .. }) => tap(&mut app, KeyCode::Digit1),
            Some(Wait::Decision { .. }) => {
                if app.world().resource::<PendingDecision>().0.is_some() {
                    saw_async_task = true;
                }
                app.update(); // let `poll_decision` advance the in-flight call
            }
            None => panic!("script ended before the reply"),
        }

        // The reply branches are the only say-lines that look like these.
        if let Some(Wait::Say { speaker, text }) = app.world().resource::<Runtime>().runner.wait() {
            if text.contains("You came back!")
                || text.contains("It's you.")
                || text.contains("unreadable")
            {
                break (speaker.clone(), text.clone());
            }
        }
    };

    let records = capture.0.lock().unwrap().clone();
    let decision_logs: Vec<_> = records
        .iter()
        .filter(|(_, target, _)| target == "unmei::decision")
        .map(|(level, _, message)| (*level, message.clone()))
        .collect();
    eprintln!("decision log records: {decision_logs:#?}");
    eprintln!("landed on {landed:?}");

    // The async task path — not the immediate offline fallback — carried the call.
    assert!(
        saw_async_task,
        "the runtime never entered the async Decisions wait"
    );
    // It really went to the model (an info `... via <model>` record) and did not
    // fall back (no warn record).
    assert!(
        decision_logs
            .iter()
            .any(|(level, message)| *level == log::Level::Info && message.contains("via")),
        "no live `decision -> ... via <model>` record: {decision_logs:?}"
    );
    assert!(
        !decision_logs
            .iter()
            .any(|(level, message)| *level == log::Level::Warn && message.contains("fell back")),
        "a live call fell back: {decision_logs:?}"
    );
    // `high` affinity should read as warm; anything else means the model judged
    // against the criteria, which is worth seeing fail loudly in a smoke test.
    assert!(
        landed.1.contains("You came back!"),
        "expected a live `warm` answer, landed on {landed:?}"
    );
}
