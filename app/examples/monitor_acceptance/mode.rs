//! The walk's four modes and the gated Decisions resolver it releases.

use crate::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    /// The fixture pack's full surface, reader and style walk.
    Walk,
    /// The same app parked on a defensive hold.
    Hold,
    /// A broken pack's startup error screen.
    Broken,
    /// The dev-mode gate itself, at whatever value this build's
    /// `cfg!(debug_assertions)` gives it.
    Gate,
}

impl Mode {
    pub(crate) fn parse() -> Mode {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            let value = arg
                .strip_prefix("--mode=")
                .map(str::to_string)
                .or_else(|| (arg == "--mode").then(|| args.next().unwrap_or_default()));
            if let Some(value) = value {
                return match value.as_str() {
                    "walk" => Mode::Walk,
                    "hold" => Mode::Hold,
                    "broken" => Mode::Broken,
                    "gate" => Mode::Gate,
                    other => panic!("unknown --mode {other:?}"),
                };
            }
        }
        Mode::Walk
    }
}

/// Resolve the effective dev mode exactly as `unmei-play --dev` does: through
/// the shipping parser's `Launch::dev_mode()`.
pub(crate) fn parsed_dev_mode() -> bool {
    match unmei_app::launch::parse([std::ffi::OsString::from("--dev")]) {
        Ok(unmei_app::launch::LaunchRequest::Run(launch)) => launch.dev_mode(),
        other => panic!("`--dev` did not parse as a run: {other:?}"),
    }
}

/// A resolver that parks the decide point until the walk releases it.
///
/// This is the acceptance walk's stand-in for the network: the `decide` point
/// really parks, the drawer really opens over a live decision wait, and the
/// release really lands the answer — all without a key, a socket or OpenRouter.
#[derive(Clone)]
pub(crate) struct GatedResolver {
    pub(crate) receiver: Arc<Mutex<Option<tokio::sync::oneshot::Receiver<ResolvedDecision>>>>,
    pub(crate) sender: Arc<Mutex<Option<tokio::sync::oneshot::Sender<ResolvedDecision>>>>,
}

impl GatedResolver {
    pub(crate) fn new() -> GatedResolver {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        GatedResolver {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            sender: Arc::new(Mutex::new(Some(sender))),
        }
    }

    pub(crate) fn release(&self, action: &str) {
        if let Some(sender) = self.sender.lock().expect("the gate lock").take() {
            let _ = sender.send(ResolvedDecision {
                action: action.to_string(),
                model: None,
                confidence: None,
                fallback_reason: None,
            });
        }
    }
}

impl DecisionResolver for GatedResolver {
    fn resolve<'a>(
        &'a self,
        _model: &'a str,
        _question_id: &'a str,
        config: &'a DecisionConfig,
        _state: &'a HashMap<String, StateValue>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ResolvedDecision> + Send + 'a>> {
        let receiver = self.receiver.lock().expect("the gate lock").take();
        let fallback = config.choice_fallback().unwrap_or_default().to_string();
        Box::pin(async move {
            // A dropped sender or an already-taken receiver is the walk's
            // "the task never completed": the engine's own transport-agnostic
            // fallback variant, so the app reports the real row.
            let landed = match receiver {
                Some(receiver) => receiver.await.ok(),
                None => None,
            };
            landed.unwrap_or(ResolvedDecision {
                action: fallback,
                model: None,
                confidence: None,
                fallback_reason: Some(DecisionError::Task),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// The walk harness
// ---------------------------------------------------------------------------
