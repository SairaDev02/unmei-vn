//! Tests for the generation core (author-cli contract §3/§5/§6).

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use unmei::gen::{
    assemble_prompt, AttemptOutcome, GenerateInput, GenerationError, PromptData, PUBLIC_API_SHAPE,
    RETRY_LIMIT,
};
use unmei::provider::{
    AuthorChat, Message, ProviderError, ShapeProblem, StructuredOutput, StructuredUnsupported,
    TransportFailure,
};

fn demo_pack() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack")
}

/// A scripted fake: pops one response per call; an Err entry terminates.
struct FakeChat {
    script: std::sync::Mutex<VecDeque<Result<String, ProviderError>>>,
    /// Served forever once the script runs out (see [`FakeChat::always`]).
    repeat: Option<Result<String, ProviderError>>,
    calls: std::sync::Mutex<Vec<Vec<Message>>>,
    /// Structured requests; NPC generation must never make one.
    structured_calls: std::sync::Mutex<usize>,
}

impl FakeChat {
    fn ok(responses: &[&str]) -> Self {
        FakeChat {
            script: Mutex::new(responses.iter().map(|r| Ok(r.to_string())).collect()),
            repeat: None,
            calls: Mutex::new(Vec::new()),
            structured_calls: Mutex::new(0),
        }
    }
    /// Always serves the same free-text reply (used when a run retries).
    fn always(response: &str) -> Self {
        FakeChat {
            script: Mutex::new(VecDeque::new()),
            repeat: Some(Ok(response.to_string())),
            calls: Mutex::new(Vec::new()),
            structured_calls: Mutex::new(0),
        }
    }
    fn calls(&self) -> Vec<Vec<Message>> {
        self.calls.lock().unwrap().clone()
    }
    fn structured_calls(&self) -> usize {
        *self.structured_calls.lock().unwrap()
    }
}

impl AuthorChat for FakeChat {
    async fn complete(&self, _model: &str, messages: &[Message]) -> Result<String, ProviderError> {
        self.calls.lock().unwrap().push(messages.to_vec());
        self.script
            .lock()
            .unwrap()
            .pop_front()
            .or_else(|| self.repeat.clone())
            .unwrap_or(Err(ProviderError::Shape(ShapeProblem::NotJson)))
    }

    async fn complete_structured(
        &self,
        _model: &str,
        _messages: &[Message],
        _structured: &StructuredOutput,
    ) -> Result<String, ProviderError> {
        *self.structured_calls.lock().unwrap() += 1;
        Err(ProviderError::NoStructuredSupport {
            reason: StructuredUnsupported::NotImplemented,
        })
    }
}

// ---------------------------------------------------------------------------
// Prompt assembly (§6)
// ---------------------------------------------------------------------------

/// RED: the prompt carries the state vocabulary with types + bucketing guidance.
#[tokio::test]
async fn prompt_includes_state_vocabulary() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let messages = assemble_prompt(&PromptData::from_pack(&loaded.pack, "ace", "greet"));
    let joined: String = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(joined.contains("affinity_level"), "{joined}");
    assert!(joined.contains("letters_found"), "{joined}");
    // Types and bucketing guidance are present.
    assert!(joined.to_lowercase().contains("bucketed"), "{joined}");
    assert!(joined.to_lowercase().contains("string"), "{joined}");
}

/// RED: the prompt carries the existing config when replacing.
#[tokio::test]
async fn prompt_includes_existing_config_when_replacing() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let mut data = PromptData::from_pack(&loaded.pack, "ace", "greet");
    data.existing = Some(unmei::gen::SampleConfig("existing warm config".into()));
    let messages = assemble_prompt(&data);
    let joined: String = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(joined.contains("existing warm config"), "{joined}");
    // And says this replaces an existing config.
    assert!(joined.to_lowercase().contains("replac"), "{joined}");
}

/// RED: `--create` (no existing config) asks the model for identity fields too.
#[tokio::test]
async fn create_mode_asks_for_identity_fields() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let mut data = PromptData::from_pack(&loaded.pack, "nova", "greet");
    data.creating = true;
    let messages = assemble_prompt(&data);
    let joined: String = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(joined.contains("nova"), "{joined}");
    assert!(joined.to_lowercase().contains("\"id\""), "{joined}");
    assert!(joined.to_lowercase().contains("\"name\""), "{joined}");
}

/// RED: the glossary is included when present.
#[tokio::test]
async fn prompt_includes_glossary() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let messages = assemble_prompt(&PromptData::from_pack(&loaded.pack, "ace", "greet"));
    let joined: String = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(joined.contains("pack-local terms"), "{joined}");
}

/// RED: the prompt demands the exact GEN envelope + ChoiceDecisionConfig shape.
#[tokio::test]
async fn prompt_demands_gen_envelope() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let messages = assemble_prompt(&PromptData::from_pack(&loaded.pack, "ace", "greet"));
    let joined: String = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(joined.contains("GEN"), "{joined}");
    assert!(joined.contains("instructions"), "{joined}");
    assert!(joined.contains("criteria"), "{joined}");
    assert!(joined.contains("fallback"), "{joined}");
    assert!(joined.contains("state_keys"), "{joined}");
}

/// RED: the brief is included when given.
#[tokio::test]
async fn prompt_includes_brief() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let mut data = PromptData::from_pack(&loaded.pack, "ace", "greet");
    data.brief = Some("her farewell softens if the ledger was found".into());
    let messages = assemble_prompt(&data);
    let joined: String = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(joined.contains("her farewell softens"), "{joined}");
}

// ---------------------------------------------------------------------------
// Retries + validation gate (§3/§5)
// ---------------------------------------------------------------------------

/// RED: a valid model response parses, validates against the demo pack, and is
/// returned as accepted.
#[tokio::test]
async fn valid_response_is_accepted() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let body = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"cold"}}"#;
    let chat = FakeChat::ok(&[body]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    assert!(matches!(attempt, AttemptOutcome::Accepted { .. }));
    assert_eq!(
        chat.structured_calls(),
        0,
        "NPC generation never requests structured output"
    );
}

/// RED: garbage JSON → retry with the parse error fed back as a correction;
/// the retry's messages include the previous assistant reply and the correction.
#[tokio::test]
async fn invalid_json_retries_with_correction() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let body = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"cold"}}"#;
    let chat = FakeChat::ok(&["not json at all {", body]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    assert!(matches!(attempt, AttemptOutcome::Accepted { .. }));
    let calls = chat.calls();
    // The correction references the failure and repeats the shape demand.
    let retry_correction = calls[1].last().unwrap();
    assert_eq!(
        retry_correction.role, "user",
        "retry ends on the correction turn"
    );
    assert!(
        retry_correction.content.contains("rejected"),
        "correction: {}",
        retry_correction.content
    );
    assert!(
        retry_correction.content.contains("GEN"),
        "correction: {}",
        retry_correction.content
    );
    // And the conversation carries the failed assistant reply for continuity.
    assert_eq!(
        calls[1].len(),
        calls[0].len() + 2,
        "retry adds assistant + user turn"
    );
    assert_eq!(calls[1][calls[1].len() - 2].role, "assistant");
}

/// RED: 3 failures → AttemptsExhausted carrying all errors; write-or-nothing is
/// the caller's job, but the core reports exactly what failed.
#[tokio::test]
async fn three_failures_reports_attempts_exhausted() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let chat = FakeChat::ok(&["nope 1", "nope 2", "nope 3"]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let err = unmei::gen::generate_once(&input).await.unwrap_err();
    match err {
        GenerationError::AttemptsExhausted {
            raw_outputs,
            errors,
        } => {
            assert_eq!(raw_outputs.len(), 3);
            // Every attempt's failures are carried (3 x one parse error each).
            assert!(errors.len() >= 3, "{errors:?}");
            assert!(
                errors.iter().any(|e| e.to_lowercase().contains("json")),
                "{errors:?}"
            );
        }
        other => panic!("expected AttemptsExhausted, got {other:?}"),
    }
}

/// RED: a provider HTTP error is not retried as a content problem — it surfaces
/// immediately (the endpoint is down; retrying content won't fix it).
#[tokio::test]
async fn provider_error_surfaces_immediately() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    struct DownChat;
    impl AuthorChat for DownChat {
        async fn complete(&self, _: &str, _: &[Message]) -> Result<String, ProviderError> {
            Err(ProviderError::Status { status: 500 })
        }
    }
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &DownChat,
        model: "author/m1",
    };
    let err = unmei::gen::generate_once(&input).await.unwrap_err();
    assert!(matches!(err, GenerationError::Provider(_)), "{err:?}");
}

/// RED: a terminal provider error after a content rejection stops
/// immediately; a later scripted success is an unreachable sentinel and is
/// never consulted.
#[tokio::test]
async fn npc_provider_error_after_rejection_never_reaches_the_later_success() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let sentinel = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate"},"fallback":"warm"}}"#;
    let chat = FakeChat {
        script: Mutex::new(
            vec![
                Ok("not json".to_string()),
                Err(ProviderError::Http(TransportFailure::Connect)),
                Ok(sentinel.to_string()),
            ]
            .into(),
        ),
        repeat: None,
        calls: Mutex::new(Vec::new()),
        structured_calls: Mutex::new(0),
    };
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let err = unmei::gen::generate_once(&input).await.unwrap_err();

    assert!(matches!(err, GenerationError::Provider(_)), "{err:?}");
    assert_eq!(
        chat.calls().len(),
        2,
        "the unreachable sentinel success is never requested"
    );
}

/// RED: schema fix-ups — the model emits `id`/`name` on replace (it shouldn't),
/// or criteria with untrimmed ids; the core cleans and validates.
#[tokio::test]
async fn schema_fixups_strip_npid_and_trim() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let body = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"cold"},"NPC":{"id":"ace","name":"Ace"}}"#;
    let chat = FakeChat::ok(&[body]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    match attempt {
        AttemptOutcome::Accepted {
            config,
            identity: _,
        } => {
            assert_eq!(config.instructions, "How does Ace greet, given trust?");
            assert_eq!(
                config.fallback,
                unmei_engine::Fallback::Action("cold".into())
            );
        }
        other => panic!("expected Accepted, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Typed generation: `--type noul` / `--type score`
// ---------------------------------------------------------------------------

/// RED: the prompt asks for the `noul` shape — boolean fallback, both
/// `true`/`false` criteria keys — when the type is noul.
#[tokio::test]
async fn prompt_demands_noul_shape() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let mut data = PromptData::from_pack(&loaded.pack, "ace", "is_hostile");
    data.question_type = unmei_engine::QuestionType::Noul;
    let joined: String = assemble_prompt(&data)
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert!(joined.contains("noul"), "{joined}");
    assert!(joined.contains("\"true\""), "{joined}");
    assert!(joined.contains("\"false\""), "{joined}");
    assert!(joined.contains("true|false"), "{joined}");
}

/// RED: the prompt asks for the `score` shape — ordered level array, 0-based
/// index fallback — when the type is score.
#[tokio::test]
async fn prompt_demands_score_shape() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let mut data = PromptData::from_pack(&loaded.pack, "ace", "trust");
    data.question_type = unmei_engine::QuestionType::Score;
    let joined: String = assemble_prompt(&data)
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert!(joined.contains("score"), "{joined}");
    assert!(joined.to_lowercase().contains("array"), "{joined}");
    assert!(joined.to_lowercase().contains("lowest first"), "{joined}");
    assert!(joined.to_lowercase().contains("index"), "{joined}");
}

/// RED: requesting `noul` parses the model's bool/true-false JSON into a typed
/// config and returns it on attempt 1.
#[tokio::test]
async fn noul_type_generates_typed_config() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let body = r#"{"GEN":{"instructions":"Is the player acting hostile toward Ace?","state_keys":["affinity_level","met_ace"],"criteria":{"true":"Openly hostile or dismissive.","false":"Neutral or friendly."},"fallback":false}}"#;
    let chat = FakeChat::ok(&[body]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "is_hostile",
        question_type: unmei_engine::QuestionType::Noul,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    match attempt {
        AttemptOutcome::Accepted { config, .. } => {
            assert_eq!(config.question_type, unmei_engine::QuestionType::Noul);
            assert_eq!(config.fallback, unmei_engine::Fallback::Bool(false));
            assert!(matches!(
                config.criteria,
                unmei_engine::Criteria::Noul(Some(_))
            ));
        }
        other => panic!("expected Accepted, got {other:?}"),
    }
    assert_eq!(chat.calls().len(), 1);
}

/// RED: requesting `score` parses the model's ordered array + numeric fallback
/// into a typed config and returns it on attempt 1.
#[tokio::test]
async fn score_type_generates_typed_config() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let body = r#"{"GEN":{"instructions":"How much does Ace trust the player?","state_keys":["affinity_level","letters_found"],"criteria":["No trust at all.","Wary but willing to talk.","Complete trust."],"fallback":0}}"#;
    let chat = FakeChat::ok(&[body]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "trust",
        question_type: unmei_engine::QuestionType::Score,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    match attempt {
        AttemptOutcome::Accepted { config, .. } => {
            assert_eq!(config.question_type, unmei_engine::QuestionType::Score);
            assert_eq!(config.fallback, unmei_engine::Fallback::Index(0));
            assert_eq!(config.score_criteria().unwrap().len(), 3);
        }
        other => panic!("expected Accepted, got {other:?}"),
    }
    assert_eq!(chat.calls().len(), 1);
}

/// RED: a reply shaped for the wrong type (a criteria map when `score` was
/// requested) fails the gate and is retried as a correction.
#[tokio::test]
async fn wrong_typed_shape_is_a_correction() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let wrong = r#"{"GEN":{"instructions":"Trust?","state_keys":["affinity_level"],"criteria":{"low":"low","high":"high"},"fallback":"low"}}"#;
    let right = r#"{"GEN":{"instructions":"Trust?","state_keys":["affinity_level"],"criteria":["None.","Some."],"fallback":0}}"#;
    let chat = FakeChat::ok(&[wrong, right]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "trust",
        question_type: unmei_engine::QuestionType::Score,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    assert!(matches!(attempt, AttemptOutcome::Accepted { .. }));
    let calls = chat.calls();
    assert_eq!(calls.len(), 2, "wrong shape triggers a correction retry");
    let correction = calls[1].last().unwrap();
    assert!(
        correction.content.contains("array"),
        "correction names the score shape: {}",
        correction.content
    );
}

/// RED: a typed config that is shape-valid but fails the shared loader gate
/// (score fallback outside the legend) is fed back as a correction, then a
/// valid one is accepted.
#[tokio::test]
async fn typed_loader_rejection_is_a_correction() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let out_of_range = r#"{"GEN":{"instructions":"Trust?","state_keys":["affinity_level"],"criteria":["None.","Some."],"fallback":5}}"#;
    let fixed = r#"{"GEN":{"instructions":"Trust?","state_keys":["affinity_level"],"criteria":["None.","Some."],"fallback":1}}"#;
    let chat = FakeChat::ok(&[out_of_range, fixed]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "trust",
        question_type: unmei_engine::QuestionType::Score,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    assert!(matches!(attempt, AttemptOutcome::Accepted { .. }));
    let calls = chat.calls();
    assert_eq!(
        calls.len(),
        2,
        "loader rejection triggers a correction retry"
    );
    let correction = calls[1].last().unwrap();
    assert!(
        correction.content.contains("legend"),
        "correction carries the loader's legend error: {}",
        correction.content
    );
}

// ---------------------------------------------------------------------------
// Accepted implies the candidate was installed and loader-validated
// ---------------------------------------------------------------------------

/// A `choice` config whose `state_keys` names an undeclared key, so the real
/// loader rejects it wherever it is actually installed and validated.
const INVALID_STATE_KEY_GEN: &str = r#"{"GEN":{"instructions":"How does Ace greet?","state_keys":["affinity_level","not_declared"],"criteria":{"warm":"warm greeting","cold":"guarded greeting","none":"neither fits"},"fallback":"cold"}}"#;

/// A real temp copy of the demo pack, so a regeneration witness can remove a
/// source file after the pack loaded without touching the repository's pack.
fn demo_pack_copy() -> (tempfile::TempDir, std::path::PathBuf) {
    let demo = demo_pack();
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
    (dir, root)
}

/// Every file under `root` with its bytes, for a non-destructive check.
fn snapshot(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

/// A `choice` generation request for the demo pack's `ace.greet` question.
fn greet_input<'a>(
    pack: &'a unmei_engine::Pack,
    chat: &'a FakeChat,
) -> GenerateInput<'a, FakeChat> {
    GenerateInput {
        pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: chat,
        model: "author/m1",
    }
}

/// Insert an extra NPC whose decision references an undeclared state key, so
/// the real loader reports an unrelated content error on every staged reload.
/// Its file, decision and key deliberately contain none of the candidate file,
/// question or NPC id, so the command's substring filter must drop it.
fn load_pack_with_unrelated_npc_error(root: &Path) -> unmei_engine::Pack {
    let mut pack = unmei_engine::load_pack(root).unwrap().pack;
    let ghost_config: unmei_engine::content::DecisionConfig =
        serde_json::from_value(serde_json::json!({
            "instructions": "Does the ghost haunt?",
            "state_keys": ["ghost_only_key"],
            "criteria": { "yes": "it haunts", "no": "it does not" },
            "fallback": "no",
        }))
        .unwrap();
    let ghost = unmei_engine::content::Npc {
        id: "ghost".to_string(),
        name: "Ghost".to_string(),
        model: None,
        decisions: std::collections::HashMap::from([("haunt".to_string(), ghost_config)]),
    };
    std::fs::write(
        root.join("npcs/ghost.ron"),
        ron::ser::to_string(&ghost).unwrap(),
    )
    .unwrap();
    pack.npcs.insert("ghost".to_string(), ghost);
    pack
}

/// Removing a source file after the pack loaded makes the transplant fail, so
/// the candidate is never installed or loader-validated. The run is a terminal
/// preparation error, not `Accepted`, and writes nothing.
#[tokio::test]
async fn deleted_source_file_is_a_terminal_preparation_failure() {
    let (_dir, root) = demo_pack_copy();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    std::fs::remove_file(root.join("npcs/ace.ron")).unwrap();
    let before = snapshot(&root);
    let chat = FakeChat::always(INVALID_STATE_KEY_GEN);

    let err = unmei::gen::generate_once(&greet_input(&loaded.pack, &chat))
        .await
        .unwrap_err();

    assert!(
        matches!(err, GenerationError::Preparation(_)),
        "a missing source file is terminal, not content: {err:?}"
    );
    let text = err.to_string();
    assert!(
        text.contains("not installed or validated"),
        "the diagnostic carries the explicit context: {text}"
    );
    let underlying = text
        .split_once("candidate was not installed or validated:")
        .map(|(_, rest)| rest.trim())
        .unwrap_or_default();
    assert!(
        !underlying.is_empty(),
        "the verbatim underlying message is carried: {text}"
    );
    assert_eq!(
        chat.calls().len(),
        1,
        "a preparation failure spends no retry"
    );
    assert_eq!(
        snapshot(&root),
        before,
        "nothing is written and authored bytes are unchanged"
    );
    assert!(
        !root.join("npcs/ace.ron").exists(),
        "intentional absence stays absent"
    );
}

/// The same preparation failure with a *valid* candidate must still terminate
/// rather than accept a candidate the loader never saw.
#[tokio::test]
async fn deleted_scene_source_never_accepts_a_valid_candidate_unvalidated() {
    let (_dir, root) = demo_pack_copy();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    std::fs::remove_file(root.join("scenes/reunion.unmei")).unwrap();
    let before = snapshot(&root);
    let chat = FakeChat::always(VALID_GREET);

    let err = unmei::gen::generate_once(&greet_input(&loaded.pack, &chat))
        .await
        .unwrap_err();

    assert!(
        matches!(err, GenerationError::Preparation(_)),
        "a valid candidate cannot be accepted without validation: {err:?}"
    );
    assert_eq!(chat.calls().len(), 1, "no retry is spent");
    assert_eq!(snapshot(&root), before, "authored bytes are unchanged");
}

/// The staged read/parse swallow is gone — a source NPC file that the
/// loader accepted at pack load but no longer parses fails terminally instead
/// of leaving the candidate silently uninstalled.
#[tokio::test]
async fn unparseable_staged_npc_file_is_a_terminal_preparation_failure() {
    let (_dir, root) = demo_pack_copy();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    std::fs::write(root.join("npcs/ace.ron"), "this is not an Npc").unwrap();
    let before = snapshot(&root);
    let chat = FakeChat::always(VALID_GREET);

    let err = unmei::gen::generate_once(&greet_input(&loaded.pack, &chat))
        .await
        .unwrap_err();

    assert!(
        matches!(err, GenerationError::Preparation(_)),
        "an unparseable staged NPC file is terminal: {err:?}"
    );
    assert_eq!(chat.calls().len(), 1, "no retry is spent");
    assert_eq!(snapshot(&root), before, "authored bytes are unchanged");
}

/// Control with an intact pack: the same undeclared-state-key candidate still
/// reaches the real loader, remains a content correction, and consumes the
/// retry budget to exhaustion — unchanged by the repair.
#[tokio::test]
async fn loader_content_rejection_still_exhausts_the_retry_budget() {
    let (_dir, root) = demo_pack_copy();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let before = snapshot(&root);
    let chat = FakeChat::always(INVALID_STATE_KEY_GEN);

    let err = unmei::gen::generate_once(&greet_input(&loaded.pack, &chat))
        .await
        .unwrap_err();

    match err {
        GenerationError::AttemptsExhausted { errors, .. } => {
            assert_eq!(chat.calls().len(), RETRY_LIMIT, "three content attempts");
            assert!(
                errors
                    .iter()
                    .any(|e| e.contains("not_declared") && e.contains("npcs/ace.ron")),
                "the real loader rejects the undeclared key: {errors:?}"
            );
        }
        other => panic!("expected AttemptsExhausted, got {other:?}"),
    }
    assert_eq!(snapshot(&root), before, "nothing is written");
}

/// The outer substring filter still applies to loader content only — a
/// valid candidate is accepted even though an unrelated NPC carries a
/// pre-existing loader error, and that diagnostic is never a correction.
#[tokio::test]
async fn unrelated_pack_error_is_dropped_and_a_valid_candidate_is_accepted() {
    let (_dir, root) = demo_pack_copy();
    let pack = load_pack_with_unrelated_npc_error(&root);
    let chat = FakeChat::always(VALID_GREET);

    let attempt = unmei::gen::generate_once(&greet_input(&pack, &chat))
        .await
        .unwrap();

    assert!(
        matches!(attempt, AttemptOutcome::Accepted { .. }),
        "the unrelated diagnostic does not block a valid candidate"
    );
    assert_eq!(chat.calls().len(), 1);
}

/// The filter's other direction — an independently invalid candidate
/// still carries only its own diagnostics; the unrelated pre-existing error
/// never becomes correction noise.
#[tokio::test]
async fn unrelated_pack_error_never_reaches_the_candidate_corrections() {
    let (_dir, root) = demo_pack_copy();
    let pack = load_pack_with_unrelated_npc_error(&root);
    let chat = FakeChat::always(INVALID_STATE_KEY_GEN);

    let err = unmei::gen::generate_once(&greet_input(&pack, &chat))
        .await
        .unwrap_err();

    match err {
        GenerationError::AttemptsExhausted { errors, .. } => {
            assert!(
                errors.iter().any(|e| e.contains("npcs/ace.ron")),
                "the candidate's own diagnostic is retained: {errors:?}"
            );
            assert!(
                !errors
                    .iter()
                    .any(|e| e.contains("npcs/ghost.ron") || e.contains("ghost_only_key")),
                "the unrelated pre-existing diagnostic is filtered out: {errors:?}"
            );
        }
        other => panic!("expected AttemptsExhausted, got {other:?}"),
    }
    assert_eq!(chat.calls().len(), RETRY_LIMIT);
}

/// A temp copy of the demo pack whose `reunion` scene also labels
/// `reply.Warm`, so a mixed-case criterion id is otherwise loader-valid: only
/// the CLI's own id-grammar pre-check rejects it.
fn pack_with_uppercase_label() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, root) = demo_pack_copy();
    let reunion = root.join("scenes/reunion.unmei");
    let mut text = std::fs::read_to_string(&reunion).unwrap();
    text.push_str("\nlabel reply.Warm\nAce: Warm.\njump end_reunion\n");
    std::fs::write(&reunion, text).unwrap();
    (dir, root)
}

/// RED: the layer-1 gate rejects a `choice` criterion id that is not
/// snake_case even when the loader would accept it — the stricter CLI-only
/// rule — and the correction names the offending id.
#[tokio::test]
async fn non_snake_case_criterion_id_is_a_correction() {
    let (_dir, root) = pack_with_uppercase_label();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let bad = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"Warm":"affectionate","none":"neither fits"},"fallback":"none"}}"#;
    let fixed = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","none":"neither fits"},"fallback":"none"}}"#;
    let chat = FakeChat::ok(&[bad, fixed]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    assert!(matches!(attempt, AttemptOutcome::Accepted { .. }));
    let calls = chat.calls();
    assert_eq!(calls.len(), 2, "a malformed id triggers a correction retry");
    let correction = calls[1].last().unwrap();
    assert!(
        correction.content.contains("Warm") && correction.content.contains("snake_case"),
        "the correction names the offending id and the rule: {}",
        correction.content
    );
    assert!(
        !correction.content.contains("missing label"),
        "`reply.Warm` exists, so the loader accepts this id — only the gate rejects it: {}",
        correction.content
    );
}

/// RED: an empty criterion id keeps the old check's `is_empty`
/// guarantee, now via the shared id grammar.
#[tokio::test]
async fn empty_criterion_id_is_a_correction() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let bad = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"":"affectionate","none":"neither fits"},"fallback":"none"}}"#;
    let fixed = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","none":"neither fits"},"fallback":"none"}}"#;
    let chat = FakeChat::ok(&[bad, fixed]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    assert!(matches!(attempt, AttemptOutcome::Accepted { .. }));
    let calls = chat.calls();
    assert_eq!(calls.len(), 2, "an empty id triggers a correction retry");
    let correction = calls[1].last().unwrap();
    assert!(
        correction.content.contains("empty criterion id"),
        "the correction names the empty id: {}",
        correction.content
    );
}

/// RED: the same rule covers the `choice` fallback.
#[tokio::test]
async fn non_snake_case_fallback_is_a_correction() {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let bad = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"Cold"}}"#;
    let fixed = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"cold"}}"#;
    let chat = FakeChat::ok(&[bad, fixed]);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };
    let attempt = unmei::gen::generate_once(&input).await.unwrap();
    assert!(matches!(attempt, AttemptOutcome::Accepted { .. }));
    let calls = chat.calls();
    assert_eq!(
        calls.len(),
        2,
        "a malformed fallback triggers a correction retry"
    );
    let correction = calls[1].last().unwrap();
    assert!(
        correction.content.contains("Cold") && correction.content.contains("snake_case"),
        "the correction names the offending fallback and the rule: {}",
        correction.content
    );
}

// ---------------------------------------------------------------------------
// Contract constants
// ---------------------------------------------------------------------------

/// RED: the retry limit is 3 attempts (contract §5).
#[test]
fn retry_limit_is_three() {
    assert_eq!(RETRY_LIMIT, 3);
}

/// RED: the module exposes exactly the pipeline surface the contract names.
#[test]
fn public_surface_is_pipeline_shaped() {
    assert_eq!(
        PUBLIC_API_SHAPE,
        "GenerateInput -> (complete attempt x RETRY_LIMIT) -> AttemptOutcome<DecisionConfig>"
    );
}

// ---------------------------------------------------------------------------
// Drafting one run of prose (contract §3/§4)
// ---------------------------------------------------------------------------

/// The demo pack plus the exact source bytes of one scene.
fn demo_draft_setup(scene_id: &str) -> (unmei_engine::Pack, unmei_engine::Scene, String) {
    let root = demo_pack();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let scene = loaded.pack.scenes.get(scene_id).unwrap().clone();
    let text =
        std::fs::read_to_string(root.join("scenes").join(format!("{scene_id}.unmei"))).unwrap();
    (loaded.pack, scene, text)
}

/// Resolve a demo scene's run handle to the [`unmei::draft::Run`] the drafting
/// core splices and re-validates.
fn demo_run(scene: &unmei_engine::Scene, handle: &str) -> unmei::draft::Run {
    let runs = unmei::draft::runs(scene);
    unmei::draft::resolve(&runs, "first_meeting", handle)
        .expect("demo run resolves")
        .clone()
}

/// RED: context is assembled automatically — whole target scene, every NPC
/// file, the glossary, the state vocabulary, the brief and the allowed cast.
#[test]
fn draft_prompt_carries_scene_cast_state_glossary_and_brief() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let checked = unmei::target::CheckedPack::new(&pack).unwrap();
    let data = unmei::gen::DraftPromptData::from_checked(
        &checked,
        "first_meeting",
        "start#2",
        &text,
        &allowed,
        Some("Ace is guarded; the player is a stranger."),
    );
    let joined: String = unmei::gen::assemble_draft_prompt(&data)
        .iter()
        .map(|m| m.content.as_str())
        .collect();

    assert!(joined.contains("start#2"), "the addressed run: {joined}");
    assert!(
        joined.contains("Ace: You're not from around here"),
        "the whole target scene: {joined}"
    );
    assert!(joined.contains("npcs/ace.ron"), "every NPC file: {joined}");
    assert!(
        joined.contains("guarded student"),
        "the pack glossary: {joined}"
    );
    assert!(
        joined.contains("affinity_level"),
        "the state vocabulary: {joined}"
    );
    assert!(
        joined.to_lowercase().contains("bucketed"),
        "bucketed-string guidance: {joined}"
    );
    assert!(
        joined.contains("Ace is guarded; the player is a stranger."),
        "the brief: {joined}"
    );
    assert!(
        joined.contains("Narrator") && joined.contains("Ace"),
        "the allowed speakers: {joined}"
    );
}

/// RED: a well-formed reply passes the layer-1 gate and returns rendered lines.
#[tokio::test]
async fn draft_once_returns_gated_say_lines() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let chat = FakeChat::ok(&["Ace: Who goes there?\nNarrator: A pause."]);
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: Some("guarded"),
        client: &chat,
        model: "author/m1",
    };

    let lines = unmei::gen::draft_once(&input).await.unwrap().lines;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].render(), "Ace: Who goes there?");
    assert_eq!(lines[1].render(), "Narrator: A pause.");
    assert_eq!(chat.calls().len(), 1, "one free-text generation call");
}

/// RED: a reply that tries to smuggle a command or an unknown speaker through
/// fails the gate and names the problem.
#[tokio::test]
async fn draft_once_rejects_commands_and_unknown_speakers() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);

    let command = FakeChat::always("sprite ace happy\nAce: Hi\n");
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &command,
        model: "author/m1",
    };
    match unmei::gen::draft_once(&input).await.unwrap_err() {
        unmei::gen::DraftError::AttemptsExhausted { errors, .. } => {
            assert!(errors.iter().any(|e| e.contains("keyword")), "{errors:?}");
        }
        other => panic!("expected AttemptsExhausted, got {other:?}"),
    }

    let stranger = FakeChat::always("Bob: Who goes there?\n");
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &stranger,
        model: "author/m1",
    };
    match unmei::gen::draft_once(&input).await.unwrap_err() {
        unmei::gen::DraftError::AttemptsExhausted { errors, .. } => {
            assert!(errors.iter().any(|e| e.contains("Bob")), "{errors:?}");
        }
        other => panic!("expected AttemptsExhausted, got {other:?}"),
    }
}

/// RED: a provider error surfaces immediately, not as a content retry.
#[tokio::test]
async fn draft_once_propagates_provider_errors() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    struct DownChat;
    impl AuthorChat for DownChat {
        async fn complete(&self, _: &str, _: &[Message]) -> Result<String, ProviderError> {
            Err(ProviderError::Status { status: 503 })
        }
    }
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &DownChat,
        model: "author/m1",
    };
    let err = unmei::gen::draft_once(&input).await.unwrap_err();
    assert!(
        matches!(err, unmei::gen::DraftError::Provider(_)),
        "{err:?}"
    );
}

/// RED: layer 2 re-parses the edited scene and re-cross-validates through the
/// shared loader, reporting its file+line errors — and accepts a good splice.
#[test]
fn validate_edited_scene_accepts_good_and_reports_loader_errors() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let runs = unmei::draft::runs(&scene);
    let run = unmei::draft::resolve(&runs, "first_meeting", "start#2").unwrap();

    let good = unmei::draft::splice_run(&text, run, &["Ace: Who goes there?".to_string()]);
    unmei::gen::validate_edited_scene(&pack, "first_meeting", &good).unwrap();

    let bad = unmei::draft::splice_run(
        &text,
        run,
        &["Ace: You have {missing_key} letters.".to_string()],
    );
    let errors = unmei::gen::validate_edited_scene(&pack, "first_meeting", &bad).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("scenes/first_meeting.unmei") && e.contains("missing_key")),
        "loader file+line error: {errors:?}"
    );
}

// ---------------------------------------------------------------------------
// Structured output, retry and free-text fallback (contract §7)
// ---------------------------------------------------------------------------

/// A fake with explicit structured-output behaviour: scripted replies for
/// `complete_structured` and `complete`, plus the messages each received.
struct StructuredFake {
    structured: Mutex<VecDeque<Result<String, ProviderError>>>,
    plain: Mutex<VecDeque<Result<String, ProviderError>>>,
    structured_messages: Mutex<Vec<Vec<Message>>>,
    plain_messages: Mutex<Vec<Vec<Message>>>,
}

impl StructuredFake {
    fn new(structured: Vec<Result<String, ProviderError>>, plain: &[&str]) -> Self {
        StructuredFake {
            structured: Mutex::new(structured.into_iter().collect()),
            plain: Mutex::new(plain.iter().map(|r| Ok(r.to_string())).collect()),
            structured_messages: Mutex::new(Vec::new()),
            plain_messages: Mutex::new(Vec::new()),
        }
    }
    fn structured_messages(&self) -> Vec<Vec<Message>> {
        self.structured_messages.lock().unwrap().clone()
    }
    fn plain_messages(&self) -> Vec<Vec<Message>> {
        self.plain_messages.lock().unwrap().clone()
    }
    fn push_plain(&self, reply: Result<String, ProviderError>) {
        self.plain.lock().unwrap().push_back(reply);
    }
}

impl AuthorChat for StructuredFake {
    async fn complete(&self, _model: &str, messages: &[Message]) -> Result<String, ProviderError> {
        self.plain_messages.lock().unwrap().push(messages.to_vec());
        self.plain
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ProviderError::Shape(ShapeProblem::NotJson)))
    }

    async fn complete_structured(
        &self,
        _model: &str,
        messages: &[Message],
        _structured: &StructuredOutput,
    ) -> Result<String, ProviderError> {
        self.structured_messages
            .lock()
            .unwrap()
            .push(messages.to_vec());
        self.structured
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(ProviderError::Shape(ShapeProblem::NotJson)))
    }
}

/// RED (contract §7): the structured `LINES` items re-serialise to
/// `Speaker: text`, with a colon in the text and escaped quotes carried safely
/// through the JSON string.
#[tokio::test]
async fn draft_structured_reply_is_reserialised_safely() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let reply = r#"{"LINES":[{"speaker":"Narrator","text":"He said: \"go\" — and left."},{"speaker":"Ace","text":"Fine: I'll wait."}]}"#;
    let chat = StructuredFake::new(vec![Ok(reply.to_string())], &[]);
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let lines = unmei::gen::draft_once(&input).await.unwrap().lines;
    assert_eq!(lines[0].render(), "Narrator: He said: \"go\" — and left.");
    assert_eq!(lines[1].render(), "Ace: Fine: I'll wait.");
    assert_eq!(chat.structured_messages().len(), 1, "structured first");
    assert!(chat.plain_messages().is_empty(), "no fallback needed");
}

/// RED: a structured text containing a newline cannot become one
/// `Speaker: text` line, so it is rejected and fed back as a correction.
#[tokio::test]
async fn draft_structured_newline_is_a_correction() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let bad = r#"{"LINES":[{"speaker":"Ace","text":"one\ntwo"}]}"#;
    let good = r#"{"LINES":[{"speaker":"Ace","text":"one two"}]}"#;
    let chat = StructuredFake::new(vec![Ok(bad.to_string()), Ok(good.to_string())], &[]);
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let lines = unmei::gen::draft_once(&input).await.unwrap().lines;
    assert_eq!(lines[0].render(), "Ace: one two");
    let calls = chat.structured_messages();
    assert!(
        calls[1].last().unwrap().content.contains("newline"),
        "correction names the newline: {}",
        calls[1].last().unwrap().content
    );
}

/// RED (contract §7): an endpoint that lacks structured-output support falls
/// back automatically to free text, and the fallback prompt asks for say-lines.
#[tokio::test]
async fn draft_falls_back_to_free_text_when_structured_unsupported() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let chat = StructuredFake::new(
        vec![Err(ProviderError::NoStructuredSupport {
            reason: StructuredUnsupported::ResponseFormat,
        })],
        &["Ace: Who goes there?"],
    );
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let outcome = unmei::gen::draft_once(&input).await.unwrap();
    assert_eq!(outcome.lines[0].render(), "Ace: Who goes there?");
    assert!(
        outcome.fallback_detail == Some(StructuredUnsupported::ResponseFormat),
        "the fallback reason is carried, not swallowed: {outcome:?}"
    );
    assert_eq!(
        chat.structured_messages().len(),
        1,
        "structured requested first"
    );
    let plain = chat.plain_messages();
    assert_eq!(plain.len(), 1, "free-text fallback used once");
    let free_text_prompt: String = plain[0].iter().map(|m| m.content.as_str()).collect();
    assert!(
        !free_text_prompt.contains("\"LINES\""),
        "the fallback asks for say-lines, not LINES JSON: {free_text_prompt}"
    );
    assert!(free_text_prompt.contains("say-line"), "{free_text_prompt}");
}

/// RED: negotiation to plain text happens inside attempt 1, so an
/// invalid plain reply then a valid one lands on attempt 2 — the extra HTTP
/// call does not spend a validation attempt.
#[tokio::test]
async fn draft_negotiated_invalid_then_valid_plain_lands_on_attempt_2() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let chat = StructuredFake::new(
        vec![Err(ProviderError::NoStructuredSupport {
            reason: StructuredUnsupported::ResponseFormat,
        })],
        &["Bob: hi", "Ace: Who goes there?"],
    );
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);

    let outcome = unmei::gen::draft_once(&input).await.unwrap();

    assert_eq!(
        outcome.attempt, 2,
        "the same-turn negotiation did not spend a validation attempt"
    );
    assert_eq!(outcome.lines[0].render(), "Ace: Who goes there?");
    assert_eq!(
        chat.structured_messages().len(),
        1,
        "structured output is requested once"
    );
    assert_eq!(chat.plain_messages().len(), 2, "one plain call per attempt");
}

/// RED: when the negotiated plain call fails terminally, the session
/// still remembers that the endpoint cannot honour structured output, so the
/// next regeneration goes straight to plain text and reports the same reason.
#[tokio::test]
async fn draft_capability_mode_and_reason_survive_a_terminal_plain_failure() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let chat = StructuredFake::new(
        vec![Err(ProviderError::NoStructuredSupport {
            reason: StructuredUnsupported::ResponseFormat,
        })],
        &[],
    );
    chat.push_plain(Err(ProviderError::Http(TransportFailure::Connect)));
    chat.push_plain(Ok("Ace: Fine.".to_string()));
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);
    let mut session = unmei::gen::DraftSession::new();

    let first = unmei::gen::draft_with_session(&input, &mut session).await;
    assert!(
        matches!(first, Err(unmei::gen::DraftError::Provider(_))),
        "{first:?}"
    );
    session.set_feedback(Some("shorter".to_string()));

    let second = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();

    assert_eq!(second.lines[0].render(), "Ace: Fine.");
    assert!(
        second.fallback_detail == Some(StructuredUnsupported::ResponseFormat),
        "the negotiation reason survives the terminal failure: {second:?}"
    );
    assert_eq!(
        chat.structured_messages().len(),
        1,
        "structured output is not requested again after the negotiation"
    );
    assert_eq!(chat.plain_messages().len(), 2);
}

/// RED (contract §5): validation failures retry up to 3 attempts, then all
/// raw replies and every error are reported.
#[tokio::test]
async fn draft_retries_validation_failures_up_to_three() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let bad = r#"{"LINES":[{"speaker":"Bob","text":"hi"}]}"#;
    let chat = StructuredFake::new(
        vec![
            Ok(bad.to_string()),
            Ok(bad.to_string()),
            Ok(bad.to_string()),
        ],
        &[],
    );
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    match unmei::gen::draft_once(&input).await.unwrap_err() {
        unmei::gen::DraftError::AttemptsExhausted {
            raw_outputs,
            errors,
        } => {
            assert_eq!(raw_outputs.len(), RETRY_LIMIT);
            assert_eq!(raw_outputs[0], bad);
            assert!(
                errors.iter().any(|e| e.contains("Bob")),
                "every error is reported: {errors:?}"
            );
        }
        other => panic!("expected AttemptsExhausted, got {other:?}"),
    }
    let calls = chat.structured_messages();
    assert!(
        calls[1].last().unwrap().content.contains("rejected"),
        "the retry carries the correction: {}",
        calls[1].last().unwrap().content
    );
}

/// RED (contract §7): a provider error on the structured request surfaces
/// immediately — it is not retried as a content problem and does not fall back.
#[tokio::test]
async fn draft_provider_error_surfaces_immediately() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let chat = StructuredFake::new(vec![Err(ProviderError::Status { status: 500 })], &[]);
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let err = unmei::gen::draft_once(&input).await.unwrap_err();
    assert!(
        matches!(err, unmei::gen::DraftError::Provider(_)),
        "{err:?}"
    );
    assert_eq!(chat.structured_messages().len(), 1, "no retry");
    assert!(chat.plain_messages().is_empty(), "no fallback");
}

/// RED (contract §7): the structured prompt demands the `LINES` JSON shape.
#[test]
fn draft_prompt_asks_for_lines_json_when_structured() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let checked = unmei::target::CheckedPack::new(&pack).unwrap();
    let mut data = unmei::gen::DraftPromptData::from_checked(
        &checked,
        "first_meeting",
        "start#2",
        &text,
        &allowed,
        Some("guarded"),
    );
    data.structured = true;
    let joined: String = unmei::gen::assemble_draft_prompt(&data)
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert!(joined.contains("\"LINES\""), "{joined}");
    assert!(joined.to_lowercase().contains("json"), "{joined}");
}

/// RED (contract §4): layer 2 runs inside the retry loop — a candidate
/// that passes the say-line gate but breaks the shared loader (`{missing_key}`
/// is not declared in state.ron) is fed back as a correction and the next
/// attempt lands.
#[tokio::test]
async fn draft_retries_layer_2_loader_failures() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let broken = r#"{"LINES":[{"speaker":"Ace","text":"You have {missing_key} letters."}]}"#;
    let good = r#"{"LINES":[{"speaker":"Ace","text":"Who goes there?"}]}"#;
    let chat = StructuredFake::new(vec![Ok(broken.to_string()), Ok(good.to_string())], &[]);
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "first_meeting",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let outcome = unmei::gen::draft_once(&input).await.unwrap();
    assert_eq!(outcome.attempt, 2, "layer-2 failure consumed attempt 1");
    assert_eq!(outcome.lines[0].render(), "Ace: Who goes there?");
    assert!(outcome.edited_scene.contains("Ace: Who goes there?"));
    let calls = chat.structured_messages();
    assert!(
        calls[1].last().unwrap().content.contains("missing_key"),
        "the layer-2 loader error is fed back: {}",
        calls[1].last().unwrap().content
    );
}

// ---------------------------------------------------------------------------
// The interactive `-i` session core (contract §5/§7)
// ---------------------------------------------------------------------------

/// Build the per-round input the interactive session rebuilds each loop.
fn draft_input<'a, C: AuthorChat>(
    pack: &'a unmei_engine::Pack,
    run: &'a unmei::draft::Run,
    text: &'a str,
    allowed: &'a std::collections::HashSet<String>,
    chat: &'a C,
) -> unmei::gen::DraftInput<'a, C> {
    unmei::gen::DraftInput {
        pack,
        scene_id: "first_meeting",
        run,
        scene_text: text,
        allowed_speakers: allowed,
        brief: None,
        client: chat,
        model: "author/m1",
    }
}

/// RED (contract §5): interactive free-text feedback rides the next
/// round's prompt, and a landed round clears the corrections so they cannot
/// leak into the feedback round that follows.
#[tokio::test]
async fn draft_session_carries_feedback_into_the_next_round() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let first = r#"{"LINES":[{"speaker":"Ace","text":"Who goes there?"}]}"#;
    let second = r#"{"LINES":[{"speaker":"Ace","text":"Who goes there? Keep it guarded."}]}"#;
    let chat = StructuredFake::new(vec![Ok(first.to_string()), Ok(second.to_string())], &[]);
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);
    let mut session = unmei::gen::DraftSession::new();

    let landed = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(landed.lines[0].render(), "Ace: Who goes there?");
    assert!(
        session.corrections().is_empty(),
        "a landed regeneration starts clean: {:?}",
        session.corrections()
    );

    session.set_feedback(Some("make Ace more guarded".to_string()));
    let regenerated = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(
        regenerated.lines[0].render(),
        "Ace: Who goes there? Keep it guarded."
    );

    let calls = chat.structured_messages();
    let feedback_prompt: String = calls[1].iter().map(|m| m.content.as_str()).collect();
    assert!(
        feedback_prompt.contains("make Ace more guarded"),
        "feedback reaches the request: {feedback_prompt}"
    );
    // The previous draft rides as the assistant turn, so relative feedback
    // ("shorter, three lines") has a referent (contract §5).
    assert!(
        calls[1]
            .iter()
            .any(|m| m.role == "assistant" && m.content.contains("Ace: Who goes there?")),
        "the previous draft is visible to the model: {calls:?}"
    );
}

/// RED (contract §5): an exhausted round leaves its corrections on the
/// session; the next round feeds them back *with* the author's feedback and, on
/// success, clears them.
#[tokio::test]
async fn draft_session_carries_corrections_across_rounds_and_clears_on_success() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let bad = r#"{"LINES":[{"speaker":"Bob","text":"hi"}]}"#;
    let good = r#"{"LINES":[{"speaker":"Ace","text":"Fine."}]}"#;
    let chat = StructuredFake::new(
        vec![
            Ok(bad.to_string()),
            Ok(bad.to_string()),
            Ok(bad.to_string()),
            Ok(good.to_string()),
        ],
        &[],
    );
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);
    let mut session = unmei::gen::DraftSession::new();

    let err = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        unmei::gen::DraftError::AttemptsExhausted { .. }
    ));
    assert!(
        session.corrections().iter().any(|e| e.contains("Bob")),
        "the exhausted regeneration leaves its errors for the next: {:?}",
        session.corrections()
    );

    session.set_feedback(Some("use Ace instead".to_string()));
    let landed = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(landed.lines[0].render(), "Ace: Fine.");
    assert!(
        session.corrections().is_empty(),
        "a landed regeneration starts clean: {:?}",
        session.corrections()
    );

    let calls = chat.structured_messages();
    let retry_prompt: String = calls[3].iter().map(|m| m.content.as_str()).collect();
    assert!(
        retry_prompt.contains("Bob") && retry_prompt.contains("use Ace instead"),
        "corrections and feedback are both carried: {retry_prompt}"
    );
}

/// RED (contract §7): once the endpoint rejects structured output, later
/// rounds in the same session go straight to free text — no repeated doomed
/// structured request, and the fallback reason stays reported.
#[tokio::test]
async fn draft_session_keeps_free_text_mode_across_rounds() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let chat = StructuredFake::new(
        vec![Err(ProviderError::NoStructuredSupport {
            reason: StructuredUnsupported::ResponseFormat,
        })],
        &["Ace: Who goes there?", "Ace: Second pass."],
    );
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);
    let mut session = unmei::gen::DraftSession::new();

    let first = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(first.lines[0].render(), "Ace: Who goes there?");
    assert!(first.fallback_detail.is_some(), "the reason is reported");

    session.set_feedback(Some("shorter".to_string()));
    let second = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(second.lines[0].render(), "Ace: Second pass.");
    assert!(
        second.fallback_detail.is_some(),
        "the reason stays reported: {second:?}"
    );
    assert_eq!(
        chat.structured_messages().len(),
        1,
        "structured output is attempted only once per session"
    );
    assert_eq!(
        chat.plain_messages().len(),
        2,
        "one free-text call per regeneration"
    );
}

/// RED (contract §5): the feedback referent tracks the newest draft — the
/// second regeneration amends the second draft, not the first.
#[tokio::test]
async fn draft_session_updates_the_feedback_referent_each_regeneration() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let replies = [
        r#"{"LINES":[{"speaker":"Ace","text":"One."}]}"#,
        r#"{"LINES":[{"speaker":"Ace","text":"Two."}]}"#,
        r#"{"LINES":[{"speaker":"Ace","text":"Three."}]}"#,
    ];
    let chat = StructuredFake::new(replies.iter().map(|r| Ok(r.to_string())).collect(), &[]);
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);
    let mut session = unmei::gen::DraftSession::new();

    unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    session.set_feedback(Some("tighten".to_string()));
    unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    session.set_feedback(Some("shorter".to_string()));
    let third = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(third.lines[0].render(), "Ace: Three.");

    let calls = chat.structured_messages();
    let referent = assistant_turn(&calls[2]).expect("the feedback turn carries a previous draft");
    assert_eq!(
        referent.content, "Ace: Two.",
        "the referent is the newest draft, not the first"
    );
}

// ---------------------------------------------------------------------------
// Terminal target preflight (author-cli contract §10)
// ---------------------------------------------------------------------------

/// A `GEN` reply the demo pack validates, used as the forbidden later success
/// on the refusal paths.
const VALID_GREET: &str = r#"{"GEN":{"instructions":"How does Ace greet, given trust?","state_keys":["affinity_level","met_ace"],"criteria":{"warm":"affectionate","cold":"guarded","none":"neither fits"},"fallback":"cold"}}"#;

/// RED: an unsafe raw NPC target is a typed terminal refusal before the
/// provider is called — never a correction, retry or accepted candidate. The
/// escaped wording is not even eligible for the old NPC substring filter,
/// which is never reached.
#[tokio::test]
async fn generate_once_refuses_an_unsafe_npc_target_before_provider() {
    let loaded = unmei_engine::load_pack(&demo_pack()).unwrap();
    let chat = FakeChat::always(VALID_GREET);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "../evil",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let err = unmei::gen::generate_once(&input).await.unwrap_err();
    assert!(matches!(err, GenerationError::Target(_)), "{err:?}");
    assert!(err.to_string().contains("\"../evil\""), "escaped id: {err}");
    assert!(
        chat.calls().is_empty(),
        "refusal precedes the provider call"
    );
}

/// RED: failure 1 of the acceptance walk — an unsafe key in the
/// supplied Pack refuses even when the requested target itself is safe, and
/// the refusal wording carries no `npcs/ace.ron` or question substring the old
/// filter would have kept.
#[tokio::test]
async fn generate_once_refuses_an_unsafe_copied_pack_key_before_provider() {
    let mut loaded = unmei_engine::load_pack(&demo_pack()).unwrap();
    let ace = loaded.pack.npcs.get("ace").expect("demo has ace").clone();
    loaded.pack.npcs.insert("bad/id".to_string(), ace);

    let chat = FakeChat::always(VALID_GREET);
    let input = GenerateInput {
        pack: &loaded.pack,
        npc_id: "ace",
        question: "greet",
        question_type: unmei_engine::QuestionType::Choice,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let err = unmei::gen::generate_once(&input).await.unwrap_err();
    assert!(matches!(err, GenerationError::Target(_)), "{err:?}");
    let text = err.to_string();
    assert!(text.contains("\"bad/id\""), "offending key named: {text}");
    assert!(
        !text.contains("npcs/ace.ron") && !text.contains("greet"),
        "the refusal is not a correction-shaped message: {text}"
    );
    assert!(
        chat.calls().is_empty(),
        "refusal precedes the provider call"
    );
}

/// RED: an unsafe raw Scene target refuses before any provider call.
#[tokio::test]
async fn draft_once_refuses_an_unsafe_scene_target_before_provider() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let reply = r#"{"LINES":[{"speaker":"Ace","text":"Who goes there?"}]}"#;
    let chat = StructuredFake::new(vec![Ok(reply.to_string())], &[]);
    let run = demo_run(&scene, "start#2");
    let input = unmei::gen::DraftInput {
        pack: &pack,
        scene_id: "../evil",
        run: &run,
        scene_text: &text,
        allowed_speakers: &allowed,
        brief: None,
        client: &chat,
        model: "author/m1",
    };

    let err = unmei::gen::draft_once(&input).await.unwrap_err();
    assert!(matches!(err, unmei::gen::DraftError::Target(_)), "{err:?}");
    assert!(err.to_string().contains("\"../evil\""), "escaped id: {err}");
    assert!(chat.structured_messages().is_empty(), "no structured call");
    assert!(chat.plain_messages().is_empty(), "no provider call");
}

/// The last assistant turn of one provider request: the draft the session
/// hands the model as its own previous reply.
fn assistant_turn(request: &[Message]) -> Option<&Message> {
    request.iter().rev().find(|m| m.role == "assistant")
}

/// RED: an absent Scene refuses terminally inside a continuing
/// structured-mode session — no provider call, and the corrections and
/// feedback it already carried are left exactly as they were. The same session
/// then regenerates the real Scene on its first attempt, still speaks the
/// structured protocol, and hands the provider the exact retained corrections
/// and feedback.
#[tokio::test]
async fn draft_session_refusal_preserves_corrections_feedback_and_structured_protocol() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    // Three distinct rejections, so the retained corrections are told apart.
    let bad = [
        r#"{"LINES":[{"speaker":"Bob","text":"nope"}]}"#,
        r#"{"LINES":[{"speaker":"Zed","text":"hi"}]}"#,
        r#"{"LINES":[{"speaker":"Ace","text":"one\ntwo"}]}"#,
    ];
    let landed = r#"{"LINES":[{"speaker":"Ace","text":"Fine."}]}"#;
    let chat = StructuredFake::new(
        bad.iter()
            .map(|reply| Ok(reply.to_string()))
            .chain(std::iter::once(Ok(landed.to_string())))
            .collect(),
        &[],
    );
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);
    let mut session = unmei::gen::DraftSession::new();

    let exhausted = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap_err();
    assert!(
        matches!(exhausted, unmei::gen::DraftError::AttemptsExhausted { .. }),
        "{exhausted:?}"
    );
    // Three distinct corrections, each carried into the next round's prompt.
    let corrections_before = session.corrections().to_vec();
    assert_eq!(
        corrections_before.len(),
        RETRY_LIMIT,
        "exhaustion leaves every rejection's correction"
    );
    let structured_before = chat.structured_messages().len();
    let plain_before = chat.plain_messages().len();
    let feedback = "use Ace instead of Bob";
    session.set_feedback(Some(feedback.to_string()));

    let absent = unmei::gen::DraftInput {
        scene_id: "no_such_scene",
        ..input
    };
    let err = unmei::gen::draft_with_session(&absent, &mut session)
        .await
        .unwrap_err();
    assert!(matches!(err, unmei::gen::DraftError::Target(_)), "{err:?}");
    assert!(err.to_string().contains("\"no_such_scene\""), "{err}");
    assert_eq!(
        session.corrections(),
        corrections_before.as_slice(),
        "the refusal leaves the remembered corrections alone"
    );
    assert_eq!(
        chat.structured_messages().len(),
        structured_before,
        "no provider call on refusal"
    );
    assert_eq!(
        chat.plain_messages().len(),
        plain_before,
        "no fallback call on refusal"
    );

    let regenerated = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(
        regenerated.attempt, 1,
        "the regeneration after the refusal lands on its first attempt"
    );
    assert_eq!(regenerated.lines[0].render(), "Ace: Fine.");
    assert!(
        regenerated.fallback_detail.is_none(),
        "the session still speaks the structured protocol: {regenerated:?}"
    );

    let structured = chat.structured_messages();
    assert_eq!(
        structured.len(),
        structured_before + 1,
        "the continuing round requests structured output"
    );
    assert_eq!(
        chat.plain_messages().len(),
        plain_before,
        "no free-text fallback was introduced"
    );
    let turn = structured[structured_before]
        .last()
        .expect("the request ends with the corrections turn");
    assert_eq!(turn.role, "user");
    for correction in &corrections_before {
        assert!(
            turn.content.contains(correction.as_str()),
            "the exact retained correction is fed back: {correction:?} in {}",
            turn.content
        );
    }
    assert!(
        turn.content.contains(feedback),
        "the retained feedback reaches the provider: {}",
        turn.content
    );
    assert!(
        session.corrections().is_empty(),
        "the landed regeneration starts clean: {:?}",
        session.corrections()
    );
}

/// RED: an absent Scene refusal inside a continuing free-text session
/// leaves the negotiated capability, the remembered accepted draft and the
/// author's feedback intact — observed through the continuing round's request,
/// not through call counts or source ordering. The pre-refusal draft is the
/// exact referent the model is shown, and no doomed structured request is
/// re-issued.
///
/// This cannot be the same request that carries retained corrections: while
/// corrections are queued `draft_prompt` shows an empty failed-reply marker
/// instead of `last_draft`, and the corrections only clear on an acceptance,
/// which overwrites the remembered draft. The two retained values are therefore
/// each witnessed in their own continuing session.
#[tokio::test]
async fn draft_session_refusal_preserves_the_plain_capability_and_remembered_draft() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let allowed = unmei::draft::allowed_speakers(&scene, &pack);
    let remembered = "Ace: Who goes there?\nNarrator: The gate stays shut.";
    let later = "Ace: Who goes there? Back away from the letter.";
    let chat = StructuredFake::new(
        vec![Err(ProviderError::NoStructuredSupport {
            reason: StructuredUnsupported::ResponseFormat,
        })],
        &[remembered],
    );
    let run = demo_run(&scene, "start#2");
    let input = draft_input(&pack, &run, &text, &allowed, &chat);
    let mut session = unmei::gen::DraftSession::new();

    let first = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(first.lines.len(), 2);
    assert!(
        first.fallback_detail == Some(StructuredUnsupported::ResponseFormat),
        "the session negotiated free text: {first:?}"
    );
    let structured_before = chat.structured_messages().len();
    let plain_before = chat.plain_messages().len();
    let feedback = "keep the gate shut for now";
    session.set_feedback(Some(feedback.to_string()));

    let absent = unmei::gen::DraftInput {
        scene_id: "no_such_scene",
        ..input
    };
    let err = unmei::gen::draft_with_session(&absent, &mut session)
        .await
        .unwrap_err();
    assert!(matches!(err, unmei::gen::DraftError::Target(_)), "{err:?}");
    assert!(err.to_string().contains("\"no_such_scene\""), "{err}");
    assert_eq!(
        chat.structured_messages().len(),
        structured_before,
        "no provider call on refusal"
    );
    assert_eq!(
        chat.plain_messages().len(),
        plain_before,
        "no fallback call on refusal"
    );

    chat.push_plain(Ok(later.to_string()));
    let regenerated = unmei::gen::draft_with_session(&input, &mut session)
        .await
        .unwrap();
    assert_eq!(
        regenerated.attempt, 1,
        "the regeneration after the refusal lands on its first attempt"
    );
    assert_eq!(regenerated.lines[0].render(), later);
    assert!(
        regenerated.fallback_detail == Some(StructuredUnsupported::ResponseFormat),
        "the negotiated reason still survives: {regenerated:?}"
    );
    assert_eq!(
        chat.structured_messages().len(),
        structured_before,
        "the session does not re-negotiate structured output"
    );

    let plain = chat.plain_messages();
    assert_eq!(
        plain.len(),
        plain_before + 1,
        "one free-text request, no extra negotiation call"
    );
    let request = &plain[plain_before];
    let referent = assistant_turn(request).expect("the feedback turn carries a draft");
    assert_eq!(
        referent.content, remembered,
        "the exact remembered accepted draft is the referent: {request:?}"
    );
    let turn = request
        .last()
        .expect("the request ends with the feedback turn");
    assert_eq!(turn.role, "user");
    assert!(
        turn.content.contains(feedback),
        "the retained feedback reaches the provider: {}",
        turn.content
    );
}

/// RED: the public Scene helper keeps its vector result as one
/// target/membership refusal diagnostic, while its loader path is unchanged.
#[test]
fn validate_edited_scene_refuses_unsafe_and_absent_scenes_as_one_diagnostic() {
    let (pack, scene, text) = demo_draft_setup("first_meeting");
    let runs = unmei::draft::runs(&scene);
    let run = unmei::draft::resolve(&runs, "first_meeting", "start#2").unwrap();
    let good = unmei::draft::splice_run(&text, run, &["Ace: Who goes there?".to_string()]);

    let absent = unmei::gen::validate_edited_scene(&pack, "no_such_scene", &good).unwrap_err();
    assert_eq!(absent.len(), 1, "{absent:?}");
    assert!(absent[0].contains("\"no_such_scene\""), "{absent:?}");

    let unsafe_err = unmei::gen::validate_edited_scene(&pack, "../evil", &good).unwrap_err();
    assert_eq!(unsafe_err.len(), 1, "{unsafe_err:?}");
    assert!(unsafe_err[0].contains("unsafe Scene"), "{unsafe_err:?}");
}

/// A real temp pack carrying a second Scene under an ordinary-but-unusual id
/// (a dot in the stem), so the loader and staging exercise the exact
/// `scenes/act.one.unmei` filename on the host filesystem.
fn pack_with_unusual_scene_id() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, root) = demo_pack_copy();
    std::fs::copy(
        demo_pack().join("scenes/first_meeting.unmei"),
        root.join("scenes/act.one.unmei"),
    )
    .unwrap();
    (dir, root)
}

/// RED: the real loader accepts an ordinary-but-unusual Scene id, and
/// the public validator stages and re-validates it through the exact native
/// filename without touching the source bytes.
#[test]
fn validate_edited_scene_accepts_an_unusual_ordinary_scene_id() {
    let (_dir, root) = pack_with_unusual_scene_id();
    let loaded = unmei_engine::load_pack(&root).unwrap();
    let scene_id = "act.one";
    let scene = loaded
        .pack
        .scenes
        .get(scene_id)
        .expect("extra scene loads")
        .clone();
    let source_path = root.join("scenes/act.one.unmei");
    let text = std::fs::read_to_string(&source_path).unwrap();
    let runs = unmei::draft::runs(&scene);
    let run = unmei::draft::resolve(&runs, scene_id, "start#2").unwrap();
    let good = unmei::draft::splice_run(&text, run, &["Ace: Who goes there?".to_string()]);

    unmei::gen::validate_edited_scene(&loaded.pack, scene_id, &good).unwrap();
    assert_eq!(
        std::fs::read_to_string(&source_path).unwrap(),
        text,
        "the source Scene is never rewritten by validation"
    );
}

/// The NPC and drafting prompt texts for one pack directory, for the glossary
/// tests below.
fn prompt_texts(root: &Path) -> (String, String) {
    let loaded = unmei_engine::load_pack(root).unwrap();
    let npc: String = assemble_prompt(&PromptData::from_pack(&loaded.pack, "ace", "greet"))
        .iter()
        .map(|message| message.content.as_str())
        .collect();

    let scene = loaded.pack.scenes.get("first_meeting").unwrap().clone();
    let text = std::fs::read_to_string(root.join("scenes/first_meeting.unmei")).unwrap();
    let allowed = unmei::draft::allowed_speakers(&scene, &loaded.pack);
    let checked = unmei::target::CheckedPack::new(&loaded.pack).unwrap();
    let data = unmei::gen::DraftPromptData::from_checked(
        &checked,
        "first_meeting",
        "start#2",
        &text,
        &allowed,
        Some("keep it short"),
    );
    let draft: String = unmei::gen::assemble_draft_prompt(&data)
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    (npc, draft)
}

/// A glossary that starts with a BOM builds the same prompts as the same
/// glossary without one, for the NPC and the drafting builder.
#[test]
fn bom_glossary_builds_the_same_prompts() {
    let (_dir, root) = demo_pack_copy();
    let plain = prompt_texts(&root);

    let glossary = std::fs::read_to_string(root.join("pack.glossary.md")).unwrap();
    std::fs::write(root.join("pack.glossary.md"), format!("\u{feff}{glossary}")).unwrap();
    let with_bom = prompt_texts(&root);

    assert_eq!(with_bom, plain, "the BOM changed a prompt");
    assert!(
        !with_bom.0.contains('\u{feff}'),
        "the NPC prompt kept the mark"
    );
    assert!(
        !with_bom.1.contains('\u{feff}'),
        "the draft prompt kept the mark"
    );
}

/// A missing glossary leaves both prompts buildable with no glossary text, as
/// before.
#[test]
fn a_missing_glossary_builds_prompts_without_one() {
    let (_dir, root) = demo_pack_copy();
    std::fs::remove_file(root.join("pack.glossary.md")).unwrap();
    let (npc, draft) = prompt_texts(&root);

    assert!(!npc.contains("pack-local terms"), "{npc}");
    assert!(!draft.contains("pack-local terms"), "{draft}");
}
