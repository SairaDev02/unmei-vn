//! The case-file parser: the version line, comments, header-vs-step order,
//! quoted tokens and one item per line (spec §2.1–§2.2, §3).
//!
//! The parser is strict on purpose. It never guesses at a misspelt verb, a
//! malformed duration or an unbalanced quote: each is a `keiko.case.syntax`
//! finding naming the line, so a broken case is caught before a take.

use std::time::Duration;

use super::model::{
    Case, DecisionFault, Fault, FixtureKind, Header, HeaderLine, KeyCheckFault, LoadTarget,
    StepLine, WriteTarget,
};
use super::parse_step::parse_step;
use crate::finding::{rules, Finding};
use crate::save::Slot;

/// The one case-format version this build reads.
pub const CASE_VERSION: u32 = 1;

/// Parse one case file. `file` is the path a finding names (the case's own
/// path, relative to the pack).
///
/// The version line is checked first and alone: when it is missing or names an
/// unknown version, the rest of the file is not parsed (spec §2.1).
pub fn parse_case(file: &str, text: &str) -> Result<Case, Finding> {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    let first = first.strip_suffix('\r').unwrap_or(first);
    if first != format!("keiko {CASE_VERSION}") {
        return Err(Finding::at_line(
            file,
            1,
            rules::CASE_VERSION,
            format!("case must start with `keiko {CASE_VERSION}`"),
        ));
    }

    let mut case = Case::default();
    let mut saw_step = false;
    for (index, raw) in lines.enumerate() {
        let number = index + 2;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let tokens = tokenize(file, number, trimmed)?;
        let Some((verb, rest)) = tokens.split_first() else {
            continue;
        };
        let Token::Bare(verb) = verb else {
            return Err(syntax(file, number, "a step must start with a verb"));
        };

        if let Some(header) = parse_header(verb.as_str(), rest, file, number)? {
            if saw_step {
                return Err(Finding::at_line(
                    file,
                    number,
                    rules::CASE_HEADER_ORDER,
                    "a header line must come before the first step",
                ));
            }
            case.headers.push(HeaderLine {
                line: number,
                header,
            });
            continue;
        }

        saw_step = true;
        let step = parse_step(verb.as_str(), rest, file, number)?;
        case.steps.push(StepLine { line: number, step });
    }
    Ok(case)
}

/// One parsed token: bare words and quoted strings are distinct, so a quoted
/// `contains` is data and a bare one is grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Token {
    Bare(String),
    Quoted(String),
}

/// The RON text of a state value: a quoted string keeps its quotes (and its
/// escapes), while a bare token is already RON's form for a number or boolean.
pub(super) fn value_text(token: &Token) -> String {
    match token {
        Token::Quoted(text) => {
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        }
        Token::Bare(text) => text.clone(),
    }
}

fn tokenize(file: &str, line: usize, text: &str) -> Result<Vec<Token>, Finding> {
    let mut tokens = Vec::new();
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Ok(tokens);
        }
        if let Some(after) = rest.strip_prefix('"') {
            let mut value = String::new();
            let mut chars = after.char_indices();
            let mut end = None;
            while let Some((index, ch)) = chars.next() {
                match ch {
                    '\\' => match chars.next() {
                        Some((_, '"')) => value.push('"'),
                        Some((_, '\\')) => value.push('\\'),
                        _ => {
                            return Err(syntax(
                                file,
                                line,
                                "only `\\\"` and `\\\\` are escapes in a quoted string",
                            ))
                        }
                    },
                    '"' => {
                        end = Some(index + 1);
                        break;
                    }
                    other => value.push(other),
                }
            }
            let Some(end) = end else {
                return Err(syntax(file, line, "unterminated quoted string"));
            };
            tokens.push(Token::Quoted(value));
            rest = &after[end..];
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, tail) = rest.split_at(end);
        tokens.push(Token::Bare(word.to_string()));
        rest = tail;
    }
}

/// Parse a header line, or return `None` when the verb is not a header.
fn parse_header(
    verb: &str,
    rest: &[Token],
    file: &str,
    line: usize,
) -> Result<Option<Header>, Finding> {
    let header = match verb {
        "decide" => {
            let [point, action] = rest else {
                return Err(syntax(file, line, "decide <npc>.<question> <action>"));
            };
            Header::Decide {
                point: bare_at(point, file, line)?.to_string(),
                action: bare_at(action, file, line)?.to_string(),
            }
        }
        "setting" => {
            let [key, value] = rest else {
                return Err(syntax(file, line, "setting <key> <value>"));
            };
            Header::Setting {
                key: bare_at(key, file, line)?.to_string(),
                value: bare_at(value, file, line)?.to_string(),
            }
        }
        "fixture" => {
            let [kind, path] = rest else {
                return Err(syntax(file, line, "fixture <kind> <path>"));
            };
            Header::Fixture {
                kind: parse_fixture_kind(bare_at(kind, file, line)?, file, line)?,
                path: bare_at(path, file, line)?.to_string(),
            }
        }
        "fault" => Header::Fault {
            fault: parse_fault(rest, file, line)?,
        },
        "dev" => {
            if !rest.is_empty() {
                return Err(syntax(file, line, "dev"));
            }
            Header::Dev
        }
        "start" => {
            let [scene] = rest else {
                return Err(syntax(file, line, "start <scene-id>"));
            };
            Header::Start {
                scene: bare_at(scene, file, line)?.to_string(),
            }
        }
        "state" => {
            let [key, equals, value] = rest else {
                return Err(syntax(file, line, "state <key> = <value>"));
            };
            if bare_at(equals, file, line)? != "=" {
                return Err(syntax(file, line, "expected `=` in a state header"));
            }
            Header::State {
                key: bare_at(key, file, line)?.to_string(),
                value: value_text(value),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(header))
}

pub(super) fn syntax(file: &str, line: usize, message: impl Into<String>) -> Finding {
    Finding::at_line(file, line, rules::CASE_SYNTAX, message)
}

pub(super) fn bare_at<'a>(token: &'a Token, file: &str, line: usize) -> Result<&'a str, Finding> {
    match token {
        Token::Bare(text) => Ok(text),
        Token::Quoted(_) => Err(syntax(
            file,
            line,
            "expected a bare word, not a quoted string",
        )),
    }
}

pub(super) fn quoted_only(
    rest: &[Token],
    file: &str,
    line: usize,
    form: &str,
) -> Result<String, Finding> {
    match rest {
        [Token::Quoted(text)] => Ok(text.clone()),
        _ => Err(syntax(file, line, format!("expected `{form}`"))),
    }
}

pub(super) fn quoted_unless_none(
    token: &Token,
    file: &str,
    line: usize,
) -> Result<String, Finding> {
    match token {
        Token::Quoted(text) => Ok(text.clone()),
        Token::Bare(_) => Err(syntax(file, line, "choice texts are quoted strings")),
    }
}

pub(super) fn open_closed(rest: &[Token], file: &str, line: usize) -> Result<bool, Finding> {
    let [token] = rest else {
        return Err(syntax(file, line, "expect monitor open|closed"));
    };
    match bare_at(token, file, line)? {
        "open" => Ok(true),
        "closed" => Ok(false),
        other => Err(syntax(
            file,
            line,
            format!("expected open or closed, got `{other}`"),
        )),
    }
}

pub(super) fn on_off(rest: &[Token], file: &str, line: usize) -> Result<bool, Finding> {
    let [token] = rest else {
        return Err(syntax(file, line, "expect auto on|off"));
    };
    match bare_at(token, file, line)? {
        "on" => Ok(true),
        "off" => Ok(false),
        other => Err(syntax(
            file,
            line,
            format!("expected on or off, got `{other}`"),
        )),
    }
}

pub(super) fn optional_id(
    rest: &[Token],
    file: &str,
    line: usize,
    form: &str,
) -> Result<Option<String>, Finding> {
    match rest {
        [Token::Bare(id)] if id == "none" => Ok(None),
        [Token::Bare(id)] => Ok(Some(id.clone())),
        _ => Err(syntax(file, line, format!("expected `{form}`"))),
    }
}

pub(super) fn parse_f32(text: &str, file: &str, line: usize) -> Result<f32, Finding> {
    text.parse()
        .map_err(|_| syntax(file, line, format!("`{text}` is not a number")))
}

/// `2s`, `0.5s`, `250ms`: a decimal number and a unit.
pub fn parse_duration(text: &str, file: &str, line: usize) -> Result<Duration, Finding> {
    let invalid = || {
        syntax(
            file,
            line,
            format!("`{text}` is not a duration (use `2s` or `250ms`)"),
        )
    };
    let (number, unit) = if let Some(ms) = text.strip_suffix("ms") {
        (ms, 1e-3)
    } else if let Some(seconds) = text.strip_suffix('s') {
        (seconds, 1.0)
    } else {
        return Err(invalid());
    };
    let value: f64 = number.parse().map_err(|_| invalid())?;
    if !value.is_finite() || value < 0.0 {
        return Err(invalid());
    }
    Ok(Duration::from_secs_f64(value * unit))
}

pub(super) fn manual_slot(text: &str, file: &str, line: usize) -> Result<u8, Finding> {
    match text {
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        _ => Err(syntax(file, line, "expected a manual slot 1-3")),
    }
}

pub(super) fn parse_load_target(
    text: &str,
    file: &str,
    line: usize,
) -> Result<LoadTarget, Finding> {
    match text {
        "autosave" => Ok(LoadTarget::Autosave),
        other => manual_slot(other, file, line).map(LoadTarget::Slot),
    }
}

pub(super) fn parse_write_target(
    text: &str,
    file: &str,
    line: usize,
) -> Result<WriteTarget, Finding> {
    match text {
        "autosave" => Ok(WriteTarget::Slot(Slot::Autosave)),
        "read-log" => Ok(WriteTarget::ReadLog),
        "settings" => Ok(WriteTarget::Settings),
        "credential" => Ok(WriteTarget::Credential),
        other => prefixed_slot(other, file, line).map(WriteTarget::Slot),
    }
}

/// `<n>` or `slot-<n>`, the two spellings a fault target and a fixture kind use.
pub(super) fn prefixed_slot(text: &str, file: &str, line: usize) -> Result<Slot, Finding> {
    let text = text.strip_prefix("slot-").unwrap_or(text);
    manual_slot(text, file, line).map(Slot::Manual)
}

pub(super) fn parse_fixture_kind(
    text: &str,
    file: &str,
    line: usize,
) -> Result<FixtureKind, Finding> {
    match text {
        "autosave" => Ok(FixtureKind::Slot(Slot::Autosave)),
        "legacy-save" => Ok(FixtureKind::LegacySave),
        "read-log" => Ok(FixtureKind::ReadLog),
        "settings" => Ok(FixtureKind::Settings),
        "credential" => Ok(FixtureKind::Credential),
        "env-key" => Ok(FixtureKind::EnvKey),
        other => prefixed_slot(other, file, line).map(FixtureKind::Slot),
    }
}

/// The closed fault list of spec §7.2.
pub(super) fn parse_fault(rest: &[Token], file: &str, line: usize) -> Result<Fault, Finding> {
    let Some((name, args)) = rest.split_first() else {
        return Err(syntax(file, line, "fault needs a name"));
    };
    let name = bare_at(name, file, line)?;
    let fault = match name {
        "write-blocked" => {
            let [target] = args else {
                return Err(syntax(file, line, "fault write-blocked <target>"));
            };
            Fault::WriteBlocked {
                target: parse_write_target(bare_at(target, file, line)?, file, line)?,
            }
        }
        "asset-missing" => Fault::AssetMissing {
            path: single_bare(args, file, line, "fault asset-missing <path>")?,
        },
        "asset-corrupt" => Fault::AssetCorrupt {
            path: single_bare(args, file, line, "fault asset-corrupt <path>")?,
        },
        "pack-file-missing" => Fault::PackFileMissing {
            path: single_bare(args, file, line, "fault pack-file-missing <path>")?,
        },
        "pack-file" => {
            let [path, fixture] = args else {
                return Err(syntax(file, line, "fault pack-file <path> <fixture>"));
            };
            Fault::PackFile {
                path: bare_at(path, file, line)?.to_string(),
                fixture: bare_at(fixture, file, line)?.to_string(),
            }
        }
        "credential-unreadable" => {
            if !args.is_empty() {
                return Err(syntax(file, line, "fault credential-unreadable"));
            }
            Fault::CredentialUnreadable
        }
        "decision-error" => {
            let [point, kind] = args else {
                return Err(syntax(
                    file,
                    line,
                    "fault decision-error <npc>.<question> transport|answer|refusal",
                ));
            };
            Fault::DecisionError {
                point: bare_at(point, file, line)?.to_string(),
                kind: match bare_at(kind, file, line)? {
                    "transport" => DecisionFault::Transport,
                    "answer" => DecisionFault::Answer,
                    "refusal" => DecisionFault::Refusal,
                    other => {
                        return Err(syntax(
                            file,
                            line,
                            format!("unknown decision fault `{other}`"),
                        ))
                    }
                },
            }
        }
        "decision-slow" => {
            let [point, duration] = args else {
                return Err(syntax(
                    file,
                    line,
                    "fault decision-slow <npc>.<question> <duration>",
                ));
            };
            Fault::DecisionSlow {
                point: bare_at(point, file, line)?.to_string(),
                duration: parse_duration(bare_at(duration, file, line)?, file, line)?,
            }
        }
        "decision-hold" => Fault::DecisionHold {
            point: single_bare(args, file, line, "fault decision-hold <npc>.<question>")?,
        },
        "key-check" => {
            let [verdict] = args else {
                return Err(syntax(
                    file,
                    line,
                    "fault key-check accepted|rejected|unreachable|hold",
                ));
            };
            Fault::KeyCheck {
                verdict: match bare_at(verdict, file, line)? {
                    "accepted" => KeyCheckFault::Accepted,
                    "rejected" => KeyCheckFault::Rejected,
                    "unreachable" => KeyCheckFault::Unreachable,
                    "hold" => KeyCheckFault::Hold,
                    other => {
                        return Err(syntax(
                            file,
                            line,
                            format!("unknown key-check verdict `{other}`"),
                        ))
                    }
                },
            }
        }
        other => {
            return Err(Finding::at_line(
                file,
                line,
                rules::CASE_REFERENCE,
                format!("unknown fault `{other}`"),
            ))
        }
    };
    Ok(fault)
}

pub(super) fn single_bare(
    args: &[Token],
    file: &str,
    line: usize,
    form: &str,
) -> Result<String, Finding> {
    let [token] = args else {
        return Err(syntax(file, line, format!("expected `{form}`")));
    };
    Ok(bare_at(token, file, line)?.to_string())
}
