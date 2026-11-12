//! The installed panic hook, end to end: a real panic writes one record through
//! `log` under [`unmei_app::panic_log::LOG_TARGET`], and the process still
//! unwinds.
//!
//! This binary holds the one test that installs the process-global hook and
//! logger, so it neither depends on nor disturbs any other test's logging.

use std::sync::{Arc, Mutex};

/// One captured record: level, target, rendered message.
type Record = (log::Level, String, String);

struct CaptureLogger(Arc<Mutex<Vec<Record>>>);

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        self.0.lock().expect("record lock").push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
    }

    fn flush(&self) {}
}

#[test]
fn a_panic_is_recorded_through_log_and_the_thread_still_unwinds() {
    let records = Arc::new(Mutex::new(Vec::new()));
    log::set_boxed_logger(Box::new(CaptureLogger(Arc::clone(&records))))
        .expect("this binary installs no other logger");
    log::set_max_level(log::LevelFilter::Trace);

    unmei_app::panic_log::install();

    // A caught panic is `Err` only while unwinding; an aborting hook would end
    // the process here instead.
    let caught = std::panic::catch_unwind(|| panic!("load failed: {}", 3));
    assert!(caught.is_err(), "a panic must still unwind");

    // Release the lock before asserting: a failed assertion panics, and the
    // installed hook logs again, so holding the lock here would deadlock the
    // test instead of failing it.
    let records: Vec<Record> = records.lock().expect("record lock").clone();
    assert_eq!(records.len(), 1, "one record per panic: {records:?}");
    assert_eq!(records[0].0, log::Level::Error);
    assert_eq!(records[0].1, unmei_app::panic_log::LOG_TARGET);
    let text = &records[0].2;
    assert!(
        text.starts_with("thread '") && text.contains("' panicked at "),
        "the record names the thread and the site: {text:?}"
    );
    assert!(
        text.contains("panic_record.rs:"),
        "the record carries the panic's own file and line: {text:?}"
    );
    assert!(
        text.ends_with(": load failed: 3"),
        "the record ends with the panic message: {text:?}"
    );
}
