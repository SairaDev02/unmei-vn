//! Exploration: walk every choice option and landing key within a step budget,
//! save each finding as a ready-to-run case, and report coverage (spec §9).

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use unmei_engine::keiko::{Header, HeaderLine, Pick, Step, StepLine, CASE_VERSION};
use unmei_engine::runner::Wait;
use unmei_engine::Pack;

use super::finding::{rules, Finding};
use super::observe::Observation;
use super::take::{self, TakeRequest, TakeResult};

/// The exploration's result.
pub struct Report {
    pub findings: Vec<Value>,
    pub coverage: Value,
    pub used: u32,
    pub limit: u32,
    pub budget_spent: bool,
}

impl Report {
    pub fn to_json(&self) -> Value {
        json!({
            "findings": self.findings,
            "coverage": self.coverage,
            "budget": { "limit": self.limit, "used": self.used },
        })
    }

    pub fn print(&self) {
        for finding in &self.findings {
            println!(
                "{} {}: {}",
                finding.get("rule").and_then(Value::as_str).unwrap_or(""),
                finding
                    .get("case")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                finding.get("message").and_then(Value::as_str).unwrap_or("")
            );
        }
        if let Some(summary) = self.coverage.get("summary").and_then(Value::as_str) {
            println!("{summary}");
        }
        if let Some(unreached) = self.coverage.get("unreached").and_then(Value::as_array) {
            if !unreached.is_empty() {
                let names: Vec<&str> = unreached.iter().filter_map(Value::as_str).collect();
                println!("not reached: {}", names.join(", "));
            }
        }
        println!(
            "budget: {} of {}{}",
            self.used,
            self.limit,
            if self.budget_spent { " (budget spent)" } else { "" }
        );
    }
}

/// One branch: the headers it carries and the steps it applies.
#[derive(Clone, Default)]
struct Branch {
    headers: Vec<HeaderLine>,
    steps: Vec<StepLine>,
}

impl Branch {
    /// The case text a saved finding holds.
    fn case_text(&self, closing: &[String]) -> String {
        let mut text = format!("keiko {CASE_VERSION}\n");
        for line in &self.headers {
            if let Some(rendered) = render_header(&line.header) {
                text.push_str(&rendered);
                text.push('\n');
            }
        }
        for line in &self.steps {
            if let Some(rendered) = render_step(&line.step) {
                text.push_str(&rendered);
                text.push('\n');
            }
        }
        for line in closing {
            text.push_str(line);
            text.push('\n');
        }
        text
    }

    fn digest(&self) -> String {
        let mut text = String::new();
        for line in &self.steps {
            if let Some(rendered) = render_step(&line.step) {
                text.push_str(&rendered);
                text.push('\n');
            }
        }
        super::sha256::hex8(&text)
    }

    fn with_decide(&self, point: &str, action: &str) -> Branch {
        let mut headers: Vec<HeaderLine> = self
            .headers
            .iter()
            .filter(|line| {
                !matches!(&line.header, Header::Decide { point: p, .. } if p == point)
            })
            .cloned()
            .collect();
        headers.push(HeaderLine {
            line: 1,
            header: Header::Decide {
                point: point.to_string(),
                action: action.to_string(),
            },
        });
        Branch {
            headers,
            steps: self.steps.clone(),
        }
    }

    fn with_step(&self, step: Step) -> Branch {
        let mut steps = self.steps.clone();
        steps.push(StepLine {
            line: steps.len() + 1,
            step,
        });
        Branch {
            headers: self.headers.clone(),
            steps,
        }
    }
}

/// Run the exploration walk.
pub fn run(pack_root: &Path, pack: &Pack, budget: u32, from: Option<&str>, keep: bool) -> Report {
    let mut used = 0u32;
    let mut budget_spent = false;
    let mut findings: Vec<Value> = Vec::new();
    let mut saved: HashSet<String> = HashSet::new();

    let start = match from {
        Some(id) => match load_branch(pack_root, id) {
            Ok(branch) => branch,
            Err(finding) => {
                return Report {
                    findings: vec![super::output::finding_json(&finding)],
                    coverage: json!({ "summary": "no coverage: --from did not load" }),
                    used,
                    limit: budget,
                    budget_spent,
                }
            }
        },
        None => Branch {
            headers: Vec::new(),
            steps: vec![StepLine {
                line: 1,
                step: Step::NewGame,
            }],
        },
    };
    // Instant text unless the case chose a speed, so a walk does not spend its
    // budget on reveal frames.
    let has_speed = start.headers.iter().any(|line| {
        matches!(&line.header, Header::Setting { key, .. } if key == "play.text_speed")
    });
    let mut start = start;
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
    let mut coverage = Coverage::new(pack);
    let mut queue: VecDeque<(Branch, TakeResult)> = VecDeque::new();
    used += start.steps.len() as u32;
    let start_result = execute(pack_root, pack, &start, keep);
    queue.push_back((start, start_result));

    while let Some((branch, result)) = queue.pop_front() {
        coverage.observe(&result);
        for finding in branch_findings(&result) {
            save_finding(pack_root, &branch, &finding, &mut findings, &mut saved);
        }
        if result.outcome.is_err() {
            continue;
        }
        let observation = result.observation.clone();
        let key = visited_key(&observation);
        if !visited.insert(key.clone()) {
            continue;
        }
        let moves = moves_for(pack, &branch, &observation);
        if moves.is_empty() {
            continue;
        }
        let mut children = Vec::new();
        let mut spent = false;
        for child in moves {
            let cost = child.steps.len() as u32;
            if used + cost > budget {
                budget_spent = true;
                spent = true;
                break;
            }
            used += cost;
            let child_result = execute(pack_root, pack, &child, keep);
            children.push((child, child_result));
        }
        // The stuck rule: every move leaves this park where it was, or lands on
        // a park already reached. A pack that loops forever is caught here
        // instead of being expanded again.
        let stuck = !children.is_empty()
            && children
                .iter()
                .all(|(_, child)| visited.contains(&visited_key(&child.observation)));
        if stuck {
            let finding = Finding::new(
                "tests/findings/explore.keiko",
                format!("line {}", observation.line.unwrap_or_default()),
                rules::STUCK,
                format!(
                    "scene `{}` line {} does not move: every move leaves it in place",
                    observation.scene.as_deref().unwrap_or("?"),
                    observation
                        .line
                        .map(|line| line.to_string())
                        .unwrap_or_else(|| "?".to_string())
                ),
            );
            let repeat = branch.with_step(Step::Advance(1));
            save_finding(pack_root, &repeat, &finding, &mut findings, &mut saved);
            continue;
        }
        for child in children {
            queue.push_back(child);
        }
        if spent {
            break;
        }
    }

    Report {
        findings,
        coverage: coverage.to_json(pack),
        used,
        limit: budget,
        budget_spent,
    }
}

fn execute(pack_root: &Path, pack: &Pack, branch: &Branch, keep: bool) -> TakeResult {
    let case = unmei_engine::keiko::Case {
        headers: branch.headers.clone(),
        steps: branch.steps.clone(),
    };
    take::run(TakeRequest {
        case: &case,
        case_file: "tests/findings/explore.keiko".to_string(),
        original_pack: pack_root,
        pack,
        take_index: 1,
        keep,
        live: None,
    })
}

/// The child branches one park offers: at a say park `advance`; at a choice
/// each option in index order; at a decision each landing key in the pack's
/// declared order, then the fallback.
fn moves_for(pack: &Pack, branch: &Branch, observation: &Observation) -> Vec<Branch> {
    let mut moves = Vec::new();
    if let Some(decision) = &observation.decision {
        let Some((npc_id, question)) = decision.point.split_once('.') else {
            return moves;
        };
        let Some(config) = pack
            .npcs
            .get(npc_id)
            .and_then(|npc| npc.decisions.get(question))
        else {
            return moves;
        };
        let mut keys = config.landing_keys();
        keys.sort();
        for key in keys {
            moves.push(branch.with_decide(&decision.point, &key));
        }
        moves.push(branch.with_decide(&decision.point, "fallback"));
        return moves;
    }
    if observation.text.is_some() {
        moves.push(branch.with_step(Step::Advance(1)));
        return moves;
    }
    if !observation.choices.is_empty() {
        for index in 1..=observation.choices.len() {
            moves.push(branch.with_step(Step::Pick(Pick::Index(index as u32))));
        }
    }
    moves
}

fn branch_findings(result: &TakeResult) -> Vec<Finding> {
    let mut findings = Vec::new();
    if let Err(finding) = &result.outcome {
        if matches!(finding.rule.as_str(), rules::PANIC | rules::UNSETTLED) {
            findings.push(finding.clone());
        }
    }
    for record in &result.records {
        if let Some(notice) = &record.notice {
            findings.push(Finding::new(
                "tests/findings/explore.keiko",
                format!("line {}", record.line.unwrap_or_default()),
                rules::NOTICE,
                format!("a Notice was shown: \"{notice}\""),
            ));
            break;
        }
        if record.held {
            findings.push(Finding::new(
                "tests/findings/explore.keiko",
                format!("line {}", record.line.unwrap_or_default()),
                rules::DEFENSIVE,
                "a defensive failure held the runner".to_string(),
            ));
            break;
        }
    }
    for line in &result.log {
        if line.level == "info" || !PACK_ROWS.contains(&line.row) {
            continue;
        }
        findings.push(Finding::new(
            "tests/findings/explore.keiko",
            String::new(),
            &rules::log_row(line.row),
            line.text.clone(),
        ));
    }
    findings.dedup_by(|left, right| left.rule == right.rule);
    findings
}

/// The failure rows a pack can cause: a pack warning, a missing or undecodable
/// asset, a missing decide point, a missing decision label.
const PACK_ROWS: &[u32] = &[2, 3, 4, 5, 6, 11, 12];

/// The closing assertion a saved finding carries.
fn closing_assertion(finding: &Finding) -> Vec<String> {
    if finding.rule == rules::NOTICE {
        return vec!["expect notice none".to_string()];
    }
    if finding.rule == rules::DEFENSIVE {
        return vec!["expect surface play".to_string()];
    }
    if let Some(row) = finding.rule.strip_prefix("keiko.log.row-") {
        let level = match row {
            "3" | "4" | "5" | "6" => "warn",
            _ => "error",
        };
        return vec![format!("expect no-log {level} row {row}")];
    }
    Vec::new()
}

fn save_finding(
    pack_root: &Path,
    branch: &Branch,
    finding: &Finding,
    findings: &mut Vec<Value>,
    saved: &mut HashSet<String>,
) {
    let path = saved_finding_path(pack_root, branch, finding);
    let relative = path
        .strip_prefix(pack_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
        .replace('\\', "/");
    if saved.insert(relative.clone()) && !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, branch.case_text(&closing_assertion(finding)));
    }
    let mut json = super::output::finding_json(finding);
    if let Some(object) = json.as_object_mut() {
        object.insert("case".to_string(), Value::String(relative));
    }
    findings.push(json);
}

/// `tests/findings/<rule>-<hash>.keiko`, named by the step list's content hash.
fn saved_finding_path(pack_root: &Path, branch: &Branch, finding: &Finding) -> PathBuf {
    pack_root
        .join("tests")
        .join("findings")
        .join(format!("{}-{}.keiko", finding.rule, branch.digest()))
}

fn visited_key(observation: &Observation) -> String {
    let mut key = format!(
        "{}|{}|",
        observation.scene.as_deref().unwrap_or(""),
        observation.line.unwrap_or_default()
    );
    for (name, value) in &observation.state {
        key.push_str(&format!("{name}={value:?};"));
    }
    key
}

fn load_branch(pack_root: &Path, id: &str) -> Result<Branch, Finding> {
    let path = pack_root.join(format!("tests/{id}.keiko"));
    let text = std::fs::read_to_string(&path).map_err(|error| {
        Finding::new(
            format!("tests/{id}.keiko"),
            String::new(),
            rules::CASE_REFERENCE,
            format!("cannot read the case: {error}"),
        )
    })?;
    let case = unmei_engine::keiko::parse_case(&format!("tests/{id}.keiko"), &text)?;
    Ok(Branch {
        headers: case.headers,
        steps: case.steps,
    })
}

fn render_header(header: &Header) -> Option<String> {
    Some(match header {
        Header::Decide { point, action } => format!("decide {point} {action}"),
        Header::Setting { key, value } => format!("setting {key} {value}"),
        Header::Fixture { kind, path } => format!("fixture {} {path}", kind.name()),
        Header::Fault { .. } => return None,
        Header::Dev => "dev".to_string(),
        Header::Start { scene } => format!("start {scene}"),
        Header::State { key, value } => format!("state {key} = {value}"),
    })
}

fn render_step(step: &Step) -> Option<String> {
    Some(match step {
        Step::NewGame => "new-game".to_string(),
        Step::Continue => "continue".to_string(),
        Step::Advance(1) => "advance".to_string(),
        Step::Advance(count) => format!("advance {count}"),
        Step::Pick(Pick::Index(index)) => format!("pick {index}"),
        Step::Pick(Pick::Text(text)) => format!("pick \"{text}\""),
        Step::Menu => "menu".to_string(),
        Step::Activate(id) => format!("activate {id}"),
        Step::Save(slot) => format!("save {slot}"),
        Step::Load(unmei_engine::keiko::LoadTarget::Slot(slot)) => format!("load {slot}"),
        Step::Load(unmei_engine::keiko::LoadTarget::Autosave) => "load autosave".to_string(),
        Step::Auto => "auto".to_string(),
        Step::Skip => "skip".to_string(),
        Step::Press(key) => format!("press {key}"),
        Step::Hold(key) => format!("hold {key}"),
        Step::Release(key) => format!("release {key}"),
        Step::Type(text) => format!("type \"{text}\""),
        Step::Paste(text) => format!("paste \"{text}\""),
        Step::Wait(duration) => format!("wait {}ms", duration.as_millis()),
        Step::ReleaseDecision(point) => format!("release-decision {point}"),
        Step::ReleaseKeyCheck => "release-key-check".to_string(),
        Step::Unblock(target) => format!("unblock {}", target.name()),
        Step::Frame(None) => "frame".to_string(),
        Step::Frame(Some(name)) => format!("frame {name}"),
        Step::Click(_) | Step::Expect(_) => return None,
    })
}

/// Per-scene coverage: reached say-lines and choices out of the pack's totals.
struct Coverage {
    scenes: BTreeSet<String>,
    say: BTreeSet<(String, u32)>,
    choices: BTreeSet<(String, u32)>,
}

impl Coverage {
    fn new(_pack: &Pack) -> Self {
        Coverage {
            scenes: BTreeSet::new(),
            say: BTreeSet::new(),
            choices: BTreeSet::new(),
        }
    }

    fn observe(&mut self, result: &TakeResult) {
        for record in &result.records {
            let Some(scene) = record.scene.clone() else {
                continue;
            };
            self.scenes.insert(scene.clone());
            if let Some(line) = record.line {
                if record.text.is_some() {
                    self.say.insert((scene.clone(), line));
                }
                if !record.choices.is_empty() {
                    self.choices.insert((scene, line));
                }
            }
        }
    }

    fn to_json(&self, pack: &Pack) -> Value {
        let mut scenes = serde_json::Map::new();
        let mut unentered = Vec::new();
        for scene in pack.scenes.values() {
            let entered = self.scenes.contains(&scene.id);
            if !entered {
                unentered.push(scene.id.clone());
            }
            let total_say = scene
                .lines
                .iter()
                .filter(|line| matches!(line, unmei_engine::Line::Say { .. }))
                .count();
            let total_choices = scene
                .lines
                .iter()
                .filter(|line| matches!(line, unmei_engine::Line::Choice { .. }))
                .count();
            let reached_say = self
                .say
                .iter()
                .filter(|(id, _)| *id == scene.id)
                .count();
            let reached_choices = self
                .choices
                .iter()
                .filter(|(id, _)| *id == scene.id)
                .count();
            scenes.insert(
                scene.id.clone(),
                json!({
                    "entered": entered,
                    "say_lines": { "reached": reached_say, "total": total_say },
                    "choices": { "reached": reached_choices, "total": total_choices },
                }),
            );
        }
        unentered.sort();
        json!({
            "summary": format!(
                "reached {} of {} scenes",
                pack.scenes.len() - unentered.len(),
                pack.scenes.len()
            ),
            "scenes": scenes,
            "unreached": unentered,
        })
    }
}

/// One park's identity for the visited rule: scene, line and state map.
pub fn park_key(observation: &Observation) -> String {
    visited_key(observation)
}

/// Whether a wait is a choice, for callers that build moves.
pub fn is_choice(wait: Option<&Wait>) -> bool {
    matches!(wait, Some(Wait::Choice { .. }))
}

/// The map key a decision point uses, exposed for tests.
pub fn decision_point(observation: &Observation) -> Option<&str> {
    observation.decision.as_ref().map(|decision| decision.point.as_str())
}
