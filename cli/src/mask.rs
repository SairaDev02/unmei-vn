//! Exact-literal masking of the recognized environment credential values
//! across the command's terminal streams ([ADR 0014]).
//!
//! Both non-empty values of `UNMEI_API_KEY` and `OPENROUTER_API_KEY` — including
//! the one provider precedence does not select — are snapshotted once, before
//! argument parsing, and every exact occurrence in the bytes written through a
//! [`MaskingWriter`] is replaced with [`MARKER`]. The writer holds back a
//! trailing suffix that a longer value could still complete, so a value split
//! across formatting or write chunks is masked whole rather than leaked in
//! part. Every writer on one stream shares that stream's buffer
//! ([`masked_stream`]) — the `env_logger` sink shares stderr — and the command
//! finalizes both buffers before it exits.
//!
//! This is a literal match only. Unknown values, fragments, encodings and
//! paraphrases are outside the guarantee; the marker is itself a fixed literal;
//! OS terminal echo and arbitrary external emitters are not covered. Generated
//! files, provider prompts and correction payloads are **not** rewritten or
//! certified secret-free, so a masked preview does not make a pack safe to
//! share ([ADR 0014]). When no recognized value is configured, Clap's help and
//! argument rendering keeps its ordinary stream and TTY styling; with one
//! configured, it is re-rendered through the mask in plain form, because the
//! styled variant is printed by Clap straight to the process streams. A
//! `RUST_LOG` spec is validated without letting `env_filter` print its own
//! malformed-spec warning, which would bypass every writer here.
//!
//! [ADR 0014]: ../../docs/adr/0014-cli-diagnostics-and-terminal-credential-safety.md

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use unmei::provider::API_KEY_ENVS;

/// The fixed literal that replaces a recognized credential value.
pub const MARKER: &str = "[redacted]";

/// The recognized credential values, snapshotted once per command.
pub struct Mask {
    /// Non-empty values, longest first so an overlapping value wins whole.
    values: Arc<[Vec<u8>]>,
}

impl Mask {
    /// Snapshot both non-empty recognized environment values.
    pub fn from_env() -> Self {
        Self::new(
            API_KEY_ENVS
                .into_iter()
                .filter_map(|name| std::env::var(name).ok()),
        )
    }

    /// Build a mask from `values`, dropping the empty ones.
    pub fn new(values: impl IntoIterator<Item = String>) -> Self {
        let mut values: Vec<Vec<u8>> = values
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(String::into_bytes)
            .collect();
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        Self {
            values: Arc::from(values),
        }
    }

    /// A writer that masks the recognized values as `inner` receives them.
    pub fn writer<W: Write>(&self, inner: W) -> MaskingWriter<W> {
        MaskingWriter {
            inner,
            values: Arc::clone(&self.values),
            pending: Vec::new(),
        }
    }

    /// Whether no recognized value is configured, i.e. no output can carry one.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Apply a filter spec without letting `env_filter` print its own malformed-spec
/// warning; a failure falls back to the default level and is returned for the
/// caller to report through the masked logger sink.
///
/// `env_logger` applies `RUST_LOG` through the crate version in
/// `cli/Cargo.toml`, so this validation must come from that same version.
fn apply_filter(builder: &mut env_logger::Builder, spec: &str) -> Option<env_filter::ParseError> {
    match env_filter::Builder::new().try_parse(spec) {
        Ok(_) => {
            // The spec parsed once, so `env_filter`'s printed form emits nothing.
            builder.parse_filters(spec);
            None
        }
        Err(err) => {
            builder.filter_level(log::LevelFilter::Warn);
            Some(err)
        }
    }
}

/// Install the process logger on the command's shared stderr buffer.
pub fn init_logging(stream: &MaskedStream<std::io::Stderr>) {
    // `env_logger::from_env`/`parse_env` would parse `RUST_LOG` through
    // `env_filter`'s printing API, whose warning text can carry a recognized
    // value; read both variables here and route a failure through the mask.
    let mut builder = env_logger::Builder::new();
    if let Ok(style) = std::env::var("RUST_LOG_STYLE") {
        builder.parse_write_style(&style);
    }
    let diagnostic = match std::env::var("RUST_LOG") {
        Ok(spec) => apply_filter(&mut builder, &spec),
        Err(_) => {
            builder.filter_level(log::LevelFilter::Warn);
            None
        }
    };
    builder.target(logger_target(stream.clone())).init();
    if let Some(err) = diagnostic {
        log::warn!("ignoring RUST_LOG: {err}");
    }
}

/// One stream's shared masked buffer. Clones write to the same held-suffix
/// state, so a recognized value cannot be split between two writers that target
/// the same stream — for example the logger sink and the terminal.
pub struct MaskedStream<W: Write>(Arc<Mutex<MaskingWriter<W>>>);

impl<W: Write> Clone for MaskedStream<W> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<W: Write> MaskedStream<W> {
    /// Emit a held fragment and flush once no more bytes will reach the stream.
    pub fn finish(&self) -> io::Result<()> {
        self.lock().finish()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MaskingWriter<W>> {
        // A poisoned lock still holds valid bytes: keep writing, never panic.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<W: Write> Write for MaskedStream<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.lock().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.lock().flush()
    }
}

/// One shared masked buffer over `inner`.
pub fn masked_stream<W: Write>(mask: &Mask, inner: W) -> MaskedStream<W> {
    MaskedStream(Arc::new(Mutex::new(mask.writer(inner))))
}

/// The masked sink for `env_logger`, so supported enabled dependency logging
/// shares the command's stream buffer rather than bypassing it.
pub fn logger_target<W: Write + Send + 'static>(stream: MaskedStream<W>) -> env_logger::Target {
    env_logger::Target::Pipe(Box::new(stream))
}

/// The longest value that `bytes` starts with, if any.
fn longest_match(values: &[Vec<u8>], bytes: &[u8]) -> Option<usize> {
    values
        .iter()
        .filter(|value| bytes.starts_with(value))
        .map(Vec::len)
        .max()
}

/// Whether a longer value could still complete from `bytes` alone.
fn could_extend(values: &[Vec<u8>], bytes: &[u8]) -> bool {
    values
        .iter()
        .any(|value| value.len() > bytes.len() && value.starts_with(bytes))
}

/// A `Write` that replaces recognized credential values with [`MARKER`].
///
/// A trailing suffix that could still extend into a value is held back until it
/// resolves or the stream ends. `flush` passes through to the inner writer but
/// deliberately does **not** drain that suffix: emitting it early could leak a
/// credential whose remainder arrives after the flush. Call [`Self::finish`]
/// once no more bytes will be written, or rely on `Drop` as the net.
pub struct MaskingWriter<W: Write> {
    inner: W,
    values: Arc<[Vec<u8>]>,
    pending: Vec<u8>,
}

impl<W: Write> MaskingWriter<W> {
    /// Mask everything held back, then flush the inner writer.
    pub fn finish(&mut self) -> io::Result<()> {
        self.release(true)?;
        self.inner.flush()
    }

    /// Emit every buffered byte that is already decided; on `final_pass`, a
    /// held suffix that never resolved is a fragment (outside the literal
    /// guarantee) and is emitted rather than lost.
    fn release(&mut self, final_pass: bool) -> io::Result<()> {
        let mut out = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            let rest = &self.pending[index..];
            if !final_pass && could_extend(&self.values, rest) {
                break;
            }
            match longest_match(&self.values, rest) {
                Some(len) => {
                    out.extend_from_slice(MARKER.as_bytes());
                    index += len;
                }
                None => {
                    out.push(rest[0]);
                    index += 1;
                }
            }
        }
        self.pending.drain(..index);
        if out.is_empty() {
            Ok(())
        } else {
            self.inner.write_all(&out)
        }
    }
}

impl<W: Write> Write for MaskingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        self.release(false)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write> Drop for MaskingWriter<W> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(values: &[&str]) -> Mask {
        Mask::new(values.iter().map(|value| value.to_string()))
    }

    /// A sink the test can read while the writer still holds it.
    #[derive(Clone, Default)]
    struct SharedSink(Arc<std::sync::Mutex<Vec<u8>>>);

    impl SharedSink {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Write `chunks` (finishing the stream) and return the bytes the sink saw.
    fn written(mask: &Mask, chunks: &[&str]) -> String {
        let mut sink = Vec::new();
        {
            let mut writer = mask.writer(&mut sink);
            for chunk in chunks {
                writer.write_all(chunk.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        String::from_utf8(sink).unwrap()
    }

    #[test]
    fn masks_both_values_and_keeps_ordinary_text() {
        let mask = mask(&["first-secret", "second-secret"]);
        assert_eq!(
            written(&mask, &["say first-secret, then second-secret.\n"]),
            "say [redacted], then [redacted].\n"
        );
    }

    #[test]
    fn empty_values_are_ignored() {
        let mask = mask(&["", "only-secret"]);
        assert_eq!(written(&mask, &["only-secret\n"]), "[redacted]\n");
    }

    #[test]
    fn an_overlapping_value_is_masked_whole() {
        let mask = mask(&["abc", "abcdef"]);
        assert_eq!(
            written(&mask, &["abcdef abc abcdefg\n"]),
            "[redacted] [redacted] [redacted]g\n"
        );
    }

    /// A value split after a shorter overlapping value must still mask the
    /// longer one, whichever write boundary the bytes arrive on.
    #[test]
    fn an_overlapping_value_split_across_writes_is_masked_whole() {
        let mask = mask(&["abc", "abcdef"]);
        assert_eq!(written(&mask, &["abc", "def"]), "[redacted]");
        assert_eq!(written(&mask, &["ab", "cd", "ef"]), "[redacted]");
    }

    #[test]
    fn write_boundaries_do_not_change_the_result() {
        let mask = mask(&["first-secret", "second-secret"]);
        let text = "a first-secret b second-secret c first-secret\n";
        let expected = "a [redacted] b [redacted] c [redacted]\n";
        for split in 0..=text.len() {
            if !text.is_char_boundary(split) {
                continue;
            }
            let chunks = [&text[..split], &text[split..]];
            assert_eq!(written(&mask, &chunks), expected, "split at {split}");
        }
        let bytewise: Vec<&str> = (0..text.len()).map(|i| &text[i..i + 1]).collect();
        assert_eq!(written(&mask, &bytewise), expected, "byte-by-byte");
    }

    #[test]
    fn nested_formatting_writes_are_masked() {
        let mask = mask(&["test-key-1"]);
        let (head, tail) = ("test-key", "-1");
        let mut sink = Vec::new();
        {
            let mut writer = mask.writer(&mut sink);
            // `write_fmt` emits `"key="`, `head` and `tail` as separate writes,
            // so the value arrives split across formatting boundaries.
            write!(writer, "key={head}{tail}").unwrap();
            writeln!(writer, " done").unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(String::from_utf8(sink).unwrap(), "key=[redacted] done\n");
    }

    #[test]
    fn a_value_split_across_a_flush_is_still_masked() {
        let mask = mask(&["test-key-1"]);
        let sink = SharedSink::default();
        let mut writer = mask.writer(sink.clone());
        writer.write_all(b"before test-ke").unwrap();
        writer.flush().unwrap();
        let flushed = sink.text();
        assert!(
            flushed.contains("before "),
            "flush emits the ordinary bytes: {flushed}"
        );
        assert!(
            !flushed.contains("test-ke"),
            "flush must not drain a held value prefix"
        );
        writer.write_all(b"y-1 after\n").unwrap();
        writer.finish().unwrap();
        assert_eq!(sink.text(), "before [redacted] after\n");
    }

    /// Two writers on one stream share its buffer, so neither can leak half of a
    /// value the other completes.
    #[test]
    fn writers_sharing_a_stream_cannot_split_a_value() {
        let mask = mask(&["shared-secret-42"]);
        let sink = SharedSink::default();
        let stream = masked_stream(&mask, sink.clone());
        let (mut first, mut second) = (stream.clone(), stream.clone());
        first.write_all(b"before shared-se").unwrap();
        second.write_all(b"cret-42 after\n").unwrap();
        stream.finish().unwrap();
        assert_eq!(sink.text(), "before [redacted] after\n");
    }

    #[test]
    fn a_fragment_left_at_the_end_is_emitted_unchanged() {
        let mask = mask(&["test-key-1"]);
        assert_eq!(written(&mask, &["ends with test-ke"]), "ends with test-ke");
    }

    #[test]
    fn a_short_value_never_leaves_a_partial_literal() {
        let mask = mask(&["ab"]);
        assert_eq!(
            written(&mask, &["abab", "a", "b"]),
            "[redacted][redacted][redacted]"
        );
    }

    /// A valid spec that carries a recognized value is still applied — never
    /// discarded by the malformed-spec guard — and its records are masked.
    #[test]
    fn a_valid_filter_spec_is_applied_and_its_records_masked() {
        use log::Log;

        let mask = mask(&["valid-filter-secret"]);
        let sink = SharedSink::default();
        let mut builder = env_logger::Builder::new();
        assert!(apply_filter(&mut builder, "valid-filter-secret=trace").is_none());
        let logger = builder
            .target(logger_target(masked_stream(&mask, sink.clone())))
            .build();
        logger.log(
            &log::Record::builder()
                .level(log::Level::Trace)
                .target("valid-filter-secret")
                .args(format_args!("loaded valid-filter-secret"))
                .build(),
        );
        logger.flush();

        let logged = sink.text();
        assert!(
            logged.contains("loaded"),
            "the valid spec is applied: {logged}"
        );
        assert!(logged.contains(MARKER), "{logged}");
        assert!(!logged.contains("valid-filter-secret"), "{logged}");
    }

    /// A malformed spec is returned to the caller rather than applied.
    #[test]
    fn a_malformed_filter_spec_returns_its_error() {
        let mut builder = env_logger::Builder::new();
        let err = apply_filter(&mut builder, "module=bogus").expect("malformed spec");
        assert!(err.to_string().contains("bogus"), "{err}");
    }

    #[test]
    fn inner_write_errors_are_propagated() {
        struct Failing;
        impl Write for Failing {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if buf.is_empty() {
                    Ok(0)
                } else {
                    Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mask = mask(&["test-key-1"]);
        let mut writer = mask.writer(Failing);
        assert!(writer.write_all(b"ordinary text\n").is_err());
    }
}
