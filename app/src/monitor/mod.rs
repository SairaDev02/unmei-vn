//! Hidden, bounded Monitor capture.
//!
//! This module is the production capture path behind the developer gate from
//! [`crate::RuntimeConfig`]: a [`tracing`](bevy::log::tracing) layer installed
//! through [`LogPlugin::custom_layer`](bevy::log::LogPlugin) that turns each
//! upstream-admitted diagnostic event into at most one escaped, size-bounded
//! logical line, queues it in a bounded pending buffer, and lets the Bevy app
//! ingest a fixed number of lines per update into one launch-wide retained
//! history.
//!
//! It is deliberately **not** a panel: the F3 drawer and its reader live in
//! `crate::drawer` and `crate::monitor_reader` and read this module's history.
//! What ships here is capture, admission, retention, and the three
//! loss/eviction/truncation counters, all verifiable headlessly.
//!
//! Contract home: `docs/runtime-hardening.md` → Monitor → Log stream. The three
//! fixed budgets are [`PENDING_CAPACITY`] (256), [`RETAINED_CAPACITY`] (1,000)
//! and [`MAX_LINE_BYTES`] (8,192); [`INGEST_PER_UPDATE`] bounds per-frame work.

mod capture;
mod clock;
mod counters;
mod history;
mod layer;
mod line;
mod render;
#[cfg(test)]
mod tests;

pub use capture::{capture_layer, MonitorCapture};
pub use clock::{MonitorClock, SystemClock};
pub use counters::MonitorCounters;
pub use history::{ingest_capture, MonitorHistory};
pub use line::MonitorLine;

/// Pending capture slots: in-flight formatter reservations plus completed lines
/// waiting for the app to ingest them.
pub const PENDING_CAPACITY: usize = 256;

/// Retained history lines: the one authoritative cap both future display views
/// share. 1,000 × 8,192 bytes = 7.8125 MiB.
pub const RETAINED_CAPACITY: usize = 1000;

/// At most this many pending lines move into retained history per app update.
/// 128 × 8,192 bytes = the 1 MiB/update ingestion payload budget.
pub const INGEST_PER_UPDATE: usize = 128;

/// The complete escaped UTF-8 line bound, prefix and `[truncated]` suffix
/// included.
pub const MAX_LINE_BYTES: usize = 8192;
