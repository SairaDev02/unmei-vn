//! Keiko's Bevy-free core: the case format and its static checks (spec §2–§3,
//! §11).
//!
//! The engine owns the case grammar, the finding shape and every reference
//! check that can be answered from the pack, so the same function serves both
//! `unmei-play`'s Keiko modes and, later, the plugin family's `--check`. The
//! two tables the engine cannot own — the settings keys and the `KeyCode` names
//! — arrive as data from the app.

mod controls;
mod model;
mod parse;
mod parse_step;
mod text;
mod validate;

#[cfg(test)]
mod tests;

pub use crate::finding::{rules, Finding, Location};
pub use controls::{is_control_id, slot_control, CONTROL_IDS, INDEXED_CONTROL_FAMILIES};
pub use model::{
    Case, Click, ControlCondition, DecisionFault, DecisionState, Expect, Fault, FixtureKind,
    Header, HeaderLine, KeyCheckFault, LoadTarget, LogLevel, Pick, SkipMode, SlotExpect, Step,
    StepLine, WriteTarget,
};
pub use parse::{parse_case, parse_duration, CASE_VERSION};
pub use parse_step::SURFACES;
pub use text::{exact, normalize};
pub use validate::{is_confined, parse_state_value, slot_of, validate, Facts};
