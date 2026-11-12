//! Parser for the `.unmei` dialogue DSL.
//!
//! Line-oriented: one command per line, `#` comments, cosmetic indentation.
//! A line is a **say-line** iff it matches `Speaker: text` (a single colon
//! surrounded by spaces); every other non-blank line must start with a keyword.
//! Branching is player `choice`, `decide`, and flat `if`/`else`/`endif` blocks
//! whose arms are addressed by pc indices, so there is no runtime block stack
//! ([ADR 0002], [ADR 0004]).
//!
//! This root holds the line loop; the `syntax` submodule owns the keyword,
//! say-line and id/label grammar, `commands` a command's argument grammar and
//! `flow` the block scanners. `KEYWORDS`, `is_keyword`, `check_id` and
//! `split_say_line` are re-exported here because the author CLI consumes them
//! at this path.

mod commands;
mod flow;
mod syntax;

use std::collections::HashMap;

use crate::content::{Line, Scene};
use crate::error::LoadError;

use self::commands::{parse_condition, parse_decide, parse_set, two_args};
use self::flow::FlowBlocks;
use self::syntax::{check_label, validate_interpolation};

pub use self::syntax::{check_id, is_keyword, split_say_line, KEYWORDS};

/// Per-file parser. Tracks the pack-relative file name for error messages.
pub struct Parser<'a> {
    file: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(file: &'a str) -> Self {
        Parser { file }
    }

    fn err(&self, line_no: usize, message: impl Into<String>) -> LoadError {
        LoadError::new(self.file, Some(line_no), message)
    }

    /// Parse the full text of one `.unmei` scene.
    pub fn parse(&self, scene_id: &str, text: &str) -> Result<Scene, LoadError> {
        let src: Vec<&str> = text.lines().collect();
        let mut lines: Vec<Line> = Vec::new();
        let mut line_numbers: Vec<usize> = Vec::new();
        let mut labels: HashMap<String, usize> = HashMap::new();
        // Open `if` blocks, outermost first; `endif` closes the innermost.
        let mut flow = FlowBlocks::default();

        let mut i = 0;
        while i < src.len() {
            let line_no = i + 1;
            let trimmed = src[i].trim();

            // Comments and blank lines.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }

            // Keyword command? (checked first: a `set` value may contain ": ")
            let keyword = trimmed.split_whitespace().next().unwrap();
            let rest = trimmed[keyword.len()..].trim();
            let parsed = match keyword {
                "if" => {
                    let condition = parse_condition(rest).map_err(|m| self.err(line_no, m))?;
                    flow.open(condition, line_no, &mut lines, &mut line_numbers);
                    i += 1;
                    continue;
                }
                "else" => {
                    if !rest.is_empty() {
                        return Err(self.err(line_no, "`else` takes no arguments"));
                    }
                    flow.else_arm(line_no, &mut lines, &mut line_numbers)
                        .map_err(|m| self.err(line_no, m))?;
                    i += 1;
                    continue;
                }
                "endif" => {
                    if !rest.is_empty() {
                        return Err(self.err(line_no, "`endif` takes no arguments"));
                    }
                    flow.close(&mut lines).map_err(|m| self.err(line_no, m))?;
                    i += 1;
                    continue;
                }
                "bg" => {
                    check_id(rest, "background id").map_err(|m| self.err(line_no, m))?;
                    Some(Line::Bg(rest.to_string()))
                }
                "sprite" => {
                    let (npc_id, pose) =
                        two_args(rest, "sprite").map_err(|m| self.err(line_no, m))?;
                    check_id(npc_id, "npc id").map_err(|m| self.err(line_no, m))?;
                    check_id(pose, "pose id").map_err(|m| self.err(line_no, m))?;
                    Some(Line::Sprite {
                        npc_id: npc_id.to_string(),
                        pose: pose.to_string(),
                    })
                }
                "hide" => {
                    check_id(rest, "npc id").map_err(|m| self.err(line_no, m))?;
                    Some(Line::Hide(rest.to_string()))
                }
                "play" => {
                    check_id(rest, "sound id").map_err(|m| self.err(line_no, m))?;
                    Some(Line::Play(rest.to_string()))
                }
                "stop" => {
                    check_id(rest, "sound id").map_err(|m| self.err(line_no, m))?;
                    Some(Line::Stop(rest.to_string()))
                }
                "set" => {
                    let (name, value) = parse_set(rest).map_err(|m| self.err(line_no, m))?;
                    Some(Line::Set { name, value })
                }
                "label" => {
                    if flow.is_open() {
                        return Err(self.err(
                            line_no,
                            "`label` is not allowed inside an `if` arm (no jump may enter a block past its condition)",
                        ));
                    }
                    check_label(rest).map_err(|m| self.err(line_no, m))?;
                    if labels.contains_key(rest) {
                        return Err(self.err(line_no, format!("duplicate label `{rest}`")));
                    }
                    labels.insert(rest.to_string(), lines.len());
                    Some(Line::Label(rest.to_string()))
                }
                "jump" => {
                    check_label(rest).map_err(|m| self.err(line_no, m))?;
                    Some(Line::Jump(rest.to_string()))
                }
                "decide" => {
                    let (npc_id, question, prefix) =
                        parse_decide(rest).map_err(|m| self.err(line_no, m))?;
                    Some(Line::Decide {
                        npc_id,
                        question,
                        prefix,
                    })
                }
                "choice" => {
                    if !rest.is_empty() {
                        return Err(self.err(line_no, "`choice` takes no arguments"));
                    }
                    let (options, end_index) = self.parse_choice_block(&src, i)?;
                    lines.push(Line::Choice(options));
                    line_numbers.push(line_no);
                    i = end_index + 1;
                    continue;
                }
                _ => None,
            };

            if let Some(line) = parsed {
                lines.push(line);
                line_numbers.push(line_no);
                i += 1;
                continue;
            }

            // Say-line fallback: `Speaker: text`.
            if let Some((speaker, text)) = split_say_line(trimmed) {
                validate_interpolation(&text).map_err(|m| self.err(line_no, m))?;
                lines.push(Line::Say { speaker, text });
                line_numbers.push(line_no);
                i += 1;
                continue;
            }

            return Err(self.err(
                line_no,
                format!("unknown command `{keyword}` (say-lines are `Speaker: text` with a space after the colon)"),
            ));
        }

        if let Some(open_line) = flow.unclosed() {
            return Err(self.err(open_line, "`if` block has no matching `endif`"));
        }

        Ok(Scene {
            id: scene_id.to_string(),
            lines,
            line_numbers,
            labels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Scene, LoadError> {
        Parser::new("test.unmei").parse("test", text)
    }

    fn demo_pack() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack")
    }

    #[test]
    fn demo_first_meeting_parses() {
        let text = std::fs::read_to_string(demo_pack().join("scenes/first_meeting.unmei"))
            .expect("demo pack scene exists");
        let scene = parse(&text).expect("parses");
        assert_eq!(scene.id, "test");
        assert!(scene.labels.contains_key("ask_letter"));
        assert!(scene.labels.contains_key("silence"));
        assert!(scene.labels.contains_key("converge"));
        // A say-line from Ace exists.
        assert!(scene
            .lines
            .iter()
            .any(|l| matches!(l, Line::Say { speaker, .. } if speaker == "Ace")));
        // Choice block has two options.
        assert!(scene
            .lines
            .iter()
            .any(|l| matches!(l, Line::Choice(opts) if opts.len() == 2)));
    }

    #[test]
    fn demo_reunion_parses() {
        let text = std::fs::read_to_string(demo_pack().join("scenes/reunion.unmei"))
            .expect("demo pack scene exists");
        let scene = parse(&text).expect("parses");
        assert!(scene
            .lines
            .iter()
            .any(|l| matches!(l, Line::Decide { npc_id, question, prefix }
                if npc_id == "ace" && question == "greet" && prefix == "reply")));
        assert!(scene.labels.contains_key("reply.warm"));
        assert!(scene.labels.contains_key("reply.cold"));
        assert!(scene.labels.contains_key("reply.none"));
    }

    #[test]
    fn staging_commands_parse() {
        let scene = parse(
            "bg school_gate\nsprite ace neutral\nhide ace\nplay music_morning\nstop music_morning\n",
        )
        .unwrap();
        assert_eq!(scene.lines[0], Line::Bg("school_gate".into()));
        assert_eq!(
            scene.lines[1],
            Line::Sprite {
                npc_id: "ace".into(),
                pose: "neutral".into()
            }
        );
        assert_eq!(scene.lines[2], Line::Hide("ace".into()));
        assert_eq!(scene.lines[3], Line::Play("music_morning".into()));
        assert_eq!(scene.lines[4], Line::Stop("music_morning".into()));
    }

    #[test]
    fn duplicate_label_is_error() {
        let err = parse("label top\nlabel top\n").unwrap_err();
        assert!(err.message.contains("duplicate label"), "{err}");
    }

    #[test]
    fn comments_and_blanks_skipped() {
        let scene = parse("# a comment\n\n   # indented comment\nlabel top\n").unwrap();
        assert_eq!(scene.lines.len(), 1);
    }

    #[test]
    fn line_numbers_track_source() {
        let scene = parse("# c\n\nbg gate\nlabel top\n").unwrap();
        assert_eq!(scene.line_numbers[0], 3);
        assert_eq!(scene.line_numbers[1], 4);
    }

    #[test]
    fn label_index_points_at_label_line() {
        let scene = parse("set a = 1\nlabel top\nset b = 2\n").unwrap();
        assert_eq!(scene.label_index("top"), Some(1));
        assert_eq!(scene.label_index("missing"), None);
    }
}
