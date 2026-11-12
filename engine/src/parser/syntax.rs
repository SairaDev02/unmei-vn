//! The line shapes and identifier grammar the line loop and the author CLI
//! share: the reserved first-token keywords, the say-line split, the snake_case
//! id and label grammar, and the `{var}` shape check. Whether a referenced
//! variable is declared is the pack loader's question, not a lexical one.

/// The DSL's reserved first-token keywords: a line beginning with one is a
/// command or a flow line, never a say-line. Shared with the author CLI's
/// drafting gate so prose cannot smuggle structure past it.
pub const KEYWORDS: &[&str] = &[
    "if", "else", "endif", "bg", "sprite", "hide", "play", "stop", "set", "label", "jump",
    "decide", "choice", "end",
];

/// True when `token` is a reserved first-token keyword.
pub fn is_keyword(token: &str) -> bool {
    KEYWORDS.contains(&token)
}

/// A say-line iff there is a `": "` separator with a non-empty speaker part
/// containing no `:`. `Ace: Hi there: ok.` -> ("Ace", "Hi there: ok.")
pub fn split_say_line(trimmed: &str) -> Option<(String, String)> {
    let idx = trimmed.find(": ")?;
    let speaker = trimmed[..idx].trim();
    if speaker.is_empty() || speaker.contains(':') {
        return None;
    }
    let text = trimmed[idx + 2..].trim_end();
    Some((speaker.to_string(), text.to_string()))
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

/// The engine's snake_case id grammar: non-empty and only `[a-z0-9_]`. `what`
/// names the kind of id for the error message. Shared with the author CLI's
/// drafting gate, which applies the same grammar to a candidate's criterion
/// ids and fallback so a malformed id is named as a fixable correction rather
/// than only as an opaque label-closure error.
pub fn check_id(name: &str, what: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("empty {what}"));
    }
    if !name.chars().all(is_ident_char) {
        return Err(format!("{what} `{name}` is not snake_case [a-z0-9_]+"));
    }
    Ok(())
}

pub(super) fn check_label(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("empty label name".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return Err(format!("label `{name}` is not [a-zA-Z0-9_.]+"));
    }
    Ok(())
}

/// `{var}` refs in say-line text must be well-formed idents. Unknown-variable
/// detection (against `state.ron`) happens at pack validation.
pub(super) fn validate_interpolation(text: &str) -> Result<(), String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let close = text[i + 1..]
                .find('}')
                .map(|p| i + 1 + p)
                .ok_or("unterminated `{` in say-line text (missing `}`)")?;
            let name = &text[i + 1..close];
            if name.is_empty() {
                return Err("empty `{}` variable reference".into());
            }
            if !name.chars().all(is_ident_char) {
                return Err(format!(
                    "variable reference `{{{name}}}` is not a valid name"
                ));
            }
            i = close + 1;
        } else {
            i += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::content::{Line, Scene};
    use crate::error::LoadError;
    use crate::parser::Parser;

    fn parse(text: &str) -> Result<Scene, LoadError> {
        Parser::new("test.unmei").parse("test", text)
    }

    #[test]
    fn say_line_with_inner_colon() {
        let scene = parse("Ace: Hi there: ok.\n").unwrap();
        match &scene.lines[0] {
            Line::Say { speaker, text } => {
                assert_eq!(speaker, "Ace");
                assert_eq!(text, "Hi there: ok.");
            }
            other => panic!("expected say-line, got {other:?}"),
        }
    }

    #[test]
    fn say_line_interpolation() {
        let scene = parse("Ace: You seem {affinity_level} today.\n").unwrap();
        match &scene.lines[0] {
            Line::Say { text, .. } => assert_eq!(text, "You seem {affinity_level} today."),
            other => panic!("expected say-line, got {other:?}"),
        }
    }

    #[test]
    fn narration_is_say_line() {
        let scene = parse("Narrator: Morning light.\n").unwrap();
        assert!(matches!(&scene.lines[0], Line::Say { speaker, .. } if speaker == "Narrator"));
    }

    #[test]
    fn unterminated_interpolation_is_error() {
        let err = parse("Ace: broken {unclosed\n").unwrap_err();
        assert!(err.message.contains("unterminated"), "{err}");
    }

    #[test]
    fn empty_interpolation_is_error() {
        let err = parse("Ace: empty {}\n").unwrap_err();
        assert!(err.message.contains("empty"), "{err}");
    }

    #[test]
    fn unknown_command_is_error() {
        let err = parse("frobnicate the gate\n").unwrap_err();
        assert!(err.message.contains("unknown command"), "{err}");
        assert_eq!(err.line, Some(1));
    }

    #[test]
    fn reserved_words_are_first_token_only() {
        // `If` is not the lowercase keyword: a legal speaker.
        let scene = parse("If: only she had known.\n").unwrap();
        assert!(matches!(&scene.lines[0], Line::Say { speaker, .. } if speaker == "If"));
        // The whole line is a say-line — `If` capitalised is not the keyword.
        let scene = parse("If only she had known: yes.\n").unwrap();
        assert!(
            matches!(&scene.lines[0], Line::Say { speaker, text } if speaker == "If only she had known" && text == "yes.")
        );
    }

    #[test]
    fn bad_id_is_error() {
        let err = parse("bg School Gate\n").unwrap_err();
        assert!(err.message.contains("snake_case"), "{err}");
    }
}
