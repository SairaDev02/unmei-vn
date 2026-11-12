//! NPC generation (§3/§5/§6): the extraction/schema gates, the private
//! session and the complete attempt the shared retry driver runs.

use unmei_engine::content::DecisionConfig;
use unmei_engine::{check_id, Pack, QuestionType};

use crate::provider::{AuthorChat, Message, ProviderError};
use crate::retry::{self, Attempt, AttemptReport, ReportOutcome, RetryState, RETRY_LIMIT};
use crate::target::{CheckedPack, NpcTarget, TargetError};

use super::npc_prompt::{assemble_prompt, type_spec, GenEnvelope, PromptData, SampleConfig};
use super::{json_object_slice, FAILED_REPLY_MARKER};

/// Everything one generation needs. Corrections and author feedback live in
/// the session ([`GenerationSession`]), not here, so there is exactly one
/// owner for each.
pub struct GenerateInput<'a, C: AuthorChat> {
    pub pack: &'a Pack,
    pub npc_id: &'a str,
    pub question: &'a str,
    /// The question type to generate (`--type`); `choice` when unspecified.
    pub question_type: QuestionType,
    pub brief: Option<&'a str>,
    pub client: &'a C,
    pub model: &'a str,
}

/// The NPC identity the model returns on `--create` (contract §2).
#[derive(Debug, Clone, PartialEq)]
pub struct NpcIdentity {
    pub id: String,
    pub name: String,
}

/// One generation run's result.
#[derive(Debug)]
pub enum AttemptOutcome {
    Accepted {
        config: DecisionConfig,
        /// Present when the model returned an `NPC` object (create mode).
        identity: Option<NpcIdentity>,
    },
    Rejected(Vec<String>),
}

#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("all {RETRY_LIMIT} attempts failed validation")]
    AttemptsExhausted {
        raw_outputs: Vec<String>,
        errors: Vec<String>,
    },
    #[error("the candidate was not installed or validated: {0}")]
    Preparation(String),
}

/// The NPC payload one complete attempt produces from the command's current
/// extraction and pack-validation gates.
///
/// An accepted payload now implies the candidate was installed in a real
/// staged pack and validated by the real engine loader; a staged
/// preparation failure is terminal and never reaches this type.
#[derive(Debug)]
pub(crate) struct NpcPayload {
    pub config: DecisionConfig,
    /// Present when the model returned an `NPC` object (create mode).
    pub identity: Option<NpcIdentity>,
}

/// The NPC command's private session: the one corrections owner plus the
/// author's latest free-text feedback. A one-shot invocation uses a fresh
/// session too, so both paths share exactly one generation core.
#[derive(Debug, Default)]
pub(crate) struct GenerationSession {
    retry: RetryState,
    /// Free-text author feedback (interactive `-i`): rides along with the next
    /// generation turn.
    feedback: Option<String>,
}

impl GenerationSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set (or clear) the author's free-text feedback for the next regeneration.
    pub fn set_feedback(&mut self, feedback: Option<String>) {
        self.feedback = feedback;
    }
}

/// One complete NPC attempt (§3/§5): assemble the prompt, ask the provider for
/// the GEN JSON, extract + convert it to a `DecisionConfig`, and run the
/// command's current pack-validation gates (see [`NpcPayload`]). A staged
/// preparation failure ends the attempt terminally. It never writes an author's
/// file.
struct NpcAttempt<'a, C: AuthorChat> {
    input: &'a GenerateInput<'a, C>,
    /// The checked NPC target (`npcs/<id>.ron`); joined, never re-derived.
    target: NpcTarget,
    /// The checked inventory of every Pack key the staging copy consumes.
    checked: CheckedPack<'a>,
    /// The author's feedback for this regeneration, borrowed read-only.
    feedback: Option<&'a str>,
}

impl<C: AuthorChat> Attempt for NpcAttempt<'_, C> {
    type Payload = NpcPayload;

    async fn run(
        &mut self,
        corrections: &[String],
    ) -> Result<AttemptReport<NpcPayload>, ProviderError> {
        let prompt = prompt_messages(self.input, corrections, self.feedback);
        let raw = self
            .input
            .client
            .complete(self.input.model, &prompt)
            .await?;
        Ok(
            match extract_config(self.input, &self.target, &self.checked, &raw) {
                Ok((config, identity)) => AttemptReport {
                    raw,
                    outcome: ReportOutcome::Accepted(NpcPayload { config, identity }),
                },
                Err(ExtractFailure::Content(errors)) => AttemptReport {
                    raw,
                    outcome: ReportOutcome::Rejected(errors),
                },
                Err(ExtractFailure::Preparation(message)) => AttemptReport {
                    raw,
                    outcome: ReportOutcome::PreparationFailed(message),
                },
            },
        )
    }
}

/// Run one NPC regeneration over the shared three-attempt driver. Provider
/// errors surface immediately (an unreachable endpoint is not a content
/// problem retrying can fix); exhaustion returns every raw reply and error.
pub(crate) async fn generate_with_session<C: AuthorChat>(
    input: &GenerateInput<'_, C>,
    session: &mut GenerationSession,
) -> Result<retry::RetryOutcome<NpcPayload>, GenerationError> {
    // Terminal preflight (contract §10): the raw target and every Pack key are
    // checked before the driver, so no prompt, provider call or staging can
    // happen for a refusal. Direct supplied-Pack callers never reload.
    let target = NpcTarget::new(input.npc_id)?;
    let checked = CheckedPack::new(input.pack)?;
    let mut attempt = NpcAttempt {
        input,
        target,
        checked,
        feedback: session.feedback.as_deref(),
    };
    match retry::drive(&mut attempt, &mut session.retry).await {
        Ok(outcome) => Ok(outcome),
        Err(retry::RetryFailure::Provider(e)) => Err(GenerationError::Provider(e)),
        Err(retry::RetryFailure::Preparation(message)) => {
            Err(GenerationError::Preparation(message))
        }
        Err(retry::RetryFailure::Exhausted {
            raw_outputs,
            errors,
        }) => Err(GenerationError::AttemptsExhausted {
            raw_outputs,
            errors,
        }),
    }
}

/// One-shot generation in a fresh session, for callers that want the legacy
/// wrapper shape. The `Rejected` variant is never constructed: content
/// rejection lives inside the completed attempt and the driver.
pub async fn generate_once<C: AuthorChat>(
    input: &GenerateInput<'_, C>,
) -> Result<AttemptOutcome, GenerationError> {
    let mut session = GenerationSession::new();
    let outcome = generate_with_session(input, &mut session).await?;
    Ok(AttemptOutcome::Accepted {
        config: outcome.payload.config,
        identity: outcome.payload.identity,
    })
}

/// The prompt for one attempt: base context, then corrections and/or the
/// author's feedback from the session.
fn prompt_messages<C: AuthorChat>(
    input: &GenerateInput<'_, C>,
    corrections: &[String],
    feedback: Option<&str>,
) -> Vec<Message> {
    let mut data = PromptData::from_pack(input.pack, input.npc_id, input.question);
    data.question_type = input.question_type;
    data.brief = input.brief.map(str::to_string);
    data.creating = !input.pack.npcs.contains_key(input.npc_id);
    if let Some(npc) = input.pack.npcs.get(input.npc_id) {
        if let Some(existing) = npc.decisions.get(input.question) {
            let rendered = ron::ser::to_string(existing).unwrap_or_default();
            data.existing = Some(SampleConfig(rendered));
        }
    }

    let mut messages = assemble_prompt(&data);
    if !corrections.is_empty() {
        // Continuity: the model sees its own failed reply, then the correction.
        messages.push(Message::assistant(FAILED_REPLY_MARKER.to_string()));
        messages.push(Message::user(corrections_message(corrections, feedback)));
    } else if let Some(instruction) = feedback {
        messages.push(Message::assistant(FAILED_REPLY_MARKER.to_string()));
        messages.push(Message::user(format!(
            "Author feedback on your previous reply: {instruction}\n\
             Regenerate the full JSON object with the feedback applied.\
             Reply with exactly one JSON object shaped {{\"GEN\": {{...}}}} (plus \"NPC\": {{\"id\", \"name\"}} only on create), no markdown fences.\n"
        )));
    }
    messages
}

/// The user turn fed back after failed attempts (§5: errors as corrections).
fn corrections_message(corrections: &[String], instruction: Option<&str>) -> String {
    let mut msg = String::from("Your reply was rejected. Fix these problems and reply again with the full corrected JSON object:\n");
    for (i, err) in corrections.iter().enumerate() {
        msg.push_str(&format!("{}. {err}\n", i + 1));
    }
    if let Some(instruction) = instruction {
        msg.push_str(&format!("\nAuthor feedback: {instruction}\n"));
    }
    msg.push_str("\nReminder: reply with exactly one JSON object shaped {\"GEN\": {...}} (plus \"NPC\": {\"id\", \"name\"} only on create), no markdown fences.\n");
    msg
}

/// Why one NPC extraction did not yield a validated candidate: a model
/// or content-rule failure is correction material, while a staged-preparation
/// failure is terminal and never enters the corrections or the substring
/// filter.
enum ExtractFailure {
    Content(Vec<String>),
    Preparation(String),
}

/// Extract the `DecisionConfig` from one raw model output, cleaning the schema
/// (strip `id`/`name`, trim ids) and validating against the pack. A content
/// failure is a correction line; a staged preparation failure is terminal.
fn extract_config<C: AuthorChat>(
    input: &GenerateInput<'_, C>,
    target: &NpcTarget,
    checked: &CheckedPack<'_>,
    raw: &str,
) -> Result<(DecisionConfig, Option<NpcIdentity>), ExtractFailure> {
    let mut errors = Vec::new();

    // JSON may ride inside a markdown fence or prose; find the first '{' .. last '}'.
    let json_src = match json_object_slice(raw) {
        Some(src) => src,
        None => {
            return Err(ExtractFailure::Content(vec![format!(
                "the reply was not parseable as JSON: {raw}"
            )]))
        }
    };

    let envelope: GenEnvelope = match serde_json::from_str(json_src) {
        Ok(env) => env,
        Err(e) => {
            return Err(ExtractFailure::Content(vec![format!("the reply was not parseable as JSON ({e}); reply with exactly one JSON object shaped {{\"GEN\": {{...}}}}")]));
        }
    };

    let Some(gen) = envelope.gen else {
        return Err(ExtractFailure::Content(vec![format!(
            "the JSON object has no \"GEN\" key; reply shaped {{\"GEN\": {}}}",
            type_spec(input.question_type).shape
        )]));
    };

    // The CLI's `--type` is authoritative: force the discriminator so the
    // typed criteria/fallback shapes below are parsed as the selected type,
    // whatever the model put (or omitted) in its `type` field.
    let gen = match gen {
        serde_json::Value::Object(mut map) => {
            map.insert(
                "type".to_string(),
                serde_json::Value::String(input.question_type.as_str().to_string()),
            );
            serde_json::Value::Object(map)
        }
        other => other,
    };

    let config: DecisionConfig = match serde_json::from_value(gen) {
        Ok(config) => config,
        Err(e) => {
            return Err(ExtractFailure::Content(vec![format!(
                "the GEN object did not match the schema ({e}); it needs exactly: {}",
                type_spec(input.question_type).schema_hint
            )]));
        }
    };

    // The `npc` object is only meaningful (and only expected) on create.
    let mut identity: Option<NpcIdentity> = None;
    if let Some(npc) = &envelope.npc {
        if npc.id != input.npc_id {
            errors.push(format!(
                "the NPC object's id is {:?} but this generation targets {:?}",
                npc.id, input.npc_id
            ));
        } else {
            identity = Some(NpcIdentity {
                id: npc.id.clone(),
                name: npc.name.clone(),
            });
        }
    }

    // Criterion ids and the fallback become landing keys (`<prefix>.<key>`),
    // so they must be snake_case ids (`[a-z0-9_]+`) — the grammar the loader
    // applies to the pack's other ids. Checked here so a malformed id is named
    // as a fixable correction. The loader pass below still runs and may also
    // report the closure error the id causes; deliberate — one round carries
    // the candidate's other, independent problems too.
    if let Some(criteria) = config.choice_criteria() {
        for id in criteria.keys() {
            if let Err(message) = check_id(id, "criterion id") {
                errors.push(message);
            }
        }
    }
    if let Some(action) = config.choice_fallback() {
        if let Err(message) = check_id(action, "fallback id") {
            errors.push(message);
        }
    }

    // The real gate (§3): the same rules the runtime loader enforces. The
    // checked staging owner transplants the pack, installs this candidate and
    // reloads through the real loader — one validation source.
    match crate::staging::validate_npc_decision(checked, target, input.question, &config) {
        Ok(()) => {}
        Err(crate::staging::NpcValidationFailure::Content(pack_errors)) => {
            // Loader content errors are filtered to what the model can fix
            // (candidate file, question/label space); unrelated pre-existing
            // pack errors are dropped so the model is not told to fix content
            // it did not author. A preparation failure never reaches this
            // filter.
            errors.extend(pack_errors.into_iter().filter(|e| {
                e.contains(&format!("npcs/{}.ron", input.npc_id))
                    || e.contains(input.question)
                    || e.contains(input.npc_id)
            }));
        }
        Err(crate::staging::NpcValidationFailure::Preparation(message)) => {
            return Err(ExtractFailure::Preparation(message));
        }
    }

    if errors.is_empty() {
        Ok((config, identity))
    } else {
        Err(ExtractFailure::Content(errors))
    }
}

// ---------------------------------------------------------------------------
// Drafting one run of prose (drafting contract §3/§4)
// ---------------------------------------------------------------------------
