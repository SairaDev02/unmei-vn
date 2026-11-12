//! The runner-driving systems: pack setup, decision start/poll, stepping the
//! script until it parks, and the auto-mode dwell.
//!
//! Each responsibility owns its module: [`pack`] loads the pack and parks the
//! runner at the entry, [`decision`] holds the request/channel vocabulary and
//! applies a resolved answer, [`script`] steps to the next park, [`skip`] and
//! [`auto`] own the two comfort clocks, and [`key_check`] the key-validation
//! ping. This root re-exports them, so every `crate::core::<name>` path callers
//! already use stays the same.

mod auto;
mod decision;
mod key_check;
mod pack;
mod script;
mod skip;

use bevy::prelude::*;

use crate::failure::DefensiveHold;

pub use auto::AutoDwell;
pub use decision::{
    DecisionChannel, DecisionFactory, HeldDecision, PendingDecision, ResolverBuilder,
};
pub use key_check::{KeyCheckChannel, PendingKeyCheck};
pub use skip::SkipState;

pub(crate) use auto::auto_dwell;
pub(crate) use decision::poll_decision;
pub(crate) use key_check::{poll_key_check, spawn_key_check};
pub(crate) use pack::{park_at_entry, setup_pack};
pub(crate) use script::drive_script;
#[cfg(test)]
pub(crate) use skip::SKIP_DWELL_SECONDS;
pub(crate) use skip::{ctrl_down, ctrl_just_pressed, force_release_mirror, skip_advance};

/// The position-replacement reset shared by every discontinuity that restores
/// or replaces the runner's position — a load, a New Game and the later
/// Timeline landing: both comfort clocks reset and both skip modes disarm, so a
/// held `Ctrl` needs a release and a fresh press in play and a restored line
/// gets a full dwell (`docs/runtime-hardening.md` § Text skip §5).
///
/// Title→play is deliberately not a position replacement: live Continue
/// preserves the remaining auto dwell and disarms skip on its own path, so it
/// does **not** go through this reset ([`SkipState::disarm`] directly).
pub(crate) fn reset_for_replacement(auto: &mut AutoDwell, skip: &mut SkipState) {
    auto.reset();
    skip.disarm();
}

/// The run condition the play path shares: a defensive hold freezes it. Saving
/// and loading stay live, so the menu (and F9) can still recover with a load.
pub fn not_held(hold: Res<DefensiveHold>) -> bool {
    !hold.0
}
