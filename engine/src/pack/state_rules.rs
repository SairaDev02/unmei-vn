//! The rules that read the declared state vocabulary: every `set` target, every
//! `{var}` reference, every condition's key and literal, and each key's declared
//! kind (content format §4). They run only when `state.ron` decoded; a broken
//! state file skips them rather than guessing (check-and-preview spec §3).

use std::collections::HashMap;

use crate::content::{Line, Literal, Scene, StateValue};
use crate::finding::{rules, Finding};

pub(super) fn validate(
    state: &HashMap<String, StateValue>,
    scenes: &HashMap<String, Scene>,
    errors: &mut Vec<Finding>,
) {
    for (scene_id, scene) in scenes {
        let file = format!("scenes/{scene_id}.unmei");

        for (idx, line) in scene.lines.iter().enumerate() {
            let line_no = scene.source_line(idx);
            match line {
                Line::Set { name, value } => match state.get(name) {
                    None => errors.push(Finding::at_line(
                        &file,
                        line_no,
                        rules::PACK_STATE,
                        format!("`set {name}` but `{name}` is not declared in state.ron"),
                    )),
                    Some(declared) if !kind_matches(declared, value) => {
                        errors.push(Finding::at_line(
                            &file,
                            line_no,
                            rules::PACK_STATE,
                            format!(
                                "`set {name}` assigns a {}, but `{name}` is declared as a {} in state.ron",
                                literal_kind(value),
                                state_kind(declared)
                            ),
                        ));
                    }
                    Some(_) => {}
                },
                Line::Say { text, .. } => {
                    for var in var_refs(text) {
                        if !state.contains_key(&var) {
                            errors.push(Finding::at_line(
                                &file,
                                line_no,
                                rules::PACK_STATE,
                                format!("unknown variable `{{{var}}}` — not declared in state.ron"),
                            ));
                        }
                    }
                }
                Line::If { condition, .. } => match state.get(&condition.key) {
                    None => errors.push(Finding::at_line(
                        &file,
                        line_no,
                        rules::PACK_STATE,
                        format!(
                            "condition key `{}` is not declared in state.ron",
                            condition.key
                        ),
                    )),
                    Some(declared) => {
                        if condition.op.is_ordering() && !matches!(declared, StateValue::Num(_)) {
                            errors.push(Finding::at_line(
                                &file,
                                line_no,
                                rules::PACK_STATE,
                                format!(
                                    "condition `{}` uses an ordering operator, but `{}` is declared as a {} in state.ron — ordering applies to numbers only",
                                    condition.op.as_str(),
                                    condition.key,
                                    state_kind(declared)
                                ),
                            ));
                        } else if condition.bare && !matches!(declared, StateValue::Bool(_)) {
                            errors.push(Finding::at_line(
                                &file,
                                line_no,
                                rules::PACK_STATE,
                                format!(
                                    "bare condition key `{}` must be a bool, but `{}` is declared as a {} in state.ron",
                                    condition.key,
                                    condition.key,
                                    state_kind(declared)
                                ),
                            ));
                        } else if !kind_matches(declared, &condition.value) {
                            errors.push(Finding::at_line(
                                &file,
                                line_no,
                                rules::PACK_STATE,
                                format!(
                                    "condition literal is a {}, but `{}` is declared as a {} in state.ron",
                                    literal_kind(&condition.value),
                                    condition.key,
                                    state_kind(declared)
                                ),
                            ));
                        }
                    }
                },
                _ => {}
            }
        }
    }
}

/// `{var}` references in say-line text (well-formed by parse time).
fn var_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(close) = text[i + 1..].find('}') {
                refs.push(text[i + 1..i + 1 + close].to_string());
                i += close + 2;
                continue;
            } else {
                break;
            }
        }
        i += 1;
    }
    refs
}

/// Name of a declared state value's kind, for error messages.
fn state_kind(v: &StateValue) -> &'static str {
    match v {
        StateValue::Str(_) => "string",
        StateValue::Num(_) => "number",
        StateValue::Bool(_) => "bool",
    }
}

/// Name of a literal's kind, for error messages.
fn literal_kind(v: &Literal) -> &'static str {
    match v {
        Literal::Str(_) => "string",
        Literal::Num(_) => "number",
        Literal::Bool(_) => "bool",
    }
}

/// A key's declared kind is fixed: a literal must have the same kind.
fn kind_matches(declared: &StateValue, literal: &Literal) -> bool {
    matches!(
        (declared, literal),
        (StateValue::Str(_), Literal::Str(_))
            | (StateValue::Num(_), Literal::Num(_))
            | (StateValue::Bool(_), Literal::Bool(_))
    )
}
