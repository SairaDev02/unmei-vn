//! The step half of the case grammar: every step form and every assertion
//! (spec §3). Kept apart from `parse.rs` so each file stays under the size limit.

use super::model::{
    Click, ControlCondition, DecisionState, Expect, LogLevel, Pick, SkipMode, SlotExpect, Step,
};
use super::parse::{
    bare_at, manual_slot, on_off, open_closed, optional_id, parse_duration, parse_f32,
    parse_load_target, parse_write_target, quoted_only, quoted_unless_none, syntax, value_text,
    Token,
};
use super::rules;
use crate::finding::Finding;

pub(super) fn parse_step(
    verb: &str,
    rest: &[Token],
    file: &str,
    line: usize,
) -> Result<Step, Finding> {
    let step = match verb {
        "new-game" => {
            if !rest.is_empty() {
                return Err(syntax(file, line, "new-game"));
            }
            Step::NewGame
        }
        "continue" => {
            if !rest.is_empty() {
                return Err(syntax(file, line, "continue"));
            }
            Step::Continue
        }
        "advance" => {
            let count = match rest {
                [] => 1,
                [token] => bare_at(token, file, line)?
                    .parse::<u32>()
                    .map_err(|_| syntax(file, line, "advance takes a whole number"))?,
                _ => return Err(syntax(file, line, "advance [n]")),
            };
            Step::Advance(count)
        }
        "pick" => Step::Pick(match rest {
            [Token::Quoted(text)] => Pick::Text(text.clone()),
            [Token::Bare(index)] => Pick::Index(
                index
                    .parse::<u32>()
                    .map_err(|_| syntax(file, line, "pick takes an index or a quoted text"))?,
            ),
            _ => return Err(syntax(file, line, "pick <n> or pick \"<text>\"")),
        }),
        "menu" => {
            if !rest.is_empty() {
                return Err(syntax(file, line, "menu"));
            }
            Step::Menu
        }
        "activate" => {
            let [id] = rest else {
                return Err(syntax(file, line, "activate <control>"));
            };
            Step::Activate(bare_at(id, file, line)?.to_string())
        }
        "save" => {
            let [slot] = rest else {
                return Err(syntax(file, line, "save <1-3>"));
            };
            Step::Save(manual_slot(bare_at(slot, file, line)?, file, line)?)
        }
        "load" => {
            let [slot] = rest else {
                return Err(syntax(file, line, "load <1-3|autosave>"));
            };
            Step::Load(parse_load_target(bare_at(slot, file, line)?, file, line)?)
        }
        "auto" => {
            if !rest.is_empty() {
                return Err(syntax(file, line, "auto"));
            }
            Step::Auto
        }
        "skip" => {
            if !rest.is_empty() {
                return Err(syntax(file, line, "skip"));
            }
            Step::Skip
        }
        "press" | "hold" | "release" => {
            let [key] = rest else {
                return Err(syntax(file, line, "press|hold|release <key>"));
            };
            let key = bare_at(key, file, line)?.to_string();
            match verb {
                "press" => Step::Press(key),
                "hold" => Step::Hold(key),
                _ => Step::Release(key),
            }
        }
        "type" => Step::Type(quoted_only(rest, file, line, "type \"<text>\"")?),
        "paste" => Step::Paste(quoted_only(rest, file, line, "paste \"<text>\"")?),
        "wait" => {
            let [duration] = rest else {
                return Err(syntax(file, line, "wait <duration>"));
            };
            Step::Wait(parse_duration(bare_at(duration, file, line)?, file, line)?)
        }
        "release-decision" => {
            let [point] = rest else {
                return Err(syntax(file, line, "release-decision <npc>.<question>"));
            };
            Step::ReleaseDecision(bare_at(point, file, line)?.to_string())
        }
        "release-key-check" => {
            if !rest.is_empty() {
                return Err(syntax(file, line, "release-key-check"));
            }
            Step::ReleaseKeyCheck
        }
        "unblock" => {
            let [target] = rest else {
                return Err(syntax(file, line, "unblock <target>"));
            };
            Step::Unblock(parse_write_target(
                bare_at(target, file, line)?,
                file,
                line,
            )?)
        }
        "frame" => match rest {
            [] => Step::Frame(None),
            [name] => Step::Frame(Some(bare_at(name, file, line)?.to_string())),
            _ => return Err(syntax(file, line, "frame [name]")),
        },
        "click" => match rest {
            [control] => Step::Click(Click::Control(bare_at(control, file, line)?.to_string())),
            [x, y] => Step::Click(Click::Position(
                parse_f32(bare_at(x, file, line)?, file, line)?,
                parse_f32(bare_at(y, file, line)?, file, line)?,
            )),
            _ => return Err(syntax(file, line, "click <control> or click <x> <y>")),
        },
        "expect" => Step::Expect(parse_expect(rest, file, line)?),
        _ => {
            return Err(Finding::at_line(
                file,
                line,
                rules::CASE_SYNTAX,
                format!("unknown verb `{verb}`"),
            ))
        }
    };
    Ok(step)
}

pub(super) fn parse_expect(rest: &[Token], file: &str, line: usize) -> Result<Expect, Finding> {
    let Some((subject, args)) = rest.split_first() else {
        return Err(syntax(file, line, "expect needs a subject"));
    };
    let subject = bare_at(subject, file, line)?;
    let expect = match subject {
        "surface" => {
            let [name] = args else {
                return Err(syntax(file, line, "expect surface <name>"));
            };
            let name = bare_at(name, file, line)?;
            if !SURFACES.contains(&name) {
                return Err(syntax(file, line, format!("unknown surface `{name}`")));
            }
            Expect::Surface(name.to_string())
        }
        "monitor" => Expect::Monitor(open_closed(args, file, line)?),
        "speaker" => match args {
            [Token::Quoted(text)] => Expect::Speaker(Some(text.clone())),
            [Token::Bare(none)] if none == "none" => Expect::Speaker(None),
            _ => return Err(syntax(file, line, "expect speaker \"<text>\"|none")),
        },
        "line" => match args {
            [Token::Quoted(text)] => Expect::Line {
                text: text.clone(),
                contains: false,
            },
            [Token::Bare(contains), Token::Quoted(text)] if contains == "contains" => {
                Expect::Line {
                    text: text.clone(),
                    contains: true,
                }
            }
            _ => return Err(syntax(file, line, "expect line [contains] \"<text>\"")),
        },
        "revealed" => {
            if !args.is_empty() {
                return Err(syntax(file, line, "expect revealed"));
            }
            Expect::Revealed(true)
        }
        "revealing" => {
            if !args.is_empty() {
                return Err(syntax(file, line, "expect revealing"));
            }
            Expect::Revealed(false)
        }
        "choices" => {
            if args.is_empty() {
                return Err(syntax(file, line, "expect choices needs at least one text"));
            }
            let mut texts = Vec::with_capacity(args.len());
            for token in args {
                texts.push(quoted_unless_none(token, file, line)?);
            }
            Expect::Choices(texts)
        }
        "notice" => match args {
            [] => return Err(syntax(file, line, "expect notice <text>|contains|none")),
            [Token::Bare(none)] if none == "none" => Expect::NoticeNone,
            [Token::Quoted(text)] => Expect::NoticeText {
                text: text.clone(),
                contains: false,
            },
            [Token::Bare(contains), Token::Quoted(text)] if contains == "contains" => {
                Expect::NoticeText {
                    text: text.clone(),
                    contains: true,
                }
            }
            _ => return Err(syntax(file, line, "expect notice \"<text>\"|contains|none")),
        },
        "control" => {
            let Some((id, condition)) = args.split_first() else {
                return Err(syntax(file, line, "expect control <id> <condition>"));
            };
            let id = bare_at(id, file, line)?.to_string();
            match condition {
                [Token::Bare(word)] => Expect::Control {
                    id,
                    condition: match word.as_str() {
                        "enabled" => ControlCondition::Enabled,
                        "disabled" => ControlCondition::Disabled,
                        "focused" => ControlCondition::Focused,
                        "absent" => ControlCondition::Absent,
                        other => {
                            return Err(syntax(
                                file,
                                line,
                                format!("unknown control condition `{other}`"),
                            ))
                        }
                    },
                },
                [Token::Bare(label), Token::Quoted(text)] if label == "label" => {
                    Expect::ControlLabel {
                        id,
                        label: text.clone(),
                    }
                }
                _ => {
                    return Err(syntax(
                        file,
                        line,
                        "expect control <id> enabled|disabled|focused|absent|label \"<text>\"",
                    ))
                }
            }
        }
        "bg" => Expect::Bg(optional_id(args, file, line, "expect bg <id>|none")?),
        "sprite" => match args {
            [npc, pose] => {
                let pose = bare_at(pose, file, line)?;
                if pose == "none" {
                    Expect::SpriteNone {
                        npc: bare_at(npc, file, line)?.to_string(),
                    }
                } else {
                    Expect::Sprite {
                        npc: bare_at(npc, file, line)?.to_string(),
                        pose: pose.to_string(),
                    }
                }
            }
            _ => return Err(syntax(file, line, "expect sprite <npc> <pose>|none")),
        },
        "music" => Expect::Music(optional_id(args, file, line, "expect music <id>|none")?),
        "auto" => Expect::Auto(on_off(args, file, line)?),
        "skip" => {
            let [token] = args else {
                return Err(syntax(file, line, "expect skip seen|force|off"));
            };
            Expect::Skip(match bare_at(token, file, line)? {
                "seen" => SkipMode::Seen,
                "force" => SkipMode::Force,
                "off" => SkipMode::Off,
                other => return Err(syntax(file, line, format!("unknown skip mode `{other}`"))),
            })
        }
        "decision" => {
            let [token] = args else {
                return Err(syntax(file, line, "expect decision pending|held|none"));
            };
            Expect::Decision(match bare_at(token, file, line)? {
                "pending" => DecisionState::Pending,
                "held" => DecisionState::Held,
                "none" => DecisionState::None,
                other => {
                    return Err(syntax(
                        file,
                        line,
                        format!("unknown decision state `{other}`"),
                    ))
                }
            })
        }
        "scene" => {
            let [scene] = args else {
                return Err(syntax(file, line, "expect scene <id>"));
            };
            Expect::Scene(bare_at(scene, file, line)?.to_string())
        }
        "label" => match args {
            [Token::Bare(none)] if none == "none" => Expect::Label(None),
            [Token::Bare(name)] => Expect::Label(Some(name.clone())),
            _ => return Err(syntax(file, line, "expect label <name>|none")),
        },
        "state" => {
            let [key, equals, value] = args else {
                return Err(syntax(file, line, "expect state <key> = <value>"));
            };
            if bare_at(equals, file, line)? != "=" {
                return Err(syntax(file, line, "expected `=` in a state assertion"));
            }
            Expect::State {
                key: bare_at(key, file, line)?.to_string(),
                value: value_text(value),
            }
        }
        "read" | "unread" => {
            let [at] = args else {
                return Err(syntax(file, line, "expect read <scene>:<line>"));
            };
            let at = bare_at(at, file, line)?;
            let (scene, number) = at
                .rsplit_once(':')
                .ok_or_else(|| syntax(file, line, "expected <scene>:<line>"))?;
            let number: u32 = number
                .parse()
                .map_err(|_| syntax(file, line, "expected a 1-based line number"))?;
            Expect::Read {
                scene: scene.to_string(),
                line: number,
                present: subject == "read",
            }
        }
        "slot" => {
            let [slot, status, rest @ ..] = args else {
                return Err(syntax(
                    file,
                    line,
                    "expect slot <slot> empty|unreadable|scene <id>",
                ));
            };
            let slot = parse_load_target(bare_at(slot, file, line)?, file, line)?;
            let status = match (status, rest) {
                (Token::Bare(word), []) if word == "empty" => SlotExpect::Empty,
                (Token::Bare(word), []) if word == "unreadable" => SlotExpect::Unreadable,
                (Token::Bare(word), [scene]) if word == "scene" => {
                    SlotExpect::Scene(bare_at(scene, file, line)?.to_string())
                }
                _ => {
                    return Err(syntax(
                        file,
                        line,
                        "expect slot <slot> empty|unreadable|scene <id>",
                    ))
                }
            };
            Expect::Slot { slot, status }
        }
        "log" | "no-log" => {
            let Some((level, rest)) = args.split_first() else {
                return Err(syntax(
                    file,
                    line,
                    "expect log <level> [row <n>] contains \"<text>\"",
                ));
            };
            let level = match bare_at(level, file, line)? {
                "error" => LogLevel::Error,
                "warn" => LogLevel::Warn,
                "info" => LogLevel::Info,
                other => return Err(syntax(file, line, format!("unknown log level `{other}`"))),
            };
            let (row, rest) = match rest {
                [Token::Bare(row), value, ..] if row == "row" => (
                    Some(
                        bare_at(value, file, line)?
                            .parse::<u32>()
                            .map_err(|_| syntax(file, line, "row takes a number"))?,
                    ),
                    &rest[2..],
                ),
                _ => (None, rest),
            };
            if subject == "no-log" {
                if !rest.is_empty() {
                    return Err(syntax(file, line, "expect no-log <level> [row <n>]"));
                }
                Expect::NoLog { level, row }
            } else {
                match rest {
                    [Token::Bare(contains), Token::Quoted(text)] if contains == "contains" => {
                        Expect::Log {
                            level,
                            row,
                            text: text.clone(),
                        }
                    }
                    _ => {
                        return Err(syntax(
                            file,
                            line,
                            "expect log <level> [row <n>] contains \"<text>\"",
                        ))
                    }
                }
            }
        }
        other => return Err(syntax(file, line, format!("unknown assertion `{other}`"))),
    };
    Ok(expect)
}

/// The surface names a case can assert.
pub const SURFACES: &[&str] = &[
    "title", "play", "menu", "settings", "gallery", "still", "timeline", "confirm", "error",
];
