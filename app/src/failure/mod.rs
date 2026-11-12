//! The runtime failure posture: one vocabulary, one funnel, one player surface.
//!
//! Settled by [ADR 0008](../../../docs/adr/0008-player-first-failure-posture.md);
//! the row-by-row inventory is [`docs/runtime-hardening.md` § Runtime failure
//! posture](../../../docs/runtime-hardening.md#runtime-failure-posture).
//!
//! The discipline lives here, not at the call sites: a call site constructs a
//! [`Failure`] and hands it to [`report`]. `report` is the only code that picks
//! the log target, the level, and whether a [`Notice`] appears, so the inventory
//! table is the single place that choice lives.
//!
//! ```text
//! call site ──Failure::new(row, detail)──▶ report(failure, &mut Notice, &mut DefensiveHold)
//!                                             │
//!              one unmei::failure log line ◀─┤──▶ Notice slot, when the row says so
//!                                             └──▶ DefensiveHold, when the posture is Defensive
//! ```
//!
//! Each responsibility owns a module: [`rows`] the inventory table, [`funnel`]
//! the one report path and the hold, [`notice`] the transient line, [`session_log`]
//! the `<pack-dir>.log` sink, [`audio_setup`] the kira row-7 observer, and
//! [`error_screen`] the unloadable-pack screen. This root re-exports them and
//! holds the `LogPlugin` composition hook, so every `failure::<name>` path
//! callers already use stays the same.

mod audio_setup;
mod error_screen;
mod funnel;
mod notice;
mod rows;
mod session_log;

use std::sync::Arc;

use bevy::log::BoxedLayer;
use bevy::prelude::*;

use crate::RuntimeConfig;

pub use audio_setup::report_audio_setup_failure;
pub use audio_setup::{AudioSetupFailure, KIRA_SETUP_MESSAGE, KIRA_SETUP_TARGET};
pub use error_screen::{default_pack_hint, error_screen_input, StartupFailure, MAX_SCREEN_LINES};
pub use funnel::{report, report_runner_call, DefensiveHold, Failure};
pub use notice::{tick_notice, Notice, NOTICE_SECONDS};
pub use rows::{FailureRow, Posture};
pub use session_log::{
    file_layer, report_log_open_error, session_log_file_name, session_log_path_for, LogOpenError,
    SessionLog, SessionLogResult,
};

use audio_setup::kira_setup_layer;

/// Every failure logs once, under this one target.
pub const LOG_TARGET: &str = "unmei::failure";

/// `LogPlugin::custom_layer`: attach the session log when the entry point opened
/// one, install the hidden Monitor capture when the effective dev gate is on,
/// and always attach the row-7 watcher for kira's own setup warning. A missing
/// session log (an unwritable pack directory) leaves the console sink alone and
/// play continues; Monitor capture and the kira observer stay independent of it.
///
/// The gate is read here — before any startup system runs — from the
/// [`RuntimeConfig`] the entry point inserted before `DefaultPlugins`. Off, no
/// capture layer, pending queue or retained history is constructed.
pub fn app_log_layer(app: &mut App) -> Option<BoxedLayer> {
    let mut layers: Vec<BoxedLayer> = Vec::new();
    if let Some(log) = app.world().get_resource::<SessionLog>() {
        layers.push(file_layer(log.file()));
    }
    let failure = AudioSetupFailure::default();
    layers.push(kira_setup_layer(failure.slot()));
    app.insert_resource(failure);

    if crate::dev_mode_is_on(app.world().get_resource::<RuntimeConfig>()) {
        let capture =
            crate::monitor::MonitorCapture::install(Arc::new(crate::monitor::SystemClock::new()));
        layers.push(crate::monitor::capture_layer(&capture));
        app.insert_resource(capture);
    }

    Some(Box::new(layers))
}

/// A shared `log` capture, so the funnel's tests and the presentation tests in
/// `crate::tests` both assert on `unmei::failure` lines.
#[cfg(test)]
pub(crate) mod captures {
    use std::cell::RefCell;

    use super::funnel::{report, DefensiveHold, Failure};
    use super::notice::Notice;
    use super::rows::FailureRow;

    /// One captured record: level, target, rendered message.
    pub(crate) type Record = (log::Level, String, String);

    thread_local! {
        static RECORDS: RefCell<Vec<Record>> = const { RefCell::new(Vec::new()) };
    }

    struct CaptureLogger;

    impl log::Log for CaptureLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            RECORDS.with(|records| {
                records.borrow_mut().push((
                    record.level(),
                    record.target().to_string(),
                    record.args().to_string(),
                ));
            });
        }

        fn flush(&self) {}
    }

    /// Install the capture logger once per process.
    pub(crate) fn install() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let _ = log::set_boxed_logger(Box::new(CaptureLogger));
            log::set_max_level(log::LevelFilter::Trace);
        });
    }

    /// Drain the records captured on this thread.
    pub(crate) fn take() -> Vec<Record> {
        RECORDS.with(|records| std::mem::take(&mut *records.borrow_mut()))
    }

    /// Install the shared capture logger once per process.
    pub(crate) fn install_capture() {
        install();
    }

    /// Drain the records captured on this thread.
    pub(crate) fn capture_records() -> Vec<Record> {
        take()
    }

    /// Run `report` for one row and return the records it produced.
    pub(crate) fn report_once(
        row: FailureRow,
        notice: &mut Notice,
        hold: &mut DefensiveHold,
    ) -> Vec<Record> {
        capture_records(); // drop anything a previous test left on this thread
        let failure = if row == FailureRow::UnreadableSlot {
            Failure::with_short(row, "the slot is unreadable", "detail")
        } else {
            Failure::new(row, "detail")
        };
        report(failure, notice, hold);
        capture_records()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    /// The hidden Monitor capture is installed through the custom-layer hook
    /// only when the effective dev gate is on; off constructs no capture layer,
    /// pending queue or history.
    #[test]
    fn the_monitor_capture_layer_is_installed_only_when_dev_mode_is_on() {
        use crate::monitor::MonitorCapture;

        let mut off = App::new();
        off.insert_resource(
            crate::RuntimeConfig::new(PathBuf::from("pack"), None).with_dev_mode(false),
        );
        let _ = app_log_layer(&mut off);
        assert!(off.world().get_resource::<MonitorCapture>().is_none());

        let mut absent = App::new();
        let _ = app_log_layer(&mut absent);
        assert!(absent.world().get_resource::<MonitorCapture>().is_none());

        let mut on = App::new();
        on.insert_resource(
            crate::RuntimeConfig::new(PathBuf::from("pack"), None).with_dev_mode(true),
        );
        let _ = app_log_layer(&mut on);
        assert!(on.world().get_resource::<MonitorCapture>().is_some());
        // The kira watcher is mandatory and independent of the capture gate.
        assert!(on.world().get_resource::<AudioSetupFailure>().is_some());
        assert!(absent.world().get_resource::<AudioSetupFailure>().is_some());
    }
}
