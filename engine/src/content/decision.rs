//! Decision configs: the judgment side of a decision point, kept in data
//! (ADR 0003), with `criteria` and `fallback` typed by the question type
//! (ADR 0005). The runtime resolver and the Decisions wire layer live in
//! `crate::decision`.

use std::collections::HashMap;

/// A criterion: one action id with its prose description the model judges against.
pub type Criterion = String;

/// The `type` discriminator on a decision config ([ADR 0005]): which Decisions
/// question the config asks. `choice` is the default when the key is absent.
///
/// Authored in RON as a plain string (`type: "noul"`), so it (de)serializes
/// through the string form rather than serde's bare-variant form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestionType {
    #[default]
    Choice,
    Noul,
    Score,
}

impl QuestionType {
    /// The authoring/wire spelling (`"choice"`, `"noul"`, `"score"`).
    pub fn as_str(self) -> &'static str {
        match self {
            QuestionType::Choice => "choice",
            QuestionType::Noul => "noul",
            QuestionType::Score => "score",
        }
    }

    /// True for the default type; the serialized form omits `type` for `choice`.
    pub fn is_choice(&self) -> bool {
        *self == QuestionType::Choice
    }
}

impl serde::Serialize for QuestionType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for QuestionType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw: String = serde::Deserialize::deserialize(deserializer)?;
        match raw.as_str() {
            "choice" => Ok(QuestionType::Choice),
            "noul" => Ok(QuestionType::Noul),
            "score" => Ok(QuestionType::Score),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["choice", "noul", "score"],
            )),
        }
    }
}

/// A decision config's `criteria`, whose shape follows its [`QuestionType`]
/// ([ADR 0005]). One key mirrors the Decisions wire, which carries one
/// `criteria` field per question.
#[derive(Clone, Debug, PartialEq)]
pub enum Criteria {
    /// `choice`: action id -> description.
    Choice(HashMap<String, Criterion>),
    /// `noul`: an optional map of exactly `"true"`/`"false"` -> description;
    /// `None` means the model judges from `instructions` alone.
    Noul(Option<HashMap<String, Criterion>>),
    /// `score`: ordered level descriptions, lowest first.
    Score(Vec<Criterion>),
}

impl serde::Serialize for Criteria {
    /// Serialize the inner shape directly: a `noul` criteria map must be a map,
    /// never RON's `Some({...})` option wrapper.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Criteria::Choice(map) => map.serialize(serializer),
            Criteria::Noul(Some(map)) => map.serialize(serializer),
            Criteria::Noul(None) => serializer.serialize_unit(),
            Criteria::Score(levels) => levels.serialize(serializer),
        }
    }
}

impl Criteria {
    /// True when an optional `noul` criteria map is absent (the key is omitted).
    pub fn is_absent(&self) -> bool {
        matches!(self, Criteria::Noul(None))
    }
}

/// The fallback landing key, typed to the [`QuestionType`]'s landing space
/// ([ADR 0005]). Applied on API failure, timeout, missing or unusable answer,
/// or unexpected response type.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum Fallback {
    /// `noul`: `true` / `false`.
    Bool(bool),
    /// `score`: a 0-based legend index.
    Index(usize),
    /// `choice`: a criterion id.
    Action(String),
}

impl Fallback {
    /// The landing-key suffix this fallback names (`cold`, `true`, `0`).
    pub fn landing_key(&self) -> String {
        match self {
            Fallback::Action(action) => action.clone(),
            Fallback::Bool(value) => value.to_string(),
            Fallback::Index(index) => index.to_string(),
        }
    }
}

/// One decision config in an NPC file — the judgment side of a decision point
/// ([ADR 0003]: judgment in data, flow in script). Its `type` fixes the shape
/// of `criteria` and the landing space of `fallback` ([ADR 0005]).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(try_from = "RawDecisionConfig")]
pub struct DecisionConfig {
    /// The question type; omitted from serialized output for the `choice` default.
    #[serde(rename = "type", skip_serializing_if = "QuestionType::is_choice")]
    pub question_type: QuestionType,
    pub instructions: String,
    /// State-map keys sent in the Decisions request; all must be declared in `state.ron`.
    pub state_keys: Vec<String>,
    /// Shaped by `question_type`; omitted when a `noul` config has no criteria map.
    #[serde(skip_serializing_if = "Criteria::is_absent")]
    pub criteria: Criteria,
    /// Landing key used on API failure, timeout, missing answer, or unexpected type.
    pub fallback: Fallback,
}

impl DecisionConfig {
    /// The action -> description map, when this is a well-formed `choice`
    /// config. `None` for typed questions or a type/criteria mismatch, so
    /// callers branch on the type in one place rather than re-matching both
    /// halves of the pair.
    pub fn choice_criteria(&self) -> Option<&HashMap<String, Criterion>> {
        match (self.question_type, &self.criteria) {
            (QuestionType::Choice, Criteria::Choice(map)) => Some(map),
            _ => None,
        }
    }

    /// The criterion id this config falls back to, when it is a well-formed
    /// `choice` config.
    pub fn choice_fallback(&self) -> Option<&str> {
        match (self.question_type, &self.fallback) {
            (QuestionType::Choice, Fallback::Action(action)) => Some(action.as_str()),
            _ => None,
        }
    }

    /// The fallback legend index, when this is a well-formed `score` config.
    /// Pack load checks it lies inside the legend (ADR 0005).
    pub fn score_fallback(&self) -> Option<usize> {
        match (self.question_type, &self.fallback) {
            (QuestionType::Score, Fallback::Index(index)) => Some(*index),
            _ => None,
        }
    }

    /// The ordered level descriptions, when this is a well-formed `score`
    /// config. `None` for the other types, so callers branch on the type in one
    /// place rather than re-matching both halves of the pair.
    pub fn score_criteria(&self) -> Option<&[Criterion]> {
        match (self.question_type, &self.criteria) {
            (QuestionType::Score, Criteria::Score(levels)) => Some(levels),
            _ => None,
        }
    }

    /// The landing-key suffixes this config can reach: every criterion id plus
    /// the fallback (`choice`), both booleans (`noul`), or every legend index
    /// (`score`). Pack load requires `<prefix>.<key>` for each (ADR 0005). A
    /// type/criteria mismatch — impossible through deserialization — yields no
    /// keys rather than a partial set.
    pub fn landing_keys(&self) -> Vec<String> {
        match (self.question_type, &self.criteria, &self.fallback) {
            (QuestionType::Choice, Criteria::Choice(criteria), Fallback::Action(fallback)) => {
                let mut keys: Vec<String> = criteria.keys().cloned().collect();
                if !criteria.contains_key(fallback) {
                    keys.push(fallback.clone());
                }
                keys
            }
            (QuestionType::Noul, Criteria::Noul(_), Fallback::Bool(_)) => {
                vec!["true".to_string(), "false".to_string()]
            }
            (QuestionType::Score, Criteria::Score(levels), Fallback::Index(_)) => {
                (0..levels.len()).map(|index| index.to_string()).collect()
            }
            // A type/criteria mismatch cannot be built through deserialization
            // (`try_from` types the pair together). Keep this total rather than
            // panicking on a hand-built config.
            _ => Vec::new(),
        }
    }
}

/// The shape `criteria`/`fallback` deserialize into before [`DecisionConfig`]
/// types them per the `type` discriminator. Kept private so the typed
/// representation is the only public one.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDecisionConfig {
    #[serde(default, rename = "type")]
    question_type: QuestionType,
    instructions: String,
    state_keys: Vec<String>,
    #[serde(default)]
    criteria: RawCriteria,
    fallback: RawFallback,
}

#[derive(Default, serde::Deserialize)]
#[serde(untagged)]
enum RawCriteria {
    /// The key is absent; only a `noul` config may omit `criteria`.
    #[default]
    Absent,
    Map(HashMap<String, Criterion>),
    Levels(Vec<Criterion>),
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum RawFallback {
    Bool(bool),
    Index(usize),
    Action(String),
}

impl TryFrom<RawDecisionConfig> for DecisionConfig {
    type Error = String;

    fn try_from(raw: RawDecisionConfig) -> Result<Self, Self::Error> {
        let RawDecisionConfig {
            question_type,
            instructions,
            state_keys,
            criteria,
            fallback,
        } = raw;
        let (criteria, fallback) = match question_type {
            QuestionType::Choice => {
                let RawCriteria::Map(map) = criteria else {
                    return Err(
                        "a `choice` config needs a `criteria` map of action id -> description"
                            .into(),
                    );
                };
                let RawFallback::Action(action) = fallback else {
                    return Err("a `choice` config's `fallback` must be an action id".into());
                };
                (Criteria::Choice(map), Fallback::Action(action))
            }
            QuestionType::Noul => {
                let map = match criteria {
                    RawCriteria::Absent => None,
                    RawCriteria::Map(map) => Some(map),
                    RawCriteria::Levels(_) => {
                        return Err("a `noul` config's `criteria` must be a map of \"true\"/\"false\" descriptions".into())
                    }
                };
                // Both-or-neither: a present map must name exactly the two
                // judgments the landing keys use (ADR 0005).
                if let Some(map) = &map {
                    if map.len() != 2 || !map.contains_key("true") || !map.contains_key("false") {
                        return Err(
                            "a `noul` config's `criteria` map must have exactly the keys \"true\" and \"false\""
                                .into(),
                        );
                    }
                }
                let RawFallback::Bool(value) = fallback else {
                    return Err("a `noul` config's `fallback` must be `true` or `false`".into());
                };
                (Criteria::Noul(map), Fallback::Bool(value))
            }
            QuestionType::Score => {
                let RawCriteria::Levels(levels) = criteria else {
                    return Err(
                        "a `score` config needs a `criteria` array of level descriptions".into(),
                    );
                };
                if levels.len() < 2 {
                    return Err(
                        "a `score` config's `criteria` needs at least two levels, lowest first"
                            .into(),
                    );
                }
                let RawFallback::Index(index) = fallback else {
                    return Err("a `score` config's `fallback` must be a legend index".into());
                };
                (Criteria::Score(levels), Fallback::Index(index))
            }
        };
        Ok(DecisionConfig {
            question_type,
            instructions,
            state_keys,
            criteria,
            fallback,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(
        question_type: QuestionType,
        criteria: Criteria,
        fallback: Fallback,
    ) -> DecisionConfig {
        DecisionConfig {
            question_type,
            instructions: "test".to_string(),
            state_keys: Vec::new(),
            criteria,
            fallback,
        }
    }

    /// Label closure is derived from the type + criteria + fallback pair
    /// (ADR 0005): every key the config can land on, fallback included.
    #[test]
    fn landing_keys_are_derived_per_type() {
        let mut criteria = HashMap::new();
        criteria.insert("warm".to_string(), "A warm greeting.".to_string());
        criteria.insert("cold".to_string(), "A cold greeting.".to_string());
        let choice = config(
            QuestionType::Choice,
            Criteria::Choice(criteria),
            Fallback::Action("none".to_string()),
        );
        let mut keys = choice.landing_keys();
        keys.sort();
        assert_eq!(keys, vec!["cold", "none", "warm"]);

        let mut map = HashMap::new();
        map.insert("true".to_string(), "Hostile.".to_string());
        map.insert("false".to_string(), "Not hostile.".to_string());
        let noul = config(
            QuestionType::Noul,
            Criteria::Noul(Some(map)),
            Fallback::Bool(false),
        );
        assert_eq!(noul.landing_keys(), vec!["true", "false"]);

        let score = config(
            QuestionType::Score,
            Criteria::Score(vec![
                "low".to_string(),
                "mid".to_string(),
                "high".to_string(),
            ]),
            Fallback::Index(0),
        );
        assert_eq!(score.landing_keys(), vec!["0", "1", "2"]);
    }

    /// A `score` legend needs at least two levels (lowest first); one level is a
    /// load-time error ([ADR 0005]).
    #[test]
    fn score_criteria_needs_at_least_two_levels() {
        let ron = r#"(
            type: "score",
            instructions: "How much trust?",
            state_keys: [],
            criteria: ["Only one level."],
            fallback: 0,
        )"#;
        let err = ron::from_str::<DecisionConfig>(ron).unwrap_err();
        assert!(err.to_string().contains("at least two levels"), "{err}");
    }
}
