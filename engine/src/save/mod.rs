//! Save/load: the four fixed sibling save slots.
//!
//! Runtime-free by design. [`SaveFile`] captures exactly what is needed to
//! resume mid-scene — the flat state map, the script position (scene id, `pc`,
//! and the parked [`Wait`]), and the presentation staging snapshot
//! ([`StagingState`]) — and RON is the wire format on disk
//! (`docs/runtime-hardening.md` § Save slots; [ADR 0006]).
//!
//! Three manual slots the player writes and one autosave the game writes, each
//! one `SaveFile` at a distinct sibling path ([`save_path_for`]). The slice's
//! single `<pack-dir>.save.ron` survives as the **legacy** file
//! ([`legacy_save_path_for`], [`legacy_save`]).
//!
//! Load is *exact resume*: a say-wait re-shows the same line, a choice-wait
//! re-shows the same options, and a decision-wait is left parked so the caller
//! re-fires the `decide` line against the reloaded state (fresh Decisions call).
//! [`list_slots`] classifies every slot up front — empty, readable, or
//! unreadable — so no caller offers a load guaranteed to fail.
//!
//! [ADR 0006]: ../../docs/adr/0006-save-slots-on-disk-format-and-version-policy.md

mod legacy;
mod position;
mod slots;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::content::StateValue;
use crate::runner::{RunnerError, ScriptRunner, StageCommand, Wait};

use position::validate_position;

pub use legacy::{import_legacy, imported_legacy_path_for, legacy_save, legacy_save_path_for};
pub use slots::{list_slots, read_slot, save_path_for, write_slot, Slot, SlotInfo, SlotStatus};

/// On-disk save format version. Bump when the shape of [`SaveFile`] changes.
///
/// The policy is an **additive floor** (ADR 0006): any `1..=SAVE_VERSION` file
/// loads, every field added after v1 carries `#[serde(default)]`, and only a
/// version newer than this build is rejected.
pub const SAVE_VERSION: u32 = 2;

/// What the script's staging commands have put on screen.
///
/// This is the presentation-decoupled view of `bg`/`sprite`/`hide`/`play`/`stop`
/// state: the runtime maps it onto sprites and audio, and the save file stores
/// it so a load re-applies visuals without replaying the scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StagingState {
    /// The current `bg` id, if a background has been staged.
    pub background: Option<String>,
    /// Visible sprites: npc id -> pose.
    pub sprites: BTreeMap<String, String>,
    /// The id of the track currently looping, or `None` when stopped.
    pub bgm: Option<String>,
}

impl StagingState {
    /// Apply one staged command.
    ///
    /// `play` **replaces** the current track (music posture: one BGM at a time),
    /// `stop <id>` clears it only when the id matches what was played, and
    /// `hide` removes a sprite so a load can't resurrect it.
    pub fn apply(&mut self, command: &StageCommand) {
        match command {
            StageCommand::Bg(id) => self.background = Some(id.clone()),
            StageCommand::Sprite { npc_id, pose } => {
                self.sprites.insert(npc_id.clone(), pose.clone());
            }
            StageCommand::Hide(npc_id) => {
                self.sprites.remove(npc_id);
            }
            StageCommand::Play(id) => self.bgm = Some(id.clone()),
            StageCommand::Stop(id) => {
                if self.bgm.as_deref() == Some(id.as_str()) {
                    self.bgm = None;
                }
            }
        }
    }

    /// Apply a run's staging effects in order.
    pub fn apply_all(&mut self, commands: &[StageCommand]) {
        for command in commands {
            self.apply(command);
        }
    }
}

/// The whole saved position: state map + script position + staging snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveFile {
    pub version: u32,
    pub state: HashMap<String, StateValue>,
    pub scene_id: String,
    pub pc: usize,
    /// The park the runner was sitting in, if any. Restored verbatim.
    pub wait: Option<Wait>,
    pub staging: StagingState,
    /// Unix epoch **seconds** the app stamped when it wrote this slot. `None`
    /// for a save the engine captured (`capture` never stamps) and for a v1
    /// file, which has no such field.
    #[serde(default)]
    pub saved_at: Option<u64>,
}

impl SaveFile {
    /// Capture the runner's whole position plus the presentation snapshot.
    ///
    /// Pure and deterministic: `saved_at` is left `None`; the **app** stamps
    /// the epoch immediately before every [`write_slot`].
    pub fn capture(runner: &ScriptRunner, staging: &StagingState) -> Self {
        SaveFile {
            version: SAVE_VERSION,
            state: runner.state().clone(),
            scene_id: runner.scene_id().to_string(),
            pc: runner.pc(),
            wait: runner.wait().cloned(),
            staging: staging.clone(),
            saved_at: None,
        }
    }

    /// Restore this save into `runner`, returning the staging snapshot the
    /// presentation layer should re-apply. The runner's pack is the loaded pack;
    /// the saved position is accepted only if it passes the same private rule
    /// slot reading applies (`validate_position`), checked against that pack
    /// before any mutation. A save naming a scene that pack lacks is rejected,
    /// not silently ignored.
    ///
    /// A refusal leaves the runner's scene, `pc`, state map and parked wait
    /// untouched and returns no staging, so the caller can keep the old session.
    /// Agreement with [`read_slot`] holds for an unchanged saved value and the
    /// same loaded pack: a value accepted against one pack can be refused
    /// against another, and a file or public-field edit after a successful read
    /// is checked again here.
    pub fn restore(self, runner: &mut ScriptRunner) -> Result<StagingState, SaveError> {
        validate_position(runner.pack(), &self)?;
        runner.restore_position(self.scene_id, self.pc, self.state, self.wait)?;
        Ok(self.staging)
    }

    pub fn to_ron(&self) -> Result<String, SaveError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| SaveError::Encode(e.to_string()))
    }

    pub fn from_ron(text: &str) -> Result<Self, SaveError> {
        ron::from_str(text).map_err(|e| SaveError::Decode(e.to_string()))
    }
}

/// Write a save to its fixed path, atomically ([`crate::sidecar::write_atomic`]).
pub fn write_save(path: &Path, save: &SaveFile) -> Result<(), SaveError> {
    let text = save.to_ron()?;
    crate::sidecar::write_atomic(path, &text)
        .map_err(|error| save_io_error(path.to_path_buf(), error.to_string()))
}

/// Read a save from its fixed path.
pub fn read_save(path: &Path) -> Result<SaveFile, SaveError> {
    let text = std::fs::read_to_string(path).map_err(|e| SaveError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    SaveFile::from_ron(&text)
}

fn save_io_error(path: PathBuf, message: impl Into<String>) -> SaveError {
    SaveError::Io {
        path,
        message: message.into(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("save file version {found} is unsupported (expected {expected})")]
    Version { found: u32, expected: u32 },
    #[error("could not read/write save file `{}`: {message}", path.display())]
    Io { path: PathBuf, message: String },
    #[error("invalid save file: {0}")]
    Decode(String),
    #[error("could not encode save file: {0}")]
    Encode(String),
    #[error(transparent)]
    Runner(#[from] RunnerError),
}
