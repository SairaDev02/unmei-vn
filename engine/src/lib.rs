//! `unmei-engine` is the shared core of Unmei, a visual-novel engine. It loads
//! and validates **content packs** ([`pack`]), parses **`.unmei` scenes**
//! ([`parser`]) and runs them ([`runner`]), with no rendering, windowing or
//! audio dependency of its own. It also reads and writes save slots ([`save`])
//! and a read log of what the player has read ([`read_log`]).
//!
//! Both the Bevy player and the `unmei` authoring CLI are built on this crate, so
//! they apply the same pack validation rules.
//!
//! # Features
//!
//! The `network` feature is off by default, so a consumer that only loads packs
//! does not build the HTTP client or its dependencies. The feature adds the
//! OpenRouter Decisions client (`decision::client`), which resolves decision
//! points through the OpenRouter Decisions API. docs.rs builds this crate with
//! the feature on.
//!
//! # Stability
//!
//! During 0.x the pack, save and settings formats are not frozen: any of them
//! may change. A build refuses a save whose format version is newer than the
//! build supports ([`save::SAVE_VERSION`]). [`load_pack`] returns an error for a
//! pack it cannot load correctly, instead of loading it wrongly. Read the
//! documentation of the release you use.
//!
//! # Documentation
//!
//! - [The pack format overview](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/format-overview.md): the pack
//!   layout, the `.unmei` DSL and the decision configs, each linked to the
//!   content format section that owns it.
//! - [The Guide](https://github.com/SairaDev02/unmei-vn/blob/v0.1.0/docs/guide/README.md): the task-oriented pages for
//!   players, authors and contributors.

pub mod content;
// The pure Decisions wire layer is used by the network client (`feature =
// "network"`) and by this crate's own tests. A loader-only build — the default —
// therefore compiles part of it unused, so the lint is off for this module in
// exactly that configuration, and only there: without `network`,
// `cargo build -p unmei-engine` is warning-free.
#[cfg_attr(not(feature = "network"), allow(dead_code))]
pub mod decision;
pub mod error;
pub mod pack;
pub mod pack_ron;
pub mod parser;
pub mod read_log;
pub mod runner;
pub mod save;
pub mod sidecar;

pub use content::{
    CondOp, Condition, Criteria, Criterion, DecisionConfig, Fallback, GalleryEntry, Line, Literal,
    Manifest, Npc, Pack, QuestionType, Scene, StateValue,
};
pub use decision::{
    decision_from_body, resolve_or_fallback, Decision, DecisionError, DecisionResolver, KeyCheck,
    KeyVerdict, ResolvedDecision, DEFAULT_ENDPOINT, KEY_CHECK_TIMEOUT, KEY_ENDPOINT,
    UNVERIFIED_MODEL,
};
pub use error::LoadError;
pub use pack::{load_pack, read_pack_text, LoadedPack, Warning};
pub use parser::{check_id, is_keyword, split_say_line, Parser, KEYWORDS};
pub use read_log::{
    load_read_log, read_log_path_for, write_read_log, ReadLog, ReadLogError, READ_LOG_VERSION,
};
pub use runner::{RunOutcome, RunnerError, ScriptRunner, StageCommand, Wait};
pub use save::{
    import_legacy, imported_legacy_path_for, legacy_save, legacy_save_path_for, list_slots,
    read_save, read_slot, save_path_for, write_save, write_slot, SaveError, SaveFile, Slot,
    SlotInfo, SlotStatus, StagingState,
};

#[cfg(feature = "network")]
pub use decision::{DecisionsClient, HttpKeyCheck};
