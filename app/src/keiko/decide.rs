//! The Keiko resolver (spec §6): a case's landings win, every other decision
//! takes the author's fallback with the player's own offline log line.
//!
//! Faults act through this same seam — `decision-error`, `decision-slow` and
//! `decision-hold` — so the shipping player gains no fault hook.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use unmei_engine::decision::{DecisionError, ResolvedDecision};
use unmei_engine::{DecisionConfig, DecisionResolver, DecisionsClient, StateValue};

use unmei_engine::keiko::{DecisionFault, Fault};

/// Why an unset decision landed where it did: the player's no-key posture.
const NO_CLIENT: &str = "no Decisions client";

/// One take's decision behaviour.
pub struct KeikoResolver {
    /// `<npc>.<question>` → landing action. The key is the question id the app
    /// hands the resolver, which is the question alone; a case's NPC prefix is
    /// validated but the seam identifies a point by its question.
    landings: HashMap<String, String>,
    faults: HashMap<String, Fault>,
    /// The real model, in live mode.
    live: Option<Arc<dyn DecisionResolver>>,
    /// Virtual milliseconds the take has let pass, for `decision-slow`.
    clock_ms: Arc<AtomicU64>,
    /// `decision-hold` releases: one flag per held point.
    released: Arc<Mutex<HashMap<String, bool>>>,
    /// Live mode marks the take's result nondeterministic.
    pub live_marked: Arc<AtomicBool>,
}

impl KeikoResolver {
    pub fn new(
        case_decisions: impl IntoIterator<Item = (String, String)>,
        faults: Vec<Fault>,
    ) -> Self {
        let mut fault_map = HashMap::new();
        for fault in faults {
            match &fault {
                Fault::DecisionError { point, .. }
                | Fault::DecisionSlow { point, .. }
                | Fault::DecisionHold { point } => {
                    fault_map.insert(question_of(point), fault);
                }
                _ => {}
            }
        }
        Self {
            landings: case_decisions.into_iter().collect(),
            faults: fault_map,
            live: None,
            clock_ms: Arc::new(AtomicU64::new(0)),
            released: Arc::new(Mutex::new(HashMap::new())),
            live_marked: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Live mode: land the case's own decisions and send the rest to the model.
    pub fn with_live(mut self, client: Arc<dyn DecisionResolver>) -> Self {
        self.live = Some(client);
        self
    }

    /// The take's virtual clock, so `decision-slow` waits on the take's own
    /// time rather than a wall clock.
    pub fn with_clock(mut self, clock_ms: Arc<AtomicU64>) -> Self {
        self.clock_ms = clock_ms;
        self
    }

    /// Release a `decision-hold` for one point.
    pub fn release(&self, point: &str) {
        if let Ok(mut released) = self.released.lock() {
            released.insert(question_of(point), true);
        }
    }

    fn landing_for(&self, config: &DecisionConfig, action: &str) -> String {
        if action == "fallback" {
            config.fallback.landing_key()
        } else {
            action.to_string()
        }
    }
}

fn question_of(point: &str) -> String {
    point
        .split_once('.')
        .map(|(_, question)| question.to_string())
        .unwrap_or_else(|| point.to_string())
}

impl DecisionResolver for KeikoResolver {
    fn resolve<'a>(
        &'a self,
        model: &'a str,
        question_id: &'a str,
        config: &'a DecisionConfig,
        state: &'a HashMap<String, StateValue>,
    ) -> Pin<Box<dyn Future<Output = ResolvedDecision> + Send + 'a>> {
        Box::pin(async move {
            if let Some(action) = self.landings.get(question_id) {
                let mut resolved = Self::model_answer(&self.landing_for(config, action));
                if let Some(Fault::DecisionError { kind, .. }) = self.faults.get(question_id) {
                    resolved = fallback(config, error_for(*kind));
                }
                return resolved;
            }

            if let Some(fault) = self.faults.get(question_id) {
                match fault {
                    Fault::DecisionError { kind, .. } => return fallback(config, error_for(*kind)),
                    Fault::DecisionSlow { duration, .. } => {
                        // The answer arrives once the take's *virtual* clock
                        // has moved by the fault's duration. The future returns
                        // Pending until then, so the app's own frame loop is
                        // what advances time — no thread waits and no wall
                        // clock is involved.
                        let target = self.clock_ms.load(Ordering::Relaxed)
                            + duration.as_nanos() as u64;
                        let clock = self.clock_ms.clone();
                        let landing = config.fallback.landing_key();
                        return std::future::poll_fn(move |context| {
                            if clock.load(Ordering::Relaxed) >= target {
                                std::task::Poll::Ready(Self::model_answer(&landing))
                            } else {
                                context.waker().wake_by_ref();
                                std::task::Poll::Pending
                            }
                        })
                        .await;
                    }
                    Fault::DecisionHold { .. } => {
                        // The answer is held until a `release-decision` step, so
                        // the overlay's `held` state is observable across waits.
                        let released = self.released.clone();
                        let question = question_id.to_string();
                        let landing = self.landing_for(
                            config,
                            self.landings
                                .get(question_id)
                                .map(String::as_str)
                                .unwrap_or("fallback"),
                        );
                        return std::future::poll_fn(move |context| {
                            let done = released
                                .lock()
                                .map(|released| released.get(&question).copied().unwrap_or(false))
                                .unwrap_or(false);
                            if done {
                                std::task::Poll::Ready(Self::model_answer(&landing))
                            } else {
                                context.waker().wake_by_ref();
                                std::task::Poll::Pending
                            }
                        })
                        .await;
                    }
                    _ => {}
                }
            }

            match &self.live {
                Some(client) => {
                    self.live_marked.store(true, Ordering::Relaxed);
                    client.resolve(model, question_id, config, state).await
                }
                None => fallback(config, DecisionError::Config(NO_CLIENT.to_string())),
            }
        })
    }
}

impl KeikoResolver {
    /// A landing the case chose is a real answer, not a fallback, so it takes
    /// no fallback diagnostic (K15).
    fn model_answer(action: &str) -> ResolvedDecision {
        ResolvedDecision {
            action: action.to_string(),
            model: None,
            confidence: None,
            fallback_reason: None,
        }
    }
}

fn fallback(config: &DecisionConfig, reason: DecisionError) -> ResolvedDecision {
    ResolvedDecision {
        action: config.fallback.landing_key(),
        model: None,
        confidence: None,
        fallback_reason: Some(reason),
    }
}

fn error_for(kind: DecisionFault) -> DecisionError {
    match kind {
        DecisionFault::Transport => DecisionError::Http,
        DecisionFault::Answer => DecisionError::Decode,
        DecisionFault::Refusal => DecisionError::Refused("keiko decision-error".to_string()),
    }
}

/// Build the real client for `--take --live`, from the environment key only.
pub fn live_client(endpoint: &str, key: &str) -> Arc<dyn DecisionResolver> {
    Arc::new(DecisionsClient::new(endpoint.to_string(), key))
}
