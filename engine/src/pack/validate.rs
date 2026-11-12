//! Whole-pack semantic validation: the declared state vocabulary and its fixed
//! kinds, scene-local label and scene-file targets, each decision point's label
//! closure, and the manifest's declared scene order. The loader runs this after
//! every file has decoded; it accumulates every diagnostic instead of stopping
//! at the first.

use std::collections::{BTreeSet, HashMap};

use crate::content::{Line, Literal, Manifest, Npc, Scene, StateValue};
use crate::error::LoadError;

use super::Warning;

pub(super) fn validate(
    manifest: &Manifest,
    state: &HashMap<String, StateValue>,
    scenes: &HashMap<String, Scene>,
    npcs: &HashMap<String, Npc>,
    errors: &mut Vec<LoadError>,
    warnings: &mut Vec<Warning>,
) {
    // Entry scene exists.
    if !scenes.contains_key(&manifest.entry) {
        errors.push(LoadError::new(
            "pack.ron",
            None,
            format!(
                "entry scene `{}` does not exist (scenes: {:?})",
                manifest.entry,
                sorted_keys(scenes)
            ),
        ));
    }

    validate_declared_order(manifest, scenes, errors, warnings);

    for (scene_id, scene) in scenes {
        let file = format!("scenes/{scene_id}.unmei");

        for (idx, line) in scene.lines.iter().enumerate() {
            let line_no = scene.source_line(idx);
            match line {
                Line::Set { name, value } => match state.get(name) {
                    None => errors.push(LoadError::new(
                        &file,
                        Some(line_no),
                        format!("`set {name}` but `{name}` is not declared in state.ron"),
                    )),
                    Some(declared) if !kind_matches(declared, value) => {
                        errors.push(LoadError::new(
                            &file,
                            Some(line_no),
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
                            errors.push(LoadError::new(
                                &file,
                                Some(line_no),
                                format!("unknown variable `{{{var}}}` — not declared in state.ron"),
                            ));
                        }
                    }
                }
                Line::Jump(target) => {
                    if !scene.labels.contains_key(target) && !scenes.contains_key(target) {
                        errors.push(LoadError::new(
                            &file,
                            Some(line_no),
                            format!(
                                "jump target `{target}` is neither a label in `{scene_id}` nor a scene file"
                            ),
                        ));
                    }
                }
                Line::Choice(options) => {
                    for (text, target) in options {
                        if !scene.labels.contains_key(target) && !scenes.contains_key(target) {
                            errors.push(LoadError::new(
                                &file,
                                Some(line_no),
                                format!(
                                    "choice option \"{text}\" targets `{target}`, which is neither a label in `{scene_id}` nor a scene file"
                                ),
                            ));
                        }
                    }
                }
                Line::If { condition, .. } => match state.get(&condition.key) {
                    None => errors.push(LoadError::new(
                        &file,
                        Some(line_no),
                        format!(
                            "condition key `{}` is not declared in state.ron",
                            condition.key
                        ),
                    )),
                    Some(declared) => {
                        if condition.op.is_ordering() && !matches!(declared, StateValue::Num(_)) {
                            errors.push(LoadError::new(
                                &file,
                                Some(line_no),
                                format!(
                                    "condition `{}` uses an ordering operator, but `{}` is declared as a {} in state.ron — ordering applies to numbers only",
                                    condition.op.as_str(),
                                    condition.key,
                                    state_kind(declared)
                                ),
                            ));
                        } else if condition.bare && !matches!(declared, StateValue::Bool(_)) {
                            errors.push(LoadError::new(
                                &file,
                                Some(line_no),
                                format!(
                                    "bare condition key `{}` must be a bool, but `{}` is declared as a {} in state.ron",
                                    condition.key,
                                    condition.key,
                                    state_kind(declared)
                                ),
                            ));
                        } else if !kind_matches(declared, &condition.value) {
                            errors.push(LoadError::new(
                                &file,
                                Some(line_no),
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
                Line::Decide {
                    npc_id,
                    question,
                    prefix,
                } => {
                    validate_decide(
                        scene_id, &file, line_no, npc_id, question, prefix, scene, npcs, errors,
                    );
                }
                _ => {}
            }
        }
    }

    // NPC config checks: state_keys declared, then the type-specific criteria
    // and fallback rules. `choice` validates its fallback against the criteria
    // and keeps the only `none` advisory; typed questions own their shape at
    // deserialization (`try_from`) and their label closure at each decide point.
    for (npc_id, npc) in npcs {
        let file = format!("npcs/{npc_id}.ron");
        for (question, config) in &npc.decisions {
            for key in &config.state_keys {
                if !state.contains_key(key) {
                    errors.push(LoadError::new(
                        &file,
                        None,
                        format!(
                            "decision `{question}`: state_keys entry `{key}` is not declared in state.ron"
                        ),
                    ));
                }
            }
            if let (Some(criteria), Some(fallback)) =
                (config.choice_criteria(), config.choice_fallback())
            {
                if !criteria.contains_key(fallback) {
                    errors.push(LoadError::new(
                        &file,
                        None,
                        format!(
                            "decision `{question}`: fallback `{fallback}` is not a criterion id (criteria: {:?})",
                            sorted_keys(criteria)
                        ),
                    ));
                }
                if !criteria.contains_key("none") {
                    warnings.push(Warning {
                        file: file.clone(),
                        message: format!(
                            "decision `{question}`: no `none` action — the model has no way to say nothing fits"
                        ),
                    });
                }
            }
            // A `score` fallback is a legend index and must be in range: like the
            // choice fallback, its membership in the landing space is a pack-load
            // rule, not a `TryFrom` shape rule (ADR 0005).
            if let (Some(legend), Some(fallback)) =
                (config.score_criteria(), config.score_fallback())
            {
                if fallback >= legend.len() {
                    errors.push(LoadError::new(
                        &file,
                        None,
                        format!(
                            "decision `{question}`: fallback index {fallback} is outside the legend (0..={})",
                            legend.len() - 1
                        ),
                    ));
                }
            }
        }
    }
}

/// Validate the manifest's optional declared scene order (`scenes`).
/// Absent and empty are equivalent — no declared order, so no diagnostics, no
/// entry requirement and no unlisted warnings. With a non-empty list the checks
/// accumulate: one error per distinct repeated id and per distinct id not among
/// the loaded scenes (an id both missing and repeated reports both), and one
/// warning for every loaded scene left unlisted (the entry scene's warning is
/// dedicated). Every diagnostic this block emits is in scene-id order,
/// independent of declaration or directory enumeration order.
fn validate_declared_order(
    manifest: &Manifest,
    scenes: &HashMap<String, Scene>,
    errors: &mut Vec<LoadError>,
    warnings: &mut Vec<Warning>,
) {
    if manifest.scenes.is_empty() {
        return;
    }
    let listed: BTreeSet<&str> = manifest.scenes.iter().map(String::as_str).collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut repeated: BTreeSet<&str> = BTreeSet::new();
    for id in &manifest.scenes {
        if !seen.insert(id.as_str()) {
            repeated.insert(id.as_str());
        }
    }
    // One error per distinct repeated id and per distinct id not among the
    // loaded scenes; an id both missing and repeated reports both.
    for id in listed.iter().copied() {
        if repeated.contains(id) {
            errors.push(LoadError::new(
                "pack.ron",
                None,
                format!("scene `{id}` is listed more than once in `scenes`"),
            ));
        }
        if !scenes.contains_key(id) {
            errors.push(LoadError::new(
                "pack.ron",
                None,
                format!(
                    "scene `{id}` is listed in `scenes` but is not among loaded scenes (scenes: {:?})",
                    sorted_keys(scenes)
                ),
            ));
        }
    }
    // Existing-but-unlisted scenes stay loaded and jump-reachable, with one
    // warning each; the entry scene gets a dedicated warning in place of the
    // generic one. An entry missing outright already errors, so it never
    // reaches this loop. Sorted by scene id.
    for scene_id in sorted_keys(scenes) {
        if listed.contains(scene_id.as_str()) {
            continue;
        }
        let (noun, tail) = if scene_id == manifest.entry {
            (
                "entry scene",
                " — play begins here but it is absent from the Timeline",
            )
        } else {
            ("scene", " — it is absent from the Timeline")
        };
        warnings.push(Warning {
            file: "pack.ron".to_string(),
            message: format!("{noun} `{scene_id}` is not listed in `scenes`{tail}"),
        });
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "one call site passes the loader's current walk state; a context struct would only rename the same nine values"
)]
fn validate_decide(
    scene_id: &str,
    file: &str,
    line_no: usize,
    npc_id: &str,
    question: &str,
    prefix: &str,
    scene: &Scene,
    npcs: &HashMap<String, Npc>,
    errors: &mut Vec<LoadError>,
) {
    let Some(npc) = npcs.get(npc_id) else {
        errors.push(LoadError::new(
            file,
            Some(line_no),
            format!(
                "decide references unknown npc `{npc_id}` (npcs: {:?})",
                npcs.keys().collect::<Vec<_>>()
            ),
        ));
        return;
    };
    let Some(config) = npc.decisions.get(question) else {
        errors.push(LoadError::new(
            file,
            Some(line_no),
            format!(
                "decide references unknown question `{question}` on npc `{npc_id}` (questions: {:?})",
                npc.decisions.keys().collect::<Vec<_>>()
            ),
        ));
        return;
    };

    // Label closure, derived per type (ADR 0005): `<prefix>.<landing key>` for
    // every key the config can reach, including the fallback. `choice` needs
    // each criterion id plus the fallback; `noul` needs both booleans; `score`
    // needs `<prefix>.0` … `<prefix>.<n-1>`.
    for key in config.landing_keys() {
        let label = format!("{prefix}.{key}");
        if !scene.labels.contains_key(&label) {
            errors.push(LoadError::new(
                file,
                Some(line_no),
                format!(
                    "decision point `{npc_id}.{question} -> {prefix}`: missing label `{label}` in scene `{scene_id}`"
                ),
            ));
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

fn sorted_keys<V>(map: &HashMap<String, V>) -> Vec<String> {
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    keys
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
