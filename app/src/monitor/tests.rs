//! The capture path's seam tests.
//!
//! Some tests drive internal seams (`reserved`, the completed lock,
//! `LineBuilder`, `enter_capture`) because overload, contention, reentrancy and
//! byte-cap states have no safe public trigger — a deterministic seam is the
//! only way to protect them.

use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};

use bevy::ecs::system::RunSystemOnce;
use bevy::log::tracing;
use bevy::log::tracing_subscriber;
use bevy::prelude::*;
use tracing_subscriber::prelude::*;

use super::capture::Reservation;
use super::layer::{enter_capture, CaptureGuard};
use super::line::{MAX_RENDERED_ELAPSED_MS, TRUNCATION_SUFFIX};
use super::render::{render_with, LineBuilder};
use super::*;

/// A controllable monotonic clock for deterministic elapsed metadata.
struct FakeClock {
    millis: AtomicU64,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            millis: AtomicU64::new(0),
        }
    }

    fn set(&self, millis: u64) {
        self.millis.store(millis, Ordering::Relaxed);
    }
}

impl MonitorClock for FakeClock {
    fn now_millis(&self) -> u64 {
        self.millis.load(Ordering::Relaxed)
    }
}

fn capture_with(clock: Arc<FakeClock>) -> MonitorCapture {
    MonitorCapture::install(clock)
}

/// A `Debug` value that counts how many times it was formatted, so a test
/// can witness that Monitor never touches a value past the byte cap or
/// formats it twice across the message/field passes.
struct CountVisits<'a>(&'a AtomicUsize);

impl fmt::Debug for CountVisits<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fetch_add(1, Ordering::SeqCst);
        f.write_str("v")
    }
}

/// Emit one event into `capture` through the real layer, leaving it in the
/// completed queue.
fn admit(capture: &MonitorCapture, event: impl FnOnce()) {
    let subscriber = tracing_subscriber::registry().with(capture_layer(capture));
    tracing::subscriber::with_default(subscriber, event);
}

/// Emit one event and drain whatever it admitted.
fn emit(capture: &MonitorCapture, event: impl FnOnce()) -> Vec<MonitorLine> {
    admit(capture, event);
    capture.pending_completed(PENDING_CAPACITY)
}

fn one_line(capture: &MonitorCapture, event: impl FnOnce()) -> MonitorLine {
    let mut lines = emit(capture, event);
    assert_eq!(lines.len(), 1, "expected exactly one admitted line");
    lines.pop().unwrap()
}

#[test]
fn distinct_upstream_and_funnel_sources_become_distinct_lines() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let subscriber = tracing_subscriber::registry().with(capture_layer(&capture));
    tracing::subscriber::with_default(subscriber, || {
        // Kira's own upstream warning...
        tracing::warn!(target: "bevy_kira_audio::audio_output", "Failed to setup audio: NoDevice");
        // ...and the funnel's row-7 mirror, two distinct source events.
        tracing::warn!(target: "unmei::failure", "couldn't start audio: Failed to setup audio: NoDevice");
    });
    let lines = capture.pending_completed(PENDING_CAPACITY);
    assert_eq!(lines.len(), 2, "each source event is one distinct line");
    assert_eq!(lines[0].target, "bevy_kira_audio::audio_output");
    assert_eq!(lines[1].target, "unmei::failure");
}

#[test]
fn a_panicking_formatter_releases_its_reservation() {
    // Assumes the default `panic = unwind` test profile so the reservation's
    // `Drop` runs during unwinding.
    struct Panics;
    impl fmt::Debug for Panics {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            panic!("emitter debug panic")
        }
    }

    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();
    let subscriber = tracing_subscriber::registry().with(capture_layer(&capture));
    let value = Panics;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "t", ?value);
        });
    }));
    assert!(result.is_err(), "the emitter panic unwinds through capture");
    // The reservation is released and the discarded admission counted; the
    // next event still admits normally.
    assert_eq!(shared.reserved.load(Ordering::Acquire), 0);
    assert_eq!(capture.counters().lost, 1);
    let line = one_line(&capture, || tracing::info!(target: "t", "recovered"));
    assert!(line.text.ends_with("recovered"));
}

#[test]
fn the_upstream_filter_hides_events_from_capture() {
    use tracing_subscriber::filter::EnvFilter;

    let capture = capture_with(Arc::new(FakeClock::new()));
    // The boxed layer goes first so the composition stays a `Subscriber`;
    // the `EnvFilter` still gates every layer, capture included.
    let subscriber = tracing_subscriber::registry()
        .with(capture_layer(&capture))
        .with(EnvFilter::new("info"));
    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!(target: "t", "below the filter");
        tracing::info!(target: "t", "above the filter");
    });

    let lines = capture.pending_completed(PENDING_CAPACITY);
    assert_eq!(lines.len(), 1, "only the admitted event is captured");
    assert!(lines[0].text.ends_with("above the filter"));
}

#[test]
fn the_line_carries_elapsed_level_exact_target_message_and_fields_in_order() {
    let clock = Arc::new(FakeClock::new());
    let capture = capture_with(Arc::clone(&clock));
    clock.set(1234);

    let line = one_line(&capture, || {
        tracing::warn!(target: "unmei::failure", answer = 42, note = "hi", "couldn't save");
    });

    assert_eq!(line.elapsed_ms, 1234);
    assert_eq!(line.level, log::Level::Warn);
    assert_eq!(line.target, "unmei::failure");
    assert!(line.text.starts_with("0000001234ms WARN  unmei::failure "));
    assert!(line.text.ends_with("couldn't save answer=42 note=hi"));
    assert!(!line.truncated);
}

#[test]
fn a_message_less_event_retains_its_fields() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let line = one_line(&capture, || {
        tracing::info!(target: "dep::crate", count = 3u64);
    });
    assert!(line.text.ends_with("count=3"), "{}", line.text);
}

#[test]
fn an_ordinary_log_target_field_neither_overrides_the_target_nor_disappears() {
    // A field may be declared with a bridge's own name. It is an ordinary
    // named field: the exact metadata target survives and the field is
    // retained in recorded order after the message. The default `unmei::`
    // projection is therefore driven by `dep::crate`, not the forged field.
    let capture = capture_with(Arc::new(FakeClock::new()));
    let line = one_line(&capture, || {
        tracing::event!(
            target: "dep::crate",
            tracing::Level::INFO,
            { "log.target" } = "unmei::forged",
            { "log.file" } = "forged.rs",
            answer = 42,
            "the real body"
        );
    });

    assert_eq!(line.target, "dep::crate");
    assert_eq!(line.target_display, "dep::crate");
    assert!(
        !line.target.starts_with("unmei::"),
        "the forged field must not become the projection predicate: {}",
        line.target
    );
    assert!(
        line.text.ends_with(
            "dep::crate the real body log.target=unmei::forged log.file=forged.rs answer=42"
        ),
        "{}",
        line.text
    );
}

#[test]
fn an_explicitly_named_message_renders_once_and_immediately_after_the_target() {
    // Declared after `answer`/`note`, the explicit `message` field still
    // renders first and exactly once; the remaining fields keep recorded
    // order.
    let capture = capture_with(Arc::new(FakeClock::new()));
    let line = one_line(&capture, || {
        tracing::event!(
            target: "unmei::ordered",
            tracing::Level::INFO,
            answer = 42,
            note = "hi",
            message = "explicit message"
        );
    });

    assert_eq!(
        line.text,
        "0000000000ms INFO  unmei::ordered explicit message answer=42 note=hi"
    );
    assert_eq!(line.text.matches("explicit message").count(), 1);
}

#[test]
fn a_genuine_log_bridge_recovers_its_target_and_omits_only_its_scaffolding() {
    // The bridge record is identified by its callsite, so the recovered
    // target and message survive while `log.target`/`log.file` scaffolding
    // is dropped once. `format_trace` dispatches through the thread-local
    // default subscriber, exactly as the installed global bridge does.
    let capture = capture_with(Arc::new(FakeClock::new()));
    let line = one_line(&capture, || {
        let record = log::Record::builder()
            .args(format_args!("bridged body"))
            .level(log::Level::Info)
            .target("unmei::bridged")
            .module_path_static(Some("unmei::bridged"))
            .file_static(Some("bridged.rs"))
            .line(Some(7))
            .build();
        tracing_log::format_trace(&record).expect("the bridge dispatches");
    });

    assert_eq!(line.target, "unmei::bridged");
    assert_eq!(line.level, log::Level::Info);
    assert!(
        line.text.ends_with("unmei::bridged bridged body"),
        "{}",
        line.text
    );
    assert!(!line.text.contains("log."), "{}", line.text);
}

#[test]
fn escaping_distinguishes_a_newline_from_a_literal_backslash_n() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let line = one_line(&capture, || {
        tracing::info!(target: "t", "line one\nline two \\n literal");
    });
    assert!(line.text.contains("line one\\nline two"), "{}", line.text);
    assert!(line.text.contains("\\\\n literal"), "{}", line.text);
    assert!(
        !line.text.contains('\n'),
        "the stored line must not contain a physical newline: {:?}",
        line.text
    );
}

#[test]
fn controls_line_separators_bidi_and_ansi_are_escaped_while_unicode_survives() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let line = one_line(&capture, || {
        tracing::info!(
            target: "t\u{202E}x",
            "\u{1b}[31mred\u{1b}[0m\ttab\rcr\u{2028}ls\u{2066}bidi \u{4f60}\u{597d}"
        );
    });
    assert!(line.text.contains("\\u{1b}[31m"), "{}", line.text);
    assert!(line.text.contains("\\t"), "{}", line.text);
    assert!(line.text.contains("\\r"), "{}", line.text);
    assert!(line.text.contains("\\u{2028}"), "{}", line.text);
    assert!(line.text.contains("\\u{2066}"), "{}", line.text);
    assert!(line.text.contains("\\u{202e}"), "{}", line.text);
    assert!(line.text.contains("\u{4f60}\u{597d}"), "{}", line.text);
    assert!(!line.text.contains('\u{1b}'), "{}", line.text);
    // The grid's target column is the escaped target, while filtering keeps
    // the exact original metadata.
    assert_eq!(line.target, "t\u{202E}x");
    assert_eq!(line.target_display, "t\\u{202e}x");
}

#[test]
fn a_long_target_column_keeps_the_whole_escaped_target() {
    // A short target is unchanged.
    let capture = capture_with(Arc::new(FakeClock::new()));
    let short = one_line(&capture, || {
        tracing::info!(target: "unmei::smoke", "short");
    });
    assert_eq!(short.target_display, "unmei::smoke");

    // A long module path renders its whole escaped target; the fixed 224px
    // column wraps it instead of shortening it under an ellipsis (B5).
    const LONG_TARGET: &str = "unmei::smoke::a_deliberately_long_module_path_that_does_not_fit";
    let line = one_line(&capture, || {
        tracing::info!(target: LONG_TARGET, "tail");
    });
    assert_eq!(
        line.target, LONG_TARGET,
        "filtering keeps the exact metadata"
    );
    assert_eq!(
        line.target_display, LONG_TARGET,
        "the target column keeps the full escaped target"
    );
    assert!(
        !line.target_display.contains('\u{2026}'),
        "no ordinary display ellipsis remains: {}",
        line.target_display
    );
    assert!(
        line.text.contains(LONG_TARGET),
        "the stored line is untouched"
    );
}

#[test]
fn an_oversized_line_is_cut_to_the_byte_cap_at_an_escape_boundary() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    // A message of multi-byte chars that overflows the content budget.
    let line = one_line(&capture, || {
        tracing::info!(target: "t", "{}", "\u{4f60}".repeat(4000));
    });
    assert!(line.truncated);
    assert!(line.text.ends_with(TRUNCATION_SUFFIX), "{}", line.text);
    assert!(
        line.text.len() <= MAX_LINE_BYTES,
        "line was {} bytes",
        line.text.len()
    );
    assert!(
        line.text.len() > MAX_LINE_BYTES - 8,
        "the cut should sit at the cap, not early: {}",
        line.text.len()
    );
    // The content before the suffix is valid UTF-8 (it is a String), and no
    // partial escape leaked: the suffix is appended exactly once.
    assert_eq!(line.text.matches("[truncated]").count(), 1);
}

#[test]
fn an_oversized_target_is_bounded_and_still_keeps_its_original_metadata() {
    // A metadata target must be a constant expression, so the huge-target
    // bound is exercised through the shared renderer directly.
    let target = format!("unmei::{}", "x".repeat(9000));
    let mut line = render_with(log::Level::Info, &target, false, |_| {}).expect("line renders");
    line.stamp_admission(0);
    assert!(line.truncated);
    assert!(line.text.len() <= MAX_LINE_BYTES);
    assert!(line.text.ends_with(TRUNCATION_SUFFIX));
    // Filtering metadata is the original target, not the shortened render.
    assert_eq!(line.target, target);
}

#[test]
fn a_field_after_the_cap_is_never_formatted() {
    let visits = AtomicUsize::new(0);
    let mut line = LineBuilder::new();
    // A message that overflows the content budget, exactly as a capture does.
    assert!(!line.push_escaped(&"x".repeat(MAX_LINE_BYTES * 2)));
    assert!(line.truncated);

    let value = CountVisits(&visits);
    line.push_debug(&value);
    assert_eq!(
        visits.load(Ordering::SeqCst),
        0,
        "a value after the cap must not be formatted"
    );
}

#[test]
fn a_value_after_an_oversized_message_is_never_formatted() {
    let visits = AtomicUsize::new(0);
    let value = CountVisits(&visits);
    let capture = capture_with(Arc::new(FakeClock::new()));
    // The message (recorded first) fills the budget, so the field pass must
    // be skipped entirely rather than format `value` into a full line.
    let line = one_line(&capture, || {
        tracing::info!(target: "t", first = ?value, "{}", "x".repeat(MAX_LINE_BYTES));
    });
    assert!(line.truncated);
    assert_eq!(
        visits.load(Ordering::SeqCst),
        0,
        "a value after an oversized message must not be formatted"
    );
}

#[test]
fn each_named_field_is_formatted_exactly_once_across_the_two_passes() {
    let visits = AtomicUsize::new(0);
    let value = CountVisits(&visits);
    let capture = capture_with(Arc::new(FakeClock::new()));
    let line = one_line(&capture, || {
        tracing::info!(target: "t", answer = ?value, "body");
    });
    assert!(line.text.ends_with("body answer=v"), "{}", line.text);
    assert_eq!(
        visits.load(Ordering::SeqCst),
        1,
        "the message pass must not format a non-message field"
    );
}

#[test]
fn a_formatter_that_fails_creates_no_line_and_releases_the_slot() {
    struct Fails;
    impl fmt::Debug for Fails {
        fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
            Err(fmt::Error)
        }
    }

    let capture = capture_with(Arc::new(FakeClock::new()));
    let broken = Fails;
    let lines = emit(&capture, || {
        tracing::info!(target: "t", ?broken);
    });
    assert!(lines.is_empty(), "a failed formatter admits no line");
    assert_eq!(capture.counters().lost, 1);
    assert_eq!(capture.shared.reserved.load(Ordering::Acquire), 0);

    // The slot is free again: the next event admits normally.
    let line = one_line(&capture, || tracing::info!(target: "t", "ok"));
    assert!(line.text.ends_with("ok"));
}

#[test]
fn a_reentrant_capture_is_suppressed_and_counted() {
    // `tracing-core` itself drops an event emitted from inside a subscriber
    // callback, so this exercises our own guard directly: with the capture
    // flag already held, the next dispatched event must be suppressed and
    // counted rather than admitted.
    let capture = capture_with(Arc::new(FakeClock::new()));
    assert!(enter_capture(), "the first claim succeeds");
    let _reset = CaptureGuard;

    let lines = emit(&capture, || tracing::info!(target: "t", "nested"));
    assert!(lines.is_empty(), "a reentrant event admits no line");
    assert_eq!(capture.counters().lost, 1);
}

#[test]
fn a_poisoned_queue_fails_open_and_recovers_a_reservation() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();

    // Poison the completed mutex.
    let shared_for_panic = Arc::clone(&shared);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = shared_for_panic.completed.lock().unwrap();
        panic!("poison");
    }));

    // A subsequent admission still works: the poison is recovered and the
    // local unavailable state is exposed.
    let line = one_line(&capture, || tracing::info!(target: "t", "after poison"));
    assert!(line.text.ends_with("after poison"));
    assert!(capture.unavailable());
}

#[test]
fn all_reserved_slots_reject_before_formatting_and_count_one_loss() {
    struct NeverFormatted<'a>(&'a AtomicUsize);
    impl fmt::Debug for NeverFormatted<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fetch_add(1, Ordering::SeqCst);
            f.write_str("x")
        }
    }

    let visits = AtomicUsize::new(0);
    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();
    let held: Vec<Reservation> = (0..PENDING_CAPACITY)
        .map(|_| shared.reserve().expect("slot available"))
        .collect();
    assert_eq!(capture.counters().lost, 0);

    let lines = emit(&capture, || {
        let value = NeverFormatted(&visits);
        tracing::info!(target: "t", ?value);
    });
    assert!(lines.is_empty());
    assert_eq!(capture.counters().lost, 1);
    assert_eq!(
        visits.load(Ordering::SeqCst),
        0,
        "the rejected event must not be formatted"
    );

    drop(held);
    // Releasing the reservations counts each as a lost admission.
    assert_eq!(capture.counters().lost, PENDING_CAPACITY as u64 + 1);
    assert_eq!(shared.reserved.load(Ordering::Acquire), 0);
}

#[test]
fn a_full_completed_queue_evicts_the_oldest_completed_line_and_keeps_order() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    for i in 0..PENDING_CAPACITY {
        admit(&capture, || tracing::info!(target: "t", "{}", i));
    }
    assert_eq!(capture.counters().lost, 0);

    admit(&capture, || tracing::info!(target: "t", "newest"));
    let lines = capture.pending_completed(PENDING_CAPACITY);
    assert_eq!(lines.len(), PENDING_CAPACITY, "one slot was freed");
    assert_eq!(
        capture.counters().lost,
        1,
        "the evicted completed line is one loss"
    );
    // Oldest (0) is gone; 1..255 survive then the newest, in order.
    assert!(lines[0].text.ends_with(" 1"));
    assert!(lines[lines.len() - 1].text.ends_with("newest"));
}

#[test]
fn contended_admission_rejects_without_waiting_and_counts_a_loss() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();

    // Hold the completed lock while a producer tries to admit.
    let guard = shared.completed.lock().unwrap();
    let lines = emit(&capture, || tracing::info!(target: "t", "blocked"));
    assert!(lines.is_empty());
    assert_eq!(capture.counters().lost, 1);
    assert_eq!(shared.reserved.load(Ordering::Acquire), 0);
    drop(guard);

    // Once uncontended, admission resumes.
    let line = one_line(&capture, || tracing::info!(target: "t", "unblocked"));
    assert!(line.text.ends_with("unblocked"));
}

#[test]
fn counters_saturate_instead_of_wrapping() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();
    shared.counters.add_evicted(u64::MAX);
    shared.counters.add_evicted(5);
    shared.counters.add_lost(u64::MAX);
    shared.counters.add_lost(5);
    shared.counters.add_truncated(u64::MAX);
    shared.counters.add_truncated(5);
    assert_eq!(
        capture.counters(),
        MonitorCounters {
            evicted: u64::MAX,
            lost: u64::MAX,
            truncated: u64::MAX,
        }
    );
}

#[test]
fn concurrent_producers_keep_per_producer_order_and_account_for_contention() {
    const PRODUCERS: usize = 4;
    const PER_PRODUCER: usize = 32;

    fn parse(line: &MonitorLine) -> Option<(usize, usize)> {
        let token = line.text.rsplit(' ').next()?;
        let rest = token.strip_prefix('p')?;
        let (producer, index) = rest.split_once('i')?;
        Some((producer.parse().ok()?, index.parse().ok()?))
    }

    let clock = Arc::new(FakeClock::new());
    let capture = capture_with(clock);
    let barrier = Arc::new(Barrier::new(PRODUCERS));

    let mut handles = Vec::new();
    for producer in 0..PRODUCERS {
        let barrier = Arc::clone(&barrier);
        let subscriber = tracing_subscriber::registry().with(capture_layer(&capture));
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            tracing::subscriber::with_default(subscriber, || {
                for index in 0..PER_PRODUCER {
                    tracing::info!(target: "t", "p{producer}i{index}");
                }
            });
        }));
    }
    for handle in handles {
        handle.join().expect("producer joined");
    }

    let lines = capture.pending_completed(PENDING_CAPACITY);
    // Contended admission may reject, but every emitted event is either
    // admitted exactly once or accounted as a loss.
    assert_eq!(
        lines.len() as u64 + capture.counters().lost,
        (PRODUCERS * PER_PRODUCER) as u64
    );

    // Each producer's surviving events stay in emitted order and never
    // duplicate; no cross-producer order is asserted.
    let mut seen: Vec<Vec<usize>> = vec![Vec::new(); PRODUCERS];
    for line in &lines {
        let (producer, index) = parse(line).expect("parsed message token");
        seen[producer].push(index);
    }
    for (producer, indices) in seen.iter().enumerate() {
        assert!(
            indices.windows(2).all(|pair| pair[0] < pair[1]),
            "producer {producer} order changed: {indices:?}"
        );
    }
}

#[test]
fn one_update_ingests_at_most_128_events_and_leaves_the_rest_pending() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    for i in 0..129 {
        admit(&capture, || tracing::info!(target: "t", "{}", i));
    }

    let mut app = App::new();
    app.insert_resource(capture.clone())
        .init_resource::<MonitorHistory>();
    app.world_mut().run_system_once(ingest_capture).unwrap();

    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        INGEST_PER_UPDATE
    );
    assert_eq!(capture.pending_completed(PENDING_CAPACITY).len(), 1);
}

#[test]
fn retention_ages_out_the_oldest_line_and_counts_one_eviction() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let mut app = App::new();
    app.insert_resource(capture.clone())
        .init_resource::<MonitorHistory>();

    // Admit and ingest in production-sized batches so the 256-slot pending
    // buffer never overflows before the app consumes it.
    for i in 0..(RETAINED_CAPACITY + 1) {
        admit(&capture, || tracing::info!(target: "t", "{}", i));
        if (i + 1) % INGEST_PER_UPDATE == 0 {
            app.world_mut().run_system_once(ingest_capture).unwrap();
        }
    }
    app.world_mut().run_system_once(ingest_capture).unwrap();

    let history = app.world().resource::<MonitorHistory>();
    assert_eq!(history.retained(), RETAINED_CAPACITY);
    assert_eq!(capture.counters().evicted, 1);
    assert!(!history.text().contains(" 0\n") && !history.lines()[0].text.ends_with(" 0"));
    assert!(history
        .lines()
        .back()
        .expect("newest")
        .text
        .ends_with(&format!(" {}", RETAINED_CAPACITY)));
}

#[test]
fn a_truncated_admitted_line_counts_truncation_once_and_eviction_separately() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let _ = emit(&capture, || {
        tracing::info!(target: "t", "{}", "x".repeat(MAX_LINE_BYTES * 2));
    });
    assert_eq!(capture.counters().truncated, 1);
    assert_eq!(capture.counters().evicted, 0);
    assert_eq!(capture.counters().lost, 0);
}

#[test]
fn a_saturated_elapsed_prefix_keeps_the_fixed_width_and_body_offset() {
    let clock = Arc::new(FakeClock::new());
    let capture = capture_with(Arc::clone(&clock));
    clock.set(MAX_RENDERED_ELAPSED_MS + 5);

    let line = one_line(&capture, || tracing::info!(target: "t", "tail"));

    assert!(line.text.starts_with("9999999999ms "), "{}", line.text);
    assert_eq!(
        line.elapsed_ms,
        MAX_RENDERED_ELAPSED_MS + 5,
        "the true admission time is retained even when the prefix saturates"
    );
    assert_eq!(line.body(), " tail", "the body offset never shifts");
    assert!(line.text.len() <= MAX_LINE_BYTES);
}

#[test]
fn a_stalled_handoff_keeps_staged_payloads_charged_against_the_pending_cap() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();

    // Withhold the consumer: 128 completed lines are staged for handoff
    // while 128 remain completed — exactly the 256-slot cap.
    for i in 0..PENDING_CAPACITY {
        admit(&capture, || tracing::info!(target: "t", "{}", i));
    }
    let mut staged = capture.take_staged(INGEST_PER_UPDATE);
    assert_eq!(staged.len(), INGEST_PER_UPDATE);
    assert_eq!(capture.staged_len(), INGEST_PER_UPDATE);
    assert_eq!(capture.pending_len(), PENDING_CAPACITY - INGEST_PER_UPDATE);

    // A producer refill evicts completed lines, but the staged payloads stay
    // charged, so only `PENDING_CAPACITY - INGEST_PER_UPDATE` completed lines
    // can exist beside the staged batch (A01: the pre-repair code let the
    // queue refill to its full 256 while 128 payloads were still staged,
    // reaching 384 payloads in total).
    for _ in 0..PENDING_CAPACITY {
        admit(&capture, || tracing::info!(target: "t", "refill"));
    }
    assert_eq!(
        capture.pending_len(),
        PENDING_CAPACITY - INGEST_PER_UPDATE,
        "the staged payloads keep the completed queue below its full cap"
    );
    assert_eq!(shared.reserved.load(Ordering::Acquire), 0);

    // Handing each staged line to history releases exactly its own charge.
    let mut app = App::new();
    app.insert_resource(capture.clone())
        .init_resource::<MonitorHistory>();
    let mut handed_off = 0;
    while let Some(line) = staged.pop_front() {
        app.world_mut()
            .resource_mut::<MonitorHistory>()
            .push(line, &capture);
        staged.release_one();
        handed_off += 1;
    }
    assert_eq!(handed_off, INGEST_PER_UPDATE);
    assert_eq!(capture.staged_len(), 0);
    assert_eq!(
        app.world().resource::<MonitorHistory>().retained(),
        INGEST_PER_UPDATE
    );
    drop(staged);
    assert_eq!(capture.counters().truncated, 0);
}

#[test]
fn dropping_a_staged_batch_counts_each_leftover_once_and_releases_capacity() {
    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();
    for i in 0..PENDING_CAPACITY {
        admit(&capture, || tracing::info!(target: "t", "{}", i));
    }

    let staged = capture.take_staged(INGEST_PER_UPDATE);
    assert_eq!(capture.staged_len(), INGEST_PER_UPDATE);
    drop(staged);

    assert_eq!(capture.staged_len(), 0);
    assert_eq!(shared.reserved.load(Ordering::Acquire), 0);
    assert_eq!(
        capture.counters().lost,
        INGEST_PER_UPDATE as u64,
        "each discarded staged payload is one loss, counted once"
    );
}

#[test]
fn admission_order_and_time_follow_completed_insertion_not_formatter_start() {
    use std::sync::mpsc;

    struct BlockingFormatter {
        started: mpsc::Sender<()>,
        resume: Mutex<mpsc::Receiver<()>>,
    }
    impl fmt::Debug for BlockingFormatter {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let _ = self.started.send(());
            let _ = self.resume.lock().expect("resume lock").recv();
            f.write_str("stalled")
        }
    }

    let clock = Arc::new(FakeClock::new());
    let capture = capture_with(Arc::clone(&clock));
    let mut app = App::new();
    app.insert_resource(capture.clone())
        .init_resource::<MonitorHistory>();

    let (started_tx, started_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let formatter = BlockingFormatter {
        started: started_tx,
        resume: Mutex::new(resume_rx),
    };
    let producer = {
        let capture = capture.clone();
        std::thread::spawn(move || {
            let subscriber = tracing_subscriber::registry().with(capture_layer(&capture));
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(target: "t", ?formatter);
            });
        })
    };

    // The first producer has reserved and is stalled mid-format.
    started_rx.recv().expect("formatter started");
    // A later-started producer admits at 100ms; a real consumer drains it
    // into retained history while the first formatter is still unfinished.
    clock.set(100);
    admit(&capture, || tracing::info!(target: "t", "later"));
    app.world_mut().run_system_once(ingest_capture).unwrap();

    // The first formatter now finishes and is admitted at 200ms, after the
    // already-drained later event.
    clock.set(200);
    resume_tx.send(()).expect("resume the formatter");
    producer.join().expect("producer joined");
    app.world_mut().run_system_once(ingest_capture).unwrap();

    let history = app.world().resource::<MonitorHistory>();
    let lines: Vec<&MonitorLine> = history.lines().iter().map(|line| line.as_ref()).collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].text.ends_with("later"), "{}", lines[0].text);
    assert_eq!(
        lines[0].sequence, 0,
        "the successfully inserted later event is admitted first"
    );
    assert_eq!(
        lines[0].elapsed_ms, 100,
        "elapsed time is stamped at the successful insertion"
    );
    assert!(lines[1].text.ends_with("stalled"), "{}", lines[1].text);
    assert_eq!(
        lines[1].sequence, 1,
        "identity follows insertion, not reservation order"
    );
    assert_eq!(
        lines[1].elapsed_ms, 200,
        "the earlier-reserved event is stamped at its later insertion"
    );
}

#[test]
fn a_truncated_candidate_that_loses_the_insertion_lock_does_not_count_truncation() {
    use std::sync::mpsc;

    struct BlockingTruncated {
        started: mpsc::Sender<()>,
        resume: Mutex<mpsc::Receiver<()>>,
        bytes: usize,
    }
    impl fmt::Debug for BlockingTruncated {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let _ = self.started.send(());
            let _ = self.resume.lock().expect("resume lock").recv();
            f.write_str(&"x".repeat(self.bytes))
        }
    }

    let capture = capture_with(Arc::new(FakeClock::new()));
    let shared = capture.shared_arc();
    let (started_tx, started_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let value = BlockingTruncated {
        started: started_tx,
        resume: Mutex::new(resume_rx),
        bytes: MAX_LINE_BYTES * 2,
    };
    let producer = {
        let capture = capture.clone();
        std::thread::spawn(move || {
            let subscriber = tracing_subscriber::registry().with(capture_layer(&capture));
            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(target: "t", ?value);
            });
        })
    };

    // Reserve succeeded and formatting is in progress; take the lock so the
    // eventual insertion is rejected as contended.
    started_rx.recv().expect("formatter started");
    let guard = shared.completed.lock().expect("hold the completed lock");
    resume_tx.send(()).expect("resume the formatter");
    producer.join().expect("producer joined");
    drop(guard);

    assert_eq!(
        capture.counters().truncated,
        0,
        "a truncated candidate that never inserted is not a truncation"
    );
    assert_eq!(capture.counters().lost, 1);
    assert_eq!(shared.reserved.load(Ordering::Acquire), 0);
}
