//! One real Decisions HTTP 500 through the headless app's spawn → poll →
//! application path, under the behavioral-protection rule that a fallback path
//! arms no autosave write of its own.
//!
//! The app is configured with the actual [`DecisionsClient`], a dummy key and a
//! loopback endpoint that withholds its 500 until the test has observed the
//! parked, in-flight request. Play is entered through the title's own `New Game`
//! input path and the entry say-line is advanced by the real input system, so the
//! decision request is spawned by the real `drive_script` on the task pool,
//! polled by the real `poll_decision`, and applied by the real resolution funnel.
//! Nothing here is pre-resolved: a fake fallback would not prove client
//! selection.
//!
//! The fixture deliberately has one scene and no choice. Entering play, walking
//! its one say-line and landing the fallback therefore arm no independent
//! autosave write point (a scene crossing or a player choice), so the absent
//! autosave is attributable to the fallback path rather than to a confound.
//!
//! Independently credited rather than re-proved:
//!
//! - the full real-client/real-runner response matrix and the stalled 30-second
//!   timeout: `engine/tests/decision_matrix.rs`;
//! - the absent-client fallback and the fake resolver's spawn/poll/held/
//!   cancellation/exactly-once model-autosave lifecycle: `app/src/tests.rs`;
//! - credential sinks, redirect handling and categorical status text:
//!   `app/tests/credential_safety.rs`, `engine/tests/decision_safety.rs`.
//!
//! This is headless functional evidence. It says nothing about binary boot,
//! rendering, physical input or archive acceptance.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::input::ButtonInput;
use bevy::prelude::*;
use unmei_app::failure::Notice;
use unmei_app::menu::{MenuFocus, MenuState};
use unmei_app::{
    DefensiveHold, HeldDecision, PendingDecision, Runtime, RuntimeConfig, RuntimeCorePlugin,
};
use unmei_engine::save::{save_path_for, Slot};
use unmei_engine::{DecisionsClient, Wait};

/// A dummy loopback credential: the fixture never reaches a real provider.
const DUMMY_KEY: &str = "sk-dummy-225-app";

const PACK_RON: &str = r#"(name: "fallback witness", entry: "start")"#;
const STATE_RON: &str = r#"(met_ace: false)"#;

const NPC_RON: &str = r#"(
    id: "ace",
    name: "Ace",
    model: "fixture/model",
    decisions: {
        "greet": (
            instructions: "How does Ace greet the player?",
            state_keys: ["met_ace"],
            criteria: {
                "warm": "A warm greeting.",
                "cold": "A cold greeting.",
            },
            fallback: "cold",
        ),
    },
)"#;

/// One scene, one say-line before the decision, and an authored fallback branch
/// with a second line so ordinary progression past the fallback is observable.
const SCENE: &str = r#"# scenes/start.unmei
Narrator: The gate is shut.

decide ace.greet -> reply

label reply.warm
Ace: Warm landing.

label reply.cold
Ace: Cold landing.
Ace: The gate stays shut.
"#;

fn write_fixture(root: &Path) {
    let files = [
        ("pack.ron", PACK_RON),
        ("state.ron", STATE_RON),
        ("npcs/ace.ron", NPC_RON),
        ("scenes/start.unmei", SCENE),
    ];
    for (relative, content) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("nested fixture path"))
            .expect("create fixture directory");
        std::fs::write(path, content).expect("write fixture file");
    }
}

/// Boot the headless core over the fixture and cross the title into play; every
/// frame after this is driven by the app's own systems.
fn boot_app(pack_root: &Path, client: DecisionsClient) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(RuntimeConfig::new(
            pack_root.to_path_buf(),
            Some(Arc::new(client)),
        ))
        .add_plugins(RuntimeCorePlugin);
    app.update(); // Startup: load the pack and show the title
    app
}

fn menu(app: &App) -> &MenuState {
    app.world().resource::<MenuState>()
}

fn runner(app: &App) -> &Runtime {
    app.world().resource::<Runtime>()
}

fn wait(app: &App) -> Option<Wait> {
    runner(app).runner.wait().cloned()
}

fn press(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
}

/// Cross the real title path: focus `New Game` and press Enter.
fn enter_play(app: &mut App) {
    app.world_mut().resource_mut::<MenuState>().focus = MenuFocus::TitleNewGame;
    press(app, KeyCode::Enter);
    assert!(!menu(app).open, "the title must actually start play");
}

/// Advance frames on the real task pool until `check` holds. The loopback call
/// is answered immediately, so this bound only catches a stuck spawn/poll.
fn pump_until(app: &mut App, mut check: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !check(app) {
        assert!(
            Instant::now() < deadline,
            "the app did not reach the expected state in 10s; waiting on {:?}",
            wait(app)
        );
        std::thread::sleep(Duration::from_millis(2));
        app.update();
    }
}

#[test]
fn a_real_http_500_reaches_the_authored_fallback_through_the_headless_app() {
    let dir = tempfile::tempdir().expect("temp fixture dir");
    let pack_root = dir.path().join("pack");
    write_fixture(&pack_root);

    let server = Server::answering_500();
    let client = DecisionsClient::new(server.endpoint(), DUMMY_KEY);
    let mut app = boot_app(&pack_root, client);
    enter_play(&mut app);

    let autosave = save_path_for(runner(&app).runner.pack(), Slot::Autosave);
    assert!(
        matches!(
            wait(&app),
            Some(Wait::Say { ref text, .. }) if text == "The gate is shut."
        ),
        "play starts on the fixture's entry say-line, got {:?}",
        wait(&app)
    );
    // The fixture itself provides no qualifying autosave trigger, so the later
    // absence can be attributed to the fallback path.
    assert!(
        !autosave.exists(),
        "the fixture must not autosave before the decision"
    );

    // One real input frame: `advance_say_input` leaves the say-line and
    // `drive_script` parks on the decide point and spawns the client request.
    press(&mut app, KeyCode::Space);
    assert!(
        matches!(wait(&app), Some(Wait::Decision { .. })),
        "the decision park must follow the entry say-line, got {:?}",
        wait(&app)
    );

    // The request is in flight but the server withholds its 500, so the parked
    // state and the pending task are observable independent of frame ordering.
    pump_until(&mut app, |_| server.request_arrived());
    assert!(
        matches!(wait(&app), Some(Wait::Decision { .. })),
        "a withheld answer cannot land the runner, got {:?}",
        wait(&app)
    );
    assert!(
        app.world().resource::<PendingDecision>().0.is_some(),
        "the real client request must actually be in flight"
    );
    assert!(
        app.world().resource::<HeldDecision>().0.is_none(),
        "nothing can be held while the request is merely in flight"
    );

    // Release the real 500; `poll_decision` applies the fallback on a later
    // frame. The authored branch is the only way to reach this say-line.
    server.release_500();
    pump_until(&mut app, |app| matches!(wait(app), Some(Wait::Say { .. })));
    assert_eq!(
        wait(&app),
        Some(Wait::Say {
            speaker: "Ace".to_string(),
            text: "Cold landing.".to_string(),
        }),
        "the exact authored fallback say-line must play"
    );
    assert!(
        server.served_500(),
        "the client must actually receive the 500, not a transport failure"
    );

    // Completed, not merely parked: the request is gone, no answer is held, and
    // the ordinary-fallback posture leaves no Notice or defensive hold.
    assert!(
        app.world().resource::<PendingDecision>().0.is_none(),
        "the completed request must leave no pending task"
    );
    assert!(
        app.world().resource::<HeldDecision>().0.is_none(),
        "a visible fallback applies immediately; nothing may stay held"
    );
    assert!(
        !app.world().resource::<DefensiveHold>().is_held(),
        "a transport fallback is not a defensive hold"
    );
    assert!(
        !app.world().resource::<Notice>().is_showing(),
        "the player is never told about an ordinary fallback"
    );
    assert!(
        !autosave.exists(),
        "a fallback resolution must not churn the autosave"
    );

    // Progression continues along the authored branch after the fallback.
    press(&mut app, KeyCode::Space);
    assert_eq!(
        wait(&app),
        Some(Wait::Say {
            speaker: "Ace".to_string(),
            text: "The gate stays shut.".to_string(),
        }),
        "the author's next line must follow the fallback"
    );

    // Settle extra frames so a delayed write would be observed, then re-check
    // the absence of fallback-only churn.
    for _ in 0..8 {
        app.update();
    }
    assert!(
        !autosave.exists(),
        "no fallback-only autosave may appear after the frame settles"
    );

    // The body the test sent is the production request for this decision point,
    // so the 500 — not a config or transport mistake — selected the fallback.
    let request = server.request_text();
    assert!(
        request.starts_with("POST /decisions "),
        "the production client must POST the decisions path"
    );
    assert!(
        request.contains("\"model\":\"fixture/model\""),
        "the pinned model must be in the request body"
    );
    assert!(
        request.contains("\"greet\""),
        "the authored question must be in the request body"
    );
}

/// A loopback `/decisions` endpoint owned by the test. It retains the request so
/// the test can show the real client asked for this decision, withholds its 500
/// until [`Server::release_500`] so the parked/in-flight state is observable
/// independent of frame ordering, and records whether the response was actually
/// written. `Drop` releases and stops the worker and joins it, so cleanup stays
/// bounded even when an assertion panics first.
struct Server {
    endpoint: String,
    request: Arc<Mutex<Vec<u8>>>,
    arrived: Arc<AtomicBool>,
    served: Arc<AtomicBool>,
    release: mpsc::Sender<()>,
    stop: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn answering_500() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener can be polled");
        let addr = listener.local_addr().expect("local addr");
        let request = Arc::new(Mutex::new(Vec::new()));
        let arrived = Arc::new(AtomicBool::new(false));
        let served = Arc::new(AtomicBool::new(false));
        let thread_request = Arc::clone(&request);
        let thread_arrived = Arc::clone(&arrived);
        let thread_served = Arc::clone(&served);
        let (release, release_rx) = mpsc::channel::<()>();
        let (stop, stop_rx) = mpsc::channel::<()>();

        let thread = std::thread::spawn(move || loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    read_full_request(&mut stream, &thread_request);
                    thread_arrived.store(true, Ordering::SeqCst);
                    // The test observes the park and the pending task before the
                    // first answer can arrive. A bounded wait keeps a panicking
                    // test's cleanup finite.
                    if release_rx.recv_timeout(Duration::from_secs(10)).is_err() {
                        break;
                    }
                    let body = "";
                    let response = format!(
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let served_ok =
                        stream.write_all(response.as_bytes()).is_ok() && stream.flush().is_ok();
                    thread_served.store(served_ok, Ordering::SeqCst);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(_) => break,
            }
        });
        Server {
            endpoint: format!("http://{addr}/decisions"),
            request,
            arrived,
            served,
            release,
            stop,
            thread: Some(thread),
        }
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn request_arrived(&self) -> bool {
        self.arrived.load(Ordering::SeqCst)
    }

    fn release_500(&self) {
        let _ = self.release.send(());
    }

    fn served_500(&self) -> bool {
        self.served.load(Ordering::SeqCst)
    }

    fn request_text(&self) -> String {
        String::from_utf8_lossy(&self.request.lock().unwrap()).into_owned()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.release.send(());
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Read one complete HTTP/1.1 request: headers, then exactly `Content-Length`
/// bytes of body. Answering over the client's unsent bytes resets the
/// connection and would be reported as a transport failure instead.
fn read_full_request(stream: &mut std::net::TcpStream, into: &Arc<Mutex<Vec<u8>>>) {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
        }
        let Some(body_start) = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|headers_end| headers_end + 4)
        else {
            continue;
        };
        let head = String::from_utf8_lossy(&raw[..body_start]).to_ascii_lowercase();
        let body_len = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if raw.len() >= body_start + body_len {
            break;
        }
    }
    into.lock().unwrap().extend_from_slice(&raw);
}
