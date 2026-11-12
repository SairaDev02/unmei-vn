//! OpenRouter **Decisions** wire layer.
//!
//! Maps the author-facing [`DecisionConfig`] + flat state map onto the
//! `POST /api/alpha/decisions` wire contract, validates the response
//! (check `type` before reading fields; a missing key or unexpected type is an
//! error), and applies the author-fallback posture (any failure ⇒ the config's
//! `fallback` action, engine logs and continues offline).
//!
//! This module is pure and always compiled. The HTTP client lives in
//! [`client`] behind the `network` feature; the two injection points the
//! runtime resolves through, with the author-fallback posture, live in
//! `seam`.

use std::collections::HashMap;

use crate::content::{Criteria, DecisionConfig, QuestionType, StateValue};

#[cfg(feature = "network")]
pub mod client;

mod seam;

#[cfg(feature = "network")]
pub use client::{DecisionsClient, HttpKeyCheck};
pub use seam::{resolve_or_fallback, DecisionResolver, KeyCheck, KeyVerdict, ResolvedDecision};

#[cfg(test)]
mod tests;

/// Default Decisions endpoint. The URL is one config value; `UNMEI_DECISIONS_URL`
/// and [`client::DecisionsClient::new`] can override it (alpha prefix may move).
pub const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/alpha/decisions";

/// The key-validation endpoint, a module constant **beside** the decisions
/// endpoint and deliberately **independent of `UNMEI_DECISIONS_URL`**: the
/// override exists because the alpha decisions prefix can move, while
/// `/api/v1/key` is the stable `/api/v1` surface on the same origin
/// (`docs/title-and-menu.md` § Decisions settings §3).
pub const KEY_ENDPOINT: &str = "https://openrouter.ai/api/v1/key";

/// The key-validation ping's bounded timeout: a hung endpoint must resolve to
/// [`KeyVerdict::Unreachable`], never stall the commit flow.
pub const KEY_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The fixed marker a successful response's echoed model collapses to when it
/// does not exactly match the requested pin.
///
/// A response `model` is response-controlled text and could reflect a
/// credential; only the pin the caller asked for is trusted provenance.
pub const UNVERIFIED_MODEL: &str = "unverified response model";

/// The Decisions request body: `{ model, state, questions }`.
///
/// Crate-internal: a caller resolves through [`DecisionResolver`], which owns
/// the request's construction.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct DecisionRequest {
    pub model: String,
    /// Filtered to the config's `state_keys` — decision models should only see
    /// what the question needs.
    pub state: HashMap<String, StateValue>,
    /// Keyed by question id; one entry per call.
    pub questions: HashMap<String, Question>,
}

/// One question in a request. The wire keeps one `criteria` field whose shape
/// follows the question type (ADR 0005); `criteria` is omitted when a `noul`
/// config has no criteria map.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub(crate) struct Question {
    #[serde(rename = "type")]
    pub question_type: QuestionType,
    pub instructions: String,
    #[serde(skip_serializing_if = "Criteria::is_absent")]
    pub criteria: Criteria,
}

/// Build the wire request for one decision point. `state` is filtered to the
/// config's `state_keys`; keys absent from the current state are omitted.
pub(crate) fn build_decision_request(
    model: &str,
    question_id: &str,
    config: &DecisionConfig,
    state: &HashMap<String, StateValue>,
) -> DecisionRequest {
    let filtered: HashMap<String, StateValue> = config
        .state_keys
        .iter()
        .filter_map(|k| state.get(k).map(|v| (k.clone(), v.clone())))
        .collect();
    let question = Question {
        question_type: config.question_type,
        instructions: config.instructions.clone(),
        criteria: config.criteria.clone(),
    };
    DecisionRequest {
        model: model.to_string(),
        state: filtered,
        questions: HashMap::from([(question_id.to_string(), question)]),
    }
}

/// A parsed Decisions response body. Unknown top-level fields (e.g. `id`,
/// `provider`) are ignored.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub(crate) struct DecisionResponse {
    pub model: String,
    #[serde(default)]
    pub answers: HashMap<String, Answer>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// One typed answer. Tagged by `type`, so serde rejects an unexpected type
/// before any field is read.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Answer {
    Choice {
        choice: String,
        #[serde(default)]
        probabilities: HashMap<String, f64>,
        #[serde(default)]
        confidence: Option<f64>,
    },
    Noul {
        noul: f64,
    },
    Score {
        score: f64,
        #[serde(default)]
        legend: HashMap<String, String>,
        #[serde(default)]
        probabilities: HashMap<String, f64>,
        #[serde(default)]
        confidence: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub(crate) struct Usage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cost: Option<f64>,
}

/// A validated, usable answer: the landing key the answer produces plus the
/// response provenance to log. The key is an action id (`choice`), `true`/
/// `false` (`noul`), or a legend index (`score`) — the suffix after `<prefix>.`,
/// not the [`Fallback::Action`](crate::content::Fallback::Action) sense of
/// "action".
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub action: String,
    /// The requested pin when the response's echoed `model` matched it
    /// exactly, otherwise [`UNVERIFIED_MODEL`].
    ///
    /// The pure wire layer echoes `response.model`; the network client boundary
    /// ([`client::DecisionsClient`]) is what restricts it, because a response
    /// model is uncontrolled text that could reflect a credential.
    pub model: String,
    pub confidence: Option<f64>,
    pub cost_usd: Option<f64>,
    /// `score` only: the model's scalar score, carried for logging. ADR 0005
    /// lands on `probabilities`, never on this uncalibrated magnitude.
    pub score: Option<f64>,
    /// `score` only: the response's echoed legend (`"0"` -> level text).
    ///
    /// Retained by the pure wire layer for decoding/tests only. The network
    /// client clears it ([`client::DecisionsClient`]): the echoed legend is
    /// response-controlled provenance, and the authored legend is the only safe
    /// diagnostic source. Never log this field.
    pub legend: Option<HashMap<String, String>>,
}

/// Failures that all funnel into the author fallback at the call site.
///
/// Every variant's `Display` is a **categorical** diagnostic: no raw response
/// body, endpoint URL, decoder text, transport text or model-chosen string is
/// retained, because any of those can reflect a credential into the fallback
/// text, the console, `<pack-dir>.log` or the Monitor. Only authored content
/// (question ids, expected type) and fixed material (status code) is carried.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DecisionError {
    #[error("invalid Decisions response JSON")]
    Decode,
    #[error("response has no answer for question `{0}`")]
    MissingAnswer(String),
    #[error("expected a `{expected}` answer for `{question}`, got `{answer_type}`")]
    WrongType {
        question: String,
        expected: String,
        answer_type: String,
    },
    #[error("model chose an action outside the criteria list")]
    UnknownAction,
    #[error("answer for `{question}` has no usable legend index in `probabilities`")]
    UnusableAnswer { question: String },
    #[error("configuration error: {0}")]
    Config(String),
    #[error("HTTP request failed")]
    Http,
    #[error("decision task did not complete")]
    Task,
    #[error("Decisions API returned HTTP {status}")]
    Status { status: u16 },
}

/// Restrict a successful response's echoed model to the requested pin.
///
/// An exact match proves the response named the pin the caller asked for;
/// anything else (a different build, or a dummy credential reflected as the
/// model) collapses to [`UNVERIFIED_MODEL`]. The network client applies this
/// before logging or returning, so `Decision.model` and
/// `ResolvedDecision.model` never carry response-controlled text.
pub(crate) fn verified_model(requested: &str, response_model: &str) -> String {
    if requested == response_model {
        requested.to_string()
    } else {
        UNVERIFIED_MODEL.to_string()
    }
}

/// Parse a raw response body (as `reqwest` would return it).
pub(crate) fn parse_decision_response(body: &str) -> Result<DecisionResponse, DecisionError> {
    // The serde error's text can quote the offending input (e.g. an unknown
    // `type` variant), so it is deliberately dropped rather than retained.
    serde_json::from_str(body).map_err(|_| DecisionError::Decode)
}

/// Validate a parsed response and extract the landing key for one question.
///
/// The landing key is the type's answer mapped to its label suffix (ADR 0005):
/// a `choice` action id (`none` included), `true`/`false` for a `noul`
/// probability at or above `0.5`, or a `score` legend index. A typed config
/// receiving the other type's answer is an unexpected type that takes the
/// author fallback; so is a model `"none"` answer that is not in the criteria
/// list.
pub(crate) fn decision_from_response(
    response: &DecisionResponse,
    question_id: &str,
    config: &DecisionConfig,
) -> Result<Decision, DecisionError> {
    let answer = response
        .answers
        .get(question_id)
        .ok_or_else(|| DecisionError::MissingAnswer(question_id.to_string()))?;
    match answer {
        Answer::Choice {
            choice, confidence, ..
        } => {
            let Some(criteria) = config.choice_criteria() else {
                return Err(DecisionError::WrongType {
                    question: question_id.to_string(),
                    expected: config.question_type.as_str().to_string(),
                    answer_type: "choice".to_string(),
                });
            };
            if !criteria.contains_key(choice) {
                // The action is model-chosen text; drop it rather than echo it.
                return Err(DecisionError::UnknownAction);
            }
            Ok(Decision {
                action: choice.clone(),
                model: response.model.clone(),
                confidence: *confidence,
                cost_usd: response.usage.as_ref().and_then(|u| u.cost),
                score: None,
                legend: None,
            })
        }
        Answer::Noul { noul } => {
            if config.question_type != QuestionType::Noul {
                return Err(DecisionError::WrongType {
                    question: question_id.to_string(),
                    expected: config.question_type.as_str().to_string(),
                    answer_type: "noul".to_string(),
                });
            }
            // ADR 0005: `>= 0.5` is `true`; `false` is a real answer, not a
            // failure, so neither branch reaches the author fallback.
            let action = if *noul >= 0.5 { "true" } else { "false" };
            Ok(Decision {
                action: action.to_string(),
                model: response.model.clone(),
                // The wire's `noul` shape carries no confidence field.
                confidence: None,
                cost_usd: response.usage.as_ref().and_then(|u| u.cost),
                score: None,
                legend: None,
            })
        }
        Answer::Score {
            score,
            legend,
            probabilities,
            confidence,
        } => {
            let Some(levels) = config.score_criteria() else {
                return Err(DecisionError::WrongType {
                    question: question_id.to_string(),
                    expected: config.question_type.as_str().to_string(),
                    answer_type: "score".to_string(),
                });
            };
            // ADR 0005: land on the legend index with the highest probability.
            // Iterating legend indices (not the map) makes ties deterministic
            // and can only ever land on a real `<prefix>.<index>` key; keys
            // outside the legend cannot land the script. An answer with no
            // usable probability is unusable and takes the fallback; the scalar
            // `score` is provenance, never a landing target.
            let mut best: Option<(usize, f64)> = None;
            for index in 0..levels.len() {
                let Some(&probability) = probabilities.get(&index.to_string()) else {
                    continue;
                };
                match best {
                    Some((_, best_probability)) if probability <= best_probability => {}
                    _ => best = Some((index, probability)),
                }
            }
            let Some((index, _)) = best else {
                return Err(DecisionError::UnusableAnswer {
                    question: question_id.to_string(),
                });
            };
            Ok(Decision {
                action: index.to_string(),
                model: response.model.clone(),
                confidence: *confidence,
                cost_usd: response.usage.as_ref().and_then(|u| u.cost),
                score: Some(*score),
                legend: (!legend.is_empty()).then(|| legend.clone()),
            })
        }
    }
}

/// Parse + validate in one step (what the client does with an HTTP body).
pub fn decision_from_body(
    body: &str,
    question_id: &str,
    config: &DecisionConfig,
) -> Result<Decision, DecisionError> {
    let response = parse_decision_response(body)?;
    decision_from_response(&response, question_id, config)
}

/// The pure status mapping: `200` → [`KeyVerdict::Accepted`]; `401`/`403` →
/// [`KeyVerdict::Rejected`]; anything else — including `0`, the transport
/// error sentinel — → [`KeyVerdict::Unreachable`].
pub(crate) fn verdict_from_status(status: u16) -> KeyVerdict {
    match status {
        200 => KeyVerdict::Accepted,
        401 | 403 => KeyVerdict::Rejected,
        _ => KeyVerdict::Unreachable,
    }
}
