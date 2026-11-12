//! The installed tracing layer: which diagnostic events become lines.
//!
//! A reentrant capture is suppressed and counted rather than recursed, and a
//! contended queue rejects without waiting or retrying.

use std::cell::Cell;
use std::sync::Arc;

use bevy::log::tracing::{Event, Level as TracingLevel, Subscriber};
use bevy::log::tracing_subscriber::layer::{Context, Layer};

use super::capture::MonitorShared;
use super::render::render_line;

pub(super) struct CaptureLayer {
    pub(super) shared: Arc<MonitorShared>,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        capture(&self.shared, event);
    }
}

// Reentrancy guard: a formatter that itself logs must not recurse into capture.
thread_local! {
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
}

pub(super) struct CaptureGuard;

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURING.with(|capturing| capturing.set(false));
    }
}

/// Claim this thread's capture slot. Returns `false` when a capture is already
/// in progress, in which case the caller suppresses and counts the event.
pub(super) fn enter_capture() -> bool {
    CAPTURING.with(|capturing| !capturing.replace(true))
}

fn capture(shared: &Arc<MonitorShared>, event: &Event<'_>) {
    if !enter_capture() {
        // Reentrant capture is suppressed and counted, never recursed.
        shared.counters.add_lost(1);
        return;
    }
    let _guard = CaptureGuard;

    let Some(reservation) = shared.reserve() else {
        return;
    };

    let metadata = event.metadata();
    let level = to_log_level(*metadata.level());

    match render_line(level, metadata.target(), event) {
        Some(line) => reservation.admit(line),
        // A formatter that failed for a reason other than our size cap creates
        // no line; dropping the reservation counts one lost admission.
        None => drop(reservation),
    }
}

pub(super) fn to_log_level(level: TracingLevel) -> log::Level {
    match level {
        TracingLevel::ERROR => log::Level::Error,
        TracingLevel::WARN => log::Level::Warn,
        TracingLevel::INFO => log::Level::Info,
        TracingLevel::DEBUG => log::Level::Debug,
        TracingLevel::TRACE => log::Level::Trace,
    }
}
