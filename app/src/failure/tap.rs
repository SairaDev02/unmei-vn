//! A Keiko-only recorder for the funnel's lines (spec §9.2, §15 rule 28).
//!
//! The session log's text does not carry a failure row id, and `expect log …
//! row <n>` needs one. The funnel therefore hands every line it logs to this
//! recorder as well. When no take is running the recorder is empty and the
//! shipping player pays one relaxed atomic load, before any locking.

use std::sync::Mutex;
use std::sync::OnceLock;

use super::rows::FailureRow;

/// One line the funnel logged during a take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedLine {
    pub row: u32,
    pub level: String,
    pub text: String,
}

fn lines() -> &'static Mutex<Vec<RecordedLine>> {
    static LINES: OnceLock<Mutex<Vec<RecordedLine>>> = OnceLock::new();
    LINES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Start recording a take: forget every earlier line.
pub fn begin_take() {
    if let Ok(mut lines) = lines().lock() {
        lines.clear();
    }
}

/// Record one funnel line. Inert unless a take began and has not finished.
pub(crate) fn record(row: FailureRow, level: log::Level, text: String) {
    if !ARMED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if let Ok(mut lines) = lines().lock() {
        lines.push(RecordedLine {
            row: row.number(),
            level: match level {
                log::Level::Error => "error".to_string(),
                log::Level::Warn => "warn".to_string(),
                _ => "info".to_string(),
            },
            text,
        });
    }
}

static ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Arm the recorder for one take.
pub fn arm() {
    begin_take();
    ARMED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Disarm the recorder, return everything it saw and forget it, so a finished
/// take leaves nothing for the next one.
pub fn finish_take() -> Vec<RecordedLine> {
    ARMED.store(false, std::sync::atomic::Ordering::Relaxed);
    lines()
        .lock()
        .map(|mut lines| std::mem::take(&mut *lines))
        .unwrap_or_default()
}

/// The lines recorded since `cursor`, and the new cursor.
pub fn since(cursor: usize) -> (Vec<RecordedLine>, usize) {
    let lines = lines()
        .lock()
        .map(|lines| lines.clone())
        .unwrap_or_default();
    let from = cursor.min(lines.len());
    (lines[from..].to_vec(), lines.len())
}
