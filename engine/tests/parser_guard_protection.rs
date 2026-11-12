//! Regression protection for two parser guards in `engine/src/parser/syntax.rs`:
//!
//! - a say-line needs a non-empty speaker with no `:` before the first `": "`
//!   (`parser::split_say_line`'s contract), so `": hi"` and `"A:B: hi"` are load
//!   errors rather than `Say` nodes with an empty or colon-bearing speaker;
//! - labels are `[a-zA-Z0-9_.]+` (`docs/content-format.md` § Commands), so
//!   `label bad-label` is a load error.
//!
//! Both go through the public `Parser`, the layer where the guards live.

use unmei_engine::{Parser, Scene};

fn parse(text: &str) -> Result<Scene, unmei_engine::LoadError> {
    Parser::new("test.unmei").parse("test", text)
}

#[test]
fn say_line_rejects_an_empty_or_colon_bearing_speaker() {
    for text in [": hi\n", "A:B: hi\n"] {
        let err = parse(text).expect_err("not a say-line");
        assert!(err.message.contains("unknown command"), "{text:?}: {err}");
    }
}

#[test]
fn label_characters_outside_the_grammar_are_an_error() {
    let err = parse("label bad-label\n").expect_err("malformed label must not parse");
    assert!(err.message.contains("is not [a-zA-Z0-9_.]+"), "{err}");
}
