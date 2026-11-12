//! Credential-safety proof for the Decisions transport (contract in
//! `docs/runtime-hardening.md` → Monitor → Log stream → credential-safety
//! prerequisite).
//!
//! Every fixture uses dummy loopback credentials and a throwaway server: no real
//! key, no external request. The suite drives the real [`DecisionsClient`] and
//! asserts the categorical diagnostics that reach the fallback text and the
//! `unmei::decision` events — not a helper formatter.
//!
//! Deviation from the testing policy's "keep tests isolated from
//! process-global logging": this binary installs one process-wide `log`
//! capture. It is append-only (no test clears it), each test's absence check
//! uses a key unique to that test and each presence check a pin unique to it,
//! so the eight tests may run in parallel without affecting one another.
#![cfg(feature = "network")]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use unmei_engine::{
    Criteria, DecisionConfig, DecisionsClient, Fallback, QuestionType, ResolvedDecision,
    StateValue, UNVERIFIED_MODEL,
};

/// The fallback reason as the sinks render it: the `Display` text of the engine
/// error the seam carries, which is the credential-safety contract's subject.
fn reason_text(resolved: &ResolvedDecision) -> Option<String> {
    resolved.fallback_reason.as_ref().map(ToString::to_string)
}

// ---------------------------------------------------------------------------
// Log capture
// ---------------------------------------------------------------------------

/// One captured `log` record: level, target, rendered message.
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

fn capture() -> Captured {
    static LOGGER: OnceLock<Captured> = OnceLock::new();
    LOGGER
        .get_or_init(|| {
            let captured = Captured::default();
            // `set_boxed_logger` needs the `log/alloc` feature, which the engine
            // does not carry; leaking is fine for a test binary's one logger.
            let logger: &'static Captured = Box::leak(Box::new(captured.clone()));
            log::set_logger(logger).expect("logger set once");
            log::set_max_level(log::LevelFilter::Trace);
            captured
        })
        .clone()
}

fn logs_contain(captured: &Captured, needle: &str) -> bool {
    captured
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|(_, _, message)| message.contains(needle))
}

// ---------------------------------------------------------------------------
// Loopback fixtures
// ---------------------------------------------------------------------------

/// Read one complete HTTP/1.1 request: headers, then exactly `Content-Length`
/// bytes of body.
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

/// A loopback server answering every request with the fixture for that
/// connection's index and the server's own base URL. Returns the decisions
/// endpoint and a connection counter.
fn spawn_server<F>(fixture: F) -> (String, Arc<AtomicUsize>)
where
    F: Fn(usize, &str) -> (String, Vec<(String, String)>, String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let base = format!("http://{addr}");
    let count = Arc::new(AtomicUsize::new(0));
    let thread_count = Arc::clone(&count);
    let thread_base = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let index = thread_count.fetch_add(1, Ordering::SeqCst);
            read_full_request(&mut stream);
            let (status, headers, body) = fixture(index, &thread_base);
            let mut response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                response.push_str(&format!("{name}: {value}\r\n"));
            }
            response.push_str("\r\n");
            response.push_str(&body);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("{base}/decisions"), count)
}

/// A distinct, live loopback listener a redirect would have to reach if the
/// client followed one. It answers every request with a fixed `200`, so a probe
/// can read the response to completion and the recording order is deterministic,
/// and it is deliberately not the origin listener, whose own count cannot show
/// whether a follow-up left the process.
fn spawn_destination() -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind destination");
    let addr = listener.local_addr().expect("destination addr");
    let count = Arc::new(AtomicUsize::new(0));
    let thread_count = Arc::clone(&count);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            thread_count.fetch_add(1, Ordering::SeqCst);
            read_full_request(&mut stream);
            let body = "{\"model\":\"m\"}";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (addr, count)
}

fn choice_config() -> DecisionConfig {
    DecisionConfig {
        question_type: QuestionType::Choice,
        instructions: "How does Ace greet the player?".to_string(),
        state_keys: vec!["met_ace".to_string()],
        criteria: Criteria::Choice(HashMap::from([
            ("warm".to_string(), "affectionate".to_string()),
            ("cold".to_string(), "guarded".to_string()),
        ])),
        fallback: Fallback::Action("cold".to_string()),
    }
}

fn score_config() -> DecisionConfig {
    DecisionConfig {
        question_type: QuestionType::Score,
        instructions: "How much does Ace trust the player?".to_string(),
        state_keys: vec!["met_ace".to_string()],
        criteria: Criteria::Score(vec![
            "No trust.".to_string(),
            "Some trust.".to_string(),
            "Complete trust.".to_string(),
        ]),
        fallback: Fallback::Index(0),
    }
}

fn state() -> HashMap<String, StateValue> {
    HashMap::from([("met_ace".to_string(), StateValue::Bool(true))])
}

fn choice_body(model: &str, choice: &str) -> String {
    format!(
        r#"{{"model":"{model}","answers":{{"greet":{{"type":"choice","choice":"{choice}","confidence":0.7}}}}}}"#
    )
}

// ---------------------------------------------------------------------------
// Diagnostics never retain response-controlled text
// ---------------------------------------------------------------------------

#[tokio::test]
async fn non_success_body_is_never_retained() {
    let captured = capture();
    const KEY: &str = "sk-dummy-non-2xx";

    let (endpoint, _) = spawn_server(|_, _| {
        (
            "500 Internal Server Error".to_string(),
            Vec::new(),
            format!("upstream reflected {KEY}"),
        )
    });
    let client = DecisionsClient::new(endpoint, KEY);
    let resolved = client
        .decide_with_fallback("m", "greet", &choice_config(), &state())
        .await;

    assert_eq!(
        reason_text(&resolved).as_deref(),
        Some("Decisions API returned HTTP 500")
    );
    assert_eq!(resolved.model, None);
    assert!(!format!("{resolved:?}").contains(KEY));
    assert!(!logs_contain(&captured, KEY));
}

#[tokio::test]
async fn malformed_body_reflecting_the_key_is_categorical() {
    let captured = capture();
    const KEY: &str = "sk-dummy-decoder";

    // An unknown answer `type` makes serde quote the reflected key in its error.
    let (endpoint, _) = spawn_server(|_, _| {
        (
            "200 OK".to_string(),
            Vec::new(),
            format!(r#"{{"model":"m","answers":{{"greet":{{"type":"{KEY}"}}}}}}"#),
        )
    });
    let client = DecisionsClient::new(endpoint, KEY);
    let resolved = client
        .decide_with_fallback("m", "greet", &choice_config(), &state())
        .await;

    assert_eq!(
        reason_text(&resolved).as_deref(),
        Some("invalid Decisions response JSON")
    );
    assert!(!logs_contain(&captured, KEY));
}

#[tokio::test]
async fn invalid_action_reflecting_the_key_is_categorical() {
    let captured = capture();
    const KEY: &str = "sk-dummy-invalid-action";

    let body = choice_body("m", KEY);
    let (endpoint, _) = spawn_server(move |_, _| ("200 OK".to_string(), Vec::new(), body.clone()));
    let client = DecisionsClient::new(endpoint, KEY);
    let resolved = client
        .decide_with_fallback("m", "greet", &choice_config(), &state())
        .await;

    assert_eq!(
        reason_text(&resolved).as_deref(),
        Some("model chose an action outside the criteria list")
    );
    assert!(!logs_contain(&captured, KEY));
}

#[tokio::test]
async fn transport_failure_on_a_secret_bearing_path_is_categorical() {
    let captured = capture();
    const KEY: &str = "sk-dummy-transport";

    // Bind then drop, so the port refuses the connection while the path still
    // carries the credential marker.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);

    let endpoint = format!("http://{addr}/decisions/{KEY}");
    let client = DecisionsClient::new(endpoint, KEY);
    let resolved = client
        .decide_with_fallback("m", "greet", &choice_config(), &state())
        .await;

    assert_eq!(
        reason_text(&resolved).as_deref(),
        Some("HTTP request failed")
    );
    assert!(!format!("{resolved:?}").contains(KEY));
    assert!(!logs_contain(&captured, KEY));
}

// ---------------------------------------------------------------------------
// Model pin and echoed legend
// ---------------------------------------------------------------------------

#[tokio::test]
async fn matching_pin_is_kept_and_labelled_as_requested() {
    let captured = capture();

    let body = choice_body("m-match-117", "warm");
    let (endpoint, _) = spawn_server(move |_, _| ("200 OK".to_string(), Vec::new(), body.clone()));
    let client = DecisionsClient::new(endpoint, "sk-dummy-match");
    let resolved = client
        .decide_with_fallback("m-match-117", "greet", &choice_config(), &state())
        .await;

    assert_eq!(resolved.action, "warm");
    assert_eq!(resolved.model.as_deref(), Some("m-match-117"));
    assert!(!resolved.is_fallback());
    assert!(logs_contain(
        &captured,
        "requested model `m-match-117` response model `m-match-117`"
    ));
}

#[tokio::test]
async fn mismatching_or_reflected_model_collapses_to_the_marker() {
    let captured = capture();
    const KEY: &str = "sk-dummy-as-model";

    let body = choice_body(KEY, "warm");
    let (endpoint, _) = spawn_server(move |_, _| ("200 OK".to_string(), Vec::new(), body.clone()));
    let client = DecisionsClient::new(endpoint, "sk-dummy-requested");
    let resolved = client
        .decide_with_fallback("m-mismatch-117", "greet", &choice_config(), &state())
        .await;

    assert_eq!(resolved.action, "warm");
    assert_eq!(resolved.model.as_deref(), Some(UNVERIFIED_MODEL));
    assert!(!logs_contain(&captured, KEY));
    assert!(logs_contain(
        &captured,
        "requested model `m-mismatch-117` response model `unverified response model`"
    ));
}

#[tokio::test]
async fn echoed_legend_is_absent_from_provenance_and_logs() {
    let captured = capture();
    const KEY: &str = "sk-dummy-legend";

    let body = format!(
        r#"{{"model":"m","answers":{{"trust":{{"type":"score","score":1.0,"legend":{{"0":"{KEY}"}},"probabilities":{{"0":0.1,"1":0.2,"2":0.7}},"confidence":0.9}}}}}}"#
    );
    let (endpoint, _) = spawn_server(move |_, _| ("200 OK".to_string(), Vec::new(), body.clone()));
    let client = DecisionsClient::new(endpoint, "sk-dummy-requested");

    let decision = client
        .decide("m", "trust", &score_config(), &state())
        .await
        .expect("score response validates");

    assert_eq!(decision.action, "2");
    assert_eq!(decision.score, Some(1.0));
    assert_eq!(decision.confidence, Some(0.9));
    assert_eq!(
        decision.legend, None,
        "echoed legend must not be provenance"
    );
    assert!(logs_contain(&captured, "authored legend"));
    assert!(!logs_contain(&captured, KEY));
}

// ---------------------------------------------------------------------------
// Redirects are never followed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn redirects_are_not_followed_and_location_is_never_logged() {
    let captured = capture();
    const KEY: &str = "sk-dummy-redirect";

    // A distinct destination listener, shown live before the exchanges so its
    // later baseline of one is evidence and not a dead socket. The probe reads
    // its response to completion, so the accept loop has recorded the connection
    // deterministically; every arrival past that baseline is a redirect follow.
    let (destination_addr, destination_connections) = spawn_destination();
    {
        let mut probe =
            std::net::TcpStream::connect(destination_addr).expect("destination listener accepts");
        probe
            .write_all(b"GET /probe HTTP/1.1\r\nHost: probe\r\nConnection: close\r\n\r\n")
            .expect("probe request writes");
        let mut response = Vec::new();
        let _ = probe.read_to_end(&mut response);
        assert!(!response.is_empty(), "the destination answered the probe");
    }
    assert_eq!(
        destination_connections.load(Ordering::SeqCst),
        1,
        "the destination listener records a connection"
    );

    for (status, code) in [
        ("301 Moved Permanently", 301),
        ("302 Found", 302),
        ("307 Temporary Redirect", 307),
        ("308 Permanent Redirect", 308),
    ] {
        // A syntactically valid absolute loopback URL reflecting the key in its
        // authority (userinfo), path and query — a malformed Location would be
        // a trivial non-follow rather than the exercised case. The host/port
        // pins stop a doubled-scheme string from parsing as a different host.
        let location = format!("http://{KEY}@{destination_addr}/{KEY}/next?token={KEY}");
        let parsed = url::Url::parse(&location).expect("the redirect Location parses");
        assert_eq!(parsed.scheme(), "http");
        assert_eq!(parsed.username(), KEY);
        assert_eq!(parsed.host_str(), Some("127.0.0.1"));
        assert_eq!(parsed.port(), Some(destination_addr.port()));
        assert!(parsed.path().contains(KEY));
        assert!(parsed.query().is_some_and(|query| query.contains(KEY)));

        let loc = location.clone();
        let (endpoint, connections) = spawn_server(move |_, _| {
            (
                status.to_string(),
                vec![("Location".to_string(), loc.clone())],
                format!("redirecting with {KEY}"),
            )
        });
        let client = DecisionsClient::new(endpoint, KEY);
        let resolved = client
            .decide_with_fallback("m", "greet", &choice_config(), &state())
            .await;

        let expected = format!("Decisions API returned HTTP {code}");
        assert_eq!(
            reason_text(&resolved).as_deref(),
            Some(expected.as_str()),
            "status {code} must take the categorical fallback"
        );
        assert!(
            !format!("{resolved:?}").contains(KEY),
            "status {code} must not return the reflected credential"
        );
        assert_eq!(
            connections.load(Ordering::SeqCst),
            1,
            "status {code} must not open a follow-up connection or retry"
        );
        assert_eq!(
            destination_connections.load(Ordering::SeqCst),
            1,
            "status {code} must not reach the distinct destination listener"
        );
        assert!(
            !logs_contain(&captured, KEY),
            "status {code} must not log the reflected credential or Location"
        );
    }
}
