//! The parser over generated scenes. A valid scene is built together with the
//! line list it must parse to, then written out with comment lines, blank
//! lines, indentation and line endings inserted anywhere; arbitrary token soup
//! must fail cleanly, never panic.

use std::collections::HashMap;

use proptest::prelude::*;

use super::Parser;
use crate::content::{CondOp, Condition, Line, Literal};
use crate::pack_ron::strategies::config;

/// One generated statement of a valid scene.
#[derive(Clone, Debug)]
enum Stmt {
    /// One source line and the line it parses to.
    Simple(String, Line),
    /// A top-level `label`; its name is assigned once the scene is built, so
    /// every label is unique.
    Label,
    /// A `choice` block's options: (display text, target label).
    Choice(Vec<(String, String)>),
    /// An `if` block, its condition's source text and parse, and its arms.
    If {
        source: String,
        condition: Condition,
        then: Vec<Stmt>,
        otherwise: Option<Vec<Stmt>>,
    },
}

fn pick(options: &'static [&'static str]) -> impl Strategy<Value = String> {
    prop::sample::select(options).prop_map(String::from)
}

/// Prose that looks like structure: a comment marker, keywords, a say-line
/// separator, an arrow and a well-formed `{var}` reference.
fn prose() -> impl Strategy<Value = String> {
    (
        pick(&["Hi", "#1", "if", "end", "choice", "{met_ace}"]),
        prop::collection::vec(
            pick(&[" ", "x", "# no", ": ", "->", "{met_ace}", "'", "end", "\t"]),
            0..5,
        ),
    )
        .prop_map(|(head, tail)| format!("{head}{}", tail.concat()).trim_end().to_string())
}

fn literal() -> impl Strategy<Value = (String, Literal)> {
    prop_oneof![
        any::<bool>().prop_map(|b| (b.to_string(), Literal::Bool(b))),
        (-50i32..50).prop_map(|n| (n.to_string(), Literal::Num(f64::from(n)))),
        prose().prop_map(|text| (format!("\"{text}\""), Literal::Str(text))),
    ]
}

fn simple() -> impl Strategy<Value = Stmt> {
    let id = || pick(&["ace", "school_gate", "music_morning", "neutral"]);
    let key = || pick(&["met_ace", "mood", "n"]);
    prop_oneof![
        id().prop_map(|id| Stmt::Simple(format!("bg {id}"), Line::Bg(id))),
        (id(), id()).prop_map(|(npc_id, pose)| Stmt::Simple(
            format!("sprite {npc_id} {pose}"),
            Line::Sprite { npc_id, pose }
        )),
        id().prop_map(|id| Stmt::Simple(format!("hide {id}"), Line::Hide(id))),
        id().prop_map(|id| Stmt::Simple(format!("play {id}"), Line::Play(id))),
        id().prop_map(|id| Stmt::Simple(format!("stop {id}"), Line::Stop(id))),
        (key(), literal()).prop_map(|(name, (text, value))| Stmt::Simple(
            format!("set {name} = {text}"),
            Line::Set { name, value }
        )),
        (pick(&["Ace", "Narrator", "Mira Ko"]), prose()).prop_map(|(speaker, text)| {
            Stmt::Simple(format!("{speaker}: {text}"), Line::Say { speaker, text })
        }),
        pick(&["after", "scene.end"])
            .prop_map(|label| Stmt::Simple(format!("jump {label}"), Line::Jump(label))),
        pick(&["reply", "ace.reply"]).prop_map(|prefix| Stmt::Simple(
            format!("decide ace.greet -> {prefix}"),
            Line::Decide {
                npc_id: "ace".into(),
                question: "greet".into(),
                prefix,
            }
        )),
    ]
}

/// A `choice` block. Option text never holds `->`: the parser splits an option
/// at its first arrow, even inside the quotes, and rejects the line.
fn choice() -> impl Strategy<Value = Stmt> {
    let text = (pick(&["", " "]), pick(&["Go", "Stay #2", "if", "a > b"]))
        .prop_map(|(pad, text)| format!("{pad}{text}{pad}"));
    prop::collection::vec((text, pick(&["after", "scene.end"])), 1..=3).prop_map(Stmt::Choice)
}

fn condition() -> impl Strategy<Value = (String, Condition)> {
    let compare = |key: &str, op: CondOp, value: Literal| Condition {
        key: key.into(),
        op,
        value,
        bare: false,
    };
    prop_oneof![
        Just((
            "met_ace".to_string(),
            Condition {
                key: "met_ace".into(),
                op: CondOp::Eq,
                value: Literal::Bool(true),
                bare: true,
            }
        )),
        (-5i32..5).prop_map(move |n| (
            format!("n >= {n}"),
            compare("n", CondOp::Ge, Literal::Num(f64::from(n)))
        )),
        Just((
            "mood == \"low\"".to_string(),
            compare("mood", CondOp::Eq, Literal::Str("low".into()))
        )),
        Just((
            "met_ace != false".to_string(),
            compare("met_ace", CondOp::Ne, Literal::Bool(false))
        )),
    ]
}

/// The statements of an `if` arm, `depth` levels of `if` deep at most. Arms
/// hold no labels: the parser refuses a label inside an arm.
fn arm(depth: u32) -> BoxedStrategy<Vec<Stmt>> {
    let stmt = if depth == 0 {
        prop_oneof![4 => simple(), 1 => choice()].boxed()
    } else {
        prop_oneof![4 => simple(), 1 => choice(), 1 => if_block(depth - 1)].boxed()
    };
    prop::collection::vec(stmt, 0..4).boxed()
}

fn if_block(depth: u32) -> BoxedStrategy<Stmt> {
    (condition(), arm(depth), prop::option::of(arm(depth)))
        .prop_map(|((source, condition), then, otherwise)| Stmt::If {
            source,
            condition,
            then,
            otherwise,
        })
        .boxed()
}

fn scene() -> impl Strategy<Value = Vec<Stmt>> {
    prop::collection::vec(
        prop_oneof![4 => simple(), 1 => Just(Stmt::Label), 1 => choice(), 2 => if_block(1)],
        0..10,
    )
}

/// A scene written out canonically: one statement per line, no layout.
#[derive(Default)]
struct Written {
    text: Vec<String>,
    lines: Vec<Line>,
    /// For each entry of `lines`, the 0-based index of its line in `text`.
    source: Vec<usize>,
    labels: HashMap<String, usize>,
}

impl Written {
    fn push(&mut self, text: String, line: Line) -> usize {
        self.lines.push(line);
        self.source.push(self.text.len());
        self.text.push(text);
        self.lines.len() - 1
    }

    /// Write `stmts`, filling in each `if`'s indices as `Line::If` and
    /// `Line::Else` document them: the else marker's index, and the index of
    /// the block's last line (the `if` itself for an empty block).
    fn write(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Simple(text, line) => {
                    self.push(text.clone(), line.clone());
                }
                Stmt::Label => {
                    let name = format!("label_{}", self.labels.len());
                    self.labels.insert(name.clone(), self.lines.len());
                    self.push(format!("label {name}"), Line::Label(name));
                }
                Stmt::Choice(options) => {
                    self.push("choice".into(), Line::Choice(options.clone()));
                    for (text, target) in options {
                        self.text.push(format!("\"{text}\" -> {target}"));
                    }
                    self.text.push("end".into());
                }
                Stmt::If {
                    source,
                    condition,
                    then,
                    otherwise,
                } => {
                    let if_index = self.push(
                        format!("if {source}"),
                        Line::If {
                            condition: condition.clone(),
                            else_index: None,
                            end_index: 0,
                        },
                    );
                    self.write(then);
                    let else_index = otherwise.as_ref().map(|arm| {
                        let at = self.push("else".into(), Line::Else { end_index: 0 });
                        self.write(arm);
                        at
                    });
                    self.text.push("endif".into());
                    let end = self.lines.len() - 1;
                    self.lines[if_index] = Line::If {
                        condition: condition.clone(),
                        else_index,
                        end_index: end,
                    };
                    if let Some(at) = else_index {
                        self.lines[at] = Line::Else { end_index: end };
                    }
                }
            }
        }
    }
}

/// Layout noise for a written scene: an indent and a trailing pad for each
/// line, extra lines inserted before given line numbers, and the line ending.
#[derive(Clone, Debug)]
struct Layout {
    pads: Vec<(&'static str, &'static str)>,
    extra: Vec<(usize, &'static str)>,
    crlf: bool,
}

const INDENTS: [&str; 5] = ["", "  ", "\t", "    ", "\u{a0}"];
const EXTRA_LINES: [&str; 7] = ["# note", "", "   ", "#", "  # if x", "\t# end", "# Ace: hi"];

fn layout(lines: usize) -> impl Strategy<Value = Layout> {
    let indent = || prop::sample::select(&INDENTS[..]);
    (
        prop::collection::vec((indent(), indent()), lines),
        prop::collection::vec((0..=lines, prop::sample::select(&EXTRA_LINES[..])), 0..8),
        any::<bool>(),
    )
        .prop_map(|(pads, extra, crlf)| Layout { pads, extra, crlf })
}

/// Apply `layout` to `text`; returns the file and, for each line of `text`,
/// its 1-based line number in the file.
fn lay_out(text: &[String], layout: &Layout) -> (String, Vec<usize>) {
    let mut extra = layout.extra.clone();
    extra.sort_by_key(|(before, _)| *before);
    let mut file = Vec::new();
    let mut numbers = Vec::new();
    let mut pending = extra.iter().peekable();
    for (index, line) in text.iter().enumerate() {
        while let Some((_, extra_line)) = pending.next_if(|(before, _)| *before == index) {
            file.push(extra_line.to_string());
        }
        let (indent, pad) = layout.pads[index];
        file.push(format!("{indent}{line}{pad}"));
        numbers.push(file.len());
    }
    file.extend(pending.map(|(_, extra_line)| extra_line.to_string()));
    let ending = if layout.crlf { "\r\n" } else { "\n" };
    (file.join(ending), numbers)
}

/// Text built from scene tokens, valid or not.
fn token_soup() -> impl Strategy<Value = String> {
    prop::collection::vec(
        pick(&[
            "if",
            "else",
            "endif",
            "choice",
            "end",
            "set",
            "label",
            "jump",
            "decide",
            "bg",
            "sprite",
            "x",
            "n",
            "Ace",
            ".",
            ":",
            ": ",
            "\"",
            "->",
            "=",
            "==",
            ">=",
            "{",
            "}",
            "{met_ace}",
            "#",
            " ",
            "\t",
            "\n",
            "\r\n",
            "1e999",
            "\u{a0}",
        ]),
        0..40,
    )
    .prop_map(|tokens| tokens.concat())
}

proptest! {
    #![proptest_config(config())]

    /// A generated scene parses to exactly the lines it was built from, with
    /// every `if`/`else` index and label in place, whatever comments, blank
    /// lines, indentation and line endings surround it; each parsed line's
    /// source line number is that line's own.
    #[test]
    fn a_scene_parses_to_its_lines_under_any_layout(
        (stmts, layout) in scene().prop_flat_map(|stmts| {
            let mut written = Written::default();
            written.write(&stmts);
            let lines = written.text.len();
            (Just(stmts), layout(lines))
        })
    ) {
        let mut written = Written::default();
        written.write(&stmts);
        let (file, numbers) = lay_out(&written.text, &layout);
        let parsed = Parser::new("generated.unmei").parse("generated", &file);
        let scene = match parsed {
            Ok(scene) => scene,
            Err(error) => return Err(TestCaseError::fail(format!("{error}\n{file}"))),
        };
        prop_assert_eq!(&scene.lines, &written.lines, "{}", file);
        prop_assert_eq!(&scene.labels, &written.labels, "{}", file);
        let expected: Vec<usize> = written.source.iter().map(|&line| numbers[line]).collect();
        prop_assert_eq!(&scene.line_numbers, &expected, "{}", file);
    }

    /// Any text built from scene tokens parses or fails with an error whose
    /// line number lies within the text; it never panics.
    #[test]
    fn token_soup_fails_cleanly(text in token_soup()) {
        if let Err(error) = Parser::new("soup.unmei").parse("soup", &text) {
            let last = text.lines().count().max(1);
            prop_assert!(
                error.line.is_some_and(|line| (1..=last).contains(&line)),
                "line {:?} outside 1..={last}: {error}\n{text:?}",
                error.line
            );
        }
    }
}
