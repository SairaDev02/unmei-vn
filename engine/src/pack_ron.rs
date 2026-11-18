//! The pack RON bridge: the one boundary for the pack's RON conversion rules
//! (`docs/content-format.md` § Pack RON bridge).
//!
//! [`decode_state`] and [`encode_state`] are the typed whole-state text
//! boundary — decode is the reader, encode is the writer. Behind them sit the
//! generic `ron::Value` conversion, the state-map shape and string-key checks,
//! the one scalar conversion and the single type-name table; those helpers are
//! private. [`content_optional_string`] and [`provider_optional_string`] are
//! the separately named Serde adapters for the two optional-string
//! compatibility contracts (content tolerates the legacy `Some("…")` wrapper,
//! provider deliberately rejects it); both delegate to one private conversion.
//!
//! The bridge performs **no filesystem I/O and no whole-pack validation** —
//! those stay with `pack.rs` and its callers.

use std::collections::HashMap;

use crate::content::StateValue;

/// Decode a whole `state.ron` document into the typed state map.
///
/// Both accepted read forms work: RON struct syntax (`( key: value, … )`)
/// deserializes as the same generic map as the explicit map form, so no key
/// shape is imposed. String keys are required; values must be a string, finite
/// number or bool. Decoding preserves the exact authored string text — no trimming,
/// normalization or escape preprocessing.
///
/// On failure the returned message preserves the historic distinction: a
/// parser failure reads `invalid RON: …`, while a shape/key/scalar failure
/// names the offending key or root.
pub fn decode_state(src: &str) -> Result<HashMap<String, StateValue>, String> {
    let value = ron::from_str::<ron::Value>(src).map_err(|e| format!("invalid RON: {e}"))?;
    state_from_value(value)
}

/// Encode a typed state map as a RON map with quoted string keys.
///
/// Keys and string values are encoded by RON serialization, never interpolated
/// into the document, so quotes, backslashes, line breaks, control characters
/// and Unicode survive with valid escaping. Entry order follows the map's own
/// iteration and formatting is not a compatibility promise: fidelity here is
/// semantic, not byte-for-byte.
pub fn encode_state(state: &HashMap<String, StateValue>) -> Result<String, String> {
    ron::ser::to_string(state).map_err(|e| e.to_string())
}

/// Decode a generic RON value as a flat state map: a root map, string keys and
/// scalar values. Preserves the historic messages for a wrong root, a non-string
/// key and a named non-scalar value.
fn state_from_value(value: ron::Value) -> Result<HashMap<String, StateValue>, String> {
    let ron::Value::Map(map) = value else {
        return Err("state.ron must hold a flat key-value map".into());
    };
    let mut out = HashMap::new();
    for (k, v) in map {
        let ron::Value::String(key) = k else {
            return Err(format!("state keys must be strings; got {k:?}"));
        };
        let val = scalar_from_value(v).map_err(|name| {
            format!("state value for `{key}` must be string, number, or bool; got {name}")
        })?;
        // RON reads `NaN`, `inf` and an overflowing exponent as numbers. NaN
        // never equals itself, so equality-based logic would break on it.
        // Saves share `scalar_from_value` and keep their own load rules
        // (ADR 0006), so the finite check lives here, not there.
        if let StateValue::Num(n) = val {
            if !n.is_finite() {
                return Err(format!(
                    "state value for `{key}` must be a finite number; got {n}"
                ));
            }
        }
        out.insert(key, val);
    }
    Ok(out)
}

/// The one scalar conversion, shared by whole-state decoding and
/// `StateValue`'s own `Deserialize`. On failure returns the offending value's
/// type name for the caller's prefix.
pub(crate) fn scalar_from_value(value: ron::Value) -> Result<StateValue, &'static str> {
    match value {
        ron::Value::String(s) => Ok(StateValue::Str(s)),
        ron::Value::Bool(b) => Ok(StateValue::Bool(b)),
        ron::Value::Number(n) => Ok(StateValue::Num(n.into_f64())),
        other => Err(type_name_of(&other)),
    }
}

/// The single scalar type-name table.
fn type_name_of(v: &ron::Value) -> &'static str {
    match v {
        ron::Value::Unit => "unit",
        ron::Value::Bool(_) => "bool",
        ron::Value::Number(_) => "number",
        ron::Value::Char(_) => "char",
        ron::Value::String(_) => "string",
        ron::Value::Bytes(_) => "bytes",
        ron::Value::Option(_) => "option",
        ron::Value::Seq(_) => "sequence",
        ron::Value::Map(_) => "map",
    }
}

/// The content format's optional string: a plain string, unit `()`, `None`, or
/// the legacy `Some("…")` wrapper. Omission stays absent, and serialization
/// stays a plain string or omission.
pub mod content_optional_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(d: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = ron::Value::deserialize(d)?;
        super::optional_string_from_value(raw, true).map_err(serde::de::Error::custom)
    }

    pub fn serialize<S>(value: &Option<String>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => s.serialize_str(v),
            None => s.serialize_none(),
        }
    }
}

/// The provider contract's optional string: a plain string, unit `()` or
/// `None`. It deliberately rejects the legacy `Some("…")` wrapper, so sharing
/// the content decoding internals does not broaden provider configuration.
pub mod provider_optional_string {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(d: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = ron::Value::deserialize(d)?;
        super::optional_string_from_value(raw, false).map_err(serde::de::Error::custom)
    }
}

/// The one optional-string conversion both named adapters delegate to.
/// `legacy_some` selects the content contract's additional `Some("…")`
/// tolerance; the provider contract passes `false`, so every wrapped value
/// falls through to the rejection arm.
fn optional_string_from_value(
    raw: ron::Value,
    legacy_some: bool,
) -> Result<Option<String>, String> {
    match raw {
        ron::Value::String(s) => Ok(Some(s)),
        ron::Value::Unit | ron::Value::Option(None) => Ok(None),
        ron::Value::Option(Some(inner)) if legacy_some => match *inner {
            ron::Value::String(s) => Ok(Some(s)),
            other => Err(format!("expected a string, got {other:?}")),
        },
        other => Err(format!("expected a string or nothing, got {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both accepted read forms decode to the same typed map: the canonical
    /// struct syntax and the explicit map form, with exact key text preserved.
    #[test]
    fn struct_and_map_reads_decode_equally_and_keep_key_text() {
        let struct_form = r#"(
            affinity_level: "low",
            met_ace: false,
            letters_found: 0,
        )"#;
        let map_form = r#"{
            "affinity_level": "low",
            "met_ace": false,
            "letters_found": 0,
        }"#;
        let mut expected: HashMap<String, StateValue> = HashMap::new();
        expected.insert("affinity_level".to_string(), StateValue::Str("low".into()));
        expected.insert("met_ace".to_string(), StateValue::Bool(false));
        expected.insert("letters_found".to_string(), StateValue::Num(0.0));
        assert_eq!(decode_state(struct_form), Ok(expected.clone()));
        assert_eq!(decode_state(map_form), Ok(expected));
    }

    /// A non-identifier string key survives with its exact text — no trimming
    /// or normalization.
    #[test]
    fn quoted_non_identifier_keys_keep_their_exact_text() {
        let decoded = decode_state(r#"{ "weird key": "value" }"#).expect("map form decodes");
        assert_eq!(
            decoded.get("weird key"),
            Some(&StateValue::Str("value".into()))
        );
    }

    /// The historic diagnostics are preserved: wrong root, non-string key and a
    /// named non-scalar value.
    #[test]
    fn shape_failures_keep_their_named_messages() {
        assert_eq!(
            decode_state("\"not a map\""),
            Err("state.ron must hold a flat key-value map".into())
        );
        let err = decode_state("{ 1: \"x\" }").unwrap_err();
        assert!(err.starts_with("state keys must be strings; got "), "{err}");
        assert_eq!(
            decode_state("{ \"key\": [1, 2] }"),
            Err("state value for `key` must be string, number, or bool; got sequence".into())
        );
    }

    /// A parser failure keeps the `invalid RON:` prefix that distinguishes it
    /// from a conversion failure.
    #[test]
    fn syntax_failure_keeps_the_invalid_ron_prefix() {
        let err = decode_state("( key: )").unwrap_err();
        assert!(err.starts_with("invalid RON: "), "{err}");
    }

    /// `encode_state` writes a quoted-key RON map: quote, backslash, newline,
    /// control character and Unicode are RON-escaped and decode back to the
    /// exact text.
    #[test]
    fn encode_escapes_keys_and_values_and_round_trips_exactly() {
        let tricky = "quote\" and backslash\\ and newline\n and control\u{7} and \u{4e2d}\u{1F600}";
        let mut state: HashMap<String, StateValue> = HashMap::new();
        state.insert(
            "quote\"and\\backslash".to_string(),
            StateValue::Str(tricky.into()),
        );
        state.insert("weird key".to_string(), StateValue::Bool(true));
        state.insert("affinity".to_string(), StateValue::Num(3.5));

        let text = encode_state(&state).expect("state encodes");
        assert!(text.contains("\"weird key\""), "keys are quoted: {text}");
        assert!(
            text.contains("\"quote\\\"and\\\\backslash\""),
            "keys are RON-escaped: {text}"
        );
        assert_eq!(decode_state(&text), Ok(state));
    }

    /// Scalar Serde shares the bridge's conversion and its named error prefix.
    #[test]
    fn scalar_serde_uses_the_shared_conversion() {
        assert_eq!(
            ron::from_str::<StateValue>("\"x\"").unwrap(),
            StateValue::Str("x".into())
        );
        assert_eq!(
            ron::from_str::<StateValue>("true").unwrap(),
            StateValue::Bool(true)
        );
        assert_eq!(
            ron::from_str::<StateValue>("2.5").unwrap(),
            StateValue::Num(2.5)
        );
        let err = ron::from_str::<StateValue>("[1]").unwrap_err().to_string();
        assert!(
            err.contains("state values are string, number, or bool; got sequence"),
            "{err}"
        );
    }
}

/// Generators for property tests of the state map, shared by this module and
/// the save suite (`save/tests/properties.rs`).
#[cfg(test)]
pub(crate) mod strategies {
    use std::collections::HashMap;

    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use crate::content::StateValue;

    /// The property-test configuration `docs/testing.md` asks for: a fixed
    /// seed, so the workspace gate is deterministic, and no regressions file.
    pub(crate) fn config() -> Config {
        Config {
            cases: 512,
            rng_seed: RngSeed::Fixed(0x756e_6d65_6901),
            failure_persistence: None,
            ..Config::default()
        }
    }

    /// Any text of up to 12 characters, control characters and every plane of
    /// Unicode included.
    pub(crate) fn text() -> impl Strategy<Value = String> {
        prop::collection::vec(any::<char>(), 0..=12).prop_map(String::from_iter)
    }

    /// Any finite number: whole numbers as authors write them, and every
    /// finite class, including subnormals and `-0.0`. Non-finite numbers are
    /// out: `decode_state` rejects them by design.
    fn finite() -> impl Strategy<Value = f64> {
        use prop::num::f64::{NEGATIVE, NORMAL, POSITIVE, SUBNORMAL, ZERO};
        prop_oneof![
            (-1000i32..=1000).prop_map(f64::from),
            POSITIVE | NEGATIVE | NORMAL | SUBNORMAL | ZERO,
        ]
    }

    fn value() -> impl Strategy<Value = StateValue> {
        prop_oneof![
            text().prop_map(StateValue::Str),
            finite().prop_map(StateValue::Num),
            any::<bool>().prop_map(StateValue::Bool),
        ]
    }

    pub(crate) fn state_map() -> impl Strategy<Value = HashMap<String, StateValue>> {
        prop::collection::hash_map(text(), value(), 0..8)
    }

    /// Whether two state maps hold the same entries, numbers compared bit for
    /// bit: `StateValue`'s own equality would let `-0.0` come back as `0.0`.
    pub(crate) fn same_state(
        a: &HashMap<String, StateValue>,
        b: &HashMap<String, StateValue>,
    ) -> bool {
        a.len() == b.len()
            && a.iter().all(|(key, value)| match (value, b.get(key)) {
                (StateValue::Num(x), Some(StateValue::Num(y))) => x.to_bits() == y.to_bits(),
                (value, other) => Some(value) == other,
            })
    }
}

#[cfg(test)]
mod properties {
    use proptest::prelude::*;

    use super::strategies::{config, same_state, state_map};
    use super::*;

    proptest! {
        #![proptest_config(config())]

        /// Any state map decodes back to exactly the map that was encoded.
        #[test]
        fn decode_returns_exactly_the_encoded_state(state in state_map()) {
            let text = encode_state(&state).expect("a finite state map encodes");
            let decoded = decode_state(&text);
            prop_assert!(
                decoded.as_ref().is_ok_and(|decoded| same_state(decoded, &state)),
                "{state:?} encoded as {text} decoded as {decoded:?}"
            );
        }
    }
}
