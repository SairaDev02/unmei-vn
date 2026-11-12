//! Per-sink credential-safety proof (contract in
//! `docs/runtime-hardening.md` → Monitor → Log stream → credential-safety
//! prerequisite and real Monitor sink).
//!
//! A permissive TRACE subscriber is installed over the real console-format
//! renderer, the real `<pack-dir>.log` layer ([`file_layer`]) and the **real
//! Monitor admission layer and retained history** (`capture_layer` →
//! [`MonitorHistory`] through the production `ingest_capture` system). The
//! supported Decisions paths are driven with dummy loopback credentials; no
//! sink — console, file or the actual ingested Monitor output — may contain the
//! dummy key, a reflected body, a redirect `Location` or a reflected legend.
//!
//! An absence check over a possibly empty or dropped capture proves nothing, so
//! every fixture case also registers the categorical event it must have
//! produced ([`Expected`]) and the test asserts that event reached all three
//! sinks with the capture's `lost`/`evicted`/`truncated` counters at zero. The
//! 301/302/307/308 `Location` fixtures are syntactically valid absolute URLs
//! reflecting the dummy key in their authority (userinfo), path and query, and
//! they name a **distinct, live destination listener**: the test first shows
//! that listener records a request, then proves none arrives through the
//! redirect. No follow-up connection, forwarded `Authorization`, emitted
//! `Referer` or retry is therefore observed at the destination, not merely
//! inferred from the origin listener's own count.
//!
//! One test only: installing a global subscriber/logger is process-wide.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use bevy::ecs::system::RunSystemOnce;
use bevy::log::tracing;
use bevy::log::tracing_subscriber;
use bevy::log::tracing_subscriber::filter::EnvFilter;
use bevy::log::tracing_subscriber::fmt::{self, MakeWriter};
use bevy::log::tracing_subscriber::prelude::*;
use bevy::prelude::App;

use unmei_app::failure::{file_layer, report, DefensiveHold, Failure, FailureRow, Notice};
use unmei_app::monitor::{
    capture_layer, ingest_capture, MonitorCapture, MonitorHistory, SystemClock,
};
use unmei_engine::{
    Criteria, DecisionConfig, DecisionError, DecisionsClient, Fallback, QuestionType,
    ResolvedDecision, StateValue, UNVERIFIED_MODEL,
};

const KEY: &str = "sk-dummy-monitor-121";

// ---------------------------------------------------------------------------
// A writer a layer can render into, so the test can inspect the bytes.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn new() -> Self {
        SharedBuffer(Arc::new(Mutex::new(Vec::new())))
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

struct SharedBufferWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for SharedBuffer {
    type Writer = SharedBufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SharedBufferWriter(Arc::clone(&self.0))
    }
}

// ---------------------------------------------------------------------------
// Loopback fixtures
// ---------------------------------------------------------------------------

/// The raw bytes of every request one loopback listener received. A connection
/// count alone cannot show *which* headers arrived, and an empty destination
/// log is only evidence once the same harness is shown to record.
#[derive(Clone, Default)]
struct Requests(Arc<Mutex<Vec<Vec<u8>>>>);

impl Requests {
    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    fn wire_text(&self) -> String {
        let all = self.0.lock().unwrap();
        all.iter()
            .map(|raw| String::from_utf8_lossy(raw).into_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// A live loopback server answering every request with the fixture for that
/// connection, while recording the raw wire bytes it received.
struct Server {
    /// The `POST /decisions` URL a caller hands the client.
    endpoint: String,
    /// The listener's loopback address, for synthesizing a redirect `Location`.
    addr: SocketAddr,
    requests: Requests,
}

/// Read one complete HTTP/1.1 request: headers, then exactly `Content-Length`
/// bytes of body. Returns the raw wire bytes.
fn read_full_request(stream: &mut TcpStream) -> Vec<u8> {
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
    raw
}

fn spawn_server<F>(fixture: F) -> Server
where
    F: Fn() -> (String, Vec<(String, String)>, String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let base = format!("http://{addr}");
    let requests = Requests::default();
    let thread_requests = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let raw = read_full_request(&mut stream);
            thread_requests.0.lock().unwrap().push(raw);
            let (status, headers, body) = fixture();
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
    Server {
        endpoint: format!("{base}/decisions"),
        addr,
        requests,
    }
}

/// Drive one plain request through `server` and read its response, proving the
/// listener is live and the harness records a request. The caller clears the
/// log afterwards, so the probe's own connection can never mask a follow-up.
fn probe(server: &Server) {
    let mut stream = TcpStream::connect(server.addr).expect("the probe listener accepts");
    let request = format!(
        "GET /probe HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        server.addr
    );
    stream
        .write_all(request.as_bytes())
        .expect("probe request writes");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    assert_eq!(
        server.requests.count(),
        1,
        "the recording harness must record the probe request"
    );
    server.requests.clear();
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
        instructions: "How close is Ace?".to_string(),
        state_keys: vec!["met_ace".to_string()],
        criteria: Criteria::Score(vec!["stranger".to_string(), "friend".to_string()]),
        fallback: Fallback::Index(0),
    }
}

fn state() -> HashMap<String, StateValue> {
    HashMap::from([("met_ace".to_string(), StateValue::Bool(true))])
}

/// Funnel a fallback through the app's real failure table, so the
/// `unmei::failure` event is produced alongside the engine's warn. Returns the
/// Notice text the failure produced, if any, for the supported-value check.
fn report_fallback(resolved: &ResolvedDecision) -> Option<String> {
    let reason = resolved.fallback_reason.as_ref()?;
    let mut notice = Notice::default();
    let mut hold = DefensiveHold::default();
    report(
        Failure::new(
            FailureRow::for_decision_reason(reason),
            format!("`{}`: {reason}", resolved.action),
        ),
        &mut notice,
        &mut hold,
    );
    notice.text
}

/// The choice-config decision call every credential case drives.
async fn decide_choice(client: &DecisionsClient) -> ResolvedDecision {
    client
        .decide_with_fallback("m", "greet", &choice_config(), &state())
        .await
}

/// Drain up to 128 pending capture lines per call into retained history.
fn drain(app: &mut App) {
    app.world_mut()
        .run_system_once(ingest_capture)
        .expect("the ingest system runs");
}

/// The supported diagnostic values a case returns may carry no uncontrolled
/// response-derived text: the fallback reason and the `Debug` rendering are the
/// only data that leaves the client, and the failure Notice mirrors the reason.
fn assert_supported_values_are_clean(
    case: &str,
    resolved: &ResolvedDecision,
    notice: Option<&str>,
) {
    let reason = resolved
        .fallback_reason
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    let rendered = format!("{resolved:?}");
    for (what, text) in [
        ("the fallback reason", reason.as_str()),
        ("the resolved Debug", rendered.as_str()),
        ("the failure Notice", notice.unwrap_or_default()),
    ] {
        assert!(
            !text.contains(KEY),
            "{case}: {what} leaked the dummy credential: {text}"
        );
        assert!(
            !text.contains("token="),
            "{case}: {what} leaked a redirect query: {text}"
        );
    }
}

/// A categorical diagnostic event a fixture case must have produced: its target
/// and a message fragment unique to that case. Requiring it in all three sinks
/// turns the credential-absence checks from "nothing was captured" into "the
/// relevant event was captured, and is clean".
struct Expected {
    case: String,
    target: &'static str,
    needle: String,
}

impl Expected {
    fn new(case: &str, target: &'static str, needle: impl Into<String>) -> Self {
        Expected {
            case: case.to_string(),
            target,
            needle: needle.into(),
        }
    }
}

/// Register the paired `unmei::decision` warn and `unmei::failure` row a
/// fallback case produces, which carry the same categorical reason text.
fn expect_both(expected: &mut Vec<Expected>, case: &str, needle: &str) {
    expected.push(Expected::new(case, "unmei::decision", needle));
    expected.push(Expected::new(case, "unmei::failure", needle));
}

#[tokio::test]
async fn no_supported_diagnostic_reaches_any_sink() {
    let dir = tempfile::tempdir().expect("temp dir");
    let log_path = dir.path().join("pack.log");
    let file: Arc<Mutex<File>> = Arc::new(Mutex::new(File::create(&log_path).expect("create log")));

    let console = SharedBuffer::new();
    let capture = MonitorCapture::install(Arc::new(SystemClock::new()));

    // The real `<pack-dir>.log` layer, the **real Monitor capture layer**, a
    // console-format layer, all under one permissive TRACE filter. The boxed
    // layers compose as one `Vec` before the concrete ones so the composition
    // stays a `Subscriber`.
    let boxed = vec![file_layer(Arc::clone(&file)), capture_layer(&capture)];
    let subscriber = tracing_subscriber::registry()
        .with(boxed)
        .with(EnvFilter::new("trace"))
        .with(fmt::layer().with_ansi(false).with_writer(console.clone()));
    tracing::subscriber::set_global_default(subscriber).expect("subscriber set once");
    tracing_log::LogTracer::init().expect("log bridge set once");

    let mut monitor = App::new();
    monitor
        .insert_resource(capture)
        .init_resource::<MonitorHistory>();

    // Every categorical event the fixtures below must have admitted, checked in
    // every sink once the exchange is done.
    let mut expected: Vec<Expected> = Vec::new();

    // (1) A successful response whose model exactly matches the requested pin.
    let body = r#"{"model":"m","answers":{"greet":{"type":"choice","choice":"warm"}}}"#.to_string();
    let origin = spawn_server(move || ("200 OK".to_string(), Vec::new(), body.clone()));
    let client = DecisionsClient::new(origin.endpoint.clone(), KEY);
    let resolved = decide_choice(&client).await;
    assert_eq!(resolved.action, "warm");
    assert!(!resolved.is_fallback());
    assert_eq!(
        resolved.model.as_deref(),
        Some("m"),
        "an exact pin match succeeds"
    );
    assert_supported_values_are_clean("exact pin", &resolved, None);
    expected.push(Expected::new(
        "exact pin",
        "unmei::decision",
        "requested model `m` response model `m`",
    ));
    drain(&mut monitor);

    // (2) A successful response reflecting the key as its model: provenance
    // collapses to the fixed marker.
    let body =
        format!(r#"{{"model":"{KEY}","answers":{{"greet":{{"type":"choice","choice":"warm"}}}}}}"#);
    let origin = spawn_server(move || ("200 OK".to_string(), Vec::new(), body.clone()));
    let client = DecisionsClient::new(origin.endpoint.clone(), KEY);
    let resolved = decide_choice(&client).await;
    assert_eq!(resolved.action, "warm");
    assert_eq!(
        resolved.model.as_deref(),
        Some(UNVERIFIED_MODEL),
        "a mismatched echoed model is the fixed marker"
    );
    assert_supported_values_are_clean("reflected model", &resolved, None);
    expected.push(Expected::new(
        "reflected model",
        "unmei::decision",
        "response model `unverified response model`",
    ));
    drain(&mut monitor);

    // (3) A non-2xx response reflecting the key: categorical status, fallback.
    let origin = spawn_server(|| {
        (
            "500 Internal Server Error".to_string(),
            Vec::new(),
            format!("upstream reflected {KEY}"),
        )
    });
    let client = DecisionsClient::new(origin.endpoint.clone(), KEY);
    let resolved = decide_choice(&client).await;
    assert!(resolved.is_fallback());
    assert_eq!(resolved.model, None, "a fallback carries no model");
    let notice = report_fallback(&resolved);
    assert_supported_values_are_clean("non-2xx body", &resolved, notice.as_deref());
    expect_both(
        &mut expected,
        "non-2xx body",
        "Decisions API returned HTTP 500",
    );
    drain(&mut monitor);

    // (4) A malformed body quoting the key in the decoder error.
    let origin = spawn_server(|| {
        (
            "200 OK".to_string(),
            Vec::new(),
            format!(r#"{{"model":"m","answers":{{"greet":{{"type":"{KEY}"}}}}}}"#),
        )
    });
    let client = DecisionsClient::new(origin.endpoint.clone(), KEY);
    let resolved = decide_choice(&client).await;
    assert!(resolved.is_fallback(), "the decoder failure falls back");
    let notice = report_fallback(&resolved);
    assert_supported_values_are_clean("decoder failure", &resolved, notice.as_deref());
    expect_both(
        &mut expected,
        "decoder failure",
        "invalid Decisions response JSON",
    );
    drain(&mut monitor);

    // (5) A valid body naming an action outside the criteria: the model-chosen
    // action text is never echoed.
    let origin = spawn_server(|| {
        (
            "200 OK".to_string(),
            Vec::new(),
            format!(
                r#"{{"model":"m","answers":{{"greet":{{"type":"choice","choice":"{KEY}"}}}}}}"#
            ),
        )
    });
    let client = DecisionsClient::new(origin.endpoint.clone(), KEY);
    let resolved = decide_choice(&client).await;
    assert!(resolved.is_fallback(), "an unknown action falls back");
    let notice = report_fallback(&resolved);
    assert_supported_values_are_clean("invalid action", &resolved, notice.as_deref());
    expect_both(
        &mut expected,
        "invalid action",
        "model chose an action outside the criteria list",
    );
    drain(&mut monitor);

    // (6) A score answer echoing the key in its legend: the network client
    // clears the echoed legend, keeping only the authored one.
    let origin = spawn_server(|| {
        (
            "200 OK".to_string(),
            Vec::new(),
            format!(
                r#"{{"model":"m","answers":{{"greet":{{"type":"score","score":0.7,"legend":{{"0":"{KEY}"}},"probabilities":{{"0":0.9,"1":0.1}}}}}}}}"#
            ),
        )
    });
    let client = DecisionsClient::new(origin.endpoint.clone(), KEY);
    let decision = client
        .decide("m", "greet", &score_config(), &state())
        .await
        .expect("the score response parses");
    assert!(
        decision.legend.is_none(),
        "the network client clears the response-echoed legend"
    );
    assert_eq!(
        decision.score,
        Some(0.7),
        "authored numeric score provenance is unchanged"
    );
    let resolved = client
        .decide_with_fallback("m", "greet", &score_config(), &state())
        .await;
    assert_eq!(
        resolved.action, "0",
        "score lands on the highest-probability index"
    );
    assert!(!resolved.is_fallback());
    assert_supported_values_are_clean("echoed legend", &resolved, None);
    expected.push(Expected::new(
        "echoed legend",
        "unmei::decision",
        "authored legend",
    ));
    drain(&mut monitor);

    // (7) A transport failure against a refused port: categorical diagnostics
    // only, no endpoint or key text — even though the endpoint URL itself is
    // credential-bearing.
    let dead = TcpListener::bind("127.0.0.1:0").expect("bind a dead port");
    let dead_endpoint = format!("http://{}/decisions/{KEY}", dead.local_addr().unwrap());
    drop(dead);
    let client = DecisionsClient::new(dead_endpoint, KEY);
    let resolved = decide_choice(&client).await;
    assert!(resolved.is_fallback(), "a transport failure falls back");
    let notice = report_fallback(&resolved);
    assert_supported_values_are_clean("transport failure", &resolved, notice.as_deref());
    expect_both(&mut expected, "transport failure", "HTTP request failed");
    drain(&mut monitor);

    // (8) 301/302/307/308 redirects whose `Location` — a syntactically valid
    // absolute loopback URL reflecting the key in its authority (userinfo),
    // path and query — names a **distinct, live** destination listener, and
    // whose body reflects the key too: never followed, never read, never
    // logged, with no forwarded header or Referer at the destination.
    let destination = spawn_server(|| ("200 OK".to_string(), Vec::new(), "{}".to_string()));
    // Show the destination listener is live and the harness records a request,
    // so the empty destination log below is evidence and not a dead socket.
    probe(&destination);
    assert_eq!(destination.requests.count(), 0);

    for status in [301u16, 302, 307, 308] {
        let location = format!("http://{KEY}@{}/{KEY}/next?token={KEY}", destination.addr);
        // The fixture must be a real redirect target: an absolute HTTP URL
        // naming the destination listener, with the dummy key reflected in its
        // authority, path and query. A malformed Location would be a trivial
        // non-follow rather than the exercised case.
        let parsed = url::Url::parse(&location).expect("the redirect Location parses");
        assert_eq!(parsed.scheme(), "http");
        assert_eq!(
            parsed.username(),
            KEY,
            "authority userinfo reflects the key"
        );
        assert_eq!(parsed.host_str(), Some("127.0.0.1"));
        assert_eq!(parsed.port(), Some(destination.addr.port()));
        assert!(parsed.path().contains(KEY), "path reflects the key");
        assert!(
            parsed.query().is_some_and(|query| query.contains(KEY)),
            "query reflects the key"
        );

        let loc = location.clone();
        let origin = spawn_server(move || {
            (
                format!("{status} Redirect"),
                vec![("Location".to_string(), loc.clone())],
                format!("redirecting with {KEY}"),
            )
        });
        let client = DecisionsClient::new(origin.endpoint.clone(), KEY);
        let resolved = decide_choice(&client).await;
        assert!(resolved.is_fallback(), "a {status} takes the fallback");
        assert_eq!(
            resolved.action, "cold",
            "a {status} keeps the authored fallback landing"
        );
        assert_eq!(
            resolved.fallback_reason,
            Some(DecisionError::Status { status }),
            "a {status} is a categorical status failure"
        );
        let notice = report_fallback(&resolved);
        assert_supported_values_are_clean(
            &format!("{status} redirect"),
            &resolved,
            notice.as_deref(),
        );

        // The origin saw exactly one request, carrying the (sensitive) bearer
        // The origin saw exactly one request carrying the sensitive bearer
        // header — no retry, and the credential only ever went to the configured
        // endpoint.
        assert_eq!(
            origin.requests.count(),
            1,
            "a {status} must make exactly one request and no retry"
        );
        let origin_wire = origin.requests.wire_text().to_ascii_lowercase();
        assert!(
            origin_wire.contains(&format!("authorization: bearer {KEY}")),
            "the configured endpoint receives the bearer credential: {origin_wire}"
        );

        // The distinct destination listener received no follow-up connection or
        // request at all. That is the Referer and forwarded-Authorization
        // evidence: reqwest only builds a `Referer` on a followed hop, and only
        // a second request could carry either header onward.
        assert_eq!(
            destination.requests.count(),
            0,
            "a {status} reached the destination listener: {}",
            destination.requests.wire_text()
        );

        expect_both(
            &mut expected,
            &format!("{status} redirect"),
            &format!("Decisions API returned HTTP {status}"),
        );
        drain(&mut monitor);
    }

    // Drain everything the fixtures produced; the ingestion path is capped per
    // update, so loop until the pending queue is empty.
    for _ in 0..16 {
        drain(&mut monitor);
    }

    let console_text = console.text();
    let file_text = std::fs::read_to_string(&log_path).expect("read the session log");
    let counters = monitor.world().resource::<MonitorCapture>().counters();
    let history = monitor.world().resource::<MonitorHistory>();
    let retained: Vec<(String, String)> = history
        .lines()
        .iter()
        .map(|line| (line.target.clone(), line.text.clone()))
        .collect();

    // The capture genuinely admitted everything the fixtures produced: no
    // discard, eviction or truncation could explain a clean absence check.
    // Zero is required rather than merely expected — the pending cap is 256
    // slots and retention 1,000 lines, and this fixture pack is far below both,
    // so any nonzero counter means an event the proof needs was dropped from
    // the retained output and the credential absence is no longer evidence.
    assert_eq!(
        counters.lost, 0,
        "a discarded capture event would make the absence checks vacuous"
    );
    assert_eq!(counters.evicted, 0, "retained history evicted a line");
    assert_eq!(counters.truncated, 0, "a retained line was truncated");
    assert!(
        !retained.is_empty(),
        "the real Monitor capture admitted nothing at all"
    );

    // Each intended categorical event reached each exercised sink. The default
    // view's exact `unmei::` targets are what the metadata projection uses, so
    // the retained event's target must be the original one.
    for want in &expected {
        let in_monitor = retained
            .iter()
            .any(|(target, text)| target == want.target && text.contains(&want.needle));
        assert!(
            in_monitor,
            "Monitor retained output never admitted {} (`{}`): {retained:?}",
            want.case, want.needle
        );
        for (sink, text) in [("console", &console_text), ("file", &file_text)] {
            assert!(
                text.contains(want.target) && text.contains(&want.needle),
                "{sink} never received {} (`{}`): {text}",
                want.case,
                want.needle
            );
        }
    }

    // The subscriber is genuinely permissive and genuinely wired: reqwest's
    // known DEBUG origin line is admitted (which is why wire verbosity off is
    // the thing keeping the credential out).
    assert!(
        console_text.contains("starting new connection"),
        "TRACE filter did not admit reqwest's origin DEBUG line: {console_text}"
    );

    // The capture recovers each bridged `log!` record's original target, not
    // `tracing-log`'s `"log"` bridge target, so the `unmei::` projection
    // metadata is exact.
    let targets: Vec<&str> = history
        .lines()
        .iter()
        .map(|line| line.target.as_str())
        .collect();
    assert!(targets.contains(&"unmei::decision"), "{targets:?}");
    assert!(targets.contains(&"unmei::failure"), "{targets:?}");
    assert!(
        !targets.contains(&"log"),
        "the real log target must be recovered: {targets:?}"
    );

    // No supported sink retained the dummy credential or any reflected
    // redirect target, query or body text.
    for (sink, text) in [
        ("console", &console_text),
        ("monitor", &history.text()),
        ("file", &file_text),
    ] {
        assert!(
            !text.contains(KEY),
            "{sink} leaked the dummy credential: {text}"
        );
        assert!(
            !text.contains("token="),
            "{sink} leaked the redirect query: {text}"
        );
        assert!(
            !text.contains("redirecting with"),
            "{sink} leaked the redirect body: {text}"
        );
        assert!(
            !text.contains("upstream reflected"),
            "{sink} leaked a reflected body: {text}"
        );
    }

    // Through request completion and past every sink assertion, the distinct
    // destination listener still holds no request at all.
    assert_eq!(
        destination.requests.count(),
        0,
        "the destination listener received a follow-up request: {}",
        destination.requests.wire_text()
    );
}
