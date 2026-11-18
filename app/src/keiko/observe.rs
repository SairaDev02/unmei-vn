//! The observation: what the player sees after a step, built from app resources
//! and the presentation's own text (spec §5).
//!
//! Nothing here reads entities, components or system order as *facts*: the
//! fields are the player's surface, lines, choices, Notice, staging, comfort
//! modes, decision state, error screen, state map, slots and the take's own log
//! lines. The text and JSON forms are the same fields.

use bevy::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use unmei_engine::runner::Wait;
use unmei_engine::save::{list_slots, SlotStatus};
use unmei_engine::StateValue;

use crate::failure::StartupFailure;
use crate::menu::MenuState;
use crate::presentation::play::DialogueProse;
use crate::HeldDecision;
use crate::PendingDecision;
use crate::Runtime;

/// One choice option on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub index: usize,
    pub text: String,
}

/// One menu control and how it currently stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub focused: bool,
}

/// The staged presentation: background, sprites and music.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Staging {
    pub background: Option<String>,
    pub sprites: BTreeMap<String, String>,
    pub music: Option<String>,
}

/// A waiting decision point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionState {
    pub point: String,
    pub status: DecisionStatus,
}

/// Whether a decision is waiting or held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionStatus {
    Pending,
    Held,
}

/// One save slot's observable status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotState {
    pub slot: String,
    pub status: String,
    pub scene: Option<String>,
}

/// One session-log line written since the previous observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub level: String,
    pub row: u32,
    pub text: String,
}

/// Everything a case can assert about the player (spec §5.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub surface: String,
    pub monitor: bool,
    pub clock_ms: u64,
    pub scene: Option<String>,
    pub label: Option<String>,
    pub line: Option<u32>,
    pub speaker: Option<String>,
    pub text: Option<String>,
    pub revealed: bool,
    pub choices: Vec<Choice>,
    pub notice: Option<String>,
    pub controls: Vec<Control>,
    pub staging: Staging,
    pub auto: bool,
    pub skip: String,
    pub decision: Option<DecisionState>,
    pub error: Vec<String>,
    pub state: BTreeMap<String, StateValue>,
    pub slots: Vec<SlotState>,
    pub live: bool,
    pub log: Vec<LogLine>,
    /// The defensive hold is engaged. Internal: an exploration finding needs it,
    /// the case format's JSON does not print it.
    pub held: bool,
}

impl Observation {
    /// Equality over everything except the log lines, which are a since-last
    /// slice rather than part of the world's state. The settle rule compares
    /// this, so two frames that differ only by log output are quiet.
    pub fn same_world(&self, other: &Observation) -> bool {
        let mut left = self.clone();
        let mut right = other.clone();
        left.log.clear();
        right.log.clear();
        left == right
    }

    /// The §5.1 JSON document, one object per observation.
    pub fn to_json(&self) -> Value {
        let state: serde_json::Map<String, Value> = self
            .state
            .iter()
            .map(|(key, value)| (key.clone(), state_value_json(value)))
            .collect();
        let sprites: serde_json::Map<String, Value> = self
            .staging
            .sprites
            .iter()
            .map(|(npc, pose)| (npc.clone(), json!(pose)))
            .collect();
        json!({
            "surface": self.surface,
            "monitor": self.monitor,
            "clock": self.clock_ms,
            "scene": self.scene,
            "label": self.label,
            "line": self.line,
            "speaker": self.speaker,
            "text": self.text,
            "revealed": self.revealed,
            "choices": self.choices.iter().map(|choice| json!({
                "index": choice.index,
                "text": choice.text,
            })).collect::<Vec<_>>(),
            "notice": self.notice,
            "controls": self.controls.iter().map(|control| json!({
                "id": control.id,
                "label": control.label,
                "enabled": control.enabled,
                "focused": control.focused,
            })).collect::<Vec<_>>(),
            "staging": {
                "background": self.staging.background,
                "sprites": sprites,
                "music": self.staging.music,
            },
            "auto": self.auto,
            "skip": self.skip,
            "decision": self.decision.as_ref().map(|decision| json!({
                "question": decision.point,
                "status": match decision.status {
                    DecisionStatus::Pending => "pending",
                    DecisionStatus::Held => "held",
                },
            })),
            "error": self.error,
            "state": state,
            "slots": self.slots.iter().map(|slot| json!({
                "slot": slot.slot,
                "status": slot.status,
                "scene": slot.scene,
            })).collect::<Vec<_>>(),
            "log": self.log.iter().map(|line| json!({
                "level": line.level,
                "row": line.row,
                "text": line.text,
            })).collect::<Vec<_>>(),
            "live": self.live,
        })
    }

    /// The surface's own name, for a failing assertion's message.
    pub fn surface(&self) -> &str {
        &self.surface
    }
}

fn state_value_json(value: &StateValue) -> Value {
    match value {
        StateValue::Str(text) => json!(text),
        StateValue::Num(number) => json!(number),
        StateValue::Bool(flag) => json!(flag),
    }
}

/// A state value for a failure message.
pub fn state_text(value: &StateValue) -> String {
    state_value_text(value)
}

pub(super) fn state_value_text(value: &StateValue) -> String {
    match value {
        StateValue::Str(text) => format!("\"{text}\""),
        StateValue::Num(number) => StateValue::Num(*number).display(),
        StateValue::Bool(flag) => flag.to_string(),
    }
}

/// The observation of a run that never booted.
pub fn empty() -> Observation {
    Observation {
        surface: "error".to_string(),
        monitor: false,
        clock_ms: 0,
        scene: None,
        label: None,
        line: None,
        speaker: None,
        text: None,
        revealed: true,
        choices: Vec::new(),
        notice: None,
        controls: Vec::new(),
        staging: Staging::default(),
        auto: false,
        skip: "off".to_string(),
        decision: None,
        error: Vec::new(),
        state: BTreeMap::new(),
        slots: Vec::new(),
        live: false,
        log: Vec::new(),
        held: false,
    }
}

/// The take's recorded lines since `cursor`, and the new cursor.
pub fn log_lines_from(cursor: usize) -> (Vec<LogLine>, usize) {
    let (lines, cursor) = crate::failure::tap::since(cursor);
    (
        lines
            .into_iter()
            .map(|line| LogLine {
                level: line.level,
                row: line.row,
                text: line.text,
            })
            .collect(),
        cursor,
    )
}

/// Build the observation from the live app.
pub fn observe(app: &mut App, log: Vec<LogLine>, live: bool) -> Observation {
    let world = app.world_mut();
    let startup_failure = world.get_resource::<StartupFailure>().is_some();
    let menu_snapshot = world.get_resource::<MenuState>().cloned();

    let surface = if startup_failure {
        "error".to_string()
    } else if let Some(menu) = &menu_snapshot {
        if menu.confirm.is_some() {
            "confirm".to_string()
        } else if !menu.open {
            "play".to_string()
        } else if menu.gallery_still {
            "still".to_string()
        } else {
            match menu.view {
                crate::menu::MenuView::Title => "title",
                crate::menu::MenuView::Slots => "menu",
                crate::menu::MenuView::Settings => "settings",
                crate::menu::MenuView::Gallery => "gallery",
                crate::menu::MenuView::Timeline => "timeline",
            }
            .to_string()
        }
    } else {
        "error".to_string()
    };

    let monitor = world
        .get_resource::<crate::drawer::MonitorPanel>()
        .is_some_and(|panel| panel.is_open());

    let clock_ms = world
        .get_resource::<Time>()
        .map(|time| (time.elapsed_secs_f64() * 1000.0).round() as u64)
        .unwrap_or(0);

    let auto = world
        .get_resource::<crate::settings::SettingsState>()
        .is_some_and(|settings| settings.auto());
    let skip = world
        .get_resource::<crate::core::SkipState>()
        .map(|skip| {
            if skip.is_seen() {
                "seen"
            } else if skip.is_force_active() || skip.is_force_armed() {
                "force"
            } else {
                "off"
            }
        })
        .unwrap_or("off")
        .to_string();

    // The parked wait: the authored say-line, the choice list, the decision
    // point. This is what the presentation draws, not a copy of it.
    let mut scene = None;
    let mut label = None;
    let mut line = None;
    let mut speaker = None;
    let mut text = None;
    let mut revealed = true;
    let mut choices = Vec::new();
    let mut decision = None;

    if let Some(runtime) = world.get_resource::<Runtime>() {
        scene = Some(runtime.runner.scene_id().to_string());
        if let Some(parked) = park_of(&runtime.runner) {
            line = Some(parked.0);
            label = parked.1;
        }
        let wait = runtime.runner.wait().cloned();
        match &wait {
            Some(Wait::Say {
                speaker: parked_speaker,
                text: parked_text,
            }) => {
                speaker = Some(parked_speaker.clone());
                text = Some(parked_text.clone());
                let drawn = drawn_prose(world);
                revealed = drawn.as_deref() == Some(parked_text.as_str());
            }
            Some(Wait::Choice { options }) => {
                choices = options
                    .iter()
                    .enumerate()
                    .map(|(index, (text, _))| Choice {
                        index: index + 1,
                        text: text.clone(),
                    })
                    .collect();
            }
            Some(Wait::Decision {
                npc_id, question, ..
            }) => {
                // A decision waiting behind the open overlay is *held*: its
                // answer cannot be applied until play resumes, whether the
                // answer is already buffered or still in flight (spec §3.5).
                let buffered = world
                    .get_resource::<HeldDecision>()
                    .is_some_and(|held| held.0.is_some());
                let paused = menu_snapshot.as_ref().is_some_and(|menu| menu.open);
                let held = buffered || paused;
                decision = Some(DecisionState {
                    point: format!("{npc_id}.{question}"),
                    status: if held {
                        DecisionStatus::Held
                    } else {
                        DecisionStatus::Pending
                    },
                });
            }
            None => {}
        }
    }
    // A decision answer in flight is a pending decision too, even before the
    // runner reports the park.
    if decision.is_none() {
        let pending = world
            .get_resource::<PendingDecision>()
            .is_some_and(|pending| pending.0.is_some());
        if pending {
            decision = Some(DecisionState {
                point: String::new(),
                status: DecisionStatus::Pending,
            });
        }
    }

    let notice = world
        .get_resource::<crate::failure::Notice>()
        .and_then(|notice| notice.text.clone());

    let staging = world
        .get_resource::<Runtime>()
        .map(|runtime| Staging {
            background: runtime.staging.background.clone(),
            sprites: runtime.staging.sprites.clone(),
            music: runtime.staging.bgm.clone(),
        })
        .unwrap_or_default();

    let state = world
        .get_resource::<Runtime>()
        .map(|runtime| {
            runtime
                .runner
                .state()
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let slots = world
        .get_resource::<Runtime>()
        .map(|runtime| {
            list_slots(runtime.runner.pack())
                .iter()
                .map(|info| SlotState {
                    slot: match info.slot {
                        unmei_engine::Slot::Manual(n) => n.to_string(),
                        unmei_engine::Slot::Autosave => "autosave".to_string(),
                    },
                    status: match &info.status {
                        SlotStatus::Empty => "empty".to_string(),
                        SlotStatus::Ok { .. } => "ok".to_string(),
                        SlotStatus::Unreadable { .. } => "unreadable".to_string(),
                    },
                    scene: match &info.status {
                        SlotStatus::Ok { scene_id, .. } => Some(scene_id.clone()),
                        _ => None,
                    },
                })
                .collect()
        })
        .unwrap_or_default();

    let error = world
        .get_resource::<StartupFailure>()
        .map(|failure| failure.screen_lines())
        .unwrap_or_default();

    let held = world
        .get_resource::<crate::DefensiveHold>()
        .is_some_and(|hold| hold.0);

    Observation {
        surface,
        monitor,
        clock_ms,
        scene,
        label,
        line,
        speaker,
        text,
        revealed,
        choices,
        notice,
        controls: controls(world),
        staging,
        auto,
        skip,
        decision,
        error,
        state,
        slots,
        live,
        log,
        held,
    }
}

/// The revealed prose the player sees, when a dialogue node exists.
fn drawn_prose(world: &mut World) -> Option<String> {
    let mut query = world.query_filtered::<&bevy::text::TextSpan, With<DialogueProse>>();
    let mut spans = query.iter(world);
    spans.next().map(|span| span.0.clone())
}

/// The parked position's 1-based source line and nearest preceding label.
fn park_of(runner: &unmei_engine::ScriptRunner) -> Option<(u32, Option<String>)> {
    let scene = runner.pack().scenes.get(runner.scene_id())?;
    let index = runner.pc().saturating_sub(1);
    let line = scene.source_line(index);
    if line == 0 {
        return None;
    }
    let label = scene
        .labels
        .iter()
        .filter(|(_, at)| **at <= index)
        .max_by_key(|(_, at)| **at)
        .map(|(name, _)| name.clone());
    Some((line as u32, label))
}

/// The menu controls on the current surface, in walk order.
fn controls(world: &mut World) -> Vec<Control> {
    crate::keiko::controls::observe_controls(world)
}
