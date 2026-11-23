use super::*;

/// Every file beside the destination, so a receipt observes the writer's
/// temps without re-deriving the writer's own naming rule.
fn leftovers(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name() != Some(std::ffi::OsStr::new("scene.unmei")))
        .collect();
    paths.sort();
    paths
}

#[test]
fn a_partial_temp_write_failure_keeps_the_target_and_a_whole_retry_lands() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scene.unmei");
    std::fs::write(&path, "old contents").unwrap();

    {
        let _armed = faults::arm(faults::WriteFault::PartialWrite(4));
        let error = write_atomic(&path, "a rewritten scene, longer than the old").unwrap_err();
        assert_eq!(error.to_string(), faults::PARTIAL_WRITE_FAILURE);
    }

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old contents");
    // This writer has no write-phase cleanup: the partial bytes stay in the
    // temp sibling. That is this receipt's observed postcondition, not a
    // promise; only a rename failure removes its temp.
    let temps = leftovers(dir.path());
    assert_eq!(temps.len(), 1, "one partial temp stays: {temps:?}");
    assert!(temps[0].to_string_lossy().ends_with(".tmp"), "{temps:?}");
    assert_eq!(std::fs::read_to_string(&temps[0]).unwrap(), "a re");

    write_atomic(&path, "a rewritten scene, longer than the old").expect("the retry lands whole");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "a rewritten scene, longer than the old"
    );
    assert!(
        leftovers(dir.path()).is_empty(),
        "this process's retry leaves no temp"
    );
}

#[test]
fn a_failed_replacement_removes_its_temp_and_keeps_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scene.unmei");
    std::fs::write(&path, "old contents").unwrap();

    {
        let _armed = faults::arm(faults::WriteFault::Rename);
        let error = write_atomic(&path, "new contents").unwrap_err();
        assert_eq!(error.to_string(), faults::RENAME_FAILURE);
    }

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old contents");
    assert!(
        leftovers(dir.path()).is_empty(),
        "the rename failure removes its temp"
    );

    write_atomic(&path, "new contents").expect("the retry lands whole");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new contents");
    assert!(leftovers(dir.path()).is_empty(), "the retry leaves no temp");
}

/// Splice fixtures: the demo pack's `ace.ron` with one edit, and a config to
/// write into it.
mod comment_fixtures {
    use unmei_engine::content::{Criteria, DecisionConfig, Fallback, Npc, QuestionType};

    pub(super) const ACE: &str = include_str!("../../../examples/demo-pack/npcs/ace.ron");

    pub(super) fn config() -> DecisionConfig {
        DecisionConfig {
            question_type: QuestionType::Choice,
            instructions: "A new instruction.".into(),
            state_keys: vec!["met_ace".into()],
            criteria: Criteria::Choice(
                [
                    ("open".to_string(), "x".to_string()),
                    ("shut".to_string(), "y".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            fallback: Fallback::Action("open".into()),
        }
    }

    /// `ace.ron` with `from` replaced by `to` exactly once.
    pub(super) fn ace_with(from: &str, to: &str) -> String {
        assert_eq!(
            ACE.matches(from).count(),
            1,
            "{from:?} occurs once in ace.ron"
        );
        ACE.replacen(from, to, 1)
    }

    /// Splice `question` into `src`, then require the result to read back as
    /// the original NPC with only that question set, keeping each of `kept`
    /// verbatim.
    pub(super) fn assert_spliced(src: &str, question: &str, kept: &[&str]) {
        let existing: Npc = ron::from_str(src).expect("the fixture parses");
        let out = super::super::splice_question(src, &existing, question, &config());
        let mut expected = existing;
        expected.decisions.insert(question.to_string(), config());
        let read: Result<Npc, _> = ron::from_str(&out);
        assert_eq!(read.ok(), Some(expected), "spliced file:\n{out}");
        for kept in kept {
            assert!(out.contains(kept), "{kept:?} survives in:\n{out}");
        }
    }
}

use comment_fixtures::{ace_with, assert_spliced};

/// A pack whose `npcs/` holds the demo `ace.ron` and one sibling file.
fn npc_pack() -> tempfile::TempDir {
    let pack = tempfile::tempdir().unwrap();
    let npcs = pack.path().join("npcs");
    std::fs::create_dir(&npcs).unwrap();
    std::fs::write(npcs.join("ace.ron"), comment_fixtures::ACE).unwrap();
    std::fs::write(npcs.join("bea.ron"), "// a sibling\n").unwrap();
    pack
}

/// Every file in the pack's `npcs/`, with its contents, in name order.
fn npc_files(pack: &Path) -> Vec<(PathBuf, String)> {
    let mut files: Vec<_> = std::fs::read_dir(pack.join("npcs"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect();
    files.sort();
    files
}

fn ace() -> Npc {
    ron::from_str(comment_fixtures::ACE).unwrap()
}

#[test]
fn a_replace_or_add_writes_the_npc_with_only_that_question_set() {
    for question in ["greet", "farewell"] {
        let pack = npc_pack();
        let config = comment_fixtures::config();
        apply_candidate(pack.path(), &ace(), question, &config, CreateMode::Replace).unwrap();
        let mut expected = ace();
        expected.decisions.insert(question.to_string(), config);
        let written = std::fs::read_to_string(pack.path().join("npcs/ace.ron")).unwrap();
        assert_eq!(
            ron::from_str::<Npc>(&written).ok(),
            Some(expected),
            "{question}"
        );
    }
}

#[test]
fn a_splice_that_would_not_read_back_is_refused_and_writes_nothing() {
    for fault in [
        faults::WriteFault::UnreadableSplice,
        faults::WriteFault::UnchangedSplice,
    ] {
        let pack = npc_pack();
        let before = npc_files(pack.path());
        let result = {
            let _armed = faults::arm(fault);
            apply_candidate(
                pack.path(),
                &ace(),
                "farewell",
                &comment_fixtures::config(),
                CreateMode::Replace,
            )
        };
        let error = result.expect_err("the splice is refused");
        assert!(error.to_string().contains("read back"), "{error}");
        assert_eq!(npc_files(pack.path()), before, "nothing is written");
    }
}

#[test]
fn a_comment_naming_the_key_before_the_npc_is_not_spliced_into() {
    let comment = "// old \"greet\": (draft)\n";
    let src = ace_with("(\n    id:", &format!("{comment}(\n    id:"));
    assert_spliced(&src, "greet", &[comment]);
}

#[test]
fn a_block_comment_with_a_brace_does_not_swallow_an_added_question() {
    let src = ace_with("        ),\n    },", "        ),\n        /* } */\n    },");
    assert_spliced(&src, "farewell", &["/* } */"]);
}

#[test]
fn a_block_comment_with_a_paren_inside_the_block_does_not_end_it() {
    let src = ace_with(
        "            fallback: \"cold\",",
        "            /* ) */\n            fallback: \"cold\",",
    );
    // The comment sits inside the replaced block, so it goes with the block.
    assert_spliced(&src, "greet", &[]);
}

#[test]
fn a_comment_with_a_paren_after_the_key_does_not_start_the_block() {
    let src = ace_with("\"greet\": (", "\"greet\": // (x)\n        (");
    assert_spliced(&src, "greet", &["// (x)"]);
}

#[test]
fn a_comment_after_the_decisions_naming_a_new_key_is_not_spliced_into() {
    let comment = "    // \"farewell\": (\n";
    let src = ace_with("    },\n)", &format!("    }},\n{comment})"));
    assert_spliced(&src, "farewell", &[comment]);
}

/// The splice over generated NPC files: comment lines inserted anywhere,
/// config strings holding brackets, quotes and comment markers, and a
/// question that is either replaced or added.
mod properties {
    use std::collections::HashMap;

    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};
    use unmei_engine::content::{Criteria, DecisionConfig, Fallback, Npc, QuestionType};

    /// Fragments that confuse a scanner reading RON by hand. The first 12
    /// hold no `/` or `*`, so a block comment can use them safely.
    const FRAGMENTS: [&str; 14] = [
        "x",
        " ",
        "(",
        ")",
        "{",
        "}",
        "[",
        "]",
        "\"",
        "\\",
        "\"q0\": (",
        "decisions: {",
        "//",
        "r#\"",
    ];
    const QUESTIONS: [&str; 3] = ["q0", "q1", "q2"];

    fn text_from(fragments: &'static [&'static str]) -> impl Strategy<Value = String> {
        prop::collection::vec(prop::sample::select(fragments), 0..6)
            .prop_map(|parts| parts.concat())
    }

    fn text() -> impl Strategy<Value = String> {
        text_from(&FRAGMENTS)
    }

    /// A comment on its own line. A block comment's text leaves out `/` and
    /// `*`, so only its own nested `/* … */` opens or closes it.
    fn comment() -> impl Strategy<Value = String> {
        let plain = || text_from(&FRAGMENTS[..12]);
        prop_oneof![
            text().prop_map(|text| format!("// {text}")),
            (plain(), prop::option::of(plain())).prop_map(|(outer, inner)| match inner {
                Some(inner) => format!("/* {outer} /* {inner} */ */"),
                None => format!("/* {outer} */"),
            }),
        ]
    }

    fn descriptions(keys: Vec<String>) -> impl Strategy<Value = HashMap<String, String>> {
        prop::collection::vec(text(), keys.len())
            .prop_map(move |texts| keys.clone().into_iter().zip(texts).collect())
    }

    fn config() -> impl Strategy<Value = DecisionConfig> {
        let choice = prop::sample::subsequence(vec!["a", "b", "q1"], 1..=3)
            .prop_flat_map(|ids| {
                let ids: Vec<String> = ids.into_iter().map(String::from).collect();
                let fallback = prop::sample::select(ids.clone());
                (descriptions(ids), fallback)
            })
            .prop_map(|(map, fallback)| {
                (
                    QuestionType::Choice,
                    Criteria::Choice(map),
                    Fallback::Action(fallback),
                )
            });
        let noul = (
            prop::option::of(descriptions(vec!["true".into(), "false".into()])),
            any::<bool>(),
        )
            .prop_map(|(map, fallback)| {
                (
                    QuestionType::Noul,
                    Criteria::Noul(map),
                    Fallback::Bool(fallback),
                )
            });
        let score = prop::collection::vec(text(), 2..=3).prop_flat_map(|levels| {
            let len = levels.len();
            (Just(levels), 0..len).prop_map(|(levels, index)| {
                (
                    QuestionType::Score,
                    Criteria::Score(levels),
                    Fallback::Index(index),
                )
            })
        });
        (
            text(),
            prop::sample::subsequence(vec!["met_ace", "trust"], 0..=2),
            prop_oneof![choice, noul, score],
        )
            .prop_map(
                |(instructions, keys, (question_type, criteria, fallback))| DecisionConfig {
                    question_type,
                    instructions,
                    state_keys: keys.into_iter().map(String::from).collect(),
                    criteria,
                    fallback,
                },
            )
    }

    fn npc() -> impl Strategy<Value = Npc> {
        (
            text(),
            prop::option::of(text()),
            prop::collection::hash_map(
                prop::sample::select(&QUESTIONS[..]).prop_map(String::from),
                config(),
                1..=3,
            ),
        )
            .prop_map(|(name, model, decisions)| Npc {
                id: "ace".into(),
                name,
                model,
                decisions,
            })
    }

    /// One generated splice.
    #[derive(Debug, Clone)]
    struct Case {
        npc: Npc,
        /// The NPC pretty-printed, with comment lines inserted.
        src: String,
        /// The text the splice must keep byte for byte: everything before
        /// and after `question`'s config, or before and after the decisions
        /// map's closing brace when the question is new.
        before: String,
        after: String,
        question: String,
        config: DecisionConfig,
    }

    fn case() -> impl Strategy<Value = Case> {
        npc().prop_flat_map(|npc| {
            let text = ron::ser::to_string_pretty(&npc, ron::ser::PrettyConfig::default())
                .expect("a generated NPC encodes");
            let lines = text.lines().count();
            let comments = prop::collection::vec((0..=lines, comment()), 0..6);
            let question = prop::sample::select(&QUESTIONS[..]).prop_map(String::from);
            (Just(npc), Just(text), comments, question, config()).prop_map(
                |(npc, text, mut comments, question, config)| {
                    comments.sort_by_key(|(line, _)| *line);
                    let mut lines: Vec<String> = text.lines().map(String::from).collect();
                    for (line, comment) in comments.iter().rev() {
                        lines.insert(*line, comment.clone());
                    }
                    let src = lines.join("\n");
                    let cut = untouched(&lines, &question);
                    Case {
                        before: src[..cut.0].to_string(),
                        after: src[cut.1..].to_string(),
                        npc,
                        src,
                        question,
                        config,
                    }
                },
            )
        })
    }

    /// The byte range of the joined `lines` a splice of `question` may
    /// change, read from the pretty-printed layout rather than by scanning
    /// RON. Each decision key sits alone at eight spaces with its config's
    /// `(` ending the line, and the config closes with `),` alone at the
    /// same indent; the decisions map closes with `},` at four. Inserted
    /// comment lines start with `//` or `/*`, so they match neither.
    fn untouched(lines: &[String], question: &str) -> (usize, usize) {
        let offset = |line: usize| lines[..line].iter().map(|l| l.len() + 1).sum::<usize>();
        let header = format!("        \"{question}\": (");
        match lines.iter().position(|line| *line == header) {
            Some(open) => {
                let close = open
                    + lines[open..]
                        .iter()
                        .position(|line| line == "        ),")
                        .expect("a pretty-printed config closes");
                (
                    offset(open) + header.len() - 1,
                    offset(close) + "        )".len(),
                )
            }
            None => {
                let close = lines
                    .iter()
                    .position(|line| line == "    },")
                    .expect("a pretty-printed decisions map closes");
                let at = offset(close) + "    ".len();
                (at, at)
            }
        }
    }

    proptest! {
        // A fixed seed keeps the workspace gate deterministic; a failure
        // becomes a named example test above, so nothing is persisted.
        #![proptest_config(Config {
            cases: 512,
            rng_seed: RngSeed::Fixed(0x756e_6d65_6902),
            failure_persistence: None,
            ..Config::default()
        })]

        /// Splicing one question leaves a file that reads back as the
        /// original NPC with only that question set, and every byte outside
        /// that question's config, comments included, is unchanged.
        #[test]
        fn a_splice_changes_only_the_one_config(case in case()) {
            let Case { npc, src, before, after, question, config } = case;
            let parsed: Npc = ron::from_str(&src).expect("the generated file parses");
            prop_assert_eq!(&parsed, &npc);
            let out = super::super::splice_question(&src, &npc, &question, &config);
            let mut expected = npc;
            expected.decisions.insert(question, config);
            let read: Result<Npc, _> = ron::from_str(&out);
            prop_assert_eq!(read.ok(), Some(expected), "spliced file:\n{}", out);
            prop_assert!(
                out.len() >= before.len() + after.len()
                    && out.starts_with(&before)
                    && out.ends_with(&after),
                "text outside the config changed:\n{out}"
            );
        }
    }
}
