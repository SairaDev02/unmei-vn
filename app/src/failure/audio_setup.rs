//! Row 7's source: the observer for `bevy_kira_audio`'s own backend setup
//! warning, and the funnel hand-off that puts it under `unmei::failure`.

use std::sync::{Arc, Mutex};

use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::{Event, Subscriber};
use bevy::log::tracing_subscriber::layer::{Context, Layer};
use bevy::log::BoxedLayer;
use bevy::prelude::*;

use super::funnel::{report, DefensiveHold, Failure};
use super::notice::Notice;
use super::rows::FailureRow;

/// bevy_kira_audio logs its own backend setup failure under this target. The
/// kira manager lives in that crate's private `AudioOutput`, so the ECS cannot
/// see the failure any other way; the app's log layer observes the warning and
/// hands it to the funnel, which is what puts row 7 under `unmei::failure`.
///
/// The target and message are pinned by `the_kira_setup_layer_captures_the_upstream_warning`
/// and must move with the upstream line in
/// `bevy_kira_audio-0.26.0/src/audio_output.rs` (`warn!("Failed to setup
/// audio: {:?}", setup_error)`).
pub const KIRA_SETUP_TARGET: &str = "bevy_kira_audio::audio_output";
/// The opening words of that warning.
pub const KIRA_SETUP_MESSAGE: &str = "Failed to setup audio";

/// The shared slot the log layer writes and [`report_audio_setup_failure`]
/// reads. `None` means either that the layer never saw a setup failure or that
/// no log plugin is installed at all (a headless test).
#[derive(Resource, Debug, Clone, Default)]
pub struct AudioSetupFailure(Arc<Mutex<Option<String>>>);

impl AudioSetupFailure {
    /// The captured setup-failure message, if there was one.
    pub fn message(&self) -> Option<String> {
        self.0.lock().ok().and_then(|slot| slot.clone())
    }

    /// The slot the log layer writes, shared rather than copied.
    pub(super) fn slot(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.0)
    }
}

/// Report a captured audio-backend failure once (row 7): a `warn!` under
/// [`LOG_TARGET`] with no Notice. Play continues silently, and "the next wait"
/// is already reached because the failing setup never blocked the runner.
///
/// [`LOG_TARGET`]: super::LOG_TARGET
pub fn report_audio_setup_failure(
    failure: Option<Res<AudioSetupFailure>>,
    mut notice: ResMut<Notice>,
    mut hold: ResMut<DefensiveHold>,
) {
    let Some(message) = failure.and_then(|failure| failure.message()) else {
        return;
    };
    report(
        Failure::new(FailureRow::AudioDevice, message),
        &mut notice,
        &mut hold,
    );
}

/// The tracing layer that captures kira's one setup-failure warning.
pub(super) fn kira_setup_layer(failure: Arc<Mutex<Option<String>>>) -> BoxedLayer {
    Box::new(KiraSetupLayer { failure })
}

struct KiraSetupLayer {
    failure: Arc<Mutex<Option<String>>>,
}

impl<S: Subscriber> Layer<S> for KiraSetupLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        if event.metadata().target() != KIRA_SETUP_TARGET {
            return;
        }
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let Some(message) = visitor.message else {
            return;
        };
        if !message.starts_with(KIRA_SETUP_MESSAGE) {
            return;
        }
        if let Ok(mut slot) = self.failure.lock() {
            if slot.is_none() {
                *slot = Some(message);
            }
        }
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::captures::{capture_records, install_capture};
    use super::super::funnel::DefensiveHold;
    use super::super::notice::Notice;
    use crate::failure::LOG_TARGET;
    use bevy::log::tracing;
    use bevy::log::tracing_subscriber;
    use bevy::log::tracing_subscriber::prelude::*;
    /// Row 7 reaches the funnel through the captured kira warning: one `warn!`
    /// under `unmei::failure`, no Notice.
    #[test]
    fn a_captured_audio_setup_failure_reports_row_7_once_without_a_notice() {
        use bevy::ecs::system::RunSystemOnce;

        install_capture();
        let mut app = App::new();
        app.insert_resource(AudioSetupFailure(Arc::new(Mutex::new(Some(
            "Failed to setup audio: NoDevice".to_string(),
        )))))
        .init_resource::<Notice>()
        .init_resource::<DefensiveHold>();

        capture_records();
        app.world_mut()
            .run_system_once(report_audio_setup_failure)
            .expect("the startup system runs once");

        let records = capture_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, log::Level::Warn);
        assert_eq!(records[0].1, LOG_TARGET);
        assert!(records[0].2.starts_with("couldn't start audio"));
        assert!(records[0].2.contains("Failed to setup audio"));
        assert!(!app.world().resource::<Notice>().is_showing());
    }
    /// The layer that carries row 7 must key on bevy_kira_audio's setup warning
    /// and nothing else — a play failure or another target is ignored, so the
    /// one row-7 warn cannot be spoofed or duplicated.
    #[test]
    fn the_kira_setup_layer_captures_the_upstream_warning() {
        let slot = Arc::new(Mutex::new(None::<String>));
        let subscriber = tracing_subscriber::registry().with(kira_setup_layer(Arc::clone(&slot)));

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "bevy_kira_audio::audio_output", "Failed to setup audio: {:?}", "NoDevice");
            tracing::warn!(target: "bevy_kira_audio::audio_output", "Failed to play sound due to {:?}", "boom");
            tracing::warn!(target: "some::other", "Failed to setup audio: nope");
        });

        assert_eq!(KIRA_SETUP_TARGET, "bevy_kira_audio::audio_output");
        assert_eq!(
            slot.lock().expect("lock").clone(),
            Some("Failed to setup audio: \"NoDevice\"".to_string())
        );
    }
}
