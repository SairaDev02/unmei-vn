//! A command's argument grammar, after the line loop has stripped the line's
//! keyword: `sprite`'s two arguments, `set` and its literal, the
//! `<state key> <op> <literal>` condition, and the `decide` / choice-option
//! operand shapes. Each returns a message and leaves the file and source line
//! to the line loop.

use crate::content::{CondOp, Condition, Literal};

use super::syntax::{check_id, check_label};

pub(super) fn two_args<'a>(rest: &'a str, cmd: &str) -> Result<(&'a str, &'a str), String> {
    let mut it = rest.split_whitespace();
    let a = it
        .next()
        .ok_or_else(|| format!("`{cmd}` needs 2 arguments"))?;
    let b = it
        .next()
        .ok_or_else(|| format!("`{cmd}` needs 2 arguments"))?;
    if it.next().is_some() {
        return Err(format!("`{cmd}` takes exactly 2 arguments"));
    }
    Ok((a, b))
}

/// `set <name> = <literal>` after keyword stripping: `<name> = <literal>`.
pub(super) fn parse_set(rest: &str) -> Result<(String, Literal), String> {
    let eq = rest
        .find('=')
        .ok_or_else(|| format!("`set` needs `name = value`, got: {rest}"))?;
    let name = rest[..eq].trim();
    check_id(name, "state key")?;
    let value = rest[eq + 1..].trim();
    if value.is_empty() {
        return Err(format!("`set` has no value: `{rest}`"));
    }
    Ok((name.to_string(), parse_literal(value)?))
}

/// `if <condition>` after keyword stripping: `<key> <op> <literal>`, or a bare
/// key meaning `== true`. Key always on the left. `==` / `!=` apply to all
/// scalar kinds; `<` / `<=` / `>` / `>=` are numbers-only and their key/literal
/// kinds are checked at pack load (ADR 0004).
pub(super) fn parse_condition(rest: &str) -> Result<Condition, String> {
    if rest.is_empty() {
        return Err("`if` needs a condition".into());
    }
    if let Some((pos, op_str, op)) = find_cond_op(rest) {
        let key = rest[..pos].trim();
        let value = rest[pos + op_str.len()..].trim();
        return condition_with(key, op, value);
    }
    // No operator: a bare key means `== true` (ADR 0004). More than one token
    // without a recognised operator is malformed, not a bare key.
    let mut parts = rest.split_whitespace();
    let key = parts.next().unwrap_or_default();
    if parts.next().is_some() {
        return Err(format!(
            "`if` condition must be `<key> <op> <literal>` or a bare key; got: `{rest}`"
        ));
    }
    check_id(key, "state key")?;
    Ok(Condition {
        key: key.to_string(),
        op: CondOp::Eq,
        value: Literal::Bool(true),
        bare: true,
    })
}

/// Probe order for condition operators: two-character forms first, so `<=` is
/// never read as `<` before its longer spelling is considered. The longest-at-
/// equal-position tie-break in `find_cond_op` makes order belt-and-braces.
const COND_OPS: [CondOp; 6] = [
    CondOp::Eq,
    CondOp::Ne,
    CondOp::Le,
    CondOp::Ge,
    CondOp::Lt,
    CondOp::Gt,
];

/// The earliest comparison operator in the condition text, preferring the
/// longer op when two start at the same position (`<=` over `<`).
fn find_cond_op(rest: &str) -> Option<(usize, &'static str, CondOp)> {
    let mut best: Option<(usize, &'static str, CondOp)> = None;
    for kind in COND_OPS {
        let op = kind.as_str();
        let Some(pos) = rest.find(op) else { continue };
        match best {
            Some((best_pos, best_op, _))
                if best_pos < pos || (best_pos == pos && best_op.len() >= op.len()) => {}
            _ => best = Some((pos, op, kind)),
        }
    }
    best
}

fn condition_with(key: &str, op: CondOp, value: &str) -> Result<Condition, String> {
    check_id(key, "state key")?;
    if value.is_empty() {
        return Err("`if` condition has no literal after the operator".into());
    }
    Ok(Condition {
        key: key.to_string(),
        op,
        value: parse_literal(value)?,
        bare: false,
    })
}

/// String / number / bool literals. Strings are double-quoted, no escapes in
/// the slice (content is prose); numbers are f64-parseable.
pub(super) fn parse_literal(value: &str) -> Result<Literal, String> {
    if value == "true" {
        return Ok(Literal::Bool(true));
    }
    if value == "false" {
        return Ok(Literal::Bool(false));
    }
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        if inner.contains('"') {
            return Err(format!("string literal contains an inner quote: {value}"));
        }
        return Ok(Literal::Str(inner.to_string()));
    }
    if let Ok(n) = value.parse::<f64>() {
        return Ok(Literal::Num(n));
    }
    Err(format!(
        "value is not a string (\"…\"), number, or bool: `{value}`"
    ))
}

/// `ace.greet -> reply` after keyword stripping: `<npc>.<question> -> <prefix>`.
pub(super) fn parse_decide(rest: &str) -> Result<(String, String, String), String> {
    let (left, prefix) = rest
        .split_once("->")
        .ok_or_else(|| format!("`decide` needs `<npc>.<question> -> <prefix>`, got: {rest}"))?;
    let prefix = prefix.trim();
    check_label(prefix).map_err(|m| format!("decide prefix: {m}"))?;
    let (npc_id, question) = left.trim().split_once('.').ok_or_else(|| {
        format!(
            "`decide` target must be `<npc>.<question>`, got: `{}`",
            left.trim()
        )
    })?;
    check_id(npc_id, "npc id")?;
    check_id(question, "question id")?;
    Ok((npc_id.to_string(), question.to_string(), prefix.to_string()))
}

/// `"<text>" -> <label>` inside a choice block.
pub(super) fn parse_choice_option(s: &str) -> Option<(String, String)> {
    let (text, target) = s.split_once("->")?;
    let text = text.trim();
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    if inner.is_empty() || inner.contains('"') {
        return None;
    }
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    Some((inner.to_string(), target.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::content::{Line, Scene};
    use crate::error::LoadError;
    use crate::parser::Parser;

    fn parse(text: &str) -> Result<Scene, LoadError> {
        Parser::new("test.unmei").parse("test", text)
    }

    #[test]
    fn set_value_may_contain_colon_space() {
        // The quoted value contains ": " — keyword parsing must win over say-line.
        let scene = parse("set mood = \"calm: steady\"\n").unwrap();
        assert_eq!(
            scene.lines[0],
            Line::Set {
                name: "mood".into(),
                value: Literal::Str("calm: steady".into())
            }
        );
    }

    #[test]
    fn set_literals() {
        let scene = parse("set a = \"bucket\"\nset b = 3\nset c = true\n").unwrap();
        assert_eq!(
            scene.lines[0],
            Line::Set {
                name: "a".into(),
                value: Literal::Str("bucket".into())
            }
        );
        assert_eq!(
            scene.lines[1],
            Line::Set {
                name: "b".into(),
                value: Literal::Num(3.0)
            }
        );
        assert_eq!(
            scene.lines[2],
            Line::Set {
                name: "c".into(),
                value: Literal::Bool(true)
            }
        );
    }

    #[test]
    fn set_without_equals_is_error() {
        let err = parse("set affinity_level \"high\"\n").unwrap_err();
        assert!(err.message.contains("name = value"), "{err}");
    }

    #[test]
    fn bare_bool_condition_and_inequality_parse() {
        let scene =
            parse("if met_ace\nAce: Yes.\nendif\nif met_ace != true\nAce: No.\nendif\n").unwrap();
        match &scene.lines[0] {
            Line::If { condition, .. } => {
                assert_eq!(condition.key, "met_ace");
                assert_eq!(condition.op, CondOp::Eq);
                assert_eq!(condition.value, Literal::Bool(true));
            }
            other => panic!("expected an if, got {other:?}"),
        }
        match &scene.lines[2] {
            Line::If { condition, .. } => assert_eq!(condition.op, CondOp::Ne),
            other => panic!("expected an if, got {other:?}"),
        }
    }

    #[test]
    fn ordering_operators_parse_into_the_right_op() {
        // `<=` must not be misread as `<` followed by a stray `=`.
        for (op, expected) in [
            ("<", CondOp::Lt),
            ("<=", CondOp::Le),
            (">", CondOp::Gt),
            (">=", CondOp::Ge),
        ] {
            let src = format!("if letters_found {op} 0\nNarrator: Yes.\nendif\n");
            let scene = parse(&src).unwrap();
            match &scene.lines[0] {
                Line::If { condition, .. } => {
                    assert_eq!(condition.key, "letters_found");
                    assert_eq!(condition.op, expected, "operator `{op}`");
                    assert_eq!(condition.value, Literal::Num(0.0));
                }
                other => panic!("expected an if, got {other:?}"),
            }
        }
    }

    #[test]
    fn ordering_operators_parse_without_surrounding_spaces() {
        for (op, expected) in [("<=", CondOp::Le), (">=", CondOp::Ge), ("<", CondOp::Lt)] {
            let src = format!("if letters_found{op}2\nNarrator: Yes.\nendif\n");
            let scene = parse(&src).unwrap();
            match &scene.lines[0] {
                Line::If { condition, .. } => {
                    assert_eq!(condition.op, expected);
                    assert_eq!(condition.value, Literal::Num(2.0));
                }
                other => panic!("expected an if, got {other:?}"),
            }
        }
    }

    #[test]
    fn negative_number_literals_parse() {
        for (src, expected_op) in [
            ("if letters_found < -1\nNarrator: Yes.\nendif\n", CondOp::Lt),
            ("if letters_found<-2\nNarrator: Yes.\nendif\n", CondOp::Lt),
            ("if letters_found<=-3\nNarrator: Yes.\nendif\n", CondOp::Le),
        ] {
            let scene = parse(src).unwrap();
            match &scene.lines[0] {
                Line::If { condition, .. } => {
                    assert_eq!(condition.op, expected_op, "{src}");
                    assert!(matches!(&condition.value, Literal::Num(n) if *n < 0.0));
                }
                other => panic!("expected an if, got {other:?}"),
            }
        }
    }

    #[test]
    fn bare_key_condition_is_flagged_bare() {
        let scene = parse("if met_ace\nNarrator: Yes.\nendif\n").unwrap();
        match &scene.lines[0] {
            Line::If { condition, .. } => {
                assert!(condition.bare);
                assert_eq!(condition.op, CondOp::Eq);
                assert_eq!(condition.value, Literal::Bool(true));
            }
            other => panic!("expected an if, got {other:?}"),
        }
    }

    #[test]
    fn decide_line_parses() {
        let scene = parse("decide ace.greet -> reply\n").unwrap();
        assert!(
            matches!(&scene.lines[0], Line::Decide { npc_id, question, prefix }
            if npc_id == "ace" && question == "greet" && prefix == "reply")
        );
    }

    #[test]
    fn decide_without_arrow_is_error() {
        let err = parse("decide ace.greet reply\n").unwrap_err();
        assert!(err.message.contains("->"), "{err}");
    }
}
