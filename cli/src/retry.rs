//! The one typed attempt driver both authoring commands share (drafting
//! contract §10).
//!
//! A command keeps its own session: prompt context, human feedback and its
//! complete attempt stay command-specific. What lives here is only the policy
//! both commands agree on — three attempts, the per-regeneration history, the
//! corrections the next attempt sees, and the terminal/retry classification of
//! a provider error.

use crate::provider::ProviderError;

/// The fixed retry policy: at most three attempts per regeneration.
pub const RETRY_LIMIT: usize = 3;

/// The one persistent corrections owner. A command session embeds it; the
/// driver replaces it with the latest rejection's errors, carries every
/// exhausted error into the next regeneration and clears it on success.
#[derive(Debug, Default)]
pub struct RetryState {
    corrections: Vec<String>,
}

impl RetryState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The corrections currently queued for the next attempt. The first
    /// attempt of a regeneration sees the previous round's exhausted errors
    /// here; later attempts see the latest rejection's.
    pub fn corrections(&self) -> &[String] {
        &self.corrections
    }

    fn replace(&mut self, corrections: Vec<String>) {
        self.corrections = corrections;
    }

    fn clear(&mut self) {
        self.corrections.clear();
    }
}

/// One complete attempt's report: the model's raw reply, unchanged, plus the
/// typed outcome the command's full gates produced from it.
#[derive(Debug)]
pub struct AttemptReport<T> {
    pub raw: String,
    pub outcome: ReportOutcome<T>,
}

/// The report's outcome: a payload that passed every gate, the corrections
/// eligible for another content attempt, or a staged-preparation failure that
/// ends the run terminally.
#[derive(Debug)]
pub enum ReportOutcome<T> {
    Accepted(T),
    Rejected(Vec<String>),
    /// The candidate could not be staged, installed or parsed. Never a
    /// correction: the driver stops without spending another attempt.
    PreparationFailed(String),
}

/// A complete command-specific attempt: prompt assembly, provider interaction,
/// extraction and the command's full gates, statically dispatched over the
/// concrete command adapter. It never writes an author's file.
pub trait Attempt {
    /// The validated payload a successful attempt produces.
    type Payload;

    fn run(
        &mut self,
        corrections: &[String],
    ) -> impl std::future::Future<Output = Result<AttemptReport<Self::Payload>, ProviderError>>;
}

/// A driver success: the validated payload and the 1-based attempt it landed on.
#[derive(Debug)]
pub struct RetryOutcome<T> {
    pub payload: T,
    pub attempt: usize,
}

/// Why the driver stopped without an accepted payload. A provider error is
/// terminal wherever it lands — even after earlier content rejections — and is
/// never converted into corrections or exhaustion. A preparation failure is
/// terminal the same way: no further attempt, no corrections, no exhaustion.
#[derive(Debug, thiserror::Error)]
pub enum RetryFailure {
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    /// The verbatim staged-preparation message; the caller adds the
    /// candidate-not-installed-or-validated context.
    #[error("{0}")]
    Preparation(String),
    #[error("all {RETRY_LIMIT} attempts failed validation")]
    Exhausted {
        /// Every raw reply, in encounter order, including duplicates.
        raw_outputs: Vec<String>,
        /// Every rejection's corrections, in encounter order, including
        /// duplicates.
        errors: Vec<String>,
    },
}

/// Run one regeneration: up to [`RETRY_LIMIT`] complete attempts, feeding each
/// rejection's corrections into the next attempt. On success the queued
/// corrections are cleared; on exhaustion they become every error this
/// regeneration saw, so interactive feedback carries them forward.
///
/// A preparation failure returns immediately ([`RetryFailure::Preparation`]):
/// it leaves the queued corrections as they were and stops before another
/// attempt runs, so it can never become a correction, an exhaustion, or
/// interactive feedback.
pub async fn drive<A: Attempt>(
    attempt: &mut A,
    state: &mut RetryState,
) -> Result<RetryOutcome<A::Payload>, RetryFailure> {
    let mut raw_outputs: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for number in 1..=RETRY_LIMIT {
        let report = attempt.run(state.corrections()).await?;
        raw_outputs.push(report.raw);
        match report.outcome {
            ReportOutcome::Accepted(payload) => {
                state.clear();
                return Ok(RetryOutcome {
                    payload,
                    attempt: number,
                });
            }
            ReportOutcome::Rejected(corrections) => {
                errors.extend(corrections.iter().cloned());
                state.replace(corrections);
            }
            ReportOutcome::PreparationFailed(message) => {
                return Err(RetryFailure::Preparation(message));
            }
        }
    }

    state.replace(errors.clone());
    Err(RetryFailure::Exhausted {
        raw_outputs,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ShapeProblem, TransportFailure};
    use std::collections::VecDeque;

    /// A scripted adapter: each `run` pops the next report and records the
    /// corrections it was handed, so the test can see both result and state.
    struct ScriptedAttempt {
        script: VecDeque<Result<AttemptReport<u32>, ProviderError>>,
        seen_corrections: Vec<Vec<String>>,
    }

    impl ScriptedAttempt {
        fn new(script: Vec<Result<AttemptReport<u32>, ProviderError>>) -> Self {
            ScriptedAttempt {
                script: VecDeque::from(script),
                seen_corrections: Vec::new(),
            }
        }
    }

    impl Attempt for ScriptedAttempt {
        type Payload = u32;

        async fn run(
            &mut self,
            corrections: &[String],
        ) -> Result<AttemptReport<u32>, ProviderError> {
            self.seen_corrections.push(corrections.to_vec());
            self.script
                .pop_front()
                .unwrap_or(Err(ProviderError::Shape(ShapeProblem::NotJson)))
        }
    }

    /// A terminal provider failure on the first call. A sentinel later success
    /// sits behind it; if the driver ever consults it, `sentinel_served` flips.
    struct ProviderFailAttempt {
        sentinel_served: bool,
    }

    impl Attempt for ProviderFailAttempt {
        type Payload = u32;

        async fn run(
            &mut self,
            _corrections: &[String],
        ) -> Result<AttemptReport<u32>, ProviderError> {
            if self.sentinel_served {
                return Ok(AttemptReport {
                    raw: "sentinel".into(),
                    outcome: ReportOutcome::Accepted(99),
                });
            }
            Err(ProviderError::Http(TransportFailure::Connect))
        }
    }

    fn accepted(raw: &str, payload: u32) -> Result<AttemptReport<u32>, ProviderError> {
        Ok(AttemptReport {
            raw: raw.into(),
            outcome: ReportOutcome::Accepted(payload),
        })
    }

    fn rejected(raw: &str, errors: &[&str]) -> Result<AttemptReport<u32>, ProviderError> {
        Ok(AttemptReport {
            raw: raw.into(),
            outcome: ReportOutcome::Rejected(errors.iter().map(|e| e.to_string()).collect()),
        })
    }

    fn provider(failure: TransportFailure) -> Result<AttemptReport<u32>, ProviderError> {
        Err(ProviderError::Http(failure))
    }

    fn preparation_failed(raw: &str, message: &str) -> Result<AttemptReport<u32>, ProviderError> {
        Ok(AttemptReport {
            raw: raw.into(),
            outcome: ReportOutcome::PreparationFailed(message.into()),
        })
    }

    /// The payload and the 1-based attempt it landed on are the driver's
    /// success result, and success clears the corrections.
    #[tokio::test]
    async fn accepted_result_reports_payload_and_attempt_and_clears_state() {
        let mut attempt = ScriptedAttempt::new(vec![
            rejected("raw one", &["fix one"]),
            accepted("raw two", 7),
        ]);
        let mut state = RetryState::new();

        let outcome = drive(&mut attempt, &mut state).await.unwrap();

        assert_eq!(outcome.payload, 7);
        assert_eq!(outcome.attempt, 2);
        assert!(
            state.corrections().is_empty(),
            "success clears the queued corrections: {:?}",
            state.corrections()
        );
        assert_eq!(
            attempt.seen_corrections,
            vec![Vec::<String>::new(), vec!["fix one".to_string()]],
            "the first attempt starts clean; the second sees the latest rejection"
        );
    }

    /// Each rejection replaces the corrections; the next attempt never sees a
    /// stale or merged list.
    #[tokio::test]
    async fn each_attempt_sees_only_the_latest_rejection() {
        let mut attempt = ScriptedAttempt::new(vec![
            rejected("raw one", &["first problem"]),
            rejected("raw two", &["second problem"]),
            accepted("raw three", 3),
        ]);
        let mut state = RetryState::new();

        let outcome = drive(&mut attempt, &mut state).await.unwrap();

        assert_eq!(outcome.attempt, 3);
        assert_eq!(
            attempt.seen_corrections[2],
            vec!["second problem".to_string()],
            "the latest rejection replaces the earlier one"
        );
    }

    /// A preparation failure after a content rejection stops the driver
    /// immediately: it is neither exhausted nor retried, it leaves the queued
    /// corrections untouched, and no later attempt runs.
    #[tokio::test]
    async fn preparation_failure_is_terminal_and_leaves_corrections_untouched() {
        let mut attempt = ScriptedAttempt::new(vec![
            rejected("raw one", &["first problem"]),
            preparation_failed("raw two", "staged parse failed"),
            accepted("never served", 9),
        ]);
        let mut state = RetryState::new();

        let failure = drive(&mut attempt, &mut state).await.unwrap_err();

        match failure {
            RetryFailure::Preparation(message) => assert_eq!(message, "staged parse failed"),
            other => panic!("expected a preparation failure, got {other:?}"),
        }
        assert_eq!(
            state.corrections(),
            ["first problem".to_string()],
            "the preparation failure neither erases nor extends the corrections"
        );
        assert_eq!(
            attempt.seen_corrections.len(),
            2,
            "the driver stops at the preparation failure instead of spending another attempt"
        );
        assert_eq!(
            attempt.script.len(),
            1,
            "the sentinel success behind the failure is never consulted"
        );
    }

    /// Exhaustion keeps every raw reply and every error in encounter order,
    /// duplicates included, and leaves them queued for human feedback.
    #[tokio::test]
    async fn exhaustion_preserves_all_raw_replies_and_errors_in_order() {
        let mut attempt = ScriptedAttempt::new(vec![
            rejected("raw one", &["same", "first"]),
            rejected("raw two", &["same"]),
            rejected("raw three", &["second"]),
        ]);
        let mut state = RetryState::new();

        let failure = drive(&mut attempt, &mut state).await.unwrap_err();

        match failure {
            RetryFailure::Exhausted {
                raw_outputs,
                errors,
            } => {
                assert_eq!(raw_outputs, vec!["raw one", "raw two", "raw three"]);
                assert_eq!(
                    errors,
                    vec!["same", "first", "same", "second"],
                    "errors stay in encounter order with duplicates"
                );
            }
            other => panic!("expected exhaustion, got {other:?}"),
        }
        assert_eq!(
            state.corrections(),
            ["same", "first", "same", "second"],
            "all exhausted errors stay queued for the next regeneration"
        );
    }

    /// A provider error stops immediately even after a content rejection: the
    /// later sentinel success is never attempted, exhaustion is not reported,
    /// and the latest rejection's corrections are left untouched.
    #[tokio::test]
    async fn provider_failure_is_terminal_after_a_rejection() {
        let mut attempt = ScriptedAttempt::new(vec![
            rejected("raw one", &["fix one"]),
            provider(TransportFailure::Connect),
            accepted("sentinel", 99),
        ]);
        let mut state = RetryState::new();

        let failure = drive(&mut attempt, &mut state).await.unwrap_err();

        assert!(
            matches!(
                failure,
                RetryFailure::Provider(ProviderError::Http(TransportFailure::Connect))
            ),
            "the provider error is surfaced as-is: {failure:?}"
        );
        assert_eq!(
            attempt.seen_corrections.len(),
            2,
            "the sentinel attempt is never consulted"
        );
        assert_eq!(
            state.corrections(),
            ["fix one"],
            "the latest rejection's corrections are preserved, not folded into exhaustion"
        );
    }

    /// A dedicated provider-failing adapter: its sentinel success is forbidden
    /// and never served.
    #[tokio::test]
    async fn provider_fail_attempt_never_reaches_its_sentinel() {
        let mut attempt = ProviderFailAttempt {
            sentinel_served: false,
        };
        let mut state = RetryState::new();

        let failure = drive(&mut attempt, &mut state).await.unwrap_err();

        assert!(
            matches!(
                failure,
                RetryFailure::Provider(ProviderError::Http(TransportFailure::Connect))
            ),
            "{failure:?}"
        );
        assert!(
            !attempt.sentinel_served,
            "the unreachable sentinel success must never be served"
        );
    }

    /// Corrections carried in from a previous exhausted round reach the first
    /// attempt of the next regeneration, and a new exhaustion reports only
    /// that regeneration's errors — the carried ones are not counted twice.
    #[tokio::test]
    async fn carried_corrections_reach_the_first_attempt_and_are_not_reaccumulated() {
        let mut attempt = ScriptedAttempt::new(vec![
            rejected("raw one", &["new one"]),
            rejected("raw two", &["new two"]),
            rejected("raw three", &["new three"]),
        ]);
        let mut state = RetryState::new();
        state.replace(vec!["carried".to_string()]);

        let failure = drive(&mut attempt, &mut state).await.unwrap_err();

        assert_eq!(
            attempt.seen_corrections[0],
            vec!["carried".to_string()],
            "the first attempt of the round sees the carried corrections"
        );
        match failure {
            RetryFailure::Exhausted { errors, .. } => assert_eq!(
                errors,
                vec!["new one", "new two", "new three"],
                "only this regeneration's errors are accumulated"
            ),
            other => panic!("expected exhaustion, got {other:?}"),
        }
        assert_eq!(state.corrections(), ["new one", "new two", "new three"]);
    }

    /// The fixed policy is three attempts: a fourth scripted success is never
    /// consulted.
    #[tokio::test]
    async fn the_policy_stops_after_three_rejections() {
        let mut attempt = ScriptedAttempt::new(vec![
            rejected("raw one", &["one"]),
            rejected("raw two", &["two"]),
            rejected("raw three", &["three"]),
            accepted("sentinel", 4),
        ]);
        let mut state = RetryState::new();

        let failure = drive(&mut attempt, &mut state).await.unwrap_err();

        assert!(
            matches!(failure, RetryFailure::Exhausted { .. }),
            "{failure:?}"
        );
        assert_eq!(attempt.seen_corrections.len(), RETRY_LIMIT);
    }
}
