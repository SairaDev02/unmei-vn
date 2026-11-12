//! The NPC generation prompt and the per-question-type `GEN` shape (§6): what
//! the model is asked for, and the `TypeSpec` rows the prompt and the
//! extraction share.

use unmei_engine::{read_pack_text, Pack, QuestionType};

use crate::provider::Message;

/// A stand-in for an existing config's rendered form (prompt reference).
pub struct SampleConfig(pub String);

/// What the prompt is assembled from (§6).
pub struct PromptData {
    pub npc_id: String,
    pub question: String,
    /// The selected question type; drives the demanded criteria/fallback shape.
    pub question_type: QuestionType,
    pub brief: Option<String>,
    pub existing: Option<SampleConfig>,
    pub creating: bool,
    pub state: Vec<(String, String)>,
    pub glossary: Option<String>,
}

impl PromptData {
    pub fn from_pack(pack: &Pack, npc_id: &str, question: &str) -> Self {
        let mut state: Vec<(String, String)> = pack
            .state
            .iter()
            .map(|(k, v)| (k.clone(), v.display()))
            .collect();
        state.sort();
        let glossary = read_pack_text(&pack.root.join("pack.glossary.md")).ok();
        PromptData {
            npc_id: npc_id.to_string(),
            question: question.to_string(),
            question_type: QuestionType::Choice,
            brief: None,
            existing: None,
            creating: false,
            state,
            glossary,
        }
    }
}

/// Assemble the chat messages for one generation (§6: context is automatic).
pub fn assemble_prompt(data: &PromptData) -> Vec<Message> {
    let spec = type_spec(data.question_type);
    let mut state_desc = String::new();
    for (key, value) in &data.state {
        state_desc.push_str(&format!("- {key} (current value: {value})\n"));
    }

    let mut user = String::new();
    user.push_str(&format!(
        "NPC id: {}\nQuestion id: {}\n\n",
        data.npc_id, data.question
    ));

    if let Some(brief) = &data.brief {
        user.push_str(&format!("Author brief:\n{brief}\n\n"));
    }

    if let Some(glossary) = &data.glossary {
        user.push_str(&format!(
            "Pack glossary (pack-local terms):\n{glossary}\n\n"
        ));
    }

    user.push_str("State vocabulary (state.ron — every state_keys entry must be declared here):\n");
    user.push_str(&state_desc);
    user.push('\n');

    if data.creating {
        user.push_str(
            "This is a NEW NPC: the GEN object must also carry an \"NPC\" object with \"id\" and \"name\".\n\n",
        );
    }

    if let Some(existing) = &data.existing {
        user.push_str(&format!(
            "This REPLACES an existing config. Existing config for reference:\n{}\n\n",
            existing.0
        ));
    }

    user.push_str("Instructions and criteria descriptions are prose for the decision model to judge against. Prefer bucketed string state (\"high\"/\"low\"), never raw counters — decision models judge, they don't compute.\n");
    if !spec.guidance.is_empty() {
        user.push('\n');
        user.push_str(spec.guidance);
        user.push('\n');
    }

    vec![
        Message::system(format!(
            "You write one decision config for the unmei visual novel engine as strict JSON. \
             Reply with exactly one JSON object, no markdown fences, shaped: \
             {{\"GEN\": {shape}}}. \
             The GEN object carries no \"id\" or \"name\".{create}{note}\n\
             State vocabulary guidance: keys are the only state the config may reference.",
            shape = spec.shape,
            create = if data.creating {
                " On create, the top-level object instead carries \"NPC\": {\"id\": string, \"name\": string} alongside GEN."
            } else {
                ""
            },
            note = if spec.note.is_empty() {
                String::new()
            } else {
                format!(" {}", spec.note)
            },
        )),
        Message::user(user),
    ]
}

/// Everything the prompt and schema errors need to know about one question
/// type, in one place: adding a type edits one row instead of four matchers.
pub(super) struct TypeSpec {
    /// The exact `GEN` JSON shape demanded of the model.
    pub(super) shape: &'static str,
    /// One-line type reminder appended to the system prompt. Empty for
    /// `choice`, whose prompt predates typed generation and must not change.
    pub(super) note: &'static str,
    /// Extra user-turn guidance (empty for `choice`).
    pub(super) guidance: &'static str,
    /// The `needs exactly` correction text for a schema mismatch.
    pub(super) schema_hint: &'static str,
}

pub(super) fn type_spec(question_type: QuestionType) -> &'static TypeSpec {
    match question_type {
        QuestionType::Choice => &CHOICE_SPEC,
        QuestionType::Noul => &NOUL_SPEC,
        QuestionType::Score => &SCORE_SPEC,
    }
}

const CHOICE_SPEC: TypeSpec = TypeSpec {
    shape: r#"{"instructions": string, "state_keys": [string], "criteria": {action_id: description}, "fallback": action_id}"#,
    note: "",
    guidance: "",
    schema_hint: "\"instructions\" (string), \"state_keys\" (array of strings), \"criteria\" (object of action_id -> description), \"fallback\" (a criterion id)",
};

const NOUL_SPEC: TypeSpec = TypeSpec {
    shape: r#"{"instructions": string, "state_keys": [string], "criteria": {"true": description, "false": description}, "fallback": true|false}"#,
    note: "Type: noul (yes/no). `criteria` maps exactly \"true\" and \"false\" to descriptions; `fallback` is the boolean taken on failure.",
    guidance: "This is a `noul` (yes/no) question: `criteria` must be an object with exactly the keys \"true\" and \"false\", and `fallback` must be `true` or `false`.",
    schema_hint: "\"instructions\" (string), \"state_keys\" (array of strings), \"criteria\" (object with exactly \"true\" and \"false\"), \"fallback\" (a boolean)",
};

const SCORE_SPEC: TypeSpec = TypeSpec {
    shape: r#"{"instructions": string, "state_keys": [string], "criteria": [level description, ... lowest first, at least two levels], "fallback": legend_index}"#,
    note: "Type: score. `criteria` is an ordered array of level descriptions, lowest first, at least two; `fallback` is a 0-based legend index.",
    guidance: "This is a `score` question: `criteria` must be an ordered array of at least two level descriptions, lowest first, and `fallback` must be a 0-based index into that array.",
    schema_hint: "\"instructions\" (string), \"state_keys\" (array of strings), \"criteria\" (array of at least two level descriptions, lowest first), \"fallback\" (a 0-based legend index)",
};

/// The envelope the model must answer in.
#[derive(serde::Deserialize)]
pub(super) struct GenEnvelope {
    #[serde(rename = "GEN", default)]
    pub(super) gen: Option<serde_json::Value>,
    #[serde(rename = "NPC", default)]
    pub(super) npc: Option<RawNpc>,
}

#[derive(serde::Deserialize)]
pub(super) struct RawNpc {
    pub(super) id: String,
    pub(super) name: String,
}
