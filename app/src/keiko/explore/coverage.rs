//! Exploration coverage: what each scene reached (spec §9.3).

use std::collections::BTreeSet;

use serde_json::{json, Value};

use unmei_engine::Pack;

use super::super::take::TakeResult;

/// Per-scene coverage: reached say-lines and choices out of the pack's totals.
pub(super) struct Coverage {
    scenes: BTreeSet<String>,
    say: BTreeSet<(String, u32)>,
    choices: BTreeSet<(String, u32)>,
}

impl Coverage {
    pub(super) fn new(_pack: &Pack) -> Self {
        Coverage {
            scenes: BTreeSet::new(),
            say: BTreeSet::new(),
            choices: BTreeSet::new(),
        }
    }

    pub(super) fn observe(&mut self, result: &TakeResult) {
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

    pub(super) fn to_json(&self, pack: &Pack) -> Value {
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
            let reached_say = self.say.iter().filter(|(id, _)| *id == scene.id).count();
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
