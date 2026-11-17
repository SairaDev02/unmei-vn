//! A Keiko-only recorder for the funnel's lines (spec §9.2, §15 rule 28).
//!
//! The session log's text does not carry a failure row id, and `expect log …
//! row <n>` needs one. The funnel therefore hands every line it logs to this
//! inert recorder as well; when no take is running the recorder is empty and
//! the shipping player pays one mutex-free check.

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
    if let Ok(mut lines) = lines().lock() {
        if !lines.is_empty() || ARMED.load(std::sync::atomic::Ordering::Relaxed) {
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
}

static ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Arm the recorder for one take.
pub fn arm() {
    begin_take();
    ARMED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Disarm the recorder and return everything it saw.
pub fn finish_take() -> Vec<RecordedLine> {
    ARMED.store(false, std::sync::atomic::Ordering::Relaxed);
    let mut lines = lines()
        .lock()
        .map(|lines| lines.clone())
        .unwrap_or_default();
    lines.retain(|_| true);
    lines
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
