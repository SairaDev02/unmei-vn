//! The generation core (author-cli contract §3/§5/§6) and drafting core
//! (drafting contract §4/§5/§7): one shared three-attempt policy with two
//! command-specific prompts and complete attempts. The public surface is
//! re-exported here, so callers keep naming it from `unmei::gen`.
//!
//! Retained provider diagnostics are categorical
//! ([ADR 0014](../../../docs/adr/0014-cli-diagnostics-and-terminal-credential-safety.md)).

mod draft;
mod draft_prompt;
mod npc;
mod npc_prompt;

pub use crate::retry::RETRY_LIMIT;
pub use draft::{
    draft_once, draft_with_session, validate_edited_scene, DraftError, DraftInput, DraftOutcome,
    DraftSession,
};
pub use draft_prompt::{assemble_draft_prompt, draft_structured_schema, DraftPromptData};
pub use npc::{generate_once, AttemptOutcome, GenerateInput, GenerationError, NpcIdentity};
pub use npc_prompt::{assemble_prompt, PromptData, SampleConfig};

pub(crate) use npc::{generate_with_session, GenerationSession, NpcPayload};

/// The pipeline shape, spelled out for tests (and readers).
pub const PUBLIC_API_SHAPE: &str =
    "GenerateInput -> (complete attempt x RETRY_LIMIT) -> AttemptOutcome<DecisionConfig>";

const FAILED_REPLY_MARKER: &str = "";

/// The JSON object inside a model reply: the first `{` through the last `}`,
/// tolerating prose or markdown fences around it.
fn json_object_slice(raw: &str) -> Option<&str> {
    match (raw.find('{'), raw.rfind('}')) {
        (Some(open), Some(close)) if close > open => Some(&raw[open..=close]),
        _ => None,
    }
}
