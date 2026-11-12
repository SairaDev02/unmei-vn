//! Prose drafting (§4/§5/§7): the session, the complete attempt and the
//! regeneration entry points the runner drives.

use std::collections::HashSet;

use unmei_engine::Pack;

use crate::provider::{
    AuthorChat, Message, ProviderError, StructuredOutput, StructuredUnsupported,
};
use crate::retry::{self, Attempt, AttemptReport, ReportOutcome, RetryState, RETRY_LIMIT};
use crate::target::{CheckedPack, SceneTarget, TargetError};

use super::draft_prompt::{draft_prompt, draft_structured_schema, parse_structured};

/// Everything one drafting generation needs (contract §3).
pub struct DraftInput<'a, C: AuthorChat> {
    pub pack: &'a Pack,
    pub scene_id: &'a str,
    /// The run region to draft and splice (already resolved by the caller).
    pub run: &'a crate::draft::Run,
    pub scene_text: &'a str,
    pub allowed_speakers: &'a HashSet<String>,
    pub brief: Option<&'a str>,
    pub client: &'a C,
    pub model: &'a str,
}

/// A successful draft (contract §4/§5/§7): the gated lines, the scene already
/// re-validated through the shared loader, and the attempt it landed on.
#[derive(Debug)]
pub struct DraftOutcome {
    pub lines: Vec<crate::draft::SayLine>,
    /// The scene text with the run spliced in, re-validated by layer 2 inside
    /// the retry loop — safe to write as-is.
    pub edited_scene: String,
    /// 1-based attempt the draft was accepted on.
    pub attempt: usize,
    /// The provider's categorical reason when the structured request was
    /// rejected for lack of support and the free-text fallback was used
    /// (contract §7); `None` when the structured request was honoured. Carried
    /// so the degradation is reported, never silent.
    pub fallback_detail: Option<StructuredUnsupported>,
}

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Target(#[from] TargetError),
    /// Every attempt's reply failed the layer-1 gate (contract §5). The caller
    /// prints the best raw output plus all errors, writes nothing and exits
    /// nonzero.
    #[error("all {RETRY_LIMIT} attempts failed validation")]
    AttemptsExhausted {
        raw_outputs: Vec<String>,
        errors: Vec<String>,
    },
}

/// The validated prose payload one complete attempt produces.
#[derive(Debug)]
pub(crate) struct DraftPayload {
    pub lines: Vec<crate::draft::SayLine>,
    /// The scene text with the run spliced in, re-validated by layer 2 inside
    /// the attempt — safe to write as-is.
    pub edited_scene: String,
}

/// The prose command's narrowly mutable session state (drafting contract §7):
/// `structured` is `Some(schema)` while the endpoint still honours
/// `response_format`; the first rejection flips it to `None` for the rest of
/// the session and records the reason in `fallback_detail`, so later
/// regenerations do not re-issue a request known to fail.
#[derive(Debug)]
pub(super) struct DraftCapability {
    pub(super) structured: Option<StructuredOutput>,
    fallback_detail: Option<StructuredUnsupported>,
}

impl DraftCapability {
    fn new() -> Self {
        DraftCapability {
            structured: Some(draft_structured_schema()),
            fallback_detail: None,
        }
    }
}

/// The state one guided `-i` session carries between regenerations (drafting
/// contract §5/§7). A one-shot invocation uses a fresh session too, so both
/// paths share exactly one generation core.
///
/// `feedback` is the author's free-text note from the last accepted preview,
/// `last_draft` is that preview's say-lines (so relative notes like "shorter,
/// three lines" have a referent), and the embedded [`RetryState`] holds the
/// validation errors from rejected regenerations; all are folded into the next
/// completed attempt.
#[derive(Debug)]
pub struct DraftSession {
    retry: RetryState,
    capability: DraftCapability,
    /// Free-text author feedback (`-i`) to apply on the next regeneration.
    feedback: Option<String>,
    /// The last accepted draft, rendered as say-lines.
    last_draft: Option<String>,
}

impl DraftSession {
    /// A fresh session: structured output is attempted first (contract §7).
    pub fn new() -> Self {
        DraftSession {
            retry: RetryState::new(),
            capability: DraftCapability::new(),
            feedback: None,
            last_draft: None,
        }
    }

    /// Set (or clear) the author's free-text feedback for the next regeneration.
    pub fn set_feedback(&mut self, feedback: Option<String>) {
        self.feedback = feedback;
    }

    /// Remember an accepted draft so the next feedback turn can refer to it.
    pub fn remember_draft(&mut self, lines: &[crate::draft::SayLine]) {
        self.last_draft = Some(
            lines
                .iter()
                .map(crate::draft::SayLine::render)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    /// The corrections currently queued for the next regeneration.
    pub fn corrections(&self) -> &[String] {
        self.retry.corrections()
    }
}

impl Default for DraftSession {
    fn default() -> Self {
        Self::new()
    }
}

/// One complete drafting attempt (contract §3/§4/§5/§7): ask for JSON-schema
/// output first, fall back to free text within the same attempt when the
/// endpoint cannot honour it, run the reply through the layer-1 gate, splice
/// the run and re-parse the whole scene through the shared loader (layer 2).
/// It never writes an author's file.
///
/// The session's structured/free-text mode is the one piece of state that
/// survives across attempts *and* interactive regenerations: it starts
/// `Some(schema)` and flips to `None` when the endpoint reports no
/// structured-output support, after which the free-text fallback is used for
/// this and every later attempt.
struct DraftAttempt<'a, C: AuthorChat> {
    input: &'a DraftInput<'a, C>,
    /// The checked Scene target (`scenes/<id>.unmei`); joined, never re-derived.
    target: SceneTarget,
    /// The checked inventory of every Pack key the staging copy consumes.
    checked: CheckedPack<'a>,

    /// The prose command's narrowly mutable capability state; every other
    /// input, including the corrections the driver hands in, is read-only.
    capability: &'a mut DraftCapability,
    feedback: Option<&'a str>,
    last_draft: Option<&'a str>,
}

impl<C: AuthorChat> DraftAttempt<'_, C> {
    /// The prompt under the session's current capability mode.
    fn messages(&self, corrections: &[String]) -> Vec<Message> {
        draft_prompt(
            self.input,
            &self.checked,
            self.capability,
            corrections,
            self.feedback,
            self.last_draft,
        )
    }
}

impl<C: AuthorChat> Attempt for DraftAttempt<'_, C> {
    type Payload = DraftPayload;

    async fn run(
        &mut self,
        corrections: &[String],
    ) -> Result<AttemptReport<DraftPayload>, ProviderError> {
        let raw = if self.capability.structured.is_some() {
            let schema = self
                .capability
                .structured
                .clone()
                .expect("structured schema is checked above");
            let messages = self.messages(corrections);
            match self
                .input
                .client
                .complete_structured(self.input.model, &messages, &schema)
                .await
            {
                Ok(raw) => raw,
                Err(ProviderError::NoStructuredSupport { reason }) => {
                    // The endpoint cannot honour `response_format`; fall back
                    // to free text within the same attempt, through the same
                    // gate. Flipping the mode makes every later attempt
                    // free-text too, even if this plain call fails terminally:
                    // the mode and reason stay recorded.
                    self.capability.structured = None;
                    self.capability.fallback_detail = Some(reason);
                    let messages = self.messages(corrections);
                    self.input
                        .client
                        .complete(self.input.model, &messages)
                        .await?
                }
                // Any other provider failure surfaces immediately: retrying
                // text will not fix an unreachable endpoint (contract §7).
                Err(e) => return Err(e),
            }
        } else {
            let messages = self.messages(corrections);
            self.input
                .client
                .complete(self.input.model, &messages)
                .await?
        };

        let outcome = if self.capability.structured.is_some() {
            parse_structured(&raw)
                .and_then(|items| crate::draft::gate_items(&items, self.input.allowed_speakers))
        } else {
            crate::draft::gate(
                &crate::draft::candidate_lines(&raw),
                self.input.allowed_speakers,
            )
        };

        Ok(match outcome {
            Ok(lines) => {
                // Layer 2 (contract §4): splice the run and re-parse the whole
                // scene through the shared loader; its file+line diagnostics
                // are corrections like layer 1's.
                let replacement: Vec<String> =
                    lines.iter().map(crate::draft::SayLine::render).collect();
                let edited =
                    crate::draft::splice_run(self.input.scene_text, self.input.run, &replacement);
                match crate::staging::validate_scene(&self.checked, &self.target, &edited) {
                    Ok(()) => AttemptReport {
                        raw,
                        outcome: ReportOutcome::Accepted(DraftPayload {
                            lines,
                            edited_scene: edited,
                        }),
                    },
                    Err(errors) => AttemptReport {
                        raw,
                        outcome: ReportOutcome::Rejected(errors),
                    },
                }
            }
            Err(errors) => AttemptReport {
                raw,
                outcome: ReportOutcome::Rejected(errors),
            },
        })
    }
}

/// One drafting regeneration (drafting contract §3/§4/§5/§7): ask for
/// JSON-schema output first, fall back automatically to free text when the
/// endpoint cannot honour it, and feed validation failures back as corrections
/// for up to [`RETRY_LIMIT`] attempts. Both layers of the gate run inside the
/// loop, so a layer-2 loader error is corrected like a layer-1 one. Provider
/// errors surface immediately.
///
/// `session` carries the structured/free-text mode and the corrections and
/// feedback across an interactive `-i` session; a landed regeneration clears
/// the corrections so they cannot leak into the next feedback turn. On
/// exhaustion the raw replies and every error are returned so the caller can
/// print the best output, write nothing and exit nonzero.
pub async fn draft_with_session<C: AuthorChat>(
    input: &DraftInput<'_, C>,
    session: &mut DraftSession,
) -> Result<DraftOutcome, DraftError> {
    // Terminal preflight (contract §10): the raw Scene target, every Pack key
    // and exact `pack.scenes` membership are checked before the driver, so a
    // refusal never reaches a prompt, provider call, staging or the session's
    // corrections/capability/remembered-draft state.
    let raw_target = SceneTarget::new(input.scene_id)?;
    let checked = CheckedPack::new(input.pack)?;
    let target = checked.require_scene(raw_target.id())?.clone();

    let outcome = {
        let DraftSession {
            retry,
            capability,
            feedback,
            last_draft,
        } = session;
        let mut attempt = DraftAttempt {
            input,
            target,
            checked,
            capability,
            feedback: feedback.as_deref(),
            last_draft: last_draft.as_deref(),
        };
        retry::drive(&mut attempt, retry).await
    };

    match outcome {
        Ok(retry::RetryOutcome { payload, attempt }) => {
            // A landed regeneration starts clean: the next `-i` feedback turn
            // must not resend these corrections, and it can refer to the draft
            // it is amending.
            session.remember_draft(&payload.lines);
            Ok(DraftOutcome {
                lines: payload.lines,
                edited_scene: payload.edited_scene,
                attempt,
                fallback_detail: session.capability.fallback_detail,
            })
        }
        Err(retry::RetryFailure::Provider(e)) => Err(DraftError::Provider(e)),
        // The drafting attempt stages through the Scene validator, whose I/O
        // failures stay corrections, so the shared driver's NPC-only terminal
        // variant is structurally unreachable here. The Scene path's
        // I/O-as-corrections classification is deliberately unchanged.
        Err(retry::RetryFailure::Preparation(_)) => {
            unreachable!("the drafting attempt does not produce preparation failures")
        }
        Err(retry::RetryFailure::Exhausted {
            raw_outputs,
            errors,
        }) => Err(DraftError::AttemptsExhausted {
            raw_outputs,
            errors,
        }),
    }
}

/// Draft one run in a fresh session — the one-shot `author draft --from` path
/// (drafting contract §3/§4/§5/§7). Behaviourally identical to a session that
/// runs a single regeneration: structured output is attempted first and the
/// fallback reason is reported in the outcome.
pub async fn draft_once<C: AuthorChat>(
    input: &DraftInput<'_, C>,
) -> Result<DraftOutcome, DraftError> {
    let mut session = DraftSession::new();
    draft_with_session(input, &mut session).await
}

/// Layer 2 (drafting contract §4): re-parse the edited scene and re-cross-
/// validate the pack through the shared loader. Diagnostics are the loader's
/// existing `LoadError`s — pack-relative file plus 1-based line, no column.
///
/// The raw public adapter keeps its signature and vector result (contract §10):
/// an unsafe Scene id or an absent Scene becomes one target/membership refusal
/// diagnostic. Shipping attempts call [`crate::staging::validate_scene`]
/// directly, so a terminal refusal never becomes a retryable correction.
pub fn validate_edited_scene(pack: &Pack, scene_id: &str, edited: &str) -> Result<(), Vec<String>> {
    let target = match SceneTarget::new(scene_id) {
        Ok(target) => target,
        Err(e) => return Err(vec![e.to_string()]),
    };
    let checked = match CheckedPack::new(pack) {
        Ok(checked) => checked,
        Err(e) => return Err(vec![e.to_string()]),
    };
    if let Err(e) = checked.require_scene(scene_id) {
        return Err(vec![e.to_string()]);
    }
    crate::staging::validate_scene(&checked, &target, edited)
}
