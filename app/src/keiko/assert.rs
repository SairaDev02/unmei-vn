//! Evaluating one assertion against the observation (spec §3.5, K12).
//!
//! A failure reports both the expected and the observed value, so the author can
//! fix the pack without rerunning blind.

use bevy::prelude::*;
use unmei_engine::keiko::{
    exact, ControlCondition, DecisionState, Expect, LogLevel, SkipMode, SlotExpect,
};

use super::finding::{rules, Finding};
use super::observe::{LogLine, Observation};
use crate::Runtime;

/// Evaluate one assertion. `all_log` is every line since the take began, which
/// is what `expect log`/`no-log` count against.
pub fn evaluate(
    expect: &Expect,
    observed: &Observation,
    all_log: &[LogLine],
    app: &App,
) -> Result<(), Finding> {
    let fail = |expected: String| {
        Finding::new(
            "case",
            format!("step `expect {}`", expect_name(expect)),
            rules::EXPECT,
            format!("expected {expected}, observed {}", describe(observed)),
        )
    };
    let log_fail = |expected: String, line: Option<&LogLine>| {
        let observed = match line {
            Some(line) => format!("{} row {} \"{}\"", line.level, line.row, line.text),
            None => "no log lines".to_string(),
        };
        Finding::new(
            "case",
            format!("step `expect {}`", expect_name(expect)),
            rules::EXPECT,
            format!("expected {expected}, observed {observed}"),
        )
    };
    match expect {
        Expect::Surface(name) => {
            if &observed.surface == name {
                Ok(())
            } else {
                Err(fail(format!("surface `{name}`")))
            }
        }
        Expect::Monitor(open) => {
            if observed.monitor == *open {
                Ok(())
            } else {
                Err(fail(format!(
                    "monitor {}",
                    if *open { "open" } else { "closed" }
                )))
            }
        }
        Expect::Speaker(expected) => match (expected, &observed.speaker) {
            (None, None) => Ok(()),
            (Some(expected), Some(actual)) if exact(expected, actual) => Ok(()),
            (None, Some(actual)) => Err(fail(format!("speaker none (observed \"{actual}\")"))),
            (Some(expected), None) => Err(fail(format!("speaker \"{expected}\""))),
            (Some(expected), Some(actual)) => Err(fail(format!(
                "speaker \"{expected}\" (observed \"{actual}\")"
            ))),
        },
        Expect::Line { text, contains } => match &observed.text {
            None => Err(fail(format!("line \"{text}\""))),
            Some(actual) if *contains && actual.contains(text.as_str()) => Ok(()),
            Some(actual) if !*contains && exact(actual, text) => Ok(()),
            Some(actual) => Err(fail(format!(
                "line {}\"{text}\" (observed \"{actual}\")",
                if *contains { "containing " } else { "" }
            ))),
        },
        Expect::Revealed(revealed) => {
            if observed.revealed == *revealed {
                Ok(())
            } else {
                Err(fail(if *revealed {
                    "revealed".to_string()
                } else {
                    "revealing".to_string()
                }))
            }
        }
        Expect::Choices(expected) => {
            let actual: Vec<&str> = observed.choices.iter().map(|c| c.text.as_str()).collect();
            let same = expected.len() == actual.len()
                && expected.iter().zip(&actual).all(|(a, b)| exact(a, b));
            if same {
                Ok(())
            } else {
                let expected: Vec<&str> = expected.iter().map(String::as_str).collect();
                Err(fail(format!(
                    "choices [{}] (observed [{}])",
                    quote_list(&expected),
                    quote_list(&actual)
                )))
            }
        }
        Expect::NoticeText { text, contains } => match &observed.notice {
            None => Err(fail(format!("notice \"{text}\""))),
            Some(actual) if *contains && actual.contains(text.as_str()) => Ok(()),
            Some(actual) if !*contains && exact(actual, text) => Ok(()),
            Some(actual) => Err(fail(format!(
                "notice {}\"{text}\" (observed \"{actual}\")",
                if *contains { "containing " } else { "" }
            ))),
        },
        Expect::NoticeNone => match &observed.notice {
            None => Ok(()),
            Some(actual) => Err(fail(format!("no notice (observed \"{actual}\")"))),
        },
        Expect::Control { id, condition } => {
            let control = observed.controls.iter().find(|control| &control.id == id);
            match (condition, control) {
                (ControlCondition::Absent, None) => Ok(()),
                (ControlCondition::Absent, Some(_)) => Err(fail(format!("`{id}` absent"))),
                (_, None) => Err(fail(format!("control `{id}` on screen"))),
                (ControlCondition::Enabled, Some(control)) if control.enabled => Ok(()),
                (ControlCondition::Disabled, Some(control)) if !control.enabled => Ok(()),
                (ControlCondition::Focused, Some(control)) if control.focused => Ok(()),
                (condition, Some(control)) => Err(fail(format!(
                    "`{id}` {} (observed {} on `{}`)",
                    match condition {
                        ControlCondition::Enabled => "enabled",
                        ControlCondition::Disabled => "disabled",
                        ControlCondition::Focused => "focused",
                        ControlCondition::Absent => "absent",
                    },
                    if control.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    control.label
                ))),
            }
        }
        Expect::ControlLabel { id, label } => {
            let control = observed.controls.iter().find(|control| &control.id == id);
            match control {
                Some(control) if control.label == *label => Ok(()),
                Some(control) => Err(fail(format!(
                    "`{id}` label \"{label}\" (observed \"{}\")",
                    control.label
                ))),
                None => Err(fail(format!("control `{id}` on screen"))),
            }
        }
        Expect::Bg(expected) => match (expected, &observed.staging.background) {
            (None, None) => Ok(()),
            (Some(expected), Some(actual)) if exact(expected, actual) => Ok(()),
            (_, actual) => Err(fail(format!(
                "bg {} (observed {})",
                expected.as_deref().unwrap_or("none"),
                actual.as_deref().unwrap_or("none")
            ))),
        },
        Expect::Sprite { npc, pose } => match observed.staging.sprites.get(npc) {
            Some(actual) if exact(actual, pose) => Ok(()),
            Some(actual) => Err(fail(format!("{npc} {pose} (observed {npc} {actual})"))),
            None => Err(fail(format!("sprite {npc} {pose} (no {npc} sprite)"))),
        },
        Expect::SpriteNone { npc } => match observed.staging.sprites.get(npc) {
            None => Ok(()),
            Some(actual) => Err(fail(format!("no {npc} sprite (observed {npc} {actual})"))),
        },
        Expect::Music(expected) => match (expected, &observed.staging.music) {
            (None, None) => Ok(()),
            (Some(expected), Some(actual)) if exact(expected, actual) => Ok(()),
            (_, actual) => Err(fail(format!(
                "music {} (observed {})",
                expected.as_deref().unwrap_or("none"),
                actual.as_deref().unwrap_or("none")
            ))),
        },
        Expect::Auto(on) => {
            if observed.auto == *on {
                Ok(())
            } else {
                Err(fail(format!("auto {}", if *on { "on" } else { "off" })))
            }
        }
        Expect::Skip(mode) => {
            let expected = match mode {
                SkipMode::Seen => "seen",
                SkipMode::Force => "force",
                SkipMode::Off => "off",
            };
            if observed.skip == expected {
                Ok(())
            } else {
                Err(fail(format!("skip {expected}")))
            }
        }
        Expect::Decision(state) => {
            let actual = observed.decision.as_ref().map(|decision| decision.status);
            let matches = match state {
                DecisionState::None => actual.is_none(),
                DecisionState::Pending => {
                    matches!(actual, Some(super::observe::DecisionStatus::Pending))
                }
                DecisionState::Held => matches!(actual, Some(super::observe::DecisionStatus::Held)),
            };
            if matches {
                Ok(())
            } else {
                Err(fail(format!(
                    "decision {}",
                    match state {
                        DecisionState::None => "none",
                        DecisionState::Pending => "pending",
                        DecisionState::Held => "held",
                    }
                )))
            }
        }
        Expect::Scene(scene) => {
            if observed.scene.as_deref() == Some(scene.as_str()) {
                Ok(())
            } else {
                Err(fail(format!(
                    "scene {scene} (observed {})",
                    observed.scene.as_deref().unwrap_or("none")
                )))
            }
        }
        Expect::Label(label) => {
            if observed.label == *label {
                Ok(())
            } else {
                Err(fail(format!(
                    "label {} (observed {})",
                    label.as_deref().unwrap_or("none"),
                    observed.label.as_deref().unwrap_or("none")
                )))
            }
        }
        Expect::State { key, value } => {
            let Some(expected) = unmei_engine::keiko::parse_state_value(value) else {
                return Err(fail(format!("a state value, not `{value}`")));
            };
            match observed.state.get(key) {
                Some(actual) if *actual == expected => Ok(()),
                Some(actual) => Err(fail(format!(
                    "{key} = {value} (observed {key} = {})",
                    super::observe::state_text(actual)
                ))),
                None => Err(fail(format!("{key} = {value} (no such state key)"))),
            }
        }
        Expect::Read {
            scene,
            line,
            present,
        } => {
            let read = app
                .world()
                .get_resource::<crate::read_log::ReadLogState>()
                .map(|log| log.is_read(scene, *line as usize))
                .unwrap_or(false);
            if read == *present {
                Ok(())
            } else {
                Err(fail(format!(
                    "{} {scene}:{line} (observed {})",
                    if *present { "read" } else { "unread" },
                    if read { "read" } else { "unread" }
                )))
            }
        }
        Expect::Slot { slot, status } => {
            let name = slot.label();
            let Some(actual) = observed.slots.iter().find(|entry| entry.slot == name) else {
                return Err(fail(format!("slot {name}")));
            };
            let matches = match status {
                SlotExpect::Empty => actual.status == "empty",
                SlotExpect::Unreadable => actual.status == "unreadable",
                SlotExpect::Scene(scene) => {
                    actual.status == "ok" && actual.scene.as_deref() == Some(scene.as_str())
                }
            };
            if matches {
                Ok(())
            } else {
                Err(fail(format!(
                    "slot {name} {} (observed {} {})",
                    match status {
                        SlotExpect::Empty => "empty".to_string(),
                        SlotExpect::Unreadable => "unreadable".to_string(),
                        SlotExpect::Scene(scene) => format!("scene {scene}"),
                    },
                    actual.status,
                    actual.scene.as_deref().unwrap_or("")
                )))
            }
        }
        Expect::Log { level, row, text } => {
            let level = level_name(*level);
            let found = all_log.iter().any(|line| {
                matches_log_line(line, level, *row) && line.text.contains(text.as_str())
            });
            if found {
                Ok(())
            } else {
                let expected = format!("log {level}{} containing \"{text}\"", row_suffix(*row));
                Err(log_fail(expected, nearest_log_line(all_log, level, *row)))
            }
        }
        Expect::NoLog { level, row } => {
            let level = level_name(*level);
            match all_log
                .iter()
                .find(|line| matches_log_line(line, level, *row))
            {
                Some(line) => Err(log_fail(
                    format!("no {level}{} line", row_suffix(*row)),
                    Some(line),
                )),
                None => Ok(()),
            }
        }
    }
}

/// Whether a log line is the one an assertion names: the level always, the row
/// when the assertion gives one.
fn matches_log_line(line: &LogLine, level: &str, row: Option<u32>) -> bool {
    line.level == level && row.is_none_or(|row| line.row == row)
}

fn row_suffix(row: Option<u32>) -> String {
    row.map(|row| format!(" row {row}")).unwrap_or_default()
}

/// The line a failed `log` assertion should show: the nearest candidate, so the
/// author sees what the log holds. The park the take stopped at says nothing
/// about the log, so it is the wrong observed value for these two assertions.
fn nearest_log_line<'a>(
    lines: &'a [LogLine],
    level: &str,
    row: Option<u32>,
) -> Option<&'a LogLine> {
    let newest = || lines.iter().rev();
    newest()
        .find(|line| matches_log_line(line, level, row))
        .or_else(|| newest().find(|line| line.level == level))
        .or_else(|| newest().find(|line| row.is_none_or(|row| line.row == row)))
        .or_else(|| lines.last())
}

fn level_name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
    }
}

fn quote_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("\"{item}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The short expected/observed summary a failing assertion reports.
fn describe(observed: &Observation) -> String {
    let mut parts = vec![format!("surface {}", observed.surface)];
    if let Some(scene) = &observed.scene {
        parts.push(format!("scene {scene}"));
    }
    if let Some(line) = observed.line {
        parts.push(format!("line {line}"));
    }
    if let Some(speaker) = &observed.speaker {
        parts.push(format!("speaker \"{speaker}\""));
    }
    if let Some(text) = &observed.text {
        parts.push(format!("line \"{text}\""));
    }
    if !observed.choices.is_empty() {
        parts.push(format!(
            "choices [{}]",
            quote_list(
                &observed
                    .choices
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
            )
        ));
    }
    if let Some(notice) = &observed.notice {
        parts.push(format!("notice \"{notice}\""));
    }
    parts.join(", ")
}

fn expect_name(expect: &Expect) -> &'static str {
    match expect {
        Expect::Surface(_) => "surface",
        Expect::Monitor(_) => "monitor",
        Expect::Speaker(_) => "speaker",
        Expect::Line { .. } => "line",
        Expect::Revealed(_) => "revealed",
        Expect::Choices(_) => "choices",
        Expect::NoticeText { .. } => "notice",
        Expect::NoticeNone => "notice",
        Expect::Control { .. } => "control",
        Expect::ControlLabel { .. } => "control",
        Expect::Bg(_) => "bg",
        Expect::Sprite { .. } | Expect::SpriteNone { .. } => "sprite",
        Expect::Music(_) => "music",
        Expect::Auto(_) => "auto",
        Expect::Skip(_) => "skip",
        Expect::Decision(_) => "decision",
        Expect::Scene(_) => "scene",
        Expect::Label(_) => "label",
        Expect::State { .. } => "state",
        Expect::Read { .. } => "read",
        Expect::Slot { .. } => "slot",
        Expect::Log { .. } => "log",
        Expect::NoLog { .. } => "no-log",
    }
}

/// The runner's scene/label, for a case that asserts a label somewhere.
pub fn label_anywhere(app: &App, label: &str) -> bool {
    app.world()
        .get_resource::<Runtime>()
        .is_some_and(|runtime| {
            runtime
                .runner
                .pack()
                .scenes
                .values()
                .any(|scene| scene.labels.contains_key(label))
        })
}
