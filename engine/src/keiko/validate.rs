//! Static validation: everything a case names must exist in the pack, the
//! settings schema, the control table or the key table before the take boots
//! (spec §2.2, §10.2).
//!
//! The engine owns the checks it can answer Bevy-free; the app hands it the two
//! tables it owns — the settings keys and the key names — as data.

use std::collections::HashMap;
use std::path::Path;

use super::controls::is_control_id;
use super::model::{Case, Expect, Fault, Header, LoadTarget, Pick, Step, WriteTarget};
use crate::content::StateValue;
use crate::finding::{rules, Finding};
use crate::{Pack, Slot};

/// The pack facts and app-owned tables a case is validated against.
pub struct Facts<'a> {
    pub pack: &'a Pack,
    /// The settings file's own keys, from the app's settings schema.
    pub settings_keys: &'a [&'a str],
    /// Every accepted `KeyCode` name, from the app's key table.
    pub key_names: &'a [&'a str],
    /// The pack's `tests/` folder, for fixture and fault paths.
    pub tests_dir: Option<&'a Path>,
}

/// Validate every reference in a parsed case. Returns every finding rather than
/// the first, so one run reports the whole list.
pub fn validate(case: &Case, file: &str, facts: &Facts) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut shared: HashMap<String, (String, usize)> = HashMap::new();
    for line in &case.headers {
        let at = |rule: &str, message: String| Finding::at_line(file, line.line, rule, message);
        match &line.header {
            Header::Decide { point, action } => {
                if let Some(message) = decide_reference(point, action, facts) {
                    findings.push(at(rules::CASE_REFERENCE, message));
                }
                shared_question(&mut shared, point, line.line, file, &mut findings);
            }
            Header::Setting { key, .. } => {
                if !facts.settings_keys.contains(&key.as_str()) {
                    findings.push(at(
                        rules::CASE_REFERENCE,
                        format!("unknown settings key `{key}`"),
                    ));
                }
            }
            Header::Fixture { path, .. } => {
                if !fixture_exists(facts, path) {
                    findings.push(at(
                        rules::CASE_REFERENCE,
                        format!("fixture `{path}` does not exist under `tests/`"),
                    ));
                }
            }
            Header::Fault { fault } => {
                if let Some(message) = fault_reference(fault, facts) {
                    findings.push(at(rules::CASE_REFERENCE, message));
                }
                // Landings and decision faults are both keyed by the question
                // id alone, so rule 38 covers `decide` and the three decision
                // faults alike.
                if let Some(point) = decision_point(fault) {
                    shared_question(&mut shared, point, line.line, file, &mut findings);
                }
            }
            Header::Dev => {}
            Header::Start { scene } => {
                if !facts.pack.scenes.contains_key(scene) {
                    findings.push(at(
                        rules::CASE_REFERENCE,
                        format!("unknown scene `{scene}`"),
                    ));
                }
            }
            Header::State { key, value } => {
                if let Some(message) = state_reference(key, value, facts) {
                    findings.push(at(rules::CASE_REFERENCE, message));
                }
            }
        }
    }
    if case.start_scene().is_none() {
        for line in &case.headers {
            if matches!(line.header, Header::State { .. }) {
                findings.push(Finding::at_line(
                    file,
                    line.line,
                    rules::CASE_REFERENCE,
                    "a `state` header needs a `start` header",
                ));
            }
        }
    }

    let declared = Declared::of(case);
    for line in &case.steps {
        let at = |rule: &str, message: String| Finding::at_line(file, line.line, rule, message);
        let at_str = |rule: &str, message: &str| Finding::at_line(file, line.line, rule, message);
        match &line.step {
            Step::Activate(id) | Step::Click(super::model::Click::Control(id)) => {
                if !is_control_id(id) {
                    findings.push(at(rules::CASE_REFERENCE, format!("unknown control `{id}`")));
                }
            }
            Step::Press(key) | Step::Hold(key) | Step::Release(key) => {
                if !facts.key_names.contains(&key.as_str()) {
                    findings.push(at(rules::CASE_SYNTAX, format!("unknown key `{key}`")));
                }
            }
            Step::ReleaseDecision(point) => {
                if !declared.decision_holds.iter().any(|held| held == point) {
                    findings.push(at(
                        rules::CASE_REFERENCE,
                        format!("no `fault decision-hold {point}` is declared"),
                    ));
                }
            }
            Step::ReleaseKeyCheck => {
                if !declared.key_check_hold {
                    findings.push(at_str(
                        rules::CASE_REFERENCE,
                        "no `fault key-check hold` is declared",
                    ));
                }
            }
            Step::Unblock(target) => {
                if !declared.blocks.contains(target) {
                    findings.push(at(
                        rules::CASE_REFERENCE,
                        format!("no `fault write-blocked {}` is declared", target.name()),
                    ));
                }
            }
            Step::Save(_) | Step::Load(_) | Step::Pick(Pick::Index(_)) => {}
            Step::Advance(_) | Step::Pick(Pick::Text(_)) | Step::Type(_) | Step::Paste(_) => {}
            Step::NewGame
            | Step::Continue
            | Step::Menu
            | Step::Auto
            | Step::Skip
            | Step::Wait(_)
            | Step::Frame(_)
            | Step::Click(super::model::Click::Position(_, _)) => {}
            Step::Expect(expect) => {
                if let Some(message) = expect_reference(expect, facts) {
                    findings.push(at(rules::CASE_REFERENCE, message));
                }
            }
        }
    }
    findings
}

/// What the case declares, for the fault-step cross-checks.
pub struct Declared {
    decision_holds: Vec<String>,
    key_check_hold: bool,
    blocks: Vec<WriteTarget>,
}

impl Declared {
    fn of(case: &Case) -> Self {
        let mut declared = Declared {
            decision_holds: Vec::new(),
            key_check_hold: false,
            blocks: Vec::new(),
        };
        for line in &case.headers {
            match &line.header {
                Header::Fault {
                    fault: Fault::DecisionHold { point },
                } => declared.decision_holds.push(point.clone()),
                Header::Fault {
                    fault:
                        Fault::KeyCheck {
                            verdict: super::model::KeyCheckFault::Hold,
                        },
                } => declared.key_check_hold = true,
                Header::Fault {
                    fault: Fault::WriteBlocked { target },
                } => declared.blocks.push(*target),
                _ => {}
            }
        }
        declared
    }
}

/// The decision point a fault names, when it is a decision fault.
fn decision_point(fault: &Fault) -> Option<&str> {
    match fault {
        Fault::DecisionError { point, .. }
        | Fault::DecisionSlow { point, .. }
        | Fault::DecisionHold { point } => Some(point),
        _ => None,
    }
}

/// Rule 38: one question id belongs to one NPC. The resolver is handed only the
/// bare question, so a case that sets the same question for two NPCs would
/// silently keep the last declaration.
fn shared_question(
    shared: &mut HashMap<String, (String, usize)>,
    point: &str,
    at: usize,
    file: &str,
    findings: &mut Vec<Finding>,
) {
    let Some((npc, question)) = point.split_once('.') else {
        return;
    };
    match shared.get(question) {
        Some((first_npc, first_line)) if first_npc != npc => findings.push(Finding::at_line(
            file,
            at,
            rules::CASE_REFERENCE,
            format!(
                "question `{question}` is set for {npc} here and for {first_npc} on line {first_line}; a case cannot tell them apart"
            ),
        )),
        Some(_) => {}
        None => {
            shared.insert(question.to_string(), (npc.to_string(), at));
        }
    }
}

fn decide_reference(point: &str, action: &str, facts: &Facts) -> Option<String> {
    let (npc_id, question) = match split_point(point) {
        Ok(pair) => pair,
        Err(message) => return Some(message),
    };
    let Some(npc) = facts.pack.npcs.get(npc_id) else {
        return Some(format!("unknown NPC `{npc_id}`"));
    };
    let Some(config) = npc.decisions.get(question) else {
        return Some(format!("unknown question `{question}` for NPC `{npc_id}`"));
    };
    if action == "fallback" {
        return None;
    }
    let mut keys = config.landing_keys();
    keys.sort();
    if keys.iter().any(|key| key == action) {
        None
    } else {
        Some(format!(
            "`{action}` is not a landing of `{point}` (expected one of: {} or `fallback`)",
            keys.join(", ")
        ))
    }
}

/// Split `<npc>.<question>`, or describe why it cannot be split.
fn split_point(point: &str) -> Result<(&str, &str), String> {
    match point.split_once('.') {
        Some((npc, question)) if !npc.is_empty() && !question.is_empty() => Ok((npc, question)),
        _ => Err(format!("`{point}` is not `<npc>.<question>`")),
    }
}

fn fixture_exists(facts: &Facts, path: &str) -> bool {
    facts
        .tests_dir
        .map(|dir| dir.join(path).is_file())
        .unwrap_or(false)
}

fn fault_reference(fault: &Fault, facts: &Facts) -> Option<String> {
    match fault {
        Fault::DecisionError { point, .. }
        | Fault::DecisionSlow { point, .. }
        | Fault::DecisionHold { point } => {
            let (npc_id, question) = match split_point(point) {
                Ok(pair) => pair,
                Err(message) => return Some(message),
            };
            let Some(npc) = facts.pack.npcs.get(npc_id) else {
                return Some(format!("unknown NPC `{npc_id}`"));
            };
            if npc.decisions.contains_key(question) {
                None
            } else {
                Some(format!("`{point}` is not a decision point of this pack"))
            }
        }
        Fault::PackFile { fixture, .. } => {
            if fixture_exists(facts, fixture) {
                None
            } else {
                Some(format!("fixture `{fixture}` does not exist under `tests/`"))
            }
        }
        Fault::WriteBlocked { .. }
        | Fault::AssetMissing { .. }
        | Fault::AssetCorrupt { .. }
        | Fault::PackFileMissing { .. }
        | Fault::CredentialUnreadable
        | Fault::KeyCheck { .. } => None,
    }
}

fn state_reference(key: &str, value: &str, facts: &Facts) -> Option<String> {
    let Some(declared) = facts.pack.state.get(key) else {
        return Some(format!("unknown state key `{key}`"));
    };
    match parse_state_value(value) {
        None => Some(format!("`{value}` is not a state value")),
        Some(parsed) if kind_of(&parsed) != kind_of(declared) => Some(format!(
            "state key `{key}` is declared as a {}, but `{value}` is a {}",
            kind_of(declared),
            kind_of(&parsed)
        )),
        Some(_) => None,
    }
}

/// Parse a `state.ron`-form scalar: a quoted string, a number or a boolean.
pub fn parse_state_value(text: &str) -> Option<StateValue> {
    ron::from_str::<StateValue>(text).ok()
}

fn kind_of(value: &StateValue) -> &'static str {
    match value {
        StateValue::Str(_) => "string",
        StateValue::Num(_) => "number",
        StateValue::Bool(_) => "boolean",
    }
}

fn expect_reference(expect: &Expect, facts: &Facts) -> Option<String> {
    match expect {
        Expect::Control { id, .. }
        | Expect::ControlLabel { id, .. }
        | Expect::ControlRect { id, .. } => {
            (!is_control_id(id)).then(|| format!("unknown control `{id}`"))
        }
        Expect::Scene(scene) => {
            (!facts.pack.scenes.contains_key(scene)).then(|| format!("unknown scene `{scene}`"))
        }
        Expect::Label(name) => {
            let Some(name) = name else { return None };
            let anywhere = facts
                .pack
                .scenes
                .values()
                .any(|scene| scene.labels.contains_key(name));
            (!anywhere).then(|| format!("unknown label `{name}`"))
        }
        Expect::Sprite { npc, .. } | Expect::SpriteNone { npc } => {
            (!facts.pack.npcs.contains_key(npc)).then(|| format!("unknown NPC `{npc}`"))
        }
        Expect::State { key, value } => state_reference(key, value, facts),
        Expect::Read { scene, line, .. } => {
            let Some(script) = facts.pack.scenes.get(scene) else {
                return Some(format!("unknown scene `{scene}`"));
            };
            // Only a say-line enters the read log (§5), so `expect read` and
            // `expect unread` must name one.
            let say_line = script.lines.iter().enumerate().any(|(index, word)| {
                script.source_line(index) == *line as usize
                    && matches!(word, crate::Line::Say { .. })
            });
            (!say_line).then(|| format!("scene `{scene}` line {line} is not a say-line"))
        }
        Expect::Slot { slot, .. } => match slot {
            LoadTarget::Slot(1..=3) | LoadTarget::Autosave => None,
            LoadTarget::Slot(n) => Some(format!("unknown slot `{n}`")),
        },
        Expect::Surface(_)
        | Expect::Monitor(_)
        | Expect::Speaker(_)
        | Expect::Line { .. }
        | Expect::Revealed(_)
        | Expect::Choices(_)
        | Expect::NoticeText { .. }
        | Expect::NoticeNone
        | Expect::Bg(_)
        | Expect::Music(_)
        | Expect::Auto(_)
        | Expect::Skip(_)
        | Expect::Decision(_)
        | Expect::Log { .. }
        | Expect::NoLog { .. } => None,
    }
}

/// The slot a fixture or fault names, for the app's sandbox.
pub fn slot_of(target: WriteTarget) -> Option<Slot> {
    match target {
        WriteTarget::Slot(slot) => Some(slot),
        _ => None,
    }
}
