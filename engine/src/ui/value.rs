//! The hand-walked RON values every plugin file shares: a field map that
//! reports unknown and missing properties where the author wrote them, the
//! scalar readers the token and node parsers build on, and the `{group.name}`
//! reference form.
//!
//! The plugin files are walked by hand rather than derived, because a finding
//! must carry the field path and an unknown property must never be dropped
//! silently (slice-1 design note, "RON syntax"). This module owns the walking,
//! not the vocabulary: the token and node modules name what each field means.

use std::collections::BTreeMap;

use crate::finding::{rules, Finding, Location};

/// Where one plugin file's parser reports, and the findings it adds.
pub(super) struct Report<'a> {
    file: &'a str,
    findings: &'a mut Vec<Finding>,
}

impl<'a> Report<'a> {
    pub(super) fn new(file: &'a str, findings: &'a mut Vec<Finding>) -> Self {
        Report { file, findings }
    }

    /// A `ui.parse` finding at a field path. The token file's root has no path,
    /// so an empty one is the file itself.
    pub(super) fn claim(&mut self, path: &str, message: impl Into<String>) {
        self.rule(path, rules::UI_PARSE, message);
    }

    pub(super) fn rule(&mut self, path: &str, rule: &str, message: impl Into<String>) {
        let location = if path.is_empty() {
            Location::File
        } else {
            Location::Field(path.to_string())
        };
        self.findings
            .push(Finding::new(self.file, location, rule, message));
    }

    /// A `ui.cap` finding, which is about the file rather than one field.
    pub(super) fn cap(&mut self, message: impl Into<String>) {
        self.findings.push(Finding::new(
            self.file,
            Location::File,
            rules::UI_CAP,
            message,
        ));
    }

    /// A reference that is written as one but names no token the group holds.
    pub(super) fn unknown_token(&mut self, path: &str, reference: &str, expected: &str) {
        self.rule(
            path,
            rules::UI_TOKEN_UNKNOWN,
            format!("`{reference}` is not a {expected} token"),
        );
    }

    /// A reference chain that returns to a token it is already resolving.
    pub(super) fn cycle(&mut self, path: &str, chain: &str) {
        self.rule(
            path,
            rules::UI_TOKEN_CYCLE,
            format!("the token reference cycle runs {chain}"),
        );
    }
}

/// One RON struct or map, split into named fields. Known fields are taken as
/// their parser reads them, so whatever is left over is a property the format
/// does not have there.
pub(super) struct Fields {
    path: String,
    entries: BTreeMap<String, ron::Value>,
}

impl Fields {
    pub(super) fn new(value: ron::Value, path: &str, report: &mut Report) -> Option<Fields> {
        // RON reads `()` as the unit value, not an empty struct, and it is how
        // an author writes a file with no fields (an empty token file, for
        // one). Every other non-map is the wrong shape.
        let map = match value {
            ron::Value::Map(map) => map,
            ron::Value::Unit => {
                return Some(Fields {
                    path: path.to_string(),
                    entries: BTreeMap::new(),
                });
            }
            other => {
                report.claim(path, format!("expected a map, found {}", kind_name(&other)));
                return None;
            }
        };
        let mut entries = BTreeMap::new();
        for (key, value) in map {
            match key {
                ron::Value::String(name) => {
                    entries.insert(name, value);
                }
                other => {
                    report.claim(
                        path,
                        format!("a field name must be a string, found {}", kind_name(&other)),
                    );
                    return None;
                }
            }
        }
        Some(Fields {
            path: path.to_string(),
            entries,
        })
    }

    /// The path of one field, such as `root.padding.top`.
    pub(super) fn path(&self, name: &str) -> String {
        if self.path.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", self.path, name)
        }
    }

    pub(super) fn take(&mut self, name: &str) -> Option<ron::Value> {
        self.entries.remove(name)
    }

    pub(super) fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Report every field nobody took as an unknown property. `context` names
    /// the shape (`on a \`text\` node`, `in a colour`) so the message says why
    /// the property is not there.
    pub(super) fn finish(&mut self, report: &mut Report, context: &str) -> bool {
        let mut clear = true;
        for name in self.entries.keys() {
            report.claim(
                &self.path(name),
                format!("unknown property `{name}`{context}"),
            );
            clear = false;
        }
        clear
    }
}

/// A required field that was not written.
pub(super) fn require(
    value: Option<ron::Value>,
    path: String,
    name: &str,
    report: &mut Report,
) -> Option<ron::Value> {
    match value {
        Some(value) => Some(value),
        None => {
            report.claim(&path, format!("missing `{name}`"));
            None
        }
    }
}

pub(super) fn string(value: ron::Value, path: &str, report: &mut Report) -> Option<String> {
    match value {
        ron::Value::String(text) => Some(text),
        other => {
            report.claim(
                path,
                format!("expected a string, found {}", kind_name(&other)),
            );
            None
        }
    }
}

/// A finite number, authored as an integer or a float.
pub(super) fn number(value: ron::Value, path: &str, report: &mut Report) -> Option<f64> {
    match value {
        ron::Value::Number(number) => {
            let number = number.into_f64();
            if number.is_finite() {
                Some(number)
            } else {
                report.claim(path, format!("expected a finite number, found {number}"));
                None
            }
        }
        other => {
            report.claim(
                path,
                format!("expected a number, found {}", kind_name(&other)),
            );
            None
        }
    }
}

/// A number that must be whole, such as a weight or a z-index, within the
/// range its field stores.
pub(super) fn whole(
    value: ron::Value,
    path: &str,
    min: f64,
    max: f64,
    report: &mut Report,
) -> Option<i64> {
    let number = number(value, path, report)?;
    if number.fract() != 0.0 || number < min || number > max {
        report.claim(
            path,
            format!("expected a whole number from {min} to {max}, found {number}"),
        );
        return None;
    }
    Some(number as i64)
}

pub(super) fn seq(value: ron::Value, path: &str, report: &mut Report) -> Option<Vec<ron::Value>> {
    match value {
        ron::Value::Seq(items) => Some(items),
        other => {
            report.claim(
                path,
                format!("expected a list, found {}", kind_name(&other)),
            );
            None
        }
    }
}

/// One of the format's lowercase kebab-case names: `"column"`, `"cover"`,
/// `"ease-in-out"`. The message lists every name the field does take.
pub(super) fn variant<T: Copy>(
    value: ron::Value,
    path: &str,
    options: &[(&str, T)],
    report: &mut Report,
) -> Option<T> {
    let name = string(value, path, report)?;
    if let Some((_, variant)) = options.iter().find(|(option, _)| *option == name) {
        return Some(*variant);
    }
    let names = options
        .iter()
        .map(|(option, _)| format!("`{option}`"))
        .collect::<Vec<_>>()
        .join(", ");
    report.claim(path, format!("`{name}` is not one of {names}"));
    None
}

/// The `group.name` inside `{group.name}`. `None` for text that is not a
/// reference, which the caller reports as the wrong type for the field.
pub(super) fn reference(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let (group, name) = inner.split_once('.')?;
    if group.is_empty() || name.is_empty() {
        return None;
    }
    Some((group, name))
}

/// Whether the author wrote a reference at all. A malformed one or one naming
/// no token is an unknown token, never a silently accepted string.
pub(super) fn is_reference(text: &str) -> bool {
    text.starts_with('{')
}

pub(super) fn kind_name(value: &ron::Value) -> &'static str {
    match value {
        ron::Value::Bool(_) => "a bool",
        ron::Value::Char(_) => "a character",
        ron::Value::Map(_) => "a map",
        ron::Value::Number(_) => "a number",
        ron::Value::Option(_) => "an option",
        ron::Value::String(_) => "a string",
        ron::Value::Bytes(_) => "bytes",
        ron::Value::Seq(_) => "a list",
        ron::Value::Unit => "an empty value",
    }
}
