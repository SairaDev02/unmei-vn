//! unmei's Bevy runtime — the vertical slice's playable shell.
//!
//! The runtime wraps the engine's headless [`ScriptRunner`]: the runner owns the
//! script position and state, this crate owns the window, the presentation, the
//! Decisions call, and the save file. Split in two so the *core* runs headlessly
//! in tests:
//!
//! - [`RuntimeCorePlugin`] — pack load, script stepping, player input, the
//!   async Decisions call, slot save/load, and the autosave write points. No
//!   assets, no rendering.
//! - [`UnmeiPlugin`] — core + presentation (background/sprites/dialogue/choices)
//!   and `bevy_kira_audio` (every `play` loops, `stop` silences, optional
//!   `sfx_select` on choice).
//!
//! Save/load scope: `docs/runtime-hardening.md` § Save slots. The save files are
//! engine-owned ([`unmei_engine::save`]); this crate decides *when* to
//! capture/restore and stamps `saved_at` on every write ([`slots`]).
//!
//! This root is the module-registration hub and holds the resources and system
//! sets every other module orders itself against. The plugin bodies live in
//! [`plugins`]; each gameplay area owns a directory module.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use unmei_engine::save::StagingState;
use unmei_engine::{DecisionResolver, ScriptRunner};

// Named `core`; note this shadows the `core` crate within this crate.
pub mod composition;
mod core;
pub mod credentials;
pub mod drawer;
pub mod failure;
mod input;
pub mod launch;
pub mod menu;
pub mod monitor;
mod monitor_reader;
pub mod panic_log;
mod plugins;
mod presentation;
pub mod read_log;
mod session;
mod session_lines;
pub mod settings;
mod slots;
#[cfg(test)]
mod tests;

pub use core::{
    DecisionChannel, DecisionFactory, HeldDecision, KeyCheckChannel, PendingDecision,
    PendingKeyCheck, ResolverBuilder,
};
pub use failure::DefensiveHold;
#[cfg(test)]
pub(crate) use plugins::register_ownership_gated_presentation;

// ---------------------------------------------------------------------------
// Configuration + resources
// ---------------------------------------------------------------------------

/// What the runtime needs before it can start: which pack to play, how that pack
/// was chosen, whether a Decisions resolver is configured, and the one
/// app-lifetime dev-mode value.
/// `main` wraps the concrete `DecisionsClient` in an `Arc`; tests pass `None` to
/// exercise the author-fallback path offline, or a test-local fake behind `Some`
/// to drive the pipeline without a network.
#[derive(Resource)]
pub struct RuntimeConfig {
    pub pack_root: PathBuf,
    /// Which rule chose [`RuntimeConfig::pack_root`]: the positional `PACK_DIR`,
    /// or the default beside the executable. Kept beside the path it explains so
    /// the startup error screen can say why the default was used without
    /// re-deriving it.
    pub pack_origin: crate::launch::PackOrigin,
    pub decisions: Option<Arc<dyn DecisionResolver>>,
    /// The single developer-affordance policy for this app lifetime: whether
    /// the F5/F9 manual-slot-1 shortcuts and the F3 Monitor drawer are enabled.
    /// Fixed at construction; never read from the process arguments, toggled or
    /// persisted. Resolved before the logger and runtime plugins are installed
    /// so the capture hook and the shortcut gate consume the same value.
    pub dev_mode: bool,
}

impl RuntimeConfig {
    /// The normal construction: the dev-mode value follows the compiler's
    /// debug-assertions setting — on in a normal debug/test build, off in a
    /// normal release build. This is `cfg!(debug_assertions)`, never an
    /// optimization level or the presence of debug symbols. The pack is treated
    /// as explicitly given until [`RuntimeConfig::with_pack_origin`] says
    /// otherwise.
    pub fn new(pack_root: PathBuf, decisions: Option<Arc<dyn DecisionResolver>>) -> Self {
        Self {
            pack_root,
            pack_origin: crate::launch::PackOrigin::Explicit,
            decisions,
            dev_mode: cfg!(debug_assertions),
        }
    }

    /// The explicit construction-time override: force dev mode either on or off
    /// in any build. This is an embedding/test seam, not a `--no-dev` flag.
    pub fn with_dev_mode(mut self, dev_mode: bool) -> Self {
        self.dev_mode = dev_mode;
        self
    }

    /// Record that the default rule, not a positional `PACK_DIR`, chose the pack.
    pub fn with_pack_origin(mut self, origin: crate::launch::PackOrigin) -> Self {
        self.pack_origin = origin;
        self
    }
}

/// The one dev-mode gate: the developer shortcuts and the Monitor capture
/// consume the value [`RuntimeConfig`] carries rather than recomputing a
/// feature-specific default. Tolerant of a world without the resource, which
/// can only mean off.
pub(crate) fn dev_mode_enabled(config: Option<Res<RuntimeConfig>>) -> bool {
    dev_mode_is_on(config.as_deref())
}

/// The same gate for a plain borrow — the `LogPlugin::custom_layer` hook runs
/// before any ECS system, so it reads the resource directly rather than through
/// a `Res`.
pub(crate) fn dev_mode_is_on(config: Option<&RuntimeConfig>) -> bool {
    config.is_some_and(|config| config.dev_mode)
}

/// The live play state: the engine runner, its staging snapshot, and where the
/// pack lives.
#[derive(Resource)]
pub struct Runtime {
    pub runner: ScriptRunner,
    pub staging: StagingState,
    pub pack_root: PathBuf,
    /// `<pack_root>/assets` — the asset server's root.
    pub asset_root: PathBuf,
}

// ---------------------------------------------------------------------------
// System sets
// ---------------------------------------------------------------------------

/// Pack load, script stepping, input, decision polling, save/load.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeCoreSet;

/// Rendering + audio, always after the core has settled the frame's state.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PresentationSet;

/// Notice ticking and error-screen input, between the core and the
/// presentation that renders the notice. This is not part of
/// [`RuntimeCoreSet`] because it runs on every surface, including the startup
/// error screen where no `Runtime` exists. Ordering it after the core makes the
/// one [`failure::Notice`] a core system reports this frame visible to the
/// presentation in the same frame, and keeps the error screen's `AppExit`
/// input clear of the in-play menu input.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct NoticeSet;

/// Monitor drawer projection and widget sync, last in `Update` so a row
/// materialized this frame renders from the frame's settled history, panel and
/// window mode. It runs on every surface (title, menu, error screen), not only
/// while a `Runtime` exists.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MonitorDrawerSet;

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

/// Headless-capable core: safe to add under `MinimalPlugins` in tests.
pub struct RuntimeCorePlugin;

/// Core + presentation + audio. This is the playable app.
pub struct UnmeiPlugin;
