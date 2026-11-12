//! Flow-block resolution: the `if`/`else`/`endif` markers and the flat
//! `choice` ... `end` option list, both resolved to absolute line indices while
//! parsing, so there is no runtime block stack ([ADR 0004]).
//!
//! The line loop in [`super`] keeps the file's text order; this module owns the
//! open-block stack and patches each block's pc targets when it closes.

use crate::content::{Condition, Line};
use crate::error::LoadError;

use super::commands::parse_choice_option;
use super::syntax::check_label;
use super::Parser;

/// An open `if` block during parsing; its matching `endif` fills in the pc
/// targets after the whole block has been scanned.
struct PendingIf {
    /// Index of the `Line::If` in `lines`.
    if_index: usize,
    /// Index of the `Line::Else`, once the arm separator is seen.
    else_index: Option<usize>,
    /// Source line of the `if`, for the unclosed-block error.
    open_line: usize,
}

/// The `if` blocks open in one scene parse, outermost first.
#[derive(Default)]
pub(super) struct FlowBlocks {
    /// Its length is the nesting depth — there is no indent stack and no depth
    /// cap.
    pending: Vec<PendingIf>,
}

impl FlowBlocks {
    /// Open an `if` block: append its marker and remember what the matching
    /// `endif` must fill in.
    pub(super) fn open(
        &mut self,
        condition: Condition,
        line_no: usize,
        lines: &mut Vec<Line>,
        line_numbers: &mut Vec<usize>,
    ) {
        let if_index = lines.len();
        lines.push(Line::If {
            condition,
            else_index: None,
            end_index: if_index,
        });
        line_numbers.push(line_no);
        self.pending.push(PendingIf {
            if_index,
            else_index: None,
            open_line: line_no,
        });
    }

    /// True while a block is open, so no `label` can land inside an arm.
    pub(super) fn is_open(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Record the `else` arm separator of the innermost open block.
    pub(super) fn else_arm(
        &mut self,
        line_no: usize,
        lines: &mut Vec<Line>,
        line_numbers: &mut Vec<usize>,
    ) -> Result<(), String> {
        let Some(open) = self.pending.last_mut() else {
            return Err("`else` without a matching `if`".into());
        };
        if open.else_index.is_some() {
            return Err(
                "duplicate `else` in one `if` block (there is no `elif`; nest instead)".into(),
            );
        }
        let else_index = lines.len();
        lines.push(Line::Else {
            end_index: else_index,
        });
        line_numbers.push(line_no);
        let if_index = open.if_index;
        open.else_index = Some(else_index);
        if let Line::If {
            else_index: slot, ..
        } = &mut lines[if_index]
        {
            *slot = Some(else_index);
        }
        Ok(())
    }

    /// Close the innermost open block at `endif`, filling in the pc targets of
    /// both markers.
    pub(super) fn close(&mut self, lines: &mut [Line]) -> Result<(), String> {
        let Some(open) = self.pending.pop() else {
            return Err("`endif` without a matching `if`".into());
        };
        // The block's last line is everything pushed since the `if`; an empty
        // block's last line is the `if` itself.
        let end_index = lines.len().saturating_sub(1).max(open.if_index);
        if let Line::If {
            end_index: slot, ..
        } = &mut lines[open.if_index]
        {
            *slot = end_index;
        }
        if let Some(else_index) = open.else_index {
            if let Line::Else { end_index: slot } = &mut lines[else_index] {
                *slot = end_index;
            }
        }
        Ok(())
    }

    /// The source line of the outermost block still open at end of scene.
    pub(super) fn unclosed(&self) -> Option<usize> {
        self.pending.first().map(|open| open.open_line)
    }
}

impl<'a> Parser<'a> {
    /// Parse a `choice` ... `end` block whose `choice` line is at `start` (0-based).
    /// Returns the options and the 0-based index of the `end` line.
    pub(super) fn parse_choice_block(
        &self,
        src: &[&'a str],
        start: usize,
    ) -> Result<(Vec<(String, String)>, usize), LoadError> {
        let mut options = Vec::new();
        let mut i = start + 1;
        while i < src.len() {
            let line_no = i + 1;
            let trimmed = src[i].trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            if trimmed == "end" {
                if options.is_empty() {
                    return Err(self.err(line_no, "`choice` block has no options"));
                }
                return Ok((options, i));
            }
            if trimmed == "choice" {
                return Err(self.err(line_no, "nested `choice` blocks are not supported"));
            }
            // Option lists stay flat: a conditional inside a `choice` block has
            // no meaning — wrap the whole block in an arm instead.
            let token = trimmed.split_whitespace().next().unwrap_or("");
            if matches!(token, "if" | "else" | "endif") {
                return Err(self.err(
                    line_no,
                    format!(
                        "`{token}` is not allowed inside a `choice` block (option lists stay flat; wrap the whole block in an arm instead)"
                    ),
                ));
            }
            let opt = parse_choice_option(trimmed).ok_or_else(|| {
                self.err(
                    line_no,
                    format!("expected `\"<text>\" -> <label>`, got: {trimmed}"),
                )
            })?;
            check_label(&opt.1).map_err(|m| self.err(line_no, m))?;
            options.push(opt);
            i += 1;
        }
        Err(self.err(start + 1, "`choice` block has no `end`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::content::Scene;

    fn parse(text: &str) -> Result<Scene, LoadError> {
        Parser::new("test.unmei").parse("test", text)
    }

    #[test]
    fn conditional_block_parses_and_labels_index_flat() {
        let scene = parse(
            "if affinity_level == \"high\"\nAce: Warm.\nelse\nAce: Cold.\nendif\nlabel after\n",
        )
        .unwrap();
        // If, then say, Else, else say, label; `endif` is consumed.
        assert_eq!(scene.lines.len(), 5);
        assert!(matches!(scene.lines[0], Line::If { .. }));
        assert!(matches!(scene.lines[2], Line::Else { .. }));
        assert_eq!(scene.label_index("after"), Some(4));
    }

    #[test]
    fn if_without_endif_is_error() {
        let err = parse("if met_ace\nAce: Hello.\n").unwrap_err();
        assert!(err.message.contains("no matching `endif`"), "{err}");
        assert_eq!(err.line, Some(1));
        assert_eq!(err.file, "test.unmei");
    }

    #[test]
    fn stray_else_is_error() {
        let err = parse("Ace: Hi.\nelse\n").unwrap_err();
        assert!(err.message.contains("without a matching `if`"), "{err}");
        assert_eq!(err.line, Some(2));
    }

    #[test]
    fn doubled_else_is_error() {
        let err = parse("if met_ace\nelse\nelse\nendif\n").unwrap_err();
        assert!(err.message.contains("duplicate `else`"), "{err}");
        assert_eq!(err.line, Some(3));
    }

    #[test]
    fn stray_endif_is_error() {
        let err = parse("endif\n").unwrap_err();
        assert!(err.message.contains("without a matching `if`"), "{err}");
        assert_eq!(err.line, Some(1));
    }

    #[test]
    fn label_inside_arm_is_error() {
        let err = parse("if met_ace\nlabel inside\nendif\n").unwrap_err();
        assert!(err.message.contains("`label` is not allowed"), "{err}");
        assert_eq!(err.line, Some(2));
    }

    #[test]
    fn label_in_an_else_arm_is_error() {
        let err = parse("if met_ace\nelse\nlabel inside\nendif\n").unwrap_err();
        assert!(err.message.contains("`label` is not allowed"), "{err}");
        assert_eq!(err.line, Some(3));
    }

    #[test]
    fn empty_arms_parse() {
        // An `if` with an empty then-arm, and one with both arms empty, are
        // structurally valid; only the markers remain flat.
        let then_empty = parse("if met_ace\nendif\nNarrator: After.\n").unwrap();
        assert_eq!(then_empty.lines.len(), 2);
        let both_empty = parse("if met_ace\nelse\nendif\nNarrator: After.\n").unwrap();
        assert_eq!(both_empty.lines.len(), 3);
    }

    #[test]
    fn nested_if_parses_with_flat_indices() {
        let scene = parse(
            "if met_ace\nif affinity_level == \"high\"\nAce: Both.\nendif\nendif\nNarrator: After.\n",
        )
        .unwrap();
        // If, If, say — both `endif`s are consumed as markers.
        assert_eq!(scene.lines.len(), 4);
        match (&scene.lines[0], &scene.lines[1]) {
            (Line::If { end_index, .. }, Line::If { .. }) => assert_eq!(*end_index, 2),
            other => panic!("expected nested ifs, got {other:?}"),
        }
    }

    #[test]
    fn conditional_inside_a_choice_block_is_error() {
        for token in ["if met_ace", "else", "endif"] {
            let src = format!("choice\n  \"a\" -> a\n  {token}\nend\n");
            let err = parse(&src).unwrap_err();
            assert!(
                err.message.contains("not allowed inside a `choice` block"),
                "{err}"
            );
            assert_eq!(err.line, Some(3));
        }
    }

    #[test]
    fn choice_block_parses() {
        let scene = parse(
            "choice\n  \"Ask about the letter\" -> ask_letter\n  \"Stay silent\" -> silence\nend\n",
        )
        .unwrap();
        match &scene.lines[0] {
            Line::Choice(opts) => {
                assert_eq!(
                    opts[0],
                    ("Ask about the letter".into(), "ask_letter".into())
                );
                assert_eq!(opts[1], ("Stay silent".into(), "silence".into()));
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn choice_block_lines_after_end_parse_normally() {
        let scene = parse("choice\n  \"Go\" -> go\nend\nlabel go\nset x = 1\n").unwrap();
        assert_eq!(scene.lines.len(), 3);
        assert!(scene.labels.contains_key("go"));
    }

    #[test]
    fn choice_without_end_is_error() {
        let err = parse("choice\n  \"Go\" -> go\n").unwrap_err();
        assert!(err.message.contains("no `end`"), "{err}");
    }

    #[test]
    fn choice_with_no_options_is_error() {
        let err = parse("choice\nend\n").unwrap_err();
        assert!(err.message.contains("no options"), "{err}");
    }

    #[test]
    fn nested_choice_is_error() {
        let err = parse("choice\n  \"a\" -> a\n  choice\nend\n").unwrap_err();
        assert!(err.message.contains("nested"), "{err}");
    }
}
