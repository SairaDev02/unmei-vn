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

/// The writer's guarantees over generated values, text and write boundaries.
///
/// Values and text share a three-letter alphabet, so values overlap, share
/// prefixes and recur often. None of those letters occurs in [`MARKER`], so a
/// value found in the output is a leak, not one rebuilt from a marker.
mod properties {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    const ALPHABET: [char; 3] = ['m', 'n', '-'];

    fn letter() -> impl Strategy<Value = String> {
        prop::sample::select(ALPHABET.to_vec()).prop_map(String::from)
    }

    fn value() -> impl Strategy<Value = String> {
        prop::collection::vec(letter(), 1..=5).prop_map(|letters| letters.concat())
    }

    /// `text` plus up to six byte offsets where it is split into writes.
    fn with_cuts(text: String) -> impl Strategy<Value = (String, Vec<usize>)> {
        let cuts = prop::collection::vec(0..=text.len(), 0..=6);
        (Just(text), cuts)
    }

    /// The writes `cuts` splits `text` into, in order. The text is ASCII, so
    /// every offset is a character boundary.
    fn chunks(text: &str, mut cuts: Vec<usize>) -> Vec<&str> {
        cuts.extend([0, text.len()]);
        cuts.sort_unstable();
        cuts.dedup();
        cuts.windows(2)
            .map(|pair| &text[pair[0]..pair[1]])
            .collect()
    }

    /// One or two values, and a text made mostly of them and loose letters.
    fn values_in_text() -> impl Strategy<Value = (Vec<String>, String, Vec<usize>)> {
        prop::collection::vec(value(), 1..=2).prop_flat_map(|values| {
            let piece = prop_oneof![prop::sample::select(values.clone()), letter()];
            prop::collection::vec(piece, 0..=12)
                .prop_flat_map(|pieces| with_cuts(pieces.concat()))
                .prop_map(move |(text, cuts)| (values.clone(), text, cuts))
        })
    }

    /// Values ending in `z`, and a text of their prefixes and loose letters
    /// that never contains `z`: every value starts in the text and none ends.
    fn prefixes_only() -> impl Strategy<Value = (Vec<String>, String, Vec<usize>)> {
        prop::collection::vec(value(), 1..=2).prop_flat_map(|stems| {
            let prefix = prop::sample::select(stems.clone()).prop_flat_map(|stem| {
                (1..=stem.len()).prop_map(move |len| stem[..len].to_string())
            });
            let piece = prop_oneof![prefix, letter()];
            let values: Vec<String> = stems.iter().map(|stem| format!("{stem}z")).collect();
            prop::collection::vec(piece, 0..=12)
                .prop_flat_map(|pieces| with_cuts(pieces.concat()))
                .prop_map(move |(text, cuts)| (values.clone(), text, cuts))
        })
    }

    proptest! {
        // A fixed seed keeps the workspace gate deterministic. A failure is a
        // confirmed defect: keep its shrunk case as a named example test above
        // rather than in a regressions file, so nothing is persisted here.
        #![proptest_config(ProptestConfig {
            cases: 512,
            rng_seed: RngSeed::Fixed(0x756e_6d65_6900),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        /// However the text is split into writes, the output is what one write
        /// of the whole text gives.
        #[test]
        fn write_boundaries_never_change_the_output((values, text, cuts) in values_in_text()) {
            let mask = Mask::new(values);
            let whole = written(&mask, &[&text]);
            prop_assert_eq!(written(&mask, &chunks(&text, cuts)), whole);
        }

        /// No value survives into the output, however the text is split.
        #[test]
        fn no_value_reaches_the_output((values, text, cuts) in values_in_text()) {
            let output = written(&Mask::new(values.clone()), &chunks(&text, cuts));
            for value in &values {
                prop_assert!(!output.contains(value.as_str()), "{value:?} leaked: {output:?}");
            }
        }

        /// Bytes held back as a possible value come out unchanged, neither lost
        /// nor repeated, when the value never completes.
        #[test]
        fn an_incomplete_value_is_emitted_unchanged((values, text, cuts) in prefixes_only()) {
            prop_assert_eq!(written(&Mask::new(values), &chunks(&text, cuts)), text);
        }
    }
}
