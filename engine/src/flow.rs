//! Flow lint: the pack's control-flow graph, read statically and state-blind
//! (check and preview spec §6).
//!
//! A node is a line position. Each line flows to the next, except a `jump`
//! (its target only) and a `choice` or `decide` (each of its targets). Both
//! arms of an `if`, every choice option and every decision landing key count as
//! reachable whatever the state or the answer would be, so the lint needs
//! neither the state map nor a model. A target resolves exactly as at runtime
//! ([content format §5](../../docs/content-format.md#5-transitions)): a local
//! label first, then a scene file's first line.
//!
//! Three warnings, one rule id each: a scene the manifest's entry scene cannot
//! reach, a label in a reachable scene that no path reaches, and a reachable
//! scene with no way out. A label inside an unreachable scene is not reported
//! again, and a `decide` with a missing landing label stays a `pack.decision`
//! load error, so the lint adds no rule for it.

use std::collections::{BTreeSet, HashMap, VecDeque};

use crate::content::{Line, Pack, Scene};
use crate::finding::{rules, Finding, Location};

/// Every flow warning a loaded pack earns.
pub fn findings(pack: &Pack) -> Vec<Finding> {
    let reached = reach(pack);
    let mut findings = Vec::new();
    for scene_id in sorted_scene_ids(pack) {
        let scene = &pack.scenes[scene_id];
        let file = format!("scenes/{scene_id}.unmei");
        if !reached.scenes.contains(scene_id) {
            findings.push(unreachable_scene(pack, scene_id));
            // A label inside an unreachable scene is not reported again: the
            // scene's own warning is what the author acts on.
            continue;
        }
        if !exits(pack, scene_id) {
            findings.push(no_exit(scene_id));
        }
        for (name, idx) in labels_by_position(scene) {
            if !reached.line(scene_id, idx) {
                findings.push(Finding::warning_at_line(
                    &file,
                    scene.source_line(idx),
                    rules::FLOW_UNREACHABLE_LABEL,
                    format!(
                        "label `{name}` is unreachable: no jump, choice option, decision landing or fall-through path reaches it"
                    ),
                ));
            }
        }
    }
    findings
}

/// What a walk from the entry scene reaches: the scenes it enters, and the line
/// positions it visits in each.
#[derive(Default)]
struct Reach<'a> {
    scenes: BTreeSet<&'a str>,
    lines: HashMap<&'a str, BTreeSet<usize>>,
}

impl<'a> Reach<'a> {
    fn enter(&mut self, scene_id: &'a str) {
        self.scenes.insert(scene_id);
    }

    /// Mark one position reached; false when the walk already visited it.
    fn visit(&mut self, scene_id: &'a str, idx: usize) -> bool {
        self.enter(scene_id);
        self.lines.entry(scene_id).or_default().insert(idx)
    }

    fn line(&self, scene_id: &str, idx: usize) -> bool {
        self.lines
            .get(scene_id)
            .is_some_and(|lines| lines.contains(&idx))
    }
}

/// The scenes and positions a walk from the manifest's entry scene reaches.
fn reach(pack: &Pack) -> Reach<'_> {
    let mut reached = Reach::default();
    let Some((entry, scene)) = pack.scenes.get_key_value(&pack.manifest.entry) else {
        return reached;
    };
    let entry = entry.as_str();
    reached.enter(entry);

    let mut queue: VecDeque<(&str, usize)> = VecDeque::new();
    if !scene.lines.is_empty() {
        queue.push_back((entry, 0));
    }
    while let Some((scene_id, idx)) = queue.pop_front() {
        if !reached.visit(scene_id, idx) {
            continue;
        }
        for (next_scene, next_idx) in successors(pack, scene_id, idx) {
            reached.enter(next_scene);
            if next_idx < pack.scenes[next_scene].lines.len() {
                queue.push_back((next_scene, next_idx));
            }
        }
    }
    reached
}

/// Every position one line leads to, in the runtime's transition rules. A
/// position one past a scene's last line is that file's end, and it is never
/// queued.
fn successors<'a>(pack: &'a Pack, scene_id: &'a str, idx: usize) -> Vec<(&'a str, usize)> {
    let scene = &pack.scenes[scene_id];
    let next = (scene_id, idx + 1);
    match &scene.lines[idx] {
        // A `jump` flows only to its target.
        Line::Jump(target) => target_position(pack, scene, target).into_iter().collect(),
        // A `choice` or `decide` flows to each of its targets, never to the
        // line after the block: the block only ever moves to a target.
        Line::Choice(options) => options
            .iter()
            .filter_map(|(_, target)| target_position(pack, scene, target))
            .collect(),
        Line::Decide {
            npc_id,
            question,
            prefix,
        } => landing_positions(pack, scene, npc_id, question, prefix),
        // Both arms are reachable state-blind: the then arm falls through, and
        // a false condition resumes at the else arm, or after the block.
        Line::If {
            else_index,
            end_index,
            ..
        } => {
            let mut out = vec![next];
            out.push((scene_id, else_index.unwrap_or(*end_index) + 1));
            out
        }
        // Reaching the marker always means the taken then arm skips the else
        // arm, exactly as the runner moves the pc.
        Line::Else { end_index } => vec![(scene_id, end_index + 1)],
        _ => vec![next],
    }
}

/// The one transition rule for `jump` and a choice option (content format §5):
/// a local label first, then a scene file from its first line. An unknown
/// target is already a `pack.scene.target` load error, so it yields no edge.
fn target_position<'a>(pack: &'a Pack, scene: &'a Scene, target: &str) -> Option<(&'a str, usize)> {
    if let Some(idx) = scene.labels.get(target) {
        return Some((scene.id.as_str(), *idx));
    }
    let (scene_id, _) = pack.scenes.get_key_value(target)?;
    Some((scene_id.as_str(), 0))
}

/// Every `<prefix>.<landing key>` label a decision point can land on. Pack load
/// already requires each to exist, and a missing one stays that load error.
fn landing_positions<'a>(
    pack: &'a Pack,
    scene: &'a Scene,
    npc_id: &str,
    question: &str,
    prefix: &str,
) -> Vec<(&'a str, usize)> {
    let Some(config) = pack
        .npcs
        .get(npc_id)
        .and_then(|npc| npc.decisions.get(question))
    else {
        return Vec::new();
    };
    config
        .landing_keys()
        .iter()
        .filter_map(|key| {
            let label = format!("{prefix}.{key}");
            scene
                .labels
                .get(&label)
                .map(|idx| (scene.id.as_str(), *idx))
        })
        .collect()
}

fn unreachable_scene(pack: &Pack, scene_id: &str) -> Finding {
    Finding::warning(
        format!("scenes/{scene_id}.unmei"), Location::File,
        rules::FLOW_UNREACHABLE_SCENE,
        format!(
            "scene `{scene_id}` is unreachable: no path from entry scene `{}` reaches it (the Timeline is not a path)",
            pack.manifest.entry
        ),
    )
}

fn no_exit(scene_id: &str) -> Finding {
    Finding::warning(
        format!("scenes/{scene_id}.unmei"), Location::File,
        rules::FLOW_NO_EXIT,
        format!(
            "scene `{scene_id}` has no exit: every path loops without reaching the end of the file or another scene"
        ),
    )
}

/// Whether play can leave a scene: from its first line, some path reaches the
/// end of the file or a line in another scene.
fn exits(pack: &Pack, scene_id: &str) -> bool {
    let len = pack.scenes[scene_id].lines.len();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let mut queue: VecDeque<usize> = VecDeque::from([0]);
    while let Some(idx) = queue.pop_front() {
        if idx >= len {
            return true;
        }
        if !seen.insert(idx) {
            continue;
        }
        for (next_scene, next_idx) in successors(pack, scene_id, idx) {
            if next_scene != scene_id {
                return true;
            }
            queue.push_back(next_idx);
        }
    }
    false
}

/// A scene's labels in line order, so the lint's output never depends on
/// `HashMap` iteration.
fn labels_by_position(scene: &Scene) -> Vec<(&str, usize)> {
    let mut labels: Vec<(&str, usize)> = scene
        .labels
        .iter()
        .map(|(name, idx)| (name.as_str(), *idx))
        .collect();
    labels.sort_unstable_by_key(|(name, idx)| (*idx, *name));
    labels
}

/// The loaded scene ids in byte order, so the lint's output never depends on
/// `HashMap` iteration.
fn sorted_scene_ids(pack: &Pack) -> Vec<&str> {
    let mut ids: Vec<&str> = pack.scenes.keys().map(String::as_str).collect();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests;
