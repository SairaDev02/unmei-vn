//! The real command runner both shipping commands share: the typed outcome,
//! the two command inputs, the missing-pin notes and the re-exports that keep
//! `unmei::runner::<Item>` stable. Each command's path lives beside this root
//! (author-cli contract §5, drafting contract §10).

use std::io::Write;

use unmei_engine::Pack;

use crate::gen;
use crate::provider::ProviderError;
use crate::target::TargetError;
pub use crate::terminal::Terminal;

mod draft;
mod npc;

pub use draft::{run_draft, DraftCommand};
pub use npc::{run_npc, NpcCommand};

/// What the runner did. `Discarded` still printed its existing
/// acknowledgement; both variants leave main to exit 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOutcome {
    Completed,
    Discarded,
}

/// Why the runner stopped. Every variant's `Display` is the exact text main
/// prints after its `✖ ` prefix.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error(transparent)]
    Target(#[from] TargetError),
    #[error("generation failed: {0}")]
    Npc(#[from] gen::GenerationError),
    #[error("provider error: {0}")]
    DraftProvider(ProviderError),
    #[error(
        "all {limit} attempts failed validation — nothing was written",
        limit = gen::RETRY_LIMIT
    )]
    DraftExhausted,
    #[error("{0}")]
    Message(String),
    #[error("splice failed: {0}")]
    Splice(#[from] crate::splice::SpliceError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// The note for each NPC that has decision configs but no model pin, sorted by
/// NPC id (`docs/nanogpt.md` N26). An NPC with no pin is valid and decided
/// offline, so this is a note, never an error.
fn missing_pin_notes(pack: &Pack) -> Vec<String> {
    let mut ids: Vec<&str> = pack
        .npcs
        .values()
        .filter(|npc| npc.model.is_none() && !npc.decisions.is_empty())
        .map(|npc| npc.id.as_str())
        .collect();
    ids.sort_unstable();
    ids.into_iter()
        .map(|id| {
            format!(
                "note: npc `{id}` has decision configs but no model pin, so its decision points always take the fallback"
            )
        })
        .collect()
}

/// Write [`missing_pin_notes`] to the terminal's error stream, which main
/// builds on the masked stderr like every other diagnostic.
pub fn write_missing_pin_notes(pack: &Pack, err: &mut dyn Write) -> std::io::Result<()> {
    for note in missing_pin_notes(pack) {
        writeln!(err, "{note}")?;
    }
    Ok(())
}
