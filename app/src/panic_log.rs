//! The panic record: a panic's message and site written through `log`, so the
//! panic reaches `<pack-dir>.log` — the file a bug report carries — and not only
//! whatever a terminal happened to show. Its rules are the failure posture's
//! panic row ([`docs/runtime-hardening.md` § Runtime failure
//! posture](../../docs/runtime-hardening.md#runtime-failure-posture)).
//!
//! The hook is installed by the binary after [`crate::composition::shipping_app`]
//! returns — by then `LogPlugin` has installed the subscriber — and it chains the
//! hook it replaced, so the default `stderr` report and the unwind are unchanged.
//! It adds no layer and touches no subscriber state, so the Monitor capture
//! receives the record like any other `unmei::` event.

use std::panic::PanicHookInfo;

/// The record's target: a `unmei::` namespace the Monitor's display filter
/// projects, and a distinct name from the failure funnel's `unmei::failure`
/// because a panic is not one of its enumerated rows.
pub const LOG_TARGET: &str = "unmei::panic";

/// The thread name the record uses for a thread that was never named, matching
/// the standard hook's own wording.
const UNNAMED_THREAD: &str = "<unnamed>";

/// One panic as one record line: the thread name, the site and the message.
///
/// A panic without a site (`resume_unwind` of a payload, which never printed
/// one) drops the `at` clause rather than inventing a location. Newlines in the
/// message are escaped so the record stays one line in a sink that writes one
/// record per line.
pub fn panic_text(message: &str, site: Option<(&str, u32, u32)>, thread: &str) -> String {
    let mut record = format!("thread '{thread}' panicked");
    if let Some((file, line, column)) = site {
        record.push_str(&format!(" at {file}:{line}:{column}"));
    }
    record.push_str(": ");
    record.push_str(&message.replace('\r', "\\r").replace('\n', "\\n"));
    record
}

/// Record one panic through `sink`. Separated from [`install`] so the recording
/// path is asserted with a test sink instead of a process-global hook.
pub fn record_panic(
    sink: &dyn Fn(&str),
    message: &str,
    site: Option<(&str, u32, u32)>,
    thread: &str,
) {
    sink(&panic_text(message, site, thread));
}

/// Install the panic record ahead of the hook already in place.
///
/// Call this after the app is built, so the subscriber `LogPlugin` installed is
/// already live. The record goes through `log::error!`, and the previous hook
/// still runs, so the standard `stderr` report and the unwind are unchanged.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = payload_text(info);
        let site = info
            .location()
            .map(|location| (location.file(), location.line(), location.column()));
        let thread = std::thread::current();
        record_panic(
            &log_record,
            &message,
            site,
            thread.name().unwrap_or(UNNAMED_THREAD),
        );
        previous(info);
    }));
}

fn log_record(text: &str) {
    log::error!(target: LOG_TARGET, "{text}");
}

/// A panic payload is `&str` or `String` for every `panic!`/`unwrap` and may be
/// any other type through `panic_any`, which has no printable form here.
fn payload_text(info: &PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_record_names_the_thread_the_site_and_the_message() {
        assert_eq!(
            panic_text(
                "called `Option::unwrap()` on a `None` value",
                Some(("app/src/main.rs", 42, 5)),
                "main"
            ),
            "thread 'main' panicked at app/src/main.rs:42:5: called `Option::unwrap()` on a `None` value"
        );
    }

    #[test]
    fn a_panic_without_a_site_or_a_thread_name_still_reads_as_one_record() {
        assert_eq!(
            panic_text("explicit panic", None, UNNAMED_THREAD),
            "thread '<unnamed>' panicked: explicit panic"
        );
    }

    #[test]
    fn a_multi_line_message_stays_on_one_record() {
        assert_eq!(
            panic_text(
                "load failed\r\nat line 3",
                Some(("engine/src/pack.rs", 7, 9)),
                "main"
            ),
            "thread 'main' panicked at engine/src/pack.rs:7:9: load failed\\r\\nat line 3"
        );
    }

    #[test]
    fn the_installed_sink_logs_the_record_under_the_panic_target_at_error_level() {
        use crate::failure::captures::{capture_records, install_capture};

        install_capture();
        capture_records();
        log_record("thread 'main' panicked at app/src/main.rs:1:1: boom");

        let records = capture_records();
        assert_eq!(records.len(), 1, "one record per panic: {records:?}");
        assert_eq!(records[0].0, log::Level::Error);
        assert_eq!(records[0].1, LOG_TARGET);
        assert_eq!(
            records[0].2,
            "thread 'main' panicked at app/src/main.rs:1:1: boom"
        );
    }

    #[test]
    fn the_recording_path_sends_the_formatted_record_to_its_sink() {
        let records = std::cell::RefCell::new(Vec::new());
        record_panic(
            &|text: &str| records.borrow_mut().push(text.to_string()),
            "the pack failed to load",
            Some(("app/src/main.rs", 12, 3)),
            "main",
        );

        assert_eq!(
            records.into_inner(),
            vec![
                "thread 'main' panicked at app/src/main.rs:12:3: the pack failed to load"
                    .to_string()
            ]
        );
    }
}
