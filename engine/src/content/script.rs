//! A parsed `.unmei` scene: its commands in script order and the flat indices
//! the runner resolves jumps, player choices and conditional arms against.

use std::collections::HashMap;

use super::state::{Condition, Literal};

/// One command in a scene, in script order. Produced by the `.unmei` parser.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// `bg <id>`
    Bg(String),
    /// `sprite <npc_id> <pose>`
    Sprite { npc_id: String, pose: String },
    /// `hide <npc_id>`
    Hide(String),
    /// `play <sound_id>`
    Play(String),
    /// `stop <sound_id>`
    Stop(String),
    /// `set <name> = <literal>`
    Set { name: String, value: Literal },
    /// A say-line: `Speaker: text` — text may contain `{var}` refs.
    Say { speaker: String, text: String },
    /// `choice` ... `end` block; each option is (display text, target label).
    Choice(Vec<(String, String)>),
    /// `label <name>`
    Label(String),
    /// `jump <name>`
    Jump(String),
    /// `decide <npc_id>.<question> -> <prefix>`
    Decide {
        npc_id: String,
        question: String,
        prefix: String,
    },
    /// `if <condition>` — opens a conditional. Arms stay flat in [`Scene::lines`],
    /// addressed by these absolute indices, so no runtime block stack is needed
    /// (ADR 0004). The indices are valid only for the exact `Scene` that parsed
    /// them: never reorder or splice a parsed scene's lines in place.
    If {
        condition: Condition,
        /// Index of the `else` marker, when the block has an else arm.
        else_index: Option<usize>,
        /// Index of the block's last line; the false arm resumes after it.
        end_index: usize,
    },
    /// `else` — a conditional's arm separator. Whether control falls through the
    /// end of a taken then-arm or an inner arm's false jump lands here, reaching
    /// this marker always means “skip the else arm” (the block's own condition
    /// was already decided, and it was not false).
    Else {
        /// Index of the block's last line.
        end_index: usize,
    },
}

/// One scene: a parsed `.unmei` file. The scene id is the filename stem.
#[derive(Clone, Debug)]
pub struct Scene {
    pub id: String,
    pub lines: Vec<Line>,
    /// Source line number (1-based) of each entry in `lines`, for error messages.
    pub line_numbers: Vec<usize>,
    /// Label name -> line index of the `label` command itself.
    pub labels: HashMap<String, usize>,
}

impl Scene {
    /// The label's line index, for a `jump` resolved inside this scene.
    pub fn label_index(&self, name: &str) -> Option<usize> {
        self.labels.get(name).copied()
    }

    /// 1-based source line of the command at `idx`, for error messages.
    pub fn source_line(&self, idx: usize) -> usize {
        self.line_numbers.get(idx).copied().unwrap_or(0)
    }
}
