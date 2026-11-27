//! The case format's model: a version line, the header lines and the steps.
//!
//! The line numbers are kept beside each item, because every finding names the
//! source line it came from.

use std::time::Duration;

use crate::save::Slot;

/// One parsed case: its headers in authored order, then its steps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Case {
    pub headers: Vec<HeaderLine>,
    pub steps: Vec<StepLine>,
}

impl Case {
    /// The last `decide` header for a decision point, which wins over an
    /// earlier one (a `--step` header added later refines the branch).
    pub fn decide_landing(&self, point: &str) -> Option<&str> {
        self.headers
            .iter()
            .rev()
            .find_map(|line| match &line.header {
                Header::Decide { point: p, action } if p == point => Some(action.as_str()),
                _ => None,
            })
    }

    /// Every `state` override, last one winning per key.
    pub fn state_overrides(&self) -> Vec<(&str, &str)> {
        let mut out: Vec<(&str, &str)> = Vec::new();
        for line in &self.headers {
            if let Header::State { key, value } = &line.header {
                match out.iter_mut().find(|(existing, _)| existing == key) {
                    Some(slot) => slot.1 = value,
                    None => out.push((key, value)),
                }
            }
        }
        out
    }

    /// The scene a `start` header names.
    pub fn start_scene(&self) -> Option<&str> {
        self.headers.iter().find_map(|line| match &line.header {
            Header::Start { scene } => Some(scene.as_str()),
            _ => None,
        })
    }

    /// Whether the case asks for dev mode.
    pub fn dev(&self) -> bool {
        self.headers.iter().any(|line| line.header == Header::Dev)
    }
}

/// One header line and its 1-based source line.
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderLine {
    pub line: usize,
    pub header: Header,
}

/// The header lines of spec §2.2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Header {
    /// `decide <npc>.<question> <action>`
    Decide { point: String, action: String },
    /// `setting <key> <value>`
    Setting { key: String, value: String },
    /// `fixture <kind> <path>`
    Fixture { kind: FixtureKind, path: String },
    /// `fault <name> [args]`
    Fault { fault: Fault },
    /// `dev`
    Dev,
    /// `start <scene-id>`
    Start { scene: String },
    /// `state <key> = <value>`
    State { key: String, value: String },
}

/// One step line and its 1-based source line.
#[derive(Clone, Debug, PartialEq)]
pub struct StepLine {
    pub line: usize,
    pub step: Step,
}

/// The steps of spec §3.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    NewGame,
    Continue,
    Advance(u32),
    Pick(Pick),
    Menu,
    Activate(String),
    Save(u8),
    Load(LoadTarget),
    Auto,
    Skip,
    Press(String),
    Hold(String),
    Release(String),
    Type(String),
    Paste(String),
    Wait(Duration),
    ReleaseDecision(String),
    ReleaseKeyCheck,
    Unblock(WriteTarget),
    Frame(Option<String>),
    Click(Click),
    Expect(Expect),
}

/// How a `pick` names its option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Index(u32),
    Text(String),
}

/// The slot a load step or a slot assertion names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadTarget {
    Slot(u8),
    Autosave,
}

impl LoadTarget {
    /// The observation's slot name.
    pub fn label(self) -> String {
        match self {
            LoadTarget::Slot(n) => n.to_string(),
            LoadTarget::Autosave => "autosave".to_string(),
        }
    }
}

/// A click's target: a named control, or reference coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Click {
    Control(String),
    Position(f32, f32),
}

/// One of the spec §7.1 fixture kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureKind {
    Slot(Slot),
    LegacySave,
    ReadLog,
    Settings,
    Credential,
    EnvKey,
}

impl FixtureKind {
    pub fn name(self) -> &'static str {
        match self {
            FixtureKind::Slot(Slot::Manual(1)) => "slot-1",
            FixtureKind::Slot(Slot::Manual(2)) => "slot-2",
            FixtureKind::Slot(Slot::Manual(3)) => "slot-3",
            FixtureKind::Slot(Slot::Autosave) => "autosave",
            FixtureKind::Slot(Slot::Manual(_)) => "slot",
            FixtureKind::LegacySave => "legacy-save",
            FixtureKind::ReadLog => "read-log",
            FixtureKind::Settings => "settings",
            FixtureKind::Credential => "credential",
            FixtureKind::EnvKey => "env-key",
        }
    }
}

/// The sidecar or file a `write-blocked` / `unblock` step obstructs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteTarget {
    Slot(Slot),
    ReadLog,
    Settings,
    Credential,
}

impl WriteTarget {
    pub fn name(self) -> &'static str {
        match self {
            WriteTarget::Slot(Slot::Manual(1)) => "slot-1",
            WriteTarget::Slot(Slot::Manual(2)) => "slot-2",
            WriteTarget::Slot(Slot::Manual(3)) => "slot-3",
            WriteTarget::Slot(Slot::Autosave) => "autosave",
            WriteTarget::Slot(Slot::Manual(_)) => "slot",
            WriteTarget::ReadLog => "read-log",
            WriteTarget::Settings => "settings",
            WriteTarget::Credential => "credential",
        }
    }
}

/// One fault from the closed list of spec §7.2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    WriteBlocked { target: WriteTarget },
    AssetMissing { path: String },
    AssetCorrupt { path: String },
    PackFileMissing { path: String },
    PackFile { path: String, fixture: String },
    CredentialUnreadable,
    DecisionError { point: String, kind: DecisionFault },
    DecisionSlow { point: String, duration: Duration },
    DecisionHold { point: String },
    KeyCheck { verdict: KeyCheckFault },
}

/// How a `decision-error` fault fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionFault {
    Transport,
    Answer,
    Refusal,
}

/// The verdict a `key-check` fault answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCheckFault {
    Accepted,
    Rejected,
    Unreachable,
    Hold,
}

/// An assertion step's subject and expectation (spec §3.5).
#[derive(Clone, Debug, PartialEq)]
pub enum Expect {
    Surface(String),
    Monitor(bool),
    Speaker(Option<String>),
    Line {
        text: String,
        contains: bool,
    },
    Revealed(bool),
    Choices(Vec<String>),
    NoticeText {
        text: String,
        contains: bool,
    },
    NoticeNone,
    Control {
        id: String,
        condition: ControlCondition,
    },
    ControlLabel {
        id: String,
        label: String,
    },
    /// The control's laid-out node: `[x, y, w, h]` in reference pixels, each
    /// within `tolerance` (spec §3.5).
    ControlRect {
        id: String,
        rect: [f32; 4],
        tolerance: f32,
    },
    Bg(Option<String>),
    Sprite {
        npc: String,
        pose: String,
    },
    SpriteNone {
        npc: String,
    },
    Music(Option<String>),
    Auto(bool),
    Skip(SkipMode),
    Decision(DecisionState),
    Scene(String),
    Label(Option<String>),
    State {
        key: String,
        value: String,
    },
    Read {
        scene: String,
        line: u32,
        present: bool,
    },
    Slot {
        slot: LoadTarget,
        status: SlotExpect,
    },
    Log {
        level: LogLevel,
        row: Option<u32>,
        text: String,
    },
    NoLog {
        level: LogLevel,
        row: Option<u32>,
    },
}

/// A control's expected condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlCondition {
    Enabled,
    Disabled,
    Focused,
    Absent,
}

/// The player's skip mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipMode {
    Seen,
    Force,
    Off,
}

/// A decision point's expected lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionState {
    Pending,
    Held,
    None,
}

/// A save slot's expected status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotExpect {
    Empty,
    Unreadable,
    Scene(String),
}

/// A session-log level an assertion names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
}
