//! Offline delegation test: [`DecisionResolver::resolve`] must equal
//! [`DecisionsClient::decide_with_fallback`] for the same answer and for the
//! same error — asserted, not assumed.
//!
//! A throwaway loopback HTTP server stands in for OpenRouter, so this runs
//! in-process with no key and no real network. Gated on the `network` feature
//! like the live smoke test, but never `#[ignore]`d.
//!
//! The requested pin is `m`; the echoed response model is restricted to it on an
//! exact match, otherwise [`unmei_engine::UNVERIFIED_MODEL`].
#![cfg(feature = "network")]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use unmei_engine::{
    Criteria, DecisionConfig, DecisionResolver, DecisionsClient, Fallback, QuestionType,
    ResolvedDecision, StateValue, UNVERIFIED_MODEL,
};

fn config() -> DecisionConfig {
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

fn state() -> HashMap<String, StateValue> {
    HashMap::from([("met_ace".to_string(), StateValue::Bool(true))])
}

/// Read one complete HTTP/1.1 request: the headers, then exactly
/// `Content-Length` bytes of body. A single `read` may return only part of the
/// request, and answering over the client's unsent bytes is a reset on the
/// wire, which the client reports as `error sending request`.
fn read_full_request(stream: &mut std::net::TcpStream) -> String {
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
    String::from_utf8_lossy(&raw).to_string()
}

/// A throwaway HTTP server answering every request with the same status line
/// and body, so the two call paths can be compared against one canned response.
fn serve(status_line: &'static str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let _ = read_full_request(&mut stream);
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    format!("http://{addr}/decisions")
}

/// Drive both the trait method and the concrete method against the same canned
/// response and assert they agree, then hand the resolution back for the
/// caller's outcome-specific checks.
async fn resolve_both(status_line: &'static str, body: &'static str) -> ResolvedDecision {
    let client = DecisionsClient::new(serve(status_line, body), "test-key");
    let via_trait = client.resolve("m", "greet", &config(), &state()).await;
    let direct = client
        .decide_with_fallback("m", "greet", &config(), &state())
        .await;
    assert_eq!(
        via_trait, direct,
        "resolve must mirror decide_with_fallback exactly"
    );
    via_trait
}

const ANSWER_BODY: &str = r#"{"model":"fake/model","answers":{"greet":{"type":"choice","choice":"warm","confidence":0.7}}}"#;

#[tokio::test]
async fn resolve_equals_decide_with_fallback_for_a_model_answer() {
    // The echoed model does not match the requested pin `m`, so provenance
    // collapses to the fixed marker rather than carrying response text.
    let resolved = resolve_both("200 OK", ANSWER_BODY).await;
    assert_eq!(resolved.action, "warm");
    assert_eq!(resolved.model.as_deref(), Some(UNVERIFIED_MODEL));
    assert!(!resolved.is_fallback());
}

#[tokio::test]
async fn resolve_equals_decide_with_fallback_when_the_echoed_model_matches() {
    const MATCHING_BODY: &str =
        r#"{"model":"m","answers":{"greet":{"type":"choice","choice":"warm"}}}"#;
    let resolved = resolve_both("200 OK", MATCHING_BODY).await;
    assert_eq!(resolved.action, "warm");
    assert_eq!(resolved.model.as_deref(), Some("m"));
    assert!(!resolved.is_fallback());
}

#[tokio::test]
async fn resolve_equals_decide_with_fallback_for_an_error() {
    let resolved = resolve_both("500 Internal Server Error", "upstream boom").await;
    assert_eq!(resolved.action, "cold");
    assert!(resolved.is_fallback());
    assert!(resolved.fallback_reason.is_some());
}
