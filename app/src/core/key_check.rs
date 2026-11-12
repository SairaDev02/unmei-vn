//! The key-validation ping: the checker seam, the in-flight task, and the
//! verdict landing on the row and the resolver.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool, Task};
use unmei_engine::{KeyCheck, KeyVerdict};

use super::DecisionChannel;
use crate::credentials::{self, CredentialsState, KeyRow};
use crate::RuntimeConfig;

/// The key-validation seam, if one is installed (production's `HttpKeyCheck`, or
/// a test fake). `None` means a commit cannot ping, so the key is trusted
/// without a network check — the offline posture, never a false rejection.
#[derive(Resource, Clone, Default)]
pub struct KeyCheckChannel {
    pub checker: Option<Arc<dyn KeyCheck>>,
}

/// A key-validation ping in flight. The task owns the key for the ping's
/// duration and returns it with the verdict; the resource holds no separate
/// copy. Dropping the task cancels the ping (a Clear, or a replacing commit).
#[derive(Resource, Default)]
pub struct PendingKeyCheck {
    pub task: Option<Task<(KeyVerdict, String)>>,
}

/// Spawn the ping for a committed key on the async compute pool, carrying the
/// key inside the future so the resource never holds a second copy. A missing
/// checker resolves [`KeyVerdict::Accepted`] without touching the network: with
/// no checker there is nothing to validate against, so the key is trusted — the
/// hermetic offline posture, and never a fabricated network failure.
pub(crate) fn spawn_key_check(
    checker: Option<Arc<dyn KeyCheck>>,
    key: String,
) -> Task<(KeyVerdict, String)> {
    AsyncComputeTaskPool::get().spawn(async move {
        let verdict = match checker {
            Some(checker) => checker.check(&key).await,
            None => KeyVerdict::Accepted,
        };
        (verdict, key)
    })
}

/// Resolve a committed key's live ping verdict: the row shows the verdict and,
/// on Accepted or Unreachable, the resolver is rebuilt from the key
/// (`docs/title-and-menu.md` § Decisions settings §3/§4.2). Rejected keeps the
/// file and leaves the resolver exactly as it was, so play never spams a `401`
/// per decision.
///
/// A verdict is applied only while the row is still `Checking…`: a Clear (or any
/// later row change) supersedes the in-flight ping, and the key is dropped with
/// the task.
pub(crate) fn poll_key_check(
    mut pending: ResMut<PendingKeyCheck>,
    factory: Res<super::DecisionFactory>,
    mut credentials: ResMut<CredentialsState>,
    mut channel: ResMut<DecisionChannel>,
    mut runtime_config: ResMut<RuntimeConfig>,
) {
    let resolved = match pending.task.as_mut() {
        Some(task) => block_on(poll_once(task)),
        None => return,
    };
    let Some((verdict, key)) = resolved else {
        return;
    };
    pending.task = None;

    // The row is no longer waiting on this verdict; drop it (and the key).
    if !credentials.row.is_checking() {
        return;
    }

    let fingerprint = credentials::fingerprint(&key);
    match verdict {
        KeyVerdict::Accepted => {
            credentials.row = KeyRow::Set { fingerprint };
        }
        KeyVerdict::Unreachable => {
            // The player is told where to look, never that the key is bad.
            log::warn!(
                "unmei: could not reach OpenRouter to check the stored key; it was kept unchecked"
            );
            credentials.row = KeyRow::SetUnchecked { fingerprint };
        }
        KeyVerdict::Rejected => {
            log::warn!("unmei: OpenRouter rejected the stored key; it stays on disk");
            credentials.row = KeyRow::Rejected { fingerprint };
            return;
        }
    }
    if let Some(build) = &factory.builder {
        let resolver = build(&key);
        channel.resolver = Some(resolver.clone());
        runtime_config.decisions = Some(resolver);
    }
}
