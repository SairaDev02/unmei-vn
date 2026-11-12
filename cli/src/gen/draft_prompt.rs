//! The drafting prompt and reply format (§7): the whole-scene context, the
//! `LINES` schema request, and the parse of a structured reply back to
//! say-line items.

use std::collections::HashSet;

use crate::provider::{AuthorChat, Message, StructuredOutput};
use crate::target::CheckedPack;
use unmei_engine::{read_pack_text, KEYWORDS};

use super::draft::{DraftCapability, DraftInput};
use super::{json_object_slice, FAILED_REPLY_MARKER};

/// The context a drafting request is assembled from (contract §3) — the whole
/// target scene, every NPC file, the glossary, the state vocabulary and the
/// brief. Gathered once so the prompt builder stays a pure function.
pub struct DraftPromptData {
    pub scene_id: String,
    /// The run's `--from` handle, for the model to fill exactly one run.
    pub run_handle: String,
    pub scene_text: String,
    /// Every `npcs/<id>.ron`, as (id, file contents), id-sorted.
    pub npc_files: Vec<(String, String)>,
    pub glossary: Option<String>,
    pub state: Vec<(String, String)>,
    pub brief: Option<String>,
    /// Every speaker a drafted line may use, sorted and comma-separated for a
    /// stable prompt.
    pub allowed_speakers: String,
    /// Whether the model is asked for the structured `LINES` JSON shape
    /// (`response_format`) rather than free-text say-lines.
    pub structured: bool,
}

impl DraftPromptData {
    /// Gather context from the checked pack: every NPC file is read through the
    /// checked inventory's target paths, never a raw key join. `scene_text` is
    /// passed in because the caller already reads it (it needs the original
    /// bytes to splice).
    pub fn from_checked(
        checked: &CheckedPack<'_>,
        scene_id: &str,
        run_handle: &str,
        scene_text: &str,
        allowed: &HashSet<String>,
        brief: Option<&str>,
    ) -> Self {
        let pack = checked.pack();
        let npc_files = checked
            .npcs()
            .iter()
            .filter_map(|target| {
                read_pack_text(&target.path_in(&pack.root))
                    .ok()
                    .map(|src| (target.id().to_string(), src))
            })
            .collect();

        let glossary = read_pack_text(&pack.root.join("pack.glossary.md")).ok();

        let mut state: Vec<(String, String)> = pack
            .state
            .iter()
            .map(|(key, value)| (key.clone(), value.display()))
            .collect();
        state.sort();

        DraftPromptData {
            scene_id: scene_id.to_string(),
            run_handle: run_handle.to_string(),
            scene_text: scene_text.to_string(),
            npc_files,
            glossary,
            state,
            brief: brief.map(str::to_string),
            allowed_speakers: crate::draft::speaker_list(allowed),
            structured: false,
        }
    }
}

/// The `LINES` shape demand, spelled once and reused by the prompt, the system
/// message, the correction and the parse error so they cannot drift.
pub(super) const LINES_SHAPE: &str = r#"{"LINES": [{"speaker": string, "text": string}]}"#;

/// Assemble the chat messages for one drafting turn (contract §3).
pub fn assemble_draft_prompt(data: &DraftPromptData) -> Vec<Message> {
    let mut user = String::new();
    user.push_str(&format!(
        "Scene id: {}\nRun to draft: `{}`\n\n",
        data.scene_id, data.run_handle
    ));

    user.push_str("The whole target scene — the author owns every label, branch, jump and command; fill only the named run:\n");
    user.push_str("```\n");
    user.push_str(&data.scene_text);
    if !data.scene_text.ends_with('\n') {
        user.push('\n');
    }
    user.push_str("```\n\n");

    user.push_str("The pack's cast (every npcs/*.ron):\n");
    for (id, src) in &data.npc_files {
        user.push_str(&format!("--- npcs/{id}.ron ---\n{src}\n"));
    }
    user.push('\n');

    if let Some(glossary) = &data.glossary {
        user.push_str("Pack glossary (pack-local terms):\n");
        user.push_str(glossary);
        user.push_str("\n\n");
    }

    user.push_str("State vocabulary (state.ron — only these keys may appear as `{key}`):\n");
    for (key, value) in &data.state {
        user.push_str(&format!("- {key} (current value: {value})\n"));
    }
    user.push_str("Prefer bucketed string state (\"high\"/\"low\"), never raw counters — decision models judge, they don't compute.\n\n");

    if let Some(brief) = &data.brief {
        user.push_str(&format!("Author brief for this run:\n{brief}\n\n"));
    }

    let output_instruction = if data.structured {
        format!(
            "Allowed speakers: {}. Return the run as exactly one JSON object shaped {LINES_SHAPE} — one item per say-line, no blank items, no commentary, no markdown. Each item is rendered to `Speaker: text`, so put any colon, quote or backslash inside the `text` string. Never emit control flow or commands ({}).",
            data.allowed_speakers,
            KEYWORDS.join(", ")
        )
    } else {
        format!(
            "Allowed speakers: {}. Every line must be a say-line `Speaker: text` — one per line, no blank lines, no commentary, no markdown. Never emit control flow or commands ({}).",
            data.allowed_speakers,
            KEYWORDS.join(", ")
        )
    };
    user.push_str(&output_instruction);

    let system = if data.structured {
        format!(
            "You write prose say-lines for the unmei visual novel engine as strict JSON. \
             Reply with ONLY one JSON object shaped {LINES_SHAPE}, no markdown fences. \
             The author owns the scene's labels, branches, jumps and commands; you fill \
             exactly one run with dialogue. Never emit control flow or commands, never \
             invent speakers, never add headers or commentary."
        )
    } else {
        "You write prose say-lines for the unmei visual novel engine. \
         Reply with ONLY say-lines, one per line, in the form `Speaker: text`. \
         The author owns the scene's labels, branches, jumps and commands; you \
         fill exactly one run with dialogue. Never emit control flow or commands, \
         never invent speakers, never add headers or commentary."
            .to_string()
    };

    vec![Message::system(system), Message::user(user)]
}

/// The JSON schema the structured request demands (contract §7): one object of
/// `LINES`, each a `speaker`/`text` pair. The CLI renders the items to
/// `Speaker: text`, so newlines, colons and escaping stay in JSON strings and
/// the model stays out of surface syntax.
pub fn draft_structured_schema() -> StructuredOutput {
    StructuredOutput {
        name: "unmei_scene_draft".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "LINES": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "speaker": {"type": "string"},
                            "text": {"type": "string"}
                        },
                        "required": ["speaker", "text"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["LINES"],
            "additionalProperties": false
        }),
    }
}

/// The structured reply envelope: `{"LINES": [{"speaker", "text"}]}`.
#[derive(serde::Deserialize)]
struct DraftEnvelope {
    #[serde(rename = "LINES")]
    lines: Vec<RawSayItem>,
}

#[derive(serde::Deserialize)]
struct RawSayItem {
    speaker: String,
    text: String,
}

/// Parse a structured reply into `(speaker, text)` pairs. A reply that is not
/// valid JSON in the demanded shape is a correction, not a splice.
pub(super) fn parse_structured(raw: &str) -> Result<Vec<(String, String)>, Vec<String>> {
    let json_src = json_object_slice(raw).unwrap_or_else(|| raw.trim());
    let envelope: DraftEnvelope = match serde_json::from_str(json_src) {
        Ok(envelope) => envelope,
        Err(e) => {
            return Err(vec![format!(
                "the reply was not valid JSON for the LINES schema ({e}); reply with exactly one object shaped {LINES_SHAPE}"
            )]);
        }
    };
    Ok(envelope
        .lines
        .into_iter()
        .map(|item| (item.speaker, item.text))
        .collect())
}

/// The drafting prompt for one attempt (contract §5): the assembled context,
/// then the corrections and/or the previous regeneration's author feedback, so
/// the model sees why it is being asked again.
pub(super) fn draft_prompt<C: AuthorChat>(
    input: &DraftInput<'_, C>,
    checked: &CheckedPack<'_>,
    capability: &DraftCapability,
    corrections: &[String],
    feedback: Option<&str>,
    last_draft: Option<&str>,
) -> Vec<Message> {
    let structured = capability.structured.is_some();
    let mut data = DraftPromptData::from_checked(
        checked,
        input.scene_id,
        &input.run.handle,
        input.scene_text,
        input.allowed_speakers,
        input.brief,
    );
    data.structured = structured;
    let mut messages = assemble_draft_prompt(&data);
    if !corrections.is_empty() {
        messages.push(Message::assistant(FAILED_REPLY_MARKER.to_string()));
        messages.push(Message::user(draft_corrections_message(
            corrections,
            structured,
            feedback,
        )));
    } else if let Some(feedback) = feedback {
        // The model sees its own previous draft as the assistant turn, so
        // relative feedback ("shorter, three lines") has a referent (§5).
        messages.push(Message::assistant(
            last_draft.unwrap_or_default().to_string(),
        ));
        messages.push(Message::user(draft_feedback_message(feedback, structured)));
    }
    messages
}

/// The user turn fed back after a failed attempt (§5: errors as corrections,
/// with the author's latest feedback carried alongside when there is any).
pub(super) fn draft_corrections_message(
    corrections: &[String],
    structured: bool,
    feedback: Option<&str>,
) -> String {
    let mut msg = String::from(
        "Your reply was rejected. Fix these problems and reply again with the full corrected draft",
    );
    if structured {
        msg.push_str(&format!(
            " as exactly one JSON object shaped {LINES_SHAPE}, no markdown fences:\n"
        ));
    } else {
        msg.push_str(" using only say-lines `Speaker: text`, one per line:\n");
    }
    for (i, err) in corrections.iter().enumerate() {
        msg.push_str(&format!("{}. {err}\n", i + 1));
    }
    if let Some(feedback) = feedback {
        msg.push_str(&format!("\nAuthor feedback: {feedback}\n"));
    }
    msg
}

/// The user turn fed back after an accepted preview the author chose to
/// regenerate (`-i`, contract §5): a pure feedback turn with no corrections.
pub(super) fn draft_feedback_message(feedback: &str, structured: bool) -> String {
    if structured {
        format!(
            "Author feedback on your previous draft: {feedback}\n\
             Regenerate the full draft as exactly one JSON object shaped {LINES_SHAPE}, no markdown fences.\n"
        )
    } else {
        format!(
            "Author feedback on your previous draft: {feedback}\n\
             Regenerate the full draft using only say-lines `Speaker: text`, one per line.\n"
        )
    }
}
