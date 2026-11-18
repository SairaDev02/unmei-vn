//! The saving half of a finding: the closing assertion that makes the saved
//! case fail while the problem remains, the path it is written to, and the write
//! that a rerun leaves alone (spec §9.4).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;
use unmei_engine::Pack;

use super::super::finding::{rules, Finding};
use super::super::observe::Observation;
use super::Branch;

/// The closing assertion a saved finding carries, so the case fails while the
/// finding remains (spec §9.4).
fn closing_assertion(finding: &Finding, park: Option<&Observation>, pack: &Pack) -> Vec<String> {
    if finding.rule == rules::NOTICE {
        return vec!["expect notice none".to_string()];
    }
    if finding.rule == rules::DEFENSIVE
        || finding.rule == rules::PANIC
        || finding.rule == rules::UNSETTLED
    {
        return vec!["expect surface play".to_string()];
    }
    if finding.rule == rules::STUCK {
        // A stuck park never moves on, so the next say-line after it is never
        // read. Asserting that read mark fails while the loop remains and
        // passes once the pack walks past the park. A park with no say-line
        // after it has no expressible closure, so it saves no case.
        if let Some(park) = park {
            if let Some((scene, line)) = next_say_line(pack, park) {
                return vec![format!("expect read {scene}:{line}")];
            }
        }
        return Vec::new();
    }
    if let Some(row) = finding.rule.strip_prefix("keiko.log.row-") {
        // The level comes from the row table, never from a copy of it here.
        let level = row
            .parse::<u32>()
            .ok()
            .and_then(crate::failure::FailureRow::of_number)
            .and_then(crate::failure::FailureRow::level)
            .map(|level| {
                if level == log::Level::Warn {
                    "warn"
                } else {
                    "error"
                }
            })
            .unwrap_or("error");
        return vec![format!("expect no-log {level} row {row}")];
    }
    Vec::new()
}

/// The first say-line after the park's own line in its scene, if one exists.
fn next_say_line(pack: &Pack, park: &Observation) -> Option<(String, usize)> {
    let scene = park.scene.as_deref()?;
    let line = usize::try_from(park.line?).ok()?;
    let script = pack.scenes.get(scene)?;
    let park = script
        .lines
        .iter()
        .enumerate()
        .find(|(index, _)| script.source_line(*index) == line)?
        .0;
    script
        .lines
        .iter()
        .enumerate()
        .skip(park + 1)
        .find(|(_, word)| matches!(word, unmei_engine::Line::Say { .. }))
        .map(|(index, _)| (scene.to_string(), script.source_line(index)))
}

/// What the walk has reported so far. `seen` keeps one problem to one finding:
/// the first branch, in walk order, that hits a (rule, message) pair reports
/// and saves it, and later branches that hit the same problem stay silent (spec
/// §9.4). `saved` keeps a rerun from rewriting a file that already exists.
#[derive(Default)]
pub(super) struct Collected {
    pub(super) findings: Vec<Value>,
    saved: HashSet<String>,
    seen: HashSet<(String, String)>,
}

pub(super) fn save_finding(
    pack_root: &Path,
    pack: &Pack,
    branch: &Branch,
    finding: &Finding,
    park: Option<&Observation>,
    collected: &mut Collected,
) {
    if !collected
        .seen
        .insert((finding.rule.clone(), finding.message.clone()))
    {
        return;
    }
    let closing = closing_assertion(finding, park, pack);
    let mut json = crate::keiko::output::finding_json(finding);
    if closing.is_empty() {
        // Rule 29: a saved case must fail while its finding remains. Without an
        // expressible closure the finding is reported unsaved, not written as a
        // case that would pass.
        if let Some(object) = json.as_object_mut() {
            object.insert("saved".to_string(), Value::Bool(false));
        }
        collected.findings.push(json);
        return;
    }
    let path = saved_finding_path(pack_root, branch, finding);
    let relative = path
        .strip_prefix(pack_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
        .replace('\\', "/");
    if collected.saved.insert(relative.clone()) && !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, branch.case_text(&closing));
    }
    if let Some(object) = json.as_object_mut() {
        object.insert("case".to_string(), Value::String(relative));
    }
    collected.findings.push(json);
}

/// `tests/findings/<rule>-<hash>.keiko`, named by the step list's content hash.
fn saved_finding_path(pack_root: &Path, branch: &Branch, finding: &Finding) -> PathBuf {
    pack_root.join("tests").join("findings").join(format!(
        "{}-{}.keiko",
        finding.rule,
        branch.digest()
    ))
}
