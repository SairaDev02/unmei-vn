//! The pure wire contract: request shape, one landing rule per question type,
//! and the author fallback every failure resolves to.

use super::*;
use crate::content::{Criteria, DecisionConfig, Fallback, QuestionType};

/// The live smoke-test response from research §6 — the independent source of
/// truth for the wire contract.
const LIVE_RESPONSE: &str = r#"{
        "model": "typesafe/jev-1.13-20260917",
        "answers": {
            "greet": {
                "type": "choice",
                "choice": "warm",
                "probabilities": { "warm": 0.86, "cold": 0.01, "none": 0.13 },
                "confidence": 0.8
            }
        },
        "usage": { "input_tokens": 369, "output_tokens": 39, "cost": 0.000015498 },
        "id": "gen-dec-1790348655-vyHwX6XsYjshaIpbV0Yn",
        "provider": "TypeSafe"
    }"#;

fn config() -> DecisionConfig {
    DecisionConfig {
        question_type: QuestionType::Choice,
        instructions: "How does Ace greet the player?".to_string(),
        state_keys: vec!["affinity_level".to_string(), "met_ace".to_string()],
        criteria: Criteria::Choice(HashMap::from([
            ("warm".to_string(), "affectionate".to_string()),
            ("cold".to_string(), "guarded".to_string()),
            ("none".to_string(), "neither fits".to_string()),
        ])),
        fallback: Fallback::Action("cold".to_string()),
    }
}

fn state() -> HashMap<String, StateValue> {
    HashMap::from([
        (
            "affinity_level".to_string(),
            StateValue::Str("high".to_string()),
        ),
        ("met_ace".to_string(), StateValue::Bool(true)),
        // Not in state_keys: must be filtered out.
        ("letters_found".to_string(), StateValue::Num(1.0)),
    ])
}

#[test]
fn request_filters_state_to_declared_keys() {
    let request = build_decision_request("typesafe/jev-1.13", "greet", &config(), &state());
    assert_eq!(request.model, "typesafe/jev-1.13");
    let mut keys: Vec<&String> = request.state.keys().collect();
    keys.sort();
    assert_eq!(keys, vec!["affinity_level", "met_ace"]);
}

#[test]
fn request_serializes_to_wire_shape() {
    let request = build_decision_request("typesafe/jev-1.13", "greet", &config(), &state());
    let value = serde_json::to_value(&request).expect("serializes");
    assert_eq!(value["model"], "typesafe/jev-1.13");
    assert_eq!(value["questions"]["greet"]["type"], "choice");
    assert_eq!(
        value["questions"]["greet"]["instructions"],
        "How does Ace greet the player?"
    );
    assert_eq!(value["state"]["affinity_level"], "high");
    assert_eq!(value["state"]["met_ace"], true);
    assert!(value["state"].get("letters_found").is_none());
}

#[test]
fn parses_live_response_shape() {
    let decision = decision_from_body(LIVE_RESPONSE, "greet", &config()).unwrap();
    assert_eq!(decision.action, "warm");
    assert_eq!(decision.model, "typesafe/jev-1.13-20260917");
    assert_eq!(decision.confidence, Some(0.8));
    assert_eq!(decision.cost_usd, Some(0.000015498));
}

#[test]
fn none_action_is_a_success() {
    let body = r#"{
            "model": "m",
            "answers": { "greet": { "type": "choice", "choice": "none" } }
        }"#;
    let decision = decision_from_body(body, "greet", &config()).unwrap();
    assert_eq!(decision.action, "none");
    assert_eq!(decision.confidence, None);
}

#[test]
fn missing_answer_is_error() {
    let body = r#"{ "model": "m", "answers": {} }"#;
    assert_eq!(
        decision_from_body(body, "greet", &config()).unwrap_err(),
        DecisionError::MissingAnswer("greet".to_string())
    );
}

#[test]
fn wrong_answer_type_is_error() {
    let body = r#"{
            "model": "m",
            "answers": { "greet": { "type": "noul", "noul": 0.96 } }
        }"#;
    assert!(matches!(
        decision_from_body(body, "greet", &config()).unwrap_err(),
        DecisionError::WrongType { .. }
    ));
}

#[test]
fn unexpected_answer_type_is_decode_error() {
    let body = r#"{
            "model": "m",
            "answers": { "greet": { "type": "banana", "choice": "warm" } }
        }"#;
    assert!(matches!(
        decision_from_body(body, "greet", &config()).unwrap_err(),
        DecisionError::Decode
    ));
}

#[test]
fn action_outside_criteria_is_error() {
    let body = r#"{
            "model": "m",
            "answers": { "greet": { "type": "choice", "choice": "purple" } }
        }"#;
    assert_eq!(
        decision_from_body(body, "greet", &config()).unwrap_err(),
        DecisionError::UnknownAction
    );
}

#[test]
fn model_answer_passes_through_fallback() {
    let decision = Decision {
        action: "warm".to_string(),
        model: "m".to_string(),
        confidence: Some(0.8),
        cost_usd: None,
        score: None,
        legend: None,
    };
    let resolved = resolve_or_fallback(&config(), Ok(decision));
    assert_eq!(resolved.action, "warm");
    assert_eq!(resolved.model.as_deref(), Some("m"));
    assert!(!resolved.is_fallback());
}

#[test]
fn any_error_falls_back_to_author_action() {
    let resolved = resolve_or_fallback(&config(), Err(DecisionError::Http));
    assert_eq!(resolved.action, "cold");
    assert!(resolved.is_fallback());
    assert_eq!(resolved.model, None);
    assert_eq!(resolved.fallback_reason, Some(DecisionError::Http));
}

// --- the type discriminator on the wire ----------------------------------

fn noul_config() -> DecisionConfig {
    DecisionConfig {
        question_type: QuestionType::Noul,
        instructions: "Is the player hostile?".to_string(),
        state_keys: vec!["met_ace".to_string()],
        criteria: Criteria::Noul(None),
        fallback: Fallback::Bool(false),
    }
}

fn score_config() -> DecisionConfig {
    DecisionConfig {
        question_type: QuestionType::Score,
        instructions: "How much does Ace trust the player?".to_string(),
        state_keys: vec!["met_ace".to_string()],
        criteria: Criteria::Score(vec![
            "No trust.".to_string(),
            "Some trust.".to_string(),
            "Complete trust.".to_string(),
        ]),
        fallback: Fallback::Index(0),
    }
}

#[test]
fn request_shape_follows_the_question_type() {
    let noul = build_decision_request("m", "is_hostile", &noul_config(), &state());
    let value = serde_json::to_value(&noul).unwrap();
    assert_eq!(value["questions"]["is_hostile"]["type"], "noul");
    // An absent `noul` criteria map is omitted from the request.
    assert!(value["questions"]["is_hostile"].get("criteria").is_none());

    let score = build_decision_request("m", "trust", &score_config(), &state());
    let value = serde_json::to_value(&score).unwrap();
    assert_eq!(value["questions"]["trust"]["type"], "score");
    assert_eq!(
        value["questions"]["trust"]["criteria"],
        serde_json::json!(["No trust.", "Some trust.", "Complete trust."])
    );
}

// --- `noul` landing ------------------------------------------------------

fn noul_body(probability: f64) -> String {
    format!(
        r#"{{ "model": "m", "answers": {{ "is_hostile": {{ "type": "noul", "noul": {probability} }} }} }}"#
    )
}

#[test]
fn noul_at_or_above_half_lands_true() {
    for probability in [0.5, 0.96, 1.0] {
        let decision =
            decision_from_body(&noul_body(probability), "is_hostile", &noul_config()).unwrap();
        assert_eq!(decision.action, "true", "noul {probability}");
    }
}

#[test]
fn noul_below_half_lands_false() {
    for probability in [0.0, 0.49] {
        let decision =
            decision_from_body(&noul_body(probability), "is_hostile", &noul_config()).unwrap();
        assert_eq!(decision.action, "false", "noul {probability}");
    }
}

#[test]
fn noul_lands_by_the_threshold_even_out_of_range() {
    // ADR 0005 states the landing rule as the bare `>= 0.5` threshold, so a
    // non-probability value still lands by it rather than taking the
    // fallback. A range guard would be a behaviour change on settled text.
    let above = decision_from_body(&noul_body(1.7), "is_hostile", &noul_config()).unwrap();
    assert_eq!(above.action, "true");
    let below = decision_from_body(&noul_body(-0.3), "is_hostile", &noul_config()).unwrap();
    assert_eq!(below.action, "false");
}

#[test]
fn a_choice_answer_on_a_noul_config_is_wrong_type() {
    let body = r#"{
            "model": "m",
            "answers": { "is_hostile": { "type": "choice", "choice": "warm" } }
        }"#;
    assert!(matches!(
        decision_from_body(body, "is_hostile", &noul_config()).unwrap_err(),
        DecisionError::WrongType { .. }
    ));
}

#[test]
fn a_missing_noul_value_is_unusable_and_takes_the_fallback() {
    let body = r#"{ "model": "m", "answers": { "is_hostile": { "type": "noul" } } }"#;
    let result = decision_from_body(body, "is_hostile", &noul_config());
    assert!(result.is_err());
    let resolved = resolve_or_fallback(&noul_config(), result);
    assert_eq!(resolved.action, "false");
    assert!(resolved.is_fallback());
}

#[test]
fn noul_criteria_map_is_sent_when_present() {
    let config = DecisionConfig {
        criteria: Criteria::Noul(Some(HashMap::from([
            ("true".to_string(), "Hostile.".to_string()),
            ("false".to_string(), "Not hostile.".to_string()),
        ]))),
        ..noul_config()
    };
    let request = build_decision_request("m", "is_hostile", &config, &state());
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["questions"]["is_hostile"]["type"], "noul");
    assert_eq!(
        value["questions"]["is_hostile"]["criteria"]["true"],
        "Hostile."
    );
    assert_eq!(
        value["questions"]["is_hostile"]["criteria"]["false"],
        "Not hostile."
    );
}

#[test]
fn typed_fallback_lands_on_its_typed_key() {
    let noul = resolve_or_fallback(&noul_config(), Err(DecisionError::Http));
    assert_eq!(noul.action, "false");

    let score = resolve_or_fallback(&score_config(), Err(DecisionError::Http));
    assert_eq!(score.action, "0");
}

// --- `score` landing -----------------------------------------------------

/// A score response with the research's echoed `legend`. `score_body` leaves
/// the probabilities to the caller so landing can be probed directly.
fn score_body(score: f64, probabilities: &str) -> String {
    format!(
        r#"{{ "model": "m", "answers": {{ "trust": {{ "type": "score", "score": {score}, "legend": {{ "0": "No trust.", "1": "Some trust.", "2": "Complete trust." }}, "probabilities": {probabilities} }} }} }}"#
    )
}

#[test]
fn score_lands_on_the_highest_probability_index() {
    let decision = decision_from_body(
        &score_body(1.2, r#"{ "0": 0.05, "1": 0.15, "2": 0.80 }"#),
        "trust",
        &score_config(),
    )
    .unwrap();
    assert_eq!(decision.action, "2");
}

#[test]
fn score_scalar_and_echoed_legend_are_provenance_not_landing() {
    // The scalar (`0.05`) sits near index 0, but the probabilities favour
    // index 2: ADR 0005 lands on argmax(probabilities), never a rounded or
    // floored scalar (scores are not calibrated magnitudes). The wire layer
    // retains the echoed legend for callers; the network client clears it
    // and logs only the authored legend.
    let decision = decision_from_body(
        &score_body(0.05, r#"{ "0": 0.0, "1": 0.0, "2": 1.0 }"#),
        "trust",
        &score_config(),
    )
    .unwrap();
    assert_eq!(decision.action, "2");
    assert_eq!(decision.score, Some(0.05));
    assert_eq!(
        decision
            .legend
            .as_ref()
            .and_then(|legend| legend.get("2"))
            .map(String::as_str),
        Some("Complete trust.")
    );
}

#[test]
fn score_ties_land_on_the_lowest_index() {
    // Iterating legend indices (not the HashMap) makes a tie deterministic.
    let decision = decision_from_body(
        &score_body(1.0, r#"{ "1": 0.5, "2": 0.5 }"#),
        "trust",
        &score_config(),
    )
    .unwrap();
    assert_eq!(decision.action, "1");
}

#[test]
fn score_probabilities_outside_the_legend_are_ignored() {
    // `9` is the highest value but is not a legend index, so it cannot land
    // the script; the best in-range entry wins.
    let decision = decision_from_body(
        &score_body(9.0, r#"{ "0": 0.3, "1": 0.2, "9": 1.0 }"#),
        "trust",
        &score_config(),
    )
    .unwrap();
    assert_eq!(decision.action, "0");
}

#[test]
fn score_confidence_is_captured() {
    let body = r#"{
            "model": "m",
            "answers": { "trust": { "type": "score", "score": 1.0, "confidence": 0.99, "probabilities": { "0": 0.0, "1": 0.0, "2": 1.0 } } }
        }"#;
    let decision = decision_from_body(body, "trust", &score_config()).unwrap();
    assert_eq!(decision.confidence, Some(0.99));
}

#[test]
fn score_without_usable_probabilities_takes_the_fallback() {
    // A score answer with no probabilities (or none naming a legend index)
    // is unusable, not a silent landing on index 0.
    let body = r#"{ "model": "m", "answers": { "trust": { "type": "score", "score": 1.0 } } }"#;
    let result = decision_from_body(body, "trust", &score_config());
    assert!(matches!(&result, Err(DecisionError::UnusableAnswer { .. })));
    let resolved = resolve_or_fallback(&score_config(), result);
    assert_eq!(resolved.action, "0");
    assert!(resolved.is_fallback());
}

#[test]
fn a_choice_answer_on_a_score_config_is_wrong_type() {
    let body = r#"{
            "model": "m",
            "answers": { "trust": { "type": "choice", "choice": "warm" } }
        }"#;
    assert!(matches!(
        decision_from_body(body, "trust", &score_config()).unwrap_err(),
        DecisionError::WrongType { .. }
    ));
}

// --- key validation ------------------------------------------------------

#[test]
fn verdict_from_status_follows_the_settled_table() {
    assert_eq!(verdict_from_status(200), KeyVerdict::Accepted);
    assert_eq!(verdict_from_status(401), KeyVerdict::Rejected);
    assert_eq!(verdict_from_status(403), KeyVerdict::Rejected);
    assert_eq!(verdict_from_status(429), KeyVerdict::Unreachable);
    assert_eq!(verdict_from_status(500), KeyVerdict::Unreachable);
    assert_eq!(
        verdict_from_status(0),
        KeyVerdict::Unreachable,
        "0 is the transport sentinel"
    );
}
