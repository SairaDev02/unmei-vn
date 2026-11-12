//! Real Decisions client ⇄ real `ScriptRunner` response matrix, built under
//! the behavioral-protection decision.
//!
//! Each row drives the actual [`DecisionsClient`] — the production constructor,
//! a dummy key, a loopback endpoint — through the [`DecisionResolver`] seam the
//! runtime uses, then lands whatever it selected in a real [`ScriptRunner`]
//! over a fixture pack whose landing labels, landing marker and next say-lines
//! are authored here, independently of the wire body. The assertion is the
//! exact landing key and the authored next wait; `is_fallback()` alone is never
//! the check, and no pre-resolved answer is injected.
//!
//! Credited rather than duplicated:
//!
//! - the pure wire/validation contracts (request filtering and shape, `noul`
//!   threshold including its out-of-range behavior, `score` argmax/tie/
//!   out-of-legend, decode, missing answer, wrong/unknown type, out-of-criteria
//!   action, typed fallback keys) in `engine/src/decision/mod.rs`;
//! - absent-client progress to the authored fallback say-line:
//!   `app/src/tests.rs::decision_without_a_client_takes_the_author_fallback`;
//! - the absent-key selection boundary (no resolver is built; absence is never
//!   simulated by sending an empty bearer):
//!   `app/src/credentials.rs::launch_resolution_is_env_then_stored_then_none`;
//! - explicit trait delegation and the real-client HTTP 500 fallback:
//!   `engine/tests/decision_resolver.rs`;
//! - credential-safety diagnostics, redirect non-following and the categorical
//!   fail-closed status/transport mapping:
//!   `engine/tests/decision_safety.rs`, `app/tests/credential_safety.rs`.
//!
//! The one slow row is the stalled loopback below: it observes the shipped
//! [`REQUEST_TIMEOUT`] rather than shortening it, and is deliberately not
//! repeated at the app layer.
#![cfg(feature = "network")]

use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use unmei_engine::decision::client::REQUEST_TIMEOUT;
use unmei_engine::{
    load_pack, DecisionConfig, DecisionError, DecisionResolver, DecisionsClient, ResolvedDecision,
    RunOutcome, ScriptRunner, StateValue, Wait,
};

/// A dummy loopback credential: the fixtures never reach a real provider.
const DUMMY_KEY: &str = "sk-dummy-224-matrix";

/// One authored fixture pack: the decision question it parks on, the NPC RON
/// that declares the config and the scene that carries the landing labels.
struct Fixture {
    id: &'static str,
    npc_ron: &'static str,
    scene: &'static str,
}

const PACK_RON: &str = r#"(
    name: "decision matrix",
    entry: "gate",
)"#;

/// `landed` is the independent landing marker every fixture label sets; it is
/// declared here because pack load requires every `set` target to be declared.
const STATE_RON: &str = r#"(
    landed: "",
    met_ace: false,
)"#;

const CHOICE_NPC: &str = r#"(
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
                "none": "Neither fits.",
            },
            fallback: "cold",
        ),
    },
)"#;

const CHOICE_SCENE: &str = r#"# scenes/gate.unmei
decide ace.greet -> greet

label greet.warm
set landed = "warm"
Ace: Warm landing.

label greet.cold
set landed = "cold"
Ace: Cold landing.

label greet.none
set landed = "none"
Narrator: None landing.
"#;

const NOUL_NPC: &str = r#"(
    id: "ace",
    name: "Ace",
    model: "fixture/model",
    decisions: {
        "is_hostile": (
            type: "noul",
            instructions: "Is the player hostile?",
            state_keys: ["met_ace"],
            criteria: {
                "true": "Hostile.",
                "false": "Not hostile.",
            },
            fallback: false,
        ),
    },
)"#;

const NOUL_SCENE: &str = r#"# scenes/gate.unmei
decide ace.is_hostile -> want

label want.true
set landed = "true"
Narrator: Hostile landing.

label want.false
set landed = "false"
Narrator: Not hostile landing.
"#;

const SCORE_NPC: &str = r#"(
    id: "ace",
    name: "Ace",
    model: "fixture/model",
    decisions: {
        "trust": (
            type: "score",
            instructions: "How much does Ace trust the player?",
            state_keys: ["met_ace"],
            criteria: [
                "No trust.",
                "Some trust.",
                "Complete trust.",
            ],
            fallback: 0,
        ),
    },
)"#;

const SCORE_SCENE: &str = r#"# scenes/gate.unmei
decide ace.trust -> trust

label trust.0
set landed = "0"
Narrator: No trust landing.

label trust.1
set landed = "1"
Narrator: Some trust landing.

label trust.2
set landed = "2"
Narrator: Complete trust landing.
"#;

const CHOICE: Fixture = Fixture {
    id: "greet",
    npc_ron: CHOICE_NPC,
    scene: CHOICE_SCENE,
};
const NOUL: Fixture = Fixture {
    id: "is_hostile",
    npc_ron: NOUL_NPC,
    scene: NOUL_SCENE,
};
const SCORE: Fixture = Fixture {
    id: "trust",
    npc_ron: SCORE_NPC,
    scene: SCORE_SCENE,
};

fn write_fixture(root: &std::path::Path, fixture: &Fixture) {
    let files = [
        ("pack.ron", PACK_RON),
        ("state.ron", STATE_RON),
        ("npcs/ace.ron", fixture.npc_ron),
        ("scenes/gate.unmei", fixture.scene),
    ];
    for (relative, content) in files {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("nested fixture path"))
            .expect("create fixture directory");
        std::fs::write(path, content).expect("write fixture file");
    }
}

/// The say-line authored under one landing label.
#[derive(Clone, Copy)]
struct AuthoredSay {
    speaker: &'static str,
    text: &'static str,
}

/// One authored fixture pack, parked on its single decision point, plus the
/// config/model the loaded pack actually declares for it.
struct Case {
    /// Keeps the fixture pack directory alive for the case's lifetime.
    _dir: tempfile::TempDir,
    runner: ScriptRunner,
    question: &'static str,
    model: String,
    config: DecisionConfig,
}

impl Case {
    /// Load the fixture through the real loader (so its landing-label closure
    /// and state vocabulary are validated) and park on the decision.
    fn new(fixture: &Fixture) -> Self {
        let dir = tempfile::tempdir().expect("temp fixture dir");
        write_fixture(dir.path(), fixture);
        let pack = load_pack(dir.path()).expect("the fixture pack loads").pack;
        let config = pack.npcs["ace"].decisions[fixture.id].clone();
        let model = pack.npcs["ace"]
            .model
            .clone()
            .expect("the fixture pins a decision model");
        let mut runner = ScriptRunner::new(pack);
        assert!(
            matches!(runner.run().wait, Some(Wait::Decision { .. })),
            "the fixture's entry scene must park on its decision"
        );
        Case {
            _dir: dir,
            runner,
            question: fixture.id,
            model,
            config,
        }
    }

    /// Ask the real client — through the resolver seam the runtime uses — with
    /// the pack's own config and live state. The fallback is selected here, by
    /// the production code path; no answer is handed in.
    async fn resolve(&self, endpoint: &str, key: &str) -> ResolvedDecision {
        let client = DecisionsClient::new(endpoint, key);
        let state: std::collections::HashMap<String, StateValue> = self.runner.state().clone();
        client
            .resolve(&self.model, self.question, &self.config, &state)
            .await
    }

    /// Apply the resolved landing key and run to the next wait — exactly what
    /// the runtime does after the async layer returns.
    fn land(&mut self, resolved: &ResolvedDecision) -> RunOutcome {
        assert!(
            matches!(self.runner.wait(), Some(Wait::Decision { .. })),
            "the fixture must still be parked on its decision"
        );
        self.runner
            .resolve_decision(&resolved.action)
            .expect("the resolved action must be an authored landing label");
        self.runner.run()
    }

    /// The fixture's independent landing marker: the key whose label ran.
    fn marker(&self) -> String {
        match self.runner.state().get("landed") {
            Some(StateValue::Str(key)) => key.clone(),
            other => panic!("expected the fixture's `landed` marker, got {other:?}"),
        }
    }

    /// Assert the exact landing key the runner took **and** the say-line
    /// authored under it, so progress — not just a fallback flag — is the
    /// subject.
    fn assert_landed(&self, landed: &RunOutcome, key: &str, say: AuthoredSay) {
        assert_eq!(
            self.marker(),
            key,
            "the runner must take the `{key}` landing label"
        );
        match landed.wait.as_ref() {
            Some(Wait::Say { speaker, text }) => {
                assert_eq!(speaker, say.speaker, "`{key}` lands on an authored speaker");
                assert_eq!(text, say.text, "`{key}` lands on its own authored say-line");
            }
            other => panic!("expected the authored say-line after `{key}`, got {other:?}"),
        }
    }
}

/// Read one complete HTTP/1.1 request: headers, then exactly `Content-Length`
/// bytes of body. Answering over the client's unsent bytes resets the
/// connection and would be reported as a transport failure instead.
fn read_full_request(stream: &mut std::net::TcpStream) {
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
}

/// A loopback `/decisions` endpoint owned by the test.
///
/// [`Server::answering`] replies to every request with one canned response;
/// [`Server::stalling`] reads every request and answers none, so only the
/// shipped client timeout can end the exchange. Both count their connections,
/// so "no retry" is observed rather than assumed, and dropping the server
/// releases its sockets and joins the worker — fixture cleanup stays bounded
/// even when an assertion panics first.
struct Server {
    endpoint: String,
    connections: Arc<AtomicUsize>,
    stop: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn answering(status: &'static str, body: &'static str) -> Self {
        Server::spawn(Some((status, body)))
    }

    fn stalling() -> Self {
        Server::spawn(None)
    }

    fn spawn(answer: Option<(&'static str, &'static str)>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener can be polled for a later connection");
        let addr = listener.local_addr().expect("local addr");
        let connections = Arc::new(AtomicUsize::new(0));
        let thread_connections = Arc::clone(&connections);
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            // An unanswered request is held open so a retry would be accepted
            // and counted rather than queued behind a closed door.
            let mut held: Vec<std::net::TcpStream> = Vec::new();
            loop {
                if stopped.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        thread_connections.fetch_add(1, Ordering::SeqCst);
                        // An accepted socket may inherit the listener's
                        // non-blocking state; read in blocking mode with a
                        // timeout so a partial request bounds shutdown instead
                        // of blocking the worker (and `Drop`'s join) forever.
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        read_full_request(&mut stream);
                        match answer {
                            Some((status, body)) => {
                                let response = format!(
                                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                    body.len()
                                );
                                let _ = stream.write_all(response.as_bytes());
                                let _ = stream.flush();
                            }
                            None => held.push(stream),
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Server {
            endpoint: format!("http://{addr}/decisions"),
            connections,
            stop,
            thread: Some(thread),
        }
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Drive one failing row over the choice fixture: the authored fallback `cold`
/// must be selected by the real client, its exact categorical reason retained,
/// and the runner must reach the say-line authored under `greet.cold` — once.
async fn assert_failure_row(status: &'static str, body: &'static str, expected: DecisionError) {
    let mut case = Case::new(&CHOICE);
    let server = Server::answering(status, body);

    let started = Instant::now();
    let resolved = case.resolve(server.endpoint(), DUMMY_KEY).await;
    let elapsed = started.elapsed();

    assert_eq!(
        resolved.action, "cold",
        "the authored fallback action must be selected"
    );
    assert_eq!(
        resolved.fallback_reason.as_ref(),
        Some(&expected),
        "the fallback reason must be the categorical engine error"
    );
    assert_eq!(
        resolved
            .fallback_reason
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some(expected.to_string().as_str()),
        "the rendered reason must be the fixed categorical text"
    );
    assert_eq!(resolved.model, None);

    // The whole exchange is local and answered immediately, so waiting means a
    // retry backoff: this bound rejects the delay the 429 fixture advertises.
    assert!(
        elapsed < Duration::from_secs(10),
        "a failed decision call must not wait on a retry backoff; took {elapsed:?}"
    );

    let landed = case.land(&resolved);
    case.assert_landed(
        &landed,
        "cold",
        AuthoredSay {
            speaker: "Ace",
            text: "Cold landing.",
        },
    );

    // An immediate retry would race the fixture's accept loop, and a short
    // backoff would arrive inside this window; either opens a second connection.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        server.connections(),
        1,
        "a failed decision call must not be retried"
    );
}

#[tokio::test]
async fn unauthorized_status_takes_the_authored_fallback_and_next_wait() {
    // 403 is the same rejection-status class: the client maps every non-success
    // status to one categorical `Status`, so one row proves the class.
    assert_failure_row(
        "401 Unauthorized",
        r#"{"error":{"message":"missing or invalid key"}}"#,
        DecisionError::Status { status: 401 },
    )
    .await;
}

#[tokio::test]
async fn rate_limited_status_takes_the_authored_fallback_without_retry_or_backoff() {
    assert_failure_row(
        "429 Too Many Requests",
        r#"{"error":{"message":"rate limited","retry_after":30}}"#,
        DecisionError::Status { status: 429 },
    )
    .await;
}

#[tokio::test]
async fn syntactically_invalid_json_takes_the_authored_fallback_and_next_wait() {
    assert_failure_row(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"#,
        DecisionError::Decode,
    )
    .await;
}

#[tokio::test]
async fn known_but_wrong_answer_type_takes_the_authored_fallback_and_next_wait() {
    // Valid JSON, valid wire `type`, wrong for this question: a known type
    // mismatch, not a decode failure.
    assert_failure_row(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"greet":{"type":"noul","noul":0.96}}}"#,
        DecisionError::WrongType {
            question: "greet".to_string(),
            expected: "choice".to_string(),
            answer_type: "noul".to_string(),
        },
    )
    .await;
}

#[tokio::test]
async fn missing_requested_answer_takes_the_authored_fallback_and_next_wait() {
    assert_failure_row(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"other":{"type":"choice","choice":"warm"}}}"#,
        DecisionError::MissingAnswer("greet".to_string()),
    )
    .await;
}

#[tokio::test]
async fn choice_answer_without_its_required_choice_data_takes_the_authored_fallback() {
    // Distinct from a missing answer: the question is answered, but the answer
    // cannot carry a choice, so it never becomes a usable model answer.
    assert_failure_row(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"greet":{"type":"choice","confidence":0.8}}}"#,
        DecisionError::Decode,
    )
    .await;
}

#[tokio::test]
async fn action_outside_the_criteria_takes_the_authored_fallback_and_next_wait() {
    assert_failure_row(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"greet":{"type":"choice","choice":"purple"}}}"#,
        DecisionError::UnknownAction,
    )
    .await;
}

#[tokio::test]
async fn valid_choice_response_lands_on_the_independently_authored_wait() {
    let mut case = Case::new(&CHOICE);
    let server = Server::answering(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"greet":{"type":"choice","choice":"warm","confidence":0.8}}}"#,
    );

    let resolved = case.resolve(server.endpoint(), DUMMY_KEY).await;
    assert_eq!(resolved.action, "warm");
    assert_eq!(resolved.fallback_reason, None);

    let landed = case.land(&resolved);
    case.assert_landed(
        &landed,
        "warm",
        AuthoredSay {
            speaker: "Ace",
            text: "Warm landing.",
        },
    );
}

#[tokio::test]
async fn valid_noul_response_lands_on_the_independently_authored_wait() {
    // The bare `>= 0.5` threshold and its boundary cases are protected in
    // `engine/src/decision/tests.rs`; this row connects one representative
    // answer.
    let mut case = Case::new(&NOUL);
    let server = Server::answering(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"is_hostile":{"type":"noul","noul":0.96}}}"#,
    );

    let resolved = case.resolve(server.endpoint(), DUMMY_KEY).await;
    assert_eq!(resolved.action, "true");
    assert_eq!(resolved.fallback_reason, None);

    let landed = case.land(&resolved);
    case.assert_landed(
        &landed,
        "true",
        AuthoredSay {
            speaker: "Narrator",
            text: "Hostile landing.",
        },
    );
}

#[tokio::test]
async fn valid_score_response_lands_on_the_independently_authored_wait() {
    // Argmax over probabilities, never the scalar: the argmax/tie/out-of-legend
    // contracts stay in `engine/src/decision/tests.rs`; this row connects one
    // answer.
    let mut case = Case::new(&SCORE);
    let server = Server::answering(
        "200 OK",
        r#"{"model":"fixture/model","answers":{"trust":{"type":"score","score":0.4,"legend":{"0":"No trust.","1":"Some trust.","2":"Complete trust."},"probabilities":{"0":0.1,"1":0.2,"2":0.7},"confidence":0.9}}}"#,
    );

    let resolved = case.resolve(server.endpoint(), DUMMY_KEY).await;
    assert_eq!(resolved.action, "2");
    assert_eq!(resolved.fallback_reason, None);

    let landed = case.land(&resolved);
    case.assert_landed(
        &landed,
        "2",
        AuthoredSay {
            speaker: "Narrator",
            text: "Complete trust landing.",
        },
    );
}

#[test]
fn stalled_loopback_exhausts_the_production_timeout_then_progresses() {
    assert_eq!(
        REQUEST_TIMEOUT,
        Duration::from_secs(30),
        "the shipped Decisions timeout is unchanged"
    );

    let mut case = Case::new(&CHOICE);
    let config = case.config.clone();
    let model = case.model.clone();
    let state = case.runner.state().clone();
    let stalled = Server::stalling();
    let endpoint = stalled.endpoint().to_string();

    // The client future is driven on its own current-thread runtime so the test
    // thread can bound the wait with `recv_timeout`; the request itself runs on
    // the client's process-lifetime runtime, exactly as the app's does.
    let (sender, receiver) = mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a test current-thread runtime builds");
        let client = DecisionsClient::new(endpoint, DUMMY_KEY);
        let resolved = runtime.block_on(client.resolve(&model, "greet", &config, &state));
        let _ = sender.send((resolved, started.elapsed()));
    });

    let (resolved, elapsed) = receiver
        .recv_timeout(REQUEST_TIMEOUT + Duration::from_secs(30))
        .expect("a stalled request must resolve at its own timeout, never hang");
    assert!(
        elapsed >= REQUEST_TIMEOUT,
        "the shipped {REQUEST_TIMEOUT:?} timeout must be what expired; waited {elapsed:?}"
    );
    assert_eq!(resolved.action, "cold");
    assert_eq!(resolved.fallback_reason, Some(DecisionError::Http));
    assert_eq!(
        resolved
            .fallback_reason
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("HTTP request failed")
    );

    let landed = case.land(&resolved);
    case.assert_landed(
        &landed,
        "cold",
        AuthoredSay {
            speaker: "Ace",
            text: "Cold landing.",
        },
    );

    // A retry would open a second connection to the still-listening fixture.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(stalled.connections(), 1, "a timed-out call is not retried");
    // Dropping the fixture releases the held socket and joins its thread.
    drop(stalled);
}
