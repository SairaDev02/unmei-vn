//! The state-value rules the runner consults: condition truth, strict
//! same-kind equality and `{var}` interpolation.
//!
//! Pack load rejects undeclared keys and kind mismatches, so these rules stay
//! total for hand-built scenes instead of panicking (see [`condition_holds`]).

use std::collections::HashMap;

use crate::content::{CondOp, Condition, Literal, StateValue};

impl From<Literal> for StateValue {
    fn from(value: Literal) -> Self {
        match value {
            Literal::Str(s) => StateValue::Str(s),
            Literal::Num(n) => StateValue::Num(n),
            Literal::Bool(b) => StateValue::Bool(b),
        }
    }
}

/// Evaluate a conditional's condition once, against the current state map.
///
/// Kinds are matched structurally; a missing key or a kind mismatch is `false`
/// (both are load-time errors for real packs, so unreachable at runtime).
/// Ordering operators apply to numbers only, also enforced at load time.
pub(super) fn condition_holds(condition: &Condition, state: &HashMap<String, StateValue>) -> bool {
    let actual = state.get(&condition.key);
    match condition.op {
        CondOp::Eq => values_equal(actual, &condition.value),
        CondOp::Ne => !values_equal(actual, &condition.value),
        CondOp::Lt | CondOp::Le | CondOp::Gt | CondOp::Ge => {
            let (Some(StateValue::Num(a)), Literal::Num(b)) = (actual, &condition.value) else {
                // Ordering on a non-number key is a load-time error; keep the
                // runtime total rather than panicking on hand-built scenes.
                return false;
            };
            match condition.op {
                CondOp::Lt => *a < *b,
                CondOp::Le => *a <= *b,
                CondOp::Gt => *a > *b,
                CondOp::Ge => *a >= *b,
                CondOp::Eq | CondOp::Ne => unreachable!("handled above"),
            }
        }
    }
}

/// Strict same-kind equality: a missing key or a kind mismatch is `false`.
fn values_equal(actual: Option<&StateValue>, expected: &Literal) -> bool {
    match (actual, expected) {
        (Some(StateValue::Str(a)), Literal::Str(b)) => a == b,
        (Some(StateValue::Num(a)), Literal::Num(b)) => a == b,
        (Some(StateValue::Bool(a)), Literal::Bool(b)) => a == b,
        _ => false,
    }
}

/// Substitute `{var}` references with the state value's display text. Unknown
/// names are left literal (pack load rejects them for real packs).
pub(super) fn interpolate(text: &str, state: &HashMap<String, StateValue>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match state.get(name) {
                    Some(value) => out.push_str(&value.display()),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}
