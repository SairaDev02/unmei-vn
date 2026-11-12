//! Regression protection for two boundaries that an earlier parser attempt
//! showed the suite did not distinguish:
//!
//! - a quoted condition value may itself contain an operator spelling. The
//!   contract is `<state key> <op> <literal>`, key always on the left, with
//!   `==`/`!=` on every scalar kind (`docs/content-format.md` § Conditionals),
//!   so `if mood != "a == b"` compares `mood` to the text `a == b`;
//! - a `choice` option is `"<text>" -> <target>` (`docs/content-format.md`
//!   § Commands), so empty text, an embedded quote, or a missing target is a
//!   load error rather than a silently blank menu entry.
//!
//! Both run through `load_pack`, so they observe pack-load behaviour instead of
//! the parser's private helpers.

use std::fs;
use std::path::{Path, PathBuf};

use unmei_engine::{load_pack, CondOp, Line, Literal};

const PACK_RON: &str = r#"(
    name: "condition protection",
    entry: "scene_a",
)"#;

fn write_pack(dir: &Path, files: &[(&str, String)]) -> PathBuf {
    fs::create_dir_all(dir.join("npcs")).unwrap();
    for (rel, content) in files {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    dir.to_path_buf()
}

/// A single-scene pack whose state declares `mood` as a string.
fn pack_with_scene(scene: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = write_pack(
        tmp.path(),
        &[
            ("pack.ron", PACK_RON.to_string()),
            ("state.ron", "(mood: \"calm\")".to_string()),
            ("scenes/scene_a.unmei", scene.to_string()),
        ],
    );
    (tmp, root)
}

#[test]
fn quoted_condition_values_may_contain_an_operator_spelling() {
    for (source, expected_op, expected_value) in [
        ("if mood == \"a != b\"", CondOp::Eq, "a != b"),
        ("if mood != \"a == b\"", CondOp::Ne, "a == b"),
        ("if mood == \"a < b\"", CondOp::Eq, "a < b"),
        ("if mood != \"a >= b\"", CondOp::Ne, "a >= b"),
    ] {
        let (_tmp, root) = pack_with_scene(&format!("{source}\nNarrator: Yes.\nendif\n"));
        let loaded = load_pack(&root).unwrap_or_else(|errors| panic!("{source}: {errors:?}"));
        match &loaded.pack.scenes["scene_a"].lines[0] {
            Line::If { condition, .. } => {
                assert_eq!(condition.key, "mood", "{source}");
                assert_eq!(condition.op, expected_op, "{source}");
                assert_eq!(
                    condition.value,
                    Literal::Str(expected_value.to_string()),
                    "{source}"
                );
                assert!(!condition.bare, "{source}");
            }
            other => panic!("{source}: expected an if, got {other:?}"),
        }
    }
}

#[test]
fn choice_options_need_quoted_text_and_a_target() {
    for option in [
        "  \"\" -> greeted",
        "  \"say \"hi\"\" -> greeted",
        "  \"Hi\" ->",
    ] {
        let (_tmp, root) = pack_with_scene(&format!(
            "choice\n{option}\nend\nlabel greeted\nNarrator: Hi.\n"
        ));
        let errors = load_pack(&root).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("expected `\"<text>\" -> <label>`")),
            "{option}: {errors:?}"
        );
    }
}
