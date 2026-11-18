//! The observation's §5.2 text form, kept apart so `observe.rs` stays under
//! the source-size limit.

use super::observe::{state_value_text, DecisionStatus, Observation};

impl Observation {
    /// The §5.2 text form: a headline, then one group per non-empty field.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "{} · {:.3}s",
            self.surface,
            self.clock_ms as f64 / 1000.0
        ));
        if let (Some(scene), Some(line)) = (&self.scene, self.line) {
            out.push_str(&format!(" · {scene}:{line}"));
            if let Some(label) = &self.label {
                out.push_str(&format!(" (after {label})"));
            }
        }
        if let Some(decision) = &self.decision {
            out.push_str(&format!(
                " · decision {} {}",
                decision.point,
                match decision.status {
                    DecisionStatus::Pending => "pending",
                    DecisionStatus::Held => "held",
                }
            ));
        }
        if self.live {
            out.push_str(" · live (nondeterministic)");
        }
        out.push('\n');

        if let Some(speaker) = &self.speaker {
            out.push_str(&format!("{speaker}: "));
        }
        if let Some(text) = &self.text {
            out.push_str(&format!("\"{text}\""));
            if self.revealed {
                out.push_str("  [revealed]");
            } else {
                out.push_str("  [revealing]");
            }
            out.push('\n');
        }
        for choice in &self.choices {
            out.push_str(&format!("  {} \"{}\"\n", choice.index, choice.text));
        }
        if let Some(notice) = &self.notice {
            out.push_str(&format!("notice: \"{notice}\"\n"));
        }
        if !self.controls.is_empty() {
            let controls: Vec<String> = self
                .controls
                .iter()
                .map(|control| {
                    let focus = if control.focused { "*" } else { "" };
                    let enabled = if control.enabled { "" } else { " (disabled)" };
                    format!("{focus}{}={}:{enabled}", control.id, control.label)
                })
                .collect();
            out.push_str(&format!("controls: {}\n", controls.join(" · ")));
        }
        let mut stage = Vec::new();
        if let Some(background) = &self.staging.background {
            stage.push(format!("bg {background}"));
        }
        for (npc, pose) in &self.staging.sprites {
            stage.push(format!("{npc}={pose}"));
        }
        if let Some(music) = &self.staging.music {
            stage.push(format!("music {music}"));
        }
        if !stage.is_empty() {
            out.push_str(&format!("staging: {}\n", stage.join(" · ")));
        }
        let mut comfort = Vec::new();
        if self.auto {
            comfort.push("auto on".to_string());
        }
        if self.skip != "off" {
            comfort.push(format!("skip {}", self.skip));
        }
        if self.monitor {
            comfort.push("monitor open".to_string());
        }
        if !comfort.is_empty() {
            out.push_str(&format!("{}\n", comfort.join(" · ")));
        }
        if !self.state.is_empty() {
            let pairs: Vec<String> = self
                .state
                .iter()
                .map(|(key, value)| format!("{key}={}", state_value_text(value)))
                .collect();
            out.push_str(&format!("state: {}\n", pairs.join(" ")));
        }
        let occupied: Vec<String> = self
            .slots
            .iter()
            .filter(|slot| slot.status != "empty")
            .map(|slot| {
                format!(
                    "{}={}{}",
                    slot.slot,
                    slot.status,
                    slot.scene
                        .as_ref()
                        .map(|scene| format!(":{scene}"))
                        .unwrap_or_default()
                )
            })
            .collect();
        if !occupied.is_empty() {
            out.push_str(&format!("slots: {}\n", occupied.join(" ")));
        }
        if !self.error.is_empty() {
            out.push_str(&format!("error: {}\n", self.error.join(" / ")));
        }
        for line in &self.log {
            out.push_str(&format!(
                "log {} row {}: {}\n",
                line.level, line.row, line.text
            ));
        }
        out
    }
}
