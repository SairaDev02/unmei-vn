//! Tests for the CLI bin surface (author-cli contract §1): arg parsing,
//! orchestration, `--dry-run`, and error paths.

mod common;
#[path = "common/pack.rs"]
mod pack;

use std::collections::VecDeque;
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A refused response body that carries material the retained diagnostic must
/// not leak: the dummy key `run_cli` exports and a URL (ADR 0014).
const REFUSING_BODY: &str = "upstream exploded (key test-key-1, url http://127.0.0.1:9/v1)";

/// One scripted reply: a chat completion, or the HTTP failure the endpoint
/// should answer with instead.
#[derive(Clone)]
enum ScriptStep {
    Reply(String),
    Http { status: u16, body: String },
}

/// A temp pack with the demo content + a fake OpenAI-compatible endpoint.
struct Fixture {
    _dir: tempfile::TempDir,
    pack_root: std::path::PathBuf,
    addr: std::net::SocketAddr,
    script: std::sync::Arc<std::sync::Mutex<VecDeque<ScriptStep>>>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    structured_supported: Arc<AtomicBool>,
    /// A source file the fake endpoint removes once the CLI's provider request
    /// has arrived — after the pack loaded, before staging runs.
    after_load_removal: std::sync::Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

impl Fixture {
    fn new() -> Self {
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pack");
        std::fs::create_dir_all(root.join("scenes")).unwrap();
        std::fs::create_dir_all(root.join("npcs")).unwrap();
        for file in ["pack.ron", "state.ron", "pack.glossary.md"] {
            std::fs::copy(demo.join(file), root.join(file)).unwrap();
        }
        for scene in ["first_meeting", "reunion", "epilogue"] {
            std::fs::copy(
                demo.join(format!("scenes/{scene}.unmei")),
                root.join(format!("scenes/{scene}.unmei")),
            )
            .unwrap();
        }
        std::fs::copy(demo.join("npcs/ace.ron"), root.join("npcs/ace.ron")).unwrap();

        // Fake OpenAI endpoint: serves scripted replies, records request bodies.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let script = std::sync::Arc::new(std::sync::Mutex::new(VecDeque::new()));
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let structured_supported = Arc::new(AtomicBool::new(true));
        let after_load_removal = std::sync::Arc::new(std::sync::Mutex::new(None));
        let script2 = script.clone();
        let requests2 = requests.clone();
        let structured_supported2 = structured_supported.clone();
        let after_load_removal2 = after_load_removal.clone();
        std::thread::spawn(move || loop {
            let (mut stream, _) = match listener.accept() {
                Ok(s) => s,
                Err(_) => return,
            };
            let request = common::read_full_request(&mut stream);
            requests2.lock().unwrap().push(request.clone());
            // The CLI has loaded the pack by the time it asks the endpoint for
            // a reply, so this removal lands before staging.
            if let Some(path) = after_load_removal2.lock().unwrap().take() {
                let _ = std::fs::remove_file(path);
            }
            let structured = request.contains("\"response_format\"");
            // Exercise the free-text fallback path: an endpoint that
            // rejects the structured request before serving the plain one.
            if structured && !structured_supported2.load(Ordering::SeqCst) {
                let body = r#"{"error":{"message":"No endpoints found that support the provided parameters (response_format)"}}"#;
                common::respond(&mut stream, "400 Bad Request", body);
                continue;
            }

            let step = script2.lock().unwrap().pop_front().unwrap_or_else(|| {
                ScriptStep::Reply(
                    serde_json::json!({
                        "GEN": {
                            "instructions": "How does Ace greet, given trust?",
                            "state_keys": ["affinity_level", "met_ace"],
                            "criteria": {
                                "warm": "affectionate",
                                "cold": "guarded",
                                "none": "neither fits"
                            },
                            "fallback": "cold"
                        }
                    })
                    .to_string(),
                )
            });
            let reply = match step {
                ScriptStep::Http { status, body } => {
                    common::respond(&mut stream, &format!("{status} Error"), &body);
                    continue;
                }
                ScriptStep::Reply(reply) => reply,
            };
            // A drafting client asks for structured `LINES`; a decision client
            // (npc) does not. Scripts are authored as say-lines and wrapped
            // when the request demands structured output.
            let reply = if structured {
                lines_to_structured(&reply)
            } else {
                reply
            };
            let body = serde_json::to_string(&serde_json::json!({
                "choices": [{"message": {"content": reply}}]
            }))
            .unwrap();
            common::respond(&mut stream, "200 OK", &body);
        });

        Fixture {
            _dir: dir,
            pack_root: root,
            addr,
            script,
            requests,
            structured_supported,
            after_load_removal,
        }
    }

    fn push(&self, content: &str) {
        self.script
            .lock()
            .unwrap()
            .push_back(ScriptStep::Reply(content.to_string()));
    }

    /// Script an HTTP failure for the next request (provider-error paths).
    fn push_http(&self, status: u16, body: &str) {
        self.script.lock().unwrap().push_back(ScriptStep::Http {
            status,
            body: body.to_string(),
        });
    }

    /// Make the fake endpoint reject a structured request, forcing the client's
    /// automatic free-text fallback (contract §7).
    fn unsupported_structured(&self) {
        self.structured_supported.store(false, Ordering::SeqCst);
    }

    /// Remove `path` once the CLI's provider request has arrived: the pack is
    /// loaded by then, so the next staged transplant cannot read it.
    fn remove_after_pack_load(&self, path: &Path) {
        *self.after_load_removal.lock().unwrap() = Some(path.to_path_buf());
    }
}

/// Wraps scripted free-text say-lines into the `{"LINES": [...]}` envelope a
/// structured drafting request expects.
fn lines_to_structured(reply: &str) -> String {
    let lines: Vec<serde_json::Value> = reply
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once(": ") {
            Some((speaker, text)) => serde_json::json!({
                "speaker": speaker.trim(),
                "text": text.trim_end(),
            }),
            None => serde_json::json!({ "speaker": line, "text": "" }),
        })
        .collect();
    serde_json::json!({ "LINES": lines }).to_string()
}

fn run_cli(args: &[&str], envs: &[(&str, &str)]) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("unmei").unwrap();
    cmd.args(args);
    cmd.env("UNMEI_API_KEY", "test-key-1");
    cmd.env("OPENROUTER_API_KEY", "");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.timeout(std::time::Duration::from_secs(30));
    cmd
}

/// RED: help names the contract's command surface.
#[test]
fn help_shows_contract_surface() {
    let out = run_cli(&["author", "npc", "--help"], &[])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for needle in [
        "--question",
        "--type",
        "--create",
        "--brief",
        "--brief-file",
        "--interactive",
        "--pack",
        "--model",
        "--base-url",
        "--dry-run",
    ] {
        assert!(text.contains(needle), "help missing {needle}:\n{text}");
    }
}

/// RED: the `author` help names both subcommands and scene-prose
/// drafting, no longer calls the commands offline, and still names the
/// OpenAI-compatible chat endpoint. Both surfaces Clap renders it on are
/// checked, because one doc comment feeds `unmei --help` and
/// `unmei author --help`.
#[test]
fn author_help_names_both_subcommands() {
    for args in [&["--help"][..], &["author", "--help"][..]] {
        let out = run_cli(args, &[]).assert().success().get_output().clone();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let surface = args.join(" ");
        assert!(
            !text.to_lowercase().contains("offline"),
            "`{surface}` still calls authoring offline:\n{text}"
        );
        assert!(
            text.contains("NPC decision config"),
            "`{surface}` does not name NPC decision configs:\n{text}"
        );
        assert!(
            text.contains("prose"),
            "`{surface}` does not name scene prose drafting:\n{text}"
        );
        assert!(
            text.contains("OpenAI-compatible"),
            "`{surface}` no longer names the OpenAI-compatible chat model:\n{text}"
        );
    }
}

/// RED: a full one-shot run generates, validates, and writes npcs/ace.ron
/// (replace), preserving the file's comments.
#[test]
fn one_shot_replaces_config_and_preserves_comments() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let before = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let after = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    // Comments preserved, and the file still parses + pack loads.
    for line in before.lines().filter(|l| l.trim_start().starts_with("//")) {
        assert!(after.contains(line), "lost comment: {line}");
    }
    // Regenerated choice output stays byte-identical to the pre-`type` form.
    assert!(
        !after.contains("type:"),
        "choice output omits type: {after}"
    );
    unmei_engine::load_pack(&fx.pack_root).unwrap();
}

/// RED: --dry-run prints the RON but writes nothing.
#[test]
fn dry_run_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let before = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--dry-run",
        ],
        &[],
    )
    .assert()
    .success();

    let out = &run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--dry-run",
        ],
        &[],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("instructions"),
        "dry-run prints the RON: {stdout}"
    );
    let after = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    assert_eq!(before, after, "--dry-run must not write");
}

/// RED: --create makes a new NPC file; running --create again on the same NPC
/// errors without touching the file.
#[test]
fn create_then_create_again_errors() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);

    run_cli(
        &[
            "author",
            "npc",
            "nova",
            "--create",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();
    assert!(fx.pack_root.join("npcs/nova.ron").exists());
    unmei_engine::load_pack(&fx.pack_root).unwrap();

    // Nova has no decisions map entry referenced by scenes, so the pack loads.
    run_cli(
        &[
            "author",
            "npc",
            "nova",
            "--create",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .failure();
}

/// RED: missing key → immediate clear error, no network call, nonzero exit.
#[test]
fn missing_key_fails_fast_without_network() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let requests_before = fx.requests.lock().unwrap().len();

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[("UNMEI_API_KEY", ""), ("OPENROUTER_API_KEY", "")],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("UNMEI_API_KEY"), "clear error: {stderr}");
    assert_eq!(
        fx.requests.lock().unwrap().len(),
        requests_before,
        "no network call on missing key"
    );
}

/// RED: retries then write-or-nothing — all-bad replies exit nonzero and the
/// NPC file is untouched.
#[test]
fn exhausted_retries_write_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    for _ in 0..3 {
        fx.push("garbage that is not json");
    }
    let before = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let output = String::from_utf8_lossy(&out.stdout).to_string()
        + String::from_utf8_lossy(&out.stderr).as_ref();
    assert!(
        output.contains("attempt") || output.contains("failed"),
        "{output}"
    );
    let after = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    assert_eq!(before, after, "write-or-nothing on failure");
}

/// RED: a staged-preparation failure is terminal on the binary surface —
/// exit 1 with the candidate-not-installed context on stderr, no stdout
/// preview/default reply, no retry and no write. The fake endpoint removes a
/// source file after the pack loaded, so the staged transplant fails.
#[test]
fn preparation_failure_is_terminal_on_stderr_with_no_write() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let ace = fx.pack_root.join("npcs/ace.ron");
    fx.remove_after_pack_load(&ace);

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stderr.contains('✖'), "the failure route: {stderr}");
    assert!(
        stderr.contains("not installed or validated"),
        "the explicit preparation context: {stderr}"
    );
    assert!(!stdout.contains("preview"), "no success preview: {stdout}");
    assert!(!stdout.contains("Wrote"), "no success line: {stdout}");
    assert_eq!(
        fx.requests.lock().unwrap().len(),
        1,
        "a preparation failure spends no retry"
    );
    assert!(
        !ace.exists(),
        "the failure wrote nothing, so the removed file stays absent"
    );
}

/// RED: validation failures are fed back as corrections — the second request's
/// messages grow by an assistant turn + a user correction.
#[test]
fn corrections_fed_back_between_attempts() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    // First: fallback not in criteria (validation error). Second: valid.
    fx.push(r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"purple"}}"#);
    fx.push(r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"cold"}}"#);

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    // The retry request carries the previous assistant reply + the correction.
    let first_messages = count_messages(&requests[0]);
    let second_messages = count_messages(&requests[1]);
    assert_eq!(
        second_messages,
        first_messages + 2,
        "retry adds assistant + user turn"
    );
}

fn count_messages(request_body: &str) -> usize {
    let body_start = request_body.find('{').unwrap();
    let value: serde_json::Value = serde_json::from_str(&request_body[body_start..]).unwrap();
    value["messages"].as_array().map(|a| a.len()).unwrap_or(0)
}

/// RED: the wire requests go to the configured base_url (OpenAI path).
#[test]
fn requests_go_to_configured_endpoint() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let _out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();
    let requests = fx.requests.lock().unwrap();
    assert!(
        requests[0].starts_with("POST /v1/chat/completions"),
        "{}",
        requests[0].split_whitespace().next().unwrap_or("")
    );
}

/// RED (ADR 0014 / contract §4): a malformed `unmei.author.ron` refuses
/// before any provider request or write, leaving the pack byte-identical.
#[test]
fn malformed_author_config_refuses_before_requests_and_writes() {
    let fx = Fixture::new();
    std::fs::write(fx.pack_root.join("unmei.author.ron"), "( model: ").unwrap();
    let endpoint = format!("http://{}", fx.addr);
    let before = pack::pack_bytes(&fx.pack_root);

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid RON in"), "{stderr}");
    assert!(stderr.contains("unmei.author.ron"), "{stderr}");
    assert!(
        fx.requests.lock().unwrap().is_empty(),
        "malformed configuration refuses before any provider request"
    );
    assert_eq!(
        pack::pack_bytes(&fx.pack_root),
        before,
        "the pack, including the malformed author file, is byte-identical"
    );
}

/// RED (ADR 0014 / contract §4): with both recognized keys set to
/// distinguishable values, the spawned command sends the `UNMEI_API_KEY`.
#[test]
fn unmei_key_precedence_is_observed_on_the_wire() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[
            ("UNMEI_API_KEY", "unmei-key-1"),
            ("OPENROUTER_API_KEY", "openrouter-key-2"),
        ],
    )
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    let request = requests[0].to_lowercase();
    assert!(
        request.contains("authorization: bearer unmei-key-1"),
        "UNMEI_API_KEY wins: {}",
        requests[0]
    );
    assert!(
        !request.contains("openrouter-key-2"),
        "the non-selected key never reaches the wire: {}",
        requests[0]
    );
}

/// RED: interactive mode accepts the first generation and writes it.
#[test]
fn interactive_accept_writes() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--interactive",
        ],
        &[],
    )
    .write_stdin(
        "keep greet as-is

accept
",
    )
    .assert()
    .success();

    let after = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    assert!(after.contains("instructions"), "config written: {after}");
}

/// RED: interactive feedback regenerates before accept — two chat calls, the
/// second carrying the author's feedback.
#[test]
fn interactive_feedback_then_accept() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--interactive",
        ],
        &[],
    )
    .write_stdin(
        "keep greet as-is

make warm less effusive
accept
",
    )
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "feedback triggers a regeneration");
    let second = &requests[1];
    assert!(
        second.contains("less effusive"),
        "feedback rides along: {second}"
    );
}

/// RED: interactive discard writes nothing.
#[test]
fn interactive_discard_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let before = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--interactive",
        ],
        &[],
    )
    .write_stdin(
        "keep greet as-is

discard
",
    )
    .assert()
    .success();

    let after = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    assert_eq!(before, after, "discard writes nothing");
}

/// RED: no brief and no glossary → a warning names what to add (§6), then
/// generation still proceeds from pack context alone.
#[test]
fn warns_when_neither_brief_nor_glossary() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    std::fs::remove_file(fx.pack_root.join("pack.glossary.md")).unwrap();

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--dry-run",
        ],
        &[],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("glossary") && stderr.to_lowercase().contains("brief"),
        "warning names both inputs: {stderr}"
    );
}

/// RED: on --create, a model-provided NPC name lands in the file (contract §2:
/// identity header comes from the generation).
#[test]
fn create_uses_model_provided_name() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    fx.push(
        &serde_json::json!({
            "GEN": {
                "instructions": "How does Nova greet, given trust?",
                "state_keys": ["affinity_level"],
                "criteria": { "warm": "warm", "cold": "cold", "none": "neither" },
                "fallback": "cold"
            },
            "NPC": { "id": "nova", "name": "Nova the Archivist" }
        })
        .to_string(),
    );

    run_cli(
        &[
            "author",
            "npc",
            "nova",
            "--create",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let written = std::fs::read_to_string(fx.pack_root.join("npcs/nova.ron")).unwrap();
    assert!(
        written.contains("Nova the Archivist"),
        "model name used: {written}"
    );
}

/// RED: `--type score` writes a valid typed config (ordered levels + index
/// fallback) and the pack still loads.
#[test]
fn score_type_writes_typed_config() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    fx.push(
        &serde_json::json!({
            "GEN": {
                "instructions": "How much does Ace trust the player?",
                "state_keys": ["affinity_level", "letters_found"],
                "criteria": ["No trust at all.", "Wary but willing to talk.", "Complete trust."],
                "fallback": 0
            }
        })
        .to_string(),
    );

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "trust",
            "--type",
            "score",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let written = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    let npc: unmei_engine::Npc = ron::from_str(&written).unwrap();
    let config = npc.decisions.get("trust").expect("trust generated");
    assert_eq!(config.question_type, unmei_engine::QuestionType::Score);
    assert_eq!(config.fallback, unmei_engine::Fallback::Index(0));
    assert_eq!(config.score_criteria().unwrap().len(), 3);
    unmei_engine::load_pack(&fx.pack_root).unwrap();
}

/// RED: `--type noul` writes a valid typed config (true/false criteria + bool
/// fallback) and the pack still loads.
#[test]
fn noul_type_writes_typed_config() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    fx.push(
        &serde_json::json!({
            "GEN": {
                "instructions": "Is the player acting hostile toward Ace?",
                "state_keys": ["affinity_level", "met_ace"],
                "criteria": { "true": "Openly hostile.", "false": "Neutral or friendly." },
                "fallback": false
            }
        })
        .to_string(),
    );

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "is_hostile",
            "--type",
            "noul",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let written = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    let npc: unmei_engine::Npc = ron::from_str(&written).unwrap();
    let config = npc
        .decisions
        .get("is_hostile")
        .expect("is_hostile generated");
    assert_eq!(config.question_type, unmei_engine::QuestionType::Noul);
    assert_eq!(config.fallback, unmei_engine::Fallback::Bool(false));
    unmei_engine::load_pack(&fx.pack_root).unwrap();
}

/// RED: on replace, the existing config's type is inherited when `--type` is
/// absent — the second (flagless) request still asks for the score shape.
#[test]
fn replace_inherits_existing_type_without_flag() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let score_body = serde_json::json!({
        "GEN": {
            "instructions": "How much does Ace trust the player?",
            "state_keys": ["affinity_level"],
            "criteria": ["None.", "Some.", "Complete."],
            "fallback": 0
        }
    })
    .to_string();
    fx.push(&score_body);
    fx.push(&score_body);

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "trust",
            "--type",
            "score",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "trust",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "one request per invocation");
    // The system prompt — not the embedded existing config — must name the
    // inherited type, so this fails if inheritance regressed to `choice`.
    assert!(
        requests[1].contains("Type: score") && !requests[1].contains("Type: choice"),
        "flagless replace still asks for the inherited score shape: {}",
        requests[1]
    );
    let written = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    let npc: unmei_engine::Npc = ron::from_str(&written).unwrap();
    assert_eq!(
        npc.decisions.get("trust").unwrap().question_type,
        unmei_engine::QuestionType::Score
    );
}

/// RED: `--type score --dry-run` prints the typed RON and writes nothing.
#[test]
fn dry_run_typed_prints_ron_and_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    fx.push(
        &serde_json::json!({
            "GEN": {
                "instructions": "How much does Ace trust the player?",
                "state_keys": ["affinity_level"],
                "criteria": ["None.", "Some."],
                "fallback": 1
            }
        })
        .to_string(),
    );
    let before = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "trust",
            "--type",
            "score",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--dry-run",
        ],
        &[],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("score"),
        "dry-run prints the typed RON: {stdout}"
    );
    assert!(stdout.contains("instructions"), "{stdout}");
    let after = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    assert_eq!(before, after, "--dry-run must not write");
}

/// RED: interactive mode with `--type` generates the typed shape and writes it
/// on accept (AC: the interactive path keeps working for typed questions).
#[test]
fn interactive_typed_accept_writes() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    fx.push(
        &serde_json::json!({
            "GEN": {
                "instructions": "Is the player acting hostile toward Ace?",
                "state_keys": ["affinity_level"],
                "criteria": { "true": "Hostile.", "false": "Friendly." },
                "fallback": false
            }
        })
        .to_string(),
    );

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "is_hostile",
            "--type",
            "noul",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--interactive",
        ],
        &[],
    )
    .write_stdin(
        "keep it short

accept
",
    )
    .assert()
    .success();

    let written = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    let npc: unmei_engine::Npc = ron::from_str(&written).unwrap();
    assert_eq!(
        npc.decisions.get("is_hostile").unwrap().question_type,
        unmei_engine::QuestionType::Noul
    );
    let requests = fx.requests.lock().unwrap();
    assert!(
        requests[0].contains("Type: noul"),
        "interactive request asks for the typed shape: {}",
        requests[0]
    );
}

// ---------------------------------------------------------------------------
// `author draft` run addressing
// ---------------------------------------------------------------------------

/// RED: `--list` prints every run's handle, span and first say-line, in scene
/// order, and drafts nothing.
#[test]
fn draft_list_prints_runs() {
    let fx = Fixture::new();
    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--list",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout,
        concat!(
            "  start       L7     Narrator: Morning light. A folded letter on the ground, half under a shoe.\n",
            "  start#2     L10    Ace: You're not from around here, are you?\n",
            "  ask_letter  L21    Ace: A letter? Ah... you saw that.\n",
            "  silence     L28    Ace: ...Fine, don't talk. See if I care.\n",
        )
    );
}

/// RED: `--from` is required unless `--list` is given.
#[test]
fn draft_requires_from_or_list() {
    let fx = Fixture::new();
    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("--list") && stderr.contains("--from"),
        "clear mode error: {stderr}"
    );
}

/// RED: a `--from` that names nothing is a clear error.
#[test]
fn draft_from_unknown_address_errors() {
    let fx = Fixture::new();
    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#9",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("no run `start#9`"), "{stderr}");
    assert!(stderr.contains("first_meeting"), "{stderr}");
}

/// RED: a one-shot `--from` draft generates, gates, validates and splices only
/// the named run back into the scene; every other line survives byte-for-byte.
#[test]
fn draft_one_shot_replaces_only_the_run() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("Ace: Who goes there? You don't look like you're from the school.\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded; the player is a stranger.",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let after = std::fs::read_to_string(&scene_path).unwrap();
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    assert_eq!(
        after_lines.len(),
        before_lines.len(),
        "no line added/removed"
    );
    for (i, (old, new)) in before_lines.iter().zip(&after_lines).enumerate() {
        if i == 9 {
            continue; // line 10 is the `start#2` run
        }
        assert_eq!(old, new, "line {} outside the run changed", i + 1);
    }
    assert_eq!(
        after_lines[9],
        "Ace: Who goes there? You don't look like you're from the school."
    );

    // The pack still loads, and the request carried the assembled context.
    unmei_engine::load_pack(&fx.pack_root).unwrap();
    let requests = fx.requests.lock().unwrap();
    // Contract §7: drafting asks for JSON-schema output and routes only
    // to endpoints that can honour it.
    assert!(
        requests[0].contains("\"response_format\"") && requests[0].contains("\"json_schema\""),
        "the primary request is structured: {}",
        requests[0]
    );
    assert!(
        requests[0].contains("\"require_parameters\":true"),
        "structured requests set require_parameters: {}",
        requests[0]
    );
    assert!(
        requests[0].contains("\"additionalProperties\":false"),
        "the strict schema forbids extra properties on every object: {}",
        requests[0]
    );
    assert!(
        requests[0].contains("first_meeting") && requests[0].contains("npcs/ace.ron"),
        "context assembly reaches the endpoint: {}",
        requests[0]
    );
    assert!(
        requests[0].contains("Ace is guarded; the player is a stranger."),
        "the brief reaches the endpoint: {}",
        requests[0]
    );
}

/// RED: `--dry-run` prints the drafted run and writes nothing.
#[test]
fn draft_dry_run_prints_and_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("Ace: Who goes there?\n");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--dry-run",
        ],
        &[],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Ace: Who goes there?"), "{stdout}");
    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert_eq!(before, after, "--dry-run must not write");
}

/// RED: a gate failure prints the errors, writes nothing and exits nonzero.
#[test]
fn draft_gate_failure_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("sprite ace happy\nAce: Hi\n");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("keyword"), "{stderr}");
    assert!(stderr.contains("nothing was written"), "{stderr}");
    // Contract §5: 3 attempts, then the best raw output + all errors.
    assert!(stderr.contains("best model output"), "{stderr}");
    assert_eq!(
        fx.requests.lock().unwrap().len(),
        3,
        "validation failures retry up to 3 attempts"
    );
    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert_eq!(before, after, "a gate failure must not write");
}

/// RED (contract §7): an endpoint that lacks structured-output support is
/// retried in free text automatically, and the run still lands.
#[test]
fn draft_falls_back_to_free_text_when_endpoint_lacks_structured() {
    let fx = Fixture::new();
    fx.unsupported_structured();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    fx.push("Ace: Who goes there? You're new here.\n");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("free-text fallback"),
        "the degradation is reported, not silent: {stdout}"
    );
    assert!(
        stdout.contains("the endpoint rejected `response_format`"),
        "the degradation names the provider's fixed category: {stdout}"
    );
    assert!(
        !stdout.contains("No endpoints found"),
        "the refused response body is not echoed: {stdout}"
    );

    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert!(
        after.contains("Ace: Who goes there? You're new here."),
        "free-text fallback landed the run: {after}"
    );
    let requests = fx.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "structured attempt then free text");
    assert!(
        requests[0].contains("\"response_format\""),
        "the first request is structured: {}",
        requests[0]
    );
    assert!(
        !requests[1].contains("\"response_format\""),
        "the fallback request is free text: {}",
        requests[1]
    );
}

/// RED: an unknown speaker is a gate rejection too, and the scene is untouched.
#[test]
fn draft_unknown_speaker_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("Bob: Who goes there?\n");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("Bob") && stderr.contains("not allowed"),
        "{stderr}"
    );
    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert_eq!(before, after, "a gate failure must not write");
}

/// RED: `--brief-file` is read and reaches the request.
#[test]
fn draft_brief_file_reaches_the_prompt() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let brief_path = fx.pack_root.join("brief.txt");
    std::fs::write(&brief_path, "Ace is guarded and terse.").unwrap();
    fx.push("Ace: Who goes there?\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief-file",
            brief_path.to_str().unwrap(),
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert!(
        requests[0].contains("Ace is guarded and terse."),
        "{}",
        requests[0]
    );
}

/// RED: with neither a brief nor a glossary, the draft warns (but proceeds).
#[test]
fn draft_warns_when_no_brief_and_no_glossary() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    std::fs::remove_file(fx.pack_root.join("pack.glossary.md")).unwrap();
    fx.push("Ace: Who goes there?\n");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("warning"), "{stderr}");
}

/// RED: the provider precedence is shared with `author npc` — no model is a
/// clear, pre-network error.
#[test]
fn draft_without_model_errors_before_any_request() {
    let fx = Fixture::new();
    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("--model"), "{stderr}");
    assert!(fx.requests.lock().unwrap().is_empty(), "no request made");
}

/// RED: a scene that is not in the pack is a terminal membership
/// refusal — drafting never stages a Scene the author did not write.
#[test]
fn draft_list_unknown_scene_errors() {
    let fx = Fixture::new();
    let out = run_cli(
        &[
            "author",
            "draft",
            "no_such_scene",
            "--list",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("no scene \"no_such_scene\""),
        "escaped absent-scene refusal: {stderr}"
    );
    assert!(
        stderr.contains("existing scene"),
        "refusal names the membership rule: {stderr}"
    );
}

/// RED: the draft help names the drafting surface (addressing, context, provider).
#[test]
fn draft_help_names_addressing_flags() {
    let out = run_cli(&["author", "draft", "--help"], &[])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for needle in [
        "--from",
        "--list",
        "--brief",
        "--brief-file",
        "--interactive",
        "--pack",
        "--model",
        "--base-url",
        "--dry-run",
    ] {
        assert!(text.contains(needle), "help missing {needle}:\n{text}");
    }
}

// ---------------------------------------------------------------------------
// `author draft -i` interactive session
// ---------------------------------------------------------------------------

/// RED (contract §5): `-i` previews the first generation and `accept`
/// writes it through the same gate, replacing only the addressed run.
#[test]
fn draft_interactive_accept_writes() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    fx.push("Ace: Who goes there? You're new here.\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded; the player is a stranger.",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("\naccept\n")
    .assert()
    .success();

    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert!(
        after.contains("Ace: Who goes there? You're new here."),
        "accepted run written: {after}"
    );
    unmei_engine::load_pack(&fx.pack_root).unwrap();
}

/// RED (contract §5): free-text feedback regenerates the run with the
/// feedback carried into the next attempt, and `accept` writes the second.
#[test]
fn draft_interactive_feedback_then_accept() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    fx.push("Ace: Who goes there?\n");
    fx.push("Ace: Who goes there? Keep it guarded.\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded.",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("\nmake Ace more guarded\naccept\n")
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "feedback triggers a regeneration");
    assert!(
        requests[1].contains("make Ace more guarded"),
        "feedback rides the second request: {}",
        requests[1]
    );
    drop(requests);
    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert!(
        after.contains("Ace: Who goes there? Keep it guarded."),
        "the regenerated run is written: {after}"
    );
}

/// RED (contract §5): `discard` writes nothing — the scene is untouched.
#[test]
fn draft_interactive_discard_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("Ace: Who goes there?\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded.",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("\ndiscard\n")
    .assert()
    .success();

    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert_eq!(before, after, "discard writes nothing");
}

/// RED (contract §5): with no `--brief`, `-i` collects one from stdin
/// before the first round, and the collected brief steers the request.
#[test]
fn draft_interactive_collects_brief() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    fx.push("Ace: Who goes there?\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("Ace is guarded and terse.\n\naccept\n")
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert!(
        requests[0].contains("Ace is guarded and terse."),
        "the collected brief reaches the request: {}",
        requests[0]
    );
}

/// RED (contract §5): `-i` shows a supplied brief and takes a one-line
/// replacement before generating.
#[test]
fn draft_interactive_edits_brief() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    fx.push("Ace: Who goes there?\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "OLD_BRIEF_MARKER",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("Ace is guarded and terse.\naccept\n")
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert!(
        requests[0].contains("Ace is guarded and terse."),
        "the replacement brief reaches the request: {}",
        requests[0]
    );
    assert!(
        !requests[0].contains("OLD_BRIEF_MARKER"),
        "the replaced brief is not sent: {}",
        requests[0]
    );
}

/// RED (contract §5): a gate failure after the retries prints every error,
/// still asks for feedback, and `discard` leaves the scene unmodified.
#[test]
fn draft_interactive_gate_failure_leaves_scene_unchanged() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("sprite ace happy\n");
    fx.push("sprite ace happy\n");
    fx.push("sprite ace happy\n");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded.",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("\ndiscard\n")
    .assert()
    .success()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("keyword"), "{stderr}");
    assert!(
        stderr.contains("free-text feedback"),
        "the session still offers feedback: {stderr}"
    );
    assert_eq!(
        fx.requests.lock().unwrap().len(),
        3,
        "validation failures retry up to 3 attempts"
    );
    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert_eq!(before, after, "a gate failure must not write");
}

/// RED: `--dry-run -i` accepts but writes nothing.
#[test]
fn draft_interactive_dry_run_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("Ace: Who goes there?\n");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded.",
            "--interactive",
            "--dry-run",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("\naccept\n")
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Ace: Who goes there?"), "{stdout}");
    assert!(stdout.contains("--dry-run"), "{stdout}");
    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert_eq!(before, after, "--dry-run must not write");
}

/// RED: a closed stdin at the brief prompt unwinds writing nothing — it
/// must not spin re-prompting on EOF.
#[test]
fn draft_interactive_eof_at_brief_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded.",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("")
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("nothing written"), "{stdout}");
    assert!(
        fx.requests.lock().unwrap().is_empty(),
        "no request is made after EOF"
    );
    assert_eq!(std::fs::read_to_string(&scene_path).unwrap(), before);
}

/// RED: a closed stdin at the accept/discard prompt unwinds writing
/// nothing, even though a valid draft was generated.
#[test]
fn draft_interactive_eof_after_preview_writes_nothing() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read_to_string(&scene_path).unwrap();
    fx.push("Ace: Who goes there?\n");

    // One blank line keeps the supplied brief, then EOF at the next prompt.
    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded.",
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .write_stdin("\n")
    .assert()
    .success();

    assert_eq!(
        fx.requests.lock().unwrap().len(),
        1,
        "the draft was generated once"
    );
    assert_eq!(
        std::fs::read_to_string(&scene_path).unwrap(),
        before,
        "EOF writes nothing"
    );
}

// ---------------------------------------------------------------------------
// Whole-command terminal masking of the recognized credentials (ADR 0014)
// ---------------------------------------------------------------------------

/// The fixed marker the CLI substitutes for a recognized credential value.
const MASK: &str = "[redacted]";

/// Assert `stream` carries the marker and neither dummy value.
fn assert_masked(label: &str, stream: &str, values: &[&str]) {
    assert!(stream.contains(MASK), "{label}: no marker in {stream}");
    for value in values {
        assert!(
            !stream.contains(value),
            "{label}: `{value}` survived in {stream}"
        );
    }
}

/// RED: a Clap argument error quotes the rejected value, so it must be
/// masked as well — `Cli::parse` renders outside the runner's terminal. Both
/// the selected and the non-selected value are exercised.
#[test]
fn argument_errors_mask_both_known_values() {
    let selected = "unmei-argument-key-3471";
    let other = "openrouter-argument-key-9265";
    for value in [selected, other] {
        let out = run_cli(
            &[
                "author",
                "npc",
                "ace",
                "--question",
                "greet",
                "--type",
                value,
            ],
            &[("UNMEI_API_KEY", selected), ("OPENROUTER_API_KEY", other)],
        )
        .assert()
        .code(2)
        .get_output()
        .clone();

        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            stderr.contains("invalid value"),
            "the ordinary argument error survives: {stderr}"
        );
        assert_masked("argument error", &stderr, &[value]);
    }
}

/// RED: the OpenRouter fallback key is masked when the higher-precedence
/// `UNMEI_API_KEY` is empty rather than absent.
#[test]
fn the_openrouter_fallback_value_is_masked_when_unmei_key_is_empty() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let key = "openrouter-fallback-key-8104";
    fx.push(
        &serde_json::json!({
            "GEN": {
                "instructions": format!("Greet with {key}"),
                "state_keys": ["affinity_level", "met_ace"],
                "criteria": { "warm": "affectionate", "cold": "guarded", "none": "neither fits" },
                "fallback": "cold"
            }
        })
        .to_string(),
    );

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--dry-run",
        ],
        &[("UNMEI_API_KEY", ""), ("OPENROUTER_API_KEY", key)],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("Greet with"),
        "the ordinary preview survives: {stdout}"
    );
    assert_masked("openrouter fallback preview", &stdout, &[key]);
}

/// RED: `--help`/`--version` still take the successful stdout route now
/// that the binary renders Clap's output itself.
#[test]
fn help_and_version_still_render_on_stdout() {
    // Pin the logger filter so an inherited `RUST_LOG` cannot add stderr noise.
    let out = run_cli(&["author", "npc", "--help"], &[("RUST_LOG", "warn")])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("--question"),
        "help stays on stdout"
    );
    assert!(out.stderr.is_empty(), "help writes no stderr");

    let out = run_cli(&["--version"], &[("RUST_LOG", "warn")])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("unmei"),
        "version stays on stdout"
    );
    assert!(out.stderr.is_empty(), "version writes no stderr");

    // With no recognized value configured there is nothing to mask, so Clap
    // renders its own help.
    let out = run_cli(
        &["author", "npc", "--help"],
        &[
            ("UNMEI_API_KEY", ""),
            ("OPENROUTER_API_KEY", ""),
            ("RUST_LOG", "warn"),
        ],
    )
    .assert()
    .success()
    .get_output()
    .clone();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("--question"),
        "unmasked help stays on stdout"
    );
    assert!(out.stderr.is_empty(), "unmasked help writes no stderr");
}

/// RED: a generated `--dry-run` preview masks both recognized values
/// while the provider prompt and the untouched pack keep their literal bytes.
#[test]
fn dry_run_preview_masks_both_values_but_not_the_prompt_or_files() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let selected = "unmei-preview-key-5182";
    let other = "openrouter-preview-key-7043";
    let before = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    fx.push(
        &serde_json::json!({
            "GEN": {
                "instructions": format!("Greet with {selected} and {other}"),
                "state_keys": ["affinity_level", "met_ace"],
                "criteria": { "warm": "affectionate", "cold": "guarded", "none": "neither fits" },
                "fallback": "cold"
            }
        })
        .to_string(),
    );

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--brief",
            &format!("keep {selected} out of transcripts"),
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
            "--dry-run",
        ],
        &[("UNMEI_API_KEY", selected), ("OPENROUTER_API_KEY", other)],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("Greet with") && stdout.contains("instructions"),
        "the ordinary dry-run preview survives: {stdout}"
    );
    assert_masked("dry-run preview", &stdout, &[selected, other]);

    let requests = fx.requests.lock().unwrap();
    assert!(
        requests[0].contains(selected),
        "the provider prompt is not rewritten"
    );
    drop(requests);
    assert_eq!(
        std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap(),
        before,
        "--dry-run writes nothing"
    );
}

/// RED: the exhaustion report prints the last raw model reply, so the
/// best-output block masks the credential it carries.
#[test]
fn exhaustion_best_output_masks_the_known_value() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let key = "unmei-exhaustion-key-6690";
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read(&scene_path).unwrap();
    // `Bob` is not an allowed speaker, so every attempt is rejected.
    for _ in 0..3 {
        fx.push(&format!("Bob: {key}"));
    }

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[("UNMEI_API_KEY", key), ("OPENROUTER_API_KEY", "")],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("best model output") && stderr.contains("validation errors"),
        "the ordinary exhaustion report survives: {stderr}"
    );
    assert_masked("exhaustion report", &stderr, &[key]);
    assert_eq!(
        std::fs::read(&scene_path).unwrap(),
        before,
        "exhaustion writes nothing"
    );
}

/// RED: a validation error carries the offending value into the retry
/// correction payload, where it stays literal (terminal-only masking).
#[test]
fn validation_corrections_keep_the_literal_value() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let key = "unmei-correction-key-6634";
    fx.push(&invalid_fallback_reply(key));
    fx.push(&valid_npc_reply());

    run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[("UNMEI_API_KEY", key), ("OPENROUTER_API_KEY", "")],
    )
    .assert()
    .success();

    let requests = fx.requests.lock().unwrap();
    assert_eq!(requests.len(), 2, "the rejected attempt is retried");
    assert!(
        requests[1].contains(key),
        "the correction payload is not rewritten: {}",
        requests[1]
    );
}

/// RED: on exhaustion the printed validation errors and best output are
/// masked, and the failure still writes nothing.
#[test]
fn exhaustion_validation_errors_are_masked() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let key = "unmei-validation-key-4029";
    let before = std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap();
    for _ in 0..3 {
        fx.push(&invalid_fallback_reply(key));
    }

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[("UNMEI_API_KEY", key), ("OPENROUTER_API_KEY", "")],
    )
    .assert()
    .failure()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("best model output") && stderr.contains("is not a criterion id"),
        "the ordinary exhaustion report survives: {stderr}"
    );
    assert_masked("validation report", &stderr, &[key]);
    assert_eq!(
        std::fs::read_to_string(fx.pack_root.join("npcs/ace.ron")).unwrap(),
        before,
        "exhaustion writes nothing"
    );
}

/// A choice reply whose fallback names the credential, so the value reaches a
/// validation error (and the correction payload) verbatim.
fn invalid_fallback_reply(key: &str) -> String {
    serde_json::json!({
        "GEN": {
            "instructions": "How does Ace greet, given trust?",
            "state_keys": ["affinity_level", "met_ace"],
            "criteria": { "warm": "affectionate", "cold": "guarded" },
            "fallback": key
        }
    })
    .to_string()
}

/// RED: a `RUST_LOG` spec carrying a recognized value never reaches
/// env_filter's own un-maskable stderr warning, and the diagnostic is rendered
/// through the production masked logger sink rather than a copy of it.
#[test]
fn a_rust_log_spec_carrying_a_key_is_masked_through_the_logger_sink() {
    let fx = Fixture::new();
    let key = "unmei-rust-log-key-1459";
    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--list",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[
            ("UNMEI_API_KEY", key),
            ("OPENROUTER_API_KEY", ""),
            ("RUST_LOG", &format!("{key}=a=b")),
        ],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("Narrator: Morning light."),
        "the command still runs: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("ignoring RUST_LOG"),
        "the safe logger diagnostic survives: {stderr}"
    );
    assert_masked("logger sink", &stderr, &[key]);
}

/// RED: `env_filter`'s own warning text — not only the spec — can carry a
/// recognized value, so a malformed spec whose diagnostic spells one out must be
/// reported through the masked sink rather than printed by the dependency.
#[test]
fn a_malformed_rust_log_diagnostic_cannot_leak_a_fixed_phrase_credential() {
    let fx = Fixture::new();
    let key = "invalid logging spec";
    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--list",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[
            ("UNMEI_API_KEY", key),
            ("OPENROUTER_API_KEY", ""),
            ("RUST_LOG", "module=bogus"),
        ],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("ignoring RUST_LOG"),
        "the safe logger diagnostic survives: {stderr}"
    );
    assert_masked("env_filter diagnostic", &stderr, &[key]);
}

/// RED: a setup/final error rendered by `main` — not the runner — is
/// masked too, and the failure still precedes any provider call.
#[test]
fn setup_errors_mask_known_values_outside_the_runner() {
    let fx = Fixture::new();
    let key = "unmei-brief-file-key-8823";
    let other = "openrouter-brief-file-key-4417";
    let missing = fx.pack_root.join(format!("{key}-brief.md"));

    let out = run_cli(
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--brief-file",
            missing.to_str().unwrap(),
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[("UNMEI_API_KEY", key), ("OPENROUTER_API_KEY", other)],
    )
    .assert()
    .code(1)
    .get_output()
    .clone();

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("cannot read --brief-file"),
        "the ordinary setup error survives: {stderr}"
    );
    assert_masked("setup error", &stderr, &[key, other]);
    assert!(
        fx.requests.lock().unwrap().is_empty(),
        "the refusal precedes any provider call"
    );
}

/// RED: the terminal/file boundary — the preview masks the literal while
/// the generated Scene keeps it byte-for-byte (terminal only, no rewriting).
#[test]
fn a_preview_is_masked_but_the_written_scene_is_not_rewritten() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let key = "unmei-written-scene-key-5519";
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let line = format!("Ace: Meet me at {key}.");
    fx.push(&line);

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[("UNMEI_API_KEY", key), ("OPENROUTER_API_KEY", "")],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("end draft"),
        "the preview survives: {stdout}"
    );
    assert_masked("draft preview", &stdout, &[key]);
    assert!(
        std::fs::read_to_string(&scene_path)
            .unwrap()
            .contains(&line),
        "the generated file is not silently rewritten"
    );
}

/// RED: `--list` prints a run's first say-line through `main`'s own
/// stdout route, so the mask covers it too.
#[test]
fn draft_list_masks_the_known_value() {
    let fx = Fixture::new();
    let key = "unmei-list-key-2073";
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let scene = std::fs::read_to_string(&scene_path).unwrap();
    std::fs::write(
        &scene_path,
        scene.replace("Morning light.", &format!("{key}.")),
    )
    .unwrap();

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--list",
            "--pack",
            fx.pack_root.to_str().unwrap(),
        ],
        &[("UNMEI_API_KEY", key), ("OPENROUTER_API_KEY", "")],
    )
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains(&format!("{MASK}. A folded letter")),
        "the ordinary list survives: {stdout}"
    );
    assert_masked("draft --list", &stdout, &[key]);
}

/// RED: the interactive brief prompt echoes the current brief, so the
/// masked value is absent while the prompt and flush remain usable.
#[test]
fn interactive_brief_prompt_masks_the_known_value() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let key = "unmei-interactive-key-7381";
    fx.push("Ace: Who goes there?");

    let out = run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            &format!("Ace is guarded; never say {key}"),
            "--interactive",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[("UNMEI_API_KEY", key), ("OPENROUTER_API_KEY", "")],
    )
    .write_stdin("discard\n")
    .assert()
    .success()
    .get_output()
    .clone();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("(enter to keep, or type a replacement):")
            && stdout.contains("Type 'accept' to write"),
        "the interactive prompts survive in full: {stdout}"
    );
    assert_masked("interactive prompt", &stdout, &[key]);
}

// ---------------------------------------------------------------------------
// Exact-byte authoring witnesses
// ---------------------------------------------------------------------------

/// Independently captured stdout/stderr/status bytes and written
/// files for the authoring commands' key paths. `.gitattributes` keeps these
/// byte-exact; expected bytes must never be regenerated from the renderer
/// under test. The base files are the capture host's (Windows) bytes; see
/// `expected_stdout` for the one host-dependent byte.
const BASELINE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/authoring");

/// The capture host emits `\r\n` for RON pretty output; the other supported
/// hosts emit `\n` (the locked `ron 0.10.1` `PrettyConfig::default`). Only
/// these two stdout records contain RON, so only they have an `lf/` variant;
/// every other record is host-independent.
const LF_STDOUT_VARIANTS: [&str; 2] = ["npc_one_shot_dry_run", "npc_interactive_accept"];

/// The host-appropriate expected stdout. A record without RON reads from the
/// base; a RON-bearing record reads its `lf/` variant on the other host. The
/// reads are required, so a missing committed file panics rather than falling
/// back silently.
fn expected_stdout(name: &str) -> Vec<u8> {
    let path = if cfg!(windows) || !LF_STDOUT_VARIANTS.contains(&name) {
        format!("{BASELINE_DIR}/{name}.stdout")
    } else {
        format!("{BASELINE_DIR}/lf/{name}.stdout")
    };
    std::fs::read(path).unwrap()
}

/// The host-appropriate expected stderr. No baseline error stream carries RON,
/// so every record compares byte-for-byte from the base on both hosts.
fn expected_stderr(name: &str) -> Vec<u8> {
    std::fs::read(format!("{BASELINE_DIR}/{name}.stderr")).unwrap()
}

/// The retired comparison's operation: CRLF->LF across a whole stream.
/// The baselines no longer apply it at comparison time; it remains only to
/// build the accepted LF-template expectation and to check that each committed
/// `lf/` stream variant is exactly the capture record with that one conversion.
fn without_crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// The inverse of `without_crlf`: give every bare LF the CRLF form. Used only
/// to build the expected accepted-Scene bytes for a CRLF source, matching
/// `splice_run`'s reuse of the source's line endings.
fn with_crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 16);
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' && (i == 0 || bytes[i - 1] != b'\r') {
            out.push(b'\r');
        }
        out.push(byte);
    }
    out
}

/// Whether a copied pack text file uses CRLF newlines. The baseline inputs are
/// whole-file LF or whole-file CRLF; a lone or mixed CR is a test-setup error,
/// not a newline style to model.
fn is_crlf_input(bytes: &[u8]) -> bool {
    let lf = bytes.iter().filter(|&&b| b == b'\n').count();
    let cr = bytes.iter().filter(|&&b| b == b'\r').count();
    let crlf = bytes.windows(2).filter(|w| w == b"\r\n").count();
    if crlf == 0 {
        assert_eq!(cr, 0, "input has a lone CR");
        false
    } else {
        assert!(cr == crlf && lf == crlf, "input mixes newline styles");
        true
    }
}

/// The exact bytes the accepted NPC file should carry for a template in the
/// given newline style. Production preserves the template's newlines and
/// rejoins the spliced decision block with LF (`splice::indent`), so this is
/// exact on every host and Git checkout with no blanket normalization. The
/// pretty-printer's newline never reaches this file.
fn expected_accepted_npc(crlf_template: bool) -> Vec<u8> {
    let base = std::fs::read(format!("{BASELINE_DIR}/npc_interactive_accept.npc_after")).unwrap();
    if crlf_template {
        base
    } else {
        without_crlf(&base)
    }
}

/// The exact bytes the accepted Scene should carry for a source in the given
/// newline style: `splice_run` reuses the source's line endings.
fn expected_accepted_scene(crlf_source: bool) -> Vec<u8> {
    let base = std::fs::read(format!(
        "{BASELINE_DIR}/draft_interactive_accept.scene_after"
    ))
    .unwrap();
    if crlf_source {
        with_crlf(&base)
    } else {
        base
    }
}

/// Describe the first differing byte of two streams, or `None` when they are
/// byte-identical. The baselines compare raw bytes, so a CRLF/LF change in
/// ordinary prose or an error message is a mismatch, not something to
/// normalize away.
fn first_mismatch(actual: &[u8], expected: &[u8]) -> Option<String> {
    if actual == expected {
        return None;
    }
    let shared = actual.len().min(expected.len());
    let offset = (0..shared)
        .find(|&i| actual[i] != expected[i])
        .unwrap_or(shared);
    let window = |bytes: &[u8]| {
        let start = offset.saturating_sub(24);
        let end = (offset + 24).min(bytes.len());
        String::from_utf8_lossy(&bytes[start..end]).into_owned()
    };
    Some(format!(
        "first difference at byte {offset} (actual {}, expected {} bytes); actual ...{}... expected ...{}...",
        actual.len(),
        expected.len(),
        window(actual),
        window(expected),
    ))
}

/// Panic unless `actual` is byte-identical to `expected`. Every baseline stream
/// and accepted-file comparison goes through here.
fn assert_raw_bytes(label: &str, actual: &[u8], expected: &[u8]) {
    if let Some(diff) = first_mismatch(actual, expected) {
        panic!("{label}: {diff}");
    }
}

/// RED: the shared assertion rejects a CRLF/LF change in ordinary prose —
/// exactly the shape the removed whole-stream normalizer hid.
#[test]
#[should_panic(expected = "an added CR in prose")]
fn baseline_assertion_rejects_a_prose_newline_the_normalizer_hid() {
    // `draft_one_shot_dry_run.stdout` is CLI prose only: no RON portion, so a
    // platform regression here must never be excused as a RON newline.
    let expected = expected_stdout("draft_one_shot_dry_run");
    let mut crlf = Vec::with_capacity(expected.len() + 8);
    for &byte in &expected {
        if byte == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(byte);
    }
    assert_eq!(
        without_crlf(&crlf),
        without_crlf(&expected),
        "the removed normalizer hid this exact difference"
    );
    assert_raw_bytes("an added CR in prose", &crlf, &expected);
}

/// RED: a single unexpected content byte is rejected too.
#[test]
#[should_panic(expected = "a flipped content byte")]
fn baseline_assertion_rejects_a_flipped_content_byte() {
    let expected = expected_stdout("draft_one_shot_dry_run");
    let mut flipped = expected.clone();
    flipped[0] ^= 1;
    assert_raw_bytes("a flipped content byte", &flipped, &expected);
}

/// RED: each committed `lf/` stream variant is the capture-host record
/// with only the justified newlines converted, so the other host's expectation
/// cannot silently change any other byte. The accepted-file branches are
/// covered end-to-end by the real-binary accept tests instead.
#[test]
fn lf_stream_variants_differ_from_the_capture_only_by_newlines() {
    for name in LF_STDOUT_VARIANTS {
        let base = std::fs::read(format!("{BASELINE_DIR}/{name}.stdout")).unwrap();
        let lf = std::fs::read(format!("{BASELINE_DIR}/lf/{name}.stdout")).unwrap();
        assert!(
            !lf.contains(&b'\r'),
            "{name}: an LF stream variant must have no CR"
        );
        assert_eq!(
            lf,
            without_crlf(&base),
            "{name}: the stream variant must convert only the CRLF newlines"
        );
    }
}

/// Run one captured scenario through the spawned binary with the `--pack` flag
/// the capture used, and compare stdout, stderr and exit status byte-for-byte.
fn assert_baseline(name: &str, fx: &Fixture, args: &[&str], stdin: &str) {
    let mut owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    owned.push("--pack".into());
    owned.push(fx.pack_root.to_str().unwrap().into());
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let output = run_cli(&refs, &[])
        .write_stdin(stdin.to_string())
        .output()
        .unwrap();

    let expected_status: i32 = std::fs::read_to_string(format!("{BASELINE_DIR}/{name}.status"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(expected_status),
        "{name}: status"
    );
    assert_raw_bytes(
        &format!("{name}: stdout bytes"),
        &output.stdout,
        &expected_stdout(name),
    );
    assert_raw_bytes(
        &format!("{name}: stderr bytes"),
        &output.stderr,
        &expected_stderr(name),
    );
}

/// The valid NPC reply the capture's accepted scenarios used. A single
/// criterion keeps the rendered RON byte-stable: `Criteria::Choice` is a
/// `HashMap`, whose iteration order is not stable across processes.
fn valid_npc_reply() -> String {
    serde_json::json!({
        "GEN": {
            "instructions": "How does Ace greet, given trust?",
            "state_keys": ["affinity_level", "met_ace"],
            "criteria": {
                "warm": "affectionate"
            },
            "fallback": "warm"
        }
    })
    .to_string()
}

/// One-shot NPC exhaustion: the last raw reply is labelled `best`, every error
/// is listed in order, the exit status is 1 and the NPC file is untouched.
#[test]
fn baseline_npc_one_shot_exhausted() {
    let fx = Fixture::new();
    fx.push("raw reply one");
    fx.push("raw reply two");
    fx.push("raw reply three");
    let endpoint = format!("http://{}", fx.addr);
    let before = std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap();

    assert_baseline(
        "npc_one_shot_exhausted",
        &fx,
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
        ],
        "",
    );

    assert_eq!(
        std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap(),
        before,
        "exhaustion writes nothing"
    );
}

/// One-shot NPC `--dry-run`: the rendered RON and the dry-run line on stdout,
/// nothing written.
#[test]
fn baseline_npc_one_shot_dry_run() {
    let fx = Fixture::new();
    fx.push(&valid_npc_reply());
    let endpoint = format!("http://{}", fx.addr);
    let before = std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap();

    assert_baseline(
        "npc_one_shot_dry_run",
        &fx,
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "--dry-run",
        ],
        "",
    );

    assert_eq!(
        std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap(),
        before,
        "--dry-run writes nothing"
    );
}

/// Interactive NPC exhaustion omits the raw replies (its distinct residual
/// rendering), then `discard` acknowledges and exits 0.
#[test]
fn baseline_npc_interactive_exhausted_discard() {
    let fx = Fixture::new();
    fx.push("raw one");
    fx.push("raw two");
    fx.push("raw three");
    let endpoint = format!("http://{}", fx.addr);
    let before = std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap();

    assert_baseline(
        "npc_interactive_exhausted_discard",
        &fx,
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
            "--brief",
            "warm greeting",
        ],
        "discard\n",
    );

    assert_eq!(
        std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap(),
        before,
        "discard writes nothing"
    );
}

/// A terminal provider failure after a content rejection stops immediately:
/// no exhaustion block, no later feedback prompt, no write.
#[test]
fn baseline_npc_interactive_provider_after_rejection() {
    let fx = Fixture::new();
    fx.push("raw one");
    // The refused body carries material that must not survive into the
    // retained diagnostic: the dummy key and a URL (ADR 0014).
    fx.push_http(500, REFUSING_BODY);
    let endpoint = format!("http://{}", fx.addr);
    let before = std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap();

    assert_baseline(
        "npc_interactive_provider_after_rejection",
        &fx,
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
            "--brief",
            "warm greeting",
        ],
        "feedback for the retry\n",
    );

    assert_eq!(
        std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap(),
        before,
        "a provider failure writes nothing"
    );
}

/// One-shot prose exhaustion: last raw labelled `best`, all gate errors in
/// order including duplicates, nonzero status, scene untouched.
#[test]
fn baseline_draft_one_shot_exhausted() {
    let fx = Fixture::new();
    fx.push("Bob: one");
    fx.push("Bob: two");
    fx.push("Bob: three");
    let endpoint = format!("http://{}", fx.addr);
    let scene = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read(&scene).unwrap();

    assert_baseline(
        "draft_one_shot_exhausted",
        &fx,
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
        ],
        "",
    );

    assert_eq!(
        std::fs::read(&scene).unwrap(),
        before,
        "exhaustion writes nothing"
    );
}

/// One-shot prose `--dry-run`: preview plus the dry-run line, nothing written.
#[test]
fn baseline_draft_one_shot_dry_run() {
    let fx = Fixture::new();
    fx.push("Ace: Who goes there?");
    let endpoint = format!("http://{}", fx.addr);
    let scene = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read(&scene).unwrap();

    assert_baseline(
        "draft_one_shot_dry_run",
        &fx,
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "--dry-run",
        ],
        "",
    );

    assert_eq!(
        std::fs::read(&scene).unwrap(),
        before,
        "--dry-run writes nothing"
    );
}

/// Interactive prose accept: preview, prompt, acknowledgement and the exact
/// written scene bytes.
#[test]
fn baseline_draft_interactive_accept() {
    let fx = Fixture::new();
    fx.push("Ace: Who goes there?");
    let endpoint = format!("http://{}", fx.addr);

    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let source = without_crlf(&std::fs::read(&scene_path).unwrap());
    std::fs::write(&scene_path, &source).unwrap();
    assert!(!is_crlf_input(&source));

    assert_baseline(
        "draft_interactive_accept",
        &fx,
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
        ],
        "\naccept\n",
    );

    assert_raw_bytes(
        "the accepted splice",
        &std::fs::read(&scene_path).unwrap(),
        &expected_accepted_scene(false),
    );
}

/// RED: the accepted Scene follows a CRLF source through the real
/// binary — the other half of the LF capture.
#[test]
fn baseline_draft_interactive_accept_follows_a_crlf_source() {
    let fx = Fixture::new();
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let source = with_crlf(&std::fs::read(&scene_path).unwrap());
    std::fs::write(&scene_path, &source).unwrap();
    assert!(is_crlf_input(&source));

    fx.push("Ace: Who goes there?");
    let endpoint = format!("http://{}", fx.addr);

    assert_baseline(
        "draft_interactive_accept",
        &fx,
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
        ],
        "\naccept\n",
    );

    assert_raw_bytes(
        "the accepted splice",
        &std::fs::read(&scene_path).unwrap(),
        &expected_accepted_scene(true),
    );
}

/// Interactive prose discard: acknowledgement, exit 0, scene untouched.
#[test]
fn baseline_draft_interactive_discard() {
    let fx = Fixture::new();
    fx.push("Ace: Who goes there?");
    let endpoint = format!("http://{}", fx.addr);
    let scene = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read(&scene).unwrap();

    assert_baseline(
        "draft_interactive_discard",
        &fx,
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
        ],
        "\ndiscard\n",
    );

    assert_eq!(
        std::fs::read(&scene).unwrap(),
        before,
        "discard writes nothing"
    );
}

/// Blank input at the preview prompt keeps prompting and never writes.
#[test]
fn baseline_draft_interactive_blank_then_discard() {
    let fx = Fixture::new();
    fx.push("Ace: Who goes there?");
    let endpoint = format!("http://{}", fx.addr);
    let scene = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read(&scene).unwrap();

    assert_baseline(
        "draft_interactive_blank_then_discard",
        &fx,
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
        ],
        "\n\n\ndiscard\n",
    );

    assert_eq!(
        std::fs::read(&scene).unwrap(),
        before,
        "nothing was written"
    );
}

/// One-shot prose: a rejection then a terminal provider error stops at the
/// provider error and writes nothing.
#[test]
fn baseline_draft_one_shot_provider_after_rejection() {
    let fx = Fixture::new();
    fx.push("Bob: one");
    // The refused body carries material that must not survive into the
    // retained diagnostic: the dummy key and a URL (ADR 0014).
    fx.push_http(500, REFUSING_BODY);
    let endpoint = format!("http://{}", fx.addr);
    let scene = fx.pack_root.join("scenes/first_meeting.unmei");
    let before = std::fs::read(&scene).unwrap();

    assert_baseline(
        "draft_one_shot_provider_after_rejection",
        &fx,
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
        ],
        "",
    );

    assert_eq!(
        std::fs::read(&scene).unwrap(),
        before,
        "a provider failure writes nothing"
    );
}

/// Interactive NPC accept: preview, prompt, acknowledgement and the exact
/// written NPC bytes.
#[test]
fn baseline_npc_interactive_accept() {
    let fx = Fixture::new();
    fx.push(&valid_npc_reply());
    let endpoint = format!("http://{}", fx.addr);

    let npc_path = fx.pack_root.join("npcs/ace.ron");
    let crlf_template = is_crlf_input(&std::fs::read(&npc_path).unwrap());

    assert_baseline(
        "npc_interactive_accept",
        &fx,
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
            "--brief",
            "warm greeting",
        ],
        "accept\n",
    );

    assert_raw_bytes(
        "the accepted config",
        &std::fs::read(&npc_path).unwrap(),
        &expected_accepted_npc(crlf_template),
    );
}

/// RED: the accepted NPC follows an LF template through the real binary
/// — the other half of the mixed CRLF-template capture.
#[test]
fn baseline_npc_interactive_accept_follows_an_lf_template() {
    let fx = Fixture::new();
    let npc_path = fx.pack_root.join("npcs/ace.ron");
    let template = without_crlf(&std::fs::read(&npc_path).unwrap());
    std::fs::write(&npc_path, &template).unwrap();
    assert!(!is_crlf_input(&template));

    fx.push(&valid_npc_reply());
    let endpoint = format!("http://{}", fx.addr);

    assert_baseline(
        "npc_interactive_accept",
        &fx,
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
            "--brief",
            "warm greeting",
        ],
        "accept\n",
    );

    assert_raw_bytes(
        "the accepted config",
        &std::fs::read(&npc_path).unwrap(),
        &expected_accepted_npc(false),
    );
}

/// RED: the accepted NPC follows a CRLF template through the real
/// binary — the capture host's style, forced so it also runs on an LF checkout.
#[test]
fn baseline_npc_interactive_accept_follows_a_crlf_template() {
    let fx = Fixture::new();
    let npc_path = fx.pack_root.join("npcs/ace.ron");
    let template = with_crlf(&std::fs::read(&npc_path).unwrap());
    std::fs::write(&npc_path, &template).unwrap();
    assert!(is_crlf_input(&template));

    fx.push(&valid_npc_reply());
    let endpoint = format!("http://{}", fx.addr);

    assert_baseline(
        "npc_interactive_accept",
        &fx,
        &[
            "author",
            "npc",
            "ace",
            "--question",
            "greet",
            "--model",
            "author/m1",
            "--base-url",
            &endpoint,
            "-i",
            "--brief",
            "warm greeting",
        ],
        "accept\n",
    );

    assert_raw_bytes(
        "the accepted config",
        &std::fs::read(&npc_path).unwrap(),
        &expected_accepted_npc(true),
    );
}

// ---------------------------------------------------------------------------
// Terminal target refusal in the spawned binary (author-cli contract §10)
// ---------------------------------------------------------------------------

/// Run the binary with an unsafe target, a configured loopback endpoint and
/// missing provider configuration, and assert the refusal preempts everything:
/// exit 1, escaped contextual stderr, empty stdout, and no request to the
/// separately configured endpoint.
fn assert_target_refusal(fx: &Fixture, args: &[&str], needle: &str) {
    let endpoint = format!("http://{}", fx.addr);
    let mut owned: Vec<&str> = args.to_vec();
    owned.push("--base-url");
    owned.push(&endpoint);

    let out = run_cli(&owned, &[("UNMEI_API_KEY", ""), ("OPENROUTER_API_KEY", "")])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stderr.contains(needle),
        "escaped contextual stderr: {stderr}"
    );
    assert!(stdout.is_empty(), "no stdout preview/success: {stdout}");
    assert_eq!(out.status.code(), Some(1), "exit 1");
    assert!(
        fx.requests.lock().unwrap().is_empty(),
        "the configured loopback endpoint received no request"
    );
}

/// Every file directly under `npcs/`, sorted.
fn npc_files(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("npcs"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// RED: an unsafe raw NPC id refuses before a missing model and missing
/// credentials, and the forbidden later provider success is never consumed.
#[test]
fn npc_unsafe_target_refuses_before_missing_model_and_credentials() {
    let fx = Fixture::new();
    let pack = fx.pack_root.to_str().unwrap();
    let before = std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap();
    fx.push(
        r#"{"GEN":{"instructions":"How does Ace greet?","state_keys":["affinity_level"],"criteria":{"warm":"warm","cold":"cold"},"fallback":"cold"}}"#,
    );

    assert_target_refusal(
        &fx,
        &[
            "author",
            "npc",
            "../evil",
            "--create",
            "--question",
            "greet",
            "--pack",
            pack,
        ],
        "unsafe NPC target \"../evil\"",
    );

    assert_eq!(
        fx.script.lock().unwrap().len(),
        1,
        "the forbidden provider reply was never consumed"
    );
    assert_eq!(
        npc_files(&fx.pack_root),
        vec!["ace.ron"],
        "no NPC file was written and no sibling temporary left"
    );
    assert_eq!(
        std::fs::read(fx.pack_root.join("npcs/ace.ron")).unwrap(),
        before,
        "the protected NPC bytes are unchanged"
    );
}

/// RED: the refusal precedes a missing pack configuration too — the raw
/// filename check runs before pack resolution.
#[test]
fn npc_unsafe_replace_target_refuses_before_a_missing_pack() {
    let fx = Fixture::new();
    let missing = fx.pack_root.join("does-not-exist");
    assert_target_refusal(
        &fx,
        &[
            "author",
            "npc",
            "../evil",
            "--question",
            "greet",
            "--pack",
            missing.to_str().unwrap(),
        ],
        "unsafe NPC target \"../evil\"",
    );
}

/// RED: the `--list` route refuses an unsafe Scene id before any model,
/// credentials or pack configuration.
#[test]
fn draft_unsafe_scene_list_refuses_before_configuration() {
    let fx = Fixture::new();
    let pack = fx.pack_root.to_str().unwrap();
    assert_target_refusal(
        &fx,
        &["author", "draft", "../evil", "--list", "--pack", pack],
        "unsafe Scene target \"../evil\"",
    );
}

/// RED: the `--dry-run` route refuses before a missing brief file and a
/// missing model, and prints no preview.
#[test]
fn draft_unsafe_scene_dry_run_refuses_before_missing_brief_file_and_model() {
    let fx = Fixture::new();
    let pack = fx.pack_root.to_str().unwrap();
    let missing_brief = fx.pack_root.join("no-brief.md");
    assert_target_refusal(
        &fx,
        &[
            "author",
            "draft",
            "../evil",
            "--from",
            "start",
            "--dry-run",
            "--brief-file",
            missing_brief.to_str().unwrap(),
            "--pack",
            pack,
        ],
        "unsafe Scene target \"../evil\"",
    );
}

/// RED: the interactive route refuses before it can prompt for a brief
/// or read feedback from stdin.
#[test]
fn draft_unsafe_scene_interactive_refuses_before_the_brief_prompt() {
    let fx = Fixture::new();
    let pack = fx.pack_root.to_str().unwrap();
    assert_target_refusal(
        &fx,
        &[
            "author",
            "draft",
            "../evil",
            "--from",
            "start",
            "--interactive",
            "--pack",
            pack,
        ],
        "unsafe Scene target \"../evil\"",
    );
}

/// A draft write into a scene that had a leading BOM leaves a file that loads,
/// with the mark dropped and only the addressed run changed.
#[test]
fn draft_write_into_a_bom_scene_drops_the_mark_and_stays_loadable() {
    let fx = Fixture::new();
    let endpoint = format!("http://{}", fx.addr);
    let scene_path = fx.pack_root.join("scenes/first_meeting.unmei");
    let original = std::fs::read_to_string(&scene_path).unwrap();
    std::fs::write(&scene_path, format!("\u{feff}{original}")).unwrap();
    fx.push("Ace: Who goes there? You don't look like you're from the school.\n");

    run_cli(
        &[
            "author",
            "draft",
            "first_meeting",
            "--from",
            "start#2",
            "--brief",
            "Ace is guarded; the player is a stranger.",
            "--pack",
            fx.pack_root.to_str().unwrap(),
            "--base-url",
            &endpoint,
            "--model",
            "author/m1",
        ],
        &[],
    )
    .assert()
    .success();

    let after = std::fs::read_to_string(&scene_path).unwrap();
    assert!(!after.starts_with('\u{feff}'), "the write drops the mark");
    assert!(!after.contains('\u{feff}'), "no mark is left in the file");

    let before_lines: Vec<&str> = original.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    assert_eq!(
        after_lines.len(),
        before_lines.len(),
        "no line added/removed"
    );
    for (i, (old, new)) in before_lines.iter().zip(&after_lines).enumerate() {
        if i == 9 {
            continue; // line 10 is the `start#2` run
        }
        assert_eq!(old, new, "line {} outside the run changed", i + 1);
    }
    assert_eq!(
        after_lines[9],
        "Ace: Who goes there? You don't look like you're from the school."
    );
    unmei_engine::load_pack(&fx.pack_root).expect("the written scene still loads");
}
