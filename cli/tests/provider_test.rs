//! Tests for provider resolution + the chat seam (author-cli contract §4).

mod common;

use std::path::Path;
use std::sync::Mutex;

use unmei::provider::{
    resolve_provider, AuthorChat, ConfigInput, Message, OpenAiChat, ProviderError,
    StructuredOutput, TransportFailure, DEFAULT_BASE_URL,
};

/// Env vars are process-global: one lock serializes every test that touches a key.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn set_env(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) };
}

fn clear_env(key: &str) {
    unsafe { std::env::remove_var(key) };
}

fn tmp_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn input<'a>(dir: &'a Path, model: Option<&'a str>, base_url: Option<&'a str>) -> ConfigInput<'a> {
    ConfigInput {
        pack_root: dir,
        model,
        base_url,
    }
}

/// RED: pack file config is honored with no flags.
#[test]
fn pack_file_config_is_used() {
    let _env = ENV_LOCK.lock().unwrap();
    set_env("UNMEI_API_KEY", "test-key-999");
    let dir = tmp_dir();
    std::fs::write(
        dir.path().join("unmei.author.ron"),
        r#"( base_url: "http://localhost:1234", model: "author/m1" )"#,
    )
    .unwrap();
    let cfg = resolve_provider(input(dir.path(), None, None)).unwrap();
    assert_eq!(cfg.base_url, "http://localhost:1234");
    assert_eq!(cfg.model, "author/m1");
}

/// RED: `--model` and `--base-url` flags beat the pack file.
#[test]
fn flags_beat_pack_file() {
    let _env = ENV_LOCK.lock().unwrap();
    set_env("UNMEI_API_KEY", "test-key-999");
    let dir = tmp_dir();
    std::fs::write(
        dir.path().join("unmei.author.ron"),
        r#"( base_url: "http://localhost:1234", model: "author/m1" )"#,
    )
    .unwrap();
    let cfg =
        resolve_provider(input(dir.path(), Some("flag/m2"), Some("http://flag:9999"))).unwrap();
    assert_eq!(cfg.base_url, "http://flag:9999");
    assert_eq!(cfg.model, "flag/m2");
}

/// RED: defaults — base_url `https://openrouter.ai/api`; no default model.
#[test]
fn default_base_url_and_no_default_model() {
    let _env = ENV_LOCK.lock().unwrap();
    let dir = tmp_dir();
    assert_eq!(
        resolve_provider(input(dir.path(), None, None)),
        Err("no authoring model set — pass --model <id> or set `model` in unmei.author.ron; the authoring model must serve OpenAI chat/completions (decision models like typesafe/jev reject that endpoint)".to_string())
    );

    set_env("UNMEI_API_KEY", "test-key-999");
    let cfg = resolve_provider(input(dir.path(), Some("author/m1"), None)).unwrap();
    assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
    assert_eq!(cfg.model, "author/m1");
}

/// RED: missing key → clear error before any network call.
#[test]
fn missing_key_is_a_clear_error() {
    let _env = ENV_LOCK.lock().unwrap();
    clear_env("UNMEI_API_KEY");
    clear_env("OPENROUTER_API_KEY");
    let dir = tmp_dir();
    let err = resolve_provider(input(dir.path(), Some("author/m1"), None)).unwrap_err();
    assert!(err.contains("UNMEI_API_KEY"), "message: {err}");
}

/// RED: `UNMEI_API_KEY` falls back to `OPENROUTER_API_KEY`.
#[test]
fn openrouter_key_fallback() {
    let _env = ENV_LOCK.lock().unwrap();
    clear_env("UNMEI_API_KEY");
    set_env("OPENROUTER_API_KEY", "or-key-1");
    let dir = tmp_dir();
    let cfg = resolve_provider(input(dir.path(), Some("author/m1"), None)).unwrap();
    assert_eq!(cfg.api_key, "or-key-1");
}

/// RED: AuthorFile round-trips the contract §4 example.
#[test]
fn author_file_parses_contract_example() {
    let file: unmei::provider::AuthorFile =
        ron::from_str(r#"( base_url: "https://openrouter.ai/api", model: "openai/gpt-4o-mini" )"#)
            .unwrap();
    assert_eq!(file.base_url.as_deref(), Some("https://openrouter.ai/api"));
    assert_eq!(file.model.as_deref(), Some("openai/gpt-4o-mini"));
}

/// RED: comments and omission both parse.
#[test]
fn author_file_allows_comments_and_omitted_fields() {
    let file: unmei::provider::AuthorFile =
        ron::from_str("// provider defaults\n( model: \"author/m1\" )").unwrap();
    assert_eq!(file.base_url, None);
    assert_eq!(file.model.as_deref(), Some("author/m1"));
}

// ---------------------------------------------------------------------------
// The seam object-safety + real-translation shape
// ---------------------------------------------------------------------------

/// RED: the real provider implements the same trait tests inject fakes for.
#[test]
fn openai_chat_completes_the_seam() {
    fn assert_seam(_t: &impl AuthorChat) {}
    assert_seam(&OpenAiChat::new("http://localhost:1", "k"));
}

/// RED: URL translation — trailing slashes are trimmed, `/v1/chat/completions`
/// appended. (Checked through the wire path with a local server.)
#[tokio::test]
async fn openai_chat_posts_openai_wire_shape() {
    // A tiny local HTTP server doubles as the "compatible endpoint".
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = common::read_full_request(&mut stream);
        common::respond(
            &mut stream,
            "200 OK",
            "{\"choices\":[{\"message\":{\"content\":\"hi from fake\"}}]}",
        );
        request
    });

    let chat = OpenAiChat::new(format!("http://{addr}/"), "key-1");
    let content = chat
        .complete("author/m1", &[Message::system("s"), Message::user("u")])
        .await
        .unwrap();
    assert_eq!(content, "hi from fake");

    let request = server.join().unwrap();
    assert!(
        request.starts_with("POST /v1/chat/completions HTTP/1.1"),
        "{request}"
    );
    assert!(request
        .to_lowercase()
        .contains("authorization: bearer key-1"));
    assert!(request.contains("\"model\":\"author/m1\""));
    assert!(request.contains("\"role\":\"system\""));
}

/// RED (ADR 0014): a non-2xx endpoint is a typed `Status` error carrying
/// the categorical status only. The provider body — here a refused URL and the
/// reflected dummy key — never survives into `Display` or `Debug`.
#[tokio::test]
async fn openai_chat_maps_http_status_to_typed_error() {
    let (base, server) = serve_once(
        "401 Unauthorized",
        "no; key dummy-key-abc123 rejected at http://127.0.0.1:9/v1",
    );

    let chat = OpenAiChat::new(base, "dummy-key-abc123");
    let err = chat
        .complete("author/m1", &[Message::user("u")])
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Status { status: 401 });
    assert_absent(&err, &["dummy-key-abc123", "127.0.0.1:9", "rejected"]);
    let _ = server.join();
}

/// One-shot local server returning a fixed status/body; yields the base URL.
fn serve_once(status: &str, body: &str) -> (String, std::thread::JoinHandle<()>) {
    let status = status.to_string();
    let body = body.to_string();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = common::read_full_request(&mut stream);
        common::respond(&mut stream, &status, &body);
    });
    (format!("http://{addr}/"), handle)
}

/// The retained error's rendered `Display` and `Debug` must not carry any of
/// the listed material (a body, a URL, the reflected dummy key).
fn assert_absent(err: &ProviderError, forbidden: &[&str]) {
    let rendered = format!("{err}\n{err:?}");
    for needle in forbidden {
        assert!(
            !rendered.contains(needle),
            "{needle:?} survived in the retained diagnostic: {rendered}"
        );
    }
}

/// A live loopback listener that counts connections until [`Watched::stop`], so
/// a redirect destination's silence is an observation about a reachable address
/// closed only after the client future has returned.
struct Watched {
    addr: std::net::SocketAddr,
    hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: std::thread::JoinHandle<()>,
}

impl Watched {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = hits.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let handle = std::thread::spawn(move || {
            // The safety deadline only keeps a panicking test from hanging the
            // suite; the assertion window ends at `stop`, after the request.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !stopping.load(std::sync::atomic::Ordering::SeqCst)
                && std::time::Instant::now() < deadline
            {
                match listener.accept() {
                    Ok(_) => {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Watched {
            addr,
            hits,
            stop,
            handle,
        }
    }

    /// Close the window and report every connection the destination accepted.
    fn stop(self) -> usize {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.handle.join().unwrap();
        self.hits.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// RED (ADR 0014): a transport failure is one fixed category — the
/// `reqwest` error's own text (which names the URL) does not survive.
#[tokio::test]
async fn transport_failure_is_categorical() {
    // A bound-then-released loopback port: connecting to it is refused. The
    // freed port could in principle be re-acquired by another process, which
    // would make this fail loudly (or hang) rather than pass silently; the
    // assertions below are on the category and the absent material.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = closed.local_addr().unwrap();
    drop(closed);

    let chat = OpenAiChat::new(format!("http://{addr}/"), "dummy-key-abc123");
    let err = chat
        .complete("author/m1", &[Message::user("u")])
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ProviderError::Http(unmei::provider::TransportFailure::Connect)
    );
    assert_absent(&err, &[&addr.to_string(), "dummy-key-abc123"]);
}

/// RED (ADR 0014): a response cut short mid-body is a categorical
/// transport failure — the endpoint's bytes and the URL do not survive.
#[tokio::test]
async fn a_truncated_body_is_categorical() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        use std::io::Write;
        let (mut stream, _) = listener.accept().unwrap();
        let _ = common::read_full_request(&mut stream);
        // Declares 100 bytes, sends 5, then closes.
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK
content-length: 100
Connection: close

dummy",
        );
        let _ = stream.flush();
    });

    let chat = OpenAiChat::new(format!("http://{addr}/"), "dummy-key-abc123");
    let err = chat
        .complete("author/m1", &[Message::user("u")])
        .await
        .unwrap_err();
    // reqwest does not flag hyper's incomplete-message error as a body error,
    // so a cut-short response is the catch-all transport category.
    assert_eq!(err, ProviderError::Http(TransportFailure::Other));
    assert_absent(&err, &[&addr.to_string(), "dummy-key-abc123", "dummy"]);
    let _ = server.join();
}

/// RED (ADR 0014): a 200 response that is not the chat envelope is a
/// fixed decode category; the decoder's text and the body do not survive.
#[tokio::test]
async fn decode_failure_is_categorical() {
    let (base, server) = serve_once("200 OK", "dummy-key-abc123{\"choices\":[{\"message\":");
    let chat = OpenAiChat::new(base, "dummy-key-abc123");
    let err = chat
        .complete("author/m1", &[Message::user("u")])
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ProviderError::Shape(unmei::provider::ShapeProblem::NotJson)
    );
    assert_absent(&err, &["dummy-key-abc123", "column", "choices"]);
    let _ = server.join();
}

/// RED (ADR 0014): an envelope with no `choices[0].message.content` has
/// its own fixed category.
#[tokio::test]
async fn an_empty_choices_array_is_categorical() {
    let (base, server) = serve_once("200 OK", "{\"choices\":[]}");
    let chat = OpenAiChat::new(base, "dummy-key-abc123");
    let err = chat
        .complete("author/m1", &[Message::user("u")])
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ProviderError::Shape(unmei::provider::ShapeProblem::NoContent)
    );
    let _ = server.join();
}

/// RED (ADR 0014): the structured-output degradation records the fixed
/// classification category, never the inspected body, URL or reflected key.
#[tokio::test]
async fn structured_degradation_is_categorical() {
    let (base, server) = serve_once(
        "400 Bad Request",
        "{\"error\":{\"message\":\"response_format is not supported (key dummy-key-abc123) at http://127.0.0.1:9\"}}",
    );
    let chat = OpenAiChat::new(base, "dummy-key-abc123");
    let err = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ProviderError::NoStructuredSupport {
            reason: unmei::provider::StructuredUnsupported::ResponseFormat,
        }
    );
    assert_absent(&err, &["dummy-key-abc123", "127.0.0.1:9", "not supported"]);
    let _ = server.join();
}

/// RED (ADR 0014): a valid redirect to a distinct live loopback
/// destination is not followed — no request reaches the destination, and the
/// 3xx surfaces as an ordinary status failure.
#[tokio::test]
async fn a_redirect_is_not_followed() {
    let target = Watched::start();
    let target_addr = target.addr;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let redirector = std::thread::spawn(move || {
        use std::io::Write;
        let (mut stream, _) = listener.accept().unwrap();
        let _ = common::read_full_request(&mut stream);
        let response = format!(
            "HTTP/1.1 302 Found\r\nlocation: http://{target_addr}/v1/chat/completions\r\ncontent-length: 0\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    let chat = OpenAiChat::new(format!("http://{addr}/"), "dummy-key-abc123");
    let err = chat
        .complete("author/m1", &[Message::user("u")])
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Status { status: 302 });
    assert_absent(&err, &[&target_addr.to_string(), "dummy-key-abc123"]);

    redirector.join().unwrap();
    assert_eq!(
        target.stop(),
        0,
        "a redirect destination must receive no request"
    );
}

/// RED (ADR 0014): the credential-bearing client and config keep an
/// opaque `Debug`.
#[test]
fn credential_bearing_debug_is_opaque() {
    let cfg = unmei::provider::ProviderConfig {
        base_url: "http://127.0.0.1:9".into(),
        model: "author/m1".into(),
        api_key: "dummy-key-abc123".into(),
    };
    let chat = OpenAiChat::new("http://127.0.0.1:9", "dummy-key-abc123");
    for rendered in [format!("{cfg:?}"), format!("{chat:?}")] {
        assert!(
            !rendered.contains("dummy-key-abc123"),
            "the key survived Debug: {rendered}"
        );
        assert!(rendered.contains(".."), "opaque form: {rendered}");
    }
}

fn draft_schema() -> StructuredOutput {
    StructuredOutput {
        name: "unmei_scene_draft".into(),
        schema: serde_json::json!({"type": "object"}),
    }
}

/// RED (contract §7): the primary request carries `response_format`
/// (`json_schema`, `strict`) and `provider.require_parameters: true`.
#[tokio::test]
async fn openai_chat_posts_structured_wire_shape() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = common::read_full_request(&mut stream);
        let body = "{\"choices\":[{\"message\":{\"content\":\"{\\\"LINES\\\":[]}\"}}]}";
        common::respond(&mut stream, "200 OK", body);
        request
    });

    let chat = OpenAiChat::new(format!("http://{addr}/"), "key-1");
    let content = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap();
    assert!(content.contains("LINES"), "{content}");

    let request = server.join().unwrap();
    assert!(request.contains("\"response_format\""), "{request}");
    assert!(request.contains("\"type\":\"json_schema\""), "{request}");
    assert!(
        request.contains("\"name\":\"unmei_scene_draft\""),
        "{request}"
    );
    assert!(request.contains("\"strict\":true"), "{request}");
    assert!(request.contains("\"require_parameters\":true"), "{request}");
}

/// RED (contract §7): a 400 naming `response_format` is the one provider
/// condition classified as "no structured support" for the fallback.
#[tokio::test]
async fn structured_unsupported_is_classified() {
    let (base, server) = serve_once(
        "400 Bad Request",
        r#"{"error":{"message":"response_format is not supported by this endpoint"}}"#,
    );
    let chat = OpenAiChat::new(base, "key-1");
    let err = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ProviderError::NoStructuredSupport {
            reason: unmei::provider::StructuredUnsupported::ResponseFormat,
        }
    );
    assert_absent(&err, &["is not supported by this endpoint"]);
    let _ = server.join();
}

/// RED (contract §7): any other provider failure stays a `Status` error
/// and surfaces instead of triggering the fallback.
#[tokio::test]
async fn other_http_errors_stay_status() {
    let (base, server) = serve_once("500 Internal Server Error", "down");
    let chat = OpenAiChat::new(base, "key-1");
    let err = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Status { status: 500 });
    assert_absent(&err, &["down"]);
    let _ = server.join();
}

/// RED (contract §7): a non-OpenRouter OpenAI-compatible endpoint that
/// rejects the OpenRouter `provider` preferences field also takes the fallback.
#[tokio::test]
async fn provider_parameter_rejection_is_structured_unsupported() {
    let (base, server) = serve_once(
        "400 Bad Request",
        r#"{"error":{"message":"Unrecognized request argument supplied: provider"}}"#,
    );
    let chat = OpenAiChat::new(base, "key-1");
    let err = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ProviderError::NoStructuredSupport {
            reason: unmei::provider::StructuredUnsupported::ProviderArgument,
        }
    );
    assert_absent(&err, &["Unrecognized request argument"]);
    let _ = server.join();
}

/// RED (contract §7): a general provider *configuration*
/// complaint that names no request argument stays a hard `Status` — only an
/// unknown request argument downgrades to the free-text fallback.
#[tokio::test]
async fn provider_config_error_stays_status() {
    let (base, server) = serve_once(
        "400 Bad Request",
        r#"{"error":{"message":"invalid provider preference"}}"#,
    );
    let chat = OpenAiChat::new(base, "key-1");
    let err = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Status { status: 400 });
    assert_absent(&err, &["invalid provider preference"]);
    let _ = server.join();
}

/// RED (contract §7): an unknown-argument rejection that names
/// the `provider` parameter still takes the fallback.
#[tokio::test]
async fn unknown_provider_parameter_is_structured_unsupported() {
    let (base, server) = serve_once(
        "400 Bad Request",
        r#"{"error":{"message":"Unknown parameter: provider"}}"#,
    );
    let chat = OpenAiChat::new(base, "key-1");
    let err = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ProviderError::NoStructuredSupport {
            reason: unmei::provider::StructuredUnsupported::ProviderArgument,
        }
    );
    let _ = server.join();
}

/// RED (contract §7): an unrelated 400 stays a hard `Status` error and
/// does not trigger the free-text fallback.
#[tokio::test]
async fn unrelated_400_stays_status() {
    let (base, server) = serve_once(
        "400 Bad Request",
        r#"{"error":{"message":"model not found"}}"#,
    );
    let chat = OpenAiChat::new(base, "key-1");
    let err = chat
        .complete_structured("author/m1", &[Message::user("u")], &draft_schema())
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Status { status: 400 });
    assert_absent(&err, &["model not found"]);
    let _ = server.join();
}

/// The provider optional-string adapter rejects the legacy `Some("…")`
/// wrapper. Sharing the engine bridge's conversion internals with the content
/// format must not broaden provider configuration's stricter contract.
#[test]
fn author_file_rejects_legacy_some_wrapper() {
    let err = ron::from_str::<unmei::provider::AuthorFile>(r#"( model: Some("author/m1") )"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("expected a string or nothing"), "{err}");
}

/// Unit and `None` remain accepted (and mean absent), as before.
#[test]
fn author_file_accepts_unit_and_none() {
    let file: unmei::provider::AuthorFile =
        ron::from_str(r#"( base_url: (), model: None )"#).unwrap();
    assert_eq!(file.base_url, None);
    assert_eq!(file.model, None);
}

/// A config file that starts with a BOM resolves the same base URL and model
/// as the same file without one.
#[test]
fn bom_author_config_resolves_like_a_plain_one() {
    let _env = ENV_LOCK.lock().unwrap();
    set_env("UNMEI_API_KEY", "test-key-999");
    let dir = tmp_dir();
    let file = r#"( base_url: "http://localhost:4321", model: "author/m9" )"#;
    std::fs::write(dir.path().join("unmei.author.ron"), file).unwrap();
    let plain = resolve_provider(input(dir.path(), None, None)).unwrap();

    std::fs::write(
        dir.path().join("unmei.author.ron"),
        format!("\u{feff}{file}"),
    )
    .unwrap();
    let with_bom = resolve_provider(input(dir.path(), None, None))
        .expect("a BOM config resolves like any other");

    assert_eq!(with_bom.base_url, plain.base_url);
    assert_eq!(with_bom.model, plain.model);
}
