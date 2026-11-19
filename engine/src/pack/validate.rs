//! Whole-pack semantic validation: the manifest's entry scene and declared
//! scene order, scene-local label and scene-file targets, each decision point's
//! label closure and each NPC config's own shape. The loader runs this after
//! every file has decoded, and it accumulates every diagnostic instead of
//! stopping at the first. The manifest rules run only when `pack.ron` decoded
//! and the state rules, which live in `state_rules`, only when `state.ron` did
//! (check-and-preview spec §3).

use std::collections::{BTreeSet, HashMap};

use crate::content::{Line, Manifest, Npc, Scene, StateValue};
use crate::finding::{rules, Finding};

pub(super) fn validate(
    manifest: Option<&Manifest>,
    state: Option<&HashMap<String, StateValue>>,
    scenes: &HashMap<String, Scene>,
    npcs: &HashMap<String, Npc>,
    errors: &mut Vec<Finding>,
    warnings: &mut Vec<Finding>,
) {
    if let Some(manifest) = manifest {
        validate_entry(manifest, scenes, errors);
        validate_declared_order(manifest, scenes, errors, warnings);
    }
    validate_wait_free_cycles(scenes, errors);
    if let Some(state) = state {
        super::state_rules::validate(state, scenes, errors);
    }
    validate_targets(scenes, npcs, errors);
    validate_npc_configs(state, npcs, errors, warnings);
}

/// The manifest's entry scene must be one of the loaded scenes.
fn validate_entry(manifest: &Manifest, scenes: &HashMap<String, Scene>, errors: &mut Vec<Finding>) {
    if scenes.contains_key(&manifest.entry) {
        return;
    }
    errors.push(Finding::new(
        "pack.ron",
        "",
        rules::PACK_MANIFEST,
        format!(
            "entry scene `{}` does not exist (scenes: {:?})",
            manifest.entry,
            sorted_keys(scenes)
        ),
    ));
}

/// Scene-file and label targets, including each `decide` point's label closure.
/// These rules read no manifest and no state, so they run whatever else
/// decoded.
fn validate_targets(
    scenes: &HashMap<String, Scene>,
    npcs: &HashMap<String, Npc>,
    errors: &mut Vec<Finding>,
) {
    for (scene_id, scene) in scenes {
        let file = format!("scenes/{scene_id}.unmei");

        for (idx, line) in scene.lines.iter().enumerate() {
            let line_no = scene.source_line(idx);
            match line {
                Line::Jump(target) => {
                    if !scene.labels.contains_key(target) && !scenes.contains_key(target) {
                        errors.push(Finding::at_line(
                            &file,
                            line_no,
                            rules::PACK_SCENE_TARGET,
                            format!(
                                "jump target `{target}` is neither a label in `{scene_id}` nor a scene file"
                            ),
                        ));
                    }
                }
                Line::Choice(options) => {
                    for (text, target) in options {
                        if !scene.labels.contains_key(target) && !scenes.contains_key(target) {
                            errors.push(Finding::at_line(
                                &file,
                                line_no,
                                rules::PACK_SCENE_TARGET,
                                format!(
                                    "choice option \"{text}\" targets `{target}`, which is neither a label in `{scene_id}` nor a scene file"
                                ),
                            ));
                        }
                    }
                }
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
}

/// NPC config checks: `state_keys` declared (only when the state decoded), then
/// the type-specific criteria and fallback rules. `choice` validates its
/// fallback against the criteria and keeps the only `none` advisory; typed
/// questions own their shape at deserialization (`try_from`) and their label
/// closure at each decide point.
fn validate_npc_configs(
    state: Option<&HashMap<String, StateValue>>,
    npcs: &HashMap<String, Npc>,
    errors: &mut Vec<Finding>,
    warnings: &mut Vec<Finding>,
) {
    for (npc_id, npc) in npcs {
        let file = format!("npcs/{npc_id}.ron");
        for (question, config) in &npc.decisions {
            if let Some(state) = state {
                for key in &config.state_keys {
                    if !state.contains_key(key) {
                        errors.push(Finding::new(
                            &file,
                            "",
                            rules::PACK_DECISION,
                            format!(
                                "decision `{question}`: state_keys entry `{key}` is not declared in state.ron"
                            ),
                        ));
                    }
                }
            }
            if let (Some(criteria), Some(fallback)) =
                (config.choice_criteria(), config.choice_fallback())
            {
                if !criteria.contains_key(fallback) {
                    errors.push(Finding::new(
                        &file,
                        "",
                        rules::PACK_DECISION,
                        format!(
                            "decision `{question}`: fallback `{fallback}` is not a criterion id (criteria: {:?})",
                            sorted_keys(criteria)
                        ),
                    ));
                }
                if !criteria.contains_key("none") {
                    warnings.push(Finding::warning(
                        &file,
                        "",
                        rules::PACK_DECISION_NO_NONE,
                        format!(
                            "decision `{question}`: no `none` action — the model has no way to say nothing fits"
                        ),
                    ));
                }
            }
            // A `score` fallback is a legend index and must be in range: like the
            // choice fallback, its membership in the landing space is a pack-load
            // rule, not a `TryFrom` shape rule (ADR 0005).
            if let (Some(legend), Some(fallback)) =
                (config.score_criteria(), config.score_fallback())
            {
                if fallback >= legend.len() {
                    errors.push(Finding::new(
                        &file,
                        "",
                        rules::PACK_DECISION,
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
    errors: &mut Vec<Finding>,
    warnings: &mut Vec<Finding>,
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
            errors.push(Finding::new(
                "pack.ron",
                "",
                rules::PACK_MANIFEST,
                format!("scene `{id}` is listed more than once in `scenes`"),
            ));
        }
        if !scenes.contains_key(id) {
            errors.push(Finding::new(
                "pack.ron",
                "",
                rules::PACK_MANIFEST,
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
        warnings.push(Finding::warning(
            "pack.ron",
            "",
            rules::PACK_MANIFEST_UNLISTED,
            format!("{noun} `{scene_id}` is not listed in `scenes`{tail}"),
        ));
    }
}

/// Reject every `jump` cycle that has no say-line, `choice`, `decide` or `if`
/// on it: once entered, the runner would never park. A cycle through an `if`
/// may leave on some state, so pack load leaves it to the runner, which
/// catches the state that spins ([`crate::runner::RunnerError::WaitFreeCycle`]).
/// Each cycle is reported once, at its first `jump` in (scene id, line)
/// order, and the cycles are reported in that order too.
fn validate_wait_free_cycles(scenes: &HashMap<String, Scene>, errors: &mut Vec<Finding>) {
    // Every position has at most one successor, so each walk either ends or
    // runs into a cycle; `done` marks positions whose walk is settled.
    let mut ids: Vec<&str> = scenes.keys().map(String::as_str).collect();
    ids.sort_unstable();
    let mut done: BTreeSet<Pos<'_>> = BTreeSet::new();
    let mut cycles: Vec<Pos<'_>> = Vec::new();
    for scene_id in ids {
        for start in 0..scenes[scene_id].lines.len() {
            let mut path: Vec<Pos<'_>> = Vec::new();
            let mut on_path: BTreeSet<Pos<'_>> = BTreeSet::new();
            let mut pos = Some((scene_id, start));
            while let Some(p) = pos {
                if done.contains(&p) {
                    break;
                }
                if on_path.contains(&p) {
                    // The cycle is the tail of `path` from `p`; report it at
                    // its first `jump` (a cycle always holds one: only a
                    // `jump` moves backward or across files).
                    let from = path.iter().position(|q| *q == p).unwrap_or(0);
                    let first_jump = path[from..]
                        .iter()
                        .filter(|(s, i)| matches!(scenes[*s].lines[*i], Line::Jump(_)))
                        .min()
                        .copied();
                    cycles.extend(first_jump);
                    break;
                }
                path.push(p);
                on_path.insert(p);
                pos = step(scenes, p);
            }
            done.extend(path);
        }
    }

    cycles.sort();
    for (scene_id, idx) in cycles {
        let scene = &scenes[scene_id];
        let Line::Jump(target) = &scene.lines[idx] else {
            continue;
        };
        errors.push(Finding::at_line(
            format!("scenes/{scene_id}.unmei"),
            scene.source_line(idx),
            rules::PACK_SCENE_CYCLE,
            format!(
                "`jump {target}` closes a cycle that reaches no say-line, choice or decide — play would freeze here"
            ),
        ));
    }
}

/// A script position: scene id and line index.
type Pos<'a> = (&'a str, usize);

/// The one position a command moves to without parking or branching; `None`
/// for a park, an `if`, the end of a scene or an unresolved target.
fn step<'a>(scenes: &'a HashMap<String, Scene>, (scene_id, idx): Pos<'a>) -> Option<Pos<'a>> {
    let scene = &scenes[scene_id];
    let to = match &scene.lines[idx] {
        Line::Say { .. } | Line::Choice(_) | Line::Decide { .. } | Line::If { .. } => return None,
        Line::Else { end_index } => (scene_id, end_index + 1),
        Line::Jump(target) => match scene.labels.get(target) {
            Some(&label) => (scene_id, label),
            None => (scenes.get_key_value(target)?.0.as_str(), 0),
        },
        _ => (scene_id, idx + 1),
    };
    (to.1 < scenes[to.0].lines.len()).then_some(to)
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
    errors: &mut Vec<Finding>,
) {
    let Some(npc) = npcs.get(npc_id) else {
        errors.push(Finding::at_line(
            file,
            line_no,
            rules::PACK_DECISION,
            format!(
                "decide references unknown npc `{npc_id}` (npcs: {:?})",
                npcs.keys().collect::<Vec<_>>()
            ),
        ));
        return;
    };
    let Some(config) = npc.decisions.get(question) else {
        errors.push(Finding::at_line(
            file,
            line_no,
            rules::PACK_DECISION,
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
            errors.push(Finding::at_line(
                file,
                line_no,
                rules::PACK_DECISION,
                format!(
                    "decision point `{npc_id}.{question} -> {prefix}`: missing label `{label}` in scene `{scene_id}`"
                ),
            ));
        }
    }
}

fn sorted_keys<V>(map: &HashMap<String, V>) -> Vec<String> {
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    keys
}
