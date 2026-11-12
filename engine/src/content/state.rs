//! The state map's vocabulary: a declared value, the literal a `set` assigns
//! and the `<state key> <op> <literal>` condition a conditional evaluates
//! (ADR 0004).

/// One value in the flat state map (`state.ron` / `set` / `{var}`).
///
/// Bucketed strings are the recommended pattern for decision state
/// (`affinity_level: "high"`, not raw counters) — decision models judge,
/// they don't compute.
#[derive(Clone, Debug, PartialEq)]
pub enum StateValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

impl StateValue {
    /// Text form used for `{var}` interpolation in say-lines.
    pub fn display(&self) -> String {
        match self {
            StateValue::Str(s) => s.clone(),
            StateValue::Num(n) => {
                // Whole numbers render without a decimal; below 1e15 the cast
                // to i64 is exact (1e15 < 2^53).
                if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            StateValue::Bool(b) => b.to_string(),
        }
    }
}

impl<'de> serde::Deserialize<'de> for StateValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Route through RON's Value so any scalar shape (bool / number / string)
        // is accepted regardless of serde's untagged ordering quirks, then
        // delegate to the same scalar conversion whole-state decoding uses
        // (`crate::pack_ron`), so the two cannot drift.
        let raw = ron::Value::deserialize(deserializer)?;
        crate::pack_ron::scalar_from_value(raw).map_err(|name| {
            serde::de::Error::custom(format!(
                "state values are string, number, or bool; got {name}"
            ))
        })
    }
}

impl serde::Serialize for StateValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            StateValue::Str(s) => serializer.serialize_str(s),
            StateValue::Num(n) => serializer.serialize_f64(*n),
            StateValue::Bool(b) => serializer.serialize_bool(*b),
        }
    }
}

/// A state literal on a `set` line: string, number, or bool.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Str(String),
    Num(f64),
    Bool(bool),
}

/// Comparison operator in a [`Condition`].
///
/// `==` / `!=` apply to all three scalar kinds; the ordering operators
/// (`<`, `<=`, `>`, `>=`) apply to numbers only, enforced at load time
/// (ADR 0004, fixed state kinds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CondOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

impl CondOp {
    /// The operator's source spelling, for parsing and error messages.
    pub fn as_str(self) -> &'static str {
        match self {
            CondOp::Eq => "==",
            CondOp::Ne => "!=",
            CondOp::Lt => "<",
            CondOp::Le => "<=",
            CondOp::Gt => ">",
            CondOp::Ge => ">=",
        }
    }

    /// `true` for the numbers-only ordering operators (`<`, `<=`, `>`, `>=`).
    pub fn is_ordering(self) -> bool {
        matches!(self, CondOp::Lt | CondOp::Le | CondOp::Gt | CondOp::Ge)
    }
}

/// A conditional's condition: `<state key> <op> <literal>`, key always on the
/// left. A bare bool key parses as `<key> == true`.
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub key: String,
    pub op: CondOp,
    pub value: Literal,
    /// True when the author wrote just a key, with no literal. A bare key means
    /// `== true`, so it requires a bool key (checked at pack load).
    pub bare: bool,
}
