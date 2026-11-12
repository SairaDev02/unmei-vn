//! Headless script runner: the runtime's script program counter, runtime-free.
//!
//! This is the pure core of the locked Bevy pattern: a script
//! ([`crate::content::Scene`] data) plus a `pc` index, stepped until it **parks**
//! on a say-line, a player `choice`, or a `decide` point. The Bevy runtime wraps
//! this in a resource; presentation consumes the returned [`RunOutcome`].
//!
//! Decisions are **not** called here — the runner parks on [`Wait::Decision`]
//! and the caller (the async decision layer, or a test's fake resolver) calls
//! [`ScriptRunner::resolve_decision`] with the chosen action id. That keeps the
//! runner synchronous, per the locked pattern.

mod state_values;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::content::{Line, Pack, StateValue};
use state_values::{condition_holds, interpolate};

/// Where the runner is parked, waiting for input or a decision answer.
///
/// Serialized into the save file: the wait is part of the saved position, so
/// restoring re-shows the same say/choice, or re-fires the same `decide` line.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Wait {
    Say {
        speaker: String,
        text: String,
    },
    Choice {
        /// (display text, target label), in script order.
        options: Vec<(String, String)>,
    },
    Decision {
        npc_id: String,
        question: String,
        prefix: String,
    },
}

/// A staging command the presentation layer applies (`bg`/`sprite`/…).
#[derive(Debug, Clone, PartialEq)]
pub enum StageCommand {
    Bg(String),
    Sprite { npc_id: String, pose: String },
    Hide(String),
    Play(String),
    Stop(String),
}

/// What one [`ScriptRunner::run`] call did: staging effects since the last call,
/// the wait it parked on (`None` = end of script), and the scene it crossed into
/// (`None` unless a `jump` landed in a different scene file).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunOutcome {
    pub staged: Vec<StageCommand>,
    pub wait: Option<Wait>,
    /// The scene file a `jump` crossed into during this call. A local-label
    /// jump, a same-scene target (including the current scene's own id) and the
    /// initial run all leave it `None`.
    pub entered_scene: Option<String>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RunnerError {
    #[error("runner is not parked on the expected wait")]
    NotWaiting,
    #[error("choice index {index} is out of range (0..{len})")]
    BadChoice { index: usize, len: usize },
    #[error("decision action `{action}` has no label `{label}` in scene `{scene}`")]
    UnknownActionLabel {
        action: String,
        label: String,
        scene: String,
    },
    #[error("saved position names scene `{scene}`, which the loaded pack does not contain")]
    UnknownScene { scene: String },
}

/// The runtime script program: a [`Pack`] plus the current scene, program
/// counter, flat state map, and current wait.
pub struct ScriptRunner {
    pack: Pack,
    scene_id: String,
    pc: usize,
    state: HashMap<String, StateValue>,
    wait: Option<Wait>,
}

impl ScriptRunner {
    /// Start at the manifest's entry scene with the pack's declared initial state.
    pub fn new(pack: Pack) -> Self {
        let scene_id = pack.manifest.entry.clone();
        let state = pack.state.clone();
        ScriptRunner {
            pack,
            scene_id,
            pc: 0,
            state,
            wait: None,
        }
    }

    pub fn pack(&self) -> &Pack {
        &self.pack
    }

    pub fn state(&self) -> &HashMap<String, StateValue> {
        &self.state
    }

    pub fn scene_id(&self) -> &str {
        &self.scene_id
    }

    /// Index into the current scene's `lines` of the next command to execute.
    pub fn pc(&self) -> usize {
        self.pc
    }

    pub fn wait(&self) -> Option<&Wait> {
        self.wait.as_ref()
    }

    /// Overwrite the whole runtime position from a save file: state map, scene,
    /// pc, and the parked wait. This is the only writer of these fields outside
    /// [`run`](Self::run); it exists so the save/load seam ([`crate::save`])
    /// restores exactly what [`run`](Self::run) left behind.
    pub fn restore_position(
        &mut self,
        scene_id: String,
        pc: usize,
        state: HashMap<String, StateValue>,
        wait: Option<Wait>,
    ) -> Result<(), RunnerError> {
        if !self.pack.scenes.contains_key(&scene_id) {
            return Err(RunnerError::UnknownScene { scene: scene_id });
        }
        self.scene_id = scene_id;
        self.pc = pc;
        self.state = state;
        self.wait = wait;
        Ok(())
    }

    /// Apply commands until the script parks on a wait or ends. Non-parking
    /// commands (`set`, `label`, `jump`, staging) are applied in order and their
    /// staging effects returned. Already parked ⇒ returns the same wait.
    pub fn run(&mut self) -> RunOutcome {
        let mut staged = Vec::new();
        let mut entered_scene = None;
        if self.wait.is_some() {
            return RunOutcome {
                staged,
                wait: self.wait.clone(),
                entered_scene,
            };
        }
        loop {
            let Some(line) = self.current_line() else {
                return RunOutcome {
                    staged,
                    wait: None,
                    entered_scene,
                };
            };
            self.pc += 1;
            match line {
                Line::Say { speaker, text } => {
                    let wait = Wait::Say {
                        speaker,
                        text: interpolate(&text, &self.state),
                    };
                    self.wait = Some(wait.clone());
                    return RunOutcome {
                        staged,
                        wait: Some(wait),
                        entered_scene,
                    };
                }
                Line::Choice(options) => {
                    let wait = Wait::Choice { options };
                    self.wait = Some(wait.clone());
                    return RunOutcome {
                        staged,
                        wait: Some(wait),
                        entered_scene,
                    };
                }
                Line::Decide {
                    npc_id,
                    question,
                    prefix,
                } => {
                    let wait = Wait::Decision {
                        npc_id,
                        question,
                        prefix,
                    };
                    self.wait = Some(wait.clone());
                    return RunOutcome {
                        staged,
                        wait: Some(wait),
                        entered_scene,
                    };
                }
                Line::Set { name, value } => {
                    self.state.insert(name, value.into());
                }
                Line::If {
                    condition,
                    else_index,
                    end_index,
                } => {
                    if !condition_holds(&condition, &self.state) {
                        // Skip the then arm (and the else marker); resume after
                        // the block, or at the else arm's first line.
                        self.pc = match else_index {
                            Some(else_index) => else_index + 1,
                            None => end_index + 1,
                        };
                    }
                }
                Line::Else { end_index } => {
                    // Reached only after a taken then-arm: skip the else arm.
                    self.pc = end_index + 1;
                }
                Line::Label(_) => {}
                Line::Jump(target) => {
                    if let Some(scene) = self.jump_to(&target) {
                        entered_scene = Some(scene);
                    }
                }
                Line::Bg(id) => staged.push(StageCommand::Bg(id)),
                Line::Sprite { npc_id, pose } => staged.push(StageCommand::Sprite { npc_id, pose }),
                Line::Hide(id) => staged.push(StageCommand::Hide(id)),
                Line::Play(id) => staged.push(StageCommand::Play(id)),
                Line::Stop(id) => staged.push(StageCommand::Stop(id)),
            }
        }
    }

    /// Clear a say-wait; the next [`run`](Self::run) continues past the line.
    pub fn advance_say(&mut self) -> Result<(), RunnerError> {
        if matches!(self.wait, Some(Wait::Say { .. })) {
            self.wait = None;
            Ok(())
        } else {
            Err(RunnerError::NotWaiting)
        }
    }

    /// Pick a `choice` option; its target resolves exactly like `jump` (local
    /// label first, then a scene file).
    pub fn choose(&mut self, index: usize) -> Result<(), RunnerError> {
        let Some(Wait::Choice { options }) = self.wait.clone() else {
            return Err(RunnerError::NotWaiting);
        };
        let Some((_, target)) = options.get(index) else {
            return Err(RunnerError::BadChoice {
                index,
                len: options.len(),
            });
        };
        let target = target.clone();
        self.wait = None;
        self.jump_to(&target);
        Ok(())
    }

    /// Land a decision point on `<prefix>.<action>` and clear the wait.
    ///
    /// The caller is expected to pass an action that survives validation
    /// (a criterion id, `none`, or the author fallback). A label miss is a
    /// programming error, not a player-facing surprise: pack load already
    /// enforces label closure for every criterion and the fallback.
    pub fn resolve_decision(&mut self, action: &str) -> Result<(), RunnerError> {
        let Some(Wait::Decision { prefix, .. }) = self.wait.clone() else {
            return Err(RunnerError::NotWaiting);
        };
        let label = format!("{prefix}.{action}");
        let idx = self
            .pack
            .scenes
            .get(&self.scene_id)
            .and_then(|s| s.labels.get(&label))
            .copied();
        match idx {
            Some(idx) => {
                self.pc = idx;
                self.wait = None;
                Ok(())
            }
            None => Err(RunnerError::UnknownActionLabel {
                action: action.to_string(),
                label,
                scene: self.scene_id.clone(),
            }),
        }
    }

    fn current_line(&self) -> Option<Line> {
        self.pack
            .scenes
            .get(&self.scene_id)?
            .lines
            .get(self.pc)
            .cloned()
    }

    /// The single transition rule (`docs/content-format.md` §5): local label
    /// first, then `scenes/<name>.unmei` from the top. Unknown targets fall
    /// through — pack load rejects them, so this is unreachable for valid packs.
    ///
    /// Returns `Some(scene)` only when the jump crossed into a **different**
    /// scene file; a local label, a same-scene target and the current scene's
    /// own id all return `None`.
    fn jump_to(&mut self, target: &str) -> Option<String> {
        let label = self
            .pack
            .scenes
            .get(&self.scene_id)
            .and_then(|s| s.labels.get(target))
            .copied();
        if let Some(idx) = label {
            self.pc = idx;
            None
        } else if self.pack.scenes.contains_key(target) {
            let crossed = target != self.scene_id;
            self.scene_id = target.to_string();
            self.pc = 0;
            crossed.then(|| target.to_string())
        } else {
            None
        }
    }
}
