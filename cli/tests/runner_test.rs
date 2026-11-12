//! Direct tests of the real command runner: injected chat and terminal
//! I/O over the real sessions, driver, complete attempts, staging, loader and
//! atomic writer — no attempt or driver fakes.

use std::collections::{HashSet, VecDeque};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use unmei::draft;
use unmei::provider::{
    AuthorChat, Message, ProviderError, ShapeProblem, StructuredOutput, TransportFailure,
};
use unmei::runner::{self, DraftCommand, NpcCommand, RunnerError, RunnerOutcome, Terminal};
#[cfg(windows)]
use unmei::target::{FileKind, FilenameFault, TargetError};
use unmei_engine::{Pack, QuestionType};

#[path = "common/pack.rs"]
mod pack;

use pack::pack_bytes;

/// A scripted chat: one queue for free-text replies and one for structured
/// replies, with every request batch recorded so prompt content stays visible.
struct Chat {
    free: Mutex<VecDeque<Result<String, ProviderError>>>,
    structured: Mutex<VecDeque<Result<String, ProviderError>>>,
    free_calls: Mutex<Vec<Vec<Message>>>,
    structured_calls: Mutex<Vec<Vec<Message>>>,
}

impl Chat {
    fn free(replies: Vec<Result<String, ProviderError>>) -> Self {
        Chat {
            free: Mutex::new(replies.into()),
            structured: Mutex::new(VecDeque::new()),
            free_calls: Mutex::new(Vec::new()),
            structured_calls: Mutex::new(Vec::new()),
        }
    }

    fn structured(replies: Vec<Result<String, ProviderError>>) -> Self {
        Chat {
            free: Mutex::new(VecDeque::new()),
            structured: Mutex::new(replies.into()),
            free_calls: Mutex::new(Vec::new()),
            structured_calls: Mutex::new(Vec::new()),
        }
    }

    fn free_prompts(&self) -> Vec<String> {
        self.free_calls
            .lock()
            .unwrap()
            .iter()
            .map(|batch| rendered(batch))
            .collect()
    }

    fn structured_prompts(&self) -> Vec<String> {
        self.structured_calls
            .lock()
            .unwrap()
            .iter()
            .map(|batch| rendered(batch))
            .collect()
    }
}

fn rendered(batch: &[Message]) -> String {
    batch
        .iter()
        .map(|message| format!("[{}] {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n")
}

impl AuthorChat for Chat {
    async fn complete(&self, _model: &str, messages: &[Message]) -> Result<String, ProviderError> {
        self.free_calls.lock().unwrap().push(messages.to_vec());
        self.free
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
        self.structured_calls
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

/// The demo pack's scenes; a test's extra id reuses `first_meeting`'s text.
const DEMO_SCENES: [&str; 3] = ["first_meeting", "reunion", "epilogue"];

/// A private copy of the demo pack plus the resolved run the draft commands
/// address, so a test's accepted write lands in its own temp directory.
struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    scene_id: String,
    scene_path: PathBuf,
    pack: Pack,
    scene_text: String,
    run: draft::Run,
    allowed: HashSet<String>,
}

fn fixture() -> Fixture {
    fixture_for("first_meeting")
}

/// The fixture with the demo's `first_meeting` content on disk under
/// `scenes/<scene_id>.unmei`, so the host filesystem carries that exact native
/// name for the loader, the staging validation and the writer.
fn fixture_for(scene_id: &str) -> Fixture {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("pack");
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    std::fs::create_dir_all(root.join("npcs")).unwrap();
    for file in ["pack.ron", "state.ron", "pack.glossary.md"] {
        std::fs::copy(demo.join(file), root.join(file)).unwrap();
    }
    for scene in DEMO_SCENES {
        std::fs::copy(
            demo.join(format!("scenes/{scene}.unmei")),
            root.join(format!("scenes/{scene}.unmei")),
        )
        .unwrap();
    }
    std::fs::copy(demo.join("npcs/ace.ron"), root.join("npcs/ace.ron")).unwrap();
    let scene_path = root.join("scenes").join(format!("{scene_id}.unmei"));
    if !DEMO_SCENES.contains(&scene_id) {
        std::fs::copy(demo.join("scenes/first_meeting.unmei"), &scene_path).unwrap();
    }

    let loaded = unmei_engine::load_pack(&root).unwrap();
    let scene = loaded.pack.scenes.get(scene_id).unwrap().clone();
    let scene_text = std::fs::read_to_string(&scene_path).unwrap();
    let runs = draft::runs(&scene);
    let run = draft::resolve(&runs, scene_id, "start").unwrap().clone();
    let allowed = draft::allowed_speakers(&scene, &loaded.pack);

    Fixture {
        _dir: dir,
        root,
        scene_id: scene_id.to_string(),
        scene_path,
        pack: loaded.pack,
        scene_text,
        run,
        allowed,
    }
}

/// Owned terminal buffers the runner borrows for one call.
struct Io {
    input: Cursor<Vec<u8>>,
    out: Vec<u8>,
    err: Vec<u8>,
}

impl Io {
    fn new(input: &str) -> Self {
        Io {
            input: Cursor::new(input.as_bytes().to_vec()),
            out: Vec::new(),
            err: Vec::new(),
        }
    }

    fn terminal(&mut self) -> Terminal<'_> {
        Terminal {
            input: &mut self.input,
            out: &mut self.out,
            err: &mut self.err,
        }
    }

    fn out(&self) -> String {
        String::from_utf8_lossy(&self.out).into_owned()
    }

    fn err(&self) -> String {
        String::from_utf8_lossy(&self.err).into_owned()
    }
}

fn valid_npc_reply() -> String {
    serde_json::json!({
        "GEN": {
            "instructions": "How does Ace greet, given trust?",
            "state_keys": ["affinity_level", "met_ace"],
            "criteria": { "warm": "affectionate" },
            "fallback": "warm"
        }
    })
    .to_string()
}

fn valid_draft_reply() -> String {
    r#"{"LINES":[{"speaker":"Ace","text":"Who goes there?"}]}"#.to_string()
}

fn bad_draft_reply() -> String {
    r#"{"LINES":[{"speaker":"Bob","text":"hi"}]}"#.to_string()
}

fn npc_command<'a>(fx: &'a Fixture, chat: &'a Chat) -> NpcCommand<'a, Chat> {
    NpcCommand {
        pack: &fx.pack,
        pack_root: &fx.root,
        npc_id: "ace",
        question: "greet",
        question_type: QuestionType::Choice,
        create: false,
        brief: Some("warm greeting"),
        interactive: true,
        dry_run: false,
        client: chat,
        model: "author/m1",
    }
}

fn draft_command<'a>(fx: &'a Fixture, chat: &'a Chat, dry_run: bool) -> DraftCommand<'a, Chat> {
    DraftCommand {
        pack: &fx.pack,
        pack_root: &fx.root,
        scene_id: &fx.scene_id,
        run: &fx.run,
        scene_text: &fx.scene_text,
        scene_path: &fx.scene_path,
        allowed_speakers: &fx.allowed,
        brief: Some("keep it short"),
        interactive: true,
        dry_run,
        client: chat,
        model: "author/m1",
    }
}

/// The same command in one-shot mode: no brief or feedback prompts, so the
/// accepted write is the only observable step after the attempt.
fn draft_once<'a>(fx: &'a Fixture, chat: &'a Chat) -> DraftCommand<'a, Chat> {
    DraftCommand {
        interactive: false,
        ..draft_command(fx, chat, false)
    }
}

/// Exhaustion → feedback → validated preview → accept: every exhausted error
/// reaches the next regeneration's first prompt, the feedback rides along, and
/// success clears the corrections before the next feedback turn.
#[tokio::test]
async fn npc_exhaustion_feedback_then_accept_carries_and_clears_corrections() {
    let fx = fixture();
    let chat = Chat::free(vec![
        Ok("raw one".into()),
        Ok("raw two".into()),
        Ok("raw three".into()),
        Ok(valid_npc_reply()),
        Ok(valid_npc_reply()),
    ]);
    let mut io = Io::new("use a warmer greeting\nshorter\naccept\n");

    let outcome = runner::run_npc(npc_command(&fx, &chat), &mut io.terminal())
        .await
        .unwrap();

    assert_eq!(outcome, RunnerOutcome::Completed);
    let err = io.err();
    assert!(
        err.contains("✖ all attempts failed validation:"),
        "the exhaustion block: {err}"
    );
    assert!(
        !err.contains("best model output"),
        "NPC interactive omits the raw replies: {err}"
    );
    let out = io.out();
    assert!(
        out.contains("--- preview (decisions[\"greet\"], not yet written) ---"),
        "{out}"
    );
    assert!(out.contains("Wrote npcs/ace.ron"), "{out}");

    let prompts = chat.free_prompts();
    assert_eq!(prompts.len(), 5, "three attempts, then two feedback rounds");
    assert!(
        prompts[1].contains("raw one") && !prompts[1].contains("raw two"),
        "attempt 2 sees the latest rejection: {}",
        prompts[1]
    );
    assert!(
        prompts[2].contains("raw two") && !prompts[2].contains("raw one"),
        "attempt 3 sees the latest rejection only: {}",
        prompts[2]
    );
    assert!(
        prompts[3].contains("raw one")
            && prompts[3].contains("raw two")
            && prompts[3].contains("raw three")
            && prompts[3].contains("use a warmer greeting"),
        "the post-exhaustion regeneration carries every error and the feedback: {}",
        prompts[3]
    );
    assert!(
        prompts[4].contains("shorter") && !prompts[4].contains("raw one"),
        "success cleared the stale corrections: {}",
        prompts[4]
    );

    let written = std::fs::read_to_string(fx.root.join("npcs/ace.ron")).unwrap();
    assert!(written.contains("warm"), "{written}");
}

/// A terminal provider error after a content rejection stops the runner before
/// any feedback prompt or write.
#[tokio::test]
async fn npc_provider_failure_after_rejection_stops_before_feedback_or_write() {
    let fx = fixture();
    let before = std::fs::read(fx.root.join("npcs/ace.ron")).unwrap();
    let chat = Chat::free(vec![
        Ok("raw one".into()),
        Err(ProviderError::Http(TransportFailure::Connect)),
    ]);
    let mut io = Io::new("this feedback must never be read\n");

    let error = runner::run_npc(npc_command(&fx, &chat), &mut io.terminal())
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            RunnerError::Npc(unmei::gen::GenerationError::Provider(_))
        ),
        "{error:?}"
    );
    assert!(
        !io.err().contains("Give free-text feedback"),
        "no later feedback prompt: {}",
        io.err()
    );
    assert!(!io.out().contains("Type 'accept'"), "{}", io.out());
    assert_eq!(
        std::fs::read(fx.root.join("npcs/ace.ron")).unwrap(),
        before,
        "a provider failure writes nothing"
    );
    assert_eq!(chat.free_prompts().len(), 2);
}

/// A terminal staged-preparation failure ends an interactive session before any
/// feedback prompt, consumes no input and writes nothing.
#[tokio::test]
async fn npc_preparation_failure_ends_the_interactive_session_without_feedback() {
    let fx = fixture();
    let ace = fx.root.join("npcs/ace.ron");
    // The pack is loaded before `run_npc`; removing a source file afterwards
    // makes the staged transplant fail, exactly as the D2/D3 witnesses do.
    std::fs::remove_file(&ace).unwrap();
    let chat = Chat::free(vec![Ok(valid_npc_reply())]);
    let mut io = Io::new("this feedback must never be read\n");

    let error = runner::run_npc(npc_command(&fx, &chat), &mut io.terminal())
        .await
        .unwrap_err();

    let display = error.to_string();
    assert!(
        display.contains("not installed or validated"),
        "the terminal error carries the context: {display}"
    );
    match error {
        RunnerError::Npc(unmei::gen::GenerationError::Preparation(message)) => {
            assert!(
                !message.is_empty(),
                "the verbatim underlying message is carried"
            );
        }
        other => panic!("expected a terminal preparation failure, got {other:?}"),
    }
    assert!(
        !io.err().contains("Give free-text feedback"),
        "no feedback prompt: {}",
        io.err()
    );
    assert!(!io.out().contains("Type 'accept'"), "{}", io.out());
    assert!(!io.out().contains("preview"), "no preview: {}", io.out());
    assert_eq!(
        io.input.position(),
        0,
        "the session's input is not consumed"
    );
    assert!(
        !ace.exists(),
        "the failure wrote nothing, so the removed file stays absent"
    );
    assert_eq!(chat.free_prompts().len(), 1, "no retry is spent");
}

/// Prose accept-after-exhaustion is refused until a validated draft exists;
/// `discard` then leaves the scene untouched.
#[tokio::test]
async fn draft_accept_after_exhaustion_is_refused_then_discard_writes_nothing() {
    let fx = fixture();
    let before = std::fs::read(fx.root.join("scenes/first_meeting.unmei")).unwrap();
    let chat = Chat::structured(vec![
        Ok(bad_draft_reply()),
        Ok(bad_draft_reply()),
        Ok(bad_draft_reply()),
    ]);
    let mut io = Io::new("\naccept\ndiscard\n");

    let outcome = runner::run_draft(draft_command(&fx, &chat, false), &mut io.terminal())
        .await
        .unwrap();

    assert_eq!(outcome, RunnerOutcome::Discarded);
    let err = io.err();
    assert!(err.contains("--- best model output ---"), "{err}");
    assert!(
        err.contains("nothing has passed validation to accept"),
        "{err}"
    );
    assert!(
        io.out().contains("Discarded — nothing written."),
        "{}",
        io.out()
    );
    assert_eq!(
        std::fs::read(fx.root.join("scenes/first_meeting.unmei")).unwrap(),
        before,
        "nothing is written"
    );
    assert_eq!(chat.structured_prompts().len(), 3);
}

/// Exhaustion → feedback → accepted preview → accept: the regeneration's first
/// prompt carries the exhausted error and the note, the second feedback round
/// proves success cleared the stale corrections, and the write changes only the
/// addressed run's bytes.
#[tokio::test]
async fn draft_exhaustion_feedback_then_accept_writes_only_the_run() {
    let fx = fixture();
    let before = std::fs::read_to_string(fx.root.join("scenes/first_meeting.unmei")).unwrap();
    let first_good = r#"{"LINES":[{"speaker":"Ace","text":"Who goes there?"}]}"#.to_string();
    let second_good =
        r#"{"LINES":[{"speaker":"Ace","text":"Who goes there? Keep it short."}]}"#.to_string();
    let chat = Chat::structured(vec![
        Ok(bad_draft_reply()),
        Ok(bad_draft_reply()),
        Ok(bad_draft_reply()),
        Ok(first_good),
        Ok(second_good),
    ]);
    // The brief prompt takes a keep-line first.
    let mut io = Io::new("\nuse Ace instead\nshorter\naccept\n");

    let outcome = runner::run_draft(draft_command(&fx, &chat, false), &mut io.terminal())
        .await
        .unwrap();

    assert_eq!(outcome, RunnerOutcome::Completed);
    let prompts = chat.structured_prompts();
    assert_eq!(prompts.len(), 5);
    assert!(
        prompts[3].contains("use Ace instead")
            && prompts[3].contains("speaker `Bob` is not allowed"),
        "the landed regeneration carries the exhausted error and the note: {}",
        prompts[3]
    );
    assert!(
        prompts[4].contains("shorter")
            && prompts[4].contains("Who goes there?")
            && !prompts[4].contains("speaker `Bob` is not allowed"),
        "success cleared the stale corrections before the next feedback turn: {}",
        prompts[4]
    );
    assert!(
        io.out()
            .contains("✔ Generated and validated: first_meeting `start` (attempt 1)"),
        "{}",
        io.out()
    );

    let after = std::fs::read_to_string(fx.root.join("scenes/first_meeting.unmei")).unwrap();
    let expected = before.replace(
        "Narrator: Morning light. A folded letter on the ground, half under a shoe.",
        "Ace: Who goes there? Keep it short.",
    );
    assert_eq!(after, expected, "only the run region changed");
}

/// Blank input at the prose preview prompt re-prompts, and EOF (or discard)
/// writes nothing with the existing acknowledgement.
#[tokio::test]
async fn draft_blank_then_eof_writes_nothing() {
    let fx = fixture();
    let before = std::fs::read(fx.root.join("scenes/first_meeting.unmei")).unwrap();
    let chat = Chat::structured(vec![Ok(valid_draft_reply())]);
    // The brief prompt consumes the leading blank; a second blank re-prompts at
    // the preview, and the empty input then reads EOF.
    let mut io = Io::new("\n\n");

    let outcome = runner::run_draft(draft_command(&fx, &chat, false), &mut io.terminal())
        .await
        .unwrap();

    assert_eq!(outcome, RunnerOutcome::Discarded);
    assert!(
        io.out().contains("Input ended — nothing written."),
        "{}",
        io.out()
    );
    assert_eq!(
        std::fs::read(fx.root.join("scenes/first_meeting.unmei")).unwrap(),
        before,
        "EOF writes nothing"
    );
}

/// `accept` under `--dry-run` prints the dry-run line and writes nothing.
#[tokio::test]
async fn draft_dry_run_accept_writes_nothing() {
    let fx = fixture();
    let before = std::fs::read(fx.root.join("scenes/first_meeting.unmei")).unwrap();
    let chat = Chat::structured(vec![Ok(valid_draft_reply())]);
    let mut io = Io::new("\naccept\n");

    let outcome = runner::run_draft(draft_command(&fx, &chat, true), &mut io.terminal())
        .await
        .unwrap();

    assert_eq!(outcome, RunnerOutcome::Completed);
    assert!(
        io.out().ends_with("--dry-run: nothing written\n"),
        "{}",
        io.out()
    );
    assert_eq!(
        std::fs::read(fx.root.join("scenes/first_meeting.unmei")).unwrap(),
        before,
        "--dry-run writes nothing"
    );
}

/// The run `start` of the demo scene, and the accepted replacement that must be
/// the only byte change in the written file.
const DEMO_RUN: &str = "Narrator: Morning light. A folded letter on the ground, half under a shoe.";
const DRAFTED_RUN: &str = "Ace: Who goes there?";

/// One accepted one-shot draft for `scene_id` over the real attempt, staging,
/// loader and atomic writer: the exact `scenes/<id>.unmei` on this host's
/// filesystem receives the drafted run, the loader round-trips that stem to the
/// id, and every other pack byte is unchanged.
async fn assert_draft_writer_lands(scene_id: &str) {
    let fx = fixture_for(scene_id);
    let before = pack_bytes(&fx.root);
    let chat = Chat::structured(vec![Ok(valid_draft_reply())]);
    let mut io = Io::new("");

    let outcome = runner::run_draft(draft_once(&fx, &chat), &mut io.terminal())
        .await
        .unwrap_or_else(|e| panic!("{scene_id:?} should be an authorable Scene: {e}"));

    assert_eq!(outcome, RunnerOutcome::Completed, "{}", io.err());
    assert_eq!(
        fx.scene_path.file_stem().and_then(|stem| stem.to_str()),
        Some(scene_id),
        "the write lands on the exact native filename"
    );
    let written = std::fs::read_to_string(&fx.scene_path).unwrap();
    assert_eq!(
        written,
        fx.scene_text.replace(DEMO_RUN, DRAFTED_RUN),
        "the addressed run is the only change in the file"
    );

    let loaded = unmei_engine::load_pack(&fx.root).expect("the loader accepts the written file");
    assert!(
        loaded.pack.scenes.contains_key(scene_id),
        "the loader round-trips the exact stem; loaded: {:?}",
        loaded.pack.scenes.keys()
    );

    let mut expected = before;
    expected.insert(
        Path::new("scenes").join(format!("{scene_id}.unmei")),
        written.into_bytes(),
    );
    assert_eq!(
        pack_bytes(&fx.root),
        expected,
        "the drafted scene is the only pack change"
    );
}

/// Native witness: an ordinary-but-unusual stem (a dot) is authored
/// through the real runner and loader on this platform's own filesystem.
#[tokio::test]
async fn draft_writer_lands_an_ordinary_unusual_scene_stem() {
    assert_draft_writer_lands("act.one").await;
}

/// Native witness: `\` and `:` are ordinary Linux filename characters,
/// so the same real runner, staging validation and writer author
/// `scenes/back\slash.unmei` and `scenes/co:lon.unmei` exactly, while the
/// matching `#[cfg(windows)]` witness below records the opposite verdict.
#[cfg(unix)]
#[tokio::test]
async fn draft_writer_keeps_backslash_and_colon_scene_stems_ordinary_on_native_linux() {
    for scene_id in [r"back\slash", "co:lon"] {
        assert_draft_writer_lands(scene_id).await;
    }
}

/// Native witness: the same two constructions are one Windows separator
/// and one alternate-stream form, so the real runner refuses them before the
/// brief, any provider call or any write.
#[cfg(windows)]
#[tokio::test]
async fn draft_writer_refuses_backslash_and_colon_scene_stems_on_native_windows() {
    for (scene_id, expected) in [
        (r"back\slash", FilenameFault::Separator('\\')),
        ("co:lon", FilenameFault::AlternateStream),
    ] {
        let fx = fixture();
        let before = pack_bytes(&fx.root);
        let chat = Chat::structured(vec![Ok(valid_draft_reply())]);
        let mut io = Io::new("");
        let command = DraftCommand {
            scene_id,
            ..draft_once(&fx, &chat)
        };

        match runner::run_draft(command, &mut io.terminal()).await {
            Err(RunnerError::Target(TargetError::UnsafeFilename {
                kind: FileKind::Scene,
                fault,
                ..
            })) => assert_eq!(fault, expected, "{scene_id:?}"),
            other => panic!("{scene_id:?} must be a terminal Scene target refusal: {other:?}"),
        }
        assert!(
            chat.structured_prompts().is_empty(),
            "no provider call was made"
        );
        assert_eq!(
            pack_bytes(&fx.root),
            before,
            "refusal changes no pack byte and leaves no temporary"
        );
    }
}
