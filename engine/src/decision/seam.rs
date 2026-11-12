//! The two injection points the runtime resolves through — a parked Decisions
//! question and a committed API key — plus the author-fallback posture they
//! share (ADR 0009, ADR 0010).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::content::{DecisionConfig, StateValue};

use super::{Decision, DecisionError};

/// The action the script will jump to, plus why. Model answers carry the
/// requested pin when the response matched it, otherwise
/// [`UNVERIFIED_MODEL`](super::UNVERIFIED_MODEL); the network client applies that restriction before
/// logging or returning. Fallbacks carry the error that forced them.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedDecision {
    pub action: String,
    pub model: Option<String>,
    pub confidence: Option<f64>,
    /// `Some(error)` iff the author fallback was used: the error that forced it,
    /// **not** its `Display` text. Callers classify by variant, and no variant
    /// can carry response-controlled text (every payload is authored, or one of
    /// the wire's fixed type names or a status code), so the reason cannot leak
    /// a credential into a Notice, the log or the Monitor.
    pub fallback_reason: Option<DecisionError>,
}

impl ResolvedDecision {
    pub fn is_fallback(&self) -> bool {
        self.fallback_reason.is_some()
    }
}

/// Apply the author-fallback posture: a usable model answer passes through,
/// any error resolves to `config.fallback`. Always returns an action, so the
/// runner can always land somewhere.
pub fn resolve_or_fallback(
    config: &DecisionConfig,
    result: Result<Decision, DecisionError>,
) -> ResolvedDecision {
    match result {
        Ok(decision) => ResolvedDecision {
            action: decision.action,
            model: Some(decision.model),
            confidence: decision.confidence,
            fallback_reason: None,
        },
        Err(error) => ResolvedDecision {
            action: config.fallback.landing_key(),
            model: None,
            confidence: None,
            fallback_reason: Some(error),
        },
    }
}

/// The injection point through which the runtime asks for a parked question's
/// resolution, **author fallback already applied**.
///
/// One call per parked question, returning the [`ResolvedDecision`] the runner
/// lands on. It abstracts the *resolved landing*, not the transport: the one
/// real implementation is `DecisionsClient`, and tests implement it locally to
/// drive the app's decision pipeline without a network (ADR 0009). The future is
/// hand-boxed, so no `async-trait` dependency is needed.
pub trait DecisionResolver: Send + Sync + 'static {
    fn resolve<'a>(
        &'a self,
        model: &'a str,
        question_id: &'a str,
        config: &'a DecisionConfig,
        state: &'a HashMap<String, StateValue>,
    ) -> Pin<Box<dyn Future<Output = ResolvedDecision> + Send + 'a>>;
}

/// The verdict of one key-validation ping (`docs/title-and-menu.md`
/// § Decisions settings §3). A network outage is never called a bad key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyVerdict {
    /// `200` — OpenRouter accepts the key.
    Accepted,
    /// `401`/`403` — OpenRouter rejects the key.
    Rejected,
    /// Any other status, a transport error, or a timeout.
    Unreachable,
}

/// The injection point through which the runtime pings a committed key, mirroring
/// [`DecisionResolver`] (ADR 0009's pattern; ADR 0010 records it as the spec's
/// one engine addition). One call per commit; the one real implementation is
/// `HttpKeyCheck`, and tests implement it locally to drive `Checking…` →
/// Set/Invalid in-process.
pub trait KeyCheck: Send + Sync + 'static {
    fn check<'a>(&'a self, key: &'a str) -> Pin<Box<dyn Future<Output = KeyVerdict> + Send + 'a>>;
}
