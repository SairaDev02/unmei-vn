//! Domain types for unmei content: state values, scripts, scenes, NPCs.
//!
//! Vocabulary per `GLOSSARY.md`: content pack, manifest, entry scene, scene,
//! say-line, label, player choice, state map, NPC, action, decision point,
//! criterion, fallback action.
//!
//! This root holds the pack-level types and re-exports the state, script and
//! decision clusters from their own modules, so every `content::<Type>` path
//! callers already use stays the same.

use std::collections::HashMap;
use std::path::PathBuf;

mod decision;
mod script;
mod state;

pub use decision::{Criteria, Criterion, DecisionConfig, Fallback, QuestionType};
pub use script::{Line, Scene};
pub use state::{CondOp, Condition, Literal, StateValue};

/// An NPC's structured definition (`npcs/<id>.ron`) — everything the author CLI
/// generates/edits lives in this one file.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Npc {
    pub id: String,
    pub name: String,
    /// Optional pinned decision-model id (alpha-risk mitigation); the response
    /// `model` is logged at decision time. Authored as a plain RON string, not
    /// RON's `Some("model-id")` — it (de)serializes through the engine pack RON
    /// bridge's content optional-string adapter (`crate::pack_ron`), which owns
    /// the one conversion shared with the other optional content strings.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::pack_ron::content_optional_string"
    )]
    pub model: Option<String>,
    /// Question id -> decision config.
    pub decisions: HashMap<String, DecisionConfig>,
}

/// Pack manifest (`pack.ron`) — minimal by decision: name + entry scene, plus
/// the optional `tagline`, `title_backdrop` and declared `scenes` order.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub entry: String,
    /// Optional title tagline: authored as a plain RON string or omitted,
    /// never `Some(…)` (the same bridge as an NPC's pinned `model`). Normalised
    /// at load: trimmed of surrounding ASCII spaces, empty means none, a
    /// control character is an error and >48 characters warns once. The title
    /// renders it uppercased; `{var}` interpolation is never performed.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::pack_ron::content_optional_string"
    )]
    pub tagline: Option<String>,
    /// Optional title backdrop: an asset id `[a-z0-9_]+` resolved exactly
    /// like `bg` to `assets/backgrounds/<id>.png`, or omitted. Authored as a
    /// plain RON string or nothing (the same bridge as `tagline`/`model`); a
    /// malformed id is a `pack.ron` load error, while a declared-but-missing
    /// file is a runtime silent degradation.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::pack_ron::content_optional_string"
    )]
    pub title_backdrop: Option<String>,
    /// Optional declared scene order: the pack's sole order source, the
    /// Timeline's enumeration. Authored as a plain RON list of scene-id stems
    /// (`scenes: ["a", "b"]`) or omitted, never RON's `Some([…])` wrapping — a
    /// wrong-typed value is a load-time error. Ids are matched exactly
    /// (case-sensitive, byte-for-byte stems, no trimming). Absent and empty are
    /// equivalent: no declared order, no alphabetical fallback. The list keeps
    /// its authored order; validation lives in [`crate::load_pack`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenes: Vec<String>,
}

/// A fully loaded, validated content pack. Shared by the runtime and the author CLI.
#[derive(Clone, Debug)]
pub struct Pack {
    pub manifest: Manifest,
    /// The declared state vocabulary and initial values.
    pub state: HashMap<String, StateValue>,
    /// Scene id -> scene. Ids are filename stems.
    pub scenes: HashMap<String, Scene>,
    /// NPC id -> definition. Ids are filename stems, cross-checked against the `id` field.
    pub npcs: HashMap<String, Npc>,
    /// The pack directory, for asset resolution by the runtime layer.
    pub root: PathBuf,
}

/// One image a pack declares for the gallery ([GLOSSARY](../../../GLOSSARY.md):
/// gallery entry): its kind and its ids, carrying **no asset path** — the app's
/// path producers own the path rules. Repeated declarations of the same image
/// are one entry; [`Pack::gallery_entries`](crate::Pack::gallery_entries)
/// derives, dedupes and orders them from the pack's own data.
///
/// The derived `Ord` fixes the gallery's order (spec `docs/title-and-menu.md`
/// § Gallery `### Entries` §3): backgrounds before sprites (the variant order),
/// then by `id` / by `(npc_id, pose)` — each group alphabetical — so the order
/// is total and never follows `HashMap` iteration.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GalleryEntry {
    /// A background id named by a `bg` line or by the manifest's
    /// `title_backdrop`. The dedupe key and the label are the raw id.
    Background { id: String },
    /// A sprite `(npc_id, pose)` pair named by a `sprite` line. The dedupe key
    /// is the pair; the label is `npc_id · pose`.
    Sprite { npc_id: String, pose: String },
}

impl GalleryEntry {
    /// The entry's label (spec `### Entries` §2): a background's raw id, or a
    /// sprite's `npc_id · pose` with U+00B7 `·` between the ids. The card and
    /// the still caption both render this one string.
    pub fn label(&self) -> String {
        match self {
            GalleryEntry::Background { id } => id.clone(),
            GalleryEntry::Sprite { npc_id, pose } => format!("{npc_id} \u{00B7} {pose}"),
        }
    }
}
