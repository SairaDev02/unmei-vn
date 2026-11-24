//! `reqwest` client for NanoGPT's `POST /api/v1/decisions` (feature `network`).
//!
//! The client is a thin transport over the pure wire layer in [`super`]: build
//! the request from the author config + filtered state, POST it, validate the
//! body, and log the requested model pin (never the response-controlled echo).
//! All failures are [`DecisionError`]s, which the caller funnels into
//! [`super::resolve_or_fallback`].
//!
//! Credential safety is a hard contract here: diagnostics are
//! categorical, the API key is a sensitive bearer header, wire verbosity stays
//! off and redirects are never followed.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Duration;

use crate::content::{DecisionConfig, StateValue};

use super::{
    build_decision_request, decision_from_body, is_openrouter_key, resolve_or_fallback,
    verified_model, Decision, DecisionError, DecisionRequest, DecisionResolver, ResolvedDecision,
    DEFAULT_ENDPOINT,
};

/// Environment variable holding the NanoGPT API key. A non-empty value is
/// the **override** of the runtime's user-level credential file ([ADR 0010]);
/// the `unmei` CLI keeps its env-only contract for 0.1.0.
///
/// [ADR 0010]: ../../../docs/adr/0010-credentials-live-in-a-user-level-file.md
pub const API_KEY_ENV: &str = "NANOGPT_API_KEY";

/// Optional endpoint override (unsupported escape hatch to another compatible
/// endpoint; an OpenRouter key is still never sent).
pub const ENDPOINT_ENV: &str = "UNMEI_DECISIONS_URL";

/// Bounded request timeout: a hung endpoint must never hang the caller
/// (the fallback posture needs a failure, not a stall).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A NanoGPT Decisions HTTP client. Cloneable via the shared `reqwest::Client`.
///
/// Requests run on a process-lifetime Tokio runtime (see `shared_runtime`),
/// because `reqwest` needs a reactor and callers are not necessarily Tokio — the
/// Bevy runtime drives its async wait from a compute task pool. Owning a runtime
/// per client would instead panic when one is dropped inside an async context.
#[derive(Clone)]
pub struct DecisionsClient {
    endpoint: String,
    api_key: String,
    http: reqwest::Client,
}

impl std::fmt::Debug for DecisionsClient {
    /// Deliberately opaque: the endpoint and the API key are sensitive, and the
    /// HTTP client's internals are client configuration. A future log site must
    /// not be able to render any of them.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionsClient").finish_non_exhaustive()
    }
}

impl DecisionsClient {
    /// Build a client against an explicit endpoint (tests, self-hosted proxies).
    pub fn new(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        DecisionsClient {
            endpoint: endpoint.into(),
            api_key: api_key.into(),
            http: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                // A permissive `RUST_LOG` must never authorize a raw wire dump:
                // reqwest's serialized bytes contain the Authorization header.
                // Enabling this (or hyper/h2 tracing) requires renewed
                // credential-safety validation.
                .connection_verbose(false)
                // Q16: a redirect's `Location` is response-controlled and can
                // reflect the credential into the next connection's logged
                // origin. Never follow one.
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("static reqwest client config always builds"),
        }
    }

    /// Build from the environment: `NANOGPT_API_KEY` (required) and an
    /// optional `UNMEI_DECISIONS_URL` override. Fails before any network call
    /// when the key is missing, naming exactly what to set.
    pub fn from_env() -> Result<Self, DecisionError> {
        let api_key = std::env::var(API_KEY_ENV).map_err(|_| {
            DecisionError::Config(format!(
                "{API_KEY_ENV} is not set — export it before making decision calls"
            ))
        })?;
        let endpoint = std::env::var(ENDPOINT_ENV).unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        Ok(DecisionsClient::new(endpoint, api_key))
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Ask one decision question and return the validated landing key.
    ///
    /// The question's shape follows `config.question_type` (ADR 0005): `choice`
    /// lands on the chosen action id, `noul` on `true`/`false`, `score` on the
    /// legend index with the highest probability.
    ///
    /// `model` is the pinned decision model (per-NPC config in content). The
    /// full `state` map is filtered to `config.state_keys` before sending.
    ///
    /// The returned `Decision.model` is `model` only when the response's echoed
    /// model matched it exactly; otherwise it is
    /// [`UNVERIFIED_MODEL`](super::UNVERIFIED_MODEL). `Decision.legend` is always
    /// `None` for a network result.
    pub async fn decide(
        &self,
        model: &str,
        question_id: &str,
        config: &DecisionConfig,
        state: &HashMap<String, StateValue>,
    ) -> Result<Decision, DecisionError> {
        // Defense in depth (`docs/nanogpt.md` N10): an OpenRouter-format key is
        // never sent, whatever the endpoint is. Refused before any network.
        if is_openrouter_key(&self.api_key) {
            return Err(DecisionError::OpenRouterKey);
        }
        let request = build_decision_request(model, question_id, config, state);
        // The call runs on the shared runtime: `reqwest` needs a Tokio reactor,
        // which a caller's executor (e.g. Bevy's task pool) may not be.
        let task = shared_runtime().spawn(send_decision(
            self.http.clone(),
            self.endpoint.clone(),
            self.api_key.clone(),
            request,
            question_id.to_string(),
            config.clone(),
        ));
        task.await.map_err(|_| DecisionError::Task)?
    }

    /// [`decide`](Self::decide) with the author-fallback posture applied, and a
    /// warn log whenever the fallback fires — so a model quirk, a bad answer, or
    /// a missing key is never silent. This is the call the runtime makes.
    pub async fn decide_with_fallback(
        &self,
        model: &str,
        question_id: &str,
        config: &DecisionConfig,
        state: &HashMap<String, StateValue>,
    ) -> ResolvedDecision {
        let resolved =
            resolve_or_fallback(config, self.decide(model, question_id, config, state).await);
        if let Some(reason) = &resolved.fallback_reason {
            log::warn!(
                target: "unmei::decision",
                "decision `{question_id}` fell back to `{}`: {reason}",
                resolved.action,
            );
        }
        resolved
    }
}

impl DecisionResolver for DecisionsClient {
    /// Delegates to [`DecisionsClient::decide_with_fallback`] — one fallback
    /// path, and exactly the call the app makes through the concrete client.
    fn resolve<'a>(
        &'a self,
        model: &'a str,
        question_id: &'a str,
        config: &'a DecisionConfig,
        state: &'a HashMap<String, StateValue>,
    ) -> Pin<Box<dyn Future<Output = ResolvedDecision> + Send + 'a>> {
        Box::pin(self.decide_with_fallback(model, question_id, config, state))
    }
}

/// Build one `POST` decisions request builder. Kept separate from
/// [`send_decision`] so a test can inspect the constructed request (the bearer
/// header must be marked sensitive).
fn decision_request(
    http: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    request: &DecisionRequest,
) -> reqwest::RequestBuilder {
    // `bearer_auth` builds a sensitive `HeaderValue`, so the typed header
    // renders as `Sensitive` rather than the key.
    http.post(endpoint).bearer_auth(api_key).json(request)
}

/// One decisions `POST`, validated. Runs on the shared runtime, so
/// everything it needs is owned rather than borrowed.
async fn send_decision(
    http: reqwest::Client,
    endpoint: String,
    api_key: String,
    request: DecisionRequest,
    question_id: String,
    config: DecisionConfig,
) -> Result<Decision, DecisionError> {
    let response = decision_request(&http, &endpoint, &api_key, &request)
        .send()
        .await
        .map_err(|_| DecisionError::Http)?;

    let status = response.status();
    // A non-success body is response-controlled and can reflect the credential,
    // so it is never read or retained. Only the fixed status code is diagnostic.
    if !status.is_success() {
        return Err(DecisionError::Status {
            status: status.as_u16(),
        });
    }
    let body = response.text().await.map_err(|_| DecisionError::Http)?;

    let mut decision = decision_from_body(&body, &question_id, &config)?;

    // A response `model` is uncontrolled text that could reflect a credential;
    // restrict it to the requested pin, and never log the request-controlled
    // echoed legend.
    let requested = request.model.as_str();
    let responded = verified_model(requested, &decision.model);
    decision.model = responded.clone();
    decision.legend = None;

    let cost = decision
        .cost_usd
        .map(|c| format!(" (${c:.8})"))
        .unwrap_or_default();
    // `score` provenance only: the scalar is numeric and safe; the legend
    // detail comes from the authored config and is explicitly labelled as such.
    let score = decision
        .score
        .map(|score| {
            let authored = config
                .score_criteria()
                .map(|levels| format!(", authored legend {levels:?}"))
                .unwrap_or_default();
            format!(" [score {score}{authored}]")
        })
        .unwrap_or_default();
    log::info!(
        target: "unmei::decision",
        "decision `{question_id}` -> `{}` requested model `{requested}` response model `{responded}`{cost}{score}",
        decision.action,
    );
    Ok(decision)
}

/// The process-lifetime Tokio runtime Decisions requests run on.
///
/// Shared rather than per-client: `reqwest` needs a reactor, callers may be on
/// any executor (Bevy's compute task pool is not one), and a runtime dropped
/// inside an async context panics — a `'static` one is never dropped at all.
pub(super) fn shared_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("unmei-decisions")
            .build()
            .expect("static tokio runtime config always builds")
    })
}

mod key_check;

pub use key_check::HttpKeyCheck;

#[cfg(test)]
mod tests {
    use super::key_check::{key_check_request, send_key_check};
    use super::*;
    use crate::content::{Criteria, Fallback, QuestionType};
    use crate::decision::{KeyCheck, KeyVerdict, KEY_ENDPOINT};
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const DUMMY_KEY: &str = "sk-dummy-117-credential";

    fn decision_config() -> DecisionConfig {
        DecisionConfig {
            question_type: QuestionType::Choice,
            instructions: "q".to_string(),
            state_keys: Vec::new(),
            criteria: Criteria::Choice(HashMap::from([(
                "cold".to_string(),
                "guarded".to_string(),
            )])),
            fallback: Fallback::Action("cold".to_string()),
        }
    }

    /// The relevant client/configuration representations must not render the
    /// dummy key, the credential-bearing endpoint or the HTTP internals.
    #[test]
    fn debug_is_opaque_for_both_clients() {
        let client = DecisionsClient::new("http://127.0.0.1:9/decisions?token=DUMMY", DUMMY_KEY);
        let rendered = format!("{client:?}");
        assert!(rendered.contains("DecisionsClient"), "{rendered}");
        assert!(!rendered.contains(DUMMY_KEY), "{rendered}");
        assert!(!rendered.contains("127.0.0.1"), "{rendered}");
        assert!(!rendered.contains("token"), "{rendered}");

        let rendered = format!("{:?}", HttpKeyCheck::new());
        assert!(rendered.contains("HttpKeyCheck"), "{rendered}");
        assert!(!rendered.contains("reqwest"), "{rendered}");
    }

    /// Both clients build their Authorization header through `bearer_auth`, so
    /// the typed value is marked sensitive rather than renderable.
    #[test]
    fn bearer_headers_are_constructed_sensitive() {
        let http = reqwest::Client::new();
        let request = build_decision_request("m", "q", &decision_config(), &HashMap::new());
        let built = decision_request(&http, "http://127.0.0.1:9/decisions", DUMMY_KEY, &request)
            .build()
            .expect("builds");
        assert!(
            built.headers()[reqwest::header::AUTHORIZATION].is_sensitive(),
            "decision bearer header must be sensitive"
        );

        let built = key_check_request(&http, "http://127.0.0.1:9/key", DUMMY_KEY)
            .build()
            .expect("builds");
        assert!(
            built.headers()[reqwest::header::AUTHORIZATION].is_sensitive(),
            "key-check bearer header must be sensitive"
        );
    }

    /// Read one complete HTTP/1.1 request: headers, then exactly
    /// `Content-Length` bytes of body.
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

    /// The key-check client never follows a `3xx`: the fixed status mapping is
    /// the whole exchange, and the reflected `Location`/body are never read.
    ///
    /// This is the **direct** `send_key_check` proof. The production key-check
    /// URL is fixed and not overridable, so `HttpKeyCheck` has no
    /// app-subscriber loopback seam, and Monitor's real-sink TRACE proof is
    /// deliberately **not** claimed for it; no endpoint-injection
    /// feature is added to manufacture one.
    #[tokio::test]
    async fn key_check_does_not_follow_redirects() {
        for status_line in [
            "301 Moved Permanently",
            "302 Found",
            "307 Temporary Redirect",
            "308 Permanent Redirect",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let addr = listener.local_addr().expect("local addr");
            let count = Arc::new(AtomicUsize::new(0));
            let count2 = Arc::clone(&count);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    count2.fetch_add(1, Ordering::SeqCst);
                    read_full_request(&mut stream);
                    let location = format!("http://{DUMMY_KEY}@{addr}/next?k={DUMMY_KEY}");
                    let body = format!("reflects {DUMMY_KEY}");
                    let response = format!(
                        "HTTP/1.1 {status_line}\r\nLocation: {location}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
            });

            let checker = HttpKeyCheck::new();
            let verdict = send_key_check(
                checker.http.clone(),
                &format!("http://{addr}/key"),
                DUMMY_KEY.to_string(),
            )
            .await;
            assert_eq!(verdict, KeyVerdict::Unreachable, "{status_line}");
            // A follow would land back on this same listener and bump the count.
            assert_eq!(count.load(Ordering::SeqCst), 1, "{status_line}");
        }
    }

    /// A loopback listener that counts connections, so a test can prove nothing
    /// was sent.
    fn counting_listener() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = Arc::clone(&count);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                count2.fetch_add(1, Ordering::SeqCst);
                read_full_request(&mut stream);
                let _ = stream.write_all(b"HTTP/1.1 500 X\r\nContent-Length: 0\r\n\r\n");
            }
        });
        (format!("http://{addr}/"), count)
    }

    /// N10: an `sk-or-` key never reaches the network, and takes the fallback.
    #[tokio::test]
    async fn an_openrouter_key_is_never_sent_by_the_decisions_client() {
        let (url, count) = counting_listener();
        let client = DecisionsClient::new(url, "sk-or-v1-dummy");
        let err = client
            .decide("m", "q", &decision_config(), &HashMap::new())
            .await
            .unwrap_err();
        assert_eq!(err, DecisionError::OpenRouterKey);
        let resolved = client
            .decide_with_fallback("m", "q", &decision_config(), &HashMap::new())
            .await;
        assert!(resolved.is_fallback());
        assert_eq!(resolved.action, "cold");
        assert_eq!(count.load(Ordering::SeqCst), 0, "no request was made");
    }

    /// N10: the key check answers `OpenRouterKey` without any request.
    #[tokio::test]
    async fn an_openrouter_key_is_never_pinged() {
        let checker = HttpKeyCheck::new();
        assert_eq!(
            checker.check("  sk-or-v1-dummy ").await,
            KeyVerdict::OpenRouterKey
        );
    }

    /// The key-check URL is the NanoGPT `api-keys/current` read and a
    /// non-OpenRouter key is sent to it as a bearer header.
    #[test]
    fn key_check_request_targets_the_nanogpt_key_endpoint() {
        let http = reqwest::Client::new();
        let built = key_check_request(&http, KEY_ENDPOINT, "sk-nano-dummy")
            .build()
            .expect("builds");
        assert_eq!(built.method(), reqwest::Method::GET);
        assert_eq!(
            built.url().as_str(),
            "https://api.nano-gpt.com/api/v1/api-keys/current"
        );
    }
}
