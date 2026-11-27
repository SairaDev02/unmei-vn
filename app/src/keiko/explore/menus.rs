//! The menu walk (spec §9.5): from each stop, every walk key, `Enter` and
//! `Esc`, breadth first, within exploration's budget.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::path::Path;

use serde_json::{json, Value};
use unmei_engine::keiko::{Header, HeaderLine, Step};
use unmei_engine::Pack;

use super::save::{save_finding, Collected};
use super::{execute, load_branch, Branch, Report};
use crate::keiko::finding::{rules, Finding, Location};
use crate::keiko::observe::Observation;
use crate::keiko::take::TakeResult;

/// The keys tried at every stop, in order.
const MOVES: [&str; 7] = [
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Tab",
    "Enter",
    "Escape",
];

/// How many `Esc` presses may pass before a surface counts as not unwinding.
const UNWIND_PRESSES: usize = 4;

/// Per surface: the controls it listed and the ones the walk focused.
#[derive(Default)]
struct MenuCoverage {
    surfaces: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
}

impl MenuCoverage {
    fn observe(&mut self, observation: &Observation) {
        if observation.controls.is_empty() {
            return;
        }
        let (listed, focused) = self
            .surfaces
            .entry(observation.surface.clone())
            .or_default();
        for control in &observation.controls {
            listed.insert(control.id.clone());
            if control.focused {
                focused.insert(control.id.clone());
            }
        }
    }

    fn to_json(&self) -> Value {
        let surfaces: Vec<Value> = self
            .surfaces
            .iter()
            .map(|(surface, (listed, focused))| {
                let never: Vec<&String> = listed.difference(focused).collect();
                json!({
                    "surface": surface,
                    "focused": focused.len(),
                    "listed": listed.len(),
                    "never_focused": never,
                })
            })
            .collect();
        let summary: Vec<String> = self
            .surfaces
            .iter()
            .map(|(surface, (listed, focused))| {
                format!("{surface} {}/{}", focused.len(), listed.len())
            })
            .collect();
        let unreached: Vec<String> = self
            .surfaces
            .values()
            .flat_map(|(listed, focused)| listed.difference(focused).cloned())
            .collect();
        json!({
            "surfaces": surfaces,
            "summary": format!("controls focused: {}", summary.join(", ")),
            "unreached": unreached,
        })
    }
}

fn stop_key(observation: &Observation) -> String {
    let focused = observation
        .controls
        .iter()
        .find(|control| control.focused)
        .map(|control| control.id.as_str())
        .unwrap_or("");
    let listed: Vec<&str> = observation
        .controls
        .iter()
        .map(|control| control.id.as_str())
        .collect();
    format!("{}|{}|{}", observation.surface, focused, listed.join(","))
}

fn press(key: &str) -> Step {
    Step::Press(key.to_string())
}

/// The findings a branch's take raises in the menu walk.
fn branch_findings(result: &TakeResult) -> Option<Finding> {
    let finding = result.outcome.as_ref().err()?;
    matches!(
        finding.rule.as_str(),
        rules::PANIC | rules::UNSETTLED | rules::FOCUS_HIDDEN
    )
    .then(|| finding.clone())
}

/// Run the menu walk.
pub fn run(pack_root: &Path, pack: &Pack, budget: u32, from: Option<&str>, keep: bool) -> Report {
    let mut used = 0u32;
    let mut budget_spent = false;
    let mut collected = Collected::default();

    let mut start = match from {
        Some(id) => match load_branch(pack_root, id) {
            Ok(branch) => branch,
            Err(finding) => {
                return Report {
                    findings: vec![crate::keiko::output::finding_json(&finding)],
                    coverage: json!({ "summary": "no coverage: --from did not load" }),
                    used,
                    limit: budget,
                    budget_spent,
                }
            }
        },
        None => Branch::default(),
    };
    let has_speed = start.headers.iter().any(
        |line| matches!(&line.header, Header::Setting { key, .. } if key == "play.text_speed"),
    );
    if !has_speed {
        start.headers.push(HeaderLine {
            line: 1,
            header: Header::Setting {
                key: "play.text_speed".to_string(),
                value: "0".to_string(),
            },
        });
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut coverage = MenuCoverage::default();
    let mut queue: VecDeque<(Branch, TakeResult)> = VecDeque::new();
    used += start.steps.len() as u32;
    let start_result = execute(pack_root, pack, &start, keep);
    queue.push_back((start, start_result));

    'walk: while let Some((branch, result)) = queue.pop_front() {
        if let Some(finding) = branch_findings(&result) {
            save_finding(pack_root, pack, &branch, &finding, None, &mut collected);
            continue;
        }
        if result.outcome.is_err() {
            continue;
        }
        let observation = result.observation.clone();
        coverage.observe(&observation);
        if !visited.insert(stop_key(&observation)) {
            continue;
        }
        let surface = observation.surface.as_str();
        if !matches!(surface, "title" | "play" | "error") {
            let mut unwind = branch.clone();
            for _ in 0..UNWIND_PRESSES {
                unwind = unwind.with_step(press("Escape"));
            }
            let cost = unwind.steps.len() as u32;
            if used + cost > budget {
                budget_spent = true;
                break 'walk;
            }
            used += cost;
            let unwound = execute(pack_root, pack, &unwind, keep);
            let reached = unwound
                .records
                .iter()
                .rev()
                .take(UNWIND_PRESSES)
                .any(|record| matches!(record.surface.as_str(), "title" | "play"));
            if unwound.outcome.is_ok() && !reached {
                let finding = Finding::new(
                    "tests/findings/explore.keiko",
                    Location::File,
                    rules::UNWIND,
                    format!(
                        "`Esc` does not back out of the {surface} surface: {UNWIND_PRESSES} presses reach neither the title nor play"
                    ),
                );
                save_finding(pack_root, pack, &branch, &finding, None, &mut collected);
            }
        }
        let moves: &[&str] = match surface {
            "error" => &[],
            "play" => &["Escape"],
            _ => &MOVES,
        };
        for key in moves {
            let child = branch.with_step(press(key));
            let cost = child.steps.len() as u32;
            if used + cost > budget {
                budget_spent = true;
                break 'walk;
            }
            used += cost;
            let child_result = execute(pack_root, pack, &child, keep);
            queue.push_back((child, child_result));
        }
    }

    Report {
        findings: collected.findings,
        coverage: coverage.to_json(),
        used,
        limit: budget,
        budget_spent,
    }
}
